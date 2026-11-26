//! Each interface's own key maps: the layers the keys are looked up in while it is shown, in front
//! of the player's maps, which they stand in for (the base map, with the pointer, the modifier
//! keys and the text keys, stays under them). The retail interface has none: the player's maps are
//! its. Horizon has its own layout, so its keys never change the retail interface's, nor theirs
//! its.
//!
//! **Horizon's keys.** Movement: W forward, S back, A and D turn (under camera-based movement they
//! run across the camera's view), Q and E strafe, Space jumps, R runs on; Shift walks, as
//! everywhere. Stop has no key. The arrows turn and tilt the camera, and V held looks at the
//! character's front.
//! Windows: C skills, I or B inventory, M map, J or L quests, K options, O social, G or X
//! examine, N crafting. F uses the selection; Tab and Shift-Tab pick the next and the previous
//! creature; P and Shift-P the next and the previous of the player's pets; `\`, `[` and `]` the
//! nearest, the previous and the next item. Enter opens the chat entry, `/` the command line and T
//! a reply to the last tell; Shift+F2 to F4 show the second to the fourth chat window. 1 to 9 and 0 use the shortcut bar's ten slots,
//! and with Ctrl its second row's; `-` and `=` show its set before and after. `` ` `` starts and stops attacking. F12 takes a screenshot,
//! and Shift+Escape logs out.
//!
//! Each of Horizon's three layers (keys alone, with Shift, with Ctrl) is read from its file beside
//! the player's ([`HORIZON_FILES`], the same format), or, when its file is missing or does not
//! read, is the layout above, which ships with the client as three files of that same format
//! ([`SHIPPED`]). A player's applied files are never overwritten by the shipped ones. The files
//! name each action as [`HORIZON_ACTIONS`] does: the game's own by the input actions' names, and
//! the actions that are Horizon's alone by the names of ids in this client's own range ([`own`]),
//! which the action map never lists: the world's controls carry those out themselves. The interface
//! hears the interface's actions the keys begin and end, the game's and this client's own alike
//! ([`crate::interface::Game::keyed`]), and carries out those that are its own (a window's key, the
//! chat entry's).

use std::sync::LazyLock;

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

/// Horizon's shipped key maps, keys alone, with Shift, with Ctrl: what a player starts with in
/// place of each of [`HORIZON_FILES`] until they apply their own, in the same format.
pub const SHIPPED: [&[u8]; 3] = [
    include_bytes!("horizon-keys/auberean-horizon.keymap"),
    include_bytes!("horizon-keys/auberean-horizon-shift.keymap"),
    include_bytes!("horizon-keys/auberean-horizon-control.keymap"),
];

