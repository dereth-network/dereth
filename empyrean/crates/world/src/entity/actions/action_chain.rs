// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Actions/ActionChain.cs
//! Port of `Source/ACE.Server/Entity/Actions/ActionChain.cs`.
//!
//! ACE links a chain as it is built: each `AddAction` calls `RunOnFinish` on the previous element,
//! so every action holds the next one. Here a chain is built as a list of [`ChainElement`]s and
//! linked when it is enqueued ([`ActionChain::enqueue_chain`]): each element's `RunOnFinish` is
//! given the element after it, and the first element goes to its actor carrying the rest. This is
//! equivalent because ACE never adds to a chain after enqueuing it.
//!
//! C# overloads get distinct names: `add_action` takes a closure (`AddAction(IActor, Action)`),
//! `add_iaction` an [`Action`] (`AddAction(IActor, IAction)`), `add_element` a [`ChainElement`].

use crate::entity::actions::conditional_action::{ConditionalAction, Predicate};
use crate::entity::actions::delay_action::DelayAction;
use crate::entity::actions::i_action::{Action, NextAct};
use crate::entity::actions::i_actor::{enqueue, Actor};
use crate::entity::actions::loop_action::{LoopAction, LoopBody};
use crate::World;

/// `ActionChain.ChainElement`: one action and the actor that runs it.
#[derive(Debug)]
pub struct ChainElement {
    /// `Action`.
    pub action: Action,
    /// `Actor`.
    pub actor: Actor,
}

impl ChainElement {
    // ACE: ActionChain.ChainElement.ChainElement
    pub fn new(actor: Actor, action: Action) -> Self {
        ChainElement { action, actor }
    }
}

// ACE: ActionChain
/// `ActionChain`: a sequence of actions, each run by its own actor, one after another.
#[derive(Debug, Default)]
pub struct ActionChain {
    /// `FirstElement` is `elements[0]`; `lastElement` is the last entry.
    elements: Vec<ChainElement>,
}

impl ActionChain {
    // ACE: ActionChain.ActionChain
    /// `new ActionChain()`.
    pub fn new() -> Self {
        ActionChain {
            elements: Vec::new(),
        }
    }

    /// `new ActionChain(firstActor, Action firstAction)`.
    pub fn with_action(
        first_actor: Actor,
        first_action: impl FnOnce(&mut World) + Send + 'static,
    ) -> Self {
        Self::from_element(ChainElement::new(
            first_actor,
            Action::delegate(first_action),
        ))
    }

    /// `new ActionChain(firstActor, IAction firstAction)`.
    pub fn with_iaction(first_actor: Actor, first_action: Action) -> Self {
        Self::from_element(ChainElement::new(first_actor, first_action))
    }

    /// `new ActionChain(ChainElement elm)`.
    pub fn from_element(elm: ChainElement) -> Self {
        ActionChain {
            elements: vec![elm],
        }
    }

    // ACE: ActionChain.FirstElement
    pub fn first_element(&self) -> Option<&ChainElement> {
        self.elements.first()
    }

    /// The elements in order (`FirstElement` and its `NextAct` links, once linked).
    pub fn elements(&self) -> &[ChainElement] {
        &self.elements
    }

    pub fn len(&self) -> usize {
        self.elements.len()
    }

    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    // ACE: ActionChain.AddAction
    /// `AddAction(IActor actor, Action action)`: a closure, wrapped in an `ActionEventDelegate`.
    pub fn add_action(
        &mut self,
        actor: Actor,
        action: impl FnOnce(&mut World) + Send + 'static,
    ) -> &mut Self {
        let new_elm = ChainElement::new(actor, Action::delegate(action));
        self.add_element(new_elm);

        self
    }

    /// `AddAction(IActor actor, IAction action)`.
    pub fn add_iaction(&mut self, actor: Actor, action: Action) -> &mut Self {
        let new_elm = ChainElement::new(actor, action);
        self.add_element(new_elm);

        self
    }

    /// `AddAction(ChainElement elm)`. The `RunOnFinish` link to the previous element is made when
    /// the chain is linked (see the module docs).
    pub fn add_element(&mut self, elm: ChainElement) -> &mut Self {
        self.elements.push(elm);

        self
    }

