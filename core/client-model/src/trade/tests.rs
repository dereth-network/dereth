use super::*;
use crate::{RecordingRequests, RecordingSink};

fn profile(id: u32) -> ContentProfile {
    ContentProfile {
        iid: ObjectId(id),
        container_properties: 0,
    }
}

// ------------------------------------------------------------------------------------
// the six corrections, each pinned against retail
// ------------------------------------------------------------------------------------

/// **Corrections 1, 2 and 3.** Registration writes exactly three fields:
/// `_partner`, `_stamp` and `_status = 2`. It does not flush the lists, does not clear the
/// acceptances, and does not touch `_initiator`.
///
/// The previous transcription did all four of those, and its own test asserted the wrong
/// status by reading it back through the same symbol. The literal **2** is stated here.
#[test]
fn register_sets_status_open_and_leaves_everything_else_alone() {
    let mut t = Trade::default();
    t.add_item(TradeListId::SelfList, ObjectId(10), 0);
    t.add_item(TradeListId::Partner, ObjectId(20), 0);
    t.accepted = true;
    t.partner_accepted = true;
    t.initiator = true;

    assert!(t.register(ObjectId(0x5000_0002), 12.5));

    assert_eq!(t.status as u32, 2, "registration writes the literal 2");
    assert_eq!(t.status, TradeStatus::Open);
    assert_eq!(t.partner, ObjectId(0x5000_0002));
    assert!((t.stamp - 12.5).abs() < f64::EPSILON);
    assert_eq!(t.self_list.len(), 1, "Register does not flush the lists");
    assert_eq!(t.partner_list.len(), 1);
    assert!(t.accepted && t.partner_accepted, "nor the acceptances");
    assert!(t.initiator, "nor _initiator, which the system writes");

    // A zero partner returns 0 — it registers nothing at all.
    let mut z = Trade::default();
    assert!(!z.register(ObjectId(0), 3.0));
    assert_eq!(z.status, TradeStatus::Undef);
}

/// **Correction 4.** The default stamp stores `0xBFF00000` in its high dword and
/// `_stamp` and `0` in the low one. As a literal: the bit pattern and the value.
#[test]
fn a_fresh_trade_starts_with_a_stamp_of_minus_one() {
    assert_eq!(Trade::default().stamp.to_bits(), 0xBFF0_0000_0000_0000);
    assert!((Trade::default().stamp - (-1.0)).abs() < f64::EPSILON);
    assert!((INITIAL_STAMP - (-1.0)).abs() < f64::EPSILON);
    // Zero is a stamp the server may legitimately send, which is why the two must differ.
    assert_ne!(Trade::default().stamp, 0.0);
}

/// **Correction 5.** Adding refuses a duplicate, honours the insert position and
/// caps the list at the literal `0x1A0A`.
#[test]
fn add_item_refuses_duplicates_inserts_at_a_position_and_caps_the_list() {
    assert_eq!(MAX_TRADE_LIST, 0x1A0A, "the cmp operand in ");
    assert_eq!(MAX_TRADE_LIST, 6666);

    let mut t = Trade::default();
    assert!(t.add_item(TradeListId::SelfList, ObjectId(10), 0));
    assert!(
        t.add_item(TradeListId::SelfList, ObjectId(11), 0),
        "position 0 = insert first"
    );
    assert_eq!(t.self_list, vec![profile(11), profile(10)]);
    assert!(t.add_item(TradeListId::SelfList, ObjectId(12), 1));
    assert_eq!(t.self_list, vec![profile(11), profile(12), profile(10)]);
    // A position past the end appends, which is the list insert's final branch.
    assert!(t.add_item(TradeListId::SelfList, ObjectId(13), 99));
    assert_eq!(t.self_list.last(), Some(&profile(13)));

    assert!(
        !t.add_item(TradeListId::SelfList, ObjectId(10), 0),
        "Search rejects a duplicate"
    );
    assert_eq!(t.self_list.len(), 4);
    assert!(
        !t.add_item(TradeListId::SelfList, ObjectId(0), 0),
        "iid == 0 adds nothing"
    );
    assert!(
        !t.add_item(TradeListId::Undef, ObjectId(14), 0),
        "no side, no list"
    );

    // The cross-list search prevents the same object from appearing on both sides.
    assert!(!t.add_item(TradeListId::Partner, ObjectId(10), 0));
    assert!(t.partner_list.is_empty());

    // The cap.
    let mut full = Trade::default();
    for i in 0..MAX_TRADE_LIST {
        assert!(full.add_item(
            TradeListId::SelfList,
            ObjectId(u32::try_from(i).expect("a small index") + 1),
            i
        ));
    }
    assert_eq!(full.self_list.len(), MAX_TRADE_LIST);
    assert!(
        !full.add_item(TradeListId::SelfList, ObjectId(0x7FFF_FFFF), 0),
        "the list count < 0x1A0A is the whole guard"
    );
}

