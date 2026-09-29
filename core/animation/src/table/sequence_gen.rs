//! Motion-table sequence generation (`get_object_sequence`) and the stopping paths.
//!
//! The routine decides, for a transition from the object's current state to a requested command,
//! which *link* animations to play, which *cycle* animation to loop afterwards, and how many of the
//! queued animations count as "the transition". That count is what the manager's completion ledger
//! runs on.
//!
//! **`num_anims − 1` on the cycle branches is the whole looping mechanism**: the last
//! animation of a cycle loops forever and never fires `AnimationDone`, so only the ones before it
//! count. The subtraction is done in wrapping `u32` arithmetic because the client does it in a
//! `ulong` and 1,222 shipped `MotionData` records have `num_anims == 0`, for which the client
//! genuinely produces `0xFFFFFFFF`.

use crate::command::MotionCommand;
use crate::data::{AnimAssets, MotionData};
use crate::seq::Sequence;

use super::{add_motion, combine_motion, same_sign, subtract_motion, MotionState, MotionTable};

/// `num_anims` of an optional `MotionData`.
fn n(md: Option<&MotionData>) -> u32 {
    md.map_or(0, MotionData::num_anims)
}

impl MotionTable {
    /// Returns the link count on success.
    ///
    /// Note the `num_anims − 1`: the single cycle it queues loops, so it contributes nothing but
    /// its own preceding animations to the ledger.
    pub fn set_default_state(
        &self,
        state: &mut MotionState,
        seq: &mut Sequence,
        assets: &dyn AnimAssets,
    ) -> Option<u32> {
        let substate = self.style_default(self.default_style())?;
        state.clear_modifiers();
        state.clear_actions();
        let md = self.cycle(self.default_style(), substate)?;
        state.style = self.default_style();
        state.substate = substate;
        state.substate_mod = 1.0;
        let num_anims = md.num_anims().wrapping_sub(1);
        seq.clear_physics();
        seq.clear_animations();
        let md = md.clone();
        add_motion(seq, Some(&md), 1.0, assets);
        Some(num_anims)
    }

    /// `get_object_sequence` with `stop_modifiers`
    /// clear.
    pub fn do_object_motion(
        &self,
        motion: MotionCommand,
        state: &mut MotionState,
        seq: &mut Sequence,
        speed: f32,
        assets: &dyn AnimAssets,
    ) -> Option<u32> {
        self.get_object_sequence(motion, state, seq, speed, false, assets)
    }

