//! Projection and view matrices, aspect handling, and the Z-up ↔ D3D swap.
//!
//! The camera preserves the projection parameters, view matrix and fixed-function conventions.
//!
//! This module owns only what turns a final `Frame` plus preferences into matrices. The
//! swept-sphere camera, the smoother and the mouse-look dead zone live with the world and physics
//! code, not here.

use dereth_primitives::num::math;
use dereth_primitives::Frame;
use glam::{Mat4, Vec3};

// The projection *parameters* -- the near/far planes, the FOV and
// aspect constants and their two functions, the fog and light blocks, and `ViewParams` itself --
// are `dereth_client_contract::camera`, for the same reason `Viewport` is `dereth_primitives::Viewport`:
// `dereth_client_runtime::present::Scene` names `ViewParams` in a signature, and a crate
// that must not depend on a renderer cannot name one. Everything is re-exported here, so
// `dereth_render::camera::ViewParams` and every sibling resolve. What stays below is the arithmetic: the matrices, the viewport clamp and the
// game-viewport computation.
pub use dereth_client_contract::camera::{
    compute_aspect_for_viewport, fov_y_from_preference, set_vdst, view_distance_from_fov,
    AspectPreference, FogParams, Light, LightBlock, ViewParams, ASPECT_FACTOR, DEFAULT_FOV_DEGREES,
    DEG_TO_RAD, FOV_ASPECT_BIAS, ZFAR, ZFAR_SKY, ZNEAR,
};
// `Viewport` is `dereth_primitives::Viewport`: it is plain data that two of
// `dereth-client`'s logic modules need without the rest of the renderer. Re-exported here, so
// `dereth_render::camera::Viewport` names it too.
pub use dereth_primitives::Viewport;

/// The client's clamp: `x` to the render target's `width-1`, `y` to
/// `height-1`, then `w`/`h` so the rectangle fits.
#[must_use]
pub fn clamp_viewport(v: Viewport, target_w: u32, target_h: u32) -> Viewport {
    let x = v.x.min(target_w.saturating_sub(1));
    let y = v.y.min(target_h.saturating_sub(1));
    let width = v.width.min(target_w - x);
    let height = v.height.min(target_h - y);
    Viewport {
        x,
        y,
        width,
        height,
    }
}

