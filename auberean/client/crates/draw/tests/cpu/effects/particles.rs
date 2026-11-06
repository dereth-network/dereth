//! Particle systems on synthetic descriptions: waveforms, keyframe links, emission limits, the
//! thrown motion, the emitter's shapes, and the keyframes' blended colors and sizes.

use auberean_assets::common::Waveform;
use auberean_assets::fx::{Emitter, EmitterKeyframe, Particles, WaveformVec3};
use auberean_draw::particles::{
    approximate_lifetime, camera_axes, next_keys, streak_corners, waveform_at, ParticleSet,
};
use auberean_draw::scene::ObjectBatch;
use auberean_primitives::random::Random;
use auberean_primitives::{Rgba, Vec3};
use auberean_world::math::Mat4;

fn steady(v: f32) -> Waveform {
    Waveform {
        kind: 1,
        base: v,
        amplitude: 1.0,
        frequency: 1.0,
        ..Waveform::default()
    }
}

fn steady3(x: f32, y: f32, z: f32) -> WaveformVec3 {
    WaveformVec3 {
        x: steady(x),
        y: steady(y),
        z: steady(z),
    }
}

/// A keyframe at `time` with `flags`, its color, size and mass; the first of a particle's life
/// also starts its motion (thrown, gravity on).
fn key(time: f32, flags: u32, color: Rgba, size: f32, mass: f32) -> EmitterKeyframe {
    EmitterKeyframe {
        time,
        key_flags: flags,
        scale_x: size,
        scale_y: size,
        color,
        mass,
        particle_kind: 1,
        keyframe_flags: 1,
        position: steady3(0.0, 0.0, 0.0),
        points: Vec::new(),
    }
}

const FIRST: u32 = 0x3B8;
const RED: Rgba = Rgba {
    r: 1.0,
    g: 0.0,
    b: 0.0,
    a: 1.0,
};
const BLUE: Rgba = Rgba {
    r: 0.0,
    g: 0.0,
    b: 1.0,
    a: 1.0,
};

/// One emitter at the origin: a point, discharging `blast` every `rate` seconds up to `limit`,
/// its particles living `life` seconds, starting at `speed` along `direction`.
fn emitter(rate: f32, blast: u32, limit: u32, life: f32, keys: Vec<EmitterKeyframe>) -> Emitter {
    Emitter {
        max_particles: 500,
        origin: steady3(0.0, 0.0, 0.0),
        shape: 0,
        particle_shape: 4,
        scale: steady(1.0),
        particle_scale: steady(1.0),
        active: true,
        birth_rate: rate,
        lifespan: steady(life),
        velocity: steady(0.0),
        streak: steady(0.0),
        rotation: steady3(0.0, 0.0, 0.0),
        world_rotation: steady3(0.0, 0.0, 0.0),
        rotate_velocity: steady3(0.0, 0.0, 0.0),
        direction: steady3(0.0, 0.0, 1.0),
        min_spread: steady(0.0),
        max_spread: steady(0.0),
        emission_limit: limit,
        blast_count: blast,
        particle_snap: steady(0.0),
        keyframes: keys,
        ..Emitter::default()
    }
}

fn system(e: Emitter) -> ParticleSet {
    let desc = Particles {
        max_particles: 500,
        scale_type: 2,
        physics_time_step: 0.05,
        start_fade_distance: 40.0,
        stop_fade_distance: 60.0,
        world_space: true,
        emitters: vec![e],
        ..Particles::default()
    };
    ParticleSet::new(
        desc,
        vec![auberean_draw::material::draw_layer(
            &auberean_assets::material::MaterialLayer::default(),
        )],
        Mat4::IDENTITY,
        Vec3::new(1.0, 1.0, 1.0),
        0.0,
        &Random::new(),
    )
}

/// Run `s` a frame every twentieth of a second, the times exact, through `until`.
/// Frames a hair longer than the systems' 0.05 s step: a system thinks only once a whole step
/// has passed, the time held to it at full precision.
fn run(s: &mut ParticleSet, until: u32) {
    for k in 0..=until {
        s.think(f64::from(k) * 0.050_000_1);
    }
}

