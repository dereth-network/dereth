use super::*;
use crate::arena::Arena;
use crate::land::{LandblockCollision, VERTEX_COUNT};
use crate::source::LandSource as _;
use crate::source::StaticLandSource;
use dereth_primitives::{Frame, LandblockId, Position, Quat};
use std::collections::BTreeMap;
use std::sync::Arc;

// Oracle: the retail sphere code (slide_sphere, land_on_sphere and
// step_sphere_down) and the validate_walkable description in
// the recovered collision and transition behavior.

fn flat_world() -> (
    StaticLandSource,
    Arena<crate::obj::PhysicsObj>,
    BTreeMap<u32, crate::cell::CellRuntime>,
) {
    let mut s = StaticLandSource::linear();
    s.add_flat_block(LandblockId::new(0xA9, 0xB4), 10); // z = 20
    (s, Arena::new(), BTreeMap::new())
}

macro_rules! bare_ctx {
    ($name:ident) => {
        let (land_, objects_, cells_) = flat_world();
        let $name = TransitionCtx {
            land: &land_,
            objects: &objects_,
            cells: &cells_,
            mover: None,
            object_table: None,
            entry_host: None,
        };
    };
}

fn transition_on_flat_ground(z: f32) -> (Transition, LandblockCollision) {
    let mut t = Transition::default();
    t.sphere_path
        .init_sphere(&[Sphere::new(Vec3::new(0.0, 0.0, 0.5), 0.5)], 1.0);
    let cell = LandblockId::new(0xA9, 0xB4).cell(1);
    let pos = Position::new(cell, Frame::new(Vec3::new(12.0, 12.0, z), Quat::IDENTITY));
    t.sphere_path.init_path(Some(cell), Some(pos), &pos);
    t.sphere_path.set_check_pos(&pos, Some(cell));
    let mut table = [0.0_f32; 256];
    for (i, v) in table.iter_mut().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        {
            *v = i as f32 * 2.0;
        }
    }
    let block = LandblockCollision::build(
        LandblockId::new(0xA9, 0xB4),
        Box::new([10; VERTEX_COUNT]),
        Box::new([0; VERTEX_COUNT]),
        false,
        8,
        &table,
    )
    .expect("full detail");
    (t, block)
}

#[test]
fn validate_walkable_reports_contact_within_the_epsilon_band() {
    // The ground is at z = 20. The object's sphere sits at local (0, 0, 0.5) with radius 0.5,
    // so an origin of exactly 20.0 puts the sphere's bottom on the plane.
    let (mut t, block) = transition_on_flat_ground(20.0);
    let cell = Cell::Land {
        id: LandblockId::new(0xA9, 0xB4).cell(1),
        block: Arc::new(block),
    };
    let r = land_find_env_collisions(&mut t, &cell);
    assert_eq!(r, TransitionState::Ok);
    assert!(
        t.collision_info.contact_plane_valid,
        "a contact plane must be recorded"
    );
    assert!(crate::is_valid_walkable(
        t.collision_info.contact_plane.normal
    ));
}

#[test]
fn validate_walkable_is_silent_when_clearly_above_the_ground() {
    let (mut t, block) = transition_on_flat_ground(25.0);
    let cell = Cell::Land {
        id: LandblockId::new(0xA9, 0xB4).cell(1),
        block: Arc::new(block),
    };
    assert_eq!(land_find_env_collisions(&mut t, &cell), TransitionState::Ok);
    assert!(
        !t.collision_info.contact_plane_valid,
        "no contact 4.5 m above the ground"
    );
}

#[test]
fn validate_walkable_pushes_a_sunken_sphere_straight_up() {
    // Bottom 0.2 m below the ground: the fix-up is vertical only.
    let (mut t, block) = transition_on_flat_ground(19.8);
    let cell = Cell::Land {
        id: LandblockId::new(0xA9, 0xB4).cell(1),
        block: Arc::new(block),
    };
    let before = t.sphere_path.check_pos.frame.origin;
    let r = land_find_env_collisions(&mut t, &cell);
    assert_eq!(r, TransitionState::Adjusted);
    let after = t.sphere_path.check_pos.frame.origin;
    assert_eq!(after.x, before.x, "the push is along Z only");
    assert_eq!(after.y, before.y);
    assert!(
        (after.z - 20.0).abs() < 1e-3,
        "pushed up to rest on the plane: {after:?}"
    );
    assert!(t.collision_info.collided_with_environment);
}

/// Deep sea is a hard `COLLIDED_TS` for anything that is not a viewer or a
/// missile — **not** "swim".
#[test]
fn an_entirely_water_landblock_is_a_hard_stop() {
    let mut table = [0.0_f32; 256];
    for (i, v) in table.iter_mut().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        {
            *v = i as f32 * 2.0;
        }
    }
    let water = 18_u16 << 2; // WaterShallowSea
    let block = LandblockCollision::build(
        LandblockId::new(0xA9, 0xB4),
        Box::new([10; VERTEX_COUNT]),
        Box::new([water; VERTEX_COUNT]),
        false,
        8,
        &table,
    )
    .expect("full detail");
    assert_eq!(block.water_type, WaterType::EntirelyWater);
    let cell = Cell::Land {
        id: LandblockId::new(0xA9, 0xB4).cell(1),
        block: Arc::new(block),
    };

    let (mut t, _) = transition_on_flat_ground(25.0);
    assert_eq!(
        land_find_env_collisions(&mut t, &cell),
        TransitionState::Collided,
        "an ordinary object cannot walk into deep sea"
    );

    // A viewer passes.
    let (mut t, _) = transition_on_flat_ground(25.0);
    t.object_info.set(ObjectInfoState::IS_VIEWER, true);
    assert_ne!(
        land_find_env_collisions(&mut t, &cell),
        TransitionState::Collided
    );

    // So does a missile.
    let (mut t, _) = transition_on_flat_ground(25.0);
    t.object_info.set(ObjectInfoState::PATH_CLIPPED, true);
    assert_ne!(
        land_find_env_collisions(&mut t, &cell),
        TransitionState::Collided
    );
}

