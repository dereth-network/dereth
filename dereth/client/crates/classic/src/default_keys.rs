//! The classic interface's default keys, as the game shipped them in 2004, and the permanent keys
//! of its January 2005 key page.
//!
//! [`SHIPPED`] is the default key map that came with the game's installation: 75 keys, each an
//! unmodified key bound to one command. Each is written here as the shared action (or, for the
//! five combat keys, the three stance actions) its command is, so the table is the classic
//! interface's default scheme with no file needed. [`PERMANENT`] adds the keys the 2005 page
//! listed as fixed (Escape, Enter, Tab, F1, the number row and Ctrl-R), which this interface lets
//! the player bind like any other. [`default_map`] lays both out as a key map in the shared
//! vocabulary, each action in its own input map.

use dereth_input::actionmap::ActionMap;
use dereth_input::keymap::MasterInputMap;
use dereth_input::ActionId;

/// A key's modifiers as this interface keeps them: Shift.
pub const SHIFT: u8 = 1;
/// Ctrl.
pub const CTRL: u8 = 2;
/// Alt.
pub const ALT: u8 = 4;

/// The classic key code a keyboard control's offset stands for, on a US layout. The offset is the
/// key's scan code, with bit 7 standing for the extended (E0) prefix.
///
/// Classic key maps name keys by Windows virtual key, with two changes that keep the keypad apart
/// from the keys it shares a virtual key with: the keypad's Enter is `0x2B`, not the main Enter's
/// `0x0D`, and the keypad's navigation keys (the digits with Num Lock off) are the keypad digit
/// codes `0x60`–`0x69` and the keypad's Delete `0x6E`, not Home, End, the arrows, Insert and Delete.
#[must_use]
pub fn virtual_key(offset: u16) -> Option<u16> {
    const PLAIN: [u16; 0x59] = [
        0, 0x1B, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x30, 0xBD, 0xBB, 0x08,
        0x09, 0x51, 0x57, 0x45, 0x52, 0x54, 0x59, 0x55, 0x49, 0x4F, 0x50, 0xDB, 0xDD, 0x0D, 0xA2,
        0x41, 0x53, 0x44, 0x46, 0x47, 0x48, 0x4A, 0x4B, 0x4C, 0xBA, 0xDE, 0xC0, 0xA0, 0xDC, 0x5A,
        0x58, 0x43, 0x56, 0x42, 0x4E, 0x4D, 0xBC, 0xBE, 0xBF, 0xA1, 0x6A, 0xA4, 0x20, 0x14, 0x70,
        0x71, 0x72, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x90, 0x91, 0x67, 0x68, 0x69, 0x6D,
        0x64, 0x65, 0x66, 0x6B, 0x61, 0x62, 0x63, 0x60, 0x6E, 0, 0, 0, 0x7A, 0x7B,
    ];
    let vk = if offset & 0x80 == 0 {
        *PLAIN.get(usize::from(offset))?
    } else {
        match offset & 0x7F {
            0x1C => 0x2B,
            0x1D => 0xA3,
            0x35 => 0x6F,
            0x38 => 0xA5,
            0x47 => 0x24,
            0x48 => 0x26,
            0x49 => 0x21,
            0x4B => 0x25,
            0x4D => 0x27,
            0x4F => 0x23,
            0x50 => 0x28,
            0x51 => 0x22,
            0x52 => 0x2D,
            0x53 => 0x2E,
            _ => 0,
        }
    };
    (vk != 0).then_some(vk)
}

/// The scan code (bit 7 for the extended prefix) of a classic key code: the inverse of
/// [`virtual_key`].
#[must_use]
pub fn scan_code(vk: u16) -> Option<u16> {
    (0..0x100u16).find(|s| virtual_key(*s) == Some(vk))
}

