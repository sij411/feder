Reference ActivityPub Server
============================

Minimal actor, WebFinger, and personal inbox endpoints using `ref-feder-core`
capabilities through `ref-feder-runtime-server`.


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

Discover the actor through WebFinger:

~~~~ sh
curl -i \
  -H 'Host: 127.0.0.1:3000' \
  'http://127.0.0.1:3000/.well-known/webfinger?resource=acct:alice@127.0.0.1:3000'
~~~~

The endpoint returns `application/jrd+json` with a `self` link to
`http://127.0.0.1:3000/users/alice`. The domain in the `acct:` resource must
match the request's `Host` header.

Send an unsigned development Follow to Alice's personal inbox:

~~~~ sh
curl -i \
  -H 'Content-Type: application/activity+json' \
  --data-binary '{
    "@context": "https://www.w3.org/ns/activitystreams",
    "id": "https://remote.example/activities/follow/1",
    "type": "Follow",
    "actor": {
      "id": "https://remote.example/users/bob",
      "type": "Person",
      "inbox": "https://remote.example/users/bob/inbox",
      "outbox": "https://remote.example/users/bob/outbox"
    },
    "object": "http://127.0.0.1:3000/users/alice"
  }' \
  http://127.0.0.1:3000/users/alice/inbox
~~~~

The endpoint stores the latest follower, records the generated `Accept`, and
returns `202 Accepted`. These example adapters deliberately retain only their
latest value, so repeated requests do not grow an in-memory protocol history.
Unsigned inbox requests are enabled only for this local development example;
`FederServer` requires signed requests by default.
