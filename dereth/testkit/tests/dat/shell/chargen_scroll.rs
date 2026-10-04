//! Shell fixtures and scenarios for chargen scroll.

use super::*;
use dereth_ui_screens::screens::chargen;
// =============================================================================================
// chargen.scroll.* -- the wizard's lists and panes really scroll
//
// The heritage screen's scrollbar, and every other scrollable on the wizard, really works. Three
// checks fold into the scenarios below rather than becoming rows of their own: the second list
// having its own bar, scrolling one pane not moving another, and the layout census are the same
// claim -- each scrollable drives the bar beside it -- read from three sides.
//
// **The wheel is a scroll path here**, not a press: it arrives at the scrollable rather than at the
// widget's own press handler. The two directions are asserted separately, because a wheel driven
// one way and then back cancels, and a working wheel and a dead one would read alike.
// =============================================================================================

const HERITAGE_PANE: ElementId = ElementId(0x1000_03C4);
const APPEARANCE_PANE: ElementId = ElementId(0x1000_03AB);
const SUMMARY_PANE: ElementId = ElementId(0x1000_0404);
const CHARGEN_SKILLS_LIST: ElementId = ElementId(0x1000_03F7);
const SUMMARY_LIST: ElementId = ElementId(0x1000_0400);

/// The wizard with a people and a home town chosen, so every description pane holds its text.
fn a_wizard_with_descriptions() -> HeadlessClient {
    let mut c = a_client_on_the_wizard();
    click_wizard(&mut c, chargen::HERITAGE_BUTTONS[0].0);
    click_wizard(&mut c, chargen::TOWN_BUTTONS[0].0);
    c
}

/// Every page shown once, which is what builds the lists on each of them.
fn show_every_page(c: &mut HeadlessClient) {
    for p in EcgProgress::PAGES {
        click_wizard(c, p.select_button().expect("a real page has a tab"));
        c.tick(4);
    }
}

fn box_of(c: &HeadlessClient, h: dereth_ui::ElemHandle) -> dereth_ui::Box2D {
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(h)
        .expect("live")
        .region
        .box_
}

/// A text pane's own scroll state.
fn text_scroll(
    c: &mut HeadlessClient,
    h: dereth_ui::ElemHandle,
) -> dereth_ui::scrollable::Scrollable {
    c.app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .ui
        .text_element_mut(h)
        .expect("a text pane")
        .scroll
}

/// A list box's own.
fn chargen_list_scroll(
    c: &HeadlessClient,
    h: dereth_ui::ElemHandle,
) -> dereth_ui::scrollable::Scrollable {
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(h)
        .and_then(|n| n.behaviour.as_ref())
        .and_then(|b| b.as_any())
        .and_then(|a| a.downcast_ref::<dereth_ui::widgets::listbox::ListBox>())
        .expect("a list box")
        .scroll
}

/// Which bar a scrollable really bound -- the whole of the pane-to-bar question.
fn bound_bar(
    c: &HeadlessClient,
    s: dereth_ui::scrollable::Scrollable,
    me: dereth_ui::ElemHandle,
) -> Option<dereth_ui::ElemHandle> {
    s.scrollbar(
        &c.view().expect_app().ui().expect("the UI shell is up").ui,
        me,
        false,
    )
}

fn bar_float(c: &HeadlessClient, h: dereth_ui::ElemHandle, id: u32) -> Option<f32> {
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(h)?
        .merged_properties()
        .get_float(id)
}

fn bar_bool(c: &HeadlessClient, h: dereth_ui::ElemHandle, id: u32) -> Option<bool> {
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(h)?
        .merged_properties()
        .get_bool(id)
}

fn bar_enum(c: &HeadlessClient, h: dereth_ui::ElemHandle, id: u32) -> Option<ElementId> {
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(h)?
        .merged_properties()
        .get_enum(id)
        .map(ElementId)
}

/// Where the list's first row is, which is what a reader watches move.
fn first_row_y(c: &HeadlessClient, list: dereth_ui::ElemHandle) -> i32 {
    let shell = c.view().expect_app().ui().expect("the UI shell is up");
    let row = *shell.ui.children(list).first().expect("the list has rows");
    shell.ui.node(row).expect("a live row").region.box_.y0
}

