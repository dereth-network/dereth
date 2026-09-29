//! Step up, step down, `check_walkable`, edge slide and cliff slide.
//!
//! "Step up" is implemented as a **step down from the raised position**: the geometry test's
//! collision response has already lifted the sphere, and `step_down` verifies that there is
//! walkable ground under it.

use dereth_primitives::Vec3;

use crate::geom::plane::Plane;
use crate::globals::{ATTEMPTS_REVALIDATE, ATTEMPTS_STEP_DOWN, DEFAULT_STEP_HEIGHT, LANDING_Z};
use crate::landdefs;
use crate::math::V3;
use crate::transition::collide;
use crate::transition::insert::transitional_insert;
use crate::transition::objectinfo::ObjectInfoState;
use crate::transition::spherepath::InsertType;
use crate::transition::{Transition, TransitionCtx, TransitionState};

/// The transition's step up.
pub fn step_up(ctx: &TransitionCtx<'_>, t: &mut Transition, normal: Vec3) -> bool {
    t.collision_info.contact_plane_valid = false;
    t.collision_info.contact_plane_is_water = false;
    t.sphere_path.step_up = true;
    t.sphere_path.step_up_normal = normal;

    let mut amt = DEFAULT_STEP_HEIGHT;
    let mut allow = LANDING_Z;
    if t.object_info.has(ObjectInfoState::ON_WALKABLE) {
        allow = crate::get_walkable_z();
        amt = t.object_info.step_up_height;
    }
    t.sphere_path.walkable_allowance = allow;
    t.sphere_path.backup_cell = t.sphere_path.check_cell;
    t.sphere_path.backup_check_pos = t.sphere_path.check_pos;

    let ok = step_down(ctx, t, amt, allow);
    t.sphere_path.step_up = false;
    t.sphere_path.walkable = None;
    if !ok {
        t.sphere_path.restore_check_pos();
        return false;
    }
    true
}

/// The transition's step down.
///
/// The `transitional_insert` inside runs with the **5** budget; the final placement re-validation
/// runs with **1**.
pub fn step_down(ctx: &TransitionCtx<'_>, t: &mut Transition, amount: f32, allowance: f32) -> bool {
    t.sphere_path.neg_poly_hit = false;
    t.sphere_path.step_down = true;
    t.sphere_path.step_down_amt = amount;
    t.sphere_path.walk_interp = 1.0;
    if !t.sphere_path.step_up {
        t.sphere_path.cell_array_valid = false;
        t.sphere_path.check_pos.frame.origin.z -= amount;
        t.sphere_path
            .cache_global_sphere(Some(Vec3::new(0.0, 0.0, -amount)));
    }
    let mut r = transitional_insert(ctx, t, ATTEMPTS_STEP_DOWN);
    t.sphere_path.step_down = false;
    if r == TransitionState::Ok
        && t.collision_info.contact_plane_valid
        && t.collision_info.contact_plane.normal.z >= allowance
    {
        if t.object_info.has(ObjectInfoState::EDGE_SLIDE)
            && !t.sphere_path.step_up
            && !check_walkable(ctx, t, allowance)
        {
            // The ground we found is a sliver: reject it.
            return false;
        }
        let backup = t.sphere_path.insert_type;
        t.sphere_path.insert_type = InsertType::Placement;
        r = transitional_insert(ctx, t, ATTEMPTS_REVALIDATE);
        t.sphere_path.insert_type = backup;
        return r == TransitionState::Ok;
    }
    false
}

/// "is there ground here?".
///
/// Returns `true` when there is ground. Note the inversion at the end: the probe returns
/// `r != OK_TS`, i.e. hitting something *is* the affirmative answer.
pub fn check_walkable(ctx: &TransitionCtx<'_>, t: &mut Transition, allowance: f32) -> bool {
    if !t.object_info.has(ObjectInfoState::ON_WALKABLE) {
        return true; // nothing to check
    }
    if t.sphere_path.check_walkables() {
        return true; // the recorded polygon still holds the sphere
    }
    let save = t.sphere_path.check_pos;
    let save_cell = t.sphere_path.check_cell;
    let mut z = t.object_info.step_down_height;
    t.sphere_path.walkable_allowance = allowance;
    t.sphere_path.check_walkable = true;
    let r0 = t.sphere_path.global_sphere[0].radius;
    if t.sphere_path.num_sphere < 2 && 2.0 * r0 < z {
        z = r0 * 0.5;
    }
    if 2.0 * r0 < z {
        z *= 0.5;
    }
    t.sphere_path
        .add_offset_to_check_pos(Vec3::new(0.0, 0.0, -z));
    let r = transitional_insert(ctx, t, ATTEMPTS_REVALIDATE);
    t.sphere_path.check_walkable = false;
    t.sphere_path.set_check_pos(&save, save_cell);
    r != TransitionState::Ok
}

