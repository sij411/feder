use feder_vocab::{Actor, Iri};

pub fn iri(value: &str) -> Iri {
    value.parse().expect("valid test IRI")
}

pub fn actor(id: &str) -> Actor {
    Actor::person(
        iri(id),
        iri(&format!("{id}/inbox")),
        iri(&format!("{id}/outbox")),
    )
}
