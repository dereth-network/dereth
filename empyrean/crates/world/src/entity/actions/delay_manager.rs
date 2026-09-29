// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Actions/DelayManager.cs
//! Port of `Source/ACE.Server/Entity/Actions/DelayManager.cs`.
//!
//! ACE's `SortedSet<DelayAction>` ordered by `DelayAction.CompareTo`, `(EndTime, sequence)`, is a
//! `BTreeSet<DelayAction>` with the same order. `EndTime` is fixed when the action is enqueued
//! (`DelayAction.Start`), never when it is built.
//!
//! [`run_actions`] keeps ACE's shape exactly, including one detail worth knowing: ACE's
//! `checkNeeded` is set to `false` at the top of the loop and never set back to `true`, so the
//! outer loop runs **once**. Every delay due at entry runs, in order; a delay that an action
//! enqueues during the pass waits for the next call even if it is already due.
//!
//! Each due delay's `Act()`, with the enqueue of its `NextAct`, runs inside its own
//! `catch_unwind`: a panic is logged and counted ([`DelayManager::act_exceptions`]) and the pass
//! goes on, so no delay already taken off the heap is lost.
//! DIVERGE: ACE catches nothing here; an exception would leave `RunActions` and drop the rest of
//! `toAct` (and, unhandled on the world thread, end the process).

use std::collections::BTreeSet;

use crate::entity::actions::action_queue::act_caught;
use crate::entity::actions::delay_action::DelayAction;
use crate::entity::actions::i_action::Action;
use crate::entity::actions::i_actor::enqueue;
use crate::entity::timers;
use crate::World;

// ACE: DelayManager
/// `DelayManager` (one instance: `WorldManager.DelayManager`).
#[derive(Debug, Default)]
pub struct DelayManager {
    /// `delayHeap`.
    delay_heap: BTreeSet<DelayAction>,
    /// `DelayAction.glblSequence`. A process-wide static in ACE; held here so that each `World`
    /// numbers its delays from 1 and tests are deterministic.
    // DIVERGE: per-World counter instead of a process-wide static.
    pub(crate) glbl_sequence: i64,
    /// Not ACE: due delays whose acting panicked (see [`run_actions`]).
    act_exceptions: u64,
}

impl DelayManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Delays waiting to run.
    pub fn len(&self) -> usize {
        self.delay_heap.len()
    }

    pub fn is_empty(&self) -> bool {
        self.delay_heap.is_empty()
    }

    /// `delayHeap.Min`: the next delay to come due.
    pub fn min(&self) -> Option<&DelayAction> {
        self.delay_heap.first()
    }

    /// Not ACE: how many due delays have panicked while acting.
    pub fn act_exceptions(&self) -> u64 {
        self.act_exceptions
    }

    /// The last sequence number handed out (`DelayAction.glblSequence`).
    pub fn glbl_sequence(&self) -> i64 {
        self.glbl_sequence
    }
}

// ACE: DelayManager.RunActions
/// Runs every delay whose `EndTime <= Timers.PortalYearTicks`, in `(EndTime, sequence)` order, and
/// enqueues each one's `NextAct`.
pub fn run_actions(w: &mut World) {
    // While the minimum time of our delayHeap is > our current time, kick off actions
    let mut check_needed = true;

    while check_needed {
        check_needed = false;
        let mut to_act: Vec<DelayAction> = Vec::new();

        let now = timers::portal_year_ticks(w);
        let delay_heap = &mut w.world_manager.delay_manager.delay_heap;
        while let Some(min) = delay_heap.first() {
            // If they wanted to run before or at now
            if min.end_time() <= now {
                if let Some(min) = delay_heap.pop_first() {
                    to_act.push(min);
                }
            } else {
                break;
            }
        }

        for action in to_act {
            // DIVERGE: one delay's panic is caught here (see the module docs).
            let ok = act_caught(w, "DelayManager", |w| {
                let next = action.act();

                if let Some((actor, action)) = next {
                    enqueue(w, actor, action);
                }
            });
            if !ok {
                w.world_manager.delay_manager.act_exceptions += 1;
            }
        }
    }
}

// ACE: DelayManager.EnqueueAction
/// Starts a delay (fixing its `EndTime`) and holds it. Any other kind of action is logged and
/// dropped, as in ACE.
pub fn enqueue_action(w: &mut World, action: Action) {
    let Action::Delay(mut delay_action) = action else {
        log::error!("Non DelayAction IAction added to DelayManager");
        return;
    };

    delay_action.start(w);

    w.world_manager
        .delay_manager
        .delay_heap
        .insert(delay_action);
}