/// Slide along the intersection of a too-steep plane and
/// the last known contact plane.
pub fn cliff_slide(t: &mut Transition, plane: &Plane) -> TransitionState {
    let c = plane
        .normal
        .cross(t.collision_info.last_known_contact_plane.normal);
    // The z components are multiplied by 0.0, so the result is the horizontal projection of the
    // cross product rotated 90 degrees.
    let mut axis = Vec3::new(-c.y, c.x, 0.0);
    if !axis.normalize_check_small() {
        // normalize_check_small returning 0 (i.e. it *did* normalise) is the OK_TS path in the
        // original, which reads `if (normalize_check_small(axis) != 0) return OK_TS`.
    } else {
        return TransitionState::Ok;
    }
    let off = landdefs::get_block_offset(t.sphere_path.curr_pos.cell, t.sphere_path.check_pos.cell);
    let d = off
        .add(
            t.sphere_path.global_sphere[0]
                .center
                .sub(t.sphere_path.global_curr_center[0]),
        )
        .dot(axis);
    let (v, n) = if d <= 0.0 {
        (axis.mul(d), axis)
    } else {
        (axis.mul(-d), axis.negate())
    };
    t.sphere_path.add_offset_to_check_pos(v);
    t.collision_info.set_collision_normal(n);
    TransitionState::Adjusted
}

/// Slide along the edge of the walkable polygon.
pub fn precipice_slide(t: &mut Transition) -> TransitionState {
    let Some(poly) = t.sphere_path.walkable.clone() else {
        return TransitionState::Collided;
    };
    let sphere = t.sphere_path.walkable_check_pos;
    let up = t.sphere_path.walkable_up;
    let Some(edge) = poly.find_crossed_edge(&sphere, up) else {
        t.sphere_path.walkable = None;
        return TransitionState::Collided;
    };
    t.sphere_path.walkable = None;
    t.sphere_path.step_up = false;
    let wp = t.sphere_path.walkable_pos;
    let mut edge_world = crate::math::localtoglobalvec(crate::math::l2g(wp.frame.rotation), edge);
    let off = landdefs::get_block_offset(t.sphere_path.curr_pos.cell, t.sphere_path.check_pos.cell);
    let movement = off.add(
        t.sphere_path.global_sphere[0]
            .center
            .sub(t.sphere_path.global_curr_center[0]),
    );
    if movement.dot(edge_world) > 0.0 {
        edge_world = edge_world.negate();
    }
    let s = t.sphere_path.global_sphere[0];
    let curr = t.sphere_path.global_curr_center[0];
    collide::slide_sphere(t, s, edge_world, curr)
}

