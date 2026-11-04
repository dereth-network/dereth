//! Creatures' steps against the September 2013 world's walls: the human training hall (copy
//! 0x0298), its rooms and doorways.

use auberean_assets::Context;
use auberean_dat::locate::{install_dats, DatName};
use auberean_dat::DatFile;
use auberean_physics::land::Land;
use auberean_world::records::WorldRecords;
use yalain_store::StoredPosition;
use yalain_testkit::{Harness, TEST_CREATURE};
use yalain_world::walls::{RecordBlocks, Standing, Walker, Walls};

/// The install's world records, opened anew.
fn records() -> WorldRecords {
    let dir = auberean_dat::testing::require_target_install();
    let mut files: Vec<(DatName, DatFile)> = install_dats(&dir)
        .expect("list the install")
        .into_iter()
        .map(|(n, p)| (n, DatFile::open(&p).expect("open")))
        .collect();
    let mut take = |name: &DatName| {
        let k = files.iter().position(|(n, _)| n == name)?;
        Some(files.swap_remove(k).1)
    };
    let portal = take(&DatName::Portal).expect("a portal file");
    let cell = take(&DatName::Cell(1)).expect("a cell file");
    let ctx = Context::from_portal(&portal).expect("the property list");
    WorldRecords::new(portal, cell, None, ctx)
}

/// Walls over the install's world, with the Drudge Digger's body.
pub(crate) fn walls() -> Walls {
    let records = records();
    let region = records.region().expect("the region");
    let mut world = auberean_physics::world::World::new();
    if let Some(list) = records.get::<auberean_assets::property::PropertyList>(0x3400_0000) {
        world.set_property_list(list);
    }
    let digger = digger_walker(&records);
    let mut w = Walls::new(
        std::sync::Arc::new(Land::new()),
        world,
        Box::new(RecordBlocks::new(records, region)),
    );
    w.walkers.insert(DIGGER, digger);
    w
}

/// The Drudge Digger's template.
pub(crate) const DIGGER: u32 = 0x4700_23B3;

fn digger_walker(records: &WorldRecords) -> Walker {
    let describe = yalain_content::describe(
        DIGGER,
        &|id| records.get::<auberean_assets::entity::Entity>(id),
        &|_| None,
    )
    .expect("the Digger's description");
    yalain_world::walls::walker(records, describe.look).expect("the Digger's body")
}

/// Is a place inside the interior cell it names?
pub(crate) fn inside(w: &Walls, at: &StoredPosition) -> bool {
    let Some(cell) = w.land().cell(dereth_primitives::CellId(at.cell)) else {
        return false;
    };
    let p = dereth_primitives::Vec3::new(at.origin[0], at.origin[1], at.origin[2] + 0.5);
    let local = auberean_physics::geom::point_to_local(&cell.frame, p);
    cell.cell_bsp
        .as_ref()
        .is_some_and(|t| t.point_inside_cell_bsp(local))
}

/// A place in the hall.
fn hall(cell: u32, x: f32, y: f32, z: f32) -> StoredPosition {
    StoredPosition {
        cell: 0x0298_0000 | cell,
        origin: [x, y, z],
        rotation: [1.0, 0.0, 0.0, 0.0],
    }
}

/// The harness with the walls, one beetle at rest that never attacks first and wanders whenever
/// it may, at home at `home`; and a player standing there.
fn beetle_at(home: StoredPosition) -> (Harness, usize, u64) {
    let mut h = Harness::new();
    h.add_creatures(1);
    let (b, _) = h.player_in_world("one", "Elara");
    let mut w = walls();
    let _ = w.joined(&home, &home);
    h.server.walls = Some(w);
    // The player stands by it (a creature no player could see rests).
    for p in h.server.players.values_mut() {
        p.position = home;
    }
    let beetle = h
        .server
        .things
        .by_id
        .values()
        .find(|t| t.template == TEST_CREATURE)
        .map(|t| t.id)
        .expect("the camp's beetle");
    let now = h.now.0;
    if let Some(t) = h.server.things.get_mut(beetle) {
        t.at = Some(home);
        if let Some(c) = t.creature.as_mut() {
            c.home = home;
            c.next_wander = now;
            c.stats.attacks_first = false;
            c.stats.movement.wander_range = 15.0;
            c.stats.movement.wander_chance = 0.0;
        }
    }
    (h, b, beetle)
}

