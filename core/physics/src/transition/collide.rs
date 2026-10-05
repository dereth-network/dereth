//! Collision tests against the world: terrain, BSP geometry and other objects.
//!
//! Covers the geometry tests through the object-versus-object rules, transcribed against the
//! client's own sphere, cylinder-sphere, BSP-tree and cell code.
//!
//! Every function here writes only into the transition; nothing reaches back into the object
//! arena mutably, which is what makes the ten-deep re-entrancy expressible.
//! Velocity cancellation is the one exception and is deferred through
//! [`super::Transition::kill_velocity`].

use dereth_primitives::{CellId, Vec3};

use crate::cell::Cell;
use crate::geom::plane::Plane;
use crate::geom::sphere::Sphere;
use crate::geom::PlaneExt;
use crate::geom::SphereExt;
use crate::globals::{EPSILON, LANDING_Z, WALK_INTERP_FLOOR_STEP};
use crate::land::WaterType;
use crate::landdefs;
use crate::math::V3;
use crate::obj::PhysicsState;
use crate::source::{BuildingGeometry, PhysicsPart};
use crate::transition::objectinfo::ObjectInfoState;
use crate::transition::spherepath::InsertType;
use crate::transition::{Transition, TransitionCtx, TransitionState};

/// The generic slide.
///
/// `sphere` is the **mover's** sphere at the check position and `curr_center` its centre at
/// `curr_pos`; `normal` is the surface normal to slide along. With a zero normal it just halves
/// the remaining offset and returns `ADJUSTED_TS`.
pub fn slide_sphere(
    t: &mut Transition,
    sphere: Sphere,
    normal: Vec3,
    curr_center: Vec3,
) -> TransitionState {
    if normal.x == 0.0 && normal.y == 0.0 && normal.z == 0.0 {
        let v = curr_center.sub(sphere.center).mul(0.5);
        t.sphere_path.add_offset_to_check_pos(v);
        return TransitionState::Adjusted;
    }
    let mut normal = normal;
    t.collision_info.set_collision_normal(normal);
    let d = t
        .sphere_path
        .curr_to_check_block_offset()
        .add(sphere.center.sub(curr_center));
    let plane = if t.collision_info.contact_plane_valid {
        t.collision_info.contact_plane
    } else {
        t.collision_info.last_known_contact_plane
    };
    let axis = normal.cross(plane.normal);
    let m2 = axis.mag2();
    if m2 >= EPSILON {
        let proj = axis.mul(axis.dot(d) / m2);
        // **Both `0.0002` guards in this sphere-slide function are verified against retail**, the
        // whole function re-derived line for line.
        //
        // This branch is live, not unreachable: over `dereth-client`'s `interiors` walks (both
        // the 0-degree and the 1-degree pass) it is taken about 7,000 times. A mutation of this
        // line that survives the tests points at a weak test, not dead code, and this is the one
        // place in the chain that returns `COLLIDED_TS`.
        if proj.mag2() < EPSILON {
            return TransitionState::Collided;
        }
        t.sphere_path.add_offset_to_check_pos(proj.sub(d));
        TransitionState::Slid
    } else if plane.normal.dot(normal) < 0.0 {
        let mut n = d.negate();
        if !n.normalize_check_small() {
            t.collision_info.set_collision_normal(n);
        }
        normal = n;
        let _ = normal;
        TransitionState::Collided
    } else {
        let offset = normal.mul(-d.dot(normal));
        t.sphere_path.add_offset_to_check_pos(offset);
        TransitionState::Slid
    }
}

/// The contact-plane variant used when a step is too high.
///
/// Differs from [`slide_sphere`] in where the normal comes from (the centre delta rather than an
/// argument) and in the degenerate branch, which is a hard `COLLIDED_TS` on an into-plane
/// requested direction and never touches the collision normal.
pub fn slide_sphere_contact_plane(
    t: &mut Transition,
    obstacle_center: Vec3,
    requested_dir: Vec3,
    index: usize,
) -> TransitionState {
    let mut n = t.sphere_path.global_curr_center[index].sub(obstacle_center);
    if n.normalize_check_small() {
        return TransitionState::Collided;
    }
    t.collision_info.set_collision_normal(n);
    let plane = if t.collision_info.contact_plane_valid {
        t.collision_info.contact_plane
    } else {
        t.collision_info.last_known_contact_plane
    };
    let axis = n.cross(plane.normal);
    let d = t.sphere_path.curr_to_check_block_offset().add(
        t.sphere_path.global_sphere[index]
            .center
            .sub(t.sphere_path.global_curr_center[index]),
    );
    let m2 = axis.mag2();
    if m2 >= EPSILON {
        let proj = axis.mul(axis.dot(d) / m2);
        if proj.mag2() < EPSILON {
            return TransitionState::Collided;
        }
        t.sphere_path.add_offset_to_check_pos(proj.sub(d));
        TransitionState::Slid
    } else {
        if plane.normal.dot(requested_dir) < 0.0 {
            return TransitionState::Collided;
        }
        let offset = n.negate().mul(d.dot(n));
        t.sphere_path.add_offset_to_check_pos(offset);
        TransitionState::Slid
    }
}

/// Build the normal from the centre delta, then
/// [`slide_sphere`].
pub fn slide_sphere_from_centers(
    t: &mut Transition,
    obstacle_center: Vec3,
    index: usize,
) -> TransitionState {
    let mut n = t.sphere_path.global_curr_center[index].sub(obstacle_center);
    if n.normalize_check_small() {
        return TransitionState::Collided;
    }
    let sphere = t.sphere_path.global_sphere[index];
    let curr = t.sphere_path.global_curr_center[index];
    slide_sphere(t, sphere, n, curr)
}

/// Landing on a sphere.
pub fn land_on_sphere(t: &mut Transition, obstacle_center: Vec3) -> TransitionState {
    let mut n = t.sphere_path.global_curr_center[0].sub(obstacle_center);
    if n.normalize_check_small() {
        return TransitionState::Collided;
    }
    t.sphere_path.set_collide(n);
    t.sphere_path.walkable_allowance = LANDING_Z;
    TransitionState::Adjusted
}

/// Stepping a sphere up.
pub fn step_sphere_up(
    ctx: &TransitionCtx<'_>,
    t: &mut Transition,
    obstacle_center: Vec3,
    delta: Vec3,
    sum: f32,
) -> TransitionState {
    if t.object_info.step_up_height < (sum + EPSILON) - delta.z {
        return slide_sphere_contact_plane(t, obstacle_center, delta, 0);
    }
    let normal = t.sphere_path.global_curr_center[0].sub(obstacle_center);
    if super::walk::step_up(ctx, t, normal) {
        TransitionState::Ok
    } else {
        // The sphere path's step-up slide.
        t.collision_info.contact_plane_valid = false;
        t.collision_info.contact_plane_is_water = false;
        t.sphere_path.step_up = false;
        let sphere = t.sphere_path.global_sphere[0];
        let curr = t.sphere_path.global_curr_center[0];
        let n = t.sphere_path.step_up_normal;
        slide_sphere(t, sphere, n, curr)
    }
}

