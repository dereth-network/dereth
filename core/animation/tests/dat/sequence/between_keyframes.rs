//! Drawn between keyframes, every step a part takes from one keyframe to the next within the
//! retail animations is drawn moving, save a handful that turn a part more than a third of the
//! way round in one keyframe; no step is taken for a leap by its length alone.
//! Fixture: the shipped retail DAT records.

use super::common;

use dereth_animation::data::AnimAssets;
use dereth_animation::seq::between::{blend, LEAP};
use dereth_dat::DbType;
use dereth_primitives::Frame;

/// Every step a part takes between two consecutive keyframes of every animation in the portal
/// dat: its frame at each end.
fn steps(assets: &common::DatAssets) -> Vec<(Frame, Frame)> {
    let mut out = Vec::new();
    for id in assets.ids_of(DbType::Anim) {
        let Some(a) = assets.animation(id) else {
            continue;
        };
        for pair in a.part_frames.windows(2) {
            out.extend(
                pair[0]
                    .frames
                    .iter()
                    .copied()
                    .zip(pair[1].frames.iter().copied()),
            );
        }
    }
    out
}

#[test]
fn every_step_a_part_takes_between_retail_keyframes_is_drawn_moving_save_a_few_sharp_turns() {
    let assets = common::open();
    let all = steps(&assets);
    assert!(
        all.len() > 1_000_000,
        "the animations were read: {}",
        all.len()
    );
    let longest = all
        .iter()
        .map(|(a, b)| {
            let d = (
                b.origin.x - a.origin.x,
                b.origin.y - a.origin.y,
                b.origin.z - a.origin.z,
            );
            (d.0 * d.0 + d.1 * d.1 + d.2 * d.2).sqrt()
        })
        .fold(0.0_f32, f32::max);
    assert!(
        longest < LEAP,
        "no limb's step is taken for a leap: the longest is {longest}"
    );
    // Half way between, a part that is drawn moving is somewhere other than its first keyframe.
    let held: Vec<&(Frame, Frame)> = all
        .iter()
        .filter(|(a, b)| a != b && blend(a, b, 0.5) == *a)
        .collect();
    assert!(
        !held.is_empty() && held.len() * 10_000 < all.len(),
        "a few sharp turns, of {} steps: {}",
        all.len(),
        held.len()
    );
}
