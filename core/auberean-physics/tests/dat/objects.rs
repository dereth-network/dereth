//! Objects of the world against the player's body in the human training hall: a sign the server
//! puts there, what the cells' own records place that collides through cylinders, and the parts
//! of every object the cells place, where they collide.
//! Fixture: hall copy 0x0298 of the September 2013 cell file; the Movement sign as the 2017
//! servers placed it (template 0x47002E09, look 0x1F0002F4, in cell 0x0109); the human male's
//! collision spheres and step heights and the player's motion values.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use auberean_assets::entity::Entity;
use auberean_physics::body::flags;
use auberean_physics::body::profile::{Profile, ProfileQualities};
use auberean_physics::interior::add_block_cells;
use auberean_physics::land::{build, build_near, land_cell_id, Land, NearBlocks, Solid};
use auberean_physics::mesh::Trees;
use auberean_physics::motion::params::MovementParameters;
use auberean_physics::motion::MotionInput;
use auberean_physics::object::Shape;
use auberean_physics::parts::{PartArray, PartGroup};
use auberean_physics::solid::Ethereality;
use auberean_physics::world::{World, WorldObject};
use auberean_primitives::DataId;
use auberean_world::look::{Group, Look};
use auberean_world::place::{PlaceRecords, DRAWN_KINDS};
use dereth_physics::arena::Arena;
use dereth_physics::geom::Sphere;
use dereth_physics::source::SetupPart;
use dereth_physics::transition::collide::{
    object_geometry, object_part_find_obj_collisions_scaled, ObjGeometry,
};
use dereth_physics::transition::{Transition, TransitionCtx, TransitionState, TreeScale};
use dereth_physics::{PhysicsObj, PhysicsState, SetupGeometry};
use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

use crate::common::need_world;
use crate::interior::{at_the_start, body};

/// The Movement sign's look and where it stands.
const SIGN_LOOK: u32 = 0x1F00_02F4;

fn sign_place() -> Position {
    Position::new(
        CellId(0x0298_0109),
        Frame::new(Vec3::new(43.8744, 240.1739, -44.2061), Quat::IDENTITY),
    )
}

fn hall() -> (Land, Trees) {
    let w = need_world!();
    let land = Land::new();
    let mut trees = Trees::default();
    let added = add_block_cells(&land, &w.records, &mut trees, 0x02, 0x98);
    assert!(added > 50, "the hall's cells: {added}");
    (land, trees)
}

/// Run a body from `start` facing `heading` for eight seconds against `world`; its places.
fn run(land: &Land, world: &mut World, start: Position, heading: f32) -> Vec<Vec3> {
    let w = need_world!();
    let mut b = body(w, start, heading);
    b.contact = false;
    b.on_walkable = false;
    (0..80)
        .map(|k| {
            b.steer(MotionInput {
                forward: if k >= 10 { 1.0 } else { 0.0 },
                ..MotionInput::default()
            });
            b.tick_in(land, world, 0.1);
            b.position.frame.origin
        })
        .collect()
}

fn across(a: Vec3, b: Vec3) -> f32 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt()
}

/// The Movement sign the server puts in the hall is solid: its look collides through one upright
/// cylinder (1.41 m in radius at its own scale, the look's 0.7 on top), so a body running west
/// from where a new character starts stops against it, never inside it; with no sign in the world
/// the same run goes through where it stands.
///
/// Behaviour: collision.objects.a-body-stops-at-the-movement-sign
#[test]
fn a_body_stops_at_the_movement_sign() {
    let w = need_world!();
    let (land, mut trees) = hall();
    let shape = Shape::of(&w.records, DataId(SIGN_LOOK), &mut trees).expect("the sign's shape");
    assert!(!shape.has_physics_tree());
    assert_eq!(shape.cylinders.len(), 1);
    let reach = shape.cylinders[0].radius * shape.scale.x;
    assert!((reach - 0.987).abs() < 0.01, "the sign's reach: {reach}");
    let sign = sign_place().frame.origin;

    let mut open = World::new();
    let through = run(&land, &mut open, at_the_start(), 270.0);
    assert!(
        through.iter().any(|p| p.x < sign.x - 1.0),
        "with no sign the run goes past it: {:?}",
        through.last()
    );

    let mut world = World::new();
    let key = world.insert(
        &land,
        0x4002_9800_0000_003B,
        &WorldObject {
            shape: Arc::new(shape),
            position: sign_place(),
            is_static: true,
            passable: false,
        },
    );
    assert!(world
        .cells_of(key)
        .is_some_and(|c| c.contains(&CellId(0x0298_0109))));
    let stopped = run(&land, &mut world, at_the_start(), 270.0);
    let nearest = stopped
        .iter()
        .map(|p| across(*p, sign))
        .fold(f32::MAX, f32::min);
    assert!(
        nearest > reach,
        "never inside the sign: {nearest} m from it"
    );
    let end = stopped[stopped.len() - 1];
    assert!(
        end.x > sign.x && end.x < sign.x + reach + 1.0,
        "at rest against it: {end:?}"
    );
}

