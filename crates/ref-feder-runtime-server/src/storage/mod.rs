// Feder: A portable ActivityPub core for many runtimes.
// Copyright (C) 2026 Feder contributors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, version 3.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

mod sqlite;

pub use sqlite::SqliteStore;

use ref_feder_core::key::KeyError;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("I/O error")]
    Io(#[from] std::io::Error),

    #[error("SQLite error")]
    Sqlite(#[from] rusqlite::Error),

    #[error("JSON error")]
    Json(#[from] serde_json::Error),

    #[error("invalid IRI: {0}")]
    InvalidIri(String),

    #[error("unsupported stored object type: {0}")]
    UnsupportedStoredObjectType(String),

    #[error("storage lock poisoned")]
    LockPoisoned,

    #[error(transparent)]
    ActorKey(#[from] KeyError),
}
