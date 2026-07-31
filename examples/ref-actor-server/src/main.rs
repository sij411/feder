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

use std::{convert::Infallible, net::SocketAddr};

use feder_vocab::{Actor, Endpoints};
use ref_feder_runtime_server::{ActorDispatcher, Error, FederServer, build_router};

const IDENTIFIER: &str = "alice";
const ORIGIN: &str = "http://127.0.0.1:3000";

struct SingleActorDispatcher {
    actor: Actor,
}

impl ActorDispatcher for SingleActorDispatcher {
    type Error = Infallible;

    fn get_actor(&self, identifier: &str) -> Result<Option<Actor>, Self::Error> {
        Ok((identifier == IDENTIFIER).then(|| self.actor.clone()))
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
    let app = build_router(FederServer::new(dispatcher));

    tracing::info!(
        bind = %bind,
        actor = %format!("{ORIGIN}/users/{IDENTIFIER}"),
        "starting reference actor endpoint example"
    );

    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .map_err(Error::Bind)?;
    axum::serve(listener, app).await.map_err(Error::Serve)?;

    Ok(())
}
