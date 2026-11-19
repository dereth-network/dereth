//! The key maps over the data: the presets as the install holds them, the keys a fresh start has,
//! and a player's saved files bound in play.

use auberean_assets::keymap::input_name;
use auberean_assets::ui::KeyMap;
use auberean_client_sdk::contract::control::ControlId;
use auberean_dat::tree;
use auberean_shell::host::{HostFiles, TestHost};
use auberean_shell::input::{Event, Key};
use auberean_shell::keys::{KeyFiles, KeyRecords, Keys, DEFAULT_SHIFT, KEY_MAPS, WOW_NORMAL};

use crate::controls::Rig;

/// The WoW preset's Ctrl map.
const WOW_CONTROL: u32 = 0x4100_000E;
const INVENTORY: u32 = 0x4100_0002;
const EXAMINATION: u32 = 0x4100_0001;
const REPLY: u32 = 0x4100_0049;
const STRAFE_LEFT: u32 = 0x4000_0022;
const STRAFE_RIGHT: u32 = 0x4000_0023;
const RUN_LOCK: u32 = 0x4000_0027;

/// A player whose plain and Ctrl key map files hold the data's WoW preset (written as the client
/// saves a map) has those keys in play: Q strafes and C opens the inventory, where the Defaults
/// run on Q and bind no C; with no Shift file the Defaults' Shift map stays bound.
///
/// Behaviour: desktop.keys.a-saved-key-map-file-is-bound-in-play
#[test]
fn a_saved_key_map_file_is_bound_in_play() {
    let records = &crate::session::data().0.records;
    let file = |e: u32| {
        let m = records
            .get::<KeyMap>(records.data_id_by_enum(e, KEY_MAPS))
            .expect("a preset map");
        tree::to_file_bytes(&mut m.to_tree())
    };
    let mut host = TestHost::default();
    host.files.insert("Main.keymap".into(), file(WOW_NORMAL));
    host.files
        .insert("Control.keymap".into(), file(WOW_CONTROL));
    let keys = Keys::start(
        KeyRecords::read(records),
        KeyFiles::default(),
        &mut HostFiles(&mut host),
    );
    let shift_default = records
        .get::<KeyMap>(records.data_id_by_enum(DEFAULT_SHIFT, KEY_MAPS))
        .expect("the Defaults' Shift map");
    assert_eq!(keys.stack.layers()[2].map, shift_default);
    assert_eq!(
        keys.interface_action(Key::C.input_word(), 0),
        Some(INVENTORY)
    );
    assert_eq!(keys.stack.action(Key::Q.input_word(), 0), Some(STRAFE_LEFT));
    let mut c = Rig::over(&keys);
    c.event(
        &Event::Key {
            key: Key::Q,
            down: true,
            repeat: false,
        },
        &keys,
    );
    assert_eq!(c.intent().sideways, -1.0);
    assert!(!c.carried.is_down(ControlId(RUN_LOCK)));
    // The version the client writes is current: nothing is written again.
    assert_eq!(host.files["Main.keymap"], file(WOW_NORMAL));
}

/// The five presets the key map screen offers, each by the screen's name for it and its maps'
/// values for keys alone, with Shift and with Ctrl.
const PRESETS: [(&str, [u32; 3]); 5] = [
    ("Defaults", [0x4100_0001, 0x4100_0003, 0x4100_0002]),
    ("WoW", [0x4100_000D, 0x4100_000F, 0x4100_000E]),
    ("FK", [0x4100_000A, 0x4100_000C, 0x4100_000B]),
    ("EQ", [0x4100_0004, 0x4100_0006, 0x4100_0005]),
    ("DAoC", [0x4100_0007, 0x4100_0009, 0x4100_0008]),
];

/// A map's bindings by name, as a key map file writes them: each action's name and its input's.
fn named(keys: &Keys, map: &KeyMap) -> Vec<(String, &'static str)> {
    map.bindings
        .iter()
        .map(|b| {
            (
                keys.records
                    .action_name(b.action)
                    .unwrap_or_default()
                    .to_owned(),
                input_name(b.input).unwrap_or_default(),
            )
        })
        .collect()
}

