Reference ActivityPub Server
============================

Minimal actor, WebFinger, personal inbox, and shared inbox endpoints using
`ref-feder-core` capabilities through `ref-feder-runtime-server`.


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

Send an unsigned development Follow to the shared inbox. The runtime selects
Alice from the Follow's `object` IRI:

~~~~ sh
curl -i \
  -H 'Content-Type: application/activity+json' \
  --data-binary '{
    "@context": "https://www.w3.org/ns/activitystreams",
    "id": "http://127.0.0.1:3000/remote/activities/follow/1",
    "type": "Follow",
    "actor": {
      "id": "http://127.0.0.1:3000/remote/users/bob",
      "type": "Person",
      "inbox": "http://127.0.0.1:3000/remote-inbox",
      "outbox": "http://127.0.0.1:3000/remote/users/bob/outbox"
    },
    "object": "http://127.0.0.1:3000/users/alice"
  }' \
  http://127.0.0.1:3000/inbox
~~~~

The endpoint stores the latest follower, loads Alice's key pair, and sends a
signed `Accept` to the example's `/remote-inbox` recipient before returning
`202 Accepted`. The recipient checks that the request has a `Signature` header
and logs receipt of the activity.

Undo that Follow:

~~~~ sh
curl -i \
  -H 'Content-Type: application/activity+json' \
  --data-binary '{
    "@context": "https://www.w3.org/ns/activitystreams",
    "id": "http://127.0.0.1:3000/remote/activities/undo/1",
    "type": "Undo",
    "actor": {
      "id": "http://127.0.0.1:3000/remote/users/bob",
      "type": "Person",
      "inbox": "http://127.0.0.1:3000/remote-inbox",
      "outbox": "http://127.0.0.1:3000/remote/users/bob/outbox"
    },
    "object": {
      "id": "http://127.0.0.1:3000/remote/activities/follow/1",
      "type": "Follow",
      "actor": "http://127.0.0.1:3000/remote/users/bob",
      "object": "http://127.0.0.1:3000/users/alice"
    }
  }' \
  http://127.0.0.1:3000/inbox
~~~~

The endpoint validates that Bob owns the embedded Follow, removes the matching
follower relationship, and returns `202 Accepted`. Repeating the Undo is safe.

Request Alice's followers collection:

~~~~ sh
curl -i \
  -H 'Accept: application/activity+json' \
  http://127.0.0.1:3000/users/alice/followers
~~~~

The endpoint returns an ActivityStreams `OrderedCollection`. Its
`orderedItems` contains Bob after the Follow request and is empty after the
Undo request.

Send an outbound Follow from Alice to the example's remote Bob actor:

~~~~ sh
curl -i -X POST http://127.0.0.1:3000/send-follow
~~~~

The application supplies the local actor, remote actor, and activity IRIs to
`FederServer::follow_actor()`. The runtime resolves Bob, asks core to construct
the activity and pending relationship, persists that relationship, loads
Alice's key, and sends a signed Follow to Bob's inbox. The pending relationship
is bounded to one entry in this example.

Confirm that pending Follow with a linked `Accept` through the shared inbox:

~~~~ sh
curl -i \
  -H 'Content-Type: application/activity+json' \
  --data-binary '{
    "@context": "https://www.w3.org/ns/activitystreams",
    "id": "http://127.0.0.1:3000/remote/activities/accept/example",
    "type": "Accept",
    "actor": {
      "id": "http://127.0.0.1:3000/remote/users/bob",
      "type": "Person",
      "inbox": "http://127.0.0.1:3000/remote-inbox",
      "outbox": "http://127.0.0.1:3000/remote/users/bob/outbox"
    },
    "object": "http://127.0.0.1:3000/users/alice/activities/follow/example"
  }' \
  http://127.0.0.1:3000/inbox
~~~~

The shared inbox uses the linked Follow IRI to find Alice, core validates Bob
against the pending relationship, and storage atomically changes that exact
relationship from pending to accepted. Repeating the Accept is a safe no-op.

Create and persist a local Note:

~~~~ sh
curl -i -X POST http://127.0.0.1:3000/create-note
~~~~

The application supplies stable Note and Create activity IRIs plus the Note's
content and addressing. Core constructs both values, and the runtime persists
only the durable Note through `NoteStore`. It then expands the followers
address through `FollowerDeliveryStore`, prefers shared inboxes, deduplicates
destinations, and sends the transient Create activity with Alice's key. Send
the earlier inbound Follow first to observe delivery to Bob's example inbox.

The example loads its actor key pair from the repository's test fixture and
retains only that pair, the latest follower, the latest outbound Follow, and
the latest Note, so repeated requests do not grow an in-memory protocol
history. The fixture key is public test data and must never be used for a real
actor. Unsigned incoming requests and private outbound addresses are enabled
only for this local development example; `FederServer` requires signed
requests and public destinations by default.
