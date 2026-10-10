//! Drawing a body between its animation's keyframes, for a presentation that asks for it.
//!
//! Played as the game plays it, a body is posed at whole keyframes: each part steps from one
//! keyframe to the next at the animation's own rate, with nothing between. Drawn between them,
//! the same motion is shown at any frame rate: [`Sequence::between`] says where the player would
//! stand a moment on from where it was last advanced, and which two keyframes that moment lies
//! between, and [`blend`] poses a part that share of the way from one to the other.
//!
//! Nothing here moves the player. The frames it crosses, the hooks they fire and the root motion
//! it integrates are all [`Sequence::update`]'s, at the game's own steps, and a body drawn between
//! keyframes is advanced exactly as one drawn at them.

use dereth_primitives::num::math::{acosf, sinf};
use dereth_primitives::{Frame, Quat, Vec3};

use super::Sequence;
use crate::data::AnimFrame;

/// Two keyframes of a sequence, each a node and a frame of it, and the share of the way from the
/// first to the second the moment drawn stands at: from 0, the first, up to 1, the second.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Between {
    pub from: (usize, i32),
    pub to: (usize, i32),
    pub share: f32,
}

/// The furthest, in the animation's own units (before the body's scale), a part moves between two
/// keyframes and is still drawn moving between them: past the longest step any part takes within
/// a retail animation. A part that leaps further (put out of the way, or brought back, or a cycle
/// whose last keyframe does not meet its first) is drawn where the first keyframe has it until
/// the second is reached, as the game draws it.
pub const LEAP: f32 = 4.0;

/// The cosine of half the sharpest turn, a third of the way round, a part makes between two
/// keyframes and is still drawn turning between them; a sharper one is a leap ([`LEAP`]).
const SHARPEST_TURN: f32 = 0.5;

impl Sequence {
    /// Where the player would stand were it advanced `ahead` seconds on from where it stands now,
    /// read without advancing it: the keyframe it would pose the parts at, the keyframe it plays
    /// to next, and how far between the two it would stand.
    ///
    /// The look ahead passes from node to node as an update would, carrying what is left of the
    /// time into the next node and wrapping from the last node to the cycle's first, so the
    /// keyframe it names is the one an update of `ahead` seconds would pose. The next keyframe
    /// is the next in playing order: the next frame of the node, or the first frame of the node
    /// played after it (the cycle's first, at the end of the last). A node that does not move
    /// (no frame rate), or the last frame with nothing played after it, has no next keyframe and
    /// stands at its own. `None` with no animation, when the parts are posed at the placement
    /// frame.
    #[must_use]
    pub fn between(&self, ahead: f64) -> Option<Between> {
        let mut curr = self.curr?;
        let mut frame = self.frame_number;
        let mut dt = ahead.max(0.0);
        for _ in 0..super::MOST_NODES {
            let node = &self.nodes[curr];
            let fr = f64::from(node.framerate);
            if fr == 0.0 || dt == 0.0 {
                break;
            }
            // The step in frames is reckoned as the update reckons it, in single precision.
            #[allow(clippy::cast_possible_truncation)]
            // LINT-OK: a fraction of a second, as the update narrows it.
            let at = frame + f64::from(node.framerate * (dt as f32));
            let (low, high) = (f64::from(node.low_frame), f64::from(node.high_frame));
            // Past the node's end, the time left over carries into the next node.
            let left = if fr > 0.0 {
                (at.floor() > high).then(|| ((at - high) - 1.0).max(0.0) / fr)
            } else {
                (at.floor() < low).then(|| (at - low).min(0.0) / fr)
            };
            let Some(left) = left else {
                frame = at;
                break;
            };
            let Some(next) = self.next_node(curr) else {
                frame = if fr > 0.0 { high } else { low };
                break;
            };
            curr = next;
            frame = self.nodes[next].get_starting_frame();
            dt = left;
        }
        let node = &self.nodes[curr];
        let at = dereth_primitives::num::to_i32_f64(frame.floor());
        #[allow(clippy::cast_possible_truncation)]
        // LINT-OK: a share of one keyframe, between zero and one.
        let part = (frame - frame.floor()) as f32;
        let (step, share) = match node.framerate {
            fr if fr > 0.0 => (1, part),
            fr if fr < 0.0 => (-1, 1.0 - part),
            _ => (0, 0.0),
        };
        let inside = if step > 0 {
            at < node.high_frame
        } else {
            at > node.low_frame
        };
        let to = if step == 0 {
            None
        } else if inside {
            Some((curr, at + step))
        } else {
            self.next_node(curr).map(|n| {
                let start = self.nodes[n].get_starting_frame();
                (n, dereth_primitives::num::to_i32_f64(start.floor()))
            })
        };
        Some(match to {
            Some(to) => Between {
                from: (curr, at),
                to,
                share,
            },
            None => Between {
                from: (curr, at),
                to: (curr, at),
                share: 0.0,
            },
        })
    }

