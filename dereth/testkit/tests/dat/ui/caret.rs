//! UI fixtures and scenarios for caret.

use super::*;
// -------------------------------------------------------------------------------------------
// ui.caret.flashes-at-the-interval-the-player-set-for-their-desktop
//
// **The client is asked, and the desktop is asked separately.** The interval the running client
// ends up with is compared against a fresh query of the desktop made here, so the reading is not
// derived from the value the client stored; and the client's own value is seeded with a
// deliberately wrong one first, so a client that never asked would fail rather than agree by
// accident on a machine whose setting happens to be this client's old default.
// -------------------------------------------------------------------------------------------

/// The chat entry of the shipped gameplay layout: a box that can be typed into, so it has a caret.
const CHAT_ENTRY_FOR_CARET: dereth_ui::ElementId = dereth_ui::ElementId(0x1000_0016);

/// An interval no desktop uses, seeded so that "the client replaced it" is a measurement.
const SENTINEL_INTERVAL: f64 = 17.0;

/// The caret flashes at the player's own desktop interval, and is drawn inside it and gone past it.
pub(super) fn the_caret_flashes_at_the_players_own_desktop_interval() {
    use dereth_desktop::platform::window::caret_blink_time_seconds;
    use dereth_primitives::LocalTime;
    use dereth_testkit::adapters_shell::element;

    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    let entry = element(&c, CHAT_ENTRY_FOR_CARET);
    let editable = {
        let shell = c.app_mut().ui_mut().expect("the shell");
        let editable = shell
            .ui
            .text_element_mut(entry)
            .expect("the chat entry is a text element")
            .bits
            .editable();
        shell.ui.take_focus(entry);
        shell.ui.caret_blink_time = SENTINEL_INTERVAL;
        editable
    };

    // One ordinary frame of the running client -- the real producer -- and then the desktop asked
    // again, here, independently.
    c.tick(1);
    let queried = caret_blink_time_seconds();
    let asked_the_desktop = c
        .view()
        .expect_app()
        .ui()
        .expect("the shell")
        .ui
        .caret_blink_time
        == queried
        && c.view()
            .expect_app()
            .ui()
            .expect("the shell")
            .ui
            .caret_blink_time
            != SENTINEL_INTERVAL;

    // The boundary itself, driven at a time the scenario names rather than by waiting: the shell's
    // own frame call is the same one the client makes, and it takes the clock as an argument, so
    // the exact edge is reachable without sleeping and without touching the player's setting.
    let host = c.view().expect_app().host_state().clone();
    let shell = c.app_mut().ui_mut().expect("the shell");
    let screen = shell.ui.screen_box(entry);
    let anchor = 1_000_000.0;
    let want = {
        let text = shell
            .ui
            .text_element_mut(entry)
            .expect("the chat entry is still live");
        text.last_flash_flip = anchor;
        text.caret_moved = false;
        text.caret_visible = true;
        let b = text
            .caret_box(screen)
            .expect("an editable, visible caret has a box");
        let colour = text.font_color;
        dereth_ui::UiFill {
            x: b.x0,
            y: b.y0,
            w: b.x1 - b.x0 + 1,
            h: b.y1 - b.y0 + 1,
            color: colour,
        }
    };
    let drawn = |shell: &mut dereth_client_shell::ui::UiShell, want: dereth_ui::UiFill| {
        shell
            .draw_list()
            .iter()
            .filter(|cmd| cmd.who == entry)
            .flat_map(|cmd| cmd.fills.iter())
            .any(|fill| *fill == want)
    };

    let mut input = dereth_ui::NullInputPump;
    shell.frame(LocalTime(anchor), &host, &mut input);
    let flashes = if queried == 0.0 {
        // A desktop that answers with no interval at all: every tick is past the boundary, so the
        // caret flips on the first one and back on the next.
        let off = !drawn(shell, want);
        shell.frame(LocalTime(anchor), &host, &mut input);
        off && drawn(shell, want)
    } else {
        let on = drawn(shell, want);
        shell.frame(LocalTime(anchor + queried * 0.5), &host, &mut input);
        let still_on = drawn(shell, want);
        let epsilon = (queried * 1.0e-9).max(0.001);
        shell.frame(LocalTime(anchor + queried + epsilon), &host, &mut input);
        let off = !drawn(shell, want);
        on && still_on && off
    };

    c.assert_behaviour(
        "ui.caret.flashes-at-the-interval-the-player-set-for-their-desktop",
        move |_| editable && asked_the_desktop && flashes,
    );
    c.shutdown();
}