fn drawn(s: &ParticleSet, now: f64) -> Vec<ObjectBatch> {
    let (right, forward, up) = camera_axes(0.0, 0.0);
    s.batches(now, Vec3::new(0.0, -5.0, 0.0), right, forward, up)
}

/// The centers and widths of the drawn quads.
fn quads(batches: &[ObjectBatch]) -> Vec<([f32; 3], f32, u32)> {
    let mut out = Vec::new();
    for b in batches {
        for q in b.vertices.chunks(4) {
            let c = [0, 1, 2].map(|i| q.iter().map(|v| v.position[i]).sum::<f32>() / 4.0);
            let w = q[3].position[0] - q[0].position[0];
            out.push((c, w, q[0].diffuse));
        }
    }
    out
}

/// A waveform's value at a time: steady is its base, speed grows with the time, sine and bounce
/// follow the sine of the frequency's angle plus the phase, square takes its two levels.
///
/// Behaviour: draw.particles.waveforms-take-their-shape-at-a-time
#[test]
fn waveforms_take_their_shape_at_a_time() {
    let rng = Random::new();
    let wave = |kind: u32| Waveform {
        kind,
        base: 1.0,
        amplitude: 2.0,
        frequency: 3.0,
        phase: 0.5,
        ..Waveform::default()
    };
    let t = 0.25;
    let angle = 3.0f32 * 0.25 + 0.5 * std::f32::consts::PI;
    let sin = dereth_primitives::num::math::sinf(angle);
    assert_eq!(waveform_at(&steady(4.0), 9.0, &rng), 4.0);
    assert!(
        (waveform_at(&wave(2), t, &rng) - (3.0 * 0.25 + 0.5 * std::f32::consts::PI + 1.0)).abs()
            < 1e-6
    );
    assert!((waveform_at(&wave(4), t, &rng) - (sin * 2.0 + 1.0)).abs() < 1e-6);
    let square = if sin < 0.0 { -1.0 } else { 3.0 };
    assert!((waveform_at(&wave(5), t, &rng) - square).abs() < 1e-6);
    assert!((waveform_at(&wave(6), t, &rng) - (sin.abs() * 2.0 + 1.0)).abs() < 1e-6);
    // noise stays within its amplitude of its base
    for _ in 0..100 {
        let v = waveform_at(&wave(3), t, &rng);
        assert!((-1.0..=3.0).contains(&v), "{v}");
    }
}

/// Each keyframe links to the next keyframe that blends the size, the mass and the color, or to
/// itself when none does; the last links to itself.
///
/// Behaviour: draw.particles.keyframes-link-to-the-next-that-blends-each-value
#[test]
fn keyframes_link_to_the_next_that_blends_each_value() {
    let keys = vec![
        key(0.0, FIRST, RED, 1.0, 1.0),
        key(0.5, 0x2, RED, 1.0, 1.0),
        key(1.0, 0x1, RED, 1.0, 1.0),
        key(1.5, 0x7, RED, 1.0, 1.0),
    ];
    assert_eq!(
        next_keys(&keys),
        vec![[2, 3, 1], [2, 3, 3], [3, 3, 3], [3, 3, 3]]
    );
    assert_eq!(next_keys(&keys[..1]), vec![[0, 0, 0]]);
}

/// A timed emitter discharges its blast every birth interval and stops at its emission limit,
/// however long it runs.
///
/// Behaviour: draw.particles.a-timed-emitter-stops-at-its-emission-limit
#[test]
fn a_timed_emitter_stops_at_its_emission_limit() {
    let mut s = system(emitter(
        0.1,
        5,
        20,
        100.0,
        vec![key(0.0, FIRST, RED, 1.0, 0.0)],
    ));
    run(&mut s, 60);
    assert_eq!(s.alive(), 20);
    let mut unlimited = system(emitter(
        0.1,
        5,
        0,
        100.0,
        vec![key(0.0, FIRST, RED, 1.0, 0.0)],
    ));
    run(&mut unlimited, 60);
    assert!(unlimited.alive() > 20, "{}", unlimited.alive());
}

