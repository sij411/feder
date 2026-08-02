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

use alloc::vec::Vec;

use feder_vocab::{Actor, Iri, Note};

use crate::{follow::PendingFollow, key::ActorKeyPair};

pub trait Storage {
    type Error;
}

pub trait ServerStorage: Storage {
    fn store_follower(&self, follower: &Actor, following: &Iri) -> Result<(), Self::Error>;

    fn load_actor_key_pair(&self, actor_id: &Iri) -> Result<Option<ActorKeyPair>, Self::Error>;

    fn remove_follower(&self, follower: &Iri, following: &Iri) -> Result<(), Self::Error>;

    fn list_followers(&self, following: &Iri) -> Result<Vec<Iri>, Self::Error>;

    fn store_pending_follow(&self, follow: &PendingFollow) -> Result<(), Self::Error>;

    fn load_pending_follow(
        &self,
        follow_activity: &Iri,
    ) -> Result<Option<PendingFollow>, Self::Error>;

    /// Confirm `expected` only if that exact relationship is still pending.
    fn confirm_pending_follow(&self, expected: &PendingFollow) -> Result<bool, Self::Error>;
}

pub trait NoteStore: Storage {
    fn store_note(&self, note: &Note) -> Result<(), Self::Error>;
}

pub trait FollowerDeliveryStore: ServerStorage {
    fn list_follower_actors(&self, local_actor: &Iri) -> Result<Vec<Actor>, Self::Error>;
}