/// The mover is walking down onto **another object's
/// sphere**, and this puts it on the surface.
///
/// Three details turn on the client mutating this function's fifth argument in place:
///
/// 1. The lift is written back into the delta's Z **before** normalization, then `1/r` is scaled
///    onto all three components. So the contact normal is the direction to where the mover's centre
///    *will be*, whose length is exactly `r`; taking it from the pre-step delta gives a normal
///    longer than 1 and a Z too large, which makes the walkable gate below too permissive.
/// 2. The plane's point is the obstacle's `center + radius * n` — the **obstacle's** centre and
///    the **obstacle's** radius, followed by in-place vector addition. That is the post-step
///    contact point.
///    The sphere-path walkable branch builds the same plane from the
///    *mover's* pre-step centre instead (the mover's `center - radius * N`); the two differ
///    by the step-down lift and the client is not consistent between them.
/// 3. `set_contact_plane`'s second argument is **1** (`true`), not 0; see
///    [`super::CollisionInfo::set_contact_plane`] for what that flag actually decides.
pub fn step_sphere_down(
    t: &mut Transition,
    obstacle: &Sphere,
    delta: Vec3,
    sum: f32,
) -> TransitionState {
    let overlapping = Sphere::collides_with_sphere(delta, sum);
    let second = t.sphere_path.num_sphere > 1 && {
        let d2 = t.sphere_path.global_sphere[1].center.sub(obstacle.center);
        Sphere::collides_with_sphere(d2, sum)
    };
    if !overlapping && !second {
        return TransitionState::Ok;
    }
    let dz = t.sphere_path.step_down_amt * t.sphere_path.walk_interp;
    if dz.abs() < EPSILON {
        return TransitionState::Collided;
    }
    let r = sum + EPSILON;
    let horiz = r * r - (delta.x * delta.x + delta.y * delta.y);
    if horiz < 0.0 {
        return TransitionState::Collided;
    }
    let f = (horiz.sqrt() - delta.z) / dz;
    let interp = (1.0 - f) * t.sphere_path.walk_interp;
    if interp >= t.sphere_path.walk_interp || interp < WALK_INTERP_FLOOR_STEP {
        return TransitionState::Collided;
    }
    // Detail 1: the lift goes into the delta before the normalise, so `|n| == 1` exactly.
    let lift = dz * f;
    let n = Vec3::new(delta.x, delta.y, delta.z + lift).mul(1.0 / r);
    if n.z <= t.sphere_path.walkable_allowance {
        // Not walkable: keep falling.
        return TransitionState::Ok;
    }
    // Detail 2: the obstacle's surface point, not the mover's.
    let contact = Plane::from_normal_and_point(n, obstacle.center.add(n.mul(obstacle.radius)));
    // Detail 3: `1`, and it means "this plane is a local tangent, do not push out against it".
    t.collision_info.set_contact_plane(contact, true);
    t.collision_info.contact_plane_cell_id = t.sphere_path.check_pos.cell;
    t.sphere_path.walk_interp = interp;
    t.sphere_path
        .add_offset_to_check_pos(Vec3::new(0.0, 0.0, lift));
    TransitionState::Adjusted
}

/// Colliding with a point.
pub fn collide_with_point(
    t: &mut Transition,
    obstacle_center: Vec3,
    sum: f32,
    index: usize,
) -> TransitionState {
    let curr = t.sphere_path.global_curr_center[index];
    if !t.object_info.has(ObjectInfoState::PERFECT_CLIP) {
        let mut n = curr.sub(obstacle_center);
        if !n.normalize_check_small() {
            t.collision_info.set_collision_normal(n);
        }
        return TransitionState::Collided;
    }
    // The PerfectClip path, an exact time of impact.
    //
    // **This branch is reachable.** `PERFECT_CLIP (0x40)` has one writer: the camera's viewer
    // object, whose transition is initialized for the player with flags `0x5C`
    // `IS_VIEWER | PATH_CLIPPED | FREE_ROTATE | PERFECT_CLIP`. This crate writes the same
    // constant as `globals::VIEWER_OBJECT_INFO_STATE`, which the camera supplies to the collision
    // transition, so the camera sweep runs this branch.
    let delta = curr.sub(obstacle_center);
    let movement = t
        .sphere_path
        .curr_to_check_block_offset()
        .add(t.sphere_path.global_sphere[index].center.sub(curr));
    let time = Sphere::find_time_of_collision(movement, delta, sum + EPSILON);
    if (EPSILON..=1.0).contains(&time) {
        let moved = movement.mul(time).sub(movement);
        let inv = 1.0 / (sum + EPSILON);
        let contact = t.sphere_path.global_sphere[index]
            .center
            .add(moved)
            .sub(obstacle_center)
            .mul(inv);
        t.collision_info.set_collision_normal(contact);
        t.sphere_path.add_offset_to_check_pos(moved);
        return TransitionState::Adjusted;
    }
    TransitionState::Collided
}

/// The object-versus-object dispatcher.
///
/// `sum` carries the epsilon the branch needs: **`r1 + r2 - 0.0002` for the overlap tests**,
/// with `+ 0.0002` applied inside the swept helpers.
///
/// The two-sphere branch operand order is not yet known; the ordering below follows ACE's
/// `Sphere.intersects_sphere`.
pub fn sphere_intersects_sphere(
    ctx: &TransitionCtx<'_>,
    t: &mut Transition,
    obstacle: &Sphere,
    candidate_is_missile_or_creature: bool,
) -> TransitionState {
    let delta = t.sphere_path.global_sphere[0].center.sub(obstacle.center);
    let sum = obstacle.radius + t.sphere_path.global_sphere[0].radius - EPSILON;

    let second_hits = || {
        t.sphere_path.num_sphere >= 2 && {
            let d = t.sphere_path.global_sphere[1].center.sub(obstacle.center);
            Sphere::collides_with_sphere(d, sum)
        }
    };

    if t.sphere_path.obstruction_ethereal || t.sphere_path.insert_type == InsertType::Placement {
        if Sphere::collides_with_sphere(delta, sum) {
            return TransitionState::Collided;
        }
        if t.sphere_path.num_sphere < 2 {
            return TransitionState::Ok;
        }
        // the second-sphere operand order here follows ACE.
        return if second_hits() {
            TransitionState::Collided
        } else {
            TransitionState::Ok
        };
    }

    if t.sphere_path.step_down {
        return if candidate_is_missile_or_creature {
            TransitionState::Ok
        } else {
            step_sphere_down(t, obstacle, delta, sum)
        };
    }

    if t.sphere_path.check_walkable {
        if !Sphere::collides_with_sphere(delta, sum) && !second_hits() {
            return TransitionState::Ok;
        }
        let m = t.sphere_path.global_curr_center[0]
            .sub(t.sphere_path.global_sphere[0].center)
            .sub(t.sphere_path.curr_to_check_block_offset());
        let m2 = m.mag2();
        if m2 < EPSILON {
            return TransitionState::Collided;
        }
        let b = -m.dot(delta);
        let r = sum + EPSILON;
        let disc = b * b - (delta.mag2() - r * r) * m2;
        if disc < 0.0 {
            return TransitionState::Collided;
        }
        let time = (b + disc.sqrt()) / m2;
        let interp = (1.0 - time) * t.sphere_path.walk_interp;
        if interp >= t.sphere_path.walk_interp || interp < WALK_INTERP_FLOOR_STEP {
            return TransitionState::Collided;
        }
        let n = delta.add(m.mul(time)).mul(1.0 / r);
        if !t.sphere_path.is_walkable_allowable(n.z) {
            return TransitionState::Ok;
        }
        let contact = Plane::from_normal_and_point(
            n,
            t.sphere_path.global_sphere[0]
                .center
                .sub(n.mul(t.sphere_path.global_sphere[0].radius)),
        );
        t.collision_info.set_contact_plane(contact, true);
        t.collision_info.contact_plane_cell_id = t.sphere_path.check_pos.cell;
        t.sphere_path.walk_interp = interp;
        t.sphere_path.add_offset_to_check_pos(m.mul(time));
        return TransitionState::Adjusted;
    }

    if t.object_info.state & (ObjectInfoState::CONTACT | ObjectInfoState::ON_WALKABLE) != 0 {
        if Sphere::collides_with_sphere(delta, sum) {
            return step_sphere_up(ctx, t, obstacle.center, delta, sum);
        }
        if t.sphere_path.num_sphere < 2 {
            return TransitionState::Ok;
        }
        if second_hits() {
            return slide_sphere_from_centers(t, obstacle.center, 1);
        }
        return TransitionState::Ok;
    }

    if t.object_info.has(ObjectInfoState::PATH_CLIPPED) {
        if Sphere::collides_with_sphere(delta, sum) {
            return collide_with_point(t, obstacle.center, sum, 0);
        }
        return TransitionState::Ok;
    }

    if Sphere::collides_with_sphere(delta, sum) {
        return land_on_sphere(t, obstacle.center);
    }
    if second_hits() {
        return collide_with_point(t, obstacle.center, sum, 1);
    }
    TransitionState::Ok
}

