//! Cylinder-sphere transitions — the middle arm of the object's collision search.
//!
//! Transcribed from the client's own cylinder-sphere code: the step down,
//! the normal of collision, the point collision, the slide, the step up, the landing, and its
//! scaling wrapper.
//!
//! These are the cylinder's counterparts of the sphere helpers in
//! [`crate::transition::collide`], and they are *not* the same functions with a different
//! primitive: the cylinder has flat caps, so its step-down lift is a closed form rather than a
//! quadratic, its landing normal is always `+Z`, and its `collide_with_point` has to decide
//! between a cap hit and a side hit. Three differences from the sphere path are worth naming
//! because they look like transcription slips and are not:
//!
//! 1. the scaling wrapper caches the local-space sphere before it does anything else; the
//!    sphere's own wrapper does not. The
//!    cylinder's `step_sphere_up` needs `localspace_pos` to rotate its normal back into the
//!    world, which is why.
//! 2. The cylinder-path dispatcher's second-sphere test decides only **whether** to run the
//!    step-down; the arithmetic that follows still uses the **first** sphere and its delta. The
//!    client never re-points the sphere and delta arguments.
//! 3. There is no walkable-allowance gate in the cylinder step-down. The contact normal it
//!    builds is `(0, 0, 1)` by construction, so the gate would never fire.

use dereth_primitives::Vec3;

use crate::geom::plane::Plane;
use crate::geom::sphere::{CylSphere, Sphere};
use crate::geom::CylSphereExt;
use crate::globals::{EPSILON, LANDING_Z, WALK_INTERP_FLOOR_STEP};
use crate::landdefs;
use crate::math::V3;
use crate::transition::collide::slide_sphere;
use crate::transition::objectinfo::ObjectInfoState;
use crate::transition::spherepath::InsertType;
use crate::transition::{Transition, TransitionCtx, TransitionState};

/// Landing on a cylinder.
///
/// The cylinder path computes the normal, normalizes it, calls `set_collide`, and uses the same
/// `LandingZ` allowance.
pub fn land_on_cylinder(
    t: &mut Transition,
    cyl: &CylSphere,
    sphere: &Sphere,
    delta: Vec3,
    sum: f32,
) -> TransitionState {
    let (_, mut n) = cyl.normal_of_collision_full(
        t.sphere_path.global_curr_center[0],
        delta,
        sphere.radius,
        sum,
    );
    if n.normalize_check_small() {
        return TransitionState::Collided;
    }
    t.sphere_path.set_collide(n);
    t.sphere_path.walkable_allowance = LANDING_Z;
    TransitionState::Adjusted
}

/// The cylinder's slide.
///
/// The return value of `normal_of_collision` is **discarded**: even the "vertically clear of the
/// cap" answer, which returns 0, still slides along the radial normal it wrote.
pub fn slide_cyl_sphere(
    t: &mut Transition,
    cyl: &CylSphere,
    sphere: &Sphere,
    delta: Vec3,
    sum: f32,
    index: usize,
) -> TransitionState {
    let (_, mut n) = cyl.normal_of_collision_full(
        t.sphere_path.global_curr_center[index],
        delta,
        sphere.radius,
        sum,
    );
    if n.normalize_check_small() {
        return TransitionState::Collided;
    }
    let curr = t.sphere_path.global_curr_center[index];
    slide_sphere(t, *sphere, n, curr)
}

/// The cylinder's step up.
///
/// The step-height test is against `(sphere.radius + cyl.height) - delta.z` — the lift to the top
/// cap — where the sphere step-up test is against `(sum + 0.0002) - delta.z`.
///
/// The normal is rotated through `localspace_pos` before the step-up path sees it. The sphere path
/// does not apply that rotation because it cached the local-space sphere against the *obstacle's*
/// frame on the way in.
pub fn step_cyl_sphere_up(
    ctx: &TransitionCtx<'_>,
    t: &mut Transition,
    cyl: &CylSphere,
    sphere: &Sphere,
    delta: Vec3,
    sum: f32,
) -> TransitionState {
    if t.object_info.step_up_height < (sphere.radius + cyl.height) - delta.z {
        return slide_cyl_sphere(t, cyl, sphere, delta, sum, 0);
    }
    let (_, mut n) = cyl.normal_of_collision_full(
        t.sphere_path.global_curr_center[0],
        delta,
        sphere.radius,
        sum,
    );
    if n.normalize_check_small() {
        return TransitionState::Collided;
    }
    let pos = t.sphere_path.localspace_pos;
    let world_n = crate::math::localtoglobalvec(crate::math::l2g(pos.frame.rotation), n);
    if super::walk::step_up(ctx, t, world_n) {
        return TransitionState::Ok;
    }
    // Failure takes the same slide tail.
    t.collision_info.contact_plane_valid = false;
    t.collision_info.contact_plane_is_water = false;
    t.sphere_path.step_up = false;
    let s0 = t.sphere_path.global_sphere[0];
    let curr = t.sphere_path.global_curr_center[0];
    let up_n = t.sphere_path.step_up_normal;
    slide_sphere(t, s0, up_n, curr)
}

