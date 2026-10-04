//! Runtime per-part translucency and its material override. A partial fade clears NoDraw, clones
//! the part's material and writes `1 - t` into its alphas; binding that material selects material
//! diffuse instead of vertex diffuse, so the fade overrides a surface's authored translucency
//! rather than multiplying it (`1 - t`, not `(1 - t) * alpha`). Full translucency sets NoDraw, and
//! returning to zero drops the override. The camera drives the value for the player's body (first
//! person hides it); the animation transparency channel drives the same part-array operation.
//!
//! Fixture: the retail dats; Holtburg with the local body on a software device, compared in
//! equal-pose pairs. The arithmetic (ramp values, material row 13, pipeline keys) is unit-tested
//! in the world scene; this file adds the raster result. Missing fixtures fail.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use std::sync::Arc;

use dereth_client::world::{SceneConfig, SceneStats, WorldScene};
use dereth_dat::RetailDatStore;
use dereth_primitives::LocalTime;
use dereth_render::device::Gpu;

/// Required DAT inputs fail the test when missing; there is no skip path.
fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

type Shot = (Vec<u8>, u32, u32);

fn shot(scene: &mut WorldScene, gpu: &mut Gpu) -> Shot {
    scene.reserve_upload_arena(gpu).expect("reserve the arena");
    gpu.begin_frame().expect("begin");
    scene.draw(gpu).expect("draw");
    gpu.end_frame().expect("end");
    let image = gpu.capture().expect("capture");
    let (w, h) = (image.width, image.height);
    (image.to_rgba(), w, h)
}

/// Return the row-major changed-RGBA-pixel mask and its population.
/// Assertions use locations as well as counts. A known-different pair calibrates the same
/// instrument before a zero difference is accepted; the measured denominator is printed.
fn mask(a: &Shot, b: &Shot) -> (Vec<bool>, usize) {
    assert_eq!((a.1, a.2), (b.1, b.2), "the two frames are different sizes");
    assert_eq!(
        a.0.len(),
        (a.1 as usize) * (a.2 as usize) * 4,
        "the frame is not RGBA8"
    );
    let n = (a.1 as usize) * (a.2 as usize);
    let mut m = vec![false; n];
    let mut count = 0usize;
    for (i, px) in m.iter_mut().enumerate() {
        if a.0[i * 4..i * 4 + 4] != b.0[i * 4..i * 4 + 4] {
            *px = true;
            count += 1;
        }
    }
    (m, count)
}

/// Populate Holtburg (0xA9B4) with terrain, scenery, buildings, interior statics and a body.
/// Pin the degrade governor and disable particles here; render() also disables weather so
/// each pair differs only in the tested material settings.
fn populated(material: bool) -> SceneConfig {
    SceneConfig {
        land_radius: 1,
        scenery_radius: 1,
        particles: false,
        material_translucency: material,
        ..SceneConfig::default()
    }
}

/// What one arm of a pair produced.
struct Arm {
    stats: SceneStats,
    /// `(animated parts submitted, parts drawn with a material-alpha override)`.
    parts: (u32, u32),
    /// Body and per-part frames let the two arms prove equal poses before comparing pixels: a
    /// rebuilt body can hold a different idle sequence frame than the other arm's, and that
    /// animation mismatch would read as a rendering differential.
    pose: (dereth_primitives::Frame, Vec<dereth_primitives::Frame>),
    /// `(has a motion table, sequence frame number, the current MotionCommand)`.
    motion: (bool, i32, dereth_animation::MotionCommand),
    frame: Shot,
}

