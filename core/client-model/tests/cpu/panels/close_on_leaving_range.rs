//! Vendor and ground-container windows close past their use radius (zero when omitted); secure
//! trade closes at 5 m; the selection watch clears or re-arms by the in-view flag and is not armed
//! for carried items; registrants are independent; book/slumlord consumers; poll count and missing
//! body.
//! Fixture: recorded messages and synthetic state or packets.

use dereth_client_model::range::{ObjectRangeGeometry, RangeHandler, SECURE_TRADE_RANGE};
use dereth_client_model::{Notice, RecordingRequests, RecordingSink, Request, World};
use dereth_primitives::{ObjectId, ServerTime};
use dereth_protocol::types::PublicWeenieDesc;

const PLAYER: ObjectId = ObjectId(0x5000_0001);
const VENDOR: ObjectId = ObjectId(0x8000_0001);
const CORPSE: ObjectId = ObjectId(0x8000_0002);
const PARTNER: ObjectId = ObjectId(0x5000_0002);
const TARGET: ObjectId = ObjectId(0x8000_0003);
const BOOK: ObjectId = ObjectId(0x8000_0004);

const USE_RADIUS: f32 = 3.0;

/// A distance table standing in for [`dereth_client_runtime::object_range::SceneRangeGeometry`].
///
/// `dereth-client-model` holds no positions, so the geometry is a trait; this is the same trait the client
/// implements over `dereth_physics::math`. A missing entry is a missing physics body, which
/// the weenie's objects-in-range walk answers as out of range.
#[derive(Default)]
struct Distances(std::collections::BTreeMap<u32, f32>);

impl Distances {
    fn at(&mut self, id: ObjectId, d: f32) {
        self.0.insert(id.0, d);
    }
}

impl ObjectRangeGeometry for Distances {
    fn distance(&self, object: ObjectId, _player: ObjectId, _r: bool, _z: bool) -> Option<f32> {
        self.0.get(&object.0).copied()
    }
}

fn put(w: &mut World, id: ObjectId, pwd: PublicWeenieDesc) {
    let mut it = dereth_client_model::Weenie::new(id);
    it.pwd = pwd;
    it.valid = true;
    w.tables.weenies.insert(id, it);
}

fn world() -> World {
    let mut w = World::new();
    put(&mut w, PLAYER, PublicWeenieDesc::default());
    w.player = Some(PLAYER);
    w.tables.inventories.insert(
        PLAYER,
        dereth_client_model::objects::ObjectInventory::new(PLAYER),
    );
    put(
        &mut w,
        VENDOR,
        PublicWeenieDesc {
            use_radius: Some(USE_RADIUS),
            ..PublicWeenieDesc::default()
        },
    );
    put(
        &mut w,
        CORPSE,
        PublicWeenieDesc {
            use_radius: Some(USE_RADIUS),
            // `USEABLE_REMOTE` (0x20), the remote-use bit — a corpse is
            // used from where you stand. See `use_is_range_blind`'s `USEABLE_REMOTE`.
            useability: Some(0x0000_0020),
            items_capacity: Some(10),
            bitfield: dereth_rules::weenie::bitfield::OPENABLE,
            ..PublicWeenieDesc::default()
        },
    );
    put(&mut w, PARTNER, PublicWeenieDesc::default());
    put(&mut w, TARGET, PublicWeenieDesc::default());
    put(
        &mut w,
        BOOK,
        PublicWeenieDesc {
            use_radius: Some(USE_RADIUS),
            ..PublicWeenieDesc::default()
        },
    );
    w
}

/// One frame of the player-system tick's range-check tail.
fn tick(
    w: &mut World,
    g: &Distances,
    now: f64,
) -> (
    Vec<Notice>,
    Vec<Request>,
    dereth_client_model::range::RangeCheckStats,
) {
    let mut out = RecordingSink::default();
    let mut req = RecordingRequests::default();
    let stats = w.calculate_object_range_checks(
        ServerTime(now),
        g,
        dereth_client_model::range::RADAR_RADIUS_OUTDOORS,
        &mut out,
        &mut req,
    );
    (out.0, req.0, stats)
}

