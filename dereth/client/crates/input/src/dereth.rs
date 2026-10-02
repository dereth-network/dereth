//! This client's own actions in the input layer ([`dereth_client_contract::actions::dereth`]):
//! the input map they sit in, their rows in the action map, and their default keys.
//!
//! The shipped action map and key maps have no row for an action the retail client did not have,
//! so these are added beside them when the manager starts: the action map learns each action's
//! toggle type and class in [`INPUT_MAP`], and the shipped defaults gain each action's default key
//! there. The map is live for the whole run at the lowest priority, so a key the player binds
//! to anything else in a game map still wins, and a focused text box's barrier still stops it.

use crate::actionmap::{ActionMap, ActionMapValue, ToggleType};
use crate::keymap::{InputMap, MasterInputMap};
use crate::spec::{ControlChord, ControlCode};
use crate::{ActionId, InputMapId};

/// The input map this client's own actions are bound in.
pub const INPUT_MAP: InputMapId = InputMapId(0x2000_0000);

/// One of this client's actions: its id, its toggle type, its action class and the key it has
/// by default, as a keyboard scan code. The class is 0: the shipped string table has no name for
/// the action, and the retail key page lists only the actions it can name (class and name both
/// set, as every shipped entry has them).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DerethAction {
    pub action: u32,
    pub toggle: ToggleType,
    pub class: u32,
    pub default_key: Option<u16>,
}

/// `DIK_F7`, the performance panel's key: no shipped key map binds it.
pub const DIK_F7: u16 = 0x41;

/// This client's actions.
pub const ACTIONS: &[DerethAction] = &[DerethAction {
    action: dereth_client_contract::actions::dereth::TOGGLE_PERFORMANCE_PANEL,
    toggle: ToggleType::OneShot,
    class: 0,
    default_key: Some(DIK_F7),
}];

impl ActionMap {
    /// Add this client's actions in [`INPUT_MAP`]. They carry no name in the shipped string
    /// table, so the retail key page does not list them.
    pub fn add_dereth_actions(&mut self) {
        for a in ACTIONS {
            self.insert(
                INPUT_MAP,
                ActionId(a.action),
                ActionMapValue {
                    toggle_type: a.toggle,
                    action_class: a.class,
                    action_name: 0,
                    description: 0,
                },
            );
        }
    }
}

/// The default keys of this client's actions, as a key map over the same keyboard as `shipped`:
/// each key takes the device, sub-control and activation of a key the shipped map binds on the
/// keyboard (its first keyboard binding), with the action's own scan code. `None` when the
/// shipped map binds no key on a keyboard.
#[must_use]
pub fn default_map(shipped: &MasterInputMap) -> Option<MasterInputMap> {
    let template = shipped
        .sections
        .iter()
        .flat_map(|s| s.bindings().iter())
        .map(|(qc, _)| *qc)
        .find(|qc| {
            shipped.device_type_of(qc.control) == Some(crate::spec::DeviceType::Keyboard)
                && qc.meta_mode == 0
        })?;
    let mut map = MasterInputMap {
        devices: shipped.devices.clone(),
        ..MasterInputMap::default()
    };
    let mut section = InputMap::new(INPUT_MAP);
    for a in ACTIONS {
        let Some(key) = a.default_key else { continue };
        let control = ControlCode::new(
            template.control.device_index(),
            template.control.sub_control(),
            key,
        );
        section.add_mapping(
            ControlChord::new(control, 0, template.activation),
            ActionId(a.action),
        );
    }
    map.sections.push(section);
    Some(map)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the supplement's own shape; the key pressed in the client is tested with
    //! the shipped maps).
    use super::*;

    #[test]
    fn the_actions_are_in_their_own_map_with_their_toggle_type() {
        let mut m = ActionMap::default();
        m.add_dereth_actions();
        let perf = ActionId(dereth_client_contract::actions::dereth::TOGGLE_PERFORMANCE_PANEL);
        assert!(m.is_action_allowed_in_input_map(INPUT_MAP, perf));
        assert_eq!(m.toggle_type(INPUT_MAP, perf), ToggleType::OneShot);
        assert!(!m.is_user_bindable(INPUT_MAP, perf), "no shipped name");
    }
}
