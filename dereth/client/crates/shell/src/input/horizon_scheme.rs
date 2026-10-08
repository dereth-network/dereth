//! The Horizon interface's key map: its own layout, the few of the game's keys the interface plays
//! with (combat, spells, the shortcut and spell slots, zoom), and the game's
//! mechanics underneath (the pointer, text editing, the boxes), kept in a file of its own so its
//! keys never change the other interfaces'.

use super::InputShell;
use dereth_input::{ActionId, MasterInputMap};
use std::path::PathBuf;

/// The Horizon interface's slug: its key maps are `<name>-horizon.keymap`.
pub const HORIZON_SLUG: &str = "horizon";

/// The Horizon interface's key map file, beside the others.
pub const HORIZON_KEYMAP_FILE: &str = "dereth-horizon.keymap";

/// Shift, as a key's modifier mask: the first of the key map's meta keys, the word's top bit.
pub const SHIFT: u32 = 0x8000_0000;

/// Ctrl, as a key's modifier mask: the second of the key map's meta keys.
pub const CTRL: u32 = 0x4000_0000;

/// Alt, as a key's modifier mask: the third of the key map's meta keys.
const ALT: u32 = 0x2000_0000;

/// What earlier layouts gave as Shift: the word's bottom bit, which no meta key sets, so the key
/// was never pressed with it and showed as the bare key.
const EARLIER_SHIFT: u32 = 1;

/// The Horizon layout's keys, each a key's scan code, its modifiers and the action it does, by the
/// name a key map file gives it. Each takes its key from whatever the shipped map gave it; a key
/// listed more than once does each of its actions, each in its own map.
///
/// Movement: W forward, S back, A and D turn (camera-based movement makes them step sideways),
/// Q and E step sideways, Space jumps, R runs on, Shift+R switches between walking and running,
/// X stops. The arrows turn and tilt the camera. Windows: C character, I or B inventory,
/// M map, J or L journal, K settings, `;` allegiance, P spellbook. Tab and Shift-Tab pick the next
/// and the previous creature, Ctrl-Tab and Ctrl-Shift-Tab the next and the previous thing on the
/// radar; `\`, `[` and `]` the closest, the previous and the next item. V held looks
/// at the character's front, G examines. Enter opens the chat line, `/` opens it with a `/` in
/// it. 0, `-` and `=` are the tenth to twelfth shortcuts, and the spell bar's in a spell stance;
/// with Ctrl they are the shortcuts', as Ctrl with 1 to 9 is.
/// F12 takes a screenshot.
pub const HORIZON_KEYS: &[(u16, u32, &str)] = &[
    (0x11, 0, "MovementForward"),
    (0x1F, 0, "MovementBackup"),
    (0x1E, 0, "MovementTurnLeft"),
    (0x20, 0, "MovementTurnRight"),
    (0x10, 0, "MovementStrafeLeft"),
    (0x12, 0, "MovementStrafeRight"),
    (0x39, 0, "MovementJump"),
    (0x13, 0, "MovementRunLock"),
    (0x13, SHIFT, "MovementWalkMode"),
    (0x2D, 0, "MovementStop"),
    (0xCB, 0, "CameraRotateLeft"),
    (0xCD, 0, "CameraRotateRight"),
    (0xC8, 0, "CameraRotateUp"),
    (0xD0, 0, "CameraRotateDown"),
    (0x21, 0, "USE"),
    (0x34, 0, "USE"),
    (0x2E, 0, "ToggleCharacterInfoPanel"),
    (0x17, 0, "ToggleInventoryPanel"),
    (0x30, 0, "ToggleInventoryPanel"),
    (0x32, 0, "ToggleMapPanel"),
    (0x24, 0, "ToggleJournalPanel"),
    (0x26, 0, "ToggleJournalPanel"),
    (0x25, 0, "ToggleOptionsPanel"),
    (0x27, 0, "ToggleAllegiancePanel"),
    (0x19, 0, "ToggleSpellbookPanel"),
    (0x0F, 0, "SelectionNextMonster"),
    (0x0F, SHIFT, "SelectionPreviousMonster"),
    (0x0F, CTRL, "SelectionNextCompassItem"),
    (0x0F, CTRL | SHIFT, "SelectionPreviousCompassItem"),
    (0x2B, 0, "SelectionClosestItem"),
    (0x1A, 0, "SelectionPreviousItem"),
    (0x1B, 0, "SelectionNextItem"),
    (0x2F, 0, "LookAtFront"),
    (0x22, 0, "SelectionExamine"),
    (0x1C, 0, "EnterChatMode"),
    (0x35, 0, "START_COMMAND"),
    (0x0B, 0, "UseQuickSlot_10"),
    (0x0C, 0, "UseQuickSlot_11"),
    (0x0D, 0, "UseQuickSlot_12"),
    (0x0B, CTRL, "UseQuickSlot_10"),
    (0x0C, CTRL, "UseQuickSlot_11"),
    (0x0D, CTRL, "UseQuickSlot_12"),
    (0x0B, 0, "UseSpellSlot_10"),
    (0x0C, 0, "UseSpellSlot_11"),
    (0x0D, 0, "UseSpellSlot_12"),
    (0x58, 0, "CaptureScreenshot"),
    (0x18, 0, "ToggleSocialPanel"),
];

