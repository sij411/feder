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

use alloc::string::String;

use feder_vocab::{Actor, Create, Iri, Note, Reference, References};

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

    CreateNoteOutcome { note, activity }
}
