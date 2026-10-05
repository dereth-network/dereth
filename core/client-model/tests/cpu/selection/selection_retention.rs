//! Forcing an unchanged selection raises nothing; the viewcone id follows the registrant; selection
//! and prev-selected survive between presses and deselects; the direction flip needs the reference
//! gone; selection-range exit arms counted apart; latch up keeps and latch down drops the target.
//! Fixture: recorded messages and synthetic state or packets.

use dereth_client_model::range::{ObjectRangeGeometry, RangeHandler, RADAR_RADIUS_OUTDOORS};
use dereth_client_model::selection::{SelectionPhysics, SelectionType};
use dereth_client_model::{Notice, RecordingRequests, RecordingSink, World};
use dereth_primitives::{CellId, ObjectId, ServerTime};
use {dereth_rules::weenie::bitfield, dereth_rules::weenie::item_type};

use std::collections::BTreeMap;

// =================================================================================================
// 1. The headline: which retained field `select_next` reads
// =================================================================================================

/// Forcing an unchanged selection raises nothing here and raises nothing in retail.
#[test]
fn forcing_an_unchanged_selection_raises_nothing_here_and_raises_nothing_in_retail() {
    let mut f = Fix::new();
    let a = ObjectId(0x5000_0010);
    f.place(a, 1.8, 2.4, 0.0);

    let mut out = RecordingSink::default();
    f.w.set_selected_object(Some(a), false, &mut out);
    let after_first = out.0.len();
    assert!(after_first > 0, "the first selection is an edge");

    f.w.set_selected_object(Some(a), true, &mut out);
    assert_eq!(
        out.0.len(),
        after_first,
        "`force` on an unchanged selection: no notice"
    );
    assert_eq!(f.w.selected, Some(a));
    assert_eq!(f.w.prev_selected, None, "and neither retained field moves");
    assert_eq!(f.w.prev_selected_valid, None);
}

/// The viewcone check object id follows the registrant and a deselect does not clear it.
#[test]
fn the_viewcone_check_object_id_follows_the_registrant_and_a_deselect_does_not_clear_it() {
    let mut w = world();
    let mut out = RecordingSink::default();
    let mut g = Distances::default();
    g.at(TARGET, 10.0);

    assert_eq!(
        w.viewcone_check_object_id, None,
        "nothing selected, nothing to draw-test against"
    );

    w.set_selected_object(Some(TARGET), false, &mut out);
    assert_eq!(
        w.viewcone_check_object_id, None,
        "not until the registrant has run"
    );
    tick(&mut w, &g, 0.0);
    assert_eq!(
        w.viewcone_check_object_id,
        Some(TARGET),
        "the selected-object setter updates the viewcone-check id"
    );

    // A **non-zero** id with no game record in the client — the same arm reached
    // with a real id, which is the only way to tell that arm apart from the `current == 0` one.
    // Added after the mutation `return false` -> `return true` SURVIVED: the id filter above fired
    // first at every station this test had, so the branch was never exercised at all.
    let ghost = ObjectId(0x5000_0FFF);
    assert!(
        w.weenie(ghost).is_none(),
        "premise: the client has no weenie object for it"
    );
    w.set_selected_object(Some(ghost), false, &mut out);
    tick(&mut w, &g, 0.5);
    assert_eq!(w.selected, Some(ghost));
    assert_eq!(
        w.viewcone_check_object_id,
        Some(TARGET),
        "that arm is a plain `return`, not a `goto tail`, so the tail did not run"
    );

    // Deselect. The weenie lookup for id 0 finds nothing, so the client returns before the tail too.
    w.set_selected_object(None, false, &mut out);
    tick(&mut w, &g, 1.0);
    assert_eq!(w.selected, None);
    assert_eq!(
        w.viewcone_check_object_id,
        Some(TARGET),
        "the missing-weenie arm is a plain `return`, so the stale id survives — transcribed, \
         and harmless because the range exit reads the latch only after the id comparison"
    );
}

// =================================================================================================
// 3. Retention, asserted over a SEQUENCE
// =================================================================================================

const PLAYER: ObjectId = ObjectId(0x5000_0001);
const TARGET: ObjectId = ObjectId(0x8000_0003);

