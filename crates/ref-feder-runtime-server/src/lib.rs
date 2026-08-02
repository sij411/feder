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
pub mod actor;
pub mod config;
pub mod follow;
pub mod followers;
pub mod inbox;
pub mod negotiation;
pub mod note;
pub mod object;
pub mod send;
pub mod url;
pub mod webfinger;

use std::sync::Arc;

pub use actor::{ActorResolveError, ActorResolver};
use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{get, post},
};
pub use config::OutboundAddressPolicy;
pub use inbox::InboxAuthPolicy;
pub use ref_feder_core::ActorDispatcher;
use ref_feder_core::storage::{NoteStore, ServerStorage};

use crate::send::{ActivitySender, SendError};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to bind server socket")]
    Bind(#[source] std::io::Error),

    #[error("server failed")]
    Serve(#[source] std::io::Error),

    #[error("failed to construct activity sender")]
    ActivitySender(#[from] SendError),

    #[error("failed to construct actor resolver")]
    ActorResolver(#[from] ActorResolveError),
}

pub struct FederServer<A, S> {
    actors: A,
    storage: S,
    resolver: ActorResolver,
    sender: ActivitySender,
    inbox_auth_policy: InboxAuthPolicy,
}

impl<A, S> FederServer<A, S> {
    pub fn new(actors: A, storage: S) -> Result<Self, Error> {
        let policy = OutboundAddressPolicy::PublicOnly;
        let resolver = ActorResolver::new(policy)?;
        let sender = ActivitySender::new(policy)?;
        Ok(Self {
            actors,
            storage,
            resolver,
            sender,
            inbox_auth_policy: InboxAuthPolicy::RequireSigned,
        })
    }

    // for development
    pub fn with_outbound_address_policy(
        actors: A,
        storage: S,
        policy: OutboundAddressPolicy,
    ) -> Result<Self, Error> {
        let resolver = ActorResolver::new(policy)?;
        let sender = ActivitySender::new(policy)?;
        Ok(Self {
            actors,
            storage,
            resolver,
            sender,
            inbox_auth_policy: InboxAuthPolicy::RequireSigned,
        })
    }

    #[must_use]
    pub fn with_inbox_auth_policy(mut self, inbox_auth_policy: InboxAuthPolicy) -> Self {
        self.inbox_auth_policy = inbox_auth_policy;
        self
    }

    pub(crate) fn actors(&self) -> &A {
        &self.actors
    }

    pub(crate) fn storage(&self) -> &S {
        &self.storage
    }

    pub(crate) fn resolver(&self) -> &ActorResolver {
        &self.resolver
    }

    pub(crate) fn sender(&self) -> &ActivitySender {
        &self.sender
    }

    pub(crate) fn inbox_auth_policy(&self) -> InboxAuthPolicy {
        self.inbox_auth_policy
    }
}

pub fn build_router<A, S>(server: FederServer<A, S>) -> Router
where
    A: ActorDispatcher + Send + Sync + 'static,
    S: NoteStore + ServerStorage + Send + Sync + 'static,
{
    build_router_with_state(Arc::new(server))
}

pub fn build_router_with_state<A, S>(server: Arc<FederServer<A, S>>) -> Router
where
    A: ActorDispatcher + Send + Sync + 'static,
    S: NoteStore + ServerStorage + Send + Sync + 'static,
{
    Router::new()
        .route("/users/{identifier}", get(actor::actor::<A, S>))
        .route(
            "/users/{identifier}/followers",
            get(followers::followers::<A, S>),
        )
        .route(
            "/users/{identifier}/posts/{post_id}",
            get(object::note::<A, S>),
        )
        .route("/.well-known/webfinger", get(webfinger::webfinger::<A, S>))
        .route("/users/{identifier}/inbox", post(inbox::inbox::<A, S>))
        .route("/inbox", post(inbox::shared_inbox::<A, S>))
        .layer(DefaultBodyLimit::max(1_048_576))
        .with_state(server)
}
