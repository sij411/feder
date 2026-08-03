use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use feder_core::storage::ServerStorage;
use feder_vocab::Actor;
use serde_json::Value;
use tower::ServiceExt;

use crate::common::{iri, test_router, test_router_with_storage};

async fn get_followers(
    app: Router,
    identifier: &str,
    accept: Option<&str>,
) -> axum::response::Response {
    let mut request = Request::builder().uri(format!("/users/{identifier}/followers"));
    if let Some(accept) = accept {
        request = request.header(header::ACCEPT, accept);
    }
    app.oneshot(request.body(Body::empty()).expect("valid request"))
        .await
        .expect("response")
}

async fn response_json(response: axum::response::Response) -> Value {
    let body = to_bytes(response.into_body(), 4096)
        .await
        .expect("read response body");
    serde_json::from_slice(&body).expect("valid JSON")
}

fn remote_actor(id: &str) -> Actor {
    Actor::person(
        iri(id),
        iri(&format!("{id}/inbox")),
        iri(&format!("{id}/outbox")),
    )
}

#[tokio::test]
async fn returns_empty_followers_collection_with_activitypub_headers() {
    let response = get_followers(test_router(), "alice", Some("application/activity+json")).await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/activity+json"
    );
    assert_eq!(response.headers().get(header::VARY).unwrap(), "Accept");
    let json = response_json(response).await;
    assert_eq!(json["type"], "OrderedCollection");
    assert_eq!(json["totalItems"], 0);
    assert_eq!(json["orderedItems"], serde_json::json!([]));
}

#[tokio::test]
async fn returns_stored_followers_in_stable_order() {
    let app = test_router_with_storage(|storage| {
        let following = iri("http://127.0.0.1:3000/users/alice");
        storage
            .store_follower(
                &remote_actor("https://remote.example/users/carol"),
                &following,
            )
            .expect("store Carol");
        storage
            .store_follower(
                &remote_actor("https://remote.example/users/bob"),
                &following,
            )
            .expect("store Bob");
    });

    let response = get_followers(app, "alice", Some("application/activity+json")).await;
    let json = response_json(response).await;
    assert_eq!(json["totalItems"], 2);
    assert_eq!(
        json["orderedItems"],
        serde_json::json!([
            "https://remote.example/users/bob",
            "https://remote.example/users/carol"
        ])
    );
}

#[tokio::test]
async fn reflects_follower_removal() {
    let app = test_router_with_storage(|storage| {
        let following = iri("http://127.0.0.1:3000/users/alice");
        let follower = remote_actor("https://remote.example/users/bob");
        storage
            .store_follower(&follower, &following)
            .expect("store follower");
        storage
            .remove_follower(&follower.id, &following)
            .expect("remove follower");
    });

    let response = get_followers(app, "alice", Some("application/activity+json")).await;
    assert_eq!(response_json(response).await["totalItems"], 0);
}

#[tokio::test]
async fn rejects_unknown_actor_and_unacceptable_media_types() {
    let unknown = get_followers(test_router(), "bob", Some("application/activity+json")).await;
    assert_eq!(unknown.status(), StatusCode::NOT_FOUND);

    for accept in [None, Some("text/html, application/activity+json;q=0.8")] {
        let response = get_followers(test_router(), "alice", accept).await;
        assert_eq!(response.status(), StatusCode::NOT_ACCEPTABLE);
        assert_eq!(response.headers().get(header::VARY).unwrap(), "Accept");
    }
}
