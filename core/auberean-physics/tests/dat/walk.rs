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
