//! A moving body meeting another object's sphere.
//!
//! Every test works in the mover's check position's landblock frame; the obstacle sphere is already
//! there. `sum` is the two radii summed less 0.0002, the overlap form every arm is handed; the
//! swept forms add the 0.0002 back.

// A negated comparison is the client's own: it passes a number that is not one where the plain
// comparison would refuse it.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use dereth_physics::geom::Sphere;
use dereth_physics::globals::LANDING_Z;
use dereth_physics::transition::{InsertType, ObjectInfoState, Transition};
use dereth_physics::TransitionState;
use dereth_primitives::{Position, Vec3};

use super::{
    eps_sq, n, overlaps, plane_through, set_collision_normal, set_contact_plane, w, StepUp,
};
use crate::geom::sphere::time_of_collision;
use crate::geom::{point_to_global_from, EPS};

/// The lowest walk interpolation a step down may leave.
const FLOOR_STEP: f64 = -0.1;

/// `a - b`, each part rounded.
fn sub(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(a.x - b.x, a.y - b.y, a.z - b.z)
}

/// The unit direction from `c` to `p`, its length summed z, then y, then x (`y_last` false) or z,
/// then x, then y (`y_last` true), the y difference rounded and the others kept wide, as the
/// client's two forms of it do; none when shorter than 0.0002.
fn away(p: Vec3, c: Vec3, y_last: bool) -> Option<Vec3> {
    let dx = w(p.x) - w(c.x);
    let dy = w(n(w(p.y) - w(c.y)));
    let dz = w(p.z) - w(c.z);
    let len = if y_last {
        ((dz * dz + dx * dx) + dy * dy).sqrt()
    } else {
        ((dz * dz + dy * dy) + dx * dx).sqrt()
    };
    if len < w(EPS) {
        return None;
    }
    let k = 1.0 / len;
    Some(Vec3::new(n(k * dx), n(dy * k), n(dz * k)))
}

/// The mover's sphere `index` against the obstacle at the start of the step: the sphere's center
/// less the current one, each difference kept wide, with the offset from the current position's
/// landblock to the check position's.
fn moved(t: &Transition, s: &Sphere, curr: Vec3) -> Vec3 {
    let off = t.sphere_path.curr_to_check_block_offset();
    Vec3::new(
        n((w(s.center.x) - w(curr.x)) + w(off.x)),
        n(w(off.y) + (w(s.center.y) - w(curr.y))),
        n(w(off.z) + w(s.center.z - curr.z)),
    )
}

/// The mover's sphere `s` slides along the surface whose normal is `normal`, from its center at the
/// start of the step `curr`: along the line where that surface meets the contact plane (or the
/// last one known) when the two are not parallel within 0.0002 squared, refused as a collision when
/// what is left of the move along it is shorter than 0.0002 (squared); against a parallel plane it
/// slides along the surface when it faces away from the plane, and otherwise collides, `normal`
/// turned back along the move. A normal of zero halves what is left of the move.
// D2-04218
pub fn slide(t: &mut Transition, s: &Sphere, normal: Vec3, curr: Vec3) -> TransitionState {
    let mut normal = normal;
    slide_turning(t, s, &mut normal, curr)
}

