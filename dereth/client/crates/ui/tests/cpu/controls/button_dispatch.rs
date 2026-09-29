//! A button's click dispatch and the modes of attribute `0x58`. A button carrying a live `0x12`
//! dispatches its input action and stops its message 1 from bubbling, even when no element listens
//! for the action; a button with no `0x12`, or with action 1 (do nothing), lets it bubble; a
//! disabled button swallows its own click and fires nothing. `0x58` is **1 toggle, 2 show, 3
//! hide**. A panel announces its own visibility (`0x18`) before its open page announces its.
//!
//! Every shipped toolbar panel button carries `0x12` naming its page's `0x57`, and the page's
//! `0x58` is 1, so on the shipped tree the action and an ancestor that toggled on every click would
//! reach the same page and a double dispatch would look like a single one. This file therefore
//! builds its own tree in which an ancestor listener **disagrees** with the action, so running it
//! once and running it twice are different pictures, and asserts which page is up and which state
//! the button is in, separately. The shipped toolbar's drawn buttons are covered in
//! `dereth-ui-screens`. No dats.

use crate::common::NoAssets;
use dereth_primitives::{DataId, LocalTime};
use dereth_ui::desc::{incorporation, ElementDesc, LayoutDesc, StateDesc};
use dereth_ui::element::ElementMessageListenResult as R;
use dereth_ui::factory::ty;
use dereth_ui::focus::action;
use dereth_ui::msg::element::id as msgid;
use dereth_ui::{ElemHandle, ElementId, ElementType, PropertyValue, StateId, UiSystem};

// ---------------------------------------------------------------------------------------------
// harness
// ---------------------------------------------------------------------------------------------

const CONTAINER: u32 = 0x1000_0010;
const BUTTON: u32 = 0x1000_0011;
const PAGE: u32 = 0x1000_0012;
/// One `InputAction` id out of the shipped range; nothing here depends on which.
const ACTION: u32 = 0x1000_000D;
/// A second action that **no element registers for**. It exists because of a survivor: see
/// [`a_live_0x12_stops_the_message_even_when_the_action_reaches_nobody`].
const ACTION_NOBODY_LISTENS_FOR: u32 = 0x1000_000E;

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

/// **The ancestor that disagrees.** It sends *the opposite of what it last heard*, and it is an
/// **external** listener registered on the button's
/// parent, which is exactly what `GamePlayScreen` is (`ListenerId::External`, registered on the
/// gameplay root by `register_for_element_messages`). So this is the real arrangement, not a
/// model of it.
///
/// It also counts, so *"the message did not reach a second handler"* is asserted as a number with
/// a denominator rather than as the absence of an effect. A handler that fires and happens to
/// agree with the action is indistinguishable from one that never ran; this one cannot agree.
const SCREEN: dereth_ui::ListenerId = dereth_ui::ListenerId::External(0x1000_0006);

struct Tree {
    ui: UiSystem,
    button: ElemHandle,
    page: ElemHandle,
    /// Message 1s from the button that reached the ancestor.
    heard: u32,
    /// What the ancestor believes about the page — deliberately its own copy, as the toolbar's
    /// `currently_visible` is.
    believes_visible: bool,
}

impl Tree {
    /// `UiFlow::deliver`: hand the screen everything the broadcast queued for it, and let it run
    /// its disagreeing arm. Bounded, because a handler may raise more messages.
    fn pump(&mut self) {
        for _ in 0..16 {
            let batch = self.ui.drain_outbox();
            if batch.is_empty() {
                break;
            }
            for d in batch {
                let dereth_ui::Delivery::Element { to, msg } = d else {
                    continue;
                };
                if to != SCREEN
                    || msg.id != msgid::BUTTON_CLICKED
                    || msg.source_id != ElementId(BUTTON)
                {
                    continue;
                }
                self.heard += 1;
                self.believes_visible = !self.believes_visible;
                self.ui.set_visible(self.page, self.believes_visible);
            }
        }
    }
}