/// The meta mode of a key held with `modifiers`: Shift `0x80000000`, Ctrl `0x40000000`, Alt
/// `0x20000000`, the bits the shared key map gives the three.
#[must_use]
pub const fn meta_of_modifiers(modifiers: u8) -> u32 {
    (if modifiers & SHIFT != 0 {
        0x8000_0000
    } else {
        0
    }) | (if modifiers & CTRL != 0 {
        0x4000_0000
    } else {
        0
    }) | (if modifiers & ALT != 0 { 0x2000_0000 } else { 0 })
}

/// The modifiers of a meta mode; `None` for one with any bit but those three.
#[must_use]
pub const fn modifiers_of_meta(meta: u32) -> Option<u8> {
    if meta & !0xE000_0000 != 0 {
        return None;
    }
    Some(
        (if meta & 0x8000_0000 != 0 { SHIFT } else { 0 })
            | (if meta & 0x4000_0000 != 0 { CTRL } else { 0 })
            | (if meta & 0x2000_0000 != 0 { ALT } else { 0 }),
    )
}

/// One default key: the classic key code, its modifiers, and the shared actions it is bound to.
pub type DefaultKey = (u16, u8, &'static [&'static str]);

/// The default key map the game shipped in 2004, row for row: 75 unmodified keys. A combat key is
/// the three stance actions its command was, one for each of melee, missile and magic.
#[rustfmt::skip]
pub const SHIPPED: [DefaultKey; 75] = [
    (8, 0, &["SelectionClosestCompassItem"]),
    (32, 0, &["MovementJump"]),
    (33, 0, &["CombatIncreaseAttackPower", "CombatIncreaseMissileAccuracy", "CombatNextSpellTab"]),
    (34, 0, &["CombatHighAttack", "CombatAimHigh", "CombatNextSpell"]),
    (35, 0, &["CombatMediumAttack", "CombatAimMedium", "CombatCastCurrentSpell"]),
    (36, 0, &["SelectionLastAttacker"]),
    (37, 0, &["MovementTurnLeft"]),
    (38, 0, &["MovementForward"]),
    (39, 0, &["MovementTurnRight"]),
    (40, 0, &["MovementBackup"]),
    (43, 0, &["CameraViewMapMode"]),
    (45, 0, &["CombatDecreaseAttackPower", "CombatDecreaseMissileAccuracy", "CombatPrevSpellTab"]),
    (46, 0, &["CombatLowAttack", "CombatAimLow", "CombatPrevSpell"]),
    (65, 0, &["MovementTurnLeft"]),
    (66, 0, &["Sleeping"]),
    (67, 0, &["MovementStrafeRight"]),
    (68, 0, &["MovementTurnRight"]),
    (69, 0, &["SelectionExamine"]),
    (70, 0, &["SelectionPickUp"]),
    (71, 0, &["Sitting"]),
    (72, 0, &["Crouch"]),
    (73, 0, &["Laugh"]),
    (74, 0, &["Wave"]),
    (75, 0, &["PointState"]),
    (76, 0, &["SelectionPreviousMonster"]),
    (77, 0, &["SelectionNextFellow"]),
    (78, 0, &["SelectionPreviousFellow"]),
    (79, 0, &["Cheer"]),
    (80, 0, &["SelectionPreviousSelection"]),
    (81, 0, &["MovementRunLock"]),
    (82, 0, &["USE"]),
    (83, 0, &["Ready"]),
    (84, 0, &["SelectionSplitStack"]),
    (85, 0, &["Cry"]),
    (87, 0, &["MovementForward"]),
    (88, 0, &["MovementBackup"]),
    (89, 0, &["Ready"]),
    (90, 0, &["MovementStrafeLeft"]),
    (96, 0, &["CameraViewDefault"]),
    (98, 0, &["CameraRotateDown"]),
    (100, 0, &["CameraRotateLeft"]),
    (101, 0, &["CameraViewLookDown"]),
    (102, 0, &["CameraRotateRight"]),
    (104, 0, &["CameraRotateUp"]),
    (106, 0, &["CaptureScreenshot"]),
    (107, 0, &["CameraMoveAway"]),
    (109, 0, &["CameraMoveToward"]),
    (110, 0, &["CameraViewFirstPerson"]),
    (111, 0, &["CameraInstantMouseLook"]),
    (113, 0, &["CameraInstantMouseLook"]),
    (114, 0, &["ToggleAllegiancePanel"]),
    (115, 0, &["ToggleFellowshipPanel"]),
    (116, 0, &["ToggleSpellbookPanel"]),
    (117, 0, &["ToggleSpellComponentsPanel"]),
    (118, 0, &["ToggleSpellResearchPanel"]),
    (119, 0, &["ToggleAttributesPanel"]),
    (120, 0, &["ToggleSkillsPanel"]),
    (121, 0, &["ToggleMapPanel"]),
    (122, 0, &["ToggleOptionsPanel"]),
    (123, 0, &["ToggleInventoryPanel"]),
    (160, 0, &["MovementWalkMode"]),
    (161, 0, &["MovementWalkMode"]),
    (164, 0, &["MovementHoldSidestep"]),
    (165, 0, &["MovementHoldSidestep"]),
    (186, 0, &["SelectionNextMonster"]),
    (187, 0, &["SelectionNextCompassItem"]),
    (188, 0, &["SelectionPreviousPlayer"]),
    (189, 0, &["SelectionPreviousCompassItem"]),
    (190, 0, &["SelectionNextPlayer"]),
    (191, 0, &["SelectionClosestPlayer"]),
    (192, 0, &["CombatToggleCombat"]),
    (219, 0, &["SelectionPreviousItem"]),
    (220, 0, &["SelectionClosestItem"]),
    (221, 0, &["SelectionNextItem"]),
    (222, 0, &["SelectionClosestMonster"]),
];

