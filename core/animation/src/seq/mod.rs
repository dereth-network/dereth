//! [`Sequence`], the animation player: the node list, `first_cyclic`, and `apricot`.
//!
//! The client's nodes form a doubly linked list. Every operation it performs on that list
//! is a **contiguous range** operation — append at the tail, delete the nodes immediately before
//! `first_cyclic`, delete from `first_cyclic` to the tail, delete from the head up to `curr_anim` —
//! so a `Vec` with two indices reproduces it exactly and without a cursor type. The indices are the
//! part that matters:
//!
//! * `first_cyclic` names the **last node appended**, which is why the cycle animations a
//!   transition queues last are the ones that loop, and why its sequence-state report uses
//!   `num_anims − 1`. There is no explicit loop flag.
//! * `curr` is the node being played; `frame_number` is a **double** and is fractional.

pub mod update;

use std::sync::Arc;

use dereth_primitives::{DataId, Vec3};

use crate::data::{AnimAssets, AnimData, AnimFrame, AnimationData};

/// `AnimSequenceNode` — one animation plus the frame range and rate it plays at.
#[derive(Debug, Clone)]
pub struct AnimSequenceNode {
    pub anim_id: DataId,
    pub anim: Arc<AnimationData>,
    pub framerate: f32,
    pub low_frame: i32,
    pub high_frame: i32,
}

impl AnimSequenceNode {
    /// Construct a sequence node from one animation, then set its animation id.
    ///
    /// **`high_frame == -1` is resolved here, at construction, not at play time**,
    /// along with the three clamps that follow it. Returns `None` when the animation is not
    /// loaded, which is `append_animation`'s "delete the node and drop it silently" case.
    #[must_use]
    pub fn new(a: AnimData, assets: &dyn AnimAssets) -> Option<Self> {
        if a.anim_id.0 == 0 {
            return None;
        }
        let anim = assets.animation(a.anim_id)?;
        let num_frames = i32::try_from(anim.num_frames).unwrap_or(i32::MAX);
        let mut low = a.low_frame;
        let mut high = a.high_frame;
        if high < 0 {
            high = num_frames - 1;
        }
        if low >= num_frames {
            low = num_frames - 1;
        }
        if high >= num_frames {
            high = num_frames - 1;
        }
        if high < low {
            high = low;
        }
        Some(Self {
            anim_id: a.anim_id,
            anim,
            framerate: a.framerate,
            low_frame: low,
            high_frame: high,
        })
    }

    /// Multiply the node's frame rate.
    ///
    /// The swap happens **whenever the multiplier is negative**, so applying `−1` twice does not
    /// simply restore the original: the bounds swap back but only because the sign of `framerate`
    /// also flipped twice.
    pub fn multiply_framerate(&mut self, f: f32) {
        if f < 0.0 {
            std::mem::swap(&mut self.low_frame, &mut self.high_frame);
        }
        self.framerate *= f;
    }

    /// The node's starting frame.
    ///
    /// The `−0.0002` keeps `floor(frame_number)` inside the range at the very end of a forward
    /// animation. It is computed in `float` and widened, as the client does.
    #[must_use]
    pub fn get_starting_frame(&self) -> f64 {
        if self.framerate >= 0.0 {
            f64::from(self.low_frame)
        } else {
            f64::from((self.high_frame as f32) + 1.0 - 0.0002)
        }
    }

    /// The node's ending frame.
    #[must_use]
    pub fn get_ending_frame(&self) -> f64 {
        if self.framerate >= 0.0 {
            f64::from((self.high_frame as f32) + 1.0 - 0.0002)
        } else {
            f64::from(self.low_frame)
        }
    }

    /// How long the player takes to play the node through, in seconds, from its starting frame
    /// until it moves on: forwards once the frame number passes `high_frame + 1`, backwards once
    /// it drops below `low_frame`. A zero frame rate never finishes (`+inf`).
    #[must_use]
    pub fn play_time(&self) -> f64 {
        let fr = f64::from(self.framerate.abs());
        if self.framerate >= 0.0 {
            (f64::from(self.high_frame) + 1.0 - self.get_starting_frame()) / fr
        } else {
            (self.get_starting_frame() - f64::from(self.low_frame)) / fr
        }
    }

