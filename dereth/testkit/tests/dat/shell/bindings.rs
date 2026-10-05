//! Shell fixtures and scenarios for bindings.

use super::*;
// ---------------------------------------------------------------------------------------------
// options.key-bindings.*
//
// Each of these owns a scratch preferences directory of its own **and clears it first**: the client
// merges the player's saved key map over the shipped default on the way in and writes one out on
// the way out, so a second run of the rebind scenario would otherwise load the binding the first
// run made and fail on its own precondition. That state leak reads exactly like a flaky test.
// ---------------------------------------------------------------------------------------------

/// The first key button of the walk-forward row of the key-bindings page.
fn walk_forward_key_button(app: &mut dereth_client::app::App) -> dereth_ui::ElemHandle {
    let shell = app.ui_mut().expect("the UI shell is up");
    let screen = shell.flow.current_mut().expect("a screen is current");
    let any: &mut dyn std::any::Any = &mut **screen;
    let gameplay = any
        .downcast_mut::<dereth_ui_screens::screens::gameplay::GamePlayScreen>()
        .expect("the gameplay screen is current");
    let i = gameplay
        .key_bindings
        .row_of(MOVEMENT, MOVE_FORWARD)
        .expect("walking forward has a row in a running client");
    gameplay.key_bindings.rows[i].key_buttons[0]
}

/// Click an element through the shell's own broadcast, which is the route a button takes.
fn click_element(c: &mut HeadlessClient, h: dereth_ui::ElemHandle) {
    c.app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .ui
        .broadcast_element_message(h, dereth_ui::msg::element::id::BUTTON_CLICKED, 7, 0);
    c.tick(1);
}

/// What the running client has bound to walking forward.
fn walk_forward_keys(c: &mut HeadlessClient) -> Vec<dereth_input::ControlChord> {
    c.app_mut()
        .input_manager_mut()
        .expect("the input shell")
        .keys_for_action(MOVE_FORWARD, MOVEMENT)
}

/// A client for the key-bindings page.
///
/// **Nothing it does persists between runs**, which matters here: the client merges the player's
/// saved key map over the shipped default on the way in and writes one out on the way out, so a
/// rebind that survived would make the next run of the scenario below fail on its own precondition
/// -- and would read exactly like a flaky test. The harness points every client's preferences at a
/// directory under the temporary one that is deliberately never created, so the load finds nothing
/// and the save has nowhere to go. That closes the state leak by construction rather than by
/// clearing a folder.
fn a_client_for_the_key_bindings_page() -> HeadlessClient {
    HeadlessClient::new(ClientSpec::gameplay_in_world(6))
}

/// The page builds one row for every bindable action, once.
pub(super) fn the_key_binding_page_builds_one_row_per_bindable_action() {
    let mut c = a_client_for_the_key_bindings_page();

    let st = c.view().expect_app().key_binding_stats();
    let built_once = st.init_calls == 1
        && st.bindable_actions > 0
        && st.rows_built == st.bindable_actions
        && st.failures == 0
        && st.headers > 0;

    // It is a per-page call and not a per-frame one: rebuilding flushes every list, so a per-frame
    // call would throw a row away in the middle of a capture.
    c.tick(10);
    let not_again = c.view().expect_app().key_binding_stats().init_calls == 1;

    // And the rows are really in the tree rather than only counted: the walk-forward row is there
    // and shows the key the merged map reports.
    let labels = {
        let shell = c.app_mut().ui_mut().expect("the UI shell is up");
        let screen = shell.flow.current_mut().expect("a screen is current");
        let any: &mut dyn std::any::Any = &mut **screen;
        let gameplay = any
            .downcast_mut::<dereth_ui_screens::screens::gameplay::GamePlayScreen>()
            .expect("the gameplay screen");
        let i = gameplay
            .key_bindings
            .row_of(MOVEMENT, MOVE_FORWARD)
            .expect("the row");
        gameplay.key_bindings.rows[i].button_labels.clone()
    };

    c.assert_behaviour(
        "options.key-bindings.the-page-builds-one-row-for-every-bindable-action-once",
        move |_| built_once && not_again && labels.first().is_some_and(|l| !l.is_empty()),
    );
    c.shutdown();
}

/// A key pressed over a row rebinds it, and the key it was taken from is left bound to nothing.
pub(super) fn a_key_pressed_over_a_row_rebinds_it() {
    use dereth_input::binding::DO_NOTHING;

    let mut c = a_client_for_the_key_bindings_page();
    let mut hands = Hands::new();

    let before = walk_forward_keys(&mut c);
    let started_on_the_shipped_key = before
        .contains(&control(SCAN_W, dereth_input::spec::activation::CLICK))
        && !before.contains(&control(SCAN_FREE, dereth_input::spec::activation::CLICK));
    let nothing_capturing = !c
        .app_mut()
        .input_manager_mut()
        .expect("the input shell")
        .manager
        .key_hit_handler_registered();

    let button = walk_forward_key_button(c.app_mut());
    click_element(&mut c, button);
    let one_message = c.view().expect_app().key_binding_stats().row_events == 1;
    let capturing = c
        .app_mut()
        .input_manager_mut()
        .expect("the input shell")
        .manager
        .key_hit_handler_registered();

    // The key, through the pump. The press is not an answer; the release is.
    hands.press(&mut c, key(KeyCode::F7));
    hands.release(&mut c, key(KeyCode::F7));
    c.tick(1);

    let st = c.view().expect_app().key_binding_stats();
    let both_diverted = st.key_hits_offered == 2 && st.key_hits_taken == 2 && st.bindings_made == 1;
    let handler_gone = !c
        .app_mut()
        .input_manager_mut()
        .expect("the input shell")
        .manager
        .key_hit_handler_registered();

    let after = walk_forward_keys(&mut c);
    let moved = after.contains(&control(SCAN_FREE, dereth_input::spec::activation::CLICK))
        && !after.contains(&control(SCAN_W, dereth_input::spec::activation::CLICK));
    // The freed key is bound to nothing at all rather than removed: a removed binding gets its
    // shipped default back on the next merge, which would leave both keys firing.
    let freed_not_deleted = c
        .app_mut()
        .input_manager_mut()
        .expect("the input shell")
        .manager
        .keymap
        .section(MOVEMENT)
        .expect("the movement section")
        .bindings()
        .iter()
        .find(|(qc, _)| {
            qc.is_exactly_equal(&control(SCAN_W, dereth_input::spec::activation::CLICK))
        })
        .map(|(_, a)| *a)
        == Some(DO_NOTHING);

    c.assert_behaviour(
        "options.key-bindings.a-key-pressed-over-a-row-rebinds-it-and-frees-the-old-key",
        move |_| {
            started_on_the_shipped_key
                && nothing_capturing
                && one_message
                && capturing
                && both_diverted
                && handler_gone
                && moved
                && freed_not_deleted
        },
    );
    c.shutdown();
}

/// A capture in flight swallows the key, and the key comes back once it is over.
pub(super) fn a_capture_in_flight_swallows_the_key() {
    let mut c = a_client_for_the_key_bindings_page();
    let mut hands = Hands::new();

    // Direction one: with nothing capturing, the key walks.
    let walks_before = held_moves(&mut c, &mut hands, KeyCode::KeyW, forward);
    let nothing_diverted = c.view().expect_app().key_binding_stats().key_hits_offered == 0;

    // Direction two: with a capture in flight, the same key on the same client does not.
    let button = walk_forward_key_button(c.app_mut());
    click_element(&mut c, button);
    let walks_during = held_moves(&mut c, &mut hands, KeyCode::KeyW, forward);
    let diverted_instead = c.view().expect_app().key_binding_stats().key_hits_offered > 0;

    // ...and afterwards it is back. Binding the key to the action it already had changes nothing,
    // so the map is where it started.
    let capture_over = !c
        .app_mut()
        .input_manager_mut()
        .expect("the input shell")
        .manager
        .key_hit_handler_registered();
    let still_bound =
        walk_forward_keys(&mut c).contains(&control(SCAN_W, dereth_input::spec::activation::CLICK));
    let walks_after = held_moves(&mut c, &mut hands, KeyCode::KeyW, forward);

    c.assert_behaviour(
        "options.key-bindings.a-capture-in-flight-swallows-the-key-and-gives-it-back-afterwards",
        move |_| {
            walks_before
                && nothing_diverted
                && !walks_during
                && diverted_instead
                && capture_over
                && still_bound
                && walks_after
        },
    );
    c.shutdown();
}

