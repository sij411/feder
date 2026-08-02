mod common;

use feder_core::note::{
    CreateNoteInput, NoteRecipient, PUBLIC_COLLECTION, create_note, is_public_note,
};
use feder_vocab::{Note, Reference, References};

use common::{actor, iri};

#[test]
fn creates_note_and_create_activity_from_runtime_facts() {
    let mut local = actor("https://local.example/users/alice");
    local.followers = Some(iri("https://local.example/users/alice/followers"));
    let remote = iri("https://remote.example/users/bob");
    let input = CreateNoteInput {
        note_id: iri("https://local.example/posts/1"),
        create_id: iri("https://local.example/activities/create/1"),
        to: References::one(iri(PUBLIC_COLLECTION)),
        cc: References::many([local.followers.clone().expect("followers"), remote.clone()]),
        content: "hello".to_string(),
        media_type: Some("text/html".to_string()),
        published: Some("2026-08-02T00:00:00Z".to_string()),
        url: Some(iri("https://local.example/@alice/1")),
    };

    let outcome = create_note(&local, input);

    assert_eq!(
        outcome.note.attributed_to,
        Some(Reference::id(local.id.clone()))
    );
    assert_eq!(outcome.note.content.as_deref(), Some("hello"));
    assert_eq!(
        outcome.activity.object,
        Reference::object(outcome.note.clone())
    );
    assert_eq!(outcome.activity.actor, Reference::id(local.id.clone()));
    assert_eq!(outcome.activity.to, outcome.note.to);
    assert_eq!(outcome.activity.cc, outcome.note.cc);
    assert_eq!(
        outcome.recipients,
        vec![
            NoteRecipient::Followers(local.id),
            NoteRecipient::Actor(remote),
        ]
    );
}

#[test]
fn deduplicates_note_recipients_and_skips_public_and_local_addresses() {
    let mut local = actor("https://local.example/users/alice");
    let followers = iri("https://local.example/users/alice/followers");
    local.followers = Some(followers.clone());
    let remote = iri("https://remote.example/users/bob");
    let outcome = create_note(
        &local,
        CreateNoteInput {
            note_id: iri("https://local.example/posts/1"),
            create_id: iri("https://local.example/activities/create/1"),
            to: References::many([
                iri(PUBLIC_COLLECTION),
                local.id.clone(),
                followers.clone(),
                remote.clone(),
            ]),
            cc: References::many([followers, remote.clone()]),
            content: "hello".to_string(),
            media_type: None,
            published: None,
            url: None,
        },
    );

    assert_eq!(
        outcome.recipients,
        vec![
            NoteRecipient::Followers(local.id),
            NoteRecipient::Actor(remote),
        ]
    );
}

#[test]
fn recognizes_public_note_addressing() {
    let mut public = Note::new(iri("https://local.example/posts/1"));
    public.cc = References::one(iri(PUBLIC_COLLECTION));
    let mut private = Note::new(iri("https://local.example/posts/2"));
    private.to = References::one(iri("https://remote.example/users/bob"));

    assert!(is_public_note(&public));
    assert!(!is_public_note(&private));
}
