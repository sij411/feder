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

/// A pending outbound Follow relationship for application-owned storage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingFollow {
    pub local_actor: Iri,
    pub remote_actor: Actor,
    pub follow_activity: Iri,
}

/// The transient result of creating one outbound Follow activity.
///
/// Core retains neither the activity nor its pending relationship. A runtime
/// persists `relationship` before delivering `activity` to the remote actor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateFollowOutcome {
    pub relationship: PendingFollow,
    pub activity: Follow,
}

#[must_use]
pub fn create_follow(
    local_actor: &Actor,
    remote_actor: &Actor,
    follow_id: Iri,
) -> CreateFollowOutcome {
    CreateFollowOutcome {
        relationship: PendingFollow {
            local_actor: local_actor.id.clone(),
            remote_actor: remote_actor.clone(),
            follow_activity: follow_id.clone(),
        },
        activity: Follow::new(
            follow_id,
            Reference::id(local_actor.id.clone()),
            Reference::id(remote_actor.id.clone()),
        ),
    }
}

pub fn receive_accept_follow(
    local_actor: &Actor,
    remote_actor: &Actor,
    pending: &PendingFollow,
    accept: Accept,
) -> Result<(), AcceptFollowError> {
    if pending.local_actor != local_actor.id {
        return Err(AcceptFollowError::WrongLocalActor);
    }
    if pending.remote_actor.id != remote_actor.id || reference_id(&accept.actor) != &remote_actor.id
    {
        return Err(AcceptFollowError::WrongActor);
    }
    if follow_reference_id(&accept.object) != &pending.follow_activity {
        return Err(AcceptFollowError::WrongFollow);
    }
    if let Reference::Object(follow) = &accept.object {
        if reference_id(&follow.actor) != &local_actor.id {
            return Err(AcceptFollowError::WrongFollowActor);
        }
        if reference_id(&follow.object) != &remote_actor.id {
            return Err(AcceptFollowError::WrongFollowObject);
        }
    }

    Ok(())
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

fn follow_reference_id(reference: &Reference<Follow>) -> &Iri {
    match reference {
        Reference::Id(id) => id,
        Reference::Object(follow) => &follow.id,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AcceptFollowError {
    WrongActor,
    WrongFollow,
    WrongFollowActor,
    WrongFollowObject,
    WrongLocalActor,
}

impl fmt::Display for AcceptFollowError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongActor => formatter.write_str("Accept actor does not match remote actor"),
            Self::WrongFollow => formatter.write_str("Accept does not reference pending Follow"),
            Self::WrongFollowActor => {
                formatter.write_str("accepted Follow actor does not match local actor")
            }
            Self::WrongFollowObject => {
                formatter.write_str("accepted Follow does not target remote actor")
            }
            Self::WrongLocalActor => {
                formatter.write_str("pending Follow does not belong to local actor")
            }
        }
    }
}

impl core::error::Error for AcceptFollowError {}

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