/// The hall's trainer stands on a two-tier dais that its cell places (cell 0x0120): its top
/// reaches over the next cell (0x011F), and a cell files what it places in every cell the box
/// around what the thing draws reaches, so a body standing on the top in the next cell runs across
/// it to its edge. Filed only in its own cell, the dais was not there to stand on from the next
/// cell: the body could not step down onto anything and stopped, still on the top, after a few
/// centimeters.
///
/// Behaviour: collision.objects.a-body-runs-across-a-dais-reaching-into-its-cell
#[test]
fn a_body_runs_across_a_dais_reaching_into_its_cell() {
    let (land, _trees) = hall();
    let mut world = World::new();
    // Where a player stood on the dais's top beside the trainer, in the next cell.
    let start = Position::new(
        CellId(0x0298_011F),
        Frame::new(Vec3::new(101.1257, 239.7474, -43.3), Quat::IDENTITY),
    );
    let track = run(&land, &mut world, start, 231.0);
    let stood = track[9];
    assert!(
        (stood.z - -43.495).abs() < 0.01,
        "standing on the dais's top: {stood:?}"
    );
    let on_top = track
        .iter()
        .filter(|p| (p.z - stood.z).abs() < 0.01)
        .map(|p| across(*p, stood))
        .fold(0.0f32, f32::max);
    assert!(
        on_top > 1.0,
        "runs across the top: {on_top} m, then {:?}",
        track[track.len() - 1]
    );
}

/// A qualities record's tables, as a profile asks them.
struct RecordQualities(auberean_assets::script::Qualities);

impl ProfileQualities for RecordQualities {
    fn truth(&mut self, key: u32) -> Option<bool> {
        self.0
            .bools
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| *v != 0)
    }
    fn int(&mut self, key: u32) -> Option<i32> {
        self.0.ints.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
    }
    fn float(&mut self, key: u32) -> Option<f32> {
        self.0
            .floats
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| *v)
    }
    fn data_id(&mut self, key: u32) -> Option<u32> {
        self.0
            .data_ids
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v.0)
    }
}

/// Only a body whose qualities say it slides along edges is held at the edge of a drop it would
/// step down: from the top of the hall's dais, running west, a body made from the player
/// template's qualities record comes to rest on the dais's lower tier, at its edge, 0.86 m over
/// the floor; one made from a record that does not say so (a placed object's) walks off it to the
/// floor.
///
/// Behaviour: collision.objects.only-a-body-whose-qualities-say-so-is-held-at-a-ledge
#[test]
fn only_a_body_whose_qualities_say_so_is_held_at_a_ledge() {
    let w = need_world!();
    let (land, _trees) = hall();
    let start = Position::new(
        CellId(0x0298_011F),
        Frame::new(Vec3::new(101.1257, 239.7474, -43.3), Quat::IDENTITY),
    );
    let lowest = |record: u32| {
        let q = w
            .records
            .get::<auberean_assets::script::Qualities>(record)
            .expect("the qualities record");
        let profile = Profile::of(&mut RecordQualities(q));
        let mut world = World::new();
        let mut b = body(w, start, 270.0);
        b.apply_profile(&profile);
        b.contact = false;
        b.on_walkable = false;
        let mut lowest = f32::MAX;
        for k in 0..100 {
            b.steer(MotionInput {
                forward: if k >= 10 { 1.0 } else { 0.0 },
                ..MotionInput::default()
            });
            b.tick_in(&land, &mut world, 0.1);
            lowest = lowest.min(b.position.frame.origin.z);
        }
        (profile, lowest, b.position.frame.origin)
    };
    let (player, held, at) = lowest(0x8100_0530);
    assert!(player.state & flags::EDGE_SLIDE != 0, "{player:?}");
    assert!(
        (held - -44.03).abs() < 0.02,
        "held on the lower tier: {held}, at {at:?}"
    );
    let (placed, fell, at) = lowest(0x8000_0136);
    assert_eq!(placed.state & flags::EDGE_SLIDE, 0, "{placed:?}");
    assert!(fell < -44.9, "down on the floor: {fell}, at {at:?}");
}