/// **Correction 6.** Removal searches **one** list, chosen by the side.
///
/// The previous transcription searched both, so a `0x0201` naming the partner's side would
/// have deleted a same-id entry out of the player's own offer.
#[test]
fn remove_item_only_touches_the_side_it_is_given() {
    let mut t = Trade::default();
    t.add_item(TradeListId::SelfList, ObjectId(10), 0);
    t.add_item(TradeListId::Partner, ObjectId(20), 0);

    assert!(
        !t.remove_item(ObjectId(10), TradeListId::Partner),
        "wrong side, no removal"
    );
    assert_eq!(t.self_list.len(), 1);
    assert!(!t.remove_item(ObjectId(20), TradeListId::SelfList));
    assert_eq!(t.partner_list.len(), 1);
    assert!(
        !t.remove_item(ObjectId(10), TradeListId::Undef),
        "side 0 does nothing"
    );
    assert!(
        !t.remove_item(ObjectId(0), TradeListId::SelfList),
        "iid 0 does nothing"
    );

    assert!(t.remove_item(ObjectId(10), TradeListId::SelfList));
    assert!(t.self_list.is_empty());
    assert!(t.remove_item(ObjectId(20), TradeListId::Partner));
    assert!(t.partner_list.is_empty());
}

/// The wire values of both enums, as literals — `TradeStatus` skips 3, and `side` is 1/2.
#[test]
fn the_status_enum_skips_three_and_the_sides_are_one_and_two() {
    assert_eq!(TradeStatus::Undef as u32, 0);
    assert_eq!(TradeStatus::Pending as u32, 1);
    assert_eq!(TradeStatus::Open as u32, 2);
    assert_eq!(TradeStatus::WaitingToClose as u32, 4);
    assert_eq!(TradeListId::SelfList as u32, 1);
    assert_eq!(TradeListId::Partner as u32, 2);
    assert_eq!(TradeListId::from_wire(1), TradeListId::SelfList);
    assert_eq!(TradeListId::from_wire(2), TradeListId::Partner);
    assert_eq!(TradeListId::from_wire(0), TradeListId::Undef);
    assert_eq!(
        TradeListId::from_wire(3),
        TradeListId::Undef,
        "any other value is no list"
    );
}

/// Reset both lists and both acceptances while preserving the registration.
#[test]
fn reset_keeps_the_registration() {
    let mut t = Trade::default();
    t.register(ObjectId(0x5000_0002), 7.0);
    t.add_item(TradeListId::SelfList, ObjectId(10), 0);
    t.accepted = true;
    t.partner_accepted = true;
    t.reset();
    assert!(t.self_list.is_empty() && t.partner_list.is_empty());
    assert!(!t.accepted && !t.partner_accepted);
    assert_eq!(t.partner, ObjectId(0x5000_0002));
    assert_eq!(t.status, TradeStatus::Open);
    assert!((t.stamp - 7.0).abs() < f64::EPSILON);
}

