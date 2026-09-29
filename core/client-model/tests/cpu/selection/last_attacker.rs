//! Select-last-attacker is refused without a player desc, answered with nothing when there is no
//! attacker, gated on the visible table and an exclusive radar-range boundary (indoor/outdoor),
//! refuses bodiless attackers, raises no notice on reselect, reads quality 0x0B, and does not re-
//! arm the watch.
//! Fixture: recorded messages and synthetic state or packets.

use dereth_client_model::qualities::{Qualities, StatKey, StatType, StatValue};
use dereth_client_model::range::{RADAR_RADIUS_INDOORS, RADAR_RADIUS_OUTDOORS};
use dereth_client_model::selection::LAST_ATTACKER_IID;
use dereth_client_model::{RecordingSink, World};
use dereth_primitives::{CellId, ObjectId, ServerTime};

const PLAYER: ObjectId = ObjectId(0x5000_0001);
const ATTACKER: ObjectId = ObjectId(0x5000_0002);

/// A world with a local player that owns a player description, and one other object.
fn world() -> World {
    let mut w = World::new();
    for id in [PLAYER, ATTACKER] {
        let mut it = dereth_client_model::Weenie::new(id);
        it.valid = true;
        w.tables.weenies.insert(id, it);
    }
    w.player = Some(PLAYER);
    // The player description — the only place instance-id qualities live.
    w.tables
        .weenies
        .get_mut(PLAYER)
        .expect("the player")
        .qualities = Some(Qualities::new());
    w
}

fn make_visible(w: &mut World, id: ObjectId) {
    w.tables.physics.insert(
        id,
        dereth_client_model::objects::PhysicsPresence {
            cell: Some(CellId(0x00A9_0100)),
            state: 0,
            parent: None,
            setup_id: 0,
        },
    );
    w.update_visible_object_list();
    assert!(
        w.tables.visible.contains(&id),
        "the sweep did not make {id:?} visible"
    );
}

fn set_last_attacker(w: &mut World, id: ObjectId) {
    w.player_qualities_mut()
        .expect("the player description")
        .set(
            StatKey::new(StatType::Iid, LAST_ATTACKER_IID),
            StatValue::Iid(id),
        );
}

// =================================================================================================
// 1. `select_last_attacker` — every gate on its own.
// =================================================================================================

/// An absent player description makes its interface invalid, which is the **only** path that
/// returns `false` — and `OnAction`'s bool decides whether the event is offered to the next
/// listener, so the two `true` returns and the one `false` are three different behaviours.
#[test]
fn without_a_player_desc_the_action_is_refused_and_nothing_is_selected() {
    let mut w = world();
    make_visible(&mut w, ATTACKER);
    set_last_attacker(&mut w, ATTACKER);
    // ...and then take the player description away, which is the state before `0x0013
    // Login_PlayerDescription` and after player-description reset. The quality is set first only so that
    // the refusal below cannot be "there was nothing to select" wearing the wrong message.
    w.tables
        .weenies
        .get_mut(PLAYER)
        .expect("the player")
        .qualities = None;
    let mut out = RecordingSink::default();
    assert!(
        !w.select_last_attacker(Some((1.0, 1.0)), RADAR_RADIUS_OUTDOORS, &mut out),
        "no player qualities: the selection is refused"
    );
    assert_eq!(w.selected, None);
}

/// The instance-id lookup over a zeroed out-parameter: an unset quality reads as 0, and the action
/// returns **true** having selected nothing.
#[test]
fn with_no_last_attacker_the_action_is_answered_and_nothing_is_selected() {
    let mut w = world();
    make_visible(&mut w, ATTACKER);
    let mut out = RecordingSink::default();
    assert!(
        w.select_last_attacker(Some((1.0, 1.0)), RADAR_RADIUS_OUTDOORS, &mut out),
        "the client answers the action true even with nothing to select"
    );
    assert_eq!(w.selected, None, "there was nothing to select");
}

/// The visibility gate, on its own: in range and **not** in the
/// `visible_object_table`.
///
/// The negative and its control are in one test, because "nothing was selected" is satisfied by a
/// broken fixture as readily as by the gate.
#[test]
fn an_attacker_that_is_not_in_the_visible_table_is_refused_however_close_it_is() {
    let mut w = world();
    set_last_attacker(&mut w, ATTACKER);
    let mut out = RecordingSink::default();

    // Not visible: the object has no cell, so `update_visible_object_list` leaves it out.
    w.update_visible_object_list();
    assert!(
        !w.tables.visible.contains(&ATTACKER),
        "the premise: it is not visible"
    );
    assert!(w.select_last_attacker(Some((1.0, 1.0)), RADAR_RADIUS_OUTDOORS, &mut out));
    assert_eq!(w.selected, None, "the visibility gate refused it");

    // The control: the identical call with the one gate satisfied.
    make_visible(&mut w, ATTACKER);
    assert!(w.select_last_attacker(Some((1.0, 1.0)), RADAR_RADIUS_OUTDOORS, &mut out));
    assert_eq!(
        w.selected,
        Some(ATTACKER),
        "and with it visible, at 1.4 m, it is selected"
    );
}