/// The wheel, turned up and down, as the camera map binds it: closer and farther. Each turn is a
/// click of its own virtual button.
const HORIZON_WHEEL: [(u32, &str); 2] = [
    (0x0008_0101, "CameraMoveToward"),
    (0x0008_0201, "CameraMoveAway"),
];

/// Bindings earlier layouts gave that this one does not: a saved map still holding one exactly
/// as the layout gave it never had it from the player, and takes the layout's key now. X examined
/// and O opened the friends list.
const RETIRED_KEYS: &[(u16, u32, &str)] = &[
    (0x2D, 0, "SelectionExamine"),
    (0x18, 0, "ToggleFriendsPanel"),
];

/// The game's keys the layout does not keep for the actions it does: Alt with 1, 2 and 3 used the
/// tenth to twelfth shortcuts, which 0, `-` and `=` use here.
const DROPPED_GAME_KEYS: &[(u16, u32, &str)] = &[
    (0x02, ALT, "UseQuickSlot_10"),
    (0x03, ALT, "UseQuickSlot_11"),
    (0x04, ALT, "UseQuickSlot_12"),
];

/// Keys the Horizon interface answers itself, which the shipped map's bindings are taken from: T puts
/// the keyboard in the stack splitter, F1 to F9 select the fellowship's members.
pub const HORIZON_OWN_KEYS: &[u16] = &[0x14, 0x3B, 0x3C, 0x3D, 0x3E, 0x3F, 0x40, 0x41, 0x42, 0x43];