/// A body made from the player template's qualities, standing at `start` facing `heading`,
/// ordered by the server to run to `target` as the server orders a player's client; where it is
/// after twenty seconds of the world's physics ticks.
fn ordered(land: &Land, start: Position, heading: f32, target: Position) -> Vec3 {
    let w = need_world!();
    let q = w
        .records
        .get::<auberean_assets::script::Qualities>(0x8100_0530)
        .expect("the player template's qualities");
    let mut world = World::new();
    let mut b = body(w, start, heading);
    b.apply_profile(&Profile::of(&mut RecordQualities(q)));
    world.insert_body(land, 0x5000_0001, &mut b);
    #[allow(clippy::cast_possible_truncation)] // a tenth of a second
    let tick = auberean_physics::body::TICK as f32;
    for _ in 0..10 {
        b.tick_in(land, &mut world, tick);
    }
    let order = MovementParameters {
        flags: 0x3,
        desired_velocity: 1.0,
        fail_distance: f32::MAX,
        desired_distance: 0.0,
        max_success_distance: 0.2,
        min_success_distance: 0.0,
        target_position: target,
        ..MovementParameters::default()
    };
    assert!(
        b.receive_move_to(&order, 1),
        "the player's body takes the order"
    );
    for _ in 0..200 {
        b.tick_in(land, &mut world, tick);
    }
    b.position.frame.origin
}

/// The 0930 client's own player, ordered by the server around the hall's dais, stops where a body
/// here stops, within a centimeter: run from the top toward the hollow west of the dais it is held
/// on the lower tier at its edge, 0.86 m over the floor (at (98.0745, 239.9422)); run from the
/// hollow north-west of the dais toward the top it is held at the foot of the dais's side, which
/// it does not climb, from two places (at (99.2486, 243.0068) and (97.9155, 241.5962)); run from
/// the hollow north of it, it climbs onto the lower tier and the top.
///
/// Behaviour: collision.objects.the-halls-dais-holds-a-body-where-the-client-holds-it
#[test]
fn the_halls_dais_holds_a_body_where_the_client_holds_it() {
    let (land, _trees) = hall();
    let at = |cell: u32, x: f32, y: f32, z: f32| {
        Position::new(CellId(cell), Frame::new(Vec3::new(x, y, z), Quat::IDENTITY))
    };
    let top = at(0x0298_011F, 101.1257, 239.7474, -43.495);
    let near = |got: Vec3, x: f32, y: f32, z: f32| {
        across(got, Vec3::new(x, y, z)) < 0.01 && (got.z - z).abs() < 0.001
    };
    let held = ordered(&land, top, 270.0, at(0x0298_011D, 97.0, 240.0, -44.985));
    assert!(
        near(held, 98.0745, 239.9422, -44.035),
        "held on the lower tier: {held:?}"
    );
    for ((x, y, z, heading), (sx, sy)) in [
        ((98.0, 243.6, -44.9695, 135.0), (99.2486, 243.0068)),
        ((97.3376, 242.3165, -45.005, 120.0), (97.9155, 241.5962)),
    ] {
        let stopped = ordered(&land, at(0x0298_011D, x, y, z), heading, top);
        assert!(
            near(stopped, sx, sy, -45.005),
            "from ({x}, {y}) held at the foot: {stopped:?}"
        );
    }
    let climbed = ordered(&land, at(0x0298_011D, 100.0, 244.6, -44.6894), 166.0, top);
    assert!(
        across(climbed, top.frame.origin) < 0.3 && (climbed.z - -43.495).abs() < 0.001,
        "up on the top: {climbed:?}"
    );
}

/// How far east a body of these masks gets across the ground of block 0x7D93 from west of a tall
/// tree-shaped wall marked for creatures to pass.
fn east_of_the_marked_wall(w: &crate::common::World, masks: Ethereality) -> f32 {
    let mut trees = Trees::default();
    let land = Land::new();
    land.add_block(&w.records, &w.region, &mut trees, 0x7D, 0x93);
    let cell = land_cell_id(0x7D, 0x93, 2, 2);
    let start = Position::new(
        cell,
        Frame::new(Vec3::new(69.8, 56.1, 127.49), Quat::IDENTITY),
    );
    let mut world = World::new();
    world.set_property_list(
        w.records
            .get::<auberean_assets::property::PropertyList>(0x3400_0000)
            .expect("the list"),
    );
    let mut b = body(w, start, 90.0);
    let key = world.insert_body(&land, 0, &mut b);
    world.set_ethereality(key, masks);
    let leg = Vec3::new(0.09, 0.08, 0.0);
    for _ in 0..100 {
        b.contact = true;
        b.on_walkable = true;
        b.velocity = Vec3::ZERO;
        b.move_through(&land, &mut world, leg, 90.0);
    }
    b.position.frame.origin.x
}

