//! The key maps and the controls they press, carried as the client carries them, over a small key
//! map and action map of their own.

use auberean_assets::strings::EnumMap;
use auberean_assets::ui::{ActionBindings, ActionEntry, KeyMap};
use auberean_client_sdk::contract::control::{class, toggle, ControlId};
use auberean_client_sdk::runtime::controls::Controls as Carried;
use auberean_client_sdk::Action;
use auberean_dat::tree;
use auberean_scene::Intent;
use auberean_shell::controls::Controls;
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
const EXAMINATION: u32 = 0x4100_0001;
const STOP: u32 = 0x4000_00A3;
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
    let mut r = KeyRecords {
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
        installed: Default::default(),
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
    };
    r.installed = r.defaults.clone();
    r
}

fn start(host: &mut TestHost) -> Keys {
    Keys::start(records(), KeyFiles::default(), &mut HostFiles(host))
}

/// The device's side of the controls and the client's side that carries them.
struct Rig {
    device: Controls,
    carried: Carried,
}

impl Rig {
    fn over(keys: &Keys) -> Self {
        Self {
            device: Controls::default(),
            carried: Carried::over(keys.records.action_map.clone()),
        }
    }

    fn intent(&mut self) -> Intent {
        let frame = self.carried.frame();
        self.device.intent(frame)
    }
}

fn key(c: &mut Rig, keys: &Keys, key: Key, down: bool) {
    c.device.event(
        &Event::Key {
            key,
            down,
            repeat: false,
        },
        keys,
    );
    for a in c.device.take_actions() {
        match a {
            Action::Control(control) => c.carried.control(control),
            Action::ReleaseControls { typing } => c.carried.release(typing),
            other => panic!("not a control: {other:?}"),
        }
    }
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
    let mut c = Rig::over(&keys);
    key(&mut c, &keys, Key::ShiftLeft, true);
    key(&mut c, &keys, Key::A, true);
    key(&mut c, &keys, Key::ShiftLeft, false);
    let i = c.intent();
    assert_eq!((i.sideways, i.turn), (-1.0, 0.0));
    key(&mut c, &keys, Key::A, false);
    let i = c.intent();
    assert_eq!((i.sideways, i.turn), (0.0, 0.0));
    key(&mut c, &keys, Key::A, true);
    assert_eq!(c.intent().turn, 1.0);
    // A release with no press (the press went elsewhere) ends nothing.
    key(&mut c, &keys, Key::W, false);
    assert_eq!(c.intent().turn, 1.0);
}

/// Each action toggles as the action map says: a held one is in force while its key is down, a
/// latched one from one press to the next (forward and backward end it), one for a single press
/// for its frame only, however long its key is held.
///
/// Behaviour: desktop.keys.actions-toggle-as-the-action-map-says
#[test]
fn held_latched_and_single_press_actions_toggle_as_the_action_map_says() {
    let keys = start(&mut TestHost::default());
    let mut c = Rig::over(&keys);
    key(&mut c, &keys, Key::Q, true);
    key(&mut c, &keys, Key::Q, false);
    assert!(c.carried.autorun());
    assert_eq!(c.intent().forward, 1.0);
    key(&mut c, &keys, Key::Q, true);
    assert!(!c.carried.autorun(), "a second press lets go");
    key(&mut c, &keys, Key::Q, false);
    key(&mut c, &keys, Key::Q, true);
    key(&mut c, &keys, Key::S, true);
    assert!(!c.carried.autorun(), "backward ends it");
    assert_eq!(c.intent().forward, -1.0);
    key(&mut c, &keys, Key::S, false);
    key(&mut c, &keys, Key::Space, true);
    assert!(c.intent().jump);
    assert!(!c.intent().jump, "a single press, once");
    key(&mut c, &keys, Key::I, true);
    assert!(c.carried.is_down(ControlId(INVENTORY)));
    key(&mut c, &keys, Key::I, false);
    assert!(c.carried.in_force().is_empty());
}