/// The four counters and the two sums the accept path actually compares.
///
/// **Every profile the client stores carries `container_properties == 0`**, because
/// constructing `ContentProfile(iid)` writes zero and nothing else writes it —
/// so `num_containers` is structurally 0 and `num_self_objects` is the list length.
/// The filters are kept as the client writes them, and this asserts both facts.
#[test]
fn the_counters_split_on_container_properties_which_is_always_zero() {
    let mut t = Trade::default();
    t.add_item(TradeListId::SelfList, ObjectId(10), 0);
    t.add_item(TradeListId::SelfList, ObjectId(11), 1);
    t.add_item(TradeListId::Partner, ObjectId(20), 0);
    assert!(t.self_list.iter().all(|c| c.container_properties == 0));
    assert_eq!(t.num_items(), 2);
    assert_eq!(t.num_containers(), 0);
    assert_eq!(t.num_partner_items(), 1);
    assert_eq!(t.num_partner_containers(), 0);
    assert_eq!(t.num_self_objects(), 2);
    assert_eq!(t.num_partner_objects(), 1);
    assert!(t.is_partner_trading_item(ObjectId(20)));
    assert!(!t.is_partner_trading_item(ObjectId(10)));
    assert!(!t.both_accepted());
    t.accepted = true;
    t.partner_accepted = true;
    assert!(t.both_accepted());

    // A `Trade` unpacked off the wire *can* carry a non-zero one, and then the split matters.
    let mut w = Trade::default();
    w.self_list.push(ContentProfile {
        iid: ObjectId(30),
        container_properties: 1,
    });
    assert_eq!(w.num_items(), 0);
    assert_eq!(w.num_containers(), 1);
    assert_eq!(w.num_self_objects(), 1);
}

// ------------------------------------------------------------------------------------
// the anti-scam comparison
// ------------------------------------------------------------------------------------

/// Exercise the client's complete anti-scam comparison.
#[test]
fn accepting_requires_the_displayed_lists_to_agree_with_the_mirror() {
    let mut t = Trade::default();
    t.add_item(TradeListId::SelfList, ObjectId(10), 0);
    t.add_item(TradeListId::Partner, ObjectId(20), 0);
    assert_eq!(accept_decision(&t, 1, 1), AcceptDecision::Accept);
    assert_eq!(accept_decision(&t, 2, 1), AcceptDecision::OutOfSync);
    assert_eq!(accept_decision(&t, 1, 0), AcceptDecision::OutOfSync);
    assert_eq!(accept_decision(&t, 0, 0), AcceptDecision::OutOfSync);
}

// ------------------------------------------------------------------------------------
// the system on the world
// ------------------------------------------------------------------------------------

const ME: ObjectId = ObjectId(0x5000_0001);
const PARTNER: ObjectId = ObjectId(0x5000_0002);
const ITEM: ObjectId = ObjectId(0x8000_0010);

fn world_with_player() -> crate::World {
    let mut w = crate::World::new();
    w.player = Some(ME);
    w
}

fn register(w: &mut crate::World) {
    let mut out = RecordingSink::default();
    w.handle_register_trade(
        &dereth_protocol::trade::TradeRegisterTrade {
            initiator: ME,
            partner: PARTNER,
            stamp: 4.25,
        },
        &mut out,
        dereth_primitives::ServerTime(0.0),
    );
}

/// `0x01FD` — the ids, the mirror, and the `_initiator` flag the *system* writes.
#[test]
fn a_register_trade_message_opens_the_mirror() {
    let mut w = world_with_player();
    let mut out = RecordingSink::default();
    assert!(w.handle_register_trade(
        &dereth_protocol::trade::TradeRegisterTrade {
            initiator: ME,
            partner: PARTNER,
            stamp: 4.25
        },
        &mut out,
        dereth_primitives::ServerTime(0.0),
    ));
    assert_eq!(w.trade.initiator, ME);
    assert_eq!(w.trade.partner, PARTNER);
    assert_eq!(w.trade.trade.status, TradeStatus::Open);
    assert!((w.trade.trade.stamp - 4.25).abs() < f64::EPSILON);
    assert!(w.trade.trade.initiator, "the initiator is me");
    assert!(matches!(out.0[0], crate::Notice::TradeRegistered { .. }));

    // …and when it is not.
    let mut w = world_with_player();
    let mut out = RecordingSink::default();
    w.handle_register_trade(
        &dereth_protocol::trade::TradeRegisterTrade {
            initiator: PARTNER,
            partner: PARTNER,
            stamp: 1.0,
        },
        &mut out,
        dereth_primitives::ServerTime(0.0),
    );
    assert!(!w.trade.trade.initiator);
}

