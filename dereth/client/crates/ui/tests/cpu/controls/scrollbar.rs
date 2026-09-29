//! A scrollbar places its own arrows, steps the content the right way, glides to a goal position,
//! and reports its position in truncated thousandths.
//!
//! What the retail client does:
//!
//! * the button-pointer helper's `true` answers the **increment** button;
//! * the scrolling-area update moves the increment arrow to `(0, 0)` and the decrement arrow to
//!   `(right, bottom)`, whatever the layout authored;
//! * the scrollbar-message handler pages for ids `0x0F` and `0x10`, and negates the step for ids
//!   `0x0D` and `0x0F`.
//!
//! So the increment arrow, the one moved to the *top*, raises `0x0D`, which is negated: a top arrow
//! that scrolls up. Three witnesses agree: the bar's own recovery arm re-places an unresolved arrow
//! with the same two moves; `MasterProperty 0x39000001` names `0x77` the increment and `0x78` the
//! decrement button; and a click on the track raises `0x10 - (clicked before the thumb)`, so `0x0F`
//! is a click **above** the thumb and must page up.
//!
//! The shipped layouts cannot settle it: the three vertical bars (`0x2100003E`'s `0x10000455` and
//! `0x10000367`, `0x2100004C`'s `0x100002DC`) author the increment at the far end and the
//! horizontal pair (`0x2100003E`'s `0x10000368` and `0x1000036D`) at the near end, so the authored
//! positions are decoration. A wrong placement and a wrong sign cancel on every shipped vertical
//! bar, so both are asserted separately. Fixture: a log and a bar from a `LayoutDesc` written here,
//! arrows authored the way the retail chat bar authors them.

use crate::common::NoAssets;
use dereth_primitives::{DataId, LocalTime};
use dereth_ui::desc::{incorporation, ElementDesc, LayoutDesc, StateDesc};
use dereth_ui::factory::ty;
use dereth_ui::focus::action;
use dereth_ui::msg::element::id as msgid;
use dereth_ui::scrollable::attr as scrollable_attr;
use dereth_ui::widgets::scrollbar::attr as bar_attr;
use dereth_ui::{ElemHandle, ElementId, ElementType, NullInputPump, UiSystem};

// ---------------------------------------------------------------------------------------------
// harness
// ---------------------------------------------------------------------------------------------

fn desc(id: u32, ty: ElementType, x: i32, y: i32, w: i32, h: i32) -> ElementDesc {
    ElementDesc {
        base: StateDesc {
            incorporation: incorporation::LEGACY_ALL_GEOMETRY,
            x,
            y,
            width: w,
            height: h,
            ..StateDesc::default()
        },
        element_id: ElementId(id),
        ty,
        ..ElementDesc::default()
    }
}

const LOG: u32 = 0x1000_0011;
const BAR: u32 = 0x1000_0012;
/// The shipped chat bar's own two arrow ids, kept because their *authored* positions are half the
/// point: `0x10000071` is attribute `0x78` (decrement) at the top and `0x10000072` is attribute
/// `0x77` (increment) at the bottom.
const ARROW_71: u32 = 0x1000_0071;
const ARROW_72: u32 = 0x1000_0072;
/// The thumb is the bar's child 1, which is where its layout pass looks for it.
const THUMB: u32 = 1;

