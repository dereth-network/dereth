//! Farther ordering with an unsigned id tiebreak and NaN handling; traversal-order independence;
//! 2-D and weighted-Z distance; inclusive range boundary; next/previous/wrap and empty-selection
//! arms; reference fallback; filters (hidden, removed, cloaked, unplaced, player, wielded,
//! item/player/monster/compass/corpse).
//! Fixture: recorded messages and synthetic state or packets.

use dereth_client_model::combat::CombatMode;
use dereth_client_model::range::{RADAR_RADIUS_INDOORS, RADAR_RADIUS_OUTDOORS};
use dereth_client_model::selection::{
    farther, get_2d_distance, weighted_z_distance, within_radar_range, SelectionPhysics,
    SelectionType, CLOAKED_PS, COMPASS_ALWAYS, REPORT_COLLISIONS_AS_ENVIRONMENT_PS,
    SELECT_NEXT_SENTINEL,
};
use dereth_client_model::weenie::{bitfield, item_type};
use dereth_client_model::{Notice, RecordingSink, World};
use dereth_primitives::{CellId, ObjectId};

use std::collections::BTreeMap;

const PLAYER: ObjectId = ObjectId(0x5000_0001);

// =================================================================================================
// Fixture
// =================================================================================================

/// A world plus the geometry seam `select_next` asks, kept apart because the closure may not
/// borrow the world it is passed to.
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

    /// Put an object in the world at a player-space offset and drive
    /// the object maintenance's visible-object list update, so that "visible" here means what it
    /// means in the running client rather than what a hand-written table would mean.
    fn place(&mut self, id: ObjectId, x: f32, y: f32, z: f32) {
        let mut it = dereth_client_model::Weenie::new(id);
        it.valid = true;
        it.has_phys_obj = true;
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
    }

    /// Make `id` pass `SelectionType::Monster`: a creature with `BF_ATTACKABLE` and a
    /// `RADAR_ENUM` the radar shows.
    fn make_monster(&mut self, id: ObjectId) {
        let w = self.w.tables.weenies.get_mut(id).expect("placed");
        w.pwd.obj_type |= item_type::CREATURE;
        w.pwd.bitfield |= bitfield::ATTACKABLE;
        w.pwd.radar_enum = Some(4); // ShowAlways
        assert!(
            self.w.object_is_attackable(id),
            "the premise: it is attackable"
        );
    }

    fn weenie_mut(&mut self, id: ObjectId) -> &mut dereth_client_model::Weenie {
        self.w.tables.weenies.get_mut(id).expect("placed")
    }

    fn select_next(
        &mut self,
        closer: bool,
        ignore_current: bool,
        kind: SelectionType,
        exclude_own_wielded: bool,
        radar_radius: f32,
    ) -> Vec<Notice> {
        let phys = self.phys.clone();
        let mut out = RecordingSink::default();
        self.w.select_next(
            closer,
            ignore_current,
            kind,
            exclude_own_wielded,
            &move |id| phys.get(&id).copied(),
            radar_radius,
            &mut out,
        );
        out.0
    }

    /// The common case: outward ("Next"), against the live selection, outdoors.
    fn next(&mut self, kind: SelectionType) -> Option<ObjectId> {
        self.select_next(false, false, kind, false, RADAR_RADIUS_OUTDOORS);
        self.w.selected
    }

    /// "Previous" — inward.
    fn previous(&mut self, kind: SelectionType) -> Option<ObjectId> {
        self.select_next(true, false, kind, false, RADAR_RADIUS_OUTDOORS);
        self.w.selected
    }

    /// `OnAction`'s "Closest" arms: `(closer = true, ignore_current = true)`.
    fn closest(&mut self, kind: SelectionType) -> Option<ObjectId> {
        self.select_next(true, true, kind, false, RADAR_RADIUS_OUTDOORS);
        self.w.selected
    }
}

/// A world with three monsters on a **non-cardinal** heading at 3, 6 and 9 metres, so no fixture
/// here sits on an axis.
fn three_monsters() -> (Fix, ObjectId, ObjectId, ObjectId) {
    let (a, b, c) = (
        ObjectId(0x5000_0010),
        ObjectId(0x5000_0011),
        ObjectId(0x5000_0012),
    );
    let mut f = Fix::new();
    // 3/4/5 scaled: (1.8, 2.4) is 3 m, (3.6, 4.8) is 6 m, (5.4, 7.2) is 9 m — 53.13° off axis.
    f.place(a, 1.8, 2.4, 0.0);
    f.place(b, 3.6, 4.8, 0.0);
    f.place(c, 5.4, 7.2, 0.0);
    for id in [a, b, c] {
        f.make_monster(id);
    }
    (f, a, b, c)
}

// =================================================================================================
// 1. The ordering — and the two distance helpers
// =================================================================================================

/// `a > b`, and nothing else, when the distances differ.
#[test]
fn farther_is_greater_distance() {
    assert!(farther((9.0, ObjectId(1)), (3.0, ObjectId(2))));
    assert!(!farther((3.0, ObjectId(2)), (9.0, ObjectId(1))));
}