/// Tree-shaped scenery placed outside asks what a mover passes through: marked for creatures to
/// pass, it lets a body of a creature's masks by and holds up one of a player's.
///
/// Behaviour: collision.solid.outdoor-tree-scenery-asks-what-a-mover-passes
#[test]
fn outdoor_tree_scenery_lets_by_only_those_that_pass_through_its_mark() {
    let w = need_world!();
    let creature = Ethereality {
        physics_type: 0x40_0020,
        movement: 0x4220_0010,
        placement: 0x42A0_0090,
    };
    let player = Ethereality {
        physics_type: 0x0480_0020,
        movement: 0x3E20_0010,
        placement: 0,
    };
    let through = east_of_the_marked_wall(w, creature);
    let held = east_of_the_marked_wall(w, player);
    assert!(through > 76.0, "through the wall: {through}");
    assert!(held < 75.0, "held at the wall: {held}");
}

/// What the hall's cells place that collides only through its setup's cylinders is solid too: a
/// body running north from beside the start stops short of the one standing 8.6 m away (2.15 m
/// in radius, 1.25 its scale), and runs through where it stands once the cells place nothing.
///
/// Behaviour: collision.objects.placed-scenery-with-cylinders-is-solid
#[test]
fn placed_scenery_with_cylinders_is_solid() {
    let (land, _) = hall();
    let cell = CellId(0x0298_0109);
    let placed = land.cell_objects(cell).expect("the cell's placed objects");
    let (at, shape) = placed
        .iter()
        .find(|(p, _)| across(p.frame.origin, Vec3::new(55.1, 248.8, 0.0)) < 0.1)
        .expect("the one north of the start");
    let reach = shape.cylinders[0].radius * shape.scale.x;
    assert!((reach - 2.683).abs() < 0.01, "its reach: {reach}");
    let start = Position::new(
        cell,
        Frame::new(Vec3::new(at.frame.origin.x, 240.2, -44.6), Quat::IDENTITY),
    );
    let mut world = World::new();
    let ran = run(&land, &mut world, start, 0.0);
    let nearest = ran
        .iter()
        .map(|p| across(*p, at.frame.origin))
        .fold(f32::MAX, f32::min);
    assert!(nearest > reach, "never inside it: {nearest} m from it");
    assert!(
        world.len() > placed.len(),
        "the hall's scenery is in the world"
    );

    for c in land.interior_cells_of(0x0298_0000) {
        land.insert_cell_objects(c, Vec::new());
    }
    let mut empty = World::new();
    let through = run(&land, &mut empty, start, 0.0);
    assert!(
        through.iter().any(|p| p.y > at.frame.origin.y),
        "with nothing placed the run goes past it: {:?}",
        through.last()
    );
}

/// A sphere of the human male's size with its base at `pos`, tested only for whether it overlaps
/// something solid (as a placement is).
fn placing_at(pos: &Position) -> Transition {
    let mut t = Transition::default();
    t.init();
    t.set_rules(auberean_physics::rules::rules());
    t.sphere_path
        .init_sphere(&[Sphere::new(Vec3::new(0.0, 0.0, 0.48), 0.48)], 1.0);
    t.sphere_path.init_path(Some(pos.cell), Some(*pos), pos);
    t.sphere_path.set_check_pos(pos, Some(pos.cell));
    t.sphere_path.obstruction_ethereal = true;
    t
}

