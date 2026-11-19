//! A click on Horizon's window while the window is in the background: it reaches Horizon, the
//! click that brings the window forward among them.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use auberean_horizon::ui::pregame::{Screen, SelectLayout};
use auberean_shell::input::{Button, Event};

use crate::session::Session;

const HORIZON: &str = "[UI]\r\nInterface=Horizon\r\n";

#[test]
fn a_first_click_on_horizon_s_window_in_the_background_reaches_it() {
    let mut s = Session::choosing(Some(HORIZON));
    let listed = s.until(10.0, |s| {
        s.horizon().borrow().screen() == Some(Screen::Select)
    });
    assert!(listed, "character select up");
    s.shell.event(&Event::Focus(false));
    s.step();
    #[allow(clippy::cast_precision_loss)]
    let screen = (s.shell.size.0 as f32, s.shell.size.1 as f32);
    let l = SelectLayout::new(screen, auberean_horizon::scale::layout_scale(screen));
    s.shell.event(&Event::Pointer {
        x: l.credits.x + l.credits.w / 2.0,
        y: l.credits.y + l.credits.h / 2.0,
    });
    s.step();
    // The window comes forward with the press, in one pass over its messages.
    s.shell.event(&Event::Focus(true));
    s.shell.event(&Event::Button {
        button: Button::Left,
        down: true,
    });
    s.step();
    s.shell.event(&Event::Button {
        button: Button::Left,
        down: false,
    });
    s.step();
    assert_eq!(
        s.horizon().borrow().screen(),
        Some(Screen::Credits),
        "the click pressed Credits"
    );
}
