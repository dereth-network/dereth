//! A script asking for an object by its id.

use auberean_script::value::{split, tag, NULL};
use auberean_script::Status;

use crate::common::{Bench, HOLDER_REF, ITEM, ITEM_REF};

/// A move's answer asking for the item it moves finds nothing while the item is being made, and
/// finds it after; it finds any other object being made, so a container just made still takes the
/// move; another script finds the item at once.
///
/// Behaviour: script.model.a-moves-answer-does-not-find-its-item-being-made
#[test]
fn a_moves_answer_does_not_find_its_item_being_made() {
    let mut b = Bench::new();
    let (low, high) = split(ITEM);
    let id = [
        (high, tag::LONG | tag::MIXED),
        (low, tag::LONG | tag::MIXED),
    ];
    b.host.instances.insert(ITEM, ITEM_REF);
    b.host.being_made.insert(ITEM);
    b.host.answering_a_move = Some(ITEM);
    let (status, slots) = b.call(0x7F, 0x0A, HOLDER_REF, &id, 1);
    assert_eq!(status, Status::Success);
    assert_eq!(slots[0].0, NULL, "the item, being made");
    b.host.answering_a_move = Some(ITEM + 1);
    let (_, slots) = b.call(0x7F, 0x0A, HOLDER_REF, &id, 1);
    assert_eq!(slots[0].0, ITEM_REF, "a move of something else finds it");
    b.host.answering_a_move = None;
    let (_, slots) = b.call(0x7F, 0x0A, HOLDER_REF, &id, 1);
    assert_eq!(slots[0].0, ITEM_REF, "another script finds it");
    b.host.answering_a_move = Some(ITEM);
    b.host.being_made.clear();
    let (status, slots) = b.call(0x7F, 0x0A, HOLDER_REF, &id, 1);
    assert_eq!(status, Status::Success);
    assert_eq!(slots[0].0, ITEM_REF, "made, the answer finds it");
}