/// The cylinder's step down.
///
/// The lift is closed-form — `(cyl.height + sphere.radius) - delta.z`, the distance that puts the
/// sphere's underside on the top cap — where the sphere step-down has to solve
/// `sqrt(r^2 - (dx^2 + dy^2)) - dz`. `set_contact_plane`'s second argument is **1** in the
/// client (see the note on [`crate::transition::CollisionInfo::set_contact_plane`]'s caller
/// in `collide.rs`).
pub fn step_cyl_sphere_down(
    t: &mut Transition,
    cyl: &CylSphere,
    sphere: &Sphere,
    delta: Vec3,
    sum: f32,
) -> TransitionState {
    if !cyl.collides_with_sphere(sphere, delta, sum) {
        // Point 2 of the module note: this decides only *whether*.
        if t.sphere_path.num_sphere <= 1 {
            return TransitionState::Ok;
        }
        let s1 = t.sphere_path.global_sphere[1];
        let d1 = s1.center.sub(cyl.low_pt);
        if !cyl.collides_with_sphere(&s1, d1, sum) {
            return TransitionState::Ok;
        }
    }
    let dz = t.sphere_path.step_down_amt * t.sphere_path.walk_interp;
    if dz.abs() < EPSILON {
        return TransitionState::Collided;
    }
    let lift = (cyl.height + sphere.radius) - delta.z;
    let interp = (1.0 - lift / dz) * t.sphere_path.walk_interp;
    if interp >= t.sphere_path.walk_interp || interp < WALK_INTERP_FLOOR_STEP {
        return TransitionState::Collided;
    }
    let point = Vec3::new(
        sphere.center.x,
        sphere.center.y,
        (lift - sphere.radius) + sphere.center.z,
    );
    let plane = Plane::from_normal_and_point(Vec3::new(0.0, 0.0, 1.0), point);
    t.collision_info.set_contact_plane(plane, true);
    t.collision_info.contact_plane_cell_id = t.sphere_path.check_pos.cell;
    t.sphere_path.walk_interp = interp;
    t.sphere_path
        .add_offset_to_check_pos(Vec3::new(0.0, 0.0, lift));
    TransitionState::Adjusted
}

