//! Shell fixtures and scenarios for intro.

use super::*;
// ---------------------------------------------------------------------------------------------
// intro.*
//
// Clicking the intro movie or a splash screen does nothing.
//
// The gestures go into the client's own input queue rather than through the pointer, and that is
// deliberate rather than a shortcut: the shipped intro layout holds a root and two pictures and
// **no button at all**, so there is nothing for a hit test to land on. The first scenario asserts
// that from the data, because otherwise "no hotspot was pressed" and "the press did nothing" are
// the same reading.
// ---------------------------------------------------------------------------------------------

/// A client on the intro sequence.
pub(super) fn a_client_on_the_intro() -> HeadlessClient {
    let c = HeadlessClient::new(ClientSpec::screen(dereth_ui::framework::mode::INTRO, 3));
    assert_eq!(
        c.view()
            .expect_app()
            .ui()
            .and_then(|u| u.flow.current_mode()),
        Some(dereth_ui::framework::mode::INTRO),
        "the intro is the screen these scenarios are about"
    );
    c
}

fn with_intro<R>(
    c: &mut HeadlessClient,
    f: impl FnOnce(&mut dereth_ui_screens::screens::intro::IntroScreen) -> R,
) -> R {
    let shell = c.app_mut().ui_mut().expect("the UI shell is up");
    let screen = shell.flow.current_mut().expect("a screen is current");
    let any: &mut dyn std::any::Any = &mut **screen;
    f(any
        .downcast_mut::<dereth_ui_screens::screens::intro::IntroScreen>()
        .expect("the intro screen is current"))
}

/// The picture the intro is showing, and the ones still queued behind it.
pub(super) fn intro_state(c: &mut HeadlessClient) -> (Option<u32>, Vec<u32>) {
    with_intro(c, |s| (s.current_state, s.states.iter().copied().collect()))
}

pub(super) fn current_screen(c: &HeadlessClient) -> Option<dereth_ui::UiMode> {
    c.view()
        .expect_app()
        .ui()
        .and_then(|u| u.flow.current_mode())
}

/// Put one action on the client's own input queue, on the map the pointer is bound in.
fn inject(c: &mut HeadlessClient, action: u32, start: bool) {
    let e = dereth_input::InputEvent {
        action: dereth_input::ActionId(action),
        input_map: dereth_client::ui::UI_INPUT_MAP,
        toggle: dereth_input::ToggleType::OneShot,
        extent: 1.0,
        start,
        repeat_delta: 1,
        repeat_total: 0,
        from_key_down: false,
    };
    c.app_mut()
        .input_manager_mut()
        .expect("the input shell")
        .inject_action(e);
}

/// One character, through the message the client's own character gate reads, so all three of its
/// gates apply.
fn inject_character(c: &mut HeadlessClient, ch: u8) {
    let m = dereth_client::pump::Win32Message::new(
        dereth_input::win32::msg::WM_CHAR,
        ch as usize,
        0,
        0,
    );
    c.app_mut()
        .input_manager_mut()
        .expect("the input shell")
        .on_message(m);
}

/// The left button, as the client's own table numbers it.
const LEFT_BUTTON_ACTION: u32 = 7;

