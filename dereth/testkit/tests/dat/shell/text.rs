//! Shell fixtures and scenarios for text.

use super::*;
use dereth_ui_screens::screens::chargen;
// ---------------------------------------------------------------------------------------------
// focus.press.*
//
// A press focuses every scrollable element, as retail does, and not only a text box.
//
// Each scenario presses **without letting go**, so the state the element is in at the moment the
// keyboard moved can be read; letting go and comparing with where it started would be reading an
// operation against its own inverse. The state numbers are written out rather than read back
// through the symbols the client writes them through.
// ---------------------------------------------------------------------------------------------

/// Whatever currently holds the keyboard.
pub(super) fn what_holds_the_keyboard(c: &HeadlessClient) -> Option<dereth_ui::ElemHandle> {
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .focus_element()
}

/// The state an element is in.
pub(super) fn state_of(c: &HeadlessClient, h: dereth_ui::ElemHandle) -> dereth_ui::StateId {
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(h)
        .expect("live")
        .state
}

/// The middle of an element's box.
pub(super) fn middle_of(c: &HeadlessClient, h: dereth_ui::ElemHandle) -> (i32, i32) {
    let b = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .screen_box(h);
    ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
}

/// Press the left button at a point and run the frame that acts on it, with no release.
pub(super) fn press_at(c: &mut HeadlessClient, hands: &mut Hands, x: i32, y: i32) {
    use dereth_input::keys::MouseButton;
    hands.move_to(c, x, y);
    let m = hands.button_message(MouseButton::Left, true);
    hands.send(c, m);
    c.tick(1);
}

/// Let it go.
pub(super) fn release(c: &mut HeadlessClient, hands: &mut Hands) {
    use dereth_input::keys::MouseButton;
    let m = hands.button_message(MouseButton::Left, false);
    hands.send(c, m);
    c.tick(1);
}

/// The wizard on its skills page, which is where the shipped tree actually *shows* a list and its
/// bar: every panel carrying one on the gameplay screen starts hidden, and a hidden element
/// cannot be pressed.
fn a_wizard_on_the_skills_page() -> HeadlessClient {
    use dereth_ui_screens::screens::chargen::{EcgProgress, HERITAGE_BUTTONS, TOWN_BUTTONS};

    let mut c = HeadlessClient::new(ClientSpec::screen(dereth_ui::framework::mode::CHAR_GEN, 8));
    press_wizard_button(&mut c, HERITAGE_BUTTONS[0].0);
    press_wizard_button(&mut c, TOWN_BUTTONS[0].0);
    press_wizard_button(
        &mut c,
        EcgProgress::Skills.select_button().expect("the skills tab"),
    );
    c.tick(4);
    c
}

/// The skills list and the bar bound to it.
const SKILLS_LIST: ElementId = ElementId(0x1000_03F7);
const SKILLS_BAR: ElementId = ElementId(0x1000_03F8);

/// A press on a button takes the keyboard and the button keeps its own look.
pub(super) fn a_press_on_a_button_takes_the_keyboard() {
    use dereth_ui::StateId;
    use dereth_ui_screens::chat::window::{ENTRY, SEND};

    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    let mut hands = Hands::new();
    let entry = element(&c, ENTRY);
    let send = element(&c, SEND);

    let (x, y) = middle_of(&c, entry);
    press_at(&mut c, &mut hands, x, y);
    release(&mut c, &mut hands);
    let entry_had_it =
        what_holds_the_keyboard(&c) == Some(entry) && state_of(&c, send) != StateId(3);

    let (x, y) = middle_of(&c, send);
    press_at(&mut c, &mut hands, x, y);
    let moved = what_holds_the_keyboard(&c) == Some(send);
    // A pressed button is in its own pressed state, never in the generic focused one -- what a
    // player sees of a button being pressed is the button, not a focus ring.
    let pressed = state_of(&c, send);
    let own_look = pressed == StateId(3) && pressed != StateId(4);

    release(&mut c, &mut hands);
    let kept_it = what_holds_the_keyboard(&c) == Some(send)
        && state_of(&c, send) != StateId(4)
        // Only now, having read the state during the press, is the end worth comparing with the
        // start: the picture goes back where it was even though the keyboard does not.
        && state_of(&c, send) == StateId(1);

    c.assert_behaviour(
        "focus.press.a-press-on-a-button-takes-the-keyboard-and-the-button-keeps-its-own-look",
        move |_| entry_had_it && moved && own_look && kept_it,
    );
    c.shutdown();
}

/// Typing stops when the keyboard leaves the entry.
pub(super) fn typing_stops_when_the_keyboard_leaves_the_entry() {
    use dereth_ui_screens::chat::window::{ENTRY, SEND};

    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    let mut hands = Hands::new();
    let entry = element(&c, ENTRY);
    let send = element(&c, SEND);

    let (x, y) = middle_of(&c, entry);
    press_at(&mut c, &mut hands, x, y);
    release(&mut c, &mut hands);
    let typing = what_holds_the_keyboard(&c) == Some(entry)
        && c.app_mut().ui_mut().expect("shell").wants_text_mode();

    hands.type_text(&mut c, "abc");
    let arrived = c
        .app_mut()
        .ui_mut()
        .expect("shell")
        .ui
        .text_element_mut(entry)
        .expect("a text element")
        .glyphs
        .inq_text(false)
        == "abc";

    let (x, y) = middle_of(&c, send);
    press_at(&mut c, &mut hands, x, y);
    release(&mut c, &mut hands);
    let stopped = what_holds_the_keyboard(&c) == Some(send)
        && !c.app_mut().ui_mut().expect("shell").wants_text_mode();

    c.assert_behaviour(
        "focus.press.typing-stops-when-the-keyboard-leaves-the-entry",
        move |_| typing && arrived && stopped,
    );
    c.shutdown();
}

/// A press on a list takes the keyboard, and the list's own look follows it.
pub(super) fn a_press_on_a_list_takes_the_keyboard_and_its_look_follows() {
    use dereth_ui::StateId;

    let mut c = a_wizard_on_the_skills_page();
    let mut hands = Hands::new();
    let list = element(&c, SKILLS_LIST);
    let (x, y) = middle_of(&c, list);
    let reachable = c
        .view()
        .expect_app()
        .ui()
        .expect("shell")
        .ui
        .hit_test_screen(x, y)
        == Some(list);
    let nothing_holds_it = what_holds_the_keyboard(&c).is_none();

    let before = state_of(&c, list);
    // Which states this list declares is layout data, so what it lands in is derived from the
    // layout; what is written out here is the **rule** -- the change fires only from these three.
    let declares_focused = c
        .view()
        .expect_app()
        .ui()
        .expect("shell")
        .ui
        .node(list)
        .expect("live")
        .desc
        .access_state(StateId(4))
        .is_some();

    press_at(&mut c, &mut hands, x, y);
    let took_it = what_holds_the_keyboard(&c) == Some(list);
    let after = state_of(&c, list);
    let look_followed = if matches!(before.0, 0 | 1 | 5) {
        after
            == if declares_focused {
                StateId(4)
            } else {
                StateId(0)
            }
    } else {
        after == before
    };

    c.assert_behaviour(
        "focus.press.a-press-on-a-list-takes-the-keyboard-and-its-look-follows",
        move |_| reachable && nothing_holds_it && took_it && look_followed,
    );
    c.shutdown();
}

