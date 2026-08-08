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

use std::{convert::Infallible, env, error::Error, net::SocketAddr, path::PathBuf};

use feder_core::{ActorDispatcher, key::ActorKeyPair};
use feder_server::{
    FederServer, InboxAuthPolicy, OutboundAddressPolicy, build_router, storage::SqliteStore,
};
use feder_vocab::{Actor, CryptographicKey, Endpoints, Iri, Reference};
use rand_core::OsRng;

const IDENTIFIER: &str = "alice";
const ORIGIN: &str = "http://127.0.0.1:3000";
const HANDLE_HOST: &str = "127.0.0.1:3000";
const DEFAULT_DATABASE_PATH: &str = "feder.sqlite3";

struct SingleActorDispatcher {
    actor: Actor,
}

impl ActorDispatcher for SingleActorDispatcher {
    type Error = Infallible;

    fn get_actor(&self, identifier: &str) -> Result<Option<Actor>, Self::Error> {
        Ok((identifier == IDENTIFIER).then(|| self.actor.clone()))
    }

    fn get_actor_by_id(&self, actor_id: &Iri) -> Result<Option<Actor>, Self::Error> {
        Ok((actor_id == &self.actor.id).then(|| self.actor.clone()))
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let database_path = env::var_os("FEDER_DATABASE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_DATABASE_PATH));
    let storage = SqliteStore::open(&database_path)?;
    let actor_id = format!("{ORIGIN}/users/{IDENTIFIER}")
        .parse()
        .expect("valid actor IRI");
    let actor_key_pair = storage.load_or_generate_actor_key_pair(&actor_id, &mut OsRng)?;
    let dispatcher = SingleActorDispatcher {
        actor: local_actor(&actor_key_pair),
    };
    let server = FederServer::with_outbound_address_policy(
        dispatcher,
        storage,
        HANDLE_HOST,
        OutboundAddressPolicy::AllowPrivateAddress,
    )?
    .with_inbox_auth_policy(InboxAuthPolicy::AllowUnsignedInsecureDev);
    let app = build_router(server);
    let bind: SocketAddr = "127.0.0.1:3000".parse()?;

    tracing::info!(
        bind = %bind,
        actor = %actor_id,
        database = %database_path.display(),
        "starting Feder single-user example"
    );

    let listener = tokio::net::TcpListener::bind(bind).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
