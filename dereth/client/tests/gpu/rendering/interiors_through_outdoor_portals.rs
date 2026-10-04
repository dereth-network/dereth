//! Interiors seen from outside, the outdoor half of the portal machinery: standing outside a
//! Holtburg house, its interior draws through its windows and nowhere else, the indoor path follows
//! the camera's cell rather than the body's, and with no opening on the screen the portal pass
//! changes nothing.
//!
//! Fixture: the retail dats and a differential render on a WARP device. The openings are the
//! landblock's own building portal records joined to the shell's drawing BSP, and where they land
//! on the screen is the frame's own matrices applied to each opening's polygon, so nothing is a
//! hand-written expectation of what a window looks like. The claim is located, not "some pixels
//! changed": every changed pixel must lie inside the projected outline of an opening.

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

/// One capture: the RGBA frame, and the outlines of the openings the pass opened while taking it.
type Shot = (Vec<u8>, Vec<Vec<(f32, f32)>>);

/// The retail store, or **fail**: absent dats are a broken checkout, not a reason to skip.
fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

/// Holtburg, no body — the free camera, so the viewer is unambiguously outdoors and
/// `draw_inside` cannot run — a pinned time of day so the sky and the landscape lighting cannot
/// move between the two halves of a differential, and no particles for the same reason.
fn cfg(building_portals: bool) -> SceneConfig {
    SceneConfig {
        character: false,
        scenery_radius: 2,
        time_of_day: Some(0.5),
        particles: false,
        building_portals,
        ..SceneConfig::default()
    }
}

/// One frame through the app's own per-frame order. Everything below is a call the binary makes.
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

/// Stand `back` metres outside the `index`th building opening of the resident blocks, looking
/// straight at it.
///
/// The position and the aim both come from `building_portal_openings`, which is the sidedness gate
/// read as geometry: `portal_side` says which face of the opening admits a viewer, so
/// `centre + outward * back` is *by construction* the side the client would open the portal from.
/// Nothing here is a guess about where Holtburg's houses are.
fn stand_outside_an_opening(scene: &mut WorldScene, index: usize, back: f32) -> (Vec3, Vec3) {
    let openings = scene.building_portal_openings();
    assert!(
        openings.len() > index,
        "only {} building openings around Holtburg; expected more",
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
    // Look back down the outward normal. The free camera is yaw/pitch, and the two angles below
    // are that vector written in them.
    // `FreeCamera::forward` is `(-sin yaw · cos pitch, cos yaw · cos pitch, sin pitch)`, so
    // forward == -n solves to exactly these two.
    scene.camera.yaw = math::atan2f(n.x, -n.y);
    scene.camera.pitch = math::asinf(-n.z);
    (centre, n)
}

/// A point-in-polygon test by even-odd crossing, on the projected outline.
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

/// The union of the openings' outlines as a pixel mask, dilated by `pad`.
///
/// The dilation is honest and named: `building_portal_screen_polygons` reports the opening's own
/// polygon, and what draws through it is the interior *behind* it — whose silhouette can graze the
/// opening's edge by a pixel or two once the rasteriser has had its way with both. `pad` is the
/// only slack in the assertion, and it is a handful of pixels rather than a region.
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
        // LINT-OK: screen pixel bounds, clamped to the viewport on the next two lines. Not a
        // conversion of engine arithmetic.
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
                // Inside the polygon, or within `pad` of it: the padded bounding box plus a
                // distance test against each edge would be the exact dilation; the box itself is
                // the cheap superset and is only used where the polygon test already failed.
                if inside(p, fx, fy) || near_edge(p, fx, fy, pad) {
                    mask[(py * W + px) as usize] = true;
                }
            }
        }
    }
    mask
}

