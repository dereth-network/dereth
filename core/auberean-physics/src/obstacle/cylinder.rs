//! A moving body meeting another object's upright cylinder.
//!
//! Every test works in the mover's check position's landblock frame; the cylinder is already there.
//! `sum` is the cylinder's radius less 0.0002 plus the mover's first sphere's radius, the overlap
//! form every arm is handed.

// A negated comparison is the client's own: it passes a number that is not one where the plain
// comparison would refuse it.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use dereth_physics::geom::{CylSphere, Sphere};
use dereth_physics::globals::LANDING_Z;
use dereth_physics::transition::{InsertType, ObjectInfoState, Transition};
use dereth_physics::TransitionState;
use dereth_primitives::shape::Plane;
use dereth_primitives::{Position, Vec3};

use super::{n, plane_through, set_collision_normal, set_contact_plane, sphere, w, StepUp};
use crate::geom::{local_to_global, point_to_global_from, EPS};

/// The lowest walk interpolation a step down may leave.
const FLOOR_STEP: f64 = -0.1;

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(a.x - b.x, a.y - b.y, a.z - b.z)
}

/// Whether the mover's sphere `s`, `d` from the cylinder's low point, overlaps it: within `sum`
/// across the ground, and within the sphere's radius (less 0.0002) of the cylinder's middle height
/// past half its height.
// D2-04224
#[must_use]
pub fn overlaps(c: &CylSphere, s: &Sphere, d: Vec3, sum: f32) -> bool {
    if !(w(sum) * w(sum) >= w(d.x) * w(d.x) + w(d.y) * w(d.y)) {
        return false;
    }
    let half = w(c.height) * 0.5;
    (w(s.radius) - w(EPS)) + half >= (half - w(d.z)).abs()
}

/// The normal of the mover's collision with the cylinder, from the mover's sphere `index` where it
/// began the step (`curr`): straight up or down (whichever side of the sphere's own center the
/// sphere `s`, `d` from the low point, is toward) when that point is within `sum` of the axis; else
/// across the ground from the axis, with whether the mover is beside the cylinder rather than
/// clear of its top or bottom (beside it when within its height, by the sphere's radius less
/// 0.0002, or level with `s` within 0.0002). The normal is not unit length.
// D2-04225
#[must_use]
pub fn normal_of_collision(
    c: &CylSphere,
    curr: Vec3,
    s: &Sphere,
    d: Vec3,
    sum: f32,
) -> (bool, Vec3) {
    let dx = w(curr.x) - w(c.low_pt.x);
    let dy = w(curr.y) - w(c.low_pt.y);
    let dz = n(w(curr.z) - w(c.low_pt.z));
    if w(sum) * w(sum) >= dy * dy + dx * dx {
        let v = w(d.z) - w(dz);
        let z = if v > 0.0 || v.is_nan() { -1.0 } else { 1.0 };
        return (true, Vec3::new(0.0, 0.0, z));
    }
    let nm = Vec3::new(n(dx), n(dy), 0.0);
    let half = w(c.height) * 0.5;
    let beside =
        (w(s.radius) - w(EPS)) + half >= (half - w(dz)).abs() || w(EPS) >= (w(dz) - w(d.z)).abs();
    (beside, nm)
}

/// The cylinder's normal of collision made unit length, or none shorter than 0.0002.
fn unit_normal(c: &CylSphere, curr: Vec3, s: &Sphere, d: Vec3, sum: f32) -> Option<(bool, Vec3)> {
    let (flag, m) = normal_of_collision(c, curr, s, d, sum);
    let len = ((w(m.y) * w(m.y) + w(m.x) * w(m.x)) + w(m.z) * w(m.z)).sqrt();
    if len < w(EPS) {
        return None;
    }
    let k = 1.0 / len;
    Some((flag, Vec3::new(n(w(m.x) * k), n(w(m.y) * k), n(w(m.z) * k))))
}

/// The mover's sphere `s` (its sphere `index`) slides along the cylinder, along the cylinder's
/// normal of collision ([`sphere::slide`]).
// D2-04227
pub fn slide(
    t: &mut Transition,
    c: &CylSphere,
    s: &Sphere,
    d: Vec3,
    sum: f32,
    index: usize,
) -> TransitionState {
    let curr = t.sphere_path.global_curr_center[index];
    let Some((_, nm)) = unit_normal(c, curr, s, d, sum) else {
        return TransitionState::Collided;
    };
    sphere::slide(t, s, nm, curr)
}