/// An action of the behavior class asks the player to play its value when it begins and to stop it
/// when it ends, once each.
///
/// Behaviour: desktop.keys.a-behavior-action-plays-and-stops-on-the-player
#[test]
fn a_behavior_action_asks_the_player_to_play_it_and_to_stop_it() {
    let keys = start(&mut TestHost::default());
    let mut c = Rig::over(&keys);
    key(&mut c, &keys, Key::O, true);
    assert_eq!(c.intent().behaviors, [(WAVE_BEHAVIOR, true)]);
    assert!(c.intent().behaviors.is_empty(), "once");
    key(&mut c, &keys, Key::O, false);
    assert_eq!(c.intent().behaviors, [(WAVE_BEHAVIOR, false)]);
    assert!(c.intent().behaviors.is_empty(), "once");
}

/// The modifier actions in force choose an action's values: Shift's entry for forward walks,
/// an action with no entry for the modifiers held takes its plain one, and one with the entry
/// for bit 8 never sees it (nothing sets that bit).
///
/// Behaviour: desktop.keys.modifiers-choose-an-actions-values
#[test]
fn the_modifiers_held_choose_an_actions_values() {
    let keys = start(&mut TestHost::default());
    let mut c = Rig::over(&keys);
    key(&mut c, &keys, Key::ShiftRight, true);
    key(&mut c, &keys, Key::W, true);
    assert_eq!(c.carried.modifiers(), 1);
    assert_eq!(c.intent().forward, 0.339);
    key(&mut c, &keys, Key::ShiftRight, false);
    assert_eq!(c.intent().forward, 1.0);
    key(&mut c, &keys, Key::W, false);
    key(&mut c, &keys, Key::ControlLeft, true);
    key(&mut c, &keys, Key::A, true);
    assert_eq!(c.carried.modifiers(), 2);
    assert_eq!(c.intent().turn, 1.0, "no Ctrl entry: the plain one");
    let r = &keys.records;
    assert_eq!(r.values(BACKWARD, 8).map(|v| v.value), Some(0x4000_0009));
    assert_eq!(r.values(BACKWARD, 9).map(|v| v.value), Some(0x4000_0001));
}

