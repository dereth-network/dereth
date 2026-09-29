// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Actions/ActionEventDelegate.cs
//! Port of `Source/ACE.Server/Entity/Actions/ActionEventDelegate.cs`.

use std::fmt;

use crate::entity::actions::action_event_base::ActionEventBase;
use crate::entity::actions::i_action::NextAct;
use crate::World;

/// The closure an `ActionEventDelegate` runs. It captures guids and values, never references;
/// `Send` keeps `World` movable to the world thread.
pub type ActionDelegate = Box<dyn FnOnce(&mut World) + Send>;

/// `ActionEventDelegate`: runs a closure, then continues with `NextAct`.
pub struct ActionEventDelegate {
    pub base: ActionEventBase,
    /// `Action`.
    pub action: ActionDelegate,
}

impl fmt::Debug for ActionEventDelegate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ActionEventDelegate")
            .field("base", &self.base)
            .finish_non_exhaustive()
    }
}

impl ActionEventDelegate {
    // ACE: ActionEventDelegate.ActionEventDelegate
    pub fn new(action: impl FnOnce(&mut World) + Send + 'static) -> Self {
        ActionEventDelegate {
            base: ActionEventBase::new(),
            action: Box::new(action),
        }
    }

    // ACE: ActionEventDelegate.Act
    pub fn act(self, w: &mut World) -> Option<NextAct> {
        (self.action)(w);

        self.base.act()
    }
}
