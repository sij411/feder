mod common;

use feder_vocab::{Follow, Reference, Undo};
use ref_feder_core::undo::{UndoFollowError, receive_undo_follow};

use common::{actor, iri};

#[test]
fn receives_undo_for_embedded_follow() {
    let local = actor("https://local.example/users/alice");
    let remote = actor("https://remote.example/users/bob");
    let follow = Follow::new(
        iri("https://remote.example/activities/follow/1"),
        Reference::id(remote.id.clone()),
        Reference::id(local.id.clone()),
    );
    let undo = Undo::new(
        iri("https://remote.example/activities/undo/1"),
        Reference::id(remote.id.clone()),
        Reference::object(follow),
    );

    let outcome = receive_undo_follow(&local, &remote, undo).expect("valid Undo");

    assert_eq!(outcome.follower, remote.id);
    assert_eq!(outcome.following, local.id);
}

#[test]
fn rejects_linked_follow_and_wrong_actor_or_object() {
    let local = actor("https://local.example/users/alice");
    let remote = actor("https://remote.example/users/bob");
    let other = actor("https://remote.example/users/mallory");
    let linked = Undo::new(
        iri("https://remote.example/activities/undo/1"),
        Reference::id(remote.id.clone()),
        Reference::id(iri("https://remote.example/activities/follow/1")),
    );
    let wrong_actor = Undo::new(
        iri("https://remote.example/activities/undo/2"),
        Reference::id(other.id),
        Reference::object(Follow::new(
            iri("https://remote.example/activities/follow/2"),
            Reference::id(remote.id.clone()),
            Reference::id(local.id.clone()),
        )),
    );
    let wrong_object = Undo::new(
        iri("https://remote.example/activities/undo/3"),
        Reference::id(remote.id.clone()),
        Reference::object(Follow::new(
            iri("https://remote.example/activities/follow/3"),
            Reference::id(remote.id.clone()),
            Reference::id(iri("https://local.example/users/mallory")),
        )),
    );

    assert_eq!(
        receive_undo_follow(&local, &remote, linked),
        Err(UndoFollowError::LinkedFollow)
    );
    assert_eq!(
        receive_undo_follow(&local, &remote, wrong_actor),
        Err(UndoFollowError::WrongActor)
    );
    assert_eq!(
        receive_undo_follow(&local, &remote, wrong_object),
        Err(UndoFollowError::WrongObject)
    );
}