#[test]
fn slide_sphere_with_a_zero_normal_halves_the_remaining_offset() {
    let (mut t, _) = transition_on_flat_ground(20.5);
    t.sphere_path
        .add_offset_to_check_pos(Vec3::new(2.0, 0.0, 0.0));
    let sphere = t.sphere_path.global_sphere[0];
    let curr = t.sphere_path.global_curr_center[0];
    let before = t.sphere_path.check_pos.frame.origin;
    let r = slide_sphere(&mut t, sphere, Vec3::ZERO, curr);
    assert_eq!(r, TransitionState::Adjusted);
    let after = t.sphere_path.check_pos.frame.origin;
    assert!(
        (after.x - (before.x - 1.0)).abs() < 1e-4,
        "{before:?} -> {after:?}"
    );
}

#[test]
fn slide_sphere_projects_movement_onto_the_wall_floor_crease() {
    let (mut t, _) = transition_on_flat_ground(20.5);
    // A floor contact plane and a wall normal facing -X: the crease runs along Y.
    t.collision_info.set_contact_plane(
        Plane {
            normal: Vec3::new(0.0, 0.0, 1.0),
            d: -20.0,
        },
        false,
    );
    t.sphere_path
        .add_offset_to_check_pos(Vec3::new(1.0, 1.0, 0.0));
    let sphere = t.sphere_path.global_sphere[0];
    let curr = t.sphere_path.global_curr_center[0];
    let r = slide_sphere(&mut t, sphere, Vec3::new(-1.0, 0.0, 0.0), curr);
    assert_eq!(r, TransitionState::Slid);
    // The X component of the move is removed, the Y component survives.
    let moved = t
        .sphere_path
        .check_pos
        .frame
        .origin
        .sub(Vec3::new(12.0, 12.0, 20.5));
    assert!(moved.x.abs() < 1e-4, "{moved:?}");
    assert!((moved.y - 1.0).abs() < 1e-4, "{moved:?}");
}

#[test]
fn land_on_sphere_arms_the_re_validation_and_resets_the_probe_budget() {
    let (mut t, _) = transition_on_flat_ground(21.0);
    t.sphere_path.walk_interp = 0.3;
    let r = land_on_sphere(&mut t, Vec3::new(12.0, 12.0, 19.0));
    assert_eq!(r, TransitionState::Adjusted);
    assert!(
        t.sphere_path.collide,
        "set_collide arms the landing re-check"
    );
    assert_eq!(
        t.sphere_path.walk_interp, 1.0,
        "and resets the step-down budget"
    );
    assert_eq!(t.sphere_path.walkable_allowance, LANDING_Z);
    assert!(
        t.sphere_path.step_up_normal.z > 0.9,
        "the landing normal points up"
    );
}

/// The step up budget is compared strictly so an exact tie is climbed.
#[test]
fn the_step_up_budget_is_compared_strictly_so_an_exact_tie_is_climbed() {
    let sum = 3.4998_f32;
    let delta = Vec3::new(2.0, 0.0, 2.86);
    // The branch's own arithmetic, in the branch's own precision.
    let tie = (sum + EPSILON) - delta.z;

    let attempt = |budget: f32| {
        let (mut t, _) = transition_on_flat_ground(20.5);
        bare_ctx!(ctx);
        t.object_info.step_up_height = budget;
        t.object_info.set(ObjectInfoState::ON_WALKABLE, true);
        let obstacle = t.sphere_path.global_curr_center[0].sub(delta);
        assert_eq!(
            t.sphere_path.step_up_normal,
            Vec3::ZERO,
            "the discriminator must start clear, or it cannot discriminate"
        );
        let _ = step_sphere_up(&ctx, &mut t, obstacle, delta, sum);
        t.sphere_path.step_up_normal != Vec3::ZERO
    };

    assert!(
        attempt(tie),
        "a budget exactly equal to (sum + EPSILON) - delta.z is NOT less than it, so the \
             client attempts the lift: `<`, not `<=`"
    );
    // One ULP under the tie, and the same call slides instead. Nothing else about the
    // geometry moved, so the comparison is the only thing that can have decided it.
    let under = f32::from_bits(tie.to_bits() - 1);
    assert!(
        under < tie && (tie - under) < 1e-6,
        "one ULP: {under} against {tie}"
    );
    assert!(
        !attempt(under),
        "one unit in the last place under the tie the budget IS less, and the client slides"
    );
    // And well over it, to say the predicate is not simply always-lift.
    assert!(
        attempt(tie + 1.0),
        "a budget a metre over the tie still climbs"
    );
    assert!(
        !attempt(tie - 1.0),
        "a budget a metre under it still slides"
    );
}

#[test]
fn land_on_sphere_refuses_a_degenerate_normal() {
    let (mut t, _) = transition_on_flat_ground(21.0);
    // The obstacle's centre coincides with the mover's current centre.
    let c = t.sphere_path.global_curr_center[0];
    assert_eq!(land_on_sphere(&mut t, c), TransitionState::Collided);
}