/// On an exact tie the id decides, and the unsigned branch makes that comparison unsigned.
///
/// Retail object ids are `0x5xxxxxxx` and above, so a signed transcription would invert the
/// tiebreak for every dynamic object in the world. The pair below is chosen so that the two
/// spellings disagree: `0x8000_0001` is negative as an `i32` and larger as a `u32`.
#[test]
fn the_tiebreak_is_the_object_id_and_it_is_unsigned() {
    let low = ObjectId(0x0000_0002);
    let high = ObjectId(0x8000_0001);
    assert!(farther((5.0, high), (5.0, low)), "unsigned: 0x80000001 > 2");
    assert!(!farther((5.0, low), (5.0, high)));
    assert!(
        (high.0 as i32) < (low.0 as i32),
        "the premise: a signed compare would answer the other way round"
    );
    // Equal in both halves is not farther — the unsigned compare takes the FALSE path on equal.
    assert!(!farther((5.0, low), (5.0, low)));
}

/// Three separate floating-point comparisons refuse an
/// **unordered** result, so a NaN reaches the false path.
#[test]
fn a_nan_distance_is_never_farther_in_either_direction() {
    let nan = f64::NAN;
    assert!(!farther((nan, ObjectId(9)), (5.0, ObjectId(1))));
    assert!(!farther((5.0, ObjectId(9)), (nan, ObjectId(1))));
    assert!(!farther((nan, ObjectId(9)), (nan, ObjectId(1))));
}

/// **The claim the tiebreak exists to support**, tested rather than described.
///
/// Selection cycling walks an intrusive hash table, whose bucket order is a function of the ids and the
/// table size and is nothing this crate reproduces. The scan is nevertheless deterministic because
/// `Farther` is a **total** order: the fold's answer is the same for every permutation of the
/// candidate set. All 24 permutations of four keys, two of which are an exact distance tie, are
/// folded both ways.
#[test]
fn the_scan_is_independent_of_the_traversal_order() {
    let keys = [
        (3.0_f64, ObjectId(0x5000_0007)),
        (9.0, ObjectId(0x5000_0003)),
        (6.0, ObjectId(0x5000_0009)),
        // The tie: same distance as the one above, different id.
        (6.0, ObjectId(0x5000_0002)),
    ];
    let reference = (4.5_f64, ObjectId(0));

    let mut outward: Option<(f64, ObjectId)> = None;
    let mut inward: Option<(f64, ObjectId)> = None;
    let mut perms = 0;
    for p in permutations(&keys) {
        // Branch A (`closer == false`): minimum strictly greater than the reference.
        let mut best = (SELECT_NEXT_SENTINEL, ObjectId(0));
        for k in &p {
            if !farther(*k, best) && farther(*k, reference) {
                best = *k;
            }
        }
        // Branch B (`closer == true`): maximum not greater than the reference.
        let mut worst = (0.0_f64, ObjectId(0));
        for k in &p {
            if farther(*k, worst) && !farther(*k, reference) {
                worst = *k;
            }
        }
        assert_eq!(
            *outward.get_or_insert(best),
            best,
            "outward answer moved with the order"
        );
        assert_eq!(
            *inward.get_or_insert(worst),
            worst,
            "inward answer moved with the order"
        );
        perms += 1;
    }
    assert_eq!(perms, 24, "every permutation was folded");
    // And the answers are the ones the order dictates, so the assertion above is not vacuous.
    assert_eq!(
        outward,
        Some((6.0, ObjectId(0x5000_0002))),
        "the tie went to the LOWER id"
    );
    assert_eq!(inward, Some((3.0, ObjectId(0x5000_0007))));
}

fn permutations(xs: &[(f64, ObjectId); 4]) -> Vec<Vec<(f64, ObjectId)>> {
    let mut out = Vec::new();
    for a in 0..4 {
        for b in 0..4 {
            for c in 0..4 {
                for d in 0..4 {
                    let idx = [a, b, c, d];
                    let mut seen = [false; 4];
                    if idx.iter().any(|&i| std::mem::replace(&mut seen[i], true)) {
                        continue;
                    }
                    out.push(idx.iter().map(|&i| xs[i]).collect());
                }
            }
        }
    }
    out
}

/// The 2-D distance is `sqrt(x² + y²)` with **no z**, and it is exact on a Pythagorean triple.
#[test]
fn get_2d_distance_is_the_xy_hypotenuse() {
    assert_eq!(get_2d_distance(45.0, 60.0), 75.0);
    assert_eq!(get_2d_distance(15.0, 20.0), 25.0);
    assert_eq!(
        get_2d_distance(-45.0, -60.0),
        75.0,
        "the squares lose the sign"
    );
    assert_eq!(get_2d_distance(0.0, 0.0), 0.0);
}

