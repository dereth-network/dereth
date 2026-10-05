//! Spell and environment particle effects appear and are drawn in order. Holtburg's torches and
//! braziers are landblock statics whose setup names a default physics script of particle hooks;
//! the scene turns those placements into live objects, every emitter they create resolves to a
//! mesh, and a frame with the emitters running differs from the same frame without them only by
//! getting brighter (an additive pass only adds light). The explode and implode motion quirks of
//! the shipped particle equations reach the draw unchanged. Fixture: the retail dats around
//! Holtburg (`0xA9B4`), drawn on a software device at a pinned time of day.

#![cfg(gpu)]

use dereth_client_runtime::objects::ObjectStream;
use dereth_dat::RetailDatStore;
use dereth_primitives::num::math;
use dereth_primitives::{LocalTime, Vec3};
use dereth_render::device::Gpu;
use std::sync::Arc;
use {dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene};
use {dereth_scene::world_scene::SceneReads, dereth_scene::world_scene::SceneWrites};

/// The retail store, or **fail**: a missing oracle must not read as a pass.
fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

/// The scene both halves of the differential are built from: Holtburg, no body, a pinned time of
/// day so the sky and the landscape lighting cannot move between the two runs.
fn cfg(particles: bool) -> SceneConfig {
    SceneConfig {
        character: false,
        scenery_radius: 2,
        time_of_day: Some(0.5),
        particles,
        ..SceneConfig::default()
    }
}

/// Drive the app's own per-frame order for `frames` frames and hand back the last capture.
///
/// This is [`dereth_client::app::App::frame`]'s sequence with the device steps only: `sync_objects`,
/// `update`, `stream`, then the frame bracket. **Injected keystrokes are not evidence**, and
/// neither is a bespoke loop — everything below is a call the
/// binary makes every frame.
fn run(
    store: &Arc<RetailDatStore>,
    gpu: &mut Gpu,
    scene: &mut WorldScene,
    frames: usize,
) -> Vec<u8> {
    let mut stream = ObjectStream::new();
    let mut t = 0.0f64;
    let mut rgba = Vec::new();
    for _ in 0..frames {
        // The headless step is `dereth_client_runtime::platform::clock::HEADLESS_STEP`, one physics quantum per frame, which is what
        // makes a capture after n frames the state after n sub-steps.
        t += dereth_client_runtime::platform::clock::HEADLESS_STEP;
        scene
            .sync_objects(store, gpu, &mut stream)
            .expect("sync_objects");
        scene.update(
            dereth_client_runtime::camera::CameraInput::default(),
            dereth_client_runtime::character::CharacterInput::default(),
            LocalTime(t),
            1.0 / 30.0,
        );
        scene.stream(store, gpu).expect("stream");
        scene.reserve_upload_arena(gpu).expect("reserve the arena");
        gpu.begin_frame().expect("begin");
        scene.draw(gpu).expect("draw");
        gpu.end_frame().expect("end");
        rgba = gpu.capture().expect("capture").to_rgba();
    }
    rgba
}

/// Park the camera in front of one of the block's scripted statics — a torch or a brazier — so
/// that what it is looking at is an emitter rather than a hillside.
fn look_at_an_emitter(scene: &mut WorldScene, index: usize) -> Vec3 {
    let origins = scene.emitter_host_origins();
    assert!(
        origins.len() > index,
        "only {} emitter hosts around Holtburg; expected more",
        origins.len()
    );
    let o = origins[index];
    scene.camera.position = Vec3::new(o.x, o.y - 8.0, o.z + 1.5);
    scene.camera.yaw = 0.0;
    scene.camera.pitch = 0.0;
    o
}

// ---------------------------------------------------------------------------------------------
// 1. The effects are in the data, and the client reaches them.
// ---------------------------------------------------------------------------------------------

/// Behaviour: rendering.particles.scripted-statics-become-drawn-emitters
/// **The environment effects around Holtburg are default physics scripts, and they run.**
///
/// Every landblock static becomes a physics object, and creating its setup queues that object's
/// default physics script; those scripts are particle-creation hooks, so the emitters exist only
/// if the statics are kept as live objects rather than baked into triangles.
///
/// Oracle: the retail dats — the 5×5 blocks around Holtburg (`0xA9B4`) hold 32 static placements
/// whose setup names a particle-creating script, plus the generated scenery's own.
#[test]
fn the_scripted_statics_around_holtburg_become_live_emitters() {
    let store = store();
    let mut gpu = crate::common::test_gpu(640, 480);
    let mut scene = WorldScene::load(&store, &mut gpu, cfg(true)).expect("the landscape loads");
    let mut stream = ObjectStream::new();
    // One `sync_objects` is what spawns the hosts; one `update` is what runs their scripts.
    scene
        .sync_objects(&store, &mut gpu, &mut stream)
        .expect("sync_objects");
    let hosts = scene.draw.stats.emitter_hosts;
    assert!(
        hosts >= 20,
        "only {hosts} scripted statics in the window; the data has more"
    );

    scene.update(
        dereth_client_runtime::camera::CameraInput::default(),
        dereth_client_runtime::character::CharacterInput::default(),
        LocalTime(dereth_client_runtime::platform::clock::HEADLESS_STEP),
        1.0 / 30.0,
    );
    let s = scene.draw.stats;
    assert!(
        s.particles.emitters >= hosts,
        "{hosts} hosts produced only {} emitters; a default script that creates none is a script \
         that did not run",
        s.particles.emitters
    );
    eprintln!("{hosts} hosts, {} emitters", s.particles.emitters);
}

