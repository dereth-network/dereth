//! Camera arithmetic: positions back from depth, and reprojection into the previous frame.

use glam::{Mat4, Vec2, Vec3, Vec4, Vec4Swizzles};

/// The swap between the frame's drawing space, whose second axis is height and third north, and
/// render space, whose third axis is height: its own inverse.
pub const HEIGHT_UP: Mat4 = Mat4::from_cols(Vec4::X, Vec4::Z, Vec4::Y, Vec4::W);

/// Normalised device coordinates and depth to a render-space position, height up, for `view`
/// and `projection` as the frame drew with them.
#[must_use]
pub fn render_from_clip(view: Mat4, projection: Mat4) -> Mat4 {
    HEIGHT_UP * (projection * view).inverse()
}

/// A render-space direction, height up, to view space.
#[must_use]
pub fn view_from_render(view: Mat4) -> Mat4 {
    view * HEIGHT_UP
}

/// The render-space position at normalised device coordinates `ndc` and depth `depth`, through
/// the inverse of the frame's world to clip transform.
#[must_use]
pub fn reconstruct(inverse_view_projection: Mat4, ndc: Vec2, depth: f32) -> Vec3 {
    let p = inverse_view_projection * ndc.extend(depth).extend(1.0);
    p.xyz() / p.w
}

/// Where render-space `position` was on screen in the previous frame, in normalised device
/// coordinates, or `None` when it was behind that camera.
#[must_use]
pub fn reproject(previous_view_projection: Mat4, position: Vec3) -> Option<Vec2> {
    let p = previous_view_projection * position.extend(1.0);
    (p.w > 0.0).then(|| p.xy() / p.w)
}