/// [`slide`], `normal` turned back along the move where it collides against a parallel plane, as
/// the client turns the normal it was handed. It is this world's slide wherever the transition
/// slides the mover ([`crate::rules::rules`]).
pub fn slide_turning(t: &mut Transition, s: &Sphere, nm: &mut Vec3, curr: Vec3) -> TransitionState {
    let c = s.center;
    if nm.x == 0.0 && nm.y == 0.0 && nm.z == 0.0 {
        let half = Vec3::new(
            n((w(curr.x) - w(c.x)) * 0.5),
            n((w(curr.y) - w(c.y)) * 0.5),
            n(w(curr.z - c.z) * 0.5),
        );
        t.sphere_path.add_offset_to_check_pos(half);
        return TransitionState::Adjusted;
    }
    set_collision_normal(&mut t.collision_info, *nm);
    let d = moved(t, s, curr);
    let ci = &t.collision_info;
    let p = if ci.contact_plane_valid {
        ci.contact_plane.normal
    } else {
        ci.last_known_contact_plane.normal
    };
    let (m, q) = (*nm, p);
    let cx = n(w(m.y) * w(q.z) - w(m.z) * w(q.y));
    let cy = w(m.z) * w(q.x) - w(q.z) * w(m.x);
    let czw = w(q.y) * w(m.x) - w(m.y) * w(q.x);
    let cz = n(czw);
    let m2 = (czw * w(cz) + cy * cy) + w(cx) * w(cx);
    if m2 >= eps_sq() || m2.is_nan() {
        let k = (w(d.z) * w(cz) + w(d.y) * cy) + w(cx) * w(d.x);
        let qxw = (1.0 / w(n(m2))) * (w(cx) * k);
        let inv = n(1.0 / w(n(m2)));
        let py = n(k * cy);
        let pz = n(k * w(cz));
        let qy = n(w(inv) * w(py));
        let qzw = w(inv) * w(pz);
        let qz = n(qzw);
        let len2 = (qzw * w(qz) + w(qy) * w(qy)) + qxw * qxw;
        if len2 < w(EPS) {
            return TransitionState::Collided;
        }
        let off = Vec3::new(n(qxw - w(d.x)), n(w(qy) - w(d.y)), n(w(qz) - w(d.z)));
        t.sphere_path.add_offset_to_check_pos(off);
        return TransitionState::Slid;
    }
    let facing = (w(m.z) * w(q.z) + w(m.y) * w(q.y)) + w(q.x) * w(m.x);
    if facing < 0.0 {
        // Turned back along the move, and collides.
        let back = Vec3::new(-d.x, -d.y, -d.z);
        *nm = back;
        let (bx, by, bz) = (w(back.x), w(back.y), w(back.z));
        let len = ((bz * bz + by * by) + bx * bx).sqrt();
        if len < w(EPS) {
            return TransitionState::Collided;
        }
        let k = 1.0 / len;
        *nm = Vec3::new(n(k * bx), n(by * k), n(bz * k));
        set_collision_normal(&mut t.collision_info, *nm);
        return TransitionState::Collided;
    }
    let k = w(n((w(d.x) * w(m.x) + w(d.z) * w(m.z)) + w(d.y) * w(m.y)));
    let off = Vec3::new(n(-w(m.x) * k), n(-w(m.y) * k), n(-w(m.z) * k));
    t.sphere_path.add_offset_to_check_pos(off);
    TransitionState::Slid
}

/// The mover's sphere `index` slides round the obstacle centered at `o`, met where the step began,
/// along the contact plane (or the last one known): as [`slide`], but parallel within 0.0002 (not
/// its square), and against a parallel plane it slides only when `toward` (the mover's first
/// sphere less the obstacle) faces away from the plane.
// D2-04217
pub fn slide_round(t: &mut Transition, o: Vec3, toward: Vec3, index: usize) -> TransitionState {
    let curr = t.sphere_path.global_curr_center[index];
    let Some(nm) = away(curr, o, true) else {
        return TransitionState::Collided;
    };
    set_collision_normal(&mut t.collision_info, nm);
    let ci = &t.collision_info;
    let q = if ci.contact_plane_valid {
        ci.contact_plane.normal
    } else {
        ci.last_known_contact_plane.normal
    };
    let cx = n(w(q.z) * w(nm.y) - w(q.y) * w(nm.z));
    let cy = n(w(nm.z) * w(q.x) - w(q.z) * w(nm.x));
    let cz = n(w(q.y) * w(nm.x) - w(q.x) * w(nm.y));
    let s = t.sphere_path.global_sphere[index];
    let d = moved(t, &s, curr);
    let m2 = (w(cz) * w(cz) + w(cy) * w(cy)) + w(cx) * w(cx);
    if m2 >= w(EPS) || m2.is_nan() {
        let k = (w(d.z) * w(cz) + w(d.y) * w(cy)) + w(d.x) * w(cx);
        let (px, py, pz) = (n(w(cx) * k), n(w(cy) * k), n(w(cz) * k));
        let inv = 1.0 / m2;
        let qx = n(w(px) * inv);
        let qy = n(w(py) * inv);
        let qzw = inv * w(pz);
        let len2 = (qzw * qzw + w(qy) * w(qy)) + w(qx) * w(qx);
        if len2 < w(EPS) {
            return TransitionState::Collided;
        }
        let off = Vec3::new(n(w(qx) - w(d.x)), n(w(qy) - w(d.y)), n(qzw - w(d.z)));
        t.sphere_path.add_offset_to_check_pos(off);
        return TransitionState::Slid;
    }
    let facing = (w(q.z) * w(toward.z) + w(q.y) * w(toward.y)) + w(q.x) * w(toward.x);
    if facing < 0.0 {
        return TransitionState::Collided;
    }
    let k = w(n((w(d.z) * w(nm.z) + w(d.x) * w(nm.x)) + w(d.y) * w(nm.y)));
    let off = Vec3::new(n(-w(nm.x) * k), n(-w(nm.y) * k), n(-w(nm.z) * k));
    t.sphere_path.add_offset_to_check_pos(off);
    TransitionState::Slid
}