    /// Build the object sequence for a motion.
    ///
    /// The `assets` argument supplies animation data explicitly: the observed client uses a global
    /// dat-object cache when appending an animation, whereas this crate resolves ids through the host.
    /// It has no global cache.
    #[allow(clippy::too_many_lines)]
    pub fn get_object_sequence(
        &self,
        motion: MotionCommand,
        state: &mut MotionState,
        seq: &mut Sequence,
        speed: f32,
        stop_modifiers: bool,
        assets: &dyn AnimAssets,
    ) -> Option<u32> {
        if state.style == MotionCommand::NONE {
            return None;
        }
        let substate = state.substate;
        if substate == MotionCommand::NONE {
            return None;
        }
        let style_default = self.style_default(state.style);

        // (0) Re-requesting the style default while a modifier-class motion is current: nothing to
        // do, and in particular the modifier is *not* dropped.
        if Some(motion) == style_default && !stop_modifiers && substate.is_modifier() {
            return Some(0);
        }

        // ---- (A) `motion` is a style -------------------------------------------------------
        if motion.is_style() {
            if state.style == motion {
                return Some(0);
            }
            if let (Some(style_default), Some(new_substate)) =
                (style_default, self.style_default(motion))
            {
                if let Some(cycle) = self.cycle(motion, new_substate) {
                    let link = if substate == style_default {
                        None
                    } else {
                        self.get_link(
                            state.style,
                            substate,
                            state.substate_mod,
                            style_default,
                            speed,
                        )
                    };
                    if cycle.clears_modifiers() {
                        state.clear_modifiers();
                    }
                    let mut mid = self.get_link(
                        state.style,
                        style_default,
                        state.substate_mod,
                        motion,
                        speed,
                    );
                    let mut extra_link = None;
                    if mid.is_none() && motion != state.style {
                        mid = self.get_link(
                            state.style,
                            style_default,
                            1.0,
                            self.default_style(),
                            1.0,
                        );
                        if let Some(d2) = self.style_default(self.default_style()) {
                            extra_link = self.get_link(self.default_style(), d2, 1.0, motion, 1.0);
                        }
                    }
                    let num_anims = n(link)
                        .wrapping_add(n(mid))
                        .wrapping_add(n(extra_link))
                        .wrapping_add(cycle.num_anims())
                        .wrapping_sub(1);

                    let (link, mid, extra_link, cycle) = (
                        link.cloned(),
                        mid.cloned(),
                        extra_link.cloned(),
                        cycle.clone(),
                    );
                    seq.clear_physics();
                    seq.remove_cyclic_anims();
                    add_motion(seq, link.as_ref(), speed, assets);
                    add_motion(seq, mid.as_ref(), speed, assets);
                    add_motion(seq, extra_link.as_ref(), speed, assets);
                    add_motion(seq, Some(&cycle), speed, assets);
                    state.substate = new_substate;
                    state.style = motion;
                    state.substate_mod = speed;
                    self.re_modify(state, seq, assets);
                    return Some(num_anims);
                }
            }
            // No style default or no cycle for it: fall through, and since a style carries none of
            // the other class bits, every remaining branch declines.
        }

        // ---- (B) `motion` is a substate / cycle --------------------------------------------
        if motion.is_substate() {
            let cycle = self
                .cycle(state.style, motion)
                .or_else(|| self.cycle(self.default_style(), motion));
            if let Some(cycle) = cycle {
                if self.is_allowed(motion, cycle, state) {
                    // (B1) Same cycle, same direction, animations already running: just retime.
                    if motion == substate && same_sign(speed, state.substate_mod) && seq.has_anims()
                    {
                        let cycle = cycle.clone();
                        super::change_cycle_speed(seq, state.substate_mod, speed);
                        subtract_motion(seq, Some(&cycle), state.substate_mod);
                        combine_motion(seq, Some(&cycle), speed);
                        state.substate_mod = speed;
                        return Some(0);
                    }

                    if cycle.clears_modifiers() {
                        state.clear_modifiers();
                    }
                    let mut link =
                        self.get_link(state.style, substate, state.substate_mod, motion, speed);
                    let mut extra_link = None;
                    if link.is_none() || !same_sign(speed, state.substate_mod) {
                        let sd = self.style_default(state.style);
                        link = sd.and_then(|sd| {
                            self.get_link(state.style, substate, state.substate_mod, sd, 1.0)
                        });
                        extra_link =
                            sd.and_then(|sd| self.get_link(state.style, sd, 1.0, motion, speed));
                    }
                    let num_anims = n(link)
                        .wrapping_add(n(extra_link))
                        .wrapping_add(cycle.num_anims())
                        .wrapping_sub(1);

                    let (link, extra_link, cycle) =
                        (link.cloned(), extra_link.cloned(), cycle.clone());
                    seq.clear_physics();
                    seq.remove_cyclic_anims();
                    if extra_link.is_none() {
                        let s = if state.substate_mod < 0.0 && speed > 0.0 {
                            -speed
                        } else {
                            speed
                        };
                        add_motion(seq, link.as_ref(), s, assets);
                    } else {
                        add_motion(seq, link.as_ref(), state.substate_mod, assets);
                        add_motion(seq, extra_link.as_ref(), speed, assets);
                    }
                    add_motion(seq, Some(&cycle), speed, assets);

                    // Remember a modifier-class substate we are leaving, so `re_modify` restores it.
                    if substate != motion
                        && substate.is_modifier()
                        && self.style_default(state.style) != Some(motion)
                    {
                        state.add_modifier_no_check(substate, state.substate_mod);
                    }
                    state.substate_mod = speed;
                    state.substate = motion;
                    self.re_modify(state, seq, assets);
                    return Some(num_anims);
                }
            }
        }

        // ---- (C) `motion` is an action -----------------------------------------------------
        if motion.is_action() {
            if let Some(cycle) = self.cycle(state.style, substate) {
                // (C1) A direct link from the current substate to the action.
                if let Some(link) =
                    self.get_link(state.style, substate, state.substate_mod, motion, speed)
                {
                    let num_anims = link.num_anims();
                    let (link, cycle) = (link.clone(), cycle.clone());
                    state.add_action(motion, speed);
                    seq.clear_physics();
                    seq.remove_cyclic_anims();
                    add_motion(seq, Some(&link), speed, assets);
                    add_motion(seq, Some(&cycle), state.substate_mod, assets);
                    self.re_modify(state, seq, assets);
                    return Some(num_anims);
                }

                // (C2) Two hops via the style default, and a third link back to the substate.
                if let Some(style_default) = self.style_default(state.style) {
                    let a = self.get_link(
                        state.style,
                        substate,
                        state.substate_mod,
                        style_default,
                        1.0,
                    );
                    let b = self.get_link(state.style, style_default, 1.0, motion, speed);
                    if let (Some(a), Some(b)) = (a, b) {
                        let d = self.get_link(
                            state.style,
                            style_default,
                            1.0,
                            substate,
                            state.substate_mod,
                        );
                        let num_anims =
                            a.num_anims().wrapping_add(b.num_anims()).wrapping_add(n(d));
                        let (a, b, d, c) = (a.clone(), b.clone(), d.cloned(), cycle.clone());
                        state.add_action(motion, speed);
                        seq.clear_physics();
                        seq.remove_cyclic_anims();
                        add_motion(seq, Some(&a), 1.0, assets);
                        add_motion(seq, Some(&b), speed, assets);
                        add_motion(seq, d.as_ref(), 1.0, assets);
                        add_motion(seq, Some(&c), state.substate_mod, assets);
                        self.re_modify(state, seq, assets);
                        return Some(num_anims);
                    }
                }
            }
        }

        // ---- (D) `motion` is a modifier ----------------------------------------------------
        if motion.is_modifier() {
            let cycle = self.cycle(state.style, substate)?;
            if cycle.clears_modifiers() {
                // A moving cycle refuses modifiers.
                return None;
            }
            let md = self.modifier(state.style, motion)?.clone();
            if !state.add_modifier(motion, speed) {
                // Already present: stop it and re-add, so the speed is the new one.
                let mut ignored = 0;
                self.stop_sequence_motion(motion, 1.0, state, seq, &mut ignored, assets);
                if !state.add_modifier(motion, speed) {
                    return None;
                }
            }
            combine_motion(seq, Some(&md), speed);
            return Some(0);
        }

        None
    }

