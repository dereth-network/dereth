//! The seam between a scale hook and the physics body: an `AnimEvent::SetScale` raised inside
//! `WorldScene::update`'s own animation step reaches the local body's scale and collision radius.
//! Everything either side of this routing is covered without a device by the scale-hook scenario
//! in `dereth/testkit/tests/dat/world.rs`; this module exists because `WorldScene` needs a device.
//!
//! A `SCALE` hook reaches the client from a physics script (every shipped one is in a physics
//! script, none in an animation). `Character` pairs a local body with a `MotionDriver`, the same
//! pair a scaled creature has, so the body plays the shipped doubling script through the normal
//! per-object script path. Fixture: the retail dats and Holtburg's default scene on a software
//! device.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use std::sync::Arc;

use dereth_client::character::CharacterInput;
use dereth_client::world::{SceneConfig, WorldScene};
use dereth_dat::RetailDatStore;
use dereth_primitives::{DataId, LocalTime};

/// The shipped script whose first hook is `SCALE end = 2.0, time = 0`; the scale-hook behavior in
/// `dereth/testkit/tests/dat/world.rs` asserts that shape against the dat.
const DOUBLE: DataId = DataId(0x3300_0117);

fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

/// Behaviour: world.physics-script.a-scale-hook-moves-the-objects-own-collision-radius
///
/// A scale hook stores the new scale on the physics object and then on the object's part array.
/// This asserts the physics-object store and the collision radius that reads it.
#[test]
fn a_scale_hook_fired_inside_the_frame_reaches_the_physics_object() {
    let mut gpu = crate::common::test_gpu(800, 600);
    let store = store();
    let region = dereth_client::world::load_region(&store).expect("region");

    let mut scene = WorldScene::load(&store, &mut gpu, SceneConfig::default()).expect("loads");
    scene
        .attach_character(&store, &region, &mut gpu)
        .expect("the character attaches");

    let step = |scene: &mut WorldScene, t: &mut f64, frames: u32| {
        for _ in 0..frames {
            *t += 1.0 / 30.0;
            scene.update(
                dereth_client::camera::CameraInput::default(),
                CharacterInput::default(),
                LocalTime(*t),
                1.0 / 30.0,
            );
        }
    };
    let body = |scene: &WorldScene| -> (f32, f32) {
        let c = scene.character.as_ref().expect("attached");
        let o = c.world.get(c.handle).expect("live");
        (o.scale, o.radius())
    };

    let mut t = 0.0;
    step(&mut scene, &mut t, 10);
    let (scale0, radius0) = body(&scene);
    assert_eq!(
        scene.draw.stats.ethereal.scale_hooks, 0,
        "no SCALE hook has fired yet"
    );
    assert!(radius0 > 0.0, "the body has a collision radius to scale");

    // Queue the physics script on the body's own motion driver.
    {
        let c = scene.character.as_ref().expect("attached");
        assert!(
            c.driver_mut().play_script_internal(DOUBLE),
            "the shipped doubling script would not queue on the body"
        );
    }
    step(&mut scene, &mut t, 30);

    let s = scene.draw.stats.ethereal;
    let (scale1, radius1) = body(&scene);
    eprintln!(
        "scale-hook seam: {} SCALE hook(s), {} reached no body; object scale {scale0} -> {scale1}, \
         radius {radius0} -> {radius1}",
        s.scale_hooks, s.scale_hooks_no_body
    );
    assert!(
        s.scale_hooks > 0,
        "no SetScale event reached the seam at all"
    );
    assert_eq!(
        s.scale_hooks_no_body, 0,
        "the hook reached the seam and found no physics object"
    );
    assert!(
        (scale1 - 2.0).abs() < 1e-6,
        "the hook fired and was counted, and object scale is still {scale1} -- the seam is not \
         reaching the physics object's scale"
    );
    assert!(
        (radius1 - radius0 * 2.0 / scale0).abs() < 1e-4,
        "object scale moved and the collision radius did not follow it: {radius0} -> {radius1}"
    );
}
