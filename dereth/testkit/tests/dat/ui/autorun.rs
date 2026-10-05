//! UI fixtures and scenarios for autorun.

use super::*;
// ---------------------------------------------------------------------------------------------
// notice.autorun.*
//
// The words themselves are literals in the scenarios below rather than the symbols the client
// writes them through.
// ---------------------------------------------------------------------------------------------

/// The two lines the client writes, and the channel it writes them on. Literals, not the symbols
/// the client writes them through: a scenario that read a value back through the symbol the
/// production code writes it through could not detect a wrong value.
const AUTORUN_ON: &str = "AutoRun ON";
const AUTORUN_OFF: &str = "AutoRun OFF";
const CLIENT_FEEDBACK_CHANNEL: u32 = 0x1A;

/// What the client has queued to say and not yet said.
fn queued_to_say(c: &HeadlessClient) -> Vec<(u32, String)> {
    c.view()
        .expect_app()
        .objects()
        .world
        .scroll
        .pending()
        .iter()
        .map(|f| (f.chat_type, f.body.clone()))
        .collect()
}

/// How many lines the client has ever written -- a count and not a queue length, because the queue
/// is drained every frame and "nothing was said" and "something was said and consumed" are the
/// same reading without it.
fn lines_ever_written(c: &HeadlessClient) -> u64 {
    c.view().expect_app().objects().world.scroll.added
}

/// A client in the world with a keyboard on it.
fn a_client_in_the_world() -> (HeadlessClient, dereth_testkit::adapters_shell::Hands) {
    (
        HeadlessClient::new(ClientSpec::gameplay_in_world(4)),
        dereth_testkit::adapters_shell::Hands::new(),
    )
}

/// Press a key and run the frame that acts on it.
fn press_and_frame(
    c: &mut HeadlessClient,
    hands: &mut dereth_testkit::adapters_shell::Hands,
    code: winit::keyboard::KeyCode,
) {
    hands.press(c, the_key(code));
    c.tick(1);
}

fn release_and_frame(
    c: &mut HeadlessClient,
    hands: &mut dereth_testkit::adapters_shell::Hands,
    code: winit::keyboard::KeyCode,
) {
    hands.release(c, the_key(code));
    c.tick(1);
}

fn the_key(code: winit::keyboard::KeyCode) -> dereth_input::keys::Key {
    dereth_desktop::platform::window::key_from_key_code(code).expect("the host names this key")
}

/// Turning the run lock on or off says so in the message window.
pub(super) fn the_run_lock_says_so_in_the_message_window() {
    use winit::keyboard::KeyCode;

    let (mut c, mut hands) = a_client_in_the_world();
    let starts_off =
        !c.view().expect_app().probe().char_input().auto_run && queued_to_say(&c).is_empty();
    let base = lines_ever_written(&c);

    press_and_frame(&mut c, &mut hands, KeyCode::KeyQ);
    let on = c.view().expect_app().probe().char_input().auto_run
        && queued_to_say(&c) == vec![(CLIENT_FEEDBACK_CHANNEL, AUTORUN_ON.to_owned())]
        && lines_ever_written(&c) == base + 1
        && c.view()
            .expect_app()
            .probe()
            .movement_commands()
            .auto_run_notices_sent
            == 1;
    release_and_frame(&mut c, &mut hands, KeyCode::KeyQ);
    let drained = queued_to_say(&c).is_empty();

    press_and_frame(&mut c, &mut hands, KeyCode::KeyQ);
    let off = !c.view().expect_app().probe().char_input().auto_run
        && queued_to_say(&c) == vec![(CLIENT_FEEDBACK_CHANNEL, AUTORUN_OFF.to_owned())]
        && lines_ever_written(&c) == base + 2
        && c.view()
            .expect_app()
            .probe()
            .movement_commands()
            .auto_run_notices_sent
            == 2;
    release_and_frame(&mut c, &mut hands, KeyCode::KeyQ);

    c.assert_behaviour(
        "notice.autorun.turning-the-run-lock-on-or-off-says-so-in-the-message-window",
        move |_| starts_off && on && drained && off,
    );
    c.shutdown();
}