/// A log and a vertical bar as siblings, the bar carrying a thumb and two 16x16 arrows **authored
/// the way the retail chat bar authors them**: `0x78` (decrement) at y = 0 and `0x77` (increment)
/// at y = 84. If `update_scrolling_area` did nothing, they would stay there.
fn tree(ui: &mut UiSystem) -> (ElemHandle, ElemHandle, ElemHandle) {
    let mut container = desc(0x1000_0010, ty::FIELD, 0, 0, 220, 100);
    let mut log = desc(LOG, ty::TEXT, 0, 0, 200, 100);
    log.base.properties.set(
        dereth_ui::props::attr::TEXT_SELECTABLE,
        dereth_ui::PropertyValue::Bool(true),
    );
    container.children.insert(ElementId(LOG), log);

    let mut bar = desc(BAR, ty::SCROLLBAR, 200, 0, 16, 100);
    bar.base.properties.set(
        bar_attr::DECREMENT_BUTTON,
        dereth_ui::PropertyValue::Enum(ARROW_71),
    );
    bar.base.properties.set(
        bar_attr::INCREMENT_BUTTON,
        dereth_ui::PropertyValue::Enum(ARROW_72),
    );
    bar.base
        .properties
        .set(bar_attr::PROPORTIONAL, dereth_ui::PropertyValue::Bool(true));
    bar.children
        .insert(ElementId(THUMB), desc(THUMB, ty::FIELD, 0, 0, 16, 16));
    bar.children.insert(
        ElementId(ARROW_71),
        desc(ARROW_71, ty::BUTTON, 0, 0, 16, 16),
    );
    bar.children.insert(
        ElementId(ARROW_72),
        desc(ARROW_72, ty::BUTTON, 0, 84, 16, 16),
    );
    container.children.insert(ElementId(BAR), bar);

    let l = LayoutDesc {
        did: DataId(0x2100_0001),
        display_width: 800,
        display_height: 600,
        elements: std::iter::once((ElementId(0x1000_0010), container)).collect(),
    };
    let d = l
        .access_element(ElementId(0x1000_0010))
        .cloned()
        .expect("root");
    let c = ui
        .create_element_recursive_from_full_desc(&NoAssets, &l, &d)
        .expect("no inheritance")
        .expect("registered");
    let root = ui.root();
    ui.set_parent(c, Some(root));
    let log = ui.get_child(c, ElementId(LOG)).expect("the log");
    let bar = ui.get_child(c, ElementId(BAR)).expect("the bar");
    ui.on_set_attribute(
        log,
        scrollable_attr::V_SCROLLBAR,
        Some(&dereth_ui::PropertyValue::Enum(BAR)),
    );
    ui.set_attribute_float(bar, bar_attr::POSITION, 0.0);
    ui.initialize_tree(c);
    fill(ui, c, log, 20);
    (c, log, bar)
}

fn fill(ui: &mut UiSystem, container: ElemHandle, log: ElemHandle, lines: usize) {
    let text: String = (0..lines)
        .map(|i| format!("line {i}\n"))
        .collect::<Vec<_>>()
        .concat();
    let t = ui.text_element_mut(log).expect("a text element");
    t.set_text(&text);
    t.bits.set_dirty(true);
    let n = dereth_ui::scrollable::recalculate_dirty_text(ui, container);
    assert_eq!(n, 1, "the log re-measured itself");
}

fn box_of(ui: &UiSystem, h: ElemHandle) -> dereth_ui::Box2D {
    ui.node(h).expect("alive").region.box_
}

fn child(ui: &UiSystem, parent: ElemHandle, id: u32) -> ElemHandle {
    ui.get_child(parent, ElementId(id)).expect("the child")
}

fn offset(ui: &mut UiSystem, log: ElemHandle) -> i32 {
    ui.text_element_mut(log).expect("a text element").scroll.y
}

/// One line, as the text layout computes it from the default metrics.
const LINE: i32 = 16;