#[test]
fn find_obj_collisions_routes_a_static_hit_to_the_environment() {
    let (land, mut objects, cells) = flat_world();
    let cell = LandblockId::new(0xA9, 0xB4).cell(1);
    let mut statik = crate::obj::PhysicsObj::new(
        dereth_primitives::ObjectId(9),
        Arc::new(crate::source::SetupGeometry {
            spheres: vec![Sphere::new(Vec3::new(0.0, 0.0, 0.5), 0.5)],
            ..crate::source::SetupGeometry::default()
        }),
        0.0,
        false,
    );
    statik.position = Position::new(
        cell,
        Frame::new(Vec3::new(12.0, 12.0, 20.0), Quat::IDENTITY),
    );
    assert!(statik.state.is_static());
    let h = objects.insert(statik);

    let (mut t, _) = transition_on_flat_ground(20.5);
    let ctx = TransitionCtx {
        land: &land,
        objects: &objects,
        cells: &cells,
        mover: None,
        object_table: None,
        entry_host: None,
    };
    let o = ctx.objects.get(h).expect("live");
    let r = find_obj_collisions(
        &ctx,
        &mut t,
        h,
        o.state,
        o.geometry.path_spheres(),
        o.scale,
        &o.position,
    );
    assert_ne!(r, TransitionState::Ok, "the spheres overlap");
    assert!(
        t.collision_info.collided_with_environment,
        "a STATIC_PS object reports as environment"
    );
    assert!(
        t.collision_info.collide_object.is_empty(),
        "and not as an object collision"
    );
}

#[test]
fn find_obj_collisions_records_a_dynamic_hit_as_an_object() {
    let (land, mut objects, cells) = flat_world();
    let cell = LandblockId::new(0xA9, 0xB4).cell(1);
    let mut dyno = crate::obj::PhysicsObj::new(
        dereth_primitives::ObjectId(9),
        Arc::new(crate::source::SetupGeometry {
            spheres: vec![Sphere::new(Vec3::new(0.0, 0.0, 0.5), 0.5)],
            ..crate::source::SetupGeometry::default()
        }),
        0.0,
        true,
    );
    dyno.position = Position::new(
        cell,
        Frame::new(Vec3::new(12.0, 12.0, 20.0), Quat::IDENTITY),
    );
    let h = objects.insert(dyno);

    let (mut t, _) = transition_on_flat_ground(20.5);
    let ctx = TransitionCtx {
        land: &land,
        objects: &objects,
        cells: &cells,
        mover: None,
        object_table: None,
        entry_host: None,
    };
    let o = ctx.objects.get(h).expect("live");
    let r = find_obj_collisions(
        &ctx,
        &mut t,
        h,
        o.state,
        o.geometry.path_spheres(),
        o.scale,
        &o.position,
    );
    assert_ne!(r, TransitionState::Ok);
    assert_eq!(t.collision_info.collide_object.len(), 1);
    assert_eq!(t.collision_info.last_collided_object, Some(h));
}

#[test]
fn an_ethereal_pair_passes_through_and_is_recorded_as_ok() {
    let (land, mut objects, cells) = flat_world();
    let cell = LandblockId::new(0xA9, 0xB4).cell(1);
    let mut ghost = crate::obj::PhysicsObj::new(
        dereth_primitives::ObjectId(9),
        Arc::new(crate::source::SetupGeometry {
            spheres: vec![Sphere::new(Vec3::new(0.0, 0.0, 0.5), 0.5)],
            ..crate::source::SetupGeometry::default()
        }),
        0.0,
        true,
    );
    ghost.state.set_ethereal_bit(true);
    ghost.position = Position::new(
        cell,
        Frame::new(Vec3::new(12.0, 12.0, 20.0), Quat::IDENTITY),
    );
    let h = objects.insert(ghost);

    let (mut t, _) = transition_on_flat_ground(20.5);
    let ctx = TransitionCtx {
        land: &land,
        objects: &objects,
        cells: &cells,
        mover: None,
        object_table: None,
        entry_host: None,
    };
    let o = ctx.objects.get(h).expect("live");
    let r = find_obj_collisions(
        &ctx,
        &mut t,
        h,
        o.state,
        o.geometry.path_spheres(),
        o.scale,
        &o.position,
    );
    assert_eq!(r, TransitionState::Ok, "an ethereal obstacle never blocks");
    assert_eq!(
        t.collision_info.collide_object.len(),
        1,
        "but it is still reported"
    );
    assert!(!t.collision_info.collision_normal_valid);
}

#[test]
fn an_object_that_is_both_ethereal_and_ignores_collisions_is_invisible_to_the_test() {
    let (land, mut objects, cells) = flat_world();
    let cell = LandblockId::new(0xA9, 0xB4).cell(1);
    let mut ghost = crate::obj::PhysicsObj::new(
        dereth_primitives::ObjectId(9),
        Arc::new(crate::source::SetupGeometry {
            spheres: vec![Sphere::new(Vec3::new(0.0, 0.0, 0.5), 0.5)],
            ..crate::source::SetupGeometry::default()
        }),
        0.0,
        true,
    );
    ghost.state.set_ethereal_bit(true);
    ghost.state.set_ignores_collisions(true);
    ghost.position = Position::new(
        cell,
        Frame::new(Vec3::new(12.0, 12.0, 20.0), Quat::IDENTITY),
    );
    let h = objects.insert(ghost);

    let (mut t, _) = transition_on_flat_ground(20.5);
    let ctx = TransitionCtx {
        land: &land,
        objects: &objects,
        cells: &cells,
        mover: None,
        object_table: None,
        entry_host: None,
    };
    let o = ctx.objects.get(h).expect("live");
    let r = find_obj_collisions(
        &ctx,
        &mut t,
        h,
        o.state,
        o.geometry.path_spheres(),
        o.scale,
        &o.position,
    );
    assert_eq!(r, TransitionState::Ok);
    assert!(
        t.collision_info.collide_object.is_empty(),
        "not even reported"
    );
}

