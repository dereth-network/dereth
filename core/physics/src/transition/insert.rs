//! `find_transitional_position`, `transitional_insert`, `validate_transition` and the outer loop.
//!
//! The top-level transition algorithms, from the outer loop down to the validation.
//!
//! The attempt budgets are **3 / 5 / 1 exactly** and they are what bounds the
//! work: every `TransitionState` except `Ok` means "retry".

use dereth_primitives::Vec3;

use crate::cell::{Cell, CellArray};
use crate::geom::plane::Plane;
use crate::geom::PlaneExt;
use crate::globals::{
    ATTEMPTS_NORMAL, ATTEMPTS_REVALIDATE, ATTEMPTS_STEP_DOWN, DEFAULT_STEP_HEIGHT, EPSILON,
    EPSILON_SQ, LANDING_Z,
};
use crate::landdefs;
use crate::math::{self, V3};
use crate::obj::PhysicsState;
use crate::transition::collide;
use crate::transition::objectinfo::ObjectInfoState;
use crate::transition::spherepath::InsertType;
use crate::transition::{Transition, TransitionCtx, TransitionState};

/// Split a move into `ceil(distance / radius)` equal
/// sub-steps, the classic "never move more than one radius per test" rule.
///
/// Returns `(total_offset, step_offset, num_steps)`.
#[must_use]
pub fn calc_num_steps(t: &Transition) -> (Vec3, Vec3, u32) {
    let Some(begin) = t.sphere_path.begin_pos.as_ref() else {
        return (Vec3::ZERO, Vec3::ZERO, 1);
    };
    let total = math::get_offset(begin, &t.sphere_path.end_pos);
    let r = t.sphere_path.local_sphere[0].radius;
    let dist = total.mag2().sqrt();

    if t.object_info.has(ObjectInfoState::IS_VIEWER) {
        if dist <= EPSILON {
            return (total, Vec3::ZERO, 0);
        }
        let n = dist / r;
        #[allow(clippy::cast_sign_loss)]
        let steps = (dereth_primitives::num::to_i32(n.floor()) + 1).max(0) as u32;
        return (total, total.mul(1.0 / n), steps);
    }

    let n = dist / (r * 1.0);
    if n >= 1.0 {
        let k = n.ceil();
        #[allow(clippy::cast_sign_loss)]
        let steps = dereth_primitives::num::to_i32(k).max(0) as u32;
        (total, total.mul(1.0 / k), steps)
    } else if total.x == 0.0 && total.y == 0.0 && total.z == 0.0 {
        (total, Vec3::ZERO, 0)
    } else {
        (total, total, 1)
    }
}

/// What makes a character slide along walls and floors
/// instead of stopping.
#[must_use]
pub fn adjust_offset(t: &mut Transition, offset: Vec3) -> Vec3 {
    let mut v = offset;
    let mut sliding = false;
    if t.collision_info.sliding_normal_valid {
        if v.dot(t.collision_info.sliding_normal) < 0.0 {
            sliding = true;
        } else {
            // Moving away from the wall: forget it.
            t.collision_info.sliding_normal_valid = false;
        }
    }

    if !t.collision_info.contact_plane_valid {
        if sliding {
            let n = t.collision_info.sliding_normal;
            v = v.sub(n.mul(v.dot(n)));
        }
    } else {
        let plane = t.collision_info.contact_plane;
        let d = v.dot(plane.normal);
        if sliding {
            // The wall/floor intersection line.
            let mut axis = t.collision_info.sliding_normal.cross(plane.normal);
            if axis.normalize_check_small() {
                // Degenerate crease: stop.
                v = Vec3::ZERO;
            } else {
                v = axis.mul(axis.dot(v));
            }
        } else if d <= 0.0 {
            v = v.sub(plane.normal.mul(d));
        } else {
            plane.snap_to_plane(&mut v);
        }

        // Keep the sphere from sinking into a non-water contact plane.
        if !t.collision_info.contact_plane_is_water && t.collision_info.contact_plane_cell_id.0 != 0
        {
            let off = landdefs::get_block_offset(
                t.sphere_path.check_pos.cell,
                t.collision_info.contact_plane_cell_id,
            );
            let s = t.sphere_path.global_sphere[0];
            let d2 = plane.dot_point(s.center.sub(off));
            if d2 < s.radius - EPSILON {
                let dz = (s.radius - d2) / plane.normal.z;
                if dz.abs() < s.radius {
                    t.sphere_path
                        .add_offset_to_check_pos(Vec3::new(0.0, 0.0, dz));
                }
            }
        }
    }
    v
}

/// Call the cell's collision entry up to `n` times,
/// stopping on `OK_TS` or `COLLIDED_TS` and clearing the contact plane on `SLID_TS`.
pub fn insert_into_cell(
    ctx: &TransitionCtx<'_>,
    t: &mut Transition,
    cell: &Cell,
    attempts: u32,
) -> TransitionState {
    let mut result = TransitionState::Invalid;
    for _ in 0..attempts {
        result = collide::cell_find_collisions(ctx, t, cell);
        match result {
            TransitionState::Ok | TransitionState::Collided => return result,
            TransitionState::Slid => {
                t.collision_info.contact_plane_valid = false;
                t.collision_info.contact_plane_is_water = false;
            }
            _ => {}
        }
    }
    result
}

