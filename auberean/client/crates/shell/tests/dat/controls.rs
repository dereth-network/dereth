//! The world's controls over the data's key maps and action map: with no key map file, the
//! "Defaults" maps, turned into the player's intent.

use std::sync::OnceLock;

use auberean_scene::camera::Press;
use auberean_scene::movement;
use auberean_shell::controls::{Command, Controls, INTERFACE};
use auberean_shell::host::{HostFiles, TestHost};
use auberean_shell::input::{Button, Event, Key};
use auberean_shell::keys::{KeyFiles, KeyRecords, Keys};

/// Shift held with forward: walking, as the action map's Shift entry for forward says.
const WALK: f32 = 0.339;
/// The base map's interface action for the Up arrow.
const UP_KEY: u32 = 0x4000_0015;
/// An action the interface class holds: the same arrow key.
const HELD_INTERFACE: u32 = UP_KEY;

/// The keys a player with no key map file starts with.
fn keys() -> &'static Keys {
    static KEYS: OnceLock<Keys> = OnceLock::new();
    KEYS.get_or_init(|| {
        let records = KeyRecords::read(&crate::session::data().0.records);
        Keys::start(
            records,
            KeyFiles::default(),
            &mut HostFiles(&mut TestHost::default()),
        )
    })
}

/// Controls whose mouse look steers the player, as the camera's option does by default.
fn controls() -> Controls {
    let mut c = Controls::default();
    c.steers = true;
    c
}

fn down(c: &mut Controls, key: Key) {
    c.event(
        &Event::Key {
            key,
            down: true,
            repeat: false,
        },
        keys(),
    );
}

fn up(c: &mut Controls, key: Key) {
    c.event(
        &Event::Key {
            key,
            down: false,
            repeat: false,
        },
        keys(),
    );
}

fn event(c: &mut Controls, e: &Event) {
    c.event(e, keys());
}

/// W runs forward, with Shift walk at 0.339 of running; S and X back up and Shift does not slow
/// them; A and D turn; Z and C strafe. The arrows are the interface's (the base map's); its
/// camera asks for their movement as keys would, Up running forward and Right turning.
///
/// Behaviour: desktop.controls.the-default-keys-move-the-body
#[test]
fn the_default_keys_move_the_body() {
    let mut c = controls();
    down(&mut c, Key::W);
    assert_eq!(c.intent(keys()).forward, 1.0);
    down(&mut c, Key::ShiftLeft);
    assert_eq!(c.intent(keys()).forward, WALK);
    up(&mut c, Key::W);
    down(&mut c, Key::X);
    assert_eq!(c.intent(keys()).forward, -1.0, "backing ignores Shift");
    up(&mut c, Key::X);
    up(&mut c, Key::ShiftLeft);
    assert_eq!(
        keys().stack.find(Key::ArrowUp.input_word(), 0),
        Some((0, UP_KEY)),
        "the base map's interface action"
    );
    down(&mut c, Key::ArrowUp);
    assert_eq!(c.intent(keys()).forward, 0.0, "not the world's controls'");
    up(&mut c, Key::ArrowUp);
    c.action_handler(keys(), movement::FORWARD, 1.0, INTERFACE, movement::FORWARD);
    assert_eq!(c.intent(keys()).forward, 1.0);
    c.action_handler(keys(), movement::FORWARD, 0.0, INTERFACE, movement::FORWARD);
    down(&mut c, Key::A);
    let i = c.intent(keys());
    assert_eq!((i.turn, i.sideways), (1.0, 0.0), "A turns left");
    up(&mut c, Key::A);
    c.action_handler(
        keys(),
        movement::TURN_RIGHT,
        1.0,
        INTERFACE,
        movement::TURN_RIGHT,
    );
    assert_eq!(c.intent(keys()).turn, -1.0);
    c.action_handler(
        keys(),
        movement::TURN_RIGHT,
        0.0,
        INTERFACE,
        movement::TURN_RIGHT,
    );
    down(&mut c, Key::Z);
    assert_eq!(c.intent(keys()).sideways, -1.0);
    down(&mut c, Key::C);
    assert_eq!(c.intent(keys()).sideways, 0.0, "both cancel");
}

