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

use std::{
    collections::{BTreeMap, HashSet},
    sync::Arc,
    time::{Duration, SystemTime},
};

use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{
        HeaderMap, Method, StatusCode, Uri,
        header::{CONTENT_TYPE, HOST},
        uri::Authority,
    },
    response::{IntoResponse, Response},
};
use feder_vocab::{Accept, Actor, CryptographicKey, Follow, Iri, Reference, Undo};
use mime::Mime;
use ref_feder_core::{
    ActorDispatcher,
    follow::{
        AcceptFollowError, FollowError, PendingFollow, receive_accept_follow, receive_follow,
    },
    key::{create_sha256_digest_header, verify_draft_cavage},
    storage::ServerStorage,
    undo::{UndoFollowError, receive_undo_follow},
};
use serde_json::{Value, from_slice, from_value};

use crate::{ActorResolver, FederServer};

const MAX_SIGNATURE_AGE: Duration = Duration::from_secs(65 * 60);
const MAX_CLOCK_SKEW: Duration = Duration::from_secs(60 * 60);
const ACTIVITYPUB_CONTENT_TYPES: &[&str] = &["application/activity+json", "application/ld+json"];

// FIXME: Remove this policy once the reference example sends signed Follow
// requests. The built-in inbox should always verify requests; applications
// that need different authentication can build an inbox and call core directly.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum InboxAuthPolicy {
    AllowUnsignedInsecureDev,
    #[default]
    RequireSigned,
}

struct InboxRequest {
    headers: HeaderMap,
    method: Method,
    uri: Uri,
    body: Bytes,
}

pub async fn inbox<A, S>(
    State(server): State<Arc<FederServer<A, S>>>,
    Path(identifier): Path<String>,
    headers: HeaderMap,
    method: Method,
    uri: Uri,
    body: Bytes,
) -> Result<Response, StatusCode>
where
    A: ActorDispatcher,
    S: ServerStorage,
{
    let local_actor = server
        .actors()
        .get_actor(&identifier)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let expected_inbox = local_actor.inbox.clone();
    let (request, value) = parse_inbox_request(headers, method, uri, body)?;

    receive_activity(&server, local_actor, expected_inbox, request, value, None).await
}

