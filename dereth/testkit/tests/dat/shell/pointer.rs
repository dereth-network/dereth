//! Shell fixtures and scenarios for pointer.

use super::*;
// ---------------------------------------------------------------------------------------------
// pointer.wheel.*
//
// The wheel scrolls scrollbars and must not also flip radios or tick boxes. The harness does not
// pack the wheel message itself -- `Hands::wheel` asks the client's own mapping for it -- so the
// harness and the client cannot disagree about it.
//
// **The row the gesture is aimed at is chosen for its value, and that is the instrument.** A tick
// box that is *off* can be flipped on and flipped straight back within the frame without either
// write being visible at the end of it; a box that starts *on* has no such eraser. So a scenario
// built on a ticked row can go red and one built on an unticked row cannot.
// ---------------------------------------------------------------------------------------------

/// The character options list, and a ticked row whose box the pointer really lands on.
fn the_list_and_a_ticked_row(
    c: &mut HeadlessClient,
) -> (dereth_ui::ElemHandle, dereth_ui::ElemHandle) {
    let (list, ticked): (dereth_ui::ElemHandle, Vec<usize>) = with_gameplay(c, |_, s| {
        let list = s
            .character_options
            .option_box
            .as_ref()
            .expect("the option list")
            .handle;
        let ticked = s
            .character_options
            .rows
            .iter()
            .enumerate()
            .filter(|(_, r)| r.current)
            .map(|(i, _)| i)
            .collect();
        (list, ticked)
    });
    assert!(
        !ticked.is_empty(),
        "this character has some setting on; without one there is no instrument"
    );
    for i in ticked {
        // Bring the row into the pane first, as any player would have to before the pointer could
        // be over it.
        with_gameplay(c, |ui, s| {
            let row = s.character_options.rows[i].row;
            let b = s
                .character_options
                .option_box
                .as_mut()
                .expect("the option list");
            if let Some(idx) = b.items.iter().position(|h| *h == row) {
                b.scroll_to_view(ui, idx);
            }
        });
        c.tick(1);
        let h = with_gameplay(c, |_, s| s.character_options.rows[i].element);
        let app = c.view().expect_app();
        let ui = &app.ui().expect("the UI shell is up").ui;
        let b = ui.screen_clip_box(h);
        if !b.is_valid() || !ui.is_visible(h) {
            continue;
        }
        if ui.hit_test_screen((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2) == Some(h) {
            return (list, h);
        }
    }
    panic!("no ticked tick box hit-tests to itself");
}

/// How far the option list has been scrolled.
fn list_scroll(c: &mut HeadlessClient, list: dereth_ui::ElemHandle) -> i32 {
    with_gameplay(c, |ui, _| {
        ui.node(list)
            .and_then(|n| {
                n.behaviour
                    .as_ref()?
                    .as_any()?
                    .downcast_ref::<dereth_ui::widgets::listbox::ListBox>()
            })
            .expect("the option list is a list box")
            .scroll
            .y
    })
}

/// A wheel detent over a tick box scrolls the list and leaves the box alone.
pub(super) fn a_detent_over_a_tick_box_scrolls_the_list() {
    let mut c = a_client_with_settings();
    let mut hands = Hands::new();
    open_the_character_options_page(&mut c, &mut hands);
    let (list, row) = the_list_and_a_ticked_row(&mut c);

    let before_value = drawn_tick(&c, row);
    let before_y = list_scroll(&mut c, list);

    let (x, y) = {
        let app = c.view().expect_app();
        let b = app
            .ui()
            .expect("the UI shell is up")
            .ui
            .screen_clip_box(row);
        ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
    };
    hands.move_to(&mut c, x, y);
    hands.wheel(&mut c, -1.0);
    c.tick(2);

    let after_y = list_scroll(&mut c, list);
    let after_value = drawn_tick(&c, row);

    c.assert_behaviour(
        "pointer.wheel.a-detent-over-a-check-box-scrolls-the-list-and-leaves-the-box-alone",
        move |_| before_value && after_y > before_y && after_value == before_value,
    );
    c.shutdown();
}

/// The same box still answers a click, and so does the list's own bar.
pub(super) fn the_same_box_still_answers_a_click_and_so_does_the_bar() {
    let mut c = a_client_with_settings();
    let mut hands = Hands::new();
    open_the_character_options_page(&mut c, &mut hands);
    let (list, row) = the_list_and_a_ticked_row(&mut c);

    let before = drawn_tick(&c, row);
    hands.click_handle(&mut c, row);
    let flipped = !drawn_tick(&c, row);

    // The other half of the blast radius: the list's own scrollbar is built out of the same kind
    // of widget, so a refusal written in the wrong place would make every bar in the client inert.
    let bar = with_gameplay(&mut c, |ui, _| {
        let s = ui
            .node(list)
            .and_then(|n| {
                n.behaviour
                    .as_ref()?
                    .as_any()?
                    .downcast_ref::<dereth_ui::widgets::listbox::ListBox>()
            })
            .expect("the option list is a list box")
            .scroll
            .clone();
        s.scrollbar(ui, list, false)
            .expect("the option list names a vertical bar")
    });
    let before_y = list_scroll(&mut c, list);
    let (x, y) = {
        let app = c.view().expect_app();
        let b = app
            .ui()
            .expect("the UI shell is up")
            .ui
            .screen_clip_box(bar);
        assert!(b.is_valid(), "the bar is drawn");
        ((b.x0 + b.x1) / 2, b.y1 - 2)
    };
    hands.click_at(&mut c, x, y);
    let after_y = list_scroll(&mut c, list);

    c.assert_behaviour(
        "pointer.wheel.the-same-box-still-answers-a-click-and-so-does-the-bar",
        move |_| before && flipped && after_y > before_y,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// pointer.a-button-lights-under-the-pointer-and-sinks-under-the-press
// ---------------------------------------------------------------------------------------------

/// Four looks in order, and the third is the one a single-look scenario cannot see: still held,
/// pointer gone, and the button is lit rather than sunk.
pub(super) fn a_button_lights_under_the_pointer_and_sinks_under_the_press() {
    use dereth_input::keys::MouseButton;

    let mut c = a_client_on_character_select();
    let exit = element(&c, LIST_EXIT);
    let look = |c: &HeadlessClient| {
        c.view()
            .expect_app()
            .ui()
            .expect("the UI shell is up")
            .ui
            .node(exit)
            .expect("alive")
            .state
    };
    // The shipped button asks to light up at all, which is what makes the lit look reachable.
    let it_asks_to_light = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(exit)
        .expect("alive")
        .merged_properties()
        .get_bool(dereth_ui::props::attr::ROLLOVER_HIGHLIGHT)
        == Some(true);
    let resting = look(&c) == dereth_ui::widgets::button::state::NORMAL;

    let (x, y) = middle_of(&c, exit);
    let mut hands = Hands::new();
    // Somewhere else first, so moving onto it is a real change.
    hands.move_to(&mut c, 4, 4);
    c.tick(1);
    hands.move_to(&mut c, x, y);
    c.tick(1);
    let lit = look(&c) == dereth_ui::widgets::button::state::ROLLOVER;

    let m = hands.button_message(MouseButton::Left, true);
    hands.send(&mut c, m);
    c.tick(1);
    let sunk = look(&c) == dereth_ui::widgets::button::state::PRESSED;

    // Still held, pointer off it.
    hands.move_to(&mut c, 4, 4);
    c.tick(1);
    let lit_again = look(&c) == dereth_ui::widgets::button::state::ROLLOVER;

    let m = hands.button_message(MouseButton::Left, false);
    hands.send(&mut c, m);
    c.tick(1);
    let resting_again = look(&c) == dereth_ui::widgets::button::state::NORMAL;

    c.assert_behaviour(
        "pointer.a-button-lights-under-the-pointer-and-sinks-under-the-press",
        move |_| it_asks_to_light && resting && lit && sunk && lit_again && resting_again,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// pointer.a-press-dragged-off-a-button-releases-it-without-firing-it
// ---------------------------------------------------------------------------------------------

/// The press and the release both happen -- which is what stops a client that simply lost the
/// gesture from passing -- and what the button does is not done.
pub(super) fn a_press_dragged_off_a_button_releases_it_without_firing_it() {
    use dereth_input::keys::MouseButton;

    let mut c = a_client_on_character_select();
    let credits = element(&c, LIST_CREDITS);
    let (x, y) = middle_of(&c, credits);
    let mut hands = Hands::new();

    hands.move_to(&mut c, x, y);
    let m = hands.button_message(MouseButton::Left, true);
    hands.send(&mut c, m);
    c.tick(1);

    // Off the button, and then let go. The button still holds the pointer, so the release does
    // reach it -- and goes no further.
    hands.move_to(&mut c, 4, 4);
    let m = hands.button_message(MouseButton::Left, false);
    hands.send(&mut c, m);
    c.tick(2);

    let it_did_not_fire = current_screen(&c) == Some(mode::CHARACTER_MANAGEMENT);
    let stats = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .stats;
    let both_halves_happened = stats.mouse_downs == 1 && stats.mouse_ups == 1;

    c.assert_behaviour(
        "pointer.a-press-dragged-off-a-button-releases-it-without-firing-it",
        move |_| it_did_not_fire && both_halves_happened,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// pointer.a-move-makes-what-is-under-the-pointer-the-one-entered-until-it-leaves
// ---------------------------------------------------------------------------------------------

/// This is the claim that fails first if the route from the window to the tree is cut, and it
/// leans on no screen's handler at all.
pub(super) fn a_move_makes_what_is_under_the_pointer_the_one_entered() {
    let mut c = a_client_on_character_select();
    let create = element(&c, LIST_CREATE);
    let under = |c: &HeadlessClient| {
        c.view()
            .expect_app()
            .ui()
            .expect("the UI shell is up")
            .ui
            .mouse_over()
    };
    let nothing_at_first = under(&c).is_none();

    let (x, y) = middle_of(&c, create);
    let mut hands = Hands::new();
    hands.move_to(&mut c, x, y);
    c.tick(1);
    let now_the_button = under(&c) == Some(create);

    // ...and the pointer leaving the window clears it, which is the one thing that does.
    hands.leave(&mut c);
    c.tick(1);
    let cleared = under(&c).is_none();

    c.assert_behaviour(
        "pointer.a-move-makes-what-is-under-the-pointer-the-one-entered-until-it-leaves",
        move |_| nothing_at_first && now_the_button && cleared,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// pointer.click.a-press-on-a-toolbar-button-opens-the-panel-it-owns
// ---------------------------------------------------------------------------------------------

/// **No panel is named.** Nothing pairs a button with a page but the layout's own attribute, so
/// this takes the first button the toolbar was given and asks what the stack did.
pub(super) fn a_press_on_a_toolbar_button_opens_the_panel_it_owns() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    let button = {
        let shell = c.view().expect_app().ui().expect("the UI shell is up");
        let screen = shell.flow.current().expect("a screen is up");
        let any: &dyn std::any::Any = screen;
        any.downcast_ref::<dereth_ui_screens::screens::gameplay::GamePlayScreen>()
            .expect("the gameplay screen")
            .toolbar
            .buttons
            .first()
            .copied()
            .expect("the toolbar was given its panel buttons")
    };
    let stack = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .get_element(dereth_ui_screens::screens::gameplay::window::PANEL_STACK)
        .expect("the stack of panels is in the shipped layout");
    let visible = |c: &HeadlessClient, h: dereth_ui::ElemHandle| {
        c.view()
            .expect_app()
            .ui()
            .expect("the UI shell is up")
            .ui
            .node(h)
            .expect("alive")
            .region
            .flags
            .visible
    };
    let it_starts_away = !visible(&c, stack);

    let (x, y) = middle_of(&c, button.handle);
    let mut hands = Hands::new();
    hands.click_at(&mut c, x, y);
    // One more frame so the stack's own showing settles.
    c.tick(1);
    let it_came_up = visible(&c, stack);

    c.assert_behaviour(
        "pointer.click.a-press-on-a-toolbar-button-opens-the-panel-it-owns",
        move |_| it_starts_away && it_came_up,
    );
    c.shutdown();
}
