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

use std::collections::HashSet;

use feder_core::{
    ActorDispatcher,
    note::{CreateNoteInput, CreateNoteOutcome, NoteRecipient, create_note},
    storage::{FollowerDeliveryStore, NoteStore},
};
use feder_vocab::Iri;

use crate::{ActorResolveError, FederServer, send::SendError};

impl<A, S> FederServer<A, S>
where
    A: ActorDispatcher,
    S: FollowerDeliveryStore + NoteStore,
{
    /// Constructs, persists, and delivers a local Note without retaining state.
    ///
    /// Persistence occurs before delivery. Every independent recipient is
    /// attempted even if another resolution or delivery fails.
    pub async fn create_note(
        &self,
        local_actor_id: &Iri,
        input: CreateNoteInput,
    ) -> Result<CreateNoteOutcome, CreateNoteError<A::Error, S::Error>> {
        let local_actor = self
            .actors()
            .get_actor_by_id(local_actor_id)
            .map_err(CreateNoteError::ActorDispatcher)?
            .ok_or_else(|| CreateNoteError::LocalActorNotFound(local_actor_id.clone()))?;
        let outcome = create_note(&local_actor, input);

        self.storage()
            .store_note(&outcome.note)
            .map_err(CreateNoteError::Storage)?;

        let (inboxes, actor_resolve_error) = self
            .resolve_note_recipients(&outcome.recipients)
            .await
            .map_err(CreateNoteError::Storage)?;
        if inboxes.is_empty() {
            if let Some(error) = actor_resolve_error {
                return Err(CreateNoteError::ActorResolver(error));
            }
            return Ok(outcome);
        }

        let key_pair = self
            .storage()
            .load_actor_key_pair(&local_actor.id)
            .map_err(CreateNoteError::Storage)?
            .ok_or_else(|| CreateNoteError::MissingActorKey(local_actor.id.clone()))?;
        let mut first_send_error = None;
        for inbox in inboxes {
            if let Err(error) = self
                .sender()
                .send_activity(&local_actor, &key_pair, &outcome.activity, &inbox)
                .await
                && first_send_error.is_none()
            {
                first_send_error = Some(error);
            }
        }

        if let Some(error) = first_send_error {
            Err(CreateNoteError::ActivitySender(error))
        } else if let Some(error) = actor_resolve_error {
            Err(CreateNoteError::ActorResolver(error))
        } else {
            Ok(outcome)
        }
    }

    async fn resolve_note_recipients(
        &self,
        recipients: &[NoteRecipient],
    ) -> Result<(Vec<Iri>, Option<ActorResolveError>), S::Error> {
        let mut inboxes = Vec::new();
        let mut covered_actor_ids = HashSet::new();
        let mut seen_inboxes = HashSet::new();
        let mut first_actor_resolve_error = None;

        for recipient in recipients {
            let NoteRecipient::Followers(local_actor_id) = recipient else {
                continue;
            };
            for target in self
                .storage()
                .list_follower_delivery_targets(local_actor_id)?
            {
                covered_actor_ids.insert(target.actor_id);
                let inbox = target.shared_inbox.unwrap_or(target.inbox);
                if seen_inboxes.insert(inbox.clone()) {
                    inboxes.push(inbox);
                }
            }
        }

        for recipient in recipients {
            let NoteRecipient::Actor(actor_id) = recipient else {
                continue;
            };
            if covered_actor_ids.contains(actor_id) {
                continue;
            }
            match self.resolver().resolve(actor_id).await {
                Ok(actor) => {
                    covered_actor_ids.insert(actor.id.clone());
                    if seen_inboxes.insert(actor.inbox.clone()) {
                        inboxes.push(actor.inbox);
                    }
                }
                Err(error) if first_actor_resolve_error.is_none() => {
                    first_actor_resolve_error = Some(error);
                }
                Err(_) => {}
            }
        }

        Ok((inboxes, first_actor_resolve_error))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CreateNoteError<A, S> {
    #[error("actor dispatcher failed")]
    ActorDispatcher(A),

    #[error("local actor not found: {0}")]
    LocalActorNotFound(Iri),

    #[error("note or follower storage failed")]
    Storage(S),

    #[error("failed to resolve a Note recipient")]
    ActorResolver(#[source] ActorResolveError),

    #[error("local actor has no stored signing key: {0}")]
    MissingActorKey(Iri),

    #[error("failed to send Create activity")]
    ActivitySender(#[source] SendError),
}