/// Build and settle a scene, then apply t through the camera's material path and capture.
/// Write CameraControl::player_translucency and call WorldScene::apply_camera_translucency
/// directly. Driving the chase camera within 0.45 m of its pivot would also measure smoothing;
/// the distance-to-translucency expression is tested in the camera module separately.
fn render(store: &Arc<RetailDatStore>, gpu: &mut Gpu, material: bool, t: f32) -> Arm {
    let mut scene = WorldScene::load(store, gpu, populated(material)).expect("the landscape loads");
    scene.set_weather_enabled(false);
    let region = dereth_client::world::load_region(store).expect("the region decodes");
    scene
        .attach_character(store, &region, gpu)
        .expect("the body is created");
    for i in 0..8u32 {
        scene.update(
            dereth_client::camera::CameraInput::default(),
            dereth_client::character::CharacterInput::default(),
            LocalTime(f64::from(i) * 0.05),
            0.05,
        );
    }
    scene.follow_character_now();
    scene
        .character
        .as_mut()
        .expect("the body is attached")
        .camera
        .player_translucency = t;
    scene.apply_camera_translucency();
    let stats = scene.draw.stats;
    let frame = shot(&mut scene, gpu);
    let parts = scene.drawn_material_parts();
    let pose = scene.character_frames().expect("the body has part frames");
    let motion = scene
        .character_motion()
        .expect("the body has a motion state");
    // Release the bake-cache textures between arms: a populated scene consumes hundreds of
    // descriptors, and several loads in one device could exhaust a small descriptor heap, which
    // would be a resource failure rather than a rendering one.
    scene.release_textures(gpu);
    Arm {
        stats,
        parts,
        pose,
        motion,
        frame,
    }
}

/// **Assert the pose before comparing the pixels.** Two arms that differ by an animation frame
/// produce a real, large differential that has nothing to do with the thing under test.
fn same_pose(a: &Arm, b: &Arm, what: &str) {
    assert_eq!(
        a.motion, b.motion,
        "{what}: the two arms are at different points of their animation ({:?} vs {:?})",
        a.motion, b.motion
    );
    assert_eq!(
        a.pose.0, b.pose.0,
        "{what}: the body is not in the same place"
    );
    assert_eq!(
        a.pose.1.len(),
        b.pose.1.len(),
        "{what}: the two arms have different part counts"
    );
    for (i, (x, y)) in a.pose.1.iter().zip(&b.pose.1).enumerate() {
        assert_eq!(x, y, "{what}: part {i} is not in the same place");
    }
}

/// `t` for the fade the assertions below are written against.
const HALF: f32 = 0.5;

/// The fade changes the body's pixels without changing pixels outside its measured silhouette.
/// Four equal-pose scenes use the same eight-tick settle and pinned degrade governor:
///
/// | arm | material switch | player translucency | purpose |
/// |---|---|---|---|
/// | opaque | on | 0.0 | default settings, no material override |
/// | faded | on | 0.5 | runtime material alpha 1 - t = 0.5 |
/// | hidden | on | 1.0 | NoDraw suppresses body submission |
/// | switched | off | 0.5 | the material path disabled |
///
/// Opaque versus hidden calibrates a known difference. Opaque versus switched must then be
/// identical.
#[test]
fn the_camera_fade_moves_the_body_and_only_the_body() {
    let store = store();
    let mut gpu = crate::common::test_gpu(640, 640);

    let opaque = render(&store, &mut gpu, true, 0.0);
    let hidden = render(&store, &mut gpu, true, 1.0);
    let faded = render(&store, &mut gpu, true, HALF);
    let switched = render(&store, &mut gpu, false, HALF);

    eprintln!(
        "scene: {} character part meshes over {} triangles; {} object batches; \
         parts submitted/with-material: opaque {:?}, faded {:?}, hidden {:?}, switch-off {:?}",
        opaque.stats.character_parts,
        opaque.stats.character_triangles,
        opaque.stats.object_batches,
        opaque.parts,
        faded.parts,
        hidden.parts,
        switched.parts,
    );

    // The pose, before any pixel is looked at.
    same_pose(&opaque, &faded, "opaque vs faded");
    same_pose(&opaque, &switched, "opaque vs switch-off");

    // The denominators, next.
    assert!(
        opaque.parts.0 > 0,
        "no animated part was submitted at all; nothing below can be seen"
    );
    assert_eq!(
        opaque.parts.1, 0,
        "a body at translucency 0 must carry no material"
    );
    assert_eq!(
        faded.parts.0, opaque.parts.0,
        "the two arms submitted different numbers of parts, so the differential means nothing"
    );
    assert!(
        faded.parts.1 > 0,
        "{} parts submitted and not one carried a material -- the driver did not run",
        faded.parts.0
    );
    assert!(
        hidden.parts.0 < opaque.parts.0,
        "full translucency did not set NoDraw"
    );
    assert_eq!(
        switched.parts.1, 0,
        "the switch is off and {} parts still took the material path",
        switched.parts.1
    );

    // 1. Calibration. The body's own silhouette: every pixel that moves when it is not drawn.
    let (body, body_px) = mask(&opaque.frame, &hidden.frame);
    eprintln!(
        "calibration: opaque vs NoDraw differs on {body_px} of {} pixels -- the body's \
         silhouette, and the non-zero this differ had to produce before any zero below counts",
        (opaque.frame.1 * opaque.frame.2) as usize
    );
    assert!(
        body_px > 500,
        "only {body_px} pixels of body on screen; the frame cannot see a fade"
    );

    // 2. The fade itself, and the containment.
    let (changed, changed_px) = mask(&opaque.frame, &faded.frame);
    let outside = changed
        .iter()
        .zip(&body)
        .filter(|(c, b)| **c && !**b)
        .count();
    eprintln!(
        "fade: t = {HALF} moved {changed_px} pixels, {} of them inside the body's own \
         silhouette and {outside} outside it",
        changed_px - outside
    );
    assert!(
        changed_px > 0,
        "{} parts carried a material and nothing moved",
        faded.parts.1
    );
    assert_eq!(
        outside, 0,
        "{outside} pixels moved outside the body -- the camera's fade reached the world"
    );

    // 3. The calibrated instrument compares the disabled path at HALF with the opaque arm: with
    // the material path off, the fade must not reach the raster at all.
    let (_, regression) = mask(&opaque.frame, &switched.frame);
    eprintln!(
        "control: switch off at t = {HALF} differs from the opaque control on {regression} pixels"
    );
    assert_eq!(
        regression, 0,
        "{regression} pixels moved with SceneConfig::material_translucency clear"
    );
}