/// The `select_next` fixture — a world plus the geometry seam, kept apart because the closure may
/// not borrow the world it is handed to.
struct Fix {
    w: World,
    phys: BTreeMap<ObjectId, SelectionPhysics>,
}

impl Fix {
    fn new() -> Self {
        let mut w = World::new();
        let mut me = dereth_client_model::Weenie::new(PLAYER);
        me.valid = true;
        me.has_phys_obj = true;
        w.tables.weenies.insert(PLAYER, me);
        w.player = Some(PLAYER);
        Self {
            w,
            phys: BTreeMap::new(),
        }
    }

    /// Place a **monster** (so every one of these stations passes `SelectionType::Monster`) at a
    /// player-space offset, through the real `update_visible_object_list` sweep.
    fn place(&mut self, id: ObjectId, x: f32, y: f32, z: f32) {
        let mut it = dereth_client_model::Weenie::new(id);
        it.valid = true;
        it.has_phys_obj = true;
        it.pwd.obj_type |= item_type::CREATURE;
        it.pwd.bitfield |= bitfield::ATTACKABLE;
        it.pwd.radar_enum = Some(4); // ShowAlways
        self.w.tables.weenies.insert(id, it);
        self.w.tables.physics.insert(
            id,
            dereth_client_model::objects::PhysicsPresence {
                cell: Some(CellId(0x00A9_0100)),
                state: 0,
                parent: None,
                setup_id: 0,
            },
        );
        self.phys.insert(
            id,
            SelectionPhysics {
                player_space: (x, y, z),
                cloaked: false,
                reports_collisions_as_environment: false,
            },
        );
        self.w.update_visible_object_list();
        assert!(
            self.w.tables.visible.contains(&id),
            "the sweep did not make {id:?} visible"
        );
        assert!(
            self.w.object_is_attackable(id),
            "premise: {id:?} is a monster"
        );
    }

    /// The object table answering NULL — the object is gone from the object
    /// table, which is one of the three conditions the player-space conversion returns 0 for.
    fn lose_the_physics_of(&mut self, id: ObjectId) {
        self.phys.remove(&id);
    }

    fn next(&mut self, kind: SelectionType) -> Option<ObjectId> {
        let phys = self.phys.clone();
        let mut out = RecordingSink::default();
        self.w.select_next(
            false,
            false,
            kind,
            false,
            &move |id| phys.get(&id).copied(),
            RADAR_RADIUS_OUTDOORS,
            &mut out,
        );
        self.w.selected
    }
}

/// Four monsters at 3, 6, 9 and 12 m on a non-cardinal 3/4/5 heading.
fn four_monsters() -> (Fix, [ObjectId; 4]) {
    let ids = [
        ObjectId(0x5000_0010),
        ObjectId(0x5000_0011),
        ObjectId(0x5000_0012),
        ObjectId(0x5000_0013),
    ];
    let mut f = Fix::new();
    for (i, id) in ids.into_iter().enumerate() {
        let k = (i + 1) as f32;
        f.place(id, 1.8 * k, 2.4 * k, 0.0);
    }
    (f, ids)
}

/// The selection survives the acts between presses and the cycle advances.
#[test]
fn the_selection_survives_the_acts_between_presses_and_the_cycle_advances() {
    let (mut f, ids) = four_monsters();

    // Press 1, from an empty selection. The direction inversion makes this the farthest,
    // which is faithful and is the state the rest of this
    // file is about.
    assert_eq!(
        f.next(SelectionType::Monster),
        Some(ids[3]),
        "empty selection -> farthest"
    );

    // Now select the nearest by hand, as a click does, and walk outward.
    let mut out = RecordingSink::default();
    f.w.set_selected_object(Some(ids[0]), false, &mut out);

    for (press, expected) in [(1_u32, ids[1]), (2, ids[2]), (3, ids[3])] {
        // The acts between presses.
        f.w.update_visible_object_list();
        let mut req = RecordingRequests::default();
        f.w.use_time(ServerTime(f64::from(press) * 2.0), &mut out, &mut req);
        assert_eq!(
            f.w.selected,
            Some(if press == 1 {
                ids[0]
            } else {
                ids[press as usize - 1]
            }),
            "press {press}: the selection must survive the frame that precedes it"
        );
        assert_eq!(
            f.next(SelectionType::Monster),
            Some(expected),
            "press {press}: the cycle advances one object outward"
        );
    }
}

