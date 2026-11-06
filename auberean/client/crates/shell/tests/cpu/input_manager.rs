//! What reaches the interface and the world's controls: the window's being active and the pointer's
//! being held decide.

use auberean_shell::input::{Button, Event, Key};
use auberean_shell::input_manager::InputManager;

fn press() -> Event {
    Event::Key {
        key: Key::W,
        down: true,
        repeat: false,
    }
}

fn pointer() -> Event {
    Event::Pointer { x: 10.0, y: 20.0 }
}

/// The press that brought the window forward isn't heard, nor its release, and the next frame's
/// click is. A press in a window already active is, and a window that was never left doesn't count
/// as coming active.
///
/// Behaviour: desktop.input.a-click-that-brings-the-window-forward-is-not-heard
#[test]
fn a_click_that_brings_the_window_forward_is_not_heard() {
    let button = |button, down| Event::Button { button, down };
    let mut m = InputManager::default();
    assert!(m.admit(&Event::Focus(true)), "the first focus, never left");
    assert!(m.admit(&button(Button::Left, true)));
    assert!(m.admit(&button(Button::Left, false)));

    assert!(m.admit(&Event::Focus(false)));
    assert!(m.admit(&Event::Focus(true)));
    assert!(!m.admit(&button(Button::Left, true)), "the press");
    m.frame_begins();
    assert!(
        m.admit(&button(Button::Right, true)),
        "another button, now the devices are up"
    );
    assert!(!m.admit(&button(Button::Left, false)), "its release");
    assert!(m.admit(&button(Button::Left, true)), "the next click");
    assert!(m.admit(&button(Button::Left, false)));

    assert!(m.admit(&Event::Focus(false)));
    assert!(m.admit(&Event::Focus(true)));
    m.frame_begins();
    assert!(
        m.admit(&button(Button::Left, true)),
        "a click frames after the window came forward"
    );
}

/// While the window is not the active one nothing hears a press or a release, the wheel, mouse
/// motion or the pointer; the window's own events always pass. Where the pointer went is heard
/// once, the last place, when the window is active again.
///
/// Behaviour: desktop.input.an-inactive-window-hears-no-device
#[test]
fn an_inactive_window_hears_no_device_and_then_the_pointer_where_it_went() {
    let mut m = InputManager::default();
    assert!(m.admit(&press()));
    assert!(m.admit(&pointer()));
    assert!(m.admit(&Event::Focus(false)));
    for e in [
        press(),
        Event::Key {
            key: Key::W,
            down: false,
            repeat: false,
        },
        Event::Button {
            button: Button::Left,
            down: true,
        },
        Event::Button {
            button: Button::Left,
            down: false,
        },
        Event::Wheel { notches: 1.0 },
        Event::Motion { dx: 1.0, dy: 1.0 },
        Event::Pointer { x: 1.0, y: 2.0 },
        pointer(),
    ] {
        assert!(!m.admit(&e), "{e:?}");
    }
    for e in [Event::Text("a".into()), Event::Close] {
        assert!(m.admit(&e), "{e:?}");
    }
    assert_eq!(m.take_missed(), None, "not yet");
    assert!(m.admit(&Event::Focus(true)));
    assert_eq!(m.take_missed(), Some((10.0, 20.0)));
    assert_eq!(m.take_missed(), None, "once");
    assert!(m.admit(&press()));
}

/// While the pointer is held nothing hears where it is, though the keys are still heard; taking
/// hold of it puts it at the middle of the display, to the whole pixel below; giving it back moves
/// nothing; asking for what already is changes nothing.
///
/// Behaviour: desktop.input.mouse-look-holds-the-pointer-at-the-middle
#[test]
fn mouse_look_holds_the_pointer_at_the_middle_of_the_display() {
    let mut m = InputManager::default();
    assert_eq!(
        m.set_mouse_look_mode(false, (801, 601)),
        None,
        "already free"
    );
    assert_eq!(
        m.set_mouse_look_mode(true, (801, 601)),
        Some(Some((400.0, 300.0)))
    );
    assert!(m.pointer_held());
    assert_eq!(
        m.set_mouse_look_mode(true, (801, 601)),
        None,
        "already held"
    );
    assert!(!m.admit(&pointer()));
    assert!(m.admit(&press()));
    assert_eq!(m.take_missed(), None, "a held pointer is not missed");
    assert_eq!(m.set_mouse_look_mode(false, (801, 601)), Some(None));
    assert!(!m.pointer_held());
    assert!(m.admit(&pointer()));
}
