//! The key maps and the action interpreter over a small key map and action map of their own.

use auberean_assets::strings::EnumMap;
use auberean_assets::ui::{ActionBindings, ActionEntry, KeyMap};
use auberean_dat::tree;
use auberean_shell::controls::{class, toggle, Controls};
use auberean_shell::host::{HostFiles, TestHost};
use auberean_shell::input::{Event, Key};
use auberean_shell::keys::{
    KeyFiles, KeyRecords, KeyStack, Keys, LEFT_CTRL, LEFT_SHIFT, META_CTRL, META_SHIFT,
};

const FORWARD: u32 = 0x4000_0020;
const BACKWARD: u32 = 0x4000_0021;
const STRAFE_LEFT: u32 = 0x4000_0022;
const TURN_LEFT: u32 = 0x4000_0024;
const JUMP: u32 = 0x4000_0026;
const RUN_LOCK: u32 = 0x4000_0027;
const SHIFT_KEY: u32 = 0x4000_000E;
const CTRL_KEY: u32 = 0x4000_000D;
const INVENTORY: u32 = 0x4100_0002;
const REPLY: u32 = 0x4100_0049;
const WAVE: u32 = 0x8000_0001;
const WAVE_BEHAVIOR: u32 = 0x4100_0007;

fn entry(action: u32, modifiers: u32, class: u32, value: u32, extent: f32) -> ActionEntry {
    ActionEntry {
        action,
        modifiers,
        class,
        value,
        extent,
    }
}

fn map(name: &str, bindings: &[(Key, u32)]) -> KeyMap {
    let mut m = KeyMap {
        name: name.into(),
        ..KeyMap::default()
    };
    for (k, a) in bindings {
        m.add_action(*a, k.input_word(), "");
    }
    m
}

/// A small game: forward (Shift walks), backward, a strafe, a turn, a jump, a run lock, the
/// modifiers, the inventory and reply.
fn records() -> KeyRecords {
    let names = [
        (FORWARD, "Forward_KEY"),
        (BACKWARD, "Backward_KEY"),
        (STRAFE_LEFT, "Strafe_Left_KEY"),
        (TURN_LEFT, "Turn_Left_KEY"),
        (JUMP, "Jump_KEY"),
        (RUN_LOCK, "RunLock_KEY"),
        (SHIFT_KEY, "LEFT_SHIFT_KEY"),
        (CTRL_KEY, "LEFT_CTRL_KEY"),
        (INVENTORY, "INVENTORY_PANEL_KEY"),
        (REPLY, "REPLY_KEY"),
        (WAVE, "Wave_KEY"),
    ];
    KeyRecords {
        base: map(
            "DefaultMap",
            &[
                (Key::ShiftLeft, SHIFT_KEY),
                (Key::ShiftRight, SHIFT_KEY),
                (Key::ControlLeft, CTRL_KEY),
            ],
        ),
        defaults: [
            map(
                "Defaults",
                &[
                    (Key::W, FORWARD),
                    (Key::S, BACKWARD),
                    (Key::A, TURN_LEFT),
                    (Key::Q, RUN_LOCK),
                    (Key::Space, JUMP),
                    (Key::I, INVENTORY),
                    (Key::O, WAVE),
                ],
            ),
            map("Defaults Shift", &[(Key::A, STRAFE_LEFT)]),
            map("Defaults Control", &[(Key::I, BACKWARD)]),
        ],
        actions: vec![EnumMap {
            id: 0x2300_002B,
            base: 0,
            names: names.iter().map(|(v, n)| (*v, (*n).to_owned())).collect(),
        }],
        action_map: ActionBindings {
            toggles: vec![
                (FORWARD, toggle::HELD),
                (BACKWARD, toggle::HELD),
                (STRAFE_LEFT, toggle::HELD),
                (TURN_LEFT, toggle::HELD),
                (JUMP, toggle::IMPULSE),
                (RUN_LOCK, toggle::LATCH),
                (SHIFT_KEY, toggle::HELD),
                (CTRL_KEY, toggle::HELD),
                (INVENTORY, toggle::HELD),
                (WAVE, toggle::HELD),
            ],
            actions: vec![
                entry(FORWARD, 0, class::MOVEMENT, 0x4000_0001, 1.0),
                entry(FORWARD, 1, class::MOVEMENT, 0x4000_0001, 0.339),
                entry(BACKWARD, 0, class::MOVEMENT, 0x4000_0001, -1.0),
                entry(BACKWARD, 8, class::MOVEMENT, 0x4000_0009, 0.0),
                entry(STRAFE_LEFT, 0, class::MOVEMENT, 0x4000_0006, 1.0),
                entry(TURN_LEFT, 0, class::MOVEMENT, 0x4000_0004, 1.0),
                entry(JUMP, 0, class::MOVEMENT, 0x4000_0007, 0.0),
                entry(RUN_LOCK, 0, class::MOVEMENT, 0x4000_0001, 1.0),
                entry(SHIFT_KEY, 0, class::MODIFIER, 1, 0.0),
                entry(CTRL_KEY, 0, class::MODIFIER, 2, 0.0),
                entry(INVENTORY, 0, class::INTERFACE, INVENTORY, 0.0),
                entry(WAVE, 0, class::BEHAVIOR, WAVE_BEHAVIOR, 0.0),
            ],
            ..ActionBindings::default()
        },
    }
}