/// Behaviour: selection.retention.a-deselect-keeps-the-reference-and-the-cycle-continues-from-it
/// A deselect leaves the reference behind and the cycle continues from it.
#[test]
fn a_deselect_leaves_the_reference_behind_and_the_cycle_continues_from_it() {
    let (mut f, ids) = four_monsters();
    let mut out = RecordingSink::default();

    f.w.set_selected_object(Some(ids[1]), false, &mut out); // the 6 m one
    f.w.set_selected_object(None, false, &mut out); // walk-away, or a deselect
    assert_eq!(f.w.selected, None);
    assert_eq!(
        f.w.prev_selected,
        Some(ids[1]),
        "the previous selection took the outgoing id"
    );
    assert_eq!(f.w.prev_selected_valid, Some(ids[1]));

    let got = f.next(SelectionType::Monster);
    assert_eq!(
        got,
        Some(ids[2]),
        "the walk resumes outward from the 6 m reference"
    );
    assert_ne!(
        got,
        Some(ids[3]),
        "not the farthest — the reference was not lost"
    );
    assert_ne!(
        got,
        Some(ids[0]),
        "not the nearest — the reference was not zero"
    );
}

/// The direction flip needs the reference object gone not merely deselected.
#[test]
fn the_direction_flip_needs_the_reference_object_gone_not_merely_deselected() {
    let (mut f, ids) = four_monsters();
    let mut out = RecordingSink::default();

    f.w.set_selected_object(Some(ids[1]), false, &mut out);
    f.w.set_selected_object(None, false, &mut out);
    assert_eq!(
        f.next(SelectionType::Monster),
        Some(ids[2]),
        "reference alive: next outward"
    );

    // Same state, except the object registry no longer holds the reference.
    let (mut f, ids) = four_monsters();
    let mut out = RecordingSink::default();
    f.w.set_selected_object(Some(ids[1]), false, &mut out);
    f.w.set_selected_object(None, false, &mut out);
    assert_eq!(
        f.w.prev_selected,
        Some(ids[1]),
        "the id is retained either way"
    );
    f.lose_the_physics_of(ids[1]);
    assert_eq!(
        f.next(SelectionType::Monster),
        Some(ids[3]),
        "reference gone: the search flips direction and walks farthest-first"
    );
}

#[derive(Default)]
struct Distances(BTreeMap<u32, f32>);

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

fn world() -> World {
    let mut w = World::new();
    for id in [PLAYER, TARGET] {
        let mut it = dereth_client_model::Weenie::new(id);
        it.valid = true;
        w.tables.weenies.insert(id, it);
    }
    w.player = Some(PLAYER);
    w.tables.inventories.insert(
        PLAYER,
        dereth_client_model::objects::ObjectInventory::new(PLAYER),
    );
    w
}

fn tick(
    w: &mut World,
    g: &Distances,
    now: f64,
) -> (Vec<Notice>, dereth_client_model::range::RangeCheckStats) {
    tick_at(w, g, now, RADAR_RADIUS_OUTDOORS)
}

fn tick_at(
    w: &mut World,
    g: &Distances,
    now: f64,
    radar_radius: f32,
) -> (Vec<Notice>, dereth_client_model::range::RangeCheckStats) {
    let mut out = RecordingSink::default();
    let mut req = RecordingRequests::default();
    let stats =
        w.calculate_object_range_checks(ServerTime(now), g, radar_radius, &mut out, &mut req);
    (out.0, stats)
}

