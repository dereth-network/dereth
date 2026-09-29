// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Actions/DelayAction.cs
//! Port of `Source/ACE.Server/Entity/Actions/DelayAction.cs`.

use std::cmp::Ordering;

use empyrean_common::dotnet::sort::double_compare_to;

use crate::entity::actions::action_event_base::ActionEventBase;
use crate::entity::actions::i_action::NextAct;
use crate::entity::timers;
use crate::World;

/// `DelayAction`: an action that the delay manager holds until `Timers.PortalYearTicks >= EndTime`.
/// It must only be enqueued on [`Actor::Delay`](crate::entity::actions::i_actor::Actor::Delay).
#[derive(Debug)]
pub struct DelayAction {
    pub base: ActionEventBase,
    /// `WaitTime`, in Portal Year seconds.
    wait_time: f64,
    /// `EndTime`: 0 until [`start`](Self::start) runs, at enqueue.
    end_time: f64,
    /// `sequence`: breaks `EndTime` ties, so two delay actions never compare equal.
    sequence: i64,
}

impl DelayAction {
    // ACE: DelayAction.DelayAction
    /// `new DelayAction(waitTimePortalYearTicks)`. The sequence number is drawn now, at
    /// construction, from `DelayAction.glblSequence` (held in the world's `DelayManager`).
    pub fn new(w: &mut World, wait_time_portal_year_ticks: f64) -> Self {
        let dm = &mut w.world_manager.delay_manager;
        // Interlocked.Increment returns the incremented value: the first sequence is 1.
        dm.glbl_sequence = dm.glbl_sequence.wrapping_add(1);
        DelayAction {
            base: ActionEventBase::new(),
            wait_time: wait_time_portal_year_ticks,
            end_time: 0.0,
            sequence: dm.glbl_sequence,
        }
    }

    // ACE: DelayAction.WaitTime
    pub fn wait_time(&self) -> f64 {
        self.wait_time
    }

    // ACE: DelayAction.EndTime
    pub fn end_time(&self) -> f64 {
        self.end_time
    }

    // ACE: DelayAction.sequence
    pub fn sequence(&self) -> i64 {
        self.sequence
    }

    // ACE: DelayAction.Start
    pub fn start(&mut self, w: &World) {
        self.end_time = timers::portal_year_ticks(w) + self.wait_time;
    }

    /// The inherited `ActionEventBase.Act`.
    pub fn act(self) -> Option<NextAct> {
        self.base.act()
    }
}

impl Ord for DelayAction {
    // ACE: DelayAction.CompareTo
    fn cmp(&self, rhs: &Self) -> Ordering {
        let ret = double_compare_to(self.end_time, rhs.end_time).cmp(&0);

        if ret == Ordering::Equal {
            return self.sequence.cmp(&rhs.sequence);
        }

        ret
    }
}

impl PartialOrd for DelayAction {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for DelayAction {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for DelayAction {}