/// The mover lands on the cylinder: the landing is saved with the cylinder's normal of collision,
/// and the walkable allowance becomes the landing's.
// D2-04230
pub fn land(t: &mut Transition, c: &CylSphere, s: &Sphere, d: Vec3, sum: f32) -> TransitionState {
    let curr = t.sphere_path.global_curr_center[0];
    let Some((_, nm)) = unit_normal(c, curr, s, d, sum) else {
        return TransitionState::Collided;
    };
    t.sphere_path.set_collide(nm);
    t.sphere_path.walkable_allowance = LANDING_Z;
    TransitionState::Adjusted
}

/// The mover, on the ground, meets the cylinder: it steps up onto it, the face it met turned by
/// the cylinder's normal of collision carried out of the frame the transition last tested in, when
/// its step height reaches the cylinder's top; else, or when the step up finds no ground, it slides.
// D2-04228
pub fn step_up(
    t: &mut Transition,
    up: &mut dyn StepUp,
    c: &CylSphere,
    s: &Sphere,
    d: Vec3,
    sum: f32,
) -> TransitionState {
    let reach = (w(s.radius) + w(c.height)) - w(d.z);
    if !(w(t.object_info.step_up_height) >= reach) {
        return slide(t, c, s, d, sum, 0);
    }
    let curr = t.sphere_path.global_curr_center[0];
    let Some((_, nm)) = unit_normal(c, curr, s, d, sum) else {
        return TransitionState::Collided;
    };
    let face = local_to_global(&t.sphere_path.localspace_pos.frame, nm);
    if up.step_up(t, face) {
        return TransitionState::Ok;
    }
    super::step_up_slide(t)
}

/// The mover steps down onto the cylinder (its first sphere `s`, `d` from the low point, or its
/// second sphere, overlapping it): lowered so its first sphere rests on the cylinder's top, the
/// walk interpolation narrowed by how much of the step down that takes, and the contact plane the
/// top's; a step down of less than 0.0002, or one that would narrow the interpolation below -0.1
/// or not at all, collides.
// D2-04229
pub fn step_down(
    t: &mut Transition,
    c: &CylSphere,
    s: &Sphere,
    d: Vec3,
    sum: f32,
) -> TransitionState {
    if !overlaps(c, s, d, sum) {
        if t.sphere_path.num_sphere <= 1 {
            return TransitionState::Ok;
        }
        let s1 = t.sphere_path.global_sphere[1];
        if !overlaps(c, &s1, sub(s1.center, c.low_pt), sum) {
            return TransitionState::Ok;
        }
    }
    let drop = w(t.sphere_path.step_down_amt) * w(t.sphere_path.walk_interp);
    let dropf = n(drop);
    if w(EPS) > drop.abs() {
        return TransitionState::Collided;
    }
    let lift = (w(c.height) + w(s.radius)) - w(d.z);
    let wi = w(t.sphere_path.walk_interp);
    let interp = (1.0 - lift / w(dropf)) * wi;
    if interp >= wi || interp < FLOOR_STEP {
        return TransitionState::Collided;
    }
    let top = (lift - w(s.radius)) + w(s.center.z);
    let plane = Plane::new(
        Vec3::new(0.0, 0.0, 1.0),
        n((w(s.center.x) + w(s.center.y)) * 0.0 - top),
    );
    set_contact_plane(&mut t.collision_info, plane);
    t.collision_info.contact_plane_cell_id = t.sphere_path.check_pos.cell;
    t.sphere_path.walk_interp = n(interp);
    t.sphere_path
        .add_offset_to_check_pos(Vec3::new(0.0, 0.0, n(lift)));
    TransitionState::Adjusted
}

/// Where along `m` (a move across the ground from `p`, `p` from the axis) the mover first comes
/// within the radius whose square is `rr` of the axis: the earlier root ahead of the start, else
/// the later; none when it never does or barely moves across the ground. `strict` refuses as the
/// client's beside-the-cylinder arm does (a number that is not one passing), else as its
/// clear-of-the-faces arm does (a number that is not one refused).
fn side_time(m: Vec3, p: Vec3, rr: f64, strict: bool) -> Option<f64> {
    let a = w(m.y) * w(m.y) + w(m.x) * w(m.x);
    let b = -(w(p.y) * w(m.y) + w(p.x) * w(m.x));
    let disc = b * b - ((w(p.y) * w(p.y) + w(p.x) * w(p.x)) - rr) * a;
    let refused = if strict {
        disc < 0.0 || a <= w(EPS)
    } else {
        !(disc >= 0.0) || !(a > w(EPS))
    };
    if refused {
        return None;
    }
    let q = disc.sqrt();
    let early = b - q;
    Some(if early < 0.0 || early.is_nan() {
        (q + b) / a
    } else {
        early / a
    })
}