/// The cylinder's point collision.
///
/// The reachable half is the first four statements: build the normal, normalise it, and — unless
/// `PERFECT_CLIP (0x40)` is set — record it and report `COLLIDED_TS`.
///
/// **The `PERFECT_CLIP` half is live, not dead.** The bit is written while initializing a player
/// transition. Flags `0x5C` mean
/// `IS_VIEWER | PATH_CLIPPED | FREE_ROTATE |
/// PERFECT_CLIP` — the **camera sweep**. This crate writes it too, as
/// [`crate::globals::VIEWER_OBJECT_INFO_STATE`], which `dereth-client`'s camera hands to
/// the world's sphere sweep. So the camera sliding past a cylsphere object runs
/// exactly this branch, and the `0x8` test above it means the camera reaches
/// `collide_with_point` in the first place.
///
/// Past the `PERFECT_CLIP` test the displacement, the normal and the time are easy to confuse;
/// the flow below is confirmed, and these are the four places that are easy to misread:
///
/// * the cap sign is on the **movement's** Z, not the normal's: it is compared against `0.0`,
///   so `movement.z > 0` takes `-((d.z + r) / movement.z)`
///   and a `-1` normal, and `movement.z < 0` takes `((r + height) - d.z) / movement.z` and `+1`;
/// * a failed side discriminant or a degenerate `a` is a **skip**, not
///   a return: the cap time and its displacement stand in the frame slots and only the normal is
///   rebuilt from them;
/// * the `0 <= time <= 1` test runs on whichever time was written last,
///   which is the side time when the side solve ran and the cap time when it did not;
/// * the tail builds the offset as `curr + disp - sphere.center` (the subtraction
///   there takes the sphere argument, while the two that build the normal take the cylinder), and
///   the normal is then divided by `r`.
///
/// The shape of it: try the **cap** first — the time at which the sphere's centre reaches the
/// plane of whichever cap it is heading for — and if the sphere is radially outside the cylinder
/// at that time, solve the 2-D quadratic for the **side** instead. `normal_of_collision`'s flag
/// selects between "try the cap, fall back to the side" (flag 0) and "the normal already says
/// which" (flag 1).
///
/// Note the client reads `global_curr_center[0]` for the movement and the cylinder-relative
/// position even when `index` is 1; only `normal_of_collision` is asked about `index`.
#[allow(clippy::too_many_lines)]
pub fn cyl_collide_with_point(
    t: &mut Transition,
    cyl: &CylSphere,
    sphere: &Sphere,
    delta: Vec3,
    sum: f32,
    index: usize,
) -> TransitionState {
    let (radial, mut n) = cyl.normal_of_collision_full(
        t.sphere_path.global_curr_center[index],
        delta,
        sphere.radius,
        sum,
    );
    if n.normalize_check_small() {
        return TransitionState::Collided;
    }
    if !t.object_info.has(ObjectInfoState::PERFECT_CLIP) {
        t.collision_info.set_collision_normal(n);
        return TransitionState::Collided;
    }

    // ---- UNVERIFIED past here: the `PERFECT_CLIP` path. ----
    let curr = t.sphere_path.global_curr_center[0];
    let block =
        landdefs::get_block_offset(t.sphere_path.curr_pos.cell, t.sphere_path.check_pos.cell);
    // The mover's displacement over this sub-step, in the cylinder's block space.
    let movement = block.add(sphere.center.sub(curr));
    // Where the mover started, relative to the cylinder's low point.
    let d = curr.sub(cyl.low_pt);
    // The swept sum, which is the overlap sum plus the epsilon.
    let r = sum + EPSILON;

    let cap_time = |up: bool| {
        if up {
            -((d.z + sphere.radius) / movement.z)
        } else {
            ((sphere.radius + cyl.height) - d.z) / movement.z
        }
    };

    let disp = if radial && n.z == 0.0 {
        // The normal is horizontal, so the client goes straight to the side quadratic.
        let a = movement.x * movement.x + movement.y * movement.y;
        let b = -(movement.x * d.x + movement.y * d.y);
        let disc = b * b - ((d.x * d.x + d.y * d.y) - r * r) * a;
        if disc < 0.0 || a < EPSILON {
            return TransitionState::Collided;
        }
        let q = disc.sqrt();
        let root = if b - q < 0.0 { q + b } else { b - q };
        let time = root / a;
        let disp = movement.mul(time);
        if !(0.0..=1.0).contains(&time) {
            return TransitionState::Collided;
        }
        n = Vec3::new(
            curr.x + disp.x - cyl.low_pt.x,
            curr.y + disp.y - cyl.low_pt.y,
            0.0,
        )
        .mul(1.0 / r);
        disp
    } else if radial {
        // The normal is one of the caps; the time is the cap crossing, with no side fallback.
        if movement.z.abs() < EPSILON {
            return TransitionState::Collided;
        }
        let time = cap_time(movement.z > 0.0);
        if !(0.0..=1.0).contains(&time) {
            return TransitionState::Collided;
        }
        movement.mul(time)
    } else {
        // Flag 0: radially outside, vertically clear of the cap. Try the cap the mover is
        // heading for, and fall back to the side if the sphere is outside the radius when it
        // gets there.
        if movement.z.abs() < EPSILON {
            return TransitionState::Collided;
        }
        let up = movement.z > 0.0;
        let mut time = cap_time(up);
        n = Vec3::new(0.0, 0.0, if up { -1.0 } else { 1.0 });
        let mut disp = movement.mul(time);
        if r * r <= (disp.x + d.x) * (disp.x + d.x) + (disp.y + d.y) * (disp.y + d.y) {
            let a = movement.x * movement.x + movement.y * movement.y;
            if a.abs() < EPSILON {
                return TransitionState::Collided;
            }
            let b = -(movement.x * d.x + movement.y * d.y);
            let disc = b * b - ((d.x * d.x + d.y * d.y) - r * r) * a;
            if disc >= 0.0 && a > EPSILON {
                // A failed discriminant is **not** a return here: the cap time and displacement
                // stand and only the normal is rebuilt from them.
                let q = disc.sqrt();
                let root = if b - q < 0.0 { q + b } else { b - q };
                time = root / a;
                disp = movement.mul(time);
            }
            n = Vec3::new(
                curr.x + disp.x - cyl.low_pt.x,
                curr.y + disp.y - cyl.low_pt.y,
                0.0,
            )
            .mul(1.0 / r);
        }
        if !(0.0..=1.0).contains(&time) {
            return TransitionState::Collided;
        }
        disp
    };

    let offset = curr.add(disp).sub(sphere.center);
    t.collision_info.set_collision_normal(n);
    t.sphere_path.add_offset_to_check_pos(offset);
    TransitionState::Adjusted
}

