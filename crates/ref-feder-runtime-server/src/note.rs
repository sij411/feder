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

use feder_vocab::Iri;
use ref_feder_core::{
    ActorDispatcher,
    note::{CreateNoteInput, CreateNoteOutcome, create_note},
    storage::NoteStore,
};

use crate::FederServer;

impl<A, S> FederServer<A, S>
where
    A: ActorDispatcher,
    S: NoteStore,
{
    /// Constructs and persists a local Note without retaining protocol state.
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

        Ok(outcome)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CreateNoteError<A, S> {
    #[error("actor dispatcher failed")]
    ActorDispatcher(A),

    #[error("local actor not found: {0}")]
    LocalActorNotFound(Iri),

    #[error("note storage failed")]
    Storage(S),
}
