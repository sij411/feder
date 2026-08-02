mod common;

use feder_vocab::{Accept, Follow, Reference};
use ref_feder_core::follow::{
    AcceptFollowError, FollowError, PendingFollow, create_follow, receive_accept_follow,
    receive_follow,
};

use common::{actor, iri};

#[test]
fn creates_outbound_follow_and_pending_relationship() {
    let local = actor("https://local.example/users/alice");
    let remote = actor("https://remote.example/users/bob");
    let follow_id = iri("https://local.example/activities/follow/1");

    let outcome = create_follow(&local, &remote, follow_id.clone());

    assert_eq!(outcome.relationship.local_actor, local.id);
    assert_eq!(outcome.relationship.remote_actor, remote);
    assert_eq!(outcome.relationship.follow_activity, follow_id);
    assert_eq!(outcome.activity.id, outcome.relationship.follow_activity);
    assert_eq!(outcome.activity.actor, Reference::id(local.id));
    assert_eq!(
        outcome.activity.object,
        Reference::id(outcome.relationship.remote_actor.id)
    );
}

#[test]
fn receives_follow_as_transient_storage_and_delivery_outcome() {
    let local = actor("https://local.example/users/alice");
    let remote = actor("https://remote.example/users/bob");
    let follow = Follow::new(
        iri("https://remote.example/activities/follow/1"),
        Reference::id(remote.id.clone()),
        Reference::id(local.id.clone()),
    );

    let outcome = receive_follow(
        &local,
        &remote,
        follow.clone(),
        iri("https://local.example/activities/accept/1"),
    )
    .expect("valid Follow");

    assert_eq!(outcome.follower, remote);
    assert_eq!(outcome.following, local.id);
    assert_eq!(outcome.recipient_inbox, outcome.follower.inbox);
    assert_eq!(outcome.accept.actor, Reference::id(outcome.following));
    let Reference::Object(accepted_follow) = outcome.accept.object else {
        panic!("Accept must embed the verified Follow");
    };
    assert_eq!(accepted_follow.id, follow.id);
    assert_eq!(accepted_follow.object, follow.object);
    assert_eq!(
        accepted_follow.actor,
        Reference::object(outcome.follower.clone())
    );
}

#[test]
fn rejects_follow_with_wrong_actor_or_object() {
    let local = actor("https://local.example/users/alice");
    let remote = actor("https://remote.example/users/bob");
    let other = actor("https://remote.example/users/mallory");
    let accept_id = iri("https://local.example/activities/accept/1");
    let wrong_actor = Follow::new(
        iri("https://remote.example/activities/follow/1"),
        Reference::id(other.id),
        Reference::id(local.id.clone()),
    );
    let wrong_object = Follow::new(
        iri("https://remote.example/activities/follow/2"),
        Reference::id(remote.id.clone()),
        Reference::id(iri("https://local.example/users/mallory")),
    );

    assert_eq!(
        receive_follow(&local, &remote, wrong_actor, accept_id.clone()),
        Err(FollowError::WrongActor)
    );
    assert_eq!(
        receive_follow(&local, &remote, wrong_object, accept_id),
        Err(FollowError::WrongObject)
    );
}

#[test]
fn confirms_accept_for_the_exact_pending_follow() {
    let local = actor("https://local.example/users/alice");
    let remote = actor("https://remote.example/users/bob");
    let pending = PendingFollow {
        local_actor: local.id.clone(),
        remote_actor: remote.clone(),
        follow_activity: iri("https://local.example/activities/follow/1"),
    };
    let follow = Follow::new(
        pending.follow_activity.clone(),
        Reference::id(local.id.clone()),
        Reference::id(remote.id.clone()),
    );
    let accept = Accept::new(
        iri("https://remote.example/activities/accept/1"),
        Reference::id(remote.id.clone()),
        Reference::object(follow),
    );

    receive_accept_follow(&local, &remote, &pending, accept).expect("valid Accept");
}

#[test]
fn rejects_accept_that_does_not_match_pending_relationship() {
    let local = actor("https://local.example/users/alice");
    let remote = actor("https://remote.example/users/bob");
    let other = actor("https://remote.example/users/mallory");
    let pending = PendingFollow {
        local_actor: local.id.clone(),
        remote_actor: remote.clone(),
        follow_activity: iri("https://local.example/activities/follow/1"),
    };
    let wrong_actor = Accept::new(
        iri("https://remote.example/activities/accept/1"),
        Reference::id(other.id),
        Reference::id(pending.follow_activity.clone()),
    );
    let wrong_follow = Accept::new(
        iri("https://remote.example/activities/accept/2"),
        Reference::id(remote.id.clone()),
        Reference::id(iri("https://local.example/activities/follow/other")),
    );

    assert_eq!(
        receive_accept_follow(&local, &remote, &pending, wrong_actor),
        Err(AcceptFollowError::WrongActor)
    );
    assert_eq!(
        receive_accept_follow(&local, &remote, &pending, wrong_follow),
        Err(AcceptFollowError::WrongFollow)
    );
}