/// At zero translucency, enabling the material path must leave the entire frame unchanged.
/// Default settings remove the override, so no submitted part uses material alpha. Compare
/// current switch-off and switch-on arms, not just the body's region. The same differ first
/// sees a known-different hidden-body arm, providing the denominator for this negative.
#[test]
fn a_part_with_has_alpha_clear_is_byte_identical_to_the_previous_build() {
    let store = store();
    let mut gpu = crate::common::test_gpu(640, 640);

    let before = render(&store, &mut gpu, false, 0.0);
    let after = render(&store, &mut gpu, true, 0.0);
    let hidden = render(&store, &mut gpu, true, 1.0);

    same_pose(&before, &after, "switch off vs switch on");
    let (_, calibration) = mask(&before.frame, &hidden.frame);
    let (_, moved) = mask(&before.frame, &after.frame);
    eprintln!(
        "hold-still: {} parts submitted, {} of them with a material; the differ reads \
         {calibration} on the known-different pair and {moved} on this one",
        after.parts.0, after.parts.1
    );
    assert!(
        calibration > 500,
        "the differ read {calibration} on a pair that must differ"
    );
    assert_eq!(
        after.parts.1, 0,
        "a scene with no driver running must clone no material"
    );
    assert_eq!(
        moved, 0,
        "{moved} pixels moved with every part's has_alpha clear"
    );
}

