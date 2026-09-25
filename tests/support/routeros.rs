#![allow(clippy::unwrap_used, reason = "test fixture")]

use std::sync::Arc;

use axum::{
    Json, Router,
    body::Body,
    extract::{Path, State},
    http::{Request, StatusCode},
    routing::get,
};
use external_dns_provider_mikrotik::{
    app::MEDIA_TYPE,
    build_router,
    config::Config,
    model::{Endpoint, ProviderSpecific, RouterRecord},
};
use serde_json::{Value, json};
use tokio::sync::Mutex;
use tower::ServiceExt;

#[derive(Clone, Default)]
pub struct Fake {
    pub records: Arc<Mutex<Vec<RouterRecord>>>,
    pub writes: Arc<Mutex<usize>>,
    pub fail_after: Arc<Mutex<Option<usize>>>,
}

async fn list(State(fake): State<Fake>) -> Json<Vec<Value>> {
    Json(
        fake.records
            .lock()
            .await
            .iter()
            .map(|record| {
                let mut data = serde_json::to_value(record).unwrap();
                data[".id"] = json!(record.id);
                data
            })
            .collect(),
    )
}

async fn create(State(fake): State<Fake>, Json(mut record): Json<RouterRecord>) -> StatusCode {
    if should_fail(&fake).await {
        return StatusCode::INTERNAL_SERVER_ERROR;
    }
    record.id = Some(format!("*{}", fake.records.lock().await.len() + 1));
    fake.records.lock().await.push(record);
    *fake.writes.lock().await += 1;
    StatusCode::CREATED
}

async fn update(
    Path(id): Path<String>,
    State(fake): State<Fake>,
    Json(mut record): Json<RouterRecord>,
) -> StatusCode {
    if should_fail(&fake).await {
        return StatusCode::INTERNAL_SERVER_ERROR;
    }
    let mut records = fake.records.lock().await;
    let Some(existing) = records
        .iter_mut()
        .find(|r| r.id.as_deref() == Some(id.as_str()))
    else {
        return StatusCode::NOT_FOUND;
    };
    record.name = record.name.or_else(|| existing.name.clone());
    record.regexp = record.regexp.or_else(|| existing.regexp.clone());
    record.match_subdomain = record
        .match_subdomain
        .or_else(|| existing.match_subdomain.clone());
    record.address_list = record
        .address_list
        .or_else(|| existing.address_list.clone());
    record.disabled = record.disabled.or_else(|| existing.disabled.clone());
    record.address = record.address.or_else(|| existing.address.clone());
    record.cname = record.cname.or_else(|| existing.cname.clone());
    record.text = record.text.or_else(|| existing.text.clone());
    record.mx_exchange = record.mx_exchange.or_else(|| existing.mx_exchange.clone());
    record.mx_preference = record
        .mx_preference
        .or_else(|| existing.mx_preference.clone());
    record.srv_port = record.srv_port.or_else(|| existing.srv_port.clone());
    record.srv_target = record.srv_target.or_else(|| existing.srv_target.clone());
    record.srv_priority = record
        .srv_priority
        .or_else(|| existing.srv_priority.clone());
    record.srv_weight = record.srv_weight.or_else(|| existing.srv_weight.clone());
    record.ns = record.ns.or_else(|| existing.ns.clone());
    if record.regexp.as_deref() == Some("") {
        record.regexp = None;
    }
    if record.match_subdomain.as_deref() == Some("false") {
        record.match_subdomain = None;
    }
    if record.address_list.as_deref() == Some("") {
        record.address_list = None;
    }
    record.id = Some(id);
    *existing = record;
    *fake.writes.lock().await += 1;
    StatusCode::OK
}

async fn delete(Path(id): Path<String>, State(fake): State<Fake>) -> StatusCode {
    if should_fail(&fake).await {
        return StatusCode::INTERNAL_SERVER_ERROR;
    }
    let mut records = fake.records.lock().await;
    let Some(index) = records
        .iter()
        .position(|r| r.id.as_deref() == Some(id.as_str()))
    else {
        return StatusCode::NOT_FOUND;
    };
    records.remove(index);
    *fake.writes.lock().await += 1;
    StatusCode::NO_CONTENT
}

async fn should_fail(fake: &Fake) -> bool {
    let mut remaining = fake.fail_after.lock().await;
    match *remaining {
        Some(0) => {
            *remaining = None;
            true
        }
        Some(value) => {
            *remaining = Some(value - 1);
            false
        }
        None => false,
    }
}

pub async fn harness() -> (Router, Fake) {
    let fake = Fake::default();
    let api = Router::new()
        .route("/rest/ip/dns/static", get(list).put(create))
        .route(
            "/rest/ip/dns/static/{id}",
            axum::routing::patch(update).delete(delete),
        )
        .with_state(fake.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, api).await;
    });
    let mut config = Config::test();
    config.base_url = format!("http://{addr}/rest/ip/dns/static");
    (build_router(config).unwrap(), fake)
}

pub async fn request(
    app: Router,
    method: &str,
    uri: &str,
    body: Value,
) -> axum::response::Response {
    app.oneshot(
        Request::builder()
            .method(method)
            .uri(uri)
            .header("Accept", MEDIA_TYPE)
            .header("Content-Type", MEDIA_TYPE)
            .body(Body::from(body.to_string()))
            .unwrap(),
    )
    .await
    .unwrap()
}

pub fn endpoint(targets: Vec<&str>) -> Endpoint {
    Endpoint {
        dns_name: Some("multi.example.org".into()),
        record_type: "A".into(),
        record_ttl: 300,
        targets: targets.into_iter().map(str::to_owned).collect(),
        provider_specific: vec![ProviderSpecific {
            name: "comment".into(),
            value: "source".into(),
        }],
        set_identifier: None,
        labels: Default::default(),
    }
}
