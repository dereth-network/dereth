// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Actions/ActionEventBase.cs
//! Port of `Source/ACE.Server/Entity/Actions/ActionEventBase.cs`.

use crate::entity::actions::i_action::{Action, NextAct};
use crate::entity::actions::i_actor::Actor;

/// `ActionEventBase`: the `NextAct` link most actions carry, embedded as `base` in each derived
/// action.
#[derive(Debug, Default)]
pub struct ActionEventBase {
    /// `NextAct`. Owned: the rest of a chain travels inside the action that runs before it.
    next_act: Option<Box<NextAct>>,
}

impl ActionEventBase {
    // ACE: ActionEventBase.ActionEventBase
    pub fn new() -> Self {
        ActionEventBase { next_act: None }
    }

    // ACE: ActionEventBase.Act
    /// `Act()`: returns `NextAct`.
    pub fn act(self) -> Option<NextAct> {
        let ret = self.next_act;
        ret.map(|b| *b)
    }

    // ACE: ActionEventBase.RunOnFinish
    pub fn run_on_finish(&mut self, next_actor: Actor, next_action: Action) {
        self.next_act = Some(Box::new((next_actor, next_action)));
    }

    /// Whether `NextAct` is set.
    pub fn has_next_act(&self) -> bool {
        self.next_act.is_some()
    }
}