/// The dispatcher, with the cylinder already in the
/// mover's block space.
///
/// `sum` is `(cyl.radius - 0.0002) + sphere.radius`: the overlap form, built once
/// at the top and handed to every arm.
pub fn cyl_intersects_sphere(
    ctx: &TransitionCtx<'_>,
    t: &mut Transition,
    cyl: &CylSphere,
) -> TransitionState {
    let s0 = t.sphere_path.global_sphere[0];
    let delta = s0.center.sub(cyl.low_pt);
    let sum = (cyl.radius - EPSILON) + s0.radius;

    // The two "does anything touch at all" branches share a tail in the client, and it is the
    // same tail: sphere 0, then sphere 1 if there is one.
    let touches_either = |t: &Transition| {
        if cyl.collides_with_sphere(&s0, delta, sum) {
            return true;
        }
        if t.sphere_path.num_sphere < 2 {
            return false;
        }
        let s1 = t.sphere_path.global_sphere[1];
        cyl.collides_with_sphere(&s1, s1.center.sub(cyl.low_pt), sum)
    };

    if t.sphere_path.insert_type == InsertType::Placement || t.sphere_path.obstruction_ethereal {
        return if touches_either(t) {
            TransitionState::Collided
        } else {
            TransitionState::Ok
        };
    }

    if t.sphere_path.step_down {
        return step_cyl_sphere_down(t, cyl, &s0, delta, sum);
    }

    if t.sphere_path.check_walkable {
        return if touches_either(t) {
            TransitionState::Collided
        } else {
            TransitionState::Ok
        };
    }

    if t.sphere_path.collide {
        // The re-validation after a landing: find the moment the sphere's underside reaches the
        // top cap and put it there, exactly as the sphere path's `check_walkable` branch does
        // with its quadratic.
        if !touches_either(t) {
            return TransitionState::Ok;
        }
        // -(movement): the client subtracts the block offset from
        // `global_curr_center[0] - global_sphere[0].center` rather than adding it to the
        // movement, which is the same vector negated.
        let back = t.sphere_path.global_curr_center[0]
            .sub(s0.center)
            .sub(t.sphere_path.curr_to_check_block_offset());
        if back.z.abs() < EPSILON {
            return TransitionState::Collided;
        }
        // The quotient is *not* written back into `back.z`, which would
        // square the Z term when `back` is scaled by it. The division only reads `back.z`, and the
        // quotient is kept separately for the `walk_interp` arithmetic. So the offset is
        // `back * time`, whose Z is exactly `(cyl.height + s0.radius) - delta.z` — the same lift
        // applies, and the only value that puts the sphere's
        // underside on the cap.
        let time = ((cyl.height + s0.radius) - delta.z) / back.z;
        let offset = back.mul(time);
        if sum * sum
            < (offset.x + delta.x) * (offset.x + delta.x)
                + (offset.y + delta.y) * (offset.y + delta.y)
        {
            // It leaves the cap sideways before it gets there: not a landing.
            return TransitionState::Ok;
        }
        let interp = (1.0 - time) * t.sphere_path.walk_interp;
        if t.sphere_path.walk_interp <= interp || interp < WALK_INTERP_FLOOR_STEP {
            return TransitionState::Collided;
        }
        let c = s0.center.add(offset);
        let plane = Plane::from_normal_and_point(
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(c.x, c.y, c.z - s0.radius),
        );
        t.collision_info.set_contact_plane(plane, true);
        t.collision_info.contact_plane_cell_id = t.sphere_path.check_pos.cell;
        t.sphere_path.walk_interp = interp;
        t.sphere_path.add_offset_to_check_pos(offset);
        return TransitionState::Adjusted;
    }

    if t.object_info.state & (ObjectInfoState::CONTACT | ObjectInfoState::ON_WALKABLE) != 0 {
        if cyl.collides_with_sphere(&s0, delta, sum) {
            return step_cyl_sphere_up(ctx, t, cyl, &s0, delta, sum);
        }
        if t.sphere_path.num_sphere < 2 {
            return TransitionState::Ok;
        }
        let s1 = t.sphere_path.global_sphere[1];
        let d1 = s1.center.sub(cyl.low_pt);
        if !cyl.collides_with_sphere(&s1, d1, sum) {
            return TransitionState::Ok;
        }
        return slide_cyl_sphere(t, cyl, &s1, d1, sum, 1);
    }

    if t.object_info.has(ObjectInfoState::PATH_CLIPPED) {
        // A missile: one sphere only, and no second-sphere fallback.
        if !cyl.collides_with_sphere(&s0, delta, sum) {
            return TransitionState::Ok;
        }
        return cyl_collide_with_point(t, cyl, &s0, delta, sum, 0);
    }

    if cyl.collides_with_sphere(&s0, delta, sum) {
        return land_on_cylinder(t, cyl, &s0, delta, sum);
    }
    if t.sphere_path.num_sphere < 2 {
        return TransitionState::Ok;
    }
    let s1 = t.sphere_path.global_sphere[1];
    let d1 = s1.center.sub(cyl.low_pt);
    if !cyl.collides_with_sphere(&s1, d1, sum) {
        return TransitionState::Ok;
    }
    cyl_collide_with_point(t, cyl, &s1, d1, sum, 1)
}

