// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Actions/LoopAction.cs
//! Port of `Source/ACE.Server/Entity/Actions/LoopAction.cs`.
//!
//! ACE's loop is a cycle of shared objects: the body chain ends with the `LoopAction` itself, and
//! the same body actions are acted again on every iteration. Here actions are owned and consumed
//! when they run, so the body is a builder called once per iteration; the loop action appends
//! itself to the fresh body and travels round the cycle by value. The sequence of actions, actors
//! and condition checks is ACE's (DIVERGENCES.md, the loop-body row).

use std::fmt;

use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::action_event_base::ActionEventBase;
use crate::entity::actions::conditional_action::Predicate;
use crate::entity::actions::i_action::{Action, NextAct};
use crate::entity::actions::i_actor::Actor;
use crate::World;

/// Builds one iteration's body chain.
pub type LoopBody = Box<dyn FnMut(&mut World) -> ActionChain + Send>;

/// `LoopAction`: while the condition holds, runs the body and comes back; then continues with
/// `NextAct`.
pub struct LoopAction {
    pub base: ActionEventBase,
    /// `Condition`.
    pub condition: Predicate,
    /// `Body`, as a builder.
    pub body: LoopBody,
    /// The `conditionActor` the constructor appends the loop to the body with.
    condition_actor: Actor,
}

impl fmt::Debug for LoopAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LoopAction")
            .field("base", &self.base)
            .field("condition_actor", &self.condition_actor)
            .finish_non_exhaustive()
    }
}

impl LoopAction {
    // ACE: LoopAction.LoopAction
    /// `new LoopAction(conditionActor, condition, body)`. ACE appends `(conditionActor, this)` to
    /// the body here; this port appends it to each iteration's body in [`act`](Self::act).
    pub fn new(condition_actor: Actor, condition: Predicate, body: LoopBody) -> Self {
        LoopAction {
            base: ActionEventBase::new(),
            condition,
            body,
            condition_actor,
        }
    }

    // ACE: LoopAction.Act
    pub fn act(mut self, w: &mut World) -> Option<NextAct> {
        if (self.condition)(w) {
            // DIVERGE: the body is rebuilt per iteration; owned FnOnce actions cannot be re-acted.
            let mut body = (self.body)(w);
            let condition_actor = self.condition_actor;
            body.add_iaction(condition_actor, Action::Loop(self));
            return body.into_first();
        }

        self.base.act()
    }
}
