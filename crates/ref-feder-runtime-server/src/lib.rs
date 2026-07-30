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

//! Experimental reference implementation of Feder's server runtime.
//!
//! This crate develops runtime orchestration against `ref-feder-core` while
//! the production `feder-runtime-server` remains operational. Its API is
//! intentionally unstable during the architecture refactoring.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use feder_vocab::Actor;

pub use ref_feder_core::actor::ActorProvider;
use ref_feder_core::actor::find_actor;

use crate::negotiation::accepts_activitypub;

mod negotiation;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to bind server socket")]
    Bind(#[source] std::io::Error),

    #[error("server failed")]
    Serve(#[source] std::io::Error),
    // #[error("storage failed")]
    // Storage(#[from] crate::storage::StoreError),
}

pub struct FederServer<A> {
    actors: Arc<A>,
}

impl<A> Clone for FederServer<A> {
    fn clone(&self) -> Self {
        Self {
            actors: Arc::clone(&self.actors),
        }
    }
}

impl<A> FederServer<A> {
    pub fn new(actors: A) -> Self {
        Self {
            actors: Arc::new(actors),
        }
    }
}

impl<A> ActorProvider for FederServer<A>
where
    A: ActorProvider,
{
    type Error = A::Error;

    fn find_actor(&self, identifier: &str) -> Result<Option<Actor>, Self::Error> {
        self.actors.find_actor(identifier)
    }
}

pub async fn actor<A>(
    State(server): State<FederServer<A>>,
    Path(identifier): Path<String>,
    headers: HeaderMap,
) -> Result<Response, StatusCode>
where
    A: ActorProvider,
{
    if !accepts_activitypub(&headers) {
        return Ok(([(header::VARY, "Accept")], StatusCode::NOT_ACCEPTABLE).into_response());
    }

    let actor = find_actor(&server, &identifier)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok((
        [
            (header::CONTENT_TYPE, "application/activity+json"),
            (header::VARY, "Accept"),
        ],
        Json(actor),
    )
        .into_response())
}

pub fn build_router<A>(server: FederServer<A>) -> Router
where
    A: ActorProvider + Send + Sync + 'static,
{
    Router::new()
        .route("/users/{identifier}", get(actor::<A>))
        .with_state(server)
}