/// The check over the other cells.
pub fn check_other_cells(
    ctx: &TransitionCtx<'_>,
    t: &mut Transition,
    except: Option<&Cell>,
) -> TransitionState {
    t.sphere_path.cell_array_valid = true;
    t.sphere_path.hits_interior_cell = false;
    let resolver = ctx.resolver();
    let mut arr = std::mem::take(&mut t.cell_array);
    let mut hits_interior = false;
    let spheres: Vec<crate::geom::Sphere> = t.sphere_path.global_spheres().to_vec();
    let pos = t.sphere_path.check_pos;
    let new_cell = resolver.find_cell_list(&pos, &spheres, &mut arr, true, &mut hits_interior);
    t.cell_array = arr;
    t.sphere_path.hits_interior_cell = hits_interior;

    let except_id = except.map(Cell::id);
    let candidates: Vec<Cell> = t
        .cell_array
        .cells
        .iter()
        .filter_map(|c| c.cell.clone())
        .collect();
    for c in candidates {
        if Some(c.id()) == except_id {
            continue;
        }
        let r = collide::cell_find_collisions(ctx, t, &c);
        match r {
            TransitionState::Collided | TransitionState::Adjusted => return r,
            TransitionState::Slid => {
                t.collision_info.contact_plane_valid = false;
                t.collision_info.contact_plane_is_water = false;
                return r;
            }
            _ => {}
        }
    }

    if let Some(c) = new_cell {
        t.sphere_path.check_cell = Some(c.id());
        t.sphere_path.adjust_check_pos(c.id());
        return TransitionState::Ok;
    }
    if t.sphere_path.step_down {
        return TransitionState::Collided;
    }
    // We left every known cell: try to fall out into the open.
    let mut tmp = t.sphere_path.check_pos;
    if landdefs::is_outdoors(tmp.cell) {
        let mut id = tmp.cell;
        let mut o = tmp.frame.origin;
        landdefs::adjust_to_outside(&mut id, &mut o);
        tmp.cell = id;
        tmp.frame.origin = o;
    }
    if tmp.cell.0 != 0 {
        t.sphere_path.adjust_check_pos(tmp.cell);
        t.sphere_path.check_pos = tmp;
        t.sphere_path.check_cell = None;
        t.sphere_path.cell_array_valid = false;
        t.sphere_path.cache_global_sphere(None);
        t.sphere_path.cell_array_valid = true;
        return TransitionState::Ok;
    }
    TransitionState::Collided
}