/// What turns a terrain triangle into a contact
/// plane.
///
/// The whole test runs along **Z only** (`lift = d / N.z`), which is why terrain contact is
/// resolved as a vertical push and never as a slide; sliding on terrain comes from `adjust_offset`
/// projecting subsequent movement onto the resulting contact plane.
#[allow(clippy::too_many_arguments)]
pub fn validate_walkable(
    t: &mut Transition,
    sphere: &Sphere,
    plane: &Plane,
    is_water: bool,
    water_depth: f32,
    cell_id: CellId,
) -> TransitionState {
    if t.object_info.has(ObjectInfoState::IS_VIEWER) {
        let d = plane.dot_point(sphere.center) - sphere.radius;
        if d <= -EPSILON {
            let move_v = sphere.center.sub(t.sphere_path.global_curr_center[0]);
            let den = move_v.dot(plane.normal);
            let f = d / den;
            let outdoors_begin = t
                .sphere_path
                .begin_pos
                .is_none_or(|p| landdefs::is_outdoors(p.cell));
            if (0.0..=1.0).contains(&f) || outdoors_begin {
                t.sphere_path.add_offset_to_check_pos(move_v.mul(-f));
                t.collision_info.set_collision_normal(plane.normal);
                t.collision_info.collided_with_environment = true;
                return TransitionState::Adjusted;
            }
        }
        return TransitionState::Ok;
    }

    // Normal objects: measure the *bottom* of the sphere against the plane, plus the water depth.
    let low = sphere.center.sub(Vec3::new(0.0, 0.0, sphere.radius));
    let d = plane.dot_point(low) + water_depth;

    if d >= -EPSILON {
        if d > EPSILON {
            return TransitionState::Ok;
        }
        let walkable = crate::is_valid_walkable(plane.normal);
        if t.sphere_path.step_down || !t.object_info.has(ObjectInfoState::ON_WALKABLE) || walkable {
            t.collision_info.set_contact_plane(*plane, is_water);
            t.collision_info.contact_plane_cell_id = cell_id;
        }
        if !t.object_info.has(ObjectInfoState::CONTACT) && !t.sphere_path.step_down {
            t.collision_info.set_collision_normal(plane.normal);
            t.collision_info.collided_with_environment = true;
        }
        return TransitionState::Ok;
    }

    // d < -2e-4: below the surface.
    if t.sphere_path.check_walkable {
        return TransitionState::Collided;
    }
    let lift = d / plane.normal.z;
    let walkable = crate::is_valid_walkable(plane.normal);
    let mut result = TransitionState::Ok;
    if t.sphere_path.step_down || !t.object_info.has(ObjectInfoState::ON_WALKABLE) || walkable {
        t.collision_info.set_contact_plane(*plane, is_water);
        t.collision_info.contact_plane_cell_id = cell_id;
        if t.sphere_path.step_down {
            let denom = t.sphere_path.step_down_amt * t.sphere_path.walk_interp;
            let interp = (1.0 - (-1.0 / denom) * lift) * t.sphere_path.walk_interp;
            if interp >= t.sphere_path.walk_interp || interp < WALK_INTERP_FLOOR_STEP {
                return TransitionState::Collided;
            }
            t.sphere_path.walk_interp = interp;
        }
        t.sphere_path
            .add_offset_to_check_pos(Vec3::new(0.0, 0.0, -lift));
        result = TransitionState::Adjusted;
    }
    if !t.object_info.has(ObjectInfoState::CONTACT) && !t.sphere_path.step_down {
        t.collision_info.set_collision_normal(plane.normal);
        t.collision_info.collided_with_environment = true;
    }
    result
}

/// **The house barrier**, and it is the whole
/// of it.
///
/// ```text
///   no object in the transition's object_info           -> COLLIDED_TS
///   the other object has no weenie                      -> OK_TS
///   bypass = the weenie's CanBypassMoveRestrictions
///   state = object_info.state
///   state & 0x100 (IS_PLAYER) clear                    -> OK_TS
///   restriction_obj == 0                               -> OK_TS
///   bypass set                                         -> OK_TS
///   look the restriction object up by iid; NULL        -> COLLIDED_TS
///   its weenie_obj NULL                                -> COLLIDED_TS
///   its CanMoveInto(mover_weenie) non-zero             -> OK_TS
///   handle_move_restriction(transition)
///   return COLLIDED_TS
/// ```
///
/// Three things in that ladder are load-bearing and easy to get backwards:
///
/// * **A cell's gate keeper is named by iid, not by pointer**, so the object must be *present* —
///   an object lookup that answers NULL is a **closed** gate, not an open one. That is the one arm
///   the rebuild can reach and retail effectively cannot: the shard creates the house object
///   before the player is ever near it. In the measured session, the two villas of landblock
///   `0x9DAF` arrive as `0xF745 CreateObject` at t=4.8 s and t=146.2 s, well before entry.
/// * **`CanBypassMoveRestrictions` is called before the `IS_PLAYER` test** and its answer is
///   tested after `restriction_obj`. The order has no observable consequence and is preserved
///   anyway, because it is free to preserve.
/// * The refusal calls a **virtual** `handle_move_restriction`, and the two overrides differ: a
///   land cell sets an axis-aligned collision normal out of the cell crossed into, while an
///   environment cell returns 1 without a normal.
///   [`Cell::handle_move_restriction`] is that split.
/// * **Only a gate keeper that is found and refuses calls it.** A lookup that answers no object,
///   or an object with no weenie, collides without the handler: no push-out normal, and no
///   refusal record (so no barrier effect, there being no barrier to play it). Calling the
///   handler for those two arms as well would push the mover out where retail does not.
///
/// The contact effect is deliberately not here; see `CollisionInfo::restricted_by`.
pub fn check_entry_restrictions(
    ctx: &TransitionCtx<'_>,
    t: &mut Transition,
    cell: &Cell,
) -> TransitionState {
    let Some(h) = t.object_info.object else {
        return TransitionState::Collided;
    };
    // An object with no weenie is never restricted. Every
    // dat-placed static is one.
    let Some(mover) = ctx.objects.get(h).and_then(|o| o.weenie.as_ref()) else {
        return TransitionState::Ok;
    };
    let exempt = mover.can_bypass;
    if !t.object_info.has(ObjectInfoState::IS_PLAYER) {
        return TransitionState::Ok;
    }
    let Some(restriction) = ctx.restriction_obj(cell.id()) else {
        return TransitionState::Ok;
    };
    if restriction.0 == 0 || exempt {
        return TransitionState::Ok;
    }
    // the gate keeper is looked up by iid every time. A context with no object table
    // does not run the check at all — see `TransitionCtx::object_table`.
    if ctx.object_table.is_none() {
        return TransitionState::Ok;
    }
    let mover_id = ctx
        .objects
        .get(h)
        .map_or(dereth_primitives::ObjectId(0), |o| o.id);
    // A host that answers the question itself replaces the gate keeper's lookup and record;
    // the client installs none.
    let allowed = match ctx.entry_host {
        Some(host) => host.can_move_into(restriction, mover_id),
        None => {
            // a gate keeper that is not there, or has no weenie: closed, without the handler
            let Some(keeper) = ctx
                .get_object_a(restriction)
                .and_then(|o| o.weenie.as_ref())
            else {
                return TransitionState::Collided;
            };
            keeper.can_move_into(mover_id, Some(mover))
        }
    };
    if allowed {
        return TransitionState::Ok;
    }
    if let Some(n) = cell.handle_move_restriction(&t.sphere_path.curr_pos) {
        t.collision_info.set_collision_normal(n);
    }
    t.collision_info.restricted_by = Some(restriction);
    TransitionState::Collided
}

/// The land cell's environment collisions -- terrain and water.
pub fn land_find_env_collisions(t: &mut Transition, cell: &Cell) -> TransitionState {
    let Cell::Land { id, block } = cell else {
        return TransitionState::Ok;
    };
    let off = landdefs::get_block_offset(t.sphere_path.check_pos.cell, *id);
    let low = t.sphere_path.global_low_point.sub(off);
    let Some(poly) = block.find_terrain_poly(id.index(), low) else {
        return TransitionState::Ok;
    };
    let plane = poly.plane;

    // An entirely-water landblock is a hard stop for anything that is not a
    // viewer or a missile. Deep sea is not "swim".
    if cell.block_water_type() == WaterType::EntirelyWater
        && !t.object_info.has(ObjectInfoState::IS_VIEWER)
        && !t.object_info.has(ObjectInfoState::PATH_CLIPPED)
    {
        return TransitionState::Collided;
    }

    let depth = cell.water_depth(low);
    let is_water = cell.water_type() != WaterType::NotWater;
    let mut s = t.sphere_path.global_sphere[0];
    s.center = s.center.sub(off);
    validate_walkable(t, &s, &plane, is_water, depth, *id)
}