/// Behaviour: selection.last-attacker.selects-a-visible-attacker-strictly-inside-radar-range
/// The distance gate, on its own, **at both ranges**, with the cell index the only
/// thing that differs between the two runs.
///
/// Stations are on a 36.87°-off-axis heading built from Pythagorean triples — (45, 60) is exactly
/// 75 and (15, 20) is exactly 25 in `f32` — so the exact-boundary station is exact rather than
/// approximate and is not a cardinal one.
#[test]
fn the_distance_gate_is_the_radar_range_and_it_excludes_the_boundary_at_both_ranges() {
    for (range, inside, edge, outside) in [
        (
            RADAR_RADIUS_OUTDOORS,
            (44.9_f32, 59.9_f32),
            (45.0_f32, 60.0_f32),
            (45.1_f32, 60.1_f32),
        ),
        (
            RADAR_RADIUS_INDOORS,
            (14.9, 19.9),
            (15.0, 20.0),
            (15.1, 20.1),
        ),
    ] {
        // Inside.
        let mut w = world();
        make_visible(&mut w, ATTACKER);
        set_last_attacker(&mut w, ATTACKER);
        let mut out = RecordingSink::default();
        assert!(w.select_last_attacker(Some(inside), range, &mut out));
        assert_eq!(w.selected, Some(ATTACKER), "inside {range}: {inside:?}");

        assert_eq!(
            (f64::from(edge.0) * f64::from(edge.0) + f64::from(edge.1) * f64::from(edge.1)).sqrt(),
            f64::from(range),
            "the station {edge:?} is not exactly on {range}"
        );
        let mut w = world();
        make_visible(&mut w, ATTACKER);
        set_last_attacker(&mut w, ATTACKER);
        assert!(w.select_last_attacker(Some(edge), range, &mut out));
        assert_eq!(
            w.selected, None,
            "exactly on {range}: {edge:?} is NOT in range"
        );

        // Outside.
        let mut w = world();
        make_visible(&mut w, ATTACKER);
        set_last_attacker(&mut w, ATTACKER);
        assert!(w.select_last_attacker(Some(outside), range, &mut out));
        assert_eq!(w.selected, None, "outside {range}: {outside:?}");
    }
}

/// One offset two answers because the range is chosen.
#[test]
fn one_offset_two_answers_because_the_range_is_chosen() {
    let at_40m = (24.0_f32, 32.0_f32); // 3/4/5 × 8

    let mut w = world();
    make_visible(&mut w, ATTACKER);
    set_last_attacker(&mut w, ATTACKER);
    let mut out = RecordingSink::default();
    assert!(w.select_last_attacker(Some(at_40m), RADAR_RADIUS_OUTDOORS, &mut out));
    assert_eq!(w.selected, Some(ATTACKER), "40 m outdoors");

    let mut w = world();
    make_visible(&mut w, ATTACKER);
    set_last_attacker(&mut w, ATTACKER);
    assert!(w.select_last_attacker(Some(at_40m), RADAR_RADIUS_INDOORS, &mut out));
    assert_eq!(w.selected, None, "40 m indoors");
}

/// A missing physics object, or a null cell.
///
/// The radar-range query returns false there. The hearing query's
/// equivalent escape returns **true**. One seam, two opposite
/// defaults, and both are asserted here so the asymmetry is a measurement rather than a comment.
#[test]
fn an_attacker_with_no_physics_object_is_refused_where_a_speaker_with_none_is_heard() {
    let mut w = world();
    make_visible(&mut w, ATTACKER);
    set_last_attacker(&mut w, ATTACKER);
    let mut out = RecordingSink::default();
    assert!(w.select_last_attacker(None, RADAR_RADIUS_OUTDOORS, &mut out));
    assert_eq!(w.selected, None, "no body -> not within radar range");

    let chat = dereth_client_model::chat::ChatState::default();
    assert!(
        chat.can_hear(ATTACKER, "", 0x02, None, RADAR_RADIUS_OUTDOORS),
        "the same absence, in `CanHear`, is audible at any range"
    );
}