/// The Horizon interface's default key map: the shipped maps, with [`HORIZON_KEYS`] laid over them and
/// [`HORIZON_OWN_KEYS`] taken off them.
#[must_use]
pub fn default_map(
    template: &MasterInputMap,
    action_map: &dereth_input::ActionMap,
    shipped: &[&MasterInputMap],
) -> MasterInputMap {
    let mut map = dereth_input::InputManager::load_over_defaults(None, shipped, Some(action_map));
    // Of the maps a key page lists, only the game's keys the interface plays with are kept; the
    // others (the pointer, text editing, the boxes) are mechanics, kept whole.
    for section in &mut map.sections {
        if !is_page_map(section.input_map_id) {
            continue;
        }
        section.retain(|_, action| kept_from_game(action));
    }
    for (scan, meta, name) in DROPPED_GAME_KEYS {
        let (Some(key), Some(dropped)) = (
            dereth_input::scheme::keyboard_key(template, *scan, *meta),
            dereth_client_contract::actions::names::action_for_enum_name(name),
        ) else {
            continue;
        };
        for section in &mut map.sections {
            section.retain(|k, action| !(k.is_exactly_equal(&key) && action == dropped));
        }
    }
    // A key the layout takes leaves the maps a key page lists; the text-editing maps, walked only
    // while a text box has the keyboard, keep it (the arrows still move the chat line's cursor).
    let unbind_everywhere = |map: &mut MasterInputMap, key: &dereth_input::ControlChord| {
        for section in &mut map.sections {
            if is_page_map(section.input_map_id) {
                section.unbind_by_key(key);
            }
        }
    };
    for scan in HORIZON_OWN_KEYS {
        if let Some(key) = dereth_input::scheme::keyboard_key(template, *scan, 0) {
            unbind_everywhere(&mut map, &key);
        }
    }
    let layout: Vec<_> = HORIZON_KEYS
        .iter()
        .filter_map(|(scan, meta, name)| {
            Some((
                dereth_input::scheme::keyboard_key(template, *scan, *meta)?,
                dereth_client_contract::actions::names::action_for_enum_name(name)?,
            ))
        })
        .chain(HORIZON_WHEEL.iter().filter_map(|(wheel, name)| {
            Some((
                dereth_input::ControlChord::new(
                    dereth_input::spec::ControlCode(*wheel),
                    0,
                    dereth_input::spec::activation::CLICK,
                ),
                dereth_client_contract::actions::names::action_for_enum_name(name)?,
            ))
        }))
        .collect();
    // Every key the layout names is taken first, so a key it names twice keeps both actions.
    for (key, _) in &layout {
        unbind_everywhere(&mut map, key);
    }
    for (key, action) in layout {
        let home = dereth_input::scheme::home_map(action_map, shipped, action);
        map.create_input_map(home).add_mapping(key, action);
    }
    map
}

/// The game's actions whose shipped keys the Horizon layout keeps: fighting (combat mode, the
/// attack's height and power, aim), the spells (cast, step, turn the tab: Insert, Page Up, Delete,
/// Page Down, End), the shortcut and spell slots on the number keys and mouse look. The
/// thirteenth to eighteenth shortcuts keep no key: the bar has twelve.
const KEPT_FROM_GAME: &[&str] = &[
    "CombatToggleCombat",
    "CombatDecreaseAttackPower",
    "CombatIncreaseAttackPower",
    "CombatLowAttack",
    "CombatMediumAttack",
    "CombatHighAttack",
    "CombatDecreaseMissileAccuracy",
    "CombatIncreaseMissileAccuracy",
    "CombatAimLow",
    "CombatAimMedium",
    "CombatAimHigh",
    "CombatCastCurrentSpell",
    "CombatPrevSpell",
    "CombatNextSpell",
    "CombatPrevSpellTab",
    "CombatNextSpellTab",
    "CombatFirstSpell",
    "CombatLastSpell",
    "CombatFirstSpellTab",
    "CombatLastSpellTab",
    "UseSpellSlot_1",
    "UseSpellSlot_2",
    "UseSpellSlot_3",
    "UseSpellSlot_4",
    "UseSpellSlot_5",
    "UseSpellSlot_6",
    "UseSpellSlot_7",
    "UseSpellSlot_8",
    "UseSpellSlot_9",
    "UseSpellSlot_10",
    "UseSpellSlot_11",
    "UseSpellSlot_12",
    "UseQuickSlot_1",
    "UseQuickSlot_2",
    "UseQuickSlot_3",
    "UseQuickSlot_4",
    "UseQuickSlot_5",
    "UseQuickSlot_6",
    "UseQuickSlot_7",
    "UseQuickSlot_8",
    "UseQuickSlot_9",
    "UseQuickSlot_10",
    "UseQuickSlot_11",
    "UseQuickSlot_12",
    "SelectQuickSlot_1",
    "SelectQuickSlot_2",
    "SelectQuickSlot_3",
    "SelectQuickSlot_4",
    "SelectQuickSlot_5",
    "SelectQuickSlot_6",
    "SelectQuickSlot_7",
    "SelectQuickSlot_8",
    "SelectQuickSlot_9",
    "CameraInstantMouseLook",
];

