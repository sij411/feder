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

//! Experimental reference implementation of Feder's protocol core.
//!
//! This crate develops the replacement architecture alongside `feder-core`.
//! Its API is intentionally unstable until the core ownership boundary has
//! been proven against the existing runtime and Federog.
#![no_std]

extern crate alloc;

pub use feder_vocab as vocab;
use feder_vocab::{Actor, Iri};

pub mod follow;
#[cfg(feature = "http-signatures")]
pub mod key;
pub mod note;
pub mod storage;
pub mod undo;

#[derive(Debug, Default)]
pub struct FederCore;

pub trait ActorDispatcher {
    type Error;

    fn get_actor(&self, identifier: &str) -> Result<Option<Actor>, Self::Error>;

    fn get_actor_by_id(&self, actor_id: &Iri) -> Result<Option<Actor>, Self::Error>;
}