/// `0x0202`'s three arms, including the one that is easy to lose: **`source == 0` clears your
/// own acceptance**, and is not a decline.
#[test]
fn accept_trade_from_the_server_has_three_arms() {
    let mut w = world_with_player();
    let mut out = RecordingSink::default();
    register(&mut w);

    w.handle_accept_trade(ME, &mut out);
    assert!(w.trade.trade.accepted && !w.trade.trade.partner_accepted);
    w.handle_accept_trade(PARTNER, &mut out);
    assert!(w.trade.trade.both_accepted());
    w.handle_accept_trade(ObjectId(0), &mut out);
    assert!(!w.trade.trade.accepted, "source 0 clears mine");
    assert!(w.trade.trade.partner_accepted, "and only mine");

    // `0x0203` has no zero arm at all: anything that is not me is the partner.
    w.handle_decline_trade(PARTNER, &mut out);
    assert!(!w.trade.trade.partner_accepted);
    w.handle_accept_trade(ME, &mut out);
    w.handle_decline_trade(ME, &mut out);
    assert!(!w.trade.trade.accepted);
    w.handle_accept_trade(ME, &mut out);
    w.handle_decline_trade(ObjectId(0), &mut out);
    assert!(
        w.trade.trade.accepted,
        "0x0203 with source 0 is NOT me, so it clears the partner's flag, not mine"
    );
}

/// `0x0208 Trade_ClearTradeAcceptance` raises its notice and **changes no mirror flag**. A client that helpfully
/// cleared them here would disagree with the server about who has accepted.
#[test]
fn clear_trade_acceptance_preserves_the_mirror_and_resets_displayed_lists() {
    let mut w = world_with_player();
    let mut out = RecordingSink::default();
    register(&mut w);
    w.handle_accept_trade(ME, &mut out);
    w.handle_accept_trade(PARTNER, &mut out);
    out.0.clear();
    w.handle_clear_trade_acceptance(dereth_primitives::ServerTime(0.0), &mut out);
    assert!(
        w.trade.trade.both_accepted(),
        "the protocol handler clears no mirror flag"
    );
    assert_eq!(w.trade.display_lists, Some([Vec::new(), Vec::new()]));
    assert!(
        w.trade.acceptance_darkened,
        "the notice receiver darkens the window"
    );
    assert_eq!(out.0.len(), 1);
    assert!(matches!(out.0[0], crate::Notice::TradeAcceptanceCleared));
}

#[test]
fn post_clear_trade_rows_follow_removal_and_failure_not_the_stale_mirror() {
    let mut w = world_with_player();
    let mut out = RecordingSink::default();
    register(&mut w);
    w.handle_clear_trade_acceptance(dereth_primitives::ServerTime(0.0), &mut out);
    for side in [1, 2] {
        w.handle_add_to_trade(
            &dereth_protocol::trade::TradeAddToTradeRecv {
                item: ObjectId(side),
                side,
                container_properties: 0,
            },
            &mut out,
        );
    }
    assert_eq!(
        w.trade.display_lists,
        Some([vec![ObjectId(1)], vec![ObjectId(2)]])
    );
    w.handle_remove_from_trade(
        &dereth_protocol::trade::TradeRemoveFromTrade {
            item: ObjectId(2),
            side: 2,
        },
        dereth_primitives::ServerTime(0.0),
        &mut out,
    );
    w.handle_trade_failure(
        &dereth_protocol::trade::TradeTradeFailure {
            item: ObjectId(1),
            reason: 0,
        },
        &mut out,
    );
    assert_eq!(w.trade.display_lists, Some([Vec::new(), Vec::new()]));
    w.handle_reset_trade(ME, dereth_primitives::ServerTime(0.0), &mut out);
    assert_eq!(
        w.trade.display_lists, None,
        "a real mirror reset retires the display override"
    );
}