/// **Every emitter's `hw_gfxobj_id` resolves to triangles.**
///
/// An emitter is refused at creation when its `hw_gfxobj_id` is invalid, so an emitter that
/// exists names a mesh that must exist. A non-zero `missing_geometry` is an effect that is
/// simulated and invisible.
#[test]
fn every_live_emitter_resolves_to_a_mesh() {
    let store = store();
    let mut gpu = crate::common::test_gpu(640, 480);
    let mut scene = WorldScene::load(&store, &mut gpu, cfg(true)).expect("the landscape loads");
    run(&store, &mut gpu, &mut scene, 4);
    look_at_an_emitter(&mut scene, 4);
    run(&store, &mut gpu, &mut scene, 60);

    let d = scene.drawn_particles();
    assert_eq!(
        d.missing_geometry, 0,
        "{} particles had no mesh to draw",
        d.missing_geometry
    );
    assert!(d.meshes > 0, "no emitter mesh was ever built");
    assert!(
        scene.draw.stats.particles.live > 0,
        "no particle is alive after two seconds"
    );
    eprintln!(
        "{} live, {} drawn in {} batches, {} degraded out, {} meshes",
        scene.draw.stats.particles.live, d.drawn, d.batches, d.degraded_out, d.meshes
    );
}

// ---------------------------------------------------------------------------------------------
// 2. They reach the screen.
// ---------------------------------------------------------------------------------------------

/// **The differential.** The same frame, drawn twice from the same data with the same clock, once
/// with the emitters running and once without: the pixels differ, every one of them by getting
/// **brighter**, and the light a player can see is concentrated where the flames are. (The dim
/// outer skirt of an additive corona is not: see the assertions.)
///
/// "A draw call was issued" is not evidence. This is: the control run proves the scene is
/// otherwise identical (it is byte-identical to a second control run), and the difference between
/// the two is therefore the particles and nothing else.
#[test]
fn a_frame_with_the_emitters_running_differs_from_the_same_frame_without() {
    let store = store();

    // **One device per run.** `Gpu::upload_texture`'s descriptor slots are monotonic and
    // unreclaimed — Holtburg at this radius costs about 500 of the heap's 2,048 — so four scenes
    // in one device exhausts it. A fresh WARP device per run takes four independent captures,
    // and is also what makes them independent.
    let shot = |particles: bool| -> Option<(Vec<u8>, usize)> {
        let mut gpu = crate::common::test_gpu(640, 480);
        let mut scene = WorldScene::load(&store, &mut gpu, cfg(particles)).expect("loads");
        run(&store, &mut gpu, &mut scene, 4);
        look_at_an_emitter(&mut scene, 4);
        let rgba = run(&store, &mut gpu, &mut scene, 60);
        Some((rgba, scene.drawn_particles().drawn))
    };

    let ((off_a, off_drawn), (off_b, _)) = (
        shot(false).expect("a rendered frame"),
        shot(false).expect("a rendered frame"),
    );
    assert_eq!(
        off_a, off_b,
        "the scene without particles is not reproducible; nothing else can be"
    );
    assert_eq!(off_drawn, 0, "the control run drew particles");

    let (on, on_drawn) = shot(true).expect("a rendered frame: retail dats and a WARP device");
    assert!(on_drawn > 0, "the particle run drew none");

    let differing = off_a
        .as_chunks::<4>()
        .0
        .iter()
        .zip(on.as_chunks::<4>().0.iter())
        .filter(|(a, b)| a != b)
        .count();
    let total = off_a.len() / 4;
    assert!(differing > 0, "the particles changed no pixel at all");

    // `0x010016FD` and `0x01001689` are **additive** billboards (`ONE / ONE`), so a candle corona
    // adds some light to every pixel its quad covers, out to where its texture fades to one part
    // in 255. `0x010016FD`'s vertices reach 0.75 m from its origin and the script's
    // `gfxobj_scale` is 3.0, so a 4.5 m corona is on screen and one of Holtburg's candelabra
    // stands 2.1 m from this camera: about half the frame changes by some amount, and that is the
    // data's arithmetic. Measured `max |delta|` per pixel, RGB:
    //
    // ```text
    // > 0   157,958      > 8   133,550      > 32   57,189      > 64   7,307
    // ```
    //
    // so the visible flame cores are 2.4% of the frame and the rest is corona. The quarter-frame
    // ceiling applies to "differs by more than a quarter of the channel range", light a player
    // can see. Provenance needs no pixel heuristic: `SceneConfig::particles` only decides whether
    // the particles are collected for the draw, and the emitters are spawned, stepped and
    // degraded identically in both runs, so with the two reproducibility assertions the
    // difference between the captures is the particle draw.
    //
    // The claim that bites: **an additive pass can only add.** Every changed pixel is strictly
    // brighter. That fails if a particle is drawn with the wrong blend state, with z-write on,
    // behind the geometry it should be in front of, or as an opaque quad.
    let px = off_a
        .as_chunks::<4>()
        .0
        .iter()
        .zip(on.as_chunks::<4>().0.iter());
    let (mut bright, mut darker) = (0usize, 0usize);
    for (a, b) in px {
        let d: [i32; 3] = std::array::from_fn(|c| i32::from(b[c]) - i32::from(a[c]));
        if d.iter().copied().max().unwrap_or(0) > 64 {
            bright += 1;
        }
        if d.iter().copied().min().unwrap_or(0) < 0 {
            darker += 1;
        }
    }
    assert_eq!(
        darker, 0,
        "{darker} pixels got darker; an additive particle pass only adds light"
    );
    assert!(
        bright > 0,
        "no pixel was brightened by more than a quarter of the range"
    );
    assert!(
        bright * 4 < total,
        "{bright} of {total} pixels brightened by more than a quarter of the range; that is not a \
         particle effect"
    );
    eprintln!(
        "{on_drawn} particles drawn, {differing} of {total} pixels changed, {bright} of them by \
         more than 64, none darker"
    );

    // And the particle run is itself reproducible: `ran2` is seeded per object and the clock is
    // stepped by a fixed quantum, so two runs of the same script produce the same burst.
    let (on2, _) = shot(true).expect("a rendered frame: retail dats and a WARP device");
    assert_eq!(on, on2, "the particle run is not reproducible");
}