/// The environment cell's collisions -- the interior BSP.
pub fn env_find_env_collisions(
    ctx: &TransitionCtx<'_>,
    t: &mut Transition,
    cell: &Cell,
) -> TransitionState {
    let Cell::Env { geom, .. } = cell else {
        return TransitionState::Ok;
    };
    t.counters.env_cells_visited += 1;
    t.sphere_path.obstruction_ethereal = false;
    let Some(bsp) = geom.physics_bsp.as_ref() else {
        t.counters.env_cells_without_bsp += 1;
        return TransitionState::Ok;
    };
    let cell_pos = cell.pos();
    t.sphere_path.cache_localspace_sphere(&cell_pos, 1.0);
    t.counters.env_bsp_walked += 1;
    let result = if t.sphere_path.insert_type == InsertType::InitialPlacement {
        bsp_placement_insert(t, bsp)
    } else {
        bsp_find_collisions(ctx, t, bsp, 1.0)
    };
    if result != TransitionState::Ok && !t.object_info.has(ObjectInfoState::CONTACT) {
        t.collision_info.collided_with_environment = true;
    }
    result
}

/// The BSP tree's collision search.
///
/// the two-sphere branch operand order is not yet known; the flow below follows
/// ACE's `BSPTree.find_collisions`. Every constant (the `0.0871557` landing allowance), branch
/// predicate and callee in it *is* verified against the client.
pub fn bsp_find_collisions(
    ctx: &TransitionCtx<'_>,
    t: &mut Transition,
    tree: &crate::geom::BspTree,
    scale: f32,
) -> TransitionState {
    let local = t.sphere_path.localspace_sphere[0];
    let center = t.sphere_path.localspace_curr_center[0];
    let movement = local.center.sub(center);

    if t.sphere_path.insert_type == InsertType::Placement || t.sphere_path.obstruction_ethereal {
        let clear_cell = if t.sphere_path.bldg_check {
            !t.sphere_path.hits_interior_cell
        } else {
            true
        };
        if tree.sphere_intersects_solid(&local, clear_cell) {
            return TransitionState::Collided;
        }
        // the second-sphere operand order here follows ACE.
        if t.sphere_path.num_sphere > 1
            && tree.sphere_intersects_solid(&t.sphere_path.localspace_sphere[1], clear_cell)
        {
            return TransitionState::Collided;
        }
        return TransitionState::Ok;
    }

    if t.sphere_path.check_walkable {
        let up = t.sphere_path.localspace_z;
        return if tree.hits_walkable(&local, up, t.sphere_path.walkable_allowance) {
            TransitionState::Collided
        } else {
            TransitionState::Ok
        };
    }

    if t.sphere_path.step_down {
        return bsp_step_sphere_down(t, tree, scale);
    }

    if t.sphere_path.collide {
        let mut valid = local;
        let mut hit = None;
        let mut changed = false;
        let mut interp = t.sphere_path.walk_interp;
        let up = t.sphere_path.localspace_z;
        tree.find_walkable(
            &mut valid,
            &mut hit,
            movement,
            up,
            t.sphere_path.walkable_allowance,
            &mut interp,
            &mut changed,
        );
        if changed {
            let Some(poly_idx) = hit else {
                return TransitionState::Ok;
            };
            let poly = tree.polygons[poly_idx as usize].clone();
            let delta = valid.center.sub(local.center).mul(scale);
            let pos = t.sphere_path.localspace_pos;
            let world_delta =
                crate::math::localtoglobalvec(crate::math::l2g(pos.frame.rotation), delta);
            t.sphere_path.walk_interp = interp;
            t.sphere_path.add_offset_to_check_pos(world_delta);
            let mut contact = poly.plane.localtoglobal(&t.sphere_path.check_pos, &pos);
            contact.d *= scale;
            t.collision_info.set_contact_plane(contact, false);
            t.collision_info.contact_plane_cell_id = t.sphere_path.check_pos.cell;
            t.sphere_path.set_walkable(valid, poly, up, pos, scale);
            return TransitionState::Adjusted;
        }
        return TransitionState::Ok;
    }

    // Transform `N` from the sphere path's local position to global space — the tree's polygons are in the
    // part's own space and every normal the client lets out of this function is rotated into the
    // world first. That is verified for the point collision, `slide_sphere`, `step_sphere_up`
    // and the default arm's second sphere.
    let to_world = |n: Vec3, t: &Transition| -> Vec3 {
        let pos = t.sphere_path.localspace_pos;
        crate::math::localtoglobalvec(crate::math::l2g(pos.frame.rotation), n)
    };

    let mut hit_poly = None;
    let mut contact_pt = Vec3::ZERO;
    if t.object_info.has(ObjectInfoState::CONTACT) {
        // Begin the contact case with separate polygon slots for both spheres, each zeroed: the
        // second sphere gets its **own** slot, and the two back-facing arms below read them
        // separately, which is why `poly0` is kept here.
        let hit0 = tree.sphere_intersects_poly(&local, movement, &mut hit_poly, &mut contact_pt);
        if hit0 {
            let n = hit_poly.map_or(Vec3::ZERO, |i| tree.polygons[i as usize].plane.normal);
            // The `CONTACT` arm takes the hit polygon's `plane.normal` and calls the step-up
            // path with `(transition, &N)`. The normal is rotated into world space here before
            // [`step_sphere_up_from_normal`] executes its failure tail. See that function's doc
            // comment for the four-way measurement.
            return step_sphere_up_from_normal(ctx, t, to_world(n, t));
        }
        let poly0 = hit_poly;
        // the second-sphere operand order here follows ACE.
        if t.sphere_path.num_sphere > 1 {
            let s2 = t.sphere_path.localspace_sphere[1];
            let mut poly1 = None;
            if tree.sphere_intersects_poly(&s2, movement, &mut poly1, &mut contact_pt) {
                let n = poly1.map_or(Vec3::ZERO, |i| tree.polygons[i as usize].plane.normal);
                // Rotated into world space here too.
                let n = to_world(n, t);
                let sphere = t.sphere_path.global_sphere[0];
                let curr = t.sphere_path.global_curr_center[0];
                return slide_sphere(t, sphere, n, curr);
            }
            // **The two back-facing arms record the first and second sphere separately.** Neither is reachable with one
            // sphere — the client leaves before both when the sphere count is 1 or less — so a
            // one-sphere body in contact walks past a back-facing overlap in retail too. The
            // two-sphere case is the only writer of the negative-polygon record in the client;
            // without it the later consumer of `neg_poly_hit` could never fire.
            if let Some(i) = poly1 {
                let world_n = to_world(tree.polygons[i as usize].plane.normal, t);
                // `false`: the second sphere's back-face is not a step-up.
                t.sphere_path.set_neg_poly_hit(false, world_n);
                return TransitionState::Ok;
            }
            if let Some(i) = poly0 {
                let world_n = to_world(tree.polygons[i as usize].plane.normal, t);
                // `true`: the first sphere's back-face is handled as a step-up.
                t.sphere_path.set_neg_poly_hit(true, world_n);
                return TransitionState::Ok;
            }
        }
        return TransitionState::Ok;
    }

    if t.object_info.has(ObjectInfoState::PATH_CLIPPED) {
        // The polygon test returns both a boolean and an optional polygon. The
        // gate is `returned || hit_poly.is_some()`, so a **back-facing** overlap clips the path too.
        let hit = tree.sphere_intersects_poly(&local, movement, &mut hit_poly, &mut contact_pt);
        if hit || hit_poly.is_some() {
            let Some(p) = hit_poly else {
                // The client would dereference NULL here; a `true` return that named no polygon
                // cannot happen on retail data and refusing the adjustment is the only answer
                // that cannot move the sphere the original would not have moved.
                return TransitionState::Ok;
            };
            return bsp_collide_with_pt(t, tree, p, contact_pt, scale);
        }
        return TransitionState::Ok;
    }

    // The same `returned || hit_poly.is_some()` gate as the `PATH_CLIPPED` arm above.
    let hit = tree.sphere_intersects_poly(&local, movement, &mut hit_poly, &mut contact_pt);
    if hit || hit_poly.is_some() {
        let n = hit_poly.map_or(Vec3::new(0.0, 0.0, 1.0), |i| {
            tree.polygons[i as usize].plane.normal
        });
        let pos = t.sphere_path.localspace_pos;
        let world_n = crate::math::localtoglobalvec(crate::math::l2g(pos.frame.rotation), n);
        t.sphere_path.set_collide(world_n);
        t.sphere_path.walkable_allowance = LANDING_Z;
        return TransitionState::Adjusted;
    }
    // the second-sphere operand order here follows ACE.
    if t.sphere_path.num_sphere > 1 {
        let s2 = t.sphere_path.localspace_sphere[1];
        // the second sphere reuses the **same** polygon slot, and the gate is again
        // `returned || hit_poly.is_some()`. The first arm above already
        // returned for any non-null slot, so `hit_poly` is `None` on the way in here.
        let hit2 = tree.sphere_intersects_poly(&s2, movement, &mut hit_poly, &mut contact_pt);
        if hit2 || hit_poly.is_some() {
            let n = hit_poly.map_or(Vec3::new(0.0, 0.0, 1.0), |i| {
                tree.polygons[i as usize].plane.normal
            });
            // Transform the vector from local space to global space before applying it; the
            // polygon's normal is **part-local**.
            let world_n = to_world(n, t);
            t.collision_info.set_collision_normal(world_n);
            return TransitionState::Collided;
        }
    }
    TransitionState::Ok
}