/// Behaviour: rendering.translucency.a-parts-translucency-ramp-reaches-the-raster
/// Measure interior ramp values against the independent expression 1 - t.
/// world_scene.rs tests the material alpha byte; this test checks the raster result:
///
/// ```text
/// frame(t) == frame(opaque) * (1 - t) + frame(nodraw) * t
/// ```
/// This is SRCALPHA / INVSRCALPHA with source alpha 1 - t. The oracle uses that
/// arithmetic directly, not this build's computed alpha byte, so a quantization error cannot
/// cancel by being copied into the expectation.
///
/// Material row 13 disables depth writes. Where body parts overlap, faded parts composite in
/// submission order while opaque parts depth-test; a single lerp cannot describe those double
/// blends. The residual `(1 - a) * a * (L_behind - background)` reached 38 of 255 on this body.
/// That is why the check is over a measured single-layer subset.
///
/// Select the subset at t = 0.5, then assert only at 0.25 and 0.75. No tested value selects its
/// own pixels. The subset must exceed 200 pixels; whole-silhouette distance must also increase
/// from opaque and decrease from hidden. In particular, t instead of 1 - t agrees at 0.5, but
/// the independent quarter/three-quarter and direction checks distinguish it.
#[test]
fn the_ramp_reaches_the_raster_at_one_minus_t() {
    let store = store();
    let mut gpu = crate::common::test_gpu(640, 640);

    let opaque = render(&store, &mut gpu, true, 0.0);
    let hidden = render(&store, &mut gpu, true, 1.0);

    let (body, body_px) = mask(&opaque.frame, &hidden.frame);
    assert!(body_px > 500, "only {body_px} pixels of body on screen");

    let n = body.len();
    /// Per-channel SRCALPHA / INVSRCALPHA with the independent alpha 1 - t.
    fn want(src: u8, dst: u8, t: f32) -> u8 {
        let v = f32::from(src) * (1.0 - t) + f32::from(dst) * t;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // LINT-OK: a convex combination of two u8s, so 0..=255.
        let v = v.round() as u8;
        v
    }
    let holds = |arm: &Shot, t: f32, i: usize| {
        (0..3).all(|c| {
            arm.0[i * 4 + c].abs_diff(want(
                opaque.frame.0[i * 4 + c],
                hidden.frame.0[i * 4 + c],
                t,
            )) <= 2
        })
    };

    // The selector, at t = 0.5 and nowhere else.
    let half = render(&store, &mut gpu, true, 0.5);
    same_pose(&opaque, &half, "opaque vs t = 0.5");
    assert!(half.parts.1 > 0, "t = 0.5: no part carried a material");
    let single: Vec<bool> = (0..n)
        .map(|i| body[i] && holds(&half.frame, 0.5, i))
        .collect();
    let single_px = single.iter().filter(|v| **v).count();
    eprintln!(
        "ramp denominator: {single_px} of {body_px} body pixels are a single blended layer \
         (the rest are two of the body's own parts overlapping with the depth write off)"
    );
    assert!(
        single_px > 200,
        "only {single_px} of {body_px} body pixels are a single layer; either the alpha is not \
         1 - t at all, or the check below would be vacuous"
    );

    let mut summary = Vec::new();
    for t in [0.25f32, 0.75] {
        let arm = render(&store, &mut gpu, true, t);
        same_pose(&opaque, &arm, "opaque vs the ramp");
        assert!(arm.parts.1 > 0, "t = {t}: no part carried a material");
        let mut worst = 0u8;
        let mut worst_at = 0usize;
        let mut compared = 0usize;
        for (i, one) in single.iter().enumerate() {
            if !*one {
                continue;
            }
            compared += 1;
            for c in 0..3 {
                // The client's own expression: `1 - t` in the material's Diffuse alpha, blended
                // SRCALPHA / INVSRCALPHA over what is behind the body.
                let err = arm.frame.0[i * 4 + c].abs_diff(want(
                    opaque.frame.0[i * 4 + c],
                    hidden.frame.0[i * 4 + c],
                    t,
                ));
                if err > worst {
                    worst = err;
                    worst_at = i;
                }
            }
        }
        assert_eq!(compared, single_px, "the sweep lost pixels");
        // Direction, over the *whole* silhouette rather than the valid set: total distance from
        // the opaque frame must rise with t and total distance from the empty frame must fall.
        // A sign flip (`t` where the client writes `1 - t`) reverses both at once.
        let sum = |other: &Shot| -> u64 {
            (0..n)
                .filter(|i| body[*i])
                .map(|i| {
                    (0..3)
                        .map(|c| u64::from(arm.frame.0[i * 4 + c].abs_diff(other.0[i * 4 + c])))
                        .sum::<u64>()
                })
                .sum()
        };
        summary.push((t, worst, sum(&opaque.frame), sum(&hidden.frame)));
        eprintln!(
            "ramp t = {t}: worst channel error {worst} over {single_px} pixels \
             (at pixel {worst_at}); distance from opaque {}, from empty {}",
            summary.last().expect("just pushed").2,
            summary.last().expect("just pushed").3
        );
        assert!(
            worst <= 2,
            "t = {t}: a body pixel is {worst} off `frame_opaque * (1 - t) + frame_empty * t`, \
             which is not the expected material alpha"
        );
    }

    // Monotone in the right direction, which is what tells `1 - t` from `t`.
    for w in summary.windows(2) {
        let (t0, _, from_opaque0, from_empty0) = w[0];
        let (t1, _, from_opaque1, from_empty1) = w[1];
        assert!(
            from_opaque1 > from_opaque0,
            "t {t0} -> {t1}: the body did not move further from opaque ({from_opaque0} -> {from_opaque1})"
        );
        assert!(
            from_empty1 < from_empty0,
            "t {t0} -> {t1}: the body did not move closer to gone ({from_empty0} -> {from_empty1})"
        );
    }
}

