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

use axum::{Router, routing::get};
pub use ref_feder_core::actor::ActorDispatcher;

pub mod actor;
mod negotiation;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to bind server socket")]
    Bind(#[source] std::io::Error),

    #[error("server failed")]
    Serve(#[source] std::io::Error),
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

pub fn build_router<A>(server: FederServer<A>) -> Router
where
    A: ActorDispatcher + Send + Sync + 'static,
{
    Router::new()
        .route("/users/{identifier}", get(actor::actor::<A>))
        .with_state(server)
}
