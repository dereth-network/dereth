//! The player's controls carried by the client: pressed and let go through the contract as a key
//! presses them, over a small action map of their own read from the data files.

use std::sync::Arc;

use auberean_assets::ui::{ActionBindings, ActionEntry};
use auberean_client_contract::control::{
    camera, class, interface, motion, movement, toggle, CameraPress, Control, ControlId,
    ControlSource,
};
use auberean_client_contract::Action;
use auberean_client_runtime::controls::{Controls, ACTION_MAP};
use auberean_client_runtime::data::{DataFiles, ENGLISH};
use auberean_client_runtime::{Client, ClientData, ClientSettings};
use auberean_dat::DatFile;

use super::download::{file, file_with};

const PORTAL: u64 = 0x1_0000_0000;
const CELL: u64 = 0x2_0000_0001;

const SHIFT: ControlId = ControlId(0x4000_000E);
const INVENTORY: ControlId = ControlId(0x4100_0002);
const ROTATE: ControlId = ControlId(0x4000_002E);
const RESET: ControlId = ControlId(0x4000_0030);
const MOUSE_LOOK: ControlId = ControlId(0x4100_0060);
const USE: ControlId = ControlId(0x4100_000D);

fn entry(id: ControlId, modifiers: u32, class: u32, value: u32, extent: f32) -> ActionEntry {
    ActionEntry {
        action: id.0,
        modifiers,
        class,
        value,
        extent,
    }
}

/// A small game: forward (Shift walks), backward, a turn, a strafe, a jump, a run lock, Shift, the
/// inventory, a camera turn and its reset, mouse look and use.
fn map() -> ActionBindings {
    ActionBindings {
        toggles: vec![
            (movement::FORWARD.0, toggle::HELD),
            (movement::BACKWARD.0, toggle::HELD),
            (movement::TURN_LEFT.0, toggle::HELD),
            (movement::STRAFE_LEFT.0, toggle::HELD),
            (movement::JUMP.0, toggle::IMPULSE),
            (movement::RUN_LOCK.0, toggle::LATCH),
            (SHIFT.0, toggle::HELD),
            (INVENTORY.0, toggle::HELD),
            (ROTATE.0, toggle::HELD),
            (RESET.0, toggle::HELD),
            (MOUSE_LOOK.0, toggle::IMPULSE),
            (USE.0, toggle::IMPULSE),
        ],
        actions: vec![
            entry(movement::FORWARD, 0, class::MOVEMENT, motion::FORWARD, 1.0),
            entry(
                movement::FORWARD,
                1,
                class::MOVEMENT,
                motion::FORWARD,
                0.339,
            ),
            entry(
                movement::BACKWARD,
                0,
                class::MOVEMENT,
                motion::FORWARD,
                -1.0,
            ),
            entry(
                movement::TURN_LEFT,
                0,
                class::MOVEMENT,
                motion::TURN_LEFT,
                1.0,
            ),
            entry(
                movement::STRAFE_LEFT,
                0,
                class::MOVEMENT,
                motion::STRAFE_LEFT,
                1.0,
            ),
            entry(movement::JUMP, 0, class::MOVEMENT, motion::JUMP, 0.0),
            entry(movement::RUN_LOCK, 0, class::MOVEMENT, motion::FORWARD, 1.0),
            entry(SHIFT, 0, class::MODIFIER, 1, 0.0),
            entry(INVENTORY, 0, class::INTERFACE, INVENTORY.0, 0.0),
            entry(ROTATE, 0, class::CAMERA, camera::ROTATE_LEFT, 0.0),
            entry(RESET, 0, class::CAMERA, camera::RESET, 0.0),
            entry(
                MOUSE_LOOK,
                0,
                class::INTERFACE,
                camera::TOGGLE_MOUSE_LOOK,
                0.0,
            ),
            entry(USE, 0, class::INTERFACE, interface::USE, 0.0),
        ],
        ..ActionBindings::default()
    }
}

fn portal(records: &[(u32, Vec<u8>)]) -> Arc<DatFile> {
    file_with(PORTAL, records)
}

fn client(portal: Arc<DatFile>) -> Client {
    Client::new(
        ClientSettings {
            login_server: "127.0.0.1:9000".parse().expect("an address"),
            account: b"a".to_vec(),
            ticket: Vec::new(),
            timestamp: 0x5000_0000,
            session_key: [0; 64],
            script_sends: false,
        },
        ClientData {
            files: DataFiles {
                files: vec![portal, file(CELL, &[])],
                language: ENGLISH,
            },
            scripts: None,
        },
    )
}

/// A client whose portal file holds the small game's action map.
fn client_with_map() -> Client {
    let mut w = auberean_dat::Writer::new();
    map().write(ACTION_MAP, &mut w);
    client(portal(&[(ACTION_MAP, w.finish())]))
}

fn press(c: &mut Client, id: ControlId) {
    c.act(Action::Control(Control::begin(id)));
}

fn let_go(c: &mut Client, id: ControlId) {
    c.act(Action::Control(Control::end(id)));
}