fn open_vendor(w: &mut World, now: f64) {
    let mut out = RecordingSink::default();
    let mut req = RecordingRequests::default();
    w.handle_vendor_info(
        &dereth_protocol::trade::VendorInfo {
            merchant_id: VENDOR,
            profile: dereth_protocol::trade::VendorProfile::default(),
            items: Vec::new(),
        },
        &mut out,
        &mut req,
        ServerTime(now),
    );
}

fn open_corpse(w: &mut World, now: f64) {
    let mut out = RecordingSink::default();
    let mut req = RecordingRequests::default();
    w.set_ground_object(&mut req, &mut out, Some(CORPSE), true, ServerTime(now));
    w.on_view_contents(CORPSE, &[], &mut out, ServerTime(now));
}

fn open_trade(w: &mut World, now: f64) {
    let mut out = RecordingSink::default();
    w.handle_register_trade(
        &dereth_protocol::trade::TradeRegisterTrade {
            initiator: PLAYER,
            partner: PARTNER,
            stamp: 1.0,
        },
        &mut out,
        ServerTime(now),
    );
}

// ---------------------------------------------------------------------------------------------
// Registrant #2 — the vendor panel opening a vendor.
// ---------------------------------------------------------------------------------------------

/// Behaviour: panels.range.a-vendor-ground-container-or-trade-closes-when-the-player-walks-away
/// Two stations: inside the vendor's use radius, then outside it.
///
/// The leave edge is the panel's own range exit → `SetVisible(false)` →
/// its visibility change → the UI system's close-vendor, which is
/// the close-vendor handler, raising [`Notice::CloseVendor`].
#[test]
fn a_vendor_window_closes_when_the_player_leaves_its_use_radius() {
    let mut w = world();
    open_vendor(&mut w, 0.0);
    assert!(w.shop.is_open(), "the shop opened");
    assert!(
        w.object_range_checks
            .is_watching(RangeHandler::Vendor, VENDOR),
        "and `OpenVendor`'s tail armed the range check"
    );
    let armed = w
        .object_range_checks
        .live()
        .find(|e| e.handler == RangeHandler::Vendor)
        .copied()
        .expect("armed");
    assert!(
        (armed.range - f64::from(USE_RADIUS)).abs() < 1e-6,
        "at the vendor's own use radius, not a literal"
    );
    assert!(
        armed.use_radii && !armed.ignore_z_delta,
        "useRadii = 1, ignoreZDelta = 0"
    );

    // Station 1: inside. `2.9 <= 3.0`.
    let mut g = Distances::default();
    g.at(VENDOR, 2.9);
    let (notices, _, stats) = tick(&mut w, &g, 1.5);
    assert_eq!(stats.polled, 1, "the poll happened");
    assert_eq!(stats.exits, 0);
    assert!(
        w.shop.is_open(),
        "staying inside the radius must not close the shop"
    );
    assert!(!notices.iter().any(|n| matches!(n, Notice::CloseVendor)));

    // Station 2: outside.
    g.at(VENDOR, 3.1);
    let (notices, _, stats) = tick(&mut w, &g, 3.0);
    assert_eq!(stats.exits, 1, "the leave edge fired exactly once");
    assert!(!w.shop.is_open(), "and the vendor window is closed");
    assert_eq!(
        notices
            .iter()
            .filter(|n| matches!(n, Notice::CloseVendor))
            .count(),
        1,
        "the close-vendor handler raised its notice"
    );
    // Nothing else was disturbed by this edge.
    assert_eq!(w.selected, None);
    assert_eq!(w.ground_object, None);
    assert!(!w.trade.open);
}

// ---------------------------------------------------------------------------------------------
// Registrant #5 — the external-container panel taking a ground object.
// ---------------------------------------------------------------------------------------------

