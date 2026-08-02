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

use std::time::SystemTime;

use feder_core::key::{
    ActorKeyPair, HttpSignatureError, create_sha256_digest_header, sign_draft_cavage,
};
use feder_vocab::{Actor, Iri, Reference};
use reqwest::{
    Client, StatusCode, Url,
    header::{CONTENT_TYPE, DATE, HOST},
};
use serde::Serialize;

use crate::{config::OutboundAddressPolicy, url};

#[derive(Clone, Debug)]
pub struct ActivitySender {
    client: Client,
    address_policy: OutboundAddressPolicy,
}

impl ActivitySender {
    /// Creates a signed ActivityPub HTTP sender.
    pub fn new(address_policy: OutboundAddressPolicy) -> Result<Self, SendError> {
        let client = url::build_client(address_policy).map_err(SendError::BuildClient)?;

        Ok(Self {
            client,
            address_policy,
        })
    }

    pub async fn send_activity<T>(
        &self,
        local_actor: &Actor,
        key_pair: &ActorKeyPair,
        activity: &T,
        inbox: &Iri,
    ) -> Result<(), SendError>
    where
        T: Serialize + ?Sized,
    {
        let key_id = match local_actor.public_key.as_ref() {
            Some(Reference::Id(key_id)) => key_id,
            Some(Reference::Object(key)) => {
                if key.owner != local_actor.id || key.public_key_pem != key_pair.public_key_pem() {
                    return Err(SendError::ActorKeyMismatch(local_actor.id.to_string()));
                }
                &key.id
            }
            None => return Err(SendError::MissingActorKey(local_actor.id.to_string())),
        };
        let body = serde_json::to_vec(activity).map_err(SendError::Serialize)?;
        let url =
            Url::parse(inbox.as_str()).map_err(|_| SendError::InvalidInbox(inbox.to_string()))?;
        if !matches!(url.scheme(), "http" | "https")
            || !url.username().is_empty()
            || url.password().is_some()
            || url.host().is_none()
        {
            return Err(SendError::InvalidInbox(inbox.to_string()));
        }
        crate::url::validate_literal_host(&url, self.address_policy).map_err(|address| {
            SendError::PrivateInboxAddress {
                inbox: inbox.to_string(),
                address,
            }
        })?;
        let mut host = url
            .host()
            .ok_or_else(|| SendError::InvalidInbox(inbox.to_string()))?
            .to_string();
        if let Some(port) = url.port() {
            host = format!("{host}:{port}");
        }
        let date = httpdate::fmt_http_date(SystemTime::now());
        let digest = create_sha256_digest_header(&body);
        let headers = [
            ("content-type", "application/activity+json"),
            ("date", date.as_str()),
            ("digest", digest.as_str()),
            ("host", host.as_str()),
        ];
        let mut request_target = url.path().to_string();
        if let Some(query) = url.query() {
            request_target.push('?');
            request_target.push_str(query);
        }
        let signature =
            sign_draft_cavage(key_pair, key_id.as_str(), "POST", &request_target, &headers)
                .map_err(SendError::Sign)?;

        let response = self
            .client
            .post(url)
            .header(CONTENT_TYPE, "application/activity+json")
            .header(DATE, date)
            .header("Digest", digest)
            .header(HOST, host)
            .header("Signature", signature)
            .body(body)
            .send()
            .await
            .map_err(SendError::Request)?;

        if !response.status().is_success() {
            return Err(SendError::UnsuccessfulStatus {
                inbox: inbox.to_string(),
                status: response.status(),
            });
        }

        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SendError {
    #[error("failed to build HTTP client")]
    BuildClient(#[source] reqwest::Error),

    #[error("failed to serialize activity")]
    Serialize(#[source] serde_json::Error),

    #[error("local actor {0} does not advertise a signing key")]
    MissingActorKey(String),

    #[error("stored signing key does not match local actor {0}")]
    ActorKeyMismatch(String),

    #[error("invalid recipient inbox: {0}")]
    InvalidInbox(String),

    #[error("recipient inbox {inbox} resolves to non-public address {address}")]
    PrivateInboxAddress {
        inbox: String,
        address: std::net::IpAddr,
    },

    #[error("failed to sign activity request")]
    Sign(#[source] HttpSignatureError),

    #[error("failed to send activity")]
    Request(#[source] reqwest::Error),

    #[error("sending activity to {inbox} returned {status}")]
    UnsuccessfulStatus { inbox: String, status: StatusCode },
}
