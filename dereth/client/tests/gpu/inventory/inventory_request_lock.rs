//! The inventory request lock: an inventory request wedges the lock until the server answers and
//! no amount of time clears it; a move ghosts the icon and moves nothing until the answer; and the
//! split state defaults to the whole stack.
//! Fixture: the client's `Interaction` and object model, with no device and no App.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::combat::game_view_clicks::{seed_container, seed_item, seed_player};

use dereth_client_model::inventory::SplitState;
use dereth_client_model::RecordingRequests;
use dereth_client_model::World;
use dereth_primitives::{ObjectId, ServerTime};

/// Behaviour: inventory.lock.an-inventory-request-wedges-until-answered
///
/// The inventory request lock has no timeout. A lost reply leaves the UI waiting until
/// another server item-move notification releases it. This test checks the persistent lock at
/// three later times; it does not exercise that eventual notification or add a timeout.
#[test]
fn the_inventory_lock_wedges_and_no_amount_of_time_clears_it() {
    let mut w = dereth_client_model::World::new();
    let player = ObjectId(0x5000_0002);
    seed_player(&mut w, player);
    let item = ObjectId(0x8000_0001);
    seed_item(&mut w, item, player);

    let mut req = RecordingRequests::default();
    let mut out = dereth_client_model::NullSink;
    w.attempt_put_in_3d(&mut req, &mut out, item, ServerTime(10.0), true)
        .expect("the first request goes");
    assert_eq!(req.0.len(), 1);

    // A day later, the lock is still held: there is no timeout anywhere in the client.
    for t in [11.0, 100.0, 86_400.0] {
        assert!(
            w.attempt_put_in_3d(&mut req, &mut out, item, ServerTime(t), true)
                .is_err(),
            "the lock must still be held at t = {t}"
        );
    }
    assert_eq!(req.0.len(), 1, "and nothing more reached the wire");
}

/// Inventory requests make no optimistic move: setting the waiting flag ghosts the
/// icon while its container remains unchanged until the server responds.
#[test]
fn an_inventory_move_ghosts_the_icon_and_moves_nothing() {
    let mut w = dereth_client_model::World::new();
    let player = ObjectId(0x5000_0002);
    let chest = ObjectId(0x8000_0500);
    let item = ObjectId(0x8000_0001);
    seed_player(&mut w, player);
    seed_item(&mut w, item, player);
    seed_container(&mut w, chest);

    let before = w.weenie(item).expect("item").pwd.container_id;
    let mut req = RecordingRequests::default();
    w.attempt_put_in_container(
        &mut req,
        &mut dereth_client_model::NullSink,
        item,
        chest,
        0,
        ServerTime(1.0),
        true,
    )
    .expect("goes");
    assert_eq!(req.0.len(), 1, "one request");
    assert_eq!(
        w.weenie(item).expect("item").pwd.container_id,
        before,
        "the item has NOT moved; only the server may move it"
    );
    assert!(
        w.weenie(item).expect("item").waiting,
        "the icon is ghosted instead"
    );
}

/// The stack slider's own default, asserted so the give/drop/split arms are known to be running the
/// whole-stack path rather than a split.
#[test]
fn the_split_state_defaults_to_the_whole_stack() {
    assert!(SplitState::default().is_whole_stack());
    assert!(World::new().split.is_whole_stack());
}