/// Q or Num Lock latches running; forward or backward turns it off, strafing and turning do not.
/// Space or J jumps once a press, however long it is held.
///
/// Behaviour: desktop.controls.autorun-latches-and-a-jump-is-one-press
#[test]
fn autorun_latches_and_a_jump_is_one_press() {
    let mut c = controls();
    down(&mut c, Key::Q);
    up(&mut c, Key::Q);
    assert_eq!(c.intent(keys()).forward, 1.0, "running on its own");
    down(&mut c, Key::Z);
    down(&mut c, Key::D);
    assert_eq!(
        c.intent(keys()).forward,
        1.0,
        "strafing and turning keep it"
    );
    up(&mut c, Key::Z);
    up(&mut c, Key::D);
    down(&mut c, Key::S);
    up(&mut c, Key::S);
    assert_eq!(c.intent(keys()).forward, 0.0, "backing stops it");
    down(&mut c, Key::NumLock);
    up(&mut c, Key::NumLock);
    down(&mut c, Key::W);
    up(&mut c, Key::W);
    assert_eq!(c.intent(keys()).forward, 0.0, "forward stops it");
    down(&mut c, Key::Space);
    assert!(c.intent(keys()).jump);
    event(
        &mut c,
        &Event::Key {
            key: Key::Space,
            down: true,
            repeat: true,
        },
    );
    assert!(!c.intent(keys()).jump, "held, it does not jump again");
    up(&mut c, Key::Space);
    down(&mut c, Key::J);
    assert!(c.intent(keys()).jump);
}

/// Each movement key pressed begins its action of the key map once, held or not: forward,
/// backward, the strafes, the turns (A and D strafe in mouse look, where the interface's camera
/// runs on the left button), the jump and the run lock; other keys begin none.
///
/// Behaviour: desktop.controls.a-movement-key-begins-its-action-once
#[test]
fn a_movement_key_begins_its_action_once() {
    let mut c = controls();
    for k in [
        Key::W,
        Key::S,
        Key::Z,
        Key::C,
        Key::A,
        Key::D,
        Key::Space,
        Key::Q,
        Key::Numpad5,
    ] {
        down(&mut c, k);
    }
    assert_eq!(
        c.intent(keys()).started,
        vec![
            movement::FORWARD,
            movement::BACKWARD,
            movement::STRAFE_LEFT,
            movement::STRAFE_RIGHT,
            movement::TURN_LEFT,
            movement::TURN_RIGHT,
            movement::JUMP,
            movement::RUN_LOCK,
        ]
    );
    event(
        &mut c,
        &Event::Key {
            key: Key::A,
            down: true,
            repeat: true,
        },
    );
    assert!(
        c.intent(keys()).started.is_empty(),
        "held, it begins nothing more"
    );
    for k in [Key::W, Key::S, Key::Z, Key::C, Key::A, Key::D] {
        up(&mut c, k);
    }
    event(
        &mut c,
        &Event::Button {
            button: Button::Middle,
            down: true,
        },
    );
    c.intent(keys());
    down(&mut c, Key::D);
    // The interface's camera runs on the left button in mouse look.
    c.action_handler(keys(), movement::FORWARD, 1.0, INTERFACE, movement::FORWARD);
    assert_eq!(
        c.intent(keys()).started,
        vec![movement::TURN_RIGHT, movement::FORWARD],
        "D begins its turn (strafing in mouse look) and the camera's run begins forward"
    );
}

/// The keypad drives the camera: 4 and 6, 8 and 2, + and - while held (Shift faster); 5, 0, the
/// decimal point, 7, 9, 1, 3 and Enter press its views; the wheel steps it; the middle button
/// turns mouse look on and off, and while it is on A and D strafe and the left button runs.
///
/// Behaviour: desktop.controls.the-keypad-and-mouse-drive-the-camera
#[test]
fn the_keypad_and_mouse_drive_the_camera() {
    let mut c = controls();
    for k in [Key::Numpad4, Key::Numpad8, Key::NumpadAdd, Key::ShiftRight] {
        down(&mut c, k);
    }
    let h = c.intent(keys()).held;
    assert!(h.rotate_left && h.tilt_up && h.zoom_in && h.fast);
    assert!(!h.rotate_right && !h.tilt_down && !h.zoom_out);
    up(&mut c, Key::ShiftRight);
    let views = [
        (Key::Numpad5, Press::Reset),
        (Key::Numpad0, Press::Shoulder),
        (Key::NumpadDecimal, Press::FirstPerson),
        (Key::Numpad7, Press::TopDown),
        (Key::Numpad9, Press::Reverse),
        (Key::Numpad1, Press::SwingLeft),
        (Key::Numpad3, Press::SwingRight),
        (Key::NumpadEnter, Press::MapView),
    ];
    for (k, p) in views {
        down(&mut c, k);
        up(&mut c, k);
        assert_eq!(c.intent(keys()).presses, vec![p], "{k:?}");
    }
    event(&mut c, &Event::Wheel { notches: 1.0 });
    assert_eq!(c.intent(keys()).presses, vec![Press::Wheel(1)]);
    event(
        &mut c,
        &Event::Button {
            button: Button::Middle,
            down: true,
        },
    );
    assert_eq!(c.intent(keys()).presses, vec![Press::MouseLook]);
    assert!(c.mouse_look);
    down(&mut c, Key::A);
    c.action_handler(keys(), movement::FORWARD, 1.0, INTERFACE, movement::FORWARD);
    event(&mut c, &Event::Motion { dx: 3.0, dy: -2.0 });
    let i = c.intent(keys());
    assert_eq!((i.turn, i.sideways, i.forward), (0.0, -1.0, 1.0));
    assert_eq!(i.mouse, (3.0, -2.0));
    assert_eq!(c.intent(keys()).mouse, (0.0, 0.0), "motion is taken once");
}