/// A key that does not change the run lock says nothing.
pub(super) fn a_key_that_changes_nothing_says_nothing() {
    use winit::keyboard::KeyCode;

    let (mut c, mut hands) = a_client_in_the_world();
    let base = lines_ever_written(&c);

    // The lock is already off, so a movement key does not change it -- and the client says
    // nothing. Without this direction the claim above would hold on a client that wrote the line
    // on every key press.
    press_and_frame(&mut c, &mut hands, KeyCode::KeyW);
    let walked = c.view().expect_app().probe().char_input().forward;
    let silent = lines_ever_written(&c) == base
        && c.view()
            .expect_app()
            .probe()
            .movement_commands()
            .auto_run_notices_sent
            == 0;
    release_and_frame(&mut c, &mut hands, KeyCode::KeyW);

    // ...and the same key when it *does* change it -- cancelling the lock -- tells the player.
    press_and_frame(&mut c, &mut hands, KeyCode::KeyQ);
    let locked = c.view().expect_app().probe().char_input().auto_run;
    release_and_frame(&mut c, &mut hands, KeyCode::KeyQ);
    let after_on = lines_ever_written(&c);
    press_and_frame(&mut c, &mut hands, KeyCode::KeyW);
    let cancelled = !c.view().expect_app().probe().char_input().auto_run
        && queued_to_say(&c) == vec![(CLIENT_FEEDBACK_CHANNEL, AUTORUN_OFF.to_owned())]
        && lines_ever_written(&c) == after_on + 1;
    release_and_frame(&mut c, &mut hands, KeyCode::KeyW);

    c.assert_behaviour(
        "notice.autorun.a-key-that-does-not-change-the-run-lock-says-nothing",
        move |_| walked && silent && locked && cancelled,
    );
    c.shutdown();
}

/// The line reaches the strip and every chat window drops it.
pub(super) fn the_autorun_line_reaches_the_strip_and_the_chat_windows_drop_it() {
    use dereth_ui_screens::chat::interface::{window, ChatInterface, ChatMessage, Routed};
    use winit::keyboard::KeyCode;

    let (mut c, mut hands) = a_client_in_the_world();
    let (spew, scroll, dropped, kept) = {
        let s = c.view().expect_app().hud().stats;
        (
            s.spew_lines,
            s.scroll_lines,
            s.chat_lines_dropped,
            s.chat_lines,
        )
    };

    press_and_frame(&mut c, &mut hands, KeyCode::KeyQ);
    // The queued line is drained by the next frame, which is what the release runs.
    release_and_frame(&mut c, &mut hands, KeyCode::KeyQ);

    let landed = {
        let app = c.view().expect_app();
        let s = app.hud().stats;
        s.scroll_lines == scroll + 1
            && s.spew_lines == spew + 1
            && app.hud().panels.spew.model.items == vec![AUTORUN_ON.to_owned()]
            && app.hud().panels.spew.model.pending.is_empty()
            && app.hud().panels.spew.drawn >= 1
            // Every chat window filtered it out, and none kept it.
            && s.chat_lines_dropped == dropped + 1
            && s.chat_lines == kept
    };

    // The filter itself, so the drop above is explained rather than only observed -- and a player
    // who asks for this kind of line does get it.
    let m = ChatMessage {
        feedback: dereth_client_contract::feedback::Feedback::ORDINARY,
        ty: CLIENT_FEEDBACK_CHANNEL as u8,
        body: AUTORUN_ON.to_owned(),
        prefix: None,
        window: 0,
    };
    let dropped_everywhere = [
        window::MAIN,
        window::FLOATY_1,
        window::FLOATY_2,
        window::FLOATY_3,
        window::FLOATY_4,
    ]
    .into_iter()
    .all(|w| ChatInterface::new(w).route(&m) == Routed::FilteredOut);
    let asked_for_it = {
        let mut main = ChatInterface::new(window::MAIN);
        main.filter |= 0x0400_0000;
        main.route(&m) == Routed::Accepted
    };

    c.assert_behaviour(
        "notice.autorun.the-line-reaches-the-strip-and-the-chat-windows-drop-it",
        move |_| landed && dropped_everywhere && asked_for_it,
    );
    c.shutdown();
}
