//! The falling rain and snow, and what a puddle mirrors, as the weather's shader works them out:
//! every drop and flake keeps its place in the world however the viewer moves, nothing near the
//! eye is drawn as a large blur, and a puddle whose mirror finds nothing holds the sky.

use dereth_render_hifi::passes::weather::{self, LATTICE_PERIOD};

use crate::common::wgsl_eval::{Evaluator, Value};

fn shader() -> Evaluator {
    Evaluator::new(&weather::shader_source())
}

/// The boxes of the form's three lattices (`rain_tier` or `snow_tier`), metres across and high.
fn boxes(shader: &Evaluator, form: &str) -> Vec<[f32; 3]> {
    (0..3u32)
        .map(|k| {
            let t = shader.call(form, &[Value::U32(k)]).floats();
            [2.0 * t[0], 2.0 * t[0], 2.0 * t[1]]
        })
        .collect()
}

/// Where particle `id` of the lattice `salt` names stands, for the eye at `eye` (in the
/// lattices' wrapped world), the lattice fallen `fall` metres and carried `drift` metres.
fn place(
    shader: &Evaluator,
    id: u32,
    salt: u32,
    size: [f32; 3],
    eye: [f32; 3],
    fall: f32,
    drift: [f32; 2],
) -> [f32; 3] {
    let p = shader
        .call(
            "lattice_place",
            &[
                Value::U32(id),
                Value::U32(salt),
                Value::vector(&size),
                Value::vector(&eye),
                Value::F32(fall),
                Value::vector(&drift),
            ],
        )
        .floats();
    [p[0], p[1], p[2]]
}

/// How far `p` stands out from `eye` in a box `size` across, 0 at the eye and 1 at a side.
fn out_in_box(p: [f32; 3], eye: [f32; 3], size: [f32; 3]) -> f32 {
    (0..3)
        .map(|i| (p[i] - eye[i]).abs() / (size[i] * 0.5))
        .fold(0.0, f32::max)
}

fn apart(a: [f32; 3], b: [f32; 3]) -> f32 {
    (0..3).map(|i| (a[i] - b[i]).abs()).fold(0.0, f32::max)
}

/// Behaviour: hifi.weather.the-rain-and-snow-hold-still-in-the-world-as-the-viewer-moves
#[test]
fn every_drop_and_flake_keeps_its_place_in_the_world_however_the_viewer_moves() {
    let shader = shader();
    // An eye somewhere in the lattices' wrapped world, and where it is after a walker's step, a
    // second's run, a jump, and a camera swung a quarter of the way round a body.
    let eye = [701.3, 1213.6, 84.2];
    let moves = [
        [0.07, -0.03, 0.0],
        [3.1, 2.4, 0.0],
        [0.0, 0.0, 1.1],
        [-4.1, 3.3, 0.6],
    ];
    let (fall, drift) = (37.25, [3.5, -12.75]);
    #[allow(clippy::cast_possible_truncation)] // the period, exact in either width
    let period = LATTICE_PERIOD as f32;
    for form in ["rain_tier", "snow_tier"] {
        for (k, size) in boxes(&shader, form).into_iter().enumerate() {
            // The period the fall, the wind's drift and render space's place in the world wrap
            // over is a whole number of the lattice's box, so no wrap moves a particle.
            for side in size {
                let n = period / side;
                assert!(
                    (n - n.round()).abs() < 1e-4,
                    "{form}: a box {side} m across goes {n} times into the period"
                );
            }
            let salt = 81 + u32::try_from(k).expect("three") * 4099;
            for by in moves {
                let moved = [eye[0] + by[0], eye[1] + by[1], eye[2] + by[2]];
                let mut kept = 0;
                for id in 0..800 {
                    let before = place(&shader, id, salt, size, eye, fall, drift);
                    let after = place(&shader, id, salt, size, moved, fall, drift);
                    // Each is drawn from the box round its eye.
                    assert!(
                        out_in_box(before, eye, size) <= 1.0 + 1e-4,
                        "{form} {k} {id}"
                    );
                    assert!(
                        out_in_box(after, moved, size) <= 1.0 + 1e-4,
                        "{form} {k} {id}"
                    );
                    // One drawn, not faded out, both before and after the move stands where it
                    // stood, to a millimetre.
                    if out_in_box(before, eye, size) < 0.97
                        && out_in_box(before, moved, size) < 0.97
                    {
                        assert!(
                            apart(before, after) < 1e-3,
                            "{form} lattice {k}: particle {id} moved from {before:?} to \
                             {after:?} as the eye moved by {by:?}"
                        );
                        kept += 1;
                    }
                }
                assert!(
                    kept > 100,
                    "{form} {k} {by:?}: only {kept} particles compared"
                );
            }
            // The particles move only as the lattice falls and the wind carries it: half a
            // metre more fall puts each half a metre lower, and nothing jumps where the fall and
            // the wind's drift wrap round the lattices' period.
            for id in 0..100 {
                let p = place(&shader, id, salt, size, eye, fall, drift);
                if out_in_box(p, eye, size) < 0.9 {
                    let lower = place(&shader, id, salt, size, eye, fall + 0.5, drift);
                    let want = [p[0], p[1], p[2] - 0.5];
                    assert!(apart(lower, want) < 1e-3, "{form} {k} {id}: {lower:?}");
                }
                let before = place(
                    &shader,
                    id,
                    salt,
                    size,
                    eye,
                    period - 0.01,
                    [period - 0.01; 2],
                );
                let after = place(&shader, id, salt, size, eye, -0.01, [-0.01; 2]);
                assert!(
                    apart(before, after) < 2e-3,
                    "{form} {k} {id}: jumped from {before:?} to {after:?} where the fall wraps"
                );
            }
        }
    }
}

