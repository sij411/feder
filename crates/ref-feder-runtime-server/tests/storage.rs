use feder_vocab::{Actor, Endpoints, Iri, Note, Reference};
use rand_core::OsRng;
use ref_feder_core::{
    follow::PendingFollow,
    key::ActorKeyPair,
    storage::{FollowerDeliveryStore, FollowerDeliveryTarget, NoteStore, ServerStorage},
};
use ref_feder_runtime_server::storage::SqliteStore;

const PRIVATE_KEY_PEM: &str = include_str!("fixtures/rsa-private-key.pem");
const PUBLIC_KEY_PEM: &str = include_str!("fixtures/rsa-public-key.pem");

fn iri(value: &str) -> Iri {
    value.parse().expect("valid test IRI")
}

fn actor(id: &str) -> Actor {
    Actor::person(
        iri(id),
        iri(&format!("{id}/inbox")),
        iri(&format!("{id}/outbox")),
    )
}

fn actor_key_pair() -> ActorKeyPair {
    ActorKeyPair::from_pem(PRIVATE_KEY_PEM.to_string(), PUBLIC_KEY_PEM.to_string())
        .expect("valid actor key pair")
}

#[test]
fn stores_lists_and_removes_follower_delivery_facts() {
    let store = SqliteStore::open_in_memory().expect("open store");
    let following = iri("https://local.example/users/alice");
    let mut follower = actor("https://remote.example/users/bob");
    follower.endpoints = Some(Endpoints {
        shared_inbox: Some(iri("https://remote.example/inbox")),
    });

    store
        .store_follower(&follower, &following)
        .expect("store follower");

    assert_eq!(
        store.list_followers(&following).expect("list followers"),
        vec![follower.id.clone()]
    );
    assert_eq!(
        store
            .list_follower_delivery_targets(&following)
            .expect("list delivery targets"),
        vec![FollowerDeliveryTarget {
            actor_id: follower.id.clone(),
            inbox: follower.inbox.clone(),
            shared_inbox: follower
                .endpoints
                .and_then(|endpoints| endpoints.shared_inbox),
        }]
    );

    store
        .remove_follower(&follower.id, &following)
        .expect("remove follower");
    assert!(
        store
            .list_followers(&following)
            .expect("list followers")
            .is_empty()
    );
}

#[test]
fn stores_and_loads_note() {
    let store = SqliteStore::open_in_memory().expect("open store");
    let mut note = Note::new(iri("https://local.example/posts/1"));
    note.attributed_to = Some(Reference::id(iri("https://local.example/users/alice")));
    note.content = Some("hello".to_string());

    store.store_note(&note).expect("store Note");

    assert_eq!(store.load_note(&note.id).expect("load Note"), Some(note));
}

#[test]
fn confirms_only_the_expected_pending_follow() {
    let store = SqliteStore::open_in_memory().expect("open store");
    let pending = PendingFollow {
        local_actor: iri("https://local.example/users/alice"),
        remote_actor: actor("https://remote.example/users/bob"),
        follow_activity: iri("https://local.example/activities/follow/1"),
    };
    store
        .store_pending_follow(&pending)
        .expect("store pending Follow");

    let mut wrong = pending.clone();
    wrong.local_actor = iri("https://local.example/users/mallory");
    assert!(
        !store
            .confirm_pending_follow(&wrong)
            .expect("reject mismatch")
    );
    assert_eq!(
        store
            .load_pending_follow(&pending.follow_activity)
            .expect("load pending Follow"),
        Some(pending.clone())
    );

    assert!(
        store
            .confirm_pending_follow(&pending)
            .expect("confirm pending Follow")
    );
    assert_eq!(
        store
            .load_pending_follow(&pending.follow_activity)
            .expect("load accepted Follow"),
        None
    );
}

#[test]
fn actor_key_pair_roundtrips() {
    let store = SqliteStore::open_in_memory().expect("open store");
    let actor_id = iri("https://local.example/users/alice");
    let key_pair = actor_key_pair();

    store
        .insert_actor_key_pair(&actor_id, &key_pair)
        .expect("store actor keys");

    assert_eq!(
        store
            .load_actor_key_pair(&actor_id)
            .expect("load actor keys"),
        Some(key_pair)
    );
}

#[test]
fn provisioning_reuses_existing_actor_key_without_generation() {
    let store = SqliteStore::open_in_memory().expect("open store");
    let actor_id = iri("https://local.example/users/alice");
    let key_pair = actor_key_pair();
    store
        .insert_actor_key_pair(&actor_id, &key_pair)
        .expect("store actor keys");

    let provisioned = store
        .load_or_generate_actor_key_pair(&actor_id, &mut OsRng)
        .expect("reuse actor key");

    assert_eq!(provisioned, key_pair);
}

#[test]
fn sqlite_store_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<SqliteStore>();
}
