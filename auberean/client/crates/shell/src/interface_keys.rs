//! Each interface's own key maps: the layers the keys are looked up in while it is shown, in front
//! of the player's maps, which they stand in for (the base map, with the pointer, the modifier
//! keys and the text keys, stays under them). The retail interface has none: the player's maps are
//! its. Horizon has its own layout, so its keys never change the retail interface's, nor theirs
//! its.
//!
//! **Horizon's keys.** Movement: W forward, S back, A and D turn (under camera-based movement they
//! run across the camera's view), Q and E strafe, Space jumps, R runs on, X stops; Shift walks, as
//! everywhere. The arrows turn and tilt the camera, and V held looks at the character's front.
//! Windows: C skills, I or B inventory, M map, J or L quests, K options, O social, G examines, N
//! crafting. F uses the selection; Tab and Shift-Tab pick the next and the previous creature; P
//! and Shift-P the next and the previous of the player's pets; `\`, `[` and `]` the nearest, the
//! previous and the next item. Enter opens the chat entry, `/` the command line and T a reply to
//! the last tell; Shift+F2 to F4 show the second to the fourth chat window. 1 to 9 and 0 use the shortcut bar's ten slots,
//! and with Ctrl its second row's; `-` and `=` show its set before and after. `` ` `` starts and stops attacking. F12 takes a screenshot,
//! and Shift+Escape logs out.
//!
//! Each of Horizon's three layers (keys alone, with Shift, with Ctrl) is read from its file beside
//! the player's ([`HORIZON_FILES`], the same format), or is the layout above when its file is
//! missing or does not read. The layout names the game's own actions by their action map ids, and
//! the actions that are Horizon's alone by ids in this client's own range ([`own`]), which the
//! action map never lists: the world's controls carry those out themselves. The interface hears
//! the interface's actions the keys begin and end, the game's and this client's own alike
//! ([`crate::interface::Game::keyed`]), and carries out those that are its own (a window's key,
//! the chat entry's).

use auberean_assets::ui::KeyMap;
use auberean_dat::tree;

use crate::interface::Choice;
use crate::keys::{KeyFileStore, KeyRecords, Layer, META_CTRL, META_SHIFT};

/// The actions in this client's own range, which no action map lists: an interface's own, heard
/// by the world's controls rather than the client's.
pub mod own {
    /// The range: every id from the first to the last is this client's own.
    pub const FIRST: u32 = 0x4F00_0000;
    pub const LAST: u32 = 0x4F00_FFFF;
    /// Held, the orbit camera looks at the player's front.
    pub const LOOK_AT_FRONT: u32 = 0x4F00_0001;
    /// Horizon's crafting window.
    pub const CRAFTING: u32 = 0x4F00_0002;

    /// Each own action and the name a key map file gives it.
    pub const NAMES: [(u32, &str); 2] = [(LOOK_AT_FRONT, "LookAtFront"), (CRAFTING, "Crafting")];

    /// Whether an action is in this client's own range.
    #[must_use]
    pub const fn is_own(action: u32) -> bool {
        action >= FIRST && action <= LAST
    }

    /// The own action a key map file's name names (letter case ignored).
    #[must_use]
    pub fn by_name(name: &str) -> Option<u32> {
        NAMES
            .iter()
            .find(|(_, n)| n.eq_ignore_ascii_case(name))
            .map(|(a, _)| *a)
    }

    /// The name a key map file gives an own action.
    #[must_use]
    pub fn name(action: u32) -> Option<&'static str> {
        NAMES.iter().find(|(a, _)| *a == action).map(|(_, n)| *n)
    }
}

/// Horizon's key map files, keys alone, with Shift, with Ctrl.
pub const HORIZON_FILES: [&str; 3] = [
    "auberean-horizon.keymap",
    "auberean-horizon-shift.keymap",
    "auberean-horizon-control.keymap",
];

/// A key's input word from its scan code.
const fn key(scan: u32) -> u32 {
    0x8100_0400 + scan
}

