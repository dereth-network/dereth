//! The handles a selected row is moved and turned by, as Lyceum has them: three arrows along east
//! (x), north (y) and up (z), and a ring about up. Where they are drawn, which one is under the
//! pointer, what dragging one does, and the steps a move or a turn snaps to.
//!
//! The handles keep their size on screen: an arrow is [`AXIS_SHARE`] of the camera's distance long.
//! A handle within [`GRAB`] pixels of the pointer is the one a press takes. A drag along an arrow
//! moves the row by how far the closest point of the arrow's line to the pointer's ray moved; a drag
//! on the ring turns it by the angle the pointer swept about the row, on the level plane through it.

use dereth_primitives::num::math;

use crate::view::View;

/// An arrow's length, as a share of the camera's distance from the row.
pub const AXIS_SHARE: f32 = 0.18;
/// The ring's radius, as a share of an arrow's length.
pub const RING_SHARE: f32 = 0.7;
/// How near the pointer must be to a handle to take it, in pixels.
pub const GRAB: f32 = 8.0;
/// How many points the ring is drawn through.
pub const RING_POINTS: usize = 48;

/// One handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handle {
    X,
    Y,
    Z,
    Turn,
}

impl Handle {
    /// The arrow's direction, for an arrow.
    #[must_use]
    pub const fn axis(self) -> Option<[f32; 3]> {
        match self {
            Self::X => Some([1.0, 0.0, 0.0]),
            Self::Y => Some([0.0, 1.0, 0.0]),
            Self::Z => Some([0.0, 0.0, 1.0]),
            Self::Turn => None,
        }
    }
}

/// A line of sight: from a point, along a direction of length one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ray {
    pub origin: [f32; 3],
    pub dir: [f32; 3],
}

/// The handles of a row, as drawn this frame: each arrow from the row's pixel to its tip's, and the
/// ring's points.
#[derive(Debug, Clone, PartialEq)]
pub struct Handles {
    pub origin: [f32; 2],
    pub arrows: Vec<(Handle, [f32; 2])>,
    pub ring: Vec<[f32; 2]>,
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Where the handles of a row at `origin` are drawn, with its ring if it can turn; `None` when it is
/// behind the camera.
#[must_use]
pub fn handles(view: &View, origin: [f32; 3], turns: bool) -> Option<Handles> {
    let d = [
        origin[0] - view.eye[0],
        origin[1] - view.eye[1],
        origin[2] - view.eye[2],
    ];
    let length = (dot(d, d).sqrt() * AXIS_SHARE).max(0.25);
    let o = view.project(origin)?;
    let tip = |a: [f32; 3]| {
        [
            origin[0] + a[0] * length,
            origin[1] + a[1] * length,
            origin[2] + a[2] * length,
        ]
    };
    let arrows = [Handle::X, Handle::Y, Handle::Z]
        .into_iter()
        .filter_map(|h| Some((h, view.project(tip(h.axis()?))?)))
        .collect();
    let r = length * RING_SHARE;
    #[allow(clippy::cast_precision_loss)] // a few dozen points
    let ring = if turns {
        (0..=RING_POINTS)
            .filter_map(|i| {
                let a = i as f32 / RING_POINTS as f32 * std::f32::consts::TAU;
                view.project([
                    origin[0] + r * math::cosf(a),
                    origin[1] + r * math::sinf(a),
                    origin[2],
                ])
            })
            .collect()
    } else {
        Vec::new()
    };
    Some(Handles {
        origin: o,
        arrows,
        ring,
    })
}

/// How far point `p` is from the segment `a`-`b`, in pixels.
#[must_use]
pub fn segment_distance(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 {
        (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    math::hypotf(p[0] - (a[0] + t * dx), p[1] - (a[1] + t * dy))
}

/// The handle under `pointer`: the nearest arrow within [`GRAB`] pixels, else the ring.
#[must_use]
pub fn handle_at(h: &Handles, pointer: [f32; 2]) -> Option<Handle> {
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

/// Where along the line `origin + t * axis` the point closest to `ray` is: its `t`. `None` when the
/// ray runs along the line.
#[must_use]
pub fn closest_on_line(origin: [f32; 3], axis: [f32; 3], ray: &Ray) -> Option<f32> {
    let w = [
        origin[0] - ray.origin[0],
        origin[1] - ray.origin[1],
        origin[2] - ray.origin[2],
    ];
    let (a, b, c) = (dot(axis, axis), dot(axis, ray.dir), dot(ray.dir, ray.dir));
    let (d, e) = (dot(axis, w), dot(ray.dir, w));
    let den = a * c - b * b;
    if den.abs() < 1e-6 {
        return None;
    }
    Some((b * e - c * d) / den)
}

/// How far a drag along `handle`'s arrow from the pointer's ray `from` to `to` moves a row at
/// `origin`, in meters along the arrow.
#[must_use]
pub fn drag_along(handle: Handle, origin: [f32; 3], from: &Ray, to: &Ray) -> Option<f32> {
    let axis = handle.axis()?;
    Some(closest_on_line(origin, axis, to)? - closest_on_line(origin, axis, from)?)
}

/// Where `ray` meets the level plane at height `z`.
#[must_use]
pub fn on_level(ray: &Ray, z: f32) -> Option<[f32; 2]> {
    if ray.dir[2].abs() < 1e-6 {
        return None;
    }
    let t = (z - ray.origin[2]) / ray.dir[2];
    (t > 0.0).then(|| {
        [
            ray.origin[0] + ray.dir[0] * t,
            ray.origin[1] + ray.dir[1] * t,
        ]
    })
}

/// How far a drag on the ring from the pointer's ray `from` to `to` turns a row at `origin`, in
/// degrees clockwise, between -180 and 180.
#[must_use]
pub fn drag_turn(origin: [f32; 3], from: &Ray, to: &Ray) -> Option<f32> {
    let heading = |p: [f32; 2]| math::atan2f(p[0] - origin[0], p[1] - origin[1]).to_degrees();
    let a = heading(on_level(from, origin[2])?);
    let b = heading(on_level(to, origin[2])?);
    Some((b - a + 540.0).rem_euclid(360.0) - 180.0)
}

/// The steps a move and a turn snap to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Snap {
    pub on: bool,
    /// Meters.
    pub grid: f32,
    /// Degrees.
    pub angle: f32,
}

impl Default for Snap {
    fn default() -> Self {
        Self {
            on: false,
            grid: 1.0,
            angle: 15.0,
        }
    }
}

/// The nearest whole number of `step` to `v`.
#[must_use]
pub fn snapped(v: f32, step: f32) -> f32 {
    if step > 0.0 {
        (v / step).round() * step
    } else {
        v
    }
}

impl Snap {
    /// Whether a drag snaps, Shift held doing the other of what the switch says.
    #[must_use]
    pub const fn applies(&self, shift: bool) -> bool {
        self.on != shift
    }

    /// The step R turns a row by.
    #[must_use]
    pub const fn turn_step(&self, plain: f32) -> f32 {
        if self.on {
            self.angle
        } else {
            plain
        }
    }
}