/// The keys with no key map file, over the data.
fn fresh() -> (Keys, TestHost) {
    let records = &crate::session::data().0.records;
    let mut host = TestHost::default();
    let keys = Keys::start(
        KeyRecords::read(records),
        KeyFiles::default(),
        &mut HostFiles(&mut host),
    );
    (keys, host)
}

/// The game's five key map presets as the install holds them: each preset's three maps (each
/// named as the client names a map it saves, with `s` for Shift and `c` for Ctrl) and as many
/// bindings as each; the Defaults examine on E and the WoW keys on X.
///
/// Behaviour: data.key-map.the-five-presets-are-the-installs
#[test]
fn the_five_presets_decode_as_the_install_holds_them() {
    let records = &crate::session::data().0.records;
    let (keys, _) = fresh();
    let counts = [
        [69, 23, 31],
        [72, 21, 27],
        [66, 12, 31],
        [55, 22, 22],
        [49, 24, 19],
    ];
    // Every preset's maps share one name, the plain map's, with Shift's and Ctrl's marked after it.
    let base = records
        .get::<KeyMap>(records.data_id_by_enum(PRESETS[0].1[0], KEY_MAPS))
        .expect("the first preset's plain map")
        .name;
    for ((name, values), count) in PRESETS.iter().zip(counts) {
        for ((value, suffix), n) in values.iter().zip(["", "s", "c"]).zip(count) {
            let m = records
                .get::<KeyMap>(records.data_id_by_enum(*value, KEY_MAPS))
                .expect("a preset's map");
            assert_eq!(m.name, format!("{base}{suffix}"), "{value:08X}");
            assert_eq!(m.bindings.len(), n, "{name}{suffix}");
        }
    }
    let plain = |value: u32| {
        let m = records
            .get::<KeyMap>(records.data_id_by_enum(value, KEY_MAPS))
            .expect("a preset's map");
        named(&keys, &m)
    };
    let examine = ("EXAMINATION_PANEL_KEY".to_owned(), "DIKEYBOARD_E");
    assert!(plain(0x4100_0001).contains(&examine));
    assert!(!plain(0x4100_000D).contains(&examine));
    assert!(plain(0x4100_000D).contains(&("EXAMINATION_PANEL_KEY".to_owned(), "DIKEYBOARD_X")));
}

