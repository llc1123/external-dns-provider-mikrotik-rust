#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "HTTP test fixtures"
)]

#[path = "support/routeros.rs"]
mod support;

use axum::{body::to_bytes, http::StatusCode};
use external_dns_provider_mikrotik::model::{
    Endpoint, ProviderSpecific, RouterRecord, endpoint_to_records,
};
use serde_json::json;
use support::{endpoint, harness, request};

#[tokio::test]
async fn records_aggregate_managed_targets_and_hide_foreign() {
    let (app, fake) = harness().await;
    let source = endpoint(vec!["192.0.2.1", "192.0.2.2"]);
    let mut records = endpoint_to_records(&source, 3600, None).unwrap();
    records[0].id = Some("*1".into());
    records[1].id = Some("*2".into());
    let foreign = RouterRecord {
        id: Some("*3".into()),
        name: Some("other.example.org".into()),
        r#type: "A".into(),
        address: Some("192.0.2.3".into()),
        ttl: "5m".into(),
        ..RouterRecord::default()
    };
    fake.records
        .lock()
        .await
        .extend(records.into_iter().chain([foreign]));

    let response = request(app, "GET", "/records", json!(null)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    let result: Vec<Endpoint> = serde_json::from_slice(&body).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].targets, source.targets);
    assert_eq!(result[0].provider_specific, source.provider_specific);
}

#[tokio::test]
async fn invalid_change_batch_never_reaches_routeros() {
    let (app, fake) = harness().await;
    let body = json!({"create":[endpoint(vec!["192.0.2.1"]), endpoint(vec!["not-an-ip"])],"updateOld":[],"updateNew":[],"delete":[]});
    let response = request(app, "POST", "/records", body).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(*fake.writes.lock().await, 0);
    assert!(fake.records.lock().await.is_empty());
}

#[tokio::test]
async fn foreign_name_type_collision_blocks_creation() {
    let (app, fake) = harness().await;
    fake.records.lock().await.push(RouterRecord {
        id: Some("*1".into()),
        name: Some("multi.example.org".into()),
        r#type: "A".into(),
        address: Some("192.0.2.99".into()),
        ttl: "5m".into(),
        ..RouterRecord::default()
    });
    let body =
        json!({"create":[endpoint(vec!["192.0.2.1"])],"updateOld":[],"updateNew":[],"delete":[]});
    let response = request(app, "POST", "/records", body).await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(*fake.writes.lock().await, 0);
}