/// A thrown particle moves from where it was born along its velocity, pulled by gravity times
/// its mass; it is drawn one physics step behind.
///
/// Behaviour: draw.particles.a-thrown-particle-falls-with-gravity-times-its-mass
#[test]
fn a_thrown_particle_falls_with_gravity_times_its_mass() {
    let mut s = system(emitter(
        0.05,
        1,
        1,
        100.0,
        vec![key(0.0, FIRST, RED, 1.0, 2.0)],
    ));
    run(&mut s, 20);
    let q = quads(&drawn(&s, 1.0));
    assert_eq!(q.len(), 1);
    // born at 0, drawn at the step before 1.0
    let t: f32 = 0.95;
    let expect = 0.5 * (-9.8 * 2.0) * t * t;
    assert!(
        (q[0].0[2] - expect).abs() < 1e-3,
        "{} vs {expect}",
        q[0].0[2]
    );
    assert!(q[0].0[0].abs() < 1e-5 && q[0].0[1].abs() < 1e-5);
    // negative mass rises
    let mut up = system(emitter(
        0.05,
        1,
        1,
        100.0,
        vec![key(0.0, FIRST, RED, 1.0, -1.0)],
    ));
    run(&mut up, 20);
    assert!(quads(&drawn(&up, 1.0))[0].0[2] > 0.0);
}

/// An unfilled square emitter starts each particle on one of its four edges.
///
/// Behaviour: draw.particles.a-square-emitter-without-fill-starts-particles-on-its-edges
#[test]
fn a_square_emitter_without_fill_starts_particles_on_its_edges() {
    let mut e = emitter(0.05, 10, 0, 100.0, vec![key(0.0, FIRST, RED, 1.0, 0.0)]);
    e.shape = 1;
    e.scale = steady(4.0);
    let mut s = system(e);
    run(&mut s, 10);
    let q = quads(&drawn(&s, 0.5));
    assert!(q.len() >= 50, "{}", q.len());
    for (c, _, _) in q {
        let edge = c[0].abs().max(c[1].abs());
        assert!((edge - 2.0).abs() < 1e-4, "{c:?}");
    }
}

/// A particle's color and size blend from its last keyframe toward the next keyframe that blends
/// them, by its age between the two.
///
/// Behaviour: draw.particles.color-and-size-blend-toward-the-next-keyframe-that-blends-them
#[test]
fn color_and_size_blend_toward_the_next_keyframe_that_blends_them() {
    let keys = vec![
        key(0.0, FIRST, RED, 1.0, 0.0),
        key(1.0, 0x3, BLUE, 3.0, 0.0),
    ];
    let mut s = system(emitter(0.05, 1, 1, 100.0, keys));
    run(&mut s, 10);
    let q = quads(&drawn(&s, 0.5));
    assert_eq!(q.len(), 1);
    let (_, width, diffuse) = q[0];
    assert!((width - 2.0).abs() < 1e-4, "{width}");
    let red = (diffuse >> 16) & 0xFF;
    let blue = diffuse & 0xFF;
    assert_eq!((diffuse >> 24, red, blue), (255, 127, 127), "{diffuse:08X}");
}

/// A streaking particle is drawn from its tail to its head: the head's corners on the side away
/// from the tail, the tail's on the side away from the head, across the way it runs on the
/// screen.
///
/// Behaviour: draw.particles.a-streak-stretches-from-its-tail-to-its-head
#[test]
fn a_streak_stretches_from_its_tail_to_its_head() {
    let rising = streak_corners(Vec3::new(0.0, 5.0, 1.0), Vec3::new(0.0, 5.0, 0.0), 0.1, 0.1);
    let z: Vec<f32> = rising.iter().map(|c| c.z).collect();
    assert_eq!(z, vec![1.1, 1.1, -0.1, -0.1]);
    let leftward = streak_corners(
        Vec3::new(-1.0, 5.0, 0.0),
        Vec3::new(0.0, 5.0, 0.0),
        0.1,
        0.1,
    );
    let x: Vec<f32> = leftward.iter().map(|c| c.x).collect();
    assert_eq!(x, vec![-1.1, -1.1, 0.1, 0.1]);
    // no length on the screen: an ordinary quad about the head
    let still = streak_corners(Vec3::new(2.0, 5.0, 0.0), Vec3::new(2.0, 9.0, 0.0), 0.5, 0.5);
    assert_eq!(still[0], Vec3::new(1.5, 5.0, 0.5));
}