fn start(host: &mut TestHost) -> Keys {
    Keys::start(records(), KeyFiles::default(), &mut HostFiles(host))
}

fn key(c: &mut Controls, keys: &Keys, key: Key, down: bool) {
    c.event(
        &Event::Key {
            key,
            down,
            repeat: false,
        },
        keys,
    );
}

/// The key maps are a stack: an input is looked up from the front, in the first map that binds it
/// whose modifier keys are all held (a plain map's always are); so with Shift held, the Shift map's
/// binding of a key comes before the plain map's, and the plain map's still answers for keys the
/// Shift map leaves alone. A player's map is never the base map.
///
/// Behaviour: desktop.keys.the-front-map-binding-an-input-names-its-action
#[test]
fn the_front_map_that_binds_an_input_with_its_modifiers_held_names_its_action() {
    let keys = start(&mut TestHost::default());
    let s = &keys.stack;
    assert_eq!(s.layers().len(), 4);
    let a = Key::A.input_word();
    assert_eq!(s.action(a, 0), Some(TURN_LEFT));
    assert_eq!(s.action(a, META_SHIFT), Some(STRAFE_LEFT));
    assert_eq!(s.action(a, META_CTRL), Some(TURN_LEFT));
    assert_eq!(
        s.action(Key::W.input_word(), META_SHIFT | META_CTRL),
        Some(FORWARD)
    );
    assert_eq!(s.action(Key::I.input_word(), META_CTRL), Some(BACKWARD));
    assert_eq!(s.find(Key::ShiftLeft.input_word(), 0), Some((0, SHIFT_KEY)));
    assert_eq!(
        keys.interface_action(Key::I.input_word(), 0),
        Some(INVENTORY)
    );
    assert_eq!(keys.interface_action(Key::W.input_word(), 0), None);
    let mut bare = KeyStack::default();
    assert!(bare.pop().is_none());
    bare.push(KeyMap::default(), LEFT_CTRL);
    assert_eq!(bare.layers()[0].meta, META_CTRL);
}

/// A key let go ends the action its press began, whatever modifier was let go in between: A
/// pressed with Shift strafes, and letting go of Shift first does not turn it into a turn.
///
/// Behaviour: desktop.keys.a-key-let-go-ends-what-its-press-began
#[test]
fn a_key_let_go_ends_the_action_its_press_began() {
    let keys = start(&mut TestHost::default());
    let mut c = Controls::default();
    key(&mut c, &keys, Key::ShiftLeft, true);
    key(&mut c, &keys, Key::A, true);
    key(&mut c, &keys, Key::ShiftLeft, false);
    let i = c.intent(&keys);
    assert_eq!((i.sideways, i.turn), (-1.0, 0.0));
    key(&mut c, &keys, Key::A, false);
    let i = c.intent(&keys);
    assert_eq!((i.sideways, i.turn), (0.0, 0.0));
    key(&mut c, &keys, Key::A, true);
    assert_eq!(c.intent(&keys).turn, 1.0);
    // A release with no press (the press went elsewhere) ends nothing.
    key(&mut c, &keys, Key::W, false);
    assert_eq!(c.intent(&keys).turn, 1.0);
}