/// The `PATH_CLIPPED` arm of [`bsp_find_collisions`],
/// and **the only caller of [`crate::geom::BspTree::adjust_to_plane`]** in the client.
///
/// It must not be replaced by the sphere-point collision routine (a different function from the
/// sphere-versus-sphere dispatcher) with the polygon's *contact point* standing in for an
/// obstacle sphere, and the difference is visible: this arm's `PERFECT_CLIP` branch bisects to the
/// exact time of touch and returns `ADJUSTED_TS` with the sphere placed **against the surface**,
/// so the walk continues; the sphere version returns `COLLIDED_TS`, after which transition
/// validation rolls the whole move back to the previous sub-step.
/// For the camera, whose whole path against a wall is shorter than one `viewer_sphere` radius,
/// "the previous sub-step" is the pivot — so the camera would snap onto the body instead of
/// gliding to the wall, and the smoother would march it back out again.
///
/// State bit `0x40` is the `PERFECT_CLIP` gate on `object_info.state`; the else-arm copies
/// `localspace_sphere[0]`, calls, and on success takes the normal
/// from **the polygon `find_collisions` found**. `adjust_to_plane` walks the tree with
/// its own private polygon slot, so its re-walks cannot change the polygon this function reads.
fn bsp_collide_with_pt(
    t: &mut Transition,
    tree: &crate::geom::BspTree,
    poly_idx: u32,
    contact_pt: Vec3,
    scale: f32,
) -> TransitionState {
    let pos = t.sphere_path.localspace_pos;
    let m = crate::math::l2g(pos.frame.rotation);
    let normal = tree.polygons[poly_idx as usize].plane.normal;
    if !t.object_info.has(ObjectInfoState::PERFECT_CLIP) {
        t.collision_info
            .set_collision_normal(crate::math::localtoglobalvec(m, normal));
        return TransitionState::Collided;
    }
    let local = t.sphere_path.localspace_sphere[0];
    let mut adjusted = local;
    let mut walking = Some(poly_idx);
    let mut cp = contact_pt;
    let cur = t.sphere_path.localspace_curr_center[0];
    if tree.adjust_to_plane(&mut adjusted, cur, &mut walking, &mut cp) {
        t.collision_info
            .set_collision_normal(crate::math::localtoglobalvec(m, normal));
        let delta = adjusted.center.sub(local.center);
        let offset = crate::math::localtoglobalvec(m, delta).mul(scale);
        t.sphere_path.add_offset_to_check_pos(offset);
        return TransitionState::Adjusted;
    }
    TransitionState::Collided
}

/// A step-up driven by a polygon normal rather than a
/// sphere centre delta.
///
/// Three statements in the client:
///
/// ```text
/// n = rotate_to_world(sphere_path.localspace_pos, polygon_normal)
/// if (transition.step_up(n)) return OK_TS
/// return sphere_path.step_up_slide(object_info, collision_info)
/// ```
///
/// The caller rotates the polygon normal from the sphere path's local position into world space
/// with the cached local-to-world rotation, then this attempts transition step-up and returns
/// `OK_TS` on success. Failure falls through to sphere-path sliding using the same rotated
/// normal. Step-up clears contact-plane validity and its water flag, marks the path as stepping
/// up, and stores the normal. It defaults the amount to **0.04** and the allowance to the default
/// step-up height. If `object_info.state & 2` (`ON_WALKABLE`) is set, it uses the walkable
/// landing height and `object_info.step_up_height` instead. It stores the allowance and backs up
/// the checked cell and position. `super::walk::step_up` implements these operations in that
/// order.
///
/// **`ctx` is threaded to reach it**: `env_find_env_collisions`, `find_building_collisions`,
/// `part_find_obj_collisions`, `object_part_find_obj_collisions` and
/// `gfxobj_find_obj_collisions` all take a `TransitionCtx`, and every path into
/// [`bsp_find_collisions`] descends from a holder.
///
/// # Why both statements matter
///
/// * **The rotation.** The tree's polygons are in the part's own space, and the retail
///   training-academy door `0x0200024F` has a **-150 degree** placement rotation about z, so an
///   unrotated normal is 150 degrees wrong on that door alone.
/// * **The step-up call.** Without it only the failure tail runs, so a body can never be lifted
///   onto anything and has to jump. In retail you can walk onto a chair and from there up onto a
///   table, and plain stairs are built the same way. `0x02000120`, a retail static in Holtburg,
///   has a collision half reaching **0.507 m** above its origin, below the **0.600 m**
///   `step_up_height` the player's own setup declares: without the step-up call a body walking
///   at it rises 0.000 m and stops short; with it the body rises onto it.
/// * **Stairs climb in several lifts.** A Holtburg dinnertable's physics mesh top is **0.920 m**
///   above the floor (its setup spheres reach only 0.762 m, while a `HAS_PHYSICS_BSP_PS` static
///   uses the part geometry). A body reaches it in three lifts of **0.525, 0.395 and 0.207 m**:
///   the 0.600 m budget is honoured per step, and the climb is multi-frame stair-stepping, not a
///   budget violation.
///
/// Only the arrangement with neither statement and the one with both exist in the retail client;
/// either one alone measures worse than both, so they must change together.
///
/// # The axis-aligned degeneracy is retail's own
///
/// When the heading is exactly 0, 90, 180 or 270 degrees against an exactly axis-aligned wall on
/// an exactly flat floor, the slide's crease branch cancels **bit for bit**: `axis` is
/// `cross((0,0,1), wall_normal)`, the offset is a multiple of the wall normal, and
/// `dot(axis, offset)` is `k * (-ny*nx + nx*ny)` = exactly 0. The offset collapses, the collision
/// walk returns with its sub-step counter still at zero, the transitional-position search finds
/// nothing, and frame installation accepts the **requested** frame with nothing consulted: the
/// body moves through the wall. Every link is verified; **this is the retail client's own
/// behaviour**, not a rebuild defect. The
/// correct world normal makes the cancellation exact, where a part-local normal made it only
/// approximate and slid the body somewhere wrong, so the correction makes retail's degeneracy
/// *reliable*. Turning the headings a quarter of a degree takes the bodies ending inside solid
/// geometry to **0**, and it stays 0 at 1, 3, 10 and 20 degrees: the composition below puts
/// **no body inside solid geometry at any non-degenerate heading**.
///
/// Tests that walk bodies at exactly axis-aligned headings therefore pin that degeneracy as a
/// declared retail behaviour, not as a regression. A test that judges "the body was blocked" by
/// where it ended, or by straight-line travel, is a proxy that can reward a spuriously stuck body
/// or accept one that walked through and out the far side; judge by whether the body was ever
/// inside the obstacle's own mesh, and by height, so a climb is not confused with a walk-through.
fn step_sphere_up_from_normal(
    ctx: &TransitionCtx<'_>,
    t: &mut Transition,
    normal: Vec3,
) -> TransitionState {
    if super::walk::step_up(ctx, t, normal) {
        return TransitionState::Ok;
    }
    t.sphere_path.step_up_normal = normal;
    // The sphere path's step-up slide, the same tail the sphere's own step-up takes when it
    // fails -- reached unconditionally here, see above.
    t.collision_info.contact_plane_valid = false;
    t.collision_info.contact_plane_is_water = false;
    t.sphere_path.step_up = false;
    let sphere = t.sphere_path.global_sphere[0];
    let curr = t.sphere_path.global_curr_center[0];
    slide_sphere(t, sphere, normal, curr)
}

