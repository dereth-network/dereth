//! The procedural turns: the head turned toward where the body looks, and the body leaned to the
//! slope it stands on.
//!
//! Both are animation nodes that play no keys: they turn bones of the pose built so far in place,
//! each bone picked by its type in the setup. They sit on the layers that add straight into the
//! pose, so they turn what the layers under them made.
//!
//! - The **look** reads the three look sliders as a direction (scaled by the node's weight, cut
//!   back to unit length when longer) and turns the head bones by 0.35 of it and the neck bones by
//!   a smaller share (0.153 across, 0.2325 along the other two axes). A turn whose result is not
//!   of unit length is dropped.
//! - The **angle** (the slope lean) steers two lean sliders toward the ground's slope under the
//!   body (the plane's normal in the body's own frame, each component held within the node's
//!   limits; nothing to lean to when the body is in the air or sliding down), at once when they are
//!   close and over a quarter second when they are more than 0.15 away; then it turns each listed
//!   bone by the sliders' current values times its shares.
//!
//! The arithmetic follows the client's: products and sums kept in double precision and stored as
//! single, as the client's floating point unit does.

use auberean_primitives::{Quat, Vec3};
use dereth_primitives::num::math;

use crate::clip::{Pose, Sum};
use crate::sliders::Sliders;

/// The look sliders: the direction the body looks, in its own frame.
pub mod look_slider {
    pub const X: u32 = 0x4000_0007;
    pub const Y: u32 = 0x4000_0008;
    pub const Z: u32 = 0x4000_0009;
}

/// The lean sliders: the turn about the body's x axis (toward the slope ahead) and about its y
/// axis (toward the slope to the side).
pub mod lean_slider {
    pub const X: u32 = 0x4000_001B;
    pub const Y: u32 = 0x4000_001C;
}

/// The bone types the look turns by its full share (the head) and by its smaller one (the neck).
pub const LOOK_FULL: [u32; 2] = [0x4000_0003, 0x4000_0005];
pub const LOOK_PARTIAL: [u32; 2] = [0x4000_0004, 0x4000_0006];

/// How far a lean slider may be from the slope before it moves over time rather than at once.
const LEAN_SNAP: f64 = 0.15f32 as f64;
/// The time a lean slider takes to catch up when it is far behind.
const LEAN_TIME: f32 = 0.25;
/// A rotation turned shorter than this is none.
const MIN_TURN: f32 = 0.0002;
/// A quaternion within this of unit length (squared) counts as one.
const UNIT_SLACK: f32 = 0.0002;

/// Where the body stands, as the slope lean reads it. The body's owner sets it each frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Ground {
    /// The body touches the ground.
    pub contact: bool,
    /// The body slides down a slope too steep to stand on.
    pub sliding: bool,
    /// The normal of the plane the body stands on, in the body's own frame.
    pub normal: Vec3,
    /// The body is placed in the world (a cell that exists, a frame that is a number): only then
    /// does the lean steer its sliders.
    pub placed: bool,
}

fn f(x: f32) -> f64 {
    f64::from(x)
}

#[allow(clippy::cast_possible_truncation)]
fn s(x: f64) -> f32 {
    x as f32
}

/// The product `a` then `b` of two rotations (`a` applied after `b`).
// D2-05827
#[must_use]
pub fn multiply(a: Quat, b: Quat) -> Quat {
    Quat {
        w: s(((f(a.w) * f(b.w) - f(b.x) * f(a.x)) - f(b.y) * f(a.y)) - f(b.z) * f(a.z)),
        x: s(((f(a.x) * f(b.w) + f(b.x) * f(a.w)) + f(b.z) * f(a.y)) - f(b.y) * f(a.z)),
        y: s(((f(b.y) * f(a.w) - f(b.z) * f(a.x)) + f(a.y) * f(b.w)) + f(b.x) * f(a.z)),
        z: s(((f(b.z) * f(a.w) + f(b.y) * f(a.x)) - f(b.x) * f(a.y)) + f(a.z) * f(b.w)),
    }
}