/// A container holding a toggle button and a page, wired the way the shipped toolbar is: the
/// button's `0x12` **is** the page's `0x57`, as it is for all 7 shipped panel buttons.
fn tree(button_action: Option<u32>, disabled: bool) -> Tree {
    let mut ui = UiSystem::new((800, 600));
    let mut container = desc(CONTAINER, ty::FIELD, 0, 0, 400, 200);

    let mut button = desc(BUTTON, ty::BUTTON, 0, 0, 40, 20);
    if let Some(a) = button_action {
        button.base.properties.set(
            dereth_ui::props::attr::BUTTON_INPUT_ACTION,
            PropertyValue::Enum(a),
        );
    }
    // The shipped panel buttons all carry `0x0B UICore_Button_toggleButton = true` and declare
    // exactly states 1, 3 and 6 (measured on `classic_gameplay`). Declared here so
    // the state-desc lookup guard lets the state move at all.
    button.base.properties.set(
        dereth_ui::props::attr::TOGGLE_BUTTON,
        PropertyValue::Bool(true),
    );
    for s in [1_u32, 3, 6, 0x0D] {
        button.states.insert(
            StateId(s),
            StateDesc {
                state_id: StateId(s),
                ..StateDesc::default()
            },
        );
    }

    let mut page = desc(PAGE, ty::FIELD, 100, 0, 40, 20);
    page.base.properties.set(
        dereth_ui::props::attr::INPUT_ACTION,
        PropertyValue::Enum(ACTION),
    );
    // **`1`.** All 41 shipped `0x57` listeners carry it; re-measured over the 101 layouts.
    page.base.properties.set(
        dereth_ui::props::attr::VISIBILITY_TOGGLE_MODE,
        PropertyValue::Enum(1),
    );

    container.children.insert(ElementId(BUTTON), button);
    container.children.insert(ElementId(PAGE), page);
    let l = LayoutDesc {
        did: DataId(0x2100_0001),
        display_width: 800,
        display_height: 600,
        elements: std::iter::once((ElementId(CONTAINER), container)).collect(),
    };
    let d = l
        .access_element(ElementId(CONTAINER))
        .cloned()
        .expect("root");
    let c = ui
        .create_element_recursive_from_full_desc(&NoAssets, &l, &d)
        .expect("no inheritance")
        .expect("registered");
    let root = ui.root();
    ui.set_parent(c, Some(root));
    let button = ui.get_child(c, ElementId(BUTTON)).expect("the button");
    let page = ui.get_child(c, ElementId(PAGE)).expect("the page");
    // Initialisation is what dispatches the description's properties through `on_set_attribute`,
    // and therefore what runs for the page's `0x57`.
    ui.initialize_tree(c);
    if disabled {
        // the route the character-management panel's button refresh takes, and the only one that
        // survives initialisation — see
        // [`a_disabled_button_swallows_its_own_click_and_fires_nothing`].
        ui.set_state(button, StateId(0x0D));
    }
    ui.set_visible(page, false);

    ui.register_for_element_messages(c, SCREEN);
    ui.drain_outbox();
    Tree {
        ui,
        button,
        page,
        heard: 0,
        believes_visible: false,
    }
}

/// A real click: the pointer onto the element, press, release. A toggle button only flips while
/// the pointer is over the button itself, so the move is not decoration.
fn click(t: &mut Tree) {
    let (ox, oy) = t.ui.screen_origin(t.button);
    let b = t.ui.node(t.button).expect("alive").region.box_;
    let (x, y) = (ox + b.width() / 2, oy + b.height() / 2);
    t.ui.mouse_move(LocalTime(1.0), x, y);
    t.ui.mouse_down(action::PRIMARY_CLICK, x, y);
    t.ui.mouse_up(action::PRIMARY_CLICK, x, y, false);
    t.pump();
}