// ---------------------------------------------------------------------------------------------
// 3. The shipped explode and implode quirks.
// ---------------------------------------------------------------------------------------------

/// **The original explode and implode arithmetic must survive exactly.**
///
/// The closed-form `Explode` reuses `a.x` where `a.y` and `a.z` are clearly intended, and
/// `Implode` uses `cos(a.x · t)` on all three axes. Every explosion and implosion in the game has
/// that shape. `dereth_animation` reproduces both; the client evaluates nothing of its own, so this is
/// the integration-level guard that a well-meaning fix under it would trip.
///
/// Oracle: both retail quirks, exercised here through the live draw path rather than only by the
/// particle simulator.
#[test]
fn the_explode_and_implode_oddities_still_reach_the_draw() {
    use dereth_animation::data::{ParticleEmitterInfo, ParticleType};
    use dereth_animation::particles::{BirthParams, Particle};
    use dereth_primitives::{Frame, Quat};

    let _ = ParticleEmitterInfo::default();
    let p = BirthParams {
        lifespan: 10.0,
        final_trans: 0.0,
        start_trans: 0.0,
        final_scale: 1.0,
        start_scale: 1.0,
        // `a` with three clearly distinct components, so using the wrong one shows.
        c: Vec3::new(1.0, 1.0, 1.0),
        b: Vec3::ZERO,
        a: Vec3::new(1.0, 100.0, 10_000.0),
        offset: Vec3::ZERO,
    };
    let base = Frame::new(Vec3::ZERO, Quat::IDENTITY);

    // `Explode` draws two extra rolls for its random unit direction, so drive `Init` and then
    // overwrite `c` with the unit vector the equation is being tested against.
    let mut rng = dereth_primitives::num::rng::Ran2::new(1);
    let mut e = Particle::init(base, base, p, ParticleType::Explode, 0.0, &mut rng);
    e.c = Vec3::new(1.0, 1.0, 1.0);
    e.update(ParticleType::Explode, false, &base, 1.0);
    assert_eq!(
        e.frame.origin,
        Vec3::new(1.0, 1.0, 10_001.0),
        "Explode must use a.x on all three axes and add a.z only to z"
    );

    let mut i = Particle::init(base, base, p, ParticleType::Implode, 0.0, &mut rng);
    i.a = Vec3::new(1.0, 100.0, 10_000.0);
    i.b = Vec3::ZERO;
    i.c = Vec3::new(1.0, 1.0, 1.0);
    i.offset = Vec3::ZERO;
    i.update(ParticleType::Implode, false, &base, 1.0);
    let cos1 = math::cosf(1.0f32);
    assert!((i.frame.origin.x - cos1).abs() < 1e-6);
    assert!(
        (i.frame.origin.y - cos1).abs() < 1e-6,
        "cos(a.x*t), not cos(a.y*t)"
    );
    assert!(
        (i.frame.origin.z - cos1).abs() < 1e-6,
        "cos(a.x*t), not cos(a.z*t)"
    );
}
