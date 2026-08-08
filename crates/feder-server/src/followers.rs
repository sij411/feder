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

use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use feder_core::{ActorDispatcher, storage::ServerStorage};
use feder_vocab::OrderedCollection;

use crate::{FederServer, negotiation::accepts_activitypub};

pub async fn followers<A, S>(
    State(server): State<Arc<FederServer<A, S>>>,
    Path(identifier): Path<String>,
    headers: HeaderMap,
) -> Result<Response, StatusCode>
where
    A: ActorDispatcher,
    S: ServerStorage,
{
    let actor = server
        .actors()
        .get_actor(&identifier)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    if !accepts_activitypub(&headers) {
        return Ok(([(header::VARY, "Accept")], StatusCode::NOT_ACCEPTABLE).into_response());
    }

    let followers = server
        .storage()
        .list_followers(&actor.id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let total_items =
        u64::try_from(followers.len()).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let collection_id = actor
        .followers
        .clone()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let collection = OrderedCollection::new(collection_id, total_items, followers);

    Ok((
        [
            (header::CONTENT_TYPE, "application/activity+json"),
            (header::VARY, "Accept"),
        ],
        Json(collection),
    )
        .into_response())
}
