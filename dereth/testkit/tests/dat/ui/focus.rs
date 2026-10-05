//! UI fixtures and scenarios for focus.

use super::*;
// =============================================================================================
// ui.focus.* and ui.scrollbar.* -- the keyboard, and the thing you drag
//
// A press in the chat box takes the keyboard so chat can be typed and sent, and the scrollbar
// beside it draws its thumb rather than an empty groove. Six scenarios, six rows.
//
// The keyboard is driven through the harness's own keyboard steps, which build the client's real
// `WM_CHAR` and hand it to the client's real message path; the pointer is `Hands`. Nothing here is
// a desktop event and nothing here is a windowed run.
// =============================================================================================

/// The chat **log**: the 368 x 73 scrollback, which is sweep-selectable and is not the entry box.
const THE_CHAT_LOG: ElementId = ElementId(0x1000_0011);
/// The chat window's own vertical bar.
const CHAT_BAR: ElementId = ElementId(0x1000_0012);
/// The toolbar strip the stack splitter lives in; the shipped layout starts it down.
pub(super) const SEL_OBJECT_FIELD: ElementId = ElementId(0x1000_019E);

/// The shipped id an element was built from.
fn shipped_id(c: &HeadlessClient, h: ElemHandle) -> Option<ElementId> {
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(h)
        .map(dereth_ui::ElementNode::element_id)
}

/// Who holds the keyboard, if anyone.
pub(crate) fn who_holds_the_keyboard(c: &HeadlessClient) -> Option<ElemHandle> {
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .focus_element()
}

/// Whether the client is taking typing at all -- the switch a box holding the keyboard turns on.
fn taking_typing(c: &mut HeadlessClient) -> bool {
    c.app_mut()
        .input_manager_mut()
        .expect("the input shell is up")
        .manager
        .text
        .text_mode
}

/// How many characters have reached an element, which is what tells "typed and refused" from
/// "never delivered".
fn characters_delivered(c: &HeadlessClient) -> u64 {
    u64::from(
        c.view()
            .expect_app()
            .ui()
            .expect("the UI shell is up")
            .stats
            .characters_delivered,
    )
}

/// What is actually in a text box.
fn typed_text(c: &mut HeadlessClient, h: ElemHandle) -> String {
    let (ui, _) = hud_gameplay(c);
    ui.text_element_mut(h).map_or(String::new(), |t| {
        String::from_utf16_lossy(&t.glyphs.glyphs.iter().map(|g| g.data).collect::<Vec<_>>())
    })
}

/// A gameplay client with the stack splitter up: the strip and its two widgets are shown the way
/// the toolbar shows them when a stack is selected, and the splitter is seeded from a stack of 20.
fn a_client_with_the_stack_splitter_up() -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    let field = hud_find(&c, SEL_OBJECT_FIELD);
    let entry = hud_find(&c, dereth_ui_screens::toolbar::splitter::ENTRY_BOX);
    let slider = hud_find(&c, dereth_ui_screens::toolbar::splitter::SLIDER);
    {
        let (ui, screen) = hud_gameplay(&mut c);
        // The toolbar puts all three down until a stack is selected, and a pointer cannot reach
        // something that is not visible -- so this stands in for the selection the scenario does
        // not make, and nothing else about the widgets is touched.
        ui.set_visible(field, true);
        ui.set_visible(entry, true);
        ui.set_visible(slider, true);
        screen.splitter = dereth_ui_screens::toolbar::splitter::Splitter::new(20);
    }
    c.tick(1);
    c
}

// ---------------------------------------------------------------------------------------------
// ui.focus.a-press-in-a-box-takes-the-keyboard-and-keeps-it
// ---------------------------------------------------------------------------------------------

