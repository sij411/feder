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
    Router,
    extract::DefaultBodyLimit,
    routing::{get, post},
};
pub use ref_feder_core::ActorDispatcher;

pub mod actor;
pub mod inbox;
pub mod negotiation;
pub mod webfinger;

pub use inbox::{ActivitySender, FollowStore, InboxAuthPolicy, RemoteResolver};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to bind server socket")]
    Bind(#[source] std::io::Error),

    #[error("server failed")]
    Serve(#[source] std::io::Error),
}

pub struct FederServer<A, S> {
    actors: A,
    services: S,
    inbox_auth_policy: InboxAuthPolicy,
}

impl<A, S> FederServer<A, S> {
    pub fn new(actors: A, services: S) -> Self {
        Self {
            actors,
            services,
            inbox_auth_policy: InboxAuthPolicy::RequireSigned,
        }
    }

    #[must_use]
    pub fn with_inbox_auth_policy(mut self, inbox_auth_policy: InboxAuthPolicy) -> Self {
        self.inbox_auth_policy = inbox_auth_policy;
        self
    }

    pub(crate) fn actors(&self) -> &A {
        &self.actors
    }

    pub(crate) fn services(&self) -> &S {
        &self.services
    }

    pub(crate) fn inbox_auth_policy(&self) -> InboxAuthPolicy {
        self.inbox_auth_policy
    }
}

pub fn build_router<A, S>(server: FederServer<A, S>) -> Router
where
    A: ActorDispatcher + Send + Sync + 'static,
    S: ActivitySender + FollowStore + RemoteResolver + Send + Sync + 'static,
{
    let server = Arc::new(server);

    Router::new()
        .route("/users/{identifier}", get(actor::actor::<A, S>))
        .route("/.well-known/webfinger", get(webfinger::webfinger::<A, S>))
        .route("/users/{identifier}/inbox", post(inbox::inbox::<A, S>))
        .layer(DefaultBodyLimit::max(1_048_576))
        .with_state(server)
}