/// The cylinder's point collision. A mover that does not clip its path exactly collides, the
/// cylinder's normal of collision its normal. One that does is moved back along its step (its
/// sphere `s` against where its first sphere began) to where it first touches, when that is within
/// the step: at the top or bottom face it is heading for when it is clear of them, unless it is
/// then beyond the cylinder's side, when it is where it first reaches the side (the face's time and
/// place kept when it never does); at the face when the normal says so; at the side when beside it.
// D2-04226
#[allow(clippy::too_many_lines)]
pub fn collide_with_point(
    t: &mut Transition,
    c: &CylSphere,
    s: &Sphere,
    d: Vec3,
    sum: f32,
    index: usize,
) -> TransitionState {
    let curr_i = t.sphere_path.global_curr_center[index];
    let Some((flag, mut nm)) = unit_normal(c, curr_i, s, d, sum) else {
        return TransitionState::Collided;
    };
    if !t.object_info.has(ObjectInfoState::PERFECT_CLIP) {
        set_collision_normal(&mut t.collision_info, nm);
        return TransitionState::Collided;
    }
    let curr = t.sphere_path.global_curr_center[0];
    let off = t.sphere_path.curr_to_check_block_offset();
    let m = Vec3::new(
        n((w(s.center.x) - w(curr.x)) + w(off.x)),
        n(w(off.y) + (w(s.center.y) - w(curr.y))),
        n(w(off.z) + w(s.center.z - curr.z)),
    );
    let p = Vec3::new(curr.x - c.low_pt.x, curr.y - c.low_pt.y, 0.0);
    let pz = w(curr.z) - w(c.low_pt.z);
    let r = n(w(EPS) + w(sum));
    let face = |m: Vec3| {
        if m.z > 0.0 || m.z.is_nan() {
            -((pz + w(s.radius)) / w(m.z))
        } else {
            ((w(s.radius) + w(c.height)) - pz) / w(m.z)
        }
    };
    let along = |time: f64| Vec3::new(n(w(m.x) * time), n(w(m.y) * time), n(time * w(m.z)));
    let side_normal = |q: Vec3| {
        let inv = 1.0 / w(r);
        Vec3::new(
            n(w(n((w(q.x) + w(curr.x)) - w(c.low_pt.x))) * inv),
            n(((w(q.y) + w(curr.y)) - w(c.low_pt.y)) * inv),
            n(inv * 0.0),
        )
    };
    let (time, q) = if !flag {
        let time = face(m);
        nm = Vec3::new(0.0, 0.0, if m.z > 0.0 || m.z.is_nan() { -1.0 } else { 1.0 });
        let mut q = along(time);
        let mut time = time;
        let rr = n(w(r) * w(r));
        let ax = w(q.x) + w(p.x);
        let ay = w(q.y) + w(p.y);
        if ay * ay + ax * ax >= w(rr) {
            if let Some(t2) = side_time(m, p, w(rr), false) {
                time = t2;
                q = along(t2);
            }
            nm = side_normal(q);
        }
        (time, q)
    } else if nm.z != 0.0 || nm.z.is_nan() {
        let time = face(m);
        (time, along(time))
    } else {
        let Some(time) = side_time(m, p, w(r) * w(r), true) else {
            return TransitionState::Collided;
        };
        let q = along(time);
        if !(time < 0.0 || time > 1.0) {
            nm = side_normal(q);
        }
        (time, q)
    };
    if time < 0.0 || time > 1.0 {
        return TransitionState::Collided;
    }
    let ty = n(w(q.y) + w(curr.y));
    let tz = n(w(q.z) + w(curr.z));
    let moved = Vec3::new(
        n((w(q.x) + w(curr.x)) - w(s.center.x)),
        n(w(ty) - w(s.center.y)),
        n(w(tz) - w(s.center.z)),
    );
    set_collision_normal(&mut t.collision_info, nm);
    t.sphere_path.add_offset_to_check_pos(moved);
    TransitionState::Adjusted
}

/// After a landing, the mover settles onto the cylinder's top: moved back along its step to where
/// its first sphere's bottom reaches the top's height, the walk interpolation narrowed by how much
/// of the step that takes back, and the contact plane the top's; none when it is then beyond the
/// cylinder's side, and a collision when the move is level within 0.0002 or takes back none of
/// the step or more than 1.1 of it.
fn settle(t: &mut Transition, c: &CylSphere, s0: &Sphere, d: Vec3, sum: f32) -> TransitionState {
    let a = sub(t.sphere_path.global_curr_center[0], s0.center);
    let m = sub(a, t.sphere_path.curr_to_check_block_offset());
    if m.z.abs() < EPS {
        return TransitionState::Collided;
    }
    let time = ((w(c.height) + w(s0.radius)) - w(d.z)) / w(m.z);
    let q = Vec3::new(n(w(m.x) * time), n(w(m.y) * time), n(time * w(m.z)));
    let ax = w(q.x) + w(d.x);
    let ay = w(q.y) + w(d.y);
    if w(sum) * w(sum) < ay * ay + ax * ax {
        return TransitionState::Ok;
    }
    let wi = w(t.sphere_path.walk_interp);
    let interp = (1.0 - time) * wi;
    if interp >= wi || interp < FLOOR_STEP {
        return TransitionState::Collided;
    }
    let point = Vec3::new(
        n(w(q.x) + w(s0.center.x)),
        n(w(q.y) + w(s0.center.y)),
        n((w(q.z) + w(s0.center.z)) - w(s0.radius)),
    );
    set_contact_plane(
        &mut t.collision_info,
        plane_through(Vec3::new(0.0, 0.0, 1.0), point),
    );
    t.collision_info.contact_plane_cell_id = t.sphere_path.check_pos.cell;
    t.sphere_path.walk_interp = n(interp);
    t.sphere_path.add_offset_to_check_pos(q);
    TransitionState::Adjusted
}