/// A system goes once none of its emitters has anything more to discharge and its last
/// particles have died; one that is stopped goes when its particles have.
///
/// Behaviour: draw.particles.a-system-goes-once-it-is-spent-and-its-particles-are-dead
#[test]
fn a_system_goes_once_it_is_spent_and_its_particles_are_dead() {
    // Five at a time every tenth of a second up to twenty, each living a second.
    let mut s = system(emitter(
        0.1,
        5,
        20,
        1.0,
        vec![key(0.0, FIRST, RED, 1.0, 0.0)],
    ));
    run(&mut s, 2);
    assert!(!s.ready_to_die(0.1), "still discharging");
    run(&mut s, 10);
    assert_eq!(s.alive(), 20);
    assert!(!s.ready_to_die(0.5), "spent, but its particles live");
    run(&mut s, 40);
    assert_eq!(s.alive(), 0);
    assert!(s.ready_to_die(2.0));

    // With no limit it runs until stopped, then goes when its particles have died.
    let mut endless = system(emitter(
        0.1,
        5,
        0,
        1.0,
        vec![key(0.0, FIRST, RED, 1.0, 0.0)],
    ));
    run(&mut endless, 40);
    assert!(!endless.ready_to_die(2.0));
    endless.stop();
    assert!(
        !endless.ready_to_die(2.0),
        "stopped, but its particles live"
    );
    for k in 41..=80 {
        endless.think(f64::from(k) * 0.05);
    }
    assert_eq!(endless.alive(), 0);
    assert!(endless.ready_to_die(4.0));
}

/// A system with a physics duration, or with an emitter that has neither an emission limit nor
/// a time limit, runs until it is stopped; any other lasts about its longest emitter's start,
/// lifespan and emitting time (its limit in blasts at its birth rate, or its time limit if that
/// is longer), less the system's fast-forward.
///
/// Behaviour: draw.particles.a-system-without-limits-runs-until-stopped
#[test]
fn a_system_without_limits_runs_until_stopped() {
    let limited = |limit: u32, time_limit: f32| Emitter {
        start_time: 0.5,
        time_limit,
        ..emitter(0.25, 2, limit, 1.5, Vec::new())
    };
    let desc = |emitters: Vec<Emitter>, duration: f32| Particles {
        physics_duration: duration,
        fast_forward_time: 0.25,
        emitters,
        ..Particles::default()
    };
    // Ten blasts of two at a quarter second: 2.5 s, longer than the time limit.
    let one = approximate_lifetime(&desc(vec![limited(20, 1.0)], 0.0));
    assert!((one - (1.5 + 0.5 - 0.25 + 2.5)).abs() < 1e-5, "{one}");
    // The time limit when it is the longer; the longest emitter wins.
    let two = approximate_lifetime(&desc(vec![limited(20, 1.0), limited(2, 6.0)], 0.0));
    assert!((two - (1.5 + 0.5 - 0.25 + 6.0)).abs() < 1e-5, "{two}");
    // A random lifespan counts its amplitude.
    let mut random = limited(20, 1.0);
    random.lifespan = Waveform {
        kind: 3,
        base: 1.5,
        amplitude: -0.5,
        ..Waveform::default()
    };
    let three = approximate_lifetime(&desc(vec![random], 0.0));
    assert!((three - (2.0 + 0.5 - 0.25 + 2.5)).abs() < 1e-5, "{three}");
    // Endless.
    assert!(approximate_lifetime(&desc(vec![limited(20, 1.0), limited(0, 0.0)], 0.0)) < 0.0);
    assert!(approximate_lifetime(&desc(vec![limited(20, 1.0)], 3.0)) < 0.0);
    assert!(approximate_lifetime(&desc(Vec::new(), 0.0)).abs() < 1e-6);
}