/// The January 2005 key page's permanent keys, at the keys it gave them: Escape cancels, Enter
/// and Tab are the chat keys, F1 is Help, 1 to 9 use the shortcuts, 0 makes one, and Ctrl-R
/// brings back the last line sent.
#[rustfmt::skip]
pub const PERMANENT: [DefaultKey; 15] = [
    (0x1B, 0, &["Cancel"]),
    (0x0D, 0, &["EnterChatMode"]),
    (0x09, 0, &["ToggleChatEntry"]),
    (0x70, 0, &["ToggleHelp"]),
    (0x31, 0, &["UseQuickSlot_1"]),
    (0x32, 0, &["UseQuickSlot_2"]),
    (0x33, 0, &["UseQuickSlot_3"]),
    (0x34, 0, &["UseQuickSlot_4"]),
    (0x35, 0, &["UseQuickSlot_5"]),
    (0x36, 0, &["UseQuickSlot_6"]),
    (0x37, 0, &["UseQuickSlot_7"]),
    (0x38, 0, &["UseQuickSlot_8"]),
    (0x39, 0, &["UseQuickSlot_9"]),
    (0x30, 0, &["CreateShortcut"]),
    (0x52, CTRL, &["RepeatLastMessage"]),
];

/// Every default key's `(scan code, meta mode, action)`, in [`SHIPPED`]'s order and then
/// [`PERMANENT`]'s.
///
/// # Panics
/// A row naming an action the action tables do not know, which this module's tests rule out.
pub fn default_keys() -> impl Iterator<Item = (u16, u32, ActionId)> {
    SHIPPED
        .iter()
        .chain(PERMANENT.iter())
        .flat_map(|(vk, modifiers, actions)| {
            let scan = scan_code(*vk).expect("every default key is a key");
            actions.iter().map(move |name| {
                (
                    scan,
                    meta_of_modifiers(*modifiers),
                    dereth_client_contract::actions::names::action_for_enum_name(name)
                        .expect("every default key's action is known"),
                )
            })
        })
}