/// Behaviour: hifi.weather.nothing-passes-the-eye-as-a-large-blur
#[test]
fn near_the_eye_no_drop_or_flake_is_drawn_large_and_within_half_a_metre_none_is_drawn() {
    let shader = shader();
    // The widest a flake is drawn, radians either side of its middle: seven pixels at 1080p for
    // the usual view, fourteen at 4K; and a streak, under one and a half pixels at 1080p.
    const FLAKE: f32 = 0.0065;
    const STREAK: f32 = 0.0013;
    // The longest a streak is drawn, radians.
    const LONGEST: f32 = 0.11;
    for height in [720.0f32, 1080.0, 2160.0] {
        let pixel = 0.9 / height;
        for flake in [false, true] {
            let widest = if flake { FLAKE } else { STREAK };
            for radius in [0.0005f32, 0.002, 0.01, 0.025, 0.05] {
                for step in 0..400 {
                    #[allow(clippy::cast_precision_loss)] // a few hundred steps
                    let dist = 0.02 * 1.03f32.powi(step);
                    let d = shader
                        .call(
                            "drawn_size",
                            &[
                                Value::F32(radius),
                                Value::F32(dist),
                                Value::F32(pixel),
                                Value::Bool(flake),
                            ],
                        )
                        .floats();
                    let (drawn, keep) = (d[0], d[1]);
                    let what = format!(
                        "{} of {radius} m at {dist} m, {height} rows",
                        if flake { "a flake" } else { "a streak" }
                    );
                    assert!(
                        drawn * pixel <= widest.max(0.6 * pixel) * 1.0001,
                        "{what}: drawn {drawn} pixels wide"
                    );
                    assert!(drawn >= 0.6 - 1e-4, "{what}: drawn {drawn} pixels wide");
                    assert!((0.0..=1.0).contains(&keep), "{what}: keeps {keep}");
                    if dist <= 0.5 {
                        assert!(keep == 0.0, "{what}: drawn so near the eye ({keep})");
                    } else if dist < 1.5 {
                        assert!(keep < 1.0, "{what}: drawn in full so near the eye");
                    }
                }
            }
        }
        // A drop a step from the eye, falling a shutter's length, is no longer on screen than
        // the longest a streak is drawn.
        let head = [640.0, 300.0];
        let tail = shader
            .call(
                "streak_tail",
                &[
                    Value::vector(&head),
                    Value::vector(&[640.0, 300.0 + 0.5 / pixel]),
                    Value::F32(pixel),
                ],
            )
            .floats();
        let run = dereth_primitives::num::math::hypotf(tail[1] - head[1], tail[0] - head[0]);
        assert!(
            run * pixel <= LONGEST,
            "{height}: a streak {run} pixels long"
        );
        let short = shader
            .call(
                "streak_tail",
                &[
                    Value::vector(&head),
                    Value::vector(&[641.0, 310.0]),
                    Value::F32(pixel),
                ],
            )
            .floats();
        assert_eq!(
            short,
            vec![641.0, 310.0],
            "a short streak is drawn as it is"
        );
    }
}

/// Behaviour: hifi.weather.where-a-puddles-mirror-finds-nothing-it-holds-the-sky
#[test]
fn where_a_puddles_mirror_finds_nothing_it_holds_the_sky_never_black() {
    let shader = shader();
    // A day's, a rainy day's and a night's fog, linear.
    let fogs = [[0.55, 0.58, 0.66], [0.33, 0.35, 0.38], [0.012, 0.012, 0.02]];
    for fog in fogs {
        for i in 0..=10 {
            #[allow(clippy::cast_precision_loss)]
            let up = i as f32 / 10.0;
            let side = (1.0 - up * up).sqrt();
            let r = [side * 0.6, side * 0.8, up];
            let sky = shader
                .call("sky_colour", &[Value::vector(&r), Value::vector(&fog)])
                .floats();
            // The sky is the fog's colour, dimmed toward the horizon by no more than three
            // tenths.
            for c in 0..3 {
                assert!(sky[c] >= fog[c] * 0.7 - 1e-6, "{fog:?} {r:?}: {sky:?}");
            }
            // A march that found nothing, its colour black and no sureness in it, gives the sky;
            // one half sure of a black thing gives at least half the sky.
            for (sure, share) in [(0.0, 1.0), (0.5, 0.5), (-0.3, 1.0)] {
                let found = [0.0, 0.0, 0.0, sure];
                let seen = shader
                    .call(
                        "mirrored",
                        &[
                            Value::vector(&found),
                            Value::vector(&r),
                            Value::vector(&fog),
                        ],
                    )
                    .floats();
                for c in 0..3 {
                    assert!(
                        seen[c] >= sky[c] * share - 1e-6,
                        "{fog:?} {r:?} sure {sure}: the puddle mirrors {seen:?} against the \
                         sky's {sky:?}"
                    );
                }
            }
            // And a march sure of what it found gives what it found.
            let found = [0.2, 0.3, 0.4, 1.0];
            let seen = shader
                .call(
                    "mirrored",
                    &[
                        Value::vector(&found),
                        Value::vector(&r),
                        Value::vector(&fog),
                    ],
                )
                .floats();
            for (s, f) in seen.iter().zip(found) {
                assert!((s - f).abs() < 1e-6, "{seen:?} for a sure {found:?}");
            }
        }
    }
}