/// A press on a scrollbar takes the keyboard too.
///
/// **What this does not say.** It says nothing about the bar's position, and the position does
/// move: a press in the middle of the track, above or below the thumb, pages the list, which is
/// what a track press is for. The claim here is only that the keyboard moves; the paging is
/// `chargen.scroll.a-press-on-the-track-moves-a-whole-page-towards-the-press`.
pub(super) fn a_press_on_a_scrollbar_takes_the_keyboard_without_moving_it() {
    let mut c = a_wizard_on_the_skills_page();
    let mut hands = Hands::new();
    let bar = element(&c, SKILLS_BAR);
    // The middle of the track, not an arrow: an arrow would step the bar, which is a different
    // question from the one this asks.
    let (x, y) = middle_of(&c, bar);
    let hit = c
        .view()
        .expect_app()
        .ui()
        .expect("shell")
        .ui
        .hit_test_screen(x, y);
    let reachable = hit.is_some();
    press_at(&mut c, &mut hands, x, y);
    let hit = hit.expect("the pointer lands on something");
    let took_it = {
        let app = c.view().expect_app();
        let ui = &app.ui().expect("shell").ui;
        (hit == bar || ui.is_ancestor_of(bar, hit))
            && ui.focus_element() == Some(hit)
            && ui.takes_focus_on_press(hit)
    };
    c.assert_behaviour(
        "focus.press.a-press-on-a-scrollbar-takes-the-keyboard-too",
        move |_| reachable && took_it,
    );
    c.shutdown();
}

