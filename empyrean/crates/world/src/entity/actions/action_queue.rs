// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Actions/ActionQueue.cs
//! Port of `Source/ACE.Server/Entity/Actions/ActionQueue.cs`.
//!
//! # Running a queue that lives inside the world
//!
//! ACE's `RunActions` reads `Queue.Count` once at entry and then dequeues at most that many
//! actions. Anything enqueued while it runs (by an action, or as an action's `NextAct`) goes to the
//! back of the same queue and runs on the **next** call. Actions need `&mut World`, and the queue is
//! part of the world, so the queue cannot be borrowed across an action.
//!
//! The design is **run by count, re-borrowing per action**. The queue never leaves the world.
//! [`run_actions`] takes the owner ([`Actor`]) instead of the queue and, for each of the `count`
//! steps, resolves the queue through [`action_queue_mut`], pops the front (`TryDequeue`), releases
//! the borrow, runs the action with the whole world, and routes its `NextAct` through
//! [`enqueue`]. Because only `count` pops happen, work added during the run is left in the live
//! queue for the next run, exactly as ACE's snapshot behaves. It also keeps two further ACE
//! behaviours that taking the queue out and merging it back would lose:
//!
//! - an action that calls `Clear()` on the running queue removes the actions still waiting, and
//!   the remaining steps find nothing to dequeue and do nothing (ACE's failed `TryDequeue`);
//! - an action enqueued during the run lands behind everything that was already waiting.
//!
//! Each action runs inside its own `catch_unwind` ([`act_caught`]): a panicking action is logged
//! and counted ([`ActionQueue::act_exceptions`]), its `NextAct` is lost with it, and the run goes
//! on with the next action.
//! DIVERGE: ACE catches nothing here; an exception escaping `Act()` leaves `RunActions`, and being
//! unhandled on the world thread, ends the process. The loop's per-stage guard alone would defer
//! the rest of the queue by a tick; this guard keeps it in the same pass.
//!
//! One difference is left for the units that add landblock and object queues: if the queue's owner
//! disappears in the middle of a run, the remaining steps find no queue and stop, where ACE would
//! keep draining the orphaned queue object it still holds.

use std::collections::VecDeque;
use std::panic::{catch_unwind, AssertUnwindSafe};

use crate::entity::actions::i_action::Action;
use crate::entity::actions::i_actor::{action_queue_mut, enqueue, Actor};
use crate::World;

// ACE: ActionQueue
/// `ActionQueue`: a FIFO of actions, run once per owner tick.
#[derive(Debug, Default)]
pub struct ActionQueue {
    /// `Queue`.
    queue: VecDeque<Action>,
    /// Not ACE: actions run from this queue that panicked (see [`run_actions`]).
    act_exceptions: u64,
}

impl ActionQueue {
    pub fn new() -> Self {
        Self::default()
    }

    // ACE: ActionQueue.EnqueueAction
    pub fn enqueue_action(&mut self, action: Action) {
        self.queue.push_back(action);
    }

    // ACE: ActionQueue.Clear
    pub fn clear(&mut self) {
        self.queue.clear();
    }

    /// `Queue.Count`.
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// `Queue.IsEmpty`.
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Not ACE: how many actions run from this queue have panicked.
    pub fn act_exceptions(&self) -> u64 {
        self.act_exceptions
    }

    /// `Queue.TryDequeue`.
    fn try_dequeue(&mut self) -> Option<Action> {
        self.queue.pop_front()
    }
}

// ACE: ActionQueue.RunActions
/// Runs the actions present in `owner`'s queue at entry (see the module docs). `owner` is the actor
/// whose queue this is; an owner with no queue runs nothing.
pub fn run_actions(w: &mut World, owner: Actor) {
    let Some(queue) = action_queue_mut(w, owner) else {
        return;
    };

    if queue.is_empty() {
        return;
    }

    let count = queue.len();

    for _ in 0..count {
        let result = action_queue_mut(w, owner).and_then(ActionQueue::try_dequeue);
        if let Some(result) = result {
            // DIVERGE: one action's panic is caught here (see the module docs).
            let panicked = !act_caught(w, "ActionQueue", |w| {
                let enqueue_next = result.act(w);

                if let Some((actor, action)) = enqueue_next {
                    enqueue(w, actor, action);
                }
            });
            if panicked {
                if let Some(queue) = action_queue_mut(w, owner) {
                    queue.act_exceptions += 1;
                }
            }
        }
    }
}

/// Not ACE: runs one action's `Act()` and the enqueue of its `NextAct` inside `catch_unwind`.
/// Returns `false`, after logging the panic as the world's unhandled-exception handler would, if
/// it panicked. `owner` names the queue in the log line.
pub(crate) fn act_caught(w: &mut World, owner: &str, f: impl FnOnce(&mut World)) -> bool {
    match catch_unwind(AssertUnwindSafe(|| f(&mut *w))) {
        Ok(()) => true,
        Err(panic) => {
            let message = panic
                .downcast_ref::<&str>()
                .map(|s| (*s).to_owned())
                .or_else(|| panic.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "panic".to_owned());
            log::error!("{owner} Act() threw an exception: {message}");
            false
        }
    }
}
