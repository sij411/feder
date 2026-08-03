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

use alloc::{format, string::String, vec::Vec};
use core::fmt;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use rsa::{
    RsaPrivateKey, RsaPublicKey,
    pkcs1v15::{Signature, SigningKey, VerifyingKey},
    pkcs8::{DecodePrivateKey, DecodePublicKey, EncodePrivateKey, EncodePublicKey, LineEnding},
    rand_core::CryptoRngCore,
    sha2::{Digest, Sha256},
    signature::{SignatureEncoding, Signer, Verifier},
};
use zeroize::Zeroizing;

const ACTOR_RSA_BITS: usize = 4096;

#[derive(Clone, Eq, PartialEq)]
pub struct ActorKeyPair {
    private_key_pem: Zeroizing<String>,
    public_key_pem: String,
}

impl ActorKeyPair {
    pub fn from_pem(private_key_pem: String, public_key_pem: String) -> Result<Self, KeyError> {
        let private_key_pem = Zeroizing::new(private_key_pem);
        let private_key =
            RsaPrivateKey::from_pkcs8_pem(&private_key_pem).map_err(KeyError::InvalidPrivateKey)?;
        let public_key = RsaPublicKey::from_public_key_pem(&public_key_pem)
            .map_err(KeyError::InvalidPublicKey)?;

        if RsaPublicKey::from(&private_key) != public_key {
            return Err(KeyError::MismatchedKeyPair);
        }

        Ok(Self {
            private_key_pem,
            public_key_pem,
        })
    }

    #[must_use]
    pub fn private_key_pem(&self) -> &str {
        &self.private_key_pem
    }

    #[must_use]
    pub fn public_key_pem(&self) -> &str {
        &self.public_key_pem
    }
}

impl fmt::Debug for ActorKeyPair {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ActorKeyPair")
            .field("private_key_pem", &"[REDACTED]")
            .field("public_key_pem", &self.public_key_pem)
            .finish()
    }
}

/// Generates a 4096-bit RSA actor key pair for draft-Cavage HTTP signatures.
/// The caller must supply a cryptographically secure random number generator for the target runtime.
pub fn generate_actor_key_pair(
    rng: &mut (impl CryptoRngCore + ?Sized),
) -> Result<ActorKeyPair, KeyError> {
    let private_key = RsaPrivateKey::new(rng, ACTOR_RSA_BITS).map_err(KeyError::Generation)?;
    let public_key = RsaPublicKey::from(&private_key);
    let private_key_pem = private_key
        .to_pkcs8_pem(LineEnding::LF)
        .map_err(KeyError::PrivateKeyEncoding)?;
    let public_key_pem = public_key
        .to_public_key_pem(LineEnding::LF)
        .map_err(KeyError::PublicKeyEncoding)?;

    Ok(ActorKeyPair {
        private_key_pem,
        public_key_pem,
    })
}

#[derive(Debug)]
pub enum KeyError {
    Generation(rsa::Error),
    PrivateKeyEncoding(rsa::pkcs8::Error),
    PublicKeyEncoding(rsa::pkcs8::spki::Error),
    InvalidPrivateKey(rsa::pkcs8::Error),
    InvalidPublicKey(rsa::pkcs8::spki::Error),
    MismatchedKeyPair,
}

impl fmt::Display for KeyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Generation(_) => formatter.write_str("failed to generate RSA actor key"),
            Self::PrivateKeyEncoding(_) => formatter.write_str("failed to encode RSA private key"),
            Self::PublicKeyEncoding(_) => formatter.write_str("failed to encode RSA public key"),
            Self::InvalidPrivateKey(_) => formatter.write_str("invalid RSA private key PEM"),
            Self::InvalidPublicKey(_) => formatter.write_str("invalid RSA public key PEM"),
            Self::MismatchedKeyPair => formatter.write_str("RSA actor keys do not match"),
        }
    }
}

impl core::error::Error for KeyError {}

/// Creates an RFC 3230 SHA-256 digest header.
#[must_use]
pub fn create_sha256_digest_header(body: &[u8]) -> String {
    let digest = Sha256::digest(body);
    format!("SHA-256={}", STANDARD.encode(digest))
}

