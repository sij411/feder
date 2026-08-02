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
use feder_vocab::Iri;
use ref_feder_core::{ActorDispatcher, note::is_public_note, storage::NoteStore};

use crate::{FederServer, negotiation::accepts_activitypub};

pub async fn note<A, S>(
    State(server): State<Arc<FederServer<A, S>>>,
    Path((identifier, post_id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Response, StatusCode>
where
    A: ActorDispatcher,
    S: NoteStore,
{
    let actor = server
        .actors()
        .get_actor(&identifier)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let note_id = note_id(&actor.id, &post_id)?;
    let note = server
        .storage()
        .load_note(&note_id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    if !is_public_note(&note) {
        return Err(StatusCode::NOT_FOUND);
    }
    if !accepts_activitypub(&headers) {
        return Ok(([(header::VARY, "Accept")], StatusCode::NOT_ACCEPTABLE).into_response());
    }

    Ok((
        [
            (header::CONTENT_TYPE, "application/activity+json"),
            (header::VARY, "Accept"),
        ],
        Json(note),
    )
        .into_response())
}

fn note_id(actor_id: &Iri, post_id: &str) -> Result<Iri, StatusCode> {
    let mut url =
        url::Url::parse(actor_id.as_str()).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    url.set_query(None);
    url.set_fragment(None);
    url.path_segments_mut()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .pop_if_empty()
        .push("posts")
        .push(post_id);
    url.as_str()
        .parse()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}