// =============================================================================================
// options.key-bindings.* -- the rest of the key-bindings page
//
// The page is whole: the cells read a key name rather than a symbol, the sections read a word
// rather than a token, undo is dark until something has changed, and pressing a cell does
// something. Eleven scenarios, eleven rows; none of them is a transcription and every gesture is a
// real press through the hit test or a real key through the client's own pump.
//
// The page's first three claims are in the `options.key-bindings.*` section above; these are the
// rest.
// =============================================================================================

/// The movement map and the walk-forward action, which every scenario below rebinds.
const KB_MOVEMENT: InputMapId = InputMapId(4);
const KB_FORWARD: ActionId = ActionId(0x29);
const KB_TURN_LEFT: ActionId = ActionId(0x2F);
const KB_UI_COMMANDS: InputMapId = InputMapId(0x1000_0009);
const KB_ESCAPE_ACTION: ActionId = ActionId(0x27);
const KB_USE_SELECTED: ActionId = ActionId(0x1000_0025);
const KB_W: u16 = 0x11;
const KB_A: u16 = 0x1E;
/// The one function key no shipped binding uses.
const KB_F7: u16 = 0x41;
const KB_DIGIT_1: u16 = 0x02;
const KB_SPELL_BAR: InputMapId = InputMapId(0x1000_0005);
const KB_SPELL_SLOT_1: ActionId = ActionId(0x1000_0065);
const KB_QUICK_SLOTS: InputMapId = InputMapId(0x1000_000C);
const KB_QUICKSLOT_1: ActionId = ActionId(0x1000_0042);
/// The one key the shipped text table gives a name of its own.
const KB_LEFT_CONTROL: u16 = 0x1D;

/// The two states the undo button is written into: greyed, and live.
const KB_GREYED: dereth_ui::StateId = dereth_ui::StateId(0x0D);
const KB_LIVE: dereth_ui::StateId = dereth_ui::StateId(1);

/// A client in the world with a settings directory of its own, which is where the page's own
/// files are written and read.
fn a_client_on_the_key_bindings(tag: &str) -> HeadlessClient {
    HeadlessClient::new(ClientSpec::gameplay_in_world(6).with_scratch_settings(tag))
}

fn kb_screen(
    c: &mut dereth_client::app::App,
) -> &mut dereth_ui_screens::screens::gameplay::GamePlayScreen {
    let shell = c.ui_mut().expect("the UI shell is up");
    let s = shell.flow.current_mut().expect("a screen is up");
    let any: &mut dyn std::any::Any = &mut **s;
    any.downcast_mut::<dereth_ui_screens::screens::gameplay::GamePlayScreen>()
        .expect("the gameplay screen")
}

pub(super) fn kb_ui(c: &mut dereth_client::app::App) -> &mut dereth_ui::UiSystem {
    &mut c.ui_mut().expect("the UI shell is up").ui
}

pub(super) fn kb_text(c: &mut dereth_client::app::App, h: dereth_ui::ElemHandle) -> String {
    kb_ui(c)
        .text_element_mut(h)
        .map(|t| t.glyphs.inq_text(false))
        .unwrap_or_default()
}

fn kb_state(c: &mut dereth_client::app::App, h: dereth_ui::ElemHandle) -> dereth_ui::StateId {
    kb_ui(c)
        .node(h)
        .map(|n| n.state)
        .expect("the element is in the tree")
}

/// The walk-forward row's first key cell.
fn forward_cell(c: &mut dereth_client::app::App) -> dereth_ui::ElemHandle {
    let s = kb_screen(c);
    let i = s
        .key_bindings
        .row_of(KB_MOVEMENT, KB_FORWARD)
        .expect("walking forward has a row");
    s.key_bindings.rows[i].key_buttons[0]
}

/// One row's key cells as **the letters actually on those cells** -- the grid a player looks at,
/// and not what the client holds underneath it.
fn drawn_cells(c: &mut dereth_client::app::App, map: InputMapId, action: ActionId) -> Vec<String> {
    let cells = {
        let s = kb_screen(c);
        let i = s
            .key_bindings
            .row_of(map, action)
            .expect("the action has a row on the page");
        s.key_bindings.rows[i].key_buttons.clone()
    };
    cells.into_iter().map(|h| kb_text(c, h)).collect()
}

/// The caption the page writes for one plain key, through the same resolver a row uses -- so an
/// expectation below is never a second guess at what the client would say.
fn kb_caption(c: &mut dereth_client::app::App, offset: u16) -> String {
    control_name(
        kb_ui(c),
        DeviceType::Keyboard,
        ControlCode::new(0, SubControlIndex::None, offset),
        false,
    )
}

/// Show an element and every ancestor of it, so a press can reach it: the key-bindings page is a
/// tab of the options window and is not on the screen by default.
fn kb_reveal(c: &mut dereth_client::app::App, mut h: dereth_ui::ElemHandle) {
    {
        let u = kb_ui(c);
        loop {
            u.set_visible(h, true);
            match u.parent(h) {
                Some(p) => h = p,
                None => break,
            }
        }
    }
    c.frame();
}

/// A real press: the element's own centre, hit-tested, then the button down and up.
fn kb_press(c: &mut dereth_client::app::App, h: dereth_ui::ElemHandle) {
    kb_reveal(c, h);
    let (cx, cy) = {
        let u = kb_ui(c);
        let b = u.screen_box(h);
        ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
    };
    assert_eq!(
        kb_ui(c).hit_test_screen(cx, cy),
        Some(h),
        "the press must land on the element itself, not on something drawn over it"
    );
    {
        let u = kb_ui(c);
        u.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, cx, cy);
        u.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, cx, cy, false);
    }
    c.frame();
    c.frame();
}

/// The right-hand press, which is what erases a binding.
fn kb_press_right(c: &mut dereth_client::app::App, h: dereth_ui::ElemHandle) {
    kb_reveal(c, h);
    let (cx, cy) = {
        let u = kb_ui(c);
        let b = u.screen_box(h);
        ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
    };
    assert_eq!(kb_ui(c).hit_test_screen(cx, cy), Some(h));
    {
        let u = kb_ui(c);
        u.mouse_down(dereth_ui::focus::action::SECONDARY_CLICK, cx, cy);
        u.mouse_up(dereth_ui::focus::action::SECONDARY_CLICK, cx, cy, false);
    }
    c.frame();
    c.frame();
}

/// One key, pressed and let go, through the client's own pump.
fn kb_tap(c: &mut dereth_client::app::App, hand: &mut KeyHand, code: KeyCode) {
    hand.key(c, code, true);
    hand.key(c, code, false);
}

/// The keyboard, driven the way the window loop drives it: the client's own pump builds each
/// message and the real input shell receives it.
pub(super) struct KeyHand {
    pub(super) pump: dereth_desktop::pump::Pump,
    pub(super) time_ms: u32,
}

impl KeyHand {
    pub(super) fn new() -> Self {
        let mut pump = dereth_desktop::pump::Pump::new();
        pump.state.is_ready = true;
        pump.state.is_active_app = true;
        Self {
            pump,
            time_ms: 500_000,
        }
    }

    pub(super) fn key(&mut self, app: &mut dereth_client::app::App, code: KeyCode, down: bool) {
        self.time_ms += 10;
        let m = self
            .pump
            .key_message_for(code, down, self.time_ms)
            .expect("the host names this key");
        self.pump.dispatch(m);
        if let Some(input) = app.input_manager_mut() {
            input.on_message(m);
        }
        app.frame();
    }
}

