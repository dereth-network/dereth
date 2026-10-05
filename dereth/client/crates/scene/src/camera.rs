//! Camera comparisons against the renderer's view matrix.

#[cfg(test)]
use dereth_client_runtime::camera::*;

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_primitives::Vec3;
    use dereth_render::camera::{swap_forward_and_up, view_from_frame};

    fn view(cam: &FreeCamera, p: Vec3) -> glam::Vec4 {
        let v = view_from_frame(&cam.frame());
        let d = swap_forward_and_up(glam::Vec3::new(p.x, p.y, p.z));
        v * glam::Vec4::new(d.x, d.y, d.z, 1.0)
    }

    /// An unrotated camera faces north and north is into the screen.
    #[test]
    fn an_unrotated_camera_faces_north_and_north_is_into_the_screen() {
        let cam = FreeCamera::new(Vec3::new(100.0, 50.0, 30.0), 0.0, 0.0);
        let here = view(&cam, cam.position);
        assert!(here.truncate().length() < 1e-4, "{here:?}");
        // The client's +y is north; an unrotated camera looks along it.
        let ahead = view(&cam, Vec3::new(100.0, 60.0, 30.0));
        assert!(
            (ahead - glam::Vec4::new(0.0, 0.0, 10.0, 1.0)).length() < 1e-3,
            "{ahead:?}"
        );
        let above = view(&cam, Vec3::new(100.0, 50.0, 32.0));
        assert!(
            (above - glam::Vec4::new(0.0, 2.0, 0.0, 1.0)).length() < 1e-3,
            "{above:?}"
        );
        let beside = view(&cam, Vec3::new(103.0, 50.0, 30.0));
        assert!(
            (beside - glam::Vec4::new(3.0, 0.0, 0.0, 1.0)).length() < 1e-3,
            "{beside:?}"
        );
    }

    // Oracle: as above, with the camera yawed and pitched so the off-diagonal terms and the
    // -R^T*origin translation are exercised rather than cancelling. Whatever the angles, whatever
    // the camera's own forward vector says is ahead must land on D3D's +Z.
    #[test]
    fn a_yawed_and_pitched_camera_still_looks_down_positive_z() {
        let cam = FreeCamera::new(Vec3::new(-40.0, 17.0, 9.0), 0.9, -0.35);
        let f = cam.forward();
        let ahead = Vec3::new(
            cam.position.x + f.x * 12.0,
            cam.position.y + f.y * 12.0,
            cam.position.z + f.z * 12.0,
        );
        let p = view(&cam, ahead);
        assert!(
            (p - glam::Vec4::new(0.0, 0.0, 12.0, 1.0)).length() < 1e-3,
            "{p:?}"
        );
        let r = cam.right();
        let beside = Vec3::new(
            cam.position.x + r.x * 5.0,
            cam.position.y + r.y * 5.0,
            cam.position.z + r.z * 5.0,
        );
        let p = view(&cam, beside);
        assert!(
            (p - glam::Vec4::new(5.0, 0.0, 0.0, 1.0)).length() < 1e-3,
            "{p:?}"
        );
        // A negative pitch looks down, so ground level is above the view centre.
        assert!(f.z < 0.0, "{f:?}");
    }
}
