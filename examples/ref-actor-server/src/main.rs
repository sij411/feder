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

use std::{convert::Infallible, fmt, net::SocketAddr, sync::Mutex};

use axum::{
    body::Bytes,
    http::{HeaderMap, StatusCode},
    routing::post,
};
use feder_vocab::{Actor, CryptographicKey, Endpoints, Iri, Reference};
use ref_feder_core::{key::ActorKeyPair, storage::ServerStorage};
use ref_feder_runtime_server::{
    ActorDispatcher, Error, FederServer, InboxAuthPolicy, OutboundAddressPolicy, build_router,
};

const IDENTIFIER: &str = "alice";
const ORIGIN: &str = "http://127.0.0.1:3000";
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

async fn remote_inbox(headers: HeaderMap, body: Bytes) -> StatusCode {
    if !headers.contains_key("signature") {
        return StatusCode::UNAUTHORIZED;
    }

    tracing::info!(body_size = body.len(), "received signed Accept activity");
    StatusCode::ACCEPTED
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
    };
    let dispatcher = SingleActorDispatcher { actor };
    let server = FederServer::with_outbound_address_policy(
        dispatcher,
        storage,
        OutboundAddressPolicy::AllowPrivateAddress,
    )?
    .with_inbox_auth_policy(InboxAuthPolicy::AllowUnsignedInsecureDev);
    let app = build_router(server).route("/remote-inbox", post(remote_inbox));

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