/// Weighted vertical distance is `1.2 × |z|` with the double 1.2, not `1.2f32`.
///
/// This asserts that the transcription uses the `f64` value, which a `1.2f32` version fails by
/// 4.8e-8 at z = 1.
#[test]
fn the_z_weight_is_a_double_and_the_axis_is_weighted_up() {
    assert_eq!(weighted_z_distance(1.0), 1.2_f64);
    assert_eq!(
        weighted_z_distance(-1.0),
        1.2_f64,
        "the compare-and-negate takes |z|"
    );
    assert_eq!(weighted_z_distance(0.0), 0.0);
    assert_ne!(
        weighted_z_distance(1.0),
        f64::from(1.2_f32),
        "1.2f32 is a different number and would be wrong in the last bits of every key"
    );
    // Weighted UP: a metre of height costs more than a metre of ground, which is what makes the
    // cycle prefer targets on your own level.
    assert!(weighted_z_distance(1.0) > get_2d_distance(1.0, 0.0));
}

// =================================================================================================
// 2. The boundary — INCLUSIVE, and its neighbour is not
// =================================================================================================

/// An object at **exactly** the radar radius is a candidate.
///
/// `(45, 60)` is 75 and `(15, 20)` is 25 — Pythagorean triples, so `d == r` holds bit-exactly
/// rather than to within a tolerance, and both are 53.13° off axis so neither station is a
/// cardinal one. The exactness is asserted as a premise, because a station that quietly moved off
/// the boundary would make this test a statement about a tolerance.
#[test]
fn the_boundary_is_inclusive_at_both_ranges() {
    for (x, y, r) in [
        (45.0_f32, 60.0_f32, RADAR_RADIUS_OUTDOORS),
        (15.0, 20.0, RADAR_RADIUS_INDOORS),
    ] {
        assert_eq!(
            get_2d_distance(x, y),
            f64::from(r),
            "the premise: the station is exactly on r"
        );
        let id = ObjectId(0x5000_0021);
        let mut f = Fix::new();
        f.place(id, x, y, 0.0);
        f.make_monster(id);
        f.select_next(false, false, SelectionType::Monster, false, r);
        assert_eq!(
            f.w.selected,
            Some(id),
            "only strictly-less is refused: d == r is IN range"
        );
    }
}

/// The neighbouring consumer refuses the station this one accepts.
#[test]
fn the_neighbouring_consumer_refuses_the_station_this_one_accepts() {
    assert!(
        !within_radar_range(45.0, 60.0, RADAR_RADIUS_OUTDOORS),
        "the radar-range test refuses equality too, so the equal case is refused there"
    );
    let id = ObjectId(0x5000_0022);
    let mut f = Fix::new();
    f.place(id, 45.0, 60.0, 0.0);
    f.make_monster(id);
    assert_eq!(
        f.next(SelectionType::Monster),
        Some(id),
        "the select-next range check accepts it"
    );
}

/// Just outside is out, at both ranges — so the test above is a boundary and not an "everything
/// passes".
#[test]
fn one_step_past_the_radius_is_out_of_range() {
    for (x, y, r) in [
        (45.01_f32, 60.01_f32, RADAR_RADIUS_OUTDOORS),
        (15.01, 20.01, RADAR_RADIUS_INDOORS),
    ] {
        assert!(get_2d_distance(x, y) > f64::from(r), "the premise");
        let id = ObjectId(0x5000_0023);
        let mut f = Fix::new();
        f.place(id, x, y, 0.0);
        f.make_monster(id);
        f.select_next(false, false, SelectionType::Monster, false, r);
        assert_eq!(f.w.selected, None);
    }
}

/// Behaviour: selection.tab.the-range-is-inclusive-and-height-orders-but-never-gates
/// The range gate reads the 2-D distance before the z weight is added. So z
/// orders the candidates and never decides their eligibility: an object 74 m away and 200 m below
/// you is still selectable, and its weighted key is 314 m.
#[test]
fn z_orders_the_candidates_and_never_gates_them() {
    let id = ObjectId(0x5000_0024);
    let mut f = Fix::new();
    // 74 m out on the 3/4/5 heading, 200 m down.
    f.place(id, 44.4, 59.2, -200.0);
    f.make_monster(id);
    assert_eq!(f.next(SelectionType::Monster), Some(id));
    let key = get_2d_distance(44.4, 59.2) + weighted_z_distance(-200.0);
    assert!(
        key > f64::from(RADAR_RADIUS_OUTDOORS),
        "and its ordering key is well outside 75"
    );

    // The same object with the same z but past 75 m in the plane is refused, so the assertion
    // above is about the 2-D test and not about z being ignored everywhere.
    let mut g = Fix::new();
    g.place(id, 45.6, 60.8, -200.0);
    g.make_monster(id);
    assert!(
        get_2d_distance(45.6, 60.8) > f64::from(RADAR_RADIUS_OUTDOORS),
        "the premise"
    );
    assert_eq!(g.next(SelectionType::Monster), None);
}