// ---- the object walk (the cell's own object-collision search) -------------------------
//
// Oracle: the retail body, which is a three-clause skip and a call:
//
//     if (insert_type != InitialPlacement && num_shadow_objects != 0) do {
//         other = shadow_object_list[i].physobj;
//         if (other.parent == NULL && other != object_info.object &&
//             (r = find_obj_collisions(other, t)) != OK_TS) return r;
//     } while (++i < num_shadow_objects);
//
// Every one of these asserts on [`CollisionCounters`] rather than on the result alone,
// because "skipped it" and "never looked at it" produce the same `OK_TS`.

/// A world with one cell, one shadow list, and a solid static obstacle overlapping the mover.
fn a_cell_with_one_obstacle() -> (
    StaticLandSource,
    Arena<crate::obj::PhysicsObj>,
    BTreeMap<u32, crate::cell::CellRuntime>,
    crate::arena::PhysHandle,
    CellId,
) {
    let (land, mut objects, mut cells) = flat_world();
    let cell = LandblockId::new(0xA9, 0xB4).cell(1);
    let mut statik = crate::obj::PhysicsObj::new(
        dereth_primitives::ObjectId(9),
        Arc::new(crate::source::SetupGeometry {
            spheres: vec![Sphere::new(Vec3::new(0.0, 0.0, 0.5), 0.5)],
            ..crate::source::SetupGeometry::default()
        }),
        0.0,
        false,
    );
    statik.position = Position::new(
        cell,
        Frame::new(Vec3::new(12.0, 12.0, 20.0), Quat::IDENTITY),
    );
    let h = objects.insert(statik);
    cells.entry(cell.0).or_default().shadow_object_list.push(h);
    (land, objects, cells, h, cell)
}

#[test]
fn the_object_walk_reaches_a_shadowed_obstacle_and_is_stopped_by_it() {
    let (land, objects, cells, _h, cell) = a_cell_with_one_obstacle();
    let (mut t, _) = transition_on_flat_ground(20.5);
    let ctx = TransitionCtx {
        land: &land,
        objects: &objects,
        cells: &cells,
        mover: None,
        object_table: None,
        entry_host: None,
    };
    assert_ne!(
        cell_find_obj_collisions(&ctx, &mut t, cell),
        TransitionState::Ok
    );
    assert_eq!(t.counters.shadows_visited, 1);
    assert_eq!(
        t.counters.objects_tested, 1,
        "the candidate reached FindObjCollisions"
    );
    assert_eq!(t.counters.skipped_self + t.counters.skipped_parented, 0);
    assert_eq!(t.counters.shadow_dangling, 0);
}

#[test]
fn the_object_walk_skips_the_mover_itself_and_records_it_as_a_skip() {
    let (land, objects, cells, h, cell) = a_cell_with_one_obstacle();
    let (mut t, _) = transition_on_flat_ground(20.5);
    // Exactly the obstacle that stopped the previous test, now named as the mover.
    let ctx = TransitionCtx {
        land: &land,
        objects: &objects,
        cells: &cells,
        mover: Some(h),
        object_table: None,
        entry_host: None,
    };
    assert_eq!(
        cell_find_obj_collisions(&ctx, &mut t, cell),
        TransitionState::Ok
    );
    assert_eq!(t.counters.shadows_visited, 1, "the entry was visited");
    assert_eq!(t.counters.skipped_self, 1, "and skipped as the mover");
    assert_eq!(t.counters.objects_tested, 0, "so nothing was tested");
    assert!(t.collision_info.collide_object.is_empty());
}

#[test]
fn the_object_walk_skips_the_transitions_own_object_even_without_a_mover_handle() {
    // `other != transition.object_info.object`: the client's test is against the
    // transition's object set during initialization, not against a separate argument.
    let (land, objects, cells, h, cell) = a_cell_with_one_obstacle();
    let (mut t, _) = transition_on_flat_ground(20.5);
    t.object_info.object = Some(h);
    let ctx = TransitionCtx {
        land: &land,
        objects: &objects,
        cells: &cells,
        mover: None,
        object_table: None,
        entry_host: None,
    };
    assert_eq!(
        cell_find_obj_collisions(&ctx, &mut t, cell),
        TransitionState::Ok
    );
    assert_eq!(t.counters.skipped_self, 1);
    assert_eq!(t.counters.objects_tested, 0);
}

#[test]
fn the_object_walk_skips_a_parented_object_and_records_it_as_a_skip() {
    let (land, mut objects, cells, h, cell) = a_cell_with_one_obstacle();
    // A holder for it to hang off. Anything with a live handle will do: the client's test is
    // whether the object has a parent at all, not a property of the parent.
    let holder = objects.insert(crate::obj::PhysicsObj::new(
        dereth_primitives::ObjectId(10),
        Arc::new(crate::source::SetupGeometry::dummy()),
        0.0,
        true,
    ));
    objects.get_mut(h).expect("live").parent = Some(holder);

    let (mut t, _) = transition_on_flat_ground(20.5);
    let ctx = TransitionCtx {
        land: &land,
        objects: &objects,
        cells: &cells,
        mover: None,
        object_table: None,
        entry_host: None,
    };
    assert_eq!(
        cell_find_obj_collisions(&ctx, &mut t, cell),
        TransitionState::Ok,
        "a held object collides through its holder, never on its own"
    );
    assert_eq!(t.counters.shadows_visited, 1);
    assert_eq!(t.counters.skipped_parented, 1);
    assert_eq!(t.counters.objects_tested, 0);
}