/// Scale the cylinder, take it into the mover's
/// block space, then dispatch.
///
/// Point 1 of the module note: the call is the client's and
/// is load-bearing for [`step_cyl_sphere_up`]. Its scale argument is the literal `1.0`, **not**
/// the object's scale — the cylinder is scaled explicitly instead.
pub fn cyl_intersects_sphere_scaled(
    ctx: &TransitionCtx<'_>,
    t: &mut Transition,
    cyl: &CylSphere,
    pos: &dereth_primitives::Position,
    scale: f32,
) -> TransitionState {
    t.sphere_path.cache_localspace_sphere(pos, 1.0);
    // Transform `scale * low_pt` from `pos` into `check_pos`'s block: rotate, translate, then add
    // the block offset.
    let m = crate::math::l2g(pos.frame.rotation);
    let off = landdefs::get_block_offset(t.sphere_path.check_pos.cell, pos.cell);
    let low_pt = crate::math::localtoglobalvec(m, cyl.low_pt.mul(scale))
        .add(pos.frame.origin)
        .add(off);
    let placed = CylSphere {
        low_pt,
        height: cyl.height * scale,
        radius: cyl.radius * scale,
    };
    cyl_intersects_sphere(ctx, t, &placed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transition::spherepath::InsertType;
    use dereth_primitives::{Frame, LandblockId, Position, Quat};

    // Oracle: the client's own cylinder-sphere code -- step_sphere_down, normal_of_collision,
    // collide_with_point, slide_sphere, step_sphere_up, land_on_cylinder and both
    // intersects_sphere wrappers.

    /// A pillar 2 m tall and 0.6 m in radius standing at the origin of the test cell.
    const PILLAR: CylSphere = CylSphere {
        low_pt: Vec3::new(0.0, 0.0, 0.0),
        height: 2.0,
        radius: 0.6,
    };

    fn cell() -> dereth_primitives::CellId {
        LandblockId::new(0xA9, 0xB4).cell(1)
    }

    /// A mover with one 0.5 m sphere at local `(0, 0, 0.5)`, standing at `at` and having moved
    /// there from `from`.
    fn mover(from: Vec3, at: Vec3) -> Transition {
        let mut t = Transition::default();
        t.sphere_path
            .init_sphere(&[Sphere::new(Vec3::new(0.0, 0.0, 0.5), 0.5)], 1.0);
        let start = Position::new(cell(), Frame::new(from, Quat::IDENTITY));
        let here = Position::new(cell(), Frame::new(at, Quat::IDENTITY));
        t.sphere_path.init_path(Some(cell()), Some(start), &here);
        t.sphere_path.curr_pos = start;
        t.sphere_path.set_check_pos(&here, Some(cell()));
        t.sphere_path.cache_global_curr_center();
        t.object_info.step_up_height = 0.3;
        t
    }

    /// The overlap sum the dispatcher builds.
    fn sum(cyl: &CylSphere, r: f32) -> f32 {
        (cyl.radius - EPSILON) + r
    }

    #[test]
    fn the_cylinder_step_down_lifts_the_sphere_exactly_onto_the_cap() {
        // drops the check position by `step_down_amt` and
        // then probes, so the sphere arrives **below** where it should rest and the lift is
        // positive. Centre 2.2 m above the low point, underside 0.3 m below the 2 m cap:
        // lift = (height + radius) - delta.z = 0.3, which is half the 0.6 m probe.
        let mut t = mover(Vec3::new(0.0, 0.0, 2.3), Vec3::new(0.0, 0.0, 1.7));
        t.sphere_path.step_down = true;
        t.sphere_path.step_down_amt = 0.6;
        t.sphere_path.walk_interp = 1.0;
        let s0 = t.sphere_path.global_sphere[0];
        let delta = s0.center.sub(PILLAR.low_pt);
        assert!((delta.z - 2.2).abs() < 1e-6, "{delta:?}");
        let before = t.sphere_path.check_pos.frame.origin.z;
        let r = step_cyl_sphere_down(&mut t, &PILLAR, &s0, delta, sum(&PILLAR, s0.radius));
        assert_eq!(r, TransitionState::Adjusted);
        let after = t.sphere_path.check_pos.frame.origin.z;
        assert!(
            (after - before - 0.3).abs() < 1e-5,
            "lifted by {}",
            after - before
        );
        // The contact plane is the flat cap: +Z through the sphere's new underside, z = 2.0.
        assert!(t.collision_info.contact_plane_valid);
        assert_eq!(
            t.collision_info.contact_plane.normal,
            Vec3::new(0.0, 0.0, 1.0)
        );
        assert!((t.collision_info.contact_plane.d + 2.0).abs() < 1e-5);
        assert!(
            (t.sphere_path.walk_interp - 0.5).abs() < 1e-5,
            "{}",
            t.sphere_path.walk_interp
        );
    }

    #[test]
    fn the_cylinder_step_down_refuses_a_stationary_probe() {
        let mut t = mover(Vec3::new(0.0, 0.0, 2.3), Vec3::new(0.0, 0.0, 1.7));
        t.sphere_path.step_down_amt = 0.0; // |step_down_amt * walk_interp| < 0.0002
        t.sphere_path.walk_interp = 1.0;
        let s0 = t.sphere_path.global_sphere[0];
        let delta = s0.center.sub(PILLAR.low_pt);
        assert_eq!(
            step_cyl_sphere_down(&mut t, &PILLAR, &s0, delta, sum(&PILLAR, s0.radius)),
            TransitionState::Collided
        );
    }

    #[test]
    fn the_cylinder_step_down_is_silent_when_neither_sphere_touches() {
        // Five metres to the side of the column: nothing to step down onto.
        let mut t = mover(Vec3::new(5.0, 0.0, 2.3), Vec3::new(5.0, 0.0, 1.7));
        t.sphere_path.step_down_amt = 0.6;
        t.sphere_path.walk_interp = 1.0;
        let s0 = t.sphere_path.global_sphere[0];
        let delta = s0.center.sub(PILLAR.low_pt);
        assert_eq!(
            step_cyl_sphere_down(&mut t, &PILLAR, &s0, delta, sum(&PILLAR, s0.radius)),
            TransitionState::Ok
        );
        assert!(!t.collision_info.contact_plane_valid);
    }

    #[test]
    fn landing_on_a_cylinder_arms_the_revalidation_and_the_landing_allowance() {
        // Directly over the axis and inside the radius: the normal is the +Z cap normal.
        let mut t = mover(Vec3::new(0.0, 0.0, 2.4), Vec3::new(0.0, 0.0, 1.9));
        let s0 = t.sphere_path.global_sphere[0];
        let delta = s0.center.sub(PILLAR.low_pt);
        let r = land_on_cylinder(&mut t, &PILLAR, &s0, delta, sum(&PILLAR, s0.radius));
        assert_eq!(r, TransitionState::Adjusted);
        assert!(
            t.sphere_path.collide,
            "setting sphere-path collision arms re-validation"
        );
        assert_eq!(t.sphere_path.walk_interp, 1.0);
        assert_eq!(t.sphere_path.step_up_normal, Vec3::new(0.0, 0.0, 1.0));
        assert_eq!(t.sphere_path.walkable_allowance, LANDING_Z);
    }

    #[test]
    fn sliding_off_a_cylinder_moves_the_check_position_and_records_the_radial_normal() {
        // Approaching the column from -X **obliquely**, current centre well outside the radius
        // so `normal_of_collision` takes its radial branch. The obliquity matters: a motion
        // exactly along the normal leaves nothing to slide
        // onto and it answers `COLLIDED_TS`, which is the client's behaviour and is asserted
        // separately below.
        let mut t = mover(Vec3::new(-1.6, -0.6, 0.0), Vec3::new(-1.0, 0.0, 0.0));
        t.collision_info.contact_plane_valid = true;
        t.collision_info.contact_plane =
            Plane::from_normal_and_point(Vec3::new(0.0, 0.0, 1.0), Vec3::ZERO);
        let s0 = t.sphere_path.global_sphere[0];
        let delta = s0.center.sub(PILLAR.low_pt);
        let before = t.sphere_path.check_pos.frame.origin;
        let r = slide_cyl_sphere(&mut t, &PILLAR, &s0, delta, sum(&PILLAR, s0.radius), 0);
        assert_eq!(r, TransitionState::Slid);
        assert!(t.collision_info.collision_normal_valid);
        // The normal is horizontal and points from the axis back at the mover.
        let n = t.collision_info.collision_normal;
        assert_eq!(n.z, 0.0, "a cylinder's side normal has no Z: {n:?}");
        assert!(n.x < -0.9 && n.y < 0.0, "{n:?}");
        assert_ne!(
            t.sphere_path.check_pos.frame.origin, before,
            "the slide moved nothing"
        );

        // Straight at the axis: nothing to slide along, so `COLLIDED_TS`.
        let mut t = mover(Vec3::new(-1.6, 0.0, 0.0), Vec3::new(-1.0, 0.0, 0.0));
        t.collision_info.contact_plane_valid = true;
        t.collision_info.contact_plane =
            Plane::from_normal_and_point(Vec3::new(0.0, 0.0, 1.0), Vec3::ZERO);
        let s0 = t.sphere_path.global_sphere[0];
        let delta = s0.center.sub(PILLAR.low_pt);
        assert_eq!(
            slide_cyl_sphere(&mut t, &PILLAR, &s0, delta, sum(&PILLAR, s0.radius), 0),
            TransitionState::Collided
        );
    }

    #[test]
    fn a_missile_against_a_cylinder_records_the_normal_and_collides() {
        // `collide_with_point`'s reachable half: no PERFECT_CLIP, so the normal is recorded and
        // the answer is COLLIDED_TS.
        let mut t = mover(Vec3::new(-1.6, 0.0, 0.0), Vec3::new(-1.0, 0.0, 0.0));
        let s0 = t.sphere_path.global_sphere[0];
        let delta = s0.center.sub(PILLAR.low_pt);
        let before = t.sphere_path.check_pos.frame.origin;
        let r = cyl_collide_with_point(&mut t, &PILLAR, &s0, delta, sum(&PILLAR, s0.radius), 0);
        assert_eq!(r, TransitionState::Collided);
        assert!(t.collision_info.collision_normal_valid);
        assert_eq!(t.collision_info.collision_normal.z, 0.0);
        assert_eq!(
            t.sphere_path.check_pos.frame.origin, before,
            "without PERFECT_CLIP nothing is moved"
        );
    }

    #[test]
    fn a_step_too_high_for_the_cap_slides_instead_of_stepping() {
        // The cap is 2 m up and `step_up_height` is 0.3, so `(radius + height) - delta.z` is far
        // over it and the client slides rather than calling. Reaching
        // `step_up` would need a `TransitionCtx`; taking this branch is what proves the test.
        let mut t = mover(Vec3::new(-1.6, -0.6, 0.0), Vec3::new(-1.0, 0.0, 0.0));
        t.collision_info.contact_plane_valid = true;
        t.collision_info.contact_plane =
            Plane::from_normal_and_point(Vec3::new(0.0, 0.0, 1.0), Vec3::ZERO);
        let s0 = t.sphere_path.global_sphere[0];
        let delta = s0.center.sub(PILLAR.low_pt);
        assert!(
            t.object_info.step_up_height < (s0.radius + PILLAR.height) - delta.z,
            "the fixture must be a step the mover cannot take"
        );
        let expect = {
            let mut c = mover(Vec3::new(-1.6, -0.6, 0.0), Vec3::new(-1.0, 0.0, 0.0));
            c.collision_info.contact_plane_valid = true;
            c.collision_info.contact_plane =
                Plane::from_normal_and_point(Vec3::new(0.0, 0.0, 1.0), Vec3::ZERO);
            let r = slide_cyl_sphere(&mut c, &PILLAR, &s0, delta, sum(&PILLAR, s0.radius), 0);
            (r, c.sphere_path.check_pos.frame.origin)
        };
        // No context is needed because the height test short-circuits before `step_up`.
        let ctx = crate::transition::TransitionCtx {
            land: &crate::source::StaticLandSource::linear(),
            objects: &crate::arena::Arena::new(),
            cells: &std::collections::BTreeMap::new(),
            mover: None,
            object_table: None,
            entry_host: None,
        };
        let r = step_cyl_sphere_up(&ctx, &mut t, &PILLAR, &s0, delta, sum(&PILLAR, s0.radius));
        assert_eq!((r, t.sphere_path.check_pos.frame.origin), expect);
    }

    #[test]
    fn the_placement_branch_is_a_bare_overlap_test_in_both_directions() {
        let ctx = crate::transition::TransitionCtx {
            land: &crate::source::StaticLandSource::linear(),
            objects: &crate::arena::Arena::new(),
            cells: &std::collections::BTreeMap::new(),
            mover: None,
            object_table: None,
            entry_host: None,
        };
        // Inside the column.
        let mut t = mover(Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 0.0));
        t.sphere_path.insert_type = InsertType::Placement;
        assert_eq!(
            cyl_intersects_sphere(&ctx, &mut t, &PILLAR),
            TransitionState::Collided
        );
        // Five metres away.
        let mut t = mover(Vec3::new(5.0, 0.0, 0.0), Vec3::new(5.0, 0.0, 0.0));
        t.sphere_path.insert_type = InsertType::Placement;
        assert_eq!(
            cyl_intersects_sphere(&ctx, &mut t, &PILLAR),
            TransitionState::Ok
        );
    }

    #[test]
    fn the_scaling_wrapper_places_the_cylinder_and_caches_the_localspace_sphere() {
        let ctx = crate::transition::TransitionCtx {
            land: &crate::source::StaticLandSource::linear(),
            objects: &crate::arena::Arena::new(),
            cells: &std::collections::BTreeMap::new(),
            mover: None,
            object_table: None,
            entry_host: None,
        };
        // The obstacle stands at (10, 0, 0); the mover is at (10.0, 0, 0), i.e. on its axis.
        let obstacle = Position::new(
            cell(),
            Frame::new(Vec3::new(10.0, 0.0, 0.0), Quat::IDENTITY),
        );
        let mut t = mover(Vec3::new(10.0, 0.0, 0.0), Vec3::new(10.0, 0.0, 0.0));
        t.sphere_path.insert_type = InsertType::Placement;
        assert_eq!(
            cyl_intersects_sphere_scaled(&ctx, &mut t, &PILLAR, &obstacle, 1.0),
            TransitionState::Collided,
            "the cylinder has to be transformed to the obstacle's position"
        );
        // Point 1 of the module note: the wrapper caches the localspace sphere against the
        // obstacle's frame, which is what `step_sphere_up` reads back.
        assert_eq!(
            t.sphere_path.localspace_pos.frame.origin,
            Vec3::new(10.0, 0.0, 0.0)
        );
        assert_eq!(
            t.sphere_path.walkable_scale, 1.0,
            "the wrapper's scale argument is a literal 1"
        );

        // A tenth-scale column at the same place is 0.06 m in radius and 0.2 m tall; a mover 0.9 m
        // to the side of it clears it, where the full-size one would not.
        let mut t = mover(Vec3::new(9.1, 0.0, 0.0), Vec3::new(9.1, 0.0, 0.0));
        t.sphere_path.insert_type = InsertType::Placement;
        assert_eq!(
            cyl_intersects_sphere_scaled(&ctx, &mut t, &PILLAR, &obstacle, 0.1),
            TransitionState::Ok
        );
        let mut t = mover(Vec3::new(9.1, 0.0, 0.0), Vec3::new(9.1, 0.0, 0.0));
        t.sphere_path.insert_type = InsertType::Placement;
        assert_eq!(
            cyl_intersects_sphere_scaled(&ctx, &mut t, &PILLAR, &obstacle, 1.0),
            TransitionState::Collided
        );
    }

    /// The cameras perfect clip sweep stops at the exact time of impact.
    #[test]
    fn the_cameras_perfect_clip_sweep_stops_at_the_exact_time_of_impact() {
        let ctx = crate::transition::TransitionCtx {
            land: &crate::source::StaticLandSource::linear(),
            objects: &crate::arena::Arena::new(),
            cells: &std::collections::BTreeMap::new(),
            mover: None,
            object_table: None,
            entry_host: None,
        };
        let mut t = mover(Vec3::new(-2.0, 0.0, 0.0), Vec3::new(-1.0, 0.0, 0.0));
        t.object_info.state = crate::globals::VIEWER_OBJECT_INFO_STATE;
        assert!(t.object_info.has(ObjectInfoState::PERFECT_CLIP));
        assert!(t.object_info.has(ObjectInfoState::PATH_CLIPPED));
        assert_eq!(
            cyl_intersects_sphere(&ctx, &mut t, &PILLAR),
            TransitionState::Adjusted,
            "the camera is placed at the moment of impact, not merely told it collided"
        );
        let x = t.sphere_path.check_pos.frame.origin.x;
        assert!(
            (x - -1.1).abs() < 1e-5,
            "the viewer sphere ended at x = {x}, wanted -1.1"
        );
        assert!(t.collision_info.collision_normal_valid);
        let n = t.collision_info.collision_normal;
        assert!(
            (n.x - -1.0).abs() < 1e-5 && n.y == 0.0 && n.z == 0.0,
            "{n:?}"
        );
    }

    /// The cap half of the same branch: `normal_of_collision` answers with a `Z` normal because
    /// the viewer is over the column, so the client takes the cap crossing and never touches the
    /// side quadratic.
    ///
    /// By hand: the centre starts 2.9 m above the low point and moves down 0.5 m over the
    /// sub-step; the cap contact is at `radius + height = 2.5`, so `time = (2.5 - 2.9) / -0.5 =
    /// 0.8`, the displacement is `-0.4`, and the check position is pushed **up** by 0.1 to leave
    /// the sphere resting on the cap.
    #[test]
    fn the_cameras_perfect_clip_sweep_lands_on_the_cap_when_it_comes_down_the_axis() {
        let ctx = crate::transition::TransitionCtx {
            land: &crate::source::StaticLandSource::linear(),
            objects: &crate::arena::Arena::new(),
            cells: &std::collections::BTreeMap::new(),
            mover: None,
            object_table: None,
            entry_host: None,
        };
        let mut t = mover(Vec3::new(0.0, 0.0, 2.4), Vec3::new(0.0, 0.0, 1.9));
        t.object_info.state = crate::globals::VIEWER_OBJECT_INFO_STATE;
        let before = t.sphere_path.check_pos.frame.origin.z;
        assert_eq!(
            cyl_intersects_sphere(&ctx, &mut t, &PILLAR),
            TransitionState::Adjusted
        );
        let after = t.sphere_path.check_pos.frame.origin.z;
        assert!(
            (after - before - 0.1).abs() < 1e-5,
            "moved by {}",
            after - before
        );
        assert!(
            (after + 0.5 - 2.5).abs() < 1e-5,
            "the sphere's centre rests on the cap: {after}"
        );
        assert_eq!(t.collision_info.collision_normal, Vec3::new(0.0, 0.0, 1.0));
    }

    /// The dispatcher's ordinary branch — no contact, not a missile — is `land_on_cylinder`, and
    /// its `PATH_CLIPPED` branch is `collide_with_point`. Both are reached only through
    /// `intersects_sphere`, so the routing is asserted rather than assumed.
    #[test]
    fn the_dispatcher_routes_a_faller_to_landing_and_a_missile_to_the_point_test() {
        let ctx = crate::transition::TransitionCtx {
            land: &crate::source::StaticLandSource::linear(),
            objects: &crate::arena::Arena::new(),
            cells: &std::collections::BTreeMap::new(),
            mover: None,
            object_table: None,
            entry_host: None,
        };
        let mut t = mover(Vec3::new(0.0, 0.0, 2.4), Vec3::new(0.0, 0.0, 1.9));
        assert_eq!(
            cyl_intersects_sphere(&ctx, &mut t, &PILLAR),
            TransitionState::Adjusted
        );
        assert!(t.sphere_path.collide, "the faller landed on the cap");
        assert_eq!(t.sphere_path.walkable_allowance, LANDING_Z);

        let mut t = mover(Vec3::new(-1.6, 0.0, 0.0), Vec3::new(-1.0, 0.0, 0.0));
        t.object_info.state |= ObjectInfoState::PATH_CLIPPED;
        assert_eq!(
            cyl_intersects_sphere(&ctx, &mut t, &PILLAR),
            TransitionState::Collided
        );
        assert!(
            t.collision_info.collision_normal_valid,
            "a missile records where it hit"
        );
        assert!(!t.sphere_path.collide, "a missile does not land");
    }
}