/// The action map's actions Horizon's layout names.
mod action {
    pub const FORWARD: u32 = 0x4000_0020;
    pub const BACKWARD: u32 = 0x4000_0021;
    pub const STRAFE_LEFT: u32 = 0x4000_0022;
    pub const STRAFE_RIGHT: u32 = 0x4000_0023;
    pub const TURN_LEFT: u32 = 0x4000_0024;
    pub const TURN_RIGHT: u32 = 0x4000_0025;
    pub const JUMP: u32 = 0x4000_0026;
    pub const RUN_LOCK: u32 = 0x4000_0027;
    pub const STOP: u32 = 0x4000_00A3;
    pub const ROTATE_LEFT: u32 = 0x4000_002E;
    pub const ROTATE_RIGHT: u32 = 0x4000_002F;
    pub const TILT_UP: u32 = 0x4000_00B6;
    pub const TILT_DOWN: u32 = 0x4000_00B7;
    pub const EXAMINATION: u32 = 0x4100_0001;
    pub const INVENTORY: u32 = 0x4100_0002;
    pub const MAP: u32 = 0x4100_0003;
    pub const OPTIONS: u32 = 0x4100_0004;
    pub const QUESTS: u32 = 0x4100_0005;
    pub const SKILLS: u32 = 0x4100_0006;
    pub const SOCIAL: u32 = 0x4100_0007;
    /// Select the first fellow (the next eight follow it).
    pub const FELLOW_1: u32 = 0x4100_0053;
    pub const USE: u32 = 0x4100_000D;
    pub const LOG_OUT: u32 = 0x4100_000E;
    pub const COMMAND: u32 = 0x4100_0010;
    /// Enter or leave the chat entry.
    pub const CHAT_ENTRY: u32 = 0x4000_0017;
    /// Reply to the last tell.
    pub const REPLY: u32 = 0x4100_0049;
    pub const SCREENSHOT: u32 = 0x4100_0017;
    pub const PREVIOUS_CREATURE: u32 = 0x4100_0019;
    pub const NEXT_CREATURE: u32 = 0x4100_001A;
    pub const PREVIOUS_ITEM: u32 = 0x4100_001C;
    pub const NEXT_ITEM: u32 = 0x4100_001D;
    pub const NEAREST_ITEM: u32 = 0x4100_001E;
    /// The shortcut bar's first slot; the ten follow one by one.
    pub const SHORTCUT_1: u32 = 0x4100_002B;
    /// The second row's first slot; the ten follow one by one.
    pub const SECOND_ROW_1: u32 = 0x4100_005F;
    /// Start or stop attacking.
    pub const AUTOATTACK: u32 = 0x4100_0013;
    /// The shortcut bar's next set, and the one before.
    pub const SET_NEXT: u32 = 0x4100_003F;
    pub const SET_PREVIOUS: u32 = 0x4100_0040;
    pub const PREVIOUS_PET: u32 = 0x4100_005C;
    pub const NEXT_PET: u32 = 0x4100_005D;
    /// Show the second chat window (the third and the fourth follow it).
    pub const CHAT_WINDOW_2: u32 = 0x4100_0042;
}

/// Horizon's layout: each key's input word, the modifier it is bound with (none, Shift or Ctrl),
/// and its action.
pub const HORIZON_KEYS: &[(u32, u32, u32)] = &[
    (key(0x11), 0, action::FORWARD),
    (key(0x1F), 0, action::BACKWARD),
    (key(0x1E), 0, action::TURN_LEFT),
    (key(0x20), 0, action::TURN_RIGHT),
    (key(0x10), 0, action::STRAFE_LEFT),
    (key(0x12), 0, action::STRAFE_RIGHT),
    (key(0x39), 0, action::JUMP),
    (key(0x13), 0, action::RUN_LOCK),
    (key(0x2D), 0, action::STOP),
    (key(0xCB), 0, action::ROTATE_LEFT),
    (key(0xCD), 0, action::ROTATE_RIGHT),
    (key(0xC8), 0, action::TILT_UP),
    (key(0xD0), 0, action::TILT_DOWN),
    (key(0x2F), 0, own::LOOK_AT_FRONT),
    (key(0x2E), 0, action::SKILLS),
    (key(0x17), 0, action::INVENTORY),
    (key(0x30), 0, action::INVENTORY),
    (key(0x32), 0, action::MAP),
    (key(0x24), 0, action::QUESTS),
    (key(0x26), 0, action::QUESTS),
    (key(0x25), 0, action::OPTIONS),
    (key(0x18), 0, action::SOCIAL),
    (key(0x22), 0, action::EXAMINATION),
    (key(0x31), 0, own::CRAFTING),
    (key(0x21), 0, action::USE),
    (key(0x0F), 0, action::NEXT_CREATURE),
    (key(0x0F), META_SHIFT, action::PREVIOUS_CREATURE),
    (key(0x19), 0, action::NEXT_PET),
    (key(0x19), META_SHIFT, action::PREVIOUS_PET),
    (key(0x2B), 0, action::NEAREST_ITEM),
    (key(0x1A), 0, action::PREVIOUS_ITEM),
    (key(0x1B), 0, action::NEXT_ITEM),
    (key(0x35), 0, action::COMMAND),
    (key(0x1C), 0, action::CHAT_ENTRY),
    (key(0x9C), 0, action::CHAT_ENTRY),
    (key(0x14), 0, action::REPLY),
    (key(0x01), META_SHIFT, action::LOG_OUT),
    (key(0x58), 0, action::SCREENSHOT),
    (key(0x29), 0, action::AUTOATTACK),
    (key(0x0C), 0, action::SET_PREVIOUS),
    (key(0x0D), 0, action::SET_NEXT),
    (key(0x02), 0, action::SHORTCUT_1),
    (key(0x03), 0, action::SHORTCUT_1 + 1),
    (key(0x04), 0, action::SHORTCUT_1 + 2),
    (key(0x05), 0, action::SHORTCUT_1 + 3),
    (key(0x06), 0, action::SHORTCUT_1 + 4),
    (key(0x07), 0, action::SHORTCUT_1 + 5),
    (key(0x08), 0, action::SHORTCUT_1 + 6),
    (key(0x09), 0, action::SHORTCUT_1 + 7),
    (key(0x0A), 0, action::SHORTCUT_1 + 8),
    (key(0x0B), 0, action::SHORTCUT_1 + 9),
    (key(0x02), META_CTRL, action::SECOND_ROW_1),
    (key(0x03), META_CTRL, action::SECOND_ROW_1 + 1),
    (key(0x04), META_CTRL, action::SECOND_ROW_1 + 2),
    (key(0x05), META_CTRL, action::SECOND_ROW_1 + 3),
    (key(0x06), META_CTRL, action::SECOND_ROW_1 + 4),
    (key(0x07), META_CTRL, action::SECOND_ROW_1 + 5),
    (key(0x08), META_CTRL, action::SECOND_ROW_1 + 6),
    (key(0x09), META_CTRL, action::SECOND_ROW_1 + 7),
    (key(0x0A), META_CTRL, action::SECOND_ROW_1 + 8),
    (key(0x0B), META_CTRL, action::SECOND_ROW_1 + 9),
    (key(0x3B), 0, action::FELLOW_1),
    (key(0x3C), 0, action::FELLOW_1 + 1),
    (key(0x3D), 0, action::FELLOW_1 + 2),
    (key(0x3E), 0, action::FELLOW_1 + 3),
    (key(0x3F), 0, action::FELLOW_1 + 4),
    (key(0x40), 0, action::FELLOW_1 + 5),
    (key(0x41), 0, action::FELLOW_1 + 6),
    (key(0x42), 0, action::FELLOW_1 + 7),
    (key(0x43), 0, action::FELLOW_1 + 8),
    (key(0x3C), META_SHIFT, action::CHAT_WINDOW_2),
    (key(0x3D), META_SHIFT, action::CHAT_WINDOW_2 + 1),
    (key(0x3E), META_SHIFT, action::CHAT_WINDOW_2 + 2),
];