#[test]
fn a_shadow_handle_whose_object_is_gone_is_counted_rather_than_silently_dropped() {
    // Unreachable in the client, whose shadow list holds raw pointers. Here it would be a bug
    // in the cell bookkeeping, and the counter is what lets the integration tests assert it
    // never happens.
    let (land, mut objects, cells, h, cell) = a_cell_with_one_obstacle();
    objects.remove(h).expect("it was there");
    let (mut t, _) = transition_on_flat_ground(20.5);
    let ctx = TransitionCtx {
        land: &land,
        objects: &objects,
        cells: &cells,
        mover: None,
        object_table: None,
        entry_host: None,
    };
    assert_eq!(
        cell_find_obj_collisions(&ctx, &mut t, cell),
        TransitionState::Ok
    );
    assert_eq!(t.counters.shadow_dangling, 1);
    assert_eq!(t.counters.objects_tested, 0);
}

#[test]
fn initial_placement_skips_a_walk_that_would_otherwise_have_collided() {
    // The companion of `initial_placement_skips_the_object_pass_entirely`, which shows the
    // early return over an *empty* list. This one puts a solid obstacle in the list first, so
    // the skip is what the `OK_TS` is made of.
    let (land, objects, cells, _h, cell) = a_cell_with_one_obstacle();
    let ctx = TransitionCtx {
        land: &land,
        objects: &objects,
        cells: &cells,
        mover: None,
        object_table: None,
        entry_host: None,
    };

    let (mut t, _) = transition_on_flat_ground(20.5);
    assert_ne!(
        cell_find_obj_collisions(&ctx, &mut t, cell),
        TransitionState::Ok
    );

    let (mut t, _) = transition_on_flat_ground(20.5);
    t.sphere_path.insert_type = InsertType::InitialPlacement;
    assert_eq!(
        cell_find_obj_collisions(&ctx, &mut t, cell),
        TransitionState::Ok
    );
    assert_eq!(t.counters.obj_walk_skipped_initial_placement, 1);
    assert_eq!(t.counters.shadows_visited, 0, "the list is never entered");
}

#[test]
fn a_step_down_probe_past_an_ethereal_obstacle_leaves_obstruction_ethereal_alone() {
    // the `step_down` early `return OK_TS` sits
    // *inside* the else-branch that set the ethereal flag to 1, two statements ahead of
    // the store to `sphere_path.obstruction_ethereal`. The flag therefore keeps whatever the
    // previous candidate left it at, and a rebuild that sets it here leaks an ethereal
    // obstruction into the next, whose placement branch is gated on
    // exactly that flag.
    let (land, mut objects, cells) = flat_world();
    let cell = LandblockId::new(0xA9, 0xB4).cell(1);
    let mut ghost = crate::obj::PhysicsObj::new(
        dereth_primitives::ObjectId(9),
        Arc::new(crate::source::SetupGeometry {
            spheres: vec![Sphere::new(Vec3::new(0.0, 0.0, 0.5), 0.5)],
            ..crate::source::SetupGeometry::default()
        }),
        0.0,
        true,
    );
    ghost.state.set_ethereal_bit(true);
    ghost.position = Position::new(
        cell,
        Frame::new(Vec3::new(12.0, 12.0, 20.0), Quat::IDENTITY),
    );
    let h = objects.insert(ghost);

    let (mut t, _) = transition_on_flat_ground(20.5);
    t.sphere_path.step_down = true;
    t.sphere_path.obstruction_ethereal = false;
    let ctx = TransitionCtx {
        land: &land,
        objects: &objects,
        cells: &cells,
        mover: None,
        object_table: None,
        entry_host: None,
    };
    let o = ctx.objects.get(h).expect("live");
    assert_eq!(
        find_obj_collisions(
            &ctx,
            &mut t,
            h,
            o.state,
            o.geometry.path_spheres(),
            o.scale,
            &o.position,
        ),
        TransitionState::Ok,
        "an ethereal thing is never ground"
    );
    assert!(
        !t.sphere_path.obstruction_ethereal,
        "the client returns before the store; the flag must be untouched"
    );
}

// ---- the building shell -----------------------------------------------------------------
//
// Oracle: the retail building, part and graphics-object collision searches. The retail-geometry half is `tests/dat/collision/building_shells.rs`.