/// The selection keys select the next object and Escape clears it; F uses what is selected. A
/// window that loses the keyboard lets every key go.
///
/// Behaviour: desktop.controls.select-use-and-letting-go-on-focus-loss
#[test]
fn select_use_and_letting_go_on_focus_loss() {
    let mut c = controls();
    for k in [Key::BracketRight, Key::O, Key::Escape, Key::F] {
        down(&mut c, k);
        up(&mut c, k);
    }
    assert_eq!(
        c.take_commands(),
        vec![
            Command::SelectNext,
            Command::SelectNext,
            Command::SelectNone,
            Command::Use
        ]
    );
    down(&mut c, Key::W);
    event(&mut c, &Event::Focus(false));
    assert_eq!(c.intent(keys()).forward, 0.0);
}

/// Mouse look switching lets the actions in force go while a text entry has the keys, and leaves
/// them when none does.
///
/// Behaviour: desktop.controls.mouse-look-switching-while-typing-lets-actions-go
#[test]
fn mouse_look_switching_while_a_text_entry_has_the_keys_lets_actions_go() {
    const FORWARD: u32 = 0x4000_0020;
    let mut c = controls();
    c.action_handler(keys(), FORWARD, 1.0, 2, 0x8200_040C);
    c.mouse_look_switched(keys());
    assert!(c.action_down(FORWARD), "no text entry: it stays");
    c.set_text_mode(keys(), true);
    c.action_handler(keys(), FORWARD, 1.0, 2, 0x8200_040C);
    assert!(c.action_down(FORWARD), "begun with the entry up");
    c.mouse_look_switched(keys());
    assert!(!c.action_down(FORWARD), "let go");
}

/// A text entry taking the keys lets a held key's action go, as the key's release would, and leaves
/// a latched run, a single press for this frame and what the modifier and interface classes hold;
/// the key's release later finds nothing, a press after the entry gives the keys back works, and a
/// held mouse button is no key: its action stays until its release.
///
/// Behaviour: desktop.controls.text-mode-lets-held-actions-go
#[test]
fn a_text_entry_taking_the_keys_lets_held_actions_go_and_leaves_a_run_and_a_press() {
    let mut c = controls();
    down(&mut c, Key::W);
    down(&mut c, Key::Z);
    assert_eq!(c.intent(keys()).sideways, -1.0, "strafing");
    c.set_text_mode(keys(), true);
    let i = c.intent(keys());
    assert_eq!((i.forward, i.sideways), (0.0, 0.0), "held keys let go");
    assert!(c.actions().is_empty(), "{:?}", c.actions());
    up(&mut c, Key::W);
    up(&mut c, Key::Z);
    c.set_text_mode(keys(), false);
    assert_eq!(
        c.intent(keys()).forward,
        0.0,
        "giving the keys back starts nothing"
    );
    down(&mut c, Key::W);
    assert_eq!(c.intent(keys()).forward, 1.0, "the next press runs");
    up(&mut c, Key::W);

    down(&mut c, Key::Q);
    up(&mut c, Key::Q);
    down(&mut c, Key::Space);
    c.set_text_mode(keys(), true);
    let i = c.intent(keys());
    assert_eq!(i.forward, 1.0, "a latched run stays");
    assert!(i.jump, "a single press stays for its frame");
    c.set_text_mode(keys(), false);
    up(&mut c, Key::Space);
    down(&mut c, Key::W);
    up(&mut c, Key::W);

    c.action_handler(keys(), HELD_INTERFACE, 1.0, INTERFACE, HELD_INTERFACE);
    assert!(c.action_down(HELD_INTERFACE), "in force");
    c.set_text_mode(keys(), true);
    assert!(c.action_down(HELD_INTERFACE), "the interface class's stays");
    event(&mut c, &Event::Focus(false));
    assert!(
        !c.action_down(HELD_INTERFACE),
        "the window's keyboard gone takes it"
    );
    c.set_text_mode(keys(), false);

    // A mouse button is no key: its action, of the interface class, stays until its own release.
    event(
        &mut c,
        &Event::Button {
            button: Button::Left,
            down: true,
        },
    );
    let held: Vec<u32> = c.actions().iter().map(|a| a.action).collect();
    assert!(!held.is_empty(), "the left button holds an action");
    c.set_text_mode(keys(), true);
    for a in &held {
        assert!(c.action_down(*a), "{a:#x} stays");
    }
    event(
        &mut c,
        &Event::Button {
            button: Button::Left,
            down: false,
        },
    );
    assert!(c.actions().is_empty(), "its release ends it");
}

