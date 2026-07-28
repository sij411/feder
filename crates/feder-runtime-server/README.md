Feder Runtime Server
====================

Reusable Axum/Tokio server integration for Feder.

This crate builds an Axum router from caller-provided runtime configuration.
It provides a health check endpoint, WebFinger discovery, and a local actor
route with its followers collection. The caller chooses concrete bind
addresses, actor IRIs, usernames, and handle hosts.

ActivityPub inbox handling for Follow and embedded Undo(Follow) activities is
included. The runtime can use in-memory storage for tests and examples, or
file-backed SQLite storage for persisted follower state. Outgoing
`SendActivity` actions are sent synchronously to recipient inboxes as
ActivityPub JSON signed with the actor's draft-Cavage RSA key. Incoming inbox
requests can require verification with the same signature scheme.


Platform support
----------------

This runtime currently targets Linux for development and deployment. Other
platforms may compile, but they are not currently supported.

On Unix targets, file-backed SQLite databases are created with owner-only
permissions, and existing database files are restricted to owner-only
permissions when opened. This protects the actor signing keys stored in the
database. Equivalent Windows ACL hardening is not currently implemented.


Example
-------

~~~~ rust
use feder_runtime_server::{InboxAuthPolicy, RuntimeConfig, StorageConfig, build_router};

let config = RuntimeConfig {
    bind: "127.0.0.1:3000".parse().expect("valid bind address"),
    actor_id: "http://127.0.0.1:3000/users/alice"
        .parse()
        .expect("valid actor IRI"),
    inbox: "http://127.0.0.1:3000/users/alice/inbox"
        .parse()
        .expect("valid inbox IRI"),
    outbox: "http://127.0.0.1:3000/users/alice/outbox"
        .parse()
        .expect("valid outbox IRI"),
    username: "alice".to_string(),
    handle_host: "127.0.0.1:3000".to_string(),
    inbox_auth_policy: InboxAuthPolicy::AllowUnsignedInsecureDev,
    storage: StorageConfig::InMemory,
};

let app = build_router(config).expect("build router");
~~~~


Demo
----

A runnable single-user demo lives in `examples/single-user-server`:

~~~~ sh
RUST_LOG=info cargo run -p single-user-server
~~~~