/// Every object the hall's cells place puts its parts together from its look; each whose parts
/// have physics trees meets a sphere placed around it (on a grid out to a meter and a half each way
/// and two meters up) exactly where the world's shape of the same look does, both testing every
/// tree in the object's own frame.
///
/// Behaviour: parts.data.the-halls-objects-parts-collide-as-their-shapes-do
#[test]
fn the_halls_objects_parts_collide_as_their_shapes_do() {
    let w = need_world!();
    let info = w.records.landblock_info(0x02, 0x98).expect("the hall");
    let mut looks = BTreeSet::new();
    for k in 0..info.cell_count {
        let Some(cell) = u16::try_from(0x100 + k)
            .ok()
            .and_then(|low| w.records.cell(0x02, 0x98, low))
        else {
            continue;
        };
        for e in cell.entities.iter().flat_map(|g| g.entities.iter()) {
            if let Some(o) = e.object {
                looks.insert(o);
            }
        }
    }
    let land = Land::new();
    let objects: Arena<PhysicsObj> = Arena::default();
    let cells = BTreeMap::new();
    let ctx = TransitionCtx {
        land: &land,
        objects: &objects,
        cells: &cells,
        mover: None,
        object_table: None,
        entry_host: None,
    };
    let here = Position::new(
        CellId(0x0298_0100),
        Frame::new(Vec3::new(50.0, 50.0, 0.0), Quat::IDENTITY),
    );
    let mut trees = Trees::default();
    let (mut with_trees, mut met, mut tried) = (0, 0, 0);
    for id in looks {
        let shape = Shape::of(&w.records, DataId(id), &mut trees).expect("a shape");
        let mut at = id;
        while at >> 24 == 0x47 {
            at = w
                .records
                .entity(DataId(at))
                .and_then(|e| e.object)
                .expect("an entity made from something");
        }
        let look = Look::of_record(&w.records, DataId(at)).expect("a look");
        let mut parts = PartGroup::default();
        assert!(parts.init_look(&w.records, &look), "{id:#x} put together");
        assert_eq!(
            parts.cache_has_physics_tree(),
            shape.has_physics_tree(),
            "{id:#x}"
        );
        if !shape.has_physics_tree() {
            continue;
        }
        with_trees += 1;
        let geometry = SetupGeometry {
            parts: shape
                .trees
                .iter()
                .map(|t| SetupPart {
                    physics_bsp: Some(Arc::clone(t)),
                    ..SetupPart::default()
                })
                .collect(),
            ..SetupGeometry::default()
        };
        let mut state = PhysicsState::DEFAULT;
        state.set_has_physics_bsp(true);
        for (x, y, z) in (-2..=2)
            .flat_map(|x| (-2..=2).map(move |y| (x, y)))
            .flat_map(|(x, y)| [0.0, 0.5, 1.0, 2.0].map(|z| (x, y, z)))
        {
            #[allow(clippy::cast_precision_loss)]
            let o = Vec3::new(
                here.frame.origin.x + x as f32 * 0.75,
                here.frame.origin.y + y as f32 * 0.75,
                here.frame.origin.z + z,
            );
            let pos = Position::new(here.cell, Frame::new(o, Quat::IDENTITY));
            let mut t = placing_at(&pos);
            let mine = parts.find_obj_collisions(&ctx, &mut t, &here, shape.scale);
            let mut t = placing_at(&pos);
            let theirs = object_geometry(
                &ctx,
                &mut t,
                state,
                ObjGeometry::Setup(&geometry, None),
                shape.scale.z,
                &here,
                false,
            );
            assert_eq!(mine, theirs, "{id:#x} at {o:?}");
            tried += 1;
            if mine != TransitionState::Ok {
                met += 1;
            }
        }
    }
    eprintln!("{with_trees} looks with trees, {met} of {tried} placements met");
    assert!(with_trees > 0 && met > 0 && met < tried);
}

/// A tree tested at a scale along each axis reaches that far: the one block 0xAE5C places at
/// (127.6, 115.8), four times its length, its width and six tenths its height, meets a sphere
/// placed 16 m from its middle along its length but none 17 m out, and one 4.5 m across but none
/// 5.5 m across (tested at its height's scale alone it reached 2.5 m along).
///
/// Behaviour: collision.objects.a-tree-tested-at-a-scale-along-each-axis-reaches-that-far
#[test]
fn a_tree_tested_at_a_scale_along_each_axis_reaches_that_far() {
    let w = need_world!();
    let mut trees = Trees::default();
    let block = build(&w.records, &w.region, &mut trees, 0xAE, 0x5C).expect("block 0xAE5C");
    let solid = block
        .solids
        .iter()
        .find(|s| across(s.part.pos.frame.origin, Vec3::new(127.6, 115.8, 0.0)) < 0.05)
        .expect("the stretched object");
    assert_eq!(solid.scale, Vec3::new(4.0, 1.0, 0.6));
    let land = Land::new();
    let objects: Arena<PhysicsObj> = Arena::default();
    let cells = BTreeMap::new();
    let ctx = TransitionCtx {
        land: &land,
        objects: &objects,
        cells: &cells,
        mover: None,
        object_table: None,
        entry_host: None,
    };
    let o = solid.part.pos.frame.origin;
    let meets = |dx: f32, dy: f32, scale: TreeScale| {
        [-1.0f32, 0.0, 0.5, 1.0, 2.0, 3.0].iter().any(|dz| {
            let p = Vec3::new(o.x + dx, o.y + dy, o.z + dz);
            let pos = Position::new(solid.part.pos.cell, Frame::new(p, Quat::IDENTITY));
            object_part_find_obj_collisions_scaled(&ctx, &mut placing_at(&pos), &solid.part, scale)
                != TransitionState::Ok
        })
    };
    let axes = TreeScale::axes(solid.scale);
    assert!(meets(16.0, 0.0, axes) && !meets(17.0, 0.0, axes), "along");
    assert!(meets(0.0, 4.5, axes) && !meets(0.0, 5.5, axes), "across");
    let height_alone = TreeScale::Uniform(solid.scale.z);
    assert!(meets(2.0, 0.0, height_alone) && !meets(3.0, 0.0, height_alone));
}

/// The land of the blocks around one, as the land's solids and ground stand for a body.
fn land_around(w: &crate::common::World, x: u8, y: u8) -> Land {
    let mut trees = Trees::default();
    let land = Land::new();
    for bx in x - 1..=x + 1 {
        for by in y - 1..=y + 1 {
            land.add_block(&w.records, &w.region, &mut trees, bx, by);
        }
    }
    land
}

