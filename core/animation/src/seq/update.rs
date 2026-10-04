//! The frame-crossing update loop: the sequence's own update, its advance-to-next-animation step
//! and its physics apply.
//!
//! **Frames snap, and motion is integrated per frame crossed.** The player crosses whole
//! animation frames one at a time; each crossing fires that frame's hooks, composes one
//! `pos_frames` entry and integrates
//! **one animation frame's worth** of the motion table's constant velocity — `quantum = 1/framerate`
//! seconds, not `dt`. Only when there is no animation at all is `apply_physics` called with `dt`.
//!
//! Consequences a "sample the animation at time t" rewrite silently loses:
//!
//! * an animation played at half speed moves the object half as far, for free, because it crosses
//!   half as many frames;
//! * root motion composes with forwards and backwards, one
//!   whole frame at a time — a fractional composition is a different path;
//! * `leftover`, the unconsumed part of `dt` converted back to seconds by dividing by the
//!   framerate, carries into the next node, so a chain of one-frame animations plays in one step.
//!
//! `frame_number` is a **double** while `delta` is computed in **float**
//! (`framerate * (float)dt`). Both are reproduced.

use dereth_primitives::{Frame, Vec3};

use crate::frame::{combine, rotate, subtract1, V3};
use crate::hooks::AnimHook;
use crate::hooks::HookKind;

use super::Sequence;

/// The 0.0002 epsilon this file tests against, spelled once.
const EPS: f64 = 0.000_199_999_994_947_575_03;

/// The global that `update_internal` pushes when a link animation
/// finishes.
///
/// The client constructs this type-4 hook once, statically. Its runtime hook unpacking
/// discards data-driven type-4 hooks, so only this hook can queue `AnimationDone`;
/// discarding the serialized form does not lose completion notifications.
pub const ANIM_DONE_HOOK: AnimHook = AnimHook {
    direction: 0,
    kind: HookKind::AnimationDone,
};

impl Sequence {
    /// Advance the sequence.
    ///
    /// `frame == None` is the **display-only** path, taken by
    /// the preview): hooks fire and frames advance, but `apply_physics` and
    /// the `pos_frames` composition are skipped entirely.
    pub fn update(&mut self, dt: f64, frame: Option<&mut Frame>, out: &mut Vec<AnimHook>) {
        if self.nodes.is_empty() {
            if let Some(f) = frame {
                // No animation at all: the constant velocity is integrated over the whole step.
                Self::apply_physics_to(f, self.velocity, self.omega, dt, dt);
            }
            return;
        }
        self.update_internal(dt, frame, out);
        self.apricot();
    }

    /// Apply the sequence's physics.
    ///
    /// The magnitude comes from `quantum`, the sign from `dt`. `quantum` is converted to `float`
    /// before the absolute value, which is what makes a 1/framerate quantum reproduce the client's
    /// rounding.
    fn apply_physics_to(f: &mut Frame, velocity: Vec3, omega: Vec3, quantum: f64, dt: f64) {
        #[allow(clippy::cast_possible_truncation)]
        let mut q = (quantum as f32).abs();
        if dt < 0.0 {
            q = -q;
        }
        f.origin = f.origin.add(velocity.mul(q));
        rotate(f, omega.mul(q));
    }

    /// Execute the frame's hooks.
    ///
    /// Hooks are **queued**, not executed: runs them at the
    /// end of the physics step. A hook whose frame is crossed twice in one step is queued twice.
    fn execute_hooks(&self, i: i32, direction: i32, out: &mut Vec<AnimHook>) {
        if !self.hooks_enabled {
            return;
        }
        let Some(curr) = self.curr else {
            return;
        };
        let Some(af) = self.nodes[curr].get_part_frame(i) else {
            return;
        };
        for h in &af.hooks {
            if h.fires(direction) {
                out.push(*h);
            }
        }
    }

