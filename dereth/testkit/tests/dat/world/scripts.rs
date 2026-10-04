use super::*;

// -------------------------------------------------------------------------------------------
// world.physics-script.* (the delayed call)
// -------------------------------------------------------------------------------------------

/// A shipped script that calls another one after a pause really plays it, after the delay it
/// rolled for itself.
///
/// The child's arrival is watched through the emitter **it** creates at its own start, not
/// through a counter: a counter can be incremented by wiring that schedules nothing.
pub fn a_delayed_script_call_plays_after_its_delay() {
    use dereth_animation::AnimEvent;
    use dereth_assets::{Decode, HookData, PhysicsScript};
    use dereth_dat::DbType;
    use dereth_primitives::ServerTime;

    let store = support::store();

    // The premise, asserted rather than assumed: the shipped script this drives creates an
    // emitter at its start and calls its neighbour half a second later, with a pause to roll
    // the delay over.
    let bytes = store
        .read_typed(DbType::PhysicsScript, world_support::RING_PARENT)
        .expect("the ring script is shipped");
    let script =
        PhysicsScript::decode_payload(world_support::RING_PARENT, &bytes).expect("it decodes");
    let shape: Vec<(f64, u32)> = script
        .script_data
        .iter()
        .map(|s| (s.start_time, s.hook.hook_type))
        .collect();
    let shaped = shape
        == vec![
            (0.0, world_support::CREATE_PARTICLE),
            (0.5, world_support::CALL_PES),
        ];
    let HookData::CallPes { pes, pause } = script.script_data[1].hook.data else {
        panic!("the ring script's second step is not a call")
    };
    let names_the_child = pes == world_support::RING_CHILD && (pause - 0.5).abs() < 1e-6;

    let mut d = world_support::script_driver(&store);
    d.cur_time = ServerTime(0.0);
    assert!(
        d.play_script_internal(world_support::RING_PARENT),
        "the shipped parent script queues"
    );

    let dt = 1.0 / 30.0;
    let mut t = 0.0;
    let mut armed_at: Option<(f64, f32)> = None;
    let mut emitters: Vec<f64> = Vec::new();
    for _ in 0..60 {
        for e in world_support::script_tick(&mut d, t) {
            match e {
                AnimEvent::CallPes { script, pause } => {
                    if armed_at.is_none() && script == world_support::RING_CHILD {
                        armed_at = Some((t, pause));
                    }
                }
                AnimEvent::CreateParticleEmitter { .. } => emitters.push(t),
                _ => {}
            }
        }
        t += dt;
    }

    let (arm_t, delay) = armed_at.expect("the parent's call step never executed at all");
    println!("effect delay: armed at t={arm_t:.4}s for {delay}s; emitters at {emitters:?}");
    let rolled = delay > 0.0 && delay <= 0.5;
    let armed_on_its_own_step = (arm_t - 0.5).abs() < dt;
    let parents_own_emitter = emitters.first().copied().is_some_and(|x| x < dt);
    let after: Vec<f64> = emitters.iter().copied().filter(|x| *x > arm_t).collect();
    let due = arm_t + f64::from(delay);
    let child_played = after
        .first()
        .copied()
        .is_some_and(|fired| fired >= due - 1e-9 && fired <= due + 3.0 * dt);

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "world.physics-script.a-delayed-call-plays-the-script-it-names-after-the-delay-it-rolled",
        move |_| {
            shaped
                && names_the_child
                && rolled
                && armed_on_its_own_step
                && parents_own_emitter
                && child_played
        },
    );
}

