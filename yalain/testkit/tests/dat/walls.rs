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
/// were, since the answer would be the same; when the goal moves it does.
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
    h.run(20);
    let found = searches(&h);
    h.run(200);
    assert_eq!(searches(&h), found, "ten seconds on, no search more");
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