/// The actions Horizon's keys can be bound to, in its key bindings page's order, each with the
/// name its key map files give it (the input actions' names for the game's, [`own`]'s for this
/// client's own). An action can be here and bound to no key.
pub const HORIZON_ACTIONS: &[(u32, &str)] = &[
    (action::FORWARD, "Forward_KEY"),
    (action::BACKWARD, "Backward_KEY"),
    (action::TURN_LEFT, "Turn_Left_KEY"),
    (action::TURN_RIGHT, "Turn_Right_KEY"),
    (action::STRAFE_LEFT, "Strafe_Left_KEY"),
    (action::STRAFE_RIGHT, "Strafe_Right_KEY"),
    (action::JUMP, "Jump_KEY"),
    (action::RUN_LOCK, "RunLock_KEY"),
    (action::STOP, "Stop_KEY"),
    (action::ROTATE_LEFT, "CameraRotateLeft"),
    (action::ROTATE_RIGHT, "CameraRotateRight"),
    (action::TILT_UP, "CameraTiltUp"),
    (action::TILT_DOWN, "CameraTiltDown"),
    (own::LOOK_AT_FRONT, "LookAtFront"),
    (action::SKILLS, "SKILL_PANEL_KEY"),
    (action::INVENTORY, "INVENTORY_PANEL_KEY"),
    (action::MAP, "NAVIGATION_PANEL_KEY"),
    (action::QUESTS, "QUEST_PANEL_KEY"),
    (action::OPTIONS, "OPTION_PANEL_KEY"),
    (action::SOCIAL, "SOCIAL_PANEL_KEY"),
    (action::EXAMINATION, "EXAMINATION_PANEL_KEY"),
    (own::CRAFTING, "Crafting"),
    (action::USE, "USE_KEY"),
    (action::NEXT_CREATURE, "SELECTION_NEXT_CREATURE_KEY"),
    (action::PREVIOUS_CREATURE, "SELECTION_PREVIOUS_CREATURE_KEY"),
    (action::NEXT_PET, "SELECTION_NEXT_PET_KEY"),
    (action::PREVIOUS_PET, "SELECTION_PREVIOUS_PET_KEY"),
    (action::NEAREST_ITEM, "SELECTION_NEAREST_ITEM_KEY"),
    (action::PREVIOUS_ITEM, "SELECTION_PREVIOUS_ITEM_KEY"),
    (action::NEXT_ITEM, "SELECTION_NEXT_ITEM_KEY"),
    (action::COMMAND, "START_COMMAND_KEY"),
    (action::CHAT_ENTRY, "INCREMENT_ACTIVATION_KEY"),
    (action::REPLY, "REPLY_KEY"),
    (action::LOG_OUT, "LOGOUT_KEY"),
    (action::SCREENSHOT, "SCREENSHOT_KEY"),
    (action::AUTOATTACK, "AUTOATTACK_KEY"),
    (action::SET_PREVIOUS, "SHORTCUT_SET_PREV_KEY"),
    (action::SET_NEXT, "SHORTCUT_SET_NEXT_KEY"),
    (action::SHORTCUT_1, "SHORTCUT_1_KEY"),
    (action::SHORTCUT_1 + 1, "SHORTCUT_2_KEY"),
    (action::SHORTCUT_1 + 2, "SHORTCUT_3_KEY"),
    (action::SHORTCUT_1 + 3, "SHORTCUT_4_KEY"),
    (action::SHORTCUT_1 + 4, "SHORTCUT_5_KEY"),
    (action::SHORTCUT_1 + 5, "SHORTCUT_6_KEY"),
    (action::SHORTCUT_1 + 6, "SHORTCUT_7_KEY"),
    (action::SHORTCUT_1 + 7, "SHORTCUT_8_KEY"),
    (action::SHORTCUT_1 + 8, "SHORTCUT_9_KEY"),
    (action::SHORTCUT_1 + 9, "SHORTCUT_0_KEY"),
    (action::SECOND_ROW_1, "SHORTCUT_B_1_KEY"),
    (action::SECOND_ROW_1 + 1, "SHORTCUT_B_2_KEY"),
    (action::SECOND_ROW_1 + 2, "SHORTCUT_B_3_KEY"),
    (action::SECOND_ROW_1 + 3, "SHORTCUT_B_4_KEY"),
    (action::SECOND_ROW_1 + 4, "SHORTCUT_B_5_KEY"),
    (action::SECOND_ROW_1 + 5, "SHORTCUT_B_6_KEY"),
    (action::SECOND_ROW_1 + 6, "SHORTCUT_B_7_KEY"),
    (action::SECOND_ROW_1 + 7, "SHORTCUT_B_8_KEY"),
    (action::SECOND_ROW_1 + 8, "SHORTCUT_B_9_KEY"),
    (action::SECOND_ROW_1 + 9, "SHORTCUT_B_0_KEY"),
    (action::FELLOW_1, "SELECTION_FELLOW_1_KEY"),
    (action::FELLOW_1 + 1, "SELECTION_FELLOW_2_KEY"),
    (action::FELLOW_1 + 2, "SELECTION_FELLOW_3_KEY"),
    (action::FELLOW_1 + 3, "SELECTION_FELLOW_4_KEY"),
    (action::FELLOW_1 + 4, "SELECTION_FELLOW_5_KEY"),
    (action::FELLOW_1 + 5, "SELECTION_FELLOW_6_KEY"),
    (action::FELLOW_1 + 6, "SELECTION_FELLOW_7_KEY"),
    (action::FELLOW_1 + 7, "SELECTION_FELLOW_8_KEY"),
    (action::FELLOW_1 + 8, "SELECTION_FELLOW_9_KEY"),
    (action::CHAT_WINDOW_2, "CHATWINDOW_2_KEY"),
    (action::CHAT_WINDOW_2 + 1, "CHATWINDOW_3_KEY"),
    (action::CHAT_WINDOW_2 + 2, "CHATWINDOW_4_KEY"),
];

/// The action [`HORIZON_ACTIONS`] gives a name (letter case ignored).
fn action_by_name(name: &str) -> Option<u32> {
    HORIZON_ACTIONS
        .iter()
        .find(|(_, n)| n.eq_ignore_ascii_case(name))
        .map(|(a, _)| *a)
}

/// Horizon's shipped map for one modifier (0, Shift or Ctrl), read from its shipped file as a
/// player's file is read; empty for any other modifier.
#[must_use]
pub fn horizon_map(meta: u32) -> KeyMap {
    let Some(i) = [0, META_SHIFT, META_CTRL].iter().position(|m| *m == meta) else {
        return KeyMap::default();
    };
    tree::parse_file(HORIZON_FILES[i].as_bytes(), SHIPPED[i], &tree::NoIncludes)
        .and_then(|mut root| KeyMap::from_tree(&mut root, &action_by_name, &[]))
        .map(|r| r.map)
        .unwrap_or_default()
}

/// Horizon's shipped layout: each key's input word, the modifier it is bound with (none, Shift or
/// Ctrl), and its action, keys alone first, then with Shift, then with Ctrl, each in its file's
/// order.
pub static HORIZON_KEYS: LazyLock<Vec<(u32, u32, u32)>> = LazyLock::new(|| {
    [0, META_SHIFT, META_CTRL]
        .into_iter()
        .flat_map(|meta| {
            horizon_map(meta)
                .bindings
                .into_iter()
                .map(move |b| (b.input, meta, b.action))
        })
        .collect()
});

/// One of an interface's maps read from its file (a file it includes looked for beside it), its
/// actions named as the input actions or this client's own name them; none when the file is
/// missing or does not read.
fn read(records: &KeyRecords, store: &dyn KeyFileStore, name: &str) -> Option<KeyMap> {
    let bytes = store.load(name)?;
    let read = |n: &str| store.load(n);
    let mut root = tree::parse_file(name.as_bytes(), &bytes, &tree::Beside(&read))?;
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
                map: read(records, store, file).unwrap_or_else(|| horizon_map(meta)),
                meta,
            })
            .collect(),
    }
}