/// The item-move handler exits before either destruction-queue call when the
/// partner row already exists. Once the window is flushed, the same mirror-known move takes
/// the missing-row arm and unreaps the container and its contents.
#[test]
fn a_partner_move_unreaps_only_a_missing_display_row() {
    let mut w = world_with_player();
    let mut out = RecordingSink::default();
    register(&mut w);
    let child = ObjectId(0x8000_0011);
    w.tables.weenies.insert(ITEM, crate::Weenie::new(ITEM));
    let mut child_weenie = crate::Weenie::new(child);
    child_weenie.pwd.container_id = Some(ITEM);
    w.tables.weenies.insert(child, child_weenie);
    w.tables
        .inventories
        .insert(ITEM, crate::objects::ObjectInventory::new(ITEM));
    w.tables
        .inventories
        .get_mut(ITEM)
        .expect("container")
        .add_content(child, false, 0);
    w.handle_add_to_trade(
        &dereth_protocol::trade::TradeAddToTradeRecv {
            item: ITEM,
            side: 2,
            container_properties: 0,
        },
        &mut out,
    );

    w.schedule_destroy(child, dereth_primitives::ServerTime(1.0));
    assert!(
        !w.trade_item_moved_to_partner(ITEM, PARTNER),
        "the row already exists"
    );
    assert!(
        w.tables.doomed.contains_key(child),
        "the early return preserves its deadline"
    );

    w.handle_clear_trade_acceptance(dereth_primitives::ServerTime(2.0), &mut out);
    assert!(
        w.trade_item_moved_to_partner(ITEM, PARTNER),
        "the flushed row is missing"
    );
    assert!(
        !w.tables.doomed.contains_key(child),
        "the missing-row arm unreaps contents"
    );
    assert_eq!(w.trade.display_lists, Some([Vec::new(), vec![ITEM]]));
}

/// `0x0200`'s third dword is an insert position: two items added at position 0 come back in
/// reverse order, which a build that treated it as container properties could not produce.
#[test]
fn the_third_dword_of_add_to_trade_is_an_insert_position() {
    let mut w = world_with_player();
    let mut out = RecordingSink::default();
    register(&mut w);
    let add = |w: &mut crate::World, out: &mut RecordingSink, id: u32, side: u32, pos: u32| {
        w.handle_add_to_trade(
            &dereth_protocol::trade::TradeAddToTradeRecv {
                item: ObjectId(id),
                side,
                container_properties: pos,
            },
            out,
        )
    };
    assert!(add(&mut w, &mut out, 0x10, 1, 0));
    assert!(add(&mut w, &mut out, 0x11, 1, 0));
    // A **non-zero** position, without which a build that ignored the dword entirely would
    // produce the same list and this test could not tell the two apart.
    assert!(add(&mut w, &mut out, 0x12, 1, 1));
    assert_eq!(
        w.trade
            .trade
            .self_list
            .iter()
            .map(|c| c.iid.0)
            .collect::<Vec<_>>(),
        vec![0x11, 0x12, 0x10]
    );
    assert!(w
        .trade
        .trade
        .self_list
        .iter()
        .all(|c| c.container_properties == 0));
    // Side 2 is the partner's.
    assert!(add(&mut w, &mut out, 0x20, 2, 0));
    assert_eq!(w.trade.trade.num_partner_objects(), 1);
    // Any other side adds nothing but still raises the notice.
    out.0.clear();
    assert!(!add(&mut w, &mut out, 0x30, 7, 0));
    assert_eq!(
        out.0.len(),
        1,
        "the add-item-to-trade notice is unconditional"
    );
}

/// `0x0207 Trade_TradeFailure` rolls the item back out of **side 1**, whatever it was, and
/// clears its trade state.
#[test]
fn a_trade_failure_rolls_the_item_out_of_your_own_offer() {
    let mut w = world_with_player();
    let mut out = RecordingSink::default();
    register(&mut w);
    w.handle_add_to_trade(
        &dereth_protocol::trade::TradeAddToTradeRecv {
            item: ITEM,
            side: 1,
            container_properties: 0,
        },
        &mut out,
    );
    assert!(w.handle_trade_failure(
        &dereth_protocol::trade::TradeTradeFailure {
            item: ITEM,
            reason: 9
        },
        &mut out,
    ));
    assert!(w.trade.trade.self_list.is_empty());
}

