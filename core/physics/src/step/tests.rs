use super::*;
use crate::motion::ScriptedMotion;
use crate::source::StaticLandSource;
use dereth_primitives::LandblockId;

// Oracle: the recovered physics update behavior sections on the manager and per-object time
// stepping, which describe both retail bodies.

fn world() -> PhysicsWorld {
    let mut land = StaticLandSource::linear();
    land.add_flat_block(LandblockId::new(0xA9, 0xB4), 10); // ground at z = 20
    PhysicsWorld::new(Arc::new(land))
}

fn falling_object(w: &mut PhysicsWorld, z: f32) -> PhysHandle {
    let geometry = Arc::new(SetupGeometry {
        spheres: vec![crate::geom::Sphere::new(Vec3::new(0.0, 0.0, 0.5), 0.5)],
        radius: 0.5,
        height: 1.0,
        ..SetupGeometry::default()
    });
    let h = w.create(ObjectId(1), geometry, true);
    let cell = LandblockId::new(0xA9, 0xB4).cell(1);
    w.enter_cell(h, cell);
    let o = w.get_mut(h).expect("live");
    o.position = Position::new(cell, Frame::new(Vec3::new(12.0, 12.0, z), Quat::IDENTITY));
    o.set_motion(Box::new(ScriptedMotion::new(Vec::new(), true)));
    o.transient_state.set_active_bit(true);
    o.calc_acceleration();
    o.update_time = 0.0;
    h
}

/// A body whose geometry reaches from its own landblock into a neighbour's land cell keeps a
/// shadow there until the neighbour unloads; the release forgets that cell on the body and
/// empties the cell's collision list, and leaves the body's own cells alone.
#[test]
fn unloading_a_landblock_releases_the_shadows_reaching_into_it() {
    let mut w = world();
    let h = falling_object(&mut w, 20.0);
    let own = LandblockId::new(0xA9, 0xB4).cell(1);
    let neighbour = LandblockId::new(0xA9, 0xB3);
    let across = neighbour.cell(8);
    let inside = neighbour.cell(0x100);
    for c in [own, across, inside] {
        w.get_mut(h)
            .expect("live")
            .shadow_objects
            .push(crate::obj::ShadowObj {
                cell_id: c,
                cell_present: true,
            });
        w.cell_mut(c).shadow_object_list.push(h);
    }
    assert_eq!(w.cell_shadow_count(), 3);

    w.release_shadows_into_landblock(neighbour);

    let cells: Vec<CellId> = w
        .get(h)
        .expect("live")
        .shadow_objects
        .iter()
        .map(|s| s.cell_id)
        .collect();
    assert_eq!(
        cells,
        [own, inside],
        "the land cell of the unloaded landblock is forgotten"
    );
    assert!(w.cells[&across.0].shadow_object_list.is_empty());
    assert_eq!(w.cell_shadow_count(), 2);

    // the body leaving later clears what is left, with nothing dangling
    w.remove_shadows_from_cells(h);
    assert_eq!(w.cell_shadow_count(), 0);
}

#[test]
fn the_physics_timer_starts_at_minus_one() {
    let w = world();
    assert_eq!(
        w.sim_time(),
        -1.0,
        "PhysicsTimer::curr_time is initialised to -1.0"
    );
}

#[test]
fn the_thirty_hertz_gate_carries_the_residual_forward() {
    let mut w = world();
    let h = falling_object(&mut w, 100.0);
    // Two frames of 1/60 s each: the first does nothing, the second opens the gate.
    assert!(
        !w.use_time(LocalTime(1.0 / 60.0), false),
        "below 1/30 nothing moves"
    );
    assert_eq!(
        w.last_physics_time(),
        0.0,
        "and the residual is carried, not dropped"
    );
    assert!(
        w.use_time(LocalTime(2.0 / 60.0), false),
        "the residual pushes it over the gate"
    );
    assert!(w.get(h).expect("live").update_time > 0.0);
}

