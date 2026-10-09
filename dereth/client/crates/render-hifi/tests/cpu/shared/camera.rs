//! Camera arithmetic: positions back from depth, and reprojection.

use dereth_render_hifi::shared::camera::{
    reconstruct, render_from_clip, reproject, view_from_render, HEIGHT_UP,
};
use glam::{Mat4, Vec2, Vec3, Vec4Swizzles};

/// A left-handed perspective camera at `eye` looking along +y with z up, as the frame's camera
/// is built: world to clip.
fn camera(eye: Vec3) -> Mat4 {
    let view = Mat4::look_to_lh(eye, Vec3::Y, Vec3::Z);
    Mat4::perspective_lh(1.0, 4.0 / 3.0, 0.5, 2400.0) * view
}

/// Behaviour: hifi.camera.positions-come-back-from-depth
#[test]
fn a_point_comes_back_from_its_depth_and_reprojects_into_the_previous_frame() {
    let now = camera(Vec3::new(10.0, 0.0, 2.0));
    let before = camera(Vec3::new(9.0, -1.0, 2.0));
    for p in [
        Vec3::new(10.0, 30.0, 2.0),
        Vec3::new(4.0, 200.0, 40.0),
        Vec3::new(25.0, 7.0, -3.0),
    ] {
        let clip = now * p.extend(1.0);
        let ndc = clip.xyz() / clip.w;
        let back = reconstruct(now.inverse(), Vec2::new(ndc.x, ndc.y), ndc.z);
        assert!(
            back.distance(p) < 1e-2 * p.distance(Vec3::new(10.0, 0.0, 2.0)).max(1.0),
            "{p} came back as {back}"
        );
        let then = reproject(before, back).expect("in front of the previous camera");
        let want = before * p.extend(1.0);
        assert!(
            then.distance(want.xy() / want.w) < 1e-3,
            "{then} against {want}"
        );
    }
    // Behind the previous camera there is nothing to reproject to.
    assert!(reproject(before, Vec3::new(9.0, -50.0, 2.0)).is_none());

    // The frame draws in a space with height on its second axis: a point of render space, height
    // up, comes back from its depth there, and render-space up is the view's up.
    let eye = Vec3::new(10.0, 0.0, 2.0);
    let view = Mat4::look_to_lh(HEIGHT_UP.transform_point3(eye), Vec3::Z, Vec3::Y);
    let projection = Mat4::perspective_lh(1.0, 4.0 / 3.0, 0.5, 2400.0);
    for p in [Vec3::new(10.0, 30.0, 2.0), Vec3::new(4.0, 200.0, 40.0)] {
        let clip = projection * view * HEIGHT_UP * p.extend(1.0);
        let ndc = clip.xyz() / clip.w;
        let back = reconstruct(
            render_from_clip(view, projection),
            Vec2::new(ndc.x, ndc.y),
            ndc.z,
        );
        assert!(
            back.distance(p) < 1e-2 * p.distance(eye).max(1.0),
            "{p} came back as {back} through the drawing space"
        );
    }
    let up = view_from_render(view).transform_vector3(Vec3::Z);
    assert!(
        up.distance(Vec3::Y) < 1e-5,
        "render-space up is {up} in view"
    );
}