    /// The frames whose hooks fire while the node plays through, in firing order: each frame's
    /// index, the time it fires (seconds from the node's start) and the direction it fires in.
    ///
    /// A frame's hooks fire as the player *leaves* it, so the frame the node ends on
    /// (`high_frame` forwards, `low_frame` backwards) fires none: the player stops on it and moves
    /// to the next node. A zero frame rate fires nothing.
    #[must_use]
    pub fn fired_frames(&self) -> Vec<(i32, f64, i32)> {
        let fr = f64::from(self.framerate.abs());
        if fr == 0.0 {
            return Vec::new();
        }
        let start = self.get_starting_frame();
        if self.framerate >= 0.0 {
            (self.low_frame..self.high_frame)
                .map(|i| (i, (f64::from(i) + 1.0 - start) / fr, 1))
                .collect()
        } else {
            ((self.low_frame + 1)..=self.high_frame)
                .rev()
                .map(|i| (i, (start - f64::from(i)) / fr, -1))
                .collect()
        }
    }

    /// The node's position frame.
    #[must_use]
    pub fn get_pos_frame(&self, i: i32) -> Option<&dereth_primitives::Frame> {
        self.anim.pos_frame(i)
    }

    /// The node's per-part frame.
    #[must_use]
    pub fn get_part_frame(&self, i: i32) -> Option<&AnimFrame> {
        self.anim.part_frame(i)
    }
}

/// The animation player.
#[derive(Debug, Default, Clone)]
pub struct Sequence {
    pub(crate) nodes: Vec<AnimSequenceNode>,
    pub(crate) first_cyclic: Option<usize>,
    pub(crate) curr: Option<usize>,
    pub(crate) frame_number: f64,
    /// The constant object-local velocity contributed by the motion table.
    pub velocity: Vec3,
    /// The constant object-local angular velocity.
    pub omega: Vec3,
    /// The fallback used by `get_curr_animframe` when there is no
    /// animation.
    pub(crate) placement_frame: Option<AnimFrame>,
    pub(crate) placement_frame_id: u32,
    /// Whether the sequence has a hook target. When false, `execute_hooks` drops every hook **and** the
    /// link-animation `AnimDoneHook` is not raised either.
    pub hooks_enabled: bool,
    /// The client's trivial flag is written by the part array and read by nothing in
    /// animation playback. Kept as a public flag so a renderer can use it as a fast path.
    pub is_trivial: bool,
}