    /// The sequence update's body.
    #[allow(clippy::too_many_lines)]
    fn update_internal(&mut self, dt: f64, mut frame: Option<&mut Frame>, out: &mut Vec<AnimHook>) {
        let mut dt = dt;
        loop {
            let Some(curr) = self.curr else {
                return;
            };
            let fr = self.nodes[curr].framerate;
            let frd = f64::from(fr);
            #[allow(clippy::cast_possible_truncation)]
            let delta = f64::from(fr * (dt as f32));
            let mut done_early = false;
            let mut i = dereth_primitives::num::to_i32_f64(self.frame_number.floor());
            self.frame_number += delta;
            let mut leftover = 0.0_f64;

            if delta == 0.0 {
                if let Some(f) = frame.as_deref_mut() {
                    if dt.abs() > EPS {
                        Self::apply_physics_to(f, self.velocity, self.omega, dt, dt);
                    }
                }
                return;
            }

            if delta < 0.0 {
                // ---- playing backwards ----
                let low = self.nodes[curr].low_frame;
                if self.frame_number.floor() < f64::from(low) {
                    leftover = self.frame_number - f64::from(low);
                    if leftover > 0.0 {
                        leftover = 0.0;
                    }
                    leftover = if frd.abs() <= EPS {
                        0.0
                    } else {
                        leftover / frd
                    };
                    self.frame_number = f64::from(low);
                    done_early = true;
                }
                while self.frame_number.floor() < f64::from(i) {
                    if let Some(f) = frame.as_deref_mut() {
                        if self.nodes[curr].anim.pos_frames.is_some() {
                            if let Some(pf) = self.nodes[curr].get_pos_frame(i) {
                                *f = subtract1(f, pf);
                            }
                        }
                        if frd.abs() > EPS {
                            Self::apply_physics_to(f, self.velocity, self.omega, 1.0 / frd, dt);
                        }
                    }
                    self.execute_hooks(i, -1, out);
                    i -= 1;
                }
            } else {
                // ---- playing forwards ----
                let high = self.nodes[curr].high_frame;
                if self.frame_number.floor() > f64::from(high) {
                    leftover = (self.frame_number - f64::from(high)) - 1.0;
                    if leftover < 0.0 {
                        leftover = 0.0;
                    }
                    leftover = if frd.abs() <= EPS {
                        0.0
                    } else {
                        leftover / frd
                    };
                    self.frame_number = f64::from(high);
                    done_early = true;
                }
                while f64::from(i) < self.frame_number.floor() {
                    if let Some(f) = frame.as_deref_mut() {
                        if self.nodes[curr].anim.pos_frames.is_some() {
                            if let Some(pf) = self.nodes[curr].get_pos_frame(i) {
                                *f = combine(f, pf);
                            }
                        }
                        if frd.abs() > EPS {
                            Self::apply_physics_to(f, self.velocity, self.omega, 1.0 / frd, dt);
                        }
                    }
                    self.execute_hooks(i, 1, out);
                    i += 1;
                }
            }

            if !done_early {
                return;
            }
            // "A link animation finished." A pure cycle never reports done, because then the head
            // *is* `first_cyclic`.
            if self.hooks_enabled && self.first_cyclic != Some(0) {
                out.push(ANIM_DONE_HOOK);
            }
            self.advance_to_next_animation(dt, frame.as_deref_mut());
            dt = leftover;
        }
    }

