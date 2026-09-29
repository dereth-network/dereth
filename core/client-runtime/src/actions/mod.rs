//! The action seam: how the runtime is told what the player is doing.
//!
//! The runtime takes **actions**, never device events. An `Action` is an id from the retail
//! action list with a phase (it began, repeated or ended) and an extent; a front end that owns a
//! keyboard turns key presses into them, a script or a bot builds them directly, and the handlers
//! below cannot tell the two apart. The vocabulary itself -- `ActionId`, `ActionPhase`,
//! `Action`, the name table -- is the shared contract's ([`dereth_client_contract::actions`]), so
//! a device pipeline that may not depend on the runtime can still produce them.
//!
//! Requests that are not an action on/off -- a chat line, "use this object" -- go in through the
//! interaction layer's request queue (`App::submit_requests`). The camera's mouse-look hold and
//! pointer motion go in through `App::mouse_look_button` and `App::cursor_moved`.
//!
//! # One frame's life of an action
//!
//! The client offers each action to one handler list, once. The list is split across the frame:
//! the front end's UI takes first refusal, then the movement interpreter and the camera controller
//! ([`crate::app::App`]'s input-action step), then the interaction layer's combat, selection and
//! UI-command arms. Each stage takes the queue and puts back what it declined, and whatever is
//! still in the queue when the next frame's UI step begins has been offered to every stage and
//! wanted by none: it is dropped and counted (`ActionStats::expired`), because an action the
//! client did not want is simply gone -- it is never offered again.

pub use dereth_client_contract::actions::{names, ui, Action, ActionId, ActionPhase};

/// The camera controller's action handler, mouse look and its preferences.
pub mod camera;
/// The emote actions' motion commands.
pub mod emote;
/// The movement interpreter's action handler and its three command lists.
pub mod movement;

/// What the queue has seen, with denominators.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ActionStats {
    /// Actions handed to the queue, from a front end or injected.
    pub submitted: u64,
    /// Actions still in the queue when a new frame began: offered to every stage and claimed by
    /// none, and therefore dropped.
    pub expired: u64,
}

/// The actions of the current frame, in dispatch order.
#[derive(Debug, Default)]
pub struct ActionQueue {
    /// This frame's actions, less what the stages so far have consumed.
    events: Vec<Action>,
    /// Actions injected since the last frame began, spliced in after that frame's expiry sweep.
    injected: Vec<Action>,
    stats: ActionStats,
}

impl ActionQueue {
    /// The start of a frame's action step: drop what the last frame left unclaimed, then take in
    /// what was injected since.
    pub fn begin_frame(&mut self) {
        let expired = self.events.len() as u64;
        if expired != 0 {
            self.stats.expired += expired;
            self.events.clear();
        }
        self.events.append(&mut self.injected);
    }

    /// This frame's actions, as a front end hands them on after its own first refusal.
    pub fn submit(&mut self, actions: impl IntoIterator<Item = Action>) {
        let before = self.events.len();
        self.events.extend(actions);
        self.stats.submitted += (self.events.len() - before) as u64;
    }

    /// An action for the **next** frame: it is spliced in after that frame's expiry sweep, so it is
    /// offered exactly once and then expires like any other. This is how a script or a test that
    /// has no device behind it acts.
    pub fn inject(&mut self, action: Action) {
        self.stats.submitted += 1;
        self.injected.push(action);
    }

    /// Take the queue, for a stage to offer each action to its handlers.
    pub fn take(&mut self) -> Vec<Action> {
        std::mem::take(&mut self.events)
    }

    /// Hand back what a stage declined, for the stages after it.
    pub fn put_back(&mut self, mut declined: Vec<Action>) {
        // Anything that arrived meanwhile is newer and stays newer.
        declined.append(&mut self.events);
        self.events = declined;
    }

    /// The actions the current frame still holds.
    #[must_use]
    pub fn pending(&self) -> &[Action] {
        &self.events
    }

    /// The counters.
    #[must_use]
    pub const fn stats(&self) -> ActionStats {
        self.stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FORWARD: ActionId = ActionId(0x29);
    const JUMP: ActionId = ActionId(0x31);

    #[test]
    fn an_unclaimed_action_is_offered_for_one_frame_and_then_counted_as_expired() {
        let mut q = ActionQueue::default();
        q.begin_frame();
        q.submit([Action::begin(FORWARD)]);
        let offered = q.take();
        assert_eq!(offered.len(), 1);
        q.put_back(offered);
        q.begin_frame();
        assert!(q.pending().is_empty(), "nothing is re-offered");
        assert_eq!(q.stats().expired, 1);
        assert_eq!(q.stats().submitted, 1);
    }

    #[test]
    fn an_injected_action_arrives_after_the_next_sweep_and_before_that_frames_submissions() {
        let mut q = ActionQueue::default();
        q.begin_frame();
        q.inject(Action::begin(JUMP));
        assert!(q.pending().is_empty(), "not this frame");
        q.begin_frame();
        q.submit([Action::begin(FORWARD)]);
        let ids: Vec<ActionId> = q.take().iter().map(|a| a.id).collect();
        assert_eq!(ids, [JUMP, FORWARD]);
        assert_eq!(q.stats().expired, 0);
    }

    #[test]
    fn a_put_back_keeps_the_declined_ahead_of_anything_newer() {
        let mut q = ActionQueue::default();
        q.submit([Action::begin(FORWARD)]);
        let taken = q.take();
        q.submit([Action::end(FORWARD)]);
        q.put_back(taken);
        let phases: Vec<ActionPhase> = q.take().iter().map(|a| a.phase).collect();
        assert_eq!(phases, [ActionPhase::Begin, ActionPhase::End]);
    }
}
