use axum::{
    Json,
    http::header,
    response::{IntoResponse, Response},
};
use serde::Serialize;

use crate::{
    app::{Changes, MEDIA_TYPE},
    config::Config,
    model,
};

pub fn validate_changes(changes: &Changes, config: &Config) -> Result<(), String> {
    if changes.update_old.len() != changes.update_new.len() {
        return Err("updateOld/updateNew lengths differ".into());
    }
    for (old, new) in changes.update_old.iter().zip(&changes.update_new) {
        if old.dns_name != new.dns_name || old.record_type != new.record_type {
            return Err("updateOld/updateNew name and type differ".into());
        }
    }
    for endpoint in changes
        .create
        .iter()
        .chain(&changes.update_old)
        .chain(&changes.update_new)
        .chain(&changes.delete)
    {
        let name = endpoint
            .dns_name
            .as_deref()
            .ok_or_else(|| "dnsName is required".to_owned())?;
        if !crate::filter::domain_matches(config, name) {
            return Err(format!("domain is outside configured filter: {name}"));
        }
        model::endpoint_to_records(endpoint, config.default_ttl, None)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub fn response_json<T: Serialize>(value: T) -> Response {
    (
        [
            (header::CONTENT_TYPE, MEDIA_TYPE),
            (header::VARY, "Content-Type"),
        ],
        Json(value),
    )
        .into_response()
}

pub fn negotiate_json<T: Serialize>(value: T) -> Response {
    ([(header::CONTENT_TYPE, MEDIA_TYPE)], Json(value)).into_response()
}
