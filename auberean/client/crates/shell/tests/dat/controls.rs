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