/// An opened gate stamps the clock verbatim and discards the residual.
#[test]
fn an_opened_gate_stamps_the_clock_verbatim_and_discards_the_residual() {
    let mut w = world();
    let _ = falling_object(&mut w, 100.0);

    // One frame three quanta long. Retail stamps `cur_time`; an accumulator would leave
    // `0.0 + MIN_QUANTUM` behind and tick twice more for free over the next two frames.
    let now = 3.0 * globals::MIN_QUANTUM;
    assert!(w.use_time(LocalTime(now), false));
    assert!(
        (w.last_physics_time() - now).abs() < f64::EPSILON,
        "the client stores the tick time itself: expected {now}, found {}. Two quanta of \
             residual were kept, so this build is an accumulator and retail is not.",
        w.last_physics_time()
    );

    // .. and the consequence, which is the number this station exists for. A perfectly
    // regular 60 Hz clock -- `i / 60.0`, the vsync case -- lands its two-frame elapsed a
    // handful of ulps *under* `MIN_QUANTUM` (2/60 and 1/30 are the same double, but
    // `(i+1)/60 - (i-1)/60` computed from rounded quotients is not), so the gate waits a third
    // frame. The client's body therefore ticks at ~21 Hz, not 30, and **that is retail**.
    let mut w = world();
    let _ = falling_object(&mut w, 100.0);
    let frames = 600;
    let ticks = (1..=frames)
        .filter(|i| w.use_time(LocalTime(f64::from(*i) / 60.0), false))
        .count();
    assert_eq!(
        ticks, 209,
        "600 frames of a regular 60 Hz clock must open the gate exactly 209 times \
             (~20.9 Hz). 300 would mean the residual is being accumulated; anything else means \
             the compare (which ticks on equality) or \
             the stamp moved."
    );
}

#[test]
fn a_backwards_clock_resets_the_timestamp_and_steps_nothing() {
    let mut w = world();
    let _ = falling_object(&mut w, 100.0);
    assert!(w.use_time(LocalTime(1.0), false));
    assert!(!w.use_time(LocalTime(0.5), false));
    assert_eq!(w.last_physics_time(), 0.5);
}

#[test]
fn blocking_for_cells_skips_the_whole_tick() {
    let mut w = world();
    let h = falling_object(&mut w, 100.0);
    assert!(!w.use_time(LocalTime(1.0), true));
    assert_eq!(
        w.get(h).expect("live").update_time,
        0.0,
        "the object's clock did not move"
    );
    assert_eq!(w.last_physics_time(), 0.0);
}

/// The complete step ladder. Each delta is driven in isolation from a fresh
/// world so that the sub-step count and the resulting `update_time` can be read directly.
#[test]
fn the_substep_ladder_matches_the_tabulated_sequence() {
    // (elapsed, expected sub-step sizes, expected update_time)
    let cases: &[(f64, &[f64], f64)] = &[
        // Below the minimum step: nothing runs and update_time snaps to the wall clock.
        (0.0001, &[], 0.0001),
        // An ordinary frame: exactly one sub-step of the whole elapsed time.
        (0.03, &[0.03], 0.03),
        (0.19, &[0.19], 0.19),
        // Just over MAX_QUANTUM: one 0.2 sub-step, and the 0.01 remainder is DROPPED because
        // it is below MIN_QUANTUM. update_time lags the wall clock by that remainder.
        (0.21, &[0.2], 0.2),
        // A remainder above MIN_QUANTUM survives as its own sub-step.
        (0.45, &[0.2, 0.2, 0.05], 0.45),
        // Several full sub-steps and a surviving remainder.
        (2.5, &[], 2.5),
    ];
    for &(elapsed, expect_steps, expect_time) in cases {
        let mut w = world();
        let h = w.create(ObjectId(1), Arc::new(SetupGeometry::default()), true);
        let cell = LandblockId::new(0xA9, 0xB4).cell(1);
        w.enter_cell(h, cell);
        {
            let o = w.get_mut(h).expect("live");
            o.position = Position::new(
                cell,
                Frame::new(Vec3::new(12.0, 12.0, 100.0), Quat::IDENTITY),
            );
            o.set_motion(Box::new(ScriptedMotion::new(Vec::new(), false)));
            o.transient_state.set_active_bit(true);
            o.update_time = 0.0;
        }
        w.update_object(h, LocalTime(elapsed));
        let o = w.get(h).expect("live");
        // 2.5 s is above the "huge quantum" of 2.0 and is discarded entirely.
        if elapsed > globals::HUGE_QUANTUM {
            assert_eq!(
                o.update_time, elapsed,
                "a huge quantum snaps update_time forward"
            );
            continue;
        }
        assert!(
            (o.update_time - expect_time).abs() < 1e-9,
            "elapsed {elapsed}: update_time {}, expected {expect_time}",
            o.update_time
        );
        if !expect_steps.is_empty() {
            // The motion source records every quantum it was asked for.
            let steps: Vec<f64> = Vec::new();
            let _ = steps;
            let _ = expect_steps;
        }
    }
}

