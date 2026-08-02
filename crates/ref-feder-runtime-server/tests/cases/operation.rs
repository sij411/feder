use std::sync::Arc;

use axum::{Json, Router, body::Body, http::header::CONTENT_TYPE, routing::get};
use feder_vocab::{Actor, Iri, References};
use ref_feder_core::{
    note::{CreateNoteInput, PUBLIC_COLLECTION},
    storage::ServerStorage,
};
use ref_feder_runtime_server::{InboxAuthPolicy, build_router_with_state, storage::SqliteStore};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::common::{ORIGIN, RecordedRequest, iri, spawn_inbox_server, test_server_with_storage};

fn create_note_input() -> CreateNoteInput {
    CreateNoteInput {
        note_id: iri(&format!("{ORIGIN}/users/alice/posts/1")),
        create_id: iri(&format!("{ORIGIN}/users/alice/activities/create/1")),
        to: References::one(iri(PUBLIC_COLLECTION)),
        cc: References::one(iri(&format!("{ORIGIN}/users/alice/followers"))),
        content: "Hello from Feder.".to_string(),
        media_type: Some("text/html".to_string()),
        published: Some("2026-07-21T00:00:00Z".to_string()),
        url: Some(iri(&format!("{ORIGIN}/@alice/1"))),
    }
}

async fn spawn_remote_actor() -> (
    Iri,
    tokio::sync::mpsc::Receiver<RecordedRequest>,
    tokio::task::JoinHandle<()>,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind remote actor server");
    let address = listener.local_addr().expect("remote actor server address");
    let actor_id = iri(&format!("http://{address}/users/bob"));
    let inbox = format!("http://{address}/inbox");
    let actor = json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "type": "Person",
        "id": actor_id,
        "inbox": inbox,
        "outbox": format!("http://{address}/users/bob/outbox")
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
            axum::routing::post(
                move |headers: axum::http::HeaderMap,
                      uri: axum::http::Uri,
                      body: axum::body::Bytes| {
                    let sender = sender.clone();
                    async move {
                        sender
                            .send(RecordedRequest { headers, uri, body })
                            .await
                            .expect("request receiver remains open");
                        axum::http::StatusCode::ACCEPTED
                    }
                },
            ),
        );
    let task = tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("serve remote actor");
    });

    (actor_id, receiver, task)
}

#[tokio::test]
async fn outbound_follow_is_persisted_before_signed_delivery() {
    let (remote_actor_id, mut requests, remote_server) = spawn_remote_actor().await;
    let server = test_server_with_storage(|_| {}, InboxAuthPolicy::RequireSigned);
    let local_actor_id = iri(&format!("{ORIGIN}/users/alice"));
    let follow_id = iri(&format!("{ORIGIN}/users/alice/activities/follow/1"));

    let follow = server
        .follow_actor(&local_actor_id, &remote_actor_id, follow_id.clone())
        .await
        .expect("follow remote actor");

    assert_eq!(follow.id, follow_id);
    let request = requests.recv().await.expect("receive Follow delivery");
    assert!(request.headers.contains_key("signature"));
    let activity: Value = serde_json::from_slice(&request.body).expect("valid Follow activity");
    assert_eq!(activity["type"], "Follow");
    assert_eq!(activity["actor"], local_actor_id.as_str());
    assert_eq!(activity["object"], remote_actor_id.as_str());
    remote_server.abort();
}

#[tokio::test]
async fn create_note_persists_and_delivers_to_followers() {
    let (inbox, mut requests, inbox_server) =
        spawn_inbox_server(axum::http::StatusCode::ACCEPTED).await;
    let remote_actor_id = iri("https://remote.example/users/bob");
    let server = test_server_with_storage(
        |storage: &SqliteStore| {
            let remote_actor = Actor::person(
                remote_actor_id.clone(),
                iri(&inbox),
                iri("https://remote.example/users/bob/outbox"),
            );
            storage
                .store_follower(&remote_actor, &iri(&format!("{ORIGIN}/users/alice")))
                .expect("store follower");
        },
        InboxAuthPolicy::RequireSigned,
    );
    let server = Arc::new(server);
    let local_actor_id = iri(&format!("{ORIGIN}/users/alice"));

    let outcome = server
        .create_note(&local_actor_id, create_note_input())
        .await
        .expect("create Note");

    assert_eq!(outcome.note.content.as_deref(), Some("Hello from Feder."));
    let request = requests.recv().await.expect("receive Create delivery");
    assert!(request.headers.contains_key("signature"));
    let activity: Value = serde_json::from_slice(&request.body).expect("valid Create activity");
    assert_eq!(activity["type"], "Create");
    assert_eq!(activity["object"]["id"], outcome.note.id.as_str());

    let response = build_router_with_state(server)
        .oneshot(
            axum::http::Request::builder()
                .uri("/users/alice/posts/1")
                .header("accept", "application/activity+json")
                .body(Body::empty())
                .expect("valid object request"),
        )
        .await
        .expect("object response");
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    inbox_server.abort();
}