    /// Re-modify the sequence.
    ///
    /// Every active modifier is re-applied on top of the new cycle, taken from the **head** of the
    /// real list each time. Because branch (D) pushes at the head again, the list comes out
    /// reversed — which is the client's own behaviour and is observable in the modifier order.
    fn re_modify(&self, state: &mut MotionState, seq: &mut Sequence, assets: &dyn AnimAssets) {
        let mut remaining = state.modifiers.len();
        while remaining > 0 {
            let Some(m) = state.modifiers.first().copied() else {
                return;
            };
            state.modifiers.remove(0);
            self.get_object_sequence(m.motion, state, seq, m.speed_mod, false, assets);
            remaining -= 1;
        }
    }

    /// An identical body to `StopSequenceMotion
    /// `; the linker did not fold them.
    pub fn stop_object_motion(
        &self,
        motion: MotionCommand,
        speed: f32,
        state: &mut MotionState,
        seq: &mut Sequence,
        assets: &dyn AnimAssets,
    ) -> Option<u32> {
        let mut num_anims = 0;
        if self.stop_sequence_motion(motion, speed, state, seq, &mut num_anims, assets) {
            Some(num_anims)
        } else {
            None
        }
    }

    fn stop_sequence_motion(
        &self,
        motion: MotionCommand,
        _speed: f32,
        state: &mut MotionState,
        seq: &mut Sequence,
        num_anims: &mut u32,
        assets: &dyn AnimAssets,
    ) -> bool {
        *num_anims = 0;
        if motion.is_substate() && motion == state.substate {
            let Some(sd) = self.style_default(state.style) else {
                return false;
            };
            // `stop_modifiers = TRUE`: this is the one caller that passes it.
            if let Some(k) = self.get_object_sequence(sd, state, seq, 1.0, true, assets) {
                *num_anims = k;
            }
            return true;
        }
        if motion.is_modifier() {
            let Some(i) = state.modifiers.iter().position(|m| m.motion == motion) else {
                return false;
            };
            let speed_mod = state.modifiers[i].speed_mod;
            let Some(md) = self.modifier(state.style, motion).cloned() else {
                return false;
            };
            subtract_motion(seq, Some(&md), speed_mod);
            state.remove_modifier_at(i);
            return true;
        }
        false
    }

