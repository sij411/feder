use std::convert::Infallible;

use axum::{
    Router,
    body::Bytes,
    http::{HeaderMap, StatusCode, Uri},
    routing::post,
};
use feder_core::{ActorDispatcher, key::ActorKeyPair};
use feder_server::{
    FederServer, InboxAuthPolicy, OutboundAddressPolicy, build_router, storage::SqliteStore,
};
use feder_vocab::{Actor, CryptographicKey, Endpoints, Iri, Reference};
use tokio::{sync::mpsc, task::JoinHandle};

pub const IDENTIFIER: &str = "alice";
pub const ORIGIN: &str = "http://127.0.0.1:3000";
pub const HANDLE_HOST: &str = "127.0.0.1:3000";

const PRIVATE_KEY_PEM: &str = include_str!("../fixtures/rsa-private-key.pem");
const PUBLIC_KEY_PEM: &str = include_str!("../fixtures/rsa-public-key.pem");

pub struct TestActors {
    actor: Actor,
}

pub struct RecordedRequest {
    pub headers: HeaderMap,
    pub uri: Uri,
    pub body: Bytes,
}

impl ActorDispatcher for TestActors {
    type Error = Infallible;

    fn get_actor(&self, identifier: &str) -> Result<Option<Actor>, Self::Error> {
        Ok((identifier == IDENTIFIER).then(|| self.actor.clone()))
    }

    fn get_actor_by_id(&self, actor_id: &Iri) -> Result<Option<Actor>, Self::Error> {
        Ok((actor_id == &self.actor.id).then(|| self.actor.clone()))
    }
}

pub fn iri(value: &str) -> Iri {
    value.parse().expect("valid test IRI")
}

pub fn actor_key_pair() -> ActorKeyPair {
    ActorKeyPair::from_pem(PRIVATE_KEY_PEM.to_string(), PUBLIC_KEY_PEM.to_string())
        .expect("valid actor key pair fixture")
}

pub fn local_actor() -> Actor {
    let actor_id = format!("{ORIGIN}/users/{IDENTIFIER}");
    let key_pair = actor_key_pair();
    let mut actor = Actor::person(
        iri(&actor_id),
        iri(&format!("{actor_id}/inbox")),
        iri(&format!("{actor_id}/outbox")),
    );
    actor.preferred_username = Some(IDENTIFIER.to_string());
    actor.name = Some("Alice".to_string());
    actor.followers = Some(iri(&format!("{actor_id}/followers")));
    actor.endpoints = Some(Endpoints {
        shared_inbox: Some(iri(&format!("{ORIGIN}/inbox"))),
    });
    actor.set_public_key(Reference::object(CryptographicKey::new(
        iri(&format!("{actor_id}#main-key")),
        actor.id.clone(),
        key_pair.public_key_pem().to_string(),
    )));
    actor
}

pub fn test_router() -> Router {
    test_router_with_storage(|_| {})
}

pub fn test_router_with_storage(configure: impl FnOnce(&SqliteStore)) -> Router {
    test_router_with_storage_and_policy(configure, InboxAuthPolicy::AllowUnsignedInsecureDev)
}

pub fn test_router_with_policy(inbox_auth_policy: InboxAuthPolicy) -> Router {
    test_router_with_storage_and_policy(|_| {}, inbox_auth_policy)
}

fn test_router_with_storage_and_policy(
    configure: impl FnOnce(&SqliteStore),
    inbox_auth_policy: InboxAuthPolicy,
) -> Router {
    build_router(test_server_with_storage(configure, inbox_auth_policy))
}

pub fn test_server_with_storage(
    configure: impl FnOnce(&SqliteStore),
    inbox_auth_policy: InboxAuthPolicy,
) -> FederServer<TestActors, SqliteStore> {
    let actor = local_actor();
    let storage = SqliteStore::open_in_memory().expect("open in-memory store");
    storage
        .insert_actor_key_pair(&actor.id, &actor_key_pair())
        .expect("store actor key pair");
    configure(&storage);
    FederServer::with_outbound_address_policy(
        TestActors { actor },
        storage,
        HANDLE_HOST,
        OutboundAddressPolicy::AllowPrivateAddress,
    )
    .expect("construct Feder server")
    .with_inbox_auth_policy(inbox_auth_policy)
}

pub async fn spawn_inbox_server(
    response_status: StatusCode,
) -> (String, mpsc::Receiver<RecordedRequest>, JoinHandle<()>) {
    let (sender, receiver) = mpsc::channel(2);
    let app = Router::new().route(
        "/inbox",
        post(move |headers: HeaderMap, uri: Uri, body: Bytes| {
            let sender = sender.clone();
            async move {
                sender
                    .send(RecordedRequest { headers, uri, body })
                    .await
                    .expect("request receiver remains open");
                response_status
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind inbox server");
    let address = listener.local_addr().expect("inbox server address");
    let task = tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("serve inbox endpoint");
    });

    (format!("http://{address}/inbox"), receiver, task)
}
