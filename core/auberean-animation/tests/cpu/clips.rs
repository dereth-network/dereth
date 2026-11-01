//! Clips sampled, blends weighted and vertices skinned, on synthetic records.
//! Fixture: a one-bone clip with two offset keys and two rotation keys; a two-slider blend grid;
//! a two-bone palette; a blend of two clips, each with a game hook.

use std::collections::BTreeMap;
use std::sync::Arc;

use auberean_animation::clip::{bone_at, slerp, Clip, Pose};
use auberean_animation::graph::{cube_weights, triggers, Animations, GraphState};
use auberean_animation::skin::{skin, Influences};
use auberean_animation::sliders::Sliders;
use auberean_animation::triggers::Gathered;
use auberean_assets::animation::{
    Animation, AnimationEvents, AnimationKind, BoneTrack, GameHook, KeyframeAnimation, RotationKey,
    SliderTicks, VectorKey,
};
use auberean_primitives::{DataId, Quat, Vec3};
use auberean_world::math::Mat4;
use dereth_primitives::num::math;

fn state(s: &Sliders) -> GraphState<'_> {
    GraphState {
        sliders: s,
        mode: 0,
        element: 0,
        terrain: 0,
    }
}

fn track() -> BoneTrack {
    BoneTrack {
        offset: vec![
            VectorKey {
                value: Vec3::new(0.0, 0.0, 0.0),
                time: 0.0,
            },
            VectorKey {
                value: Vec3::new(2.0, 0.0, 0.0),
                time: 1.0,
            },
        ],
        rotation: vec![
            RotationKey {
                value: Quat::IDENTITY,
                time: 0.0,
            },
            RotationKey {
                value: Quat {
                    w: 0.0,
                    x: 0.0,
                    y: 0.0,
                    z: 1.0,
                },
                time: 1.0,
            },
        ],
        ..BoneTrack::default()
    }
}