fn page_up(t: &Tree) -> bool {
    t.ui.node(t.page).expect("alive").region.flags.visible
}

fn button_state(t: &Tree) -> StateId {
    t.ui.node(t.button).expect("alive").state
}

// ---------------------------------------------------------------------------------------------
// a click opens and closes the page once
// ---------------------------------------------------------------------------------------------

/// Behaviour: ui.button.a-click-on-a-button-with-an-action-stops-bubbling
/// Behaviour: ui.panel.the-first-visibility-mode-toggles-the-page
///
/// **Two stations, both halves asserted at each, over an element whose ancestor disagrees.**
///
/// Station A: one click opens the page. Station B: a second click closes it. The ancestor arm
/// would flip the page on every click as well; if it ran, station B would read *open*.
///
/// The halves are asserted separately: **which page is up**
/// (the model) and **which state the button is in** (the picture). Two errors that cancel show up
/// as agreement between a wrong page and a wrong button, so asserting only the composition would
/// miss them.
#[test]
fn a_live_0x12_stops_the_message_and_the_page_toggles_at_two_stations() {
    let mut t = tree(Some(ACTION), false);

    // ---- station 0 --------------------------------------------------------------------------
    assert!(!page_up(&t), "station 0: the page is down");
    assert_eq!(
        button_state(&t),
        StateId(1),
        "station 0: the button is unlit"
    );
    assert_eq!(t.heard, 0, "station 0: nothing has been clicked");

    // ---- station A: one click ---------------------------------------------------------------
    click(&mut t);
    assert!(page_up(&t), "station A: `0x58 = 1` toggled the page up");
    assert_eq!(
        button_state(&t),
        StateId(6),
        "station A: and the button is lit"
    );
    assert_eq!(
        t.heard, 0,
        "station A: handle_button_click returned true, so the button's listener returned \
         StopProcessing and the ancestor never saw message 1"
    );

    // ---- station B: the second click, where a second dispatch would disagree ---------------
    click(&mut t);
    assert!(
        !page_up(&t),
        "station B: the same action toggled it back down"
    );
    assert_eq!(
        button_state(&t),
        StateId(1),
        "station B: and the button is back down"
    );
    assert_eq!(t.heard, 0, "station B: still nothing bubbled");
}

/// Behaviour: ui.button.a-click-on-a-button-with-an-action-stops-bubbling
///
/// **`StopProcessing` isolated, which the test above cannot do.**
///
/// Returning `R::Default` instead of `R::StopProcessing` passes the test above, because
/// `handle_button_click` runs
/// `key_press`, whose tail raises element message `0x31` on
/// the page — a **nested** broadcast, inside the message-1 broadcast still in flight. It takes the
/// next serial and stamps every ancestor, so when message 1 resumes bubbling `claim_serial` refuses
/// it at each one and it dies **whatever** this function returned: on any button whose action
/// reaches a `0x57` listener, the action's own broadcast eats the rest of the bubble.
///
/// So the discriminating case is a button whose `0x12` names an action **no element is registered
/// for**: `dispatch_input_action` finds no bucket, raises no element message, stamps nothing — and
/// then only `StopProcessing` can stop the message. `handle_button_click` still answers
/// **true** there, because its four refusals are the missing attribute, action 1, and the two null
/// singletons; whether anybody *listens* is not one of them.
#[test]
fn a_live_0x12_stops_the_message_even_when_the_action_reaches_nobody() {
    let mut t = tree(Some(ACTION_NOBODY_LISTENS_FOR), false);
    // The premise, asserted rather than assumed: nothing is registered for this action, so no
    // `0x31` is raised and no serial is stamped on the way past the ancestor.
    assert!(
        !t.ui.dispatch_input_action(ACTION_NOBODY_LISTENS_FOR),
        "the action has no bucket -- the toggle action returns false and broadcasts nothing"
    );
    assert!(!page_up(&t), "and the page is down");

    click(&mut t);
    assert_eq!(
        t.heard, 0,
        "StopProcessing alone stopped it: nothing else could have, because the action raised no \
         message"
    );
    assert!(
        !page_up(&t),
        "and the ancestor's own arm, which would have opened the page, did not run"
    );

    // Twice, because a handler that runs once and then stops is a different bug.
    click(&mut t);
    assert_eq!(t.heard, 0);
    assert!(!page_up(&t));
}

