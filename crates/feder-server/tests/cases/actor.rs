use axum::{
    Json, Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
    routing::get,
};
use feder_server::{ActorResolveError, ActorResolver, OutboundAddressPolicy};
use feder_vocab::Actor;
use serde_json::Value;
use tower::ServiceExt;

use crate::common::{iri, test_router};

#[tokio::test]
async fn returns_local_actor() {
    let response = test_router()
        .oneshot(
            Request::builder()
                .uri("/users/alice")
                .header(header::ACCEPT, "application/activity+json")
                .body(Body::empty())
                .expect("valid request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/activity+json"
    );
    assert_eq!(response.headers().get(header::VARY).unwrap(), "Accept");

    let body = to_bytes(response.into_body(), 8192)
        .await
        .expect("read response body");
    let json: Value = serde_json::from_slice(&body).expect("valid JSON");
    assert_eq!(json["id"], "http://127.0.0.1:3000/users/alice");
    assert_eq!(json["preferredUsername"], "alice");
    assert_eq!(
        json["publicKey"]["id"],
        "http://127.0.0.1:3000/users/alice#main-key"
    );
    assert_eq!(
        json["publicKey"]["publicKeyPem"],
        include_str!("../fixtures/rsa-public-key.pem")
    );
}

#[tokio::test]
async fn rejects_actor_request_without_acceptable_media_type() {
    for accept in [None, Some("text/html, application/activity+json;q=0.8")] {
        let mut request = Request::builder().uri("/users/alice");
        if let Some(accept) = accept {
            request = request.header(header::ACCEPT, accept);
        }
        let response = test_router()
            .oneshot(request.body(Body::empty()).expect("valid request"))
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::NOT_ACCEPTABLE);
        assert_eq!(response.headers().get(header::VARY).unwrap(), "Accept");
    }
}

#[tokio::test]
async fn rejects_unknown_actor() {
    let response = test_router()
        .oneshot(
            Request::builder()
                .uri("/users/bob")
                .header(header::ACCEPT, "application/activity+json")
                .body(Body::empty())
                .expect("valid request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

async fn spawn_actor_server(
    content_type: &'static str,
    mismatched_id: bool,
) -> (feder_vocab::Iri, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind actor server");
    let address = listener.local_addr().expect("actor server address");
    let actor_id = iri(&format!("http://{address}/users/bob"));
    let returned_id = if mismatched_id {
        iri(&format!("http://{address}/users/mallory"))
    } else {
        actor_id.clone()
    };
    let actor = Actor::person(
        returned_id,
        iri(&format!("http://{address}/users/bob/inbox")),
        iri(&format!("http://{address}/users/bob/outbox")),
    );
    let app = Router::new().route(
        "/users/bob",
        get(move || {
            let actor = actor.clone();
            async move { ([(header::CONTENT_TYPE, content_type)], Json(actor)) }
        }),
    );
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve actor");
    });
    (actor_id, task)
}

#[tokio::test]
async fn resolves_remote_actor_and_checks_canonical_id() {
    let resolver =
        ActorResolver::new(OutboundAddressPolicy::AllowPrivateAddress).expect("construct resolver");
    let (actor_id, server) = spawn_actor_server("application/activity+json", false).await;

    let actor = resolver.resolve(&actor_id).await.expect("resolve actor");

    assert_eq!(actor.id, actor_id);
    server.abort();
}

#[tokio::test]
async fn rejects_mismatched_actor_id_and_unsupported_content_type() {
    let resolver =
        ActorResolver::new(OutboundAddressPolicy::AllowPrivateAddress).expect("construct resolver");
    let (mismatched_id, mismatched_server) =
        spawn_actor_server("application/activity+json", true).await;
    let mismatch = resolver.resolve(&mismatched_id).await;
    assert!(matches!(
        mismatch,
        Err(ActorResolveError::ActorIdMismatch { .. })
    ));
    mismatched_server.abort();

    let (html_id, html_server) = spawn_actor_server("text/html", false).await;
    let unsupported = resolver.resolve(&html_id).await;
    assert!(matches!(
        unsupported,
        Err(ActorResolveError::UnsupportedContentType(_))
    ));
    html_server.abort();
}

#[tokio::test]
async fn public_policy_blocks_loopback_actor_resolution() {
    let resolver =
        ActorResolver::new(OutboundAddressPolicy::PublicOnly).expect("construct resolver");
    let actor_id = iri("http://127.0.0.1:3000/users/bob");

    let result = resolver.resolve(&actor_id).await;

    assert!(matches!(
        result,
        Err(ActorResolveError::PrivateResourceAddress { address, .. }) if address.is_loopback()
    ));
}