/// Two stations, and the leave edge is the interesting one:
/// the panel's range exit → `SetVisible(false)` → its visibility change →
/// its close-current-container, whose first act is to use the ground object with both flags zero.
/// That is not a stray use: ACE's `Container.ActOnUse` closes a container already open
/// by this viewer, so the use is how the client asks the server to close it, and the server's
/// `0x0052` is what finally clears the ground-object id — which retail does not clear
/// here either.
#[test]
fn an_open_ground_container_closes_when_the_player_leaves_its_use_radius() {
    let mut w = world();
    open_corpse(&mut w, 0.0);
    assert_eq!(w.ground_object, Some(CORPSE));
    assert!(
        w.object_range_checks
            .is_watching(RangeHandler::ExternalContainer, CORPSE),
        "`SetGroundObject`'s non-zero arm armed the range check"
    );

    let mut g = Distances::default();
    g.at(CORPSE, 2.9);
    let (notices, requests, stats) = tick(&mut w, &g, 1.5);
    assert_eq!(stats.polled, 1);
    assert_eq!(stats.exits, 0);
    assert_eq!(
        w.ground_object,
        Some(CORPSE),
        "staying inside must not close it"
    );
    assert!(!notices
        .iter()
        .any(|n| matches!(n, Notice::SetGroundObject(ObjectId(0)))));
    assert!(requests.is_empty());

    g.at(CORPSE, 3.1);
    let (notices, requests, stats) = tick(&mut w, &g, 3.0);
    assert_eq!(stats.exits, 1);
    assert_eq!(
        notices
            .iter()
            .filter(|n| matches!(n, Notice::SetGroundObject(ObjectId(0))))
            .count(),
        1,
        "the panel's `SetVisible(false)`, as this build expresses it"
    );
    assert!(
        requests
            .iter()
            .any(|r| matches!(r, Request::UseEvent(u) if u.object == CORPSE)),
        "closing the current container's use request on the ground object reached the wire"
    );
    assert!(
        !w.object_range_checks
            .is_watching(RangeHandler::ExternalContainer, CORPSE),
        "and the registration is gone"
    );
    assert!(!w.shop.is_open() && !w.trade.open, "nothing else moved");
}

/// Omitted use radius still arms vendor and ground container at zero.
#[test]
fn omitted_use_radius_still_arms_vendor_and_ground_container_at_zero() {
    // The vendor half.
    let mut w = world();
    w.weenie_mut(VENDOR).expect("vendor").pwd.use_radius = None;
    open_vendor(&mut w, 0.0);
    assert!(w.shop.is_open());
    let watch = w
        .object_range_checks
        .live()
        .find(|entry| entry.handler == RangeHandler::Vendor)
        .copied()
        .expect("the omitted field still registers");
    assert_eq!(watch.range, 0.0, "the descriptor's reset default");
    assert!(watch.use_radii && !watch.ignore_z_delta);

    let mut at = Distances::default();
    at.at(VENDOR, 0.0);
    let (notices, requests, stats) = tick(&mut w, &at, 2.0);
    assert_eq!(stats.polled, 1);
    assert_eq!(
        stats.exits, 0,
        "zero distance is inside the inclusive zero radius"
    );
    assert!(notices.is_empty() && requests.is_empty());
    assert!(w.shop.is_open());

    at.at(VENDOR, 0.01);
    let (notices, _, stats) = tick(&mut w, &at, 4.0);
    assert_eq!(stats.exits, 1);
    assert!(!w.shop.is_open());
    assert_eq!(
        notices
            .iter()
            .filter(|n| matches!(n, Notice::CloseVendor))
            .count(),
        1
    );

    // The ground-container half, in its own world for the reason above.
    let mut w = world();
    w.weenie_mut(CORPSE).expect("corpse").pwd.use_radius = None;
    open_corpse(&mut w, 0.0);
    assert_eq!(w.ground_object, Some(CORPSE));
    let watch = w
        .object_range_checks
        .live()
        .find(|entry| entry.handler == RangeHandler::ExternalContainer)
        .copied()
        .expect("the omitted field still registers");
    assert_eq!(watch.range, 0.0, "the descriptor's reset default");
    assert!(watch.use_radii && !watch.ignore_z_delta);

    let mut at = Distances::default();
    at.at(CORPSE, 0.0);
    let (notices, requests, stats) = tick(&mut w, &at, 2.0);
    assert_eq!(stats.polled, 1);
    assert_eq!(
        stats.exits, 0,
        "zero distance is inside the inclusive zero radius"
    );
    assert!(notices.is_empty() && requests.is_empty());
    assert_eq!(w.ground_object, Some(CORPSE));

    at.at(CORPSE, 0.01);
    let (notices, requests, stats) = tick(&mut w, &at, 4.0);
    assert_eq!(stats.exits, 1);
    assert_eq!(
        notices
            .iter()
            .filter(|n| matches!(n, Notice::SetGroundObject(ObjectId(0))))
            .count(),
        1
    );
    assert_eq!(
        requests
            .iter()
            .filter(|r| matches!(r, Request::UseEvent(u) if u.object == CORPSE))
            .count(),
        1
    );
}

