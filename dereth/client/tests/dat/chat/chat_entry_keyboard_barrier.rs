//! The chat entry's keyboard barrier follows its focus: focusing the chat entry registers the
//! focused-text maps and the keyboard barrier (pseudo-map 1, `MAP_BLOCK_KEYBOARD`) at 2990, ten
//! below the 3000 of the focused-text maps, so a typed `w` inserts text and does not walk the
//! character; losing focus removes them again. The map walk itself is asserted in
//! `dereth_client::input` (`the_typing_barrier_stops_w_walking_only_while_a_text_box_has_focus`);
//! this module adds the half that needs a live element tree: clicking the entry registers the maps
//! and blurring it takes them away.
//!
//! Fixture: a headless App with no shard link, driven through `Pump` and the input shell; every
//! assertion is read back out of this process and every fixture path is an `expect`.

use crate::common::sim_app::app_in_gameplay;

use dereth_client::app::App;

use dereth_client::pump::{Pump, Win32Message};

use dereth_ui::{ElemHandle, ElementId, UiSystem};
use dereth_ui_screens::chat::window::ENTRY;
use dereth_ui_screens::screens::gameplay::GamePlayScreen;
use winit::event::MouseButton;

fn gameplay(app: &mut App) -> (&mut UiSystem, &mut GamePlayScreen) {
    let shell = app.ui_mut().expect("shell");
    let ui = &mut shell.ui;
    let screen = shell.flow.current_mut().expect("a screen is up");
    let any: &mut dyn std::any::Any = &mut **screen;
    (
        ui,
        any.downcast_mut::<GamePlayScreen>()
            .expect("the gameplay screen"),
    )
}

fn find(app: &App, id: ElementId) -> ElemHandle {
    let shell = app.ui().expect("shell");
    let root = shell.flow.current().expect("a screen").roots()[0];
    shell
        .ui
        .get_child_recursive(root, id)
        .unwrap_or_else(|| panic!("{id:?} is in the shipped layout"))
}

fn centre(app: &App, h: ElemHandle) -> (i32, i32) {
    let b = app.ui().expect("shell").ui.screen_box(h);
    ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
}

/// Keyboard and pointer messages passed to `Pump` and the input shell; no operating-system
/// window procedure is invoked.
struct Hand {
    pump: Pump,
    time_ms: u32,
}

impl Hand {
    fn new() -> Self {
        let mut pump = Pump::new();
        pump.state.is_ready = true;
        pump.state.is_active_app = true;
        Self {
            pump,
            time_ms: 200_000,
        }
    }

    fn send(&mut self, app: &mut App, m: Win32Message) {
        self.pump.dispatch(m);
        if let Some(input) = app.input_manager_mut() {
            input.on_message(m);
        }
    }

    fn click(&mut self, app: &mut App, at: (i32, i32)) {
        self.time_ms += 10;
        let m = self
            .pump
            .mouse_move_message(f64::from(at.0), f64::from(at.1), self.time_ms);
        self.send(app, m);
        for pressed in [true, false] {
            self.time_ms += 10;
            let m = self
                .pump
                .mouse_button_message(MouseButton::Left, pressed, self.time_ms)
                .expect("the left button is in the 0x201 block");
            self.send(app, m);
        }
        app.frame();
    }
}

/// Whether the keyboard barrier (`MAP_BLOCK_KEYBOARD`) is in the manager's map stack right now.
///
/// Focused text registers the barrier at 2990, ten below the focus priority 3000 (the focus
/// priority minus 10). Looking only at `FOCUSED_UI` would miss a barrier that is present and
/// invert the assertions, so this predicate checks both the map and its barrier priority.
fn barrier_registered(app: &mut App) -> bool {
    app.input_manager_mut()
        .expect("the input shell exists in a UI build")
        .manager
        .maps
        .entries()
        .iter()
        .any(|e| {
            e.map == dereth_input::MAP_BLOCK_KEYBOARD
                && e.priority == dereth_input::dispatch::priority::FOCUSED_UI_TEXT_BARRIER
        })
}

// -------------------------------------------------------------------------------------------
// 1. The barrier arrives with the focus and leaves with it
// -------------------------------------------------------------------------------------------

