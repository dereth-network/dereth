//! A script asking for an object by its id.

use auberean_script::value::{split, tag, NULL};
use auberean_script::Status;

use crate::common::{Bench, HOLDER_REF, ITEM, ITEM_REF};

/// A move's answer asking for an object by id finds nothing while it is being made, and finds it
/// after; another script finds it at once.
///
/// Behaviour: script.model.a-moves-answer-does-not-find-an-object-being-made
#[test]
fn a_moves_answer_does_not_find_an_object_being_made() {
    let mut b = Bench::new();
    let (low, high) = split(ITEM);
    let id = [
        (high, tag::LONG | tag::MIXED),
        (low, tag::LONG | tag::MIXED),
    ];
    b.host.instances.insert(ITEM, ITEM_REF);
    b.host.being_made.insert(ITEM);
    b.host.answering_a_move = true;
    let (status, slots) = b.call(0x7F, 0x0A, HOLDER_REF, &id, 1);
    assert_eq!(status, Status::Success);
    assert_eq!(slots[0].0, NULL);
    b.host.answering_a_move = false;
    let (_, slots) = b.call(0x7F, 0x0A, HOLDER_REF, &id, 1);
    assert_eq!(slots[0].0, ITEM_REF);
    b.host.answering_a_move = true;
    b.host.being_made.clear();
    let (status, slots) = b.call(0x7F, 0x0A, HOLDER_REF, &id, 1);
    assert_eq!(status, Status::Success);
    assert_eq!(slots[0].0, ITEM_REF);
}