// ---------------------------------------------------------------------------------------------
// Registrant #4 — the secure-trade panel registering a trade.
// ---------------------------------------------------------------------------------------------

/// The one registration with a literal range: **5.0 m**, `push 0x40140000; push 0`, and no
/// dependence on the partner's use radius — which the partner in this fixture does not have, so
/// a transcription that reached for the use radius here would register nothing at all.
#[test]
fn a_secure_trade_closes_at_five_metres_and_not_at_the_partners_use_radius() {
    let mut w = world();
    open_trade(&mut w, 0.0);
    let mut out = RecordingSink::default();
    w.handle_add_to_trade(
        &dereth_protocol::trade::TradeAddToTradeRecv {
            item: TARGET,
            side: 1,
            container_properties: 0,
        },
        &mut out,
    );
    w.handle_accept_trade(PLAYER, &mut out);
    w.handle_accept_trade(PARTNER, &mut out);
    assert!(w.trade.open);
    let armed = w
        .object_range_checks
        .live()
        .find(|e| e.handler == RangeHandler::SecureTrade)
        .copied()
        .expect("the trade-register handler armed the range check");
    assert!(
        (armed.range - SECURE_TRADE_RANGE).abs() < 1e-9,
        "the literal 5.0"
    );
    assert_eq!(armed.object, PARTNER);

    let mut g = Distances::default();
    g.at(PARTNER, 4.9);
    let (_, requests, stats) = tick(&mut w, &g, 1.5);
    assert_eq!(stats.polled, 1);
    assert_eq!(stats.exits, 0);
    assert!(
        w.trade.open,
        "staying inside must not close the negotiation"
    );
    assert!(requests.is_empty());

    g.at(PARTNER, 5.1);
    let (notices, requests, stats) = tick(&mut w, &g, 3.0);
    assert_eq!(stats.exits, 1);
    assert!(w.trade.open, "the range exit does not hide the window");
    assert_eq!(w.trade.display_lists, Some([Vec::new(), Vec::new()]));
    assert_eq!(
        w.trade
            .trade
            .self_list
            .iter()
            .map(|p| p.iid)
            .collect::<Vec<_>>(),
        vec![TARGET]
    );
    assert!(
        w.trade.trade.both_accepted(),
        "the UI Reset is not the authoritative 0x01FF"
    );
    assert!(w.trade.acceptance_darkened);
    assert!(
        requests
            .iter()
            .any(|r| matches!(r, Request::TradeCloseTradeNegotiations(_))),
        " put `0x01F7` on the wire"
    );
    assert!(notices
        .iter()
        .any(|n| matches!(n, Notice::CloseSecureTrade { .. })));
    assert!(
        !w.shop.is_open() && w.ground_object.is_none(),
        "nothing else moved"
    );
}

// ---------------------------------------------------------------------------------------------
// Registrant #6 — the player system taking the set-selected-item notice.
// ---------------------------------------------------------------------------------------------