/// Whether the game's shipped binding of `action` is one the Horizon layout keeps.
fn kept_from_game(action: ActionId) -> bool {
    KEPT_FROM_GAME.iter().any(|name| {
        dereth_client_contract::actions::names::action_for_enum_name(name) == Some(action)
    })
}

/// Whether a key page lists the actions of `map`, as against the maps of the game's mechanics.
fn is_page_map(map: dereth_input::InputMapId) -> bool {
    dereth_input::presentation::ROWS
        .iter()
        .any(|r| r.map == map)
}

/// Take from a saved Horizon key map the game's own bindings it was given before the layout stopped
/// inheriting them: a binding the game's map has, word for word, in a map a key page lists, that
/// the layout no longer has. A binding the player made is theirs and stays. The maps no key page
/// lists (the pointer, text editing, the boxes) the player cannot have changed: they are taken
/// from the layout as they are.
pub fn drop_inherited(
    map: &mut MasterInputMap,
    shipped: &[&MasterInputMap],
    defaults: &MasterInputMap,
) {
    let game = dereth_input::InputManager::load_over_defaults(None, shipped, None);
    let has = |m: &MasterInputMap, id, key: &dereth_input::ControlChord, action| {
        m.section(id).is_some_and(|s| {
            s.bindings()
                .iter()
                .any(|(k, a)| k.is_exactly_equal(key) && *a == action)
        })
    };
    for section in &mut map.sections {
        let id = section.input_map_id;
        if !is_page_map(id) {
            section.retain(|_, _| false);
            for (key, action) in defaults.section(id).map_or(&[][..], |s| s.bindings()) {
                section.add_mapping(*key, *action);
            }
            continue;
        }
        section
            .retain(|key, action| !has(&game, id, key, action) || has(defaults, id, key, action));
    }
}

/// Mend a saved Horizon key map's Shift keys from an earlier layout, given with [`EARLIER_SHIFT`]:
/// each binding takes Shift in place of what the key with Shift did in its map, and a freed one
/// (bound to nothing) goes.
pub fn mend_shift(map: &mut MasterInputMap) {
    if map.used_meta_keys & EARLIER_SHIFT != 0 {
        return;
    }
    for section in &mut map.sections {
        let earlier: Vec<_> = section
            .bindings()
            .iter()
            .filter(|(k, _)| k.meta_mode == EARLIER_SHIFT)
            .copied()
            .collect();
        section.retain(|k, _| k.meta_mode != EARLIER_SHIFT);
        for (key, action) in earlier {
            if action == dereth_input::binding::DO_NOTHING {
                continue;
            }
            let shifted = dereth_input::ControlChord::new(key.control, SHIFT, key.activation);
            section.unbind_by_key(&shifted);
            section.add_mapping(shifted, action);
        }
    }
}

/// Take from a Horizon key map the freed keys (bound to nothing) its layout does not bind in the
/// same map. Such a key holds nothing back, as the layout gives it nothing there, but it is found
/// first and so hides what the key does in the maps under it: the camera's arrows under the
/// movement keys'.
pub fn drop_needless_frees(map: &mut MasterInputMap, defaults: &MasterInputMap) {
    for section in &mut map.sections {
        let id = section.input_map_id;
        section.retain(|key, action| {
            action != dereth_input::binding::DO_NOTHING
                || defaults.section(id).is_some_and(|s| {
                    s.bindings().iter().any(|(k, a)| {
                        k.is_exactly_equal(key) && *a != dereth_input::binding::DO_NOTHING
                    })
                })
        });
    }
}

/// What `key` does in `map`: each input map it is bound in and the action, in order.
fn bound_to(map: &MasterInputMap, key: &dereth_input::ControlChord) -> Vec<(u32, u32)> {
    let mut out: Vec<(u32, u32)> = dereth_input::scheme::keyboard_bindings(map)
        .filter(|(_, k, _)| k.is_exactly_equal(key))
        .map(|(m, _, a)| (m.0, a.0))
        .collect();
    out.sort_unstable();
    out
}