/// A system of its own draws at its place without the place's turn: on an object turned a quarter
/// about the vertical, its particle 2 m along the emitter's x is still 2 m east of the place, and its
/// quad as wide across the screen as on an object not turned.
///
/// Behaviour: draw.particles.a-system-of-its-own-draws-without-its-places-turn
#[test]
fn a_system_of_its_own_draws_without_its_places_turn() {
    let e = Emitter {
        origin: steady3(2.0, 0.0, 0.0),
        ..emitter(1.0, 1, 1, 100.0, Vec::new())
    };
    let desc = Particles {
        max_particles: 500,
        scale_type: 2,
        physics_time_step: 0.05,
        start_fade_distance: 40.0,
        stop_fade_distance: 60.0,
        world_space: false,
        emitters: vec![e],
        ..Particles::default()
    };
    let at = Vec3::new(10.0, 0.0, 0.0);
    let half = std::f32::consts::FRAC_1_SQRT_2;
    let turned = Mat4::from_parts(
        at,
        auberean_primitives::Quat {
            w: half,
            x: 0.0,
            y: 0.0,
            z: half,
        },
        Vec3::new(1.0, 1.0, 1.0),
    );
    let quad_on = |place: Mat4| {
        let mut s = ParticleSet::new(
            desc.clone(),
            vec![auberean_draw::material::draw_layer(
                &auberean_assets::material::MaterialLayer::default(),
            )],
            place,
            Vec3::new(1.0, 1.0, 1.0),
            0.0,
            &Random::new(),
        );
        run(&mut s, 40);
        quads(&drawn(&s, 2.0))
    };
    let turned = quad_on(turned);
    let plain = quad_on(Mat4::translation(at));
    assert_eq!(turned.len(), 1);
    assert_eq!(turned, plain);
    assert!((turned[0].0[0] - 12.0).abs() < 1e-3, "{turned:?}");
}

/// A system whose particles live in the world starts its emitters' distance reckoning where it is
/// first placed: placed 30 m from the origin, an emitter discharging every metre it moves
/// discharges nothing until it moves, then a blast for each metre.
///
/// Behaviour: draw.particles.a-world-system-reckons-distance-from-where-it-is-first-placed
#[test]
fn a_world_system_reckons_distance_from_where_it_is_first_placed() {
    let by_distance = Emitter {
        emission_distance: 1.0,
        ..emitter(0.0, 1, 0, 100.0, vec![key(0.0, FIRST, RED, 0.1, 0.0)])
    };
    let mut s = system(by_distance);
    let at = |x: f32| Mat4::translation(Vec3::new(x, 0.0, 0.0));
    s.set_place(at(30.0), Vec3::new(1.0, 1.0, 1.0));
    s.place_emitters(0.0);
    s.think(0.0);
    s.think(0.05);
    assert_eq!(s.alive(), 0, "nothing until it moves");
    s.set_place(at(33.0), Vec3::new(1.0, 1.0, 1.0));
    s.place_emitters(0.1);
    s.think(0.1);
    assert_eq!(s.alive(), 3, "a blast each metre of three");
    assert!(
        s.lighting_origin().x.abs() < 1e-6,
        "lit where each particle is"
    );
}