/// The mover's sphere `index` slides along the obstacle centered at `o`, the normal from the
/// obstacle to where the sphere began the step ([`slide`]).
// D2-04221
pub fn slide_along(t: &mut Transition, o: Vec3, index: usize) -> TransitionState {
    let curr = t.sphere_path.global_curr_center[index];
    let Some(nm) = away(curr, o, true) else {
        return TransitionState::Collided;
    };
    let s = t.sphere_path.global_sphere[index];
    slide(t, &s, nm, curr)
}

/// The mover lands on the obstacle centered at `o`: the landing is saved with the normal from the
/// obstacle to where the mover began the step, and the walkable allowance becomes the landing's.
// D2-04220
pub fn land(t: &mut Transition, o: Vec3) -> TransitionState {
    let curr = t.sphere_path.global_curr_center[0];
    let Some(nm) = away(curr, o, false) else {
        return TransitionState::Collided;
    };
    t.sphere_path.set_collide(nm);
    t.sphere_path.walkable_allowance = LANDING_Z;
    TransitionState::Adjusted
}

/// The mover, on the ground, meets the obstacle `o` (its first sphere `d` from it): it steps up
/// onto it, the face it met turned from the obstacle's center to where it began the step, when its
/// step height reaches the top of the overlap; else, or when the step up finds no ground, it slides.
// D2-04219
pub fn step_up(
    t: &mut Transition,
    up: &mut dyn StepUp,
    o: Vec3,
    d: Vec3,
    sum: f32,
) -> TransitionState {
    let r = w(EPS) + w(sum);
    let reach = r - w(d.z);
    if !(w(t.object_info.step_up_height) >= reach) {
        return slide_round(t, o, d, 0);
    }
    let curr = t.sphere_path.global_curr_center[0];
    let face = sub(curr, o);
    if up.step_up(t, face) {
        return TransitionState::Ok;
    }
    super::step_up_slide(t)
}

/// The mover steps down onto the obstacle `o` (its first sphere `d` from it, or its second sphere
/// overlapping): it is lowered onto the obstacle's top, the walk interpolation narrowed by how much
/// of the step down that takes, and the contact plane set tangent to the obstacle where the mover
/// rests on it, when that plane is steeper than the walkable allowance allows; a step down that
/// would narrow the interpolation below -0.1, or not at all, collides.
// D2-04215
pub fn step_down(t: &mut Transition, o: &Sphere, d: Vec3, sum: f32) -> TransitionState {
    let yy = n(w(d.y) * w(d.y));
    let xx = n(w(d.x) * w(d.x));
    let near = (w(d.z) * w(d.z) + w(xx)) + w(yy);
    if !(w(sum) * w(sum) >= near) {
        if t.sphere_path.num_sphere <= 1 {
            return TransitionState::Ok;
        }
        let d1 = sub(t.sphere_path.global_sphere[1].center, o.center);
        if !overlaps(d1, sum) {
            return TransitionState::Ok;
        }
    }
    let r = n(w(EPS) + w(sum));
    let drop = w(t.sphere_path.step_down_amt) * w(t.sphere_path.walk_interp);
    let dropf = n(drop);
    let f = if w(EPS) > drop.abs() {
        0.0
    } else {
        ((w(r) * w(r) - (w(xx) + w(yy))).sqrt() - w(d.z)) / w(dropf)
    };
    let wi = w(t.sphere_path.walk_interp);
    let interp = (1.0 - f) * wi;
    if interp >= wi || interp < FLOOR_STEP {
        return TransitionState::Collided;
    }
    let lift = n(w(dropf) * f);
    let zsum = w(lift) + w(d.z);
    let inv = 1.0 / w(r);
    let nx = n(inv * w(d.x));
    let ny = n(inv * w(d.y));
    let nzw = zsum * inv;
    if !(nzw > w(t.sphere_path.walkable_allowance)) {
        return TransitionState::Ok;
    }
    let rr = w(o.radius);
    let point = Vec3::new(
        n(w(nx) * rr + w(o.center.x)),
        n(w(n(w(ny) * rr)) + w(o.center.y)),
        n(w(n(nzw * rr)) + w(o.center.z)),
    );
    let plane = plane_through(Vec3::new(nx, ny, n(nzw)), point);
    set_contact_plane(&mut t.collision_info, plane);
    t.collision_info.contact_plane_cell_id = t.sphere_path.check_pos.cell;
    t.sphere_path.walk_interp = n(interp);
    t.sphere_path
        .add_offset_to_check_pos(Vec3::new(0.0, 0.0, lift));
    TransitionState::Adjusted
}