/// One press of an arrow, through the arrow **button** the bar names, so that the client picks
/// which message that arrow raises rather than the scenario picking it.
fn press_arrow(c: &mut HeadlessClient, bar: dereth_ui::ElemHandle, increment: bool) {
    let which = if increment {
        bar_attr::INCREMENT_BUTTON
    } else {
        bar_attr::DECREMENT_BUTTON
    };
    let id = bar_enum(c, bar, which).expect("the bar names that arrow");
    let h = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .get_child_recursive(bar, id)
        .expect("the arrow is a child of the bar");
    c.app_mut()
        .ui_mut()
        .expect("the UI shell is up")
        .ui
        .broadcast_element_message(h, dereth_ui::msg::element::id::BUTTON_HOT_CLICK, 7, 0);
    c.tick(1);
}

/// The thumb put at a fraction of the track, telling its owner as the bar does.
fn drag_thumb_to(c: &mut HeadlessClient, bar: dereth_ui::ElemHandle, pos: f32) {
    let shell = c.app_mut().ui_mut().expect("the UI shell is up");
    shell.ui.set_attribute_float(bar, bar_attr::POSITION, pos);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let p1 = (pos * dereth_ui::widgets::scrollbar::POSITION_SCALE) as u32;
    shell
        .ui
        .broadcast_element_message(bar, dereth_ui::msg::element::id::SCROLL_POSITION, p1, 0);
    c.tick(1);
}

// ---------------------------------------------------------------------------------------------
// chargen.scroll.three-panes-and-two-lists-scroll-and-the-town-page-has-nothing-to-scroll
// ---------------------------------------------------------------------------------------------

