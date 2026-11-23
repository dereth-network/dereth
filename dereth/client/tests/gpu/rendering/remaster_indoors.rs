//! Behaviour: none (experimental)
//!
//! The remaster's outdoor additions are drawn only where the frame draws the outdoors. Indoors,
//! the outdoors reaches a frame only through an opening that shows it, and past an opening that
//! shows nothing retail draws nothing, the black clear standing there: so in a sealed room of a
//! dungeon the remaster writes no far trees and draws no water, where outdoors it does both.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware Vulkan 1.3 device.

#![cfg(gpu)]
#![cfg(feature = "remaster")]

use dereth_render::device::{DeviceConfig, Gpu};
use dereth_scene::world_scene::{RemasterTerrain, RemasterWater};

use crate::instruments::remaster_frames;
use crate::instruments::stations::{Clock, Place, Shot, Station};

/// A room of the training academy's dungeon, which no opening of shows the outdoors.
const SEALED_CELL: u32 = 0x8602_0140;

/// What the remaster drew at `station` with its ground, its far forest and its water on: the far
/// trees it wrote, and whether it drew water.
fn drawn_at(station: &Station) -> (u32, bool) {
    let store = crate::common::dats();
    let cfg = DeviceConfig {
        width: 960,
        height: 540,
        remaster: true,
        ..DeviceConfig::default()
    };
    let mut gpu = Gpu::new_on(crate::common::remaster_backend(), None, &cfg).expect("a device");
    let mut shot = Shot::open(&store, &mut gpu, station);
    shot.poll(&mut gpu);
    shot.scene
        .draw
        .set_remaster(true, &store, &mut gpu)
        .expect("the remaster starts");
    shot.scene.draw.set_remaster_terrain(
        RemasterTerrain {
            splat: true,
            grid: true,
            drawn: true,
            horizon: true,
        },
        &mut gpu,
    );
    let w = shot.scene.draw.remaster_water();
    shot.scene
        .draw
        .set_remaster_water(RemasterWater { on: true, ..w }, &mut gpu);
    shot.step_held(&mut gpu);
    shot.scene.wait_for_remaster_ground();
    // The far trees come to the device a few models a frame.
    for _ in 0..200 {
        shot.step_held(&mut gpu);
        let s = shot.scene.draw.remaster_forest_stats(&mut gpu);
        if s.is_none_or(|s| s.wanted == 0 || s.pictures >= s.wanted) {
            break;
        }
    }
    for _ in 0..4 {
        shot.step_held(&mut gpu);
    }
    let trees = shot
        .scene
        .draw
        .remaster_forest_stats(&mut gpu)
        .map_or(0, |s| s.standing);
    let water = shot
        .scene
        .draw
        .remaster_counts(&mut gpu)
        .is_some_and(|c| c.water);
    shot.scene
        .draw
        .set_remaster(false, &store, &mut gpu)
        .expect("the remaster stops");
    (trees, water)
}

/// In a sealed room of a dungeon the remaster writes no far trees and draws no water; in Holtburg
/// it writes far trees, and at the shore it draws water.
#[test]
fn in_a_sealed_room_the_remaster_draws_no_far_forest_and_no_water() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let outdoors = remaster_frames::stations(&store, &["holtburg", "shore"]);
    let (trees, _) = drawn_at(&outdoors[0]);
    assert!(trees > 0, "Holtburg: far trees written");
    let (_, water) = drawn_at(&outdoors[1]);
    assert!(water, "the shore: water drawn");
    let origin = super::indoor_outdoor_gate::point_in(&store, SEALED_CELL)
        .expect("a point inside the sealed room");
    let sealed = Station {
        name: "sealed",
        why: "a room of a dungeon no opening of shows the outdoors",
        place: Place::Body {
            cell: SEALED_CELL,
            origin,
            yaw: 0.0,
        },
        clock: Clock::Sunny(0.5),
        weather: false,
        land_radius: 2,
    };
    let (trees, water) = drawn_at(&sealed);
    eprintln!("the sealed room: {trees} far trees written, water drawn {water}");
    assert_eq!(trees, 0, "no far trees in the sealed room");
    assert!(!water, "no water in the sealed room");
}