/// A click advances one picture, and letting go is not a second one.
pub(super) fn a_click_advances_one_picture_and_the_release_is_not_another() {
    use dereth_ui_screens::screens::intro;

    let mut c = a_client_on_the_intro();

    // The premise: the shipped layout is a root and two pictures, with no button in it -- so
    // nothing here can be a press on a hotspot.
    let ids = {
        let app = c.view().expect_app();
        let shell = app.ui().expect("the UI shell is up");
        let root = shell.flow.current().expect("a screen").roots()[0];
        let mut ids = Vec::new();
        let mut stack = vec![root];
        while let Some(h) = stack.pop() {
            ids.push(shell.ui.node(h).expect("live").element_id().0);
            stack.extend(shell.ui.children(h));
        }
        ids.sort_unstable();
        ids
    };
    let no_hotspot = ids.len() == 3;

    let (before, queued) = intro_state(&mut c);
    let starts_on_the_first =
        before == Some(intro::SHIPPED_STATES[0]) && queued == intro::SHIPPED_STATES[1..].to_vec();

    inject(&mut c, LEFT_BUTTON_ACTION, true);
    c.tick(1);
    let (after, still_queued) = intro_state(&mut c);
    let advanced_one = after == Some(intro::SHIPPED_STATES[1])
        && still_queued == intro::SHIPPED_STATES[2..].to_vec()
        && current_screen(&c) == Some(dereth_ui::framework::mode::INTRO)
        && c.view()
            .expect_app()
            .ui()
            .expect("shell")
            .stats
            .intro_actions
            == 1;
    c.shutdown();

    // A real click is a press and a release. If the release advanced too, one click would eat two
    // pictures and the intro would be half as long as it should be.
    let mut c = a_client_on_the_intro();
    inject(&mut c, LEFT_BUTTON_ACTION, true);
    inject(&mut c, LEFT_BUTTON_ACTION, false);
    c.tick(1);
    let one_advance = intro_state(&mut c).0 == Some(intro::SHIPPED_STATES[1])
        && c.view()
            .expect_app()
            .ui()
            .expect("shell")
            .stats
            .intro_actions
            == 1;

    c.assert_behaviour(
        "intro.click.a-click-advances-one-picture-and-letting-go-is-not-another",
        move |_| no_hotspot && starts_on_the_first && advanced_one && one_advance,
    );
    c.shutdown();
}

/// Clicking through the whole sequence ends at character select and not before.
pub(super) fn clicking_through_the_intro_ends_at_character_select() {
    use dereth_ui::framework::mode;
    use dereth_ui_screens::screens::intro;

    let mut c = a_client_on_the_intro();
    let mut one_per_click = true;
    for expected in &intro::SHIPPED_STATES[1..] {
        inject(&mut c, LEFT_BUTTON_ACTION, true);
        c.tick(1);
        one_per_click &=
            intro_state(&mut c).0 == Some(*expected) && current_screen(&c) == Some(mode::INTRO);
    }
    // ...and the click after the last picture finds nothing left.
    inject(&mut c, LEFT_BUTTON_ACTION, true);
    c.tick(1);
    let ends_there = current_screen(&c) == Some(mode::CHARACTER_MANAGEMENT);

    c.assert_behaviour(
        "intro.click.clicking-through-the-sequence-ends-at-character-select-and-not-before",
        move |_| one_per_click && ends_there && intro::SHIPPED_STATES.len() > 1,
    );
    c.shutdown();
}

/// The quit action skips the rest of the intro where a click advances it.
pub(super) fn the_quit_action_skips_the_rest_of_the_intro() {
    use dereth_ui::framework::mode;

    let mut c = a_client_on_the_intro();
    inject(&mut c, dereth_ui_screens::screens::intro::ACTION_QUIT, true);
    c.tick(1);
    let skipped = current_screen(&c) == Some(mode::CHARACTER_MANAGEMENT);

    c.assert_behaviour(
        "intro.quit.the-quit-action-skips-the-rest-of-it",
        move |_| skipped,
    );
    c.shutdown();
}

/// Any character advances one picture, and escape skips.
pub(super) fn any_character_advances_and_escape_skips() {
    use dereth_ui::framework::mode;
    use dereth_ui_screens::screens::intro;

    let mut c = a_client_on_the_intro();
    // One frame first, so the client has opened the gate a character has to pass -- which is
    // what it has done by the time a player could press anything.
    c.tick(1);
    inject_character(&mut c, b'a');
    c.tick(1);
    let advanced = intro_state(&mut c).0 == Some(intro::SHIPPED_STATES[1])
        && current_screen(&c) == Some(mode::INTRO)
        && c.view()
            .expect_app()
            .ui()
            .expect("shell")
            .stats
            .intro_characters
            == 1;
    c.shutdown();

    let mut c = a_client_on_the_intro();
    c.tick(1);
    inject_character(&mut c, 0x1B);
    c.tick(1);
    let escaped = current_screen(&c) == Some(mode::CHARACTER_MANAGEMENT);

    c.assert_behaviour(
        "intro.keyboard.any-character-advances-one-picture-and-escape-skips",
        move |_| advanced && escaped,
    );
    c.shutdown();
}
