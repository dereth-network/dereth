//! Input-map identifiers and their serialized names.

use crate::InputMapId;

/// `Invalid`.
pub const INVALID: InputMapId = InputMapId(0x00000000);
/// `EatKeyboardInput`.
pub const BLOCK_KEYBOARD: InputMapId = InputMapId(0x00000001);
/// `IgnoreRemainingMaps`.
pub const BLOCK_ALL: InputMapId = InputMapId(0x00000002);
/// `MouseCommands`.
pub const MOUSE: InputMapId = InputMapId(0x00000003);
/// `MovementCommands`.
pub const MOVEMENT: InputMapId = InputMapId(0x00000004);
/// `CameraControls`.
pub const CAMERA: InputMapId = InputMapId(0x00000005);
/// `CameraAlternateControls`.
pub const CAMERA_ALTERNATE: InputMapId = InputMapId(0x00000006);
/// `EditControls`.
pub const EDIT: InputMapId = InputMapId(0x00000007);
/// `CopyAndPasteControls`.
pub const COPY_PASTE: InputMapId = InputMapId(0x00000008);
/// `DialogBoxes`.
pub const DIALOG_BOXES: InputMapId = InputMapId(0x00000009);
/// `ScrollableControls`.
pub const SCROLLABLE: InputMapId = InputMapId(0x0000000A);
/// `DebugConsole`.
pub const DEBUG_CONSOLE: InputMapId = InputMapId(0x0000000B);
/// `ProfilerUI`.
pub const PROFILER: InputMapId = InputMapId(0x0000000C);
/// `UIDebugger`.
pub const UI_DEBUGGER: InputMapId = InputMapId(0x0000000D);
/// `DebugCommands`.
pub const DEBUG: InputMapId = InputMapId(0x0000000E);
/// `PreprocCommands`.
pub const PREPROCESSOR: InputMapId = InputMapId(0x0000000F);
/// `SystemKeys`.
pub const SYSTEM: InputMapId = InputMapId(0x00000010);
/// `Combat`.
pub const COMBAT: InputMapId = InputMapId(0x10000002);
/// `MeleeCombat`.
pub const MELEE: InputMapId = InputMapId(0x10000003);
/// `MissileCombat`.
pub const MISSILE: InputMapId = InputMapId(0x10000004);
/// `MagicCombat`.
pub const MAGIC: InputMapId = InputMapId(0x10000005);
/// `Emotes`.
pub const EMOTES: InputMapId = InputMapId(0x10000006);
/// `ItemSelectionCommands`.
pub const ITEM_SELECTION: InputMapId = InputMapId(0x10000007);
/// `CharacterOptionCommands`.
pub const CHARACTER_OPTIONS: InputMapId = InputMapId(0x10000008);
/// `UICommands`.
pub const UI: InputMapId = InputMapId(0x10000009);
/// `ChatCommands`.
pub const CHAT: InputMapId = InputMapId(0x1000000A);
/// `TargetedUsage`.
pub const TARGETED_USAGE: InputMapId = InputMapId(0x1000000B);
/// `QuickslotCommands`.
pub const QUICKSLOTS: InputMapId = InputMapId(0x1000000C);
/// `ToggleChatEntry`.
pub const TOGGLE_CHAT_ENTRY: InputMapId = InputMapId(0x1000000D);
/// This client's own action map. Its key-file name remains numeric.
pub const OWN: InputMapId = InputMapId(0x2000_0000);

pub(crate) const ENUM_NAMES: &[(InputMapId, &str)] = &[
    (INVALID, "Invalid"),
    (BLOCK_KEYBOARD, "EatKeyboardInput"),
    (BLOCK_ALL, "IgnoreRemainingMaps"),
    (MOUSE, "MouseCommands"),
    (MOVEMENT, "MovementCommands"),
    (CAMERA, "CameraControls"),
    (CAMERA_ALTERNATE, "CameraAlternateControls"),
    (EDIT, "EditControls"),
    (COPY_PASTE, "CopyAndPasteControls"),
    (DIALOG_BOXES, "DialogBoxes"),
    (SCROLLABLE, "ScrollableControls"),
    (DEBUG_CONSOLE, "DebugConsole"),
    (PROFILER, "ProfilerUI"),
    (UI_DEBUGGER, "UIDebugger"),
    (DEBUG, "DebugCommands"),
    (PREPROCESSOR, "PreprocCommands"),
    (SYSTEM, "SystemKeys"),
    (COMBAT, "Combat"),
    (MELEE, "MeleeCombat"),
    (MISSILE, "MissileCombat"),
    (MAGIC, "MagicCombat"),
    (EMOTES, "Emotes"),
    (ITEM_SELECTION, "ItemSelectionCommands"),
    (CHARACTER_OPTIONS, "CharacterOptionCommands"),
    (UI, "UICommands"),
    (CHAT, "ChatCommands"),
    (TARGETED_USAGE, "TargetedUsage"),
    (QUICKSLOTS, "QuickslotCommands"),
    (TOGGLE_CHAT_ENTRY, "ToggleChatEntry"),
];

#[cfg(test)]
mod tests {
    //! Behaviour: none (map identifiers retain their named and numeric file forms).
    use super::*;
    use crate::names::{enum_name_for_input_map, input_map_for_enum_name};

    #[test]
    fn named_and_unknown_maps_round_trip_without_naming_the_own_map() {
        assert_eq!(ENUM_NAMES.len(), 29);
        let mut ids = std::collections::BTreeSet::new();
        for &(id, name) in ENUM_NAMES {
            assert!(ids.insert(id));
            assert_eq!(enum_name_for_input_map(id), name);
            assert_eq!(input_map_for_enum_name(name), Some(id));
        }
        for id in [OWN, InputMapId(0xFFFF_FFFF), InputMapId(0x1000_0001)] {
            let text = id.0.to_string();
            assert_eq!(enum_name_for_input_map(id), text);
            assert_eq!(input_map_for_enum_name(&text), Some(id));
        }
        assert_eq!(input_map_for_enum_name("0x20000000"), None);
        assert_eq!(input_map_for_enum_name("movementcommands"), None);
    }
}