pub async fn shared_inbox<A, S>(
    State(server): State<Arc<FederServer<A, S>>>,
    headers: HeaderMap,
    method: Method,
    uri: Uri,
    body: Bytes,
) -> Result<Response, StatusCode>
where
    A: ActorDispatcher,
    S: ServerStorage,
{
    let (request, value) = parse_inbox_request(headers, method, uri, body)?;
    let Some((target_id, pending_follow)) = shared_inbox_target(server.storage(), &value)? else {
        return Ok(StatusCode::ACCEPTED.into_response());
    };
    let Some(local_actor) = server
        .actors()
        .get_actor_by_id(&target_id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    else {
        return Ok(StatusCode::ACCEPTED.into_response());
    };
    let Some(expected_inbox) = local_actor
        .endpoints
        .as_ref()
        .and_then(|endpoints| endpoints.shared_inbox.clone())
    else {
        return Ok(StatusCode::ACCEPTED.into_response());
    };

    receive_activity(
        &server,
        local_actor,
        expected_inbox,
        request,
        value,
        pending_follow,
    )
    .await
}

fn parse_inbox_request(
    headers: HeaderMap,
    method: Method,
    uri: Uri,
    body: Bytes,
) -> Result<(InboxRequest, Value), StatusCode> {
    let content_type = headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<Mime>().ok());
    if !content_type
        .is_some_and(|media_type| ACTIVITYPUB_CONTENT_TYPES.contains(&media_type.essence_str()))
    {
        return Err(StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    let request = InboxRequest {
        headers,
        method,
        uri,
        body,
    };
    let value: Value = from_slice(&request.body).map_err(|_| StatusCode::BAD_REQUEST)?;

    Ok((request, value))
}

async fn receive_activity<A, S>(
    server: &FederServer<A, S>,
    local_actor: Actor,
    expected_inbox: Iri,
    request: InboxRequest,
    value: Value,
    pending_follow: Option<PendingFollow>,
) -> Result<Response, StatusCode>
where
    A: ActorDispatcher,
    S: ServerStorage,
{
    let activity_actor_id = activity_actor_id(&value);
    let verified_actor = match server.inbox_auth_policy() {
        InboxAuthPolicy::AllowUnsignedInsecureDev => None,
        InboxAuthPolicy::RequireSigned => Some(
            verify_signed_request(
                server.resolver(),
                &request,
                activity_actor_id.as_ref().ok_or(StatusCode::UNAUTHORIZED)?,
                &expected_inbox,
            )
            .await?,
        ),
    };

    match value.get("type").and_then(Value::as_str) {
        Some("Follow") => {}
        Some("Accept") => {
            let accept: Accept = from_value(value).map_err(|_| StatusCode::BAD_REQUEST)?;
            let remote_actor = match verified_actor {
                Some(actor) => actor,
                None => resolve_actor_reference(server.resolver(), &accept.actor).await?,
            };
            let follow_activity = follow_reference_id(&accept.object);
            let pending = match pending_follow {
                Some(pending) if pending.follow_activity == *follow_activity => pending,
                Some(_) => return Ok(StatusCode::ACCEPTED.into_response()),
                None => match server
                    .storage()
                    .load_pending_follow(follow_activity)
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                {
                    Some(pending) => pending,
                    None => return Ok(StatusCode::ACCEPTED.into_response()),
                },
            };
            match receive_accept_follow(&local_actor, &remote_actor, &pending, accept) {
                Ok(_) => {}
                Err(AcceptFollowError::WrongActor) => return Err(StatusCode::UNAUTHORIZED),
                Err(
                    AcceptFollowError::WrongFollow
                    | AcceptFollowError::WrongFollowActor
                    | AcceptFollowError::WrongFollowObject
                    | AcceptFollowError::WrongLocalActor,
                ) => return Ok(StatusCode::ACCEPTED.into_response()),
            }

            server
                .storage()
                .confirm_pending_follow(&pending)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            return Ok(StatusCode::ACCEPTED.into_response());
        }
        Some("Undo") => {
            let undo: Undo = from_value(value).map_err(|_| StatusCode::BAD_REQUEST)?;
            let remote_actor = match verified_actor {
                Some(actor) => actor,
                None => resolve_actor_reference(server.resolver(), &undo.actor).await?,
            };
            let outcome = match receive_undo_follow(&local_actor, &remote_actor, undo) {
                Ok(outcome) => outcome,
                Err(UndoFollowError::LinkedFollow | UndoFollowError::WrongObject) => {
                    return Ok(StatusCode::ACCEPTED.into_response());
                }
                Err(UndoFollowError::WrongActor) => return Err(StatusCode::UNAUTHORIZED),
            };

            server
                .storage()
                .remove_follower(&outcome.follower, &outcome.following)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            return Ok(StatusCode::ACCEPTED.into_response());
        }
        _ => return Ok(StatusCode::ACCEPTED.into_response()),
    }

    let follow: Follow = from_value(value).map_err(|_| StatusCode::BAD_REQUEST)?;
    let remote_actor = match verified_actor {
        Some(actor) => actor,
        None => resolve_actor_reference(server.resolver(), &follow.actor).await?,
    };
    let accept_id = accept_id_for_follow(&local_actor.id, &follow.id)?;
    let outcome = match receive_follow(&local_actor, &remote_actor, follow, accept_id) {
        Ok(outcome) => outcome,
        Err(FollowError::WrongObject) => return Ok(StatusCode::ACCEPTED.into_response()),
        Err(FollowError::WrongActor) => return Err(StatusCode::UNAUTHORIZED),
    };

    server
        .storage()
        .store_follower(&outcome.follower, &outcome.following)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let key_pair = server
        .storage()
        .load_actor_key_pair(&local_actor.id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    server
        .sender()
        .send_activity(
            &local_actor,
            &key_pair,
            &outcome.accept,
            &outcome.recipient_inbox,
        )
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;

    Ok(StatusCode::ACCEPTED.into_response())
}

async fn resolve_actor_reference(
    resolver: &ActorResolver,
    actor: &Reference<Actor>,
) -> Result<Actor, StatusCode> {
    match actor {
        Reference::Object(actor) => Ok((**actor).clone()),
        Reference::Id(actor_id) => resolver
            .resolve(actor_id)
            .await
            .map_err(|_| StatusCode::BAD_GATEWAY),
    }
}

async fn verify_signed_request(
    resolver: &ActorResolver,
    request: &InboxRequest,
    activity_actor_id: &Iri,
    expected_inbox: &Iri,
) -> Result<Actor, StatusCode> {
    let signature_header = request
        .headers
        .get("signature")
        .and_then(|value| value.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let signature = parse_signature_header(signature_header).ok_or(StatusCode::UNAUTHORIZED)?;
    if signature.algorithm != "rsa-sha256"
        || signature.signed_headers.first().map(String::as_str) != Some("(request-target)")
    {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let mut seen_headers = HashSet::new();
    let mut signed_headers = Vec::new();
    for name in signature.signed_headers.iter().skip(1) {
        if name.starts_with('(') || !seen_headers.insert(name.as_str()) {
            return Err(StatusCode::UNAUTHORIZED);
        }
        let values = request.headers.get_all(name).iter().collect::<Vec<_>>();
        let [value] = values.as_slice() else {
            return Err(StatusCode::UNAUTHORIZED);
        };
        let value = value.to_str().map_err(|_| StatusCode::UNAUTHORIZED)?;
        signed_headers.push((name.as_str(), value));
    }
    if !["host", "date", "digest"]
        .iter()
        .all(|required| seen_headers.contains(required))
    {
        return Err(StatusCode::UNAUTHORIZED);
    }

    verify_request_host(&request.headers, expected_inbox)?;
    verify_request_date(&request.headers)?;
    verify_request_digest(&request.headers, &request.body)?;

    let key_id: Iri = signature
        .key_id
        .parse()
        .map_err(|_| StatusCode::UNAUTHORIZED)?;
    let public_key = resolver
        .resolve_key(&key_id)
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    if public_key.id != key_id || public_key.owner != *activity_actor_id {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let request_target = request
        .uri
        .path_and_query()
        .map_or(request.uri.path(), |value| value.as_str());
    verify_draft_cavage(
        &public_key.public_key_pem,
        request.method.as_str(),
        request_target,
        &signed_headers,
        &signature.signature,
    )
    .map_err(|_| StatusCode::UNAUTHORIZED)?;

    let actor = resolver
        .resolve(activity_actor_id)
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    if actor.id != *activity_actor_id || !actor_owns_key(&actor, &public_key) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(actor)
}

fn actor_owns_key(actor: &Actor, key: &CryptographicKey) -> bool {
    match actor.public_key.as_ref() {
        Some(Reference::Id(advertised_key_id)) => advertised_key_id == &key.id,
        Some(Reference::Object(advertised_key)) => {
            advertised_key.id == key.id
                && advertised_key.owner == actor.id
                && advertised_key.public_key_pem == key.public_key_pem
        }
        None => false,
    }
}

fn accept_id_for_follow(local_actor_id: &Iri, follow_id: &Iri) -> Result<Iri, StatusCode> {
    let encoded_follow_id = percent_encoding::utf8_percent_encode(
        follow_id.as_str(),
        percent_encoding::NON_ALPHANUMERIC,
    );
    format!("{local_actor_id}#accepts/{encoded_follow_id}")
        .parse()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

fn activity_actor_id(value: &Value) -> Option<Iri> {
    let actor = value.get("actor")?;
    actor
        .as_str()
        .or_else(|| actor.get("id").and_then(Value::as_str))?
        .parse()
        .ok()
}

fn activity_target_id(value: &Value) -> Option<Iri> {
    let target = match value.get("type").and_then(Value::as_str) {
        Some("Follow") => value.get("object")?,
        Some("Undo") => value.get("object")?.get("object")?,
        _ => return None,
    };

    target
        .as_str()
        .or_else(|| target.get("id").and_then(Value::as_str))?
        .parse()
        .ok()
}

fn shared_inbox_target<S>(
    storage: &S,
    value: &Value,
) -> Result<Option<(Iri, Option<PendingFollow>)>, StatusCode>
where
    S: ServerStorage,
{
    if value.get("type").and_then(Value::as_str) == Some("Accept") {
        let Some(follow_activity) = value.get("object").and_then(value_reference_id) else {
            return Ok(None);
        };
        let pending = storage
            .load_pending_follow(&follow_activity)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        return Ok(pending.map(|pending| {
            let local_actor = pending.local_actor.clone();
            (local_actor, Some(pending))
        }));
    }

    Ok(activity_target_id(value).map(|target| (target, None)))
}

fn value_reference_id(value: &Value) -> Option<Iri> {
    value
        .as_str()
        .or_else(|| value.get("id").and_then(Value::as_str))?
        .parse()
        .ok()
}

fn follow_reference_id(reference: &Reference<Follow>) -> &Iri {
    match reference {
        Reference::Id(id) => id,
        Reference::Object(follow) => &follow.id,
    }
}

fn verify_request_host(headers: &HeaderMap, inbox: &Iri) -> Result<(), StatusCode> {
    let signed_host = headers
        .get(HOST)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<Authority>().ok())
        .filter(|authority| !authority.as_str().contains('@'))
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let inbox_uri = inbox
        .as_str()
        .parse::<Uri>()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let expected_host = inbox_uri
        .authority()
        .filter(|authority| !authority.as_str().contains('@'))
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let default_port = match inbox_uri.scheme_str() {
        Some(scheme) if scheme.eq_ignore_ascii_case("http") => Some(80),
        Some(scheme) if scheme.eq_ignore_ascii_case("https") => Some(443),
        _ => None,
    };
    let signed_port = effective_port(&signed_host, default_port).ok_or(StatusCode::UNAUTHORIZED)?;
    let expected_port =
        effective_port(expected_host, default_port).ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    if signed_host
        .host()
        .eq_ignore_ascii_case(expected_host.host())
        && signed_port == expected_port
    {
        Ok(())
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

fn effective_port(authority: &Authority, default_port: Option<u16>) -> Option<Option<u16>> {
    let suffix = authority.as_str().get(authority.host().len()..)?;
    if suffix.is_empty() {
        Some(default_port)
    } else if suffix.starts_with(':') {
        authority.port_u16().map(Some)
    } else {
        None
    }
}

fn verify_request_date(headers: &HeaderMap) -> Result<(), StatusCode> {
    let date = headers
        .get("date")
        .and_then(|value| value.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)
        .and_then(|value| httpdate::parse_http_date(value).map_err(|_| StatusCode::UNAUTHORIZED))?;
    let now = SystemTime::now();
    if now
        .duration_since(date)
        .is_ok_and(|age| age > MAX_SIGNATURE_AGE)
        || date
            .duration_since(now)
            .is_ok_and(|skew| skew > MAX_CLOCK_SKEW)
    {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(())
}

fn verify_request_digest(headers: &HeaderMap, body: &[u8]) -> Result<(), StatusCode> {
    let digest = headers
        .get("digest")
        .and_then(|value| value.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let expected = create_sha256_digest_header(body);
    let matches = digest.split(',').any(|entry| {
        entry
            .trim()
            .split_once('=')
            .is_some_and(|(algorithm, value)| {
                algorithm.eq_ignore_ascii_case("sha-256")
                    && expected
                        .split_once('=')
                        .is_some_and(|(_, expected)| value == expected)
            })
    });
    if matches {
        Ok(())
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

struct ParsedSignature {
    key_id: String,
    algorithm: String,
    signed_headers: Vec<String>,
    signature: String,
}

fn parse_signature_header(header: &str) -> Option<ParsedSignature> {
    let mut parameters = BTreeMap::new();
    let mut remaining = header;
    while !remaining.trim_start().is_empty() {
        remaining = remaining.trim_start();
        let equals = remaining.find('=')?;
        let name = remaining[..equals].trim();
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return None;
        }
        remaining = &remaining[equals + 1..];
        let (value, rest) = parse_quoted_parameter(remaining.trim_start())?;
        if parameters
            .insert(name.to_ascii_lowercase(), value)
            .is_some()
        {
            return None;
        }
        remaining = rest.trim_start();
        if remaining.is_empty() {
            break;
        }
        remaining = remaining.strip_prefix(',')?;
    }

    let key_id = parameters.remove("keyid")?;
    let algorithm = parameters.remove("algorithm")?;
    let signed_headers = parameters
        .remove("headers")?
        .split_ascii_whitespace()
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>();
    let signature = parameters.remove("signature")?;
    if key_id.is_empty() || signed_headers.is_empty() || signature.is_empty() {
        return None;
    }
    Some(ParsedSignature {
        key_id,
        algorithm: algorithm.to_ascii_lowercase(),
        signed_headers,
        signature,
    })
}

fn parse_quoted_parameter(input: &str) -> Option<(String, &str)> {
    let input = input.strip_prefix('"')?;
    let mut value = String::new();
    let mut escaped = false;
    for (index, character) in input.char_indices() {
        if escaped {
            value.push(character);
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == '"' {
            return Some((value, &input[index + character.len_utf8()..]));
        } else if character.is_control() {
            return None;
        } else {
            value.push(character);
        }
    }
    None
}