/// A repulsor pushes a particle within its range along the way from it, harder the nearer
/// (`a (1 - d / range)`: half its push halfway out), not at all at or past its range, nor along an
/// axis the emitter constrains; an attractor pulls. It pushes while its object moves at least the
/// speed it asks for, within its lifetime (one under 0.0002 s lasts as long as its object keeps
/// it), and goes once its outlived lifetime is found or its object's last hold on it goes.
///
/// Behaviour: draw.particles.a-repulsor-pushes-the-particles-in-its-range
#[test]
fn a_repulsor_pushes_the_particles_in_its_range() {
    use auberean_draw::particles::{push_of, Push};
    use auberean_draw::systems::{ParticleSystems, Repulsor};
    let r = Push {
        origin: Vec3::new(10.0, 0.0, 0.0),
        range: 4.0,
        acceleration: 3.0,
        attracts: false,
    };
    let free = [false; 3];
    let half = push_of(&r, Vec3::new(12.0, 0.0, 0.0), free);
    assert!(
        (half.x - 1.5).abs() < 1e-6 && half.y == 0.0 && half.z == 0.0,
        "{half:?}"
    );
    assert_eq!(
        push_of(&r, Vec3::new(14.0, 0.0, 0.0), free),
        Vec3::ZERO,
        "at its range"
    );
    assert_eq!(
        push_of(&r, Vec3::new(10.0, 0.0, 0.0), free),
        Vec3::ZERO,
        "on it"
    );
    let diagonal = push_of(&r, Vec3::new(11.0, 1.0, 0.0), [true, false, false]);
    assert_eq!(diagonal.x, 0.0, "x constrained");
    assert!(diagonal.y > 0.0);
    let pull = push_of(
        &Push {
            attracts: true,
            ..r
        },
        Vec3::new(12.0, 0.0, 0.0),
        free,
    );
    assert!((pull.x + 1.5).abs() < 1e-6, "an attractor pulls: {pull:?}");

    let walker = Repulsor::new(3.0, 3.5, -1.0, 0.1, false, 0.0);
    let mut moving = walker;
    moving.speed = 1.0;
    assert!(!walker.active(5.0), "standing still");
    assert!(moving.active(1000.0), "moving, for as long as it is kept");
    assert!(!walker.ready_to_die(1000.0));
    let burst = Repulsor::new(5.0, 3.0, 1.0, 0.0, false, 0.0);
    assert!(burst.active(0.5) && !burst.ready_to_die(0.5));
    assert!(!burst.active(1.5) && burst.ready_to_die(1.5));

    let systems = ParticleSystems::default();
    let held = systems.create_repulsor(walker);
    let again = held.clone();
    assert_eq!(systems.repulsors(), 1);
    drop(held);
    assert_eq!(systems.repulsors(), 1, "held still");
    drop(again);
    assert_eq!(systems.repulsors(), 0, "the last hold gone");
}

/// An effect entry moves its slider to its intensity over its ramp time, and an entry with a color
/// moves the object's four color sliders to that color's parts; an entry with neither moves none.
///
/// Behaviour: draw.effects.an-effect-moves-its-sliders-over-its-ramp
#[test]
fn an_effect_moves_its_sliders_over_its_ramp() {
    use auberean_assets::fx::FxNode;
    use auberean_draw::effects::{node_sliders, FxSlider};
    let slider = |slider, value| FxSlider {
        slider,
        value,
        time: 2.0,
        children: false,
    };
    let node = FxNode {
        slider: Some(0x4000_0015),
        intensity: Some(0.25),
        ramp_time: Some(2.0),
        ..FxNode::default()
    };
    assert_eq!(node_sliders(&node), [slider(0x4000_0015, 0.25)]);
    let node = FxNode {
        color: Some((
            true,
            auberean_primitives::Rgba {
                r: 0.1,
                g: 0.2,
                b: 0.3,
                a: 0.4,
            },
        )),
        ramp_time: Some(2.0),
        ..FxNode::default()
    };
    assert_eq!(
        node_sliders(&node),
        [
            slider(0x4000_0016, 0.1),
            slider(0x4000_0017, 0.2),
            slider(0x4000_0018, 0.3),
            slider(0x4000_001A, 0.4),
        ]
    );
    assert!(node_sliders(&FxNode::default()).is_empty());
}

/// A system draws with its look's particle scale, times its object's scale when its description
/// scales it with its object; a system that does not draws by the particle scale alone.
///
/// Behaviour: draw.particles.a-looks-particle-scale-sizes-its-systems
#[test]
fn a_looks_particle_scale_sizes_its_systems() {
    let object = Vec3::new(1.5, 1.5, 3.0);
    let with_object = system(Emitter::default());
    assert_eq!(with_object.desc().scale_type, 2);
    assert_eq!(
        with_object.drawn_scale(object, 2.0),
        Vec3::new(3.0, 3.0, 6.0)
    );
    let mut desc = with_object.desc().clone();
    desc.scale_type = 1;
    let alone = ParticleSet::new(
        desc,
        Vec::new(),
        Mat4::IDENTITY,
        object,
        0.0,
        &Random::new(),
    );
    assert_eq!(alone.drawn_scale(object, 2.0), Vec3::new(2.0, 2.0, 2.0));
}