/// **The other direction, without which the test above proves nothing.**
///
/// The button's own message listener stops the message only when
/// `handle_button_click` answered *true*. A button with **no** `0x12` falls through to
/// the text element's listener underneath it, and its click
/// reaches the ancestor, which is how `IndicatorStrip`'s log-out button (`0x100000FA`, no `0x12`)
/// and every screen-level click arm work.
///
/// Without this case, "the ancestor heard nothing" would also pass on a button that swallows
/// everything, which is a different and worse defect.
#[test]
fn a_button_with_no_0x12_still_bubbles_to_its_ancestor() {
    let mut t = tree(None, false);
    click(&mut t);
    assert_eq!(t.heard, 1, "no 0x12: the message bubbled, once");
    assert!(
        page_up(&t),
        "and the ancestor's own arm ran instead of the action"
    );

    click(&mut t);
    assert_eq!(t.heard, 2);
    assert!(!page_up(&t));
}

/// `0x12 == 1` is the client's *do nothing* action and is refused **before** the action map is
/// consulted, so `handle_button_click` answers false
/// and the message bubbles exactly as if the attribute were absent.
#[test]
fn action_one_is_refused_and_the_message_bubbles() {
    let mut t = tree(Some(1), false);
    click(&mut t);
    assert_eq!(
        t.heard, 1,
        "action 1 fires nothing and does not stop the message"
    );
    assert!(page_up(&t), "the ancestor ran");
}

/// **The disabled arm, which a click cannot reach.**
///
/// The disabled-state check returns 2 immediately, so a disabled button
/// **swallows its own message 1 without calling `handle_button_click` at all**; the message does
/// not go on bubbling.
///
/// A *click* cannot get there: the button's mouse-up reads `0x0D` and `0x0C`
/// before it decides, and broadcasts message 1 with `(7, 0)` from itself only when the button is
/// enabled (or carries `0x0C UICore_Button_clickWhileDisabled`) and was not hot-clicking. So the
/// arm is exercised the way the client's own broadcast of message 1 with `(7, 0)` would be,
/// **and the click route is asserted separately** to prove the two are different questions rather
/// than to paper over one of them.
///
/// The disabled bit is written with `set_state(0x0D)` after initialisation, because the layout-time
/// route does not survive it: the button's state setter, in its `0x0D` arm, answers a
/// desc that ships `Disabled = true` by writing the attribute **false** and refusing the state,
/// so a button disabled in the layout comes up enabled.
#[test]
fn a_disabled_button_swallows_its_own_click_and_fires_nothing() {
    let mut t = tree(Some(ACTION), true);
    assert_eq!(
        button_state(&t),
        StateId(0x0D),
        "set_state(0x0D) disabled it"
    );

    // The click route: a mouse-up on a disabled button raises nothing at all.
    click(&mut t);
    assert!(!page_up(&t), "no action fired");
    assert_eq!(
        t.heard, 0,
        "and a disabled button raises no message 1 from a click"
    );

    // The arm itself, driven the way the mouse-up would drive it on an enabled button.
    t.ui.broadcast_element_message(t.button, msgid::BUTTON_CLICKED, 7, 0);
    t.pump();
    assert!(
        !page_up(&t),
        "the disabled arm does not call handle_button_click"
    );
    assert_eq!(
        t.heard, 0,
        "and it still returns StopProcessing, so nothing bubbled"
    );

    // And the click did not move the toggle either: the mouse-up flips `0x0E` *inside*
    // the pointer-over-the-button-and-not-disabled test.
    assert_eq!(
        t.ui.node(t.button)
            .unwrap()
            .merged_properties()
            .get_bool(dereth_ui::props::attr::TOGGLED),
        None,
        "a disabled toggle button does not change position under a click"
    );

    // Both directions on the same button: enable it and the identical message does both things.
    t.ui.set_state(t.button, StateId(1));
    assert_eq!(
        button_state(&t),
        StateId(1),
        "set_state(1) cleared 0x0D and re-ran update_state"
    );
    t.ui.broadcast_element_message(t.button, msgid::BUTTON_CLICKED, 7, 0);
    t.pump();
    assert!(page_up(&t), "enabled: the action fired");
    assert_eq!(
        t.heard, 0,
        "and it still stopped, because handle_button_click answered true"
    );
}

