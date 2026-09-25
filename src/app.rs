use std::{collections::BTreeMap, sync::Arc};

use axum::{
    Json, Router,
    extract::{State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode, header},
    response::IntoResponse,
    routing::{get, post},
};
use regex::Regex;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::{
    config::Config,
    model::{self, Endpoint},
    routeros::RouterOs,
};

pub const MEDIA_TYPE: &str = "application/external.dns.webhook+json;version=1";

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub routeros: RouterOs,
    pub write_lock: Arc<Mutex<()>>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainFilter {
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub regex_include: String,
    pub regex_exclude: String,
}
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Changes {
    #[serde(default, deserialize_with = "null_changes")]
    pub create: Vec<Endpoint>,
    #[serde(default, deserialize_with = "null_changes")]
    pub update_old: Vec<Endpoint>,
    #[serde(default, deserialize_with = "null_changes")]
    pub update_new: Vec<Endpoint>,
    #[serde(default, deserialize_with = "null_changes")]
    pub delete: Vec<Endpoint>,
}
fn null_changes<'de, D>(deserializer: D) -> Result<Vec<Endpoint>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<Vec<Endpoint>>::deserialize(deserializer)?.unwrap_or_default())
}
pub fn health_router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(health))
        .route("/readyz", get(ready))
        .route("/metrics", get(metrics))
        .with_state(state)
}

pub fn build_router(config: Config) -> anyhow::Result<Router> {
    Ok(build_routers(config)?.0)
}

pub fn build_routers(config: Config) -> anyhow::Result<(Router, Router)> {
    if (!config.domain_filter.is_empty() || !config.exclude_domains.is_empty())
        && (config.regex_include.is_some() || config.regex_exclude.is_some())
    {
        anyhow::bail!("domain list and regular expression filters cannot be combined");
    }
    for pattern in [&config.regex_include, &config.regex_exclude]
        .into_iter()
        .flatten()
    {
        Regex::new(pattern)
            .map_err(|_| anyhow::anyhow!("invalid domain filter regular expression"))?;
    }
    let routeros = RouterOs::new(&config)?;
    let state = AppState {
        config,
        routeros,
        write_lock: Arc::new(Mutex::new(())),
    };
    let operational = health_router(state.clone());
    let webhook = Router::new()
        .route("/", get(negotiate))
        .route("/records", get(records).post(apply_changes))
        .route("/adjustendpoints", post(adjust))
        .with_state(state);
    Ok((webhook, operational))
}