/// Each action toggles as the action map says: a held one is in force while its key is down, a
/// latched one from one press to the next (forward and backward end it), one for a single press
/// for its frame only, however long its key is held.
///
/// Behaviour: desktop.keys.actions-toggle-as-the-action-map-says
#[test]
fn held_latched_and_single_press_actions_toggle_as_the_action_map_says() {
    let keys = start(&mut TestHost::default());
    let mut c = Controls::default();
    key(&mut c, &keys, Key::Q, true);
    key(&mut c, &keys, Key::Q, false);
    assert!(c.autorun());
    assert_eq!(c.intent(&keys).forward, 1.0);
    key(&mut c, &keys, Key::Q, true);
    assert!(!c.autorun(), "a second press lets go");
    key(&mut c, &keys, Key::Q, false);
    key(&mut c, &keys, Key::Q, true);
    key(&mut c, &keys, Key::S, true);
    assert!(!c.autorun(), "backward ends it");
    assert_eq!(c.intent(&keys).forward, -1.0);
    key(&mut c, &keys, Key::S, false);
    key(&mut c, &keys, Key::Space, true);
    assert!(c.intent(&keys).jump);
    assert!(!c.intent(&keys).jump, "a single press, once");
    key(&mut c, &keys, Key::I, true);
    assert!(c.action_down(INVENTORY));
    key(&mut c, &keys, Key::I, false);
    assert!(c.actions().is_empty());
}

/// An action of the behavior class asks the player to play its value when it begins and to stop it
/// when it ends, once each.
///
/// Behaviour: desktop.keys.a-behavior-action-plays-and-stops-on-the-player
#[test]
fn a_behavior_action_asks_the_player_to_play_it_and_to_stop_it() {
    let keys = start(&mut TestHost::default());
    let mut c = Controls::default();
    key(&mut c, &keys, Key::O, true);
    assert_eq!(c.intent(&keys).behaviors, [(WAVE_BEHAVIOR, true)]);
    assert!(c.intent(&keys).behaviors.is_empty(), "once");
    key(&mut c, &keys, Key::O, false);
    assert_eq!(c.intent(&keys).behaviors, [(WAVE_BEHAVIOR, false)]);
    assert!(c.intent(&keys).behaviors.is_empty(), "once");
}

/// The modifier actions in force choose an action's values: Shift's entry for forward walks,
/// an action with no entry for the modifiers held takes its plain one, and one with the entry
/// for bit 8 never sees it (nothing sets that bit).
///
/// Behaviour: desktop.keys.modifiers-choose-an-actions-values
#[test]
fn the_modifiers_held_choose_an_actions_values() {
    let keys = start(&mut TestHost::default());
    let mut c = Controls::default();
    key(&mut c, &keys, Key::ShiftRight, true);
    key(&mut c, &keys, Key::W, true);
    assert_eq!(c.modifier_bitfield(&keys), 1);
    assert_eq!(c.intent(&keys).forward, 0.339);
    key(&mut c, &keys, Key::ShiftRight, false);
    assert_eq!(c.intent(&keys).forward, 1.0);
    key(&mut c, &keys, Key::W, false);
    key(&mut c, &keys, Key::ControlLeft, true);
    key(&mut c, &keys, Key::A, true);
    assert_eq!(c.modifier_bitfield(&keys), 2);
    assert_eq!(c.intent(&keys).turn, 1.0, "no Ctrl entry: the plain one");
    let r = &keys.records;
    assert_eq!(r.values(BACKWARD, 8).map(|v| v.value), Some(0x4000_0009));
    assert_eq!(r.values(BACKWARD, 9).map(|v| v.value), Some(0x4000_0001));
}