/// Two objects at the same ground distance and different heights sort by the weighted z, and the
/// nearer-in-z one is "closer" for the cycle.
#[test]
fn the_z_weight_decides_the_order_between_two_objects_at_the_same_ground_distance() {
    let (flat, high) = (ObjectId(0x5000_0031), ObjectId(0x5000_0032));
    let mut f = Fix::new();
    f.place(flat, 1.8, 2.4, 0.0);
    f.place(high, 1.8, 2.4, 4.0);
    f.make_monster(flat);
    f.make_monster(high);
    assert_eq!(f.closest(SelectionType::Monster), Some(flat));
    assert_eq!(
        f.next(SelectionType::Monster),
        Some(high),
        "4 m up sorts 4.8 m further out"
    );
}

// =================================================================================================
// 3. Direction, the reference, and the flip that produces the wrap-around
// =================================================================================================

/// Behaviour: selection.tab.next-steps-outward-previous-inward-and-the-cycle-wraps
#[test]
fn next_steps_outward_and_previous_steps_inward() {
    let (mut f, a, b, c) = three_monsters();
    assert_eq!(f.closest(SelectionType::Monster), Some(a));
    assert_eq!(f.next(SelectionType::Monster), Some(b));
    assert_eq!(f.next(SelectionType::Monster), Some(c));
    // Off the end: with no candidate beyond `c`, nothing is selected and the selection stands.
    assert_eq!(
        f.next(SelectionType::Monster),
        Some(c),
        "the scan found nothing to take"
    );
    assert_eq!(f.previous(SelectionType::Monster), Some(b));
    assert_eq!(f.previous(SelectionType::Monster), Some(a));
    assert_eq!(
        f.previous(SelectionType::Monster),
        Some(a),
        "and nothing inside `a`"
    );
}

/// `OnAction`'s retry: **only if the selection did not move**, call again with `ignore_current` and
/// `closer` already inverted — which the function's own flip inverts back. That is the whole
/// wrap-around, and both ends are driven here through the same retry sequence.
#[test]
fn the_retry_is_what_wraps_the_cycle_at_both_ends() {
    let (mut f, a, _b, c) = three_monsters();

    // Sitting on the farthest, "Next" finds nothing...
    f.w.selected = Some(c);
    assert_eq!(f.next(SelectionType::Monster), Some(c));
    // ...so `SelectionNextMonster` retries with `(closer = true, ignore_current = true)`.
    f.select_next(
        true,
        true,
        SelectionType::Monster,
        false,
        RADAR_RADIUS_OUTDOORS,
    );
    assert_eq!(f.w.selected, Some(a), "wrapped to the nearest");

    // And the other end: sitting on the nearest, "Previous" retries with `(false, true)`.
    assert_eq!(f.previous(SelectionType::Monster), Some(a));
    f.select_next(
        false,
        true,
        SelectionType::Monster,
        false,
        RADAR_RADIUS_OUTDOORS,
    );
    assert_eq!(f.w.selected, Some(c), "wrapped to the farthest");
}

/// **The surprising half of the flip, pinned because it is faithful and reads like a defect.**
///
/// The selection search inverts `closer` whenever there is no usable reference object — which
/// includes *nothing selected at all*, not only the `ignore_current` retry. So
/// `SelectionNextMonster` pressed with an empty selection asks for the outward direction, gets the
/// inward one against a reference of 73728.0, and selects the **farthest** monster in radar range.
/// `SelectionPreviousMonster` on an empty selection selects the nearest. This is the reason
/// "Closest" is a separate action rather than an alias for "Next".
#[test]
fn with_nothing_selected_next_takes_the_farthest_and_previous_takes_the_nearest() {
    let (mut f, a, _b, c) = three_monsters();
    assert_eq!(f.w.selected, None, "the premise: nothing is selected");
    assert_eq!(
        f.next(SelectionType::Monster),
        Some(c),
        "outward from nothing is the FARTHEST"
    );

    let (mut g, a2, _, _) = three_monsters();
    assert_eq!(
        g.previous(SelectionType::Monster),
        Some(a2),
        "inward from nothing is the nearest"
    );
    assert_eq!(a, a2);
}

/// The reference is the selection, or **the previous selection** when nothing is selected
/// — and the two give different answers, which is what makes this a test
/// of the fallback rather than of the flip.
#[test]
fn the_reference_falls_back_to_the_previous_selection() {
    let (mut f, a, b, c) = three_monsters();
    // Select `b`, then clear the selection the way selecting `None` does, which moves
    // `b` into `prev_selected`.
    f.w.selected = Some(b);
    let mut out = RecordingSink::default();
    f.w.set_selected_object(None, false, &mut out);
    assert_eq!(f.w.selected, None);
    assert_eq!(
        f.w.prev_selected,
        Some(b),
        "the premise: the fallback has something to find"
    );

    // Outward from `b` is `c`. If the fallback were not read, the flip would fire and the answer
    // would be `c` as well — so the discriminating direction is inward, where the fallback gives
    // `a` and the flip would give `a` too... use the third station: outward from `b` is `c` and
    // outward from "nothing" (the flip) is also `c`. Inward from `b` is `a`; inward from nothing
    // is `a`. The two agree at the ends, so drive the middle: with the reference at `a`, outward
    // is `b`, while the flip would answer `c`.
    f.w.selected = None;
    f.w.prev_selected = Some(a);
    assert_eq!(
        f.next(SelectionType::Monster),
        Some(b),
        "the reference was `a`, not nothing"
    );
    assert_ne!(
        Some(c),
        f.w.selected,
        "the flip would have answered the farthest"
    );
}

