//! Creatures' steps against the September 2013 world's walls: the human training hall (copy
//! 0x0298), its rooms and doorways. And a player's body on the server following what its client
//! reports through the same walls: the open ground outdoors at landblock 0x6FA9, the building north
//! of it, and the hall's rock. And sight through them: what stands between two places (walls,
//! floors, rock, the ground, buildings, closed doors), and the creatures that attack, give up and
//! take on again by it.

use auberean_assets::Context;
use auberean_dat::locate::{install_dats, DatName};
use auberean_dat::DatFile;
use auberean_physics::ethereal::SightLine;
use auberean_physics::geom::point_to_local;
use auberean_physics::land::Land;
use auberean_physics::world::World;
use auberean_world::records::WorldRecords;
use dereth_primitives::num::math;
use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};
use yalain_store::StoredPosition;
use yalain_testkit::{Harness, TEST_CREATURE};
use yalain_world::walls::{
    setup_walker, RecordBlocks, Standing, Walker, Walls, FOLLOWS_FARTHEST, SIGHT_RISE,
};

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
    let digger = walker_of(&records, DIGGER);
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

/// The body a template gives, passing through what its description says.
fn walker_of(records: &WorldRecords, template: u32) -> Walker {
    let description = yalain_testkit::authored::Portal::open()
        .template(template)
        .description;
    yalain_world::walls::walker(records, &description).expect("the body")
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
        // What it counts while it is still after the player (one it neither sees nor can reach it
        // soon gives up).
        let unreached = |h: &Harness| {
            h.server
                .things
                .get(beetle)
                .and_then(|t| t.creature.as_ref())
                .map(|c| c.unreached.clone())
                .unwrap_or_default()
        };
        let mut seen = Vec::new();
        for _ in 0..100 {
            h.step();
            let now = unreached(&h);
            if !now.is_empty() {
                seen = now;
                break;
            }
        }
        (seen, player)
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
    let small = furthest_north(vec![a_barrel(0.1)]);
    let large = furthest_north(vec![a_barrel(1.0)]);
    assert!(open > 96.0, "open ground: {open}");
    // A tenth of its size, its top is within what the creature's sphere reaches: it goes up and
    // over, not through.
    assert!(small.0 > 94.51, "past the barrel: {small:?}");
    assert!(small.1 < 85.3 + 0.2, "back on the ground beyond: {small:?}");
    // At its size, its side is too tall to step onto.
    assert!(large.0 < 93.6, "held off it: {large:?}");
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
/// A template with one large sphere for a body.
const LARGE: u32 = 0x4700_2229;
/// The creature recorded going through marked scenery, and one recorded held up at scenery.
const PASSER: u32 = 0x4700_23B9;
const HELD: u32 = 0x4700_29A5;
/// A chest the world places.
const CHEST: u32 = 0x4700_0F30;

/// Walks a creature at a barrel of `scale` from `sides` sides in strides of `stride` meters, from
/// 4 m off the place it stands to as far beyond it, and returns the farthest any stride put it back
/// along its way or carried it past the stride.
fn farthest_put_back(scale: f32, stride: f32, sides: u8) -> f32 {
    let mut w = walls();
    w.set_standing(vec![a_barrel(scale)]);
    let [cx, cy, z] = a_barrel(scale).at.origin;
    let mut worst = 0.0f32;
    for side in 0..sides {
        let a = f32::from(side) / f32::from(sides) * core::f32::consts::TAU;
        let (ux, uy) = (
            dereth_primitives::num::math::sinf(a),
            dereth_primitives::num::math::cosf(a),
        );
        let (cell, x, y) = yalain_world::spawn::outdoor(0x6FA9, cx - ux * 4.0, cy - uy * 4.0);
        let mut at = StoredPosition {
            cell,
            origin: [x, y, z],
            rotation: [1.0, 0.0, 0.0, 0.0],
        };
        for _ in 0..30 {
            let (dx, dy) = (ux * stride, uy * stride);
            let s = w.step(DIGGER, 1.0, &at, [dx, dy]).expect("placed");
            let (mx, my) = (s.at.origin[0] - at.origin[0], s.at.origin[1] - at.origin[1]);
            worst = worst.max(-(mx * ux + my * uy)).max(s.moved - stride);
            at = s.at;
        }
    }
    worst
}

/// A creature held up at what a world places is not put back along its way, from whichever side
/// it comes, whatever the length of its strides and whether it can climb what it met (2017's
/// creatures: no such reversal in 2,831 segments at solid shapes).
/// Behaviour: server.creatures.held-up-not-put-back
#[test]
fn a_creature_held_up_at_what_a_world_places_is_not_put_back() {
    for scale in [0.3, 0.6, 1.0, 1.5, 3.0] {
        for stride in [0.5, 0.78, 1.0] {
            let back = farthest_put_back(scale, stride, 16);
            assert!(back < 0.3, "scale {scale}, stride {stride}: {back}");
        }
    }
}

/// Where what stands in a town holds a large body up at a place a placement would refuse it, it
/// stands held there, and is not put where a placement would.
/// Behaviour: server.creatures.held-up-not-put-back
#[test]
fn a_body_held_where_a_placement_would_refuse_it_stays_where_it_is() {
    let body = walker_of(&records(), LARGE);
    let mut w = walls();
    w.walkers.insert(LARGE, body);
    let (cx, cy, z) = (96.6244, 141.5138, 190.505);
    let person = |id, template, look, origin, rotation| Standing {
        id,
        template,
        look,
        scale: 1.0,
        at: StoredPosition {
            cell: yalain_world::spawn::outdoor(0x5765, cx, cy).0,
            origin,
            rotation,
        },
        ethereality: auberean_physics::solid::Ethereality {
            physics_type: 0x0100_0020,
            movement: 0x0220_0010,
            placement: 0x02A0_0090,
        },
    };
    w.set_standing(vec![
        person(
            0x4000_0000_0000_0001,
            0x4700_2EDD,
            0x1F00_0F20,
            [cx, cy, z],
            [0.793_353, 0.0, 0.0, -0.608_761],
        ),
        person(
            0x4000_0000_0000_0002,
            0x4700_0CD6,
            0x1F00_0691,
            [85.2, 140.3001, 190.6],
            [
                core::f32::consts::FRAC_1_SQRT_2,
                0.0,
                0.0,
                -core::f32::consts::FRAC_1_SQRT_2,
            ],
        ),
    ]);
    let mut worst = 0.0f32;
    for side in 0..8u8 {
        let a = f32::from(side) * core::f32::consts::FRAC_PI_4;
        let (ux, uy) = (
            dereth_primitives::num::math::sinf(a),
            dereth_primitives::num::math::cosf(a),
        );
        let (x, y) = (cx + ux * 10.0, cy + uy * 10.0);
        let (cell, x, y) = yalain_world::spawn::outdoor(0x5765, x, y);
        let mut at = StoredPosition {
            cell,
            origin: [x, y, z],
            rotation: [1.0, 0.0, 0.0, 0.0],
        };
        let goal = (cx - ux * 10.0, cy - uy * 10.0);
        let mut first = true;
        for _ in 0..28 {
            let (dx, dy) = (goal.0 - at.origin[0], goal.1 - at.origin[1]);
            let len = (dx * dx + dy * dy).sqrt();
            if len < 0.5 {
                break;
            }
            let go = len.min(1.0);
            let s = w
                .step(LARGE, 1.0, &at, [dx / len * go, dy / len * go])
                .expect("placed");
            let along = (s.at.origin[0] - at.origin[0]) * dx / len
                + (s.at.origin[1] - at.origin[1]) * dy / len;
            if !first {
                worst = worst.max(-along);
            }
            first = false;
            at = s.at;
        }
    }
    assert!(worst < 0.3, "put back {worst}");
}

/// Where one creature stopped holds no other up: a creature of another template walks through the
/// place.
/// Behaviour: server.creatures.chase-around-walls
#[test]
fn where_a_creature_stopped_holds_no_other_up() {
    let on_open_ground = |y| StoredPosition {
        cell: yalain_world::spawn::outdoor(0x6FA9, 45.24, y).0,
        origin: [45.24, y, 85.26],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    let mut w = walls();
    let mut stopped = on_open_ground(90.51);
    for _ in 0..4 {
        stopped = w
            .step(DIGGER, 1.0, &stopped, [0.0, 0.5])
            .expect("placed")
            .at;
    }
    let mut other = on_open_ground(88.51);
    for _ in 0..12 {
        other = w
            .step(TEST_CREATURE, 1.0, &other, [0.0, 0.5])
            .expect("placed")
            .at;
    }
    assert!(
        other.origin[1] > stopped.origin[1] + 1.0,
        "past it: {:?}, it stopped at {:?}",
        other.origin,
        stopped.origin
    );
}

/// Takes a creature of `template` in strides of a meter straight from `from` toward `(x, y)`, up
/// to 30 strides, and tells how near it got on the ground.
fn left_of_a_walk(w: &mut Walls, template: u32, from: StoredPosition, x: f32, y: f32) -> f32 {
    let mut at = from;
    let left = |at: &StoredPosition| {
        let (dx, dy) = (x - at.origin[0], y - at.origin[1]);
        (dx * dx + dy * dy).sqrt()
    };
    for _ in 0..30 {
        let len = left(&at);
        if len < 0.2 {
            break;
        }
        let go = len.min(1.0);
        let d = [(x - at.origin[0]) / len * go, (y - at.origin[1]) / len * go];
        at = w.step(template, 1.0, &at, d).expect("placed").at;
    }
    left(&at)
}

/// A creature goes through what the world marks for creatures to pass, along the very way a
/// recorded one went: its template's description names that mark among what it passes through.
/// Behaviour: server.creatures.pass-what-creatures-pass
#[test]
fn a_creature_goes_through_scenery_marked_for_creatures_to_pass() {
    let records = records();
    let mut w = walls();
    w.walkers.insert(PASSER, walker_of(&records, PASSER));
    let from = StoredPosition {
        cell: yalain_world::spawn::outdoor(0x7D93, 69.8, 56.1).0,
        origin: [69.8, 56.1, 127.49],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    let left = left_of_a_walk(&mut w, PASSER, from, 78.8, 63.9);
    assert!(left < 0.5, "it went through, {left} m short");
}

/// A creature's body passes no more than its description says: what is not marked holds it up,
/// scenery and what a world places alike, a chest among them.
/// Behaviour: server.creatures.pass-what-creatures-pass
#[test]
fn a_creature_is_still_held_up_by_what_is_not_marked() {
    let records = records();
    let mut w = walls();
    w.walkers.insert(HELD, walker_of(&records, HELD));
    let from = StoredPosition {
        cell: yalain_world::spawn::outdoor(0x6868, 15.75, 80.3).0,
        origin: [15.75, 80.3, 94.25],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    let left = left_of_a_walk(&mut w, HELD, from, 15.75, 85.3);
    assert!(left > 0.5, "held up at the scenery, {left} m short");

    let chest = yalain_testkit::authored::Portal::open().template(CHEST);
    let mut w = walls();
    w.set_standing(vec![Standing {
        id: 0x4000_0000_0000_0001,
        template: CHEST,
        look: chest.description.look,
        scale: 1.0,
        at: StoredPosition {
            cell: yalain_world::spawn::outdoor(0x6FA9, 45.24, 94.51).0,
            origin: [45.24, 94.51, 85.26],
            rotation: [1.0, 0.0, 0.0, 0.0],
        },
        ethereality: yalain_world::walls::ethereality_of(&chest.description),
    }]);
    let from = StoredPosition {
        cell: yalain_world::spawn::outdoor(0x6FA9, 45.24, 90.51).0,
        origin: [45.24, 90.51, 85.26],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    let left = left_of_a_walk(&mut w, DIGGER, from, 45.24, 94.51);
    assert!(left > 0.5, "held up at the chest, {left} m short");
}

/// A creature running straight at what it cannot climb stands where it met it, step after step;
/// aimed across it, it slides along.
/// Behaviour: server.creatures.held-up-not-put-back
#[test]
fn a_creature_held_up_stands_head_on_and_slides_along_when_aimed_across() {
    let mut w = walls();
    w.set_standing(vec![a_barrel(3.0)]);
    let start = StoredPosition {
        cell: yalain_world::spawn::outdoor(0x6FA9, 45.24, 90.51).0,
        origin: [45.24, 90.51, 85.26],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    let mut at = start;
    for _ in 0..12 {
        at = w.step(DIGGER, 1.0, &at, [0.0, 1.0]).expect("placed").at;
    }
    let held = at;
    assert!(held.origin[1] < 94.51, "short of the barrel: {held:?}");
    let s = w.step(DIGGER, 1.0, &held, [0.0, 1.0]).expect("placed");
    assert!(s.moved < 0.05, "it stands: {s:?}");
    let s = w.step(DIGGER, 1.0, &held, [0.5, 0.87]).expect("placed");
    assert!(
        s.moved > 0.2 && s.at.origin[0] > held.origin[0] + 0.15,
        "it slides along: {s:?}"
    );
}

/// A creature that stands in what a world places, and can move no way at all, is put where it fits
/// nearest and walks on from there.
/// Behaviour: server.creatures.stuck-in-something-put-out
#[test]
fn a_creature_standing_in_something_is_put_where_it_fits_and_walks_on() {
    let mut w = walls();
    w.set_standing(vec![a_barrel(3.0)]);
    let inside = StoredPosition {
        cell: yalain_world::spawn::outdoor(0x6FA9, 45.24, 94.51).0,
        origin: [45.24, 94.51, 85.26],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    let s = w.step(DIGGER, 1.0, &inside, [0.0, 0.5]).expect("placed");
    assert!(s.moved > 0.1, "it was put out of it: {s:?}");
    let next = w.step(DIGGER, 1.0, &s.at, [0.0, 0.5]).expect("placed");
    assert!(next.moved > 0.1, "it walks on: {next:?}");
}

/// A dungeon's room and a place on the other side of its rock: their cells are joined by doorways,
/// and no way short enough leads from one to the other, so a search goes on to its limit.
const IN_THE_ROOM: StoredPosition = StoredPosition {
    cell: 0x03E9_0226,
    origin: [-105.5, 17.4, 27.4],
    rotation: [1.0, 0.0, 0.0, 0.0],
};
const BEYOND_THE_ROCK: StoredPosition = StoredPosition {
    cell: 0x03E9_019D,
    origin: [-105.8, -4.4, 8.4],
    rotation: [1.0, 0.0, 0.0, 0.0],
};

/// Two places a long way apart on foot, near on a straight line: a search that finds its way, late.
const A_LONG_WAY_FROM: StoredPosition = StoredPosition {
    cell: 0x9AB5_0120,
    origin: [-13.6, 19.5, 110.8],
    rotation: [1.0, 0.0, 0.0, 0.0],
};
const A_LONG_WAY_TO: StoredPosition = StoredPosition {
    cell: 0x9AB5_0123,
    origin: [-31.309_17, 20.552_551, 103.896_774],
    rotation: [1.0, 0.0, 0.0, 0.0],
};

/// `n` creatures going after a player where no way leads: the harness, their ids.
fn held_after_a_player(n: u32) -> (Harness, Vec<u64>) {
    let mut h = Harness::new();
    h.add_creatures(n);
    let (_, player) = h.player_in_world("one", "Elara");
    let mut w = walls();
    let _ = w.joined(&IN_THE_ROOM, &IN_THE_ROOM);
    h.server.walls = Some(w);
    for p in h.server.players.values_mut() {
        p.position = BEYOND_THE_ROCK;
    }
    let ids: Vec<u64> = h
        .server
        .things
        .by_id
        .values()
        .filter(|t| t.template == TEST_CREATURE)
        .map(|t| t.id)
        .collect();
    assert_eq!(ids.len(), n as usize);
    let clock = h.clock();
    for (i, id) in ids.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let place = StoredPosition {
            origin: [
                IN_THE_ROOM.origin[0] + (i % 3) as f32 * 0.2,
                IN_THE_ROOM.origin[1] + (i % 4) as f32 * 0.2,
                IN_THE_ROOM.origin[2],
            ],
            ..IN_THE_ROOM
        };
        if let Some(t) = h.server.things.get_mut(*id) {
            t.at = Some(place);
            if let Some(c) = t.creature.as_mut() {
                c.home = place;
                c.stats.movement.perception = 400.0;
            }
        }
        yalain_world::ai::go_after(&mut h.server, &mut h.net, *id, player, clock);
    }
    (h, ids)
}

fn searched_nowhere(h: &Harness, id: u64) -> bool {
    h.server
        .things
        .get(id)
        .and_then(|t| t.creature.as_ref())
        .is_some_and(|c| c.way.none_found.is_some())
}

fn search_moves(h: &Harness) -> u64 {
    h.server.walls.as_ref().map_or(0, |w| w.search_moves)
}

/// The moves a turn of a search can take past what it was given: one lattice point's links.
const ONE_POINT_MOVES: u64 = 27;

/// However many creatures are held up, a pass of the world spends no more than its budget on their
/// searches, and that budget is reached: what would stall a world goes on over later passes.
/// Behaviour: server.creatures.way-searches-share-a-budget
#[test]
fn a_pass_of_the_world_spends_no_more_than_its_budget_on_way_searches() {
    let (mut h, _) = held_after_a_player(30);
    let mut most = 0;
    for _ in 0..200 {
        let before = search_moves(&h);
        h.step();
        most = most.max(search_moves(&h) - before);
    }
    let budget = yalain_world::walls::PASS_MOVES;
    assert!(most <= budget + ONE_POINT_MOVES, "{most} moves in a pass");
    assert!(most >= budget, "the budget was reached: {most} moves");
}

/// Each of a crowd of held creatures gets its answer, the last within the passes all their searches
/// take at the budget, though the player they are after keeps moving and so the first answered ask
/// again: those waiting take turns, and a new search goes behind them.
/// Behaviour: server.creatures.way-searches-share-a-budget
#[test]
fn every_held_creature_is_answered_however_many_are_waiting_and_asking_again() {
    let (mut h, ids) = held_after_a_player(30);
    let mut answered = std::collections::BTreeSet::new();
    let mut taken = None;
    for step in 0..4000u32 {
        h.step();
        // A meter back and forth every second: enough that no answer holds for long.
        if step % 20 == 0 {
            let side = if step % 40 == 0 { 1.0 } else { 0.0 };
            for p in h.server.players.values_mut() {
                p.position.origin[0] = BEYOND_THE_ROCK.origin[0] + side;
            }
        }
        answered.extend(ids.iter().copied().filter(|id| searched_nowhere(&h, *id)));
        if answered.len() == ids.len() {
            taken = Some(step);
            break;
        }
    }
    let taken = taken.unwrap_or_else(|| panic!("{} of {} answered", answered.len(), ids.len()));
    // No search takes more than this; all of them at the budget, a pass every five steps.
    let most_of_all = 30 * 6_000 / yalain_world::walls::PASS_MOVES;
    assert!(
        u64::from(taken) <= most_of_all * 5,
        "answered by step {taken}"
    );
}

/// Creatures all asking again the moment they are answered, more than the budget can serve, are
/// answered in turn: none is kept out by the others' asking.
/// Behaviour: server.creatures.way-searches-share-a-budget
#[test]
fn creatures_asking_again_at_once_are_answered_in_turn() {
    let mut w = walls();
    let mut answers = [0u32; 12];
    for _ in 0..200 {
        w.new_pass();
        for (id, n) in answers.iter_mut().enumerate() {
            let asked = w.ask(
                id as u64,
                DIGGER,
                1.0,
                &IN_THE_ROOM,
                &BEYOND_THE_ROCK,
                2.0,
                false,
            );
            *n += u32::from(asked != yalain_world::walls::Asked::Waiting);
        }
    }
    let (few, many) = (
        answers.iter().min().copied().unwrap_or(0),
        answers.iter().max().copied().unwrap_or(0),
    );
    assert!(few >= 1, "each is answered: {answers:?}");
    assert!(many - few <= 1, "in turn: {answers:?}");
}

/// A short search behind many long ones has its turn among them: it does not wait for them to end.
/// Behaviour: server.creatures.way-searches-share-a-budget
#[test]
fn a_short_search_behind_long_ones_does_not_wait_for_them_to_end() {
    let mut w = walls();
    let long =
        |w: &mut Walls, id: u64| w.ask(id, DIGGER, 1.0, &IN_THE_ROOM, &BEYOND_THE_ROCK, 2.0, false);
    // Somewhere it can walk straight to.
    let near = [[1.0, 0.0], [-1.0, 0.0], [0.0, 1.0], [0.0, -1.0]]
        .iter()
        .filter_map(|d: &[f32; 2]| w.step(DIGGER, 1.0, &IN_THE_ROOM, [d[0] * 3.0, d[1] * 3.0]))
        .find(|s| s.moved > 2.5)
        .expect("room to walk")
        .at;
    use yalain_world::walls::Asked;
    w.new_pass();
    for id in 0..10 {
        assert_eq!(long(&mut w, id), Asked::Waiting);
    }
    // The pass's budget is spent: this one waits too, and its turn comes.
    let short = |w: &mut Walls| w.ask(10, DIGGER, 1.0, &IN_THE_ROOM, &near, 0.5, false);
    assert_eq!(short(&mut w), Asked::Waiting);
    let mut passes = 0;
    while short(&mut w) == Asked::Waiting {
        w.new_pass();
        passes += 1;
        assert!(passes <= 3, "its turn comes within a few passes");
    }
    for id in 0..10 {
        assert_eq!(long(&mut w, id), Asked::Waiting, "the long ones go on");
    }
}

/// A creature whose search found no way does not search again while it and its goal stay where they
/// were, since the answer would be the same; when the goal moves it does (all within the five
/// seconds before a creature that neither sees nor can reach its player gives up).
/// Behaviour: server.creatures.way-searches-share-a-budget
#[test]
fn a_creature_with_no_way_searches_again_only_when_it_or_its_goal_moves() {
    let (mut h, ids) = held_after_a_player(1);
    let searches = |h: &Harness| h.server.walls.as_ref().map_or(0, |w| w.searches);
    for _ in 0..400 {
        h.step();
        if searched_nowhere(&h, ids[0]) {
            break;
        }
    }
    assert!(searched_nowhere(&h, ids[0]), "it finds no way");
    let found = searches(&h);
    h.run(20);
    assert_eq!(searches(&h), found, "nothing moved: no search more");
    for p in h.server.players.values_mut() {
        p.position.origin[0] += 1.0;
    }
    h.run(30);
    assert!(searches(&h) > found, "the goal moved: it searches again");
}

/// A way searched in turns over many passes is the way searched at once.
/// Behaviour: server.creatures.way-searches-share-a-budget
#[test]
fn a_way_searched_in_turns_is_the_way_searched_at_once() {
    let (from, goal) = (A_LONG_WAY_FROM, A_LONG_WAY_TO);
    let mut w = walls();
    let at_once = w.way(DIGGER, 1.0, &from, &goal, 2.0).expect("a way");
    assert!(w.search_moves > 512, "a long search: {}", w.search_moves);
    let mut passes = 0;
    let in_turns = loop {
        w.new_pass();
        passes += 1;
        match w.ask(1, DIGGER, 1.0, &from, &goal, 2.0, false) {
            yalain_world::walls::Asked::Waiting => {
                assert!(passes < 100, "it is answered");
            }
            yalain_world::walls::Asked::Way(places) => break places,
            yalain_world::walls::Asked::NoWay => panic!("a way leads there"),
        }
    };
    assert_eq!(in_turns, at_once);
    assert!(passes > 1, "it took turns");
}

/// A walk at rest takes no way that needs a long search: it can do without one.
/// Behaviour: server.creatures.way-searches-share-a-budget
#[test]
fn a_walk_at_rest_gives_up_on_a_way_that_takes_a_long_search() {
    let (from, goal) = (A_LONG_WAY_FROM, A_LONG_WAY_TO);
    let mut w = walls();
    let most = yalain_world::walls::REST_WALK_MOVES;
    assert!(w.way(DIGGER, 1.0, &from, &goal, 2.0).is_some());
    let long = w.search_moves;
    assert!(long > most, "the search takes {long} moves");
    let before = w.search_moves;
    assert_eq!(w.way_within(DIGGER, 1.0, &from, &goal, 2.0, most), None);
    assert!(w.search_moves - before <= most + ONE_POINT_MOVES);
}

/// A creature held up on a way it was told asks again with care: that search tries a walk's short
/// legs, which take more moves, where the first took a search's longer ones.
/// Behaviour: server.creatures.way-searches-share-a-budget
#[test]
fn a_careful_search_tries_a_walks_short_legs() {
    let (from, goal) = (A_LONG_WAY_FROM, A_LONG_WAY_TO);
    let mut w = walls();
    let mut moves_of = |careful: bool| {
        let before = w.search_moves;
        for _ in 0..100 {
            w.new_pass();
            let asked = w.ask(1, DIGGER, 1.0, &from, &goal, 2.0, careful);
            if asked != yalain_world::walls::Asked::Waiting {
                assert!(matches!(asked, yalain_world::walls::Asked::Way(_)));
                return w.search_moves - before;
            }
        }
        panic!("it is answered");
    };
    let (quick, careful) = (moves_of(false), moves_of(true));
    assert!(careful > quick * 2, "{careful} moves against {quick}");
}

/// How far a creature stands from a player goes with its body: its largest sphere at its look's
/// scale and its template's, a human's for a body not known.
/// Behaviour: server.creatures.stops-by-its-body
#[test]
fn a_bodys_radius_is_its_largest_sphere_at_its_looks_scale_and_its_own() {
    let w = walls();
    let digger = w.walkers.get(&DIGGER).expect("the Digger's body").clone();
    let largest = digger.spheres.iter().map(|s| s[3]).fold(0.0, f32::max);
    assert!(largest > 0.0);
    let radius = yalain_world::walls::body_radius(Some(&w), DIGGER, 2.0);
    assert!(
        (radius - largest * digger.scale * 2.0).abs() < 1e-5,
        "{radius}"
    );
    let unknown = yalain_world::walls::body_radius(Some(&w), 0x4700_0000, 2.0);
    assert!((unknown - 0.8).abs() < 1e-5, "{unknown}");
    assert!((yalain_world::walls::body_radius(None, DIGGER, 1.0) - 0.4).abs() < 1e-5);
}

/// A wasp, which flies.
const WASP: u32 = 0x4700_2966;

/// Walls that know the wasp's body.
fn wasp_walls() -> Walls {
    let records = records();
    let wasp = walker_of(&records, WASP);
    let mut w = walls();
    w.walkers.insert(WASP, wasp);
    w
}

/// A place where wasps are made, over the floor of a room.
fn over_a_floor() -> StoredPosition {
    StoredPosition {
        cell: 0x02A0_0103,
        origin: [0.0, -45.0, 102.1],
        rotation: [1.0, 0.0, 0.0, 0.0],
    }
}

/// A body says it flies by its motion description: two meters up at rest, one in a fight. One that
/// walks says nothing.
/// Behaviour: server.creatures.flies-as-its-body-says
#[test]
fn a_bodys_motion_description_says_how_high_it_flies() {
    let records = records();
    let hover = walker_of(&records, WASP).flies.expect("a wasp flies");
    assert_eq!((hover.at_rest, hover.fighting), (2.0, 1.0));
    assert_eq!(walker_of(&records, DIGGER).flies, None);
}

/// With no floor touching it a flyer still moves, and keeps its height over what is under it: lower
/// in a fight.
/// Behaviour: server.creatures.flies-at-its-height
#[test]
fn a_flyer_moves_where_no_floor_touches_it_and_keeps_its_height() {
    let mut w = wasp_walls();
    let from = over_a_floor();
    let rest = w.step(WASP, 1.0, &from, [0.0, 1.25]).expect("a step");
    assert!(rest.moved > 1.0, "it moved {} m", rest.moved);
    let fight = w
        .step_fighting(WASP, 1.0, &from, [0.0, 1.25])
        .expect("a step");
    assert!(fight.moved > 1.0, "it moved {} m", fight.moved);
    let lower = rest.at.origin[2] - fight.at.origin[2];
    assert!((lower - 1.0).abs() < 0.05, "{lower} m lower in a fight");
    // And a walker of the same size, set down there, comes to the floor those heights are over.
    let mut on_it = rest.at;
    on_it.origin[2] -= 2.0;
    let walked = w.step(DIGGER, 1.0, &on_it, [0.0, 0.5]).expect("a step");
    assert!(walked.moved > 0.4, "the floor is {} m under", 2.0);
    assert!(
        (walked.at.origin[2] - on_it.origin[2]).abs() < 0.3,
        "floor at {}, wasp at {}",
        walked.at.origin[2],
        rest.at.origin[2]
    );
}

/// A flyer meets walls as a walker does: however it goes, it ends in the room it names. Past a
/// floor's edge with no wall over it, too.
/// Behaviour: server.creatures.flyers-meet-walls
#[test]
fn a_flyer_never_goes_through_a_wall() {
    let at_an_edge = StoredPosition {
        cell: 0x0499_0138,
        origin: [118.9, 136.6, 102.23],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    let mut w = wasp_walls();
    for from in [over_a_floor(), at_an_edge] {
        for k in 0u8..8 {
            let a = f32::from(k) * core::f32::consts::FRAC_PI_4;
            let mut at = from;
            let mut went = 0.0;
            for _ in 0..80 {
                let d = [math::sinf(a) * 1.25, math::cosf(a) * 1.25];
                let s = w.step(WASP, 1.0, &at, d).expect("a step");
                went += s.moved;
                at = s.at;
                assert!(inside(&w, &at), "{k}: out of its cell at {at:?}");
            }
            assert!(went < 80.0, "{k}: nothing held it, {went} m");
        }
    }
}

/// A flyer after a player in the next room goes through the doorway, as a walker does.
/// Behaviour: server.creatures.flyers-through-doorways
#[test]
fn a_flyer_after_a_player_in_the_next_room_flies_through_the_doorway() {
    let start = hall(0x010C, 16.0, 428.0, -43.1);
    let there = hall(0x010D, 6.0, 431.0, -43.5);
    let (mut h, _, id) = beetle_at(start);
    let records = records();
    let wasp = walker_of(&records, WASP);
    let walls = h.server.walls.as_mut().expect("the walls");
    walls.walkers.insert(TEST_CREATURE, wasp);
    let _ = walls.joined(&start, &there);
    for p in h.server.players.values_mut() {
        p.position = there;
    }
    let me = h
        .server
        .players
        .values()
        .next()
        .expect("a player")
        .character;
    let clock = h.clock();
    yalain_world::ai::go_after(&mut h.server, &mut h.net, id, me, clock);
    let reached = h.run_until(160, |h| {
        let at = place_of(h, id);
        let (dx, dy) = (
            at.origin[0] - there.origin[0],
            at.origin[1] - there.origin[1],
        );
        at.cell == there.cell && (dx * dx + dy * dy).sqrt() <= yalain_world::ai::MELEE_REACH
    });
    assert!(reached, "it reached the player: {:?}", place_of(&h, id));
}

/// A flyer appearing on open ground appears at its height.
/// Behaviour: server.creatures.flies-from-where-it-appears
#[test]
fn a_flyer_of_a_camp_on_open_ground_appears_at_its_height() {
    let mut h = Harness::new();
    h.add_creatures(1);
    let camp = &mut h.server.spawns.camps[0];
    camp.spots.clear();
    camp.radius = 3.0;
    let ground = camp.origin[2];
    let records = records();
    let mut w = walls();
    w.walkers.insert(TEST_CREATURE, walker_of(&records, WASP));
    h.server.walls = Some(w);
    h.player_in_world("one", "Elara");
    let at = h
        .server
        .things
        .by_id
        .values()
        .find(|t| t.template == TEST_CREATURE)
        .and_then(|t| t.at)
        .expect("the camp's creature");
    assert!((at.origin[2] - (ground + 2.0)).abs() < 0.01, "{at:?}");
}

/// A flyer after a player goes at its fighting height.
/// Behaviour: server.creatures.flies-lower-in-a-fight
#[test]
fn a_flyer_after_a_player_comes_down_to_its_fighting_height() {
    let home = over_a_floor();
    let (mut h, _, id) = beetle_at(home);
    let records = records();
    let wasp = walker_of(&records, WASP);
    h.server
        .walls
        .as_mut()
        .expect("the walls")
        .walkers
        .insert(TEST_CREATURE, wasp);
    let clock = h.clock();
    let me = h
        .server
        .players
        .values()
        .next()
        .expect("a player")
        .character;
    let mut far = home;
    far.origin[1] -= 12.0;
    for p in h.server.players.values_mut() {
        p.position = far;
    }
    yalain_world::ai::go_after(&mut h.server, &mut h.net, id, me, clock);
    h.run(25);
    let at = h.server.things.get(id).and_then(|t| t.at).expect("placed");
    let rest = h
        .server
        .walls
        .as_mut()
        .expect("the walls")
        .step(TEST_CREATURE, 1.0, &at, [0.0, 0.1])
        .expect("a step");
    let lower = rest.at.origin[2] - at.origin[2];
    assert!(
        (lower - 1.0).abs() < 0.05,
        "{lower} m under its height at rest"
    );
}

/// The Pulsing Bloodstone, which walks.
const BLOODSTONE: u32 = 0x4700_2262;

/// Where 2017 made the Bloodstones of a camp: 2 m over the floor of their room.
const OVER_THE_FLOOR: StoredPosition = StoredPosition {
    cell: 0x09F7_0168,
    origin: [70.09, -54.15, 137.78],
    rotation: [1.0, 0.0, 0.0, 0.0],
};

/// Where 2017 told one of them as it walked from there, through two rooms: across, and its height.
const WALKED: [(f32, f32, f32); 13] = [
    (71.57, -58.18, 136.11),
    (67.65, -60.85, 137.01),
    (62.56, -61.46, 139.54),
    (58.19, -62.17, 140.72),
    (54.09, -62.89, 141.17),
    (50.47, -60.36, 141.72),
    (49.86, -55.08, 144.07),
    (50.62, -50.58, 145.55),
    (50.63, -45.78, 145.72),
    (51.07, -42.46, 145.59),
    (50.11, -35.00, 145.63),
    (49.15, -30.04, 145.68),
    (48.24, -26.03, 145.65),
];

/// A creature set down over its floor, further than its step goes down, comes down to the floor
/// and walks from there: from where 2017 made the Bloodstones, one walks the way 2017's walked,
/// each place it was told at reached at the height it was told at.
/// Behaviour: server.creatures.falls-to-the-floor
#[test]
fn a_creature_made_over_its_floor_comes_down_and_walks_where_2017s_walked() {
    let records = records();
    let mut w = walls();
    w.walkers
        .insert(BLOODSTONE, walker_of(&records, BLOODSTONE));
    let scale = yalain_content::CreatureStats::from_props(
        &yalain_testkit::authored::Portal::open()
            .template(BLOODSTONE)
            .props,
    )
    .expect("a creature")
    .scale;
    // Its first step comes down to the floor 2 m under it.
    let first = w
        .step(BLOODSTONE, scale, &OVER_THE_FLOOR, [0.0, 0.5])
        .expect("placed");
    assert!(
        first.moved > 0.4 && first.moved < 0.6,
        "it stepped, coming straight down: {first:?}"
    );
    let floor = first.at.origin[2];
    assert!((floor - 135.79).abs() < 0.2, "on the floor at {floor}");
    let mut at = first.at;
    for (k, &(x, y, z)) in WALKED.iter().enumerate() {
        for _ in 0..40 {
            let (dx, dy) = (x - at.origin[0], y - at.origin[1]);
            let gap = (dx * dx + dy * dy).sqrt();
            if gap < 0.5 {
                break;
            }
            let go = gap.min(1.0) / gap;
            at = w
                .step(BLOODSTONE, scale, &at, [dx * go, dy * go])
                .expect("placed")
                .at;
        }
        let (dx, dy) = (x - at.origin[0], y - at.origin[1]);
        assert!(
            (dx * dx + dy * dy).sqrt() < 0.5,
            "{k}: held up at {at:?} short of ({x}, {y})"
        );
        assert!(
            (at.origin[2] - z).abs() < 0.5,
            "{k}: at {at:?}, told at {z}"
        );
    }
}

/// Made where 2017 made the Bloodstones, one after a player in the next room comes down and runs to
/// it through the doorway.
/// Behaviour: server.creatures.falls-to-the-floor
#[test]
fn a_creature_made_over_its_floor_goes_after_a_player_in_the_next_room() {
    let there = StoredPosition {
        cell: 0x09F7_016D,
        origin: [50.63, -45.78, 145.72],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    let (mut h, _, id) = beetle_at(OVER_THE_FLOOR);
    let records = records();
    let walls = h.server.walls.as_mut().expect("the walls");
    walls
        .walkers
        .insert(TEST_CREATURE, walker_of(&records, BLOODSTONE));
    let _ = walls.joined(&OVER_THE_FLOOR, &there);
    for p in h.server.players.values_mut() {
        p.position = there;
    }
    // The player is 21 m off: it keeps after one that far.
    if let Some(c) = h
        .server
        .things
        .get_mut(id)
        .and_then(|t| t.creature.as_mut())
    {
        c.stats.movement.perception = 40.0;
    }
    let me = h
        .server
        .players
        .values()
        .next()
        .expect("a player")
        .character;
    let clock = h.clock();
    yalain_world::ai::go_after(&mut h.server, &mut h.net, id, me, clock);
    let reached = h.run_until(300, |h| {
        let at = place_of(h, id);
        let (dx, dy) = (
            at.origin[0] - there.origin[0],
            at.origin[1] - there.origin[1],
        );
        at.cell == there.cell && (dx * dx + dy * dy).sqrt() <= yalain_world::ai::MELEE_REACH
    });
    assert!(reached, "it reached the player: {:?}", place_of(&h, id));
}

/// Outdoors too a creature set down over the ground comes down to it, and never under it.
/// Behaviour: server.creatures.falls-to-the-floor
#[test]
fn a_creature_set_down_over_open_ground_comes_down_to_it() {
    let mut w = walls();
    let (cell, x, y) = yalain_world::spawn::outdoor(0x6A68, 55.22, 39.10);
    let mut over = StoredPosition {
        cell,
        origin: [x, y, 190.38],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    // A step reads the block.
    let _ = w.step(DIGGER, 1.0, &over, [0.0, 0.5]).expect("placed");
    let ground = |w: &Walls, at: &StoredPosition| {
        let p = dereth_primitives::Vec3::new(at.origin[0], at.origin[1], at.origin[2]);
        w.land()
            .ground(dereth_primitives::CellId(at.cell).landblock(), p)
            .expect("the ground there")
            .0
    };
    over.origin[2] = ground(&w, &over) + 3.0;
    let s = w.step(DIGGER, 1.0, &over, [0.0, 0.5]).expect("placed");
    assert!(s.moved > 0.4 && s.moved < 0.6, "it stepped: {s:?}");
    let under = ground(&w, &s.at);
    assert!(
        (s.at.origin[2] - under).abs() < 0.1,
        "{s:?} over ground at {under}"
    );
}

/// The place a practice gem may be used has no rooms: no landblock within its 100 meters holds an
/// interior cell, so what a gem makes stands outdoors.
/// Behaviour: server.creatures.a-gems-dummy-lasts-its-time
#[test]
fn the_place_a_practice_gem_may_be_used_has_no_rooms() {
    let records = records();
    let hall = records
        .landblock_info(0x02, 0x98)
        .expect("the training hall");
    assert!(hall.cell_count > 0, "it has rooms");
    // The place is at (62, 40) of landblock C6A2, a landblock being 192 m on a side.
    for (x, y) in [
        (0xC5, 0xA1),
        (0xC5, 0xA2),
        (0xC6, 0xA1),
        (0xC6, 0xA2),
        (0xC7, 0xA1),
        (0xC7, 0xA2),
    ] {
        let cells = records.landblock_info(x, y).map_or(0, |i| i.cell_count);
        assert_eq!(cells, 0, "{x:02X}{y:02X}");
    }
}

/// A camp's creature made at a spot whose point lies outside the cell the spot names is put in the
/// cell that holds the point, one the named cell sees, as anything put somewhere is: in block
/// `00A7`, a spot naming room `0106` at `(-64.5, -33.5, -22.9)`, a point room `010C` holds, makes its
/// creature in room `010C`. A spot inside the room it names keeps it.
///
/// Behaviour: server.creatures.made-in-the-cell-that-holds-them
#[test]
fn a_camps_creature_is_made_in_the_cell_that_holds_its_spot() {
    let room = |low: u32| 0xA7 << 16 | low;
    for (named, origin, want) in [
        (room(0x0106), [-64.5, -33.5, -22.9], room(0x010C)),
        (room(0x010C), [-64.5, -33.5, -22.9], room(0x010C)),
    ] {
        let mut h = Harness::new();
        h.add_creatures(1);
        h.server.spawns.camps[0].spots = vec![(named, origin)];
        h.server.walls = Some(walls());
        let now = h.now.0;
        let id = yalain_world::spawn::spawn(&mut h.server, 0, now).expect("made");
        let at = place_of(&h, id);
        assert_eq!(
            at.cell, want,
            "{named:08X}: made in {:08X}, not {want:08X}",
            at.cell
        );
        assert_eq!(at.origin, origin, "at the spot");
        let w = h.server.walls.as_ref().expect("the walls");
        assert!(inside(w, &at), "{named:08X}: the cell holds it");
    }
}

// A player's body following what its client reports.

/// The human male's setup, whose body players are given.
const HUMAN_SETUP: u32 = 0x0400_0029;

const PLAYER: u64 = 0x2130_0000_0000_0001;

/// The walls, players given the human's body.
fn player_walls() -> Walls {
    let mut w = walls();
    w.player_walker = setup_walker(&records(), HUMAN_SETUP, 1.0).expect("the human's body");
    w
}

/// A place on the ground of landblock 0x6FA9.
fn on_ground(w: &mut Walls, x: f32, y: f32) -> StoredPosition {
    let (cell, x, y) = yalain_world::spawn::outdoor(0x6FA9, x, y);
    w.read_place(cell);
    let z = w
        .land()
        .ground(CellId(cell).landblock(), Vec3::new(x, y, 0.0))
        .expect("the ground")
        .0;
    StoredPosition {
        cell,
        origin: [x, y, z],
        rotation: [1.0, 0.0, 0.0, 0.0],
    }
}

/// A body following its client's reports north over the open ground, one every 0.7 m, reaches each
/// and stands exactly where it was reported.
/// Behaviour: server.places.a-body-follows-its-reports
#[test]
fn a_players_body_reaches_each_report_over_open_ground_and_stands_there() {
    let mut w = player_walls();
    let start = on_ground(&mut w, 45.24, 90.51);
    w.place_player(PLAYER, &start);
    for k in 1..=10u8 {
        let to = on_ground(&mut w, 45.24, 90.51 + 0.7 * f32::from(k));
        let f = w.follow_player(PLAYER, &to).expect("judged");
        assert!(f.reached, "report {k}: {to:?}, the body at {:?}", f.at);
        assert_eq!(
            w.player_at(PLAYER).map(|p| (p.cell, p.origin)),
            Some((to.cell, to.origin))
        );
    }
}

/// Reports going on north through the building leave the body held up at it: from there on it
/// reaches none, and stands south of where the reports went.
/// Behaviour: server.places.a-body-is-held-up-where-reports-go-through
#[test]
fn a_players_body_is_held_up_at_a_building_its_reports_go_through() {
    let mut w = player_walls();
    let start = on_ground(&mut w, 45.24, 90.51);
    w.place_player(PLAYER, &start);
    let mut missed = 0;
    let mut last = start;
    for k in 1..=60u8 {
        last = on_ground(&mut w, 45.24, 90.51 + 1.0 * f32::from(k));
        let f = w.follow_player(PLAYER, &last).expect("judged");
        if !f.reached {
            missed += 1;
        } else {
            assert_eq!(missed, 0, "a report reached after one missed, at {k}");
        }
    }
    assert!(missed > 0, "the building held it up");
    let at = w.player_at(PLAYER).expect("a body");
    assert!(
        at.cell >> 16 == 0x6FA9 && at.origin[1] < last.origin[1] - 1.0,
        "south of the last report: {at:?} against {last:?}"
    );
}

/// A report in the next room, the hall's rock between, is not reached: the body stays in its room,
/// inside it.
/// Behaviour: server.places.a-body-is-held-up-where-reports-go-through
#[test]
fn a_players_body_does_not_go_through_rock_into_the_next_room() {
    let mut w = player_walls();
    let start = hall(0x010C, 16.0, 428.0, -43.1);
    let there = hall(0x010D, 6.0, 431.0, -43.5);
    w.place_player(PLAYER, &start);
    let f = w.follow_player(PLAYER, &there).expect("judged");
    assert!(!f.reached, "through the rock: {:?}", f.at);
    assert_ne!(f.at.cell, there.cell, "still in its room");
    let cell = w.land().cell(CellId(f.at.cell)).expect("a room");
    let p = Vec3::new(f.at.origin[0], f.at.origin[1], f.at.origin[2] + 0.5);
    assert!(
        cell.cell_bsp
            .as_ref()
            .is_some_and(|t| t.point_inside_cell_bsp(point_to_local(&cell.frame, p))),
        "inside the room it names: {:?}",
        f.at
    );
}

/// A report further than a body is carried is not followed: no move is tried, and the body stays.
/// One in a cell the walls hold no record of is not judged at all.
/// Behaviour: server.places.a-body-follows-its-reports
#[test]
fn a_report_too_far_is_not_followed_and_one_in_no_known_cell_is_not_judged() {
    let mut w = player_walls();
    let start = on_ground(&mut w, 45.24, 90.51);
    w.place_player(PLAYER, &start);
    let far = on_ground(&mut w, 45.24, 90.51 + FOLLOWS_FARTHEST + 5.0);
    let f = w.follow_player(PLAYER, &far).expect("judged");
    assert!(!f.reached);
    assert_eq!(f.moves, 0);
    assert_eq!(w.player_at(PLAYER).map(|p| p.origin), Some(start.origin));
    let nowhere = hall(0xFFF0, 16.0, 428.0, -43.1);
    assert_eq!(w.follow_player(PLAYER, &nowhere), None);
    assert_eq!(w.follow_player(PLAYER + 1, &start), None, "no body yet");
}

// A player's reports on the real path: its body follows them through the walls and doors.

const DOOR: u64 = 0x4002_9900_0000_0071;
const DOOR_TEMPLATE: u32 = 0x4700_0FCE;
const DOOR_PACKAGE: u32 = 0x0EB2;
const USE: u32 = 0x8216_0016;

/// A use of an item (on an object, or on nothing), as the client asks for it.
fn use_of(item: u64, me: u64, on: u64) -> auberean_script::args::Arg {
    use yalain_world::graph::{Builder, Slot};
    let mut b = Builder::new();
    b.object(
        0x245,
        &[
            Slot::Ref(None),
            Slot::Ref(None),
            Slot::Long(item),
            Slot::Ref(None),
            Slot::Int(0),
            Slot::Long(me),
            Slot::Int((-1.0f32).to_bits()),
            Slot::Long(on),
            Slot::Int(0),
            Slot::Long(0),
            Slot::Int(0),
            Slot::Ref(None),
            Slot::Int(0),
            Slot::Int(316),
            Slot::Int(0),
            Slot::Int(0),
        ],
    );
    auberean_script::args::Arg::Object(b.graph())
}

/// The harness holding reports, with the walls and, when `door` says where across the way north
/// from the start, a plain door there facing north; one player standing at the start on the open
/// ground of 0x6FA9, at force stamp 0.
fn held_player(door: Option<f32>) -> (Harness, usize, u64, yalain_net::ConnId, StoredPosition) {
    let mut h = Harness::new();
    h.server.cfg.holds_reports = true;
    h.server.catalog = yalain_testkit::test_catalog();
    let mut w = player_walls();
    let start = on_ground(&mut w, 45.24, 90.51);
    if let Some(y) = door {
        let at = on_ground(&mut w, 45.24, y);
        h.server.content = yalain_content::ContentPack {
            objects: vec![yalain_content::ContentObject {
                object: Some(DOOR),
                template: DOOR_TEMPLATE,
                description: yalain_content::Description {
                    name_text: None,
                    package: DOOR_PACKAGE,
                    look: 0x1F00_0FCE,
                    enter_fx: 0,
                    scale: None,
                    name: Some((0x1C06_0001, 0x2500_0001)),
                    numbers: [0; 6],
                    source: "authored for a test".into(),
                },
                cell: at.cell,
                origin: at.origin,
                rotation: [1.0, 0.0, 0.0, 0.0],
                kind: yalain_content::Kind::Static,
                label: String::new(),
                placed: "authored for a test".into(),
                destination: None,
                facts: yalain_content::Facts::default(),
                walks: None,
                gate: None,
            }],
            ..yalain_content::ContentPack::default()
        };
        h.server
            .doors
            .packages
            .insert(DOOR_PACKAGE, yalain_world::usable::doors::Kind::Plain);
    }
    h.server.walls = Some(w);
    h.server.stand_placed();
    let (a, me) = h.player_in_world("one", "Elara");
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in the world")
        .0;
    if let Some(p) = h.server.players.get_mut(&world) {
        p.position = start;
    }
    h.run(10);
    (h, a, me, world, start)
}

fn run_for(h: &mut Harness, seconds: f64) {
    let t = h.now.0 + seconds;
    while h.now.0 < t {
        h.step();
    }
}

/// Report a place on the ground of 0x6FA9 at the force stamp given, and let a quarter second pass.
fn report_at(h: &mut Harness, a: usize, y: f32, force: u16) -> StoredPosition {
    report_at_x(h, a, 45.24, y, force)
}

/// Report a place on the ground of 0x6FA9, as [`report_at`], at `x` across.
fn report_at_x(h: &mut Harness, a: usize, x: f32, y: f32, force: u16) -> StoredPosition {
    let to = {
        let w = h.server.walls.as_mut().expect("the walls");
        on_ground(w, x, y)
    };
    let mut m = yalain_testkit::play::report(
        to.cell,
        auberean_primitives::Vec3::new(to.origin[0], to.origin[1], to.origin[2]),
        0,
        yalain_world::world::IDLE_MOTION,
        0,
    );
    if let auberean_protocol::ClientMessage::Position(p) = &mut m {
        p.force_position_stamp = force;
    }
    h.bots[a].send(true, auberean_protocol::queue::WEENIE, &m);
    run_for(h, 0.25);
    to
}

/// The places the player's client was told with a new force stamp since `from`.
fn told_back(h: &Harness, a: usize, me: u64, from: usize) -> Vec<(u16, [f32; 3])> {
    h.world_messages(a, from)
        .iter()
        .filter_map(|m| match m {
            auberean_protocol::ServerMessage::Position(p) if p.sender.id == me => {
                Some((p.force_position_stamp, [p.origin.x, p.origin.y, p.origin.z]))
            }
            _ => None,
        })
        .filter(|(f, _)| *f > 0)
        .collect()
}

/// A player whose reports go on north through the building: the server takes them up to the
/// building and none past it, the player standing where its body was held up (sliding along the
/// wall), and two seconds after
/// the first report not reached its client is told that place; from there its reports are taken
/// again.
/// Behaviour: server.places.a-body-behind-is-told-back-after-two-seconds
#[test]
fn a_player_reporting_through_a_building_is_held_at_it_and_told_back_after_two_seconds() {
    let (mut h, a, me, world, _) = held_player(None);
    let from = h.bots[a].inbox.len();
    let mut first_miss = None;
    for k in 1..=70u8 {
        let y = 90.51 + f32::from(k);
        let to = report_at(&mut h, a, y, 0);
        let at = h.server.players[&world].position;
        if at.origin != to.origin && first_miss.is_none() {
            first_miss = Some(h.now.0);
        }
        if first_miss.is_some() {
            assert_ne!(at.origin, to.origin, "no report past the building taken");
        }
        if h.server.players[&world].force_position_stamp > 0 {
            break;
        }
    }
    let missed_at = first_miss.expect("the building held the body up");
    let told = told_back(&h, a, me, from);
    let p = &h.server.players[&world];
    assert_eq!(p.force_position_stamp, 1, "told back once");
    assert_eq!(told.len(), 1, "{told:?}");
    assert_eq!(told[0].1, p.position.origin, "told where its body stands");
    let waited = h.now.0 - missed_at;
    assert!(
        (2.0..2.6).contains(&waited),
        "two seconds after the first miss: {waited}"
    );
    // From the place told back, a report back south is taken.
    let (x, back) = (p.position.origin[0], p.position.origin[1] - 1.0);
    let to = report_at_x(&mut h, a, x, back, 1);
    assert_eq!(h.server.players[&world].position.origin, to.origin);
}

/// A client that goes quiet after a report its body could not reach is told where it stands two
/// seconds on all the same, by the world's own step.
/// Behaviour: server.places.a-body-behind-is-told-back-after-two-seconds
#[test]
fn a_silent_client_behind_is_told_back_when_its_two_seconds_are_up() {
    let (mut h, a, me, world, start) = held_player(None);
    let from = h.bots[a].inbox.len();
    // Straight into the building, further than a step but within reach.
    let _ = report_at(&mut h, a, 90.51 + 45.0, 0);
    let held = h.server.players[&world].position;
    assert!(
        held.origin[1] < start.origin[1] + 45.0 - 1.0,
        "held short: {held:?}"
    );
    assert_eq!(h.server.players[&world].force_position_stamp, 0);
    run_for(&mut h, 1.5);
    assert_eq!(h.server.players[&world].force_position_stamp, 0, "not yet");
    run_for(&mut h, 0.75);
    assert_eq!(h.server.players[&world].force_position_stamp, 1);
    assert_eq!(told_back(&h, a, me, from).len(), 1);
}

/// Reports that come late and bunched along the open ground are each reached, however long the
/// client was quiet before them: no one is told back.
/// Behaviour: server.places.a-body-follows-its-reports
#[test]
fn reports_late_and_bunched_over_open_ground_are_all_taken() {
    let (mut h, a, _, world, _) = held_player(None);
    for k in 1..=6u8 {
        let to = report_at(&mut h, a, 90.51 + 0.7 * f32::from(k), 0);
        assert_eq!(h.server.players[&world].position.origin, to.origin);
    }
    run_for(&mut h, 3.0);
    for k in 7..=12u8 {
        // Three at once, then the next.
        let y = 90.51 + 0.7 * f32::from(k);
        let to = report_at(&mut h, a, y, 0);
        assert_eq!(h.server.players[&world].position.origin, to.origin);
    }
    assert_eq!(h.server.players[&world].force_position_stamp, 0);
}

/// A closed door across the way holds the body on its near side, and the client is told back
/// there; once the player has opened it the same way north is taken.
/// Behaviour: server.places.a-closed-door-holds-a-players-body
#[test]
fn a_closed_door_holds_a_players_body_and_an_open_one_lets_it_through() {
    let door_y = 93.0;
    let (mut h, a, me, world, _) = held_player(Some(door_y));
    // Through the closed door: held on the near side, then told back.
    let mut y = 90.51;
    while y < door_y + 3.0 {
        y += 0.7;
        let _ = report_at(&mut h, a, y, 0);
        assert!(
            h.server.players[&world].position.origin[1] < door_y,
            "on the near side: {:?}",
            h.server.players[&world].position
        );
    }
    run_for(&mut h, 2.5);
    assert_eq!(
        h.server.players[&world].force_position_stamp, 1,
        "told back"
    );
    // Opened from where it stands, the door lets the next reports through.
    h.bots[a].send(
        true,
        auberean_protocol::queue::WEENIE,
        &yalain_testkit::play::server_call(USE, &[use_of(DOOR, me, 0)]),
    );
    run_for(&mut h, 1.0);
    let mut y = h.server.players[&world].position.origin[1];
    while y < door_y + 3.0 {
        y += 0.7;
        let to = report_at(&mut h, a, y, 1);
        assert_eq!(
            h.server.players[&world].position.origin, to.origin,
            "through the open door"
        );
    }
}

/// The Drudge Outcast, the Arwic mines' creature.
const OUTCAST: u32 = 0x4700_2399;

/// A place in the Arwic mines (landblock 0x01AC).
fn mines(cell: u32, x: f32, y: f32, z: f32) -> StoredPosition {
    StoredPosition {
        cell: 0x01AC_0000 | cell,
        origin: [x, y, z],
        rotation: [1.0, 0.0, 0.0, 0.0],
    }
}

/// Where the Outcast is made, and places of the mines around it, each within its perception (10 m)
/// but the last: one in plain sight, one round the tunnel's corner with the rock between them and a
/// way to it, and one far off at the mines' other end (the first and last places the world's camps
/// make Outcasts at).
fn outcast_home() -> StoredPosition {
    mines(0x101, -3.7, -7.8, 1.0)
}
fn in_sight() -> StoredPosition {
    mines(0x101, -7.5, -7.8, 1.0)
}
fn beyond_the_rock() -> StoredPosition {
    mines(0x100, 1.3, -1.8, 1.0)
}
fn far_off() -> StoredPosition {
    mines(0x1C7, -9.0, 117.8, -26.9)
}

/// The harness with the walls, one Drudge Outcast made at its home in the mines, and a player
/// standing at `at` with its scene drawn; returns the harness, the client, the player and the
/// Outcast.
fn outcast_and_player(at: StoredPosition) -> (Harness, usize, u64, u64) {
    let records = records();
    let p = yalain_testkit::authored::Portal::open();
    let mut h = Harness::new();
    let mut catalog = yalain_testkit::test_catalog();
    catalog.insert(p.template(OUTCAST));
    h.server.catalog = catalog;
    h.server.loot = yalain_testkit::test_loot();
    h.server.skill_table = p.skills();
    let home = outcast_home();
    h.server.spawns.camps.push(yalain_content::Camp {
        id: "outcasts".into(),
        cell: home.cell,
        origin: home.origin,
        radius: 0.0,
        population: 1,
        regen: 600.0,
        templates: vec![(OUTCAST, 1)],
        spots: vec![(home.cell, home.origin)],
        placed: "a place of the world's camps".into(),
        confidence: "authored".into(),
        moves: yalain_content::CampMoves::default(),
        name: None,
    });
    h.server
        .camps
        .push(yalain_world::spawn::CampState::default());
    let mut w = walls();
    w.walkers.insert(OUTCAST, walker_of(&records, OUTCAST));
    let _ = w.joined(&home, &at);
    h.server.walls = Some(w);
    let (b, me) = h.player_in_world("one", "Elara");
    stand(&mut h, me, at);
    h.bots[b].send(
        true,
        auberean_protocol::queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    let outcast = h
        .server
        .things
        .by_id
        .values()
        .find(|t| t.template == OUTCAST)
        .map(|t| t.id)
        .expect("the Outcast is made");
    (h, b, me, outcast)
}

/// Put the player at a place, as the server would take it.
fn stand(h: &mut Harness, me: u64, at: StoredPosition) {
    for p in h.server.players.values_mut().filter(|p| p.character == me) {
        p.position = at;
    }
}

/// The attack stories told against the player since `from`, each with whether the walls stood
/// between the attacker and the player as it was told.
fn attacks_seen(h: &mut Harness, b: usize, from: usize, id: u64, me: u64, told: &mut Vec<bool>) {
    let n = h
        .world_messages(b, from)
        .iter()
        .filter(|m| {
            matches!(m, auberean_protocol::ServerMessage::DoStory(e)
                if e.sender.id == id && e.action.hooks.iter().any(|k| k.target == me))
        })
        .count();
    let at = place_of(h, id);
    let player = h
        .server
        .players
        .values()
        .find(|p| p.character == me)
        .map(|p| p.position)
        .expect("in the world");
    let clear = h
        .server
        .walls
        .as_mut()
        .expect("the walls")
        .sees(&at, &player);
    while told.len() < n {
        told.push(clear);
    }
}

/// Drudge Outcasts in the Arwic mines attack only a player they see. One attacks a player in
/// plain sight; with the player beyond the rock, it attacks nothing while the rock stands between
/// them and runs through the tunnels until it sees the player, then attacks again; every attack
/// told came with nothing of the walls between them. Left behind at the mines' far end, it gives up,
/// takes its peaceful mode, and attacks no more.
/// Behaviour: server.creatures.attack-only-what-it-sees
#[test]
fn drudge_outcasts_in_the_arwic_mines_attack_only_a_player_they_see() {
    let (mut h, b, me, outcast) = outcast_and_player(in_sight());
    {
        let w = h.server.walls.as_mut().expect("the walls");
        let home = outcast_home();
        assert!(w.sees(&home, &in_sight()), "in plain sight");
        assert!(!w.sees(&home, &beyond_the_rock()), "rock between");
        assert!(w.joined(&home, &beyond_the_rock()), "the tunnels join them");
    }
    let clock = h.clock();
    yalain_world::ai::go_after(&mut h.server, &mut h.net, outcast, me, clock);
    let mut told = Vec::new();
    for _ in 0..200 {
        h.step();
        attacks_seen(&mut h, b, 0, outcast, me, &mut told);
        if !told.is_empty() {
            break;
        }
    }
    assert!(!told.is_empty(), "it attacks the player in sight");

    // Beyond the rock.
    stand(&mut h, me, beyond_the_rock());
    let from = h.bots[b].inbox.len();
    let mut told = Vec::new();
    let mut blocked_steps = 0;
    // Where it stood at each pass: it runs no faster than its run, a world step's stride a step.
    let mut trail: Vec<StoredPosition> = Vec::new();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let passes_a_step = (yalain_world::spawn::STEP_SECONDS / yalain_testkit::STEP).round() as usize;
    for _ in 0..400 {
        h.step();
        let at = place_of(&h, outcast);
        trail.push(at);
        if trail.len() > passes_a_step {
            let back = trail[trail.len() - 1 - passes_a_step];
            let went = yalain_world::presence::distance(&back, &at).unwrap_or(0.0);
            #[allow(clippy::cast_possible_truncation)]
            let stride = yalain_world::ai::RUN_SPEED * yalain_world::spawn::STEP_SECONDS as f32;
            assert!(
                went <= stride * 1.5,
                "{went} m in a world step, more than its run's {stride} m"
            );
        }
        if !h
            .server
            .walls
            .as_mut()
            .expect("the walls")
            .sees(&at, &beyond_the_rock())
        {
            blocked_steps += 1;
        }
        attacks_seen(&mut h, b, from, outcast, me, &mut told);
        if !told.is_empty() {
            break;
        }
    }
    assert!(blocked_steps > 0, "the rock stood between them for a while");
    assert!(
        !told.is_empty(),
        "it comes round and attacks again: {:?}",
        place_of(&h, outcast)
    );
    assert!(
        told.iter().all(|clear| *clear),
        "every attack came in sight: {told:?}"
    );

    // Left behind at the far end of the mines.
    let from = h.bots[b].inbox.len();
    stand(&mut h, me, far_off());
    let peace = h.run_until(100, |h| {
        h.world_messages(b, from).iter().any(|m| {
            matches!(m, auberean_protocol::ServerMessage::DoMode(e)
                if e.sender.id == outcast && e.action.0 == 0x4000_0001)
        })
    });
    assert!(peace, "it gives up and takes its peaceful mode");
    let after = h.bots[b].inbox.len();
    h.run(60);
    let mut told = Vec::new();
    attacks_seen(&mut h, b, after, outcast, me, &mut told);
    assert!(told.is_empty(), "no attack once it gave up");
}

/// A creature going after a player it does not see and that no way through the walls leads to
/// (the player in the hall's other copy, within its perception) gives up a moment after its search
/// finds no way, and goes home, instead of running at the rock for as long as the player stays.
/// Behaviour: server.creatures.attack-only-what-it-sees
#[test]
fn a_creature_that_neither_sees_nor_can_reach_its_player_gives_up() {
    let start = hall(0x010C, 16.0, 428.0, -43.1);
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
        p.position = StoredPosition {
            cell: 0x0299_010C,
            ..start
        };
    }
    let clock = h.clock();
    yalain_world::ai::go_after(&mut h.server, &mut h.net, beetle, player, clock);
    let gave_up = h.run_until(200, |h| {
        h.server
            .things
            .get(beetle)
            .and_then(|t| t.creature.as_ref())
            .is_some_and(|c| c.target.is_none())
    });
    assert!(gave_up, "it gives up the player it cannot see or reach");
}

/// A creature whose attack is due on a player it does not see looks for the player once a world
/// step, however often the world passes in between: an Outcast that only turns, its player beyond
/// the rock, traces its line of sight no more than once a step, and is still after the player.
/// Behaviour: server.creatures.attack-only-what-it-sees
#[test]
fn a_creature_that_does_not_see_its_player_looks_once_a_world_step() {
    let (mut h, _, me, outcast) = outcast_and_player(beyond_the_rock());
    if let Some(c) = h
        .server
        .things
        .get_mut(outcast)
        .and_then(|t| t.creature.as_mut())
    {
        c.stats.movement.kind = yalain_content::MovementType::TurnOnly;
    }
    let clock = h.clock();
    yalain_world::ai::go_after(&mut h.server, &mut h.net, outcast, me, clock);
    h.run(10);
    let lines = |h: &Harness| h.server.walls.as_ref().map_or(0, |w| w.sight_lines);
    let (from, began) = (lines(&h), h.now.0);
    h.run(80);
    let steps = (h.now.0 - began) / yalain_world::spawn::STEP_SECONDS;
    let traced = lines(&h) - from;
    #[allow(clippy::cast_precision_loss)]
    let per_step = traced as f64 / steps;
    assert!(
        per_step <= 1.25,
        "{traced} lines of sight over {steps:.0} world steps ({} passes)",
        80
    );
    assert!(traced > 0, "it still looks");
    assert!(
        h.server
            .things
            .get(outcast)
            .and_then(|t| t.creature.as_ref())
            .is_some_and(|c| c.target == Some(me)),
        "still after the player"
    );
}

// Sight: what stands between two places, and the creatures that attack, give up and take on again
// by it.

/// A place of a block outdoors, on the ground there.
fn ground_at(w: &mut Walls, block: u32, x: f32, y: f32) -> StoredPosition {
    let (cell, x, y) = yalain_world::spawn::outdoor(block, x, y);
    w.read_place(cell);
    let z = w
        .land()
        .ground(CellId(cell).landblock(), Vec3::new(x, y, 0.0))
        .expect("the ground")
        .0;
    StoredPosition {
        cell,
        origin: [x, y, z],
        rotation: [1.0, 0.0, 0.0, 0.0],
    }
}

/// A place in an interior cell or outdoors, as given.
fn place(cell: u32, x: f32, y: f32, z: f32) -> StoredPosition {
    StoredPosition {
        cell,
        origin: [x, y, z],
        rotation: [1.0, 0.0, 0.0, 0.0],
    }
}

/// Whether the ground alone stops the line between two places outdoors in one block, at either
/// height a line of sight runs at.
fn ground_stops(w: &Walls, a: &StoredPosition, b: &StoredPosition) -> bool {
    let block = CellId(a.cell).landblock();
    SIGHT_RISE.iter().all(|rise| {
        let from = Vec3::new(a.origin[0], a.origin[1], a.origin[2] + rise);
        let to = Vec3::new(b.origin[0], b.origin[1], b.origin[2] + rise);
        w.land().trace(block, from, to, false).is_some()
    })
}

/// Walls, floors and rock stand between rooms: in the Arwic mines an Outcast's place sees a place
/// down its tunnel and not one round the corner with the rock between, from either end; a room of
/// the training hall sees nothing of the same room in the hall's other copy beside it.
/// Behaviour: server.sight.walls-and-rock
#[test]
fn walls_and_rock_stand_between_two_places_indoors() {
    let mut w = walls();
    let home = outcast_home();
    assert!(w.sees(&home, &in_sight()), "down its tunnel");
    assert!(!w.sees(&home, &beyond_the_rock()), "round the corner");
    assert!(!w.sees(&beyond_the_rock(), &home), "from the other end");
    let room = hall(0x010C, 16.0, 428.0, -43.1);
    let copy = StoredPosition {
        cell: 0x0299_010C,
        ..room
    };
    assert!(!w.sees(&room, &copy), "the other copy, through the rock");
}

/// Two places see each other along a line two meters above each where the lines a meter and a meter
/// and a half up stop, and not along one that gets through only higher, nor along one that gets
/// through from one end only: places 2017's creatures shot players from and at, both standing still,
/// in block 0x06FD across a hall (17 shots) and outdoors in block 0xE487 (11 shots); in block 0x05FC
/// a creature that had shot the player stood beside it in one room, the line clear only two and a
/// half meters up, and shot nothing in half a minute; in block 0x049F a line between two rooms
/// gets through from one of them only, at every height.
/// Behaviour: server.sight.heights
#[test]
fn two_places_see_each_other_along_a_line_two_meters_up_where_lower_lines_stop() {
    let mut w = walls();
    let lines = sight_world();
    for (shooter, target, what) in [
        (
            place(0x06FD_0258, 348.046_23, 114.503_845, 28.471_941),
            place(0x06FD_0259, 347.166_32, 92.999_04, 25.460_26),
            "across the hall",
        ),
        (
            place(0xE487_0018, 47.866_264, 150.439_76, 94.035_09),
            place(0xE487_0010, 21.980_806, 150.244_89, 96.431_03),
            "outdoors",
        ),
    ] {
        assert!(
            !lines_at(&mut w, &lines, &shooter, &target, &[1.0, 1.5]),
            "{what}: lower lines stop"
        );
        assert!(w.sees(&shooter, &target), "{what}: seen two meters up");
        assert!(w.sees(&target, &shooter), "{what}: from the other end");
    }
    let shooter = place(0x05FC_0118, 4.296_125, -102.128_265, -16.943_937);
    let target = place(0x05FC_0118, 2.657_464_7, -106.280_9, -16.955_05);
    assert!(
        lines_at(&mut w, &lines, &shooter, &target, &[2.5]),
        "clear higher up"
    );
    assert!(!w.sees(&shooter, &target), "not seen");
    let a = place(0x049F_0105, 82.901_985, -110.747_2, 11.597_852);
    let b = place(0x049F_0104, 66.810_394, -109.955_734, 10.898_771);
    w.read_around(a.cell);
    assert!(
        SIGHT_RISE
            .iter()
            .all(|h| one_line(&w, &lines, &b, &a, *h) && !one_line(&w, &lines, &a, &b, *h)),
        "clear from one end only"
    );
    assert!(!w.sees(&a, &b), "not seen");
    assert!(!w.sees(&b, &a), "from either end");
}

/// Whether the line from `a` to `b`, `rise` above each, gets through, traced as sight traces it.
fn one_line(w: &Walls, lines: &World, a: &StoredPosition, b: &StoredPosition, rise: f32) -> bool {
    let at = |p: &StoredPosition| {
        Position::new(
            CellId(p.cell),
            Frame::new(
                Vec3::new(p.origin[0], p.origin[1], p.origin[2] + rise),
                Quat::IDENTITY,
            ),
        )
    };
    let mut los = SightLine::default();
    los.set_from_begin_and_end_position(&at(a), &at(b));
    !lines.los_check(w.land(), &mut los) || los.trace_result != 1
}

/// A world holding nothing placed, with the install's physics types, to trace lines of sight over
/// the walls' land.
fn sight_world() -> World {
    let mut world = World::new();
    if let Some(list) = records().get::<auberean_assets::property::PropertyList>(0x3400_0000) {
        world.set_property_list(list);
    }
    world
}

/// Whether two places see each other along a line at any of `rises` above each, from both ends.
fn lines_at(
    w: &mut Walls,
    lines: &World,
    a: &StoredPosition,
    b: &StoredPosition,
    rises: &[f32],
) -> bool {
    w.read_around(a.cell);
    w.read_around(b.cell);
    rises
        .iter()
        .any(|h| one_line(w, lines, a, b, *h) && one_line(w, lines, b, a, *h))
}

/// Outdoors the ground stands between two places on either side of a hill: in block 0x6FA9 a rise
/// several meters over both ends stops the line between them, the ground alone; over flat ground
/// the line gets through.
/// Behaviour: server.sight.ground
#[test]
fn the_ground_stands_between_two_places_on_either_side_of_a_hill() {
    let mut w = walls();
    let south = ground_at(&mut w, 0x6FA9, 140.0, 118.0);
    let north = ground_at(&mut w, 0x6FA9, 140.0, 152.0);
    let top = ground_at(&mut w, 0x6FA9, 140.0, 135.0);
    assert!(
        top.origin[2] > south.origin[2].max(north.origin[2]) + 3.0,
        "a hill between: {:?} {:?} {:?}",
        south.origin,
        top.origin,
        north.origin
    );
    assert!(
        ground_stops(&w, &south, &north),
        "the ground alone stops it"
    );
    assert!(!w.sees(&south, &north), "over the hill");
    let a = ground_at(&mut w, 0x6FA9, 70.0, 60.0);
    let b = ground_at(&mut w, 0x6FA9, 70.0, 80.0);
    assert!(w.sees(&a, &b), "over flat ground");
}

/// Outdoors a building stands between two places: north of block 0x6FA9's arrival place the line
/// gets through over the open ground, and stops at the first building beyond, where the ground
/// alone would let it through.
/// Behaviour: server.sight.buildings
#[test]
fn a_building_stands_between_two_places_outdoors() {
    let mut w = walls();
    let start = on_ground(&mut w, 45.24, 90.51);
    let open = on_ground(&mut w, 45.24, 110.0);
    let beyond = on_ground(&mut w, 45.24, 130.0);
    assert!(w.sees(&start, &open), "over the open ground");
    assert!(
        !ground_stops(&w, &start, &beyond),
        "the ground alone lets it through"
    );
    assert!(!w.sees(&start, &beyond), "the building stands between");
    assert!(!w.sees(&beyond, &start), "from the other end");
}

/// From outdoors a place sees into a building's entry room through its doorway, as 2017's
/// creatures shot into and out of entry rooms, and no further: in block 0x8B91 a place west of the
/// building sees one in its entry room through the doorway (a place a creature shot a player at from
/// there), and not one in the room below it; in block 0x9C96 a place in the entry room sees out
/// through its doorway to a place a creature inside shot a player at.
/// Behaviour: server.sight.into-buildings
#[test]
fn outdoors_a_place_sees_into_a_buildings_entry_room_through_its_doorway_and_no_further() {
    let mut w = walls();
    let outside = place(0x8B91_0022, 92.65, 36.06, 66.02);
    let entry = place(0x8B91_0100, 104.82, 40.73, 65.75);
    w.read_around(entry.cell);
    let below = w
        .land()
        .cell(CellId(0x8B91_0101))
        .map(|g| {
            place(
                0x8B91_0101,
                g.frame.origin.x,
                g.frame.origin.y,
                g.frame.origin.z,
            )
        })
        .expect("the room below");
    assert!(inside(&w, &entry), "in the entry room");
    assert!(inside(&w, &below), "in the room below");
    assert!(w.sees(&outside, &entry), "through the doorway");
    assert!(w.sees(&entry, &outside), "and out");
    assert!(!w.sees(&outside, &below), "not the room below");
    assert!(!w.sees(&below, &outside), "nor out of it");
    // A creature by the entry room's far wall shot a player west of the building along a line that
    // gets through both ways only two meters up.
    let by_the_wall = place(0x8B91_0100, 109.6, 41.79, 63.35);
    let west = place(0x8B91_0023, 96.95, 40.01, 66.0);
    assert!(
        w.sees(&by_the_wall, &west),
        "out through the doorway, two meters up"
    );
    assert!(w.sees(&west, &by_the_wall), "and in");
    let shooter = place(0x9C96_0100, 137.57, 128.36, 179.75);
    let target = place(0x9C96_0037, 127.35, 128.53, 177.87);
    assert!(w.sees(&shooter, &target), "out through the doorway");
}

/// A closed door stands between two places, an open one doesn't: a door across the line between
/// two places on open ground stops it while it is shut and lets it through once open.
/// Behaviour: server.sight.closed-doors
#[test]
fn a_closed_door_stands_between_two_places_and_an_open_one_does_not() {
    let mut w = walls();
    let a = ground_at(&mut w, 0x6FA9, 70.0, 60.0);
    let b = ground_at(&mut w, 0x6FA9, 70.0, 80.0);
    let door = ground_at(&mut w, 0x6FA9, 70.0, 70.0);
    assert!(w.sees(&a, &b), "open ground");
    w.set_doors(vec![(7, door)]);
    assert!(!w.sees(&a, &b), "the door shut");
    assert!(!w.sees(&b, &a), "from the other side");
    w.set_door_open(7, true);
    assert!(w.sees(&a, &b), "the door open");
}

/// The missile stories a creature loosed at the player since `from`, and the arrivals the player
/// told of its missiles, with their outcomes.
fn shots_and_arrivals(h: &Harness, b: usize, from: usize, id: u64, me: u64) -> (usize, Vec<i32>) {
    let mut shots = 0;
    let mut arrivals = Vec::new();
    for m in h.world_messages(b, from) {
        let auberean_protocol::ServerMessage::DoStory(e) = m else {
            continue;
        };
        if e.sender.id == id
            && e.action
                .hooks
                .iter()
                .any(|k| k.target == me && k.bits & 0xF == 3)
        {
            shots += 1;
        }
        if e.sender.id == me && e.action.attacker == id {
            arrivals.extend(e.action.hooks.iter().map(|k| k.numbers[0]));
        }
    }
    (shots, arrivals)
}

/// A creature's missile misses a player who steps out of its sight while it flies: an Outcast
/// shooting a player in plain sight, the player round the tunnel's corner before each missile
/// arrives, is told every one missed, its health untouched (a target that moved alone is missed
/// only about half the time).
/// Behaviour: server.creatures.missile-meets-what-stands-between
#[test]
fn a_creatures_missile_misses_a_player_who_steps_behind_the_rock_as_it_flies() {
    let (mut h, b, me, outcast) = outcast_and_player(in_sight());
    let health = |h: &Harness| {
        h.server
            .players
            .values()
            .find(|p| p.character == me)
            .and_then(|p| {
                yalain_world::quals::int(&p.qualities, yalain_world::player::quality::HEALTH)
            })
    };
    let before = health(&h);
    let clock = h.clock();
    yalain_world::ai::go_after(&mut h.server, &mut h.net, outcast, me, clock);
    let mut outcomes = Vec::new();
    for _ in 0..8 {
        // From its home, shooting, the player in plain sight.
        let now = h.now.0;
        if let Some(t) = h.server.things.get_mut(outcast) {
            t.at = Some(outcast_home());
            if let Some(c) = t.creature.as_mut() {
                c.ranged = true;
                c.closing = None;
                c.next_attack = c.next_attack.min(now + 0.5);
            }
        }
        stand(&mut h, me, in_sight());
        let from = h.bots[b].inbox.len();
        let shot = h.run_until(200, |h| shots_and_arrivals(h, b, from, outcast, me).0 > 0);
        assert!(shot, "it shoots the player in sight");
        let loosed = h.bots[b].inbox.len();
        stand(&mut h, me, beyond_the_rock());
        let arrived = h.run_until(100, |h| {
            !shots_and_arrivals(h, b, loosed, outcast, me).1.is_empty()
        });
        assert!(arrived, "the missile arrives");
        outcomes.push(shots_and_arrivals(&h, b, loosed, outcast, me).1[0]);
    }
    let miss = yalain_world::rules::Outcome::Miss.code().cast_signed();
    assert!(
        outcomes.iter().all(|o| *o == miss),
        "every one missed: {outcomes:x?}"
    );
    assert_eq!(health(&h), before, "unhurt");
}

/// A creature that gave up a player it neither saw nor found a way to doesn't take that player on
/// again while it still doesn't see it, and does once it does: a beetle that attacks first notices
/// a player in the training hall's other copy, beside its own and within its perception, goes after
/// it, gives up, and goes home; for the next half minute it leaves the player be; with the player
/// come into its room, it takes it on again.
/// Behaviour: server.creatures.given-up-unseen-not-taken-on-again
#[test]
fn a_creature_that_gave_up_a_player_it_neither_sees_nor_reaches_takes_it_on_again_only_once_it_sees_it(
) {
    let start = hall(0x010C, 16.0, 428.0, -43.1);
    let (mut h, b, beetle) = beetle_at(start);
    h.bots[b].send(
        true,
        auberean_protocol::queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    if let Some(c) = h
        .server
        .things
        .get_mut(beetle)
        .and_then(|t| t.creature.as_mut())
    {
        c.stats.movement.perception = 400.0;
        c.stats.attacks_first = true;
    }
    for p in h.server.players.values_mut() {
        p.position = StoredPosition {
            cell: 0x0299_010C,
            ..start
        };
    }
    let target = |h: &Harness| {
        h.server
            .things
            .get(beetle)
            .and_then(|t| t.creature.as_ref())
            .and_then(|c| c.target)
    };
    assert!(
        h.run_until(100, |h| target(h).is_some()),
        "it notices the player"
    );
    assert!(h.run_until(400, |h| target(h).is_none()), "and gives up");
    let mut again = 0;
    for _ in 0..600 {
        h.step();
        again += usize::from(target(&h).is_some());
    }
    assert_eq!(again, 0, "it leaves the player be while it does not see it");
    let home = place_of(&h, beetle);
    let near = StoredPosition {
        origin: [home.origin[0] + 2.0, home.origin[1], home.origin[2]],
        ..home
    };
    for p in h.server.players.values_mut() {
        p.position = near;
    }
    assert!(
        h.run_until(60, |h| target(h).is_some()),
        "in its sight, it takes the player on again"
    );
}

/// A creature that only turns gives up a player it does not see: it has no way to anything, so out
/// of its sight it is as good as gone, five seconds on.
/// Behaviour: server.creatures.attack-only-what-it-sees
#[test]
fn a_creature_that_only_turns_gives_up_a_player_it_does_not_see() {
    let (mut h, _, me, outcast) = outcast_and_player(beyond_the_rock());
    if let Some(c) = h
        .server
        .things
        .get_mut(outcast)
        .and_then(|t| t.creature.as_mut())
    {
        c.stats.movement.kind = yalain_content::MovementType::TurnOnly;
    }
    let clock = h.clock();
    yalain_world::ai::go_after(&mut h.server, &mut h.net, outcast, me, clock);
    let gave_up = h.run_until(200, |h| {
        h.server
            .things
            .get(outcast)
            .and_then(|t| t.creature.as_ref())
            .is_some_and(|c| c.target.is_none())
    });
    assert!(gave_up, "it gives up the player it does not see");
}

/// A creature fighting another creature it neither sees nor finds a way to gives up five seconds on,
/// as it gives up such a player: a beetle set on a creature in the training hall's other copy,
/// beside its own, runs at the rock, finds no way, and goes home.
/// Behaviour: server.creatures.fights-things-only-in-sight
#[test]
fn a_creature_fighting_another_it_neither_sees_nor_reaches_gives_up() {
    let start = hall(0x010C, 16.0, 428.0, -43.1);
    let (mut h, _, beetle) = beetle_at(start);
    h.add_creatures(1);
    h.run(10);
    let other = h
        .server
        .things
        .by_id
        .values()
        .find(|t| {
            t.id != beetle && t.creature.is_some() && t.template == yalain_testkit::TEST_CREATURE
        })
        .map(|t| t.id)
        .expect("a second creature");
    let apart = StoredPosition {
        cell: 0x0299_010C,
        ..start
    };
    if let Some(t) = h.server.things.get_mut(other) {
        t.at = Some(apart);
        if let Some(c) = t.creature.as_mut() {
            c.home = apart;
            c.stats.fights = false;
        }
    }
    if let Some(c) = h
        .server
        .things
        .get_mut(beetle)
        .and_then(|t| t.creature.as_mut())
    {
        c.stats.movement.perception = 400.0;
    }
    let clock = h.clock();
    yalain_world::ai::take_on(&mut h.server, &mut h.net, beetle, other, clock);
    assert_eq!(
        h.server
            .things
            .get(beetle)
            .and_then(|t| t.creature.as_ref())
            .and_then(|c| c.target),
        Some(other),
        "set on the other"
    );
    let gave_up = h.run_until(400, |h| {
        h.server
            .things
            .get(beetle)
            .and_then(|t| t.creature.as_ref())
            .is_some_and(|c| c.target.is_none())
    });
    assert!(
        gave_up,
        "it gives up the creature it neither sees nor reaches"
    );
}

/// A flyer killed in a fight falls to the floor under it: its place told on the way, lower each
/// time as anything falls, the last on the floor its fighting height below (a meter down by about
/// half a second), and its corpse made there, not in the air.
/// Behaviour: server.creatures.a-dead-flyer-falls
#[test]
fn a_flyer_killed_falls_to_the_floor_and_its_corpse_lies_there() {
    use auberean_protocol::ServerMessage;
    let home = over_a_floor();
    let (mut h, b, id) = beetle_at(home);
    let records = records();
    let wasp = walker_of(&records, WASP);
    let walls = h.server.walls.as_mut().expect("the walls");
    walls.walkers.insert(TEST_CREATURE, wasp);
    let up = walls
        .step_fighting(TEST_CREATURE, 1.0, &home, [0.0, 0.1])
        .expect("a step")
        .at;
    if let Some(t) = h.server.things.get_mut(id) {
        t.at = Some(up);
    }
    let me = h
        .server
        .players
        .values()
        .next()
        .expect("a player")
        .character;
    let from = h.bots[b].inbox.len();
    let died = h.now.0;
    let clock = h.clock();
    yalain_world::death::creature_dies(&mut h.server, &mut h.net, id, me, clock);
    // Step by step, the heights told and when.
    let mut told: Vec<(f64, f32)> = Vec::new();
    let mut seen = from;
    for _ in 0..80 {
        h.step();
        for m in h.world_messages(b, seen) {
            if let ServerMessage::Position(p) = m {
                if p.sender.id == id {
                    told.push((h.now.0 - died, p.origin.z));
                }
            }
        }
        seen = h.bots[b].inbox.len();
    }
    let floor = up.origin[2] - 1.0;
    assert!(told.len() >= 3, "{told:?}");
    assert!(
        told.windows(2).all(|w| w[1].1 <= w[0].1),
        "only down: {told:?}"
    );
    let (landed, last) = *told.last().expect("told");
    assert!((last - floor).abs() < 0.01, "on the floor: {told:?}");
    assert!(
        (0.4..0.75).contains(&landed),
        "landed at {landed} s: {told:?}"
    );
    let corpse = h.world_messages(b, from).into_iter().find_map(|m| match m {
        ServerMessage::CreateObject(c) if c.physics.position.cell == up.cell && c.id != id => {
            Some(c.physics.position.frame.origin.z)
        }
        _ => None,
    });
    let z = corpse.expect("its corpse");
    assert!(
        (z - floor).abs() < 0.01,
        "the corpse at {z}, the floor at {floor}"
    );
}
