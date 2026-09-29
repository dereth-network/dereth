//! `MotionTableManager`: the pending-animation ledger, `remove_redundant_links` and
//! completion accounting.
//!
//! `remove_redundant_links` is **not cosmetic**: without it, spamming a movement key
//! visibly replays the stand→walk transition every time.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use crate::command::MotionCommand;
use crate::data::{AnimAssets, MotionTableData};
#[cfg(test)]
use crate::hooks::AnimEvent;
use crate::seq::Sequence;

use super::{MotionState, MotionTable};

/// The client's movement-type enum (`MovementTypes::Type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MovementType {
    Invalid = 0,
    RawCommand = 1,
    InterpretedCommand = 2,
    StopRawCommand = 3,
    StopInterpretedCommand = 4,
    StopCompletely = 5,
    MoveToObject = 6,
    MoveToPosition = 7,
    TurnToObject = 8,
    TurnToHeading = 9,
}

/// The subset of `MovementStruct` this manager reads. The full struct also carries the object ids,
/// the target position and the radius/height, all of which belong to `MoveToManager`.
#[derive(Debug, Clone, Copy)]
pub struct MovementStruct {
    pub kind: MovementType,
    pub motion: MotionCommand,
    pub speed: f32,
}

/// `AnimNode { motion, num_anims }` — one queued motion awaiting completion.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnimNode {
    pub motion: MotionCommand,
    pub num_anims: u32,
}

/// The source continuation reads its ORIGINAL node's animation count after MotionDone.
/// A retained logical count permits live callback mutation without a borrow over reentry.
/// If a callback removes that node, retain its count safely; retail can read freed
/// memory there. Allocator reuse bytes are deliberately not emulated.
#[derive(Debug, Clone)]
pub(crate) struct PendingAnimation {
    pub(crate) motion: MotionCommand,
    count: SharedCount,
}

impl PendingAnimation {
    fn new(motion: MotionCommand, count: u32) -> Self {
        Self {
            motion,
            count: SharedCount::new(count),
        }
    }

    fn snapshot(&self) -> AnimNode {
        AnimNode {
            motion: self.motion,
            num_anims: self.count.get(),
        }
    }
}

/// The retained count's shared cell. An atomic behind an `Arc` rather than a `Cell` behind an
/// `Rc` only so that a `MotionDriver` is `Send` (a physics world can then move between threads);
/// the ledger is single-threaded, and relaxed ordering is all a `Cell` ever gave.
#[derive(Debug, Clone)]
struct SharedCount(Arc<AtomicU32>);

impl SharedCount {
    fn new(count: u32) -> Self {
        Self(Arc::new(AtomicU32::new(count)))
    }

    fn get(&self) -> u32 {
        self.0.load(Ordering::Relaxed)
    }

    fn set(&self, count: u32) {
        self.0.store(count, Ordering::Relaxed);
    }
}

/// `WeenieError::BadMovementCommand` in ACE's numbering. The client's callers treat any non-zero
/// as failure.
pub const BAD_MOVEMENT_COMMAND: u32 = 0x43;
/// `NoAnimationTable`.
pub const NO_ANIMATION_TABLE: u32 = 7;

/// `MotionTableManager` — the per-object driver.
#[derive(Debug)]
pub struct MotionTableManager {
    table: Option<MotionTable>,
    pub state: MotionState,
    /// Animations completed but not yet attributed to a queued motion.
    animation_counter: u32,
    pending: VecDeque<PendingAnimation>,
}

impl Clone for MotionTableManager {
    fn clone(&self) -> Self {
        // Driver/table clones own independent mutable state, unlike a callback's temporary
        // reference to one logical node. Never share count cells between cloned managers.
        Self {
            table: self.table.clone(),
            state: self.state.clone(),
            animation_counter: self.animation_counter,
            pending: self
                .pending
                .iter()
                .map(|n| PendingAnimation::new(n.motion, n.count.get()))
                .collect(),
        }
    }
}