/// The selection watch clears the selection or re arms by the in view flag.
#[test]
fn the_selection_watch_clears_the_selection_or_re_arms_by_the_in_view_flag() {
    let mut w = world();
    let mut out = RecordingSink::default();
    w.set_selected_object(Some(TARGET), false, &mut out);

    let mut g = Distances::default();
    g.at(TARGET, 10.0);
    // The first tick reconciles the selection edge and arms the watch.
    let (_, _, stats) = tick(&mut w, &g, 0.0);
    assert_eq!(stats.polled, 0, "nothing was armed when this tick started");
    let armed = w
        .object_range_checks
        .live()
        .find(|e| e.handler == RangeHandler::Selection)
        .copied()
        .expect("armed");
    assert!(
        (armed.range - f64::from(dereth_client_model::range::RADAR_RADIUS_OUTDOORS)).abs() < 1e-6,
        "at the radar radius, the outdoor 75"
    );
    assert!(
        armed.ignore_z_delta,
        "the only registrant measured with `xy_distance`"
    );

    // Station 1: inside 75 m.
    let (_, _, stats) = tick(&mut w, &g, 1.5);
    assert_eq!(stats.polled, 1);
    assert_eq!(stats.exits, 0);
    assert_eq!(
        w.selected,
        Some(TARGET),
        "staying inside must not clear the selection"
    );

    // Station 2: outside, with the flag clear — the arm a never-drawn selection takes.
    g.at(TARGET, 75.1);
    let (_, _, stats) = tick(&mut w, &g, 3.0);
    assert_eq!(stats.exits, 1);
    assert_eq!(stats.selection_rearms, 0);
    assert_eq!(w.selected, None, "the selection is cleared");
    assert!(!w
        .object_range_checks
        .is_watching(RangeHandler::Selection, TARGET));

    // Station 2': the same edge with the flag set — the usual arm, once a frame has drawn it.
    let mut w = world();
    let mut out = RecordingSink::default();
    w.set_selected_object(Some(TARGET), false, &mut out);
    w.selected_object_in_view = true;
    tick(&mut w, &g, 0.0);
    let (_, _, stats) = tick(&mut w, &g, 3.0);
    assert_eq!(stats.exits, 1);
    assert_eq!(
        stats.selection_rearms, 1,
        "`is_selected_object_in_view` re-registers"
    );
    assert_eq!(w.selected, Some(TARGET), "and the selection survives");
    assert!(
        w.object_range_checks
            .is_watching(RangeHandler::Selection, TARGET),
        "the watch is armed again rather than retired"
    );

    // The smart box's find-object is the only thing that clears the flag in the client.
    w.find_object();
    assert!(!w.selected_object_in_view);
}

/// The selected-item notice handler's four-way gate: the watch is armed
/// only for something **out in the world**.
///
/// An item in your own pack, in the open ground container, in the vendor's stock or on the trade
/// partner is exempt, because walking away from those is not walking away from the item. Each of
/// the four exemptions is driven separately — an aggregate would pass with three of them dead.
#[test]
fn the_selection_watch_is_not_armed_for_something_you_are_carrying_or_looking_at() {
    let contained = |owner: ObjectId| PublicWeenieDesc {
        container_id: Some(owner),
        ..PublicWeenieDesc::default()
    };

    // (a) owned by the player — `is_owned_by_player`.
    let mut w = world();
    put(&mut w, TARGET, contained(PLAYER));
    let mut out = RecordingSink::default();
    w.set_selected_object(Some(TARGET), false, &mut out);
    tick(&mut w, &Distances::default(), 0.0);
    assert!(
        !w.object_range_checks
            .is_watching(RangeHandler::Selection, TARGET),
        "in my pack"
    );

    // (b) inside the open ground container — `is_owned_by_object` with the ground object.
    let mut w = world();
    put(&mut w, TARGET, contained(CORPSE));
    open_corpse(&mut w, 0.0);
    let mut out = RecordingSink::default();
    w.set_selected_object(Some(TARGET), false, &mut out);
    tick(&mut w, &Distances::default(), 0.0);
    assert!(
        !w.object_range_checks
            .is_watching(RangeHandler::Selection, TARGET),
        "in the corpse"
    );

    // (c) in the vendor's stock — `is_owned_by_object` with the vendor.
    let mut w = world();
    put(&mut w, TARGET, contained(VENDOR));
    open_vendor(&mut w, 0.0);
    let mut out = RecordingSink::default();
    w.set_selected_object(Some(TARGET), false, &mut out);
    tick(&mut w, &Distances::default(), 0.0);
    assert!(
        !w.object_range_checks
            .is_watching(RangeHandler::Selection, TARGET),
        "vendor stock"
    );

    let mut w = world();
    put(&mut w, TARGET, contained(PARTNER));
    open_trade(&mut w, 0.0);
    let mut out = RecordingSink::default();
    w.set_selected_object(Some(TARGET), false, &mut out);
    tick(&mut w, &Distances::default(), 0.0);
    assert!(
        !w.object_range_checks
            .is_watching(RangeHandler::Selection, TARGET),
        "on the table"
    );
    assert_eq!(
        w.viewcone_check_object_id,
        Some(TARGET),
        "the exemption jumps to the common tail even though nothing was registered"
    );

    // …and the control: an ordinary object in the world IS watched, so the four negatives above
    // are measurements and not a gate that refuses everything.
    let mut w = world();
    let mut out = RecordingSink::default();
    w.set_selected_object(Some(TARGET), false, &mut out);
    tick(&mut w, &Distances::default(), 0.0);
    assert!(
        w.object_range_checks
            .is_watching(RangeHandler::Selection, TARGET),
        "control"
    );

    // …and selecting something else drops the previous watch. That is the unregister loop,
    // which the selected-item handler runs over the previous selection before it registers
    // anything — without it every object you ever clicked would keep a live poll.
    let other = ObjectId(0x8000_0009);
    put(&mut w, other, PublicWeenieDesc::default());
    let mut out = RecordingSink::default();
    w.set_selected_object(Some(other), false, &mut out);
    tick(&mut w, &Distances::default(), 0.0);
    assert!(
        !w.object_range_checks
            .is_watching(RangeHandler::Selection, TARGET),
        "the previous selection's watch is retired"
    );
    assert!(
        w.object_range_checks
            .is_watching(RangeHandler::Selection, other),
        "and the new one is armed"
    );
}

