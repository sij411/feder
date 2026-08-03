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

use feder_core::{ActorDispatcher, follow::create_follow, storage::ServerStorage};
use feder_vocab::{Follow, Iri};

use crate::{ActorResolveError, FederServer, send::SendError};

impl<A, S> FederServer<A, S>
where
    A: ActorDispatcher,
    S: ServerStorage,
{
    /// Creates, persists, and delivers a Follow initiated by a local actor.
    ///
    /// The pending relationship is persisted before delivery so applications
    /// can retain the intent when delivery fails and implement retries.
    pub async fn follow_actor(
        &self,
        local_actor_id: &Iri,
        remote_actor_id: &Iri,
        follow_id: Iri,
    ) -> Result<Follow, FollowActorError<A::Error, S::Error>> {
        let local_actor = self
            .actors()
            .get_actor_by_id(local_actor_id)
            .map_err(FollowActorError::ActorDispatcher)?
            .ok_or_else(|| FollowActorError::LocalActorNotFound(local_actor_id.clone()))?;
        let remote_actor = self
            .resolver()
            .resolve(remote_actor_id)
            .await
            .map_err(FollowActorError::ActorResolver)?;
        let outcome = create_follow(&local_actor, &remote_actor, follow_id);

        self.storage()
            .store_pending_follow(&outcome.relationship)
            .map_err(FollowActorError::Storage)?;
        let key_pair = self
            .storage()
            .load_actor_key_pair(&local_actor.id)
            .map_err(FollowActorError::Storage)?
            .ok_or_else(|| FollowActorError::MissingActorKey(local_actor.id.clone()))?;
        let inbox = remote_actor
            .endpoints
            .as_ref()
            .and_then(|endpoints| endpoints.shared_inbox.as_ref())
            .unwrap_or(&remote_actor.inbox);

        self.sender()
            .send_activity(&local_actor, &key_pair, &outcome.activity, inbox)
            .await
            .map_err(FollowActorError::ActivitySender)?;

        Ok(outcome.activity)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum FollowActorError<A, S> {
    #[error("actor dispatcher failed")]
    ActorDispatcher(A),

    #[error("local actor not found: {0}")]
    LocalActorNotFound(Iri),

    #[error("failed to resolve remote actor")]
    ActorResolver(#[source] ActorResolveError),

    #[error("server storage failed")]
    Storage(S),

    #[error("local actor has no stored signing key: {0}")]
    MissingActorKey(Iri),

    #[error("failed to send Follow activity")]
    ActivitySender(#[source] SendError),
}