impl MotionTableManager {
    #[must_use]
    pub fn new(table: Option<Arc<MotionTableData>>) -> Self {
        Self {
            table: table.map(MotionTable::new),
            state: MotionState::new(),
            animation_counter: 0,
            pending: VecDeque::new(),
        }
    }

    #[must_use]
    pub fn table(&self) -> Option<&MotionTable> {
        self.table.as_ref()
    }

    /// Release the old table and take the new.
    pub fn set_table(&mut self, table: Option<Arc<MotionTableData>>) {
        self.table = table.map(MotionTable::new);
    }

    #[must_use]
    pub fn pending(&self) -> VecDeque<AnimNode> {
        self.pending
            .iter()
            .map(PendingAnimation::snapshot)
            .collect()
    }

    /// Nonallocating live ledger probes; `pending()` is a detached diagnostic snapshot.
    #[must_use]
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    #[must_use]
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Initialise the motion state.
    ///
    /// Note the queued motion is `Ready`, not the style's default substate: the ledger is keyed by
    /// what the *caller* asked for, and `enter_default_state` asks for `Ready`.
    pub fn initialize_state(&mut self, seq: &mut Sequence, assets: &dyn AnimAssets) {
        let mut n = 0;
        if let Some(t) = &self.table {
            if let Some(k) = t.set_default_state(&mut self.state, seq, assets) {
                n = k;
            }
        }
        self.pending
            .push_back(PendingAnimation::new(MotionCommand::READY, n));
        self.remove_redundant_links(seq);
    }

    /// `Ok(())` is the client's 0.
    pub fn perform_movement(
        &mut self,
        ms: &MovementStruct,
        seq: &mut Sequence,
        assets: &dyn AnimAssets,
    ) -> Result<(), u32> {
        let Some(table) = self.table.clone() else {
            return Err(NO_ANIMATION_TABLE);
        };
        match ms.kind {
            MovementType::InterpretedCommand => {
                let n = table
                    .do_object_motion(ms.motion, &mut self.state, seq, ms.speed, assets)
                    .ok_or(BAD_MOVEMENT_COMMAND)?;
                self.add_to_queue(ms.motion, n, seq);
                Ok(())
            }
            MovementType::StopInterpretedCommand => {
                let n = table
                    .stop_object_motion(ms.motion, ms.speed, &mut self.state, seq, assets)
                    .ok_or(BAD_MOVEMENT_COMMAND)?;
                self.add_to_queue(MotionCommand::READY, n, seq);
                Ok(())
            }
            MovementType::StopCompletely => {
                let n = table.stop_object_completely(&mut self.state, seq, assets);
                self.add_to_queue(MotionCommand::READY, n, seq);
                Ok(())
            }
            // UNVERIFIED: the unknown-type result is unresolved. The client's default arm appears
            // to return a sequence pointer as an unsigned integer,
            // probably an unreachable path. The caller returns
            // `0x47 GeneralMovementFailure` for an unknown type; that remains the approximation here.
            _ => Err(0x47),
        }
    }

    /// Queue one motion.
    pub fn add_to_queue(&mut self, motion: MotionCommand, num_anims: u32, seq: &mut Sequence) {
        self.pending
            .push_back(PendingAnimation::new(motion, num_anims));
        self.remove_redundant_links(seq);
    }