/// Probe downward along `-localspace_z` scaled by
/// `1 / scale`.
fn bsp_step_sphere_down(
    t: &mut Transition,
    tree: &crate::geom::BspTree,
    scale: f32,
) -> TransitionState {
    let mut valid = t.sphere_path.localspace_sphere[0];
    let up = t.sphere_path.localspace_z;
    let amount = t.sphere_path.step_down_amt * t.sphere_path.walk_interp / scale;
    let movement = up.mul(-amount);
    let mut hit = None;
    let mut changed = false;
    let mut interp = t.sphere_path.walk_interp;
    tree.find_walkable(
        &mut valid,
        &mut hit,
        movement,
        up,
        t.sphere_path.walkable_allowance,
        &mut interp,
        &mut changed,
    );
    if !changed {
        return TransitionState::Ok;
    }
    let Some(poly_idx) = hit else {
        return TransitionState::Ok;
    };
    let poly = tree.polygons[poly_idx as usize].clone();
    let pos = t.sphere_path.localspace_pos;
    let delta = valid
        .center
        .sub(t.sphere_path.localspace_sphere[0].center)
        .mul(scale);
    let world_delta = crate::math::localtoglobalvec(crate::math::l2g(pos.frame.rotation), delta);
    t.sphere_path.walk_interp = interp;
    t.sphere_path.add_offset_to_check_pos(world_delta);
    let mut contact = poly.plane.localtoglobal(&t.sphere_path.check_pos, &pos);
    contact.d *= scale;
    t.collision_info.set_contact_plane(contact, false);
    t.collision_info.contact_plane_cell_id = t.sphere_path.check_pos.cell;
    t.sphere_path.set_walkable(valid, poly, up, pos, scale);
    TransitionState::Adjusted
}

/// At most **20** iterations of "find the solid polygon,
/// push the spheres off it".
///
/// # The two exits
///
/// The loop counter is explicit, and the clear-sphere exit tests it before it decides anything.
/// With no iterations, clear geometry returns `OK_TS`. After at least one adjustment, compute all
/// three components of the displacement from the original local-space sphere center to the
/// working center, transform it through the local-space position, add it to the checked position
/// and return `ADJUSTED_TS` (3).
///
/// Both the one-sphere and two-sphere arms apply this rule.
///
/// **`OK_TS` on the untouched sphere is the whole of the indoor teleport**, because placement
/// accepts only that value. If this returned `ADJUSTED_TS` on every exit, placement would retry
/// once, get `ADJUSTED_TS` again because nothing about the sphere changed, and return 0: every
/// placement into a cell carrying a physics BSP — every interior cell in the game — would fail
/// before commit. The contact plane, `CONTACT_TS`, `ON_WALKABLE_TS`, and the resulting
/// ground-contact edge would keep describing the body's previous location, and without an edge
/// there is no ground-entry or `LeaveGround` callback, the only routes to update that state.
/// Over the real academy cell `0x7F0301AD` and the recorded `early-inventory-and-casting`
/// standing position, that failure mode commits **0 of 2** placements, against 2 of 2 outdoors.
///
/// The original client keeps working spheres on the stack and never copies them back
/// into the sphere path. Only `add_offset_to_check_pos` transfers displacement to
/// world state.
pub fn bsp_placement_insert(t: &mut Transition, tree: &crate::geom::BspTree) -> TransitionState {
    let start = t.sphere_path.localspace_sphere[0];
    let mut a = start;
    let mut b = t.sphere_path.localspace_sphere[1];
    let two = t.sphere_path.num_sphere > 1;
    for iteration in 0..crate::globals::PLACEMENT_INSERT_ITERATIONS {
        let mut center_solid = false;
        let mut hit = None;
        let ra = a.radius;
        let solid = tree.sphere_intersects_solid_poly(&a, ra, &mut center_solid, &mut hit, true);
        if !solid && hit.is_none() {
            if iteration == 0 {
                // the sphere was already clear, so nothing was placed.
                return TransitionState::Ok;
            }
            // the accumulated push-out, taken into the world through
            // `localspace_pos` and handed to the check position.
            let delta = a.center.sub(start.center);
            let pos = t.sphere_path.localspace_pos;
            let world = crate::math::localtoglobalvec(crate::math::l2g(pos.frame.rotation), delta);
            t.sphere_path.add_offset_to_check_pos(world);
            return TransitionState::Adjusted;
        }
        let Some(idx) = hit else {
            return TransitionState::Collided;
        };
        let poly = &tree.polygons[idx as usize];
        if two {
            poly.adjust_to_placement_poly(&mut a, Some(&mut b), ra, center_solid, true);
        } else {
            poly.adjust_to_placement_poly(&mut a, None, ra, center_solid, true);
        }
    }
    TransitionState::Collided
}

/// What a caller of [`find_obj_collisions_geom`] can offer as the candidate's geometry.
///
/// The original client supplies a complete part array. A caller here may have only
/// spheres, as [`find_obj_collisions`] does.
#[derive(Debug, Clone, Copy)]
pub enum ObjGeometry<'a> {
    /// The part's own sphere array, already clamped to what the sphere path's initialisation
    /// would see.
    Spheres(&'a [Sphere]),
    /// The complete setup collision geometry consumed by the three-way test, together
    /// with its current pose: one `Frame` per part.
    ///
    /// `None` for the pose is the client's case of an object with no current animation: the
    /// setup's placement frame, which is right only for a body that has never been animated.
    /// See [`crate::source::SetupGeometry::placed_part_posed`].
    Setup(
        &'a crate::source::SetupGeometry,
        Option<&'a [dereth_primitives::Frame]>,
    ),
}

/// Object collision with only the candidate's **spheres** available.
///
/// It is [`find_obj_collisions_geom`] with
/// [`ObjGeometry::Spheres`], which can only ever take the sphere arm.
#[allow(clippy::too_many_arguments)]
pub fn find_obj_collisions(
    ctx: &TransitionCtx<'_>,
    t: &mut Transition,
    other: crate::arena::PhysHandle,
    other_state: PhysicsState,
    other_spheres: &[Sphere],
    other_scale: f32,
    other_pos: &dereth_primitives::Position,
) -> TransitionState {
    find_obj_collisions_geom(
        ctx,
        t,
        other,
        other_state,
        ObjGeometry::Spheres(other_spheres),
        other_scale,
        other_pos,
    )
}

