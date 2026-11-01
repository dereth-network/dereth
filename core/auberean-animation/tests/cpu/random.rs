//! A random node on synthetic records: idles that pick a variant now and then.
//! Fixture: a random node over four one-bone clips, rolling its variant on one slider and
//! crossfading along another, about once a second (give or take a quarter) over a quarter second;
//! a third slider moving the whole time keeps the body's sliders running.

use std::sync::Arc;

use auberean_animation::graph::Eval;
use auberean_animation::sliders::Sliders;
use auberean_animation::turns::Ground;
use auberean_animation::{Animations, Pose};
use auberean_assets::animation::{Animation, AnimationKind, KeyframeAnimation};
use auberean_primitives::random::Random;
use auberean_primitives::DataId;

const NODE: u32 = 0x0300_0100;
const CLIPS: [u32; 4] = [0x0300_0101, 0x0300_0102, 0x0300_0103, 0x0300_0104];
const PLAYED: u32 = 0x11;
const ROLLED: u32 = 0x12;
const MOVING: u32 = 0x13;

struct Records;

impl Animations for Records {
    fn animation(&self, id: DataId) -> Option<Arc<Animation>> {
        let animator = if id.0 == NODE {
            AnimationKind::Random {
                slider: PLAYED,
                animations: CLIPS.iter().map(|&c| DataId(c)).collect(),
                frequency: 1.0,
                ramp_time: 0.25,
                random_slider: ROLLED,
                variation: 0.25,
            }
        } else if CLIPS.contains(&id.0) {
            AnimationKind::Keyframes(KeyframeAnimation::default())
        } else {
            return None;
        };
        Some(Arc::new(Animation { animator }))
    }
}

/// A body playing the node: its sliders, the third one moving for the whole run.
fn body() -> Sliders {
    let mut s = Sliders::new();
    s.do_slider(0, MOVING, 1000.0, 1000.0);
    s
}

/// One frame of `dt` seconds of a body playing the node with `dice`: the variant it starts
/// crossfading to this frame, if it rolled one.
fn frame(sliders: &mut Sliders, dice: &mut Random, dt: f32) -> Option<f32> {
    let mut pose = Pose::new(1);
    let mut eval = Eval {
        sliders,
        dice,
        mode: 0,
        element: 0,
        terrain: 0,
        bone_types: &[0],
        ground: Ground::default(),
    };
    auberean_animation::graph::animate(
        &Records,
        DataId(NODE),
        &mut eval,
        1.0,
        0.0,
        &[None],
        &mut pose,
    );
    let c = *sliders.interp(ROLLED);
    let rolled = (c.active && c.time == 0.0).then_some(c.end);
    sliders.update(dt);
    rolled
}

/// Two bodies playing the same node for a minute at thirty frames a second, one after the other
/// each frame, each drawing from its own `dice`: the variants each picked, in order.
fn two_bodies(mut first: Random, mut second: Random) -> [Vec<f32>; 2] {
    let (mut a, mut b) = (body(), body());
    let mut picks = [Vec::new(), Vec::new()];
    for _ in 0..1800 {
        picks[0].extend(frame(&mut a, &mut first, 1.0 / 30.0));
        picks[1].extend(frame(&mut b, &mut second, 1.0 / 30.0));
    }
    picks
}

/// Bodies draw their random choices from the client's one generator, so two bodies playing the
/// same idle do not pick their variants in step: each draw moves the numbers the next body gets.
/// (Two generators in the same state, one a body, would pick in step.)
///
/// Behaviour: animation.random.bodies-pick-their-variants-from-the-clients-one-generator
#[test]
fn bodies_pick_their_variants_from_the_clients_one_generator() {
    let [a, b] = two_bodies(Random::new(), Random::new());
    assert!(a.len() > 40, "{} picks in a minute", a.len());
    assert_eq!(a, b, "separate generators in the same state pick in step");

    let shared = Random::new();
    let [c, d] = two_bodies(shared.clone(), shared);
    assert!(
        c.len() > 40 && d.len() > 40,
        "{} and {} picks",
        c.len(),
        d.len()
    );
    let n = c.len().min(d.len());
    let differ = (0..n).filter(|&k| c[k] != d[k]).count();
    assert!(
        differ * 2 > n,
        "{differ} of {n} picks differ between the two bodies"
    );
    assert_ne!(
        c, a,
        "the first body's picks move with the second body's draws"
    );
}