/// Exercise the part-array destination of the animation transparency channel.
/// A transparency hook with a duration below 0.0002 takes the immediate path; the animation
/// driver implements that threshold. This test calls the destination setter directly
/// once per ramp step on a single scene. It does not decode or dispatch a hook.
///
/// Three checks complement the camera pairs:
/// 1. Exact full translucency sets NoDraw before changing material values, so the body stops
///    being submitted rather than being submitted with alpha zero.
/// 2. Returning to zero restores default settings and drops each material override. The test
///    counts these overrides directly, not just their pixel effects.
/// 3. The returned frame is byte-identical to the frame before the ramp.
#[test]
fn the_hook_channel_ramps_to_invisible_and_comes_back() {
    let store = store();
    let mut gpu = crate::common::test_gpu(640, 640);

    let mut scene =
        WorldScene::load(&store, &mut gpu, populated(true)).expect("the landscape loads");
    scene.set_weather_enabled(false);
    let region = dereth_client::world::load_region(&store).expect("the region decodes");
    scene
        .attach_character(&store, &region, &mut gpu)
        .expect("the body is created");
    for i in 0..8u32 {
        scene.update(
            dereth_client::camera::CameraInput::default(),
            dereth_client::character::CharacterInput::default(),
            LocalTime(f64::from(i) * 0.05),
            0.05,
        );
    }
    scene.follow_character_now();

    // Apply the common part-array destination directly. Immediate transparency, ramp updates
    // and hierarchical body updates all reach this operation; testing it here
    // isolates material/submission behavior from the upstream hook and hierarchy producers.
    let set = |scene: &WorldScene, t: f32| {
        scene
            .character
            .as_ref()
            .expect("the body is attached")
            .driver_mut()
            .part_array
            .set_translucency_internal(t, false);
    };
    let materials = |scene: &WorldScene| -> usize {
        scene
            .character
            .as_ref()
            .expect("the body is attached")
            .driver()
            .part_array
            .parts
            .iter()
            .filter(|p| p.material.is_some())
            .count()
    };
    let nodraw = |scene: &WorldScene| -> usize {
        scene
            .character
            .as_ref()
            .expect("the body is attached")
            .driver()
            .part_array
            .parts
            .iter()
            .filter(|p| p.no_draw())
            .count()
    };

    let before = shot(&mut scene, &mut gpu);
    let submitted_before = scene.drawn_material_parts();
    assert_eq!(
        materials(&scene),
        0,
        "the body starts with no cloned material"
    );
    assert_eq!(nodraw(&scene), 0, "the body starts drawn");
    assert!(
        submitted_before.0 > 0,
        "no part submitted; nothing below can be seen"
    );

    let mut ramp = Vec::new();
    for t in [0.2f32, 0.5, 0.8] {
        set(&scene, t);
        let frame = shot(&mut scene, &mut gpu);
        let parts = scene.drawn_material_parts();
        let (_, changed) = mask(&before, &frame);
        eprintln!(
            "hook ramp t = {t}: {} of {} parts carry a material, {} part(s) NoDraw, \
             {changed} pixels differ from the un-ramped frame",
            parts.1,
            parts.0,
            nodraw(&scene)
        );
        assert_eq!(
            materials(&scene),
            parts.0 as usize,
            "t = {t}: not every part cloned"
        );
        assert_eq!(
            parts.1, parts.0,
            "t = {t}: a cloned material did not reach the draw"
        );
        ramp.push(changed);
    }
    assert!(
        ramp.iter().all(|c| *c > 0),
        "the ramp moved no pixels at some step: {ramp:?}"
    );
    assert!(
        ramp.windows(2).all(|w| w[1] >= w[0]),
        "the ramp did not spread monotonically: {ramp:?}"
    );

    // 1. Full transparency: NoDraw, not alpha 0.
    set(&scene, 1.0);
    let gone = shot(&mut scene, &mut gpu);
    let after_gone = scene.drawn_material_parts();
    eprintln!(
        "hook ramp t = 1.0: {} of {} parts NoDraw, {} part(s) submitted",
        nodraw(&scene),
        materials(&scene) + nodraw(&scene),
        after_gone.0
    );
    assert_eq!(
        after_gone.0, 0,
        "full translucency must set NoDraw, not alpha 0"
    );
    let (_, vanished) = mask(&before, &gone);
    assert!(
        vanished > 500,
        "the body did not vanish: only {vanished} pixels moved"
    );

    // 2 and 3. It comes back, the clone is dropped, and the frame is byte-identical.
    set(&scene, 0.0);
    let back = shot(&mut scene, &mut gpu);
    let after_back = scene.drawn_material_parts();
    let (_, residue) = mask(&before, &back);
    eprintln!(
        "hook ramp back to 0: {} cloned material(s) left, {} part(s) NoDraw, \
         {} submitted, {residue} pixels differ from before the ramp (the differ read \
         {vanished} on the known-different pair)",
        materials(&scene),
        nodraw(&scene),
        after_back.0
    );
    assert_eq!(
        after_back.0, submitted_before.0,
        "the parts did not come back"
    );
    assert_eq!(
        after_back.1, 0,
        "a material survived restoration of default settings"
    );
    assert_eq!(
        materials(&scene),
        0,
        "default settings did not drop the clone"
    );
    assert_eq!(
        nodraw(&scene),
        0,
        "the NoDraw bit was not cleared on the way back"
    );
    assert_eq!(
        residue, 0,
        "{residue} pixels did not come back to where they started"
    );
}

