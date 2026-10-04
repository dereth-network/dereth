//! This client's own actions in the input layer ([`dereth_client_contract::actions::dereth`]):
//! their map, toggle types, classes and names.
//!
//! The shipped action map has no row for these actions, so the manager registers them beside
//! the shipped rows in [`INPUT_MAP`]. They have no default keys. The map is live for the whole
//! run at the lowest priority, and a focused text box's barrier still stops its bindings.

use crate::actionmap::{ActionMap, ActionMapValue, ToggleType};
use crate::keymap::MasterInputMap;
use crate::spec::{ControlChord, ControlCode};
use crate::{ActionId, InputMapId};

/// The input map this client's own actions are bound in.
pub const INPUT_MAP: InputMapId = InputMapId(0x2000_0000);

/// One of this client's actions: its id, toggle type, action class (the key page's tab:
/// 1 movement, 3 interface, 7 character settings), and name. The shipped string table has no
/// name for it, so the name is this client's own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DerethAction {
    pub action: u32,
    pub toggle: ToggleType,
    pub class: u32,
    pub name: &'static str,
}

/// The action-class tabs of the key page these actions are listed on.
pub mod class {
    pub const MOVEMENT: u32 = 1;
    pub const INTERFACE: u32 = 3;
    pub const CHARACTER_SETTINGS: u32 = 7;
}

/// The key page's heading over this client's actions on each tab.
pub const SECTION_NAME: &str = "Dereth";

/// The name the key page shows for one of this client's actions.
#[must_use]
pub fn name(action: ActionId) -> Option<&'static str> {
    ACTIONS
        .iter()
        .find(|a| a.action == action.0)
        .map(|a| a.name)
}

/// This client's actions.
pub const ACTIONS: &[DerethAction] = {
    use dereth_client_contract::actions::dereth as a;
    const fn one_shot(action: u32, class: u32, name: &'static str) -> DerethAction {
        DerethAction {
            action,
            toggle: ToggleType::OneShot,
            class,
            name,
        }
    }
    &[
        // The performance panel has no key of its own: the player binds one, or uses its option.
        one_shot(
            a::TOGGLE_PERFORMANCE_PANEL,
            class::INTERFACE,
            "Performance Panel",
        ),
        DerethAction {
            toggle: ToggleType::Hold,
            ..one_shot(a::MOVEMENT_HOLD_SIDESTEP, class::MOVEMENT, "Hold Sidestep")
        },
        one_shot(a::TOGGLE_TRADE_PANEL, class::INTERFACE, "Trade Panel"),
        one_shot(
            a::TOGGLE_SPELL_RESEARCH_PANEL,
            class::INTERFACE,
            "Spell Research Panel",
        ),
        one_shot(
            a::PLAYER_OPTION_AUTO_CREATE_SHORTCUTS,
            class::CHARACTER_SETTINGS,
            "Automatically Create Shortcuts",
        ),
        one_shot(
            a::TOGGLE_INVERT_MOUSE_LOOK,
            class::CHARACTER_SETTINGS,
            "Invert Mouse Look",
        ),
        one_shot(
            a::TOGGLE_RIGHT_CLICK_MOUSE_LOOK,
            class::CHARACTER_SETTINGS,
            "Right-Click Mouse Look (classic interface)",
        ),
        one_shot(
            a::TOGGLE_STRETCH_UI,
            class::CHARACTER_SETTINGS,
            "Stretch UI (classic interface)",
        ),
        one_shot(
            a::TOGGLE_MUTE_ON_LOSING_FOCUS,
            class::CHARACTER_SETTINGS,
            "Mute When Inactive",
        ),
        one_shot(a::CANCEL, class::INTERFACE, "Cancel (classic interface)"),
        one_shot(
            a::REPEAT_LAST_MESSAGE,
            class::INTERFACE,
            "Repeat Last Message (classic interface)",
        ),
    ]
};

impl ActionMap {
    /// Add this client's actions in [`INPUT_MAP`]. They carry no name in the shipped string
    /// table: their name and description ids are their own action ids, which no string table
    /// answers, and the key page shows [`name`] for them.
    ///
    /// [`INPUT_MAP`] conflicts with every map a key page lists keys in: it is live for the whole
    /// run beside all of them, so a key given to one of these actions is taken from whatever
    /// else had it, after the page asks, and a key given to anything else is taken from these,
    /// as the shipped maps' own conflicts work.
    pub fn add_dereth_actions(&mut self) {
        let mut maps: Vec<u32> = crate::presentation::ROWS.iter().map(|r| r.map).collect();
        maps.sort_unstable();
        maps.dedup();
        for m in maps.into_iter().filter(|m| *m != INPUT_MAP.0) {
            self.add_conflict(INPUT_MAP, InputMapId(m));
        }
        for a in ACTIONS {
            self.insert(
                INPUT_MAP,
                ActionId(a.action),
                ActionMapValue {
                    toggle_type: a.toggle,
                    action_class: a.class,
                    action_name: a.action,
                    description: a.action,
                },
            );
        }
    }
}

/// An unmodified key of the keyboard `map` binds keys on, by scan code (bit 7 for the extended
/// prefix): the device, sub-control and activation of the first unmodified keyboard key `map`
/// binds, with `scan` as the key. `None` when `map` binds no key on a keyboard.
#[must_use]
pub fn keyboard_chord(map: &MasterInputMap, scan: u16) -> Option<ControlChord> {
    let template = map
        .sections
        .iter()
        .flat_map(|s| s.bindings().iter())
        .map(|(qc, _)| *qc)
        .find(|qc| {
            map.device_type_of(qc.control) == Some(crate::spec::DeviceType::Keyboard)
                && qc.meta_mode == 0
        })?;
    let control = ControlCode::new(
        template.control.device_index(),
        template.control.sub_control(),
        scan,
    );
    Some(ControlChord::new(control, 0, template.activation))
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
        assert!(
            m.is_user_bindable(INPUT_MAP, perf),
            "listed on the key page"
        );
        assert_eq!(m.action_class(INPUT_MAP, perf), class::INTERFACE);
        assert_eq!(name(perf), Some("Performance Panel"));
        for a in ACTIONS {
            assert!(
                m.is_user_bindable(INPUT_MAP, ActionId(a.action)),
                "{}",
                a.name
            );
        }
    }
}