/// The transitional insert -- the core loop.
///
/// `attempts` is **3** from `find_transitional_position`, **5** from `step_down` and **1** for the
/// final placement re-check.
pub fn transitional_insert(
    ctx: &TransitionCtx<'_>,
    t: &mut Transition,
    attempts: u32,
) -> TransitionState {
    let Some(check_cell_id) = t.sphere_path.check_cell else {
        return TransitionState::Ok;
    };
    let resolver = ctx.resolver();
    let mut result = TransitionState::Invalid;

    for _ in 0..attempts {
        let Some(cell) = resolver.get_visible(check_cell_id) else {
            return TransitionState::Ok;
        };
        let mut r = insert_into_cell(ctx, t, &cell, attempts);
        match r {
            TransitionState::Ok => {
                r = check_other_cells(ctx, t, Some(&cell));
                if r != TransitionState::Ok {
                    t.sphere_path.neg_poly_hit = false;
                }
                if r == TransitionState::Collided {
                    return TransitionState::Collided;
                }
            }
            TransitionState::Collided => {
                t.sphere_path.neg_poly_hit = false;
                return TransitionState::Collided;
            }
            TransitionState::Slid => {
                t.collision_info.contact_plane_valid = false;
                t.collision_info.contact_plane_is_water = false;
                t.sphere_path.neg_poly_hit = false;
            }
            TransitionState::Adjusted => {
                t.sphere_path.neg_poly_hit = false;
            }
            TransitionState::Invalid => {}
        }
        result = r;
        if r != TransitionState::Ok {
            continue;
        }

        // --- a geometrically valid position; now the walk logic ---
        if t.sphere_path.collide {
            t.sphere_path.collide = false;
            let mut failed = true;
            if t.collision_info.contact_plane_valid
                && super::walk::check_walkable(ctx, t, LANDING_Z)
            {
                let backup = t.sphere_path.insert_type;
                t.sphere_path.insert_type = InsertType::Placement;
                result = transitional_insert(ctx, t, attempts);
                t.sphere_path.insert_type = backup;
                if result == TransitionState::Ok {
                    failed = false;
                }
            }
            t.sphere_path.walkable = None;
            if !failed {
                return TransitionState::Ok;
            }
            t.sphere_path.restore_check_pos();
            t.collision_info.contact_plane_valid = false;
            t.collision_info.contact_plane_is_water = false;
            if !t.collision_info.last_known_contact_plane_valid {
                let n = t.sphere_path.step_up_normal;
                t.collision_info.set_collision_normal(n);
                return TransitionState::Collided;
            }
            t.kill_velocity = true;
            t.collision_info.last_known_contact_plane_valid = false;
            return TransitionState::Collided;
        }

        if t.sphere_path.neg_poly_hit && !t.sphere_path.step_down && !t.sphere_path.step_up {
            t.sphere_path.neg_poly_hit = false;
            if t.sphere_path.neg_step_up {
                let n = t.sphere_path.neg_collision_normal;
                if !super::walk::step_up(ctx, t, n) {
                    t.collision_info.contact_plane_valid = false;
                    t.collision_info.contact_plane_is_water = false;
                    t.sphere_path.step_up = false;
                    let sphere = t.sphere_path.global_sphere[0];
                    let curr = t.sphere_path.global_curr_center[0];
                    let up = t.sphere_path.step_up_normal;
                    result = collide::slide_sphere(t, sphere, up, curr);
                }
            } else {
                let sphere = t.sphere_path.global_sphere[0];
                let curr = t.sphere_path.global_curr_center[0];
                let n = t.sphere_path.neg_collision_normal;
                result = collide::slide_sphere(t, sphere, n, curr);
            }
            continue;
        }

        // --- try to stay on the ground ---
        if t.collision_info.contact_plane_valid {
            return TransitionState::Ok;
        }
        if !t.object_info.has(ObjectInfoState::CONTACT) {
            return TransitionState::Ok;
        }
        if t.sphere_path.step_down {
            return TransitionState::Ok;
        }
        if t.sphere_path.check_cell.is_none() {
            return TransitionState::Ok;
        }
        if !t.object_info.step_down {
            return TransitionState::Ok; // missiles never step down
        }

        let mut amt = DEFAULT_STEP_HEIGHT;
        let mut allow = LANDING_Z;
        if t.object_info.has(ObjectInfoState::ON_WALKABLE) {
            allow = crate::get_walkable_z();
            amt = t.object_info.step_down_height;
        }
        t.sphere_path.walkable_allowance = allow;
        t.sphere_path.save_check_pos();
        let r0 = t.sphere_path.global_sphere[0].radius;
        if t.sphere_path.num_sphere < 2 && 2.0 * r0 < amt {
            amt = r0 * 0.5;
        }
        let ok = if amt <= 2.0 * r0 {
            super::walk::step_down(ctx, t, amt, allow)
        } else {
            amt *= 0.5;
            super::walk::step_down(ctx, t, amt, allow) || super::walk::step_down(ctx, t, amt, allow)
        };
        if ok {
            t.sphere_path.walkable = None;
            return TransitionState::Ok;
        }
        // **`edge_slide`'s out-parameter IS the loop's own transition state.**
        //
        // The client builds the out-parameter as the address of the loop's own transition-state
        // slot. The loop tail keeps that slot current, and both exits return it: the end of the
        // attempt budget returns the live state, and the successful `edge_slide` exit reloads the
        // same slot. So a slide, a cliff slide or a
        // precipice slide that the remaining attempts never recover from is what this function
        // *returns*, and `validate_transition` then takes its non-OK arm: it puts the check
        // position back at `curr_pos`, restores the last known contact plane when the body is
        // still within `radius + EPSILON` of it, and keeps the body on the ledge.
        //
        // Writing the state into a fresh local and throwing it away would make the same sub-step
        // return `OK_TS` with `contact_plane_valid` already cleared by `edge_slide` -- which is
        // "the walk succeeded and the body is in the air", i.e. the player walks off the ledge.
        if super::walk::edge_slide(ctx, t, &mut result, amt, allow) {
            return result;
        }
    }
    result
}