/// **The literals.** Everything above reads these through symbols, and a test that reads a
/// constant through the same symbol it writes it through cannot detect a wrong constant.
/// Each is paired with what it decides.
#[test]
fn the_button_attributes_and_messages_have_their_shipped_numbers() {
    // the button's input action is attribute `0x12`.
    assert_eq!(dereth_ui::props::attr::BUTTON_INPUT_ACTION, 0x12);
    // the disabled bit is attribute `0x0D`.
    assert_eq!(dereth_ui::props::attr::DISABLED, 0x0D);
    // the visibility-toggle mode is attribute `0x58`.
    assert_eq!(dereth_ui::props::attr::VISIBILITY_TOGGLE_MODE, 0x58);
    // the input action an element registers for is attribute `0x57`.
    assert_eq!(dereth_ui::props::attr::INPUT_ACTION, 0x57);
    // the click a button raises is element message **1**.
    assert_eq!(msgid::BUTTON_CLICKED, dereth_ui::MessageId(1));
    // `StopProcessing` is the literal 2.
    assert_eq!(
        R::StopProcessing as u32,
        2,
        "the listener's stop-processing result"
    );
}

// ---------------------------------------------------------------------------------------------
// a `Panel` announces its own visibility before its page's
// ---------------------------------------------------------------------------------------------

/// **A `Panel` (element type `0x00000008`) announces its own visibility (`0x18`), and then its
/// open page announces its.**
///
/// The panel's own message listener, in its from-self `0x18` arm, puts its open tab
/// page back up (it finds the open page among its descendants and makes it visible).
/// Making an element visible itself raises `0x18`, so doing that *inside* the `0x18` broadcast
/// nests one: the inner message takes the next serial and stamps every ancestor, and when the
/// outer one resumes bubbling `claim_serial` refuses it at each, unless the panel's own
/// announcement is delivered first.
///
/// `PanelStack::on_page_visibility_changed` on the gameplay screen is the only thing that tells the
/// panel stack which page is current, and **six of `PanelStack`'s sixteen pages are this type**,
/// the six that six of the seven toolbar panel buttons open, so a lost announcement would stop
/// those buttons working.
///
/// Asserted as **both** messages arriving and **in order** — the panel's first, then the page's.
/// Asserting only "the panel's arrived" would pass on a build that dropped the inner one instead.
#[test]
fn a_panel_announces_its_own_visibility_before_its_page_announces_its() {
    let mut t = panel_tree();

    // Up: the panel's own 0x18, then the open page's.
    t.ui.set_visible(t.panel, true);
    assert_eq!(
        drained(&mut t.ui),
        vec![(PANEL, 1), (PANEL_PAGE, 1)],
        "showing the panel announces the panel and then the page it re-opens"
    );

    // Down: the same, in the same order — the arm's other half, over the same element.
    t.ui.set_visible(t.panel, false);
    assert_eq!(
        drained(&mut t.ui),
        vec![(PANEL, 0), (PANEL_PAGE, 0)],
        "hiding it takes every page down and announces both"
    );

    // And a page that is not a panel is unaffected, which is the control: five of the sixteen
    // shipped pages are ordinary elements and always announced correctly.
    t.ui.set_visible(t.plain, true);
    assert_eq!(drained(&mut t.ui), vec![(PLAIN, 1)]);
}

