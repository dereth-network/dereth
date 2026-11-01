//! The key maps over the data: a player's saved files bound in play.

use auberean_assets::ui::KeyMap;
use auberean_dat::tree;
use auberean_shell::controls::Controls;
use auberean_shell::host::{HostFiles, TestHost};
use auberean_shell::input::{Event, Key};
use auberean_shell::keys::{KeyFiles, KeyRecords, Keys, DEFAULT_SHIFT, KEY_MAPS};

/// The WoW preset's maps: keys alone, with Ctrl.
const WOW_NORMAL: u32 = 0x4100_000D;
const WOW_CONTROL: u32 = 0x4100_000E;
const INVENTORY: u32 = 0x4100_0002;
const STRAFE_LEFT: u32 = 0x4000_0022;
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
    let mut c = Controls::default();
    c.event(
        &Event::Key {
            key: Key::Q,
            down: true,
            repeat: false,
        },
        &keys,
    );
    assert_eq!(c.intent(&keys).sideways, -1.0);
    assert!(!c.action_down(RUN_LOCK));
    // The version the client writes is current: nothing is written again.
    assert_eq!(host.files["Main.keymap"], file(WOW_NORMAL));
}