    /// Remove the redundant links.
    ///
    /// Collapses an A→B→A flicker. Starting from the tail, skip the nodes that queued no
    /// animations to find the last node that did; call its motion `M`. Then, depending on `M`'s
    /// class, walk backwards for an earlier node with the same motion, stopping the search at any
    /// intervening node that queued animations and whose motion has one of the interruption bits.
    ///
    /// The two masks differ and both are literal: `0xB0000000` (style | modifier | action) for a
    /// cycle, `0x70000000` (substate | modifier | action) for a style.
    pub fn remove_redundant_links(&mut self, seq: &mut Sequence) {
        // The last node that actually queued animations.
        let mut last = None;
        for i in (0..self.pending.len()).rev() {
            if self.pending[i].count.get() != 0 {
                last = Some(i);
                break;
            }
        }
        let Some(last) = last else {
            return;
        };
        let m = self.pending[last].motion;

        let found = if m.is_substate() && !m.is_modifier() {
            self.scan_back(last, m, 0xB000_0000)
        } else if m.is_style() {
            self.scan_back(last, m, 0x7000_0000)
        } else {
            None
        };
        if let Some(i) = found {
            self.truncate_animation_list(i, seq);
        }
    }

    /// The backwards walk shared by both arms of `remove_redundant_links`. The cycle arm also
    /// requires the earlier node to have queued animations; the style arm does not.
    fn scan_back(&self, from: usize, motion: MotionCommand, stop_mask: u32) -> Option<usize> {
        let require_anims = stop_mask == 0xB000_0000;
        for i in (0..from).rev() {
            let node = &self.pending[i];
            if node.motion == motion && (!require_anims || node.count.get() != 0) {
                return Some(i);
            }
            if node.count.get() != 0 && node.motion.0 & stop_mask != 0 {
                return None;
            }
        }
        None
    }

    /// Sum the `num_anims` of every node
    /// **after** `node`, zero them, and remove exactly that many queued link animations so the
    /// object snaps back to the state it was already in.
    fn truncate_animation_list(&mut self, node: usize, seq: &mut Sequence) {
        let mut total = 0u32;
        for i in (node + 1)..self.pending.len() {
            total = total.wrapping_add(self.pending[i].count.get());
            self.pending[i].count.set(0);
        }
        seq.remove_link_animations(total);
    }

    // ---------------------------------------------------------------------------------------
    // The event-only table adapters are test-only as well as crate-private.
    // Production MotionOwner uses the begin/finish primitives below to run the actual
    // completion callback BEFORE reloading/popping
    // the table head. Draining events first would lose recursive mutations of either ledger.
    // The public isolated-interpreter and full MovementManager adapters share that owner.
    // A test pins the boundaries and the callback-before-continuation order.
    // ---------------------------------------------------------------------------------------

    /// driven by the `AnimationDone` hook.
    #[cfg(test)]
    pub(crate) fn animation_done(&mut self, success: bool, out: &mut Vec<AnimEvent>) {
        if !self.begin_animation_done() {
            return;
        }
        while let Some(head) = self.begin_completion(true) {
            out.push(AnimEvent::MotionDone {
                motion: head.motion,
                success,
            });
            self.finish_completion(Some(&head));
        }
        self.finish_animation_done();
    }

    /// The completed-motion check and the per-frame tick --
    /// identical bodies. They drain every head node whose `num_anims == 0`, i.e. the motions that
    /// queued no transition animation and are therefore instantly complete.
    #[cfg(test)]
    pub(crate) fn check_for_completed_motions(&mut self, out: &mut Vec<AnimEvent>) {
        while let Some(head) = self.begin_completion(false) {
            out.push(AnimEvent::MotionDone {
                motion: head.motion,
                success: true,
            });
            self.finish_completion(None);
        }
    }

    /// The animation-done step increments only when a pending head exists.
    pub(crate) fn begin_animation_done(&mut self) -> bool {
        if self.pending.is_empty() {
            return false;
        }
        self.animation_counter = self.animation_counter.wrapping_add(1);
        true
    }

    /// Stop immediately BEFORE the physics-owner callback.
    pub(crate) fn begin_completion(&mut self, animation_done: bool) -> Option<PendingAnimation> {
        let head = self.pending.front()?.clone();
        if if animation_done {
            self.animation_counter < head.count.get()
        } else {
            head.count.get() != 0
        } {
            return None;
        }
        if head.motion.is_action() {
            self.state.remove_action_head();
        }
        Some(head)
    }