/// Bring a saved Horizon key map up to the layout as it is now: a key of the layout (or one the
/// interface answers itself, or one an earlier layout bound) that the saved map still binds
/// exactly as the game's own map does, or as the earlier layout did, was never the player's
/// choice, and takes the layout's binding. A key the player rebound is left as they bound it.
pub fn bring_up_to_date(
    map: &mut MasterInputMap,
    template: &MasterInputMap,
    shipped: &[&MasterInputMap],
    defaults: &MasterInputMap,
) {
    let game = dereth_input::InputManager::load_over_defaults(None, shipped, None);
    let keys = HORIZON_KEYS
        .iter()
        .chain(RETIRED_KEYS)
        .map(|(scan, meta, _)| (*scan, *meta))
        .chain(HORIZON_OWN_KEYS.iter().map(|scan| (*scan, 0)));
    for (scan, meta) in keys {
        let Some(key) = dereth_input::scheme::keyboard_key(template, scan, meta) else {
            continue;
        };
        let saved = bound_to(map, &key);
        let wanted = bound_to(defaults, &key);
        let retired = RETIRED_KEYS
            .iter()
            .filter(|(s, m, _)| (*s, *m) == (scan, meta))
            .filter_map(|(_, _, name)| {
                dereth_client_contract::actions::names::action_for_enum_name(name)
            })
            .any(|action| saved.iter().map(|(_, a)| *a).eq([action.0]));
        if saved == wanted || (saved != bound_to(&game, &key) && !retired) {
            continue;
        }
        for section in &mut map.sections {
            section.unbind_by_key(&key);
        }
        for (input_map, action) in wanted {
            map.create_input_map(dereth_input::InputMapId(input_map))
                .add_mapping(key, dereth_input::ActionId(action));
        }
    }
}

/// The Horizon interface's key map: its file read over its default layout.
#[derive(Debug, Clone)]
pub struct HorizonKeymap {
    /// The map as it is now.
    pub map: MasterInputMap,
    /// The default layout it is read over.
    pub defaults: MasterInputMap,
    /// Its file, `None` when the client keeps no files.
    pub path: Option<PathBuf>,
}

impl InputShell {
    /// The Horizon interface's default layout, over the same keyboard as the retail map.
    #[must_use]
    pub fn horizon_defaults(&self) -> MasterInputMap {
        let shipped: Vec<&MasterInputMap> = self
            .manager
            .shipped_maps
            .as_ref()
            .map(|m| vec![&m.0, &m.1])
            .unwrap_or_default();
        default_map(self.modern_map(), &self.manager.action_map, &shipped)
    }

    /// The Horizon interface's key map, read the first time it is wanted: its file over its default
    /// layout, the layout alone when there is no file yet.
    pub fn horizon_keymap(&mut self) -> &mut HorizonKeymap {
        if self.horizon.is_none() {
            let defaults = self.horizon_defaults();
            let path = self
                .keymap_path
                .as_deref()
                .and_then(std::path::Path::parent)
                .map(|dir| dir.join(HORIZON_KEYMAP_FILE));
            let file = path
                .as_ref()
                .and_then(|p| dereth_client_runtime::platform::files::read_to_string(p).ok());
            let mut map = dereth_input::InputManager::load_over_defaults(
                file.as_deref(),
                &[&defaults],
                Some(&self.manager.action_map),
            );
            if file.is_some() {
                mend_shift(&mut map);
                let shipped: Vec<&MasterInputMap> = self
                    .manager
                    .shipped_maps
                    .as_ref()
                    .map(|m| vec![&m.0, &m.1])
                    .unwrap_or_default();
                bring_up_to_date(&mut map, self.modern_map(), &shipped, &defaults);
                drop_inherited(&mut map, &shipped, &defaults);
                drop_needless_frees(&mut map, &defaults);
            }
            self.horizon = Some(HorizonKeymap {
                map,
                defaults,
                path,
            });
        }
        if self.horizon_active {
            self.horizon.as_mut().expect("active map").map = self.manager.keymap.clone();
        }
        self.horizon.as_mut().expect("just made")
    }

