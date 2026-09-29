// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Actions/EmptyAction.cs
//! Port of `Source/ACE.Server/Entity/Actions/EmptyAction.cs`.

use crate::entity::actions::action_event_base::ActionEventBase;
use crate::entity::actions::i_action::NextAct;

// ACE: EmptyAction
/// `EmptyAction`: does nothing but continue with `NextAct`.
#[derive(Debug, Default)]
pub struct EmptyAction {
    pub base: ActionEventBase,
}

impl EmptyAction {
    pub fn new() -> Self {
        Self::default()
    }

    /// The inherited `ActionEventBase.Act`.
    pub fn act(self) -> Option<NextAct> {
        self.base.act()
    }
}