/// An object that leaves the world before its delayed call is due plays nothing, and coming back
/// does not resurrect the timer it spent.
pub fn an_object_that_leaves_before_the_delay_plays_nothing() {
    use dereth_animation::AnimEvent;
    use dereth_primitives::ServerTime;

    let store = support::store();
    let mut d = world_support::script_driver(&store);
    d.cur_time = ServerTime(0.0);
    assert!(
        d.play_script_internal(world_support::RING_PARENT),
        "the shipped parent script queues"
    );

    let dt = 1.0 / 30.0;
    let mut t = 0.0;
    let mut armed: Option<(f64, f32)> = None;
    while armed.is_none() && t < 3.0 {
        for e in world_support::script_tick(&mut d, t) {
            if let AnimEvent::CallPes { pause, .. } = e {
                armed = Some((t, pause));
            }
        }
        t += dt;
    }
    let (arm_t, delay) = armed.expect("the call step never executed");
    let timer_armed = !d.fp_hooks.is_empty();

    // The object leaves its room before the timer is due. The room is read at the moment the
    // timer fires, not at the moment it was armed.
    d.env.in_cell = false;
    let mut emitters = 0u32;
    for _ in 0..90 {
        for e in world_support::script_tick(&mut d, t) {
            if matches!(e, AnimEvent::CreateParticleEmitter { .. }) {
                emitters += 1;
            }
        }
        t += dt;
    }
    let nothing_played = emitters == 0;
    let timer_unlinked = d.fp_hooks.is_empty();

    // ...and it stays cancelled: coming back does not resurrect the timer that was spent.
    d.env.in_cell = true;
    let mut later = 0u32;
    for _ in 0..90 {
        for e in world_support::script_tick(&mut d, t) {
            if matches!(e, AnimEvent::CreateParticleEmitter { .. }) {
                later += 1;
            }
        }
        t += dt;
    }
    println!("effect delay cancel: armed at {arm_t:.4}s for {delay}s, {emitters} then {later}");

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "world.physics-script.an-object-that-leaves-its-room-before-the-delay-is-up-plays-nothing",
        move |_| timer_armed && nothing_played && timer_unlinked && later == 0,
    );
}

/// A shipped call with no pause still plays the script it names on the spot.
pub fn a_script_call_with_no_delay_plays_on_the_spot() {
    use dereth_animation::AnimEvent;
    use dereth_assets::{Decode, HookData, PhysicsScript};
    use dereth_dat::DbType;
    use dereth_primitives::ServerTime;

    let store = support::store();

    // The first shipped script whose call has no pause and names another script that ships.
    // Searched rather than named, so that this arm is measured over whatever the data holds.
    let mut found = None;
    'outer: for id in &store.ids_of(DbType::PhysicsScript) {
        let Ok(bytes) = store.read_typed(DbType::PhysicsScript, *id) else {
            continue;
        };
        let Ok(s) = PhysicsScript::decode_payload(*id, &bytes) else {
            continue;
        };
        for step in &s.script_data {
            if let HookData::CallPes { pes, pause } = step.hook.data {
                if pause < world_support::HOOK_EPSILON
                    && pes != *id
                    && store.read_typed(DbType::PhysicsScript, pes).is_ok()
                {
                    found = Some((*id, pes, step.start_time));
                    break 'outer;
                }
            }
        }
    }
    let (parent, child, at) =
        found.expect("some shipped script calls another one with no pause at all");
    println!(
        "effect delay immediate: {:#010X} calls {:#010X} at t={at:.3}",
        parent.0, child.0
    );

    let mut d = world_support::script_driver(&store);
    d.cur_time = ServerTime(0.0);
    assert!(
        d.play_script_internal(parent),
        "the shipped parent script queues"
    );

    let dt = 1.0 / 30.0;
    let mut t = 0.0;
    let mut on_the_spot = false;
    let mut early = false;
    let mut delayed = false;
    while t < at + 1.0 {
        for e in world_support::script_tick(&mut d, t) {
            if let AnimEvent::CallPes { script, pause } = e {
                if script == child {
                    on_the_spot = true;
                    delayed |= pause != 0.0;
                    early |= t < at - dt;
                }
            }
        }
        t += dt;
    }

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "world.physics-script.a-call-with-no-delay-still-plays-on-the-spot",
        move |_| on_the_spot && !delayed && !early,
    );
}