/// With no key map file a player starts with the keys the game's installer laid down: the WoW
/// preset's keys alone (X examines, Q and E strafe, R replies), the Defaults' Shift map, and the
/// installer's Ctrl map, the WoW preset's less its developer keys and Ctrl+F6, binding for binding.
/// Nothing is written.
///
/// Behaviour: desktop.keys.a-fresh-start-has-the-installed-keys
#[test]
fn a_fresh_start_has_the_keys_the_installer_laid_down() {
    let records = &crate::session::data().0.records;
    let (keys, host) = fresh();
    let map = |e: u32| {
        records
            .get::<KeyMap>(records.data_id_by_enum(e, KEY_MAPS))
            .expect("a map")
    };
    let l = keys.stack.layers();
    assert_eq!(l.len(), 4);
    assert_eq!(l[1].map, map(WOW_NORMAL));
    assert_eq!(l[2].map, map(DEFAULT_SHIFT));
    let mut control = named(&keys, &l[3].map);
    control.sort_unstable();
    let mut file: Vec<(String, &str)> = [
        ("EXAMINATION_PANEL_KEY", "DIKEYBOARD_F5"),
        ("NAVIGATION_PANEL_KEY", "DIKEYBOARD_F4"),
        ("RADAR_KEY", "DIKEYBOARD_F2"),
        ("SCREENSHOT_KEY", "DIKEYBOARD_P"),
        ("SELECTION_NEAREST_CREATURE_KEY", "DIKEYBOARD_RBRACKET"),
        ("SELECTION_NEAREST_ITEM_KEY", "DIKEYBOARD_O"),
        ("SELECTION_NEAREST_PC_KEY", "DIKEYBOARD_LBRACKET"),
        ("SELECTION_NEAREST_FOE_KEY", "DIKEYBOARD_EQUALS"),
        ("SELECTION_NEAREST_FELLOW_KEY", "DIKEYBOARD_U"),
        ("SHORTCUT_SET_1_KEY", "DIKEYBOARD_1"),
        ("SHORTCUT_SET_2_KEY", "DIKEYBOARD_2"),
        ("SHORTCUT_SET_3_KEY", "DIKEYBOARD_3"),
        ("SHORTCUT_SET_4_KEY", "DIKEYBOARD_4"),
        ("SHORTCUT_SET_5_KEY", "DIKEYBOARD_5"),
        ("SHORTCUT_SET_6_KEY", "DIKEYBOARD_6"),
        ("SHORTCUT_SET_7_KEY", "DIKEYBOARD_7"),
        ("SHORTCUT_SET_8_KEY", "DIKEYBOARD_8"),
        ("SHORTCUT_SET_9_KEY", "DIKEYBOARD_9"),
        ("SHORTCUT_SET_0_KEY", "DIKEYBOARD_0"),
        ("KEYSTONE_PANEL_KEY", "DIKEYBOARD_F1"),
    ]
    .iter()
    .map(|(a, i)| ((*a).to_owned(), *i))
    .collect();
    file.sort_unstable();
    assert_eq!(control, file);
    assert_eq!(l[3].meta, auberean_shell::keys::META_CTRL);
    assert_eq!(
        keys.interface_action(Key::X.input_word(), 0),
        Some(EXAMINATION),
        "X examines"
    );
    assert_eq!(keys.interface_action(Key::E.input_word(), 0), None);
    assert_eq!(
        keys.stack.action(Key::E.input_word(), 0),
        Some(STRAFE_RIGHT)
    );
    assert_eq!(keys.stack.action(Key::Q.input_word(), 0), Some(STRAFE_LEFT));
    assert_eq!(keys.interface_action(Key::R.input_word(), 0), Some(REPLY));
    assert!(host.files.is_empty(), "nothing written");
}

/// Horizon's whole layout, written to its three files as its Key Bindings page applies it and
/// read back over the data, is the same layout binding for binding: every action's name, the
/// game's (reply among them) and this client's own, finds its action again. So the files a player
/// applied hold the map they made, to keep or to send.
///
/// Behaviour: desktop.keys.horizons-key-files-read-back-as-written
#[test]
fn horizons_layout_written_to_its_files_reads_back_as_it_is() {
    use auberean_shell::interface::Choice;
    use auberean_shell::interface_keys::{horizon_map, layers, write, HORIZON_FILES};
    use auberean_shell::keys::{META_CTRL, META_SHIFT};
    let (keys, mut host) = fresh();
    let metas = [0, META_SHIFT, META_CTRL];
    let maps = metas.map(|m| {
        horizon_map(&keys.records, m)
            .bindings
            .iter()
            .map(|b| (b.input, b.action))
            .collect::<Vec<_>>()
    });
    assert!(maps[0].contains(&(Key::T.input_word(), REPLY)), "T replies");
    let [n, s, c] = &maps;
    assert!(write(
        Choice::Horizon,
        [n, s, c],
        &keys.records,
        &mut HostFiles(&mut host)
    ));
    for f in HORIZON_FILES {
        assert!(host.files.contains_key(f), "{f} written");
    }
    let back = layers(Choice::Horizon, &keys.records, &HostFiles(&mut host));
    assert_eq!(back.len(), 3);
    for ((layer, meta), map) in back.iter().zip(metas).zip(&maps) {
        assert_eq!(layer.meta, meta);
        let read: Vec<(u32, u32)> = layer
            .map
            .bindings
            .iter()
            .map(|b| (b.input, b.action))
            .collect();
        assert_eq!(&read, map, "the layer with {meta}");
    }
}