/// A press in a box takes the keyboard, in one gesture and then ten frames of nothing.
pub(super) fn a_press_in_a_box_takes_the_keyboard_and_keeps_it() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    let nobody_held_it = who_holds_the_keyboard(&c).is_none();

    let presses_before = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .stats
        .mouse_downs;
    // The shipped id names a log in each of the five chat windows, so the gesture says which one
    // it means and the box under test is whichever one the press actually reached.
    c.when(Player::Click(dereth_testkit::Target::Nth {
        id: THE_CHAT_LOG,
        index: 0,
    }));
    let holder = who_holds_the_keyboard(&c);
    // A real press and not a broadcast: the hit test is what found the box.
    let a_real_press = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .stats
        .mouse_downs
        == presses_before + 1
        && c.view()
            .expect_app()
            .ui()
            .expect("the UI shell is up")
            .ui
            .mouse_over()
            == holder;
    let it_took_the_keyboard = holder.is_some_and(|h| shipped_id(&c, h) == Some(THE_CHAT_LOG));
    let log = holder.expect("the press gave the keyboard to the box it landed on");

    // ...and it keeps it, rather than losing it again by the next frame.
    let mut kept_it = true;
    for _ in 0..10 {
        c.tick(1);
        kept_it &= who_holds_the_keyboard(&c) == Some(log)
            && c.view()
                .expect_app()
                .ui()
                .expect("the UI shell is up")
                .ui
                .is_alive(log);
    }

    c.assert_behaviour(
        "ui.focus.a-press-in-a-box-takes-the-keyboard-and-keeps-it",
        move |_| nobody_held_it && a_real_press && it_took_the_keyboard && kept_it,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// ui.focus.what-is-typed-reaches-the-box-holding-the-keyboard-and-stops-when-it-lets-go
// ---------------------------------------------------------------------------------------------

/// The other half of the seam, on the one box in the toolbar a player really types a number into.
pub(super) fn what_is_typed_reaches_the_box_holding_the_keyboard() {
    let mut c = a_client_with_the_stack_splitter_up();
    let entry = hud_find(&c, dereth_ui_screens::toolbar::splitter::ENTRY_BOX);
    let it_is_a_box_to_type_in = {
        let (ui, _) = hud_gameplay(&mut c);
        ui.text_element_mut(entry)
            .expect("a text element")
            .bits
            .editable()
    };
    let off_to_start_with = !taking_typing(&mut c);

    c.when(Player::Click(
        dereth_ui_screens::toolbar::splitter::ENTRY_BOX.into(),
    ));
    let it_holds_the_keyboard = who_holds_the_keyboard(&c) == Some(entry);
    let typing_came_on = taking_typing(&mut c);

    let before = characters_delivered(&c);
    c.when(Player::Type("42".into()));
    let both_arrived = characters_delivered(&c) == before + 2;
    let both_are_in_the_box = typed_text(&mut c, entry) == "42";

    // Let the keyboard go. The toolbar reads the box on that edge and settles what it holds -- 42
    // out of a stack of 20 comes back 20 -- and this scenario's claim is that nothing more can be
    // typed into it, not what the toolbar left there.
    {
        let (ui, _) = hud_gameplay(&mut c);
        ui.set_focus_element(None);
    }
    c.tick(1);
    let typing_went_off = !taking_typing(&mut c);
    let settled = typed_text(&mut c, entry);
    let the_owner_settled_it = settled == "20";

    let before = characters_delivered(&c);
    c.when(Player::Type("9".into()));
    let nothing_was_delivered = characters_delivered(&c) == before;
    let the_box_took_nothing = typed_text(&mut c, entry) == settled;

    c.assert_behaviour(
        "ui.focus.what-is-typed-reaches-the-box-holding-the-keyboard-and-stops-when-it-lets-go",
        move |_| {
            it_is_a_box_to_type_in
                && off_to_start_with
                && it_holds_the_keyboard
                && typing_came_on
                && both_arrived
                && both_are_in_the_box
                && typing_went_off
                && the_owner_settled_it
                && nothing_was_delivered
                && the_box_took_nothing
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// ui.focus.a-log-takes-the-keyboard-and-none-of-what-is-typed
// ---------------------------------------------------------------------------------------------

/// Both arms, so the boundary is a fact and not a hedge: as shipped the keystrokes reach the log
/// and are inserted nowhere, and with the one bit set the same keystrokes land.
pub(super) fn a_log_takes_the_keyboard_and_none_of_what_is_typed() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    // One of the five chat windows' logs, named the way the press names it, and read afterwards
    // so that the box inspected is the very box the press reached.
    c.when(Player::Click(dereth_testkit::Target::Nth {
        id: THE_CHAT_LOG,
        index: 0,
    }));
    let holder = who_holds_the_keyboard(&c);
    let it_still_takes_the_keyboard =
        holder.is_some_and(|h| shipped_id(&c, h) == Some(THE_CHAT_LOG));
    let log = holder.expect("the press gave the keyboard to the box it landed on");
    let (sweepable, not_writable) = {
        let (ui, _) = hud_gameplay(&mut c);
        let bits = ui.text_element_mut(log).expect("a text element").bits;
        (bits.selectable(), !bits.editable())
    };
    c.when(Player::Type("hi".into()));
    let nothing_went_in = typed_text(&mut c, log).is_empty();

    // The one bit, set the way the client's own writer sets it.
    let the_bit_took = {
        let (ui, _) = hud_gameplay(&mut c);
        ui.set_attribute_bool(log, dereth_ui::props::attr::TEXT_EDITABLE, true);
        ui.text_element_mut(log)
            .expect("a text element")
            .bits
            .editable()
    };
    c.tick(1);
    c.when(Player::Type("hi".into()));
    let now_it_lands = typed_text(&mut c, log) == "hi";

    c.assert_behaviour(
        "ui.focus.a-log-takes-the-keyboard-and-none-of-what-is-typed",
        move |_| {
            sweepable
                && not_writable
                && it_still_takes_the_keyboard
                && nothing_went_in
                && the_bit_took
                && now_it_lands
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// ui.scrollbar.dragging-the-stack-slider-moves-the-thumb-and-the-quantity-follows
// ---------------------------------------------------------------------------------------------

/// The acceptance gesture: press on the track, drag, and read the thumb's own pixels and the
/// quantity at three positions. A split of the wrong size looks exactly like a split of the right
/// one, so the quantity is read at each of them rather than once at the end.
pub(super) fn dragging_the_stack_slider_moves_the_thumb_and_the_quantity_follows() {
    let mut c = a_client_with_the_stack_splitter_up();
    let slider = hud_find(&c, dereth_ui_screens::toolbar::splitter::SLIDER);

    let sb = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .screen_box(slider);
    let the_shipped_box = (sb.width(), sb.height()) == (90, 14);
    let y = (sb.y0 + sb.y1) / 2;

    let thumb_x = |c: &HeadlessClient| {
        let ui = &c.view().expect_app().ui().expect("the UI shell is up").ui;
        let t = ui
            .get_child(slider, ElementId(1))
            .expect("the thumb is the slider's own child");
        ui.node(t).expect("alive").region.box_.x0
    };
    let position = |c: &HeadlessClient| {
        let ui = &c.view().expect_app().ui().expect("the UI shell is up").ui;
        ui.node(slider)
            .expect("alive")
            .merged_properties()
            .get_float(dereth_ui::widgets::scrollbar::attr::POSITION)
    };
    let split = |c: &mut HeadlessClient| hud_gameplay(c).1.splitter.split_size;

    let mut hands = Hands::new();
    // The near end of the track: nothing moved, one item.
    hands.move_to(&mut c, sb.x0, y);
    let m = hands.button_message(dereth_input::keys::MouseButton::Left, true);
    hands.send(&mut c, m);
    c.tick(1);
    let at_the_near_end = position(&c) == Some(0.0) && thumb_x(&c) == 0 && split(&mut c) == 1;

    // Half way along. The pointer sits half a thumb ahead of the thumb's own left edge, and the
    // travel is the track less the thumb.
    hands.move_to(&mut c, sb.x0 + 44, y);
    c.tick(1);
    let mid = position(&c).expect("a dragged slider has a position");
    let half_way = (mid - 36.0 / 73.0).abs() < 1e-5 && thumb_x(&c) == 36 && split(&mut c) == 10;

    // Past the far end: both the thumb and the quantity stop hard.
    hands.move_to(&mut c, sb.x1 + 500, y);
    c.tick(1);
    let clamped = position(&c) == Some(1.0) && thumb_x(&c) == 73 && split(&mut c) == 20;

    // ...and the drag ends when the button does.
    let m = hands.button_message(dereth_input::keys::MouseButton::Left, false);
    hands.send(&mut c, m);
    c.tick(1);
    hands.move_to(&mut c, sb.x0, y);
    c.tick(1);
    let let_go = position(&c) == Some(1.0) && split(&mut c) == 20;

    c.assert_behaviour(
        "ui.scrollbar.dragging-the-stack-slider-moves-the-thumb-and-the-quantity-follows",
        move |_| the_shipped_box && at_the_near_end && half_way && clamped && let_go,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// ui.scrollbar.the-chat-bars-thumb-is-sized-and-placed-inside-its-track
// ---------------------------------------------------------------------------------------------

/// The cheap half of the same row: a thumb, in the track, between the two arrows -- not an empty
/// groove with the thumb still at the box the layout drew it in.
pub(super) fn the_chat_bars_thumb_is_sized_and_placed_inside_its_track() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    let bar = hud_find(&c, CHAT_BAR);

    let (the_shipped_bar, below_the_top_arrow, above_the_bottom_one, as_wide_as_the_bar, fills_it) = {
        let ui = &c.view().expect_app().ui().expect("the UI shell is up").ui;
        let bar_box = ui.node(bar).expect("alive").region.box_;
        // Which arrow ends up at which end is the bar's own doing and not the layout's: it puts
        // the one that scrolls forward at the top and the one that scrolls back at the bottom,
        // and the shipped chat bar authors them the other way round.
        let up = ui
            .get_child(bar, ElementId(0x1000_0072))
            .expect("the arrow the bar moves to the top");
        let down = ui
            .get_child(bar, ElementId(0x1000_0071))
            .expect("the arrow the bar moves to the bottom");
        let up_box = ui.node(up).expect("alive").region.box_;
        let down_box = ui.node(down).expect("alive").region.box_;
        let t = ui
            .node(
                ui.get_child(bar, ElementId(1))
                    .expect("the thumb is the bar's own child"),
            )
            .expect("alive")
            .region
            .box_;
        // With nothing to scroll the thumb covers the whole groove. That is the number the layout
        // box cannot give: the thumb ships square, and a bar that had only moved it would still
        // pass a "somewhere in the track" test while showing a stub in a long groove.
        let track = down_box.y0 - up_box.y1 - 1;
        (
            (bar_box.width(), bar_box.height()) == (16, 73),
            t.y0 > up_box.y1 && t.y0 == up_box.y1 + 1,
            t.y1 < down_box.y0,
            t.width() == bar_box.width(),
            track == 41 && t.height() == track,
        )
    };

    c.assert_behaviour(
        "ui.scrollbar.the-chat-bars-thumb-is-sized-and-placed-inside-its-track",
        move |_| {
            the_shipped_bar
                && below_the_top_arrow
                && above_the_bottom_one
                && as_wide_as_the_bar
                && fills_it
        },
    );
    c.shutdown();
}