/// the reference object itself is not a candidate unless `ignore_current` is set —
/// and with it set, it is. Both arms, because "nothing was selected" is satisfied by a broken
/// fixture as readily as by the gate.
#[test]
fn the_current_selection_is_a_candidate_only_when_ignore_current_is_set() {
    let id = ObjectId(0x5000_0041);
    let mut f = Fix::new();
    f.place(id, 1.8, 2.4, 0.0);
    f.make_monster(id);

    // With it selected and `ignore_current` clear, the one object in the world is excluded, so
    // nothing is found and no notice is raised.
    f.w.selected = Some(id);
    let notices = f.select_next(
        false,
        false,
        SelectionType::Monster,
        false,
        RADAR_RADIUS_OUTDOORS,
    );
    assert!(
        notices.is_empty(),
        "nothing was selected, so nothing changed"
    );

    // The "Closest" arms pass `ignore_current = true`, and then it is a candidate again.
    f.w.selected = None;
    assert_eq!(f.closest(SelectionType::Monster), Some(id));
}

/// The selection search passes `force = 0`, so re-selecting what is already selected raises no
/// `SelectionChanged`, and selecting something else raises exactly one.
#[test]
fn the_selection_is_set_with_force_zero() {
    let (mut f, a, b, _c) = three_monsters();
    let notices = f.select_next(
        true,
        true,
        SelectionType::Monster,
        false,
        RADAR_RADIUS_OUTDOORS,
    );
    assert_eq!(f.w.selected, Some(a));
    assert_eq!(notices.len(), 1);
    assert!(matches!(
        notices[0],
        Notice::SelectionChanged { previous: None, current: Some(x) } if x == a
    ));

    // The same "Closest" press again: the scan finds `a` and selecting `a` unforced takes the
    // unchanged-selection early return.
    let again = f.select_next(
        true,
        true,
        SelectionType::Monster,
        false,
        RADAR_RADIUS_OUTDOORS,
    );
    assert!(
        again.is_empty(),
        "force = 0 on an unchanged selection raises nothing"
    );

    let moved = f.select_next(
        false,
        false,
        SelectionType::Monster,
        false,
        RADAR_RADIUS_OUTDOORS,
    );
    assert_eq!(f.w.selected, Some(b));
    assert_eq!(moved.len(), 1);
}

/// The indoor range is a real gate on the cycle, not only on the radar: the same three monsters
/// at 30/60/90 m are all candidates outdoors and none is indoors.
#[test]
fn the_indoor_range_shrinks_the_cycle() {
    let (a, b, c) = (
        ObjectId(0x5000_0051),
        ObjectId(0x5000_0052),
        ObjectId(0x5000_0053),
    );
    let mut f = Fix::new();
    f.place(a, 18.0, 24.0, 0.0); // 30 m
    f.place(b, 36.0, 48.0, 0.0); // 60 m
    f.place(c, 54.0, 72.0, 0.0); // 90 m — outside 75 too
    for id in [a, b, c] {
        f.make_monster(id);
    }
    f.select_next(
        true,
        true,
        SelectionType::Monster,
        false,
        RADAR_RADIUS_OUTDOORS,
    );
    assert_eq!(f.w.selected, Some(a), "30 m is inside 75");
    f.select_next(
        false,
        false,
        SelectionType::Monster,
        false,
        RADAR_RADIUS_OUTDOORS,
    );
    assert_eq!(f.w.selected, Some(b), "60 m is inside 75");
    f.select_next(
        false,
        false,
        SelectionType::Monster,
        false,
        RADAR_RADIUS_OUTDOORS,
    );
    assert_eq!(f.w.selected, Some(b), "90 m is outside 75");

    let mut g = Fix::new();
    g.place(a, 18.0, 24.0, 0.0);
    g.make_monster(a);
    g.select_next(
        true,
        true,
        SelectionType::Monster,
        false,
        RADAR_RADIUS_INDOORS,
    );
    assert_eq!(g.w.selected, None, "30 m is outside 25");
}

// =================================================================================================
// 4. The gates, one at a time
// =================================================================================================

/// A ui hidden object is skipped and bit 31 is not the flag.
#[test]
fn a_ui_hidden_object_is_skipped_and_bit_31_is_not_the_flag() {
    let id = ObjectId(0x5000_0061);
    let mut f = Fix::new();
    f.place(id, 1.8, 2.4, 0.0);
    f.make_monster(id);
    assert_eq!(f.closest(SelectionType::Monster), Some(id), "the control");

    let mut g = Fix::new();
    g.place(id, 1.8, 2.4, 0.0);
    g.make_monster(id);
    g.weenie_mut(id).pwd.bitfield |= bitfield::UI_HIDDEN;
    assert_eq!(g.closest(SelectionType::Monster), None);

    let mut h = Fix::new();
    h.place(id, 1.8, 2.4, 0.0);
    h.make_monster(id);
    h.weenie_mut(id).pwd.bitfield |= 0x8000_0000;
    assert_eq!(
        h.closest(SelectionType::Monster),
        Some(id),
        "bit 31 is not the hidden flag"
    );
}