#[tokio::test]
async fn stale_metadata_delete_is_rejected_before_writes() {
    let (app, fake) = harness().await;
    let current = endpoint(vec!["192.0.2.1"]);
    let mut record = endpoint_to_records(&current, 3600, None).unwrap().remove(0);
    record.id = Some("*1".into());
    fake.records.lock().await.push(record);
    let mut stale = current;
    stale.provider_specific[0].value = "different".into();

    let response = request(
        app,
        "POST",
        "/records",
        json!({"create": [], "updateOld": [], "updateNew": [], "delete": [stale]}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(*fake.writes.lock().await, 0);
    assert_eq!(fake.records.lock().await.len(), 1);
}

#[tokio::test]
async fn mismatched_update_pair_is_rejected_before_writes() {
    let (app, fake) = harness().await;
    let old = endpoint(vec!["192.0.2.1"]);
    let mut new = endpoint(vec!["192.0.2.2"]);
    new.dns_name = Some("other.example.org".into());
    let response = request(
        app,
        "POST",
        "/records",
        json!({"create": [], "updateOld": [old], "updateNew": [new], "delete": []}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(*fake.writes.lock().await, 0);
}

#[tokio::test]
async fn update_preserves_unchanged_target_id_and_deletes_removed_target() {
    let (app, fake) = harness().await;
    let old = endpoint(vec!["192.0.2.1", "192.0.2.2"]);
    let create = request(
        app.clone(),
        "POST",
        "/records",
        json!({"create": [old], "updateOld": [], "updateNew": [], "delete": []}),
    )
    .await;
    assert_eq!(create.status(), StatusCode::NO_CONTENT);
    let current = endpoint(vec!["192.0.2.1", "192.0.2.2"]);
    let new = endpoint(vec!["192.0.2.1", "192.0.2.3"]);
    let response = request(
        app,
        "POST",
        "/records",
        json!({"create": [], "updateOld": [current], "updateNew": [new], "delete": []}),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::NO_CONTENT,
        "{}",
        String::from_utf8_lossy(&to_bytes(response.into_body(), 1 << 20).await.unwrap())
    );
    let records = fake.records.lock().await;
    assert_eq!(
        records
            .iter()
            .find(|r| r.address.as_deref() == Some("192.0.2.1"))
            .and_then(|r| r.id.as_deref()),
        Some("*1")
    );
    assert_eq!(
        records
            .iter()
            .filter_map(|r| r.address.as_deref())
            .collect::<Vec<_>>(),
        ["192.0.2.1", "192.0.2.3"]
    );
    assert_eq!(*fake.writes.lock().await, 4);
}

#[tokio::test]
async fn delete_uses_observed_id_after_default_comment_change() {
    let (app, fake) = harness().await;
    let old = endpoint(vec!["192.0.2.1"]);
    let mut record = endpoint_to_records(&old, 3600, Some("previous default"))
        .unwrap()
        .remove(0);
    record.id = Some("*42".into());
    fake.records.lock().await.push(record);
    let response = request(
        app,
        "POST",
        "/records",
        json!({"create": [], "updateOld": [], "updateNew": [], "delete": [old]}),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::NO_CONTENT,
        "{}",
        String::from_utf8_lossy(&to_bytes(response.into_body(), 1 << 20).await.unwrap())
    );
    assert!(fake.records.lock().await.is_empty());
}

#[tokio::test]
async fn partial_write_is_replayed_from_observed_state_without_duplicates() {
    let (app, fake) = harness().await;
    *fake.fail_after.lock().await = Some(1);
    let body = json!({"create": [endpoint(vec!["192.0.2.10", "192.0.2.11"])], "updateOld": [], "updateNew": [], "delete": []});
    let failed = request(app.clone(), "POST", "/records", body.clone()).await;
    assert_eq!(failed.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(fake.records.lock().await.len(), 1);
    let replay = request(app, "POST", "/records", body).await;
    assert_eq!(replay.status(), StatusCode::NO_CONTENT);
    assert_eq!(fake.records.lock().await.len(), 2);
}

#[tokio::test]
async fn patch_clears_optional_routeros_fields_and_switches_regexp_to_name() {
    let (app, fake) = harness().await;
    let old = Endpoint {
        dns_name: Some("clear.example.org".into()),
        record_type: "A".into(),
        record_ttl: 300,
        targets: vec!["192.0.2.20".into()],
        provider_specific: vec![
            ProviderSpecific {
                name: "disabled".into(),
                value: "true".into(),
            },
            ProviderSpecific {
                name: "match-subdomain".into(),
                value: "true".into(),
            },
            ProviderSpecific {
                name: "address-list".into(),
                value: "qa".into(),
            },
            ProviderSpecific {
                name: "regexp".into(),
                value: "^clear\\.example\\.org$".into(),
            },
        ],
        ..Endpoint::default()
    };
    let mut new = old.clone();
    new.provider_specific.clear();
    let create = request(
        app.clone(),
        "POST",
        "/records",
        json!({"create": [old.clone()], "updateOld": [], "updateNew": [], "delete": []}),
    )
    .await;
    assert_eq!(create.status(), StatusCode::NO_CONTENT);
    let update = request(
        app,
        "POST",
        "/records",
        json!({"create": [], "updateOld": [old], "updateNew": [new], "delete": []}),
    )
    .await;
    assert_eq!(update.status(), StatusCode::NO_CONTENT);
    let record = fake
        .records
        .lock()
        .await
        .iter()
        .find(|record| record.address.as_deref() == Some("192.0.2.20"))
        .cloned()
        .expect("record");
    assert_eq!(record.name.as_deref(), Some("clear.example.org"));
    assert_eq!(record.regexp.as_deref(), None);
    assert_eq!(record.disabled.as_deref(), Some("false"));
    assert_eq!(record.match_subdomain.as_deref(), None);
    assert_eq!(record.address_list.as_deref(), None);
}