fn valid(headers: &HeaderMap, content: bool) -> Result<(), StatusCode> {
    let key = if content {
        header::CONTENT_TYPE
    } else {
        header::ACCEPT
    };
    match headers.get(key).and_then(|v| v.to_str().ok()) {
        Some(v) if v == MEDIA_TYPE => Ok(()),
        Some(_) => Err(if content {
            StatusCode::UNSUPPORTED_MEDIA_TYPE
        } else {
            StatusCode::NOT_ACCEPTABLE
        }),
        None => Err(StatusCode::NOT_ACCEPTABLE),
    }
}
async fn negotiate(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    if let Err(s) = valid(&headers, false) {
        return (s, "invalid Accept header").into_response();
    }
    let f = DomainFilter {
        include: state.config.domain_filter,
        exclude: state.config.exclude_domains,
        regex_include: state.config.regex_include.unwrap_or_default(),
        regex_exclude: state.config.regex_exclude.unwrap_or_default(),
    };
    crate::app_support::negotiate_json(f)
}
async fn records(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    if let Err(s) = valid(&headers, false) {
        return (s, "invalid Accept header").into_response();
    }
    let result = state.routeros.list().await.and_then(|records| {
        let endpoints: Vec<_> = records
            .into_iter()
            .filter_map(|r| match crate::metadata::decode(&r.comment) {
                Ok(Some(_)) => Some(model::record_to_endpoint(&r).map_err(Into::into)),
                Ok(None) => None,
                Err(error) => Some(Err(error.into())),
            })
            .collect::<Result<Vec<_>, anyhow::Error>>()?;
        let mut grouped = BTreeMap::<String, Endpoint>::new();
        for endpoint in endpoints.into_iter().filter(|e| {
            crate::filter::domain_matches(&state.config, e.dns_name.as_deref().unwrap_or_default())
        }) {
            let mut key_endpoint = endpoint.clone();
            key_endpoint.targets.clear();
            let key = serde_json::to_string(&key_endpoint).map_err(|e| anyhow::anyhow!(e))?;
            if let Some(existing) = grouped.get_mut(&key) {
                existing.targets.extend(endpoint.targets);
            } else {
                grouped.insert(key, endpoint);
            }
        }
        Ok(grouped.into_values().collect::<Vec<_>>())
    });
    match result {
        Ok(v) => crate::app_support::response_json(v),
        Err(e) => {
            tracing::error!(error = %e, "RouterOS read failed");
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}
async fn apply_changes(
    State(state): State<AppState>,
    headers: HeaderMap,
    changes: Result<Json<Option<Changes>>, JsonRejection>,
) -> impl IntoResponse {
    if let Err(s) = valid(&headers, true) {
        return (s, "invalid Content-Type").into_response();
    }
    let changes = match changes {
        Ok(Json(changes)) => changes.unwrap_or_default(),
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let _guard = state.write_lock.lock().await;
    if let Err(e) = crate::app_support::validate_changes(&changes, &state.config) {
        tracing::warn!(reason = %e, "invalid changes batch");
        return (StatusCode::BAD_REQUEST, e).into_response();
    }
    let result = crate::reconcile::apply(&state, changes).await;
    match result {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            tracing::error!(error = %e, "RouterOS write failed");
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}
async fn adjust(
    State(state): State<AppState>,
    headers: HeaderMap,
    endpoints: Result<Json<Option<Vec<Endpoint>>>, JsonRejection>,
) -> impl IntoResponse {
    let mut endpoints = match endpoints {
        Ok(Json(endpoints)) => endpoints.unwrap_or_default(),
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    if let Err(status) = valid(&headers, true) {
        return (status, "invalid Content-Type").into_response();
    }
    if let Err(status) = valid(&headers, false) {
        return (status, "invalid Accept").into_response();
    }
    for endpoint in &mut endpoints {
        if let Err(error) = model::endpoint_to_records(
            endpoint,
            state.config.default_ttl,
            state.config.default_comment.as_deref(),
        ) {
            return (StatusCode::BAD_REQUEST, error.to_string()).into_response();
        }
        if endpoint
            .set_identifier
            .as_deref()
            .is_some_and(|value| !value.is_empty())
        {
            return (StatusCode::BAD_REQUEST, "setIdentifier is unsupported").into_response();
        }
        if endpoint.record_ttl == 0 {
            if !endpoint
                .provider_specific
                .iter()
                .any(|property| property.name == crate::model::ORIGINAL_TTL_PROPERTY)
            {
                endpoint
                    .provider_specific
                    .push(crate::model::ProviderSpecific {
                        name: crate::model::ORIGINAL_TTL_PROPERTY.into(),
                        value: "0".into(),
                    });
            }
            endpoint.record_ttl = state.config.default_ttl;
        }
        if let Ok(records) = model::endpoint_to_records(
            endpoint,
            state.config.default_ttl,
            state.config.default_comment.as_deref(),
        ) {
            let mut targets = Vec::new();
            for record in records {
                if let Ok(target) = model::target_from_record(&record) {
                    if !targets.contains(&target) {
                        targets.push(target);
                    }
                }
            }
            endpoint.targets = targets;
        }
    }
    crate::app_support::response_json(endpoints)
}
async fn health() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}
async fn ready(State(state): State<AppState>) -> impl IntoResponse {
    match state.routeros.list().await {
        Ok(_) => (StatusCode::OK, "ready").into_response(),
        Err(_) => (StatusCode::SERVICE_UNAVAILABLE, "not ready").into_response(),
    }
}
async fn metrics() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/plain; version=0.0.4")],
        "external_dns_provider_up 1\n",
    )
}
