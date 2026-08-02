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

use std::{
    path::Path,
    sync::{Mutex, MutexGuard},
};

#[cfg(unix)]
use std::{
    fs::OpenOptions,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
};

use feder_vocab::{Actor, Iri, Note};
use rand_core::CryptoRngCore;
use ref_feder_core::{
    follow::PendingFollow,
    key::{ActorKeyPair, generate_actor_key_pair},
    storage::{FollowerDeliveryStore, FollowerDeliveryTarget, NoteStore, ServerStorage, Storage},
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use super::StoreError;

pub struct SqliteStore {
    connection: Mutex<Connection>,
}

impl SqliteStore {
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        #[cfg(unix)]
        let database_file = prepare_database_file(path)?;

        let store = Self {
            connection: Mutex::new(Connection::open(path)?),
        };

        #[cfg(unix)]
        drop(database_file);

        store.init()?;
        Ok(store)
    }

    pub fn open_in_memory() -> Result<Self, StoreError> {
        let store = Self {
            connection: Mutex::new(Connection::open_in_memory()?),
        };
        store.init()?;
        Ok(store)
    }

    pub fn insert_actor_key_pair(
        &self,
        actor_id: &Iri,
        key_pair: &ActorKeyPair,
    ) -> Result<(), StoreError> {
        self.connection()?.execute(
            r#"
            INSERT INTO keys (actor_id, private_key_pem, public_key_pem)
            VALUES (?1, ?2, ?3)
            "#,
            params![
                actor_id.as_str(),
                key_pair.private_key_pem(),
                key_pair.public_key_pem(),
            ],
        )?;
        Ok(())
    }

    /// Loads the actor's existing signing identity or provisions it once.
    ///
    /// Concurrent provisioners keep the first key pair inserted for the actor;
    /// an existing identity is never replaced.
    pub fn load_or_generate_actor_key_pair(
        &self,
        actor_id: &Iri,
        rng: &mut (impl CryptoRngCore + ?Sized),
    ) -> Result<ActorKeyPair, StoreError> {
        self.load_or_insert_actor_key_pair(actor_id, || {
            generate_actor_key_pair(rng).map_err(StoreError::from)
        })
    }

    fn load_or_insert_actor_key_pair(
        &self,
        actor_id: &Iri,
        generate: impl FnOnce() -> Result<ActorKeyPair, StoreError>,
    ) -> Result<ActorKeyPair, StoreError> {
        {
            let connection = self.connection()?;
            if let Some(key_pair) = load_actor_key_pair(&connection, actor_id)? {
                return Ok(key_pair);
            }
        }

        let generated = generate()?;
        let connection = self.connection()?;
        let inserted = connection.execute(
            r#"
            INSERT INTO keys (actor_id, private_key_pem, public_key_pem)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(actor_id) DO NOTHING
            "#,
            params![
                actor_id.as_str(),
                generated.private_key_pem(),
                generated.public_key_pem(),
            ],
        )?;
        if inserted == 1 {
            Ok(generated)
        } else {
            load_actor_key_pair(&connection, actor_id)?.ok_or(StoreError::ActorKeyProvisioning)
        }
    }

    fn init(&self) -> Result<(), StoreError> {
        self.connection()?.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS followers (
                follower_actor_id TEXT NOT NULL,
                following_actor_id TEXT NOT NULL,
                inbox_url TEXT,
                shared_inbox_url TEXT,
                PRIMARY KEY (follower_actor_id, following_actor_id)
            );
            CREATE INDEX IF NOT EXISTS idx_followers_following_actor_id
                ON followers (following_actor_id);
            CREATE TABLE IF NOT EXISTS keys (
                actor_id TEXT PRIMARY KEY NOT NULL,
                private_key_pem TEXT NOT NULL,
                public_key_pem TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS objects (
                object_id TEXT PRIMARY KEY NOT NULL,
                object_type TEXT NOT NULL,
                object_json TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS outbound_follows (
                follow_activity_id TEXT PRIMARY KEY NOT NULL,
                local_actor_id TEXT NOT NULL,
                remote_actor_json TEXT NOT NULL,
                state TEXT NOT NULL CHECK (state IN ('pending', 'accepted'))
            );
            "#,
        )?;
        Ok(())
    }

    fn connection(&self) -> Result<MutexGuard<'_, Connection>, StoreError> {
        self.connection.lock().map_err(|_| StoreError::LockPoisoned)
    }
}

