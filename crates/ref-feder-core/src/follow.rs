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

use feder_vocab::{Accept, Actor, Follow, Iri, Reference};

/// The transient result of accepting one valid Follow activity.
///
/// Core does not retain this value or write it to storage. A runtime persists
/// `follower` and `following`, then delivers `accept` to `recipient_inbox`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FollowOutcome {
    pub follower: Actor,
    pub following: Iri,
    pub accept: Accept,
    pub recipient_inbox: Iri,
}

pub fn receive_follow(
    local_actor: &Actor,
    remote_actor: &Actor,
    mut follow: Follow,
    accept_id: Iri,
) -> Result<FollowOutcome, FollowError> {
    if reference_id(&follow.object) != &local_actor.id {
        return Err(FollowError::WrongObject);
    }
    if reference_id(&follow.actor) != &remote_actor.id {
        return Err(FollowError::WrongActor);
    }

    follow.actor = Reference::object(remote_actor.clone());

    Ok(FollowOutcome {
        follower: remote_actor.clone(),
        following: local_actor.id.clone(),
        accept: Accept::new(
            accept_id,
            Reference::id(local_actor.id.clone()),
            Reference::object(follow),
        ),
        recipient_inbox: remote_actor.inbox.clone(),
    })
}

fn reference_id(reference: &Reference<Actor>) -> &Iri {
    match reference {
        Reference::Id(id) => id,
        Reference::Object(actor) => &actor.id,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FollowError {
    WrongActor,
    WrongObject,
}

impl fmt::Display for FollowError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongActor => formatter.write_str("Follow actor does not match remote actor"),
            Self::WrongObject => formatter.write_str("Follow does not target the local actor"),
        }
    }
}

impl core::error::Error for FollowError {}