    /// Resume AFTER the owner callback. Count belongs to the original node,
    /// but the pop always reloads the CURRENT head. No identity guard.
    pub(crate) fn finish_completion(&mut self, original: Option<&PendingAnimation>) {
        if let Some(original) = original {
            self.animation_counter = self.animation_counter.wrapping_sub(original.count.get());
        }
        self.pending.pop_front();
    }

    pub(crate) fn finish_animation_done(&mut self) {
        if self.pending.is_empty() {
            self.animation_counter = 0;
        }
    }

    /// The per-frame tick.
    #[cfg(test)]
    pub(crate) fn use_time(&mut self, out: &mut Vec<AnimEvent>) {
        self.check_for_completed_motions(out);
    }

    /// Drain the whole queue with
    /// `AnimationDone(0)`.
    #[cfg(test)]
    pub(crate) fn handle_exit_world(&mut self, out: &mut Vec<AnimEvent>) {
        while !self.pending.is_empty() {
            self.animation_done(false, out);
        }
    }

    /// Remove the link animations.
    #[cfg(test)]
    pub(crate) fn remove_link_animations(&mut self, seq: &mut Sequence, out: &mut Vec<AnimEvent>) {
        seq.remove_all_link_animations();
        self.handle_exit_world(out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::NoAssets;

    #[test]
    fn isolated_table_adapters_retain_use_time_and_remove_link_semantics() {
        let mut table = MotionTableManager::new(None);
        let mut seq = Sequence::new();
        let mut events = Vec::new();
        table.add_to_queue(MotionCommand::READY, 0, &mut seq);
        table.add_to_queue(MotionCommand::WAVE, 1, &mut seq);
        table.use_time(&mut events);
        assert_eq!(table.pending_len(), 1);
        table.remove_link_animations(&mut seq, &mut events);
        assert!(!table.has_pending());
        assert_eq!(
            events,
            vec![
                AnimEvent::MotionDone {
                    motion: MotionCommand::READY,
                    success: true
                },
                AnimEvent::MotionDone {
                    motion: MotionCommand::WAVE,
                    success: false
                },
            ]
        );
    }

    /// Retail reads the original node after the callback, not a count copied before the call.
    /// The count write is an explicit constructed callback-state perturbation; this does
    /// not establish a retail journey that edits a live head's count.
    #[test]
    fn callback_continuation_reads_the_retained_originals_live_count() {
        let mut m = mgr(&[(MotionCommand::WAVE, 1), (MotionCommand::READY, 0)]);
        assert!(m.begin_animation_done());
        let original = m.begin_completion(true).expect("one completed animation");
        original.count.set(0);
        m.finish_completion(Some(&original));
        assert_eq!(
            m.animation_counter, 1,
            "use live original count, not pre-callback one"
        );
        assert_eq!(m.pending()[0].motion, MotionCommand::READY);
    }

    /// Recursive AnimationDone removes the original allocation. Its logical count survives
    /// safely here; reproducing retail's freed-memory/allocator reuse is explicitly excluded.
    #[test]
    fn callback_continuation_safely_retains_a_recursively_removed_original() {
        let mut m = mgr(&[(MotionCommand::WAVE, 1)]);
        assert!(m.begin_animation_done());
        let original = m.begin_completion(true).expect("outer completion");
        let mut nested = Vec::new();
        m.animation_done(false, &mut nested);
        assert!(m.pending.is_empty());
        assert_eq!(motions(&nested), vec![MotionCommand::WAVE]);
        m.finish_completion(Some(&original));
        assert_eq!(
            m.animation_counter,
            u32::MAX,
            "native wrapping subtraction before empty reset"
        );
        m.finish_animation_done();
        assert_eq!(m.animation_counter, 0);
    }

    /// The completed-motion check reloads the CURRENT head after callback; no old-node
    /// identity guard may protect a newly installed head, even one with animations remaining.
    #[test]
    fn callback_continuation_pops_the_replacement_current_head_without_identity_protection() {
        let mut m = mgr(&[(MotionCommand::READY, 0)]);
        let original = m.begin_completion(false).expect("zero-link original");
        let mut nested = Vec::new();
        m.check_for_completed_motions(&mut nested);
        m.pending
            .push_back(PendingAnimation::new(MotionCommand::WALK_FORWARD, 3));
        m.pending
            .push_back(PendingAnimation::new(MotionCommand::TURN_RIGHT, 2));
        m.finish_completion(None);
        assert_eq!(original.motion, MotionCommand::READY);
        assert_eq!(
            m.pending()
                .iter()
                .map(|n| (n.motion, n.num_anims))
                .collect::<Vec<_>>(),
            vec![(MotionCommand::TURN_RIGHT, 2)]
        );
    }

    #[test]
    fn cloned_managers_do_not_share_live_pending_counts() {
        let m = mgr(&[(MotionCommand::WAVE, 1)]);
        let cloned = m.clone();
        cloned.pending[0].count.set(7);
        assert_eq!(m.pending()[0].num_anims, 1);
        assert_eq!(cloned.pending()[0].num_anims, 7);
    }

    fn mgr(nodes: &[(MotionCommand, u32)]) -> MotionTableManager {
        let mut m = MotionTableManager::new(None);
        for (motion, num_anims) in nodes {
            m.pending
                .push_back(PendingAnimation::new(*motion, *num_anims));
        }
        m
    }

    fn motions(out: &[AnimEvent]) -> Vec<MotionCommand> {
        out.iter()
            .filter_map(|e| match e {
                AnimEvent::MotionDone { motion, .. } => Some(*motion),
                _ => None,
            })
            .collect()
    }

    /// ORACLE: the recovered motion-table behavior section 6,
    /// the redundant-link removal and the animation-list truncation.
    ///
    /// The A→B→A cycle flicker: walking, standing, walking again collapses back to the first walk
    /// and un-queues the animations the second transition added.
    #[test]
    fn remove_redundant_links_collapses_a_cycle_flicker() {
        let walk = MotionCommand::WALK_FORWARD;
        let ready = MotionCommand::READY;
        let mut m = mgr(&[(walk, 1), (ready, 1), (walk, 2)]);
        let mut seq = Sequence::new();
        m.remove_redundant_links(&mut seq);
        // The earlier `walk` is found; everything after it is zeroed.
        assert_eq!(
            m.pending().iter().map(|n| n.num_anims).collect::<Vec<_>>(),
            vec![1, 0, 0]
        );
    }

    /// The interruption mask stops the search: a style change between the two cycles means the
    /// transition is not redundant.
    #[test]
    fn an_intervening_style_change_blocks_the_cycle_collapse() {
        let walk = MotionCommand::WALK_FORWARD;
        let mut m = mgr(&[(walk, 1), (MotionCommand::SWORD_COMBAT, 2), (walk, 3)]);
        let mut seq = Sequence::new();
        m.remove_redundant_links(&mut seq);
        assert_eq!(
            m.pending().iter().map(|n| n.num_anims).collect::<Vec<_>>(),
            vec![1, 2, 3],
            "0xB0000000 includes the style bit"
        );
        // A node with `num_anims == 0` does not block, because the mask test is conditional on it.
        let mut m = mgr(&[(walk, 1), (MotionCommand::SWORD_COMBAT, 0), (walk, 3)]);
        m.remove_redundant_links(&mut seq);
        assert_eq!(
            m.pending().iter().map(|n| n.num_anims).collect::<Vec<_>>(),
            vec![1, 0, 0]
        );
    }

    /// The style arm uses `0x70000000`, so an intervening *cycle* blocks a style collapse.
    #[test]
    fn the_style_arm_uses_a_different_mask() {
        let combat = MotionCommand::SWORD_COMBAT;
        let mut seq = Sequence::new();
        let mut m = mgr(&[(combat, 1), (MotionCommand::NON_COMBAT, 2), (combat, 3)]);
        m.remove_redundant_links(&mut seq);
        assert_eq!(
            m.pending().iter().map(|n| n.num_anims).collect::<Vec<_>>(),
            vec![1, 0, 0],
            "an intervening style does NOT block a style collapse: 0x70000000 has bit 31 clear"
        );
        // A walk between the two style changes *does* block: it has bit 30.
        let mut m = mgr(&[(combat, 1), (MotionCommand::WALK_FORWARD, 2), (combat, 3)]);
        m.remove_redundant_links(&mut seq);
        assert_eq!(
            m.pending().iter().map(|n| n.num_anims).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
    }

    /// The completion ledger fires `MotionDone` exactly once per queued motion, pops the action
    /// head for a `0x1xxxxxxx` motion, and never fires for a pure cycle (whose `num_anims` covers
    /// only the link animations).
    #[test]
    fn the_ledger_fires_motion_done_once_per_queued_motion() {
        let mut m = mgr(&[(MotionCommand::WALK_FORWARD, 2), (MotionCommand::CHEER, 1)]);
        m.state.add_action(MotionCommand::CHEER, 1.0);
        let mut out = Vec::new();

        m.animation_done(true, &mut out);
        assert!(
            motions(&out).is_empty(),
            "one of the two link animations has finished"
        );
        m.animation_done(true, &mut out);
        assert_eq!(motions(&out), vec![MotionCommand::WALK_FORWARD]);
        assert_eq!(m.pending().len(), 1);

        out.clear();
        m.animation_done(true, &mut out);
        assert_eq!(motions(&out), vec![MotionCommand::CHEER]);
        assert!(m.pending().is_empty());
        assert_eq!(m.state.num_actions(), 0, "the action head was popped");
    }

    /// A pure cycle queues a node with `num_anims == 0`, which the *other* path drains: it is
    /// instantly complete and never waits for an `AnimationDone`.
    #[test]
    fn a_zero_length_motion_completes_through_check_for_completed_motions() {
        let mut m = mgr(&[(MotionCommand::READY, 0), (MotionCommand::WALK_FORWARD, 2)]);
        let mut out = Vec::new();
        m.check_for_completed_motions(&mut out);
        assert_eq!(motions(&out), vec![MotionCommand::READY]);
        assert_eq!(
            m.pending().len(),
            1,
            "the walk is still waiting for its animations"
        );
        out.clear();
        m.check_for_completed_motions(&mut out);
        assert!(out.is_empty());
    }

    /// No table at all is error 7, `NoAnimationTable`.
    #[test]
    fn perform_movement_without_a_table_is_error_seven() {
        let mut m = MotionTableManager::new(None);
        let mut seq = Sequence::new();
        let r = m.perform_movement(
            &MovementStruct {
                kind: MovementType::InterpretedCommand,
                motion: MotionCommand::WALK_FORWARD,
                speed: 1.0,
            },
            &mut seq,
            &NoAssets,
        );
        assert_eq!(r, Err(NO_ANIMATION_TABLE));
    }

    /// `HandleExitWorld` drains the queue with `success = false`.
    #[test]
    fn handle_exit_world_reports_every_pending_motion_as_failed() {
        let mut m = mgr(&[(MotionCommand::WALK_FORWARD, 2), (MotionCommand::READY, 1)]);
        let mut out = Vec::new();
        m.handle_exit_world(&mut out);
        assert_eq!(
            motions(&out),
            vec![MotionCommand::WALK_FORWARD, MotionCommand::READY]
        );
        assert!(out
            .iter()
            .all(|e| matches!(e, AnimEvent::MotionDone { success: false, .. })));
        assert!(m.pending().is_empty());
    }
}