/// Adding an item marks it and removing it unmarks it.
#[test]
fn adding_an_item_marks_it_and_removing_it_unmarks_it() {
    let mut w = world_with_player();
    let mut out = RecordingSink::default();
    let mut req = RecordingRequests::default();
    register(&mut w);
    // A weenie the player owns.
    w.tables.weenies.insert(
        ITEM,
        crate::weenie::Weenie {
            id: ITEM,
            ..Default::default()
        },
    );
    if let Some(x) = w.weenie_mut(ITEM) {
        x.pwd.container_id = Some(ME);
    }

    assert!(
        w.trade_add_item(ITEM, 0, &mut out, &mut req),
        "the item is carried"
    );
    assert_eq!(
        w.weenie(ITEM).expect("exists").trade_state,
        1,
        "the trade state is set to 1"
    );
    assert_eq!(req.0.len(), 1);
    assert!(matches!(req.0[0], crate::Request::TradeAddToTrade(_)));

    w.handle_remove_from_trade(
        &dereth_protocol::trade::TradeRemoveFromTrade {
            item: ITEM,
            side: 1,
        },
        dereth_primitives::ServerTime(0.0),
        &mut out,
    );
    assert_eq!(w.weenie(ITEM).expect("exists").trade_state, 0);

    // …and the whole-list clear, which is the client's first loop.
    w.set_trade_state(ITEM, 1);
    assert_eq!(w.clear_all_trade_states(), 1);
    assert_eq!(w.weenie(ITEM).expect("exists").trade_state, 0);
    assert_eq!(w.clear_all_trade_states(), 0, "nothing left to clear");
}

/// The ownership check refuses an item the player is not carrying, with the
/// client's own literal string, and sends nothing.
#[test]
fn an_item_the_player_is_not_carrying_is_refused_with_the_clients_own_words() {
    let mut w = world_with_player();
    let mut out = RecordingSink::default();
    let mut req = RecordingRequests::default();
    w.tables.weenies.insert(
        ITEM,
        crate::weenie::Weenie {
            id: ITEM,
            ..Default::default()
        },
    );

    assert_eq!(w.trade_item_acceptable(ITEM), Some(messages::ONLY_CARRIED));
    assert!(!w.trade_add_item(ITEM, 0, &mut out, &mut req));
    assert_eq!(req.0.len(), 0, "nothing leaves the machine");
    assert_eq!(
        out.0,
        vec![crate::Notice::DisplayString {
            feedback: dereth_client_contract::feedback::Feedback::WARNING,
            channel: 0x1A,
            text: "You can only trade items you are carrying".to_string(),
        }]
    );
    assert_eq!(w.weenie(ITEM).expect("exists").trade_state, 0);
    // An object the tables do not hold at all is refused the same way.
    assert_eq!(
        w.trade_item_acceptable(ObjectId(0x8000_9999)),
        Some(messages::ONLY_CARRIED)
    );
}

/// The three refusal strings, as retail's own literals. Nothing else states them.
#[test]
fn the_three_refusal_strings_are_the_clients_own() {
    assert_eq!(
        messages::ONLY_CARRIED,
        "You can only trade items you are carrying"
    );
    assert_eq!(
        messages::MUST_SPLIT,
        "You must split the stack before trading it."
    );
    assert_eq!(messages::CANNOT_SPLIT, "Cannot split the stack to trade it");
    assert_eq!(TRADE_MESSAGE_CHANNEL, 0x1A);
}

/// **The out-of-sync fallback, which is the part a rebuild that drops it can be robbed over.**
///
/// Both branches send `0x01FA`. The accepting one carries the mirror and sets `_accepted`; the
/// refusing one carries a **default-constructed** `Trade` — empty lists, no partner, stamp
/// `-1.0` — and sets nothing.
#[test]
fn accepting_out_of_sync_sends_an_empty_trade_and_does_not_set_accepted() {
    let mut w = world_with_player();
    let mut out = RecordingSink::default();
    register(&mut w);
    w.handle_add_to_trade(
        &dereth_protocol::trade::TradeAddToTradeRecv {
            item: ITEM,
            side: 1,
            container_properties: 0,
        },
        &mut out,
    );

    // The window shows two rows where the mirror holds one.
    let mut req = RecordingRequests::default();
    assert_eq!(w.accept_trade(2, 0, &mut req), AcceptDecision::OutOfSync);
    assert!(
        !w.trade.trade.accepted,
        "the out-of-sync notice does not accept"
    );
    let crate::Request::TradeAcceptTrade(sent) = &req.0[0] else {
        panic!("0x01FA")
    };
    assert!(sent.0.self_list.is_empty() && sent.0.partner_list.is_empty());
    assert_eq!(sent.0.partner, ObjectId(0));
    assert_eq!(sent.0.stamp.to_bits(), 0xBFF0_0000_0000_0000);

    // …and the agreeing case carries the mirror.
    let mut req = RecordingRequests::default();
    assert_eq!(w.accept_trade(1, 0, &mut req), AcceptDecision::Accept);
    assert!(w.trade.trade.accepted);
    let crate::Request::TradeAcceptTrade(sent) = &req.0[0] else {
        panic!("0x01FA")
    };
    assert_eq!(sent.0.self_list.len(), 1);
    assert_eq!(sent.0.partner, PARTNER);
    assert_eq!(sent.0.accepted, 1);
    assert_eq!(sent.0.status, 2);
}