fn place_of(h: &Harness, id: u64) -> StoredPosition {
    h.server.things.get(id).and_then(|t| t.at).expect("placed")
}

/// A creature going after a player in the next room, a wall between them, runs to it through the
/// doorway: every place it is told at lies inside the room it names, and within six seconds it is
/// in the player's room within reach.
/// Behaviour: server.creatures.chase-around-walls
#[test]
fn a_creature_after_a_player_in_the_next_room_runs_through_the_doorway() {
    let start = hall(0x010C, 16.0, 428.0, -43.1);
    let there = hall(0x010D, 6.0, 431.0, -43.5);
    let (mut h, _, beetle) = beetle_at(start);
    {
        let walls = h.server.walls.as_mut().expect("the walls");
        let _ = walls.joined(&start, &there);
        let wall = auberean_physics::interior::trace(
            walls.land(),
            dereth_primitives::CellId(start.cell),
            dereth_primitives::Vec3::new(16.0, 428.0, -42.1),
            dereth_primitives::Vec3::new(6.0, 431.0, -42.5),
            true,
        );
        assert!(wall.is_some(), "the cave's rock stands between them");
    }
    let player = h
        .server
        .players
        .values()
        .map(|p| p.character)
        .next()
        .expect("the player");
    for p in h.server.players.values_mut() {
        p.position = there;
    }
    let clock = h.clock();
    yalain_world::ai::go_after(&mut h.server, &mut h.net, beetle, player, clock);
    let reached = h.run_until(120, |h| {
        let at = place_of(h, beetle);
        let walls = h.server.walls.as_ref().expect("the walls");
        assert!(
            inside(walls, &at),
            "inside the room it names: {:08X} {:?}",
            at.cell,
            at.origin
        );
        let (dx, dy) = (
            at.origin[0] - there.origin[0],
            at.origin[1] - there.origin[1],
        );
        at.cell == there.cell && (dx * dx + dy * dy).sqrt() <= yalain_world::ai::MELEE_REACH
    });
    assert!(reached, "it reached the player: {:?}", place_of(&h, beetle));
}