/// Turn `q` by the rotation vector `v` (its direction the axis, its length the angle in radians),
/// the turn applied after `q`. A vector shorter than 0.0002 turns nothing and answers false.
// D2-13249
pub fn rotate(q: &mut Quat, v: Vec3) -> bool {
    let len2 = (f(v.x) * f(v.x) + f(v.y) * f(v.y)) + f(v.z) * f(v.z);
    if len2 < f(MIN_TURN) * f(MIN_TURN) {
        return false;
    }
    let len = len2.sqrt();
    let inv = 1.0 / len;
    let half = len * 0.5;
    let sin = s(math::sin(half));
    let r = Quat {
        w: s(math::cos(half)),
        x: s(f(sin) * f(v.x) * inv),
        y: s(f(sin) * inv * f(v.y)),
        z: s(f(sin) * inv * f(v.z)),
    };
    *q = multiply(r, *q);
    true
}

/// A quaternion's squared length as the client stores it to compare with one.
fn stored_norm2(q: Quat) -> f32 {
    s(((f(q.z) * f(q.z) + f(q.y) * f(q.y)) + f(q.x) * f(q.x)) + f(q.w) * f(q.w))
}

/// Whether `q` is a rotation: numbers, and of unit length within the slack.
fn is_unit(q: Quat) -> bool {
    let n = stored_norm2(q);
    !(q.w.is_nan() || q.x.is_nan() || q.y.is_nan() || q.z.is_nan())
        && !n.is_nan()
        && (f(n) - 1.0).abs() < f(UNIT_SLACK)
}

/// Bring a rotation to unit length whatever it holds; one too short (squared length under 0.0002)
/// becomes no rotation.
// D2-13265
pub fn normalize_even_invalid(q: &mut Quat) {
    let n = ((f(q.w) * f(q.w) + f(q.x) * f(q.x)) + f(q.y) * f(q.y)) + f(q.z) * f(q.z);
    if n < f(UNIT_SLACK) {
        *q = Quat::IDENTITY;
        return;
    }
    let k = 1.0 / n.sqrt();
    *q = Quat {
        w: s(k * f(q.w)),
        x: s(k * f(q.x)),
        y: s(k * f(q.y)),
        z: s(k * f(q.z)),
    };
}

/// Bring a bone's rotation to unit length when the bone is not a valid transform (an offset or a
/// scale that is not a number, or a rotation that is not one); a valid bone is left as it is.
// D2-13270
pub fn normalize_if_invalid(b: &mut Sum) {
    let numbers = [b.offset, b.scale]
        .iter()
        .all(|v| !(v.x.is_nan() || v.y.is_nan() || v.z.is_nan()));
    if numbers && is_unit(b.rotation) {
        return;
    }
    normalize_even_invalid(&mut b.rotation);
}

/// A squared length under this (2^-46) leaves no direction to scale.
const TINY: f32 = f32::from_bits(0x2880_0000);

/// A vector scaled to unit length; one whose squared length is under 2^-46 becomes zero.
fn unit(v: Vec3) -> Vec3 {
    let n = (v.x * v.x + v.y * v.y) + v.z * v.z;
    if TINY > n {
        return Vec3::ZERO;
    }
    let r = 1.0 / n.sqrt();
    let k = (0.5 * r) * (3.0 - (n * r) * r);
    Vec3::new(k * v.x, k * v.y, k * v.z)
}

/// The look: turn the head and neck bones of `out` toward the look sliders' direction, scaled by
/// `weight`. `bone_types` gives each bone's type.
// D2-15337
pub fn look(weight: f32, sliders: &Sliders, bone_types: &[u32], out: &mut Pose) {
    let (x, y) = (
        sliders.current(look_slider::X) * weight,
        sliders.current(look_slider::Y) * weight,
    );
    let z_exact = f(sliders.current(look_slider::Z)) * f(weight);
    let z = s(z_exact);
    let mut d = Vec3::new(x, y, z);
    if (z_exact * f(z) + f(y) * f(y)) + f(x) * f(x) > 1.0 {
        d = unit(d);
    }
    let full = Vec3::new(d.x * 0.35, (d.y * 0.25) * 1.4, (d.z * 0.25) * 1.4);
    let partial = Vec3::new(d.x * 0.153_450_01, (d.y * 0.25) * 0.93, (d.z * 0.25) * 0.93);
    if full.x.is_nan() || full.y.is_nan() || full.z.is_nan() {
        return;
    }
    // D2-12874
    let ty = |i: usize| bone_types.get(i).copied().unwrap_or(0);
    for (i, bone) in out.bones.iter_mut().enumerate() {
        let by = if LOOK_FULL.contains(&ty(i)) {
            full
        } else if LOOK_PARTIAL.contains(&ty(i)) {
            partial
        } else {
            continue;
        };
        normalize_if_invalid(bone);
        let mut q = bone.rotation;
        rotate(&mut q, by);
        if is_unit(q) {
            bone.rotation = q;
        }
    }
}

