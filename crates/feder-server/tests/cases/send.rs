use axum::http::StatusCode;
use feder_core::key::verify_draft_cavage;
use feder_server::{
    OutboundAddressPolicy,
    send::{ActivitySender, SendError},
};
use feder_vocab::{Follow, Reference};

use crate::common::{actor_key_pair, iri, local_actor, spawn_inbox_server};

fn follow() -> Follow {
    Follow::new(
        iri("https://local.example/activities/follow/1"),
        Reference::id(iri("http://127.0.0.1:3000/users/alice")),
        Reference::id(iri("https://remote.example/users/bob")),
    )
}

#[tokio::test]
async fn sends_signed_activity_to_exact_inbox_target() {
    let (inbox, mut requests, server) = spawn_inbox_server(StatusCode::ACCEPTED).await;
    let inbox = format!("{inbox}?shared=true");
    let sender =
        ActivitySender::new(OutboundAddressPolicy::AllowPrivateAddress).expect("construct sender");
    let actor = local_actor();
    let key_pair = actor_key_pair();

    sender
        .send_activity(&actor, &key_pair, &follow(), &iri(&inbox))
        .await
        .expect("send Follow");

    let request = requests.recv().await.expect("receive request");
    assert_eq!(request.uri, "/inbox?shared=true");
    assert_eq!(request.headers["content-type"], "application/activity+json");
    assert_eq!(
        request.headers["digest"],
        feder_core::key::create_sha256_digest_header(&request.body)
    );
    let signature = request.headers["signature"]
        .to_str()
        .expect("signature header")
        .rsplit_once("signature=\"")
        .and_then(|(_, signature)| signature.strip_suffix('"'))
        .expect("signature parameter");
    let headers = [
        (
            "content-type",
            request.headers["content-type"].to_str().unwrap(),
        ),
        ("date", request.headers["date"].to_str().unwrap()),
        ("digest", request.headers["digest"].to_str().unwrap()),
        ("host", request.headers["host"].to_str().unwrap()),
    ];
    verify_draft_cavage(
        key_pair.public_key_pem(),
        "POST",
        "/inbox?shared=true",
        &headers,
        signature,
    )
    .expect("verify sent request");
    let activity: serde_json::Value =
        serde_json::from_slice(&request.body).expect("valid activity");
    assert_eq!(activity["type"], "Follow");
    server.abort();
}

#[tokio::test]
async fn reports_unsuccessful_inbox_status() {
    let (inbox, mut requests, server) = spawn_inbox_server(StatusCode::INTERNAL_SERVER_ERROR).await;
    let sender =
        ActivitySender::new(OutboundAddressPolicy::AllowPrivateAddress).expect("construct sender");

    let result = sender
        .send_activity(&local_actor(), &actor_key_pair(), &follow(), &iri(&inbox))
        .await;

    assert!(matches!(result, Err(SendError::UnsuccessfulStatus { .. })));
    requests.recv().await.expect("receive failed request");
    server.abort();
}

#[tokio::test]
async fn public_policy_blocks_loopback_inbox() {
    let sender = ActivitySender::new(OutboundAddressPolicy::PublicOnly).expect("construct sender");
    let inbox = iri("http://127.0.0.1:3000/inbox");

    let result = sender
        .send_activity(&local_actor(), &actor_key_pair(), &follow(), &inbox)
        .await;

    assert!(matches!(
        result,
        Err(SendError::PrivateInboxAddress { address, .. }) if address.is_loopback()
    ));
}

#[tokio::test]
async fn rejects_missing_or_mismatched_actor_key() {
    let sender =
        ActivitySender::new(OutboundAddressPolicy::AllowPrivateAddress).expect("construct sender");
    let key_pair = actor_key_pair();
    let mut actor = local_actor();
    actor.public_key = None;
    let inbox = iri("https://remote.example/inbox");

    let missing = sender
        .send_activity(&actor, &key_pair, &follow(), &inbox)
        .await;

    assert!(matches!(missing, Err(SendError::MissingActorKey(_))));
}
