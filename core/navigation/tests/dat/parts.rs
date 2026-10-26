//! Dungeons of parts no walk joins, linked only by portals inside them: Shoushi's Training Academy
//! (`0x7F03`), where a new character starts, and the Whispering Caverns (`0xEA6C`).

use dereth_navigation::{
    global_distance, plan_journey, EdgeSource, JourneyConfig, Leg, NoPath, PortalEdge, Realm,
};
use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

use crate::common::*;

fn at(cell: u32, x: f32, y: f32, z: f32) -> Position {
    Position::new(CellId(cell), Frame::new(Vec3::new(x, y, z), Quat::IDENTITY))
}

/// Where a new character enters Shoushi's Academy, as a recorded first login placed one.
fn academy_start() -> Position {
    at(0x7F03_01AD, 12.32, -28.482, 0.005)
}

/// The Academy's exit portal, as the world places it, and where it sends: Shoushi.
fn academy_exit() -> PortalEdge {
    edge(
        "Exit to Shoushi",
        at(0x7F03_0169, 158.641, -149.516, -6.063),
        at(0xDA55_001D, 84.8, 99.0, 20.0),
    )
}

fn edge(name: &str, from: Position, to: Position) -> PortalEdge {
    PortalEdge {
        source: EdgeSource::Published,
        wcid: None,
        name: name.to_owned(),
        from,
        to: Some(to),
        verified: 0,
        uses: 0,
        failures: 0,
        levels: (None, None),
        retired: None,
    }
}

/// Behaviour: world.dungeons.the-academys-first-rooms-are-not-joined-to-its-exit-portals-room
#[test]
fn a_new_characters_first_rooms_in_the_academy_are_not_walked_to_its_exit_portal() {
    let mut d = dungeon(0x7F03);
    let exit = academy_exit().from;
    let (start, end) = (d.nav.realm(&academy_start()), d.nav.realm(&exit));
    assert!(
        matches!((start, end), (Some(Realm::Part(a)), Some(Realm::Part(b))) if a != b),
        "two parts of one dungeon: {start:?} and {end:?}"
    );
    assert!(matches!(
        d.nav.plan(&academy_start(), &exit),
        Err(NoPath::Disconnected { .. })
    ));
}

/// Behaviour: plugins.nav.a-journey-between-two-parts-of-a-dungeon-takes-the-portal-inside-it
#[test]
fn a_journey_between_parts_of_a_dungeon_takes_its_portal_and_none_leaves_a_part_with_no_way_out() {
    let mut d = dungeon(0xEA6C);
    // The Whispering Caverns portal, inside the dungeon, and where it sends.
    let caverns = edge(
        "Whispering Caverns",
        at(0xEA6C_015B, 192.236, 143.991, -11.263),
        at(0xEA6C_0156, 155.588, 246.538, -11.195),
    );
    // A place in the portal's part and the place of the other part nearest it.
    let (from, to) = {
        let mut near = |cell: u32| {
            let r = d.nav.region(CellId(cell)).expect("the part is loaded");
            let ids: Vec<u32> = r.places().filter(|i| r.is_clear(*i)).step_by(16).collect();
            ids.into_iter().map(|i| r.position(i)).collect::<Vec<_>>()
        };
        let (a, b) = (near(0xEA6C_015B), near(0xEA6C_0156));
        let mut best = (f32::MAX, a[0], b[0]);
        for p in &a {
            for q in &b {
                let g = global_distance(p, q);
                if g < best.0 {
                    best = (g, *p, *q);
                }
            }
        }
        (best.1, best.2)
    };
    let realm = |p: &Position| d.nav.realm(p);
    assert_ne!(realm(&from), realm(&to), "premise: two parts");
    let j = plan_journey(
        &from,
        &to,
        &[&caverns],
        &[],
        &realm,
        &JourneyConfig::default(),
    )
    .expect("a journey");
    assert!(
        matches!(
            j.legs.as_slice(),
            [Leg::Walk { .. }, Leg::Portal { .. }, Leg::Walk { .. }]
        ),
        "{:?}",
        j.legs
    );
    // In the Academy, its exit portal stands in a part no walk from a new character's first rooms
    // reaches: knowing the portal, there is still no journey out from there.
    let academy = dungeon(0x7F03);
    let exit = academy_exit();
    let shoushi = exit.to.expect("a destination");
    let realm = |p: &Position| academy.nav.realm(p);
    assert_eq!(
        plan_journey(
            &academy_start(),
            &shoushi,
            &[&exit],
            &[],
            &realm,
            &JourneyConfig::default(),
        ),
        None
    );
}
