//! Horizon's credits from the install's records: the lines the credits' layout names, and the
//! credits screen showing them once Credits is pressed.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use auberean_horizon::ui::pregame::{Screen, SelectLayout};
use auberean_shell::input::{Button, Event};

use crate::session::{data, Session};

const HORIZON: &str = "[UI]\r\nInterface=Horizon\r\n";

#[test]
fn the_credits_lines_come_from_the_install_s_records() {
    let (_, _, records) = data();
    let lines = auberean_horizon::credits(records.as_ref());
    assert!(
        lines.iter().filter(|l| !l.trim().is_empty()).count() > 20,
        "the credits' lines: {lines:?}"
    );
}

#[test]
fn the_credits_screen_shows_the_credits_lines_once_credits_is_pressed() {
    let mut s = Session::choosing(Some(HORIZON));
    let listed = s.until(10.0, |s| {
        s.horizon().borrow().screen() == Some(Screen::Select)
    });
    assert!(listed, "character select up");
    #[allow(clippy::cast_precision_loss)]
    let screen = (s.shell.size.0 as f32, s.shell.size.1 as f32);
    let l = SelectLayout::new(screen, auberean_horizon::scale::layout_scale(screen));
    s.shell.event(&Event::Pointer {
        x: l.credits.x + l.credits.w / 2.0,
        y: l.credits.y + l.credits.h / 2.0,
    });
    s.step();
    for down in [true, false] {
        s.shell.event(&Event::Button {
            button: Button::Left,
            down,
        });
        s.step();
    }
    assert_eq!(s.horizon().borrow().screen(), Some(Screen::Credits));
    s.step();
    let h = s.horizon();
    let h = h.borrow();
    assert!(
        h.credits().iter().any(|l| !l.trim().is_empty()),
        "the credits shown have lines"
    );
}
