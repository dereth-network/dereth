//! The drawn frame is pitched down 16.7 degrees, as retail's is. The shipped eye offset is
//! `(0, -2.5 * s, 0.75 * s)` with the gameplay screen's camera scale `s = 1.1`, so `offset.z` is
//! 0.825, over the 0.75 threshold above which the camera takes `LOOK_AT_PIVOT` and re-aims the eye
//! at the pivot: `atan(0.825 / 2.75)` = 16.699 degrees down. A level camera draws world verticals
//! as image verticals; this one makes a door-shaped pair of vertical world lines, pushed through
//! `view_from_frame` / `perspective_fov_lh` into viewport pixels, converge going down, and their
//! vanishing point gives the pitch. Fixture: a body standing still on the retail
//! `DEFAULT_LANDBLOCK`, the camera driven by the frame loop until it settles, on a software GPU
//! device. Fails when the dats or the device are absent.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use dereth_client::world::SceneWrites;
use std::sync::Arc;

use dereth_client::camera::{target, CameraInput, FreeCamera, GAMEPLAY_CAMERA_SCALE};
use dereth_client::character::CharacterInput;
use dereth_client::world::{SceneConfig, WorldScene};
use dereth_dat::RetailDatStore;
use dereth_primitives::num::math;
use dereth_primitives::{LocalTime, Vec3};
use dereth_render::camera::{
    compute_aspect_for_viewport, fov_y_from_preference, perspective_fov_lh, swap_forward_and_up,
    view_from_frame, AspectPreference, Viewport, DEFAULT_FOV_DEGREES, DEG_TO_RAD, ZFAR, ZNEAR,
};

/// The pitch the retail constants demand: `atan(0.75 * 1.1 / (2.5 * 1.1))`, which is
/// `atan(0.3)` and independent of the scale — the scale only decides whether
/// the offset-target selection takes the `LOOK_AT_PIVOT` arm at all.
fn retail_pitch_degrees() -> f64 {
    math::atan(0.75f64 * 1.1 / (2.5 * 1.1)).to_degrees()
}

/// A 1200x900 client area with the Normal aspect preference. Only the viewport *shape* matters
/// to the geometry below, and this one is 4:3 like the retail screenshots.
const VIEW_W: u32 = 1200;
const VIEW_H: u32 = 900;

/// The retail store, or **fail**: an absent dat is a failure, never a skip.
fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

/// One world point, pushed through exactly the matrices a vertex reaches: the world matrix has
/// already applied the Z-up -> D3D y/z swap, then the renderer's view
/// matrix, then its field-of-view projection, then the viewport.
///
/// `None` when the point is behind the eye, which would make the divide meaningless.
fn to_pixels(
    cam: &FreeCamera,
    vp: Viewport,
    fov_y: f32,
    aspect: f32,
    p: Vec3,
) -> Option<(f64, f64)> {
    let view = view_from_frame(&cam.frame());
    let proj = perspective_fov_lh(fov_y, aspect, ZNEAR, ZFAR);
    let d = swap_forward_and_up(glam::Vec3::new(p.x, p.y, p.z));
    let clip = proj * view * glam::Vec4::new(d.x, d.y, d.z, 1.0);
    if clip.w <= 1e-4 {
        return None;
    }
    let ndc_x = f64::from(clip.x / clip.w);
    let ndc_y = f64::from(clip.y / clip.w);
    let x = f64::from(vp.x) + (ndc_x * 0.5 + 0.5) * f64::from(vp.width);
    let y = f64::from(vp.y) + (0.5 - ndc_y * 0.5) * f64::from(vp.height);
    Some((x, y))
}

/// The image of a vertical world line, as the two pixels its ends land on. The projection of a
/// straight line is a straight line, so these two points are the whole of it.
struct Edge {
    top: (f64, f64),
    bottom: (f64, f64),
}

impl Edge {
    /// Where this edge crosses a given scan row, as the edge would be read off a screenshot.
    fn x_at_row(&self, row: f64) -> f64 {
        let (x0, y0) = self.top;
        let (x1, y1) = self.bottom;
        x0 + (x1 - x0) * (row - y0) / (y1 - y0)
    }

    /// The image slope, in pixels of x per pixel of y. Zero is dead vertical.
    fn slope(&self) -> f64 {
        (self.bottom.0 - self.top.0) / (self.bottom.1 - self.top.1)
    }
}