// ---------------------------------------------------------------------------------------------
// The whole census, and the two that are absent.
// ---------------------------------------------------------------------------------------------

/// The four registrants this build has are independent.
#[test]
fn the_four_registrants_this_build_has_are_independent() {
    let mut w = world();
    open_vendor(&mut w, 0.0);
    assert!(
        w.object_range_checks
            .is_watching(RangeHandler::Vendor, VENDOR),
        "the vendor arms first, so the exclusion below is a teardown and not a no-op"
    );
    open_corpse(&mut w, 0.0);
    assert!(
        !w.shop.is_open(),
        "opening a ground container closes the vendor first"
    );
    assert!(
        !w.object_range_checks
            .is_watching(RangeHandler::Vendor, VENDOR),
        "and `CloseVendor` takes the vendor's range watch with it"
    );
    open_trade(&mut w, 0.0);
    let mut out = RecordingSink::default();
    w.set_selected_object(Some(TARGET), false, &mut out);

    let mut g = Distances::default();
    g.at(CORPSE, 1.0);
    g.at(PARTNER, 1.0);
    g.at(TARGET, 1.0);
    tick(&mut w, &g, 0.5);
    let live: Vec<RangeHandler> = w.object_range_checks.live().map(|e| e.handler).collect();
    assert_eq!(live.len(), 3, "three armed: {live:?}");

    // One at a time, in the order their radii differ.
    g.at(PARTNER, 6.0); // > 5.0
    let (notices, _, s) = tick(&mut w, &g, 2.0);
    assert_eq!(s.exits, 1);
    assert!(
        notices
            .iter()
            .any(|n| matches!(n, Notice::CloseSecureTrade { .. })),
        "the trade watch, and only it, ended its negotiation. Got {notices:?}"
    );
    assert!(w.ground_object.is_some() && w.selected.is_some());

    g.at(CORPSE, 4.0); // > 3.0
    let (_, _, s) = tick(&mut w, &g, 6.0);
    assert_eq!(s.exits, 1);
    assert!(w.selected.is_some());

    g.at(TARGET, 80.0); // > 75.0
    let (_, _, s) = tick(&mut w, &g, 8.0);
    assert_eq!(s.exits, 1);
    assert_eq!(w.selected, None);
    assert_eq!(
        w.object_range_checks.live().count(),
        0,
        "and the list is empty"
    );

    // **The fourth registrant, in its own world** — the one thing the exclusion above costs is
    // the vendor's place in the ladder, so it is driven beside the two registrants it *can*
    // coexist with, and the same "only its own thing" claim is asserted for it.
    let mut w = world();
    open_vendor(&mut w, 0.0);
    open_trade(&mut w, 0.0);
    let mut out = RecordingSink::default();
    w.set_selected_object(Some(TARGET), false, &mut out);

    let mut g = Distances::default();
    g.at(VENDOR, 1.0);
    g.at(PARTNER, 1.0);
    g.at(TARGET, 1.0);
    tick(&mut w, &g, 0.5);
    let live: Vec<RangeHandler> = w.object_range_checks.live().map(|e| e.handler).collect();
    assert_eq!(live.len(), 3, "three armed beside the vendor: {live:?}");

    g.at(VENDOR, 4.0); // > 3.0
    let (_, _, s) = tick(&mut w, &g, 2.0);
    assert_eq!(s.exits, 1);
    assert!(!w.shop.is_open(), "the vendor's own edge closed the shop");
    assert!(
        w.trade.open && w.selected.is_some(),
        "and disturbed neither of the other two"
    );
}

