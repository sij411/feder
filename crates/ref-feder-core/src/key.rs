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

use alloc::string::String;
use core::fmt;

use rsa::{
    RsaPrivateKey, RsaPublicKey,
    pkcs8::{DecodePrivateKey, DecodePublicKey, EncodePrivateKey, EncodePublicKey, LineEnding},
    rand_core::CryptoRngCore,
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