const OUTER: u32 = 0x1000_0020;
const PANEL: u32 = 0x1000_0021;
const PANEL_TAB: u32 = 0x1000_0022;
const PANEL_PAGE: u32 = 0x1000_0023;
const PLAIN: u32 = 0x1000_0024;

struct PanelTree {
    ui: UiSystem,
    panel: ElemHandle,
    plain: ElemHandle,
}

/// Every `0x18` that reached the ancestor since the last call, as `(element id, p1)`.
fn drained(ui: &mut UiSystem) -> Vec<(u32, u32)> {
    ui.drain_outbox()
        .into_iter()
        .filter_map(|d| match d {
            dereth_ui::Delivery::Element { msg, .. } if msg.id == msgid::VISIBILITY_CHANGED => {
                Some((msg.source_id.0, msg.p1))
            }
            _ => None,
        })
        .collect()
}

/// A container holding a `Panel` with one tab page, and a plain sibling page as the
/// control. The screen listens on the container, as `GamePlayScreen` listens on the gameplay root.
fn panel_tree() -> PanelTree {
    use dereth_assets::ui::BaseProperty as Property;
    use dereth_ui::widgets::panel::attr as pattr;
    use dereth_ui::PropertyValue as P;

    let mut ui = UiSystem::new((800, 600));
    let mut outer = desc(OUTER, ty::FIELD, 0, 0, 400, 200);
    let mut pnl = desc(PANEL, ty::PANEL, 0, 0, 200, 100);
    let row = P::Struct(vec![
        (
            pattr::TAB_ELEMENT,
            Property {
                id: pattr::TAB_ELEMENT,
                value: P::Enum(PANEL_TAB),
            },
        ),
        (
            pattr::PAGE_ELEMENT,
            Property {
                id: pattr::PAGE_ELEMENT,
                value: P::Enum(PANEL_PAGE),
            },
        ),
        (
            pattr::PAGE_OPEN,
            Property {
                id: pattr::PAGE_OPEN,
                value: P::Bool(true),
            },
        ),
    ]);
    pnl.base.properties.set(
        pattr::PAGES,
        P::Array(vec![Property {
            id: pattr::PAGE_DATA,
            value: row,
        }]),
    );
    pnl.children.insert(
        ElementId(PANEL_TAB),
        desc(PANEL_TAB, ty::TEXT, 0, 0, 20, 10),
    );
    pnl.children.insert(
        ElementId(PANEL_PAGE),
        desc(PANEL_PAGE, ty::FIELD, 0, 20, 100, 50),
    );
    outer.children.insert(ElementId(PANEL), pnl);
    outer
        .children
        .insert(ElementId(PLAIN), desc(PLAIN, ty::FIELD, 200, 0, 100, 50));

    let l = LayoutDesc {
        did: DataId(0x2100_0002),
        display_width: 800,
        display_height: 600,
        elements: std::iter::once((ElementId(OUTER), outer)).collect(),
    };
    let d = l.access_element(ElementId(OUTER)).cloned().expect("root");
    let c = ui
        .create_element_recursive_from_full_desc(&NoAssets, &l, &d)
        .expect("no inheritance")
        .expect("registered");
    let root = ui.root();
    ui.set_parent(c, Some(root));
    ui.initialize_tree(c);
    let panel = ui.get_child(c, ElementId(PANEL)).expect("the panel");
    let plain = ui.get_child(c, ElementId(PLAIN)).expect("the plain page");
    // the panel's own child setup ends in a hide-all loop.
    ui.set_visible(panel, false);
    ui.set_visible(plain, false);
    ui.register_for_element_messages(c, SCREEN);
    ui.drain_outbox();
    PanelTree { ui, panel, plain }
}
