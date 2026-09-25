#![allow(clippy::unwrap_used, reason = "HTTP test fixture construction")]

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use external_dns_provider_mikrotik::{
    app::{MEDIA_TYPE, build_routers},
    build_router,
    config::Config,
};
use serde_json::{Value, json};
use tower::ServiceExt;

#[tokio::test]
async fn negotiate_requires_exact_media_type_and_returns_filter() {
    let app = build_router(Config::test()).unwrap();
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/")
                .header("Accept", MEDIA_TYPE)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_ACCEPTABLE);
    let response = build_router(Config::test())
        .unwrap()
        .oneshot(
            Request::builder()
                .uri("/")
                .header("Accept", "application/json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_ACCEPTABLE);
}

#[tokio::test]
async fn health_does_not_require_routeros() {
    let (_, app) = build_routers(Config::test()).unwrap();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn malformed_apply_body_is_rejected() {
    let app = build_router(Config::test()).unwrap();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/records")
                .method("POST")
                .header("Content-Type", MEDIA_TYPE)
                .body(Body::from("not-json"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn adjust_accepts_null_and_preserves_explicit_values() {
    let app = build_router(Config::test()).unwrap();
    let null_request = Request::builder()
        .uri("/adjustendpoints")
        .method("POST")
        .header("Content-Type", MEDIA_TYPE)
        .header("Accept", MEDIA_TYPE)
        .body(Body::from("null"))
        .unwrap();
    let response = app.clone().oneshot(null_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    assert_eq!(serde_json::from_slice::<Value>(&bytes).unwrap(), json!([]));

    let input = json!([{"dnsName":"x.example", "recordType":"A", "recordTTL":0, "targets":["192.0.2.1"], "providerSpecific":null, "labels":null}]);
    let request = Request::builder()
        .uri("/adjustendpoints")
        .method("POST")
        .header("Content-Type", MEDIA_TYPE)
        .header("Accept", MEDIA_TYPE)
        .body(Body::from(input.to_string()))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    let adjusted: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(adjusted[0]["recordTTL"], 3600);
    assert_eq!(adjusted[0]["targets"], json!(["192.0.2.1"]));
}

#[tokio::test]
async fn adjust_canonicalizes_non_txt_targets() {
    let app = build_router(Config::test()).unwrap();
    let input = json!([{"dnsName":"x.example", "recordType":"MX", "recordTTL":300, "targets":["0010 mail.example."], "providerSpecific":[]}]);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/adjustendpoints")
                .method("POST")
                .header("Content-Type", MEDIA_TYPE)
                .header("Accept", MEDIA_TYPE)
                .body(Body::from(input.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&bytes).unwrap()[0]["targets"][0],
        "10 mail.example"
    );
}

#[test]
fn invalid_domain_filter_rejected_at_startup() {
    let mut config = Config::test();
    config.regex_exclude = Some("[".into());
    assert!(build_router(config).is_err());
}

#[tokio::test]
async fn changes_outside_domain_filter_are_rejected_before_router_access() {
    let mut config = Config::test();
    config.domain_filter = vec!["managed.example".into()];
    let app = build_router(config).unwrap();
    let body = json!({"create":[{"dnsName":"outside.example", "recordType":"A", "recordTTL":300, "targets":["192.0.2.1"]}], "updateOld":null, "updateNew":null, "delete":null});
    let response = app
        .oneshot(
            Request::builder()
                .uri("/records")
                .method("POST")
                .header("Content-Type", MEDIA_TYPE)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