    /// Stop every modifier, then the substate.
    /// Returns the accumulated link count; the client returns 1 if anything was stopped.
    pub fn stop_object_completely(
        &self,
        state: &mut MotionState,
        seq: &mut Sequence,
        assets: &dyn AnimAssets,
    ) -> u32 {
        let mut total: u32 = 0;
        while let Some(head) = state.modifiers.first().copied() {
            let mut n: u32 = 0;
            if !self.stop_sequence_motion(head.motion, 1.0, state, seq, &mut n, assets) {
                // UNVERIFIED: the client loops on `state.modifier_head` until it is empty and
                // relies on `StopSequenceMotion` removing the head every time. It can refuse — the
                // modifier lookup misses — and what retail does then is not known; a
                // literal transcription would spin. Dropping the refused head is the minimal
                // deviation that terminates, and no shipped table reaches it (the expansion gate
                // walks all 436 and never takes this path).
                state.remove_modifier_at(0);
            }
            total = total.wrapping_add(n);
        }
        let substate = state.substate;
        let mut n: u32 = 0;
        self.stop_sequence_motion(substate, 1.0, state, seq, &mut n, assets);
        total.wrapping_add(n)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use dereth_primitives::{DataId, Vec3};

    use super::*;
    use crate::data::{AnimData, AnimationData, MapAssets, MotionTableData};
    use crate::seq::Sequence;

    const STYLE: MotionCommand = MotionCommand::NON_COMBAT;
    const COMBAT: MotionCommand = MotionCommand::SWORD_COMBAT;

    /// A tiny synthetic table with one style, a Ready cycle, a walk cycle and the links between
    /// them, so every branch can be driven deterministically.
    fn fixture() -> (MotionTable, MapAssets) {
        let mut assets = MapAssets::default();
        for i in 1..=9u32 {
            assets.animations.insert(
                0x0300_0000 | i,
                Arc::new(AnimationData {
                    num_frames: 4,
                    num_parts: 1,
                    pos_frames: None,
                    part_frames: vec![crate::data::AnimFrame::default(); 4],
                    has_hooks: false,
                }),
            );
        }
        let one = |i: u32, fr: f32| MotionData {
            anims: vec![AnimData {
                anim_id: DataId(0x0300_0000 | i),
                low_frame: 0,
                high_frame: -1,
                framerate: fr,
            }],
            bitfield: 0,
            velocity: Vec3::ZERO,
            omega: Vec3::ZERO,
        };

        let mut d = MotionTableData {
            default_style: STYLE,
            ..MotionTableData::default()
        };
        d.style_defaults.insert(STYLE.0, MotionCommand::READY);
        d.style_defaults.insert(COMBAT.0, MotionCommand::READY);
        // Cycles.
        d.cycles
            .insert(MotionTable::key(STYLE, MotionCommand::READY), one(1, 30.0));
        d.cycles.insert(
            MotionTable::key(STYLE, MotionCommand::WALK_FORWARD),
            MotionData {
                velocity: Vec3::new(0.0, 3.12, 0.0),
                bitfield: 1,
                ..one(2, 30.0)
            },
        );
        d.cycles
            .insert(MotionTable::key(COMBAT, MotionCommand::READY), one(3, 30.0));
        // A modifier, twice: once keyed by the style and once bare, which is the
        // style-independent fallback `GetObjectSequence` branch (D) applies when the styled lookup
        // misses.
        d.modifiers
            .insert(MotionTable::key(STYLE, MotionCommand::JUMP), one(9, 30.0));
        d.modifiers
            .insert(MotionCommand::JUMP.ordinal(), one(9, 30.0));
        // Links: Ready -> WalkForward, WalkForward -> Ready, Ready -> Cheer (an action),
        // Ready -> SwordCombat.
        let mut ready_links = std::collections::BTreeMap::new();
        ready_links.insert(MotionCommand::WALK_FORWARD.0, one(4, 30.0));
        ready_links.insert(MotionCommand::CHEER.0, one(6, 30.0));
        ready_links.insert(COMBAT.0, one(7, 30.0));
        d.links
            .insert(MotionTable::key(STYLE, MotionCommand::READY), ready_links);
        let mut walk_links = std::collections::BTreeMap::new();
        walk_links.insert(MotionCommand::READY.0, one(5, 30.0));
        d.links.insert(
            MotionTable::key(STYLE, MotionCommand::WALK_FORWARD),
            walk_links,
        );

        (MotionTable::new(Arc::new(d)), assets)
    }

    fn anim_ids(seq: &Sequence) -> Vec<u32> {
        seq.nodes().iter().map(|n| n.anim_id.0 & 0xFFFF).collect()
    }

    /// ORACLE: the recovered motion-table behavior section 3,
    /// the default-state setter.
    #[test]
    fn set_default_state_queues_the_default_cycle_and_reports_num_anims_minus_one() {
        let (t, assets) = fixture();
        let mut state = MotionState::new();
        let mut seq = Sequence::new();
        let n = t
            .set_default_state(&mut state, &mut seq, &assets)
            .expect("default state");
        assert_eq!(n, 0, "one animation in the cycle, minus the one that loops");
        assert_eq!(state.style, STYLE);
        assert_eq!(state.substate, MotionCommand::READY);
        assert_eq!(state.substate_mod, 1.0);
        assert_eq!(anim_ids(&seq), vec![1]);
        assert_eq!(seq.first_cyclic(), Some(0));
    }

    /// Branch (B): a link animation then the new cycle, and the link count is
    /// `link + cycle − 1`.
    #[test]
    fn branch_b_queues_the_link_then_the_cycle() {
        let (t, assets) = fixture();
        let mut state = MotionState::new();
        let mut seq = Sequence::new();
        t.set_default_state(&mut state, &mut seq, &assets)
            .expect("default");
        let n = t
            .do_object_motion(
                MotionCommand::WALK_FORWARD,
                &mut state,
                &mut seq,
                1.0,
                &assets,
            )
            .expect("walk");
        assert_eq!(n, 1, "one link animation");
        assert_eq!(anim_ids(&seq), vec![4, 2], "link 4 then cycle 2");
        assert_eq!(seq.first_cyclic(), Some(1));
        assert_eq!(state.substate, MotionCommand::WALK_FORWARD);
        assert_eq!(state.substate_mod, 1.0);
        assert_eq!(seq.velocity, Vec3::new(0.0, 3.12, 0.0));
    }

    /// Branch b1 retimes a running cycle without requeuing.
    #[test]
    fn branch_b1_retimes_a_running_cycle_without_requeuing() {
        let (t, assets) = fixture();
        let mut state = MotionState::new();
        let mut seq = Sequence::new();
        t.set_default_state(&mut state, &mut seq, &assets)
            .expect("default");
        t.do_object_motion(
            MotionCommand::WALK_FORWARD,
            &mut state,
            &mut seq,
            1.0,
            &assets,
        )
        .expect("walk");
        let before = seq.nodes().len();
        let n = t
            .do_object_motion(
                MotionCommand::WALK_FORWARD,
                &mut state,
                &mut seq,
                2.0,
                &assets,
            )
            .expect("walk faster");
        assert_eq!(n, 0);
        assert_eq!(seq.nodes().len(), before, "no new nodes");
        assert_eq!(
            seq.nodes()[1].framerate,
            60.0,
            "the cyclic block was retimed"
        );
        assert_eq!(state.substate_mod, 2.0);
        assert_eq!(seq.velocity, Vec3::new(0.0, 6.24, 0.0));
    }

    /// Branch (A): a style change queries the current substate's link back to the style default,
    /// the default-to-new-style link, and the new style's cycle.
    #[test]
    fn branch_a_changes_style_through_the_style_default() {
        let (t, assets) = fixture();
        let mut state = MotionState::new();
        let mut seq = Sequence::new();
        t.set_default_state(&mut state, &mut seq, &assets)
            .expect("default");
        t.do_object_motion(
            MotionCommand::WALK_FORWARD,
            &mut state,
            &mut seq,
            1.0,
            &assets,
        )
        .expect("walk");
        let n = t
            .do_object_motion(COMBAT, &mut state, &mut seq, 1.0, &assets)
            .expect("draw");
        assert_eq!(state.style, COMBAT);
        assert_eq!(state.substate, MotionCommand::READY);
        // `remove_cyclic_anims` drops only the cyclic block, so the already-queued *link* node 4
        // from the walk transition is still in the list; `apricot` frees it during playback, not
        // here. Then: walk->Ready (5), Ready->SwordCombat (7), the SwordCombat Ready cycle (3).
        assert_eq!(anim_ids(&seq), vec![4, 5, 7, 3]);
        assert_eq!(
            n, 2,
            "the link count counts only what this transition queued"
        );
        // Requesting the style we are already in is a no-op.
        assert_eq!(
            t.do_object_motion(COMBAT, &mut state, &mut seq, 1.0, &assets),
            Some(0)
        );
    }

    /// Branch c1 queues an action and returns to the cycle.
    #[test]
    fn branch_c1_queues_an_action_and_returns_to_the_cycle() {
        let (t, assets) = fixture();
        let mut state = MotionState::new();
        let mut seq = Sequence::new();
        t.set_default_state(&mut state, &mut seq, &assets)
            .expect("default");
        let n = t
            .do_object_motion(MotionCommand::CHEER, &mut state, &mut seq, 1.0, &assets)
            .expect("cheer");
        assert_eq!(n, 1, "the link count does not subtract one for an action");
        assert_eq!(anim_ids(&seq), vec![6, 1], "the action, then back to Ready");
        assert_eq!(state.actions.len(), 1);
        assert_eq!(state.actions[0].motion, MotionCommand::CHEER);
        assert_eq!(
            state.substate,
            MotionCommand::READY,
            "an action does not change the substate"
        );
    }

    /// Branch (D): a modifier only adds velocity, and a cycle with `bitfield & 1` refuses it.
    #[test]
    fn branch_d_adds_a_modifier_without_touching_the_animations() {
        let (t, assets) = fixture();
        let mut state = MotionState::new();
        let mut seq = Sequence::new();
        t.set_default_state(&mut state, &mut seq, &assets)
            .expect("default");
        let before = anim_ids(&seq);
        let n = t
            .do_object_motion(MotionCommand::JUMP, &mut state, &mut seq, 1.0, &assets)
            .expect("jump modifier");
        assert_eq!(n, 0);
        assert_eq!(anim_ids(&seq), before, "no animations queued");
        assert_eq!(state.modifiers.len(), 1);

        // A moving cycle (`bitfield & 1`) refuses modifiers outright.
        let mut state = MotionState::new();
        let mut seq = Sequence::new();
        t.set_default_state(&mut state, &mut seq, &assets)
            .expect("default");
        t.do_object_motion(
            MotionCommand::WALK_FORWARD,
            &mut state,
            &mut seq,
            1.0,
            &assets,
        )
        .expect("walk");
        assert_eq!(
            t.do_object_motion(MotionCommand::JUMP, &mut state, &mut seq, 1.0, &assets),
            None
        );
    }

    /// The `(0)` short-circuit: re-requesting the style default while a modifier-class substate is
    /// current returns immediately and changes nothing.
    #[test]
    fn the_zero_short_circuit_keeps_a_modifier_class_substate() {
        let (t, assets) = fixture();
        let mut state = MotionState {
            style: STYLE,
            // A modifier-class substate: `Jump` has bit 0x20000000.
            substate: MotionCommand::JUMP,
            substate_mod: 1.0,
            ..MotionState::new()
        };
        let mut seq = Sequence::new();
        assert_eq!(
            t.do_object_motion(MotionCommand::READY, &mut state, &mut seq, 1.0, &assets),
            Some(0)
        );
        assert_eq!(state.substate, MotionCommand::JUMP, "unchanged");
        assert!(seq.nodes().is_empty());
    }

    /// `bitfield & 1` on the cycle being entered clears the modifier list.
    #[test]
    fn a_moving_cycle_clears_the_modifiers() {
        let (t, assets) = fixture();
        let mut state = MotionState::new();
        let mut seq = Sequence::new();
        t.set_default_state(&mut state, &mut seq, &assets)
            .expect("default");
        t.do_object_motion(MotionCommand::JUMP, &mut state, &mut seq, 1.0, &assets)
            .expect("modifier");
        assert_eq!(state.modifiers.len(), 1);
        t.do_object_motion(
            MotionCommand::WALK_FORWARD,
            &mut state,
            &mut seq,
            1.0,
            &assets,
        )
        .expect("walk");
        assert!(
            state.modifiers.is_empty(),
            "the walk cycle has bitfield & 1"
        );
    }

    /// The stop paths: stopping the current substate re-enters the style default, and stopping a
    /// modifier removes it and un-applies its velocity.
    #[test]
    fn the_stop_paths_return_to_the_style_default_and_drop_modifiers() {
        let (t, assets) = fixture();
        let mut state = MotionState::new();
        let mut seq = Sequence::new();
        t.set_default_state(&mut state, &mut seq, &assets)
            .expect("default");
        t.do_object_motion(
            MotionCommand::WALK_FORWARD,
            &mut state,
            &mut seq,
            1.0,
            &assets,
        )
        .expect("walk");
        let n = t
            .stop_object_motion(
                MotionCommand::WALK_FORWARD,
                1.0,
                &mut state,
                &mut seq,
                &assets,
            )
            .expect("stop walking");
        assert_eq!(state.substate, MotionCommand::READY);
        // Node 4 is the stale link from the walk transition; `apricot` frees it during playback.
        assert_eq!(
            anim_ids(&seq),
            vec![4, 5, 1],
            "walk->Ready link, then the Ready cycle"
        );
        assert_eq!(n, 1);

        // A modifier.
        t.do_object_motion(MotionCommand::JUMP, &mut state, &mut seq, 1.0, &assets)
            .expect("modifier");
        assert_eq!(state.modifiers.len(), 1);
        assert!(t
            .stop_object_motion(MotionCommand::JUMP, 1.0, &mut state, &mut seq, &assets)
            .is_some());
        assert!(state.modifiers.is_empty());
        // Stopping something that is not running fails.
        assert!(t
            .stop_object_motion(MotionCommand::JUMP, 1.0, &mut state, &mut seq, &assets)
            .is_none());
    }

    /// `StopObjectCompletely` drains the modifiers and then the substate.
    #[test]
    fn stop_object_completely_drains_everything() {
        let (t, assets) = fixture();
        let mut state = MotionState::new();
        let mut seq = Sequence::new();
        t.set_default_state(&mut state, &mut seq, &assets)
            .expect("default");
        t.do_object_motion(MotionCommand::JUMP, &mut state, &mut seq, 1.0, &assets)
            .expect("modifier");
        t.stop_object_completely(&mut state, &mut seq, &assets);
        assert!(state.modifiers.is_empty());
        assert_eq!(state.substate, MotionCommand::READY);
    }

    /// A state with no style refuses every request, which is the `state.style == 0` guard.
    #[test]
    fn an_uninitialised_state_refuses_everything() {
        let (t, assets) = fixture();
        let mut state = MotionState::new();
        let mut seq = Sequence::new();
        assert_eq!(
            t.do_object_motion(
                MotionCommand::WALK_FORWARD,
                &mut state,
                &mut seq,
                1.0,
                &assets
            ),
            None
        );
    }

    /// `re_modify` re-applies the modifiers over the new cycle, and the list comes out reversed
    /// because branch (D) pushes at the head again.
    #[test]
    fn re_modify_replays_the_modifiers_over_the_new_cycle() {
        let (t, assets) = fixture();
        let mut state = MotionState::new();
        let mut seq = Sequence::new();
        t.set_default_state(&mut state, &mut seq, &assets)
            .expect("default");
        // Two modifiers on the Ready cycle. Only `Jump` exists in the table; add a second by hand
        // so the ordering is observable.
        t.do_object_motion(MotionCommand::JUMP, &mut state, &mut seq, 1.0, &assets)
            .expect("modifier");
        state.add_modifier_no_check(MotionCommand::STOP_TURNING, 1.0);
        assert_eq!(state.modifiers[0].motion, MotionCommand::STOP_TURNING);
        // Changing to the combat style replays them; `StopTurning` has no modifier entry, so it is
        // dropped by branch (D) and only `Jump` survives.
        t.do_object_motion(COMBAT, &mut state, &mut seq, 1.0, &assets)
            .expect("draw");
        assert_eq!(state.modifiers.len(), 1);
        assert_eq!(state.modifiers[0].motion, MotionCommand::JUMP);
    }
}