/// Objects being removed are ineligible.
#[test]
fn an_object_being_removed_is_skipped() {
    let id = ObjectId(0x5000_0062);
    let mut f = Fix::new();
    f.place(id, 1.8, 2.4, 0.0);
    f.make_monster(id);
    f.weenie_mut(id).being_removed = true;
    assert_eq!(f.closest(SelectionType::Monster), None);
    f.weenie_mut(id).being_removed = false;
    assert_eq!(f.closest(SelectionType::Monster), Some(id), "the control");
}

/// The cloaked physics-state bit (`0x100000`) makes an object ineligible.
#[test]
fn a_cloaked_object_is_skipped() {
    let id = ObjectId(0x5000_0063);
    let mut f = Fix::new();
    f.place(id, 1.8, 2.4, 0.0);
    f.make_monster(id);
    f.phys.get_mut(&id).expect("placed").cloaked = true;
    assert_eq!(f.closest(SelectionType::Monster), None);
    f.phys.get_mut(&id).expect("placed").cloaked = false;
    assert_eq!(f.closest(SelectionType::Monster), Some(id), "the control");
    assert_eq!(CLOAKED_PS, 0x0010_0000, "PhysicsState::CLOAKED_PS");
}

/// No physics object, or the player-space conversion refused. One `None`
/// from the seam, both gates, for the reason given at [`SelectionPhysics`].
#[test]
fn an_object_the_geometry_seam_cannot_place_is_skipped() {
    let id = ObjectId(0x5000_0064);
    let mut f = Fix::new();
    f.place(id, 1.8, 2.4, 0.0);
    f.make_monster(id);
    f.phys.remove(&id);
    assert!(
        f.w.tables.visible.contains(&id),
        "the premise: it is still in the visible table"
    );
    assert_eq!(f.closest(SelectionType::Monster), None);
}

/// The player is never a candidate, however attackable the attackable test
/// says they are (it answers **true** for the player, by its
/// second line).
#[test]
fn the_player_is_never_a_candidate() {
    let mut f = Fix::new();
    // Give the player everything a monster has and place them at the origin.
    f.w.tables.physics.insert(
        PLAYER,
        dereth_client_model::objects::PhysicsPresence {
            cell: Some(CellId(0x00A9_0100)),
            state: 0,
            parent: None,
            setup_id: 0,
        },
    );
    f.phys.insert(
        PLAYER,
        SelectionPhysics {
            player_space: (0.0, 0.0, 0.0),
            cloaked: false,
            reports_collisions_as_environment: false,
        },
    );
    f.w.update_visible_object_list();
    assert!(
        f.w.tables.visible.contains(&PLAYER),
        "the premise: the player is visible"
    );
    f.make_monster(PLAYER);
    assert!(
        f.w.object_is_attackable(PLAYER),
        "the premise: the player is attackable"
    );
    assert_eq!(f.closest(SelectionType::Monster), None);
}

/// Only the closest item action excludes what you are wielding.
#[test]
fn only_the_closest_item_action_excludes_what_you_are_wielding() {
    let id = ObjectId(0x5000_0065);
    let mut f = Fix::new();
    f.place(id, 1.8, 2.4, 0.0);
    f.weenie_mut(id).pwd.wielder_id = Some(PLAYER);
    // Wielded, so `SelectionType::Item`'s own first gate rejects it as well; use MONSTER for the
    // flag's own test so the two gates cannot stand in for each other.
    f.make_monster(id);

    f.select_next(
        true,
        true,
        SelectionType::Monster,
        true,
        RADAR_RADIUS_OUTDOORS,
    );
    assert_eq!(f.w.selected, None, "excluded because you are wielding it");
    f.select_next(
        true,
        true,
        SelectionType::Monster,
        false,
        RADAR_RADIUS_OUTDOORS,
    );
    assert_eq!(
        f.w.selected,
        Some(id),
        "the control: the flag off, it is a candidate"
    );

    // Wielded by somebody else is never excluded by this flag.
    let mut g = Fix::new();
    g.place(id, 1.8, 2.4, 0.0);
    g.weenie_mut(id).pwd.wielder_id = Some(ObjectId(0x5000_00FF));
    g.make_monster(id);
    g.select_next(
        true,
        true,
        SelectionType::Monster,
        true,
        RADAR_RADIUS_OUTDOORS,
    );
    assert_eq!(
        g.w.selected,
        Some(id),
        "the flag is about YOUR wielded items"
    );
}

// =================================================================================================
// 5. The five filters, one at a time
// =================================================================================================