/// A track takes the last key at or before the time and moves toward the next by the fraction of
/// the way (rotations along the short arc); past its last key it holds it.
///
/// Behaviour: animation.clip.keys-interpolate-and-the-last-holds
#[test]
fn keys_interpolate_and_the_last_holds() {
    let t = track();
    let (off, scale, rot, w) = bone_at(&t, 0.25);
    assert!((off.x - 0.5).abs() < 1e-6, "{off:?}");
    assert_eq!(
        (scale, w),
        (Vec3::new(1.0, 1.0, 1.0), 1.0),
        "no keys: unit scale, full weight"
    );
    let quarter = math::cosf(std::f32::consts::FRAC_PI_4 / 2.0);
    assert!(
        (rot.w - quarter).abs() < 1e-4,
        "a quarter of a half turn: {rot:?}"
    );
    let (off, _, rot, _) = bone_at(&t, 3.0);
    assert_eq!((off.x, rot.z), (2.0, 1.0), "held past the last key");
    let h = slerp(
        Quat::IDENTITY,
        Quat {
            w: -1.0,
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        0.5,
    );
    assert!(
        (h.w.abs() - 1.0).abs() < 1e-5,
        "the same rotation either sign: {h:?}"
    );
}

/// A clip plays at a phase in cycles: the phase's fraction times the clip's period is the time,
/// so one and a half cycles into a 1 s clip samples its middle; its bones add into the pose by their weight.
///
/// Behaviour: animation.clip.a-clip-plays-at-the-fraction-of-its-cycle
#[test]
fn a_clip_plays_at_the_fraction_of_its_cycle() {
    let keys = KeyframeAnimation {
        period: 1.0,
        bones: vec![track()],
        events: None,
    };
    let mut pose = Pose::new(1);
    Clip { keys: &keys }.animate(0.5, 1.5, &[None], &mut pose);
    assert!(
        (pose.bones[0].offset.x - 0.5).abs() < 1e-6,
        "half the weight of the middle: {:?}",
        pose.bones[0]
    );
    assert!((pose.weights[0] - 0.5).abs() < 1e-6);
}

/// A blend weighs its children over the slider grid multilinearly, the first slider varying
/// fastest: sliders midway between ticks give each of the four corners a quarter; a slider on a
/// tick gives that tick's children all of its weight; a slider past the ends is held at the end.
///
/// Behaviour: animation.cube.a-blend-weighs-its-children-over-the-slider-grid
#[test]
fn a_blend_weighs_its_children_over_the_slider_grid() {
    let ticks = |v: &[f32]| v.iter().map(|t| t.to_bits()).collect::<Vec<u32>>();
    let sliders = vec![
        SliderTicks {
            slider: 1,
            ticks: ticks(&[0.0, 2.0]),
        },
        SliderTicks {
            slider: 2,
            ticks: ticks(&[-1.0, 1.0]),
        },
    ];
    let mut s = Sliders::default();
    s.set(1, 1.0);
    s.set(2, 0.0);
    assert_eq!(cube_weights(&sliders, 4, &state(&s)), vec![0.25; 4]);
    s.set(1, 2.0);
    s.set(2, 9.0);
    assert_eq!(
        cube_weights(&sliders, 4, &state(&s)),
        vec![0.0, 0.0, 0.0, 1.0]
    );
    s.set(1, 0.5);
    s.set(2, -1.0);
    let w = cube_weights(&sliders, 4, &state(&s));
    assert!(
        (w[0] - 0.75).abs() < 1e-6 && (w[1] - 0.25).abs() < 1e-6 && w[2] == 0.0,
        "{w:?}"
    );
}

/// Synthetic animation records by id.
struct Records(BTreeMap<u32, Arc<Animation>>);

impl Animations for Records {
    fn animation(&self, id: DataId) -> Option<Arc<Animation>> {
        self.0.get(&id.0).cloned()
    }
}

/// A one-second clip whose only event is a game hook (kind `0x40000002`, data 3) at `time` of
/// its cycle.
fn hooked_clip(time: f32) -> Arc<Animation> {
    Arc::new(Animation {
        animator: AnimationKind::Keyframes(KeyframeAnimation {
            period: 1.0,
            bones: vec![track()],
            events: Some(AnimationEvents {
                hooks: vec![GameHook {
                    kind: 0x4000_0002,
                    number: 0,
                    data: 3,
                    triggered: false,
                    time,
                }],
                ..AnimationEvents::default()
            }),
        }),
    })
}

/// A blend of two clips whose game hooks (alike but for their times, 0.4 and 0.6 of the cycle)
/// merge into one at their times weighed by the children's weights, due at that time: with the
/// slider midway, at 0.5. Before then it has nothing to set off; past it, the one hook. A clip's
/// own hook is due at its own time.
///
/// Behaviour: animation.cube.a-blends-hooks-merge-and-fall-due-at-their-weighed-time
#[test]
fn a_blends_hooks_merge_and_fall_due_at_their_weighed_time() {
    let ticks = |v: &[f32]| v.iter().map(|t| t.to_bits()).collect::<Vec<u32>>();
    let blend = Arc::new(Animation {
        animator: AnimationKind::Blend {
            sliders: vec![SliderTicks {
                slider: 1,
                ticks: ticks(&[0.0, 1.0]),
            }],
            animations: vec![DataId(0x0500_0001), DataId(0x0500_0002)],
        },
    });
    let records = Records(BTreeMap::from([
        (0x0500_0001, hooked_clip(0.4)),
        (0x0500_0002, hooked_clip(0.6)),
        (0x0500_0003, blend),
    ]));
    let mut s = Sliders::default();
    s.set(1, 0.5);
    let gather = |id: u32, time: f32| {
        let mut g = Gathered::default();
        triggers(&records, DataId(id), &state(&s), time, true, &mut g);
        g.hooks
    };
    assert!(gather(0x0500_0003, 0.1).is_empty(), "nothing at the start");
    assert!(gather(0x0500_0003, 0.45).is_empty(), "nothing before 0.5");
    let due = gather(0x0500_0003, 0.55);
    assert_eq!(due.len(), 1, "one hook: {due:?}");
    assert!((due[0].time - 0.5).abs() < 1e-6, "{due:?}");
    assert_eq!((due[0].kind, due[0].data), (0x4000_0002, 3));
    assert!(
        gather(0x0500_0001, 0.3).is_empty(),
        "a clip's hook waits for its time"
    );
    assert_eq!(gather(0x0500_0001, 0.45).len(), 1);
}

/// A skinned vertex moves by the weighted sum of its bones' transforms.
///
/// Behaviour: animation.skin.weights-blend-bone-transforms
#[test]
fn weights_blend_bone_transforms() {
    let palette = [
        Mat4::translation(Vec3::new(1.0, 0.0, 0.0)),
        Mat4::translation(Vec3::new(0.0, 0.0, 3.0)),
    ];
    let inf = Influences {
        count: 2,
        bones: [0, 1, 0, 0],
        weights: [0.5, 0.5, 0.0, 0.0],
    };
    let (p, _) = skin(&[Vec3::new(1.0, 1.0, 1.0)], &[Vec3::ZERO], &[inf], &palette);
    assert_eq!(p[0], Vec3::new(1.5, 1.0, 2.5));
}
