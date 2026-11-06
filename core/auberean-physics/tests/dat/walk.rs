//! The player's body run through the September 2013 world.
//! Fixture: landblock 0x6FA9 and its neighbors; the arrival point of the 2017 sessions' commonest
//! entry block; the human male's collision spheres and step heights and the player's motion values.

use auberean_assets::animation::MotionDesc;
use auberean_assets::setup::Setup;
use auberean_physics::body::{setup_spheres, Body, BodyConfig};
use auberean_physics::land::{cell_of, land_cell_id, Land};
use auberean_physics::mesh::Trees;
use auberean_physics::motion::MotionInput;
use dereth_primitives::{Frame, Position, Quat, Vec3};

use crate::common::need_world;

/// Run north from the arrival point at 0x6FA9: the body keeps to the ground over the rising
/// lawn, is turned aside by the first building's pillars, and comes to rest against its wall,
/// still asking to run.
///
/// Behaviour: motion.walk.a-running-body-keeps-to-the-ground-and-stops-at-a-wall
#[test]
fn a_running_body_keeps_to_the_ground_and_stops_at_a_wall() {
    let w = need_world!();
    let land = Land::new();
    let mut trees = Trees::default();
    for x in 0x6E..=0x70u8 {
        for y in 0xA8..=0xAAu8 {
            land.add_block(&w.records, &w.region, &mut trees, x, y);
        }
    }
    let setup: Setup = w.records.get(0x0400_0029).expect("the human setup");
    let md: MotionDesc = w.records.get(0x3200_0000).expect("the player's motion");
    let pick = |element: u32| {
        let (_, set) = md
            .set_of
            .iter()
            .find(|(k, _)| k.element == element && k.mode == 0x4000_0001)
            .expect("a set");
        md.sets[*set as usize].default
    };
    let config = BodyConfig {
        spheres: setup_spheres(&setup),
        step_up: setup.step_up_height,
        step_down: setup.step_down_height,
        scale: 1.0,
        ground: pick(0x4000_0001),
        air: pick(0x4000_0002),
    };
    let at = Vec3::new(45.24, 90.51, 85.26);
    let (cx, cy) = cell_of(at).expect("in the block");
    let start = Position::new(
        land_cell_id(0x6F, 0xA9, cx, cy),
        Frame::new(at, Quat::IDENTITY),
    );
    let mut body = Body::new(config, start, 0.0);
    let mut lowest_gap = f32::MAX;
    let mut track = Vec::new();
    for k in 0..100 {
        body.steer(MotionInput {
            forward: if k >= 5 { 1.0 } else { 0.0 },
            ..MotionInput::default()
        });
        body.tick(&land, 0.1);
        let p = body.position.frame.origin;
        let block = body.position.cell.landblock();
        if let Some((ground, _)) =
            land.ground(dereth_primitives::LandblockId::new(block.x(), block.y()), p)
        {
            lowest_gap = lowest_gap.min(p.z - ground);
        }
        track.push(p);
    }
    let end = track[track.len() - 1];
    let ten_ticks_before = track[track.len() - 11];
    let moved =
        ((end.x - ten_ticks_before.x).powi(2) + (end.y - ten_ticks_before.y).powi(2)).sqrt();
    assert!(
        lowest_gap > -0.05,
        "never sinks into the ground: {lowest_gap}"
    );
    assert!(end.y > 120.0, "it ran north: {end:?}");
    assert!(
        (end.x - at.x).abs() > 2.0,
        "turned aside on the way: {end:?}"
    );
    assert!(
        moved < 0.05,
        "at rest against the wall: moved {moved} m in the last second"
    );
    assert!(body.contact, "standing");
}

/// What a body stands on names a physics material of the world's: the ground's by the vertex nearest
/// it, a road's on a road, and a lake's block names its water's. (Block `0x75B9`, Arwic's roads and
/// fields; block `0x9889`, a lake.)
///
/// Behaviour: motion.collision.a-body-knows-the-ground-and-water-it-touches
#[test]
fn a_body_knows_the_ground_and_water_it_touches() {
    use auberean_assets::property::PhysicsMaterial;
    let w = crate::common::need_world!();
    let land = auberean_physics::land::Land::new();
    let mut trees = auberean_physics::mesh::Trees::default();
    assert!(land.add_block(&w.records, &w.region, &mut trees, 0x75, 0xB9));
    let block = land.block(0x75, 0xB9).expect("the block");
    let mut seen = std::collections::BTreeMap::<u32, u32>::new();
    for i in 0..160 {
        for j in 0..160 {
            #[allow(clippy::cast_precision_loss)]
            let at = dereth_primitives::Vec3::new(i as f32 + 0.5, j as f32 + 0.5, 0.0);
            *seen.entry(block.physics_material(at)).or_insert(0) += 1;
        }
    }
    let named = |id: u32| w.records.get::<PhysicsMaterial>(id).is_some();
    assert!(seen.keys().all(|&id| id == 0 || named(id)), "{seen:x?}");
    assert!(seen.len() >= 3, "ground and road: {seen:x?}");
    assert!(seen.get(&0).copied().unwrap_or(0) < 400, "{seen:x?}");
    assert!(land.add_block(&w.records, &w.region, &mut trees, 0x98, 0x89));
    let water = land.water_material(dereth_primitives::CellId(0x9889_000B));
    assert!(named(water), "{water:#x}");
    assert_eq!(
        land.water_material(dereth_primitives::CellId(0x75B9_0001)),
        0
    );
}

/// An interior floor names its own physics material: the room's, whichever cell of it a body stands
/// in, the first cells of three blocks each with their own; a cell that names none touches nothing.
///
/// Behaviour: motion.interior.a-room-names-the-material-of-its-floor
#[test]
fn a_room_names_the_material_of_its_floor() {
    let w = crate::common::need_world!();
    let land = auberean_physics::land::Land::new();
    let mut trees = auberean_physics::mesh::Trees::default();
    let at = dereth_primitives::Vec3::ZERO;
    for (x, y, low, material) in [
        (0x02, 0x88, 0x0100, 0x3700_0002),
        (0x01, 0xDD, 0x0100, 0x3700_0004),
        (0x03, 0xE6, 0x0100, 0x3700_0003),
    ] {
        auberean_physics::interior::add_block_cells(&land, &w.records, &mut trees, x, y);
        let id = dereth_primitives::CellId(u32::from(x) << 24 | u32::from(y) << 16 | low);
        assert_eq!(land.ground_material(id, at), material, "{id:?}");
        assert!(w
            .records
            .get::<auberean_assets::property::PhysicsMaterial>(material)
            .is_some());
    }
    auberean_physics::interior::add_block_cells(&land, &w.records, &mut trees, 0x06, 0xFB);
    assert_eq!(
        land.ground_material(dereth_primitives::CellId(0x06FB_0266), at),
        0
    );
    assert_eq!(
        land.ground_material(dereth_primitives::CellId(0x0298_FFF0), at),
        0,
        "not loaded"
    );
}
