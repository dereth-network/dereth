//! The remaster across the landscape's light tick: the tick re-lights the land in place and
//! builds no block again, and the remaster, which lights the land from the frame's sun rather
//! than from the blocks' baked colours, still follows the sun across it. Its frame after the
//! tick is the one it draws once every block has been built again under the same light.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware Vulkan 1.3 device.

#![cfg(gpu)]
#![cfg(feature = "remaster")]

use std::sync::Arc;

use dereth_render::device::{DeviceConfig, Gpu};
use dereth_scene::world_scene::{RemasterLight, RemasterSettings, SceneReads};

use crate::instruments::remaster_frames;
use crate::instruments::stations::{capture, Clock, Shot};

const SIZE: (u32, u32) = (480, 270);

/// The frame as presented, and which of its pixels the remaster drew the world on.
struct Picture {
    rgba: Vec<u8>,
    world: Vec<bool>,
}

fn picture(shot: &Shot, gpu: &mut Gpu) -> Picture {
    let rgba = capture(gpu);
    let (w, h) = gpu.size();
    let c = shot
        .scene
        .draw
        .remaster_capture(gpu)
        .expect("the remaster drew");
    let mut world = vec![false; (w * h) as usize];
    for y in 0..c.height {
        for x in 0..c.width {
            let (fx, fy) = (x + c.x, y + c.y);
            if fx < w && fy < h {
                world[(fy * w + fx) as usize] = c.depth[(y * c.width + x) as usize] < 1.0;
            }
        }
    }
    Picture { rgba, world }
}

/// The pixels of `a` and `b` that differ, of those `a` drew the world on; and how many it drew
/// the world on.
fn world_differing(a: &Picture, b: &Picture) -> (usize, usize) {
    let pixels = a
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .zip(b.rgba.as_chunks::<4>().0);
    let mut differ = 0;
    let mut world = 0;
    for (k, (p, q)) in pixels.enumerate() {
        if a.world[k] {
            world += 1;
            differ += usize::from(p != q);
        }
    }
    (differ, world)
}

/// Behaviour: remaster.light.the-remaster-follows-the-sun-across-the-light-tick
/// **The remaster follows the sun across the light tick, with no block built again.** In the
/// town in the morning, one step a long way on in game time is one light tick: it builds no
/// block, the land the remaster draws changes as the sun moved, and its frame then is the frame
/// it draws once every block has been built again under the same light. With retail's light
/// worked out per vertex and per pixel alike.
///
/// The clock is the station's injected one; the frames compared are drawn with it held.
///
/// Oracle: the retail dats, and the remaster's own frame of the blocks built again.
#[test]
fn the_remaster_follows_the_sun_across_the_light_tick_without_a_block_built_again() {
    let store = Arc::new(crate::common::dat_store());
    let mut station = remaster_frames::stations(&store, &["holtburg"])
        .pop()
        .expect("the town");
    station.clock = Clock::Sunny(0.3);
    for light in [RemasterLight::Vertex, RemasterLight::Pixel] {
        let cfg = DeviceConfig {
            width: SIZE.0,
            height: SIZE.1,
            remaster: true,
            ..DeviceConfig::default()
        };
        let mut gpu = Gpu::new_on(crate::common::remaster_backend(), None, &cfg).expect("a device");
        assert_eq!(
            gpu.adapter_kind(),
            dereth_render::device::AdapterKind::Hardware,
            "{} is a software rasterizer",
            gpu.adapter_name()
        );
        let mut shot = Shot::open(&store, &mut gpu, &station);
        shot.poll(&mut gpu);
        shot.scene
            .draw
            .set_remaster(true, &store, &mut gpu)
            .expect("the remaster starts");
        shot.scene
            .draw
            .set_remaster_settings(RemasterSettings::off(), &mut gpu);
        shot.scene.draw.set_remaster_light(light, &mut gpu);
        for k in 0..6 {
            shot.step(&mut gpu);
            if k == 0 {
                shot.scene.wait_for_remaster_ground();
            }
        }
        assert_eq!(
            shot.scene.pending_slot_count(),
            0,
            "{light:?}: the land is still building"
        );
        // From here on a frame moves nothing on: the clouds, the water and what sways hold
        // still, so two frames of the same light are the same frame.
        shot.frame_seconds = 0.0;
        shot.step_held(&mut gpu);
        let before = picture(&shot, &mut gpu);
        let sun = shot.scene.landscape_lighting().sunlight;
        let built = shot.scene.draw.stats.blocks_built;
        let relit = shot.scene.draw.stats.blocks_relit;

        // Ten minutes of clock in one step: one light tick.
        shot.step_by(&mut gpu, 600.0);
        assert!(
            shot.scene.draw.stats.blocks_relit > relit,
            "{light:?}: no light tick in the step"
        );
        assert_eq!(
            shot.scene.draw.stats.blocks_built, built,
            "{light:?}: the light tick built blocks again"
        );
        assert_ne!(
            shot.scene.landscape_lighting().sunlight,
            sun,
            "the sun did not move"
        );
        for _ in 0..3 {
            shot.step_held(&mut gpu);
        }
        let after = picture(&shot, &mut gpu);
        let (moved, world) = world_differing(&before, &after);
        assert!(
            world > before.world.len() / 4 && moved * 2 > world,
            "{light:?}: {moved} of the {world} pixels of the world changed across the light tick"
        );

        // Every block built again under the same light: the remaster draws the same frame.
        shot.scene
            .draw
            .rebuild_resident_blocks(&mut shot.scene.world);
        assert!(shot.scene.pending_slot_count() > 0, "no block was queued");
        shot.step_held(&mut gpu);
        shot.scene.wait_for_remaster_ground();
        assert_eq!(
            shot.scene.pending_slot_count(),
            0,
            "the blocks are still building"
        );
        assert!(
            shot.scene.draw.stats.blocks_built > built,
            "no block was built again"
        );
        for _ in 0..3 {
            shot.step_held(&mut gpu);
        }
        let rebuilt = picture(&shot, &mut gpu);
        let differing = after
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .zip(rebuilt.rgba.as_chunks::<4>().0)
            .filter(|(a, b)| a != b)
            .count();
        assert_eq!(
            differing, 0,
            "{light:?}: {differing} pixels differ from the frame of the blocks built again"
        );
        eprintln!(
            "{light:?}: {moved} of {world} world pixels moved with the sun; the rebuilt frame is \
             the same"
        );
        shot.scene
            .draw
            .set_remaster(false, &store, &mut gpu)
            .expect("the remaster stops");
        drop(shot);
        drop(gpu);
    }
}