/// The book panel is registrant 1 in the original client. Opening an unowned book registers its use radius,
/// and the range driver's matching leave edge reaches the visible panel notice rather than the
/// former consumerless counter.
#[test]
fn the_book_registrant_has_a_range_consumer() {
    assert!(RangeHandler::Book.has_range_exit_consumer_in_this_build());
    assert!(RangeHandler::Slumlord.has_range_exit_consumer_in_this_build());

    let mut w = world();
    let mut out = RecordingSink::default();
    w.open_book(
        BOOK,
        1,
        dereth_protocol::trade::PageDataList::default(),
        String::new(),
        ObjectId(0),
        String::new(),
        &mut out,
        ServerTime(0.0),
    );
    assert!(
        w.object_range_checks.is_watching(RangeHandler::Book, BOOK),
        "OpenBook registered the unowned book watch"
    );

    let (notices, requests, stats) = tick(&mut w, &Distances::default(), 2.0);
    assert_eq!(stats.exits, 1);
    assert_eq!(stats.exits_without_a_window, 0);
    assert_eq!(notices, vec![Notice::CloseBook(BOOK)]);
    assert!(requests.is_empty());
}

/// The public weenie description's reset initialises the use radius to `0.0`; `OpenBook` reads that
/// member directly after its sole object/ownership guards, so omission on the wire still registers
/// a zero-radius watch rather than suppressing the registration.
#[test]
fn an_unowned_book_with_no_wire_use_radius_registers_the_native_zero_default() {
    let mut w = world();
    w.weenie_mut(BOOK).expect("book").pwd.use_radius = None;
    let mut out = RecordingSink::default();
    w.open_book(
        BOOK,
        1,
        dereth_protocol::trade::PageDataList::default(),
        String::new(),
        ObjectId(0),
        String::new(),
        &mut out,
        ServerTime(0.0),
    );
    let watch = w
        .object_range_checks
        .live()
        .find(|entry| entry.handler == RangeHandler::Book)
        .expect("the omitted field still registers");
    assert_eq!(watch.range, 0.0);
}

/// The denominator, so "the checks ran and nothing left range" and "the checks never ran" are
/// different answers — and the missing-body arm, which is what closes a panel whose object's cell
/// was released.
#[test]
fn the_poll_count_is_a_denominator_and_a_missing_body_is_out_of_range() {
    let mut w = world();
    open_vendor(&mut w, 0.0);

    // Nothing has reached `next_update` yet.
    let mut g = Distances::default();
    g.at(VENDOR, 1.0);
    let (_, _, s) = tick(&mut w, &g, 0.5);
    assert_eq!(s.polled, 0, "armed but not yet due");
    assert_eq!(s.exits, 0);
    assert!(w.shop.is_open());

    // Due, and in range.
    let (_, _, s) = tick(&mut w, &g, 1.5);
    assert_eq!(s.polled, 1);
    assert_eq!(s.exits, 0);

    // The object loses its body: the physics lookup returns null and `objects_in_range` returns 0.
    let (_, _, s) = tick(&mut w, &Distances::default(), 3.0);
    assert_eq!(s.polled, 1);
    assert_eq!(s.exits, 1);
    assert!(
        !w.shop.is_open(),
        "an object with no physics body is out of range"
    );
}