// -------------------------------------------------------------------------------------------
// world.physics-script.* (the scale hook)
// -------------------------------------------------------------------------------------------

/// A shipped hook that makes an object bigger makes its collision body bigger with it.
pub fn a_scale_hook_moves_the_collision_radius() {
    use dereth_primitives::Vec3;

    let store = support::store();
    let scale = world_support::scale_the_shipped_script_asks_for(&store);
    let doubles = (scale - 2.0).abs() < 1e-6;

    let mut w = world_support::flat_world();
    let h = world_support::place_obstacle(&mut w, Vec3::new(30.0, 30.0, world_support::GROUND));
    let starts_at_one = {
        let o = w.get(h).expect("the obstacle is live");
        (o.scale - 1.0).abs() < 1e-6
            && (o.radius() - 0.6).abs() < 1e-6
            && (o.height() - 1.2).abs() < 1e-6
    };

    // What the scene does with the hook the script raised.
    w.get_mut(h).expect("the obstacle is live").scale = scale;

    let o = w.get(h).expect("the obstacle is live");
    let followed = (o.radius() - 1.2).abs() < 1e-6 && (o.height() - 2.4).abs() < 1e-6;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "world.physics-script.a-scale-hook-moves-the-objects-own-collision-radius",
        move |_| doubles && starts_at_one && followed,
    );
}

/// A body walking at an object a shipped hook has made bigger stops at the bigger distance.
///
/// A differential: the same world, the same walk and the same object in both arms, and the only
/// variable is whether the hook reached the object. The approach comes in off the normal,
/// because a head-on walk can creep through a surface and would make this a measurement of that
/// instead.
pub fn a_body_walking_at_a_scaled_object_stops_further_away() {
    use dereth_primitives::{LocalTime, Vec3};

    let store = support::store();
    let scale = world_support::scale_the_shipped_script_asks_for(&store);

    let obstacle = Vec3::new(40.0, 40.0, world_support::GROUND);
    // Six metres out, aimed a metre to one side of the centre so the walk grazes rather than
    // wedges.
    let start = Vec3::new(40.0 - 6.0, 40.0 - 1.0, world_support::GROUND);
    let step = Vec3::new(0.05, 0.0, 0.0);

    let closest = |apply: bool| -> f32 {
        let mut w = world_support::flat_world();
        let obj = world_support::place_obstacle(&mut w, obstacle);
        if apply {
            w.get_mut(obj).expect("the obstacle is live").scale = scale;
        }
        let body = world_support::spawn_walker(&mut w, start, step);
        let mut t = 0.0;
        let mut best = f32::MAX;
        for _ in 0..300 {
            t += 1.0 / 30.0;
            w.use_time(LocalTime(t), false);
            let p = w
                .get(body)
                .expect("the walker is live")
                .position
                .frame
                .origin;
            best = best.min(dereth_primitives::num::math::hypotf(
                p.x - obstacle.x,
                p.y - obstacle.y,
            ));
        }
        best
    };

    let plain = closest(false);
    let scaled = closest(true);
    println!("scale hook: closest approach {plain:.4} m plain, {scaled:.4} m scaled ({scale})");

    // Scaling an object scales its sphere's centre as well as its radius, so the distance two
    // spheres touch at, projected into the ground plane, is what the walk should rest at. It is
    // written out rather than fitted to the run, because "stops at the scaled radius" is a
    // number and not a direction.
    let contact = |s: f32| -> f32 {
        let dz = 0.6 * s - 0.5;
        let d = 0.6 * s + 0.5;
        (d * d - dz * dz).sqrt()
    };
    let unscaled_is_right = (plain - contact(1.0)).abs() < 0.03;
    let further = scaled > plain + 0.3;
    let scaled_is_right = (scaled - contact(scale)).abs() < 0.03;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "world.physics-script.a-body-walking-at-a-scaled-object-stops-at-the-scaled-distance",
        move |_| unscaled_is_right && further && scaled_is_right,
    );
}