/// Called when a step-down failed. Returns `true` when the
/// caller should stop and use `*state`.
pub fn edge_slide(
    ctx: &TransitionCtx<'_>,
    t: &mut Transition,
    state: &mut TransitionState,
    amt: f32,
    allowance: f32,
) -> bool {
    if !t.object_info.has(ObjectInfoState::ON_WALKABLE)
        || !t.object_info.has(ObjectInfoState::EDGE_SLIDE)
    {
        // Just walk off the edge.
        t.sphere_path.walkable = None;
        t.sphere_path.restore_check_pos();
        t.collision_info.contact_plane_valid = false;
        t.collision_info.contact_plane_is_water = false;
        t.sphere_path.cell_array_valid = true;
        *state = TransitionState::Ok;
        return true;
    }

    if t.collision_info.contact_plane_valid && t.collision_info.contact_plane.normal.z < allowance {
        // A too-steep surface.
        t.sphere_path.walkable = None;
        t.sphere_path.restore_check_pos();
        let plane = t.collision_info.contact_plane;
        *state = cliff_slide(t, &plane);
        t.collision_info.contact_plane_valid = false;
        t.collision_info.contact_plane_is_water = false;
        return false;
    }

    if t.sphere_path.walkable.is_some() {
        t.sphere_path.restore_check_pos();
        t.collision_info.contact_plane_valid = false;
        t.collision_info.contact_plane_is_water = false;
        *state = precipice_slide(t);
        return *state == TransitionState::Collided;
    }

    if !t.collision_info.contact_plane_valid {
        // No contact at all: re-probe downward from the current centre, once.
        let offset = t.sphere_path.global_curr_center[0].sub(t.sphere_path.global_sphere[0].center);
        t.sphere_path.add_offset_to_check_pos(offset);
        step_down(ctx, t, amt, allowance);
        t.collision_info.contact_plane_valid = false;
        t.collision_info.contact_plane_is_water = false;
        t.sphere_path.restore_check_pos();
        if t.sphere_path.walkable.is_none() {
            *state = TransitionState::Collided;
            t.sphere_path.cell_array_valid = true;
            return true;
        }
        t.collision_info.contact_plane_valid = false;
        t.collision_info.contact_plane_is_water = false;
        let wp = t.sphere_path.walkable_pos;
        let scale = t.sphere_path.walkable_scale;
        t.sphere_path.cache_localspace_sphere(&wp, scale);
        let ls = t.sphere_path.localspace_sphere[0];
        t.sphere_path.set_walkable_check_pos(ls);
        *state = precipice_slide(t);
        return *state == TransitionState::Collided;
    }

    t.sphere_path.walkable = None;
    t.sphere_path.restore_check_pos();
    t.sphere_path.cell_array_valid = true;
    t.collision_info.contact_plane_valid = false;
    t.collision_info.contact_plane_is_water = false;
    *state = TransitionState::Ok;
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arena::Arena;
    use crate::geom::polygon::Polygon;
    use crate::geom::Sphere;
    use crate::source::StaticLandSource;
    use dereth_primitives::{CellId, Frame, LandblockId, Position, Quat};
    use std::collections::BTreeMap;

    // Oracle: the step_up, step_down, check_walkable,
    // edge_slide and cliff_slide pseudocode in
    // the recovered collision and transition behavior.

    struct World {
        land: StaticLandSource,
        objects: Arena<crate::obj::PhysicsObj>,
        cells: BTreeMap<u32, crate::cell::CellRuntime>,
        mover: crate::arena::PhysHandle,
    }

    fn world() -> World {
        let mut land = StaticLandSource::linear();
        land.add_flat_block(LandblockId::new(0xA9, 0xB4), 10); // ground at z = 20
        let mut objects = Arena::new();
        // check_entry_restrictions returns COLLIDED_TS outright for a transition with
        // no moving object, so every walk test needs a real one in the arena.
        let mover = objects.insert(crate::obj::PhysicsObj::new(
            dereth_primitives::ObjectId(1),
            std::sync::Arc::new(crate::source::SetupGeometry::dummy()),
            0.0,
            true,
        ));
        World {
            land,
            objects,
            cells: BTreeMap::new(),
            mover,
        }
    }

    fn ctx(w: &World) -> TransitionCtx<'_> {
        TransitionCtx {
            land: &w.land,
            objects: &w.objects,
            cells: &w.cells,
            mover: Some(w.mover),
            object_table: None,
            entry_host: None,
        }
    }

    fn standing(w: &World, z: f32) -> Transition {
        let mut t = Transition::default();
        t.object_info.object = Some(w.mover);
        t.sphere_path
            .init_sphere(&[Sphere::new(Vec3::new(0.0, 0.0, 0.5), 0.5)], 1.0);
        let cell = LandblockId::new(0xA9, 0xB4).cell(1);
        let pos = Position::new(cell, Frame::new(Vec3::new(12.0, 12.0, z), Quat::IDENTITY));
        t.sphere_path.init_path(Some(cell), Some(pos), &pos);
        t.sphere_path.set_check_pos(&pos, Some(cell));
        t.object_info.step_up_height = 0.3;
        t.object_info.step_down_height = 0.3;
        t.object_info.step_down = true;
        t
    }

    #[test]
    fn step_down_finds_flat_ground_just_below_the_sphere() {
        let w = world();
        let c = ctx(&w);
        // Ground at z = 20; the sphere's bottom starts 0.2 m above it, so a 0.3 m probe reaches.
        let mut t = standing(&w, 20.2);
        t.object_info.set(ObjectInfoState::CONTACT, true);
        t.object_info.set(ObjectInfoState::ON_WALKABLE, true);
        assert!(step_down(&c, &mut t, 0.3, crate::get_walkable_z()));
        assert!(t.collision_info.contact_plane_valid);
        assert!(t.collision_info.contact_plane.normal.z >= crate::get_walkable_z());
    }

    #[test]
    fn step_down_fails_when_the_ground_is_out_of_reach() {
        let w = world();
        let c = ctx(&w);
        // Two metres above the ground: a 0.3 m probe cannot reach it.
        let mut t = standing(&w, 22.0);
        t.object_info.set(ObjectInfoState::CONTACT, true);
        assert!(!step_down(&c, &mut t, 0.3, LANDING_Z));
    }

    #[test]
    fn step_down_restores_nothing_by_itself_but_step_up_does() {
        let w = world();
        let c = ctx(&w);
        let mut t = standing(&w, 22.0);
        t.object_info.set(ObjectInfoState::CONTACT, true);
        let before = t.sphere_path.check_pos.frame.origin;
        assert!(!step_up(&c, &mut t, Vec3::new(1.0, 0.0, 0.0)));
        assert_eq!(
            t.sphere_path.check_pos.frame.origin, before,
            "a failed step_up restores the check position"
        );
    }

    /// Step up probes with the objects own budget only while it is on a walkable.
    #[test]
    fn step_up_probes_with_the_objects_own_budget_only_while_it_is_on_a_walkable() {
        let w = world();
        let c = ctx(&w);
        let n = Vec3::new(0.0, 0.0, 1.0);

        // On a walkable: the setup's own 0.3 m, and the walkable allowance.
        let mut on = standing(&w, 22.0);
        on.object_info.set(ObjectInfoState::CONTACT, true);
        on.object_info.set(ObjectInfoState::ON_WALKABLE, true);
        on.object_info.step_up_height = 0.3;
        let _ = step_up(&c, &mut on, n);
        assert!(
            (on.sphere_path.step_down_amt - 0.3).abs() < 1e-6,
            "on a walkable the probe is the object's own step_up_height (object_info+0xC), not \
             the {DEFAULT_STEP_HEIGHT} m default: {}",
            on.sphere_path.step_down_amt
        );
        assert!((on.sphere_path.walkable_allowance - crate::get_walkable_z()).abs() < 1e-6);

        // Off it: the two baked defaults, and the object's field must not reach the probe.
        let mut off = standing(&w, 22.0);
        off.object_info.set(ObjectInfoState::CONTACT, true);
        off.object_info.step_up_height = 0.3;
        let _ = step_up(&c, &mut off, n);
        assert!(
            (off.sphere_path.step_down_amt - DEFAULT_STEP_HEIGHT).abs() < 1e-6,
            "off a walkable the probe is the baked 0.04 m: {}",
            off.sphere_path.step_down_amt
        );
        assert!((off.sphere_path.walkable_allowance - LANDING_Z).abs() < 1e-6);
    }

    #[test]
    fn step_up_clears_the_contact_plane_and_arms_the_normal() {
        let w = world();
        let c = ctx(&w);
        let mut t = standing(&w, 20.2);
        t.collision_info.set_contact_plane(
            Plane {
                normal: Vec3::new(0.0, 0.0, 1.0),
                d: -20.0,
            },
            false,
        );
        let n = Vec3::new(1.0, 0.0, 0.0);
        let _ = step_up(&c, &mut t, n);
        assert_eq!(t.sphere_path.step_up_normal, n);
        assert!(!t.sphere_path.step_up, "the flag is cleared on the way out");
    }

    /// Contract item 5.4, the narrow-ledge scenario: `check_walkables` halves the probe radius in
    /// place, so a sphere sitting near the edge of its walkable polygon eventually stops being
    /// held by it, and `check_walkable` falls through to the real re-probe.
    #[test]
    fn check_walkable_falls_through_once_the_halving_probe_gives_up() {
        let w = world();
        let c = ctx(&w);
        // Bottom 0.2 m above the ground, so the 0.3 m re-probe reaches it.
        let mut t = standing(&w, 20.2);
        t.object_info.set(ObjectInfoState::ON_WALKABLE, true);
        // A 1 m square ledge with the sphere's projected centre 0.9 m past its +X edge.
        let poly = Polygon::new(vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(1.0, 1.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ]);
        t.sphere_path.set_walkable(
            Sphere::new(Vec3::new(1.9, 0.5, 0.0), 4.0),
            poly,
            Vec3::new(0.0, 0.0, 1.0),
            Position::new(CellId(1), Frame::default()),
            1.0,
        );
        // Radius 4 -> 2 -> 1 all still reach the polygon; 0.5 does not.
        let mut probes = 0;
        loop {
            let held = t.sphere_path.check_walkables();
            probes += 1;
            if !held || probes > 10 {
                break;
            }
        }
        assert_eq!(probes, 3, "4 -> 2 -> 1 hold, and 1 -> 0.5 does not");
        assert_eq!(t.sphere_path.walkable_check_pos.radius, 0.5);
        // With the polygon no longer holding it, check_walkable does the real probe, which finds
        // the terrain 0.2 m below and answers "yes, there is ground".
        assert!(check_walkable(&c, &mut t, crate::get_walkable_z()));
        // ..and from 2 m up, with the same exhausted ledge recorded, the probe finds nothing and
        // answers "no". (Without a recorded polygon `check_walkables` returns an unconditional
        // yes, which is the branch the next test covers.)
        let mut high = standing(&w, 22.0);
        high.object_info.set(ObjectInfoState::ON_WALKABLE, true);
        high.sphere_path.set_walkable(
            Sphere::new(Vec3::new(1.9, 0.5, 0.0), 0.5),
            Polygon::new(vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(1.0, 1.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
            ]),
            Vec3::new(0.0, 0.0, 1.0),
            Position::new(CellId(1), Frame::default()),
            1.0,
        );
        assert!(!check_walkable(&c, &mut high, crate::get_walkable_z()));
    }

    #[test]
    fn check_walkable_is_an_unconditional_yes_when_not_on_walkable_ground() {
        let w = world();
        let c = ctx(&w);
        let mut t = standing(&w, 20.5);
        assert!(!t.object_info.has(ObjectInfoState::ON_WALKABLE));
        assert!(check_walkable(&c, &mut t, LANDING_Z));
    }

    #[test]
    fn check_walkable_restores_the_check_position_it_probed_from() {
        let w = world();
        let c = ctx(&w);
        let mut t = standing(&w, 20.5);
        t.object_info.set(ObjectInfoState::ON_WALKABLE, true);
        let before = t.sphere_path.check_pos;
        let _ = check_walkable(&c, &mut t, crate::get_walkable_z());
        assert_eq!(t.sphere_path.check_pos, before);
    }

    #[test]
    fn edge_slide_without_the_edge_slide_bit_just_walks_off() {
        let w = world();
        let c = ctx(&w);
        let mut t = standing(&w, 20.5);
        t.object_info.set(ObjectInfoState::ON_WALKABLE, true);
        // EDGE_SLIDE is absent.
        t.sphere_path.save_check_pos();
        let mut state = TransitionState::Collided;
        assert!(edge_slide(&c, &mut t, &mut state, 0.3, LANDING_Z));
        assert_eq!(
            state,
            TransitionState::Ok,
            "blocked but OK: the object walks off the edge"
        );
    }

    #[test]
    fn cliff_slide_picks_a_horizontal_axis_and_reports_adjusted() {
        let w = world();
        let mut t = standing(&w, 20.5);
        t.collision_info.last_known_contact_plane = Plane {
            normal: Vec3::new(0.0, 0.0, 1.0),
            d: -20.0,
        };
        t.collision_info.last_known_contact_plane_valid = true;
        // A 60-degree slope facing +X.
        let steep = Plane {
            normal: Vec3::new(0.866, 0.0, 0.5),
            d: 0.0,
        };
        t.sphere_path
            .add_offset_to_check_pos(Vec3::new(0.5, 0.0, 0.0));
        let r = cliff_slide(&mut t, &steep);
        assert_eq!(r, TransitionState::Adjusted);
        assert!(t.collision_info.collision_normal_valid);
        assert!(
            t.collision_info.collision_normal.z.abs() < 1e-5,
            "the cliff-slide axis is horizontal: {:?}",
            t.collision_info.collision_normal
        );
    }

    #[test]
    fn cliff_slide_is_a_no_op_when_the_two_planes_are_parallel() {
        let w = world();
        let mut t = standing(&w, 20.5);
        t.collision_info.last_known_contact_plane = Plane {
            normal: Vec3::new(0.0, 0.0, 1.0),
            d: -20.0,
        };
        t.collision_info.last_known_contact_plane_valid = true;
        let same = Plane {
            normal: Vec3::new(0.0, 0.0, 1.0),
            d: -20.0,
        };
        assert_eq!(cliff_slide(&mut t, &same), TransitionState::Ok);
    }

    #[test]
    fn precipice_slide_without_a_walkable_polygon_is_a_collision() {
        let w = world();
        let mut t = standing(&w, 20.5);
        assert_eq!(precipice_slide(&mut t), TransitionState::Collided);
    }
}
