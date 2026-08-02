// Feder: A portable ActivityPub core for many runtimes.
// Copyright (C) 2026 Feder contributors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, version 3.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

use std::{
    convert::Infallible,
    fmt,
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use axum::{
    Json,
    body::Bytes,
    http::{HeaderMap, StatusCode, header},
    response::IntoResponse,
    routing::{get, post},
};
use feder_vocab::{Actor, CryptographicKey, Endpoints, Iri, Reference};
use ref_feder_core::{follow::PendingFollow, key::ActorKeyPair, storage::ServerStorage};
use ref_feder_runtime_server::{
    ActorDispatcher, Error, FederServer, InboxAuthPolicy, OutboundAddressPolicy,
    build_router_with_state,
};

const IDENTIFIER: &str = "alice";
const ORIGIN: &str = "http://127.0.0.1:3000";
const REMOTE_ACTOR_ID: &str = "http://127.0.0.1:3000/remote/users/bob";
const ACTOR_PRIVATE_KEY_PEM: &str =
    include_str!("../../../crates/feder-core/tests/fixtures/rsa-private-key.pem");
const ACTOR_PUBLIC_KEY_PEM: &str =
    include_str!("../../../crates/feder-core/tests/fixtures/rsa-public-key.pem");

struct SingleActorDispatcher {
    actor: Actor,
}

struct ExampleStorage {
    local_actor_id: Iri,
    actor_key_pair: ActorKeyPair,
    latest_follower: Mutex<Option<(Actor, Iri)>>,
    latest_pending_follow: Mutex<Option<PendingFollow>>,
}

#[derive(Debug)]
struct ExampleStorageError(&'static str);

impl fmt::Display for ExampleStorageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl std::error::Error for ExampleStorageError {}

impl ActorDispatcher for SingleActorDispatcher {
    type Error = Infallible;

    fn get_actor(&self, identifier: &str) -> Result<Option<Actor>, Self::Error> {
        Ok((identifier == IDENTIFIER).then(|| self.actor.clone()))
    }

    fn get_actor_by_id(&self, actor_id: &Iri) -> Result<Option<Actor>, Self::Error> {
        Ok((actor_id == &self.actor.id).then(|| self.actor.clone()))
    }
}

impl ServerStorage for ExampleStorage {
    type Error = ExampleStorageError;

    fn store_follower(&self, follower: &Actor, following: &Iri) -> Result<(), Self::Error> {
        *self
            .latest_follower
            .lock()
            .map_err(|_| ExampleStorageError("follower state lock poisoned"))? =
            Some((follower.clone(), following.clone()));
        tracing::info!(follower = %follower.id, following = %following, "stored follower");
        Ok(())
    }

    fn load_actor_key_pair(&self, actor_id: &Iri) -> Result<Option<ActorKeyPair>, Self::Error> {
        Ok((actor_id == &self.local_actor_id).then(|| self.actor_key_pair.clone()))
    }

    fn remove_follower(&self, follower: &Iri, following: &Iri) -> Result<(), Self::Error> {
        let mut latest_follower = self
            .latest_follower
            .lock()
            .map_err(|_| ExampleStorageError("follower state lock poisoned"))?;
        if latest_follower
            .as_ref()
            .is_some_and(|(stored_follower, stored_following)| {
                stored_follower.id == *follower && stored_following == following
            })
        {
            *latest_follower = None;
            tracing::info!(%follower, %following, "removed follower");
        }
        Ok(())
    }

    fn list_followers(&self, following: &Iri) -> Result<Vec<Iri>, Self::Error> {
        let latest_follower = self
            .latest_follower
            .lock()
            .map_err(|_| ExampleStorageError("follower state lock poisoned"))?;
        Ok(latest_follower
            .as_ref()
            .filter(|(_, stored_following)| stored_following == following)
            .map(|(follower, _)| vec![follower.id.clone()])
            .unwrap_or_default())
    }

    fn store_pending_follow(&self, follow: &PendingFollow) -> Result<(), Self::Error> {
        *self
            .latest_pending_follow
            .lock()
            .map_err(|_| ExampleStorageError("pending Follow state lock poisoned"))? =
            Some(follow.clone());
        tracing::info!(
            local_actor = %follow.local_actor,
            remote_actor = %follow.remote_actor.id,
            follow_activity = %follow.follow_activity,
            "stored pending Follow"
        );
        Ok(())
    }
}

fn local_actor(key_pair: &ActorKeyPair) -> Actor {
    let actor_id = format!("{ORIGIN}/users/{IDENTIFIER}");
    let mut actor = Actor::person(
        actor_id.parse().expect("valid actor IRI"),
        format!("{actor_id}/inbox")
            .parse()
            .expect("valid inbox IRI"),
        format!("{actor_id}/outbox")
            .parse()
            .expect("valid outbox IRI"),
    );
    actor.preferred_username = Some(IDENTIFIER.to_string());
    actor.name = Some("Alice".to_string());
    actor.followers = Some(
        format!("{actor_id}/followers")
            .parse()
            .expect("valid followers collection IRI"),
    );
    actor.endpoints = Some(Endpoints {
        shared_inbox: Some(
            format!("{ORIGIN}/inbox")
                .parse()
                .expect("valid shared inbox IRI"),
        ),
    });
    actor.set_public_key(Reference::object(CryptographicKey::new(
        format!("{actor_id}#main-key")
            .parse()
            .expect("valid actor key IRI"),
        actor.id.clone(),
        key_pair.public_key_pem().to_string(),
    )));
    actor
}

fn remote_actor() -> Actor {
    let mut actor = Actor::person(
        REMOTE_ACTOR_ID.parse().expect("valid remote actor IRI"),
        format!("{ORIGIN}/remote-inbox")
            .parse()
            .expect("valid remote inbox IRI"),
        format!("{REMOTE_ACTOR_ID}/outbox")
            .parse()
            .expect("valid remote outbox IRI"),
    );
    actor.preferred_username = Some("bob".to_string());
    actor
}

async fn remote_actor_document() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/activity+json")],
        Json(remote_actor()),
    )
}

