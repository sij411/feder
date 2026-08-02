use axum::{
    Json, Router,
    body::{Body, Bytes, to_bytes},
    http::{HeaderMap, Request, StatusCode, Uri, header::CONTENT_TYPE},
    routing::{get, post},
};
use feder_core::key::{create_sha256_digest_header, sign_draft_cavage};
use feder_server::InboxAuthPolicy;
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::common::{
    HANDLE_HOST, ORIGIN, RecordedRequest, actor_key_pair, test_router, test_router_with_policy,
};

fn follow_body(actor_id: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "type": "Follow",
        "id": format!("{actor_id}/follows/1"),
        "actor": actor_id,
        "object": format!("{ORIGIN}/users/alice")
    }))
    .expect("serialize Follow")
}

fn undo_follow_body(actor_id: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "type": "Undo",
        "id": format!("{actor_id}/undos/1"),
        "actor": actor_id,
        "object": {
            "type": "Follow",
            "id": format!("{actor_id}/follows/1"),
            "actor": actor_id,
            "object": format!("{ORIGIN}/users/alice")
        }
    }))
    .expect("serialize Undo")
}

async fn spawn_remote_actor() -> (
    String,
    tokio::sync::mpsc::Receiver<RecordedRequest>,
    tokio::task::JoinHandle<()>,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind remote actor server");
    let address = listener.local_addr().expect("remote actor server address");
    let actor_id = format!("http://{address}/users/bob");
    let key_id = format!("{actor_id}#main-key");
    let inbox = format!("http://{address}/inbox");
    let actor = json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "type": "Person",
        "id": actor_id,
        "inbox": inbox,
        "outbox": format!("http://{address}/users/bob/outbox"),
        "preferredUsername": "bob",
        "publicKey": {
            "id": key_id,
            "owner": actor_id,
            "publicKeyPem": actor_key_pair().public_key_pem()
        }
    });
    let (sender, receiver) = tokio::sync::mpsc::channel(2);
    let app = Router::new()
        .route(
            "/users/bob",
            get(move || {
                let actor = actor.clone();
                async move { ([(CONTENT_TYPE, "application/activity+json")], Json(actor)) }
            }),
        )
        .route(
            "/inbox",
            post(move |headers: HeaderMap, uri: Uri, body: Bytes| {
                let sender = sender.clone();
                async move {
                    sender
                        .send(RecordedRequest { headers, uri, body })
                        .await
                        .expect("request receiver remains open");
                    StatusCode::ACCEPTED
                }
            }),
        );
    let task = tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("serve remote actor");
    });

    (actor_id, receiver, task)
}

async fn post_inbox(
    app: Router,
    uri: &str,
    content_type: &str,
    body: impl Into<Body>,
) -> axum::response::Response {
    app.oneshot(
        Request::builder()
            .method("POST")
            .uri(uri)
            .header(CONTENT_TYPE, content_type)
            .body(body.into())
            .expect("valid inbox request"),
    )
    .await
    .expect("inbox response")
}

async fn post_signed_inbox(
    app: Router,
    uri: &str,
    actor_id: &str,
    signed_body: &[u8],
    delivered_body: impl Into<Body>,
    host: &str,
) -> axum::response::Response {
    let date = httpdate::fmt_http_date(std::time::SystemTime::now());
    let digest = create_sha256_digest_header(signed_body);
    let headers = [
        ("content-type", "application/activity+json"),
        ("date", date.as_str()),
        ("digest", digest.as_str()),
        ("host", host),
    ];
    let signature = sign_draft_cavage(
        &actor_key_pair(),
        &format!("{actor_id}#main-key"),
        "POST",
        uri,
        &headers,
    )
    .expect("sign inbox request");

    app.oneshot(
        Request::builder()
            .method("POST")
            .uri(uri)
            .header(CONTENT_TYPE, "application/activity+json")
            .header("date", date)
            .header("digest", digest)
            .header("host", host)
            .header("signature", signature)
            .body(delivered_body.into())
            .expect("valid signed inbox request"),
    )
    .await
    .expect("signed inbox response")
}

async fn follower_count(app: Router) -> u64 {
    let response = app
        .oneshot(
            Request::builder()
                .uri("/users/alice/followers")
                .header("accept", "application/activity+json")
                .body(Body::empty())
                .expect("valid followers request"),
        )
        .await
        .expect("followers response");
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read followers response");
    let collection: Value = serde_json::from_slice(&body).expect("valid followers collection");
    collection["totalItems"]
        .as_u64()
        .expect("numeric follower count")
}