/// Horizon's built-in map for one modifier (0, Shift or Ctrl), each binding named as a file names
/// it.
#[must_use]
pub fn horizon_map(records: &KeyRecords, meta: u32) -> KeyMap {
    let mut m = KeyMap::default();
    for &(input, _, a) in HORIZON_KEYS.iter().filter(|(_, m, _)| *m == meta) {
        let name = own::name(a)
            .or_else(|| records.action_name(a))
            .unwrap_or_default()
            .to_owned();
        m.add_action(a, input, &name);
    }
    m
}

/// One of an interface's maps read from its file, its actions named as the input actions or this
/// client's own name them; none when the file is missing or does not read.
fn read(records: &KeyRecords, store: &dyn KeyFileStore, name: &str) -> Option<KeyMap> {
    let bytes = store.load(name)?;
    let mut root = tree::parse_bytes(name.as_bytes(), &bytes)?;
    let action_of = |n: &str| own::by_name(n).or_else(|| records.action_of(n));
    Some(KeyMap::from_tree(&mut root, &action_of, &[])?.map)
}

/// Interface `choice`'s own maps as its bindings page applies them (each a list of an input and its
/// action: keys alone, with Shift, with Ctrl), written to its files, each binding named as the
/// files name it; whether every file was written. The retail interface has no files of its own.
pub fn write(
    choice: Choice,
    maps: [&[(u32, u32)]; 3],
    records: &KeyRecords,
    store: &mut dyn KeyFileStore,
) -> bool {
    if choice != Choice::Horizon {
        return false;
    }
    let mut ok = true;
    for (bindings, file) in maps.iter().zip(HORIZON_FILES) {
        let mut m = KeyMap {
            name: file.trim_end_matches(".keymap").into(),
            ..KeyMap::default()
        };
        for &(input, action) in *bindings {
            let name = own::name(action)
                .or_else(|| records.action_name(action))
                .unwrap_or_default()
                .to_owned();
            m.add_action(action, input, &name);
        }
        let mut t = m.to_tree();
        ok &= store.save(file, &tree::to_file_bytes(&mut t));
    }
    ok
}

/// The layers interface `choice` shows the keys through, from the back: none for the retail
/// interface; Horizon's three (keys alone, with Shift, with Ctrl), each from its file or its
/// built-in map.
#[must_use]
pub fn layers(choice: Choice, records: &KeyRecords, store: &dyn KeyFileStore) -> Vec<Layer> {
    match choice {
        Choice::Retail => Vec::new(),
        Choice::Horizon => HORIZON_FILES
            .iter()
            .zip([0, META_SHIFT, META_CTRL])
            .map(|(file, meta)| Layer {
                map: read(records, store, file).unwrap_or_else(|| horizon_map(records, meta)),
                meta,
            })
            .collect(),
    }
}