/// Signs a prepared HTTP request using the draft-Cavage header format.
///
/// Header names must be supplied in the order in which they should appear in
/// the signature's `headers` parameter.
pub fn sign_draft_cavage(
    key_pair: &ActorKeyPair,
    key_id: &str,
    method: &str,
    request_target: &str,
    headers: &[(&str, &str)],
) -> Result<String, HttpSignatureError> {
    let signature_base = draft_cavage_signature_base(method, request_target, headers);
    let private_key = RsaPrivateKey::from_pkcs8_pem(key_pair.private_key_pem())
        .map_err(HttpSignatureError::InvalidPrivateKey)?;
    let signing_key = SigningKey::<Sha256>::new(private_key);
    let signature = signing_key.sign(signature_base.as_bytes()).to_bytes();
    let signed_headers = headers
        .iter()
        .map(|(name, _)| name.to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join(" ");

    Ok(format!(
        "keyId=\"{key_id}\",algorithm=\"rsa-sha256\",headers=\"(request-target) {signed_headers}\",signature=\"{}\"",
        STANDARD.encode(signature)
    ))
}

/// Verifies a draft-Cavage RSA-SHA256 signature over a prepared request.
///
/// `headers` must contain the signed HTTP headers in their declared order,
/// excluding the `(request-target)` pseudo-header.
pub fn verify_draft_cavage(
    public_key_pem: &str,
    method: &str,
    request_target: &str,
    headers: &[(&str, &str)],
    signature: &str,
) -> Result<(), HttpSignatureVerificationError> {
    let signature_base = draft_cavage_signature_base(method, request_target, headers);
    let public_key = RsaPublicKey::from_public_key_pem(public_key_pem)
        .map_err(HttpSignatureVerificationError::InvalidPublicKey)?;
    let signature = STANDARD
        .decode(signature)
        .map_err(HttpSignatureVerificationError::InvalidSignatureEncoding)?;
    let signature = Signature::try_from(signature.as_slice())
        .map_err(HttpSignatureVerificationError::InvalidSignature)?;
    let verifying_key = VerifyingKey::<Sha256>::new(public_key);

    verifying_key
        .verify(signature_base.as_bytes(), &signature)
        .map_err(HttpSignatureVerificationError::Verification)
}

fn draft_cavage_signature_base(
    method: &str,
    request_target: &str,
    headers: &[(&str, &str)],
) -> String {
    let mut lines = Vec::with_capacity(headers.len() + 1);
    lines.push(format!(
        "(request-target): {} {request_target}",
        method.to_ascii_lowercase()
    ));
    lines.extend(
        headers
            .iter()
            .map(|(name, value)| format!("{}: {}", name.to_ascii_lowercase(), value.trim())),
    );
    lines.join("\n")
}

/// Errors produced while creating an HTTP signature.
#[derive(Debug)]
pub enum HttpSignatureError {
    InvalidPrivateKey(rsa::pkcs8::Error),
}

impl fmt::Display for HttpSignatureError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPrivateKey(_) => formatter.write_str("invalid RSA private key PEM"),
        }
    }
}

impl core::error::Error for HttpSignatureError {}

/// Errors produced while verifying an HTTP signature.
#[derive(Debug)]
pub enum HttpSignatureVerificationError {
    InvalidPublicKey(rsa::pkcs8::spki::Error),
    InvalidSignatureEncoding(base64::DecodeError),
    InvalidSignature(rsa::signature::Error),
    Verification(rsa::signature::Error),
}

impl fmt::Display for HttpSignatureVerificationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPublicKey(_) => formatter.write_str("invalid RSA public key PEM"),
            Self::InvalidSignatureEncoding(_) => {
                formatter.write_str("invalid base64 signature encoding")
            }
            Self::InvalidSignature(_) => formatter.write_str("invalid RSA signature"),
            Self::Verification(_) => formatter.write_str("HTTP signature verification failed"),
        }
    }
}

impl core::error::Error for HttpSignatureVerificationError {}
