//! Dungeons whose lower floors no walk reaches, only a drop off a ledge: the Black Dominion,
//! whose portal lands a character on a floor its stairs and ramps join to one 48 m down, below
//! which the way goes on only by dropping off a ledge; and Colier Mine, whose floors at -12 m and
//! 12 m lie under holes in the floor its entrance is on. A walk never takes a drop; a journey
//! takes one, one way, where nothing else reaches, within a fall the body survives.

use dereth_navigation::{
    plan_journey_or_drop, survivable_drop, DropLink, JourneyConfig, Leg, NoPath, Realm,
};
use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

use crate::common::*;
use crate::drive::{drive, step_off};

fn at(cell: u32, x: f32, y: f32, z: f32) -> Position {
    Position::new(CellId(cell), Frame::new(Vec3::new(x, y, z), Quat::IDENTITY))
}

/// A character whose full-power jump leaves the ground at 10.5 metres a second (a jump of 5.6 m),
/// who lands unhurt from a hop off a 12-metre ledge; and one at 10.0 (5.1 m), who would not.
const SPRINGY: f32 = 10.5;
const HEAVY: f32 = 10.0;

/// The journey from `from` to `to` over walking alone, or, where that finds none, the drops off
/// the ledges of the dungeon's built places that a character jumping at `jump_speed` lands from
/// unhurt, as the client's planning asks it: where a walk can go told by the stretches of floor
/// the places join. With it, whether the drops were asked for.
fn journey(
    d: &Dungeon,
    from: &Position,
    to: &Position,
    jump_speed: f32,
) -> (Option<Vec<Leg>>, bool) {
    let realm = |p: &Position| -> Option<Realm> { d.nav.walk_set(p).or_else(|| d.nav.realm(p)) };
    let asked = std::cell::Cell::new(false);
    let drops = || -> Vec<DropLink> {
        asked.set(true);
        [from, to]
            .into_iter()
            .flat_map(|p| d.nav.drops(p, survivable_drop(jump_speed)))
            .collect()
    };
    let j = plan_journey_or_drop(
        from,
        to,
        &[],
        &[],
        &[],
        drops,
        &realm,
        &JourneyConfig::default(),
    );
    (j.map(|j| j.legs), asked.get())
}

/// The journey's drops, each walked to: a walk to a ledge, the drop, and so on, ending in a walk.
fn the_drops(legs: &[Leg]) -> Vec<DropLink> {
    let mut drops = Vec::new();
    for pair in legs.windows(2) {
        if let [Leg::Walk { to }, Leg::Drop { drop }] = pair {
            assert_eq!(*to, drop.stand, "walked to the ledge: {legs:?}");
            drops.push(drop.clone());
        }
    }
    assert!(
        legs.iter()
            .all(|l| matches!(l, Leg::Walk { .. } | Leg::Drop { .. }))
            && matches!(legs.last(), Some(Leg::Walk { .. }))
            && legs.len() == 2 * drops.len() + 1,
        "walks and drops, ending in a walk: {legs:?}"
    );
    drops
}

/// Hop off `drop`'s ledge: where the body comes down, which is the floor below, near where the
/// drop was said to land.
fn hop(d: &mut Dungeon, drop: &DropLink) -> Position {
    let came = step_off(&mut d.world, &d.player, &drop.stand, &drop.land).expect("off the ledge");
    let (c, l) = (
        dereth_navigation::global(&came),
        dereth_navigation::global(&drop.land),
    );
    assert!(
        (c.z - l.z).abs() < 1.0,
        "down to the floor below: {came:?} for {drop:?}"
    );
    assert!(
        ((c.x - l.x).powi(2) + (c.y - l.y).powi(2)).sqrt() < 6.0,
        "near where it was said to land: {came:?} for {drop:?}"
    );
    came
}

/// Behaviour: plugins.nav.a-way-only-a-drop-leads-on-is-taken-by-the-drop
#[test]
fn a_journey_down_the_black_dominion_past_its_ledge_takes_the_drop_no_walk_takes() {
    let mut d = dungeon(0x010A);
    let portal_drop = at(0x010A_0223, 0.0, -70.0, 0.0);
    let (from, walked, low) = {
        let region = d.nav.region(portal_drop.cell).expect("the dungeon");
        let s = region
            .nearest_place(&portal_drop, 12.0)
            .expect("a place at the drop");
        let l = lowest(region).expect("a lowest place");
        (
            region.position(s),
            region.position(bottom(region, s)),
            region.position(l),
        )
    };
    assert!(
        walked.frame.origin.z > low.frame.origin.z + 6.0,
        "premise: the walk ends above the bottom ({} against {})",
        walked.frame.origin.z,
        low.frame.origin.z
    );
    // A walk alone never takes the drop: it names it.
    let named = match d.nav.plan(&from, &low) {
        Err(NoPath::OnlyByDrop {
            stand,
            land,
            height,
        }) => {
            assert!((5.0..7.0).contains(&height), "{height}");
            assert!(
                (stand.frame.origin.z - land.frame.origin.z - height).abs() < 0.01,
                "{stand:?} {land:?}"
            );
            height
        }
        other => panic!("only a drop leads on, and the walk names it: {other:?}"),
    };
    // With nothing else, the journey takes the drops, ledge after ledge, the first the one the
    // walk named: each a fall the body survives.
    let (legs, asked) = journey(&d, &from, &low, SPRINGY);
    assert!(asked, "the drops looked for only now");
    let drops = the_drops(&legs.expect("a journey by the drops"));
    assert!((drops[0].height - named).abs() < 0.01, "{drops:?}");
    assert!(
        drops.iter().all(|x| x.height <= survivable_drop(SPRINGY)),
        "{drops:?}"
    );
    // One of them is 12 metres: a heavier jumper would be hurt, and has no journey.
    assert!(drops.iter().any(|x| x.height > survivable_drop(HEAVY)));
    assert!(journey(&d, &from, &low, HEAVY).0.is_none());
    // Checked by the body's own movement: hopping off each ledge, it comes down on the floor
    // below, near where the drop was said to land.
    for drop in &drops {
        hop(&mut d, drop);
    }
}