/// A press on something that cannot scroll moves the keyboard nowhere.
pub(super) fn a_press_on_something_that_cannot_scroll_moves_the_keyboard_nowhere() {
    use dereth_ui::ElementType;
    use dereth_ui_screens::chat::window::ENTRY;

    /// The first element of `ty` the hit test really answers when aimed at its own middle.
    ///
    /// Found by walking rather than by naming an id, because which panels a fresh gameplay screen
    /// shows is shipped data; `None` rather than a panic, so the caller can say what it could not
    /// find and a helper that failed is not mistaken for the claim failing.
    fn first_reachable(c: &HeadlessClient, ty: ElementType) -> Option<dereth_ui::ElemHandle> {
        let app = c.view().expect_app();
        let ui = &app.ui().expect("the UI shell is up").ui;
        ui.element_list().iter().copied().find(|h| {
            if ui.node(*h).map(dereth_ui::element::ElementNode::ty) != Some(ty) {
                return false;
            }
            let b = ui.screen_box(*h);
            if b.x1 < b.x0 || b.y1 < b.y0 {
                return false;
            }
            ui.hit_test_screen((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2) == Some(*h)
        })
    }

    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    let mut hands = Hands::new();
    let entry = element(&c, ENTRY);
    let (x, y) = middle_of(&c, entry);
    press_at(&mut c, &mut hands, x, y);
    release(&mut c, &mut hands);
    let entry_has_it = what_holds_the_keyboard(&c) == Some(entry);

    // Every kind of element that is not in the scrolling family and that the shipped gameplay
    // tree actually shows, by its own type number.
    let outside: [u32; 8] = [0x02, 0x03, 0x07, 0x08, 0x09, 0x0D, 0x10, 0x11];
    let mut tried = 0usize;
    let mut none_took_it = true;
    for ty in outside {
        let Some(h) = first_reachable(&c, ElementType(ty)) else {
            continue;
        };
        tried += 1;
        let (x, y) = middle_of(&c, h);
        press_at(&mut c, &mut hands, x, y);
        none_took_it &=
            what_holds_the_keyboard(&c) != Some(h) && what_holds_the_keyboard(&c) == Some(entry);
        release(&mut c, &mut hands);
    }

    c.assert_behaviour(
        "focus.press.a-press-on-something-that-cannot-scroll-moves-the-keyboard-nowhere",
        move |_| {
            // The denominator: a loop that pressed nothing would pass in silence.
            entry_has_it && tried > 0 && none_took_it
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// text-entry.focus, the screen's own side of it
//
// The client's switch for "the player is typing" must not follow the keyboard one frame late: if it
// did, the first character after a *screen* took the keyboard would be destroyed rather than
// delayed -- the client decides whether to keep a character at the moment the message arrives.
//
// Each half is here with its opposite. Asserting only that a character arrives passes on a client
// that always accepts one; asserting only that one is lost passes on a client that never opens the
// gate at all, which is the defect itself.
//
// The shipped prompt, read out of the text table, is what the first typed character has to replace;
// that is asserted in the first scenario below as a fixture guard rather than as a row of its own.
// ---------------------------------------------------------------------------------------------

/// The wizard's name box, and the two buttons these scenarios press.
const ALUVIAN_BULLET: ElementId = ElementId(0x1000_03BF);
const SUMMARY_TAB: ElementId = ElementId(0x1000_03F4);

/// The wizard with a heritage settled and **nothing** holding the keyboard, which is where the
/// direction that must lose a character is asserted from.
fn a_wizard_with_a_heritage() -> HeadlessClient {
    let mut c = a_client_on_the_wizard();
    press_wizard_button(&mut c, ALUVIAN_BULLET);
    c.tick(1);
    c
}

/// How many characters the shipped text elements have actually been handed -- the denominator
/// without which "the box is empty" and "nothing was ever offered" are the same reading.
fn characters_handed_over(c: &HeadlessClient) -> u64 {
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .stats
        .characters_delivered
}

/// How many times the client has thrown the switch that says the player is typing.
fn typing_switch_edges(c: &HeadlessClient) -> u64 {
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .stats
        .text_mode_edges
}

fn box_text(c: &mut HeadlessClient, h: dereth_ui::ElemHandle) -> String {
    c.app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .ui
        .text_element_mut(h)
        .map_or_else(String::new, |t| t.glyphs.inq_text(false))
}

/// A character typed the moment a screen takes the keyboard is not lost.
pub(super) fn a_character_typed_the_moment_a_screen_takes_the_keyboard_is_not_lost() {
    use dereth_ui_screens::screens::chargen::{ERROR_STRING_TABLE, NAME_FIELD, NAME_PROMPT};

    let mut c = a_wizard_with_a_heritage();
    let mut hands = Hands::new();

    // ---- the direction that must LOSE the character ------------------------------------------
    // Nothing on this page can be typed into, so nothing holds the keyboard.
    let nothing_holds_it = c
        .view()
        .expect_app()
        .ui()
        .expect("shell")
        .ui
        .focus_element()
        .is_none();
    let edges_before = typing_switch_edges(&c);
    let offered_at = characters_handed_over(&c);
    // One character and **no frame between**: whatever the last frame left the switch at is what
    // decides, which is the whole of this claim.
    hands.character(&mut c, 'Q');
    c.tick(1);
    let lost = characters_handed_over(&c) == offered_at && typing_switch_edges(&c) == edges_before;

    // ---- the frame in which the screen takes the keyboard -------------------------------------
    press_wizard_button(&mut c, SUMMARY_TAB);
    let name = element(&c, NAME_FIELD);
    let took_it = c.view().expect_app().ui().expect("shell").ui.focus_element() == Some(name)
        // Exactly one throw of the switch: on the change, not on every frame.
        && typing_switch_edges(&c) == edges_before + 1;
    // The prompt the first typed character has to replace, read out of the shipped text table
    // rather than written here.
    let prompt = c
        .view()
        .expect_app()
        .ui()
        .expect("shell")
        .ui
        .resolve_string(
            ERROR_STRING_TABLE,
            dereth_ui::persist::preferences::token_of(NAME_PROMPT),
        )
        .expect("the prompt is in the shipped text table");
    let prompt_is_there = !prompt.is_empty();

    // ---- the direction that must KEEP it, with no frame in between ----------------------------
    hands.character(&mut c, 'Z');
    c.tick(1);
    let kept = characters_handed_over(&c) == offered_at + 1
        // ...and it needed no further throw of the switch to get there: the switch was already
        // over when the message arrived, which is what deciding at message time requires. A
        // client that followed the keyboard a frame late would throw it here, and the character
        // would already be gone.
        && typing_switch_edges(&c) == edges_before + 1
        && box_text(&mut c, name) == "Z";
    let the_wizard_has_it = with_wizard(&mut c, |_, w| w.state.name.clone()) == "Z"
        && with_wizard(&mut c, |_, w| w.name_entered);

    c.assert_behaviour(
        "text-entry.focus.a-character-typed-the-moment-a-screen-takes-the-keyboard-is-not-lost",
        move |_| {
            nothing_holds_it && lost && took_it && prompt_is_there && kept && the_wizard_has_it
        },
    );
    c.shutdown();
}

/// The same when the screen takes the keyboard in its own per-frame pass.
pub(super) fn a_screen_that_takes_the_keyboard_in_its_own_pass_keeps_the_next_character() {
    use dereth_ui_screens::screens::chargen::NAME_FIELD;

    let mut c = a_wizard_with_a_heritage();
    let mut hands = Hands::new();
    press_wizard_button(&mut c, SUMMARY_TAB);
    let name = element(&c, NAME_FIELD);
    assert_eq!(
        c.view()
            .expect_app()
            .ui()
            .expect("shell")
            .ui
            .focus_element(),
        Some(name),
        "the summary page took the keyboard"
    );

    // Take it away again, so the reading below cannot pass on the keyboard the last frame had.
    c.app_mut()
        .ui_mut()
        .expect("shell")
        .ui
        .relinquish_focus(name);
    c.tick(1);
    let given_up = c
        .view()
        .expect_app()
        .ui()
        .expect("shell")
        .ui
        .focus_element()
        .is_none();
    let offered_at = characters_handed_over(&c);

    // The control: a character offered now goes nowhere.
    hands.character(&mut c, 'X');
    c.tick(1);
    let lost = characters_handed_over(&c) == offered_at;

    // The screen's own per-frame pass takes it back -- a different step of the frame from the one
    // above, and the other half of "in its own update or in answering a message".
    with_wizard(&mut c, |_, w| w.pending_refresh = true);
    c.tick(1);
    let took_it_back = c
        .view()
        .expect_app()
        .ui()
        .expect("shell")
        .ui
        .focus_element()
        == Some(name);

    hands.character(&mut c, 'Y');
    c.tick(1);
    let kept = characters_handed_over(&c) == offered_at + 1 && box_text(&mut c, name) == "Y";

    c.assert_behaviour("text-entry.focus.a-screen-that-takes-the-keyboard-in-its-own-pass-keeps-the-next-character", move |_| {
        given_up && lost && took_it_back && kept
    });
    c.shutdown();
}

/// The switch follows a box that can be typed into and nothing else, once per change.
pub(super) fn the_typing_switch_follows_an_editable_box_and_nothing_else() {
    use dereth_ui_screens::screens::chargen::NAME_FIELD;

    let mut c = a_wizard_with_a_heritage();
    let nothing_focused = !c.app_mut().ui_mut().expect("shell").wants_text_mode();

    let edges_before = typing_switch_edges(&c);
    press_wizard_button(&mut c, SUMMARY_TAB);
    let name = element(&c, NAME_FIELD);
    let one_edge = typing_switch_edges(&c) == edges_before + 1;

    let editable_focus = {
        let shell = c.app_mut().ui_mut().expect("shell");
        shell.ui.focus_element() == Some(name)
            && shell
                .ui
                .text_element_mut(name)
                .expect("a text element")
                .bits
                .editable()
            && shell.wants_text_mode()
    };
    // The third answer, which a yes-or-no about "something is focused" gets wrong: a box that can
    // be picked at but not typed into is not a place characters go.
    let not_editable = {
        let shell = c.app_mut().ui_mut().expect("shell");
        let t = shell.ui.text_element_mut(name).expect("a text element");
        t.bits.set_editable(false);
        t.bits.set_selectable(true);
        let answer = !shell.wants_text_mode();
        shell
            .ui
            .text_element_mut(name)
            .expect("a text element")
            .bits
            .set_editable(true);
        answer && shell.wants_text_mode()
    };

    // Frames with nothing changing throw the switch no further times -- it is thrown on the
    // change, and a client that threw it every frame would be re-arming a latch the client arms
    // once.
    c.tick(8);
    let quiet = typing_switch_edges(&c) == edges_before + 1;

    // ...and once more on the way back.
    c.app_mut()
        .ui_mut()
        .expect("shell")
        .ui
        .relinquish_focus(name);
    c.tick(1);
    let back = c
        .view()
        .expect_app()
        .ui()
        .expect("shell")
        .ui
        .focus_element()
        .is_none()
        && typing_switch_edges(&c) == edges_before + 2;

    c.assert_behaviour("text-entry.focus.the-typing-switch-follows-a-box-that-can-be-typed-into-and-is-thrown-once-per-change", move |_| {
        nothing_focused && one_edge && editable_focus && not_editable && quiet && back
    });
    c.shutdown();
}

// =============================================================================================
// pointer.wheel.* and window.focus.* -- the wheel over the chat log
//
// Guards against *"the mouse wheel produces no action at all in the running client"*. Several
// related checks are not rows: state map and priority numbers written as literals beside the
// symbols the client carries them through are transcriptions; the wheel message's own packing is
// not built by the harness, because `Hands::wheel` goes through the client's own mapping; a map id
// **no shipped element carries** would be a claim about a fixture rather than about a client; and
// the layout census is folded into the pre-game-keys scenario as the denominator it is.
//
// **The log is filled from the shipped welcome text** the chat scenarios already use, not from a
// raw recording: this crate reads only the published recordings, and the claim is about the wheel
// and not about the words.
// =============================================================================================

/// The reference server's own welcome burst, which is what a player's log holds a moment after
/// they arrive, and which the chat scenarios already fill a window from.
const WELCOME_BURST: &str = "Welcome to Asheron's Call\n  powered by ACEmulator\n\nFor more information on commands supported by this server, type @acehelp\n";

/// The arrow that walks the log **back** -- the one at the top.
const LOG_ARROW_UP: ElementId = ElementId(0x1000_0072);
/// The one at the bottom.
const LOG_ARROW_DOWN: ElementId = ElementId(0x1000_0071);

/// The line the client composes from one of the shard's system messages, made on a client of its
/// own so that nothing else is in the window that takes it.
fn a_system_line(text: &str) -> dereth_ui_screens::chat::interface::ChatMessage {
    let mut c = HeadlessClient::model();
    c.when(dereth_testkit::Inbound::message(
        &dereth_protocol::comms::CommunicationTextboxString {
            text: text.to_owned(),
            text_type: 0,
        },
    ));
    let lines = c.view().chat_lines();
    assert_eq!(lines.len(), 1, "one line per system message");
    lines[0].clone()
}

/// Fill the log past its pane and leave it at its end, the way the client does.
fn fill_the_log(c: &mut HeadlessClient) {
    let line = a_system_line(WELCOME_BURST);
    for _ in 0..3 {
        let app = c.app_mut();
        let shell = app.ui_mut().expect("the UI shell is up");
        let ui = &mut shell.ui;
        let screen = shell.flow.current_mut().expect("a screen is up");
        let any: &mut dyn std::any::Any = &mut **screen;
        let gameplay = any
            .downcast_mut::<dereth_ui_screens::screens::gameplay::GamePlayScreen>()
            .expect("the gameplay screen");
        assert!(
            !gameplay
                .recv_display_final_string_info(ui, &line)
                .is_empty(),
            "the line reached a shipped chat window"
        );
    }
    c.tick(1);
    let log = element(c, LOG);
    let (content, view) = {
        let ui = &mut c.app_mut().ui_mut().expect("the UI shell is up").ui;
        (
            ui.text_element_mut(log).map_or(0, |t| t.scroll.height),
            ui.screen_box(log).height(),
        )
    };
    assert!(
        content > view,
        "the burst overflows the pane ({content} > {view})"
    );
}

/// How far down the chat log is scrolled.
fn log_scroll(c: &mut HeadlessClient) -> i32 {
    let log = element(c, LOG);
    c.app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .ui
        .text_element_mut(log)
        .map_or(0, |t| t.scroll.y)
}

/// The maps a focused element has put in front of the client, ids only.
fn focused_maps(c: &mut HeadlessClient) -> Vec<u32> {
    c.app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .focused_input_maps()
        .into_iter()
        .map(|(m, _)| m)
        .collect()
}

/// The order the client walks its maps in, ids only.
fn walk_order(c: &mut HeadlessClient) -> Vec<u32> {
    map_stack(c).iter().map(|e| e.map.0).collect()
}

/// The map a scrollable element puts in front of the client when it takes the keyboard.
const SCROLL_MAP: u32 = 0x0A;

/// One press, through the pointer, with the frames the gesture needs.
fn press_point(c: &mut HeadlessClient, hands: &mut Hands, at: (i32, i32)) {
    hands.click_at(c, at.0, at.1);
}

// ---------------------------------------------------------------------------------------------
// pointer.wheel.one-detent-over-the-chat-log-moves-it-one-line-and-the-other-way-puts-it-back
// ---------------------------------------------------------------------------------------------

/// One turn of the wheel is one line, the same line an arrow takes, and the two ways are exact
/// inverses -- with the end of the log a clamp rather than a dead wheel.
pub(super) fn one_detent_over_the_chat_log_moves_it_one_line() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    fill_the_log(&mut c);
    let mut hands = Hands::new();

    // The arrow's own step first, from the state the wheel will be driven from, so the two are
    // comparable -- a client that wheeled by a whole page would look right in a picture.
    let at_end = log_scroll(&mut c);
    assert!(
        at_end > 0,
        "the log is at its end, so there is somewhere to go back to"
    );
    let up = element(&c, LOG_ARROW_UP);
    let up_at = middle_of(&c, up);
    press_point(&mut c, &mut hands, up_at);
    let one_line = at_end - log_scroll(&mut c);
    let an_arrow_moves_it = one_line > 0;

    // Back to the end.
    let down = element(&c, LOG_ARROW_DOWN);
    let down_at = middle_of(&c, down);
    for _ in 0..64 {
        if log_scroll(&mut c) >= at_end {
            break;
        }
        press_point(&mut c, &mut hands, down_at);
    }
    let back_at_the_end = log_scroll(&mut c) == at_end;

    // The player is typing, which is what puts the scrolling map in front of the client.
    let entry = element(&c, ENTRY);
    c.app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .ui
        .take_focus(entry);
    c.tick(1);
    let armed =
        what_holds_the_keyboard(&c) == Some(entry) && walk_order(&mut c).contains(&SCROLL_MAP);

    // The pointer over the log itself, and not over its bar: a bar is a button and takes its own
    // presses.
    let log = element(&c, LOG);
    let over_log = middle_of(&c, log);
    hands.move_to(&mut c, over_log.0, over_log.1);
    c.tick(1);
    let over_the_log = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .mouse_over()
        == Some(log);

    // Backwards from the end: clamped, **not** lost. The count of presses that reached the tree
    // is the denominator without which a clamped wheel and a dead one are one reading.
    let downs_before = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .stats
        .mouse_downs;
    hands.wheel(&mut c, -1.0);
    c.tick(1);
    let clamped = log_scroll(&mut c) == at_end
        && c.view()
            .expect_app()
            .ui()
            .expect("the UI shell is up")
            .stats
            .mouse_downs
            == downs_before + 1;

    hands.wheel(&mut c, 1.0);
    c.tick(1);
    let one_detent_is_one_line = at_end - log_scroll(&mut c) == one_line;

    hands.wheel(&mut c, -1.0);
    c.tick(1);
    let the_exact_inverse = log_scroll(&mut c) == at_end;

    for _ in 0..5 {
        hands.wheel(&mut c, 1.0);
        c.tick(1);
    }
    let five_is_five = at_end - log_scroll(&mut c) == one_line * 5;

    c.assert_behaviour("pointer.wheel.one-detent-over-the-chat-log-moves-it-one-line-and-the-other-way-puts-it-back", move |_| {
        an_arrow_moves_it
            && back_at_the_end
            && armed
            && over_the_log
            && clamped
            && one_detent_is_one_line
            && the_exact_inverse
            && five_is_five
    });
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// pointer.wheel.pressing-the-log-is-what-lets-the-wheel-move-it-and-typing-does-not-take-it-away
// ---------------------------------------------------------------------------------------------

/// Four states of the same client, and both halves read at each: which maps are in front of it,
/// and what a real detent does to the log.
pub(super) fn pressing_the_log_is_what_lets_the_wheel_move_it() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    fill_the_log(&mut c);
    let log = element(&c, LOG);
    let entry = element(&c, ENTRY);
    let over_log = middle_of(&c, log);
    let mut hands = Hands::new();

    // The log really is the case the whole thing turns on: it can be picked at but not typed
    // into, read off the shipped layout rather than assumed.
    let the_log_is_what_it_is = {
        let ui = &mut c.app_mut().ui_mut().expect("the UI shell is up").ui;
        let t = ui.text_element_mut(log).expect("the log is a text element");
        t.bits.selectable() && !t.bits.editable()
    };

    // Nothing holding the keyboard: no map, and the detent does nothing.
    hands.move_to(&mut c, over_log.0, over_log.1);
    c.tick(1);
    let at_start = log_scroll(&mut c);
    let nothing_yet = what_holds_the_keyboard(&c).is_none() && focused_maps(&mut c).is_empty();
    hands.wheel(&mut c, 1.0);
    c.tick(1);
    let inert = log_scroll(&mut c) == at_start;

    // Pressing the log: the scrolling map goes in and the one that swallows typing does not,
    // because the log is not a box that can be typed into.
    press_point(&mut c, &mut hands, over_log);
    let pressing_arms_it =
        what_holds_the_keyboard(&c) == Some(log) && focused_maps(&mut c) == vec![SCROLL_MAP, 8];

    // ...and the picture. One detent is one arrow's step.
    let up = element(&c, LOG_ARROW_UP);
    let up_at = middle_of(&c, up);
    let before_arrow = log_scroll(&mut c);
    press_point(&mut c, &mut hands, up_at);
    let one_line = before_arrow - log_scroll(&mut c);
    let the_arrow_moved_it = one_line > 0;
    // Pressing the arrow moved the keyboard to the arrow, which is itself a thing that scrolls --
    // so the map survives.
    let the_arrow_keeps_it = focused_maps(&mut c).contains(&SCROLL_MAP);
    hands.move_to(&mut c, over_log.0, over_log.1);
    c.tick(1);
    let before_wheel = log_scroll(&mut c);
    hands.wheel(&mut c, 1.0);
    c.tick(1);
    let a_detent_is_a_line = before_wheel - log_scroll(&mut c) == one_line;

    // Typing into the entry instead: all four maps go in, and the wheel over the log still works
    // -- which is the case that used to be the *only* one that did.
    let at_entry = middle_of(&c, entry);
    press_point(&mut c, &mut hands, at_entry);
    let typing_arms_it = what_holds_the_keyboard(&c) == Some(entry)
        && focused_maps(&mut c) == vec![SCROLL_MAP, 1, 7, 8];
    hands.move_to(&mut c, over_log.0, over_log.1);
    c.tick(1);
    let before_wheel = log_scroll(&mut c);
    hands.wheel(&mut c, 1.0);
    c.tick(1);
    let still_works_while_typing = before_wheel - log_scroll(&mut c) == one_line;

    // And letting the keyboard go takes the whole set back with it.
    c.app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .ui
        .relinquish_focus(entry);
    c.tick(1);
    let disarmed = focused_maps(&mut c).is_empty();
    let before_wheel = log_scroll(&mut c);
    hands.wheel(&mut c, 1.0);
    c.tick(1);
    let inert_again = log_scroll(&mut c) == before_wheel;

    c.assert_behaviour("pointer.wheel.pressing-the-log-is-what-lets-the-wheel-move-it-and-typing-does-not-take-it-away", move |_| {
        the_log_is_what_it_is
            && nothing_yet
            && inert
            && pressing_arms_it
            && the_arrow_moved_it
            && the_arrow_keeps_it
            && a_detent_is_a_line
            && typing_arms_it
            && still_works_while_typing
            && disarmed
            && inert_again
    });
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// pointer.wheel.the-arming-happens-when-the-keyboard-moves-and-once-per-move
// ---------------------------------------------------------------------------------------------

/// The maps go in when the keyboard moves, once, and not again every frame.
pub(super) fn the_arming_happens_when_the_keyboard_moves_and_once_per_move() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    let log = element(&c, LOG);
    let mut hands = Hands::new();
    let before = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .stats
        .focused_map_edges;

    let at = middle_of(&c, log);
    press_point(&mut c, &mut hands, at);
    let after_press = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .stats
        .focused_map_edges;
    let one_gain_one_arming = after_press == before + 1;

    c.tick(8);
    // Eight more frames with the keyboard where it is arm nothing further: re-arming every frame
    // would throw away what the maps are holding on to.
    let quiet = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .stats
        .focused_map_edges
        == after_press;

    c.app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .ui
        .relinquish_focus(log);
    c.tick(1);
    let the_other_edge = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .stats
        .focused_map_edges
        == after_press + 1;

    c.assert_behaviour(
        "pointer.wheel.the-arming-happens-when-the-keyboard-moves-and-once-per-move",
        move |_| one_gain_one_arming && quiet && the_other_edge,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// window.focus.a-window-remembers-the-box-that-held-the-keyboard-and-gives-it-back
// ---------------------------------------------------------------------------------------------

/// A window keeps the caret's place while it is put aside, and hands it back when it is worked in
/// again.
pub(super) fn a_window_remembers_the_box_that_held_the_keyboard() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    let root = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .flow
        .current()
        .expect("a screen")
        .roots()[0];
    let entry = element(&c, ENTRY);

    let remembered_on_the_window = {
        let ui = &mut c.app_mut().ui_mut().expect("the UI shell is up").ui;
        ui.activate(root);
        ui.take_focus(entry);
        // The window remembers the box itself, not the child on the way down to it -- and the box
        // is not the window's own child, so storing the wrong one would be invisible in a flat
        // tree.
        ui.node(root).expect("the window").focus_descendant == Some(entry)
            && ui.parent(entry) != Some(root)
            && ui.focus_element() == Some(entry)
    };

    // Put aside: the caret goes, and the memory of it does not.
    let put_aside = {
        let ui = &mut c.app_mut().ui_mut().expect("the UI shell is up").ui;
        ui.deactivate(root);
        ui.focus_element().is_none()
            && ui.node(root).expect("the window").focus_descendant == Some(entry)
    };

    // Worked in again: the caret comes back to where it was.
    let handed_back = {
        let ui = &mut c.app_mut().ui_mut().expect("the UI shell is up").ui;
        ui.activate(root);
        ui.focus_element() == Some(entry)
    };

    // ...and a window already being worked in does not have the caret put back into it: moving
    // the caret away and working in the same window again leaves it where the player put it.
    let not_re_done = {
        let ui = &mut c.app_mut().ui_mut().expect("the UI shell is up").ui;
        ui.set_focus_element(None);
        ui.activate(root);
        ui.focus_element().is_none()
    };
    let listed = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .activatable_elements()
        .contains(&root);

    // **The case the two paths do not collapse into one**: a window that loses to *another*
    // window. Its own tail is the only thing that takes its caret away then.
    let lost_to_another = {
        let ui = &mut c.app_mut().ui_mut().expect("the UI shell is up").ui;
        let other = ui
            .get_child_recursive(root, LOG_ARROW_UP)
            .expect("the log's arrow is in the shipped layout");
        let off_the_chain = !ui.is_ancestor_of(other, entry);
        ui.node_mut(other)
            .expect("the arrow")
            .flags
            .set_is_root_element(true);

        ui.deactivate(root);
        ui.activate(root);
        ui.take_focus(entry);
        let precondition = ui.focus_element() == Some(entry) && ui.active_element() == Some(root);

        ui.activate(other);
        let took_over = ui.active_element() == Some(other) && ui.focus_element().is_none();
        let still_remembered = ui.node(root).expect("the window").focus_descendant == Some(entry);
        ui.activate(root);
        let came_back = ui.focus_element() == Some(entry);

        ui.node_mut(other)
            .expect("the arrow")
            .flags
            .set_is_root_element(false);
        ui.set_focus_element(None);
        off_the_chain && precondition && took_over && still_remembered && came_back
    };

    // And working in something inside a window works in the **window**, never the thing itself.
    let the_window_not_the_thing = {
        let ui = &mut c.app_mut().ui_mut().expect("the UI shell is up").ui;
        ui.deactivate(root);
        let cleared = ui.active_element().is_none();
        let not_a_window = !ui.node(entry).expect("the box").flags.is_root_element();
        let forwarded = ui.activate(entry);
        cleared
            && not_a_window
            && forwarded
            && ui.active_element() == Some(root)
            && !ui.node(entry).expect("the box").flags.is_active()
    };

    c.assert_behaviour(
        "window.focus.a-window-remembers-the-box-that-held-the-keyboard-and-gives-it-back",
        move |_| {
            remembered_on_the_window
                && put_aside
                && handed_back
                && not_re_done
                && listed
                && lost_to_another
                && the_window_not_the_thing
        },
    );
    c.shutdown();
}

// =============================================================================================
// text-entry.filters.* and text-entry.ime.* -- what a box will take, and what an IME sends
//
// Eight scenarios, eight rows: the census of which boxes take only certain characters is itself a
// claim a player meets -- most boxes take anything and three of them do not -- so it keeps a row of
// its own rather than retiring as a denominator.
//
// Every character below is a real message built by the client's own pump and delivered where the
// window loop delivers one. No datagram leaves the process except the one the last scenario reads
// out of a socket-free endpoint.
// =============================================================================================

/// The chat entry: editable, one line, and one of the boxes that takes anything -- which is what
/// makes it the right box to prove a composed character on.
const TEXT_CHAT_ENTRY: ElementId = ElementId(0x1000_0016);
/// The caption beside a profession slider, which the client filters although nobody can type in it.
const ATTRIB_CAPTION: u32 = 0x1000_02ED;
/// The strip the stack splitter lives in, hidden until something is picked.
const SEL_OBJECT_FIELD: ElementId = ElementId(0x1000_019E);
/// The four boxes the client puts a filter on, and which filter.
const FILTERED_IDS: [u32; 4] = [0x1000_0402, ATTRIB_CAPTION, 0x1000_046B, 0x1000_01A3];

/// Digits only.
const NUMBER_PROBE: (bool, bool, bool) = (true, false, false);
/// Letters, an apostrophe, a space and a hyphen; no digits.
const NAME_PROBE: (bool, bool, bool) = (false, true, true);

/// Whether a box will take a character at all, probed by asking it -- three characters chosen to
/// tell the two filters apart: a digit, a letter, and the hyphen a name may have.
fn filter_probe(
    app: &mut dereth_client::app::App,
    h: dereth_ui::ElemHandle,
) -> Option<(bool, bool, bool)> {
    let f = kb_ui(app).text_element_mut(h)?.filter?;
    Some((f(0x37), f(0x61), f(0x2D)))
}

/// Press an element, having first proved the pointer really lands on it or inside it.
fn press_text_element(
    app: &mut dereth_client::app::App,
    hand: &mut KeyHand,
    h: dereth_ui::ElemHandle,
) {
    let at = {
        let u = kb_ui(app);
        let mut a = Some(h);
        while let Some(x) = a {
            assert!(
                u.node(x).expect("live").region.flags.visible,
                "every element between the screen and {h:?} must be shown"
            );
            a = u.parent(x);
        }
        let b = u.screen_clip_box(h);
        assert!(b.is_valid(), "the element has a real rectangle");
        let at = ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2);
        let hit = u.hit_test_screen(at.0, at.1);
        assert!(
            hit.is_some_and(|x| x == h || u.is_ancestor_of(h, x)),
            "the pointer at {at:?} must land on {h:?} or something inside it; it landed on {hit:?}"
        );
        at
    };
    hand.click_at(app, at);
}

fn text_element(app: &dereth_client::app::App, id: ElementId) -> dereth_ui::ElemHandle {
    let shell = app.ui().expect("the UI shell is up");
    let root = shell.flow.current().expect("a screen").roots()[0];
    shell
        .ui
        .get_child_recursive(root, id)
        .unwrap_or_else(|| panic!("{id:?} is in the shipped layout"))
}

// ---------------------------------------------------------------------------------------------
// text-entry.filters.three-of-the-clients-boxes-take-only-certain-characters
// ---------------------------------------------------------------------------------------------

/// Which boxes are fussy, out of all the boxes there are.
pub(super) fn three_of_the_clients_boxes_take_only_certain_characters() {
    use dereth_primitives::AssetSource as _;

    let mut c = HeadlessClient::new(ClientSpec::retail());
    let store = c.dat_store().expect("a retail client has a store").clone();
    let master_id = dereth_primitives::DataId(0x3900_0001);
    let bytes = store.read(master_id).expect("the master property record");
    let types =
        <dereth_assets::MasterProperty as dereth_assets::Decode>::decode_payload(master_id, &bytes)
            .expect("it decodes")
            .property_types();

    /// The last value an element's own look, or any of its named looks, gives this attribute.
    /// Every named look is searched, because an undercount here reads exactly like the census
    /// being right.
    fn declared(e: &dereth_ui::desc::ElementDesc, want: u32) -> Option<bool> {
        let mut v = None;
        for st in std::iter::once(&e.base).chain(e.states.values()) {
            for (id, value) in &st.properties.0 {
                if *id == want {
                    if let dereth_assets::ui::PropertyValue::Bool(b) = value {
                        v = Some(*b);
                    }
                }
            }
        }
        v
    }

    fn walk(
        e: &dereth_ui::desc::ElementDesc,
        lid: dereth_primitives::DataId,
        out: &mut Vec<(dereth_primitives::DataId, u32, Option<bool>)>,
    ) {
        // The two attributes together: a box that can be typed into, or only picked at.
        let editable = declared(e, 0x16);
        if editable.is_some() || declared(e, 0x27).is_some() {
            out.push((lid, e.element_id.0, editable));
        }
        for child in e.children.values() {
            walk(child, lid, out);
        }
    }

    let ids = store.ids_of(dereth_dat::DbType::UiLayout);
    let mut rows = Vec::new();
    let mut decoded = 0_usize;
    for id in &ids {
        let raw = store.read(*id).expect("a layout the directory lists reads");
        let layout = dereth_ui::desc::LayoutDesc::read(*id, &raw, &types).expect("it decodes");
        decoded += 1;
        for e in layout.elements.values() {
            walk(e, *id, &mut rows);
        }
    }
    // The space, so a census that silently read half the layouts cannot pass.
    let whole_corpus = ids.len() == 101 && decoded == ids.len();

    let editable: Vec<_> = rows.iter().filter(|r| r.2 == Some(true)).collect();
    let pairs: std::collections::BTreeSet<(dereth_primitives::DataId, u32)> =
        editable.iter().map(|r| (r.0, r.1)).collect();
    let element_ids: std::collections::BTreeSet<u32> = editable.iter().map(|r| r.1).collect();
    let counted = editable.len() == 41 && pairs.len() == 39 && element_ids.len() == 36;

    let filtered: std::collections::BTreeSet<u32> = FILTERED_IDS.iter().copied().collect();
    let four_of_them = filtered.len() == 4;
    let not_a_box: Vec<u32> = filtered
        .iter()
        .copied()
        .filter(|id| !element_ids.contains(id))
        .collect();
    let real_boxes = filtered
        .iter()
        .copied()
        .filter(|id| element_ids.contains(id))
        .count();
    // The odd one out, and the whole answer turns on it: one of the four is a caption the client
    // filters although nobody can type in it, so three of the client's boxes are fussy and the
    // other thirty-three take whatever the player types.
    let three_boxes_and_a_caption = not_a_box == vec![ATTRIB_CAPTION] && real_boxes == 3;

    c.assert_behaviour("text-entry.filters.three-of-the-clients-boxes-take-only-certain-characters-and-the-rest-take-anything", move |_| {
        whole_corpus && counted && four_of_them && three_boxes_and_a_caption
    });
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// text-entry.filters.the-name-box-takes-the-letters-a-name-may-have-and-nothing-else
// ---------------------------------------------------------------------------------------------

/// The character's name box, typed into by hand.
pub(super) fn the_name_box_takes_the_letters_a_name_may_have() {
    let mut c = a_wizard_on_the_summary_page();
    let app = c.app_mut();
    let name = text_element(app, chargen::NAME_FIELD);
    let mut hand = KeyHand::new();
    press_text_element(app, &mut hand, name);
    let focused = kb_ui(app).focus_element() == Some(name);

    // A press is not a select-all, so the prompt stays and the caret sits past it; the prompt is
    // read off the live box rather than written down here.
    let before = kb_text(app, name);
    let prompted = !before.is_empty();

    // Everything a name may have, interleaved with everything a player is most likely to try that
    // it may not: digits and the punctuation that is not an apostrophe, a space or a hyphen.
    hand.type_text(app, "Bo0!b_-.Sm+ith's");
    let refused_the_rest = kb_text(app, name) == format!("{before}Bob-Smith's");

    // The space on its own, because it is the one accepted character easiest to lose by keeping
    // only the letters.
    hand.type_text(app, " Jr");
    let space_allowed = kb_text(app, name) == format!("{before}Bob-Smith's Jr");

    // ...and the same from the other side: the box really is asking the name question and not
    // some other one that happens to agree on these sixteen characters.
    let the_right_filter = filter_probe(app, name) == Some(NAME_PROBE);

    c.assert_behaviour(
        "text-entry.filters.the-name-box-takes-the-letters-a-name-may-have-and-nothing-else",
        move |_| focused && prompted && refused_the_rest && space_allowed && the_right_filter,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// text-entry.filters.the-how-many-box-takes-digits-only
// ---------------------------------------------------------------------------------------------

/// The box a player says how many of a stack to move in.
pub(super) fn the_how_many_box_takes_digits_only() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(8));
    {
        let app = c.app_mut();
        let field = text_element(app, SEL_OBJECT_FIELD);
        let shell = app.ui_mut().expect("the UI shell is up");
        let u = &mut shell.ui;
        u.set_visible(field, true);
        let box_h = u
            .get_child_recursive(field, dereth_ui_screens::toolbar::splitter::ENTRY_BOX)
            .expect("the box is under the strip");
        u.set_visible(box_h, true);
        if let Some(s) = u.get_child_recursive(field, dereth_ui_screens::toolbar::splitter::SLIDER)
        {
            u.set_visible(s, true);
        }
        if let Some(t) = u.text_element_mut(box_h) {
            t.set_text("20");
        }
        let screen = shell.flow.current_mut().expect("a screen is up");
        let any: &mut dyn std::any::Any = &mut **screen;
        any.downcast_mut::<dereth_ui_screens::screens::gameplay::GamePlayScreen>()
            .expect("the gameplay screen")
            .splitter = dereth_ui_screens::toolbar::splitter::Splitter::new(20);
    }
    c.tick(1);

    let app = c.app_mut();
    let box_h = text_element(app, dereth_ui_screens::toolbar::splitter::ENTRY_BOX);
    let mut hand = KeyHand::new();
    press_text_element(app, &mut hand, box_h);
    let focused = kb_ui(app).focus_element() == Some(box_h);

    // This shape is the one that matters: the number is read the way a program reads one, so an
    // unfiltered box would read a leading zero and an x as sixteen-and-something and a player
    // asking for twelve would move a different number of things. The filter is what makes that
    // reading safe.
    hand.type_text(app, "0x1x2");
    let digits_only = kb_text(app, box_h) == "012";
    let the_right_filter = filter_probe(app, box_h) == Some(NUMBER_PROBE);

    c.assert_behaviour(
        "text-entry.filters.the-how-many-box-takes-digits-only",
        move |_| focused && digits_only && the_right_filter,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// text-entry.ime.every-composition-message-is-handed-to-the-desktop
// ---------------------------------------------------------------------------------------------

/// The client writes no input method of its own: it offers every composition message on and lets
/// the desktop drive it.
pub(super) fn every_composition_message_is_handed_to_the_desktop() {
    let mut pump = dereth_desktop::pump::Pump::new();
    pump.state.is_ready = true;
    pump.state.is_active_app = true;

    let eight = [
        win_msg::WM_INPUTLANGCHANGE,
        win_msg::WM_IME_STARTCOMPOSITION,
        win_msg::WM_IME_ENDCOMPOSITION,
        win_msg::WM_IME_COMPOSITION,
        win_msg::WM_IME_SETCONTEXT,
        win_msg::WM_IME_NOTIFY,
        win_msg::WM_IME_CONTROL,
        win_msg::WM_IME_COMPOSITIONFULL,
    ];
    let mut all_offered = true;
    for m in eight {
        let r = pump.dispatch(dereth_input::win32::Win32Message::new(m, 0, 0, 1_000));
        all_offered &= r.effects == vec![Effect::ForwardToBrowser] && !r.handled && r.result == 0;
    }

    // The control: an ordinary typed character takes the other road -- offered on, and then given
    // to the client's own input. Without it the eight above would prove nothing about the arm.
    let r = pump.dispatch(dereth_input::win32::Win32Message::new(
        win_msg::WM_CHAR,
        u32::from('a') as usize,
        0,
        1_100,
    ));
    let a_character_is_different =
        r.effects == vec![Effect::ForwardToBrowser, Effect::ForwardToInputManager];

    let mut c = HeadlessClient::model();
    c.assert_behaviour("text-entry.ime.every-composition-message-is-handed-to-the-desktop-and-an-ordinary-character-is-not", move |_| {
        all_offered && a_character_is_different
    });
}

// ---------------------------------------------------------------------------------------------
// text-entry.ime.a-composed-character-reaches-the-box-unchanged
// ---------------------------------------------------------------------------------------------

/// A character an input method commits arrives like any other and must not be narrowed.
pub(super) fn a_composed_character_reaches_the_box_unchanged() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(8));
    let app = c.app_mut();
    let entry = text_element(app, TEXT_CHAT_ENTRY);
    let takes_anything = kb_ui(app)
        .text_element_mut(entry)
        .expect("the entry")
        .filter
        .is_none();

    let mut hand = KeyHand::new();
    press_text_element(app, &mut hand, entry);
    let focused = kb_ui(app).focus_element() == Some(entry);

    // A plain letter, an accented one an input method or a dead key commits, a Chinese character,
    // and another plain letter.
    hand.type_units(app, &[0x0065, 0x00E9, 0x4E2D, 0x007A]);
    let unchanged = kb_text(app, entry) == "e\u{00E9}\u{4E2D}z";

    c.assert_behaviour(
        "text-entry.ime.a-composed-character-reaches-the-box-unchanged",
        move |_| takes_anything && focused && unchanged,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// text-entry.paste.a-paste-puts-the-clipboard-in-once-and-not-a-letter-with-it
// ---------------------------------------------------------------------------------------------

/// Pasting with the keyboard pastes, and does not also type the letter that was held.
pub(super) fn a_paste_puts_the_clipboard_in_once_and_not_a_letter_with_it() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(8));
    let app = c.app_mut();
    let entry = text_element(app, TEXT_CHAT_ENTRY);
    let mut hand = KeyHand::new();
    press_text_element(app, &mut hand, entry);
    let focused = kb_ui(app).focus_element() == Some(entry);
    kb_ui(app).clipboard = "paste".to_string();

    // The desktop reports the character the key really produced with the modifier held, which is
    // not the letter on the key -- and the client must not put that letter in as well.
    hand.key(app, KeyCode::ControlLeft, true);
    hand.key_with_text(app, KeyCode::KeyV, Some("\u{16}"), false);
    app.frame();
    let pasted_once = kb_text(app, entry) == "paste";

    hand.key(app, KeyCode::KeyV, false);
    hand.key(app, KeyCode::ControlLeft, false);
    hand.key_with_text(app, KeyCode::KeyV, Some("v"), false);
    hand.key(app, KeyCode::KeyV, false);
    hand.key(app, KeyCode::ShiftLeft, true);
    hand.key_with_text(app, KeyCode::KeyV, Some("V"), false);
    hand.key(app, KeyCode::KeyV, false);
    hand.key(app, KeyCode::ShiftLeft, false);
    app.frame();
    let plain_and_shifted = kb_text(app, entry) == "pastevV";

    // And a character the desktop produces with a keyboard of its own, and one it produces with
    // the right-hand alt key, still reach the box.
    let t = hand.time_ms + 10;
    for m in key_text_messages(true, false, Some("\u{4E2D}"), t) {
        hand.send(app, m);
    }
    let t = hand.time_ms + 20;
    for m in key_text_messages(true, true, Some("\u{20AC}"), t) {
        hand.send(app, m);
    }
    app.frame();
    let other_layouts = kb_text(app, entry) == "pastevV\u{4E2D}\u{20AC}";

    c.assert_behaviour(
        "text-entry.paste.a-paste-puts-the-clipboard-in-once-and-not-a-letter-with-it",
        move |_| focused && pasted_once && plain_and_shifted && other_layouts,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// text-entry.paste.line-breaks-in-what-was-pasted-never-reach-the-shard
// ---------------------------------------------------------------------------------------------

/// Pasting several lines into a one-line box leaves one line, and that is what is said.
pub(super) fn line_breaks_in_what_was_pasted_never_reach_the_shard() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(8));
    let mut net = dereth_client_runtime::net::ClientNetwork::new(
        "127.0.0.1:19000",
        7304,
        "paste",
        "unused",
        0,
    )
    .expect("a socket-free endpoint");
    net.session.transport.add_connection(
        0xB,
        0,
        1,
        0xDEAD_BEEF,
        0x1234_5678,
        Some("127.0.0.1:19000".parse().expect("the peer address")),
    );
    c.attach_replay(net);

    let app = c.app_mut();
    let entry = text_element(app, TEXT_CHAT_ENTRY);
    let mut hand = KeyHand::new();
    press_text_element(app, &mut hand, entry);
    kb_ui(app).clipboard = "one\r\ntwo\nthree\rfour\tfive".into();
    hand.key(app, KeyCode::ControlLeft, true);
    hand.key_with_text(app, KeyCode::KeyV, Some("\u{16}"), false);
    hand.key(app, KeyCode::KeyV, false);
    hand.key(app, KeyCode::ControlLeft, false);
    app.frame();

    let one_line = kb_text(app, entry) == "onetwothreefourfive";
    // A pasted line break is not a press of the return key: the line is not sent by pasting it.
    let not_sent =
        kb_ui(app).focus_element() == Some(entry) && app.interaction().last_sent.is_empty();

    // The two edges of the return key with no frame between them, so that the frame which turns
    // them into a line is the harness's own -- which is what records the line for the reading
    // below. What the client last asked for is a one-frame window that the next frame writes
    // over, so a scenario that ran its own frames here would find it already gone.
    hand.key_quiet(app, KeyCode::Enter, true);
    hand.key_quiet(app, KeyCode::Enter, false);
    c.tick(1);
    let said = c
        .outbound()
        .iter()
        .filter_map(|r| match r {
            dereth_client_model::Request::Talk(m) => Some(m.message.clone()),
            _ => None,
        })
        .collect::<Vec<String>>()
        == vec!["onetwothreefourfive".to_string()];
    c.tick(1);

    let mut actions = Vec::new();
    for (bytes, _) in c.replay_net_mut().expect("the endpoint").take_outgoing() {
        let packet =
            dereth_transport::wire::ParsedPacket::parse(&bytes).expect("the endpoint's output");
        for fragment in packet.fragments {
            if fragment.header.queue_id == 3 {
                assert_eq!(fragment.header.num_frags, 1);
                actions.push(fragment.payload);
            }
        }
    }
    let mut expected = vec![0xb1, 0xf7, 0, 0, 1, 0, 0, 0, 0x15, 0, 0, 0, 19, 0];
    expected.extend_from_slice(b"onetwothreefourfive");
    expected.extend_from_slice(&[0, 0, 0]);
    let one_request = actions == vec![expected];
    let cleared = kb_text(c.app_mut(), entry).is_empty();

    c.assert_behaviour(
        "text-entry.paste.line-breaks-in-what-was-pasted-never-reach-the-shard",
        move |_| one_line && not_sent && said && one_request && cleared,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// text-entry.filters.the-how-many-of-a-component-box-takes-digits-only-on-every-row
// ---------------------------------------------------------------------------------------------

/// long-solo-play's own character.
const COMPONENT_PLAYER: dereth_primitives::ObjectId = dereth_primitives::ObjectId(0x5000_000A);
/// The panel page the spell pages live in.
const SPELL_PAGE_ID: ElementId = dereth_ui_screens::panels::remaining::SPELL_PAGE;
/// The list of components, which is how the right sub-page is found without naming it.
const COMPONENT_LIST_ID: ElementId = dereth_ui_screens::panels::spellcomponent::COMPONENT_LIST;

/// The box beside each spell component, where a player says how many to keep.
pub(super) fn the_how_many_of_a_component_box_takes_digits_only_on_every_row() {
    let n = dereth_client_net::client_session::testing::Corpus::load("long-solo-play")
        .expect("the recordings are committed to the repository")
        .expect("long-solo-play is one of them")
        .blobs
        .len();

    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    c.app_mut().probe_mut().objects_mut().world.player = Some(COMPONENT_PLAYER);
    c.when(dereth_testkit::Inbound::from_corpus("long-solo-play", 0..n));
    c.tick(3);

    // Raise the page the components live on, the way the screen does, and then press the tab a
    // player presses -- found by which sub-page carries the list rather than by naming it.
    {
        let app = c.app_mut();
        let shell = app.ui_mut().expect("the UI shell is up");
        let any: &mut dyn std::any::Any = &mut **shell.flow.current_mut().expect("a screen");
        let s = any
            .downcast_mut::<dereth_ui_screens::screens::gameplay::GamePlayScreen>()
            .expect("the gameplay screen");
        let panel = s
            .panels
            .pages
            .iter()
            .find(|p| p.element == SPELL_PAGE_ID)
            .expect("the spell page is a registered page")
            .panel_id;
        s.recv_set_panel_visibility(&mut shell.ui, panel, true);
    }
    c.tick(3);

    let app = c.app_mut();
    let tab = {
        let u = kb_ui(app);
        let page = text_element_in(u, SPELL_PAGE_ID);
        let pairs: Vec<(ElementId, ElementId)> = u
            .node(page)
            .and_then(|n| {
                n.behaviour
                    .as_ref()?
                    .as_any()?
                    .downcast_ref::<dereth_ui::widgets::panel::Panel>()
            })
            .expect("the spell page is a panel")
            .page_to_tab
            .iter()
            .map(|(p, t)| (*p, *t))
            .collect();
        let mut found = None;
        for (page_id, tab_id) in pairs {
            let Some(pe) = u.get_child_recursive(page, page_id) else {
                continue;
            };
            if u.get_child_recursive(pe, COMPONENT_LIST_ID).is_some() {
                found = u.get_child_recursive(page, tab_id);
                break;
            }
        }
        found.expect("one sub-page of the spell page carries the component list")
    };
    let mut hand = KeyHand::new();
    let at = {
        let u = kb_ui(app);
        let b = u.screen_clip_box(tab);
        assert!(b.is_valid(), "the tab has a real rectangle");
        ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
    };
    hand.click_at(app, at);
    app.frame();
    app.frame();

    let rows: Vec<dereth_ui_screens::panels::spellcomponent::DrawnRow> =
        app.hud().panels.spell_components.rows.clone();
    let there_are_rows = !rows.is_empty();
    let field = rows[0]
        .desired_field
        .expect("the first row carries the box");

    press_text_element(app, &mut hand, field);
    let focused = kb_ui(app).focus_element() == Some(field);
    // This box is one of the two in the whole shipped layout that select everything on the first
    // press, so the first accepted key replaces what was there.
    hand.type_text(app, "9zz9");
    let digits_only = kb_text(app, field) == "99";

    // Every row, not only the one typed into: the box is set up inside the per-component loop, so
    // a client that did it once when the panel was built would pass the reading above and fail
    // here on the second row.
    let mut every_row = true;
    for r in &rows {
        let h = r.desired_field.expect("every row carries the box");
        every_row &= filter_probe(app, h) == Some(NUMBER_PROBE);
    }

    c.assert_behaviour(
        "text-entry.filters.the-how-many-of-a-component-box-takes-digits-only-on-every-row",
        move |_| there_are_rows && focused && digits_only && every_row,
    );
    c.shutdown();
}

/// An element by id under the element manager's own root, off a system the caller already holds.
fn text_element_in(u: &dereth_ui::UiSystem, id: ElementId) -> dereth_ui::ElemHandle {
    u.get_element(id)
        .unwrap_or_else(|| panic!("{id:?} is in the shipped layout"))
}