/// A door-shaped silhouette in front of the camera: two vertical world lines a metre apart,
/// four metres ahead, spanning the two metres of wall a doorway occupies. Built from the
/// camera's own horizontal forward and right, so it is square-on however the body is facing and
/// carries no pitch of its own.
fn door_edges(cam: &FreeCamera, vp: Viewport, fov_y: f32, aspect: f32) -> (Edge, Edge) {
    let f = cam.forward();
    let flat = (f.x * f.x + f.y * f.y).sqrt();
    assert!(flat > 1e-3, "the camera is looking straight down: {f:?}");
    let hf = Vec3::new(f.x / flat, f.y / flat, 0.0);
    let r = cam.right();
    let centre = Vec3::new(
        cam.position.x + hf.x * 4.0,
        cam.position.y + hf.y * 4.0,
        cam.position.z,
    );
    // The eye is 2.25 m above the body's origin, so this spans a doorway from the floor to two
    // metres up.
    let z_lo = centre.z - 2.25;
    let z_hi = centre.z - 0.25;
    let edge = |side: f32| {
        let x = centre.x + r.x * side * 0.5;
        let y = centre.y + r.y * side * 0.5;
        let top = to_pixels(cam, vp, fov_y, aspect, Vec3::new(x, y, z_hi))
            .expect("the doorway's head is in front of the eye");
        let bottom = to_pixels(cam, vp, fov_y, aspect, Vec3::new(x, y, z_lo))
            .expect("the doorway's sill is in front of the eye");
        Edge { top, bottom }
    };
    (edge(-1.0), edge(1.0))
}

/// Drive the real client for long enough that the smoother has settled, standing still.
fn settled_camera() -> (FreeCamera, dereth_client::camera::CameraManager) {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    let region = dereth_client::world::load_region(&store).expect("the region decodes");
    let mut scene =
        WorldScene::load(&store, &mut gpu, SceneConfig::default()).expect("the scene loads");
    scene
        .attach_character(&store, &region, &mut gpu)
        .expect("the body is created");

    let step = dereth_physics::globals::MIN_QUANTUM;
    #[allow(clippy::cast_possible_truncation)] // LINT-OK: a fixed simulation step in seconds
    let dtf = step as f32;
    let mut now = 0.0f64;
    // The smoother closes 45% of the gap per 100 ms; four seconds of standing still is far past
    // the 0.0002 rotational early-out.
    for _ in 0..240 {
        now += step;
        scene.update(
            CameraInput::default(),
            CharacterInput::default(),
            LocalTime(now),
            dtf,
        );
        dereth_client::camera::update_viewer(
            &mut scene,
            CameraInput::default(),
            LocalTime(now),
            step,
        );
    }
    let c = scene.character.as_ref().expect("a body");
    assert!(
        c.camera.stats.sweeps > 0,
        "no sweep ran, so the camera never ran either"
    );
    (scene.camera, c.camera.manager.clone())
}