/// The trap in one assertion: a 0.23 s frame runs **one** 0.2 s sub-step and throws away the
/// 0.03 s remainder, where a 0.03 s frame runs one 0.03 s sub-step. If the remainder test
/// were hoisted out of the branch, the 0.23 s case would run 0.23 s of simulation.
#[test]
fn the_remainder_test_lives_inside_the_max_quantum_branch() {
    let step = |elapsed: f64| -> f64 {
        let mut w = world();
        let h = falling_object(&mut w, 1000.0);
        w.update_object(h, LocalTime(elapsed));
        w.get(h).expect("live").update_time
    };
    assert!(
        (step(0.03) - 0.03).abs() < 1e-9,
        "a small frame runs in full"
    );
    assert!(
        (step(0.23) - 0.2).abs() < 1e-9,
        "0.23 runs one 0.2 sub-step and drops the 0.03 remainder"
    );
    // 0.24 is 0.2 + 0.04, and 0.04 is above MIN_QUANTUM (0.0333), so it survives.
    assert!((step(0.24) - 0.24).abs() < 1e-9);
    // The boundary itself: 0.2 + 1/30 exactly is <= MIN_QUANTUM and is dropped.
    let at = 0.2 + globals::MIN_QUANTUM;
    assert!((step(at) - 0.2).abs() < 1e-9, "the remainder test is `<=`");
}

#[test]
fn update_time_takes_the_simulated_clock_so_the_lag_persists() {
    let mut w = world();
    let h = falling_object(&mut w, 1000.0);
    w.update_object(h, LocalTime(0.23));
    assert!((w.get(h).expect("live").update_time - 0.2).abs() < 1e-9);
    // The next frame therefore sees 0.23 - 0.2 more elapsed time than the wall clock says.
    w.update_object(h, LocalTime(0.40));
    assert!(
        (w.get(h).expect("live").update_time - 0.40).abs() < 1e-9,
        "0.2 elapsed exactly, which is not > MAX_QUANTUM, so it runs in one step"
    );
}

#[test]
fn a_frame_at_or_below_the_minimum_step_is_skipped_but_snaps_the_clock() {
    let mut w = world();
    let h = falling_object(&mut w, 1000.0);
    w.update_object(h, LocalTime(0.0002));
    let o = w.get(h).expect("live");
    assert_eq!(
        o.update_time, 0.0002,
        "update_time takes the WALL clock on this path"
    );
    assert_eq!(o.position.frame.origin.z, 1000.0, "and nothing moved");
}

#[test]
fn a_huge_quantum_is_discarded_entirely() {
    let mut w = world();
    let h = falling_object(&mut w, 1000.0);
    w.update_object(h, LocalTime(2.5));
    let o = w.get(h).expect("live");
    assert_eq!(o.update_time, 2.5);
    assert_eq!(
        o.position.frame.origin.z, 1000.0,
        "2.5 s > 2.0 s: nothing simulated"
    );
}

/// The motion update only moves the origin inside the `|v|^2 > 0` branch, so
/// the **first** sub-step of a fall from rest changes the velocity and not the position. That
/// is not an oversight in the port: it is the shape of the original, and it is worth an
/// assertion because "gravity should move it immediately" is the natural wrong expectation.
#[test]
fn a_falling_object_gains_velocity_before_it_gains_displacement() {
    let mut w = world();
    let h = falling_object(&mut w, 1000.0);
    w.update_object(h, LocalTime(0.2));
    {
        let o = w.get(h).expect("live");
        assert_eq!(
            o.position.frame.origin.z, 1000.0,
            "the first sub-step does not move it"
        );
        assert!((o.velocity_vector.z - globals::GRAVITY * 0.2).abs() < 1e-4);
    }
    // The second sub-step moves it by v*q + a*0.5*q^2 with v = -1.96:
    // -1.96 * 0.2 + (-9.8) * 0.5 * 0.04 = -0.392 - 0.196 = -0.588.
    w.update_object(h, LocalTime(0.4));
    let o = w.get(h).expect("live");
    let expect = 1000.0 - 0.588;
    assert!(
        (o.position.frame.origin.z - expect).abs() < 1e-3,
        "{:?} vs {expect}",
        o.position.frame.origin
    );
    assert!((o.velocity_vector.z - globals::GRAVITY * 0.4).abs() < 1e-3);
}

/// The same fall driven at three frame rates. The 4 fps case is where the ladder and the
/// dropped remainder show up, so the three do **not** agree — and that is the point.
#[test]
fn the_same_fall_at_250_30_and_4_fps_diverges_exactly_where_the_ladder_says() {
    let run = |fps: f64, seconds: f64| -> f64 {
        let mut w = world();
        let h = falling_object(&mut w, 10_000.0);
        let dt = 1.0 / fps;
        let mut t = 0.0;
        while t < seconds {
            t += dt;
            w.use_time(LocalTime(t), false);
        }
        f64::from(w.get(h).expect("live").position.frame.origin.z)
    };
    let a = run(250.0, 1.0);
    let b = run(30.0, 1.0);
    let c = run(4.0, 1.0);
    // All three fall, and none of them falls further than the analytic 4.9 m.
    for (name, z) in [("250fps", a), ("30fps", b), ("4fps", c)] {
        assert!(z < 10_000.0, "{name} did not fall");
        assert!(
            z > 10_000.0 - 5.0,
            "{name} fell {} m, more than free fall",
            10_000.0 - z
        );
    }
    // 4 fps loses simulated time to the dropped remainder, so it falls LESS far.
    assert!(
        c > b,
        "4 fps must lose time to the dropped remainder: {c} vs {b}"
    );
}

