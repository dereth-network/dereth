//! The classic interface's keys in the one key map both interfaces keep.
//!
//! **One store.** The player's key bindings live in the client's key map (`dereth.keymap`, or the
//! file the profile names), in the shared action vocabulary: a key on the keyboard, bound to an
//! action in an input map. The retail interface edits it on its key page; the classic interface
//! edits the same map from its Keyboard Configuration page. Each interface brings its own default
//! scheme (the retail shipped key maps; the classic `Default.map`, or its stand-in), and the
//! player's own bindings, the ones that differ from the shipped defaults, are laid over it.
//!
//! The host hands the classic interface [`SharedKeys`] and carries out the [`KeyStoreRequest`]s
//! its page raises. A classic command is the shared action [`shared_actions`] names: the same
//! action for the 180 commands the final client has, each stance's action for the five combat
//! keys, and one of this client's own actions for the commands only the classic interface has.

use dereth_client_contract::actions::{dereth as own, names, ActionId};

/// One of the player's own bindings in the shared key map: an unmodified keyboard key (its scan
/// code, bit 7 for the extended prefix) bound to an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SharedBinding {
    pub scan: u16,
    pub action: u32,
}

/// What the shared key map holds for the classic interface.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SharedKeys {
    /// The player's own bindings: those that differ from the shipped defaults.
    pub player: Vec<SharedBinding>,
    /// The shipped defaults' bindings the player has taken away.
    pub removed: Vec<SharedBinding>,
    /// The key map files beside the one in use, by name without the extension.
    pub files: Vec<String>,
    /// The key map file in use, by name without the extension.
    pub current: Option<String>,
}

/// What the classic interface's key page asks of the shared key map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyStoreRequest {
    /// Bind the key `scan` to `action`, in place of whatever the key did beside it; `replaced`,
    /// when set, is a key `action` no longer has.
    Bind {
        scan: u16,
        action: u32,
        replaced: Option<u16>,
    },
    /// `action` no longer has the key `scan`.
    Unbind { scan: u16, action: u32 },
    /// Save the key map as the file `name`, replacing one of that name when `overwrite`.
    SaveAs { name: String, overwrite: bool },
    /// Use the key map file `name`.
    Load(String),
    /// Delete the key map file `name`.
    Delete(String),
    /// Back to the shipped defaults: the player's own bindings are dropped.
    Defaults,
}

/// The shared actions a classic command is. Empty for a command no shared action stands for.
#[must_use]
pub fn shared_actions(command: &str) -> Vec<u32> {
    let one = |a: u32| vec![a];
    match command {
        // The five combat keys are each stance's own action.
        "HighAttack"
        | "MediumAttack"
        | "LowAttack"
        | "DecreasePowerSetting"
        | "IncreasePowerSetting" => [2, 4, 8]
            .into_iter()
            .filter_map(|mode| crate::keybindings::runtime_action_in_mode(command, mode))
            .map(|a| a.0)
            .collect(),
        // Holding to walk is the final client's walk-mode key.
        "HoldRun" => one(0x32),
        // Holding to look around is the final client's instant mouse look.
        "ShiftView" => one(0x3D),
        // The final client's slash-command key.
        "IssueSlashCommand" => one(0x1000_0028),
        // The commands only the classic interface had: this client's own actions.
        "HoldSidestep" => one(own::MOVEMENT_HOLD_SIDESTEP),
        "TradePanel" => one(own::TOGGLE_TRADE_PANEL),
        "SpellResearchPanel" => one(own::TOGGLE_SPELL_RESEARCH_PANEL),
        "AutoCreateShortcuts" => one(own::PLAYER_OPTION_AUTO_CREATE_SHORTCUTS),
        "InvertMouseLook" => one(own::TOGGLE_INVERT_MOUSE_LOOK),
        "RightClickToMouseLook" => one(own::TOGGLE_RIGHT_CLICK_MOUSE_LOOK),
        "StretchUI" => one(own::TOGGLE_STRETCH_UI),
        "MuteOnLosingFocus" => one(own::TOGGLE_MUTE_ON_LOSING_FOCUS),
        name => crate::keybindings::runtime_action(name)
            .map(|a| a.0)
            .into_iter()
            .collect(),
    }
}

/// The classic command (by name) a shared action is, out of `commands`.
#[must_use]
pub fn command_for_action<'a>(
    commands: impl IntoIterator<Item = &'a str>,
    action: u32,
) -> Option<&'a str> {
    commands
        .into_iter()
        .find(|c| shared_actions(c).contains(&action))
}

/// The scan code (bit 7 for the extended prefix) of a classic key code: the inverse of
/// [`crate::default_keys::virtual_key`].
#[must_use]
pub fn scan_code(vk: u16) -> Option<u16> {
    (0..0x100u16).find(|s| crate::default_keys::virtual_key(*s) == Some(vk))
}

/// Whether `action` is one of this client's own actions.
#[must_use]
pub fn is_own(action: ActionId) -> bool {
    names::DERETH_ACTION_NAMES
        .iter()
        .any(|(a, _)| *a == action.0)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the command-to-action table; the shared map itself is tested where the
    //! host keeps it).
    use super::*;

    #[test]
    fn every_classic_command_is_a_shared_action_and_comes_back() {
        let catalogue = crate::key_catalogue::classic();
        let names: Vec<&str> = catalogue.actions.iter().map(|c| c.name.as_str()).collect();
        for c in &names {
            let actions = shared_actions(c);
            assert!(!actions.is_empty(), "{c} is no shared action");
            for a in actions {
                assert_eq!(command_for_action(names.iter().copied(), a), Some(*c));
            }
        }
        assert_eq!(shared_actions("HighAttack").len(), 3);
        assert_eq!(shared_actions("HoldRun"), [0x32]);
        assert_eq!(
            shared_actions("TogglePerformancePanel"),
            [own::TOGGLE_PERFORMANCE_PANEL]
        );
    }

    #[test]
    fn a_key_code_and_its_scan_code_are_one_key() {
        for vk in [0x57, 0x0D, 0x2B, 0x20, 0x76, 0x26, 0x68, 0x2E] {
            let s = scan_code(vk).expect("a key");
            assert_eq!(crate::default_keys::virtual_key(s), Some(vk));
        }
    }
}