/// The mover's sphere `s` (its sphere `index`) meets the obstacle centered at `o` as a point: a
/// mover that does not clip its path exactly collides, the normal from the obstacle to where it
/// began the step; one that does is moved back along its step to where it first touches (when it
/// touches within the step), the normal from the obstacle to there.
// D2-04216
pub fn collide_with_point(
    t: &mut Transition,
    o: Vec3,
    s: &Sphere,
    sum: f32,
    index: usize,
) -> TransitionState {
    let curr = t.sphere_path.global_curr_center[index];
    if !t.object_info.has(ObjectInfoState::PERFECT_CLIP) {
        if let Some(nm) = away(curr, o, false) {
            set_collision_normal(&mut t.collision_info, nm);
        }
        return TransitionState::Collided;
    }
    let r = n(w(EPS) + w(sum));
    let delta = sub(curr, o);
    let m = moved(t, s, curr);
    let time = time_of_collision(m, delta, r);
    if time < w(EPS) || time > 1.0 {
        return TransitionState::Collided;
    }
    let py = n(w(m.y) * time);
    let pz = n(w(m.z) * time);
    let off = Vec3::new(
        n(w(m.x) * time - w(m.x)),
        n(w(py) - w(m.y)),
        n(w(pz) - w(m.z)),
    );
    let c = s.center;
    let qz = n(w(off.z) + w(c.z));
    let qx = n((w(off.x) + w(c.x)) - w(o.x));
    let yw = (w(off.y) + w(c.y)) - w(o.y);
    let zw = w(qz) - w(o.z);
    let inv = 1.0 / w(r);
    let nm = Vec3::new(n(w(qx) * inv), n(yw * inv), n(zw * inv));
    set_collision_normal(&mut t.collision_info, nm);
    t.sphere_path.add_offset_to_check_pos(off);
    TransitionState::Adjusted
}

/// After a landing, the mover settles onto the obstacle `o` (its first sphere `d` from it): moved
/// back along its step to where it first touches the obstacle, the walk interpolation narrowed by
/// how much of the step that takes back, and the contact plane set tangent to the obstacle there,
/// when that plane is walkable by the allowance; it collides when the move takes back none of the
/// step or more than 1.1 of it.
#[allow(clippy::many_single_char_names)]
fn settle(t: &mut Transition, s0: &Sphere, d: Vec3, sum: f32) -> TransitionState {
    let a = sub(t.sphere_path.global_curr_center[0], s0.center);
    let m = sub(a, t.sphere_path.curr_to_check_block_offset());
    let r = n(w(EPS) + w(sum));
    let aa = (w(m.x) * w(m.x) + w(m.y) * w(m.y)) + w(m.z) * w(m.z);
    let b = -((w(d.y) * w(m.y) + w(m.x) * w(d.x)) + w(m.z) * w(d.z));
    let c = ((w(d.y) * w(d.y) + w(d.x) * w(d.x)) + w(d.z) * w(d.z)) - w(r) * w(r);
    let mut root = b + (b * b - c * aa).sqrt();
    if root > 1.0 {
        root = (b + b) - root;
    }
    let time = root / aa;
    let wi = w(t.sphere_path.walk_interp);
    let interp = (1.0 - time) * wi;
    if interp >= wi || interp < FLOOR_STEP {
        return TransitionState::Collided;
    }
    let off = Vec3::new(n(w(m.x) * time), n(w(m.y) * time), n(time * w(m.z)));
    let pz = n(w(off.z) + w(d.z));
    let px = w(off.x) + w(d.x);
    let py = w(off.y) + w(d.y);
    let invw = 1.0 / w(r);
    let inv = w(n(invw));
    let nm = Vec3::new(n(invw * px), n(py * inv), n(inv * w(pz)));
    if !t.sphere_path.is_walkable_allowable(nm.z) {
        return TransitionState::Ok;
    }
    let rr = w(s0.radius);
    let c = s0.center;
    let point = Vec3::new(
        n(w(c.x) - w(nm.x) * rr),
        n(w(c.y) - w(n(w(nm.y) * rr))),
        n(w(c.z) - w(n(rr * w(nm.z)))),
    );
    set_contact_plane(&mut t.collision_info, plane_through(nm, point));
    t.collision_info.contact_plane_cell_id = t.sphere_path.check_pos.cell;
    t.sphere_path.walk_interp = n(interp);
    t.sphere_path.add_offset_to_check_pos(off);
    TransitionState::Adjusted
}