/// One candidate object's contribution to the
/// mover's transition.
///
/// The player-pair exemption is true only when both objects are
/// players, neither is impenetrable, and they do not share PK or PKLite status. An ordinary NPK
/// pair therefore passes through; those status matches retain collision.
///
/// **The geometry test is a three-way choice**:
///
/// ```text
/// if ((state & HAS_PHYSICS_BSP_PS) == 0) || pass_through || missile_ignore:
///     if the part array has cylspheres and not pass_through:        cylspheres
///     else:                                                        spheres
/// else:
///     every part's BSP
/// ```
///
/// `HAS_PHYSICS_BSP_PS` is the setup geometry's scan of the parts, so
/// the BSP arm is reachable only from an [`ObjGeometry::Setup`] whose parts are modelled; a
/// caller with only spheres keeps the arm it had.
///
/// The middle arm is cylinder collision. Cylinder collision and its five
/// transition helpers live in [`crate::transition::cylinder`]; the arm is taken whenever the
/// candidate has **no** part BSP and the cylinder count is non-zero, which
/// here is an [`ObjGeometry::Setup`] with a non-empty `cyl_spheres`. An empty cylsphere list —
/// which is every [`ObjGeometry::Spheres`] caller and every setup that carries none — takes
/// the sphere loop.
///
/// **Every geometry arm is skipped for a `pass_through` pair.**
#[allow(clippy::too_many_arguments)]
pub fn find_obj_collisions_geom(
    ctx: &TransitionCtx<'_>,
    t: &mut Transition,
    other: crate::arena::PhysHandle,
    other_state: PhysicsState,
    other_geom: ObjGeometry<'_>,
    other_scale: f32,
    other_pos: &dereth_primitives::Position,
) -> TransitionState {
    // 1. Mutual pass-through, then the viewer's creature exemption. These are the two halves of
    // the first guard in object-collision geometry; both return before
    // `obstruction_ethereal`, missile ownership, or any geometry is inspected. The candidate's
    // creature-test answer is taken from its accepted state rather than inferred
    // from its setup or animation.
    let viewer_ignores_creature = t.object_info.has(ObjectInfoState::IS_VIEWER)
        && ctx
            .objects
            .get(other)
            .and_then(|o| o.weenie.as_ref())
            .is_some_and(|w| w.is_creature);
    if (other_state.is_ethereal() && other_state.ignores_collisions()) || viewer_ignores_creature {
        return TransitionState::Ok;
    }

    // 2. Is this pair non-blocking?
    let obstruction_ethereal =
        if other_state.is_ethereal() || (t.object_info.ethereal && !other_state.is_static()) {
            if t.sphere_path.step_down {
                // The client returns here **before** it stores `sphere_path.obstruction_ethereal`:
                // the early `return OK_TS` sits inside the else-branch, two statements ahead of the
                // store, so the flag keeps whatever the previous candidate left it at. Writing `true`
                // here would leak an ethereal obstruction into the *next* cell's BSP search,
                // which takes its solid-test-only branch on that flag.
                return TransitionState::Ok; // an ethereal thing is never ground
            }
            true
        } else {
            false
        };
    t.sphere_path.obstruction_ethereal = obstruction_ethereal;

    // The candidate's own `state & MISSILE_PS`, or its weenie is a creature. Read by the
    // sphere-versus-sphere test and by the recording step.
    let candidate_is_missile_or_creature = other_state.is_missile()
        || ctx
            .objects
            .get(other)
            .and_then(|o| o.weenie.as_ref())
            .is_some_and(|w| w.is_creature);
    // Missile-ignore, three arms. A candidate missile is always ignored. When the mover is a
    // missile and the candidate is not its target, the candidate is ignored when it is ethereal
    // and backed by a weenie, or when the mover has a target and the candidate's weenie is
    // a creature. This crate's `WeenieRestrictions` is exactly the pushed, non-null weenie seam.
    //
    // The target id comes from the body's `projectile_target_id`, which the client never writes:
    // on the client it is always zero, every real object id passes the non-target comparison,
    // and the creature arm cannot fire. It is live only for a host that aims its projectiles.
    let mover_is_missile = t
        .object_info
        .object
        .and_then(|h| ctx.objects.get(h))
        .is_some_and(|o| o.state.is_missile());
    let target_id = t.object_info.target_id;
    let non_target_ignored = ctx.objects.get(other).is_some_and(|o| {
        o.id != target_id
            && o.weenie
                .as_ref()
                .is_some_and(|w| other_state.is_ethereal() || (target_id.0 != 0 && w.is_creature))
    });
    let missile_ignore = other_state.is_missile() || (mover_is_missile && non_target_ignored);

    // Players pass through each other only when neither is impenetrable and the pair
    // is neither mutually PK nor mutually PK-lite. Candidate facts come from its game
    // record; mover facts come from the initialized object-info state.
    let pass_through = ctx
        .objects
        .get(other)
        .and_then(|o| o.weenie.as_ref())
        .is_some_and(|w| {
            w.is_player
                && t.object_info.has(ObjectInfoState::IS_PLAYER)
                && !w.is_impenetrable
                && !t.object_info.has(ObjectInfoState::IS_IMPENETRABLE)
                && !(w.is_pk && t.object_info.has(ObjectInfoState::IS_PK))
                && !(w.is_pk_lite && t.object_info.has(ObjectInfoState::IS_PK_LITE))
        });

    // 4. Run the geometry — the three-way test above.
    let mut r = TransitionState::Ok;
    if pass_through {
        // Pass-through skips BSP, cylinder-sphere and sphere collision tests.
    } else if missile_ignore {
        // Neither geometry arm runs; the result stays `OK_TS`.
        t.counters.objects_missile_ignored += 1;
    } else if other_state.has_physics_bsp() {
        // Every part is tested in order; the first non-OK result wins. A
        // null part is stepped over; here an absent one simply ends the loop.
        if let ObjGeometry::Setup(g, frames) = other_geom {
            for i in 0..g.parts.len() {
                // The pose is the candidate object's current animation frame, which
                // is what part placement consumes.
                let Some(part) = g.placed_part_posed(i, other_pos, other_scale, frames) else {
                    break;
                };
                t.counters.object_parts_visited += 1;
                r = object_part_find_obj_collisions(ctx, t, &part);
                if r != TransitionState::Ok {
                    break;
                }
            }
        }
    } else if let Some(cyls) = cylspheres(other_geom) {
        // A non-zero cylsphere count selects the middle arm: one test per cylsphere, first non-OK
        // wins.
        for c in cyls {
            t.counters.object_cylspheres_tested += 1;
            r = crate::transition::cylinder::cyl_intersects_sphere_scaled(
                ctx,
                t,
                c,
                other_pos,
                other_scale,
            );
            if r != TransitionState::Ok {
                break;
            }
        }
    } else {
        // The mover's spheres are already in check-pos block space; the obstacle's have to be
        // scaled and transformed into the same space.
        let spheres: &[Sphere] = match other_geom {
            ObjGeometry::Spheres(s) => s,
            ObjGeometry::Setup(g, _) => g.path_spheres(),
        };
        let off = landdefs::get_block_offset(t.sphere_path.check_pos.cell, other_pos.cell);
        let m = crate::math::l2g(other_pos.frame.rotation);
        for s in spheres {
            let center = crate::math::localtoglobalvec(m, s.center.mul(other_scale))
                .add(other_pos.frame.origin)
                .add(off);
            let obstacle = Sphere::new(center, s.radius * other_scale);
            r = sphere_intersects_sphere(ctx, t, &obstacle, candidate_is_missile_or_creature);
            if r != TransitionState::Ok {
                break;
            }
        }
    }

    // 5. Record.
    if r != TransitionState::Ok && !t.sphere_path.step_down {
        if other_state.is_static() {
            // A static that is hit reports as *environment* rather than as an object.
            if !t.object_info.has(ObjectInfoState::CONTACT) {
                t.collision_info.collided_with_environment = true;
            }
        } else if obstruction_ethereal
            || (candidate_is_missile_or_creature
                && t.object_info.has(ObjectInfoState::IGNORE_CREATURES))
        {
            r = TransitionState::Ok;
            t.collision_info.collision_normal_valid = false;
            t.collision_info.add_object(other, TransitionState::Ok);
        } else {
            t.collision_info.add_object(other, r);
        }
    }
    t.sphere_path.obstruction_ethereal = false;
    r
}

/// Return cylinder spheres exactly as the geometry arm test needs them: `Some` only when the
/// candidate actually carries cylinder spheres.
///
/// A caller that hands over bare spheres has no part array's cylsphere list to offer, which is
/// an object with no part array in the client and is the same answer.
fn cylspheres(g: ObjGeometry<'_>) -> Option<&[crate::geom::CylSphere]> {
    match g {
        ObjGeometry::Spheres(_) => None,
        ObjGeometry::Setup(s, _) => {
            if s.cyl_spheres.is_empty() {
                None
            } else {
                Some(&s.cyl_spheres)
            }
        }
    }
}