    /// Put the Horizon interface's key map in use, or back to the modern one, after the outgoing
    /// actions have been delivered.
    pub fn activate_horizon(&mut self, horizon: bool) {
        if horizon == self.horizon_active {
            return;
        }
        let next = if horizon {
            self.modern = self.manager.keymap.clone();
            self.horizon_keymap().map.clone()
        } else {
            self.horizon.as_mut().expect("active Horizon map").map = self.manager.keymap.clone();
            self.modern.clone()
        };
        self.set_focused_input_maps(&[]);
        self.set_active_input_maps(&[]);
        self.set_mode_input_maps(&[]);
        self.manager.set_text_mode(false);
        self.manager.keymap = next;
        // In the Horizon interface Ctrl and Alt make a key a different command: Ctrl+C is not C.
        self.manager.exact_command_modifiers = horizon;
        self.horizon_active = horizon;
    }

    /// Whether the Horizon interface's key map is the one in use.
    #[must_use]
    pub fn horizon_keys_active(&self) -> bool {
        self.horizon_active
    }

    /// Write the key map in use to its file: the Horizon interface's to its own while it is in use,
    /// the modern one's otherwise.
    ///
    /// # Errors
    /// [`dereth_input::InputError::Io`] if the file cannot be written.
    pub fn save_active_keymap(&mut self) -> Result<bool, dereth_input::InputError> {
        if !self.horizon_active {
            return self.save_keymap();
        }
        self.save_horizon_keymap()
    }

    /// Write the Horizon interface's key map to its file, when it has been read and the client keeps
    /// files, first dropping from the map in use the keys freed where the layout leaves them free
    /// ([`drop_needless_frees`]).
    ///
    /// # Errors
    /// [`dereth_input::InputError::Io`] if the file cannot be written.
    pub fn save_horizon_keymap(&mut self) -> Result<bool, dereth_input::InputError> {
        let Some(horizon) = self.horizon.as_ref() else {
            return Ok(false);
        };
        // A key freed in a map the layout leaves it free in would hide the maps under it.
        if self.horizon_active {
            drop_needless_frees(&mut self.manager.keymap, &horizon.defaults);
        }
        let horizon = self.horizon_keymap();
        let Some(path) = horizon.path.clone() else {
            return Ok(false);
        };
        dereth_client_runtime::platform::files::write(&path, horizon.map.to_keymap_text())?;
        Ok(true)
    }

    /// The Horizon interface's default layout back in use, the player's own keys dropped.
    pub fn restore_horizon_defaults(&mut self) {
        let defaults = self.horizon_keymap().defaults.clone();
        if self.horizon_active {
            self.manager.keymap = defaults.clone();
        }
        self.horizon_keymap().map = defaults;
    }

    /// The keys the Horizon layout gives `action` in `map`.
    pub fn horizon_default_keys(
        &mut self,
        action: ActionId,
        map: dereth_input::InputMapId,
    ) -> Vec<dereth_input::ControlChord> {
        self.horizon_keymap()
            .defaults
            .section(map)
            .map(|s| s.keys_for_action(action))
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the Horizon interface's own key layout)
    use super::*;

    #[test]
    fn the_layout_leaves_the_game_s_spell_keys_as_they_are() {
        // Insert and Page Up turn the spell bar's tab, Delete and Page Down step through its
        // spells, End casts: the game's own keys, which the layout neither takes nor rebinds.
        for scan in [0xD2, 0xC9, 0xD3, 0xD1, 0xCF] {
            assert!(
                !HORIZON_KEYS.iter().any(|(s, _, _)| *s == scan),
                "{scan:#x} is rebound"
            );
            assert!(!HORIZON_OWN_KEYS.contains(&scan), "{scan:#x} is taken");
        }
    }

    #[test]
    fn every_key_of_the_layout_names_a_known_action_once() {
        let mut keys = std::collections::BTreeSet::new();
        for (scan, meta, name) in HORIZON_KEYS {
            assert!(
                dereth_client_contract::actions::names::action_for_enum_name(name).is_some(),
                "{name}"
            );
            assert!(keys.insert((*scan, *meta, *name)), "{scan:#x} {name} twice");
        }
    }
}