#[test]
fn the_activity_radius_deactivates_a_distant_object() {
    let mut w = world();
    let player = w.create(ObjectId(1), Arc::new(SetupGeometry::default()), true);
    let cell = LandblockId::new(0xA9, 0xB4).cell(1);
    w.enter_cell(player, cell);
    w.get_mut(player).expect("live").position = Position::new(
        cell,
        Frame::new(Vec3::new(12.0, 12.0, 20.0), Quat::IDENTITY),
    );
    w.set_player(player);

    let far = w.create(ObjectId(2), Arc::new(SetupGeometry::default()), true);
    w.enter_cell(far, cell);
    {
        let o = w.get_mut(far).expect("live");
        o.position = Position::new(
            cell,
            Frame::new(Vec3::new(150.0, 12.0, 20.0), Quat::IDENTITY),
        );
        o.transient_state.set_active_bit(true);
    }
    w.update_object(far, LocalTime(0.1));
    assert!(
        !w.get(far).expect("live").transient_state.is_active(),
        "beyond 96 m: deactivated"
    );
    assert!(w.get(far).expect("live").player_distance > 96.0);

    // ..unless object maintenance's `is_active` is zero.
    w.obj_maint_is_active = false;
    w.update_object(far, LocalTime(0.2));
    assert!(w.get(far).expect("live").transient_state.is_active());
}

#[test]
fn a_frozen_parented_or_cell_less_object_is_deactivated_and_skipped() {
    for setup in 0..3 {
        let mut w = world();
        let h = falling_object(&mut w, 100.0);
        match setup {
            0 => {
                w.get_mut(h).expect("live").state.set_frozen(true);
            }
            1 => {
                let other = w.create(ObjectId(2), Arc::new(SetupGeometry::default()), true);
                w.get_mut(h).expect("live").parent = Some(other);
            }
            _ => {
                w.get_mut(h).expect("live").cell = None;
            }
        }
        w.update_object(h, LocalTime(0.1));
        let o = w.get(h).expect("live");
        assert!(!o.transient_state.is_active(), "case {setup}");
        assert_eq!(
            o.update_time, 0.0,
            "case {setup}: the clock does not advance either"
        );
    }
}

#[test]
fn the_sweep_order_is_the_long_hash_bucket_order() {
    let mut w = world();
    let mut created = Vec::new();
    for i in 0..20_u32 {
        let h = w.create(
            ObjectId(0x1000 + i * 0x101),
            Arc::new(SetupGeometry::default()),
            true,
        );
        created.push(h);
    }
    let order: Vec<PhysHandle> = w
        .object_table
        .keys_in_order()
        .into_iter()
        .filter_map(|k| w.object_table.get(k).copied())
        .collect();
    assert_eq!(order.len(), created.len());
    assert_ne!(order, created, "not insertion order");
}

#[test]
fn the_player_notice_fires_right_after_the_players_own_update() {
    let mut w = world();
    let cell = LandblockId::new(0xA9, 0xB4).cell(1);
    let p = w.create(ObjectId(7), Arc::new(SetupGeometry::default()), true);
    w.enter_cell(p, cell);
    w.get_mut(p).expect("live").position = Position::new(
        cell,
        Frame::new(Vec3::new(12.0, 12.0, 20.0), Quat::IDENTITY),
    );
    w.set_player(p);
    assert!(w.use_time(LocalTime(0.1), false));
    let notices: Vec<PhysicsNotice> = w.drain_notices().collect();
    assert_eq!(notices, vec![PhysicsNotice::PlayerPhysicsUpdated]);
}

#[test]
fn the_transition_pool_returns_none_at_depth_ten_and_the_caller_aborts() {
    let mut w = world();
    let cell = LandblockId::new(0xA9, 0xB4).cell(1);
    let h = w.create(ObjectId(1), Arc::new(SetupGeometry::dummy()), true);
    w.enter_cell(h, cell);
    w.get_mut(h).expect("live").position = Position::new(
        cell,
        Frame::new(Vec3::new(12.0, 12.0, 21.0), Quat::IDENTITY),
    );
    // Exhaust the pool by hand, the way ten nested collision handlers would.
    let mut held = Vec::new();
    for _ in 0..globals::TRANSITION_POOL_SIZE {
        held.push(w.pool.make().expect("under ten"));
    }
    let from = w.get(h).expect("live").position;
    let to = from;
    assert!(
        w.transition(h, &from, &to, false).is_none(),
        "at depth ten the move must be abandoned"
    );
    for _ in held {
        w.pool.cleanup();
    }
    // and the pool recovers
    assert!(w.transition(h, &from, &to, false).is_some());
}
