//! The handles a selected static is moved and turned by: three arrows along east (x), north (y)
//! and up (z), and a ring about up. Where they are drawn, which one is under the pointer, and
//! what dragging one does.
//!
//! The handles keep their size on screen: an arrow is [`AXIS_SHARE`] of the camera's distance
//! long. A handle within [`GRAB`] pixels of the pointer is the one a press takes. A drag along an
//! arrow moves the static by how far the closest point of the arrow's line to the pointer's ray
//! moved; a drag on the ring turns it by the angle the pointer swept about the static, seen on
//! the level plane through it.

use dereth_primitives::num::math;
use dereth_primitives::Vec3;
use lyceum_foundry::pick::Ray;

use crate::view::View;

/// An arrow's length, as a share of the camera's distance from the static.
pub const AXIS_SHARE: f32 = 0.18;

/// The ring's radius, as a share of an arrow's length.
pub const RING_SHARE: f32 = 0.7;

/// How near the pointer must be to a handle to take it, in pixels.
pub const GRAB: f32 = 8.0;

/// How many points the ring is drawn through.
pub const RING_POINTS: usize = 48;

/// One handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Handle {
    X,
    Y,
    Z,
    Turn,
}

impl Handle {
    /// The arrow's direction, for an arrow.
    #[must_use]
    pub fn axis(self) -> Option<Vec3> {
        match self {
            Self::X => Some(Vec3::new(1.0, 0.0, 0.0)),
            Self::Y => Some(Vec3::new(0.0, 1.0, 0.0)),
            Self::Z => Some(Vec3::new(0.0, 0.0, 1.0)),
            Self::Turn => None,
        }
    }
}

/// The handles of a static at `origin`, as drawn this frame: each arrow from the origin's pixel
/// to its tip's, and the ring's points.
#[derive(Debug, Clone, PartialEq)]
pub struct Handles {
    pub origin: (f32, f32),
    pub arrows: Vec<(Handle, (f32, f32))>,
    pub ring: Vec<(f32, f32)>,
    /// An arrow's length, in meters.
    pub length: f32,
}

fn add(a: Vec3, b: Vec3, s: f32) -> Vec3 {
    Vec3::new(a.x + b.x * s, a.y + b.y * s, a.z + b.z * s)
}

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(a.x - b.x, a.y - b.y, a.z - b.z)
}

/// Where the handles of a static at `origin` (world meters) are drawn; `None` when it is behind
/// the camera.
#[must_use]
pub fn handles(view: &View, eye: Vec3, origin: Vec3) -> Option<Handles> {
    let length = (sub(origin, eye).magnitude() * AXIS_SHARE).max(0.25);
    let o = view.project(origin)?;
    let arrows = [Handle::X, Handle::Y, Handle::Z]
        .into_iter()
        .filter_map(|h| Some((h, view.project(add(origin, h.axis()?, length))?)))
        .collect();
    let r = length * RING_SHARE;
    #[allow(clippy::cast_precision_loss)] // a few dozen points
    let ring = (0..=RING_POINTS)
        .filter_map(|i| {
            let a = i as f32 / RING_POINTS as f32 * std::f32::consts::TAU;
            view.project(Vec3::new(
                origin.x + r * math::cosf(a),
                origin.y + r * math::sinf(a),
                origin.z,
            ))
        })
        .collect();
    Some(Handles {
        origin: o,
        arrows,
        ring,
        length,
    })
}

/// How far point `p` is from the segment `a`-`b`, in pixels.
#[must_use]
pub fn segment_distance(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let (cx, cy) = (a.0 + t * dx, a.1 + t * dy);
    ((p.0 - cx).powi(2) + (p.1 - cy).powi(2)).sqrt()
}

/// The handle under `pointer`: the nearest arrow within [`GRAB`] pixels, else the ring.
#[must_use]
pub fn handle_at(h: &Handles, pointer: (f32, f32)) -> Option<Handle> {
    let arrow = h
        .arrows
        .iter()
        .map(|(k, tip)| (segment_distance(pointer, h.origin, *tip), *k))
        .filter(|(d, _)| *d <= GRAB)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, k)| k);
    arrow.or_else(|| {
        h.ring
            .windows(2)
            .any(|w| segment_distance(pointer, w[0], w[1]) <= GRAB)
            .then_some(Handle::Turn)
    })
}

/// Where along the line `origin + t * axis` the point closest to `ray` is: its `t`. `None` when
/// the ray runs along the line.
#[must_use]
pub fn closest_on_line(origin: Vec3, axis: Vec3, ray: &Ray) -> Option<f32> {
    let w = sub(origin, ray.origin);
    let (a, b, c) = (axis.dot(axis), axis.dot(ray.dir), ray.dir.dot(ray.dir));
    let (d, e) = (axis.dot(w), ray.dir.dot(w));
    let den = a * c - b * b;
    if den.abs() < 1e-6 {
        return None;
    }
    Some((b * e - c * d) / den)
}