/// Outdoors a creature's steps meet what stands in the world: walking north from the arrival
/// place of landblock 0x6FA9 toward a point past the first building, it is stopped short at the
/// building, where a straight line would have gone through.
/// Behaviour: server.creatures.chase-around-walls
#[test]
fn outdoors_a_creatures_walk_stops_at_a_building() {
    let start = StoredPosition {
        cell: yalain_world::spawn::outdoor(0x6FA9, 45.24, 90.51).0,
        origin: [45.24, 90.51, 85.26],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    let mut w = walls();
    let mut at = start;
    for _ in 0..100 {
        let s = w
            .step(TEST_CREATURE, 1.0, &at, [0.0, 0.78])
            .expect("placed");
        at = s.at;
    }
    assert!(
        at.origin[1] > 100.0 || at.cell >> 16 != 0x6FA9,
        "it went north: {at:?}"
    );
    assert!(
        at.cell >> 16 == 0x6FA9 && at.origin[1] < 150.0,
        "stopped at the building, not 78 m on: {:08X} {:?}",
        at.cell,
        at.origin
    );
}

/// A creature going after a player that no way through the walls leads to (the player in the
/// hall's other copy, beside this one) runs until the rock holds it, finds no way, and counts the
/// player as one it could not reach this fight (its kill no fair fight); one going after a player
/// in the next room, through the doorway, counts no one.
/// Behaviour: server.creatures.could-not-reach
#[test]
fn a_creature_with_no_way_to_its_player_counts_it_as_one_it_could_not_reach() {
    let start = hall(0x010C, 16.0, 428.0, -43.1);
    let run = |there: StoredPosition| {
        let (mut h, _, beetle) = beetle_at(start);
        if let Some(c) = h
            .server
            .things
            .get_mut(beetle)
            .and_then(|t| t.creature.as_mut())
        {
            c.stats.movement.perception = 400.0;
        }
        let player = h
            .server
            .players
            .values()
            .map(|p| p.character)
            .next()
            .expect("the player");
        for p in h.server.players.values_mut() {
            p.position = there;
        }
        let clock = h.clock();
        yalain_world::ai::go_after(&mut h.server, &mut h.net, beetle, player, clock);
        h.run(100);
        let unreached = h
            .server
            .things
            .get(beetle)
            .and_then(|t| t.creature.as_ref())
            .map(|c| c.unreached.clone())
            .unwrap_or_default();
        (unreached, player)
    };
    let beside = StoredPosition {
        cell: 0x0299_010C,
        ..start
    };
    let (none, player) = run(beside);
    assert_eq!(none, vec![player], "no way leads there");
    let (next_room, _) = run(hall(0x010D, 6.0, 431.0, -43.5));
    assert!(
        next_room.is_empty(),
        "the doorway leads there: {next_room:x?}"
    );
}

/// How far north a creature gets from open ground, against what is `placed` there, and how high.
fn furthest_north(placed: Vec<Standing>) -> (f32, f32) {
    let mut w = walls();
    w.set_standing(placed);
    let mut at = StoredPosition {
        cell: yalain_world::spawn::outdoor(0x6FA9, 45.24, 90.51).0,
        origin: [45.24, 90.51, 85.26],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    for _ in 0..12 {
        at = w.step(DIGGER, 1.0, &at, [0.0, 0.5]).expect("placed").at;
    }
    (at.origin[1], at.origin[2])
}

/// What a world places on that ground, 4 m on from where the creature sets out.
fn a_barrel(scale: f32) -> Standing {
    let records = records();
    let describe = yalain_content::describe(
        BARREL,
        &|id| records.get::<auberean_assets::entity::Entity>(id),
        &|_| None,
    )
    .expect("the barrel's description");
    Standing {
        id: 0x4000_0000_0000_0001,
        template: BARREL,
        look: describe.look,
        scale,
        at: StoredPosition {
            cell: yalain_world::spawn::outdoor(0x6FA9, 45.24, 94.51).0,
            origin: [45.24, 94.51, 85.26],
            rotation: [1.0, 0.0, 0.0, 0.0],
        },
        ethereality: auberean_physics::solid::Ethereality::default(),
    }
}

/// A barrel the world places stands in a creature's way at the size it stands, and a creature
/// goes on where the world no longer places it.
/// Behaviour: server.creatures.chase-around-walls
#[test]
fn what_a_world_places_holds_a_creature_up_at_the_size_it_stands() {
    let open = furthest_north(Vec::new()).0;
    let small = furthest_north(vec![a_barrel(1.0)]);
    let large = furthest_north(vec![a_barrel(3.0)]);
    assert!(open > 96.0, "open ground: {open}");
    // The barrel's own height is within the creature's step: it goes up and over, not through.
    assert!(small.1 > 86.5, "on the barrel: {small:?}");
    // Three times the size, its top is out of reach.
    assert!(large.0 < 92.5, "held off it: {large:?}");
    assert!(large.1 < 85.5, "on the ground: {large:?}");
    let mut w = walls();
    w.set_standing(vec![a_barrel(1.0)]);
    w.set_standing(Vec::new());
    let start = StoredPosition {
        cell: yalain_world::spawn::outdoor(0x6FA9, 45.24, 94.0).0,
        origin: [45.24, 94.0, 85.26],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    let on = w.step(DIGGER, 1.0, &start, [0.0, 1.0]).expect("placed");
    assert!(on.at.origin[1] > 94.5, "taken away: {:?}", on.at.origin);
}

/// The barrel's template.
const BARREL: u32 = 0x4700_26C4;