/// A slope-lean node: the bones it turns (each a bone type with its share of the turn about x and
/// about y) and the limits of the slope it follows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lean<'a> {
    pub bones: &'a [(u32, f32, f32)],
    pub max_x: f32,
    pub min_x: f32,
    pub max_y: f32,
    pub min_y: f32,
}

/// Where the slope lean steers its sliders this frame, from the ground and the sliders' current
/// values (`x`, `y`): for each lean slider, the value it is asked for and over how long.
#[must_use]
pub fn lean_steering(
    node: &Lean<'_>,
    ground: &Ground,
    current: (f32, f32),
) -> [(u32, f32, f32); 2] {
    let (mut ahead, mut side) = (0.0f32, 0.0f32);
    if ground.contact && !ground.sliding {
        let n = ground.normal;
        ahead = s((f(n.x) * 0.0 + f(n.z) * 0.0) + f(n.y));
        side = s((f(n.y) * 0.0 + f(n.z) * 0.0) + f(n.x));
        if ahead > node.max_y {
            ahead = node.max_y;
        }
        if ahead < node.min_y {
            ahead = node.min_y;
        }
        if side > node.max_x {
            side = node.max_x;
        }
        if side < node.min_x {
            side = node.min_x;
        }
    }
    let time = |cur: f32, want: f32| {
        if (f(cur) - f(want)).abs() > LEAN_SNAP {
            LEAN_TIME
        } else {
            0.0
        }
    };
    [
        (lean_slider::X, -ahead, time(current.0, -ahead)),
        (lean_slider::Y, side, time(current.1, side)),
    ]
}

/// The slope lean: steer the lean sliders toward the ground's slope (when the body is placed),
/// then turn the node's bones of `out` by the sliders' values as they were before this frame's
/// steering, times `weight` and each bone's shares.
// D2-15343
pub fn angle(
    node: &Lean<'_>,
    weight: f32,
    ground: &Ground,
    sliders: &mut Sliders,
    bone_types: &[u32],
    out: &mut Pose,
) {
    let cur_x = sliders.current(lean_slider::X);
    let cur_y = sliders.current(lean_slider::Y);
    if ground.placed {
        for (slider, value, time) in lean_steering(node, ground, (cur_x, cur_y)) {
            sliders.do_slider(0, slider, value, time);
        }
    }
    if cur_x.is_nan() || cur_y.is_nan() {
        return;
    }
    for (i, bone) in out.bones.iter_mut().enumerate() {
        let ty = bone_types.get(i).copied().unwrap_or(0);
        for &(kind, share_x, share_y) in node.bones {
            if kind != ty {
                continue;
            }
            let v = Vec3::new(
                (cur_x * weight) * share_x,
                (cur_y * weight) * share_y,
                weight * 0.0,
            );
            if v.x.is_nan() || v.y.is_nan() || v.z.is_nan() {
                continue;
            }
            rotate(&mut bone.rotation, v);
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (quaternion arithmetic checked against its definition)
    use super::*;

    #[test]
    fn a_turn_composes_after_the_rotation_it_turns() {
        // A quarter turn about z, then a quarter turn about x: the product is the second times
        // the first.
        let h = std::f32::consts::FRAC_1_SQRT_2;
        let about_z = Quat {
            w: h,
            x: 0.0,
            y: 0.0,
            z: h,
        };
        let mut q = about_z;
        assert!(rotate(
            &mut q,
            Vec3::new(std::f32::consts::FRAC_PI_2, 0.0, 0.0)
        ));
        let want = Quat {
            w: 0.5,
            x: 0.5,
            y: -0.5,
            z: 0.5,
        };
        for (a, b) in [(q.w, want.w), (q.x, want.x), (q.y, want.y), (q.z, want.z)] {
            assert!((a - b).abs() < 1e-6, "{q:?}");
        }
        let mut still = about_z;
        assert!(!rotate(&mut still, Vec3::new(0.0001, 0.0, 0.0)));
        assert_eq!(still, about_z, "too short a turn turns nothing");
    }
}
