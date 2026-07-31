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

use crate::FederServer;
use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use ref_feder_core::actor::ActorDispatcher;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct WebFingerQuery {
    resource: Option<String>,
}

#[derive(Serialize)]
pub struct WebFingerLink {
    rel: &'static str,
    #[serde(rename = "type")]
    media_type: &'static str,
    href: String,
}

#[derive(Serialize)]
pub struct WebFingerResponse {
    subject: String,
    aliases: Vec<String>,
    links: Vec<WebFingerLink>,
}

pub async fn webfinger<A>(
    State(server): State<FederServer<A>>,
    headers: HeaderMap,
    Query(query): Query<WebFingerQuery>,
) -> Result<Response, StatusCode>
where
    A: ActorDispatcher,
{
    let resource = query.resource.ok_or(StatusCode::BAD_REQUEST)?;

    let account = resource
        .strip_prefix("acct:")
        .ok_or(StatusCode::BAD_REQUEST)?;

    let (identifier, resource_host) = account.rsplit_once('@').ok_or(StatusCode::BAD_REQUEST)?;

    if identifier.is_empty() || resource_host.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let request_host = request_host(&headers).ok_or(StatusCode::BAD_REQUEST)?;

    if !resource_host.eq_ignore_ascii_case(request_host) {
        return Err(StatusCode::NOT_FOUND);
    }

    let actor = server
        .get_actor(identifier)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let actor_id = actor.id.to_string();

    Ok((
        [(header::CONTENT_TYPE, "application/jrd+json")],
        Json(WebFingerResponse {
            subject: resource,
            aliases: vec![actor_id.clone()],
            links: vec![WebFingerLink {
                rel: "self",
                media_type: "application/activity+json",
                href: actor_id,
            }],
        }),
    )
        .into_response())
}

fn request_host(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
}