/// Behaviour: rendering.translucency.first-person-hides-the-body
/// Exercise the viewer-update function called by App::frame, rather than applying the fade
/// directly as the preceding camera pairs do. dereth_client::camera::update_viewer updates the
/// camera, then calls scene.apply_camera_translucency(). Removing that call leaves the direct
/// pairs green but fails this integration check. This is not a full App::frame invocation.
///
/// The first-person branch compares the viewer offset exactly with (0, 0.18, 0) and requests
/// full player translucency, making the body NoDraw. The current CameraState::in_head
/// predicate and set_in_head operation select that branch here.
#[test]
fn first_person_hides_the_body_through_the_frames_own_camera_call() {
    let store = store();
    let mut gpu = crate::common::test_gpu(640, 640);

    let mut scene =
        WorldScene::load(&store, &mut gpu, populated(true)).expect("the landscape loads");
    scene.set_weather_enabled(false);
    let region = dereth_client::world::load_region(&store).expect("the region decodes");
    scene
        .attach_character(&store, &region, &mut gpu)
        .expect("the body is created");
    for i in 0..8u32 {
        scene.update(
            dereth_client::camera::CameraInput::default(),
            dereth_client::character::CharacterInput::default(),
            LocalTime(f64::from(i) * 0.05),
            0.05,
        );
    }
    scene.follow_character_now();
    let _ = shot(&mut scene, &mut gpu);
    let third_person = scene.drawn_material_parts();
    assert!(third_person.0 > 0, "no part submitted in third person");
    assert_eq!(
        third_person.1, 0,
        "the body is faded before anything asked for it"
    );

    {
        let c = scene.character.as_mut().expect("the body is attached");
        let cam = &mut c.camera;
        assert!(
            !dereth_client::camera::CameraState::in_head(&cam.manager),
            "already first person"
        );
        let (set, manager) = (&mut cam.set, &mut cam.manager);
        set.set_in_head(manager);
        assert!(
            dereth_client::camera::CameraState::in_head(&cam.manager),
            "set_in_head did nothing"
        );
    }

    // The one call `App::frame` makes. Nothing else here touches the translucency.
    dereth_client::camera::update_viewer(
        &mut scene,
        dereth_client::camera::CameraInput::default(),
        LocalTime(0.4),
        0.05,
    );
    let t = scene
        .character
        .as_ref()
        .expect("the body is attached")
        .camera
        .player_translucency;
    let nodraw = scene
        .character
        .as_ref()
        .expect("the body is attached")
        .driver()
        .part_array
        .parts
        .iter()
        .filter(|p| p.no_draw())
        .count();
    let _ = shot(&mut scene, &mut gpu);
    let first_person = scene.drawn_material_parts();
    eprintln!(
        "first person: player_translucency {t}, {nodraw} part(s) NoDraw, \
         {} submitted (was {} in third person)",
        first_person.0, third_person.0
    );
    assert!(
        (t - 1.0).abs() < f32::EPSILON,
        "the camera update's first-person branch sets player translucency to 1.0, not {t}"
    );
    assert_eq!(
        nodraw, third_person.0 as usize,
        "full translucency did not set NoDraw"
    );
    assert_eq!(
        first_person.0, 0,
        "the body is still submitted in first person"
    );
}