fn kb_control(offset: u16) -> dereth_input::ControlChord {
    dereth_input::ControlChord::new(
        ControlCode::new(0, SubControlIndex::None, offset),
        0,
        dereth_input::spec::activation::CLICK,
    )
}

fn keys_for_forward(c: &mut dereth_client::app::App) -> Vec<dereth_input::ControlChord> {
    c.input_manager_mut()
        .expect("the input manager")
        .keys_for_action(KB_FORWARD, KB_MOVEMENT)
}

/// The box a row has raised, and the place it holds in the queue.
fn row_dialog(c: &mut dereth_client::app::App, which: RowDialog) -> (u64, dereth_ui::ElemHandle) {
    let context = {
        let s = kb_screen(c);
        let i = s
            .key_bindings
            .row_of(KB_MOVEMENT, KB_FORWARD)
            .expect("walking forward has a row");
        s.key_bindings.rows[i]
            .dialog_context(which)
            .expect("the row owns the place")
    };
    let root = kb_ui(c)
        .dialogs
        .info(context)
        .and_then(|info| info.element)
        .expect("the row's place has a box drawn for it");
    (context, root)
}

/// The one text element a box draws its question in.
fn kb_prompt(c: &mut dereth_client::app::App, dialog: dereth_ui::ElemHandle) -> String {
    let h = kb_ui(c)
        .get_child_recursive(dialog, dereth_ui::dialog::base::child::TEXT)
        .expect("the box's own text element");
    kb_text(c, h)
}

fn kb_child(
    c: &mut dereth_client::app::App,
    dialog: dereth_ui::ElemHandle,
    id: ElementId,
) -> dereth_ui::ElemHandle {
    kb_ui(c)
        .get_child_recursive(dialog, id)
        .expect("the shipped dialog layout carries it")
}

/// The page's own file box, which is not a row's.
fn page_dialog(
    c: &mut dereth_client::app::App,
    kind: dereth_ui::dialog::DialogKind,
) -> (u64, dereth_ui::ElemHandle) {
    let info = kb_ui(c)
        .dialogs
        .open_on(DIALOG_QUEUE)
        .expect("the file box is open")
        .clone();
    assert_eq!(info.kind, kind, "the shipped kind of that file box");
    (
        info.context,
        info.element.expect("it has a box drawn for it"),
    )
}

fn submit_save_name(c: &mut dereth_client::app::App, name: &str) {
    let save = kb_screen(c)
        .key_bindings
        .save_button
        .expect("the save button is bound");
    kb_press(c, save);
    let (context, dialog) = page_dialog(c, dereth_ui::dialog::DialogKind::ConfirmationTextInput);
    let box_ = kb_ui(c)
        .get_child_recursive(
            dialog,
            dereth_ui::dialog::base::child::CONFIRM_TEXT_INPUT_BOX,
        )
        .filter(|h| *h != dialog)
        .expect("the file-name box");
    kb_ui(c)
        .text_element_mut(box_)
        .expect("the name is editable text")
        .set_text(name);
    let accept = kb_child(
        c,
        dialog,
        dereth_ui::dialog::base::child::CONFIRM_TEXT_INPUT_ACCEPT,
    );
    kb_press(c, accept);
    assert!(
        kb_ui(c).dialogs.info(context).is_none(),
        "the name box closes"
    );
}

// ---------------------------------------------------------------------------------------------
// options.key-bindings.a-cell-shows-the-key-the-way-the-desktop-names-it
// ---------------------------------------------------------------------------------------------