/// Scrollbar reporting multiplies a loaded float by 1000 without an intermediate float store.
/// `0.005f` is below the mathematical value: narrowing the product to float changes 4
/// into 5. The toolbar's integer message argument makes that rounding difference observable.
#[test]
fn the_scroll_position_message_truncates_the_unrounded_product() {
    let mut ui = UiSystem::new((800, 600));
    let d = desc(BAR, ty::SCROLLBAR, 0, 0, 100, 16);
    let layout = LayoutDesc::default();
    let bar = ui
        .create_element_recursive_from_full_desc(&NoAssets, &layout, &d)
        .expect("standalone producer descriptor")
        .expect("registered scrollbar");
    ui.register_for_element_messages(bar, dereth_ui::ListenerId::External(0x373));
    ui.drain_outbox();
    let mut scroll = dereth_ui::widgets::scrollbar::Scrollbar::default();
    scroll.set_scrollbar_position(&mut ui, bar, f32::from_bits(0x3ba3_d70a));
    let positions: Vec<_> = ui
        .drain_outbox()
        .into_iter()
        .filter_map(|d| match d {
            dereth_ui::Delivery::Element { msg, .. } if msg.id == msgid::SCROLL_POSITION => {
                Some(msg.p1)
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        positions,
        [4],
        "the actual producer emits truncated thousandths, not rounded float product"
    );
}

// ---------------------------------------------------------------------------------------------
// 1. The arrows are placed by the element, not by the layout
// ---------------------------------------------------------------------------------------------

/// **The scrolling-area update moves the increment arrow to `(0, 0)` and the decrement to
/// `(right, bottom)`, and the layout's authored positions do not survive it.**
///
/// The tree authors them the way the retail chat bar does — decrement at the top, increment at the
/// bottom — so this test fails in *both* directions: if the moves are dropped the arrows stay
/// where they were authored, and if the two booleans are swapped they land at each other's ends.
#[test]
fn the_increment_arrow_is_moved_to_the_origin_and_the_decrement_to_the_far_end() {
    let mut ui = UiSystem::new((800, 600));
    let (_c, _log, bar) = tree(&mut ui);
    let inc = child(&ui, bar, ARROW_72);
    let dec = child(&ui, bar, ARROW_71);

    // Authored: 0x71 at y = 0, 0x72 at y = 84. Placed: the other way round.
    assert_eq!(
        (box_of(&ui, inc).x0, box_of(&ui, inc).y0),
        (0, 0),
        "the increment button (attribute 0x77) is at the bar's origin"
    );
    assert_eq!(
        (box_of(&ui, dec).x0, box_of(&ui, dec).y0),
        (0, 84),
        "and the decrement (0x78) at (right, bottom) = (16 - 16, 100 - 16)"
    );

    // The track is what is left between them, and it is the same span either way round — which is
    // exactly why the *area* could not have settled which arrow goes where.
    let thumb = child(&ui, bar, THUMB);
    let t = box_of(&ui, thumb);
    assert!(
        t.y0 >= 16 && t.y1 <= 84,
        "the thumb sits in the track between the arrows: {t:?}"
    );
}

/// The same function on a **horizontal** bar, where the move to `(right, bottom)` is a move in x.
///
/// This is the case the shipped data gets wrong and the runtime repairs: `0x2100003E`'s
/// `0x10000368` is 72 wide with 20-wide arrows and authors its far arrow at x = 32, where the
/// arithmetic puts it at 52. Reproduced here at the same numbers.
#[test]
fn a_horizontal_bar_places_its_arrows_in_x_and_repairs_the_shipped_authoring_slip() {
    let mut ui = UiSystem::new((800, 600));
    let mut bar = desc(BAR, ty::SCROLLBAR, 0, 0, 72, 20);
    bar.base
        .properties
        .set(bar_attr::HORIZONTAL, dereth_ui::PropertyValue::Bool(true));
    bar.base.properties.set(
        bar_attr::DECREMENT_BUTTON,
        dereth_ui::PropertyValue::Enum(ARROW_71),
    );
    bar.base.properties.set(
        bar_attr::INCREMENT_BUTTON,
        dereth_ui::PropertyValue::Enum(ARROW_72),
    );
    bar.children
        .insert(ElementId(THUMB), desc(THUMB, ty::FIELD, 0, 0, 28, 20));
    // `0x1000036B` is authored at x = 32 in the retail layout; `0x1000036C` at x = 0.
    bar.children.insert(
        ElementId(ARROW_71),
        desc(ARROW_71, ty::BUTTON, 32, 0, 20, 20),
    );
    bar.children.insert(
        ElementId(ARROW_72),
        desc(ARROW_72, ty::BUTTON, 0, 0, 20, 20),
    );
    let l = LayoutDesc {
        did: DataId(0x2100_0002),
        display_width: 800,
        display_height: 600,
        elements: std::iter::once((ElementId(BAR), bar)).collect(),
    };
    let d = l.access_element(ElementId(BAR)).cloned().expect("root");
    let h = ui
        .create_element_recursive_from_full_desc(&NoAssets, &l, &d)
        .expect("no inheritance")
        .expect("registered");
    let root = ui.root();
    ui.set_parent(h, Some(root));
    ui.initialize_tree(h);

    assert_eq!(
        (box_of(&ui, child(&ui, h, ARROW_72)).x0, 0),
        (0, 0),
        "increment at the near end"
    );
    assert_eq!(
        box_of(&ui, child(&ui, h, ARROW_71)).x0,
        52,
        "decrement at width - its own width, not the 32 the layout authored"
    );
}

// ---------------------------------------------------------------------------------------------
// 2. Which step ids are negated
// ---------------------------------------------------------------------------------------------

/// Behaviour: ui.scrollbar.the-arrows-step-the-content-the-right-way
///
/// **`0x0D` and `0x0F` are negated; `0x0E` and `0x10` are not.** The ids are literals here,
/// because a test that reads a message id back through the symbol it wrote it through cannot
/// detect a wrong one.
///
/// Driven as the four broadcasts the bar itself raises, so the assertion is about the *consumer*
/// (`inq_scroll_delta`'s negate flag) and not about which
/// gesture produced them. The direction of each is what a player sees:
///
/// | id | raised by | must |
/// |---|---|---|
/// | `0x0D` | the increment arrow, moved to the top | walk back toward line 0 |
/// | `0x0E` | the decrement arrow, at the bottom | walk on toward the newest line |
/// | `0x0F` | a track click **above** the thumb | page back |
/// | `0x10` | a track click below it | page on |
#[test]
fn the_negated_step_ids_are_0x0d_and_0x0f() {
    let mut ui = UiSystem::new((800, 600));
    let (_c, log, bar) = tree(&mut ui);

    // Park the log in the middle so both directions have room; a test that starts at a clamp
    // cannot tell "moved the wrong way" from "did not move".
    let step = |ui: &mut UiSystem, id: u32| {
        ui.broadcast_element_message(bar, dereth_ui::MessageId(id), 0, 0);
        offset(ui, log)
    };
    for _ in 0..7 {
        step(&mut ui, 0x0E);
    }
    let parked = offset(&mut ui, log);
    assert_eq!(
        parked,
        7 * LINE,
        "seven decrement steps walked seven lines on"
    );
    // 320 px of content in a 100 px view is 220 px of travel, and a page is 84, so 112 has room
    // for a page in both directions. A test parked against a clamp cannot tell "moved the wrong
    // way" from "did not move".
    assert!(
        parked - (100 - LINE) > 0 && parked + (100 - LINE) < 220,
        "room both ways: {parked}"
    );

    assert_eq!(
        step(&mut ui, 0x0D),
        parked - LINE,
        "0x0D is negated: one line back"
    );
    assert_eq!(
        step(&mut ui, 0x0E),
        parked,
        "0x0E is not: one line on again"
    );

    // The page pair. The text scroll delta is `view - step`, not the whole
    // view, which is the client's own overlap-a-line behaviour.
    let page = 100 - LINE;
    assert_eq!(
        step(&mut ui, 0x0F),
        parked - page,
        "0x0F is negated: a page back"
    );
    assert_eq!(step(&mut ui, 0x10), parked, "0x10 is not: a page on again");
}

/// Behaviour: ui.scrollbar.the-arrows-step-the-content-the-right-way
///
/// The same four ids, driven through the **arrows themselves**, which is what joins the negate
/// group to the placement: the arrow at the top of the bar must walk the log back.
///
/// Without the placement half this passes with the arrows at each other's ends, and without the
/// sign half it passes with both wrong: the two errors cancel.
#[test]
fn the_arrow_at_the_top_of_the_bar_walks_the_log_back() {
    let mut ui = UiSystem::new((800, 600));
    let (_c, log, bar) = tree(&mut ui);
    for _ in 0..5 {
        ui.broadcast_element_message(bar, dereth_ui::MessageId(0x0E), 0, 0);
    }
    let parked = offset(&mut ui, log);
    assert_eq!(parked, 5 * LINE);

    // Whichever element is at the top of the bar, pressed through the hot click its own
    // default hot-click setup armed.
    let top = [ARROW_71, ARROW_72]
        .into_iter()
        .map(|id| child(&ui, bar, id))
        .min_by_key(|h| box_of(&ui, *h).y0)
        .expect("two arrows");
    let bottom = [ARROW_71, ARROW_72]
        .into_iter()
        .map(|id| child(&ui, bar, id))
        .max_by_key(|h| box_of(&ui, *h).y0)
        .expect("two arrows");
    assert_ne!(top, bottom);

    ui.broadcast_element_message(top, msgid::BUTTON_HOT_CLICK, action::PRIMARY_CLICK, 0);
    assert_eq!(
        offset(&mut ui, log),
        parked - LINE,
        "the top arrow walks the log back"
    );
    ui.broadcast_element_message(bottom, msgid::BUTTON_HOT_CLICK, action::PRIMARY_CLICK, 0);
    assert_eq!(
        offset(&mut ui, log),
        parked,
        "and the bottom arrow walks it on"
    );
}

// ---------------------------------------------------------------------------------------------
// 3. The smooth movement
// ---------------------------------------------------------------------------------------------

/// Behaviour: ui.scrollbar.a-goal-position-glides-over-its-duration
///
/// **Starting an animation and running it in the global loop: attribute `0x85` with `0x83` set
/// glides to the goal over `0x84` seconds and unregisters from global message 3 when it arrives.**
///
/// Two screens write `0x85`: the profession page's `0x12`/`0x44` arm (a number typed into an
/// attribute box, clamped into `0.1 ..= 1.0`) and the combat panel. Without the arm their thumbs
/// would not move at all.
#[test]
fn a_goal_position_glides_over_its_duration_and_stops_registering_when_it_arrives() {
    let mut ui = UiSystem::new((800, 600));
    let (_c, _log, bar) = tree(&mut ui);
    let mut pump = NullInputPump;
    ui.set_attribute_bool(bar, bar_attr::SMOOTH_MOVEMENT, true);
    ui.set_attribute_float(bar, bar_attr::ANIM_DURATION, 2.0);
    ui.set_attribute_float(bar, bar_attr::POSITION, 0.0);

    ui.use_time(LocalTime(10.0), &mut pump);
    ui.set_attribute_float(bar, bar_attr::ANIM_TARGET, 1.0);
    let pos = |ui: &UiSystem| {
        ui.node(bar)
            .expect("alive")
            .merged_properties()
            .get_float(bar_attr::POSITION)
            .unwrap_or(-1.0)
    };
    assert!(
        (pos(&ui) - 0.0).abs() < 1e-6,
        "writing the goal does not jump the position"
    );

    ui.use_time(LocalTime(10.5), &mut pump);
    assert!(
        (pos(&ui) - 0.25).abs() < 1e-4,
        "a quarter of the way in, a quarter of the way there"
    );
    ui.use_time(LocalTime(11.0), &mut pump);
    assert!((pos(&ui) - 0.5).abs() < 1e-4, "half");

    // The bar is registered for message 3 for as long as it is animating, and **only** for that
    // long: the global loop clears flag bit `0x400000` *and* unregisters from global message 3.
    // The two halves are separable and the second is invisible
    // from behaviour — the bit alone stops the position moving — which is why the listener table
    // is read directly here, through `UiSystem::global_message_listeners`.
    let listening = |ui: &UiSystem| {
        ui.global_message_listeners(dereth_ui::msg::global::TICK)
            .contains(&dereth_ui::ListenerId::Element(bar))
    };
    assert!(
        listening(&ui),
        "a bar mid-glide is registered for the frame tick"
    );

    // Just past the end — 12.5 s against a start of 10.0 and a duration of 2.0, so the raw ratio
    // is 1.25. The sample is deliberately *close* to the boundary: `t` is clamped at 1.0 and the
    // animation ends there, and a run that only ever sampled far past the end would pass on a
    // build whose clamp was at any threshold at all.
    ui.use_time(LocalTime(12.5), &mut pump);
    assert!(
        (pos(&ui) - 1.0).abs() < 1e-6,
        "and it stops at the goal rather than overshooting"
    );
    ui.set_attribute_float(bar, bar_attr::POSITION, 0.3);
    // The unregistration was queued rather than applied:
    // defers while a broadcast is in flight and drains
    // the queue at the **top of the next `use_time`**, which is the client's only protection
    // against a listener removing itself inside its own callback. So the drop-out is observable
    // one frame later, and asserting it on the same frame would be asserting against the client.
    ui.use_time(LocalTime(13.0), &mut pump);
    assert!(
        !listening(&ui),
        "and drops out of the tick table on the frame after it arrives"
    );
    assert!(
        (pos(&ui) - 0.3).abs() < 1e-6,
        "the finished animation no longer writes the position"
    );
}

/// The two ways `start_animation` declines, both of which are the client's and both of which look
/// identical on screen to a broken animation.
///
/// * **smooth movement clear** — the attribute handler's `0x85` case writes `0x86` outright.
/// * **no `0x84`** — the duration read is a presence test as well as a value, and there is no
///   default, so a bar with smooth movement and no duration does nothing at all.
#[test]
fn without_smooth_movement_the_goal_is_a_jump_and_without_a_duration_it_is_nothing() {
    let pos = |ui: &UiSystem, bar: ElemHandle| {
        ui.node(bar)
            .expect("alive")
            .merged_properties()
            .get_float(bar_attr::POSITION)
            .unwrap_or(-1.0)
    };

    // No 0x83: the goal is applied immediately.
    let mut ui = UiSystem::new((800, 600));
    let (_c, _log, bar) = tree(&mut ui);
    ui.set_attribute_float(bar, bar_attr::ANIM_DURATION, 2.0);
    ui.set_attribute_float(bar, bar_attr::POSITION, 0.0);
    ui.set_attribute_float(bar, bar_attr::ANIM_TARGET, 0.75);
    assert!(
        (pos(&ui, bar) - 0.75).abs() < 1e-6,
        "0x85 without 0x83 is a jump"
    );

    // 0x83 but no 0x84: nothing happens, and it keeps not happening on later frames.
    let mut ui = UiSystem::new((800, 600));
    let (_c, _log, bar) = tree(&mut ui);
    let mut pump = NullInputPump;
    ui.set_attribute_bool(bar, bar_attr::SMOOTH_MOVEMENT, true);
    ui.set_attribute_float(bar, bar_attr::POSITION, 0.0);
    ui.use_time(LocalTime(1.0), &mut pump);
    ui.set_attribute_float(bar, bar_attr::ANIM_TARGET, 0.75);
    ui.use_time(LocalTime(2.0), &mut pump);
    ui.use_time(LocalTime(9.0), &mut pump);
    assert!(
        (pos(&ui, bar) - 0.0).abs() < 1e-6,
        "no 0x84 duration, no animation at all"
    );
}