    // ACE: ActionChain.AddChain
    /// Appends `chain`'s elements (or takes them, if this chain is empty). A `None` or empty chain
    /// adds nothing.
    pub fn add_chain(&mut self, chain: Option<ActionChain>) -> &mut Self {
        if let Some(chain) = chain {
            if !chain.elements.is_empty() {
                self.elements.extend(chain.elements);
            }
        }

        self
    }

    // ACE: ActionChain.AddBranch
    /// `AddBranch(conditionActor, condition, ActionChain trueBranch, ActionChain falseBranch)`.
    pub fn add_branch(
        &mut self,
        condition_actor: Actor,
        condition: impl FnMut(&mut World) -> bool + Send + 'static,
        true_branch: ActionChain,
        false_branch: ActionChain,
    ) -> &mut Self {
        let condition: Predicate = Box::new(condition);
        self.add_element(ChainElement::new(
            condition_actor,
            Action::Conditional(ConditionalAction::new(condition, true_branch, false_branch)),
        ));

        self
    }

    /// `AddBranch(conditionActor, condition, ChainElement trueBranch, ChainElement falseBranch)`.
    pub fn add_branch_elements(
        &mut self,
        condition_actor: Actor,
        condition: impl FnMut(&mut World) -> bool + Send + 'static,
        true_branch: ChainElement,
        false_branch: ChainElement,
    ) -> &mut Self {
        self.add_branch(
            condition_actor,
            condition,
            ActionChain::from_element(true_branch),
            ActionChain::from_element(false_branch),
        );

        self
    }

    // ACE: ActionChain.AddLoop
    /// `AddLoop(conditionActor, condition, body)`. `body` builds the loop body afresh for each
    /// iteration (see [`LoopAction`]).
    pub fn add_loop(
        &mut self,
        condition_actor: Actor,
        condition: impl FnMut(&mut World) -> bool + Send + 'static,
        body: impl FnMut(&mut World) -> ActionChain + Send + 'static,
    ) -> &mut Self {
        let body: LoopBody = Box::new(body);
        self.add_element(ChainElement::new(
            condition_actor,
            Action::Loop(LoopAction::new(condition_actor, Box::new(condition), body)),
        ));

        self
    }

    // ACE: ActionChain.AddDelaySeconds
    /// Adds a wait of `time_in_seconds` on the delay manager. If `time_in_seconds <= 0` nothing is
    /// added (use [`add_delay_for_one_tick`](Self::add_delay_for_one_tick)); NaN warns and adds
    /// nothing. `w` supplies the delay's sequence number, as ACE's static `glblSequence` does.
    pub fn add_delay_seconds(&mut self, w: &mut World, time_in_seconds: f64) -> &mut Self {
        if time_in_seconds.is_nan() {
            log::warn!("WARNING: ActionChain.AddDelaySeconds(NaN)");
            return self;
        }

        if time_in_seconds <= 0.0 {
            return self;
        }

        let delay = DelayAction::new(w, time_in_seconds);
        self.add_iaction(Actor::Delay, Action::Delay(delay));

        self
    }

    // ACE: ActionChain.AddDelayForOneTick
    /// A 0.001 s wait. ACE writes `0.001f`, a `float`, so the wait is `0.001f32` widened to double.
    pub fn add_delay_for_one_tick(&mut self, w: &mut World) -> &mut Self {
        // ACE's own TODO: this can still run in the same tick if enqueued before that tick's
        // RunActions.
        let delay = DelayAction::new(w, f64::from(0.001_f32));
        self.add_iaction(Actor::Delay, Action::Delay(delay));

        self
    }

    /// Links the chain (each element's `RunOnFinish` set to the next element) and returns its first
    /// element carrying the rest. `None` for an empty chain (a null `FirstElement`).
    pub fn into_first(self) -> Option<NextAct> {
        let mut next: Option<NextAct> = None;
        for ChainElement { mut action, actor } in self.elements.into_iter().rev() {
            if let Some((next_actor, next_action)) = next.take() {
                action.run_on_finish(next_actor, next_action);
            }
            next = Some((actor, action));
        }
        next
    }

    // ACE: ActionChain.EnqueueChain
    /// Hands the first element, carrying the rest of the chain, to its actor.
    pub fn enqueue_chain(self, w: &mut World) {
        if let Some((actor, action)) = self.into_first() {
            enqueue(w, actor, action);
        }
    }
}