impl Sequence {
    /// Constructor : everything empty, `frame_number = 0.0`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            hooks_enabled: true,
            ..Self::default()
        }
    }

    /// Append an animation to the sequence.
    ///
    /// A missing dat animation is **silently dropped** — the node is constructed, found to have no
    /// animation asset, and deleted. `first_cyclic` then still names whatever it named before.
    pub fn append_animation(&mut self, a: AnimData, assets: &dyn AnimAssets) {
        let Some(node) = AnimSequenceNode::new(a, assets) else {
            return;
        };
        self.nodes.push(node);
        self.first_cyclic = Some(self.nodes.len() - 1);
        if self.curr.is_none() {
            self.curr = Some(0);
            self.frame_number = self.nodes[0].get_starting_frame();
        }
    }

    /// How long the first `n` nodes take to play through one after another, from the first's
    /// starting frame: the time until the player has finished `n` animations (leftover time
    /// carries from one node into the next, so the nodes' own times add up exactly).
    #[must_use]
    pub fn play_time(&self, n: usize) -> f64 {
        self.nodes
            .iter()
            .take(n)
            .map(AnimSequenceNode::play_time)
            .sum()
    }

    /// Clear the sequence's animations.
    pub fn clear_animations(&mut self) {
        self.nodes.clear();
        self.first_cyclic = None;
        self.curr = None;
        self.frame_number = 0.0;
    }

    /// Clear the sequence's physics.
    pub fn clear_physics(&mut self) {
        self.velocity = Vec3::ZERO;
        self.omega = Vec3::ZERO;
    }

    /// Clear the sequence.
    pub fn clear(&mut self) {
        self.clear_animations();
        self.clear_physics();
        self.placement_frame = None;
        self.placement_frame_id = 0;
    }

    /// Set the sequence's velocity.
    pub fn set_velocity(&mut self, v: Vec3) {
        self.velocity = v;
    }

    /// Set the sequence's omega.
    pub fn set_omega(&mut self, w: Vec3) {
        self.omega = w;
    }

    /// Combine physics into the sequence.
    pub fn combine_physics(&mut self, v: Vec3, w: Vec3) {
        use crate::frame::V3;
        self.velocity = self.velocity.add(v);
        self.omega = self.omega.add(w);
    }

    /// Subtract physics from the sequence.
    pub fn subtract_physics(&mut self, v: Vec3, w: Vec3) {
        use crate::frame::V3;
        self.velocity = self.velocity.sub(v);
        self.omega = self.omega.sub(w);
    }

    /// Every node from `first_cyclic` to
    /// the tail.
    pub fn multiply_cyclic_animation_fr(&mut self, f: f32) {
        let Some(start) = self.first_cyclic else {
            return;
        };
        for n in &mut self.nodes[start..] {
            n.multiply_framerate(f);
        }
    }

    /// Delete up to `n` nodes **immediately
    /// before `first_cyclic`**, newest first.
    pub fn remove_link_animations(&mut self, n: u32) {
        for _ in 0..n {
            if !self.remove_one_link_animation() {
                return;
            }
        }
    }

    /// The same loop without a count.
    pub fn remove_all_link_animations(&mut self) {
        while self.remove_one_link_animation() {}
    }

    fn remove_one_link_animation(&mut self) -> bool {
        let Some(fc) = self.first_cyclic else {
            return false;
        };
        if fc == 0 {
            return false; // `first_cyclic->GetPrev()` is NULL: nothing to remove.
        }
        let p = fc - 1;
        if self.curr == Some(p) {
            // We were playing the node being removed: jump to the cyclic node.
            self.curr = Some(fc);
            self.frame_number = self.nodes[fc].get_starting_frame();
        }
        self.nodes.remove(p);
        self.first_cyclic = Some(fc - 1);
        self.curr = self.curr.map(|c| if c > p { c - 1 } else { c });
        true
    }

    /// Delete from `first_cyclic` to the tail.
    ///
    /// The client reads `n->GetPrev()` when it finds `curr_anim` inside the range being deleted.
    /// For the first node of the range that is the node before `first_cyclic`, which is the
    /// intended answer; for a later node the previous node has already been freed and the client
    /// reads a dangling pointer. Only the intended case is reproduced, because the other has no
    /// defined behaviour to reproduce.
    pub fn remove_cyclic_anims(&mut self) {
        let Some(fc) = self.first_cyclic else {
            self.first_cyclic = self.nodes.len().checked_sub(1);
            return;
        };
        if let Some(c) = self.curr {
            if c >= fc {
                self.curr = fc.checked_sub(1);
                self.frame_number = match self.curr {
                    Some(i) => self.nodes[i].get_ending_frame(),
                    None => 0.0,
                };
            }
        }
        self.nodes.truncate(fc);
        self.first_cyclic = self.nodes.len().checked_sub(1);
    }

    /// The garbage collector, run once per `update`. It deletes
    /// from the **head** until it reaches `curr_anim`, stopping early if it reaches `first_cyclic`.
    pub fn apricot(&mut self) {
        let Some(curr) = self.curr else {
            return;
        };
        let mut k = 0usize;
        while k < curr {
            if self.first_cyclic == Some(k) {
                break;
            }
            k += 1;
        }
        if k == 0 {
            return;
        }
        self.nodes.drain(0..k);
        self.curr = Some(curr - k);
        self.first_cyclic = self.first_cyclic.map(|f| f.saturating_sub(k));
    }

    /// Whether the sequence holds any animation.
    #[must_use]
    pub fn has_anims(&self) -> bool {
        !self.nodes.is_empty()
    }

    /// `(int)floor(frame_number)`.
    #[must_use]
    pub fn curr_frame_number(&self) -> i32 {
        dereth_primitives::num::to_i32_f64(self.frame_number.floor())
    }

    /// The fractional frame, for a test or a trace.
    #[must_use]
    pub const fn frame_number(&self) -> f64 {
        self.frame_number
    }

    /// The node the player wraps back to. `None` for an empty list.
    #[must_use]
    pub const fn first_cyclic(&self) -> Option<usize> {
        self.first_cyclic
    }

    /// The node being played.
    #[must_use]
    pub const fn curr(&self) -> Option<usize> {
        self.curr
    }

    #[must_use]
    pub fn nodes(&self) -> &[AnimSequenceNode] {
        &self.nodes
    }

    /// Set the sequence's placement frame.
    pub fn set_placement_frame(&mut self, f: Option<AnimFrame>, id: u32) {
        self.placement_frame = f;
        self.placement_frame_id = id;
    }

    /// `part_frames[floor(frame_number)]`, or the
    /// placement frame when there is no animation.
    ///
    /// **This is the whole of part placement.** There is no interpolation between animation frames
    /// anywhere in the client.
    #[must_use]
    pub fn get_curr_animframe(&self) -> Option<&AnimFrame> {
        match self.curr {
            Some(i) => self.nodes[i].get_part_frame(self.curr_frame_number()),
            None => self.placement_frame.as_ref(),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::data::MapAssets;
    use dereth_primitives::Frame;

    pub(crate) fn anim(id: u32, frames: u32) -> (DataId, Arc<AnimationData>) {
        let a = AnimationData {
            num_frames: frames,
            num_parts: 1,
            pos_frames: None,
            part_frames: (0..frames)
                .map(|_| AnimFrame {
                    frames: vec![Frame::default()],
                    hooks: Vec::new(),
                })
                .collect(),
            has_hooks: false,
        };
        (DataId(0x0300_0000 | id), Arc::new(a))
    }

    pub(crate) fn assets_with(anims: &[(DataId, Arc<AnimationData>)]) -> MapAssets {
        let mut m = MapAssets::default();
        for (id, a) in anims {
            m.animations.insert(id.0, Arc::clone(a));
        }
        m
    }

    /// ORACLE: the recovered animation-playback behavior section 1 —
    /// `set_animation_id` resolves `high_frame == -1` at construction and clamps.
    #[test]
    fn high_frame_minus_one_becomes_the_last_frame_at_construction() {
        let (id, a) = anim(1, 10);
        let assets = assets_with(&[(id, a)]);
        let n = AnimSequenceNode::new(
            AnimData {
                anim_id: id,
                low_frame: 0,
                high_frame: -1,
                framerate: 30.0,
            },
            &assets,
        )
        .expect("node");
        assert_eq!(n.high_frame, 9);
        // Clamped to the last frame, and `high < low` is repaired.
        let n = AnimSequenceNode::new(
            AnimData {
                anim_id: id,
                low_frame: 50,
                high_frame: 60,
                framerate: 30.0,
            },
            &assets,
        )
        .expect("node");
        assert_eq!((n.low_frame, n.high_frame), (9, 9));
    }

    /// `get_starting_frame`/`get_ending_frame` and the `−0.0002`.
    #[test]
    fn the_end_of_a_forward_animation_is_high_plus_one_minus_two_ten_thousandths() {
        let (id, a) = anim(1, 10);
        let assets = assets_with(&[(id, a)]);
        let n = AnimSequenceNode::new(
            AnimData {
                anim_id: id,
                low_frame: 0,
                high_frame: 9,
                framerate: 30.0,
            },
            &assets,
        )
        .expect("node");
        assert_eq!(n.get_starting_frame(), 0.0);
        assert_eq!(n.get_ending_frame(), f64::from(10.0_f32 - 0.0002));
        assert_eq!(
            dereth_primitives::num::to_i32_f64(n.get_ending_frame().floor()),
            9
        );
    }

    /// A negative framerate swaps the bounds at `multiply_framerate` time, and
    /// `get_starting_frame`/`get_ending_frame` then read the swapped values.
    #[test]
    fn a_negative_multiplier_swaps_the_bounds() {
        let (id, a) = anim(1, 10);
        let assets = assets_with(&[(id, a)]);
        let mut n = AnimSequenceNode::new(
            AnimData {
                anim_id: id,
                low_frame: 2,
                high_frame: 7,
                framerate: 30.0,
            },
            &assets,
        )
        .expect("node");
        n.multiply_framerate(-1.0);
        assert_eq!((n.low_frame, n.high_frame), (7, 2));
        assert_eq!(n.framerate, -30.0);
        assert_eq!(n.get_starting_frame(), f64::from(3.0_f32 - 0.0002));
        assert_eq!(n.get_ending_frame(), 7.0);
        // Applying it twice restores the original ordering *and* the sign.
        n.multiply_framerate(-1.0);
        assert_eq!((n.low_frame, n.high_frame), (2, 7));
        assert_eq!(n.framerate, 30.0);
    }

    /// `append_animation` names the last node appended as `first_cyclic`, and a missing animation
    /// is dropped rather than queued.
    #[test]
    fn first_cyclic_is_the_last_node_appended_and_missing_anims_are_dropped() {
        let (id1, a1) = anim(1, 4);
        let (id2, a2) = anim(2, 4);
        let assets = assets_with(&[(id1, a1), (id2, a2)]);
        let mut s = Sequence::new();
        s.append_animation(
            AnimData {
                anim_id: id1,
                ..AnimData::default()
            },
            &assets,
        );
        assert_eq!(s.first_cyclic(), Some(0));
        assert_eq!(s.curr(), Some(0));
        s.append_animation(
            AnimData {
                anim_id: id2,
                ..AnimData::default()
            },
            &assets,
        );
        assert_eq!(s.first_cyclic(), Some(1));
        assert_eq!(s.curr(), Some(0), "curr stays on the node being played");
        s.append_animation(
            AnimData {
                anim_id: DataId(0x0300_9999),
                ..AnimData::default()
            },
            &assets,
        );
        assert_eq!(
            s.nodes().len(),
            2,
            "a missing animation is silently dropped"
        );
        assert_eq!(s.first_cyclic(), Some(1));
    }

    /// `remove_link_animations` deletes the nodes immediately before `first_cyclic`, newest first,
    /// and moves `curr` onto the cyclic node when it was playing one of them.
    #[test]
    fn remove_link_animations_deletes_backwards_from_first_cyclic() {
        let (id1, a1) = anim(1, 4);
        let (id2, a2) = anim(2, 4);
        let (id3, a3) = anim(3, 4);
        let assets = assets_with(&[(id1, a1), (id2, a2), (id3, a3)]);
        let mut s = Sequence::new();
        for id in [id1, id2, id3] {
            s.append_animation(
                AnimData {
                    anim_id: id,
                    ..AnimData::default()
                },
                &assets,
            );
        }
        assert_eq!(s.first_cyclic(), Some(2));
        s.remove_link_animations(1);
        assert_eq!(s.nodes().len(), 2);
        assert_eq!(s.nodes()[1].anim_id, id3);
        assert_eq!(s.first_cyclic(), Some(1));
        assert_eq!(s.curr(), Some(0));
        // Removing the node we are playing moves `curr` to the cyclic node.
        s.remove_link_animations(1);
        assert_eq!(s.nodes().len(), 1);
        assert_eq!(s.curr(), Some(0));
        assert_eq!(s.first_cyclic(), Some(0));
        // Nothing left to remove: the loop returns rather than running off the front.
        s.remove_all_link_animations();
        assert_eq!(s.nodes().len(), 1);
    }

    /// `remove_cyclic_anims` deletes from `first_cyclic` to the tail and re-points `first_cyclic`
    /// at the new tail.
    #[test]
    fn remove_cyclic_anims_deletes_the_tail_block() {
        let (id1, a1) = anim(1, 4);
        let (id2, a2) = anim(2, 4);
        let assets = assets_with(&[(id1, a1), (id2, a2)]);
        let mut s = Sequence::new();
        s.append_animation(
            AnimData {
                anim_id: id1,
                ..AnimData::default()
            },
            &assets,
        );
        s.append_animation(
            AnimData {
                anim_id: id2,
                ..AnimData::default()
            },
            &assets,
        );
        s.remove_cyclic_anims();
        assert_eq!(s.nodes().len(), 1);
        assert_eq!(s.first_cyclic(), Some(0), "may now be a link animation");
        s.remove_cyclic_anims();
        assert_eq!(s.nodes().len(), 0);
        assert_eq!(s.first_cyclic(), None);
        assert_eq!(s.curr(), None);
    }

    /// `apricot` frees consumed nodes from the head but never passes `first_cyclic`.
    #[test]
    fn apricot_frees_consumed_nodes_and_stops_at_first_cyclic() {
        let (id1, a1) = anim(1, 4);
        let (id2, a2) = anim(2, 4);
        let (id3, a3) = anim(3, 4);
        let assets = assets_with(&[(id1, a1), (id2, a2), (id3, a3)]);
        let mut s = Sequence::new();
        for id in [id1, id2, id3] {
            s.append_animation(
                AnimData {
                    anim_id: id,
                    ..AnimData::default()
                },
                &assets,
            );
        }
        s.curr = Some(2);
        s.apricot();
        assert_eq!(s.nodes().len(), 1, "the two consumed link nodes are freed");
        assert_eq!(s.curr(), Some(0));
        assert_eq!(s.first_cyclic(), Some(0));

        // With `first_cyclic` in front of `curr`, apricot stops there.
        let mut s = Sequence::new();
        for id in [id1, id2, id3] {
            s.append_animation(
                AnimData {
                    anim_id: id,
                    ..AnimData::default()
                },
                &assets,
            );
        }
        s.first_cyclic = Some(1);
        s.curr = Some(2);
        s.apricot();
        assert_eq!(s.nodes().len(), 2);
        assert_eq!(s.first_cyclic(), Some(0));
        assert_eq!(s.curr(), Some(1));
    }
}