#[cfg(unix)]
fn prepare_database_file(path: &Path) -> Result<std::fs::File, std::io::Error> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(path)?;

    let mut permissions = file.metadata()?.permissions();
    permissions.set_mode(0o600);
    file.set_permissions(permissions)?;
    Ok(file)
}

impl Storage for SqliteStore {
    type Error = StoreError;
}

impl ServerStorage for SqliteStore {
    fn store_follower(&self, follower: &Actor, following: &Iri) -> Result<(), Self::Error> {
        let shared_inbox = follower
            .endpoints
            .as_ref()
            .and_then(|endpoints| endpoints.shared_inbox.as_ref());
        self.connection()?.execute(
            r#"
            INSERT INTO followers (
                follower_actor_id,
                following_actor_id,
                inbox_url,
                shared_inbox_url
            )
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(follower_actor_id, following_actor_id) DO UPDATE SET
                inbox_url = excluded.inbox_url,
                shared_inbox_url = excluded.shared_inbox_url
            "#,
            params![
                follower.id.as_str(),
                following.as_str(),
                follower.inbox.as_str(),
                shared_inbox.map(|inbox| inbox.as_str()),
            ],
        )?;
        Ok(())
    }

    fn load_actor_key_pair(&self, actor_id: &Iri) -> Result<Option<ActorKeyPair>, Self::Error> {
        let connection = self.connection()?;
        load_actor_key_pair(&connection, actor_id)
    }

    fn remove_follower(&self, follower: &Iri, following: &Iri) -> Result<(), Self::Error> {
        self.connection()?.execute(
            r#"
            DELETE FROM followers
            WHERE follower_actor_id = ?1 AND following_actor_id = ?2
            "#,
            params![follower.as_str(), following.as_str()],
        )?;
        Ok(())
    }

    fn list_followers(&self, following: &Iri) -> Result<Vec<Iri>, Self::Error> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            r#"
            SELECT follower_actor_id
            FROM followers
            WHERE following_actor_id = ?1
            ORDER BY follower_actor_id
            "#,
        )?;
        let rows = statement.query_map([following.as_str()], |row| row.get::<_, String>(0))?;
        rows.map(|row| parse_iri(row?)).collect()
    }

    fn store_pending_follow(&self, follow: &PendingFollow) -> Result<(), Self::Error> {
        let remote_actor_json = serde_json::to_string(&follow.remote_actor)?;
        self.connection()?.execute(
            r#"
            INSERT INTO outbound_follows (
                follow_activity_id,
                local_actor_id,
                remote_actor_json,
                state
            )
            VALUES (?1, ?2, ?3, 'pending')
            ON CONFLICT(follow_activity_id) DO UPDATE SET
                local_actor_id = excluded.local_actor_id,
                remote_actor_json = excluded.remote_actor_json,
                state = 'pending'
            "#,
            params![
                follow.follow_activity.as_str(),
                follow.local_actor.as_str(),
                remote_actor_json,
            ],
        )?;
        Ok(())
    }

    fn load_pending_follow(
        &self,
        follow_activity: &Iri,
    ) -> Result<Option<PendingFollow>, Self::Error> {
        let connection = self.connection()?;
        load_pending_follow(&connection, follow_activity)
    }

    fn confirm_pending_follow(&self, expected: &PendingFollow) -> Result<bool, Self::Error> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let stored = load_pending_follow(&transaction, &expected.follow_activity)?;
        if stored.as_ref() != Some(expected) {
            return Ok(false);
        }
        let changed = transaction.execute(
            r#"
            UPDATE outbound_follows
            SET state = 'accepted'
            WHERE follow_activity_id = ?1 AND state = 'pending'
            "#,
            [expected.follow_activity.as_str()],
        )?;
        transaction.commit()?;
        Ok(changed == 1)
    }
}

