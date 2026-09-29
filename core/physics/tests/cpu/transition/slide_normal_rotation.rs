//! Behaviour: none (checks rotation of collision-response normals)
//! Collision response against synthetic obstacle geometry.
//! Fixture: shared rotated surfaces, a flat block and spherical obstacles.

use crate::common::step_up_fixture::*;

/// The second spheres slide normal is the parts normal taken into the world.
#[test]
fn the_second_spheres_slide_normal_is_the_parts_normal_taken_into_the_world() {
    let rot = quarter_turn_about_x();
    let part = Position::new(CELL, Frame::new(Vec3::ZERO, rot));
    let tree = one_square_tree();

    let mut t = Transition::default();
    t.sphere_path.init_sphere(
        &[
            Sphere::new(Vec3::new(0.0, -20.0, 0.0), 0.5),
            Sphere::new(Vec3::new(0.0, -0.4, 0.0), 0.5),
        ],
        1.0,
    );
    assert_eq!(
        t.sphere_path.num_sphere, 2,
        "the arm under test is unreachable with one sphere"
    );

    let from = Position::new(CELL, Frame::new(Vec3::new(0.0, -0.2, 0.0), Quat::IDENTITY));
    let to = Position::new(CELL, Frame::new(Vec3::ZERO, Quat::IDENTITY));
    t.sphere_path.init_path(Some(CELL), Some(from), &to);
    t.sphere_path.set_check_pos(&to, Some(CELL));
    t.sphere_path.cache_localspace_sphere(&part, 1.0);

    let s0 = t.sphere_path.localspace_sphere[0];
    let s1 = t.sphere_path.localspace_sphere[1];
    let movement = s0.center.sub(t.sphere_path.localspace_curr_center[0]);
    assert!(
        (s0.center.sub(Vec3::new(0.0, 0.0, 20.0))).mag2() < 1e-6,
        "sphere 0 is part-local (0,0,20): {:?}",
        s0.center
    );
    assert!(
        (s1.center.sub(Vec3::new(0.0, 0.0, 0.4))).mag2() < 1e-6,
        "sphere 1 is part-local (0,0,0.4): {:?}",
        s1.center
    );
    assert!(
        (movement.sub(Vec3::new(0.0, 0.0, -0.2))).mag2() < 1e-6,
        "the movement is part-local -z, into the square's face: {movement:?}"
    );

    let mut poly = None;
    let mut cp = Vec3::ZERO;
    assert!(
        !tree.sphere_intersects_poly(&s0, movement, &mut poly, &mut cp),
        "the FIRST sphere must miss, or the CONTACT arm returns through step_sphere_up instead"
    );
    assert_eq!(poly, None, "and it must miss without even naming a polygon");
    let mut poly1 = None;
    assert!(
        tree.sphere_intersects_poly(&s1, movement, &mut poly1, &mut cp),
        "the SECOND sphere must hit front-facing, or the slide is never reached"
    );
    assert_eq!(poly1, Some(0));

    // ---- drive the arm ------------------------------------------------------------------------
    t.object_info.set(ObjectInfoState::CONTACT, true);
    let land = StaticLandSource::linear();
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
    let state = collide::bsp_find_collisions(&ctx, &mut t, &tree, 1.0);

    // ---- the direct reading -------------------------------------------------------------------
    //
    // Sliding begins by recording the collision normal, so the vector the client slid along
    // is readable, and it is the one
    // rotated. Part-local `(0,0,1)` -> world `(0,-1,0)`.
    assert_ne!(
        state,
        TransitionState::Ok,
        "the second sphere hit; something must have happened"
    );
    assert!(
        t.collision_info.collision_normal_valid,
        "the slide must have set a collision normal"
    );
    let got = t.collision_info.collision_normal;
    let world = Vec3::new(0.0, -1.0, 0.0);
    let local = Vec3::new(0.0, 0.0, 1.0);
    assert!(
        got.sub(world).mag2() < 1e-6,
        "the slide normal must be the polygon normal taken into world space, {world:?}, not {got:?}"
    );
    assert!(
        got.dot(local).abs() < 1e-4,
        "the part-local normal {local:?} is perpendicular to the world one; reading {got:?} as \
         either means the rotation is not being applied"
    );
}

/// The placement rotation is off z and moves the normal this fixture uses.
#[test]
fn the_placement_rotation_is_off_z_and_moves_the_normal_this_fixture_uses() {
    let m = dereth_physics::math::l2g(quarter_turn_about_x());
    let map = |v: Vec3| dereth_physics::math::localtoglobalvec(m, v);

    // The mapping itself, on the three axes.
    assert!(
        map(Vec3::new(1.0, 0.0, 0.0))
            .sub(Vec3::new(1.0, 0.0, 0.0))
            .mag2()
            < 1e-6
    );
    assert!(
        map(Vec3::new(0.0, 1.0, 0.0))
            .sub(Vec3::new(0.0, 0.0, 1.0))
            .mag2()
            < 1e-6
    );
    assert!(
        map(Vec3::new(0.0, 0.0, 1.0))
            .sub(Vec3::new(0.0, -1.0, 0.0))
            .mag2()
            < 1e-6
    );

    let n = Vec3::new(0.0, 0.0, 1.0);
    assert!(
        map(n).dot(n).abs() < 1e-4,
        "an off-z rotation moves a z normal"
    );
    for turns in 0_i16..8 {
        let a = f32::from(turns) * std::f32::consts::PI / 4.0;
        let about_z = Quat::new(math::cosf(a * 0.5), 0.0, 0.0, math::sinf(a * 0.5));
        let mz = dereth_physics::math::l2g(about_z);
        let moved = dereth_physics::math::localtoglobalvec(mz, n);
        assert!(
            moved.sub(n).mag2() < 1e-6,
            "a rotation about z fixes {n:?}; that is why the aggregate pin could not see this \
             arm ({moved:?} at {a} rad)"
        );
    }
}