/// The mover meets the sphere `o`, in its check position's landblock frame. `creature` is whether
/// the obstacle is a creature or a missile: the mover never steps down onto one, nor settles onto
/// one after a landing.
///
/// Placed or passing through what it meets, it collides when either of its spheres overlaps. Then,
/// by what it is doing: stepping down, it steps down onto it ([`step_down`]); checking where it may
/// walk, it collides when either sphere overlaps; after a landing, it settles onto it; on the
/// ground, overlapping, it steps up onto it ([`step_up`]), or its second sphere slides along it
/// ([`slide_along`]); clipping its path, it meets it as a point ([`collide_with_point`]); else,
/// in the air, it lands on it ([`land`]), or its second sphere meets it as a point.
// D2-04222
pub fn meet(
    t: &mut Transition,
    up: &mut dyn StepUp,
    o: &Sphere,
    creature: bool,
) -> TransitionState {
    let s0 = t.sphere_path.global_sphere[0];
    let s1 = t.sphere_path.global_sphere[1];
    let d = sub(s0.center, o.center);
    let sum = n((w(o.radius) + w(s0.radius)) - w(EPS));
    let second = t.sphere_path.num_sphere > 1 && overlaps(sub(s1.center, o.center), sum);
    let sp = &t.sphere_path;
    if sp.obstruction_ethereal || sp.insert_type == InsertType::Placement {
        return if overlaps(d, sum) || second {
            TransitionState::Collided
        } else {
            TransitionState::Ok
        };
    }
    if sp.step_down {
        if creature {
            return TransitionState::Ok;
        }
        return step_down(t, o, d, sum);
    }
    if sp.check_walkable {
        return if overlaps(d, sum) || second {
            TransitionState::Collided
        } else {
            TransitionState::Ok
        };
    }
    if sp.collide {
        if creature {
            return TransitionState::Ok;
        }
        if !overlaps(d, sum) && !second {
            return TransitionState::Ok;
        }
        return settle(t, &s0, d, sum);
    }
    let state = t.object_info.state;
    if state & (ObjectInfoState::CONTACT | ObjectInfoState::ON_WALKABLE) != 0 {
        if overlaps(d, sum) {
            return step_up(t, up, o.center, d, sum);
        }
        if second {
            return slide_along(t, o.center, 1);
        }
        return TransitionState::Ok;
    }
    if state & ObjectInfoState::PATH_CLIPPED != 0 {
        if overlaps(d, sum) {
            return collide_with_point(t, o.center, &s0, sum, 0);
        }
        return TransitionState::Ok;
    }
    if overlaps(d, sum) {
        return land(t, o.center);
    }
    if second {
        return collide_with_point(t, o.center, &s1, sum, 1);
    }
    TransitionState::Ok
}

/// [`meet`] for a sphere given in its object's frame: scaled by `scale` along each axis (its
/// radius by the scale's z), then carried from the object's position `pos` into the mover's check
/// position's landblock frame.
// D2-04223
pub fn meet_placed(
    t: &mut Transition,
    up: &mut dyn StepUp,
    s: &Sphere,
    pos: &Position,
    scale: Vec3,
    creature: bool,
) -> TransitionState {
    let local = Vec3::new(
        n(w(scale.x) * w(s.center.x)),
        n(w(scale.y) * w(s.center.y)),
        n(w(s.center.z) * w(scale.z)),
    );
    let radius = n(w(s.radius) * w(scale.z));
    let center = point_to_global_from(&t.sphere_path.check_pos, pos, local);
    meet(t, up, &Sphere::new(center, radius), creature)
}