    /// Advance to the next animation.
    ///
    /// The `subtract1`/`combine` pairs exist because a node whose framerate has the *opposite* sign
    /// to the direction of travel has its boundary `pos_frames` contribution counted the other way
    /// round: the old node's is un-applied and the new node's is applied.
    fn advance_to_next_animation(&mut self, dt: f64, mut frame: Option<&mut Frame>) {
        let Some(curr) = self.curr else {
            return;
        };
        let fr_old = self.nodes[curr].framerate;
        // The client's `fr` local, assigned in whichever arm reaches the shared tail below.
        #[allow(clippy::needless_late_init)]
        let next_fr: f64;

        if dt < 0.0 {
            if fr_old >= 0.0 {
                if let Some(f) = frame.as_deref_mut() {
                    let i = dereth_primitives::num::to_i32_f64(self.frame_number.floor());
                    if self.nodes[curr].anim.pos_frames.is_some() {
                        if let Some(pf) = self.nodes[curr].get_pos_frame(i) {
                            *f = subtract1(f, pf);
                        }
                    }
                    if f64::from(fr_old).abs() > EPS {
                        Self::apply_physics_to(
                            f,
                            self.velocity,
                            self.omega,
                            1.0 / f64::from(fr_old),
                            dt,
                        );
                    }
                }
            }
            // `GetPrev() ?: tail` — stepping backwards wraps to the tail.
            let next = if curr == 0 {
                self.nodes.len().checked_sub(1)
            } else {
                Some(curr - 1)
            };
            let Some(next) = next else {
                self.curr = None;
                return;
            };
            self.curr = Some(next);
            self.frame_number = self.nodes[next].get_ending_frame();
            if self.nodes[next].framerate >= 0.0 {
                return;
            }
            let Some(f) = frame.as_deref_mut() else {
                return;
            };
            let i = dereth_primitives::num::to_i32_f64(self.frame_number.floor());
            if self.nodes[next].anim.pos_frames.is_some() {
                if let Some(pf) = self.nodes[next].get_pos_frame(i) {
                    *f = combine(f, pf);
                }
            }
            next_fr = f64::from(self.nodes[next].framerate);
        } else {
            if fr_old < 0.0 {
                if let Some(f) = frame.as_deref_mut() {
                    let i = dereth_primitives::num::to_i32_f64(self.frame_number.floor());
                    if self.nodes[curr].anim.pos_frames.is_some() {
                        if let Some(pf) = self.nodes[curr].get_pos_frame(i) {
                            *f = subtract1(f, pf);
                        }
                    }
                    if f64::from(fr_old).abs() > EPS {
                        Self::apply_physics_to(
                            f,
                            self.velocity,
                            self.omega,
                            1.0 / f64::from(fr_old),
                            dt,
                        );
                    }
                }
            }
            // `GetNext() ?: first_cyclic` — **this wrap is the whole looping mechanism**.
            let next = if curr + 1 < self.nodes.len() {
                Some(curr + 1)
            } else {
                self.first_cyclic
            };
            let Some(next) = next else {
                self.curr = None;
                return;
            };
            self.curr = Some(next);
            self.frame_number = self.nodes[next].get_starting_frame();
            if self.nodes[next].framerate <= 0.0 {
                return;
            }
            let Some(f) = frame.as_deref_mut() else {
                return;
            };
            let i = dereth_primitives::num::to_i32_f64(self.frame_number.floor());
            if self.nodes[next].anim.pos_frames.is_some() {
                if let Some(pf) = self.nodes[next].get_pos_frame(i) {
                    *f = combine(f, pf);
                }
            }
            next_fr = f64::from(self.nodes[next].framerate);
        }

        if next_fr.abs() > EPS {
            if let Some(f) = frame {
                Self::apply_physics_to(f, self.velocity, self.omega, 1.0 / next_fr, dt);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use dereth_primitives::{DataId, Frame, Vec3};

    use super::*;
    use crate::data::{AnimData, AnimFrame, AnimationData, MapAssets};
    use crate::hooks::HookKind;
    use crate::seq::Sequence;

    fn tagged_anim(id: u32, frames: u32, with_pos: bool) -> (DataId, Arc<AnimationData>) {
        // One hook per frame, tagged with the frame index through `DestroyParticle`'s id.
        let part_frames = (0..frames)
            .map(|i| AnimFrame {
                frames: vec![Frame::default()],
                hooks: vec![AnimHook::new(
                    0,
                    HookKind::DestroyParticle { emitter_id: i },
                )],
            })
            .collect();
        let pos_frames = with_pos.then(|| {
            (0..frames)
                .map(|_| Frame::new(Vec3::new(0.0, 1.0, 0.0), dereth_primitives::Quat::IDENTITY))
                .collect()
        });
        (
            DataId(0x0300_0000 | id),
            Arc::new(AnimationData {
                num_frames: frames,
                num_parts: 1,
                pos_frames,
                part_frames,
                has_hooks: true,
            }),
        )
    }

    fn assets(anims: &[(DataId, Arc<AnimationData>)]) -> MapAssets {
        let mut m = MapAssets::default();
        for (id, a) in anims {
            m.animations.insert(id.0, Arc::clone(a));
        }
        m
    }

    fn fired(out: &[AnimHook]) -> Vec<i64> {
        out.iter()
            .map(|h| match h.kind {
                HookKind::DestroyParticle { emitter_id } => i64::from(emitter_id),
                HookKind::AnimationDone => -1,
                _ => -2,
            })
            .collect()
    }

    /// ORACLE: the recovered animation-playback behavior section 3, the
    /// `update_internal` step, cross-read against retail.
    ///
    /// The spec's acceptance test: 30 fps, frames 0..9, driven at 1/30 s crosses exactly one frame
    /// per call and ends at `high_frame + 1 − 0.0002`.
    #[test]
    fn one_frame_per_thirtieth_of_a_second_and_the_end_is_high_plus_one_minus_epsilon() {
        let (id, a) = tagged_anim(1, 10, false);
        let assets = assets(&[(id, a)]);
        let mut s = Sequence::new();
        s.append_animation(
            AnimData {
                anim_id: id,
                low_frame: 0,
                high_frame: 9,
                framerate: 30.0,
            },
            &assets,
        );
        let dt = 1.0 / 30.0;
        let mut out = Vec::new();
        for expect in 0..9 {
            out.clear();
            s.update(dt, None, &mut out);
            assert_eq!(fired(&out), vec![i64::from(expect)], "step {expect}");
            assert_eq!(s.curr_frame_number(), expect + 1);
        }
        // The tenth step runs off the end. Two things happen that are easy to get wrong:
        //
        // * **the high frame's hooks never fire when playing forwards.** `frame_number` is clamped
        //   back to `high_frame` before the crossing loop runs, so `i < floor(frame_number)` is
        //   `9 < 9` and frame 9 is never *left*. It fires only when the frame is crossed the other
        //   way (see the backwards test below).
        // * a pure cycle never reports `AnimationDone`, because the list head **is**
        //   `first_cyclic`.
        out.clear();
        s.update(dt, None, &mut out);
        assert!(fired(&out).is_empty());
        assert_eq!(
            s.curr_frame_number(),
            0,
            "wrapped to first_cyclic, which is itself"
        );
    }

    /// A negative framerate plays backwards, fires `BACKWARD` hooks and swaps the bounds.
    #[test]
    fn a_negative_framerate_plays_backwards() {
        let (id, a) = tagged_anim(1, 10, false);
        let assets = assets(&[(id, a)]);
        let mut s = Sequence::new();
        s.append_animation(
            AnimData {
                anim_id: id,
                low_frame: 0,
                high_frame: 9,
                framerate: -30.0,
            },
            &assets,
        );
        // The bounds are **not** swapped here: `multiply_framerate` does the swapping, and a node
        // constructed straight from a negative-framerate `AnimData` keeps `low = 0, high = 9`.
        // `get_starting_frame` for a reversed node is `high + 1 - 0.0002`, so it starts at the end.
        assert_eq!(s.curr_frame_number(), 9);
        let mut out = Vec::new();
        s.update(1.0 / 30.0, None, &mut out);
        assert_eq!(
            fired(&out),
            vec![9],
            "frame 9's hook fires as it is crossed backwards"
        );
        assert_eq!(s.curr_frame_number(), 8);
    }

    /// Several frames crossed in one step fire every hook and apply every `pos_frames` delta.
    /// This pins every crossed animation frame.
    #[test]
    fn multiple_frames_in_one_step_fire_every_hook_and_move_once_per_frame() {
        let (id, a) = tagged_anim(1, 10, true);
        let assets = assets(&[(id, a)]);
        let mut s = Sequence::new();
        s.append_animation(
            AnimData {
                anim_id: id,
                low_frame: 0,
                high_frame: 9,
                framerate: 30.0,
            },
            &assets,
        );
        s.set_velocity(Vec3::new(0.0, 3.0, 0.0));
        let mut f = Frame::default();
        let mut out = Vec::new();
        // 5/30 s at 30 fps: five frames crossed.
        s.update(5.0 / 30.0, Some(&mut f), &mut out);
        assert_eq!(fired(&out), vec![0, 1, 2, 3, 4]);
        // Five `pos_frames` of +1 in y, plus five quanta of velocity at 1/30 s each.
        let expect = 5.0 + 5.0 * 3.0 / 30.0;
        assert!(
            (f.origin.y - expect).abs() < 1e-5,
            "{} != {expect}",
            f.origin.y
        );
    }

    /// An animation played at half speed moves the object half as far in the same wall-clock time,
    /// **for free**, because it crosses half as many frames. This is the property a "sample at
    /// time t" rewrite loses.
    ///
    /// The channel that has it is the **root motion** (`pos_frames`): one fixed delta per frame
    /// crossed. The motion table's *constant velocity* does not, because `apply_physics` is called
    /// with `quantum = 1/framerate` — halving the framerate doubles the quantum and exactly
    /// cancels the halved crossing count. (What makes a slower motion move slower on that channel
    /// is `add_motion` scaling the velocity by the same speed.) Both halves are asserted here,
    /// because getting the second one backwards is the easy mistake.
    #[test]
    fn half_speed_crosses_half_the_frames_so_root_motion_halves() {
        let (id, a) = tagged_anim(1, 30, true);
        let assets = assets(&[(id, a)]);
        let run = |framerate: f32, velocity: f32| {
            let mut s = Sequence::new();
            s.append_animation(
                AnimData {
                    anim_id: id,
                    low_frame: 0,
                    high_frame: 29,
                    framerate,
                },
                &assets,
            );
            s.set_velocity(Vec3::new(0.0, velocity, 0.0));
            let mut f = Frame::default();
            let mut out = Vec::new();
            s.update(10.0 / 30.0, Some(&mut f), &mut out);
            f.origin.y
        };
        // Root motion alone: 10 frames of +1 at full speed, 5 at half.
        assert!((run(30.0, 0.0) - 10.0).abs() < 1e-4, "{}", run(30.0, 0.0));
        assert!((run(15.0, 0.0) - 5.0).abs() < 1e-4, "{}", run(15.0, 0.0));
        // The constant-velocity channel alone: identical, because quantum = 1/framerate.
        let (id2, a2) = tagged_anim(2, 30, false);
        let assets2 = super::tests::assets(&[(id2, a2)]);
        let vel_only = |framerate: f32| {
            let mut s = Sequence::new();
            s.append_animation(
                AnimData {
                    anim_id: id2,
                    low_frame: 0,
                    high_frame: 29,
                    framerate,
                },
                &assets2,
            );
            s.set_velocity(Vec3::new(0.0, 3.0, 0.0));
            let mut f = Frame::default();
            let mut out = Vec::new();
            s.update(10.0 / 30.0, Some(&mut f), &mut out);
            f.origin.y
        };
        assert!((vel_only(30.0) - 1.0).abs() < 1e-5, "{}", vel_only(30.0));
        assert!((vel_only(15.0) - 1.0).abs() < 1e-5, "{}", vel_only(15.0));
    }

    /// `leftover` carries into the next node, so a chain of one-frame animations plays in a single
    /// call and every one of their hooks fires.
    #[test]
    fn leftover_carries_so_a_chain_of_short_animations_plays_in_one_step() {
        let (id1, a1) = tagged_anim(1, 2, false);
        let (id2, a2) = tagged_anim(2, 2, false);
        let (id3, a3) = tagged_anim(3, 2, false);
        let assets = assets(&[(id1, a1), (id2, a2), (id3, a3)]);
        let mut s = Sequence::new();
        for id in [id1, id2, id3] {
            s.append_animation(
                AnimData {
                    anim_id: id,
                    low_frame: 0,
                    high_frame: 1,
                    framerate: 30.0,
                },
                &assets,
            );
        }
        let mut out = Vec::new();
        // A fifth of a second is six frames at 30 fps: enough to run off the end of the first two
        // nodes and reach the third. A "one node per call" player would still be on the first.
        s.update(0.2, None, &mut out);
        let f = fired(&out);
        // Three `AnimationDone`s, not two: the test is `anim_list.head != first_cyclic`, on the
        // list **head**, not on the node that finished. While any link animation is still in the
        // list — and `apricot` only runs at the end of `update` — even the cyclic node's first
        // wrap reports done.
        assert_eq!(f.iter().filter(|&&x| x == -1).count(), 3, "{f:?}");
        assert!(
            f.iter().filter(|&&x| x == 0).count() >= 2,
            "several nodes played: {f:?}"
        );
        // `apricot` freed the consumed link nodes.
        assert_eq!(s.nodes().len(), 1, "only the cyclic node survives: {f:?}");
    }

    /// The display-only path: `frame == None` still fires hooks and advances frames but applies no
    /// physics at all.
    #[test]
    fn a_none_frame_is_display_only() {
        let (id, a) = tagged_anim(1, 10, true);
        let assets = assets(&[(id, a)]);
        let mut s = Sequence::new();
        s.append_animation(
            AnimData {
                anim_id: id,
                low_frame: 0,
                high_frame: 9,
                framerate: 30.0,
            },
            &assets,
        );
        s.set_velocity(Vec3::new(0.0, 3.0, 0.0));
        let mut out = Vec::new();
        s.update(3.0 / 30.0, None, &mut out);
        assert_eq!(fired(&out), vec![0, 1, 2], "hooks still fire");
        assert_eq!(s.curr_frame_number(), 3);
    }

    /// With no animations at all, `apply_physics` is called once with `quantum = dt`.
    #[test]
    fn with_no_animation_the_velocity_integrates_over_the_whole_step() {
        let mut s = Sequence::new();
        s.set_velocity(Vec3::new(0.0, 2.0, 0.0));
        let mut f = Frame::default();
        let mut out = Vec::new();
        s.update(0.5, Some(&mut f), &mut out);
        assert!((f.origin.y - 1.0).abs() < 1e-6, "{}", f.origin.y);
        assert!(out.is_empty());
    }

    /// A hook whose frame is crossed twice in one step is queued twice: the client does not
    /// deduplicate.
    #[test]
    fn a_frame_crossed_twice_fires_twice() {
        let (id, a) = tagged_anim(1, 2, false);
        let assets = assets(&[(id, a)]);
        let mut s = Sequence::new();
        s.append_animation(
            AnimData {
                anim_id: id,
                low_frame: 0,
                high_frame: 1,
                framerate: 30.0,
            },
            &assets,
        );
        let mut out = Vec::new();
        // Long enough to loop the two-frame cycle several times.
        s.update(6.0 / 30.0, None, &mut out);
        let f = fired(&out);
        assert!(f.iter().filter(|&&x| x == 0).count() >= 2, "{f:?}");
    }

    /// `play_time` and `fired_frames` are what the update loop does: driven in small steps, each
    /// link node reports done after its play time and fires its frames at the times listed,
    /// forwards and backwards, over part of an animation and over the whole of one.
    #[test]
    fn play_time_and_fired_frames_match_the_update_loop() {
        let (id, a) = tagged_anim(1, 12, false);
        let assets = assets(&[(id, a)]);
        let mut s = Sequence::new();
        for (low, high, fr) in [(2, 7, 30.0), (0, -1, -20.0), (3, 5, 10.0)] {
            s.append_animation(
                AnimData {
                    anim_id: id,
                    low_frame: low,
                    high_frame: high,
                    framerate: fr,
                },
                &assets,
            );
        }
        // The cycle the links lead into.
        s.append_animation(
            AnimData {
                anim_id: id,
                low_frame: 0,
                high_frame: -1,
                framerate: 30.0,
            },
            &assets,
        );

        let mut want_hooks = Vec::new();
        let mut want_done = Vec::new();
        let mut start = 0.0;
        for n in &s.nodes()[..3] {
            want_hooks.extend(
                n.fired_frames()
                    .into_iter()
                    .map(|(i, t, _)| (i64::from(i), start + t)),
            );
            start += n.play_time();
            want_done.push(start);
        }
        assert!((s.play_time(3) - start).abs() < 1e-9);

        let dt = 1.0 / 8192.0;
        let (mut t, mut hooks, mut done) = (0.0, Vec::new(), Vec::new());
        let mut out = Vec::new();
        while done.len() < 3 {
            out.clear();
            s.update(dt, None, &mut out);
            t += dt;
            for x in fired(&out) {
                if x == -1 {
                    done.push(t);
                } else {
                    hooks.push((x, t));
                }
            }
        }
        assert_eq!(done.len(), want_done.len());
        for (got, want) in done.iter().zip(&want_done) {
            assert!((got - want).abs() <= 2.0 * dt, "done at {got}, want {want}");
        }
        assert_eq!(
            hooks.iter().map(|h| h.0).collect::<Vec<_>>(),
            want_hooks.iter().map(|h| h.0).collect::<Vec<_>>()
        );
        for (got, want) in hooks.iter().zip(&want_hooks) {
            assert!(
                (got.1 - want.1).abs() <= 2.0 * dt,
                "frame {} at {}, want {}",
                got.0,
                got.1,
                want.1
            );
        }
    }
}
