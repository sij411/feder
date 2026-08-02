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

use core::fmt;

use feder_vocab::{Actor, Iri, Reference, Undo};

/// The transient result of undoing one valid Follow activity.
///
/// Core does not retain this value or remove anything from storage. A runtime
/// passes `follower` and `following` to its follower-removal capability.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UndoFollowOutcome {
    pub follower: Iri,
    pub following: Iri,
}

pub fn receive_undo_follow(
    local_actor: &Actor,
    remote_actor: &Actor,
    undo: Undo,
) -> Result<UndoFollowOutcome, UndoFollowError> {
    if reference_id(&undo.actor) != &remote_actor.id {
        return Err(UndoFollowError::WrongActor);
    }

    let Reference::Object(follow) = undo.object else {
        return Err(UndoFollowError::LinkedFollow);
    };
    if reference_id(&follow.actor) != &remote_actor.id {
        return Err(UndoFollowError::WrongActor);
    }
    if reference_id(&follow.object) != &local_actor.id {
        return Err(UndoFollowError::WrongObject);
    }

    Ok(UndoFollowOutcome {
        follower: remote_actor.id.clone(),
        following: local_actor.id.clone(),
    })
}

fn reference_id(reference: &Reference<Actor>) -> &Iri {
    match reference {
        Reference::Id(id) => id,
        Reference::Object(actor) => &actor.id,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UndoFollowError {
    LinkedFollow,
    WrongActor,
    WrongObject,
}

impl fmt::Display for UndoFollowError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LinkedFollow => formatter.write_str("Undo does not embed its Follow activity"),
            Self::WrongActor => formatter.write_str("Undo actor does not own the embedded Follow"),
            Self::WrongObject => formatter.write_str("undone Follow does not target local actor"),
        }
    }
}

impl core::error::Error for UndoFollowError {}
