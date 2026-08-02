use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use feder_core::{note::PUBLIC_COLLECTION, storage::NoteStore};
use feder_vocab::{Note, Reference, References};
use serde_json::Value;
use tower::ServiceExt;

use crate::common::{iri, test_router_with_storage};

fn stored_note() -> Note {
    let mut note = Note::new(iri("http://127.0.0.1:3000/users/alice/posts/1"));
    note.attributed_to = Some(Reference::id(iri("http://127.0.0.1:3000/users/alice")));
    note.to = References::one(iri(PUBLIC_COLLECTION));
    note.cc = References::one(iri("http://127.0.0.1:3000/users/alice/followers"));
    note.content = Some("Hello from Feder.".to_string());
    note.media_type = Some("text/html".to_string());
    note
}

fn router_with_note(note: Note) -> Router {
    test_router_with_storage(|storage| storage.store_note(&note).expect("store Note"))
}

async fn get_object(app: Router, uri: &str, accept: Option<&str>) -> axum::response::Response {
    let mut request = Request::builder().uri(uri);
    if let Some(accept) = accept {
        request = request.header(header::ACCEPT, accept);
    }
    app.oneshot(request.body(Body::empty()).expect("valid request"))
        .await
        .expect("response")
}

#[tokio::test]
async fn returns_public_stored_note_with_activitypub_headers() {
    let response = get_object(
        router_with_note(stored_note()),
        "/users/alice/posts/1",
        Some("application/activity+json"),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/activity+json"
    );
    assert_eq!(response.headers().get(header::VARY).unwrap(), "Accept");
    let body = to_bytes(response.into_body(), 4096)
        .await
        .expect("read response body");
    let json: Value = serde_json::from_slice(&body).expect("valid JSON");
    assert_eq!(json["type"], "Note");
    assert_eq!(json["content"], "Hello from Feder.");
}

#[tokio::test]
async fn returns_note_when_public_is_in_cc() {
    let mut note = stored_note();
    note.to = References::one(iri("https://remote.example/users/bob"));
    note.cc = References::one(iri(PUBLIC_COLLECTION));

    let response = get_object(
        router_with_note(note),
        "/users/alice/posts/1",
        Some("application/activity+json"),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn hides_non_public_notes() {
    for (to, cc) in [
        (
            References::one(iri("https://remote.example/users/bob")),
            References::new(),
        ),
        (
            References::one(iri("http://127.0.0.1:3000/users/alice/followers")),
            References::new(),
        ),
        (References::new(), References::new()),
    ] {
        let mut note = stored_note();
        note.to = to;
        note.cc = cc;
        let response = get_object(
            router_with_note(note),
            "/users/alice/posts/1",
            Some("application/activity+json"),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}

#[tokio::test]
async fn rejects_unknown_routes_and_unacceptable_media_types() {
    let unknown_note = get_object(
        router_with_note(stored_note()),
        "/users/alice/posts/unknown",
        Some("application/activity+json"),
    )
    .await;
    assert_eq!(unknown_note.status(), StatusCode::NOT_FOUND);

    let unknown_actor = get_object(
        router_with_note(stored_note()),
        "/users/bob/posts/1",
        Some("application/activity+json"),
    )
    .await;
    assert_eq!(unknown_actor.status(), StatusCode::NOT_FOUND);

    for accept in [None, Some("text/html, application/activity+json;q=0.8")] {
        let response = get_object(
            router_with_note(stored_note()),
            "/users/alice/posts/1",
            accept,
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_ACCEPTABLE);
        assert_eq!(response.headers().get(header::VARY).unwrap(), "Accept");
    }
}
