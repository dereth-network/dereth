// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Actions/ConditionalAction.cs
//! Port of `Source/ACE.Server/Entity/Actions/ConditionalAction.cs`.

use std::fmt;

use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_action::{Action, NextAct};
use crate::entity::actions::i_actor::Actor;
use crate::World;

/// ACE's `Func<bool>` condition, evaluated on the condition actor's queue.
pub type Predicate = Box<dyn FnMut(&mut World) -> bool + Send>;

/// `ConditionalAction`: evaluates a condition and continues with one of two chains.
pub struct ConditionalAction {
    /// `Condition`.
    pub condition: Predicate,
    /// `TrueChain`.
    pub true_chain: ActionChain,
    /// `FalseChain`.
    pub false_chain: ActionChain,
    /// What `RunOnFinish` gave. ACE appends it to both chains at once; one owned continuation
    /// cannot sit in both, so it is held here and appended to whichever chain `Act` picks, which
    /// yields the same sequence of actions.
    on_finish: Option<Box<NextAct>>,
}

impl fmt::Debug for ConditionalAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConditionalAction")
            .field("true_chain", &self.true_chain)
            .field("false_chain", &self.false_chain)
            .field("on_finish", &self.on_finish)
            .finish_non_exhaustive()
    }
}

impl ConditionalAction {
    // ACE: ConditionalAction.ConditionalAction
    pub fn new(condition: Predicate, true_chain: ActionChain, false_chain: ActionChain) -> Self {
        ConditionalAction {
            condition,
            true_chain,
            false_chain,
            on_finish: None,
        }
    }

    // ACE: ConditionalAction.RunOnFinish
    pub fn run_on_finish(&mut self, actor: Actor, action: Action) {
        self.on_finish = Some(Box::new((actor, action)));
    }

    // ACE: ConditionalAction.Act
    /// Returns the chosen chain's first element (with the rest of that chain, then this action's
    /// continuation, behind it).
    ///
    /// # Panics
    /// When the chosen chain is empty and nothing follows the conditional: ACE dereferences a null
    /// `FirstElement` there.
    pub fn act(mut self, w: &mut World) -> Option<NextAct> {
        let cond = (self.condition)(w);

        let mut chain = if cond {
            self.true_chain
        } else {
            self.false_chain
        };
        if let Some(next) = self.on_finish.take() {
            let (actor, action) = *next;
            chain.add_iaction(actor, action);
        }

        // ACE-BUG: an empty branch with nothing after the conditional throws
        // NullReferenceException on FirstElement.
        let first = chain
            .into_first()
            .expect("NullReferenceException: ConditionalAction.Act on an empty chain");
        Some(first)
    }
}