/// The three empty-bodied senders, and the one thing about them worth stating: **reset is
/// `0x0204`**, not a member of the `0x01F6..0x01FE` block.
#[test]
fn the_empty_senders_carry_the_opcodes_their_bytes_spell() {
    use dereth_protocol::Message;
    let mut w = world_with_player();
    let mut out = RecordingSink::default();
    let mut req = RecordingRequests::default();
    register(&mut w);
    w.handle_add_to_trade(
        &dereth_protocol::trade::TradeAddToTradeRecv {
            item: ITEM,
            side: 1,
            container_properties: 0,
        },
        &mut out,
    );
    w.handle_accept_trade(ME, &mut out);
    w.handle_accept_trade(PARTNER, &mut out);

    w.decline_trade(&mut req);
    w.handle_accept_trade(ME, &mut out);
    w.reset_trade_request(&mut req);
    w.close_trade_negotiations(dereth_primitives::ServerTime(0.0), &mut out, &mut req);
    assert_eq!(req.0.len(), 3);
    assert!(matches!(req.0[0], crate::Request::TradeDeclineTrade(_)));
    assert!(matches!(req.0[1], crate::Request::TradeResetTrade(_)));
    assert!(matches!(
        req.0[2],
        crate::Request::TradeCloseTradeNegotiations(_)
    ));
    assert_eq!(
        dereth_protocol::trade::TradeDeclineTradeRequest::OPCODE.0,
        0x01FB
    );
    assert_eq!(
        dereth_protocol::trade::TradeResetTradeRequest::OPCODE.0,
        0x0204
    );
    assert_eq!(
        dereth_protocol::trade::TradeCloseTradeNegotiations::OPCODE.0,
        0x01F7
    );
    assert_eq!(dereth_protocol::trade::TradeAddToTrade::OPCODE.0, 0x01F8);
    assert_eq!(
        dereth_protocol::trade::TradeAcceptTradeRequest::OPCODE.0,
        0x01FA
    );

    // The close sender has no side effects beyond sending the request. Both callers pair it
    // with a displayed-list reset; neither operation is a
    // `SetVisible`**, so the displayed lists are flushed, the mirror remains live until
    // `0x01FF`, and the window stays up.
    assert!(
        w.trade.open,
        "both close-sender callers send and flush; they do not hide"
    );
    assert_eq!(w.trade.display_lists, Some([Vec::new(), Vec::new()]));
    assert_eq!(
        w.trade
            .trade
            .self_list
            .iter()
            .map(|p| p.iid)
            .collect::<Vec<_>>(),
        vec![ITEM]
    );
    assert!(w.trade.trade.both_accepted());
    assert!(w.trade.acceptance_darkened);

    // The close button is the one path that hides it; that visibility change raises the send.
    w.close_trade_window(dereth_primitives::ServerTime(0.0), &mut out, &mut req);
    assert!(!w.trade.open);
    assert!(matches!(
        req.0[3],
        crate::Request::TradeCloseTradeNegotiations(_)
    ));
    assert_eq!(req.0.len(), 4, "one request either way");
}

/// A close notice zeroes both ids and resets the mirror; ending the character session discards
/// the whole system.
#[test]
fn closing_and_ending_the_session_both_clear_the_system() {
    let mut w = world_with_player();
    let mut out = RecordingSink::default();
    register(&mut w);
    w.handle_open_trade(PARTNER, &mut out);
    assert!(w.trade.open);
    w.handle_close_trade(3, dereth_primitives::ServerTime(0.0), &mut out);
    assert_eq!(w.trade.partner, ObjectId(0));
    assert_eq!(w.trade.initiator, ObjectId(0));
    assert!(w.trade.open, "a 0x01FF leaves an empty window on screen");

    register(&mut w);
    w.trade.end_character_session();
    assert_eq!(w.trade, TradeSystem::default());
    assert_eq!(w.trade.trade.stamp.to_bits(), 0xBFF0_0000_0000_0000);
}
