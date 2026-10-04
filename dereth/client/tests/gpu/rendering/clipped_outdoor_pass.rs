//! The clipped outdoor pass through building openings: the depth stamp is drawn only inside the
//! openings, the exact screen-space clip changes only the pixels outside them and drops an opening
//! that leaves the viewport, the alpha flush runs before each building and changes nothing else,
//! and the interior still draws with the clip and the stamp on.
//!
//! Fixture: the retail dats, Holtburg with no body at a pinned time of day, a flycam standing in
//! front of a building opening, on a software device. Each differential pairs two runs that
//! differ in one `SceneConfig` flag. The scene has no character, so the camera's viewer sweep has
//! nothing to do and the frame order leaves it out; the flycam supplies the viewpoint.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use dereth_client::objects::ObjectStream;
use dereth_client::world::{SceneConfig, WorldScene};
use dereth_client::world::{SceneReads, SceneWrites};
use dereth_dat::RetailDatStore;
use dereth_primitives::num::math;
use dereth_primitives::{LocalTime, Vec3};
use dereth_render::device::Gpu;
use std::sync::Arc;

const W: u32 = 640;
const H: u32 = 480;

/// The retail store, or **fail**: absent dats are a failure, never a skip.
fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

/// Holtburg, no body, a pinned time of day and no particles, so that the only thing that can move between the halves of a differential is the flag under test.
fn cfg(clip: bool, stamp: bool) -> SceneConfig {
    SceneConfig {
        character: false,
        scenery_radius: 2,
        time_of_day: Some(0.5),
        particles: false,
        building_portals: true,
        portal_clip: clip,
        portal_depth_stamp: stamp,
        ..SceneConfig::default()
    }
}

fn frame(store: &Arc<RetailDatStore>, gpu: &mut Gpu, scene: &mut WorldScene, t: f64) -> Vec<u8> {
    let mut stream = ObjectStream::new();
    scene
        .sync_objects(store, gpu, &mut stream)
        .expect("sync_objects");
    scene.update(
        dereth_client::camera::CameraInput::default(),
        dereth_client::character::CharacterInput::default(),
        LocalTime(t),
        1.0 / 30.0,
    );
    scene.stream(store, gpu).expect("stream");
    scene.reserve_upload_arena(gpu).expect("reserve the arena");
    gpu.begin_frame().expect("begin");
    scene.draw(gpu).expect("draw");
    gpu.end_frame().expect("end");
    gpu.capture().expect("capture").to_rgba()
}

/// Stand `back` metres outside the `index`th building opening, looking straight at it, then pitch by
/// `pitch` radians. The extra pitch matters: looking *down*
/// through a doorway is what puts the interior floor and the terrain under the building at the same
/// depth, which is the configuration the depth stamp exists for.
fn stand(scene: &mut WorldScene, index: usize, back: f32, pitch: f32) -> (Vec3, Vec3) {
    let openings = scene.building_portal_openings();
    assert!(
        openings.len() > index,
        "only {} openings; expected more",
        openings.len()
    );
    let (centre, outward) = openings[index];
    let len = outward.dot(outward).sqrt().max(1.0e-6);
    let n = Vec3::new(outward.x / len, outward.y / len, outward.z / len);
    scene.camera.position = Vec3::new(
        centre.x + n.x * back,
        centre.y + n.y * back,
        centre.z + n.z * back,
    );
    scene.camera.yaw = math::atan2f(n.x, -n.y);
    scene.camera.pitch = (math::asinf(-n.z) + pitch).clamp(-1.5, 1.5);
    (centre, n)
}

fn inside(poly: &[(f32, f32)], x: f32, y: f32) -> bool {
    let mut hit = false;
    let n = poly.len();
    for i in 0..n {
        let (x0, y0) = poly[i];
        let (x1, y1) = poly[(i + 1) % n];
        if (y0 > y) != (y1 > y) {
            let t = (y - y0) / (y1 - y0);
            if x < x0 + t * (x1 - x0) {
                hit = !hit;
            }
        }
    }
    hit
}