/// Colier Mine's entrance, and the stairs check's start on the same floor.
const ENTRANCE: (u32, f32, f32, f32) = (0x01AE_032C, 76.59, -97.36, 0.0);
const SAME_FLOOR: (u32, f32, f32, f32) = (0x01AE_0361, 140.36, -67.47, 0.01);

/// Behaviour: plugins.nav.a-drop-is-taken-only-where-the-character-lands-unhurt
/// From Colier Mine's entrance to its floors at -12 m and 12 m, under the holes in the entrance's
/// floor: for a character whose jump lands it unhurt from 12 metres, a walk to a hole's edge, the
/// drop, and a walk on, every leg walked by the body, driven by keys; for one whose jump would
/// not, no journey.
#[test]
fn colier_mines_floors_under_its_holes_are_reached_from_its_entrance_by_a_drop() {
    let mut d = dungeon(0x01AE);
    let entrance = at(ENTRANCE.0, ENTRANCE.1, ENTRANCE.2, ENTRANCE.3);
    for (floor, to) in [
        ("-12 m", at(0x01AE_01FE, 97.99, -28.29, -11.99)),
        ("12 m", at(0x01AE_03DC, 130.26, -49.23, 12.01)),
    ] {
        assert!(
            matches!(d.nav.plan(&entrance, &to), Err(NoPath::OnlyByDrop { .. })),
            "premise: no walk reaches the {floor} floor"
        );
        assert!(
            journey(&d, &entrance, &to, HEAVY).0.is_none(),
            "{floor}: no drop the heavier jumper is hurt by"
        );
        let (legs, _) = journey(&d, &entrance, &to, SPRINGY);
        let drops = the_drops(&legs.unwrap_or_else(|| panic!("{floor}: no journey")));
        let [drop] = drops.as_slice() else {
            panic!("{floor}: one drop down a hole: {drops:?}");
        };
        assert!(
            (11.5..=survivable_drop(SPRINGY)).contains(&drop.height),
            "{floor}: {drop:?}"
        );
        let drop = drop.clone();
        // To the hole's edge, off it, and on.
        let player = std::sync::Arc::clone(&d.player);
        let Dungeon { world, nav, .. } = &mut d;
        let path = nav
            .plan(&entrance, &drop.stand)
            .expect("a walk to the ledge");
        let mut replan = |p: &Position, _: &Position| nav.plan(p, &drop.stand).ok();
        let r = drive(world, &player, path, 20.0, &mut replan);
        assert!(r.arrived && r.stuck == 0, "{floor}: to the ledge, {r:?}");
        let came = hop(&mut d, &drop);
        let player = std::sync::Arc::clone(&d.player);
        let Dungeon { world, nav, .. } = &mut d;
        let path = nav
            .plan(&came, &to)
            .unwrap_or_else(|e| panic!("{floor}: no walk on from {came:?}: {e}"));
        let mut replan = |p: &Position, _: &Position| nav.plan(p, &to).ok();
        let r = drive(world, &player, path, 20.0, &mut replan);
        assert!(r.arrived && r.stuck == 0, "{floor}: on to it, {r:?}");
    }
}

/// Behaviour: plugins.nav.a-way-only-a-drop-leads-on-is-taken-by-the-drop
/// On the floor the body stands on, a walk: no drop is taken, and none is looked for.
#[test]
fn where_a_walk_reaches_no_drop_is_taken_or_looked_for() {
    let mut d = dungeon(0x01AE);
    let entrance = at(ENTRANCE.0, ENTRANCE.1, ENTRANCE.2, ENTRANCE.3);
    let near = at(SAME_FLOOR.0, SAME_FLOOR.1, SAME_FLOOR.2, SAME_FLOOR.3);
    d.nav.plan(&entrance, &near).expect("a walk");
    let (legs, asked) = journey(&d, &entrance, &near, SPRINGY);
    let legs = legs.expect("a journey");
    assert!(
        legs.iter().all(|l| matches!(l, Leg::Walk { .. })),
        "{legs:?}"
    );
    assert!(!asked, "no drop looked for where a walk reaches");
}