/// Re selecting the same attacker raises no notice.
#[test]
fn re_selecting_the_same_attacker_raises_no_notice() {
    let mut w = world();
    make_visible(&mut w, ATTACKER);
    set_last_attacker(&mut w, ATTACKER);
    let mut out = RecordingSink::default();
    assert!(w.select_last_attacker(Some((1.0, 1.0)), RADAR_RADIUS_OUTDOORS, &mut out));
    let after_first = out.0.len();
    assert!(
        after_first > 0,
        "the first selection is an edge and raises something"
    );
    assert!(w.select_last_attacker(Some((1.0, 1.0)), RADAR_RADIUS_OUTDOORS, &mut out));
    assert_eq!(
        out.0.len(),
        after_first,
        "the second is not an edge, so nothing is raised"
    );
    assert_eq!(w.selected, Some(ATTACKER));

    // `force = true` on an unchanged selection, which skips the early
    // return: retail runs the body, reaches the old-versus-new compare with the two equal, and
    // skips the notice call. No notice, here or there.
    w.set_selected_object(Some(ATTACKER), true, &mut out);
    assert_eq!(
        out.0.len(),
        after_first,
        "`force = true` on an UNCHANGED selection raises no selection-changed notice -- the \
         notice call is inside the changed-guard's skip. See this test's doc comment."
    );
}

/// The visible table is `update_visible_object_list`'s, rebuilt at **1 Hz** — so an attacker whose
/// object arrived since the last sweep is refused even standing next to you, and one that has gone
/// on being visible is not.
///
/// Driven through the world's own timer rather than by calling the sweep directly, so this
/// is a statement about the client's rebuild interval and not about the helper above.
#[test]
fn the_visible_table_is_the_one_second_sweeps_and_the_gate_follows_it() {
    let mut w = world();
    set_last_attacker(&mut w, ATTACKER);
    let mut out = RecordingSink::default();
    let mut req = dereth_client_model::RecordingRequests::default();

    w.tables.physics.insert(
        ATTACKER,
        dereth_client_model::objects::PhysicsPresence {
            cell: Some(CellId(0x00A9_0100)),
            state: 0,
            parent: None,
            setup_id: 0,
        },
    );
    // Nothing has swept yet.
    assert!(!w.tables.visible.contains(&ATTACKER));
    assert!(w.select_last_attacker(Some((1.0, 1.0)), RADAR_RADIUS_OUTDOORS, &mut out));
    assert_eq!(w.selected, None, "not yet in the visible table");

    // The client's own 1 Hz rebuild.
    w.use_time(ServerTime(0.0), &mut out, &mut req);
    w.use_time(ServerTime(2.0), &mut out, &mut req);
    assert!(w.tables.visible.contains(&ATTACKER), "the sweep ran");
    assert!(w.select_last_attacker(Some((1.0, 1.0)), RADAR_RADIUS_OUTDOORS, &mut out));
    assert_eq!(w.selected, Some(ATTACKER));
}

/// **The quality number, pinned as a literal.**
///
/// Every other test here writes the last attacker through [`LAST_ATTACKER_IID`] and reads it back
/// through the same symbol, which cannot detect a wrong constant. Measured: changing the
/// constant to `0x0C` **survived** the whole of the rest of this file.
///
/// So this one writes `0x0B` as a number. The action handler pushes `0xB` immediately before the
/// instance-id read, and the combat system's auto-target reads the same one.
#[test]
fn the_last_attacker_is_instance_id_quality_0x0b() {
    assert_eq!(LAST_ATTACKER_IID, 0x0B);

    let mut w = world();
    make_visible(&mut w, ATTACKER);
    w.player_qualities_mut()
        .expect("the player description")
        .set(StatKey::new(StatType::Iid, 0x0B), StatValue::Iid(ATTACKER));
    let mut out = RecordingSink::default();
    assert!(w.select_last_attacker(Some((1.0, 1.0)), RADAR_RADIUS_OUTDOORS, &mut out));
    assert_eq!(
        w.selected,
        Some(ATTACKER),
        "quality 0x0B is the one the action reads"
    );

    // ...and the neighbours are not it, so this is a statement about 0x0B rather than about
    // "some quality was set".
    for other in [0x0A_u32, 0x0C] {
        let mut w = world();
        make_visible(&mut w, ATTACKER);
        w.player_qualities_mut()
            .expect("the player description")
            .set(StatKey::new(StatType::Iid, other), StatValue::Iid(ATTACKER));
        assert!(w.select_last_attacker(Some((1.0, 1.0)), RADAR_RADIUS_OUTDOORS, &mut out));
        assert_eq!(
            w.selected, None,
            "instance-id quality {other:#X} is not the last attacker"
        );
    }
}