/// At startup the player's maps come from their files: the plain map's file, read with the update
/// that binds R to reply when it is older and R is free, is written again with it; a file that
/// does not read leaves the data's Defaults map in its place, and a file that is not there the
/// installer's.
///
/// Behaviour: desktop.keys.maps-come-from-their-files-else-the-defaults
#[test]
fn the_key_maps_come_from_their_files_else_the_defaults() {
    let mut host = TestHost::default();
    let mut records = records();
    records.installed[1] = map("Installed Shift", &[(Key::A, JUMP)]);
    host.files.insert(
        "Main.keymap".into(),
        b"\"\" [ old ]\r\n\"\" [ 0 0 0 0 0 0 0 0 0 0 0 ]\r\n\"\" [ Forward_KEY DIKEYBOARD_UP ]\r\n"
            .to_vec(),
    );
    host.files
        .insert("Control.keymap".into(), b"not a key map".to_vec());
    let keys = Keys::start(records, KeyFiles::default(), &mut HostFiles(&mut host));
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
        ("Installed Shift", META_SHIFT),
        "no Shift file: the installer's map"
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

/// A key map file may include another file, looked for beside it: its bindings are read in the
/// include's place. Without the included file the map does not read, and the data's Defaults map
/// stands in.
///
/// Behaviour: desktop.keys.a-key-map-file-includes-a-file-beside-it
#[test]
fn a_key_map_file_reads_the_bindings_of_a_file_it_includes() {
    let main = b"\"\" [ old ]\r\n\"\" [ 0 0 0 0 0 0 0 0 0 0 0 ]\r\n!include [ moves.txt ]\r\n";
    let mut host = TestHost::default();
    host.files.insert("Main.keymap".into(), main.to_vec());
    host.files.insert(
        "moves.txt".into(),
        b"\"\" [ Forward_KEY DIKEYBOARD_UP ]\r\n".to_vec(),
    );
    let keys = Keys::start(records(), KeyFiles::default(), &mut HostFiles(&mut host));
    let plain = &keys.stack.layers()[1].map;
    assert_eq!(plain.name, "old");
    assert!(
        plain
            .bindings
            .iter()
            .any(|b| (b.input, b.action) == (Key::ArrowUp.input_word(), FORWARD)),
        "{:?}",
        plain.bindings
    );

    let mut host = TestHost::default();
    host.files.insert("Main.keymap".into(), main.to_vec());
    let keys = Keys::start(records(), KeyFiles::default(), &mut HostFiles(&mut host));
    assert_eq!(keys.stack.layers()[1].map.name, "Defaults");
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

/// A preset's maps are bound at once and written to no file.
///
/// Behaviour: desktop.keys.a-preset-binds-and-writes-nothing
#[test]
fn a_preset_binds_and_writes_nothing() {
    let mut host = TestHost::default();
    let mut keys = start(&mut host);
    let files = host.files.clone();
    let w = Key::W.input_word();
    keys.bind([&[(w, BACKWARD)], &[(w, FORWARD)], &[]]);
    assert_eq!(keys.stack.action(w, 0), Some(BACKWARD));
    assert_eq!(keys.stack.action(w, META_SHIFT), Some(FORWARD));
    assert_eq!(keys.stack.layers().len(), 4, "in place of the three");
    assert_eq!(host.files, files, "nothing written");
}

/// An interface's own maps, while it is shown, are looked up before the stack and stand in for the
/// player's: their bindings answer (A turns, Q strafes, Shift with Tab picks the previous
/// creature), a key only the player's maps bind answers nothing, a key neither binds falls to the
/// base map (Shift), and the interface's plain map answers with a modifier its own maps leave
/// alone. With none, the player's maps answer again as they did.
///
/// Behaviour: desktop.keys.an-interfaces-own-maps-stand-in-for-the-players
#[test]
fn an_interfaces_own_maps_stand_in_for_the_players_over_the_base_map() {
    use auberean_shell::interface::Choice;
    use auberean_shell::interface_keys::layers;
    let mut host = TestHost::default();
    let mut keys = start(&mut host);
    let word = |k: Key| k.input_word();
    let before = [Key::A, Key::Q, Key::O].map(|k| keys.stack.action(word(k), 0));
    assert_eq!(before, [Some(TURN_LEFT), Some(RUN_LOCK), Some(WAVE)]);
    let own = layers(Choice::Horizon, &keys.records, &HostFiles(&mut host));
    assert_eq!(own.len(), 3);
    keys.stack.set_interface(own);
    let s = &keys.stack;
    assert_eq!(s.action(word(Key::A), 0), Some(TURN_LEFT));
    assert_eq!(s.action(word(Key::Q), 0), Some(STRAFE_LEFT));
    assert_eq!(
        s.action(word(Key::A), META_SHIFT),
        Some(TURN_LEFT),
        "no Shift map of the player's"
    );
    assert_eq!(s.action(word(Key::Tab), META_SHIFT), Some(0x4100_0019));
    assert_eq!(s.action(word(Key::Tab), 0), Some(0x4100_001A));
    assert_eq!(s.action(word(Key::Space), META_CTRL), Some(JUMP));
    assert_eq!(s.action(word(Key::Z), 0), None);
    assert_eq!(
        s.find(word(Key::ShiftLeft), 0),
        Some((0, SHIFT_KEY)),
        "the base map"
    );
    keys.stack.set_interface(Vec::new());
    assert_eq!(
        [Key::A, Key::Q, Key::O].map(|k| keys.stack.action(word(k), 0)),
        before
    );
    assert!(layers(Choice::Retail, &keys.records, &HostFiles(&mut host)).is_empty());
}

/// An interface's own maps as its bindings page applies them are written to its own files, which
/// are its layers from then on (Z runs forward, W with Shift backs away, W alone is no longer
/// Horizon's); the player's own files are left as they were, and the retail interface, which has
/// no maps of its own, writes none.
///
/// Behaviour: desktop.keys.an-interfaces-own-maps-are-written-to-its-files
#[test]
fn an_interfaces_own_maps_are_written_to_its_files_and_read_back_as_its_layers() {
    use auberean_shell::interface::Choice;
    use auberean_shell::interface_keys::{layers, write, HORIZON_FILES};
    let mut host = TestHost::default();
    let mut keys = start(&mut host);
    let theirs = host.files.clone();
    let (w, z) = (Key::W.input_word(), Key::Z.input_word());
    let maps: [&[(u32, u32)]; 3] = [&[(z, FORWARD)], &[(w, BACKWARD)], &[]];
    assert!(write(
        Choice::Horizon,
        maps,
        &keys.records,
        &mut HostFiles(&mut host)
    ));
    for f in HORIZON_FILES {
        assert!(host.files.contains_key(f), "{f} written");
    }
    let mut left = host.files.clone();
    left.retain(|name, _| !HORIZON_FILES.contains(&name.as_str()));
    assert_eq!(left, theirs, "the player's files left alone");
    let own = layers(Choice::Horizon, &keys.records, &HostFiles(&mut host));
    keys.stack.set_interface(own);
    assert_eq!(keys.stack.action(z, 0), Some(FORWARD));
    assert_eq!(keys.stack.action(w, META_SHIFT), Some(BACKWARD));
    assert_eq!(keys.stack.action(w, 0), None);
    assert!(!write(
        Choice::Retail,
        maps,
        &keys.records,
        &mut HostFiles(&mut host)
    ));
}

/// A key bound to one of this client's own actions (Horizon's V, looking at the front) is held by
/// the world's controls while it is down and never sent to the client as a control; letting it go,
/// or the window losing the keyboard, ends it. The mouse buttons the controls hear are held
/// likewise for the orbit camera.
///
/// Behaviour: desktop.keys.own-actions-are-held-by-the-world-controls
#[test]
fn an_own_action_is_held_by_the_world_controls_and_never_sent() {
    use auberean_shell::input::Button;
    use auberean_shell::interface::Choice;
    use auberean_shell::interface_keys::layers;
    let mut host = TestHost::default();
    let mut keys = start(&mut host);
    let own = layers(Choice::Horizon, &keys.records, &HostFiles(&mut host));
    keys.stack.set_interface(own);
    let mut c = Rig::over(&keys);
    key(&mut c, &keys, Key::V, true);
    assert!(c.device.take_actions().is_empty(), "nothing sent");
    assert!(c.intent().front_view, "held");
    key(&mut c, &keys, Key::V, false);
    assert!(!c.intent().front_view, "let go");
    key(&mut c, &keys, Key::V, true);
    c.device.event(
        &Event::Button {
            button: Button::Right,
            down: true,
        },
        &keys,
    );
    let i = c.intent();
    assert!(i.front_view && i.buttons == (false, true));
    c.device.event(&Event::Focus(false), &keys);
    let i = c.intent();
    assert!(
        !i.front_view && i.buttons == (false, false),
        "the window lost the keyboard"
    );
}

/// Horizon's layer for one modifier (0, Shift or Ctrl) when the player has applied no Horizon
/// keys of their own, as `(input, action)` pairs.
fn horizon_fresh(meta: u32) -> Vec<(u32, u32)> {
    use auberean_shell::interface::Choice;
    use auberean_shell::interface_keys::layers;
    let mut host = TestHost::default();
    let keys = start(&mut host);
    let own = layers(Choice::Horizon, &keys.records, &HostFiles(&mut host));
    let layer = own.iter().find(|l| l.meta == meta).expect("a layer");
    layer
        .map
        .bindings
        .iter()
        .map(|b| (b.input, b.action))
        .collect()
}

/// With no Horizon keys of the player's own, Horizon's keys alone are the shipped map's 57: W runs
/// forward, Q strafes, V looks at the front, N opens crafting, B and I the inventory, G and X the
/// examine window, Enter and the numpad's Enter the chat entry, F1 selects the first fellow and 0
/// uses the tenth slot; Stop has no key.
///
/// Behaviour: desktop.keys.horizon-starts-with-its-shipped-key-maps
#[test]
fn horizons_keys_alone_are_its_shipped_map_with_x_examining() {
    use auberean_shell::interface_keys::own;
    let map = horizon_fresh(0);
    let bound = |k: Key| {
        map.iter()
            .find(|(i, _)| *i == k.input_word())
            .map(|(_, a)| *a)
    };
    assert_eq!(map.len(), 57);
    assert_eq!(bound(Key::W), Some(FORWARD));
    assert_eq!(bound(Key::Q), Some(STRAFE_LEFT));
    assert_eq!(bound(Key::V), Some(own::LOOK_AT_FRONT));
    assert_eq!(bound(Key::N), Some(own::CRAFTING));
    assert_eq!(bound(Key::B), Some(INVENTORY));
    assert_eq!(bound(Key::I), Some(INVENTORY));
    assert_eq!(bound(Key::Enter), Some(0x4000_0017));
    assert_eq!(bound(Key::NumpadEnter), Some(0x4000_0017));
    assert_eq!(bound(Key::F1), Some(0x4100_0053));
    assert_eq!(bound(Key::Digit0), Some(0x4100_0034));
    assert_eq!(bound(Key::G), Some(EXAMINATION));
    assert_eq!(bound(Key::X), Some(EXAMINATION), "X examines, as G does");
    assert!(map.iter().all(|(_, a)| *a != STOP), "Stop has no key");
}

/// With no Horizon keys of the player's own, Horizon's Shift map is the shipped one's six keys:
/// Shift+Tab and Shift+P pick the previous creature and pet, Shift+Escape logs out, and Shift+F2
/// to F4 show the second to the fourth chat window.
///
/// Behaviour: desktop.keys.horizon-starts-with-its-shipped-key-maps
#[test]
fn horizons_shift_map_is_its_shipped_one() {
    let w = |k: Key| k.input_word();
    assert_eq!(
        horizon_fresh(META_SHIFT),
        [
            (w(Key::Tab), 0x4100_0019),
            (w(Key::P), 0x4100_005C),
            (w(Key::Escape), 0x4100_000E),
            (w(Key::F2), 0x4100_0042),
            (w(Key::F3), 0x4100_0043),
            (w(Key::F4), 0x4100_0044),
        ]
    );
}

/// With no Horizon keys of the player's own, Horizon's Ctrl map is the shipped one's ten keys:
/// Ctrl+1 to Ctrl+9 and Ctrl+0 use the shortcut bar's second row, slot by slot, and nothing else.
///
/// Behaviour: desktop.keys.horizon-starts-with-its-shipped-key-maps
#[test]
fn horizons_ctrl_map_is_its_shipped_one() {
    let digits = [
        Key::Digit1,
        Key::Digit2,
        Key::Digit3,
        Key::Digit4,
        Key::Digit5,
        Key::Digit6,
        Key::Digit7,
        Key::Digit8,
        Key::Digit9,
        Key::Digit0,
    ];
    let want: Vec<(u32, u32)> = digits
        .iter()
        .zip(0x4100_005F..)
        .map(|(k, a)| (k.input_word(), a))
        .collect();
    assert_eq!(horizon_fresh(META_CTRL), want);
}