/// An object with a physics tree at an uneven scale is walked through, and nothing stands on it,
/// as the retail client walked: a body running south under the arch of the one block 0xAE5C places
/// at (127.6, 115.8) comes out past it, one running north at the wall section block 0x85BB places
/// at (72.2, 135.4), a tenth taller than its look, comes out past that. Evenly scaled objects still
/// stop a body: running south from east of the arch it stands short of the arch's foot.
///
/// Behaviour: collision.objects.an-object-with-a-tree-at-an-uneven-scale-is-walked-through
#[test]
fn an_object_with_a_tree_at_an_uneven_scale_is_walked_through() {
    let w = need_world!();
    let land = land_around(w, 0xAE, 0x5C);
    let at = |x, y, z| {
        Position::new(
            land_cell_id(0xAE, 0x5C, 5, 5),
            Frame::new(Vec3::new(x, y, z), Quat::IDENTITY),
        )
    };
    let mut world = World::new();
    let through = run(&land, &mut world, at(126.0, 123.5, 91.5), 180.0);
    let end = through[through.len() - 1];
    assert!(end.y < 112.0, "through the arch: {end:?}");

    let mut world = World::new();
    let held = run(&land, &mut world, at(135.0, 126.0, 92.5), 180.0);
    let end = held[held.len() - 1];
    assert!(end.y > 120.0, "held by what is evenly scaled: {end:?}");

    let land = land_around(w, 0x85, 0xBB);
    let start = Position::new(
        land_cell_id(0x85, 0xBB, 2, 5),
        Frame::new(Vec3::new(71.2, 130.0, 153.0), Quat::IDENTITY),
    );
    let mut world = World::new();
    let through = run(&land, &mut world, start, 0.0);
    let end = through[through.len() - 1];
    assert!(end.y > 138.0, "through the wall: {end:?}");
}

/// An object a cell places with a physics tree at an uneven scale holds nothing up: a body dropped
/// on the flat one that cell 0x03C6011D places at (153, -15, 77.2), 6 x 6 x 0.1, falls past it to
/// the cell's floor, where a body is at 77.9 against 78.46 on its top.
///
/// Behaviour: collision.objects.a-cells-object-at-an-uneven-scale-holds-nothing-up
#[test]
fn a_cells_object_at_an_uneven_scale_holds_nothing_up() {
    let w = need_world!();
    let land = Land::new();
    let mut trees = Trees::default();
    add_block_cells(&land, &w.records, &mut trees, 0x03, 0xC6);
    let start = Position::new(
        CellId(0x03C6_011D),
        Frame::new(Vec3::new(153.0, -15.0, 79.2), Quat::IDENTITY),
    );
    let mut world = World::new();
    let mut b = body(w, start, 0.0);
    world.insert_body(&land, 0x5000_0001, &mut b);
    b.contact = false;
    b.on_walkable = false;
    for _ in 0..60 {
        b.tick_in(&land, &mut world, 0.1);
    }
    let landed = b.position.frame.origin;
    assert!(
        b.contact && b.on_walkable && landed.z < 78.0,
        "below it: {landed:?}"
    );
}

/// The look whose tree is drawn on a bone 0.11 m below the place it stands.
const LIFTED_LOOK: u32 = 0x1F00_017E;

/// What a placed entity is made of, followed through the entities it is made from: the look's id,
/// the look (one group of the setup, for a setup), and the object's scale (the entity's, every
/// entity's it is made from, and the look's).
fn placed_look(w: &crate::common::World, e: &Entity) -> Option<(u32, Look, Vec3)> {
    let mut at = e.object?;
    let mut scale = Vec3::new(1.0, 1.0, 1.0);
    let mut times = |s: Option<auberean_primitives::Vec3>| {
        if let Some(s) = s {
            scale = Vec3::new(scale.x * s.x, scale.y * s.y, scale.z * s.z);
        }
    };
    times(e.scale);
    while at >> 24 == 0x47 {
        let made_from = w.records.entity(DataId(at))?;
        times(made_from.scale);
        at = made_from.object?;
    }
    let look = match at >> 24 {
        0x1F => Look::of_record(&w.records, DataId(at)).ok()?,
        0x04 => {
            let mut l = Look::default();
            l.add_group(&Group {
                key: 1,
                setup: DataId(at),
                ..Group::default()
            })
            .ok()?;
            l
        }
        _ => return None,
    };
    times(Some(look.scale));
    Some((at, look, scale))
}