/// A cell shows what a player would call the key, not the name the client looks it up by.
pub(super) fn a_cell_shows_the_key_the_way_the_desktop_names_it() {
    let mut c = a_client_on_the_key_bindings("keybinding-name");
    let starts_bound = keys_for_forward(c.app_mut()).contains(&kb_control(KB_W));

    let cell = forward_cell(c.app_mut());
    let caption = kb_text(c.app_mut(), cell);
    let plain = caption == "W" && !caption.starts_with("DIK_");
    // The other half of the same lookup: the one key the shipped text table really does name, so
    // a flat table of symbols cannot pass for the lookup.
    let named = kb_caption(c.app_mut(), KB_LEFT_CONTROL) == "Left Ctrl";

    c.assert_behaviour(
        "options.key-bindings.a-cell-shows-the-key-the-way-the-desktop-names-it",
        move |_| starts_bound && plain && named,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// options.key-bindings.resting-on-a-cell-says-what-a-press-there-would-do
// ---------------------------------------------------------------------------------------------

/// The letters a tooltip really draws when the pointer rests on a cell.
fn kb_tooltip(c: &mut dereth_client::app::App, cell: dereth_ui::ElemHandle, time: f64) -> String {
    kb_reveal(c, cell);
    let u = kb_ui(c);
    let b = u.screen_box(cell);
    let (x, y) = ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2);
    assert_eq!(u.hit_test_screen(x, y), Some(cell));
    u.mouse_move(dereth_primitives::LocalTime(time), x, y);
    for n in 1..=8 {
        u.use_time(
            dereth_primitives::LocalTime(time + f64::from(n) * 0.25),
            &mut dereth_ui::NullInputPump,
        );
    }
    let tooltip = u
        .tooltip_element()
        .expect("resting on a key cell shows a tooltip");
    let mut draw = dereth_ui::RecordingDrawBackend::default();
    u.draw(&mut draw);
    draw.calls
        .iter()
        .filter(|call| {
            let mut h = Some(call.who);
            while let Some(p) = h {
                if p == tooltip {
                    return true;
                }
                h = u.parent(p);
            }
            false
        })
        .flat_map(|call| {
            call.glyphs
                .iter()
                .map(|g| char::from_u32(u32::from(g.ch)).expect("a character"))
        })
        .collect()
}

/// A cell with a key in it says how to take it away; an empty one says how to fill it.
pub(super) fn resting_on_a_cell_says_what_a_press_there_would_do() {
    let mut c = a_client_on_the_key_bindings("keybinding-tooltips");
    let cells = {
        let s = kb_screen(c.app_mut());
        let i = s
            .key_bindings
            .row_of(KB_MOVEMENT, KB_FORWARD)
            .expect("the row");
        s.key_bindings.rows[i].key_buttons.clone()
    };
    let three_cells = cells.len() == 3;
    let empty = *cells.last().expect("a third cell");
    let third_is_empty = kb_text(c.app_mut(), empty).is_empty();

    let bound = kb_tooltip(c.app_mut(), cells[0], 20.0);
    let says_the_key = bound.contains('W') && bound.contains("Right-click");
    let vacant = kb_tooltip(c.app_mut(), empty, 25.0);
    let says_how_to_fill = (vacant.contains("Left-Click") || vacant.contains("Left-click"))
        && !vacant.contains("Right-click");

    // Take both of the shipped bindings away through the press that takes them away, and the
    // first cell now says what an empty one says.
    for _ in 0..2 {
        kb_press_right(c.app_mut(), cells[0]);
    }
    let erased = keys_for_forward(c.app_mut()).is_empty();
    let now_vacant = kb_tooltip(c.app_mut(), cells[0], 30.0) == vacant;

    c.assert_behaviour(
        "options.key-bindings.resting-on-a-cell-says-what-a-press-there-would-do",
        move |_| {
            three_cells
                && third_is_empty
                && says_the_key
                && says_how_to_fill
                && erased
                && now_vacant
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// options.key-bindings.every-section-is-titled-in-words
// ---------------------------------------------------------------------------------------------

/// The headings are words, not the tokens that look the words up.
pub(super) fn every_section_is_titled_in_words() {
    let mut c = a_client_on_the_key_bindings("keybinding-titles");
    let headers = kb_screen(c.app_mut()).key_bindings.header_elements.clone();
    let enough = headers.len() >= 6;
    let titles: Vec<String> = headers
        .into_iter()
        .map(|h| kb_text(c.app_mut(), h))
        .collect();
    let all_words = titles
        .iter()
        .all(|t| !t.is_empty() && !t.starts_with("ID_"));
    let one_of_them = titles.iter().any(|t| t == "Movement");

    c.assert_behaviour(
        "options.key-bindings.every-section-is-titled-in-words",
        move |_| enough && all_words && one_of_them,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// options.key-bindings.undo-opens-greyed-because-nothing-has-changed-yet
// ---------------------------------------------------------------------------------------------

/// The undo button is dead when the page comes up.
pub(super) fn undo_opens_greyed_because_nothing_has_changed_yet() {
    let mut c = a_client_on_the_key_bindings("keybinding-revert");
    let nothing_changed = !kb_screen(c.app_mut()).key_bindings.changed();
    let undo = kb_screen(c.app_mut())
        .key_bindings
        .revert_to_saved_button
        .expect("undo is bound");
    let greyed = kb_state(c.app_mut(), undo) == KB_GREYED;

    c.assert_behaviour(
        "options.key-bindings.undo-opens-greyed-because-nothing-has-changed-yet",
        move |_| nothing_changed && greyed,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// options.key-bindings.a-press-on-a-cell-waits-for-a-key-and-undo-puts-the-old-one-back
// ---------------------------------------------------------------------------------------------

/// The whole gesture: press a cell, be told to press a key, cancel, do it again, and undo.
pub(super) fn a_press_on_a_cell_waits_for_a_key_and_undo_puts_the_old_one_back() {
    let mut c = a_client_on_the_key_bindings("keybinding-gesture");
    let mut hand = KeyHand::new();

    let before = keys_for_forward(c.app_mut());
    let starts_right = before.contains(&kb_control(KB_W)) && !before.contains(&kb_control(KB_F7));

    let cell = forward_cell(c.app_mut());
    kb_press(c.app_mut(), cell);
    let waiting = c
        .app_mut()
        .input_manager_mut()
        .expect("the input manager")
        .manager
        .key_hit_handler_registered();

    // The visible half: a box that says what to do, with the action's own name in it.
    let (context, dialog) = row_dialog(c.app_mut(), RowDialog::MapWarn);
    let shown = kb_ui(c.app_mut())
        .node(dialog)
        .is_some_and(|n| n.region.flags.visible);
    let prompt = kb_prompt(c.app_mut(), dialog);
    let says_what_to_do = prompt.starts_with("The next key you press")
        && prompt.contains("Move Forward")
        && prompt.contains("Press the ESC key to cancel");

    kb_tap(c.app_mut(), &mut hand, KeyCode::Escape);
    let cancelled = keys_for_forward(c.app_mut()) == before
        && !c
            .app_mut()
            .input_manager_mut()
            .expect("the input manager")
            .manager
            .key_hit_handler_registered()
        && kb_ui(c.app_mut()).dialogs.info(context).is_none()
        && kb_ui(c.app_mut()).node(dialog).is_none();

    kb_press(c.app_mut(), cell);
    let waiting_again = c
        .app_mut()
        .input_manager_mut()
        .expect("the input manager")
        .manager
        .key_hit_handler_registered();
    kb_tap(c.app_mut(), &mut hand, KeyCode::F7);
    c.tick(4);
    let bound = keys_for_forward(c.app_mut()).contains(&kb_control(KB_F7))
        && !c
            .app_mut()
            .input_manager_mut()
            .expect("the input manager")
            .manager
            .key_hit_handler_registered();

    let undo = kb_screen(c.app_mut())
        .key_bindings
        .revert_to_saved_button
        .expect("undo is bound");
    let lit = kb_screen(c.app_mut()).key_bindings.changed()
        && kb_state(c.app_mut(), undo) == KB_LIVE
        && kb_text(c.app_mut(), cell) == "F7";

    kb_press(c.app_mut(), undo);
    let restored = keys_for_forward(c.app_mut());
    let undone = restored.contains(&kb_control(KB_W))
        && !restored.contains(&kb_control(KB_F7))
        && !kb_screen(c.app_mut()).key_bindings.changed()
        && kb_state(c.app_mut(), undo) == KB_GREYED;

    c.assert_behaviour(
        "options.key-bindings.a-press-on-a-cell-waits-for-a-key-and-undo-puts-the-old-one-back",
        move |_| {
            starts_right
                && waiting
                && shown
                && says_what_to_do
                && cancelled
                && waiting_again
                && bound
                && lit
                && undone
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// options.key-bindings.a-key-already-in-use-asks-first-and-one-that-cannot-be-taken-refuses
// ---------------------------------------------------------------------------------------------

/// The two ways a captured key is not simply taken.
pub(super) fn a_key_already_in_use_asks_first_and_one_that_cannot_be_taken_refuses() {
    let mut c = a_client_on_the_key_bindings("keybinding-conflicts");
    let mut hand = KeyHand::new();
    let cell = forward_cell(c.app_mut());

    let the_conflict_is_shipped = c
        .app_mut()
        .input_manager_mut()
        .expect("the input manager")
        .manager
        .find_keys_for_action(KB_TURN_LEFT, KB_MOVEMENT)
        .contains(&kb_control(KB_A));

    // No: the question is really there, and answering no leaves both actions alone.
    kb_press(c.app_mut(), cell);
    kb_tap(c.app_mut(), &mut hand, KeyCode::KeyA);
    let (no_context, question) = row_dialog(c.app_mut(), RowDialog::Overwrite);
    let it_is_a_question = dereth_ui::dialog::types::dialog_element(kb_ui(c.app_mut()), question)
        .map(|d| d.kind)
        == Some(dereth_ui::dialog::DialogKind::Confirmation);
    let no = kb_child(
        c.app_mut(),
        question,
        dereth_ui::dialog::base::child::BUTTON2,
    );
    kb_press(c.app_mut(), no);
    let no_changed_nothing = kb_ui(c.app_mut()).dialogs.info(no_context).is_none()
        && kb_ui(c.app_mut()).node(question).is_none()
        && !keys_for_forward(c.app_mut()).contains(&kb_control(KB_A))
        && c.app_mut()
            .input_manager_mut()
            .expect("the input manager")
            .manager
            .find_keys_for_action(KB_TURN_LEFT, KB_MOVEMENT)
            .contains(&kb_control(KB_A));

    // Yes: the key moves, and the action that had it loses it.
    kb_press(c.app_mut(), cell);
    kb_tap(c.app_mut(), &mut hand, KeyCode::KeyA);
    let (yes_context, question) = row_dialog(c.app_mut(), RowDialog::Overwrite);
    let yes = kb_child(
        c.app_mut(),
        question,
        dereth_ui::dialog::base::child::BUTTON1,
    );
    kb_press(c.app_mut(), yes);
    let yes_moved_it = kb_ui(c.app_mut()).dialogs.info(yes_context).is_none()
        && kb_ui(c.app_mut()).node(question).is_none()
        && keys_for_forward(c.app_mut()).contains(&kb_control(KB_A))
        && !c
            .app_mut()
            .input_manager_mut()
            .expect("the input manager")
            .manager
            .find_keys_for_action(KB_TURN_LEFT, KB_MOVEMENT)
            .contains(&kb_control(KB_A));

    // The refusal. The shipped maps' one key a player may not take is the one that cancels the
    // capture itself, so a key that is free is put on that action here to make the refusal
    // reachable at all, and everything after it is the client's own.
    {
        let m = &mut c
            .app_mut()
            .input_manager_mut()
            .expect("the input manager")
            .manager;
        assert!(!m
            .action_map
            .is_user_bindable(KB_UI_COMMANDS, KB_ESCAPE_ACTION));
        assert!(m
            .find_conflicting_input_maps(KB_MOVEMENT)
            .contains(&KB_UI_COMMANDS));
        m.bind_action(kb_control(KB_F7), KB_ESCAPE_ACTION, KB_UI_COMMANDS);
    }
    let before_refusal = keys_for_forward(c.app_mut());
    kb_press(c.app_mut(), cell);
    kb_tap(c.app_mut(), &mut hand, KeyCode::F7);
    let (notice_context, notice) = row_dialog(c.app_mut(), RowDialog::CantOverwrite);
    let it_is_a_notice = dereth_ui::dialog::types::dialog_element(kb_ui(c.app_mut()), notice)
        .map(|d| d.kind)
        == Some(dereth_ui::dialog::DialogKind::Message);
    let dismiss = kb_child(
        c.app_mut(),
        notice,
        dereth_ui::dialog::base::child::MESSAGE_BUTTON,
    );
    kb_press(c.app_mut(), dismiss);
    let refused = kb_ui(c.app_mut()).dialogs.info(notice_context).is_none()
        && kb_ui(c.app_mut()).node(notice).is_none()
        && keys_for_forward(c.app_mut()) == before_refusal;

    c.assert_behaviour(
        "options.key-bindings.a-key-already-in-use-asks-first-and-one-that-cannot-be-taken-refuses",
        move |_| {
            the_conflict_is_shipped
                && it_is_a_question
                && no_changed_nothing
                && yes_moved_it
                && it_is_a_notice
                && refused
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// options.key-bindings.taking-a-key-clears-it-from-the-rows-that-had-it-on-the-screen
// ---------------------------------------------------------------------------------------------

/// Saying yes really does rebind, and the row that lost the key stops drawing it.
pub(super) fn taking_a_key_clears_it_from_the_rows_that_had_it() {
    let mut c = a_client_on_the_key_bindings("keybinding-refresh");
    let mut hand = KeyHand::new();
    let cell = forward_cell(c.app_mut());
    let a = kb_caption(c.app_mut(), KB_A);
    let one = kb_caption(c.app_mut(), KB_DIGIT_1);

    // One row loses it. Both sides of this are the shipped data.
    let before = drawn_cells(c.app_mut(), KB_MOVEMENT, KB_TURN_LEFT);
    let starts_there =
        before.contains(&a) && !drawn_cells(c.app_mut(), KB_MOVEMENT, KB_FORWARD).contains(&a);

    kb_press(c.app_mut(), cell);
    kb_tap(c.app_mut(), &mut hand, KeyCode::KeyA);
    let (context, question) = row_dialog(c.app_mut(), RowDialog::Overwrite);
    let yes = kb_child(
        c.app_mut(),
        question,
        dereth_ui::dialog::base::child::BUTTON1,
    );
    kb_press(c.app_mut(), yes);
    let took_it = kb_ui(c.app_mut()).dialogs.info(context).is_none()
        && keys_for_forward(c.app_mut()).contains(&kb_control(KB_A));
    let one_row_cleared = !drawn_cells(c.app_mut(), KB_MOVEMENT, KB_TURN_LEFT).contains(&a)
        && drawn_cells(c.app_mut(), KB_MOVEMENT, KB_FORWARD).contains(&a);

    // Two rows lose it at once, in two different sections -- also the shipped data: the key is on
    // a spell slot and on a quick slot, in two sets that each clash with movement but not with
    // one another.
    let both_are_shipped = {
        let m = &mut c
            .app_mut()
            .input_manager_mut()
            .expect("the input manager")
            .manager;
        let maps = m.find_conflicting_input_maps(KB_MOVEMENT);
        m.action_map.is_user_bindable(KB_SPELL_BAR, KB_SPELL_SLOT_1)
            && m.action_map
                .is_user_bindable(KB_QUICK_SLOTS, KB_QUICKSLOT_1)
            && maps.contains(&KB_SPELL_BAR)
            && maps.contains(&KB_QUICK_SLOTS)
    };
    let both_draw_it = drawn_cells(c.app_mut(), KB_SPELL_BAR, KB_SPELL_SLOT_1).contains(&one)
        && drawn_cells(c.app_mut(), KB_QUICK_SLOTS, KB_QUICKSLOT_1).contains(&one);

    kb_press(c.app_mut(), cell);
    kb_tap(c.app_mut(), &mut hand, KeyCode::Digit1);
    let (context, question) = row_dialog(c.app_mut(), RowDialog::Overwrite);
    let yes = kb_child(
        c.app_mut(),
        question,
        dereth_ui::dialog::base::child::BUTTON1,
    );
    kb_press(c.app_mut(), yes);
    let took_that_one = kb_ui(c.app_mut()).dialogs.info(context).is_none()
        && keys_for_forward(c.app_mut()).contains(&kb_control(KB_DIGIT_1));

    let quick = drawn_cells(c.app_mut(), KB_QUICK_SLOTS, KB_QUICKSLOT_1);
    let both_rows_cleared = !drawn_cells(c.app_mut(), KB_SPELL_BAR, KB_SPELL_SLOT_1).contains(&one)
        && !quick.contains(&one)
        && drawn_cells(c.app_mut(), KB_MOVEMENT, KB_FORWARD).contains(&one);
    // ...and the quick slot's *modified* key on the same letter is untouched, because a key with
    // a modifier held is a different key and was never in the way.
    let the_modified_one_survives = quick
        .iter()
        .any(|cell| cell.ends_with(&one) && cell != &one);

    c.assert_behaviour(
        "options.key-bindings.taking-a-key-clears-it-from-the-rows-that-had-it-on-the-screen",
        move |_| {
            starts_there
                && took_it
                && one_row_cleared
                && both_are_shipped
                && both_draw_it
                && took_that_one
                && both_rows_cleared
                && the_modified_one_survives
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// options.key-bindings.the-questions-about-a-key-in-use-are-the-shipped-sentences
// ---------------------------------------------------------------------------------------------

/// The three prompts, word for word, with the key and the action in them.
pub(super) fn the_questions_about_a_key_in_use_are_the_shipped_sentences() {
    let mut c = a_client_on_the_key_bindings("keybinding-conflict-text");
    let mut hand = KeyHand::new();
    let cell = forward_cell(c.app_mut());

    // One clash, on the shipped data.
    kb_press(c.app_mut(), cell);
    kb_tap(c.app_mut(), &mut hand, KeyCode::KeyA);
    let (one_context, question) = row_dialog(c.app_mut(), RowDialog::Overwrite);
    let one_clash = kb_prompt(c.app_mut(), question)
        == "'A' is currently bound to 'Turn Left'. Do you wish to erase that binding?";
    let no = kb_child(
        c.app_mut(),
        question,
        dereth_ui::dialog::base::child::BUTTON2,
    );
    kb_press(c.app_mut(), no);
    let no_answered = kb_ui(c.app_mut()).dialogs.info(one_context).is_none()
        && !keys_for_forward(c.app_mut()).contains(&kb_control(KB_A));

    // Several clashes. A key no shipped map uses is put on two actions that really are bindable
    // and really do clash with each other, so the several-clash sentence is reachable.
    let order: Vec<InputMapId> = {
        let m = &mut c
            .app_mut()
            .input_manager_mut()
            .expect("the input manager")
            .manager;
        assert!(m.action_map.is_user_bindable(KB_MOVEMENT, KB_TURN_LEFT));
        assert!(m
            .action_map
            .is_user_bindable(KB_UI_COMMANDS, KB_USE_SELECTED));
        m.bind_action(kb_control(KB_F7), KB_TURN_LEFT, KB_MOVEMENT);
        m.bind_action(kb_control(KB_F7), KB_USE_SELECTED, KB_UI_COMMANDS);
        // The order the client walks them in is the order the lines come out in, read here rather
        // than assumed.
        m.find_conflicting_input_maps(KB_MOVEMENT).to_vec()
    };
    let movement_first = order.iter().position(|m| *m == KB_MOVEMENT)
        < order.iter().position(|m| *m == KB_UI_COMMANDS);
    let turn_left = "'Turn Left' ('F7')\n";
    let use_selected = "'Use Selected Object' ('F7')\n";
    let list = if movement_first {
        format!("{turn_left}{use_selected}")
    } else {
        format!("{use_selected}{turn_left}")
    };
    kb_press(c.app_mut(), cell);
    kb_tap(c.app_mut(), &mut hand, KeyCode::F7);
    let (many_context, question) = row_dialog(c.app_mut(), RowDialog::Overwrite);
    let several_clashes = kb_prompt(c.app_mut(), question)
        == format!(
            "'F7' conflicts with the following bindings:\n{list}\nDo you wish to erase those \
             bindings?"
        );
    let no = kb_child(
        c.app_mut(),
        question,
        dereth_ui::dialog::base::child::BUTTON2,
    );
    kb_press(c.app_mut(), no);
    let still_free = kb_ui(c.app_mut()).dialogs.info(many_context).is_none()
        && !keys_for_forward(c.app_mut()).contains(&kb_control(KB_F7));

    // And the refusal, which names the key and not the row it was pressed on.
    {
        let m = &mut c
            .app_mut()
            .input_manager_mut()
            .expect("the input manager")
            .manager;
        assert!(!m
            .action_map
            .is_user_bindable(KB_UI_COMMANDS, KB_ESCAPE_ACTION));
        m.bind_action(kb_control(KB_F7), KB_ESCAPE_ACTION, KB_UI_COMMANDS);
    }
    let before_refusal = keys_for_forward(c.app_mut());
    kb_press(c.app_mut(), cell);
    kb_tap(c.app_mut(), &mut hand, KeyCode::F7);
    let (notice_context, notice) = row_dialog(c.app_mut(), RowDialog::CantOverwrite);
    let refusal = kb_prompt(c.app_mut(), notice)
        == "'F7' is currently bound to a non user-bindable action. Please select a different \
            binding.";
    let dismiss = kb_child(
        c.app_mut(),
        notice,
        dereth_ui::dialog::base::child::MESSAGE_BUTTON,
    );
    kb_press(c.app_mut(), dismiss);
    let dismissed = kb_ui(c.app_mut()).dialogs.info(notice_context).is_none()
        && keys_for_forward(c.app_mut()) == before_refusal;

    c.assert_behaviour("options.key-bindings.the-questions-about-a-key-in-use-are-the-shipped-sentences-with-the-key-and-the-action-in-them", move |_| {
        one_clash && no_answered && several_clashes && still_free && refusal && dismissed
    });
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// options.key-bindings.a-set-of-keys-can-be-saved-under-a-name-and-loaded-back
// ---------------------------------------------------------------------------------------------

/// Save, change, load: the loaded keys come back and the page redraws them.
pub(super) fn a_set_of_keys_can_be_saved_under_a_name_and_loaded_back() {
    let mut c = a_client_on_the_key_bindings("keybinding-files");
    let dir = c
        .scratch_settings()
        .expect("a settings directory")
        .dir()
        .to_path_buf();
    let saved = dir.join("keys-roundtrip-modern.keymap");
    let mut hand = KeyHand::new();
    let cell = forward_cell(c.app_mut());

    // Something recognisable to save.
    kb_press(c.app_mut(), cell);
    kb_tap(c.app_mut(), &mut hand, KeyCode::F7);
    let changed_first = keys_for_forward(c.app_mut()).contains(&kb_control(KB_F7));

    submit_save_name(c.app_mut(), "keys-roundtrip");
    let written = saved.exists();

    // Change it again, so loading the file has something to undo.
    kb_press(c.app_mut(), cell);
    kb_tap(c.app_mut(), &mut hand, KeyCode::KeyA);
    let (_, question) = row_dialog(c.app_mut(), RowDialog::Overwrite);
    let yes = kb_child(
        c.app_mut(),
        question,
        dereth_ui::dialog::base::child::BUTTON1,
    );
    kb_press(c.app_mut(), yes);
    let now_different = {
        let keys = keys_for_forward(c.app_mut());
        keys.contains(&kb_control(KB_A)) && !keys.contains(&kb_control(KB_F7))
    };

    let load = kb_screen(c.app_mut())
        .key_bindings
        .load_button
        .expect("the load button is bound");
    kb_press(c.app_mut(), load);
    let (load_context, load_dialog) =
        page_dialog(c.app_mut(), dereth_ui::dialog::DialogKind::ConfirmationMenu);
    let menu = kb_child(
        c.app_mut(),
        load_dialog,
        dereth_ui::dialog::base::child::CONFIRM_MENU_MENU,
    );
    // "Default" first, then this interface's saved key maps by the names they were saved
    // under.
    let default_first = dereth_ui::widgets::menu::get_item(kb_ui(c.app_mut()), menu, 0)
        .is_some_and(|row| kb_text(c.app_mut(), row) == "Default");
    let row = dereth_ui::widgets::menu::get_item(kb_ui(c.app_mut()), menu, 1)
        .expect("the saved file is a real row of the list");
    let listed = default_first && kb_text(c.app_mut(), row) == "keys-roundtrip";
    kb_press(c.app_mut(), menu);
    kb_press(c.app_mut(), row);
    let accept = kb_child(
        c.app_mut(),
        load_dialog,
        dereth_ui::dialog::base::child::CONFIRM_MENU_ACCEPT,
    );
    kb_press(c.app_mut(), accept);
    let closed = kb_ui(c.app_mut()).dialogs.info(load_context).is_none();

    let loaded = keys_for_forward(c.app_mut());
    let came_back = loaded.contains(&kb_control(KB_F7)) && !loaded.contains(&kb_control(KB_A));
    let cell = forward_cell(c.app_mut());
    let redrawn = kb_text(c.app_mut(), cell) == "F7";

    c.assert_behaviour(
        "options.key-bindings.a-set-of-keys-can-be-saved-under-a-name-and-loaded-back",
        move |_| {
            changed_first && written && now_different && listed && closed && came_back && redrawn
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// options.key-bindings.saving-over-a-set-asks-first-and-one-that-cannot-be-written-refuses
// ---------------------------------------------------------------------------------------------

/// Saving over a file that exists asks, and a file that cannot be written says so.
pub(super) fn saving_over_a_set_asks_first_and_one_that_cannot_be_written_refuses() {
    let mut c = a_client_on_the_key_bindings("keybinding-overwrite");
    let dir = c
        .scratch_settings()
        .expect("a settings directory")
        .dir()
        .to_path_buf();
    let writable = dir.join("existing-modern.keymap");
    let read_only = dir.join("read-only-modern.keymap");
    let sentinel = b"existing bytes must survive no";
    let read_only_sentinel = b"read-only bytes must not change";
    std::fs::write(&writable, sentinel).expect("a disposable target");
    std::fs::write(&read_only, read_only_sentinel).expect("a disposable target");
    let permissions = std::fs::metadata(&read_only)
        .expect("its metadata")
        .permissions();
    {
        let mut ro = permissions.clone();
        ro.set_readonly(true);
        std::fs::set_permissions(&read_only, ro).expect("mark the disposable target read-only");
    }

    let mut hand = KeyHand::new();
    let cell = forward_cell(c.app_mut());
    kb_press(c.app_mut(), cell);
    kb_tap(c.app_mut(), &mut hand, KeyCode::F7);
    let something_to_save = keys_for_forward(c.app_mut()).contains(&kb_control(KB_F7));

    // A file that exists: no leaves its bytes exactly alone.
    submit_save_name(c.app_mut(), "existing");
    let (no_context, question) =
        page_dialog(c.app_mut(), dereth_ui::dialog::DialogKind::Confirmation);
    let no = kb_child(
        c.app_mut(),
        question,
        dereth_ui::dialog::base::child::BUTTON2,
    );
    kb_press(c.app_mut(), no);
    let untouched = kb_ui(c.app_mut()).dialogs.info(no_context).is_none()
        && std::fs::read(&writable).expect("the target remains") == sentinel;

    // The same name and yes replace it with the keys that are live now.
    submit_save_name(c.app_mut(), "existing");
    let (yes_context, question) =
        page_dialog(c.app_mut(), dereth_ui::dialog::DialogKind::Confirmation);
    let yes = kb_child(
        c.app_mut(),
        question,
        dereth_ui::dialog::base::child::BUTTON1,
    );
    kb_press(c.app_mut(), yes);
    let written = std::fs::read_to_string(&writable).expect("yes wrote a file");
    let replaced = kb_ui(c.app_mut()).dialogs.info(yes_context).is_none()
        && written.as_bytes() != sentinel
        && dereth_input::MasterInputMap::from_keymap_text(&written)
            .expect("the file is a set of keys")
            .section(KB_MOVEMENT)
            .expect("the movement section")
            .bindings()
            .iter()
            .any(|(k, a)| *k == kb_control(KB_F7) && *a == KB_FORWARD);

    // One that cannot be written never asks: it says so, in a box with one button, and the bytes
    // do not change.
    submit_save_name(c.app_mut(), "read-only");
    let (notice_context, notice) = page_dialog(c.app_mut(), dereth_ui::dialog::DialogKind::Message);
    let dismiss = kb_child(
        c.app_mut(),
        notice,
        dereth_ui::dialog::base::child::MESSAGE_BUTTON,
    );
    kb_press(c.app_mut(), dismiss);
    let refused = kb_ui(c.app_mut()).dialogs.info(notice_context).is_none()
        && std::fs::read(&read_only).expect("the target remains") == read_only_sentinel;

    // Put the attribute back, so the directory can be removed with the client.
    let _ = std::fs::set_permissions(&read_only, permissions);

    c.assert_behaviour(
        "options.key-bindings.saving-over-a-set-asks-first-and-one-that-cannot-be-written-refuses",
        move |_| something_to_save && untouched && replaced && refused,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// options.key-bindings.restoring-the-defaults-gives-back-the-shipped-keys-and-not-the-saved-ones
// ---------------------------------------------------------------------------------------------

/// Two whole client lifetimes against one settings directory: the first rebinds and exits, which
/// writes the file, and the second starts with that file already merged in -- which is the state
/// in which "restore the defaults" could restore the rebind instead of the shipped key.
///
/// **It owns its clients outright**, through `adapters_shell::AppSpec` / `build_app`: the harness
/// removes a scenario's settings directory when the client is dropped, which is right for every
/// other scenario and is exactly what this one cannot have.
pub(super) fn restoring_the_defaults_gives_back_the_shipped_keys() {
    use dereth_testkit::adapters_shell::{build_app, scratch_preferences, AppSpec};

    let prefs = scratch_preferences("keybinding-defaults");
    let dir = prefs.parent().expect("a directory").to_path_buf();
    // A fresh start: a previous run's file would be this run's precondition.
    for name in ["UserPreferences.keymap", "UserPreferences.ini"] {
        let _ = std::fs::remove_file(dir.join(name));
    }
    let spec = AppSpec {
        preferences_file: Some(prefs.clone()),
        static_scene: true,
        ..AppSpec::in_gameplay(6)
    };

    // Life one: rebind, and let the exit write the file.
    let wrote_the_file = {
        let mut app = build_app(&spec);
        let mut hand = KeyHand::new();
        let cell = forward_cell(&mut app);
        kb_press(&mut app, cell);
        kb_tap(&mut app, &mut hand, KeyCode::F7);
        for _ in 0..4 {
            app.frame();
        }
        let took = keys_for_forward(&mut app).contains(&kb_control(KB_F7));
        let path = app
            .input_manager_mut()
            .expect("the input manager")
            .keymap_path()
            .map(std::path::Path::to_path_buf);
        let _ = app.shutdown();
        let path = path.expect("the client keeps its keys beside its preferences");
        took && path.exists()
    };

    // Life two: the page is built with the rebind already merged in.
    let mut app = build_app(&spec);
    let loaded = keys_for_forward(&mut app).contains(&kb_control(KB_F7));

    let i = {
        let s = kb_screen(&mut app);
        s.key_bindings
            .row_of(KB_MOVEMENT, KB_FORWARD)
            .expect("walking forward has a row")
    };
    let (defaults, current) = {
        let r = &kb_screen(&mut app).key_bindings.rows[i];
        (r.defaults.clone(), r.current.clone())
    };
    // What the row is showing is the merged set, which has the rebind; what "the defaults" means
    // is the shipped set, which does not.
    let the_two_differ = current.contains(&kb_control(KB_F7))
        && defaults.contains(&kb_control(KB_W))
        && !defaults.contains(&kb_control(KB_F7));

    let reset = kb_screen(&mut app)
        .key_bindings
        .reset_defaults_button
        .expect("the reset button is bound");
    kb_press(&mut app, reset);
    let restored = keys_for_forward(&mut app);
    let shipped_came_back =
        restored.contains(&kb_control(KB_W)) && !restored.contains(&kb_control(KB_F7));
    let cell = forward_cell(&mut app);
    let redrawn = kb_text(&mut app, cell) == "W";
    let _ = app.shutdown();
    let _ = std::fs::remove_dir_all(&dir);

    let mut c = HeadlessClient::model();
    c.assert_behaviour("options.key-bindings.restoring-the-defaults-gives-back-the-shipped-keys-and-not-the-saved-ones", move |_| {
        wrote_the_file && loaded && the_two_differ && shipped_came_back && redrawn
    });
}

impl KeyHand {
    /// One press of the button at a point, with the frames the gesture needs.
    pub(super) fn click_at(&mut self, app: &mut dereth_client::app::App, at: (i32, i32)) {
        use dereth_input::keys::MouseButton;
        self.time_ms += 10;
        let m = self
            .pump
            .mouse_move_message(f64::from(at.0), f64::from(at.1), self.time_ms);
        self.send(app, m);
        for pressed in [true, false] {
            self.time_ms += 10;
            let m = self
                .pump
                .button_message(MouseButton::Left, pressed, self.time_ms)
                .expect("the left button is one of the client's own messages");
            self.send(app, m);
        }
        app.frame();
        app.frame();
    }

    pub(super) fn send(
        &mut self,
        app: &mut dereth_client::app::App,
        m: dereth_input::win32::Win32Message,
    ) {
        self.pump.dispatch(m);
        if let Some(input) = app.input_manager_mut() {
            input.on_message(m);
        }
    }

    /// One typed code unit per call, unsigned, exactly as the message carries it -- which is what
    /// an IME's committed string arrives as too.
    pub(super) fn type_units(&mut self, app: &mut dereth_client::app::App, units: &[u16]) {
        for u in units {
            self.time_ms += 10;
            let m = dereth_input::win32::Win32Message::new(
                win_msg::WM_CHAR,
                *u as usize,
                0,
                self.time_ms,
            );
            self.send(app, m);
        }
        app.frame();
    }

    pub(super) fn type_text(&mut self, app: &mut dereth_client::app::App, s: &str) {
        let units: Vec<u16> = s.encode_utf16().collect();
        self.type_units(app, &units);
    }

    /// One key edge with **no frame after it**, for a scenario that wants the harness to run the
    /// frame -- which is the only way what the client asked for on that frame is recorded.
    pub(super) fn key_quiet(
        &mut self,
        app: &mut dereth_client::app::App,
        code: KeyCode,
        down: bool,
    ) {
        self.time_ms += 10;
        let m = self
            .pump
            .key_message_for(code, down, self.time_ms)
            .expect("the host names this key");
        self.send(app, m);
    }

    /// A key down together with the text the desktop says that key produced with the modifiers
    /// that are held -- which is not the same as the letter on the key.
    pub(super) fn key_with_text(
        &mut self,
        app: &mut dereth_client::app::App,
        code: KeyCode,
        text: Option<&str>,
        alt_down: bool,
    ) {
        self.key(app, code, true);
        self.time_ms += 10;
        for m in key_text_messages(true, alt_down, text, self.time_ms) {
            self.send(app, m);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// keys.own.each-of-this-clients-actions-works-on-a-key-the-page-gives-it
// ---------------------------------------------------------------------------------------------

/// The row of one of this client's own actions, its first key cell.
fn own_cell(app: &mut dereth_client::app::App, action: u32) -> dereth_ui::ElemHandle {
    let s = kb_screen(app);
    let i = s
        .key_bindings
        .row_of(dereth_input::dereth::INPUT_MAP, ActionId(action))
        .expect("this client's action has a row");
    s.key_bindings.rows[i].key_buttons[0]
}

fn pref_on(name: &str) -> bool {
    matches!(
        dereth_client_contract::options::store::inq_value(name),
        Some(dereth_client_contract::PrefValue::Bool(true))
    )
}

/// Each of this client's own actions the retail interface answers, given a free key on the key
/// page as a player gives it one, answers that key: the performance panel, the inverted mouse
/// look and mute-when-inactive flip their settings, the trade key shows the trade window, and
/// hold sidestep is held for as long as its key is.
pub(super) fn each_of_this_clients_actions_works_on_a_key_the_page_gives_it() {
    use dereth_client_contract::actions::dereth as own;
    let mut c = a_client_for_the_key_bindings_page();
    let mut hands = Hands::new();
    // Keys no shipped map binds, one for each action.
    let free = [
        (own::TOGGLE_PERFORMANCE_PANEL.0, KeyCode::F7),
        (own::TOGGLE_INVERT_MOUSE_LOOK.0, KeyCode::KeyV),
        (own::TOGGLE_MUTE_ON_LOSING_FOCUS.0, KeyCode::ScrollLock),
        (own::TOGGLE_TRADE_PANEL.0, KeyCode::Numpad7),
        (own::MOVEMENT_HOLD_SIDESTEP.0, KeyCode::Numpad9),
    ];
    let mut bound = Vec::new();
    for (action, code) in free {
        let cell = own_cell(c.app_mut(), action);
        click_element(&mut c, cell);
        hands.tap(&mut c, key(code));
        c.tick(2);
        let keys = c
            .app_mut()
            .input_manager_mut()
            .expect("the input shell")
            .keys_for_action(ActionId(action), dereth_input::dereth::INPUT_MAP);
        bound.push((action, keys.len()));
    }
    let all_bound = bound.iter().all(|(_, n)| *n == 1);

    let flips = |c: &mut HeadlessClient, hands: &mut Hands, code, name: &str| {
        let was = pref_on(name);
        hands.tap(c, key(code));
        c.tick(3);
        pref_on(name) != was
    };
    let perf = flips(
        &mut c,
        &mut hands,
        KeyCode::F7,
        dereth_client_contract::options::performance::PERFORMANCE_PANEL,
    );
    let invert = flips(
        &mut c,
        &mut hands,
        KeyCode::KeyV,
        "Input.InvertMouseLookYAxis",
    );
    let mute = flips(
        &mut c,
        &mut hands,
        KeyCode::ScrollLock,
        "Sound.PlaySoundOnlyWhenActive",
    );
    let trade_window = {
        let shell = c.app_mut().ui_mut().expect("the shell");
        let root = shell
            .flow
            .current()
            .and_then(|s| s.roots().first().copied())
            .expect("the gameplay root");
        shell
            .ui
            .get_child_recursive(root, dereth_ui_screens::panels::trade::WINDOW)
            .expect("the trade window")
    };
    let trade_shown = |c: &mut HeadlessClient| {
        c.app_mut()
            .ui_mut()
            .and_then(|s| s.ui.node(trade_window))
            .is_some_and(|n| n.region.flags.visible)
    };
    let trade_before = trade_shown(&mut c);
    hands.tap(&mut c, key(KeyCode::Numpad7));
    c.tick(3);
    let trade = trade_shown(&mut c) != trade_before;
    hands.key(&mut c, key(KeyCode::Numpad9), true);
    c.tick(3);
    let held = c.app_mut().movement.lists.hold_sidestep;
    hands.key(&mut c, key(KeyCode::Numpad9), false);
    c.tick(3);
    let let_go = !c.app_mut().movement.lists.hold_sidestep;

    c.assert_behaviour(
        "keys.own.each-of-this-clients-actions-works-on-a-key-the-page-gives-it",
        move |_| {
            eprintln!(
                "bound {bound:?} perf {perf} invert {invert} mute {mute} trade {trade} held {held} let go {let_go}"
            );
            all_bound && perf && invert && mute && trade && held && let_go
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// keys.own.a-key-in-use-given-to-this-clients-action-asks-first-and-is-taken
// ---------------------------------------------------------------------------------------------

/// A key another action has, given to one of this client's own actions, raises the same question
/// a shipped action's row raises; yes takes the key from the other action, and the key then
/// answers this client's action.
pub(super) fn a_key_in_use_given_to_this_clients_action_asks_first_and_is_taken() {
    use dereth_client_contract::actions::dereth as own;
    const PICK_UP: ActionId = ActionId(0x1000_002C);
    const ITEMS: InputMapId = InputMapId(0x1000_0007);
    const KB_F: u16 = 0x21;
    let mut c = a_client_on_the_key_bindings("own-conflict");
    let mut hand = KeyHand::new();
    let perf = own::TOGGLE_PERFORMANCE_PANEL;
    let picks_up_on_f = |c: &mut dereth_client::app::App| {
        c.input_manager_mut()
            .expect("the input manager")
            .keys_for_action(PICK_UP, ITEMS)
            .iter()
            .any(|k| k.control.offset() == KB_F && k.meta_mode == 0)
    };
    let f_picked_up_first = picks_up_on_f(c.app_mut());
    let cell = {
        let s = kb_screen(c.app_mut());
        let i = s
            .key_bindings
            .row_of(dereth_input::dereth::INPUT_MAP, perf)
            .expect("the performance panel has a row");
        s.key_bindings.rows[i].key_buttons[0]
    };
    // The row is down the interface tab's list, out of the list's view: its press is the
    // element message a click raises.
    click_element(&mut c, cell);
    kb_tap(c.app_mut(), &mut hand, KeyCode::KeyF);
    let context = {
        let s = kb_screen(c.app_mut());
        let i = s
            .key_bindings
            .row_of(dereth_input::dereth::INPUT_MAP, perf)
            .expect("the row");
        s.key_bindings.rows[i].dialog_context(RowDialog::Overwrite)
    };
    let asked = context.is_some();
    let not_yet = !c
        .app_mut()
        .input_manager_mut()
        .expect("the input manager")
        .keys_for_action(perf, dereth_input::dereth::INPUT_MAP)
        .iter()
        .any(|k| k.control.offset() == KB_F);
    if let Some(context) = context {
        let question = kb_ui(c.app_mut())
            .dialogs
            .info(context)
            .and_then(|info| info.element)
            .expect("the question is drawn");
        let yes = kb_child(
            c.app_mut(),
            question,
            dereth_ui::dialog::base::child::BUTTON1,
        );
        kb_press(c.app_mut(), yes);
    }
    let taken = c
        .app_mut()
        .input_manager_mut()
        .expect("the input manager")
        .keys_for_action(perf, dereth_input::dereth::INPUT_MAP)
        .iter()
        .any(|k| k.control.offset() == KB_F)
        && !picks_up_on_f(c.app_mut());
    let name = dereth_client_contract::options::performance::PERFORMANCE_PANEL;
    let on = |n: &str| {
        matches!(
            dereth_client_contract::options::store::inq_value(n),
            Some(dereth_client_contract::PrefValue::Bool(true))
        )
    };
    let was = on(name);
    kb_tap(c.app_mut(), &mut hand, KeyCode::KeyF);
    c.app_mut().frame();
    let answers = on(name) != was;

    c.assert_behaviour(
        "keys.own.a-key-in-use-given-to-this-clients-action-asks-first-and-is-taken",
        move |_| {
            eprintln!(
                "first {f_picked_up_first} asked {asked} not yet {not_yet} taken {taken} answers {answers}"
            );
            f_picked_up_first && asked && not_yet && taken && answers
        },
    );
    c.shutdown();
}
