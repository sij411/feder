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

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use feder_vocab::Actor;

use ref_feder_core::actor::ActorDispatcher;

use crate::{FederServer, negotiation::accepts_activitypub};

impl<A> ActorDispatcher for FederServer<A>
where
    A: ActorDispatcher,
{
    type Error = A::Error;

    fn get_actor(&self, identifier: &str) -> Result<Option<Actor>, Self::Error> {
        self.actors.get_actor(identifier)
    }
}

pub async fn actor<A>(
    State(server): State<FederServer<A>>,
    Path(identifier): Path<String>,
    headers: HeaderMap,
) -> Result<Response, StatusCode>
where
    A: ActorDispatcher,
{
    if !accepts_activitypub(&headers) {
        return Ok(([(header::VARY, "Accept")], StatusCode::NOT_ACCEPTABLE).into_response());
    }

    let actor = server
        .get_actor(&identifier)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok((
        [
            (header::CONTENT_TYPE, "application/activity+json"),
            (header::VARY, "Accept"),
        ],
        Json(actor),
    )
        .into_response())
}