/// How many parts of a group and the groups under it have physics trees, and the farthest the bone
/// of such a part's index stands from its group's origin in the setup's pose.
fn parts_with_trees(g: &PartGroup) -> (usize, f32) {
    let mut n = 0;
    let mut farthest = 0.0f32;
    for (i, p) in g.array.parts.iter().enumerate() {
        if !p.as_ref().is_some_and(|p| p.has_physics_tree()) {
            continue;
        }
        n += 1;
        if let Some(b) = g.bone_matrix(i) {
            let o = b.0[3];
            farthest = farthest.max((o[0] * o[0] + o[1] * o[1] + o[2] * o[2]).sqrt());
        }
    }
    for c in &g.children {
        let (k, f) = parts_with_trees(c);
        n += k;
        farthest = farthest.max(f);
    }
    (n, farthest)
}

/// What placed objects were compared, and how often a sphere met them.
#[derive(Default)]
struct Tally {
    objects: usize,
    /// The farthest the bone of a part with a tree stands from its group's origin.
    farthest_bone: f32,
    lifted: usize,
    met: usize,
    tried: usize,
}

/// Each placed entity of `entities` (in `cell`) whose parts have physics trees, against its solid
/// parts, the next of `solids` in order: a sphere placed around it (on a grid out to a meter and a
/// half each way, from a meter below it to three above, a tenth of a meter apart up and down) meets
/// its solid parts exactly where it meets its parts put together from its look, standing where it
/// stands. Every one of `solids` is some entity's.
fn collide_alike<'a>(
    w: &crate::common::World,
    ctx: &TransitionCtx<'_>,
    entities: impl Iterator<Item = &'a Entity>,
    solids: &[Solid],
    cell: CellId,
    tally: &mut Tally,
) {
    let mut next = 0;
    for e in entities {
        if !e.kind.is_some_and(|k| DRAWN_KINDS.contains(&k)) {
            continue;
        }
        let Some((look_id, look, scale)) = placed_look(w, e) else {
            continue;
        };
        let mut parts = PartGroup::default();
        if !parts.init_look(&w.records, &look) || !parts.cache_has_physics_tree() {
            continue;
        }
        tally.objects += 1;
        let (n, farthest) = parts_with_trees(&parts);
        tally.farthest_bone = tally.farthest_bone.max(farthest);
        let mine = solids.get(next..next + n).expect("a solid for each tree");
        next += n;
        if look_id == LIFTED_LOOK {
            tally.lifted += 1;
        }
        let f = e.offset.as_ref().expect("placed somewhere").frame;
        let (o, q) = (f.origin, f.rotation);
        let at = Position::new(
            cell,
            Frame::new(Vec3::new(o.x, o.y, o.z), Quat::new(q.w, q.x, q.y, q.z)),
        );
        for (x, y, z) in (-2..=2)
            .flat_map(|x| (-2..=2).map(move |y| (x, y)))
            .flat_map(|(x, y)| (-10..=30).map(move |z| (x, y, z)))
        {
            #[allow(clippy::cast_precision_loss)]
            let p = Vec3::new(
                o.x + x as f32 * 0.75,
                o.y + y as f32 * 0.75,
                o.z + z as f32 * 0.1,
            );
            let pos = Position::new(cell, Frame::new(p, Quat::IDENTITY));
            let solid = mine
                .iter()
                .map(|s| {
                    object_part_find_obj_collisions_scaled(
                        ctx,
                        &mut placing_at(&pos),
                        &s.part,
                        TreeScale::axes(s.scale),
                    )
                })
                .find(|r| *r != TransitionState::Ok)
                .unwrap_or(TransitionState::Ok);
            let retail = parts.find_obj_collisions(ctx, &mut placing_at(&pos), &at, scale);
            assert_eq!(solid, retail, "{look_id:#x} in {cell:?} at {p:?}");
            tally.tried += 1;
            if retail != TransitionState::Ok {
                tally.met += 1;
            }
        }
    }
    assert_eq!(next, solids.len(), "every solid of {cell:?} is an object's");
}