/// The classic interface's default scheme as a key map: each default key bound in its action's
/// input map ([`dereth_input::scheme::home_map`]), as `template` (the final client's shipped map)
/// binds keyboard keys, with `template`'s devices and meta keys.
#[must_use]
pub fn default_map(
    template: &MasterInputMap,
    action_map: &ActionMap,
    shipped: &[&MasterInputMap],
) -> MasterInputMap {
    let mut map = MasterInputMap {
        devices: template.devices.clone(),
        meta_keys: template.meta_keys.clone(),
        used_meta_keys: template.used_meta_keys,
        ..MasterInputMap::default()
    };
    for (scan, meta, action) in default_keys() {
        let Some(key) = dereth_input::scheme::keyboard_key(template, scan, meta) else {
            continue;
        };
        let home = dereth_input::scheme::home_map(action_map, shipped, action);
        map.create_input_map(home).add_mapping(key, action);
    }
    map
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the harvested table's own consistency; the scheme it makes is tested
    //! where the host keeps it).
    use super::*;
    use dereth_input::presentation::{Interface, ROWS};

    /// Behaviour: keys.classic.the-default-keys-are-the-eras-shipped-map-and-the-permanent-keys
    #[test]
    fn the_shipped_map_is_seventy_five_keys_each_a_row_both_pages_list() {
        assert_eq!(SHIPPED.len(), 75);
        let mut keys = std::collections::BTreeSet::new();
        for (vk, modifiers, actions) in SHIPPED.iter().chain(PERMANENT.iter()) {
            assert!(keys.insert((*vk, *modifiers)), "key {vk} twice");
            assert!(scan_code(*vk).is_some(), "key {vk} is a key");
            assert!(actions.len() == 1 || actions.len() == 3);
            for name in *actions {
                let row = ROWS
                    .iter()
                    .find(|r| r.action_name == *name)
                    .unwrap_or_else(|| panic!("{name} is a row"));
                assert!(
                    row.not_used(Interface::Classic).is_none(),
                    "{name} acts here"
                );
            }
        }
        assert_eq!(default_keys().count(), 75 + 2 * 5 + 15);
    }

    /// Behaviour: keys.classic.a-row-this-interface-does-not-use-has-no-default-key
    #[test]
    fn every_row_this_interface_does_not_use_has_no_default_key() {
        let defaults: Vec<(u32, u32)> = default_keys()
            .map(|(_, _, action)| {
                let home = dereth_input::presentation::rows_of(action)
                    .next()
                    .expect("a row")
                    .map;
                (home.0, action.0)
            })
            .collect();
        let unused: Vec<_> = ROWS
            .iter()
            .filter(|r| r.not_used(Interface::Classic).is_some())
            .collect();
        assert_eq!(unused.len(), 19);
        for r in unused {
            assert!(
                !defaults.contains(&(r.map.0, r.action().0)),
                "{} has a default key here",
                r.action_name
            );
        }
    }

    #[test]
    fn scan_codes_become_classic_key_codes_with_the_keypad_kept_apart() {
        assert_eq!(virtual_key(0x11), Some(0x57));
        assert_eq!(virtual_key(0x1C), Some(0x0D));
        assert_eq!(virtual_key(0x80 | 0x1C), Some(0x2B));
        assert_eq!(virtual_key(0x39), Some(0x20));
        assert_eq!(virtual_key(0x58), Some(0x7B));
        assert_eq!(virtual_key(0x80 | 0x48), Some(0x26));
        assert_eq!(virtual_key(0x48), Some(0x68));
        assert_eq!(virtual_key(0x4C), Some(0x65));
        assert_eq!(virtual_key(0x53), Some(0x6E));
        assert_eq!(virtual_key(0x80 | 0x53), Some(0x2E));
        assert_eq!(virtual_key(0x7F), None);
        for vk in [0x57, 0x0D, 0x2B, 0x20, 0x76, 0x26, 0x68, 0x2E] {
            assert_eq!(scan_code(vk).and_then(virtual_key), Some(vk));
        }
    }

    #[test]
    fn modifiers_are_the_shared_maps_meta_bits() {
        assert_eq!(meta_of_modifiers(CTRL), 0x4000_0000);
        assert_eq!(modifiers_of_meta(0xA000_0000), Some(SHIFT | ALT));
        assert_eq!(modifiers_of_meta(0x1000_0000), None);
    }
}