fn lone(kind: SelectionType, set: impl FnOnce(&mut Fix, ObjectId)) -> Option<ObjectId> {
    let id = ObjectId(0x5000_0071);
    let mut f = Fix::new();
    f.place(id, 1.8, 2.4, 0.0);
    set(&mut f, id);
    f.closest(kind)
}

/// `SelectionType::Item`. **The `RADAR_ENUM` test reads backwards, and that is not a mistake**: an
/// object the radar was never told to show is what "closest item" wants, and one with a
/// `RADAR_ENUM` has to prove itself with [`COMPASS_ALWAYS`].
#[test]
fn the_item_filter() {
    let id = ObjectId(0x5000_0071);
    // Ordinary loot: no wielder, no radar enum.
    assert_eq!(lone(SelectionType::Item, |_, _| {}), Some(id));
    // Wielded by anyone at all — the item filter's first gate.
    assert_eq!(
        lone(SelectionType::Item, |f, id| f
            .weenie_mut(id)
            .pwd
            .wielder_id =
            Some(ObjectId(0x5000_00FF))),
        None
    );
    // A `RADAR_ENUM` and none of the three fixture bits: refused.
    assert_eq!(
        lone(SelectionType::Item, |f, id| f
            .weenie_mut(id)
            .pwd
            .radar_enum = Some(4)),
        None
    );
    // The same object with `BF_PORTAL`: accepted.
    assert_eq!(
        lone(SelectionType::Item, |f, id| {
            f.weenie_mut(id).pwd.radar_enum = Some(4);
            f.weenie_mut(id).pwd.bitfield |= 0x0004_0000;
        }),
        Some(id)
    );
    // `ShowNever` (1) is non-zero too, so it takes the same path.
    assert_eq!(
        lone(SelectionType::Item, |f, id| f
            .weenie_mut(id)
            .pwd
            .radar_enum = Some(1)),
        None
    );
    assert_eq!(
        COMPASS_ALWAYS, 0x0804_4000,
        "BF_LIFESTONE | BF_PORTAL | BF_BINDSTONE"
    );
}

/// The is-player predicate **and** the showable-on-radar query.
#[test]
fn the_player_filter() {
    let id = ObjectId(0x5000_0071);
    let player_ish = |f: &mut Fix, id: ObjectId| {
        f.weenie_mut(id).pwd.bitfield |= bitfield::PLAYER;
        f.weenie_mut(id).pwd.radar_enum = Some(2);
    };
    assert_eq!(lone(SelectionType::Player, player_ish), Some(id));
    // Not a player.
    assert_eq!(
        lone(SelectionType::Player, |f, id| f
            .weenie_mut(id)
            .pwd
            .radar_enum = Some(2)),
        None
    );
    // A player the radar is not told to show.
    assert_eq!(
        lone(SelectionType::Player, |f, id| f
            .weenie_mut(id)
            .pwd
            .bitfield |=
            bitfield::PLAYER),
        None,
        "`RADAR_ENUM` Undef fails the showable-on-radar test"
    );
    // ...and one whose `_phys_obj` is null fails the same predicate for the other reason.
    assert_eq!(
        lone(SelectionType::Player, |f, id| {
            player_ish(f, id);
            f.weenie_mut(id).has_phys_obj = false;
        }),
        None,
        "an object with a wielder is not showable on the radar"
    );
}

/// `SelectionType::Monster` — attackable, not a vendor, showable, not a fellow.
#[test]
fn the_monster_filter() {
    let id = ObjectId(0x5000_0071);
    assert_eq!(
        lone(SelectionType::Monster, |f, id| f.make_monster(id)),
        Some(id)
    );
    // Not attackable: a creature without `BF_ATTACKABLE`.
    assert_eq!(
        lone(SelectionType::Monster, |f, id| {
            f.make_monster(id);
            f.weenie_mut(id).pwd.bitfield &= !bitfield::ATTACKABLE;
        }),
        None
    );
    // A vendor.
    assert_eq!(
        lone(SelectionType::Monster, |f, id| {
            f.make_monster(id);
            f.weenie_mut(id).pwd.bitfield |= bitfield::VENDOR;
        }),
        None
    );
    // Not showable on the radar.
    assert_eq!(
        lone(SelectionType::Monster, |f, id| {
            f.make_monster(id);
            f.weenie_mut(id).pwd.radar_enum = None;
        }),
        None
    );
    // A fellow.
    assert_eq!(
        lone(SelectionType::Monster, |f, id| {
            f.make_monster(id);
            let mut fs = dereth_client_model::fellowship::Fellowship::default();
            fs.members
                .insert(id, dereth_client_model::fellowship::Fellow::default());
            f.w.fellowship = Some(fs);
        }),
        None
    );
}