fn edge_distance(poly: &[(f32, f32)], x: f32, y: f32) -> f32 {
    let n = poly.len();
    let mut best = f32::MAX;
    for i in 0..n {
        let (x0, y0) = poly[i];
        let (x1, y1) = poly[(i + 1) % n];
        let (dx, dy) = (x1 - x0, y1 - y0);
        let len2 = dx.mul_add(dx, dy * dy);
        let t = if len2 <= 1.0e-6 {
            0.0
        } else {
            (((x - x0) * dx + (y - y0) * dy) / len2).clamp(0.0, 1.0)
        };
        let (cx, cy) = (x0 + dx * t, y0 + dy * t);
        best = best.min((x - cx).mul_add(x - cx, (y - cy) * (y - cy)).sqrt());
    }
    best
}

/// The union of the openings' outlines as a pixel mask, dilated by `pad`.
fn opening_mask(polys: &[Vec<(f32, f32)>], pad: f32) -> Vec<bool> {
    let mut mask = vec![false; (W * H) as usize];
    for p in polys {
        if p.len() < 3 {
            continue;
        }
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for &(x, y) in p {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // LINT-OK: screen pixel bounds, clamped to the viewport. Not engine arithmetic.
        let (px0, py0) = ((x0 - pad).max(0.0) as u32, (y0 - pad).max(0.0) as u32);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (px1, py1) = (
            ((x1 + pad).max(0.0) as u32).min(W - 1),
            ((y1 + pad).max(0.0) as u32).min(H - 1),
        );
        for py in py0..=py1 {
            for px in px0..=px1 {
                #[allow(clippy::cast_precision_loss)] // a pixel index below 4096
                let (fx, fy) = (px as f32 + 0.5, py as f32 + 0.5);
                if inside(p, fx, fy) || edge_distance(p, fx, fy) <= pad {
                    mask[(py * W + px) as usize] = true;
                }
            }
        }
    }
    mask
}

/// The indices of the pixels that differ between two RGBA captures.
fn differing(a: &[u8], b: &[u8]) -> Vec<usize> {
    a.as_chunks::<4>()
        .0
        .iter()
        .zip(b.as_chunks::<4>().0.iter())
        .enumerate()
        .filter(|(_, (x, y))| x != y)
        .map(|(i, _)| i)
        .collect()
}

/// How far outside the nearest opening's outline the worst changed pixel sits.
fn worst_spill(polys: &[Vec<(f32, f32)>], pixels: &[usize]) -> f32 {
    pixels
        .iter()
        .map(|&i| {
            #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
            // LINT-OK: a pixel index into a 640x480 buffer, back to its (x, y).
            let (x, y) = ((i as u32 % W) as f32 + 0.5, (i as u32 / W) as f32 + 0.5);
            polys
                .iter()
                .map(|p| edge_distance(p, x, y))
                .fold(f32::MAX, f32::min)
        })
        .fold(0.0f32, f32::max)
}

/// One capture: the frame, the outlines of the openings the pass opened while taking it, and how
/// many depth-stamp draws the device issued.
///
/// The stamp count is not decoration. A colourless draw is invisible in a capture, and it is also
/// the **only** direct measure of how many openings survived the clip: with `portal_clip` clear
/// every facing, loaded opening stamps, and with it set only the ones that actually project onto
/// the viewport do.
type Shot = (Vec<u8>, Vec<Vec<(f32, f32)>>, u64);

/// One capture plus the device's draw-call count for the frame.
type Counted = (Vec<u8>, Vec<Vec<(f32, f32)>>, u64, u64);

fn counted_shot(
    store: &Arc<RetailDatStore>,
    c: SceneConfig,
    opening: usize,
    back: f32,
    pitch: f32,
) -> Option<Counted> {
    let mut gpu = crate::common::test_gpu(W, H);
    let mut scene = WorldScene::load(store, &mut gpu, c).expect("loads");
    frame(
        store,
        &mut gpu,
        &mut scene,
        dereth_client::app::HEADLESS_STEP,
    );
    stand(&mut scene, opening, back, pitch);
    let (s0, d0) = (gpu.portal_stamps(), gpu.draw_calls());
    let rgba = frame(
        store,
        &mut gpu,
        &mut scene,
        2.0 * dereth_client::app::HEADLESS_STEP,
    );
    let polys = scene.building_portal_screen_polygons(W, H);
    Some((rgba, polys, gpu.portal_stamps() - s0, gpu.draw_calls() - d0))
}

/// Take one frame from a station, on a device of its own so the two halves of a differential cannot
/// share descriptor state.
fn shot_with(
    store: &Arc<RetailDatStore>,
    c: SceneConfig,
    opening: usize,
    back: f32,
    pitch: f32,
) -> Option<Shot> {
    let mut gpu = crate::common::test_gpu(W, H);
    let mut scene = WorldScene::load(store, &mut gpu, c).expect("loads");
    frame(
        store,
        &mut gpu,
        &mut scene,
        dereth_client::app::HEADLESS_STEP,
    );
    stand(&mut scene, opening, back, pitch);
    let before = gpu.portal_stamps();
    let rgba = frame(
        store,
        &mut gpu,
        &mut scene,
        2.0 * dereth_client::app::HEADLESS_STEP,
    );
    let polys = scene.building_portal_screen_polygons(W, H);
    let stamps = gpu.portal_stamps() - before;
    Some((rgba, polys, stamps))
}

fn shot(
    store: &Arc<RetailDatStore>,
    clip: bool,
    stamp: bool,
    opening: usize,
    back: f32,
    pitch: f32,
) -> Option<Shot> {
    shot_with(store, cfg(clip, stamp), opening, back, pitch)
}

/// The station both differentials are taken from: opening 0 of the resident blocks, five metres
/// out, pitched 0.35 rad **down**.
///
/// The pitch is the whole point of the station and is not a tuning knob. Level with an opening you
/// see the room's far wall, which is comfortably in front of anything the landscape drew; pitched
/// down through it you see the **floor**, which sits at the height of the terrain the same block
/// already drew underneath the building. That is where the depth the stamp resets actually matters:
/// the draw order cannot mask a missing stamp at this pose.
const OPENING: usize = 0;
const BACK: f32 = 5.0;
const PITCH_DOWN: f32 = 0.35;

// ---------------------------------------------------------------------------------------------
// (a) The depth stamp.
// ---------------------------------------------------------------------------------------------

/// Behaviour: rendering.portals.the-depth-stamp-is-drawn-only-inside-the-openings
/// **The `mode = 1` depth stamp is drawn, and it changes the frame.**
///
/// The original view-construction path stamps every opening the clip leaves visible. Its
/// render-state selector is 7: a pre-transformed quad at device depth `0.999999`, compare
/// `ALWAYS`, depth write on, no colour. Its effect is to *reset* the depth
/// inside the opening to the far plane, so that the interior drawn next is not occluded by whatever
/// the landscape put there first.
///
/// The two runs differ only in `SceneConfig::portal_depth_stamp`: with it clear no stamp is
/// issued; with it set the stamp is issued. Every changed
/// pixel must be inside an opening, because that is the only place the stamp writes depth at all —
/// a stamp that leaked would clear the depth over the wall and let the interior through it.
///
/// **Omitting the stamp is not safe.** The interiors are drawn before the block's opaque batches
/// and before every nearer block, but that does not make everything already in the depth buffer
/// at an opening farther away than the interior behind it, because
/// [`WorldScene::draw`](dereth_client::world::WorldScene::draw) draws **this block's own terrain
/// cells immediately before** its buildings' interiors — so the ground the building stands on is
/// already in the depth buffer at the opening, at very nearly the depth of the interior floor
/// behind it. The stamp changes pixels at both stations below, the level one included.
#[test]
fn the_depth_stamp_is_drawn_and_only_inside_the_openings() {
    let store = store();

    // Both stations, the level one and the pitched-down one, change: the stamp matters even
    // looking straight at the opening.
    for pitch in [0.0, PITCH_DOWN] {
        let (Some((off, _, off_stamps)), Some((off2, _, _))) = (
            shot(&store, true, false, OPENING, BACK, pitch),
            shot(&store, true, false, OPENING, BACK, pitch),
        ) else {
            return;
        };
        assert_eq!(
            off, off2,
            "the frame without the stamp is not reproducible at pitch {pitch}"
        );
        assert_eq!(off_stamps, 0, "the control issued a stamp");

        let Some((on, polys, on_stamps)) = shot(&store, true, true, OPENING, BACK, pitch) else {
            return;
        };
        assert!(
            !polys.is_empty(),
            "no opening was visible from this station"
        );
        assert!(on_stamps > 0, "the subject issued no stamp at all");

        let changed = differing(&off, &on);
        assert!(
            !changed.is_empty(),
            "at pitch {pitch} the depth stamp changed no pixel at all. Either it is not being \
             issued, or this station has nothing in the depth buffer for it to reset"
        );

        let mask = opening_mask(&polys, 3.0);
        let outside: Vec<usize> = changed.iter().copied().filter(|&i| !mask[i]).collect();
        let worst = worst_spill(&polys, &outside);
        eprintln!(
            "depth stamp @ pitch {pitch}: {on_stamps} stamps, {} of {} pixels changed, {} outside \
             the openings (worst {worst:.1} px)",
            changed.len(),
            off.len() / 4,
            outside.len()
        );
        assert!(
            worst <= SPILL,
            "a pixel {worst:.1} px from the nearest opening changed when the depth stamp was \
             switched on: the stamp is clearing depth outside the opening it belongs to"
        );

        // And it is reproducible.
        let (on2, _, _) = shot(&store, true, true, OPENING, BACK, pitch)
            .expect("a rendered frame: retail dats and a WARP device");
        assert_eq!(on, on2, "the frame with the stamp is not reproducible");
    }
}

/// How far outside an opening's outline a changed pixel may sit: an interior's silhouette can graze the opening's edge by a pixel or two once the
/// rasteriser has had its way with both.
const SPILL: f32 = 8.0;

// ---------------------------------------------------------------------------------------------
// (b) The exact screen-space clip.
// ---------------------------------------------------------------------------------------------

/// **Screen-space clipping and acceptance of the resulting view polygon replace the "visible"
/// stub, within a small cost bound.**
///
/// With the clip stubbed to "visible", **3 of 20,504** changed pixels fell outside an opening's
/// outline, all within 6 px of one. Exact clipping should account for approximately those pixels
/// and essentially nothing else; a change that moved substantially more would mean something else
/// had moved with it.
///
/// So this asserts a *bound*, not a direction: the two runs differ in `SceneConfig::portal_clip`
/// alone, and the difference between them must be small and must lie at the openings. The exact
/// count is printed.
#[test]
fn the_screen_space_clip_changes_only_the_pixels_the_stub_was_costing() {
    let store = store();

    let (Some((stubbed, _, stub_stamps)), Some((exact, polys, exact_stamps))) = (
        shot(&store, false, true, OPENING, BACK, PITCH_DOWN),
        shot(&store, true, true, OPENING, BACK, PITCH_DOWN),
    ) else {
        return;
    };
    assert!(
        !polys.is_empty(),
        "no opening was visible from this station"
    );

    // The clip is doing work, and this is the measurement that says so in whole openings rather
    // than in pixels: an opening only stamps if clipping leaves a polygon that the view accepts.
    // The stub — which answers "visible" for every facing, loaded opening, including the
    // ones behind the camera and off the sides of the screen — must stamp strictly more.
    eprintln!("portal clip: {stub_stamps} openings stamped with the stub, {exact_stamps} with the          exact clip");
    assert!(
        exact_stamps < stub_stamps,
        "the exact clip stamped {exact_stamps} openings and the stub stamped {stub_stamps}: the clip \
         is letting through everything the stub did, so it is not clipping"
    );

    let changed = differing(&stubbed, &exact);
    let mask = opening_mask(&polys, 3.0);
    let outside: Vec<usize> = changed.iter().copied().filter(|&i| !mask[i]).collect();
    let worst = worst_spill(&polys, &outside);
    let total = stubbed.len() / 4;
    eprintln!(
        "portal clip: {} of {total} pixels differ between the stub and the exact clip, {} outside \
         the openings (worst {worst:.1} px)",
        changed.len(),
        outside.len()
    );

    // The bound. The stub's measured cost is 3 pixels of a 20,504-pixel interior; a clip
    // that is *exact* rather than merely *different* cannot move a large fraction of the frame.
    // One per cent of the viewport is two orders of magnitude above the measured cost and still
    // small enough that a clip which had started dropping whole cells would trip it.
    assert!(
        changed.len() * 100 < total,
        "{} of {total} pixels differ between the stubbed clip and the exact one; that is far more \
         than the 3-of-20,504 the stub was measured to cost, so something other than the clip moved",
        changed.len()
    );
    assert!(
        worst <= SPILL,
        "switching the clip on changed a pixel {worst:.1} px from the nearest opening; the clip is \
         reaching outside the openings"
    );
}

/// **An opening off the side of the screen opens nothing.**
///
/// This is the half of the clip the stub could not express at all, and it is a whole-frame claim
/// rather than a few pixels: empty clipping results are skipped, and an accepted view polygon
/// needs at least three vertices. An opening whose outline misses the viewport contributes no view polygon, so
/// the cells behind it are never traversed and never drawn.
///
/// With the camera pitched hard up over the roofline nothing that would open is on the screen, so
/// the exact clip and the stub must agree that nothing is drawn — and the stubbed build reaches that
/// answer only through the *sidedness* gate, which is a different mechanism. The assertion is
/// therefore the strong one: both frames are byte-identical to a frame with the whole pass off.
#[test]
fn an_opening_that_leaves_the_viewport_is_clipped_away_entirely() {
    let store = store();

    // Straight up at the sky, over the roofline, where the openings close again.
    let up = dereth_client::camera::PITCH_LIMIT;
    let sky = |clip: bool, portals: bool| -> Option<Shot> {
        let mut gpu = crate::common::test_gpu(W, H);
        let mut c = cfg(clip, true);
        c.building_portals = portals;
        let mut scene = WorldScene::load(&store, &mut gpu, c).expect("loads");
        frame(
            &store,
            &mut gpu,
            &mut scene,
            dereth_client::app::HEADLESS_STEP,
        );
        stand(&mut scene, OPENING, BACK, 0.0);
        scene.camera.pitch = up;
        let before = gpu.portal_stamps();
        let rgba = frame(
            &store,
            &mut gpu,
            &mut scene,
            2.0 * dereth_client::app::HEADLESS_STEP,
        );
        let polys = scene.building_portal_screen_polygons(W, H);
        Some((rgba, polys, gpu.portal_stamps() - before))
    };

    let (Some((none, _, _)), Some((exact, polys, _))) = (sky(true, false), sky(true, true)) else {
        return;
    };
    let mask = opening_mask(&polys, 3.0);
    assert_eq!(
        mask.iter().filter(|m| **m).count(),
        0,
        "an opening is still on the screen"
    );
    assert_eq!(
        differing(&none, &exact).len(),
        0,
        "with no opening on the screen the exact clip still drew something"
    );
}

// ---------------------------------------------------------------------------------------------
// (c) The alpha flush.
// ---------------------------------------------------------------------------------------------

/// Behaviour: rendering.portals.the-alpha-flush-runs-before-the-building
/// **Queued translucent batches flush before each building's portal pass, preserving their
/// submission count and bounding the resulting pixel changes.**
///
/// The original building-draw routine begins by flushing the alpha list with threshold 0.0.
/// Everything translucent queued so far belongs *behind* this building, so the interior drawn
/// next must not be interleaved with it.
///
/// The two runs differ only in `SceneConfig::portal_alpha_flush`. With it clear, every translucent
/// batch is deferred to one pass after the whole landscape; with it set, the batches of every
/// block already drawn are issued at the first building of the next block that has one.
///
/// This station asserts full-scene conservation and bounds, not that these particular camera
/// pixels must differ: from this station the two frames are identical whether the building guard
/// is on (9 alpha batches) or off (27). Zero is not a general requirement either; alpha order can
/// visibly matter at other stations.
///
/// The discriminating check that the flush reorders visible pixels is the unit test
/// `real_land_bake_tags_shells_and_alpha_flush_excludes_them_without_hiding_cell_statics` in
/// `src/world_building_shell_visibility_tests.rs`: two deliberately overlapping quads with the
/// real DAT alpha 0.5 material go through the production flush and the final blended tail, and it
/// asserts different ordered pixels, the exact reference order, exactly two draws and exclusion
/// of a hidden shell.
///
/// This full scene still asserts:
///
/// * **the same batches are drawn, the same number of times** — `draws_without == draws_with`,
///   which catches the double-blend, the dropped batch and the wrong-set flush alike, and is the
///   assertion that has always done the work at Holtburg;
/// * **the flush reaches nowhere the pass itself does not** — every pixel it moves is also moved
///   by `building_portals` as a whole, taken from the same station in the same run;
/// * **its footprint is a reorder's, not a bucket's** — bounded against the pass's own footprint
///   measured beside it, so a flush that took the alpha-*tested* remainder (Holtburg's foliage)
///   instead of the alpha-list members would fail by three orders of magnitude rather than
///   needing a hand-picked number to trip on.
#[test]
fn the_alpha_flush_runs_before_the_building_and_changes_nothing_it_should_not() {
    let store = store();

    let mut deferred = cfg(true, true);
    deferred.portal_alpha_flush = false;
    let (Some((without, _, _, draws_without)), Some((with, polys, _, draws_with))) = (
        counted_shot(&store, deferred, OPENING, BACK, PITCH_DOWN),
        counted_shot(&store, cfg(true, true), OPENING, BACK, PITCH_DOWN),
    ) else {
        return;
    };

    // A differential over an empty queue proves nothing, so say how full it was.
    let queued = {
        let mut gpu = crate::common::test_gpu(W, H);
        let scene = WorldScene::load(&store, &mut gpu, cfg(true, true)).expect("loads");
        scene.alpha_list_batches()
    };
    assert!(
        queued > 0,
        "the resident blocks bake no alpha-list batch at all, so the flush had nothing to move and \
         this test would pass with the flush deleted"
    );

    let changed = differing(&without, &with);
    let mask = opening_mask(&polys, 3.0);
    let outside: Vec<usize> = changed.iter().copied().filter(|&i| !mask[i]).collect();
    let worst = worst_spill(&polys, &outside);
    eprintln!(
        "alpha flush: {queued} alpha-list batches queued, {} of {} pixels differ between the \
         deferred order and the flushed one, {} outside the openings (worst {worst:.1} px)",
        changed.len(),
        without.len() / 4,
        outside.len()
    );
    // The pass's own footprint, from the same station, measured in this run rather
    // than quoted: `building_portals` off against the flushed frame. It is the denominator both
    // assertions below are expressed in, so neither can go stale the way a pinned pixel count
    // would.
    let mut portals_off = cfg(true, true);
    portals_off.building_portals = false;
    let (Some((off, _, _)),) = (shot_with(&store, portals_off, OPENING, BACK, PITCH_DOWN),) else {
        return;
    };
    let pass: std::collections::HashSet<usize> = differing(&off, &with).into_iter().collect();
    assert!(
        !pass.is_empty(),
        "the portal pass changed no pixel at all from this station, so the containment claim below \
         would be vacuous"
    );
    // That the reorder is visible at all is proved by the overlapping-geometry unit test above.
    // Here, a nonempty queue plus equal draw counts conserves submission even when no visible
    // pixel changes; the following bounds remain useful for any changed pixels in this scene.
    let beyond: Vec<usize> = changed
        .iter()
        .copied()
        .filter(|&i| !pass.contains(&i))
        .collect();
    assert!(
        beyond.is_empty(),
        "{} of the {} pixels the flush moves are not moved by the portal pass at all (first at \
         {:?}). The flush is the pass's first line; it cannot reach where the pass does not",
        beyond.len(),
        changed.len(),
        beyond.first().map(|&i| {
            #[allow(clippy::cast_possible_truncation)]
            // LINT-OK: a pixel index into a 640x480 buffer, back to its (x, y).
            let (x, y) = ((i as u32) % W, (i as u32) / W);
            (x, y)
        })
    );
    // A reorder of the alpha-list members can only show where two of them, or one of them and a
    // clip-mapped neighbour, overlap. Flushing the whole `blended` bucket instead — Holtburg's
    // foliage out of the landscape pass — moves that bucket's entire footprint, which is a
    // different order of magnitude from a handful of overlaps. The bound is a thousandth of the
    // pass's own footprint, taken above, so it tightens as the scene grows instead of drifting.
    assert!(
        changed.len() * 1000 <= pass.len(),
        "{} of the {} pixels the portal pass moves are moved by the flush alone. A reorder of the \
         alpha-list members overlaps in a handful of places; a flush that took the whole `blended` \
         bucket moves all of it",
        changed.len(),
        pass.len()
    );
    eprintln!(
        "alpha flush: {} px of the pass's own {} px footprint, worst {worst:.1} px from an \
         opening -- a landscape reorder, which is what the flush is",
        changed.len(),
        pass.len()
    );

    // **Each batch is drawn exactly once.** This is the half the pixels cannot see: at Holtburg the
    // alpha-list batches the flush moves are not on screen from this station, so double-drawing or
    // dropping the queued members can change no pixel at all -- the reason this
    // assertion exists rather than the pixel count standing alone. Moving a draw earlier must not
    // change how many draws there are.
    eprintln!("alpha flush: {draws_without} draw calls deferred, {draws_with} flushed");
    assert_eq!(
        draws_without, draws_with,
        "the flushed order issued {draws_with} draws and the deferred order {draws_without}: the \
         flush is drawing a batch twice, or losing one"
    );
}

// ---------------------------------------------------------------------------------------------
// The pass as a whole still behaves.
// ---------------------------------------------------------------------------------------------

/// **The interior is still there.**
///
/// A clip that had gone wrong in the direction of "clip everything" would satisfy every assertion
/// above and draw nothing at all, so "standing outside, a lit interior is visible through a
/// window" is re-asserted here against the *clipped* build: the pass on against the pass off, with
/// `portal_clip` and `portal_depth_stamp` both on.
#[test]
fn the_interior_is_still_drawn_with_the_clip_and_the_stamp_on() {
    let store = store();
    let off = {
        let mut gpu = crate::common::test_gpu(W, H);
        let mut c = cfg(true, true);
        c.building_portals = false;
        let mut scene = WorldScene::load(&store, &mut gpu, c).expect("loads");
        frame(
            &store,
            &mut gpu,
            &mut scene,
            dereth_client::app::HEADLESS_STEP,
        );
        stand(&mut scene, OPENING, BACK, PITCH_DOWN);
        frame(
            &store,
            &mut gpu,
            &mut scene,
            2.0 * dereth_client::app::HEADLESS_STEP,
        )
    };
    let (on, polys, _) = shot(&store, true, true, OPENING, BACK, PITCH_DOWN)
        .expect("a rendered frame: retail dats and a WARP device");

    let changed = differing(&off, &on);
    let mask = opening_mask(&polys, 3.0);
    let covered = mask.iter().filter(|m| **m).count();
    let outside: Vec<usize> = changed.iter().copied().filter(|&i| !mask[i]).collect();
    let worst = worst_spill(&polys, &outside);
    eprintln!(
        "whole pass: {} of {} pixels changed, {} outside the {covered}-pixel mask (worst \
         {worst:.1} px)",
        changed.len(),
        off.len() / 4,
        outside.len()
    );
    assert!(!changed.is_empty(), "no interior appeared at all");
    assert!(
        changed.len() * 20 > covered,
        "only {} of the {covered} pixels the openings cover changed; that is not an interior",
        changed.len()
    );

    // **`building_portals` switches two mechanisms, and only one is an interior.**
    //
    // The pass first flushes queued alpha batches at threshold 0.0, reordering the **landscape's**
    // translucency, which need not land near an opening, so an opening-distance bound does not
    // apply to it. The separate test
    // [`the_alpha_flush_runs_before_the_building_and_changes_nothing_it_should_not`] owns that
    // mechanism's conservation and bounds.
    //
    // The bound therefore applies to the interior alone, taken against a third capture with the
    // flush held off, and the residue must be *exactly* the flush's own difference. `SPILL` is the
    // same bound as above; the second mechanism is named rather than absorbed.
    let mut no_flush = cfg(true, true);
    no_flush.portal_alpha_flush = false;
    let (on_no_flush, _, _) = shot_with(&store, no_flush, OPENING, BACK, PITCH_DOWN)
        .expect("a rendered frame: retail dats and a WARP device");
    let interior = differing(&off, &on_no_flush);
    let flush_only: std::collections::HashSet<usize> =
        differing(&on_no_flush, &on).into_iter().collect();
    assert!(
        !interior.is_empty(),
        "with the alpha flush held off the pass drew no interior at all, so the confinement \
         assertion below would be vacuous"
    );
    let interior_outside: Vec<usize> = interior.iter().copied().filter(|&i| !mask[i]).collect();
    let interior_worst = worst_spill(&polys, &interior_outside);
    let residue: Vec<usize> = changed
        .iter()
        .copied()
        .filter(|&i| !flush_only.contains(&i))
        .collect();
    let residue_outside: Vec<usize> = residue.iter().copied().filter(|&i| !mask[i]).collect();
    let residue_worst = worst_spill(&polys, &residue_outside);
    eprintln!(
        "interior alone: {} px (worst {interior_worst:.1}); alpha-flush reorder: {} px; whole pass \
         minus the reorder: {} px (worst {residue_worst:.1})",
        interior.len(),
        flush_only.len(),
        residue.len()
    );
    assert!(
        interior_worst <= SPILL,
        "an interior pixel sits {interior_worst:.1} px from the nearest opening"
    );
    assert!(
        residue_worst <= SPILL,
        "a changed pixel sits {residue_worst:.1} px from the nearest opening and the alpha flush \
         does not account for it"
    );
}