/// An action of the behavior class has the player play its behavior as it begins: the data's one
/// such action, which no default key names and which lasts one press, so never stops it. One held
/// has it stopped as it ends.
///
/// Behaviour: desktop.controls.a-behavior-action-plays-and-stops-its-behavior
#[test]
fn a_behavior_action_plays_and_stops_its_behavior() {
    use auberean_assets::ui::ActionEntry;
    use auberean_shell::controls::{class, toggle};
    const ACTION: u32 = 0x8000_0001;
    const BEHAVIOR: u32 = 0x4100_0007;
    let mut c = controls();
    c.action_handler(keys(), ACTION, 1.0, 1, 7);
    assert_eq!(c.intent(keys()).behaviors, [(BEHAVIOR, true)]);
    assert!(c.intent(keys()).behaviors.is_empty(), "once");

    const HELD: u32 = 0x8000_0002;
    let mut records = keys().records.clone();
    records.action_map.toggles.push((HELD, toggle::HELD));
    records.action_map.actions.push(ActionEntry {
        action: HELD,
        modifiers: 0,
        class: class::BEHAVIOR,
        value: BEHAVIOR,
        extent: 1.0,
    });
    let held = Keys::start(
        records,
        KeyFiles::default(),
        &mut HostFiles(&mut TestHost::default()),
    );
    let mut c = controls();
    c.action_handler(&held, HELD, 1.0, 1, 7);
    assert_eq!(c.intent(&held).behaviors, [(BEHAVIOR, true)]);
    c.action_handler(&held, HELD, 0.0, 1, 7);
    assert_eq!(c.intent(&held).behaviors, [(BEHAVIOR, false)]);
}

/// The action map's rows for modifier bit 8 give movement values no other row gives, yet no modifier
/// action sets that bit: with every modifier key held the bits are Shift, Ctrl and Alt alone, and
/// each movement action keeps its ordinary value.
///
/// Behaviour: desktop.controls.no-modifier-key-reaches-the-rows-for-bit-eight
#[test]
fn no_modifier_key_reaches_the_rows_for_bit_eight() {
    const MOVEMENT_CLASS: u32 = 0x4000_0001;
    const MODIFIER_CLASS: u32 = 0x4000_0005;
    let map = &keys().records.action_map;
    let hidden: Vec<(u32, u32)> = map
        .actions
        .iter()
        .filter(|e| e.modifiers == 8)
        .map(|e| (e.action, e.value))
        .collect();
    assert_eq!(
        hidden,
        vec![
            (0x4000_0021, 0x4000_0009),
            (0x4000_0022, 0x4000_000C),
            (0x4000_0023, 0x4000_000D),
            (0x4000_0024, 0x4000_000B),
            (0x4000_0025, 0x4000_000A),
        ],
    );
    let bits = map
        .actions
        .iter()
        .filter(|e| e.class == MODIFIER_CLASS)
        .fold(0, |m, e| m | e.value);
    assert_eq!(bits, 7, "the modifier actions set Shift, Ctrl and Alt");
    let mut c = controls();
    for key in [Key::ShiftLeft, Key::ControlLeft, Key::AltLeft] {
        down(&mut c, key);
    }
    let held = c.modifier_bitfield(keys());
    assert_eq!(held & 8, 0, "held: {held:#x}");
    assert_ne!(held, 0, "the keys took");
    for (action, value) in hidden {
        let v = keys().records.values(action, held).expect("its values");
        assert_eq!(v.class, MOVEMENT_CLASS);
        assert_ne!(v.value, value, "{action:#x}");
    }
}