impl NoteStore for SqliteStore {
    fn store_note(&self, note: &Note) -> Result<(), Self::Error> {
        let note_json = serde_json::to_string(note)?;
        self.connection()?.execute(
            r#"
            INSERT INTO objects (object_id, object_type, object_json)
            VALUES (?1, 'Note', ?2)
            ON CONFLICT(object_id) DO UPDATE SET
                object_type = excluded.object_type,
                object_json = excluded.object_json
            "#,
            params![note.id.as_str(), note_json],
        )?;
        Ok(())
    }

    fn load_note(&self, note_id: &Iri) -> Result<Option<Note>, Self::Error> {
        let stored = self
            .connection()?
            .query_row(
                r#"
                SELECT object_type, object_json
                FROM objects
                WHERE object_id = ?1
                "#,
                [note_id.as_str()],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;

        stored
            .map(|(object_type, object_json)| {
                if object_type != "Note" {
                    return Err(StoreError::UnsupportedStoredObjectType(object_type));
                }
                serde_json::from_str(&object_json).map_err(StoreError::from)
            })
            .transpose()
    }
}

impl FollowerDeliveryStore for SqliteStore {
    fn list_follower_delivery_targets(
        &self,
        local_actor: &Iri,
    ) -> Result<Vec<FollowerDeliveryTarget>, Self::Error> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            r#"
            SELECT follower_actor_id, inbox_url, shared_inbox_url
            FROM followers
            WHERE following_actor_id = ?1 AND inbox_url IS NOT NULL
            ORDER BY follower_actor_id
            "#,
        )?;
        let rows = statement.query_map([local_actor.as_str()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })?;

        rows.map(|row| {
            let (actor_id, inbox, shared_inbox) = row?;
            Ok(FollowerDeliveryTarget {
                actor_id: parse_iri(actor_id)?,
                inbox: parse_iri(inbox)?,
                shared_inbox: shared_inbox.map(parse_iri).transpose()?,
            })
        })
        .collect()
    }
}

fn load_pending_follow(
    connection: &Connection,
    follow_activity: &Iri,
) -> Result<Option<PendingFollow>, StoreError> {
    let stored = connection
        .query_row(
            r#"
            SELECT local_actor_id, remote_actor_json
            FROM outbound_follows
            WHERE follow_activity_id = ?1 AND state = 'pending'
            "#,
            [follow_activity.as_str()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;
    stored
        .map(|(local_actor, remote_actor_json)| {
            Ok(PendingFollow {
                local_actor: parse_iri(local_actor)?,
                remote_actor: serde_json::from_str(&remote_actor_json)?,
                follow_activity: follow_activity.clone(),
            })
        })
        .transpose()
}

fn load_actor_key_pair(
    connection: &Connection,
    actor_id: &Iri,
) -> Result<Option<ActorKeyPair>, StoreError> {
    let encoded_keys = connection
        .query_row(
            r#"
            SELECT private_key_pem, public_key_pem
            FROM keys
            WHERE actor_id = ?1
            "#,
            [actor_id.as_str()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;

    encoded_keys
        .map(|(private_key_pem, public_key_pem)| {
            ActorKeyPair::from_pem(private_key_pem, public_key_pem)
        })
        .transpose()
        .map_err(StoreError::from)
}

fn parse_iri(value: String) -> Result<Iri, StoreError> {
    value
        .parse()
        .map_err(|_| StoreError::InvalidIri(value.to_owned()))
}