/// Behaviour: focus.press.typing-stops-when-the-keyboard-leaves-the-entry
///
/// Clicking the chat box is what puts the barrier up, and only while it holds focus.
///
/// Focus installs maps 1, 7, 8 along with scrollable map 0x0A; losing focus removes them. Both
/// directions matter: registering the barrier unconditionally would block movement forever
/// while passing a one-sided presence check.
///
/// Equal-priority insertions prepend, and the barrier's priority is ten below the other maps.
/// Thus registering `0x0A, 1, 7, 8` walks as `8, 7, 0x0A, 1`: three maps at 3000, newest first, then
/// the barrier at 2990. The text maps must remain reachable before keyboard dispatch stops.
#[test]
fn the_keyboard_barrier_follows_the_chat_boxs_focus() {
    let mut app = app_in_gameplay(4);
    let entry = find(&app, ENTRY);
    let at = centre(&app, entry);

    assert!(
        !barrier_registered(&mut app),
        "nothing is focused, so nothing blocks the keyboard"
    );

    let mut hand = Hand::new();
    hand.click(&mut app, at);
    {
        let (ui, _) = gameplay(&mut app);
        assert_eq!(
            ui.focus_element(),
            Some(entry),
            "the click focused the entry box"
        );
    }
    assert!(
        barrier_registered(&mut app),
        "focusing the box put the barrier up"
    );

    // ...and 7 and 8 are in front of it, which is what makes them reachable.
    //
    // Text focus first registers scrollable map 0x0A unconditionally; equal-priority insertions
    // prepend and the barrier uses the focus priority minus 10, so the run is `8, 7, 0x0A, 1`.
    //
    // Reachability does not itself establish a particular Ctrl-arrow binding: the input-module
    // test `map_0x0a_answers_the_wheel_and_not_ctrl_arrow_while_a_text_box_has_focus` covers
    // those bindings. Lower-priority gameplay movement and the lowest-priority map 3 remain
    // blocked; the `w` key itself is the input-module test's subject, not this one's.
    let band: Vec<u32> = app
        .input_manager_mut()
        .expect("the input shell")
        .manager
        .maps
        .entries()
        .iter()
        .filter(|e| {
            // Two priorities: 3000 for 0x0A/7/8 and 2990 for the barrier. A filter on 3000
            // alone would drop map 1 and the band would read `8, 7, 0x0A` whatever the barrier
            // did.
            e.priority == dereth_input::dispatch::priority::FOCUSED_UI
                || e.priority == dereth_input::dispatch::priority::FOCUSED_UI_TEXT_BARRIER
        })
        .map(|e| e.map.0)
        .collect();
    assert_eq!(
        band,
        vec![8, 7, 0x0A, 1],
        "8, 7 and 0x0A prepend at 3000, then the barrier sorts in below them at 2990: {band:?}"
    );
    let barrier = band
        .iter()
        .position(|m| *m == dereth_input::MAP_BLOCK_KEYBOARD.0)
        .expect("the barrier is in the band");
    assert!(
        band[..barrier].contains(&7) && band[..barrier].contains(&8),
        "7 and 8 are in front of the barrier, which is what makes them reachable: {band:?}"
    );
    assert!(
        band[..barrier].contains(&0x0A),
        "and the scrollable map is in FRONT of it, so Ctrl+DIK_UP/DOWN scroll the \
         focused element rather than being swallowed: {band:?}"
    );
    assert_eq!(
        barrier,
        band.len() - 1,
        "the barrier is the last entry in the run: {band:?}"
    );

    // The chat child-action Escape arm (0x27) drops focus; removal of that callback's maps
    // follows. This call exercises the action consumer directly, rather than typing Escape.
    {
        let (ui, screen) = gameplay(&mut app);
        let ev = dereth_ui::focus::InputEvent {
            action: 0x27,
            start: true,
            x: 0,
            y: 0,
        };
        assert!(
            screen.chat_on_child_action(ui, entry, &ev),
            "the Escape arm consumed it"
        );
    }
    app.frame();
    assert!(
        !barrier_registered(&mut app),
        "losing focus took the barrier with it"
    );
    app.shutdown();
}