/// `ObjectRangeGeometry` for the driver below: a fixed distance per object, which is what
/// the weenie's objects-in-range walk asks its caller for. `None` — "no physics body" —
/// is the client's out-of-range answer.
#[derive(Default)]
struct Distances(std::collections::BTreeMap<u32, f32>);

impl dereth_client_model::range::ObjectRangeGeometry for Distances {
    fn distance(&self, object: ObjectId, _player: ObjectId, _r: bool, _z: bool) -> Option<f32> {
        self.0.get(&object.0).copied()
    }
}

/// One object-range check pass, with the radar radius's
/// answer chosen by the caller — which is the whole point: the registrant re-reads the radius on
/// every run, so a run taken at a *different* radius is the cheapest proof that the run happened.
fn tick_at(w: &mut World, g: &Distances, now: f64, radar_radius: f32) {
    let mut out = RecordingSink::default();
    let mut req = dereth_client_model::RecordingRequests::default();
    w.calculate_object_range_checks(ServerTime(now), g, radar_radius, &mut out, &mut req);
}

/// The selection range watch as it stands, or a panic naming the absence.
fn armed(w: &World) -> dereth_client_model::range::ObjectRangeInfo {
    w.object_range_checks
        .live()
        .find(|e| e.handler == dereth_client_model::range::RangeHandler::Selection)
        .copied()
        .expect("the selection watch is armed")
}

/// The auto target re press does not re arm the watch because it passes force zero.
#[test]
fn the_auto_target_re_press_does_not_re_arm_the_watch_because_it_passes_force_zero() {
    let mut w = world();
    make_visible(&mut w, ATTACKER);
    set_last_attacker(&mut w, ATTACKER);
    let mut g = Distances::default();
    g.at(ATTACKER, 10.0);
    let mut out = RecordingSink::default();

    // Press once, outdoors. This one IS an edge, so the watch is armed at 75 m either way.
    assert!(w.select_last_attacker(Some((1.0, 1.0)), RADAR_RADIUS_OUTDOORS, &mut out));
    assert_eq!(w.selected, Some(ATTACKER));
    tick_at(&mut w, &g, 0.0, RADAR_RADIUS_OUTDOORS);
    let before = armed(&w);
    assert!(
        (before.range - f64::from(RADAR_RADIUS_OUTDOORS)).abs() < 1e-9,
        "the first press armed the watch at the outdoor radius, got {}",
        before.range
    );
    let notices_before = out.0.len();

    // Walk indoors and press the same key on the same attacker. the radar radius now answers
    // 25.0, and the only thing that can carry that number into the watch is a re-registration.
    assert!(w.select_last_attacker(Some((1.0, 1.0)), RADAR_RADIUS_INDOORS, &mut out));
    tick_at(&mut w, &g, 0.5, RADAR_RADIUS_INDOORS);
    let after = armed(&w);

    assert!(
        (after.range - f64::from(RADAR_RADIUS_OUTDOORS)).abs() < 1e-9,
        "force = 0 took the unchanged-selection early return, so range registration was \
         never reached and the watch still holds the radius the FIRST press read: expected {}, \
         got {}. A {} here is the forced path running — see this test's doc comment and row .",
        RADAR_RADIUS_OUTDOORS,
        after.range,
        RADAR_RADIUS_INDOORS
    );
    assert!(
        (after.next_update - before.next_update).abs() < 1e-9,
        "and the poll clock did not restart: {} -> {}",
        before.next_update,
        after.next_update
    );
    assert_eq!(
        out.0.len(),
        notices_before,
        "the re-press is not an edge, so it raises nothing"
    );
    assert_eq!(
        w.selected,
        Some(ATTACKER),
        "and the selection itself did not move"
    );

    // ---- the positive control ------------------------------------------------------------------
    // Everything above is an assertion that something did NOT happen, and a fixture in which the
    // re-arm is impossible would satisfy all of it. The positive control uses the same world, the
    // same indoor radius, one frame later, with `force = 1` this time. If this does not move the
    // radius the setup could never have shown a re-arm and the assertions above measured nothing.
    w.set_selected_object(Some(ATTACKER), true, &mut out);
    tick_at(&mut w, &g, 1.0, RADAR_RADIUS_INDOORS);
    let forced = armed(&w);
    assert!(
        (forced.range - f64::from(RADAR_RADIUS_INDOORS)).abs() < 1e-9,
        "control: `force = 1` DOES reach and re-arm at the indoor \
         radius, so this fixture can observe a re-arm and the assertion above is a measurement \
         rather than a silence: expected {}, got {}",
        RADAR_RADIUS_INDOORS,
        forced.range
    );
}

impl Distances {
    fn at(&mut self, id: ObjectId, d: f32) {
        self.0.insert(id.0, d);
    }
}