/// The three arms of the selection range exit are counted apart.
#[test]
fn the_three_arms_of_the_selection_range_exit_are_counted_apart() {
    // (a) `is_selected_object_in_view()` false — the arm a never-drawn selection takes.
    let mut w = world();
    let mut out = RecordingSink::default();
    w.set_selected_object(Some(TARGET), false, &mut out);
    let mut g = Distances::default();
    g.at(TARGET, 10.0);
    tick(&mut w, &g, 0.0);
    g.at(TARGET, 75.1);
    let (_, stats) = tick(&mut w, &g, 3.0);
    assert_eq!(stats.exits, 1);
    assert_eq!(stats.selection_rearms, 0);
    assert_eq!(stats.selection_clears, 1, "the selection was DROPPED");
    assert_eq!(w.selected, None);

    // (b) The latch set — retail's usual arm.
    let mut w = world();
    let mut out = RecordingSink::default();
    w.set_selected_object(Some(TARGET), false, &mut out);
    w.selected_object_in_view = true;
    let mut g = Distances::default();
    g.at(TARGET, 10.0);
    tick(&mut w, &g, 0.0);
    g.at(TARGET, 75.1);
    let (_, stats) = tick(&mut w, &g, 3.0);
    assert_eq!(stats.exits, 1);
    assert_eq!(stats.selection_rearms, 1);
    assert_eq!(stats.selection_clears, 0, "a re-arm is not a drop");
    assert_eq!(w.selected, Some(TARGET));

    // (c) A stale registration: the watch names an object that is no longer selected. Neither
    // arm runs, and neither counter moves.
    let mut w = world();
    let mut out = RecordingSink::default();
    w.set_selected_object(Some(TARGET), false, &mut out);
    let mut g = Distances::default();
    g.at(TARGET, 10.0);
    tick(&mut w, &g, 0.0);
    w.set_selected_object(None, false, &mut out); // the id moves on
                                                  // Let the edge detector retire the real watch first, so what is registered below is a node
                                                  // the client itself would have left behind rather than the live one.
    tick(&mut w, &g, 2.0);
    assert!(!w
        .object_range_checks
        .is_watching(RangeHandler::Selection, TARGET));
    w.object_range_checks.register(
        RangeHandler::Selection,
        TARGET,
        f64::from(RADAR_RADIUS_OUTDOORS),
        true,
        true,
        dereth_client_model::range::POLL_INTERVAL,
        0.0,
        3.0,
    );
    g.at(TARGET, 75.1);
    let (_, stats) = tick(&mut w, &g, 5.0);
    assert_eq!(stats.exits, 1, "the node did fire");
    assert_eq!(stats.selection_rearms, 0);
    assert_eq!(
        stats.selection_clears, 0,
        "a stale registration is not a dropped selection"
    );
}

/// The same walk keeps the target with the latch up and drops it with the latch down.
#[test]
fn the_same_walk_keeps_the_target_with_the_latch_up_and_drops_it_with_the_latch_down() {
    let run = |latched: bool| {
        let mut w = world();
        let mut out = RecordingSink::default();
        w.set_selected_object(Some(TARGET), false, &mut out);
        w.selected_object_in_view = latched;
        let mut g = Distances::default();
        g.at(TARGET, 10.0);
        tick(&mut w, &g, 0.0);

        let (mut exits, mut clears, mut rearms) = (0, 0, 0);
        // Walk out, back in, and out again — three edges over ten polls.
        for step in 1..=10 {
            let now = f64::from(step) * 1.5;
            g.at(TARGET, if (3..=5).contains(&step) { 10.0 } else { 90.0 });
            let (_, s) = tick(&mut w, &g, now);
            exits += s.exits;
            clears += s.selection_clears;
            rearms += s.selection_rearms;
        }
        (w.selected, w.prev_selected, exits, clears, rearms)
    };

    let (selected, prev, exits, clears, rearms) = run(false);
    assert_eq!(selected, None, "latch down: the target is gone");
    assert_eq!(
        prev,
        Some(TARGET),
        "but the reference is retained, so the cycle still continues"
    );
    assert_eq!(
        (exits, clears, rearms),
        (1, 1, 0),
        "one exit, and it dropped the selection"
    );

    let (selected, prev, exits, clears, rearms) = run(true);
    assert_eq!(
        selected,
        Some(TARGET),
        "latch up: the target survives the walk"
    );
    assert_eq!(
        prev, None,
        "and nothing was ever pushed into the previous selection"
    );
    assert!(
        exits >= 2,
        "the watch re-arms and fires again, so there are more edges: {exits}"
    );
    assert_eq!(clears, 0);
    assert_eq!(rearms, exits, "every one of them re-armed");
}
