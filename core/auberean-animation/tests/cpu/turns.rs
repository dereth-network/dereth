//! The head turn and the slope lean on made-up poses.
//! Fixture: a four-bone pose (a head bone, a neck bone, a leg bone, an untyped bone), every bone
//! at rest; a lean node that turns the leg bone with shares 0.5 about x and -0.25 about y, its
//! slope held within 0.3 either way.

use auberean_animation::clip::{Pose, Sum};
use auberean_animation::sliders::Sliders;
use auberean_animation::turns::{
    angle, lean_slider, lean_steering, look, look_slider, Ground, Lean, LOOK_FULL, LOOK_PARTIAL,
};
use auberean_primitives::{Quat, Vec3};
use dereth_primitives::num::math;

const LEG: u32 = 0x8100_000E;
const TYPES: [u32; 4] = [LOOK_FULL[0], LOOK_PARTIAL[0], LEG, 0];

fn rest() -> Pose {
    let mut p = Pose::new(TYPES.len());
    for b in &mut p.bones {
        *b = Sum {
            offset: Vec3::ZERO,
            scale: Vec3::new(1.0, 1.0, 1.0),
            rotation: Quat::IDENTITY,
        };
    }
    p
}

/// The angle of a rotation about z, and how far it is from being about z alone.
fn about_z(q: Quat) -> (f32, f32) {
    (2.0 * math::atan2f(q.z, q.w), q.x.abs() + q.y.abs())
}

/// The look turns the head bones by 0.35 of the look direction and the neck bones by 0.2325 of
/// it (about the body's up axis here), leaving other bones; a direction longer than one is cut
/// back to unit length first, and a bone whose rotation is not of unit length is brought to it
/// before it turns.
///
/// Behaviour: animation.turns.the-look-turns-the-head-fully-and-the-neck-partly
#[test]
fn the_look_turns_the_head_fully_and_the_neck_partly() {
    let mut s = Sliders::default();
    s.set(look_slider::Z, 1.0);
    let mut p = rest();
    look(1.0, &s, &TYPES, &mut p);
    let (head, off) = about_z(p.bones[0].rotation);
    assert!((head - 0.35).abs() < 1e-6 && off == 0.0, "{head} {off}");
    let (neck, off) = about_z(p.bones[1].rotation);
    assert!(
        (neck - 0.25 * 0.93).abs() < 1e-6 && off == 0.0,
        "{neck} {off}"
    );
    assert_eq!(p.bones[2].rotation, Quat::IDENTITY);
    assert_eq!(p.bones[3].rotation, Quat::IDENTITY);

    // Twice as far reads as unit length; half the weight turns half as far.
    s.set(look_slider::Z, 2.0);
    let mut far = rest();
    look(1.0, &s, &TYPES, &mut far);
    assert!((about_z(far.bones[0].rotation).0 - 0.35).abs() < 1e-6);
    s.set(look_slider::Z, 1.0);
    let mut half = rest();
    look(0.5, &s, &TYPES, &mut half);
    assert!((about_z(half.bones[0].rotation).0 - 0.175).abs() < 1e-6);

    // A summed rotation twice unit length is made unit first.
    let mut long = rest();
    long.bones[0].rotation = Quat {
        w: 2.0,
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    look(1.0, &s, &TYPES, &mut long);
    let q = long.bones[0].rotation;
    assert!((q.w * q.w + q.z * q.z - 1.0).abs() < 1e-6, "{q:?}");
}

/// The slope lean steers its two sliders toward the ground's slope under the body (the normal's
/// forward part, negated, and its sideways part, each held within the node's limits): at once
/// when a slider is within 0.15 of it, else over a quarter second; toward level when the body is
/// in the air or sliding down; not at all when the body is not placed. Its bones turn by the
/// sliders' values from before the frame's steering, times the weight and their shares.
///
/// Behaviour: animation.turns.the-lean-follows-the-slope-within-its-limits
#[test]
fn the_lean_follows_the_slope_within_its_limits() {
    let bones = [(LEG, 0.5, -0.25)];
    let node = Lean {
        bones: &bones,
        max_x: 0.3,
        min_x: -0.3,
        max_y: 0.3,
        min_y: -0.3,
    };
    let slope = Ground {
        contact: true,
        sliding: false,
        normal: Vec3::new(0.1, 0.6, 0.79),
        placed: true,
    };
    let asked = lean_steering(&node, &slope, (0.0, 0.0));
    assert_eq!(
        asked[0],
        (lean_slider::X, -0.3, 0.25),
        "held to the limit, far: over time"
    );
    assert_eq!(asked[1], (lean_slider::Y, 0.1, 0.0), "close: at once");
    let air = Ground {
        contact: false,
        ..slope
    };
    let sliding = Ground {
        sliding: true,
        ..slope
    };
    for g in [air, sliding] {
        let asked = lean_steering(&node, &g, (-0.3, 0.1));
        assert_eq!(asked[0], (lean_slider::X, 0.0, 0.25));
        assert_eq!(asked[1], (lean_slider::Y, 0.0, 0.0));
    }

    let mut s = Sliders::default();
    let mut p = rest();
    angle(&node, 1.0, &slope, &mut s, &TYPES, &mut p);
    assert_eq!(
        p.bones[2].rotation,
        Quat::IDENTITY,
        "the sliders were level"
    );
    assert_eq!(s.desired(lean_slider::X), -0.3);
    assert_eq!(s.current(lean_slider::Y), 0.1, "set at once");
    s.update(0.25);
    assert_eq!(s.current(lean_slider::X), -0.3);

    let mut p = rest();
    angle(&node, 1.0, &slope, &mut s, &TYPES, &mut p);
    let q = p.bones[2].rotation;
    let (want_x, want_y) = (-0.3f32 * 0.5, 0.1f32 * -0.25);
    let angle_len = (want_x * want_x + want_y * want_y).sqrt();
    let k = math::sinf(angle_len / 2.0) / angle_len;
    assert!(
        (q.x - want_x * k).abs() < 1e-6 && (q.y - want_y * k).abs() < 1e-6,
        "{q:?}"
    );
    for b in [0, 1, 3] {
        assert_eq!(p.bones[b].rotation, Quat::IDENTITY);
    }

    let mut idle = Sliders::default();
    let unplaced = Ground {
        placed: false,
        ..slope
    };
    angle(&node, 1.0, &unplaced, &mut idle, &TYPES, &mut rest());
    assert_eq!(idle.desired(lean_slider::X), 0.0, "not placed: not steered");
}