/// The transition validation.
///
/// The "stationary fall" counter is the anti-stuck mechanism: three consecutive sub-steps in which
/// a gravity-affected object failed to move and had no contact produce a synthetic horizontal
/// plane under the sphere, zero the velocity and set `STATIONARY_STUCK_TS`.
pub fn validate_transition(
    ctx: &TransitionCtx<'_>,
    t: &mut Transition,
    mut state: TransitionState,
    mover_has_gravity: bool,
) -> TransitionState {
    let mut contact_ok = true;

    // **`OK_TS` and the body did not move is not a contact.** The client seeds its
    // contact flag to 1; the `OK_TS` arm compares `check_pos.objcell_id` with
    // `curr_pos.objcell_id` and then compares the two frames. When both say "unchanged", control
    // falls into the arm that zeroes the flag; only a move jumps past it with the flag still set.
    // Without it a sub-step that succeeded without moving would
    // reset `frames_stationary_fall` instead of advancing it, and the client's anti-stuck counter
    // could never reach 3 from a standstill. Measured over `dereth-client`'s eighty `interiors`
    // trials it moves **no trial** — a body pressed against a wall is blocked (`state != Ok`)
    // rather than `Ok`-and-unmoved — so it is fidelity with no observable in this corpus.
    let unmoved = t.sphere_path.check_pos.cell == t.sphere_path.curr_pos.cell
        && math::frame_is_equal(
            &t.sphere_path.check_pos.frame,
            &t.sphere_path.curr_pos.frame,
        );
    if state == TransitionState::Ok && unmoved {
        contact_ok = false;
    }

    if state != TransitionState::Ok {
        contact_ok = false;
        if matches!(
            state,
            TransitionState::Collided | TransitionState::Adjusted | TransitionState::Slid
        ) {
            if t.collision_info.last_known_contact_plane_valid {
                t.kill_velocity = true;
                let lk = t.collision_info.last_known_contact_plane;
                let d = lk.dot_point(t.sphere_path.global_curr_center[0]);
                if d.abs() < t.sphere_path.global_sphere[0].radius + EPSILON {
                    let water = t.collision_info.last_known_contact_plane_is_water;
                    t.collision_info.set_contact_plane(lk, water);
                    t.collision_info.contact_plane_cell_id =
                        t.collision_info.last_known_contact_plane_cell_id;
                    if t.object_info.has(ObjectInfoState::ON_WALKABLE) {
                        contact_ok = true;
                    }
                }
            }
            if !t.collision_info.collision_normal_valid {
                t.collision_info
                    .set_collision_normal(Vec3::new(0.0, 0.0, 1.0));
            }
            let (pos, cell) = (t.sphere_path.curr_pos, t.sphere_path.curr_cell);
            t.sphere_path.set_check_pos(&pos, cell);
            build_cell_array(ctx, t);
            state = TransitionState::Ok;
        }
    } else {
        // Accepted.
        t.sphere_path.curr_pos = t.sphere_path.check_pos;
        t.sphere_path.curr_cell = t.sphere_path.check_cell;
        t.sphere_path.cache_global_curr_center();
        let (pos, cell) = (t.sphere_path.curr_pos, t.sphere_path.curr_cell);
        t.sphere_path.check_pos = pos;
        t.sphere_path.check_cell = cell;
        t.sphere_path.cell_array_valid = false;
        t.sphere_path.cache_global_sphere(None);
    }

    if t.collision_info.collision_normal_valid {
        let n = t.collision_info.collision_normal;
        t.collision_info.set_sliding_normal(n);
    }

    // Stationary-fall detection.
    if !t.object_info.has(ObjectInfoState::IS_VIEWER) && mover_has_gravity {
        if contact_ok {
            t.collision_info.frames_stationary_fall = 0;
        } else {
            let before = t.collision_info.frames_stationary_fall;
            t.collision_info.frames_stationary_fall = (before + 1).min(3);
            // **Retail, not the edge.** The client tests the counter as three cases, not as a
            // saturating increment: `0` becomes `1`, `1` becomes `2`, and **everything else**
            // writes `3` *and builds the synthetic plane*. So a body still wedged on its fourth,
            // fifth and later sub-step gets the plane **every time**, where a `before < 3` test
            // would give it once. That matters because `find_transitional_position` clears
            // `contact_plane_valid` at the top of every sub-step, so suppressed frames would leave
            // a wedged body with no contact plane at all — `CONTACT` and `ON_WALKABLE` both
            // cleared, which changes the next frame's `adjust_offset` and the next physics update's
            // `ON_WALKABLE_TS` gate.
            // Measured over `dereth-client`'s eighty `interiors` trials at both 0 and 1 degree it
            // moves **no trial**: fidelity with no observable in this corpus.
            if before >= 2 {
                let s = t.sphere_path.global_sphere[0];
                let plane = Plane {
                    normal: Vec3::new(0.0, 0.0, 1.0),
                    d: s.radius - s.center.z,
                };
                t.collision_info.set_contact_plane(plane, false);
                t.collision_info.contact_plane_cell_id = t.sphere_path.check_pos.cell;
                if !t.object_info.has(ObjectInfoState::CONTACT) {
                    t.collision_info.set_collision_normal(plane.normal);
                    t.collision_info.collided_with_environment = true;
                }
            }
        }
    }

    t.collision_info.last_known_contact_plane_valid = t.collision_info.contact_plane_valid;
    if t.collision_info.contact_plane_valid {
        t.collision_info.last_known_contact_plane = t.collision_info.contact_plane;
        t.collision_info.last_known_contact_plane_cell_id = t.collision_info.contact_plane_cell_id;
        t.collision_info.last_known_contact_plane_is_water =
            t.collision_info.contact_plane_is_water;
    }

    if t.collision_info.contact_plane_valid {
        t.object_info.set(ObjectInfoState::CONTACT, true);
        let walkable = crate::is_valid_walkable(t.collision_info.contact_plane.normal);
        t.object_info.set(ObjectInfoState::ON_WALKABLE, walkable);
    } else {
        t.object_info.set(ObjectInfoState::CONTACT, false);
        t.object_info.set(ObjectInfoState::ON_WALKABLE, false);
    }
    state
}