/// Controls pressed through the contract are carried as the data's action map says each toggles:
/// a held one moves the player from its press to its release, a latched run goes on after its
/// release until it is pressed again or the player goes forward or backward, a jump is one press
/// for its frame however long it is held, and a modifier held chooses the values (Shift walks
/// forward at 0.339 and turns the camera faster). The movement controls begun are told once. A
/// client whose data has no action map carries none.
///
/// Behaviour: session.controls.held-latched-and-single-press-controls-through-the-contract
#[test]
fn held_latched_and_single_press_controls_through_the_contract() {
    let mut bare = client(portal(&[]));
    press(&mut bare, movement::FORWARD);
    assert!(
        bare.controls.in_force().is_empty(),
        "no action map, no control"
    );

    let mut c = client_with_map();
    press(&mut c, movement::FORWARD);
    let f = c.take_control_frame();
    assert_eq!(
        (f.forward, f.started.as_slice()),
        (1.0, &[movement::FORWARD][..])
    );
    let f = c.take_control_frame();
    assert_eq!((f.forward, f.started.len()), (1.0, 0), "held, begun once");
    let_go(&mut c, movement::FORWARD);
    assert_eq!(c.take_control_frame().forward, 0.0, "let go");

    press(&mut c, movement::RUN_LOCK);
    let_go(&mut c, movement::RUN_LOCK);
    assert!(c.controls.autorun());
    press(&mut c, movement::TURN_LEFT);
    let f = c.take_control_frame();
    assert_eq!((f.forward, f.turn), (1.0, 1.0), "turning keeps the run");
    let_go(&mut c, movement::TURN_LEFT);
    press(&mut c, movement::RUN_LOCK);
    assert!(!c.controls.autorun(), "pressed again, it ends");
    press(&mut c, movement::RUN_LOCK);
    press(&mut c, movement::BACKWARD);
    assert!(!c.controls.autorun(), "backing ends it");
    assert_eq!(c.take_control_frame().forward, -1.0);
    let_go(&mut c, movement::BACKWARD);

    press(&mut c, movement::JUMP);
    assert!(c.take_control_frame().jump);
    assert!(!c.take_control_frame().jump, "one press, one jump");
    let_go(&mut c, movement::JUMP);

    press(&mut c, SHIFT);
    press(&mut c, movement::FORWARD);
    press(&mut c, ROTATE);
    let f = c.take_control_frame();
    assert_eq!(f.forward, 0.339, "Shift walks");
    assert!(f.camera.rotate_left && f.camera.fast);
    let_go(&mut c, SHIFT);
    let f = c.take_control_frame();
    assert_eq!(f.forward, 1.0);
    assert!(f.camera.rotate_left && !f.camera.fast);
}

/// A control's release ends the press its own source began, whatever else was pressed between:
/// forward pressed by a key and by itself is let go by itself and still runs on the key's press;
/// the key's press of another control takes the place of what it pressed before, and the key's
/// release ends that.
///
/// Behaviour: session.controls.a-release-ends-the-press-its-source-began
#[test]
fn a_release_ends_the_press_its_source_began() {
    let key = ControlSource {
        device: 0,
        input: 0x8100_0411,
    };
    let mut c = Controls::over(map());
    c.control(Control::begin(movement::FORWARD).from(key));
    c.control(Control::begin(movement::FORWARD));
    c.control(Control::end(movement::FORWARD));
    assert_eq!(c.frame().forward, 1.0, "the key's press stays");
    c.control(Control::begin(movement::STRAFE_LEFT).from(key));
    let f = c.frame();
    assert_eq!(
        (f.forward, f.sideways),
        (0.0, -1.0),
        "the key's press replaced"
    );
    c.control(Control::end(movement::FORWARD).from(key));
    assert!(
        c.in_force().is_empty(),
        "a release from the key ends what it pressed"
    );
    c.control(Control::at(movement::FORWARD, 0.0));
    assert!(c.in_force().is_empty(), "a press of 0 is a release");
}

/// Letting go of the controls in force: with a text entry taking the keys, the held ones end but
/// the modifiers' and the interface's, and a latched run and a single press stay; with the window's
/// keyboard gone, every held one ends and the run and the press still stay. Pressing mouse look
/// flips it and turns strafe while the mouse steers; reset presses the camera; use asks for its
/// use.
///
/// Behaviour: session.controls.letting-go-keeps-a-run-and-a-press-and-typing-keeps-modifiers
#[test]
fn letting_go_keeps_a_run_and_a_press_and_typing_keeps_modifiers() {
    let mut c = client_with_map();
    for id in [
        movement::RUN_LOCK,
        SHIFT,
        INVENTORY,
        movement::STRAFE_LEFT,
        movement::JUMP,
    ] {
        press(&mut c, id);
    }
    let_go(&mut c, movement::RUN_LOCK);
    c.act(Action::ReleaseControls { typing: true });
    for id in [movement::RUN_LOCK, SHIFT, INVENTORY, movement::JUMP] {
        assert!(c.controls.is_down(id), "{id:?} stays");
    }
    assert!(!c.controls.is_down(movement::STRAFE_LEFT), "held: let go");
    c.act(Action::ReleaseControls { typing: false });
    assert!(!c.controls.is_down(SHIFT) && !c.controls.is_down(INVENTORY));
    let f = c.take_control_frame();
    assert_eq!((f.forward, f.sideways, f.jump), (1.0, 0.0, true));
    press(&mut c, movement::BACKWARD);
    let_go(&mut c, movement::BACKWARD);

    c.controls.steers = true;
    press(&mut c, MOUSE_LOOK);
    press(&mut c, RESET);
    press(&mut c, USE);
    let f = c.take_control_frame();
    assert_eq!(f.presses, [CameraPress::MouseLook, CameraPress::Reset]);
    assert!(c.controls.mouse_look);
    press(&mut c, movement::TURN_LEFT);
    let f = c.take_control_frame();
    assert_eq!(
        (f.turn, f.sideways),
        (0.0, -1.0),
        "in mouse look a turn strafes"
    );
    let mut bare = Controls::over(map());
    bare.control(Control::begin(USE));
    assert_eq!(
        bare.take_asked(),
        [Action::UseSelected],
        "use asks for its use"
    );
}