/// The game viewport -- the 3D viewport is the display with
/// the docked UI bars subtracted. Only the *name* of the edge enum is unconfirmed.
///
/// `bars` is `(edge, x, y, width, height)` per visible docked object, with `edge` 1 = top,
/// 2 = bottom, 3 = left, 4 = right.
#[must_use]
pub fn compute_game_viewport(
    display_w: u32,
    display_h: u32,
    bars: &[(u8, i64, i64, i64, i64)],
) -> Viewport {
    let mut left: i64 = 0;
    let mut top: i64 = 0;
    let mut right: i64 = i64::from(display_w) - 1;
    let mut bottom: i64 = i64::from(display_h) - 1;
    for &(edge, x, y, w, h) in bars {
        match edge {
            1 => top = top.max(y + h),
            2 => bottom = bottom.min(y - 1),
            3 => left = left.max(x + w),
            4 => right = right.min(x - 1),
            _ => {}
        }
    }
    left = left.min(i64::from(display_w) - 2);
    top = top.min(i64::from(display_h) - 2);
    if right <= left {
        right = left + 1;
    }
    if bottom <= top {
        bottom = top + 1;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // LINT-OK: integer arithmetic throughout; the clamps above bound every value into u32.
    Viewport {
        x: left as u32,
        y: top as u32,
        width: (right - left + 1) as u32,
        height: (bottom - top + 1) as u32,
    }
}

/// `D3DXMatrixPerspectiveFovLH(fovy, aspect, zn, zf)`, which is what the client's field-of-view
/// setter calls.
///
/// The client's convention, spelled out rather than delegated so the arithmetic is visible:
/// `yScale = cot(fovY/2)`, `xScale = yScale / aspect`, `m33 = zf/(zf−zn)`, `m43 = −zn·zf/(zf−zn)`.
///
/// UNVERIFIED: `D3DXMatrixPerspectiveFovLH` computes the
/// cotangent as the quotient `cos(fovY/2) / sin(fovY/2)` at the client's 53-bit precision and
/// then stores to `float`. Whether an `f64` `cos`/`sin` quotient reproduces that bit-for-bit over
/// the client's FOV range can only be settled side by side against the 2003-era D3DX. The `f64`
/// intermediate here is the documented approximation: it matches the 53-bit working precision, which
/// is the part that is known.
#[must_use]
pub fn projection(v: &ViewParams) -> Mat4 {
    match view_distance_override::get() {
        Some(d) => {
            let (fov, znear) = set_vdst(d);
            perspective_fov_lh(fov, v.aspect, znear, v.zfar)
        }
        None => perspective_fov_lh(v.fov_y_rad, v.aspect, v.znear, v.zfar),
    }
}

/// View-distance override and its field of view, reproduced as renderer state.
///
/// The near plane and FOV are **file-scope globals** in the original renderer. The view-distance
/// override reaches them through the renderer's own setter
/// so that every subsequent draw -- the world, and the preview space
/// when it is set to use the world view's FOV -- picks it up without
/// being told. The teleport tunnel is that override ramping to 0.001 and back.
///
/// Scoped to the calling thread rather than the process, because rendering happens on one thread
/// and a process-wide cell would leak between concurrently running tests. Default is `None`, which
/// is the override switched off and leaves [`projection`] exactly as it was.
pub mod view_distance_override {
    use std::cell::Cell;

    thread_local! {
        static VIEW_DISTANCE: Cell<Option<f32>> = const { Cell::new(None) };
    }

    /// Set or clear the global field-of-view distance override.
    pub fn set(d: Option<f32>) {
        VIEW_DISTANCE.with(|c| c.set(d));
    }

    /// What [`projection`](super::projection) will use, if anything.
    #[must_use]
    pub fn get() -> Option<f32> {
        VIEW_DISTANCE.with(Cell::get)
    }

    /// Run `f` with the override set, and put back whatever was there before.
    ///
    /// The client has no such bracket — it simply leaves the global set — but a caller that wants
    /// the override to apply to exactly one draw wants this, and so does every test.
    pub fn with<R>(d: Option<f32>, f: impl FnOnce() -> R) -> R {
        let prev = get();
        set(d);
        let r = f();
        set(prev);
        r
    }
}

/// The bare matrix builder, so tests can drive it without a whole [`ViewParams`].
#[must_use]
pub fn perspective_fov_lh(fov_y: f32, aspect: f32, zn: f32, zf: f32) -> Mat4 {
    let half = f64::from(fov_y) * 0.5;
    // cot(x) as the cos/sin quotient the original evaluates, at 53-bit precision, then stored to float.
    // The narrowing *is* the behaviour being reproduced: D3DXMatrixPerspectiveFovLH computes the
    // cotangent at the client's `_PC_53` precision and stores it into a `float` field.
    #[allow(clippy::cast_possible_truncation)]
    // LINT-OK: the f64 -> f32 store is the modelled behaviour, not an accident.
    let y_scale = (math::cos(half) / math::sin(half)) as f32;
    let x_scale = y_scale / aspect;
    let q = zf / (zf - zn);
    Mat4::from_cols_array(&[
        x_scale,
        0.0,
        0.0,
        0.0, //
        0.0,
        y_scale,
        0.0,
        0.0, //
        0.0,
        0.0,
        q,
        1.0, //
        0.0,
        0.0,
        -q * zn,
        0.0,
    ])
}

/// The D3D view matrix, built by hand from a [`Frame`] exactly as the client's viewpoint update
/// does.
///
/// The client is Z-up and D3D is Y-up, hence the swap of the 2nd and 3rd columns:
///
/// ```text
/// l = globaltolocal(frame, (0,0,0))           // −Rᵀ·origin
/// V = | X.x  Z.x  Y.x  0 |
///     | X.z  Z.z  Y.z  0 |
///     | X.y  Z.y  Y.y  0 |
///     | l.x  l.z  l.y  1 |
/// ```
///
/// with `Xaxis, Yaxis, Zaxis` the rows 0, 1 and 2 of the frame's 3×3.
///
/// **The swap is applied to both indices**, so the matrix expects its input already in D3D order:
/// a client-space point `(x, y, z)` reaches it as `(x, z, y)`. That is what the *world* matrix
/// does (the world matrix swaps forward and up), so by the
/// time a vertex meets the view matrix it is Y-up. Use [`swap_forward_and_up`] on a bare client
/// point before transforming it.
#[must_use]
pub fn view_from_frame(f: &Frame) -> Mat4 {
    let m = rotation_matrix(f);
    // Rows 0, 1, 2 of the 3x3.
    let x_axis = m[0];
    let y_axis = m[1];
    let z_axis = m[2];
    let o = [f.origin.x, f.origin.y, f.origin.z];
    // globaltolocal(frame, (0,0,0)) = -R^T * origin, i.e. minus the dot of each row with the origin.
    let l = [
        -(x_axis[0] * o[0] + x_axis[1] * o[1] + x_axis[2] * o[2]),
        -(y_axis[0] * o[0] + y_axis[1] * o[1] + y_axis[2] * o[2]),
        -(z_axis[0] * o[0] + z_axis[1] * o[1] + z_axis[2] * o[2]),
    ];
    // glam multiplies `M * v` as a linear combination of *columns*, while D3D multiplies `v * M`
    // with the basis vectors in *rows*. Column i of the glam matrix is therefore row i of the D3D
    // matrix, and `from_cols_array` is fed the four D3D rows in order.
    Mat4::from_cols_array(&[
        x_axis[0], z_axis[0], y_axis[0], 0.0, // D3D row 0: | X.x  Z.x  Y.x  0 |
        x_axis[2], z_axis[2], y_axis[2], 0.0, // D3D row 1: | X.z  Z.z  Y.z  0 |
        x_axis[1], z_axis[1], y_axis[1], 0.0, // D3D row 2: | X.y  Z.y  Y.y  0 |
        l[0], l[2], l[1], 1.0, // D3D row 3: | l.x  l.z  l.y  1 |
    ])
}

/// The frame's local-to-global 3×3, **row by row**: row `j` is the frame's local axis `j` expressed
/// in world coordinates.
///
/// That is the client's row-vector convention: a view matrix takes the camera's `Xaxis`, `Yaxis`,
/// and `Zaxis` from rows 0, 1, and 2. Storing the transpose instead is invisible for an unrotated
/// frame and wrong for every other one, which is why [`view_from_frame`]'s tests drive a rotated frame.
///
/// `dereth_primitives::Quat` stores `w` first because that is the dat order.
fn rotation_matrix(f: &Frame) -> [[f32; 3]; 3] {
    let (w, x, y, z) = (f.rotation.w, f.rotation.x, f.rotation.y, f.rotation.z);
    let (xx, yy, zz) = (x * x, y * y, z * z);
    let (xy, xz, yz) = (x * y, x * z, y * z);
    let (wx, wy, wz) = (w * x, w * y, w * z);
    [
        [1.0 - 2.0 * (yy + zz), 2.0 * (xy + wz), 2.0 * (xz - wy)],
        [2.0 * (xy - wz), 1.0 - 2.0 * (xx + zz), 2.0 * (yz + wx)],
        [2.0 * (xz + wy), 2.0 * (yz - wx), 1.0 - 2.0 * (xx + yy)],
    ]
}

/// The Z-up → D3D y/z swap on a bare point, which is what
/// applies through the forward/up matrix swap: client `(x, y, z)` reaches D3D as `(x, z, y)`.
#[must_use]
pub fn swap_forward_and_up(v: Vec3) -> Vec3 {
    Vec3::new(v.x, v.z, v.y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_primitives::{Quat, Vec3 as TVec3};

    // At the 4:3 default (viewport aspect ratio 1.3333), the 90-degree preference becomes a
    // vertical FOV of
    // 90 / 1.2333 = 72.97 degrees; at Wide (16:9, 1.7778) it becomes 53.6 degrees." Contract 11.5.
    #[test]
    fn the_documented_field_of_view_values_are_reproduced() {
        let pref = DEFAULT_FOV_DEGREES * DEG_TO_RAD;

        // 4:3: a full-window viewport at 800x600 with the Normal preference gives aspect 4/3
        // exactly, because `compute_aspect_for_viewport`'s (4/3) * 0.75 cancels the viewport ratio.
        let display_aspect = AspectPreference::Normal.display_aspect_ratio(800.0, 600.0);
        let aspect = compute_aspect_for_viewport(800.0, 600.0, display_aspect, false);
        assert!((aspect - 4.0 / 3.0).abs() < 1e-6, "{aspect}");
        let fov = fov_y_from_preference(pref, aspect);
        assert!(
            (fov.to_degrees() - 72.97).abs() < 0.01,
            "{}",
            fov.to_degrees()
        );

        // Wide on the same 800x600 window: (16/9)*(3/4)*(4/3) = 16/9.
        let display_aspect = AspectPreference::Wide.display_aspect_ratio(800.0, 600.0);
        let aspect = compute_aspect_for_viewport(800.0, 600.0, display_aspect, false);
        assert!((aspect - 16.0 / 9.0).abs() < 1e-6, "{aspect}");
        let fov = fov_y_from_preference(pref, aspect);
        assert!(
            (fov.to_degrees() - 53.6).abs() < 0.05,
            "{}",
            fov.to_degrees()
        );
    }

    // Observed viewport aspect: raw mode uses w/h; otherwise it uses
    // (w/h) * display_aspect_ratio * 0.75.
    #[test]
    fn aspect_is_the_viewport_ratio_times_the_display_ratio_times_three_quarters() {
        // A 1920x1080 window on Auto: display aspect is the window ratio, so the product is
        // (16/9) * (16/9) * 0.75.
        let da = AspectPreference::Auto.display_aspect_ratio(1920.0, 1080.0);
        assert!((da - 16.0 / 9.0).abs() < 1e-6);
        let a = compute_aspect_for_viewport(1920.0, 1080.0, da, false);
        assert!((a - (16.0 / 9.0) * (16.0 / 9.0) * 0.75).abs() < 1e-5, "{a}");
        // Raw ignores both extra factors.
        assert!((compute_aspect_for_viewport(1920.0, 1080.0, da, true) - 16.0 / 9.0).abs() < 1e-6);
        // A docked UI bar shrinks the viewport and therefore changes the projection.
        // A bottom bar costs height, so the aspect
        // widens; a side bar costs width, so it narrows.
        let full = compute_aspect_for_viewport(800.0, 600.0, 4.0 / 3.0, false);
        let bottom_bar = compute_aspect_for_viewport(800.0, 500.0, 4.0 / 3.0, false);
        let side_bar = compute_aspect_for_viewport(700.0, 600.0, 4.0 / 3.0, false);
        assert!(bottom_bar > full, "{bottom_bar} vs {full}");
        assert!(side_bar < full, "{side_bar} vs {full}");
    }

    // Observed view-distance conversion: fov = 2*atan(1/d) and
    // znear = (d >= 0.4) ? 0.1 : d * 0.25.
    #[test]
    fn set_vdst_matches_the_documented_formula() {
        let (fov, zn) = set_vdst(1.0);
        assert!((fov - std::f32::consts::FRAC_PI_2).abs() < 1e-6, "{fov}");
        assert_eq!(zn, 0.1);
        let (fov, zn) = set_vdst(0.4);
        assert!(
            (fov - 2.0 * math::atanf(1.0f32 / 0.4)).abs() < 1e-6,
            "{fov}"
        );
        assert_eq!(zn, 0.1, "d == 0.4 takes the >= branch");
        let (_, zn) = set_vdst(0.2);
        assert!((zn - 0.05).abs() < 1e-7, "{zn}");
    }

    /// Oracle: the client's `1 / tan(0.5 * fov)`, which is the
    /// exact inverse of `set_vdst`'s FOV; the renderer caches it as its view distance.
    #[test]
    fn the_view_distance_and_the_fov_are_inverses_of_each_other() {
        for d in [0.001_f32, 0.2, 0.4, 1.0, 1.354_264_5, 4.0] {
            let (fov, _) = set_vdst(d);
            let back = view_distance_from_fov(fov);
            assert!(
                (back - d).abs() < 1e-4 * d.max(1.0),
                "{d} -> {fov} -> {back}"
            );
        }
        // The default 90 degree preference at 4:3.
        let aspect = compute_aspect_for_viewport(800.0, 600.0, 4.0 / 3.0, false);
        let fov = fov_y_from_preference(DEFAULT_FOV_DEGREES * DEG_TO_RAD, aspect);
        let d = view_distance_from_fov(fov);
        assert!(
            (1.0..2.0).contains(&d),
            "the game's own view distance is about 1.35, got {d}"
        );
    }

    /// Oracle: the override is renderer state that
    /// every subsequent draw picks up, and the override switched off must leave the projection
    /// byte-identical to the projection with no override at all.
    ///
    /// This is the failing-when-removed test for the override: with `set(None)` the matrix is the
    /// preference FOV's, with `set(Some(0.001))` it is a 179.9 degree one, and the two differ.
    #[test]
    fn the_view_distance_override_replaces_the_projection_and_is_inert_when_off() {
        let v = ViewParams::default();
        let plain = projection(&v);
        assert_eq!(view_distance_override::get(), None, "off by default");

        let collapsed = view_distance_override::with(Some(0.001), || projection(&v));
        assert_ne!(plain.to_cols_array(), collapsed.to_cols_array());
        // A 179.9 degree vertical FOV has a cotangent of about 0.001, so yScale collapses.
        let (fov, znear) = set_vdst(0.001);
        assert!(fov > 3.13 && fov < std::f32::consts::PI, "{fov}");
        assert!((znear - 0.000_25).abs() < 1e-9);
        assert_eq!(
            collapsed.to_cols_array(),
            perspective_fov_lh(fov, v.aspect, znear, v.zfar).to_cols_array()
        );

        // The bracket puts it back, so nothing leaks into the next draw.
        assert_eq!(view_distance_override::get(), None);
        assert_eq!(projection(&v).to_cols_array(), plain.to_cols_array());
    }

    // Observed projection distances: near 0.1, world far 4000, sky far 16000.
    #[test]
    fn the_depth_range_constants_are_the_shipped_ones() {
        assert_eq!(ZNEAR, 0.1);
        assert_eq!(ZFAR, 4000.0);
        assert_eq!(ZFAR_SKY, 16000.0);
    }

    // Oracle: D3DXMatrixPerspectiveFovLH's documented convention uses `m43 = -(q*zn)`. Checked by
    // transforming known points rather than by restating the
    // matrix: a point on the near plane maps to z = 0, one on the far plane to z = 1.
    #[test]
    fn the_projection_maps_the_near_and_far_planes_to_zero_and_one() {
        let m = perspective_fov_lh(std::f32::consts::FRAC_PI_2, 4.0 / 3.0, ZNEAR, ZFAR);
        for (z_view, expected) in [(ZNEAR, 0.0f32), (ZFAR, 1.0f32)] {
            let p = m * glam::Vec4::new(0.0, 0.0, z_view, 1.0);
            assert!((p.w - z_view).abs() < 1e-3, "w must be view z, got {}", p.w);
            let ndc_z = p.z / p.w;
            assert!((ndc_z - expected).abs() < 1e-4, "{ndc_z} != {expected}");
        }
        // A 90 degree vertical FOV puts a point at y = z on the top edge of the frustum.
        let p = m * glam::Vec4::new(0.0, 10.0, 10.0, 1.0);
        assert!((p.y / p.w - 1.0).abs() < 1e-5);
        // ..and the aspect divides x, so x = z lands inside the right edge at 4:3.
        let p = m * glam::Vec4::new(10.0, 0.0, 10.0, 1.0);
        assert!((p.x / p.w - 0.75).abs() < 1e-5, "{}", p.x / p.w);
    }

    // The observed viewpoint update swaps the view matrix's second and third columns.
    // Transform a known world point to check the resulting axes.
    //
    // The client's world is Z-up and the *world* matrix has already applied the y/z swap by the
    // time a vertex reaches the view matrix, so the test swaps its client points the same way.
    #[test]
    fn the_view_matrix_applies_the_z_up_to_d3d_swap() {
        let f = Frame::new(TVec3::ZERO, Quat::IDENTITY);
        let v = view_from_frame(&f);
        let xf = |p: TVec3| -> glam::Vec4 {
            let d = swap_forward_and_up(Vec3::new(p.x, p.y, p.z));
            v * glam::Vec4::new(d.x, d.y, d.z, 1.0)
        };
        // The client's forward axis is +Y; after the swap it must be D3D's +Z (into the screen).
        let fwd = xf(TVec3::new(0.0, 1.0, 0.0));
        assert!(
            (fwd - glam::Vec4::new(0.0, 0.0, 1.0, 1.0)).length() < 1e-6,
            "{fwd:?}"
        );
        // The client's up axis is +Z; after the swap it must be D3D's +Y.
        let up = xf(TVec3::new(0.0, 0.0, 1.0));
        assert!(
            (up - glam::Vec4::new(0.0, 1.0, 0.0, 1.0)).length() < 1e-6,
            "{up:?}"
        );
        // Right stays right.
        let right = xf(TVec3::new(1.0, 0.0, 0.0));
        assert!(
            (right - glam::Vec4::new(1.0, 0.0, 0.0, 1.0)).length() < 1e-6,
            "{right:?}"
        );
    }

    // Oracle: the same transcription, with the viewer moved off the origin so the
    // `l = globaltolocal(frame, (0,0,0))` translation row is exercised rather than zero.
    #[test]
    fn the_view_matrix_puts_the_viewer_at_the_origin() {
        let f = Frame::new(TVec3::new(10.0, 20.0, 3.0), Quat::IDENTITY);
        let v = view_from_frame(&f);
        let xf = |p: TVec3| -> glam::Vec4 {
            let d = swap_forward_and_up(Vec3::new(p.x, p.y, p.z));
            v * glam::Vec4::new(d.x, d.y, d.z, 1.0)
        };
        let here = xf(TVec3::new(10.0, 20.0, 3.0));
        assert!(here.truncate().length() < 1e-4, "{here:?}");
        // 5 m in front of the viewer (client +Y) is 5 m down D3D's +Z.
        let ahead = xf(TVec3::new(10.0, 25.0, 3.0));
        assert!(
            (ahead - glam::Vec4::new(0.0, 0.0, 5.0, 1.0)).length() < 1e-4,
            "{ahead:?}"
        );
        // 2 m above is 2 m up D3D's +Y.
        let above = xf(TVec3::new(10.0, 20.0, 5.0));
        assert!(
            (above - glam::Vec4::new(0.0, 2.0, 0.0, 1.0)).length() < 1e-4,
            "{above:?}"
        );
        // 3 m to the right is 3 m along D3D's +X.
        let beside = xf(TVec3::new(13.0, 20.0, 3.0));
        assert!(
            (beside - glam::Vec4::new(3.0, 0.0, 0.0, 1.0)).length() < 1e-4,
            "{beside:?}"
        );
    }

    // Oracle: the same transcription, driven with a rotation so the off-diagonal entries and the
    // -R^T*origin term are exercised rather than cancelling. Whatever the frame's rotation, the
    // viewer's own local axes must land on D3D's +X / +Y / +Z after the swap.
    #[test]
    fn the_view_matrix_inverts_a_rotated_frame() {
        let half = std::f32::consts::FRAC_PI_4; // a 90 degree yaw about the client's up axis (+Z)
        let q = Quat::new(math::cosf(half), 0.0, 0.0, math::sinf(half));
        let f = Frame::new(TVec3::new(-4.0, 7.0, 1.5), q);
        let v = view_from_frame(&f);
        let m = rotation_matrix(&f);
        // Row j of the local-to-global matrix is the world direction of the frame's local axis j.
        let axis = |j: usize| TVec3::new(m[j][0], m[j][1], m[j][2]);
        let xf = |p: TVec3| -> glam::Vec4 {
            let d = swap_forward_and_up(Vec3::new(p.x, p.y, p.z));
            v * glam::Vec4::new(d.x, d.y, d.z, 1.0)
        };
        let at = |a: TVec3, d: f32| {
            TVec3::new(
                f.origin.x + a.x * d,
                f.origin.y + a.y * d,
                f.origin.z + a.z * d,
            )
        };
        // Local +Y (forward) -> D3D +Z; local +Z (up) -> D3D +Y; local +X (right) -> D3D +X.
        let p = xf(at(axis(1), 7.0));
        assert!(
            (p - glam::Vec4::new(0.0, 0.0, 7.0, 1.0)).length() < 1e-4,
            "forward {p:?}"
        );
        let p = xf(at(axis(2), 2.0));
        assert!(
            (p - glam::Vec4::new(0.0, 2.0, 0.0, 1.0)).length() < 1e-4,
            "up {p:?}"
        );
        let p = xf(at(axis(0), 3.0));
        assert!(
            (p - glam::Vec4::new(3.0, 0.0, 0.0, 1.0)).length() < 1e-4,
            "right {p:?}"
        );
        // ..and the yaw really did rotate the frame: local forward is no longer world +Y.
        assert!(axis(1).y.abs() < 1e-6, "{:?}", axis(1));
    }

    // Check the observed viewport-edge arithmetic. The original edge enum's name
    // remains uncertain; the arithmetic does not depend on that name.
    #[test]
    fn the_game_viewport_subtracts_docked_bars() {
        // No bars: the whole display, inclusive box (right - left + 1).
        assert_eq!(
            compute_game_viewport(800, 600, &[]),
            Viewport {
                x: 0,
                y: 0,
                width: 800,
                height: 600
            }
        );
        // A 60-pixel bar docked to the bottom, at y = 540.
        assert_eq!(
            compute_game_viewport(800, 600, &[(2, 0, 540, 800, 60)]),
            Viewport {
                x: 0,
                y: 0,
                width: 800,
                height: 540
            }
        );
        // A 100-pixel bar docked left plus a 40-pixel bar docked top.
        assert_eq!(
            compute_game_viewport(800, 600, &[(3, 0, 0, 100, 600), (1, 0, 0, 800, 40)]),
            Viewport {
                x: 100,
                y: 40,
                width: 700,
                height: 560
            }
        );
        // Degenerate: bars that cover everything still leave a 2x2 rectangle, because of the
        // `if (right <= left) right = left + 1` guards and the two clamps above them.
        let v = compute_game_viewport(800, 600, &[(3, 0, 0, 5000, 600), (1, 0, 0, 800, 5000)]);
        assert_eq!(
            v,
            Viewport {
                x: 798,
                y: 598,
                width: 2,
                height: 2
            }
        );
    }

    // Check the observed client viewport clamp.
    #[test]
    fn the_viewport_is_clamped_into_the_render_target() {
        let v = Viewport {
            x: 700,
            y: 500,
            width: 400,
            height: 400,
        };
        assert_eq!(
            clamp_viewport(v, 800, 600),
            Viewport {
                x: 700,
                y: 500,
                width: 100,
                height: 100
            }
        );
        let v = Viewport {
            x: 900,
            y: 900,
            width: 10,
            height: 10,
        };
        assert_eq!(
            clamp_viewport(v, 800, 600),
            Viewport {
                x: 799,
                y: 599,
                width: 1,
                height: 1
            }
        );
    }

    // The forward/up swap in the world matrix makes D3D receive (clipX, clipY, 1.0).
    #[test]
    fn the_bare_forward_up_swap_exchanges_y_and_z() {
        assert_eq!(
            swap_forward_and_up(Vec3::new(1.0, 2.0, 3.0)),
            Vec3::new(1.0, 3.0, 2.0)
        );
    }
}
