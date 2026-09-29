//! `MotionState` and its two lists.
//!
//! One singly linked node, `{ ulong motion; float speed_mod; next }`, serves as both lists. The
//! modifier list is **LIFO** ( pushes at the head) and the action
//! list is **FIFO** ( appends at the tail). Both orders are observable:
//! `re_modify` walks the modifier list head-first and the action ledger pops the action head.

use std::collections::VecDeque;

use crate::command::MotionCommand;

/// One entry of a motion list.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionEntry {
    pub motion: MotionCommand,
    pub speed_mod: f32,
}

/// The per-object motion state — 24 bytes in the client.
#[derive(Debug, Clone, PartialEq)]
pub struct MotionState {
    /// The current stance (`0x8xxxxxxx`).
    pub style: MotionCommand,
    /// The current cycle (`0x4xxxxxxx`).
    pub substate: MotionCommand,
    /// The current speed multiplier of the cycle. **May be negative** — that is how a reversed
    /// cycle is expressed.
    pub substate_mod: f32,
    /// Active modifiers, head first. LIFO.
    pub modifiers: Vec<MotionEntry>,
    /// One-shot actions in flight. FIFO.
    pub actions: VecDeque<MotionEntry>,
}

impl Default for MotionState {
    /// Style 0, substate 0, `substate_mod = 1.0`, both
    /// lists empty. Note style 0 is **not** `MotionCommand::INVALID`; `GetObjectSequence` refuses
    /// outright when `style == 0`.
    fn default() -> Self {
        Self {
            style: MotionCommand::NONE,
            substate: MotionCommand::NONE,
            substate_mod: 1.0,
            modifiers: Vec::new(),
            actions: VecDeque::new(),
        }
    }
}

impl MotionState {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Refuses a duplicate motion and refuses a modifier
    /// equal to the current substate. Returns false when it refused.
    pub fn add_modifier(&mut self, motion: MotionCommand, speed_mod: f32) -> bool {
        if self.modifiers.iter().any(|m| m.motion == motion) {
            return false;
        }
        if self.substate == motion {
            return false;
        }
        self.add_modifier_no_check(motion, speed_mod);
        true
    }

    /// Push at the **head**.
    pub fn add_modifier_no_check(&mut self, motion: MotionCommand, speed_mod: f32) {
        self.modifiers.insert(0, MotionEntry { motion, speed_mod });
    }

    /// Remove one modifier -- unlink one entry.
    pub fn remove_modifier_at(&mut self, index: usize) -> Option<MotionEntry> {
        if index < self.modifiers.len() {
            Some(self.modifiers.remove(index))
        } else {
            None
        }
    }

    /// Remove the first entry with this motion, if any.
    pub fn remove_modifier(&mut self, motion: MotionCommand) -> Option<MotionEntry> {
        let i = self.modifiers.iter().position(|m| m.motion == motion)?;
        Some(self.modifiers.remove(i))
    }

    pub fn clear_modifiers(&mut self) {
        self.modifiers.clear();
    }

    /// Append at the **tail**.
    pub fn add_action(&mut self, motion: MotionCommand, speed_mod: f32) {
        self.actions.push_back(MotionEntry { motion, speed_mod });
    }

    /// Pop and return the motion, or 0.
    pub fn remove_action_head(&mut self) -> MotionCommand {
        self.actions
            .pop_front()
            .map_or(MotionCommand::NONE, |e| e.motion)
    }

    pub fn clear_actions(&mut self) {
        self.actions.clear();
    }

    #[must_use]
    pub fn num_actions(&self) -> usize {
        self.actions.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ORACLE: the client's own motion-primitive code -- the modifier add, the unchecked
    /// modifier add, the action add and the action-head removal.
    #[test]
    fn modifiers_are_lifo_and_actions_are_fifo() {
        let mut s = MotionState::new();
        assert!(s.add_modifier(MotionCommand::STOP_TURNING, 1.0));
        assert!(s.add_modifier(MotionCommand::JUMP, 2.0));
        assert_eq!(
            s.modifiers[0].motion,
            MotionCommand::JUMP,
            "pushed at the head"
        );
        assert_eq!(s.modifiers[1].motion, MotionCommand::STOP_TURNING);

        s.add_action(MotionCommand::CHEER, 1.0);
        s.add_action(MotionCommand::HOP, 1.0);
        assert_eq!(
            s.remove_action_head(),
            MotionCommand::CHEER,
            "appended at the tail"
        );
        assert_eq!(s.remove_action_head(), MotionCommand::HOP);
        assert_eq!(
            s.remove_action_head(),
            MotionCommand::NONE,
            "an empty list returns 0"
        );
    }

    /// `add_modifier` refuses a duplicate and refuses the current substate.
    #[test]
    fn add_modifier_refuses_a_duplicate_and_the_current_substate() {
        let mut s = MotionState {
            substate: MotionCommand::READY,
            ..MotionState::new()
        };
        assert!(s.add_modifier(MotionCommand::JUMP, 1.0));
        assert!(!s.add_modifier(MotionCommand::JUMP, 1.0), "duplicate");
        assert!(
            !s.add_modifier(MotionCommand::READY, 1.0),
            "equal to the substate"
        );
        assert_eq!(s.modifiers.len(), 1);
        // `add_modifier_no_check` skips both tests, which is what `GetObjectSequence` branch (B)
        // relies on when it remembers the modifier-class substate it is leaving.
        s.add_modifier_no_check(MotionCommand::JUMP, 1.0);
        assert_eq!(s.modifiers.len(), 2);
    }

    /// The default state starts at speed one with no style.
    #[test]
    fn the_default_state_starts_at_speed_one_with_no_style() {
        let s = MotionState::default();
        assert_eq!(s.style, MotionCommand::NONE);
        assert_eq!(s.substate, MotionCommand::NONE);
        assert_eq!(s.substate_mod, 1.0);
        assert!(s.modifiers.is_empty() && s.actions.is_empty());
    }
}