/// What in the wizard scrolls, and what does not.
pub(super) fn three_panes_and_two_lists_scroll_and_the_town_page_has_none() {
    let mut c = a_wizard_with_descriptions();
    show_every_page(&mut c);

    let (bars, mut scrollables) = {
        let shell = c.view().expect_app().ui().expect("the UI shell is up");
        let root = shell.flow.current().expect("a screen").roots()[0];
        let mut stack = vec![root];
        let mut bars = 0;
        let mut scrollables = Vec::new();
        while let Some(h) = stack.pop() {
            stack.extend(shell.ui.children(h));
            let Some(n) = shell.ui.node(h) else { continue };
            if n.ty().0 == 0x0B {
                bars += 1;
            }
            let p = n.merged_properties();
            if p.get_enum(dereth_ui::scrollable::attr::H_SCROLLBAR)
                .is_some()
                || p.get_enum(dereth_ui::scrollable::attr::V_SCROLLBAR)
                    .is_some()
            {
                scrollables.push((n.element_id(), n.ty().0));
            }
        }
        (bars, scrollables)
    };
    scrollables.sort_by_key(|(id, _)| id.0);

    let twelve_bars = bars == 12;
    let five = scrollables
        == vec![
            (APPEARANCE_PANE, 0x0C),
            (HERITAGE_PANE, 0x0C),
            (CHARGEN_SKILLS_LIST, 0x05),
            (SUMMARY_LIST, 0x05),
            (SUMMARY_PANE, 0x0C),
        ];

    c.assert_behaviour(
        "chargen.scroll.three-panes-and-two-lists-scroll-and-the-town-page-has-nothing-to-scroll",
        move |_| twelve_bars && five,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// chargen.scroll.each-pane-drives-the-bar-beside-it-and-not-another-pages
// ---------------------------------------------------------------------------------------------

/// Three panes name the same bar and drive three different ones, and moving one moves no other.
pub(super) fn each_pane_drives_the_bar_beside_it() {
    let mut c = a_wizard_with_descriptions();
    show_every_page(&mut c);

    let pages: [(ElementId, ElementId); 3] = [
        (HERITAGE_PANE, ElementId(0x1000_03D1)),
        (APPEARANCE_PANE, ElementId(0x1000_03D4)),
        (SUMMARY_PANE, ElementId(0x1000_03D6)),
    ];
    let mut names_the_shared_one = true;
    let mut inside_its_own_page = true;
    let mut bound = Vec::new();
    for (pane, page) in pages {
        let ph = element(&c, pane);
        let s = text_scroll(&mut c, ph);
        // They all name one id, which is why binding the right element matters at all.
        names_the_shared_one &= s.v_scrollbar == Some(ElementId(0x1000_02E7));
        let bar = bound_bar(&c, s, ph).expect("the pane binds a bar");
        let page_h = element(&c, page);
        inside_its_own_page &= c
            .view()
            .expect_app()
            .ui()
            .expect("the UI shell is up")
            .ui
            .is_ancestor_of(page_h, bar);
        bound.push(bar);
    }
    bound.sort_unstable();
    bound.dedup();
    let three_different = bound.len() == 3;

    // The second list is not on the first list's bar either.
    click_wizard(
        &mut c,
        EcgProgress::Summary
            .select_button()
            .expect("the summary tab"),
    );
    c.tick(4);
    let list = element(&c, SUMMARY_LIST);
    let s = chargen_list_scroll(&c, list);
    let overflows = s.height > box_of(&c, list).height();
    let bar = bound_bar(&c, s, list).expect("the summary list binds a bar");
    let its_own = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .node(bar)
        .expect("live")
        .element_id()
        == ElementId(0x1000_0401);
    let top = first_row_y(&c, list);
    press_arrow(&mut c, bar, false);
    let moved = chargen_list_scroll(&c, list).y;
    let it_scrolled = moved > 0 && first_row_y(&c, list) == top - moved;

    // And moving one pane leaves another alone, which is the cross-talk the shared id invites.
    show_every_page(&mut c);
    let summary = element(&c, SUMMARY_PANE);
    let appearance = element(&c, APPEARANCE_PANE);
    let s = text_scroll(&mut c, summary);
    let bar = bound_bar(&c, s, summary).expect("the summary pane's bar");
    let before = text_scroll(&mut c, appearance).y;
    press_arrow(&mut c, bar, false);
    let no_cross_talk =
        text_scroll(&mut c, summary).y > 0 && text_scroll(&mut c, appearance).y == before;

    c.assert_behaviour(
        "chargen.scroll.each-pane-drives-the-bar-beside-it-and-not-another-pages",
        move |_| {
            names_the_shared_one
                && inside_its_own_page
                && three_different
                && overflows
                && its_own
                && it_scrolled
                && no_cross_talk
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// chargen.scroll.a-pane-with-text-in-it-has-a-live-bar-whose-thumb-is-the-size-of-what-is-shown
// ---------------------------------------------------------------------------------------------

/// A filled pane knows how much it holds, and its bar says so.
pub(super) fn a_filled_pane_has_a_live_bar_sized_to_what_is_shown() {
    let mut c = a_wizard_with_descriptions();
    show_every_page(&mut c);

    let mut measured = true;
    let mut bars_follow = true;
    for pane in [HERITAGE_PANE, APPEARANCE_PANE, SUMMARY_PANE] {
        let h = element(&c, pane);
        let s = text_scroll(&mut c, h);
        let b = box_of(&c, h);
        measured &= s.height > 0 && s.width > 0;
        let bar = bound_bar(&c, s, h).expect("its bar");
        let overflows = s.height > b.height();
        // A bar is dead exactly when there is nothing to scroll...
        bars_follow &= bar_bool(&c, bar, bar_attr::DISABLED) == Some(!overflows);
        // ...and the thumb covers as much of the track as the box covers of the text.
        #[allow(clippy::cast_precision_loss)]
        let want = (b.height() as f32 / s.height.max(b.height()) as f32).min(1.0);
        let got = bar_float(&c, bar, bar_attr::PROPORTION).expect("a proportion");
        bars_follow &= (got - want).abs() < 1e-4;
    }

    c.assert_behaviour("chargen.scroll.a-pane-with-text-in-it-has-a-live-bar-whose-thumb-is-the-size-of-what-is-shown", move |_| {
        measured && bars_follow
    });
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// chargen.scroll.dragging-the-thumb-moves-the-rows-by-the-same-fraction
// ---------------------------------------------------------------------------------------------

/// Half way down the track is half way down the list, and the rows really move.
pub(super) fn dragging_the_thumb_moves_the_rows_by_the_same_fraction() {
    let mut c = a_wizard_with_descriptions();
    click_wizard(
        &mut c,
        EcgProgress::Skills.select_button().expect("the skills tab"),
    );
    c.tick(4);
    let list = element(&c, CHARGEN_SKILLS_LIST);
    let s = chargen_list_scroll(&c, list);
    let view = box_of(&c, list).height();
    let travel = s.height - view;
    assert!(
        travel > 0,
        "the skill rows overflow a {view} pixel box; they are {} tall",
        s.height
    );
    let bar = bound_bar(&c, s, list).expect("the list binds its bar");
    let top = first_row_y(&c, list);

    drag_thumb_to(&mut c, bar, 0.5);
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    let want = (0.5_f32 * travel as f32) as i32;
    // The offset is the fraction of the travel, and -- the half a screenshot would miss -- the
    // rows themselves moved up by exactly that many pixels.
    let half_way = chargen_list_scroll(&c, list).y == want && first_row_y(&c, list) == top - want;

    drag_thumb_to(&mut c, bar, 0.0);
    let back = chargen_list_scroll(&c, list).y == 0 && first_row_y(&c, list) == top;

    c.assert_behaviour(
        "chargen.scroll.dragging-the-thumb-moves-the-rows-by-the-same-fraction",
        move |_| half_way && back,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// chargen.scroll.an-arrow-moves-one-row-and-the-two-arrows-go-opposite-ways
// ---------------------------------------------------------------------------------------------

/// One press of an arrow is one row, the two arrows undo each other, and the top is the top.
pub(super) fn an_arrow_moves_one_row_and_the_two_go_opposite_ways() {
    let mut c = a_wizard_with_descriptions();
    click_wizard(
        &mut c,
        EcgProgress::Skills.select_button().expect("the skills tab"),
    );
    c.tick(4);
    let list = element(&c, CHARGEN_SKILLS_LIST);
    let s = chargen_list_scroll(&c, list);
    let bar = bound_bar(&c, s, list).expect("the bar");
    let rows = i32::try_from(
        c.view()
            .expect_app()
            .ui()
            .expect("the UI shell is up")
            .ui
            .children(list)
            .len(),
    )
    .expect("a row count");
    let one_row = s.height / rows;
    assert!(one_row > 0, "{rows} rows over {} pixels", s.height);
    let top = first_row_y(&c, list);

    // Down: the rows move **up** past the box, which is the direction a reader means by down.
    press_arrow(&mut c, bar, false);
    let one = chargen_list_scroll(&c, list).y == one_row && first_row_y(&c, list) == top - one_row;
    press_arrow(&mut c, bar, false);
    let two = chargen_list_scroll(&c, list).y == 2 * one_row;

    // Up: the other arrow undoes it, exactly.
    press_arrow(&mut c, bar, true);
    let back_one = chargen_list_scroll(&c, list).y == one_row;
    press_arrow(&mut c, bar, true);
    let home = chargen_list_scroll(&c, list).y == 0 && first_row_y(&c, list) == top;

    // And the top is the top: another press there goes nowhere.
    press_arrow(&mut c, bar, true);
    let clamped = chargen_list_scroll(&c, list).y == 0;

    c.assert_behaviour(
        "chargen.scroll.an-arrow-moves-one-row-and-the-two-arrows-go-opposite-ways",
        move |_| one && two && back_one && home && clamped,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// chargen.scroll.a-press-on-the-track-moves-a-whole-page-towards-the-press
// ---------------------------------------------------------------------------------------------

/// A press on the track below the thumb pages down, and one above it pages back.
pub(super) fn a_press_on_the_track_moves_a_whole_page_towards_the_press() {
    let mut c = a_wizard_with_descriptions();
    click_wizard(
        &mut c,
        EcgProgress::Skills.select_button().expect("the skills tab"),
    );
    c.tick(4);
    let list = element(&c, CHARGEN_SKILLS_LIST);
    let s = chargen_list_scroll(&c, list);
    let bar = bound_bar(&c, s, list).expect("the bar");
    let view = box_of(&c, list).height();

    let press_track = |c: &mut HeadlessClient, below: bool| {
        let id = if below { 0x10 } else { 0x0F };
        c.app_mut()
            .ui_mut()
            .expect("the UI shell is up")
            .ui
            .broadcast_element_message(bar, dereth_ui::MessageId(id), 0, 0);
        c.tick(1);
    };

    press_track(&mut c, true);
    // For a list a page is the whole box, not a box less one row.
    let a_page = chargen_list_scroll(&c, list).y == view;
    press_track(&mut c, false);
    let back = chargen_list_scroll(&c, list).y == 0;

    c.assert_behaviour(
        "chargen.scroll.a-press-on-the-track-moves-a-whole-page-towards-the-press",
        move |_| a_page && back,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// chargen.scroll.the-wheel-moves-a-list-one-row-and-stops-at-the-top
// ---------------------------------------------------------------------------------------------

/// A wheel detent is one row, both ways, and the top is the top.
pub(super) fn the_wheel_moves_a_list_one_row_and_stops_at_the_top() {
    let mut c = a_wizard_with_descriptions();
    click_wizard(
        &mut c,
        EcgProgress::Skills.select_button().expect("the skills tab"),
    );
    c.tick(4);
    let list = element(&c, CHARGEN_SKILLS_LIST);

    let wheel = |c: &mut HeadlessClient, action: u32| {
        c.app_mut()
            .ui_mut()
            .expect("the UI shell is up")
            .ui
            .broadcast_element_message(list, dereth_ui::msg::element::id::MOUSE_PRESS, action, 0);
        c.tick(1);
    };

    let at_the_top = chargen_list_scroll(&c, list).y == 0;
    // Backwards at the top: nowhere to go. **Asserted separately from the other direction**,
    // because a wheel driven both ways cancels and a dead wheel would read the same.
    wheel(&mut c, dereth_ui::focus::action::WHEEL_UP);
    let clamped = chargen_list_scroll(&c, list).y == 0;

    wheel(&mut c, dereth_ui::focus::action::WHEEL_DOWN);
    let moved = chargen_list_scroll(&c, list).y;
    let s = chargen_list_scroll(&c, list);
    let rows = i32::try_from(
        c.view()
            .expect_app()
            .ui()
            .expect("the UI shell is up")
            .ui
            .children(list)
            .len(),
    )
    .expect("a row count");
    // ...and a detent is one row and not a whole page, which is what pressing the arrow does.
    let one_row = moved > 0 && moved == s.height / rows;

    wheel(&mut c, dereth_ui::focus::action::WHEEL_UP);
    let back = chargen_list_scroll(&c, list).y == 0;

    c.assert_behaviour(
        "chargen.scroll.the-wheel-moves-a-list-one-row-and-stops-at-the-top",
        move |_| at_the_top && clamped && one_row && back,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// chargen.scroll.a-pane-measures-every-line-of-its-text-its-margins-and-the-blank-row-after-it
// ---------------------------------------------------------------------------------------------

/// The people's own paragraph, measured: every wrapped line, the pane's four margins, and the
/// blank row the paragraph's closing newline leaves behind -- which is what gives the bar its
/// fourteen pixels of travel rather than none.
pub(super) fn a_pane_measures_its_whole_paragraph_and_the_blank_row_after_it() {
    let mut c = a_wizard_with_descriptions();
    let pane = element(&c, HERITAGE_PANE);
    c.tick(4);
    let s = text_scroll(&mut c, pane);
    let b = box_of(&c, pane);
    let shipped_box = (b.width(), b.height()) == (265, 450);

    // The line table first, so the pair below is a measurement and not two magic numbers: a pane
    // that lost its margins, or the trailing row, or that broke the paragraph differently would
    // fail here with the reason named.
    let lines_are = {
        let shell = c.app_mut().ui_mut().expect("the UI shell is up");
        let t = shell
            .ui
            .text_element_mut(pane)
            .expect("the pane is a text element");
        let margins = t.margins == (15, 15, 9, 26);
        let wrapped = !t.glyphs.one_line;
        let lines = &t.glyphs.lines;
        let count = lines.len() == 31;
        let one_height = lines.iter().all(|l| l.height == 14);
        let last = *lines.last().expect("a last line");
        let trailing_is_empty =
            (last.end - last.start, last.width) == (0, 0) && last.start == t.glyphs.glyphs.len();
        let widest = lines.iter().map(|l| l.width).max() == Some(226);
        margins && wrapped && count && one_height && trailing_is_empty && widest
    };
    let measured = (s.width, s.height) == (226 + 9 + 26, 31 * 14 + 15 + 15)
        && (s.width, s.height) == (261, 464);

    let bar = bound_bar(&c, s, pane).expect("its own bar");
    let live = s.height > b.height()
        && bar_bool(&c, bar, bar_attr::DISABLED) == Some(s.height <= b.height());

    c.assert_behaviour("chargen.scroll.a-pane-measures-every-line-of-its-text-its-margins-and-the-blank-row-after-it", move |_| {
        shipped_box && lines_are && measured && live
    });
    c.shutdown();
}