#[tokio::test]
async fn valid_signed_follow_is_stored_and_accept_is_sent() {
    let (actor_id, mut requests, remote_server) = spawn_remote_actor().await;
    let app = test_router_with_policy(InboxAuthPolicy::RequireSigned);
    let body = follow_body(&actor_id);

    let response = post_signed_inbox(
        app.clone(),
        "/users/alice/inbox",
        &actor_id,
        &body,
        body.clone(),
        HANDLE_HOST,
    )
    .await;

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert_eq!(follower_count(app).await, 1);
    let request = requests.recv().await.expect("receive Accept activity");
    assert!(request.headers.contains_key("signature"));
    let activity: Value = serde_json::from_slice(&request.body).expect("valid Accept activity");
    assert_eq!(activity["type"], "Accept");
    assert_eq!(activity["actor"], format!("{ORIGIN}/users/alice"));
    remote_server.abort();
}

#[tokio::test]
async fn unsigned_follow_is_rejected_when_signatures_are_required() {
    let (actor_id, _requests, remote_server) = spawn_remote_actor().await;
    let body = follow_body(&actor_id);

    let response = post_inbox(
        test_router_with_policy(InboxAuthPolicy::RequireSigned),
        "/users/alice/inbox",
        "application/activity+json",
        body,
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    remote_server.abort();
}

#[tokio::test]
async fn signed_follow_rejects_wrong_host() {
    let (actor_id, _requests, remote_server) = spawn_remote_actor().await;
    let body = follow_body(&actor_id);

    let response = post_signed_inbox(
        test_router_with_policy(InboxAuthPolicy::RequireSigned),
        "/users/alice/inbox",
        &actor_id,
        &body,
        body.clone(),
        "other.example",
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    remote_server.abort();
}

#[tokio::test]
async fn signed_follow_rejects_a_tampered_body() {
    let (actor_id, _requests, remote_server) = spawn_remote_actor().await;
    let signed_body = follow_body(&actor_id);
    let delivered_body = follow_body("https://attacker.example/users/mallory");

    let response = post_signed_inbox(
        test_router_with_policy(InboxAuthPolicy::RequireSigned),
        "/users/alice/inbox",
        &actor_id,
        &signed_body,
        delivered_body,
        HANDLE_HOST,
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    remote_server.abort();
}

#[tokio::test]
async fn signed_undo_removes_the_persisted_follower() {
    let (actor_id, mut requests, remote_server) = spawn_remote_actor().await;
    let app = test_router_with_policy(InboxAuthPolicy::RequireSigned);
    let follow = follow_body(&actor_id);
    let follow_response = post_signed_inbox(
        app.clone(),
        "/users/alice/inbox",
        &actor_id,
        &follow,
        follow.clone(),
        HANDLE_HOST,
    )
    .await;
    assert_eq!(follow_response.status(), StatusCode::ACCEPTED);
    requests.recv().await.expect("receive Accept activity");

    let undo = undo_follow_body(&actor_id);
    let undo_response = post_signed_inbox(
        app.clone(),
        "/users/alice/inbox",
        &actor_id,
        &undo,
        undo.clone(),
        HANDLE_HOST,
    )
    .await;

    assert_eq!(undo_response.status(), StatusCode::ACCEPTED);
    assert_eq!(follower_count(app).await, 0);
    remote_server.abort();
}

#[tokio::test]
async fn shared_inbox_routes_a_signed_follow() {
    let (actor_id, mut requests, remote_server) = spawn_remote_actor().await;
    let app = test_router_with_policy(InboxAuthPolicy::RequireSigned);
    let body = follow_body(&actor_id);

    let response = post_signed_inbox(
        app.clone(),
        "/inbox",
        &actor_id,
        &body,
        body.clone(),
        HANDLE_HOST,
    )
    .await;

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert_eq!(follower_count(app).await, 1);
    requests.recv().await.expect("receive Accept activity");
    remote_server.abort();
}

#[tokio::test]
async fn inbox_rejects_invalid_content_before_dispatch() {
    let unsupported = post_inbox(
        test_router(),
        "/users/alice/inbox",
        "application/json",
        "{}",
    )
    .await;
    assert_eq!(unsupported.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);

    let malformed = post_inbox(
        test_router(),
        "/users/alice/inbox",
        "application/activity+json",
        "{not json",
    )
    .await;
    assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn personal_inbox_rejects_an_unknown_local_actor() {
    let response = post_inbox(
        test_router(),
        "/users/bob/inbox",
        "application/activity+json",
        "{}",
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
