//! What the animation records say about themselves: each animation's kind number, and a place on
//! the body before a setup has filled it in.
//! Fixture: one animation of each kind, built in place; an empty holding location.

use auberean_animation::graph::kind_of;
use auberean_assets::animation::{Animation, AnimationKind, KeyframeAnimation};
use auberean_assets::setup::Location;
use auberean_primitives::DataId;

/// Each kind of animation answers the number the records give it: keyframes `0x40000001`, a
/// blend `0x40000002`, a body's `0x40000003`, a look `0x40000005`, an axis cube `0x40000006`, a
/// random node `0x40000007`, a switch `0x40000008`, the angle limits `0x4000000A`.
///
/// Behaviour: animation.records.each-animation-kind-answers-its-number
#[test]
fn each_animation_kind_answers_its_number() {
    let kinds = [
        (
            AnimationKind::Keyframes(KeyframeAnimation::default()),
            0x4000_0001,
        ),
        (
            AnimationKind::Blend {
                sliders: Vec::new(),
                animations: Vec::new(),
            },
            0x4000_0002,
        ),
        (
            AnimationKind::Body {
                animation: DataId(0x0500_0001),
            },
            0x4000_0003,
        ),
        (AnimationKind::Look, 0x4000_0005),
        (
            AnimationKind::AxisCube {
                origin: DataId(0x0500_0001),
                animations: Vec::new(),
                sliders: Vec::new(),
            },
            0x4000_0006,
        ),
        (
            AnimationKind::Random {
                slider: 0,
                animations: Vec::new(),
                frequency: 1.0,
                ramp_time: 0.25,
                random_slider: 0,
                variation: 0.0,
            },
            0x4000_0007,
        ),
        (
            AnimationKind::Switch {
                switch_kind: 0,
                default: DataId(0x0500_0001),
                overrides: Vec::new(),
            },
            0x4000_0008,
        ),
        (
            AnimationKind::Angle {
                bones: Vec::new(),
                max_x: 0.0,
                min_x: 0.0,
                max_y: 0.0,
                min_y: 0.0,
            },
            0x4000_000A,
        ),
    ];
    for (animator, number) in kinds {
        let a = Animation { animator };
        assert_eq!(kind_of(&a), number, "{:?}", a.animator);
    }
}

/// A holding location or connection point not yet read from a setup is of kind 0 and on no bone
/// (`u32::MAX`), so a body asked where it holds something it has no place for answers from its own
/// frame rather than from its first bone.
///
/// Behaviour: animation.setups.a-place-not-read-from-a-setup-is-on-no-bone
#[test]
fn a_place_not_read_from_a_setup_is_on_no_bone() {
    let empty = Location::default();
    assert_eq!(empty.kind, 0);
    assert_eq!(empty.bone, u32::MAX);
}
