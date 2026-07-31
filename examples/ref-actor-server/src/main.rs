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
    future::{Future, ready},
    net::SocketAddr,
    sync::Mutex,
};

use feder_vocab::{Accept, Actor, CryptographicKey, Endpoints, Iri, Reference};
use ref_feder_runtime_server::{
    ActivitySender, ActorDispatcher, Error, FederServer, FollowStore, InboxAuthPolicy,
    RemoteResolver, build_router,
};

const IDENTIFIER: &str = "alice";
const ORIGIN: &str = "http://127.0.0.1:3000";

struct SingleActorDispatcher {
    actor: Actor,
}

struct ExampleServices {
    remote_actor: Actor,
    latest_follower: Mutex<Option<(Actor, Iri)>>,
    latest_accept: Mutex<Option<(Accept, Iri)>>,
}

#[derive(Debug)]
struct ExampleServiceError(&'static str);

impl fmt::Display for ExampleServiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl std::error::Error for ExampleServiceError {}

impl ActorDispatcher for SingleActorDispatcher {
    type Error = Infallible;

    fn get_actor(&self, identifier: &str) -> Result<Option<Actor>, Self::Error> {
        Ok((identifier == IDENTIFIER).then(|| self.actor.clone()))
    }
}

impl FollowStore for ExampleServices {
    type Error = ExampleServiceError;

    fn store_follower(&self, follower: &Actor, following: &Iri) -> Result<(), Self::Error> {
        *self
            .latest_follower
            .lock()
            .map_err(|_| ExampleServiceError("follower state lock poisoned"))? =
            Some((follower.clone(), following.clone()));
        tracing::info!(follower = %follower.id, following = %following, "stored follower");
        Ok(())
    }
}

impl RemoteResolver for ExampleServices {
    type Error = ExampleServiceError;

    fn resolve_actor<'a>(
        &'a self,
        actor_id: &'a Iri,
    ) -> impl Future<Output = Result<Actor, Self::Error>> + Send + 'a {
        ready(if actor_id == &self.remote_actor.id {
            Ok(self.remote_actor.clone())
        } else {
            Err(ExampleServiceError("remote actor not found"))
        })
    }

    fn resolve_key<'a>(
        &'a self,
        key_id: &'a Iri,
    ) -> impl Future<Output = Result<CryptographicKey, Self::Error>> + Send + 'a {
        ready(match self.remote_actor.public_key.as_ref() {
            Some(Reference::Object(key)) if key.id == *key_id => Ok((**key).clone()),
            _ => Err(ExampleServiceError("remote key not found")),
        })
    }
}

impl ActivitySender for ExampleServices {
    type Error = ExampleServiceError;

    fn send_accept<'a>(
        &'a self,
        _local_actor: &'a Actor,
        accept: &'a Accept,
        inbox: &'a Iri,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a {
        let result = self
            .latest_accept
            .lock()
            .map_err(|_| ExampleServiceError("delivery state lock poisoned"))
            .map(|mut latest_accept| {
                *latest_accept = Some((accept.clone(), inbox.clone()));
                tracing::info!(activity = %accept.id, recipient = %inbox, "delivered Accept");
            });
        ready(result)
    }
}

fn local_actor() -> Actor {
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
    actor
}

fn remote_actor() -> Actor {
    let actor_id = "https://remote.example/users/bob";
    let mut actor = Actor::person(
        actor_id.parse().expect("valid remote actor IRI"),
        format!("{actor_id}/inbox")
            .parse()
            .expect("valid remote inbox IRI"),
        format!("{actor_id}/outbox")
            .parse()
            .expect("valid remote outbox IRI"),
    );
    actor.set_public_key(Reference::object(CryptographicKey::new(
        format!("{actor_id}#main-key")
            .parse()
            .expect("valid remote key IRI"),
        actor.id.clone(),
        "unused by the unsigned development example".to_string(),
    )));
    actor
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let bind: SocketAddr = "127.0.0.1:3000"
        .parse()
        .expect("valid default bind address");
    let dispatcher = SingleActorDispatcher {
        actor: local_actor(),
    };
    let services = ExampleServices {
        remote_actor: remote_actor(),
        latest_follower: Mutex::new(None),
        latest_accept: Mutex::new(None),
    };
    let server = FederServer::new(dispatcher, services)
        .with_inbox_auth_policy(InboxAuthPolicy::AllowUnsignedInsecureDev);
    let app = build_router(server);

    tracing::info!(
        bind = %bind,
        actor = %format!("{ORIGIN}/users/{IDENTIFIER}"),
        webfinger = %format!("{ORIGIN}/.well-known/webfinger"),
        inbox = %format!("{ORIGIN}/users/{IDENTIFIER}/inbox"),
        "starting reference ActivityPub server example"
    );

    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .map_err(Error::Bind)?;
    axum::serve(listener, app).await.map_err(Error::Serve)?;

    Ok(())
}