/// How far a drag along `handle`'s arrow from the pointer's ray `from` to `to` moves a static at
/// `origin`, in meters along the arrow.
#[must_use]
pub fn drag_along(handle: Handle, origin: Vec3, from: &Ray, to: &Ray) -> Option<f32> {
    let axis = handle.axis()?;
    Some(closest_on_line(origin, axis, to)? - closest_on_line(origin, axis, from)?)
}

/// Where `ray` meets the level plane at height `z`.
#[must_use]
pub fn on_level(ray: &Ray, z: f32) -> Option<Vec3> {
    if ray.dir.z.abs() < 1e-6 {
        return None;
    }
    let t = (z - ray.origin.z) / ray.dir.z;
    (t > 0.0).then(|| add(ray.origin, ray.dir, t))
}

/// The heading (degrees clockwise from north) of `p` seen from `origin` on the level plane.
#[must_use]
pub fn heading_to(origin: Vec3, p: Vec3) -> f32 {
    math::atan2f(p.x - origin.x, p.y - origin.y).to_degrees()
}

/// How far a drag on the ring from the pointer's ray `from` to `to` turns a static at `origin`, in
/// degrees clockwise, between -180 and 180.
#[must_use]
pub fn drag_turn(origin: Vec3, from: &Ray, to: &Ray) -> Option<f32> {
    let a = heading_to(origin, on_level(from, origin.z)?);
    let b = heading_to(origin, on_level(to, origin.z)?);
    let mut d = b - a;
    while d > 180.0 {
        d -= 360.0;
    }
    while d < -180.0 {
        d += 360.0;
    }
    Some(d)
}

/// The heading (degrees clockwise from north) a static's rotation `q` (`w, x, y, z`) faces.
#[must_use]
pub fn heading_of(q: dereth_primitives::Quat) -> f32 {
    let f = dereth_primitives::Frame::new(Vec3::ZERO, q);
    dereth_primitives::frame::get_heading(&f)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the world editor's move and turn handles)
    use super::*;

    fn ray(origin: Vec3, dir: Vec3) -> Ray {
        let l = dir.magnitude();
        Ray {
            origin,
            dir: Vec3::new(dir.x / l, dir.y / l, dir.z / l),
        }
    }

    #[test]
    fn a_handle_is_taken_within_its_grab_and_the_nearest_arrow_wins() {
        let h = Handles {
            origin: (100.0, 100.0),
            arrows: vec![
                (Handle::X, (200.0, 100.0)),
                (Handle::Y, (100.0, 0.0)),
                (Handle::Z, (100.0, 0.0)),
            ],
            ring: vec![(170.0, 170.0), (180.0, 160.0)],
            length: 5.0,
        };
        assert_eq!(handle_at(&h, (150.0, 104.0)), Some(Handle::X));
        assert_eq!(handle_at(&h, (150.0, 110.0)), None);
        assert_eq!(handle_at(&h, (175.0, 166.0)), Some(Handle::Turn));
    }

    #[test]
    fn a_drag_along_an_arrow_moves_by_how_far_the_pointers_closest_point_moved() {
        // Looking down at a static at the origin from 10 m up, two pointer rays 3 m apart east.
        let origin = Vec3::new(0.0, 0.0, 0.0);
        let from = ray(Vec3::new(1.0, 0.0, 10.0), Vec3::new(0.0, 0.1, -1.0));
        let to = ray(Vec3::new(4.0, 0.0, 10.0), Vec3::new(0.0, 0.1, -1.0));
        let d = drag_along(Handle::X, origin, &from, &to).unwrap();
        assert!((d - 3.0).abs() < 1e-4, "{d}");
        // Along the north arrow the same drag moves nothing.
        let n = drag_along(Handle::Y, origin, &from, &to).unwrap();
        assert!(n.abs() < 1e-4, "{n}");
    }

    #[test]
    fn a_drag_on_the_ring_turns_by_the_angle_swept_on_the_level() {
        let origin = Vec3::new(0.0, 0.0, 5.0);
        let down = Vec3::new(0.0, 0.0, -1.0);
        // From north of it to east of it: a quarter turn clockwise.
        let from = ray(Vec3::new(0.0, 3.0, 20.0), down);
        let to = ray(Vec3::new(3.0, 0.0, 20.0), down);
        let d = drag_turn(origin, &from, &to).unwrap();
        assert!((d - 90.0).abs() < 1e-3, "{d}");
        // And back across south-west, the short way round.
        let d = drag_turn(origin, &to, &ray(Vec3::new(-3.0, 0.1, 20.0), down)).unwrap();
        assert!((d - -178.09).abs() < 0.05, "{d}");
    }

    #[test]
    fn a_heading_reads_back_from_the_rotation_it_makes() {
        for h in [0.0f32, 30.0, 90.0, 200.0, 330.0] {
            let q = lyceum_foundry::compile::heading_quat(h);
            let back = heading_of(q);
            assert!(
                ((back - h + 540.0) % 360.0 - 180.0).abs() < 0.01,
                "{h}: {back}"
            );
        }
    }
}
