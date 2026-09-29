//! Contracts for request lock.
//! Fixture: shared recorded messages and synthetic state.
//! Behaviour: none (codec, fixture conformance or host-state contracts)

use crate::common::model_replay::*;

#[test]
fn the_request_lock_clears_on_a_matching_reply_and_wedges_without_one() {
    let mut w = World::new();
    let mut req = RecordingRequests::default();
    let mut out = RecordingSink::default();
    let mut item = dereth_client_model::Weenie::new(ObjectId(0x8000_0001));
    item.pwd.name = "Sword".into();
    w.tables.weenies.insert(ObjectId(0x8000_0001), item);

    w.attempt_wield(
        &mut req,
        &mut out,
        ObjectId(0x8000_0001),
        0x0010_0000,
        SplitState::default(),
        ServerTime(1.0),
        true,
    )
    .unwrap();
    assert_eq!(w.request_lock.pending, InventoryRequest::Wield);

    w.server_says_move_item(
        ObjectId(0x8000_0002),
        ObjectId(0),
        0,
        ObjectId(0),
        0,
        false,
        &mut out,
    );
    assert_eq!(w.request_lock.pending, InventoryRequest::Wield);
    for t in 1..1000 {
        w.use_time(ServerTime(f64::from(t) * 60.0), &mut out, &mut NullRequests);
    }
    assert_eq!(
        w.request_lock.pending,
        InventoryRequest::Wield,
        "sixteen hours later the lock is still held: the retail client wedges"
    );

    w.server_says_move_item(
        ObjectId(0x8000_0001),
        ObjectId(0),
        0,
        ObjectId(0),
        0,
        false,
        &mut out,
    );
    assert!(w.request_lock.is_idle());
}