/// What the world places collides where the client tests it, every part of an object in the
/// object's own frame, whatever bone it is drawn on: the solid parts of every object the hall's
/// cells place, and of every object the land of block 0x3AA8 places, meet a sphere placed around
/// the object exactly where its parts put together from its look do, standing where it stands.
/// Among them the look whose tree is drawn on a bone 0.11 m below its place collides at its place,
/// not 0.11 m lower, and outside, trees whose setups' poses put the part with the physics tree on
/// a bone 8 m up collide at their places too.
///
/// Behaviour: parts.data.placed-objects-collide-in-their-own-frame
#[test]
fn placed_objects_collide_in_their_own_frame() {
    let w = need_world!();
    let (land, mut trees) = hall();
    let objects: Arena<PhysicsObj> = Arena::default();
    let cells = BTreeMap::new();
    let ctx = TransitionCtx {
        land: &land,
        objects: &objects,
        cells: &cells,
        mover: None,
        object_table: None,
        entry_host: None,
    };
    let info = w.records.landblock_info(0x02, 0x98).expect("the hall");
    let mut inside = Tally::default();
    for k in 0..info.cell_count {
        let Some((low, cell)) = u16::try_from(0x100 + k)
            .ok()
            .and_then(|low| Some((low, w.records.cell(0x02, 0x98, low)?)))
        else {
            continue;
        };
        let id = CellId(0x0298_0000 | u32::from(low));
        let solids = land.cell_solids(id).unwrap_or_default();
        let entities = cell.entities.iter().flat_map(|g| g.entities.iter());
        collide_alike(w, &ctx, entities, &solids, id, &mut inside);
    }
    eprintln!(
        "inside: {} objects with trees, {} of {} placements met",
        inside.objects, inside.met, inside.tried
    );
    assert!(
        inside.lifted > 0,
        "the hall places the look drawn on a lowered bone"
    );
    assert!(inside.met > 0 && inside.met < inside.tried);

    let block = build(&w.records, &w.region, &mut trees, 0x3A, 0xA8).expect("block 0x3AA8");
    let info = w.records.landblock_info(0x3A, 0xA8).expect("its objects");
    let mut outside = Tally::default();
    collide_alike(
        w,
        &ctx,
        info.entities.entities.iter(),
        &block.solids,
        land_cell_id(0x3A, 0xA8, 0, 0),
        &mut outside,
    );
    eprintln!(
        "outside: {} objects with trees, {} of {} placements met, trees' bones up to {} m away",
        outside.objects, outside.met, outside.tried, outside.farthest_bone
    );
    assert!(outside.farthest_bone > 8.0, "{}", outside.farthest_bone);
    assert!(outside.met > 0 && outside.met < outside.tried);
}

/// The world's records keep one loaded setup for each setup: what one part array of the human
/// male's setup changes in it (its step-down height, its spheres) another array of it sees, and so
/// does whatever reads the setup from the records afterwards.
///
/// Behaviour: parts.data.the-worlds-records-keep-one-loaded-setup
#[test]
fn the_worlds_records_keep_one_loaded_setup() {
    // A world of its own: the change stays in it.
    let w = crate::common::open_world();
    let id = DataId(0x0400_0029);
    let (mut a, mut b) = (PartArray::default(), PartArray::default());
    assert!(a.set_setup(&w.records, id) && b.set_setup(&w.records, id));
    let before = b.step_down_height();
    assert!(!b.spheres().is_empty());
    assert!(a.set_step_down_height(before + 1.0) && a.set_spheres(&[]));
    assert_eq!(b.step_down_height(), before + 1.0);
    assert!(b.spheres().is_empty());
    let read = w.records.setup(id).expect("the setup");
    assert_eq!(
        (read.step_down_height, read.spheres.len()),
        (before + 1.0, 0)
    );
}

/// Blocks built along the columns from the landblocks kept near them come out as those built
/// alone, and what is kept is only the columns beside the one being built.
///
/// Behaviour: server.operator.start-reads-each-landblock-once
#[test]
fn blocks_built_from_the_landblocks_kept_near_them_are_as_those_built_alone() {
    let w = need_world!();
    let (mut alone, mut along) = (Trees::default(), Trees::default());
    let mut near = NearBlocks::default();
    for (x, y) in [(0x75, 0xB9), (0x75, 0xBA), (0x76, 0xB9)] {
        let a = build(&w.records, &w.region, &mut alone, x, y).expect("a block");
        let b = build_near(&w.records, &w.region, &mut along, &mut near, x, y).expect("a block");
        assert_eq!(a.geometry.vertices.len(), b.geometry.vertices.len());
        let heights = |k: &auberean_physics::land::Block| -> Vec<u32> {
            k.geometry.vertices.iter().map(|v| v.z.to_bits()).collect()
        };
        assert_eq!(heights(&a), heights(&b));
        assert_eq!(a.solids.len(), b.solids.len());
        assert_eq!(a.objects.len(), b.objects.len());
        let materials = |k: &auberean_physics::land::Block, i: usize, j: usize| {
            k.materials.as_ref().map(|m| m.vertices.ground[9 * i + j])
        };
        for (i, j) in [(0, 0), (4, 4), (8, 8)] {
            assert_eq!(materials(&a, i, j), materials(&b, i, j));
        }
    }
    assert_eq!(
        near.kept(),
        11,
        "the columns beside the last built: 4, 4 and 3 blocks"
    );
    build_near(&w.records, &w.region, &mut along, &mut near, 0x80, 0xB9).expect("a block");
    assert_eq!(near.kept(), 9, "a far column forgets the others");
}
