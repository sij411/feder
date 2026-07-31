Reference Actor Server Example
==============================

Minimal actor endpoint using `ref-feder-core` capabilities through
`ref-feder-runtime-server`.


Run
---

~~~~ sh
RUST_LOG=info cargo run -p ref-actor-server
~~~~

Request the hardcoded local actor:

~~~~ sh
curl -i \
  -H 'Accept: application/activity+json' \
  http://127.0.0.1:3000/users/alice
~~~~

The endpoint returns `200 OK` with an ActivityPub actor document. Requests for
another identifier return `404 Not Found`, and requests that do not prefer an
ActivityPub representation return `406 Not Acceptable`.
