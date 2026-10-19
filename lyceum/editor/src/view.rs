//! The projection the scene draws with: a world point to its pixel, and a pixel to its ray.
//!
//! The scene draws in a space whose origin is the south-west corner of the block its window is
//! centered on, with the device's axes (up second); [`View`] takes world meters on both sides, so
//! a gizmo drawn from it lies where the world is drawn and a pick from it meets what is under the
//! pointer.

use dereth_client_contract::camera::ViewParams;
use dereth_primitives::{Vec3, Viewport};
use glam::{Mat4, Vec4};
use lyceum_foundry::pick::Ray;

use crate::camera::block_meters;

/// One frame's projection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    view_proj: Mat4,
    inverse: Mat4,
    viewport: Viewport,
    block: (i32, i32),
}

impl View {
    /// The projection of `params` (the scene's, this frame), whose space is relative to the block
    /// `block`.
    #[must_use]
    pub fn new(params: &ViewParams, block: (i32, i32)) -> Self {
        let view_proj = dereth_render::camera::projection(params) * params.view;
        Self {
            view_proj,
            inverse: view_proj.inverse(),
            viewport: params.viewport,
            block,
        }
    }

    /// The device-space point of world meters `p`.
    fn device(&self, p: Vec3) -> Vec4 {
        let (ox, oy) = block_meters(self.block);
        Vec4::new(p.x - ox, p.z, p.y - oy, 1.0)
    }

    /// The pixel `p` (world meters) is drawn at, and its depth from 0 (near) to 1 (far); `None`
    /// behind the camera.
    #[must_use]
    pub fn project_depth(&self, p: Vec3) -> Option<(f32, f32, f32)> {
        let c = self.view_proj * self.device(p);
        if c.w <= 1e-4 {
            return None;
        }
        let n = c / c.w;
        #[allow(clippy::cast_precision_loss)] // a viewport, at most a few thousand pixels
        let (x, y, w, h) = (
            self.viewport.x as f32,
            self.viewport.y as f32,
            self.viewport.width as f32,
            self.viewport.height as f32,
        );
        Some((x + (n.x + 1.0) * 0.5 * w, y + (1.0 - n.y) * 0.5 * h, n.z))
    }

    /// The pixel `p` (world meters) is drawn at; `None` behind the camera.
    #[must_use]
    pub fn project(&self, p: Vec3) -> Option<(f32, f32)> {
        self.project_depth(p).map(|(x, y, _)| (x, y))
    }

    /// The ray through pixel `(px, py)`, in world meters.
    #[must_use]
    pub fn ray(&self, px: f32, py: f32) -> Ray {
        #[allow(clippy::cast_precision_loss)] // a viewport, at most a few thousand pixels
        let (x, y, w, h) = (
            self.viewport.x as f32,
            self.viewport.y as f32,
            (self.viewport.width as f32).max(1.0),
            (self.viewport.height as f32).max(1.0),
        );
        let nx = (px - x) / w * 2.0 - 1.0;
        let ny = 1.0 - (py - y) / h * 2.0;
        let at = |z: f32| {
            let p = self.inverse * Vec4::new(nx, ny, z, 1.0);
            p / p.w
        };
        let (near, far) = (at(0.0), at(1.0));
        let (ox, oy) = block_meters(self.block);
        let origin = Vec3::new(near.x + ox, near.z + oy, near.y);
        let d = Vec3::new(far.x - near.x, far.z - near.z, far.y - near.y);
        let len = d.magnitude().max(1e-9);
        Ray {
            origin,
            dir: Vec3::new(d.x / len, d.y / len, d.z / len),
        }
    }

    /// The viewport this projection fills.
    #[must_use]
    pub fn viewport(&self) -> Viewport {
        self.viewport
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the world editor's projection)
    use super::*;
    use dereth_client_runtime::camera::FreeCamera;

    /// A camera at (100, 50, 30) of block (2, 3), looking north and a little down.
    pub(crate) fn view() -> View {
        let cam = FreeCamera::new(Vec3::new(100.0, 50.0, 30.0), 0.0, -0.3);
        let params = ViewParams {
            view: dereth_render::camera::view_from_frame(&cam.frame()),
            viewport: Viewport {
                x: 0,
                y: 0,
                width: 800,
                height: 600,
            },
            ..ViewParams::default()
        };
        View::new(&params, (2, 3))
    }

    #[test]
    fn a_pixels_ray_passes_through_the_point_drawn_there() {
        let v = view();
        let p = Vec3::new(2.0 * 192.0 + 110.0, 3.0 * 192.0 + 80.0, 20.0);
        let (x, y) = v.project(p).expect("in front of the camera");
        let r = v.ray(x, y);
        // The point is on the ray: its distance from it is nothing.
        let to = Vec3::new(p.x - r.origin.x, p.y - r.origin.y, p.z - r.origin.z);
        let along = to.dot(r.dir);
        let off = Vec3::new(
            to.x - r.dir.x * along,
            to.y - r.dir.y * along,
            to.z - r.dir.z * along,
        );
        assert!(off.magnitude() < 0.01, "{off:?}");
        assert!(along > 0.0);
    }

    #[test]
    fn the_point_ahead_is_drawn_in_the_middle_and_one_behind_is_not_drawn() {
        let v = view();
        let cam = FreeCamera::new(Vec3::new(100.0, 50.0, 30.0), 0.0, -0.3);
        let f = cam.forward();
        let ahead = Vec3::new(
            2.0 * 192.0 + 100.0 + f.x * 40.0,
            3.0 * 192.0 + 50.0 + f.y * 40.0,
            30.0 + f.z * 40.0,
        );
        let (x, y) = v.project(ahead).unwrap();
        assert!(
            (x - 400.0).abs() < 0.5 && (y - 300.0).abs() < 0.5,
            "{x} {y}"
        );
        let behind = Vec3::new(2.0 * 192.0 + 100.0, 3.0 * 192.0 + 10.0, 30.0);
        assert!(v.project(behind).is_none());
    }
}