/// Rebuild the transition cell list around the current check
/// position without picking a container.
pub fn build_cell_array(ctx: &TransitionCtx<'_>, t: &mut Transition) {
    let resolver = ctx.resolver();
    let mut arr = std::mem::take(&mut t.cell_array);
    let mut hits_interior = false;
    let spheres: Vec<crate::geom::Sphere> = t.sphere_path.global_spheres().to_vec();
    let pos = t.sphere_path.check_pos;
    resolver.find_cell_list(&pos, &spheres, &mut arr, false, &mut hits_interior);
    t.cell_array = arr;
    t.sphere_path.hits_interior_cell = hits_interior;
    t.sphere_path.cell_array_valid = true;
}

/// The transitional-position search -- the outer loop.
pub fn find_transitional_position(
    ctx: &TransitionCtx<'_>,
    t: &mut Transition,
    mover_state: PhysicsState,
) -> bool {
    if t.sphere_path.begin_cell.is_none() {
        return false;
    }
    let mut state = TransitionState::Ok;
    let (total, mut step, num_steps) = calc_num_steps(t);

    if t.object_info.has(ObjectInfoState::FREE_ROTATE) {
        let q = t.sphere_path.end_pos.frame.rotation;
        let mut f = t.sphere_path.curr_pos.frame;
        math::set_rotate(&mut f, q.w, q.x, q.y, q.z);
        t.sphere_path.curr_pos.frame = f;
    }

    t.sphere_path.check_pos = t.sphere_path.curr_pos;
    t.sphere_path.check_cell = t.sphere_path.curr_cell;
    t.sphere_path.cell_array_valid = false;
    t.sphere_path.cache_global_sphere(None);

    if num_steps == 0 {
        if !t.object_info.has(ObjectInfoState::FREE_ROTATE) {
            let q = t.sphere_path.end_pos.frame.rotation;
            let mut f = t.sphere_path.curr_pos.frame;
            math::set_rotate(&mut f, q.w, q.x, q.y, q.z);
            t.sphere_path.curr_pos.frame = f;
        }
        let (pos, cell) = (t.sphere_path.curr_pos, t.sphere_path.curr_cell);
        t.sphere_path.set_check_pos(&pos, cell);
        t.sphere_path.cell_array_valid = true;
        t.sphere_path.hits_interior_cell = false;
        build_cell_array(ctx, t);
        return true;
    }

    let dist = total.mag2().sqrt();
    for i in 0..num_steps {
        // A viewer shortens its last step so the total distance comes out exact.
        if t.object_info.has(ObjectInfoState::IS_VIEWER) && i == num_steps - 1 && dist > EPSILON {
            #[allow(clippy::cast_precision_loss)]
            let f = (dist - (num_steps - 1) as f32 * t.sphere_path.local_sphere[0].radius) / dist;
            step = total.mul(f);
        }

        let global_offset = adjust_offset(t, step);
        t.sphere_path.global_offset = global_offset;
        if !t.object_info.has(ObjectInfoState::IS_VIEWER) && global_offset.mag2() < EPSILON_SQ {
            return if i == 0 {
                false
            } else {
                state == TransitionState::Ok
            };
        }

        if !t.object_info.has(ObjectInfoState::FREE_ROTATE) {
            if let Some(begin) = t.sphere_path.begin_pos {
                #[allow(clippy::cast_precision_loss)]
                let frac = (i + 1) as f32 / num_steps as f32;
                let end = t.sphere_path.end_pos;
                let mut f = t.sphere_path.check_pos.frame;
                math::interpolate_rotation(&mut f, &begin.frame, &end.frame, frac);
                t.sphere_path.check_pos.frame = f;
            }
        }

        t.collision_info.sliding_normal_valid = false;
        t.collision_info.contact_plane_valid = false;
        t.collision_info.contact_plane_is_water = false;

        if t.sphere_path.insert_type == InsertType::Transition {
            t.sphere_path.cell_array_valid = false;
            t.sphere_path.check_pos.frame.origin =
                t.sphere_path.check_pos.frame.origin.add(global_offset);
            t.sphere_path.cache_global_sphere(Some(global_offset));
            let inner = transitional_insert(ctx, t, ATTEMPTS_NORMAL);
            state = validate_transition(ctx, t, inner, mover_state.has_gravity());
            if t.collision_info.frames_stationary_fall != 0 {
                break;
            }
        } else {
            let inner = transitional_insert(ctx, t, ATTEMPTS_NORMAL);
            state = validate_placement_transition(t, inner);
            if state == TransitionState::Ok {
                return true;
            }
            if !t.sphere_path.placement_allows_sliding {
                return false;
            }
            t.sphere_path.add_offset_to_check_pos(global_offset);
        }

        if t.collision_info.collision_normal_valid
            && t.object_info.has(ObjectInfoState::PATH_CLIPPED)
        {
            break;
        }
    }
    state == TransitionState::Ok
}

/// On `OK_TS` accept; on a blocking
/// state with sliding allowed, reset all collision information and return the state unchanged so
/// the caller can try a different scatter point.
pub fn validate_placement_transition(
    t: &mut Transition,
    state: TransitionState,
) -> TransitionState {
    if state == TransitionState::Ok {
        t.sphere_path.curr_pos = t.sphere_path.check_pos;
        t.sphere_path.curr_cell = t.sphere_path.check_cell;
        t.sphere_path.cache_global_curr_center();
        return state;
    }
    if matches!(
        state,
        TransitionState::Collided | TransitionState::Adjusted | TransitionState::Slid
    ) && t.sphere_path.placement_allows_sliding
    {
        t.collision_info.init();
    }
    state
}