/// Walk the cell's **shadow** list.
///
/// Skipped entirely for [`InsertType::InitialPlacement`], which is why an object being placed for
/// the first time does not push other objects around.
pub fn cell_find_obj_collisions(
    ctx: &TransitionCtx<'_>,
    t: &mut Transition,
    cell_id: CellId,
) -> TransitionState {
    if t.sphere_path.insert_type == InsertType::InitialPlacement {
        t.counters.obj_walk_skipped_initial_placement += 1;
        return TransitionState::Ok;
    }
    let shadows: Vec<crate::arena::PhysHandle> = ctx.shadow_objects(cell_id).to_vec();
    for h in shadows {
        t.counters.shadows_visited += 1;
        // `other != transition.object_info.object` — the client compares against the
        // transition's own object, which initialization set.
        // `TransitionCtx::mover` is that same handle; both are honoured so that a walk driven
        // straight from a test, with no `init_object`, still excludes the mover.
        if Some(h) == ctx.mover || t.object_info.object == Some(h) {
            t.counters.skipped_self += 1;
            continue;
        }
        let Some(o) = ctx.objects.get(h) else {
            t.counters.shadow_dangling += 1;
            continue;
        };
        if o.parent.is_some() {
            t.counters.skipped_parented += 1;
            continue;
        }
        // Geometry dispatch is entered unconditionally; an object with
        // neither spheres nor cylspheres falls through every branch and returns `OK_TS`, so a
        // candidate with no geometry still counts as tested rather than as a skip.
        t.counters.objects_tested += 1;
        // The whole setup collision half, not just the spheres: object collision search chooses
        // between the parts' BSPs and the spheres, and only the geometry knows
        // which. Cloned because `ctx.objects` is borrowed immutably while `t` is borrowed mutably.
        let geometry = std::sync::Arc::clone(&o.geometry);
        // Preserve the pose its parts are in. Part collision reads the part position
        // written by the last animation update, not the setup's placement frame.
        let frames = o.part_frames.clone();
        let (state, scale, pos) = (o.state, o.scale, o.position);
        let r = find_obj_collisions_geom(
            ctx,
            t,
            h,
            state,
            ObjGeometry::Setup(&geometry, frames.as_deref().map(Vec::as_slice)),
            scale,
            &pos,
        );
        if r != TransitionState::Ok {
            return r;
        }
    }
    TransitionState::Ok
}

/// Graphics-object collision: the bounding-sphere prune, then the tree.
///
/// The prune compares the graphics object's drawing sphere — the root node's own sphere — against
/// each of the mover's spheres **in the part's local space**, in the
/// client's slightly permissive `|d|^2 - sum^2 < 0.0002` form (see
/// [`crate::geom::bsp::spheres_intersect`]). The **first** sphere that passes decides: the tree is
/// walked once and its answer returned, and the remaining spheres are never looked at.
fn gfxobj_find_obj_collisions(
    ctx: &TransitionCtx<'_>,
    t: &mut Transition,
    tree: &crate::geom::BspTree,
    physics_sphere: Sphere,
    scale: f32,
    owner: PartOwner,
) -> TransitionState {
    for i in 0..t.sphere_path.num_sphere {
        if !crate::geom::bsp::spheres_intersect(
            &physics_sphere,
            &t.sphere_path.localspace_sphere[i],
        ) {
            continue;
        }
        match owner {
            PartOwner::Building => t.counters.buildings_bsp_walked += 1,
            PartOwner::Object => t.counters.object_bsp_walked += 1,
        }
        return if t.sphere_path.insert_type == InsertType::InitialPlacement {
            bsp_placement_insert(t, tree)
        } else {
            bsp_find_collisions(ctx, t, tree, scale)
        };
    }
    match owner {
        PartOwner::Building => t.counters.buildings_sphere_pruned += 1,
        PartOwner::Object => t.counters.object_sphere_pruned += 1,
    }
    TransitionState::Ok
}

/// Cache the mover's spheres in the part's local space, then test the graphics
/// object's physics BSP.
///
/// If there is no physics BSP, return `OK_TS` **without caching** the local spheres.
/// A later reader therefore sees the previous contents of `localspace_sphere`.
pub fn part_find_obj_collisions(
    ctx: &TransitionCtx<'_>,
    t: &mut Transition,
    part: &PhysicsPart,
) -> TransitionState {
    part_find_obj_collisions_counted(ctx, t, part, PartOwner::Building)
}

/// The same collision walk for an ordinary object's part array.
///
/// The original client shares the part collision implementation for buildings and
/// ordinary objects. Separate entry points here keep the building counters specific
/// to buildings, as asserted by `dereth-client`'s `dereth/client/tests/dat/world/collision_obstacles.rs`.
pub fn object_part_find_obj_collisions(
    ctx: &TransitionCtx<'_>,
    t: &mut Transition,
    part: &PhysicsPart,
) -> TransitionState {
    part_find_obj_collisions_counted(ctx, t, part, PartOwner::Object)
}

/// Which counter set a part walk belongs to. Rebuild-only bookkeeping; the client has one path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PartOwner {
    Building,
    Object,
}

fn part_find_obj_collisions_counted(
    ctx: &TransitionCtx<'_>,
    t: &mut Transition,
    part: &PhysicsPart,
    owner: PartOwner,
) -> TransitionState {
    let no_bsp = |t: &mut Transition| match owner {
        PartOwner::Building => t.counters.building_parts_without_bsp += 1,
        PartOwner::Object => t.counters.object_parts_without_bsp += 1,
    };
    let Some(tree) = part.physics_bsp.clone() else {
        no_bsp(t);
        return TransitionState::Ok;
    };
    let Some(physics_sphere) = part.physics_sphere() else {
        // A tree with no root node has no bounding sphere to prune against. The client would
        // dereference NULL; refusing the walk is the only answer that cannot move an object the
        // original would not have moved. Counted, and asserted to be zero over the retail dat.
        no_bsp(t);
        return TransitionState::Ok;
    };
    t.sphere_path
        .cache_localspace_sphere(&part.pos, part.gfxobj_scale);
    gfxobj_find_obj_collisions(ctx, t, &tree, physics_sphere, part.gfxobj_scale, owner)
}

/// The building's collision search -- the building **shell**.
///
/// Four statements, all load-bearing:
///
/// * a building with no part array is `OK_TS` and `bldg_check` is never touched;
/// * `bldg_check` is raised for the duration, which is what makes [`bsp_find_collisions`]'s
///   placement branch pass `!hits_interior_cell` as its "the whole cell counts as solid" flag
///   instead of an unconditional `true` — an object already inside the building must not be shoved
///   out through its own walls;
/// * **part 0 only.** The object collision path walks every part; this building path does not call
///   it and instead indexes the part array's first part directly;
/// * `collided_with_environment` is set when the part collided **and** the mover is not already in
///   contact (`object_info.state & CONTACT`) — a building is scenery, so hitting it reads as
///   hitting the world rather than as hitting an object.
pub fn find_building_collisions(
    ctx: &TransitionCtx<'_>,
    t: &mut Transition,
    building: &BuildingGeometry,
) -> TransitionState {
    let Some(part) = building.parts.first() else {
        t.counters.buildings_without_parts += 1;
        return TransitionState::Ok;
    };
    t.sphere_path.bldg_check = true;
    let r = part_find_obj_collisions(ctx, t, part);
    t.sphere_path.bldg_check = false;
    if r != TransitionState::Ok && !t.object_info.has(ObjectInfoState::CONTACT) {
        t.collision_info.collided_with_environment = true;
    }
    r
}

/// Search a land cell's building only when one is present. Interior cells have no
/// building entry.
pub fn sort_cell_find_collisions(
    ctx: &TransitionCtx<'_>,
    t: &mut Transition,
    cell_id: CellId,
) -> TransitionState {
    let Some(building) = ctx.land.building(cell_id) else {
        return TransitionState::Ok;
    };
    t.counters.buildings_visited += 1;
    find_building_collisions(ctx, t, &building)
}

/// The per-cell entry point `insert_into_cell` calls: for
/// a land cell and for an interior one.
pub fn cell_find_collisions(
    ctx: &TransitionCtx<'_>,
    t: &mut Transition,
    cell: &Cell,
) -> TransitionState {
    // check_entry_restrictions runs first for both cell kinds, and reads both weenie
    // predicates itself.
    let r = check_entry_restrictions(ctx, t, cell);
    if r != TransitionState::Ok {
        return r;
    }
    let env = match cell {
        Cell::Land { .. } => land_find_env_collisions(t, cell),
        Cell::Env { .. } => env_find_env_collisions(ctx, t, cell),
    };
    if env != TransitionState::Ok {
        return env;
    }
    // Land-cell collision has three stages: environment, building shell, then objects.
    // Interior-cell collision omits the building stage because only land cells carry
    // a building entry.
    if cell.is_land() {
        let r = sort_cell_find_collisions(ctx, t, cell.id());
        if r != TransitionState::Ok {
            return r;
        }
    }
    cell_find_obj_collisions(ctx, t, cell.id())
}

#[cfg(test)]
mod tests;