/// `SelectionType::CompassItem` — the only arm that reads the combat mode, and the
/// only reader of `REPORT_COLLISIONS_AS_ENVIRONMENT_PS` in the function.
#[test]
fn the_compass_item_filter_out_of_a_combat_stance() {
    let id = ObjectId(0x5000_0071);
    // Showable, peace mode: accepted with no further test.
    assert_eq!(
        lone(SelectionType::CompassItem, |f, id| f
            .weenie_mut(id)
            .pwd
            .radar_enum =
            Some(2)),
        Some(id)
    );
    // Neither showable nor a fixture: refused before the mode is even read.
    assert_eq!(lone(SelectionType::CompassItem, |_, _| {}), None);
    // A lifestone: `COMPASS_ALWAYS` lets it past the showable-on-radar test.
    assert_eq!(
        lone(SelectionType::CompassItem, |f, id| f
            .weenie_mut(id)
            .pwd
            .bitfield |=
            0x0000_4000),
        Some(id),
        "BF_LIFESTONE"
    );
}

#[test]
fn the_compass_item_filter_in_a_combat_stance() {
    let id = ObjectId(0x5000_0071);
    let armed = |mode: CombatMode| {
        move |f: &mut Fix, id: ObjectId| {
            f.make_monster(id);
            f.w.combat.combat_mode = mode;
        }
    };
    // Melee, attackable, not a fellow, not a vendor, ordinary collisions: accepted.
    assert_eq!(
        lone(SelectionType::CompassItem, armed(CombatMode::Melee)),
        Some(id)
    );
    assert_eq!(
        lone(SelectionType::CompassItem, armed(CombatMode::Missile)),
        Some(id)
    );
    // Magic is neither `2` nor `4`, so the whole attackability gate is skipped — which is why a
    // vendor that would be refused in melee is accepted in a casting stance.
    assert_eq!(
        lone(SelectionType::CompassItem, |f, id| {
            armed(CombatMode::Magic)(f, id);
            f.weenie_mut(id).pwd.bitfield |= bitfield::VENDOR;
        }),
        Some(id),
        "`cmp 2 / cmp 4`: magic mode takes the accept path"
    );
    assert_eq!(
        lone(SelectionType::CompassItem, |f, id| {
            armed(CombatMode::Melee)(f, id);
            f.weenie_mut(id).pwd.bitfield |= bitfield::VENDOR;
        }),
        None,
        "and in melee the same vendor is refused"
    );
    // Not attackable.
    assert_eq!(
        lone(SelectionType::CompassItem, |f, id| {
            armed(CombatMode::Melee)(f, id);
            f.weenie_mut(id).pwd.bitfield &= !bitfield::ATTACKABLE;
        }),
        None
    );
    // A fellow.
    assert_eq!(
        lone(SelectionType::CompassItem, |f, id| {
            armed(CombatMode::Melee)(f, id);
            let mut fs = dereth_client_model::fellowship::Fellowship::default();
            fs.members
                .insert(id, dereth_client_model::fellowship::Fellow::default());
            f.w.fellowship = Some(fs);
        }),
        None
    );
    // `REPORT_COLLISIONS_AS_ENVIRONMENT_PS` — the physics state bit, this arm alone.
    assert_eq!(
        lone(SelectionType::CompassItem, |f, id| {
            armed(CombatMode::Melee)(f, id);
            f.phys
                .get_mut(&id)
                .expect("placed")
                .reports_collisions_as_environment = true;
        }),
        None
    );
    assert_eq!(REPORT_COLLISIONS_AS_ENVIRONMENT_PS, 0x0020_0000);
}

/// The unopened corpse filter.
#[test]
fn the_unopened_corpse_filter() {
    let id = ObjectId(0x5000_0071);
    assert_eq!(
        lone(SelectionType::UnopenedCorpse, |f, id| f
            .weenie_mut(id)
            .pwd
            .bitfield |=
            bitfield::CORPSE),
        Some(id)
    );
    // Not a corpse.
    assert_eq!(lone(SelectionType::UnopenedCorpse, |_, _| {}), None);
    // A corpse this session has already opened.
    assert_eq!(
        lone(SelectionType::UnopenedCorpse, |f, id| {
            f.weenie_mut(id).pwd.bitfield |= bitfield::CORPSE;
            f.w.opened_corpses.insert(id);
            assert!(f.w.has_corpse_been_opened(id), "the premise");
        }),
        None
    );
}

/// The unopened corpse cycle skips the corpse you already opened.
#[test]
fn the_unopened_corpse_cycle_skips_the_corpse_you_already_opened() {
    let (near, far) = (ObjectId(0x5000_0081), ObjectId(0x5000_0082));
    let mut f = Fix::new();
    f.place(near, 1.8, 2.4, 0.0);
    f.place(far, 3.6, 4.8, 0.0);
    for id in [near, far] {
        f.weenie_mut(id).pwd.bitfield |= bitfield::CORPSE;
    }
    assert_eq!(f.closest(SelectionType::UnopenedCorpse), Some(near));

    // Opening a ground container records that corpse as opened.
    f.w.opened_corpses.insert(near);
    f.w.selected = None;
    f.w.prev_selected = None;
    assert_eq!(
        f.closest(SelectionType::UnopenedCorpse),
        Some(far),
        "the nearest UNOPENED corpse is now the far one"
    );
}
