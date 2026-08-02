Single-User Server Example
==========================

Demo app using `feder-server` with one hardcoded local actor and
the built-in SQLite storage adapter.


Run
---

The example currently targets Linux:

~~~~ sh
RUST_LOG=info cargo run -p single-user-server
~~~~

The server stores its actor signing identity, followers, outbound Follow
state, and Notes in `feder.sqlite3`. Set `FEDER_DATABASE` to use another path.

The demo actor is:

~~~~ text
http://127.0.0.1:3000/users/alice
~~~~

The server listens on:

~~~~ text
127.0.0.1:3000
~~~~

Fetch the actor document:

~~~~ sh
curl -i \
  -H 'Accept: application/activity+json' \
  http://127.0.0.1:3000/users/alice
~~~~

Discover the actor through WebFinger:

~~~~ sh
curl -i \
  'http://127.0.0.1:3000/.well-known/webfinger?resource=acct:alice@127.0.0.1:3000'
~~~~
