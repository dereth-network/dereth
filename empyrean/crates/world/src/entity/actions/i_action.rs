// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Actions/IAction.cs
//! Port of `Source/ACE.Server/Entity/Actions/IAction.cs`.
//!
//! ACE's `IAction` is an interface with five implementations; here it is a closed enum over them.
//! Each variant's struct lives in the module of its ACE file.

use crate::entity::actions::action_event_delegate::ActionEventDelegate;
use crate::entity::actions::conditional_action::ConditionalAction;
use crate::entity::actions::delay_action::DelayAction;
use crate::entity::actions::empty_action::EmptyAction;
use crate::entity::actions::i_actor::Actor;
use crate::entity::actions::loop_action::LoopAction;
use crate::World;

/// ACE's `Tuple<IActor, IAction>`: what `Act()` hands back to be enqueued next.
pub type NextAct = (Actor, Action);

// ACE: IAction
/// One queued unit of work (`IAction`).
#[derive(Debug)]
pub enum Action {
    Delegate(ActionEventDelegate),
    Delay(DelayAction),
    Conditional(ConditionalAction),
    Loop(LoopAction),
    Empty(EmptyAction),
}

impl Action {
    /// `new ActionEventDelegate(action)`, the common case.
    pub fn delegate(action: impl FnOnce(&mut World) + Send + 'static) -> Self {
        Action::Delegate(ActionEventDelegate::new(action))
    }

    /// `IAction.Act()`: runs the action and returns what to enqueue next, if anything. The action
    /// is consumed: each ACE action object is acted once, except loop bodies, which are rebuilt
    /// per iteration (see [`LoopAction`]).
    pub fn act(self, w: &mut World) -> Option<NextAct> {
        match self {
            Action::Delegate(a) => a.act(w),
            Action::Delay(a) => a.act(),
            Action::Conditional(a) => a.act(w),
            Action::Loop(a) => a.act(w),
            Action::Empty(a) => a.act(),
        }
    }

    /// `IAction.RunOnFinish(actor, action)`: sets what follows this action.
    pub fn run_on_finish(&mut self, actor: Actor, action: Action) {
        match self {
            Action::Delegate(a) => a.base.run_on_finish(actor, action),
            Action::Delay(a) => a.base.run_on_finish(actor, action),
            Action::Conditional(a) => a.run_on_finish(actor, action),
            Action::Loop(a) => a.base.run_on_finish(actor, action),
            Action::Empty(a) => a.base.run_on_finish(actor, action),
        }
    }
}