/// The mover meets the upright cylinder `c`, in its check position's landblock frame.
///
/// Placed or passing through what it meets, it collides when either of its spheres overlaps. Then,
/// by what it is doing: stepping down, it steps down onto the top ([`step_down`]); checking where
/// it may walk, it collides when either sphere overlaps; after a landing, it settles onto the top;
/// on the ground, overlapping, it steps up onto it ([`step_up`]), or its second sphere slides along
/// it ([`slide`]); clipping its path, it meets it as a point ([`collide_with_point`]); else, in the
/// air, it lands on it ([`land`]), or its second sphere meets it as a point.
// D2-04231
pub fn meet(t: &mut Transition, up: &mut dyn StepUp, c: &CylSphere) -> TransitionState {
    let s0 = t.sphere_path.global_sphere[0];
    let s1 = t.sphere_path.global_sphere[1];
    let d = sub(s0.center, c.low_pt);
    let d1 = sub(s1.center, c.low_pt);
    let sum = n((w(c.radius) - w(EPS)) + w(s0.radius));
    let two = t.sphere_path.num_sphere > 1;
    let second = two && overlaps(c, &s1, d1, sum);
    let sp = &t.sphere_path;
    if sp.insert_type == InsertType::Placement || sp.obstruction_ethereal {
        return if overlaps(c, &s0, d, sum) || second {
            TransitionState::Collided
        } else {
            TransitionState::Ok
        };
    }
    if sp.step_down {
        return step_down(t, c, &s0, d, sum);
    }
    if sp.check_walkable {
        return if overlaps(c, &s0, d, sum) || second {
            TransitionState::Collided
        } else {
            TransitionState::Ok
        };
    }
    if sp.collide {
        if !overlaps(c, &s0, d, sum) && !second {
            return TransitionState::Ok;
        }
        return settle(t, c, &s0, d, sum);
    }
    let state = t.object_info.state;
    if state & (ObjectInfoState::CONTACT | ObjectInfoState::ON_WALKABLE) != 0 {
        if overlaps(c, &s0, d, sum) {
            return step_up(t, up, c, &s0, d, sum);
        }
        if second {
            return slide(t, c, &s1, d1, sum, 1);
        }
        return TransitionState::Ok;
    }
    if state & ObjectInfoState::PATH_CLIPPED != 0 {
        if overlaps(c, &s0, d, sum) {
            return collide_with_point(t, c, &s0, d, sum, 0);
        }
        return TransitionState::Ok;
    }
    if overlaps(c, &s0, d, sum) {
        return land(t, c, &s0, d, sum);
    }
    if second {
        return collide_with_point(t, c, &s1, d1, sum, 1);
    }
    TransitionState::Ok
}

/// [`meet`] for a cylinder given in its object's frame: the transition's last tested frame set to
/// the object's position (unscaled), the cylinder scaled by `scale` (its height by the scale's z,
/// its radius by the mean of its x and y) and carried from `pos` into the mover's check position's
/// landblock frame.
// D2-04232
pub fn meet_placed(
    t: &mut Transition,
    up: &mut dyn StepUp,
    c: &CylSphere,
    pos: &Position,
    scale: Vec3,
) -> TransitionState {
    t.sphere_path
        .cache_localspace_sphere_axes(pos, Vec3::new(1.0, 1.0, 1.0));
    let radius = n(((w(scale.y) + w(scale.x)) * 0.5) * w(c.radius));
    let height = n(w(c.height) * w(scale.z));
    let local = Vec3::new(
        n(w(c.low_pt.x) * w(scale.x)),
        n(w(c.low_pt.y) * w(scale.y)),
        n(w(c.low_pt.z) * w(scale.z)),
    );
    let low_pt = point_to_global_from(&t.sphere_path.check_pos, pos, local);
    meet(
        t,
        up,
        &CylSphere {
            low_pt,
            radius,
            height,
        },
    )
}
