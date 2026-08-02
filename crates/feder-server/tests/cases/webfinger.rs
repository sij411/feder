use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use serde_json::Value;
use tower::ServiceExt;

use crate::common::test_router;

#[tokio::test]
async fn returns_webfinger_descriptor_for_local_actor() {
    let response = test_router()
        .oneshot(
            Request::builder()
                .uri("/.well-known/webfinger?resource=acct:alice@127.0.0.1:3000")
                .body(Body::empty())
                .expect("valid request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/jrd+json"
    );
    let body = to_bytes(response.into_body(), 1024)
        .await
        .expect("read response body");
    let json: Value = serde_json::from_slice(&body).expect("valid JSON");
    assert_eq!(json["subject"], "acct:alice@127.0.0.1:3000");
    assert_eq!(
        json["links"][0]["href"],
        "http://127.0.0.1:3000/users/alice"
    );
}

#[tokio::test]
async fn rejects_missing_malformed_unknown_and_non_authoritative_resources() {
    for uri in [
        "/.well-known/webfinger",
        "/.well-known/webfinger?resource=https://127.0.0.1/users/alice",
        "/.well-known/webfinger?resource=acct:bob@127.0.0.1:3000",
        "/.well-known/webfinger?resource=acct:alice@attacker.example",
    ] {
        let response = test_router()
            .oneshot(
                Request::builder()
                    .uri(uri)
                    .header(header::HOST, "attacker.example")
                    .body(Body::empty())
                    .expect("valid request"),
            )
            .await
            .expect("response");

        assert!(matches!(
            response.status(),
            StatusCode::BAD_REQUEST | StatusCode::NOT_FOUND
        ));
    }
}