    /// The node played after node `curr`: the next one, or the cycle's first after the last.
    fn next_node(&self, curr: usize) -> Option<usize> {
        if curr + 1 < self.nodes.len() {
            Some(curr + 1)
        } else {
            self.first_cyclic
        }
    }

    /// The part frames of one keyframe, a node and a frame of it, as [`Self::between`] names it.
    #[must_use]
    pub fn keyframe(&self, (node, frame): (usize, i32)) -> Option<&AnimFrame> {
        self.nodes.get(node)?.get_part_frame(frame)
    }
}

/// A part `share` of the way from its frame `a` to its frame `b`: its origin along the straight
/// line between them, its rotation along the shorter arc at an even pace. A part that leaps
/// between the two ([`LEAP`]) stays at `a`.
#[must_use]
pub fn blend(a: &Frame, b: &Frame, share: f32) -> Frame {
    let d = Vec3::new(
        b.origin.x - a.origin.x,
        b.origin.y - a.origin.y,
        b.origin.z - a.origin.z,
    );
    let (p, mut q) = (a.rotation, b.rotation);
    let mut cos = p.w * q.w + p.x * q.x + p.y * q.y + p.z * q.z;
    if cos < 0.0 {
        q = Quat::new(-q.w, -q.x, -q.y, -q.z);
        cos = -cos;
    }
    if share <= 0.0 || d.dot(d) > LEAP * LEAP || cos < SHARPEST_TURN {
        return *a;
    }
    let t = share.min(1.0);
    let origin = Vec3::new(
        a.origin.x + d.x * t,
        a.origin.y + d.y * t,
        a.origin.z + d.z * t,
    );
    // Turned less than about eleven degrees, the straight line between them, renormalised, keeps
    // to the arc's pace within a five-hundredth of a degree, and costs no trigonometry.
    let (wa, wb) = if cos > 0.995 {
        (1.0 - t, t)
    } else {
        let angle = acosf(cos);
        let s = sinf(angle);
        (sinf((1.0 - t) * angle) / s, sinf(t * angle) / s)
    };
    let r = Quat::new(
        p.w * wa + q.w * wb,
        p.x * wa + q.x * wb,
        p.y * wa + q.y * wb,
        p.z * wa + q.z * wb,
    );
    let n = (r.w * r.w + r.x * r.x + r.y * r.y + r.z * r.z).sqrt();
    Frame::new(origin, Quat::new(r.w / n, r.x / n, r.y / n, r.z / n))
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (a presentation's own drawing between keyframes, not the game's)
    use super::*;
    use crate::data::{AnimData, AnimationData, MapAssets};
    use dereth_primitives::num::math::cosf;
    use std::sync::Arc;

    /// An animation of `frames` keyframes of one part, keyframe `i` at `(i, 0, 0)` turned `10 i`
    /// degrees about the vertical.
    fn ramp(id: u32, frames: u32, base: f32) -> (dereth_primitives::DataId, Arc<AnimationData>) {
        let a = AnimationData {
            num_frames: frames,
            num_parts: 1,
            pos_frames: None,
            part_frames: (0..frames)
                .map(|i| {
                    #[allow(clippy::cast_precision_loss)]
                    let x = base + i as f32;
                    let half = (x * 10.0).to_radians() / 2.0;
                    AnimFrame {
                        frames: vec![Frame::new(
                            Vec3::new(x, 0.0, 0.0),
                            Quat::new(cosf(half), 0.0, 0.0, sinf(half)),
                        )],
                        hooks: Vec::new(),
                    }
                })
                .collect(),
            has_hooks: false,
        };
        (dereth_primitives::DataId(0x0300_0000 | id), Arc::new(a))
    }

    fn sequence(nodes: &[(u32, u32, f32, f32)]) -> Sequence {
        let mut assets = MapAssets::default();
        let mut s = Sequence::new();
        for &(id, frames, rate, base) in nodes {
            let (aid, a) = ramp(id, frames, base);
            assets.animations.insert(aid.0, a);
            s.append_animation(
                AnimData {
                    anim_id: aid,
                    low_frame: 0,
                    high_frame: -1,
                    framerate: rate,
                },
                &assets,
            );
        }
        s
    }

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn a_part_is_drawn_exactly_at_each_keyframe_and_evenly_between_them() {
        let a = Frame::new(Vec3::new(0.0, 0.0, 0.0), Quat::IDENTITY);
        let half = 45_f32.to_radians();
        let b = Frame::new(
            Vec3::new(0.2, -0.4, 0.6),
            Quat::new(cosf(half), 0.0, 0.0, sinf(half)),
        );
        assert_eq!(blend(&a, &b, 0.0), a, "at the first keyframe, exactly");
        let end = blend(&a, &b, 1.0);
        assert!(
            near(end.origin.x, 0.2) && near(end.origin.y, -0.4) && near(end.origin.z, 0.6),
            "{end:?}"
        );
        assert!(near(end.rotation.w, b.rotation.w) && near(end.rotation.z, b.rotation.z));
        // A quarter of the way: a quarter of the line and a quarter of the 90 degree turn.
        let q = blend(&a, &b, 0.25);
        assert!(
            near(q.origin.x, 0.05) && near(q.origin.y, -0.1) && near(q.origin.z, 0.15),
            "{q:?}"
        );
        let quarter = (22.5_f32.to_radians()) / 2.0;
        assert!(
            near(q.rotation.w, cosf(quarter)) && near(q.rotation.z, sinf(quarter)),
            "{q:?}"
        );
        // The same rotation written the other way round turns the short way, not the long.
        let flipped = Frame::new(b.origin, Quat::new(-b.rotation.w, 0.0, 0.0, -b.rotation.z));
        let q = blend(&a, &flipped, 0.25);
        assert!(near(q.rotation.w.abs(), cosf(quarter)), "{q:?}");
    }

    #[test]
    fn a_part_that_leaps_between_two_keyframes_stays_at_the_first_until_the_second() {
        let a = Frame::new(Vec3::new(0.0, 0.0, 1.0), Quat::IDENTITY);
        let far = Frame::new(Vec3::new(0.0, 0.0, 1.0 + LEAP * 1.5), Quat::IDENTITY);
        assert_eq!(blend(&a, &far, 0.5), a, "put out of the way: no slide");
        let round = Frame::new(a.origin, Quat::new(0.0, 0.0, 0.0, 1.0));
        assert_eq!(blend(&a, &round, 0.5), a, "turned half round: no spin");
    }

    #[test]
    fn standing_where_it_was_last_advanced_it_is_drawn_between_the_keyframe_it_poses_and_the_next()
    {
        let mut s = sequence(&[(1, 10, 30.0, 0.0)]);
        let mut hooks = Vec::new();
        s.update(0.1 + 0.25 / 30.0, Some(&mut Frame::default()), &mut hooks);
        let b = s.between(0.0).expect("an animation");
        assert_eq!(b.from, (0, s.curr_frame_number()), "the keyframe it poses");
        assert_eq!(b.to, (0, s.curr_frame_number() + 1));
        assert!(near(b.share, 0.25), "{b:?}");
    }

    #[test]
    fn looked_ahead_it_names_the_keyframe_an_update_of_that_long_would_pose() {
        // A link of six frames into a cycle of eight, at different rates, one backwards.
        for (rate, cycle_rate) in [(30.0, 30.0), (24.0, -15.0), (60.0, 10.0), (30.0, 19.5)] {
            let start = sequence(&[(1, 6, rate, 0.0), (2, 8, cycle_rate, 100.0)]);
            for k in 0..200 {
                let ahead = f64::from(k) * 0.004;
                let mut s = start.clone();
                let mut hooks = Vec::new();
                s.update(ahead, Some(&mut Frame::default()), &mut hooks);
                let b = start.between(ahead).expect("an animation");
                // The update lets go of the nodes it has played through, so the node is named by
                // its animation.
                let posed = s.nodes()[s.curr().expect("playing")].anim_id;
                assert_eq!(
                    (start.nodes()[b.from.0].anim_id, b.from.1),
                    (posed, s.curr_frame_number()),
                    "rates {rate} and {cycle_rate}, {ahead} s on"
                );
            }
        }
    }

    #[test]
    fn at_a_link_s_last_keyframe_it_plays_on_into_the_cycle_and_at_the_cycle_s_last_back_to_its_first(
    ) {
        let s = sequence(&[(1, 4, 30.0, 0.0), (2, 5, 30.0, 100.0)]);
        // Three and a half frames in: at the link's last keyframe, half way to the cycle's first.
        let b = s.between(3.5 / 30.0).expect("an animation");
        assert_eq!((b.from, b.to), ((0, 3), (1, 0)), "{b:?}");
        assert!(near(b.share, 0.5));
        // Four frames of the link, then four and a half of the cycle: its last keyframe, half way
        // back round to its first.
        let b = s.between(8.5 / 30.0).expect("an animation");
        assert_eq!((b.from, b.to), ((1, 4), (1, 0)), "{b:?}");
        assert!(near(b.share, 0.5));
        let (a, z) = (
            s.keyframe(b.from).expect("a keyframe").frames[0],
            s.keyframe(b.to).expect("a keyframe").frames[0],
        );
        assert!(near(a.origin.x, 104.0) && near(z.origin.x, 100.0));
    }

    #[test]
    fn a_sequence_cut_to_another_animation_is_drawn_from_that_animation_alone() {
        let mut assets = MapAssets::default();
        let (a, b) = (ramp(1, 10, 0.0), ramp(2, 10, 100.0));
        assets.animations.insert(a.0 .0, a.1);
        assets.animations.insert(b.0 .0, b.1);
        let mut s = Sequence::new();
        let node = |id| AnimData {
            anim_id: id,
            low_frame: 0,
            high_frame: -1,
            framerate: 30.0,
        };
        s.append_animation(node(a.0), &assets);
        let mut hooks = Vec::new();
        s.update(4.5 / 30.0, Some(&mut Frame::default()), &mut hooks);
        // Cut: the old animation let go of, the new one begun with no link between.
        s.clear_animations();
        s.append_animation(node(b.0), &assets);
        let at = s.between(0.5 / 30.0).expect("an animation");
        assert_eq!((at.from, at.to), ((0, 0), (0, 1)), "{at:?}");
        let (x, y) = (
            s.keyframe(at.from).expect("a keyframe").frames[0].origin.x,
            s.keyframe(at.to).expect("a keyframe").frames[0].origin.x,
        );
        assert!(
            near(x, 100.0) && near(y, 101.0),
            "the new animation's own keyframes, nothing of the old"
        );
    }

    #[test]
    fn played_backwards_it_is_drawn_toward_the_keyframe_before() {
        let s = sequence(&[(1, 10, -30.0, 0.0)]);
        // Backwards from the end: a quarter of a frame in, at the last keyframe going to the one
        // before.
        let b = s.between(0.25 / 30.0).expect("an animation");
        assert_eq!((b.from, b.to), ((0, 9), (0, 8)), "{b:?}");
        assert!((b.share - 0.25).abs() < 1e-3, "{b:?}");
    }

    #[test]
    fn a_node_that_does_not_move_or_has_nothing_after_it_stands_at_its_own_keyframe() {
        let s = sequence(&[(1, 10, 0.0, 0.0)]);
        let b = s.between(0.5).expect("an animation");
        assert_eq!((b.from, b.to, b.share), ((0, 0), (0, 0), 0.0));
        assert_eq!(Sequence::new().between(0.1), None, "no animation");
    }
}