/// A wall standing in the part's local +X = outside, -X = inside. The solid leaf carries one
/// small square polygon at `x = 0` spanning `y in [-1, 1]`, `z in [0, 2]`, so "behind the
/// splitting plane" and "actually touching the wall" are different places — which is what the
/// `bldg_check` tests need.
fn wall_tree() -> crate::geom::BspTree {
    use crate::geom::bsp::{BspNode, BspNodeKind};
    let wall = crate::geom::Polygon::new(vec![
        Vec3::new(0.0, -1.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::new(0.0, 1.0, 2.0),
        Vec3::new(0.0, -1.0, 2.0),
    ]);
    let big = Sphere::new(Vec3::ZERO, 100.0);
    let far = Plane {
        normal: Vec3::new(0.0, 0.0, 1.0),
        d: 1000.0,
    };
    crate::geom::BspTree {
        nodes: vec![
            BspNode {
                sphere: big,
                splitting_plane: Plane {
                    normal: Vec3::new(1.0, 0.0, 0.0),
                    d: 0.0,
                },
                pos_child: Some(1),
                neg_child: Some(2),
                kind: BspNodeKind::Node,
                in_polys: vec![],
            },
            BspNode {
                sphere: big,
                splitting_plane: far,
                pos_child: None,
                neg_child: None,
                kind: BspNodeKind::Leaf {
                    leaf_index: 0,
                    solid: false,
                },
                in_polys: vec![],
            },
            BspNode {
                sphere: big,
                splitting_plane: far,
                pos_child: None,
                neg_child: None,
                kind: BspNodeKind::Leaf {
                    leaf_index: 1,
                    solid: true,
                },
                in_polys: vec![0],
            },
        ],
        polygons: vec![wall],
    }
}

/// One unscaled physics part at `origin`, carrying [`wall_tree`].
fn wall_part(origin: Vec3) -> PhysicsPart {
    PhysicsPart {
        pos: Position::new(
            LandblockId::new(0xA9, 0xB4).cell(1),
            Frame::new(origin, Quat::IDENTITY),
        ),
        gfxobj_scale: 1.0,
        physics_bsp: Some(Arc::new(wall_tree())),
        bound_box: None,
        drawing_sphere: None,
        first_degrade_mode: None,
    }
}

/// An arena holding one ordinary dynamic object, to stand in for the mover.
fn a_mover() -> (Arena<crate::obj::PhysicsObj>, crate::arena::PhysHandle) {
    let mut objects = Arena::new();
    let h = objects.insert(crate::obj::PhysicsObj::new(
        dereth_primitives::ObjectId(1),
        Arc::new(crate::source::SetupGeometry::dummy()),
        0.0,
        true,
    ));
    (objects, h)
}

/// A transition whose single sphere is centred at `(12, 12, 21)`, set up for the
/// `PLACEMENT_INSERT` branch of — the one `bldg_check` steers.
fn a_placement_at_the_wall() -> Transition {
    let (mut t, _) = transition_on_flat_ground(20.5);
    t.sphere_path.insert_type = InsertType::Placement;
    t
}

#[test]
fn the_building_shell_stops_a_sphere_that_is_inside_its_geometry() {
    bare_ctx!(ctx);
    let mut t = a_placement_at_the_wall();
    // The wall's origin one metre east of the sphere centre: local x = -1, i.e. indoors.
    let b = BuildingGeometry {
        parts: vec![wall_part(Vec3::new(13.0, 12.0, 20.0))],
    };
    assert_eq!(
        find_building_collisions(&ctx, &mut t, &b),
        TransitionState::Collided
    );
    assert!(
        t.collision_info.collided_with_environment,
        "a building reports as environment"
    );
    assert_eq!(t.counters.buildings_bsp_walked, 1);
    assert_eq!(t.counters.buildings_sphere_pruned, 0);
    assert!(
        !t.sphere_path.bldg_check,
        "the flag is lowered again on the way out"
    );
}

#[test]
fn the_building_shell_lets_a_sphere_outside_its_geometry_through() {
    bare_ctx!(ctx);
    let mut t = a_placement_at_the_wall();
    // Origin one metre *west*: local x = +1, outdoors, in the empty leaf.
    let b = BuildingGeometry {
        parts: vec![wall_part(Vec3::new(11.0, 12.0, 20.0))],
    };
    assert_eq!(
        find_building_collisions(&ctx, &mut t, &b),
        TransitionState::Ok
    );
    assert!(!t.collision_info.collided_with_environment);
    assert_eq!(
        t.counters.buildings_bsp_walked, 1,
        "the tree was walked and said no"
    );
}

#[test]
fn bldg_check_and_hits_interior_cell_together_stop_the_walls_pushing_an_occupant_out() {
    bare_ctx!(ctx);
    // Local (-1, 5, 1): behind the splitting plane, so in the solid leaf, but four metres
    // from the wall polygon itself. That is where somebody standing inside the building is.
    let outside_the_wall = Vec3::new(13.0, 7.0, 20.0);

    // Not in an interior cell: `clear_cell` is true, the solid leaf answers on its flag alone
    // and the placement is refused.
    let mut t = a_placement_at_the_wall();
    t.sphere_path.hits_interior_cell = false;
    let b = BuildingGeometry {
        parts: vec![wall_part(outside_the_wall)],
    };
    assert_eq!(
        find_building_collisions(&ctx, &mut t, &b),
        TransitionState::Collided
    );

    // In an interior cell: `bldg_check` makes `clear_cell` follow `!hits_interior_cell`, the
    // solid-leaf shortcut is skipped, and only real polygon contact counts.
    let mut t = a_placement_at_the_wall();
    t.sphere_path.hits_interior_cell = true;
    assert_eq!(
        find_building_collisions(&ctx, &mut t, &b),
        TransitionState::Ok,
        "an occupant of the building is not shoved out through its own walls"
    );
}

#[test]
fn only_part_zero_of_the_building_is_consulted() {
    bare_ctx!(ctx);
    // `find_building_collisions` indexes the part array's first part; it does **not** call
    // which would walk all of them.
    let empty = PhysicsPart {
        physics_bsp: None,
        ..PhysicsPart::default()
    };
    let wall = wall_part(Vec3::new(13.0, 12.0, 20.0));

    let mut t = a_placement_at_the_wall();
    let b = BuildingGeometry {
        parts: vec![empty.clone(), wall.clone()],
    };
    assert_eq!(
        find_building_collisions(&ctx, &mut t, &b),
        TransitionState::Ok,
        "part 1 collides and is never looked at"
    );
    assert_eq!(t.counters.building_parts_without_bsp, 1);
    assert_eq!(t.counters.buildings_bsp_walked, 0);

    // The same two parts the other way round.
    let mut t = a_placement_at_the_wall();
    let b = BuildingGeometry {
        parts: vec![wall, empty],
    };
    assert_eq!(
        find_building_collisions(&ctx, &mut t, &b),
        TransitionState::Collided
    );
    assert_eq!(t.counters.building_parts_without_bsp, 0);
    assert_eq!(t.counters.buildings_bsp_walked, 1);
}

#[test]
fn a_building_with_no_part_array_answers_ok_without_touching_bldg_check() {
    bare_ctx!(ctx);
    let mut t = a_placement_at_the_wall();
    // The client's `if (part_array != NULL)` guards the whole body, the two `bldg_check`
    // writes included. A pre-set flag therefore survives.
    t.sphere_path.bldg_check = true;
    let b = BuildingGeometry::default();
    assert_eq!(
        find_building_collisions(&ctx, &mut t, &b),
        TransitionState::Ok
    );
    assert!(
        t.sphere_path.bldg_check,
        "neither the raise nor the clear ran"
    );
    assert_eq!(t.counters.buildings_without_parts, 1);
}

#[test]
fn collided_with_environment_needs_the_collision_and_the_absence_of_contact() {
    bare_ctx!(ctx);
    let b = BuildingGeometry {
        parts: vec![wall_part(Vec3::new(13.0, 12.0, 20.0))],
    };

    // Collided, not in contact -> set.
    let mut t = a_placement_at_the_wall();
    assert_eq!(
        find_building_collisions(&ctx, &mut t, &b),
        TransitionState::Collided
    );
    assert!(t.collision_info.collided_with_environment);

    // Collided, already in contact -> not set. `(object_info.state & 1) == 0` is the whole
    // condition; CONTACT is bit 0.
    let mut t = a_placement_at_the_wall();
    t.object_info.state |= ObjectInfoState::CONTACT;
    assert_eq!(
        find_building_collisions(&ctx, &mut t, &b),
        TransitionState::Collided
    );
    assert!(
        !t.collision_info.collided_with_environment,
        "an object already in contact does not re-report the environment"
    );

    // Not collided, not in contact -> not set.
    let mut t = a_placement_at_the_wall();
    let away = BuildingGeometry {
        parts: vec![wall_part(Vec3::new(11.0, 12.0, 20.0))],
    };
    assert_eq!(
        find_building_collisions(&ctx, &mut t, &away),
        TransitionState::Ok
    );
    assert!(!t.collision_info.collided_with_environment);
}

#[test]
fn the_bounding_sphere_prune_can_skip_the_tree_entirely() {
    bare_ctx!(ctx);
    // tests `physics_sphere` — the root node's own sphere —
    // before it walks anything. A part 200 m away is pruned.
    let mut t = a_placement_at_the_wall();
    let b = BuildingGeometry {
        parts: vec![wall_part(Vec3::new(212.0, 12.0, 20.0))],
    };
    assert_eq!(
        find_building_collisions(&ctx, &mut t, &b),
        TransitionState::Ok
    );
    assert_eq!(t.counters.buildings_sphere_pruned, 1);
    assert_eq!(
        t.counters.buildings_bsp_walked, 0,
        "the tree was never entered"
    );
}

#[test]
fn sort_cell_find_collisions_is_a_null_check_on_the_cells_own_building() {
    let mut land = StaticLandSource::linear();
    land.add_flat_block(LandblockId::new(0xA9, 0xB4), 10);
    let with_building = LandblockId::new(0xA9, 0xB4).cell(1);
    let without = LandblockId::new(0xA9, 0xB4).cell(2);
    land.add_building(
        with_building,
        BuildingGeometry {
            parts: vec![wall_part(Vec3::new(13.0, 12.0, 20.0))],
        },
    );
    let objects = Arena::new();
    let cells = BTreeMap::new();
    let ctx = TransitionCtx {
        land: &land,
        objects: &objects,
        cells: &cells,
        mover: None,
        object_table: None,
        entry_host: None,
    };

    let mut t = a_placement_at_the_wall();
    assert_eq!(
        sort_cell_find_collisions(&ctx, &mut t, without),
        TransitionState::Ok
    );
    assert_eq!(t.counters.buildings_visited, 0, "no building, no call");

    let mut t = a_placement_at_the_wall();
    assert_eq!(
        sort_cell_find_collisions(&ctx, &mut t, with_building),
        TransitionState::Collided
    );
    assert_eq!(t.counters.buildings_visited, 1);
}

#[test]
fn add_building_keeps_the_first_offer_and_drops_the_second() {
    // Only an empty building slot takes the offered building.
    let mut land = StaticLandSource::linear();
    land.add_flat_block(LandblockId::new(0xA9, 0xB4), 10);
    let cell = LandblockId::new(0xA9, 0xB4).cell(1);
    land.add_building(cell, BuildingGeometry::default());
    land.add_building(
        cell,
        BuildingGeometry {
            parts: vec![wall_part(Vec3::new(13.0, 12.0, 20.0))],
        },
    );
    let b = land.building(cell).expect("the first one is there");
    assert!(b.parts.is_empty(), "the second add_building was dropped");
}

#[test]
fn cell_find_collisions_runs_the_building_shell_for_a_land_cell() {
    // = find_env_collisions, then
    // sort-cell collision, then object-cell collision.
    let mut land = StaticLandSource::linear();
    land.add_flat_block(LandblockId::new(0xA9, 0xB4), 10);
    let cell = LandblockId::new(0xA9, 0xB4).cell(1);
    land.add_building(
        cell,
        BuildingGeometry {
            parts: vec![wall_part(Vec3::new(13.0, 12.0, 20.0))],
        },
    );
    // runs first and refuses a transition with
    // no object, so the walk needs a mover even though nothing else reads it here.
    let (mut objects, mover) = a_mover();
    let _ = &mut objects;
    let cells = BTreeMap::new();
    let ctx = TransitionCtx {
        land: &land,
        objects: &objects,
        cells: &cells,
        mover: Some(mover),
        object_table: None,
        entry_host: None,
    };

    let mut t = a_placement_at_the_wall();
    t.object_info.object = Some(mover);
    let c = ctx
        .resolver()
        .get_visible(cell)
        .expect("a resident land cell");
    assert_eq!(
        cell_find_collisions(&ctx, &mut t, &c),
        TransitionState::Collided
    );
    assert_eq!(t.counters.buildings_visited, 1);
    assert!(t.collision_info.collided_with_environment);
}

#[test]
fn cell_find_collisions_never_asks_an_interior_cell_for_a_building() {
    // Only a sort cell — a land cell — has a `building` field. Environment-cell collision is
    // two steps, not three, so the seam must not be consulted at all.
    let mut land = StaticLandSource::linear();
    let interior = CellId(0xA9B4_0100);
    land.add_cell(crate::source::EnvCellGeometry {
        id: interior,
        frame: Frame::new(Vec3::ZERO, Quat::IDENTITY),
        ..crate::source::EnvCellGeometry::default()
    });
    // Registered against the interior cell's own id: if the land-cell branch were reached for
    // an env cell, this is what it would find.
    land.add_building(
        interior,
        BuildingGeometry {
            parts: vec![wall_part(Vec3::new(13.0, 12.0, 20.0))],
        },
    );
    let (objects, mover) = a_mover();
    let cells = BTreeMap::new();
    let ctx = TransitionCtx {
        land: &land,
        objects: &objects,
        cells: &cells,
        mover: Some(mover),
        object_table: None,
        entry_host: None,
    };

    let mut t = a_placement_at_the_wall();
    t.object_info.object = Some(mover);
    let c = ctx
        .resolver()
        .get_visible(interior)
        .expect("a resident interior cell");
    assert!(!c.is_land());
    assert_eq!(cell_find_collisions(&ctx, &mut t, &c), TransitionState::Ok);
    assert_eq!(
        t.counters.buildings_visited, 0,
        "environment cells have no sort-cell building"
    );
}

/// The interior branch has to be *visible* from outside this crate, because "the interior
/// branch ran and found nothing" and "the interior branch never ran" are otherwise the same
/// observation. `CollisionCounters` covered shadows, objects, buildings and object parts and
/// nothing else; these three are the env-cell equivalents.
#[test]
fn the_env_cell_counters_separate_a_branch_that_ran_from_one_that_did_not() {
    let mut land = StaticLandSource::linear();
    land.add_flat_block(LandblockId::new(0xA9, 0xB4), 10);
    let interior = CellId(0xA9B4_0100);
    land.add_cell(crate::source::EnvCellGeometry {
        id: interior,
        frame: Frame::new(Vec3::ZERO, Quat::IDENTITY),
        ..crate::source::EnvCellGeometry::default()
    });
    let (objects, mover) = a_mover();
    let cells = BTreeMap::new();
    let ctx = TransitionCtx {
        land: &land,
        objects: &objects,
        cells: &cells,
        mover: Some(mover),
        object_table: None,
        entry_host: None,
    };

    // A land cell reaches the land-cell branch and never the interior one, so every env
    // counter stays at zero.
    let (mut t, _) = transition_on_flat_ground(20.5);
    t.object_info.object = Some(mover);
    let land_cell = ctx
        .resolver()
        .get_visible(LandblockId::new(0xA9, 0xB4).cell(1));
    let land_cell = land_cell.expect("a resident land cell");
    assert!(land_cell.is_land());
    let _ = cell_find_collisions(&ctx, &mut t, &land_cell);
    assert_eq!(
        (t.counters.env_cells_visited, t.counters.env_bsp_walked),
        (0, 0),
        "a land cell must not report as an interior one"
    );

    // An interior cell with no BSP is entered and answers `OK_TS`
    // without walking anything — which is exactly the case that used to look identical to
    // "never entered".
    let mut t = a_placement_at_the_wall();
    t.object_info.object = Some(mover);
    let c = ctx
        .resolver()
        .get_visible(interior)
        .expect("a resident interior cell");
    assert_eq!(cell_find_collisions(&ctx, &mut t, &c), TransitionState::Ok);
    assert_eq!(
        t.counters.env_cells_visited, 1,
        "the interior branch was entered"
    );
    assert_eq!(
        t.counters.env_cells_without_bsp, 1,
        "and it had no tree to walk"
    );
    assert_eq!(t.counters.env_bsp_walked, 0);

    // And an interior cell that *does* carry one walks it, so the two cases are told apart
    // rather than merely both being non-zero.
    let mut land = StaticLandSource::linear();
    let solid = CellId(0xA9B4_0101);
    land.add_cell(crate::source::EnvCellGeometry {
        id: solid,
        frame: Frame::new(Vec3::new(13.0, 12.0, 20.0), Quat::IDENTITY),
        physics_bsp: Some(Arc::new(wall_tree())),
        ..crate::source::EnvCellGeometry::default()
    });
    let ctx = TransitionCtx {
        land: &land,
        objects: &objects,
        cells: &cells,
        mover: Some(mover),
        object_table: None,
        entry_host: None,
    };
    let mut t = a_placement_at_the_wall();
    t.object_info.object = Some(mover);
    let c = ctx
        .resolver()
        .get_visible(solid)
        .expect("a resident interior cell");
    let r = cell_find_collisions(&ctx, &mut t, &c);
    assert_eq!(t.counters.env_cells_visited, 1);
    assert_eq!(t.counters.env_cells_without_bsp, 0);
    assert_eq!(t.counters.env_bsp_walked, 1, "the interior tree was walked");
    assert_eq!(
        r,
        TransitionState::Collided,
        "and the wall it holds is solid"
    );
    assert!(t.collision_info.collided_with_environment);
}

#[test]
fn initial_placement_skips_the_object_pass_entirely() {
    let (land, objects, cells) = flat_world();
    let (mut t, _) = transition_on_flat_ground(20.5);
    t.sphere_path.insert_type = InsertType::InitialPlacement;
    let ctx = TransitionCtx {
        land: &land,
        objects: &objects,
        cells: &cells,
        mover: None,
        object_table: None,
        entry_host: None,
    };
    assert_eq!(
        cell_find_obj_collisions(&ctx, &mut t, LandblockId::new(0xA9, 0xB4).cell(1)),
        TransitionState::Ok
    );
}