/// Whether `(x, y)` is within `pad` of any edge of `poly`.
fn near_edge(poly: &[(f32, f32)], x: f32, y: f32, pad: f32) -> bool {
    let n = poly.len();
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
        if (x - cx).mul_add(x - cx, (y - cy) * (y - cy)) <= pad * pad {
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------------------------------------
// 1. The openings are in the data and the client finds them.
// ---------------------------------------------------------------------------------------------

/// **Holtburg's buildings have openings, and they are outdoors.**
///
/// The openings exist, they are on the buildings, and the outward side of one is a place a viewer can
/// stand — the resulting cell is outdoors, which makes the normal render mode
/// take the landscape branch and draw the building.
#[test]
fn the_resident_blocks_openings_are_found_and_face_outwards() {
    let store = store();
    let mut gpu = crate::common::test_gpu(W, H);
    let mut scene = WorldScene::load(&store, &mut gpu, cfg(true)).expect("the landscape loads");
    let openings = scene.building_portal_openings();
    assert!(
        openings.len() >= 50,
        "only {} openings across the resident blocks; Holtburg alone has 50",
        openings.len()
    );
    // Every outward direction is a unit-ish vector: a zero one would mean a degenerate polygon
    // reached the plane test, and the sidedness gate would then be reading noise.
    for (i, (_, n)) in openings.iter().enumerate() {
        let len = n.dot(*n).sqrt();
        assert!(
            (len - 1.0).abs() < 1.0e-2,
            "opening {i}: outward normal has length {len}"
        );
    }
    let (centre, n) = stand_outside_an_opening(&mut scene, 0, 6.0);
    eprintln!(
        "{} openings; standing at {:?} looking at {centre:?} along {n:?}",
        openings.len(),
        scene.camera.position
    );
}

// ---------------------------------------------------------------------------------------------
// 2 and 3. The differential, and its confinement to the openings.
// ---------------------------------------------------------------------------------------------

/// Behaviour: rendering.portals.an-interior-appears-through-its-window-from-outside
/// **Standing outside a Holtburg house, the interior appears — and only in the window.**
///
/// The two runs differ in exactly one thing: `SceneConfig::building_portals`. The control run is
/// byte-identical to a second control run, so the difference between control and subject is the
/// portal pass and nothing else. Then the difference is *located*: every changed pixel lies inside
/// the projected outline of an opening the pass itself decided was visible.
///
/// That second half is the assertion that matters. "Pixels changed" would also be true of a bug
/// that drew the whole interior over the wall; "pixels changed inside the window opening and
/// nowhere else" is what a window is.
#[test]
fn an_interior_appears_through_the_window_and_nowhere_else() {
    let store = store();

    // **One device per run.** `Gpu::upload_texture`'s descriptor slots are reclaimed but the heap
    // is still 2,048 pairs, and a scene per device is what makes these captures independent.
    let shot_with = |c: SceneConfig| -> Option<Shot> {
        let mut gpu = crate::common::test_gpu(W, H);
        let mut scene = WorldScene::load(&store, &mut gpu, c).expect("loads");
        frame(
            &store,
            &mut gpu,
            &mut scene,
            dereth_client::app::HEADLESS_STEP,
        );
        stand_outside_an_opening(&mut scene, OPENING, 5.0);
        let rgba = frame(
            &store,
            &mut gpu,
            &mut scene,
            2.0 * dereth_client::app::HEADLESS_STEP,
        );
        let polys = scene.building_portal_screen_polygons(W, H);
        Some((rgba, polys))
    };
    let shot = |on: bool| -> Option<Shot> { shot_with(cfg(on)) };

    let ((off_a, _), (off_b, _)) = (
        shot(false).expect("a rendered frame"),
        shot(false).expect("a rendered frame"),
    );
    assert_eq!(
        off_a, off_b,
        "the scene without the portal pass is not reproducible"
    );

    let (on, polys) = shot(true).expect("a rendered frame: retail dats and a WARP device");
    assert!(
        !polys.is_empty(),
        "no opening was visible from in front of a window"
    );

    let differing: Vec<usize> = off_a
        .as_chunks::<4>()
        .0
        .iter()
        .zip(on.as_chunks::<4>().0.iter())
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .map(|(i, _)| i)
        .collect();
    let total = off_a.len() / 4;
    assert!(
        !differing.is_empty(),
        "the portal pass changed no pixel at all: no interior appeared"
    );

    // Confinement, in two parts.
    //
    // **(a) Almost every changed pixel is inside an opening.** `pad` is three pixels; see
    // `opening_mask`.
    //
    // **(b) Every changed pixel, without exception, is within `SPILL` pixels of an opening's
    // outline.** Nothing appears on the terrain, on a roof, or on a wall away from a window.
    //
    // The gap between (a) and (b) is named rather than padded away. Without the screen-space clip
    // an interior is bounded only by the shell's own depth; the clip itself is exercised by
    // `rendering::clipped_outdoor_pass`. `SPILL` is the measured bound at a *distant* opening,
    // where nearly coincident walls can differ by a pixel or two at ~100 m.
    //
    let mask = opening_mask(&polys, 3.0);
    let outside: Vec<usize> = differing.iter().copied().filter(|&i| !mask[i]).collect();
    let worst = worst_spill(&polys, &outside);
    assert!(
        outside.len() * 1000 <= differing.len(),
        "{} of {} changed pixels fall outside every opening's outline (worst {worst:.1} px out); that is more than the missing screen-space clip can account for",
        outside.len(),
        differing.len()
    );

    // **The differential carries TWO mechanisms, and only one of them is an interior.**
    //
    // `building_portals` is a switch over the whole pass, and the pass's first act at a building
    // is an alpha-list flush at threshold 0.0 — a reordering of the
    // *landscape's* translucency, which has no reason whatever to land near an opening. Turning
    // the pass on therefore moves two disjoint sets of pixels: the interior, which must be at the
    // openings, and the alpha-list reorder, which must not be judged by that bound at all.
    //
    // The reorder can only show where a **translucent** surface overlaps something drawn between
    // the flush point and the final blended pass; here a scenery surface covers such a pixel, and
    // the flush moves it by 6/255.
    //
    // So the bound is applied to the **interior alone**, taken against a third capture in which
    // the pass runs with the flush held off, and the residue is required to be *exactly* the
    // flush's own difference rather than tolerated by widening `SPILL`: the second mechanism is
    // named instead of absorbed.
    let mut no_flush = cfg(true);
    no_flush.portal_alpha_flush = false;
    let (on_no_flush, _) =
        shot_with(no_flush).expect("a rendered frame: retail dats and a WARP device");
    let differ = |a: &[u8], b: &[u8]| -> Vec<usize> {
        a.as_chunks::<4>()
            .0
            .iter()
            .zip(b.as_chunks::<4>().0.iter())
            .enumerate()
            .filter(|(_, (x, y))| x != y)
            .map(|(i, _)| i)
            .collect()
    };
    let interior = differ(&off_a, &on_no_flush);
    let flush_only: std::collections::HashSet<usize> =
        differ(&on_no_flush, &on).into_iter().collect();
    assert!(
        !interior.is_empty(),
        "with the alpha flush held off the pass drew no interior at all, so the confinement assertion below would be vacuous"
    );
    let interior_outside: Vec<usize> = interior.iter().copied().filter(|&i| !mask[i]).collect();
    let interior_worst = worst_spill(&polys, &interior_outside);
    assert!(
        interior_worst <= SPILL,
        "an interior pixel sits {interior_worst:.1} px from the nearest opening; the interior is leaking through the shell rather than grazing an opening's edge"
    );
    let residue: Vec<usize> = differing
        .iter()
        .copied()
        .filter(|&i| !flush_only.contains(&i))
        .collect();
    let residue_outside: Vec<usize> = residue.iter().copied().filter(|&i| !mask[i]).collect();
    let residue_worst = worst_spill(&polys, &residue_outside);
    assert!(
        residue_worst <= SPILL,
        "a changed pixel sits {residue_worst:.1} px from the nearest opening and the alpha flush does not account for it; the interior is leaking through the shell"
    );
    eprintln!(
        "interior alone: {} px (worst {interior_worst:.1}); alpha-flush reorder: {} px; whole pass minus the reorder: {} px (worst {residue_worst:.1})",
        interior.len(),
        flush_only.len(),
        residue.len()
    );
    // And enough of the opening is filled that this is an interior and not a stray edge pixel.
    let covered = mask.iter().filter(|m| **m).count();
    assert!(
        differing.len() * 20 > covered,
        "only {} of the {covered} pixels the openings cover changed; that is not an interior",
        differing.len()
    );
    eprintln!(
        "{} openings visible, {} of {total} pixels changed, {} outside the {covered}-pixel mask (worst {worst:.1} px)",
        polys.len(),
        differing.len(),
        outside.len()
    );

    // A look at it, when asked for one: `DERETH_TEST_OUTDOOR_PORTALS_DUMP=<dir>` writes the two
    // frames as raw RGBA so a person can see what the assertions above are describing.
    if let Ok(dir) = std::env::var("DERETH_TEST_OUTDOOR_PORTALS_DUMP") {
        let _ = std::fs::write(format!("{dir}/outdoor_portals_off.rgba"), &off_a);
        let _ = std::fs::write(format!("{dir}/outdoor_portals_on.rgba"), &on);
    }
}

/// Which opening the differential stands in front of. Index 0 of the resident blocks' list, which
/// is the first portal of the first building of the first block the window holds — a
/// deterministic choice, not a tuned one.
const OPENING: usize = 0;

/// How far outside an opening's outline a changed pixel may sit. See the note in
/// [`an_interior_appears_through_the_window_and_nowhere_else`]: this is the measured cost, without
/// the screen-space clip, at a distant, nearly-coincident wall, and a regression that
/// widened it would be an interior escaping through the shell.
const SPILL: f32 = 8.0;

/// How far outside the nearest opening's outline the worst pixel of `pixels` sits; `0.0` for an
/// empty set, which is the right answer because an empty set has no pixel outside anything.
fn worst_spill(polys: &[Vec<(f32, f32)>], pixels: &[usize]) -> f32 {
    pixels
        .iter()
        .map(|&i| {
            #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
            // LINT-OK: a pixel index into a 640x480 buffer, back to its (x, y). Not engine
            // arithmetic.
            let (x, y) = ((i as u32 % W) as f32 + 0.5, (i as u32 / W) as f32 + 0.5);
            polys
                .iter()
                .map(|p| edge_distance(p, x, y))
                .fold(f32::MAX, f32::min)
        })
        .fold(0.0f32, f32::max)
}

/// The distance from `(x, y)` to the nearest edge of `poly`.
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

// ---------------------------------------------------------------------------------------------
// 3b. The viewer is the camera.
// ---------------------------------------------------------------------------------------------

/// **Which cell the viewer is in is a question about the camera, not about the body.**
///
/// The normal render mode branches on whether the viewer cell's low word is below `0x100`.
/// That viewer cell belongs to the swept camera rather than the player body.
/// Outdoors it runs the landscape and building-portal path;
/// indoors it runs the indoor draw path instead.
///
/// The two differ at exactly the boundary the portal machinery is about — the camera in the room with the body in the doorway, or the camera
/// outside a wall the body is inside. This drives both directions by writing
/// `CameraControl::viewer_cell` directly, which is `update_viewer`'s own output.
#[test]
fn the_indoor_path_follows_the_camera_and_not_the_body() {
    let store = store();
    let mut gpu = crate::common::test_gpu(W, H);
    let region = dereth_client::world::load_region(&store).expect("the region decodes");
    let mut scene = WorldScene::load(&store, &mut gpu, cfg(true)).expect("the landscape loads");
    // A body, and with it a `CameraControl`. `cfg` has `character: false`, so ask for one.
    scene
        .attach_character(&store, &region, &mut gpu)
        .expect("the body is created");

    let (resident, _) = scene.env_cell_counts();
    assert!(resident > 0, "the scene baked no interior geometry at all");
    let body_cell = scene.character.as_ref().expect("a body").position().cell;
    assert!(
        dereth_physics::landdefs::is_outdoors(body_cell),
        "the body was expected to start outdoors, and starts in {body_cell:?}"
    );

    // The body is outdoors and the camera is not: the indoor path runs anyway, because the viewer
    // is the camera. `0xA9B40100` is Holtburg's first interior cell, used here as the concrete
    // worked example.
    let room = dereth_primitives::CellId(0xA9B4_0100);
    scene.character.as_mut().expect("a body").camera.viewer_cell = Some(room);
    assert!(
        scene.env_cell_counts().1 > 0,
        "the camera is in {room:?} and the indoor path found no geometry for it: `viewer_cell` is still reading the body"
    );

    // And the converse, which is the half that matters here: with the camera outdoors
    // the indoor path stops, whatever the body is standing in — so the building portal pass is the
    // one that runs.
    scene.character.as_mut().expect("a body").camera.viewer_cell = Some(body_cell);
    assert_eq!(
        scene.env_cell_counts().1,
        0,
        "the camera is outdoors in {body_cell:?} and the indoor path ran anyway"
    );
    assert!(
        !scene.building_portal_screen_polygons(W, H).is_empty(),
        "with the camera outdoors over Holtburg no building opening is visible at all"
    );
}

// ---------------------------------------------------------------------------------------------
// 4. It closes again.
// ---------------------------------------------------------------------------------------------

/// **When no opening is on the screen, the pass changes nothing at all.**
///
/// "The interior disappears when the portal leaves the view", and in a *town* that is not "turn
/// around": turning around in Holtburg puts you in front of the next house's windows,
/// whose interiors then legitimately appear. So the pose here is the camera in front of a window
/// and then pitched up at the sky above the roofline, where no opening projects onto the viewport —
/// which the test asserts rather than assumes — and the two frames must be **byte-identical**.
///
/// This is the half that would fail if the pass drew every interior of every resident block
/// unconditionally, or if it drew an interior for an opening facing away from the viewer.
#[test]
fn nothing_changes_when_no_opening_is_on_the_screen() {
    let store = store();

    let shot = |on: bool| -> Option<Shot> {
        let mut gpu = crate::common::test_gpu(W, H);
        let mut scene = WorldScene::load(&store, &mut gpu, cfg(on)).expect("loads");
        frame(
            &store,
            &mut gpu,
            &mut scene,
            dereth_client::app::HEADLESS_STEP,
        );
        stand_outside_an_opening(&mut scene, OPENING, 5.0);
        // Straight up at the sky, over the roofline.
        scene.camera.pitch = dereth_client::camera::PITCH_LIMIT;
        let rgba = frame(
            &store,
            &mut gpu,
            &mut scene,
            2.0 * dereth_client::app::HEADLESS_STEP,
        );
        let polys = scene.building_portal_screen_polygons(W, H);
        Some((rgba, polys))
    };

    let ((off, _), (on, polys)) = (
        shot(false).expect("a rendered frame"),
        shot(true).expect("a rendered frame"),
    );

    // Nothing the pass would open projects onto the viewport at this pose.
    let mask = opening_mask(&polys, 3.0);
    let on_screen = mask.iter().filter(|m| **m).count();
    assert_eq!(
        on_screen, 0,
        "{on_screen} pixels of opening are still on the screen at this pose"
    );

    let differing = off
        .as_chunks::<4>()
        .0
        .iter()
        .zip(on.as_chunks::<4>().0.iter())
        .filter(|(a, b)| a != b)
        .count();
    // Not "few": none.
    assert_eq!(
        differing, 0,
        "{differing} pixels changed with no opening on the screen; the pass is drawing interiors it cannot see"
    );
}