/// At startup the player's maps come from their files: the plain map's file, read with the update
/// that binds R to reply when it is older and R is free, is written again with it; a file that is
/// missing or does not read leaves the data's Defaults map in its place.
///
/// Behaviour: desktop.keys.maps-come-from-their-files-else-the-defaults
#[test]
fn the_key_maps_come_from_their_files_else_the_defaults() {
    let mut host = TestHost::default();
    host.files.insert(
        "Main.keymap".into(),
        b"\"\" [ old ]\r\n\"\" [ 0 0 0 0 0 0 0 0 0 0 0 ]\r\n\"\" [ Forward_KEY DIKEYBOARD_UP ]\r\n"
            .to_vec(),
    );
    host.files
        .insert("Control.keymap".into(), b"not a key map".to_vec());
    let keys = start(&mut host);
    let l = keys.stack.layers();
    assert_eq!(l[1].map.name, "old");
    assert_eq!(
        l[1].map
            .bindings
            .iter()
            .map(|b| (b.input, b.action))
            .collect::<Vec<_>>(),
        [
            (Key::ArrowUp.input_word(), FORWARD),
            (Key::R.input_word(), REPLY)
        ]
    );
    assert_eq!(
        (l[2].map.name.as_str(), l[2].meta),
        ("Defaults Shift", META_SHIFT)
    );
    assert_eq!(
        (l[3].map.name.as_str(), l[3].meta),
        ("Defaults Control", META_CTRL)
    );
    let rewritten = String::from_utf8(host.files["Main.keymap"].clone()).unwrap();
    assert!(rewritten.starts_with("\r\nVERSION [ 1 ]\r\n\r\n\"\" [ old ]"));
    assert!(rewritten.ends_with("\"\" [ REPLY_KEY DIKEYBOARD_R ]\r\n"));
    assert!(
        !host.files.contains_key("Shift.keymap"),
        "a missing file is not made"
    );
}

/// Applying keys writes the three files the preferences name (`mydefault`, `mydefaults`,
/// `mydefaultc`, each binding named by its action) and binds them in place of the three player's
/// maps; when a file cannot be written, nothing changes.
///
/// Behaviour: desktop.keys.apply-writes-the-files-and-binds-them
#[test]
fn applying_writes_the_three_files_and_binds_them() {
    let mut host = TestHost::default();
    let mut keys = start(&mut host);
    let w = Key::W.input_word();
    let ok = keys.apply(
        [&[(w, BACKWARD)], &[(w, FORWARD)], &[]],
        &mut HostFiles(&mut host),
    );
    assert!(ok);
    let l = keys.stack.layers();
    assert_eq!(l.len(), 4);
    assert_eq!(l[0].map.name, "DefaultMap");
    assert_eq!(l[1].map.name, "mydefault");
    assert_eq!(keys.stack.action(w, 0), Some(BACKWARD));
    assert_eq!(keys.stack.action(w, META_SHIFT), Some(FORWARD));
    let mut root = tree::parse_bytes(b"Shift.keymap", &host.files["Shift.keymap"]).unwrap();
    assert_eq!(
        root.child_mut(1).unwrap().child_text(0).unwrap(),
        b"mydefaults"
    );
    assert_eq!(
        root.child_mut(3).unwrap().child_text(0).unwrap(),
        b"Forward_KEY"
    );
    assert!(host.files.contains_key("Control.keymap"));

    /// A store that cannot write.
    #[derive(Debug)]
    struct ReadOnly;
    impl auberean_shell::keys::KeyFileStore for ReadOnly {
        fn load(&self, _: &str) -> Option<Vec<u8>> {
            None
        }
        fn save(&mut self, _: &str, _: &[u8]) -> bool {
            false
        }
    }
    let before = keys.stack.clone();
    assert!(!keys.apply([&[], &[], &[]], &mut ReadOnly));
    assert_eq!(keys.stack, before);
    let _ = LEFT_SHIFT;
}