async fn remote_inbox(headers: HeaderMap, body: Bytes) -> StatusCode {
    if !headers.contains_key("signature") {
        return StatusCode::UNAUTHORIZED;
    }

    tracing::info!(body_size = body.len(), "received signed activity");
    StatusCode::ACCEPTED
}

type ExampleServer = FederServer<SingleActorDispatcher, ExampleStorage>;

async fn send_example_follow(server: Arc<ExampleServer>) -> StatusCode {
    let local_actor_id = format!("{ORIGIN}/users/{IDENTIFIER}")
        .parse()
        .expect("valid local actor IRI");
    let remote_actor_id = REMOTE_ACTOR_ID.parse().expect("valid remote actor IRI");
    let follow_id = format!("{ORIGIN}/users/{IDENTIFIER}/activities/follow/example")
        .parse()
        .expect("valid Follow activity IRI");

    match server
        .follow_actor(&local_actor_id, &remote_actor_id, follow_id)
        .await
    {
        Ok(follow) => {
            tracing::info!(follow = %follow.id, "sent Follow");
            StatusCode::ACCEPTED
        }
        Err(error) => {
            tracing::error!(%error, "failed to send Follow");
            StatusCode::BAD_GATEWAY
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let bind: SocketAddr = "127.0.0.1:3000"
        .parse()
        .expect("valid default bind address");
    let actor_key_pair = ActorKeyPair::from_pem(
        ACTOR_PRIVATE_KEY_PEM.to_string(),
        ACTOR_PUBLIC_KEY_PEM.to_string(),
    )
    .expect("bundled example actor key pair is valid");
    let actor = local_actor(&actor_key_pair);
    let storage = ExampleStorage {
        local_actor_id: actor.id.clone(),
        actor_key_pair,
        latest_follower: Mutex::new(None),
        latest_pending_follow: Mutex::new(None),
    };
    let dispatcher = SingleActorDispatcher { actor };
    let server = Arc::new(
        FederServer::with_outbound_address_policy(
            dispatcher,
            storage,
            OutboundAddressPolicy::AllowPrivateAddress,
        )?
        .with_inbox_auth_policy(InboxAuthPolicy::AllowUnsignedInsecureDev),
    );
    let follow_server = Arc::clone(&server);
    let app = build_router_with_state(server)
        .route("/remote/users/bob", get(remote_actor_document))
        .route("/remote-inbox", post(remote_inbox))
        .route(
            "/send-follow",
            post(move || send_example_follow(Arc::clone(&follow_server))),
        );

    tracing::info!(
        bind = %bind,
        actor = %format!("{ORIGIN}/users/{IDENTIFIER}"),
        webfinger = %format!("{ORIGIN}/.well-known/webfinger"),
        inbox = %format!("{ORIGIN}/users/{IDENTIFIER}/inbox"),
        shared_inbox = %format!("{ORIGIN}/inbox"),
        "starting reference ActivityPub server example"
    );

    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .map_err(Error::Bind)?;
    axum::serve(listener, app).await.map_err(Error::Serve)?;

    Ok(())
}