/// Behaviour: camera.pitch.the-drawn-frame-is-pitched-down-as-retail
/// **The verdict.** The drawn silhouette of a vertical world line is not vertical, and the angle
/// it implies is the 16.699 degrees the retail camera constants demand.
#[test]
fn the_drawn_door_silhouette_converges_at_retail_s_sixteen_degrees() {
    let (cam, _cm) = settled_camera();

    #[allow(clippy::cast_precision_loss)] // LINT-OK: a viewport size, exact in f32
    let (w, h) = (VIEW_W as f32, VIEW_H as f32);
    let display_aspect = AspectPreference::Normal.display_aspect_ratio(w, h);
    let aspect = compute_aspect_for_viewport(w, h, display_aspect, false);
    let fov_y = fov_y_from_preference(DEFAULT_FOV_DEGREES * DEG_TO_RAD, aspect);
    let vp = Viewport {
        x: 0,
        y: 0,
        width: VIEW_W,
        height: VIEW_H,
    };

    let (left, right) = door_edges(&cam, vp, fov_y, aspect);

    // The silhouette's width at the top of a 250-row band and at the bottom of it, on the drawn
    // frame.
    let row_top = left.top.1.max(right.top.1) + 2.0;
    let row_bottom = row_top + 250.0;
    let w_top = right.x_at_row(row_top) - left.x_at_row(row_top);
    let w_bottom = right.x_at_row(row_bottom) - left.x_at_row(row_bottom);
    let drift_right = right.x_at_row(row_bottom) - right.x_at_row(row_top);
    let drift_left = left.x_at_row(row_bottom) - left.x_at_row(row_top);
    eprintln!(
        "drawn door over 250 rows: width {w_top:.1} -> {w_bottom:.1} px, right edge drifts \
         {drift_right:+.1} px, left edge {drift_left:+.1} px"
    );

    // A level camera draws world verticals as image verticals: both drifts are exactly zero.
    assert!(
        drift_right.abs() > 5.0 && drift_left.abs() > 5.0,
        "the door's edges are dead vertical over 250 rows ({drift_left:+.2} px left, \
         {drift_right:+.2} px right): the drawn camera is not pitched at all"
    );
    // The silhouette **converges going down**, which is the direction a camera pitched *down*
    // converges in: the vanishing point world verticals run to is the **nadir**, `-z`, and for a
    // downward pitch that is the one in front of the eye, below the image. A retail screenshot of
    // a doorway narrows going down too.
    assert!(
        w_bottom < w_top - 5.0,
        "the silhouette does not converge downwards: {w_top:.1} px at the top and \
         {w_bottom:.1} px 250 rows below"
    );

    // The vanishing point itself: where the two image lines meet. With no roll it sits on the
    // viewport's vertical centre line, and its distance below the centre is `f / tan(pitch)`.
    let (sl, sr) = (left.slope(), right.slope());
    assert!(
        (sl - sr).abs() > 1e-9,
        "the edges are parallel: there is no vanishing point"
    );
    // x_l(row) == x_r(row) solved for row.
    let row_vp = {
        let xl0 = left.x_at_row(0.0);
        let xr0 = right.x_at_row(0.0);
        (xr0 - xl0) / (sl - sr)
    };
    let cy = f64::from(VIEW_H) * 0.5;
    let focal = cy / f64::from(math::tanf(fov_y * 0.5));
    let pitch = math::atan(focal / (row_vp - cy)).to_degrees();
    let want = retail_pitch_degrees();
    eprintln!(
        "the world-vertical vanishing point is at row {row_vp:.1} ({:.1} px below the centre), \
         which is a pitch of {pitch:.3} degrees against retail's {want:.3}",
        row_vp - cy
    );
    // The nadir below the centre is a camera pitched *down*, which is what retail shows and what
    // `LOOK_AT_PIVOT` from an eye 0.825 above the pivot produces.
    assert!(
        row_vp > cy,
        "the vanishing point is above the viewport centre, so the drawn camera is pitched up"
    );
    assert!(
        (pitch - want).abs() < 0.5,
        "the drawn camera is pitched {pitch:.3} degrees where the retail camera constants \
         give {want:.3}"
    );
}

/// The mechanism, so a later reader can tell *why* the frame above is pitched: the frame loop
/// itself applies the gameplay camera scale of 1.1, and the scaled offset is what picks
/// up `LOOK_AT_PIVOT`. Nothing here calls `set_scale`; it is read back out of a camera that only
/// `CameraControl::update` has touched.
#[test]
fn the_frame_loop_applies_the_gameplay_ui_s_camera_scale() {
    let (_cam, cm) = settled_camera();
    assert!(
        (cm.scale - GAMEPLAY_CAMERA_SCALE).abs() < 1e-6,
        "the frame loop left the camera at scale {}",
        cm.scale
    );
    let want = Vec3::new(
        0.0,
        -2.5 * GAMEPLAY_CAMERA_SCALE,
        0.75 * GAMEPLAY_CAMERA_SCALE,
    );
    assert!(
        (cm.viewer_offset.x - want.x).abs() < 1e-5
            && (cm.viewer_offset.y - want.y).abs() < 1e-5
            && (cm.viewer_offset.z - want.z).abs() < 1e-5,
        "the shipped offset is {:?}, not {want:?}",
        cm.viewer_offset
    );
    assert!(
        cm.target_status & target::LOOK_AT_PIVOT != 0,
        "the target mask is {:#x} and carries no LOOK_AT_PIVOT, so the eye is never re-aimed",
        cm.target_status
    );
}
