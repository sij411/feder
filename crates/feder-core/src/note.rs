// Feder: A portable ActivityPub core for many runtimes.
// Copyright (C) 2026 Feder contributors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, version 3.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

use alloc::{string::String, vec::Vec};

use feder_vocab::{Actor, Create, Iri, Note, Reference, References};

pub const PUBLIC_COLLECTION: &str = "https://www.w3.org/ns/activitystreams#Public";

/// A transient delivery intent derived from a Note's addressing fields.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NoteRecipient {
    Followers(Iri),
    Actor(Iri),
}

/// Runtime-provided facts for constructing one local Note and Create activity.
///
/// IDs and timestamps are inputs so core does not depend on clocks, randomness,
/// or an operating-system-specific identifier source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateNoteInput {
    pub note_id: Iri,
    pub create_id: Iri,
    pub to: References<Iri>,
    pub cc: References<Iri>,
    pub content: String,
    pub media_type: Option<String>,
    pub published: Option<String>,
    pub url: Option<Iri>,
}

/// The transient result of constructing one local Note.
///
/// Core retains neither value. A runtime persists `note`; `activity` remains
/// available for subsequent delivery orchestration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateNoteOutcome {
    pub note: Note,
    pub activity: Create<Note>,
    pub recipients: Vec<NoteRecipient>,
}

#[must_use]
pub fn create_note(local_actor: &Actor, input: CreateNoteInput) -> CreateNoteOutcome {
    let actor = Reference::id(local_actor.id.clone());
    let mut note = Note::new(input.note_id);
    note.attributed_to = Some(actor.clone());
    note.to = input.to;
    note.cc = input.cc;
    note.content = Some(input.content);
    note.media_type = input.media_type;
    note.published = input.published;
    note.url = input.url;

    let mut activity = Create::new(input.create_id, actor, Reference::object(note.clone()));
    activity.to = note.to.clone();
    activity.cc = note.cc.clone();

    let recipients = note_recipients(local_actor, &note);

    CreateNoteOutcome {
        note,
        activity,
        recipients,
    }
}

#[must_use]
pub fn is_public_note(note: &Note) -> bool {
    note.to
        .iter()
        .chain(note.cc.iter())
        .any(|recipient| recipient.as_str() == PUBLIC_COLLECTION)
}

fn note_recipients(local_actor: &Actor, note: &Note) -> Vec<NoteRecipient> {
    let mut recipients = Vec::new();

    for address in note.to.iter().chain(note.cc.iter()) {
        let recipient = if address.as_str() == PUBLIC_COLLECTION || address == &local_actor.id {
            continue;
        } else if local_actor.followers.as_ref() == Some(address) {
            NoteRecipient::Followers(local_actor.id.clone())
        } else {
            NoteRecipient::Actor(address.clone())
        };

        if !recipients.contains(&recipient) {
            recipients.push(recipient);
        }
    }

    recipients
}