/// The placement validation.
pub fn validate_placement(
    ctx: &TransitionCtx<'_>,
    t: &mut Transition,
    state: TransitionState,
    retry: bool,
) -> TransitionState {
    if state == TransitionState::Ok {
        t.sphere_path.curr_pos = t.sphere_path.check_pos;
        t.sphere_path.curr_cell = t.sphere_path.check_cell;
        t.sphere_path.cache_global_curr_center();
        return state;
    }
    if retry && matches!(state, TransitionState::Adjusted | TransitionState::Slid) {
        let r = placement_insert(ctx, t);
        return validate_placement(ctx, t, r, false);
    }
    state
}

/// `insert_into_cell(3)` then `check_other_cells`.
pub fn placement_insert(ctx: &TransitionCtx<'_>, t: &mut Transition) -> TransitionState {
    let Some(id) = t.sphere_path.check_cell else {
        return TransitionState::Collided;
    };
    let resolver = ctx.resolver();
    let Some(cell) = resolver.get_visible(id) else {
        return TransitionState::Collided;
    };
    let r = insert_into_cell(ctx, t, &cell, ATTEMPTS_NORMAL);
    if r != TransitionState::Ok {
        return r;
    }
    check_other_cells(ctx, t, Some(&cell))
}

/// The valid-position search.
pub fn find_valid_position(
    ctx: &TransitionCtx<'_>,
    t: &mut Transition,
    mover_state: PhysicsState,
) -> bool {
    if t.sphere_path.insert_type == InsertType::Transition {
        find_transitional_position(ctx, t, mover_state)
    } else {
        super::place::find_placement_position(ctx, t, mover_state)
    }
}

/// A fresh cell array, for callers that need to seed one.
#[must_use]
pub fn empty_cell_array() -> CellArray {
    CellArray::new()
}

