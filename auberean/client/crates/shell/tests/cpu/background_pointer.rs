//! The pointer over a window in the background, for an interface that asks to hear it: its
//! buttons, the wheel and where it is are heard while the window is not the active one, and the
//! click that brings the window forward counts; the keys still wait for the window.
//!
//! Behaviour: none (this client's own interface's rule; the game's own interface keeps the game's)

use auberean_shell::input::{Button, Event, Key};
use auberean_shell::input_manager::InputManager;

fn button(button: Button, down: bool) -> Event {
    Event::Button { button, down }
}

#[test]
fn the_click_that_brings_the_window_forward_counts_when_the_pointer_is_heard_in_the_background() {
    let mut m = InputManager::default();
    m.hear_the_pointer_in_background(true);
    assert!(m.admit(&Event::Focus(false)));
    assert!(m.admit(&Event::Pointer { x: 10.0, y: 20.0 }), "where it is");
    assert!(m.admit(&Event::Wheel { notches: 1.0 }), "the wheel");
    assert!(
        !m.admit(&Event::Key {
            key: Key::W,
            down: true,
            repeat: false,
        }),
        "no key while in the background"
    );
    assert!(
        m.admit(&button(Button::Left, true)),
        "a press in the background"
    );
    assert!(m.admit(&button(Button::Left, false)));
    assert_eq!(m.take_missed(), None, "nothing missed");
    // The press that brings the window forward, in the same pass as its coming active.
    assert!(m.admit(&Event::Focus(true)));
    assert!(m.admit(&button(Button::Left, true)), "the activating press");
    assert!(m.admit(&button(Button::Left, false)), "and its release");

    // Without it, the game's rule: neither is heard.
    m.hear_the_pointer_in_background(false);
    assert!(m.admit(&Event::Focus(false)));
    assert!(!m.admit(&button(Button::Left, true)));
    assert!(m.admit(&Event::Focus(true)));
    assert!(!m.admit(&button(Button::Left, true)));
}