/// The three attempt budgets, exposed so a test can assert them without re-deriving them.
pub const ATTEMPT_BUDGETS: (u32, u32, u32) =
    (ATTEMPTS_NORMAL, ATTEMPTS_STEP_DOWN, ATTEMPTS_REVALIDATE);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Sphere;
    use dereth_primitives::{CellId, Frame, Position, Quat};

    // Oracle: the description of calc_num_steps and adjust_offset in
    // the recovered collision and transition behavior.

    fn transition_moving(radius: f32, from: Vec3, to: Vec3) -> Transition {
        let mut t = Transition::default();
        t.sphere_path
            .init_sphere(&[Sphere::new(Vec3::new(0.0, 0.0, radius), radius)], 1.0);
        let cell = CellId(0xA9B4_0001);
        let begin = Position::new(cell, Frame::new(from, Quat::IDENTITY));
        let end = Position::new(cell, Frame::new(to, Quat::IDENTITY));
        t.sphere_path.init_path(Some(cell), Some(begin), &end);
        t.sphere_path.set_check_pos(&begin, Some(cell));
        t
    }

    /// The budgets are 3 / 5 / 1 exactly.
    #[test]
    fn the_attempt_budgets_are_three_five_one() {
        assert_eq!(ATTEMPT_BUDGETS, (3, 5, 1));
    }

    #[test]
    fn calc_num_steps_is_ceil_distance_over_radius() {
        // A 0.5 m sphere moving 3 m: 6 sub-steps.
        let t = transition_moving(0.5, Vec3::ZERO, Vec3::new(3.0, 0.0, 0.0));
        let (total, step, n) = calc_num_steps(&t);
        assert_eq!(n, 6);
        assert_eq!(total, Vec3::new(3.0, 0.0, 0.0));
        assert!((step.x - 0.5).abs() < 1e-6, "{step:?}");
        // 3.1 m rounds up to 7.
        let t = transition_moving(0.5, Vec3::ZERO, Vec3::new(3.1, 0.0, 0.0));
        assert_eq!(calc_num_steps(&t).2, 7);
    }

    #[test]
    fn a_move_shorter_than_one_radius_is_a_single_step_of_the_whole_offset() {
        let t = transition_moving(0.5, Vec3::ZERO, Vec3::new(0.2, 0.0, 0.0));
        let (total, step, n) = calc_num_steps(&t);
        assert_eq!(n, 1);
        assert_eq!(step, total);
    }

    #[test]
    fn an_exactly_zero_move_produces_zero_steps() {
        let t = transition_moving(0.5, Vec3::new(1.0, 2.0, 3.0), Vec3::new(1.0, 2.0, 3.0));
        let (_, step, n) = calc_num_steps(&t);
        assert_eq!(n, 0, "the test is an *exact* zero, not an epsilon");
        assert_eq!(step, Vec3::ZERO);
        // A sub-epsilon but non-zero move still gets one step.
        let t = transition_moving(0.5, Vec3::ZERO, Vec3::new(1e-7, 0.0, 0.0));
        assert_eq!(calc_num_steps(&t).2, 1);
    }

    #[test]
    fn the_viewer_step_rule_is_floor_plus_one_not_ceil() {
        // 3 m at radius 0.5: floor(6) + 1 = 7 steps for a viewer, where an ordinary object gets 6.
        let mut t = transition_moving(0.5, Vec3::ZERO, Vec3::new(3.0, 0.0, 0.0));
        assert_eq!(calc_num_steps(&t).2, 6);
        t.object_info.set(ObjectInfoState::IS_VIEWER, true);
        assert_eq!(calc_num_steps(&t).2, 7);
        // and a viewer move under the epsilon gets zero steps
        let mut t = transition_moving(0.5, Vec3::ZERO, Vec3::new(1e-5, 0.0, 0.0));
        t.object_info.set(ObjectInfoState::IS_VIEWER, true);
        assert_eq!(calc_num_steps(&t).2, 0);
    }

    #[test]
    fn calc_num_steps_with_no_begin_position_is_one_step_of_nothing() {
        let mut t = Transition::default();
        t.sphere_path
            .init_sphere(&[Sphere::new(Vec3::ZERO, 0.5)], 1.0);
        let end = Position::new(CellId(1), Frame::default());
        t.sphere_path.init_path(Some(CellId(1)), None, &end);
        assert_eq!(calc_num_steps(&t), (Vec3::ZERO, Vec3::ZERO, 1));
    }

    /// `adjust_offset`, all five branches plus the sink correction.
    #[test]
    fn adjust_offset_with_no_planes_returns_the_offset_unchanged() {
        let mut t = transition_moving(0.5, Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0));
        let v = Vec3::new(1.0, 2.0, 3.0);
        assert_eq!(adjust_offset(&mut t, v), v);
    }

    #[test]
    fn adjust_offset_slides_along_a_wall_when_only_a_sliding_normal_is_valid() {
        let mut t = transition_moving(0.5, Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0));
        t.collision_info
            .set_sliding_normal(Vec3::new(-1.0, 0.0, 0.0));
        // Moving into the wall: the X component is removed.
        let out = adjust_offset(&mut t, Vec3::new(1.0, 1.0, 0.0));
        assert!(out.x.abs() < 1e-6, "{out:?}");
        assert!((out.y - 1.0).abs() < 1e-6);
    }

    #[test]
    fn adjust_offset_forgets_a_sliding_normal_once_we_move_away_from_it() {
        let mut t = transition_moving(0.5, Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0));
        t.collision_info
            .set_sliding_normal(Vec3::new(-1.0, 0.0, 0.0));
        let out = adjust_offset(&mut t, Vec3::new(-1.0, 0.0, 0.0));
        assert_eq!(
            out,
            Vec3::new(-1.0, 0.0, 0.0),
            "moving away is unrestricted"
        );
        assert!(
            !t.collision_info.sliding_normal_valid,
            "and the normal is forgotten"
        );
    }

    #[test]
    fn adjust_offset_projects_movement_onto_a_floor_contact_plane() {
        let mut t = transition_moving(0.5, Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0));
        t.collision_info.set_contact_plane(
            Plane {
                normal: Vec3::new(0.0, 0.0, 1.0),
                d: 0.0,
            },
            false,
        );
        t.collision_info.contact_plane_cell_id = CellId(0);
        // Moving down into the floor: the -Z component is projected out.
        let out = adjust_offset(&mut t, Vec3::new(1.0, 0.0, -1.0));
        assert!((out.x - 1.0).abs() < 1e-6, "{out:?}");
        assert!(out.z.abs() < 1e-6, "{out:?}");
    }

    #[test]
    fn adjust_offset_snaps_to_the_plane_when_moving_away_from_it() {
        let mut t = transition_moving(0.5, Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0));
        // A 45-degree ramp; moving horizontally "away" (positive dot) snaps Z onto the plane.
        let inv = 1.0 / 2.0_f32.sqrt();
        t.collision_info.set_contact_plane(
            Plane {
                normal: Vec3::new(-inv, 0.0, inv),
                d: 0.0,
            },
            false,
        );
        t.collision_info.contact_plane_cell_id = CellId(0);
        let out = adjust_offset(&mut t, Vec3::new(0.0, 0.0, 1.0));
        // dot > 0 so snap_to_plane runs: v.z becomes -(v.x*N.x + v.y*N.y)/N.z = 0.
        assert!(out.z.abs() < 1e-4, "{out:?}");
    }

    #[test]
    fn adjust_offset_follows_the_crease_when_both_planes_are_valid() {
        let mut t = transition_moving(0.5, Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0));
        t.collision_info.set_contact_plane(
            Plane {
                normal: Vec3::new(0.0, 0.0, 1.0),
                d: 0.0,
            },
            false,
        );
        t.collision_info.contact_plane_cell_id = CellId(0);
        t.collision_info
            .set_sliding_normal(Vec3::new(-1.0, 0.0, 0.0));
        // Into the wall and along the floor: only the Y component survives, along the crease.
        let out = adjust_offset(&mut t, Vec3::new(1.0, 2.0, 0.0));
        assert!(out.x.abs() < 1e-5, "{out:?}");
        assert!((out.y.abs() - 2.0).abs() < 1e-5, "{out:?}");
    }

    #[test]
    fn adjust_offset_stops_dead_on_a_degenerate_crease() {
        let mut t = transition_moving(0.5, Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0));
        // A sliding normal parallel to the contact plane normal: the cross product collapses.
        t.collision_info.set_contact_plane(
            Plane {
                normal: Vec3::new(1.0, 0.0, 0.0),
                d: 0.0,
            },
            false,
        );
        t.collision_info.contact_plane_cell_id = CellId(0);
        t.collision_info.sliding_normal_valid = true;
        t.collision_info.sliding_normal = Vec3::new(1.0, 0.0, 0.0);
        let out = adjust_offset(&mut t, Vec3::new(-1.0, 0.0, 0.0));
        assert_eq!(out, Vec3::ZERO);
    }

    #[test]
    fn adjust_offset_lifts_a_sphere_that_has_sunk_into_its_contact_plane() {
        // A floor at z = 0 and a sphere whose centre is only 0.3 above it (origin -0.2 plus the
        // sphere's own local 0.5): the sink correction must lift it.
        let mut t = transition_moving(0.5, Vec3::new(0.0, 0.0, -0.2), Vec3::new(1.0, 0.0, -0.2));
        t.collision_info.set_contact_plane(
            Plane {
                normal: Vec3::new(0.0, 0.0, 1.0),
                d: 0.0,
            },
            false,
        );
        t.collision_info.contact_plane_cell_id = t.sphere_path.check_pos.cell;
        let before = t.sphere_path.check_pos.frame.origin.z;
        let _ = adjust_offset(&mut t, Vec3::new(1.0, 0.0, 0.0));
        let after = t.sphere_path.check_pos.frame.origin.z;
        assert!(
            after > before,
            "the sink correction must lift it: {before} -> {after}"
        );
    }

    /// A body that arrives already wedged still gets the synthetic contact plane.
    #[test]
    fn a_body_that_arrives_already_wedged_still_gets_the_synthetic_contact_plane() {
        let world =
            crate::PhysicsWorld::new(std::sync::Arc::new(crate::StaticLandSource::linear()));
        let ctx = world.transition_ctx(None);
        let mut t = transition_moving(
            0.5,
            Vec3::new(12.0, 12.0, 20.0),
            Vec3::new(13.0, 12.0, 20.0),
        );
        t.sphere_path.cache_global_sphere(None);
        let s = t.sphere_path.global_sphere[0];

        // The frame before left the body `STATIONARY_STUCK_TS`, which `PhysicsWorld` seeds back
        // into the transition, and its contact was not walkable, so nothing resets the counter.
        t.collision_info.frames_stationary_fall = 3;
        t.collision_info.last_known_contact_plane_valid = false;
        t.object_info.set(ObjectInfoState::ON_WALKABLE, false);
        t.collision_info.contact_plane_valid = false;

        let out = validate_transition(&ctx, &mut t, TransitionState::Collided, true);
        assert_eq!(
            out,
            TransitionState::Ok,
            "a blocked sub-step is reported as OK_TS"
        );
        assert_eq!(t.collision_info.frames_stationary_fall, 3);
        assert!(
            t.collision_info.contact_plane_valid,
            "the fourth wedged frame got no synthetic plane; retail builds one every time"
        );
        assert_eq!(
            t.collision_info.contact_plane.normal,
            Vec3::new(0.0, 0.0, 1.0)
        );
        // The literal the client computes, stated as arithmetic rather than through the symbol.
        assert!(
            (t.collision_info.contact_plane.d - (s.radius - s.center.z)).abs() < 1e-6,
            "the synthetic plane's d is {} where `radius - center.z` is {}",
            t.collision_info.contact_plane.d,
            s.radius - s.center.z
        );
        assert!(t.object_info.has(ObjectInfoState::CONTACT));
    }

    /// The first three frames, so the arm above is a *third* case and not the only one.
    #[test]
    fn the_stationary_fall_counter_climbs_one_two_three_before_any_plane_is_built() {
        let world =
            crate::PhysicsWorld::new(std::sync::Arc::new(crate::StaticLandSource::linear()));
        let ctx = world.transition_ctx(None);
        let mut t = transition_moving(
            0.5,
            Vec3::new(12.0, 12.0, 20.0),
            Vec3::new(13.0, 12.0, 20.0),
        );
        t.sphere_path.cache_global_sphere(None);
        t.object_info.set(ObjectInfoState::ON_WALKABLE, false);

        let mut seen = Vec::new();
        for _ in 0..3 {
            t.collision_info.contact_plane_valid = false;
            t.collision_info.last_known_contact_plane_valid = false;
            validate_transition(&ctx, &mut t, TransitionState::Collided, true);
            seen.push((
                t.collision_info.frames_stationary_fall,
                t.collision_info.contact_plane_valid,
            ));
        }
        assert_eq!(seen, vec![(1, false), (2, false), (3, true)]);
    }
}
