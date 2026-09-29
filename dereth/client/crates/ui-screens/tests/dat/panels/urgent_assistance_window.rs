//! The urgent-assistance window is in the shipped tree and opens from an input action; its three
//! pages switch by its own state; post-init binds two children; the wizard walks warning to form to
//! confirmation; empty send does nothing; typing enables send; cancel resets.
//! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

use crate::common::layout::RegistrationOrder;

use dereth_ui::{Delivery, ElemHandle, ElementId, Screen, StateId, UiSystem};
use dereth_ui_screens::panels::remaining::RemainingPanels;
use dereth_ui_screens::panels::urgent_assistance as ua;
use dereth_ui_screens::screens::gameplay::GamePlayScreen;
use dereth_ui_screens::view::EmptyGameView;
use dereth_ui_screens::UiRequest;

fn env() -> UiSystem {
    let (ui, _flow, _store) =
        crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
    ui
}

/// The shipped gameplay screen with `RemainingPanels` bound off its root, exactly as
/// `dereth_client::hud::Hud::drive` binds it.
fn screen() -> (UiSystem, GamePlayScreen, RemainingPanels) {
    let mut ui = env();
    let mut s = GamePlayScreen::default();
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the gameplay screen builds");
    let root = s.root().expect("the gameplay root");
    let mut panels = RemainingPanels::default();
    panels.post_init(&mut ui, root);
    ui.drain_outbox();
    ui.requests.clear();
    (ui, s, panels)
}

/// One host frame's worth of element dispatch, to the panels holder.
fn pump(ui: &mut UiSystem, panels: &mut RemainingPanels) -> usize {
    let view = EmptyGameView;
    let mut n = 0;
    for _ in 0..16 {
        let batch = ui.drain_outbox();
        if batch.is_empty() {
            break;
        }
        for d in batch {
            if let Delivery::Element { msg, .. } = d {
                panels.on_element_message(ui, &msg, &view);
                n += 1;
            }
        }
    }
    n
}

fn panel(panels: &RemainingPanels) -> ElemHandle {
    panels
        .urgent_assistance
        .panel
        .expect("UrgentAssistancePanel is in the shipped tree")
}

/// Show `h` and every ancestor — the panel pages start hidden and a hidden ancestor is unhittable.
fn reveal(ui: &mut UiSystem, mut h: ElemHandle) {
    loop {
        ui.set_visible(h, true);
        match ui.parent(h) {
            Some(p) => h = p,
            None => break,
        }
    }
    ui.drain_outbox();
}

/// A real click in the middle of `id`'s screen box, dispatched to the panels holder.
fn click(ui: &mut UiSystem, panels: &mut RemainingPanels, id: ElementId) {
    let p = panel(panels);
    let h = ui
        .get_child_recursive(p, id)
        .unwrap_or_else(|| panic!("{:#010X} is a child of the urgent-assistance window", id.0));
    reveal(ui, h);
    let b = ui.node(h).expect("the button").region.box_;
    let (ox, oy) = ui.screen_origin(h);
    let (x, y) = (ox + b.width() / 2, oy + b.height() / 2);
    ui.mouse_down(7, x, y);
    ui.mouse_up(7, x, y, false);
    pump(ui, panels);
}

/// Put text in the entry box the way the generic `TextElement` does when the player types, then
/// raise the `0x44` the element raises. The panel's own arm owns only the decision, not the edit —
/// see `panels/abuse.rs`, which says the same thing about its sibling window.
fn type_into_box(ui: &mut UiSystem, panels: &mut RemainingPanels, text: &str) {
    let p = panel(panels);
    let h = ui.get_child_recursive(p, ua::ENTRY_BOX).expect("entry box");
    ui.text_element_mut(h)
        .expect("the entry box is a TextElement")
        .set_text(text);
    ui.broadcast_element_message(h, dereth_ui::msg::element::id::TEXT_CHANGED, 0, 0);
    pump(ui, panels);
}

fn state(ui: &UiSystem, h: ElemHandle) -> Option<StateId> {
    ui.node(h).map(|n| n.state)
}

fn button_state(ui: &UiSystem, panels: &RemainingPanels, id: ElementId) -> Option<StateId> {
    let p = panel(panels);
    ui.get_child_recursive(p, id).and_then(|h| state(ui, h))
}

// -------------------------------------------------------------------------------------------
// The shipped window
// -------------------------------------------------------------------------------------------

/// **The premise correction, asserted.** The card said an *inbound* message opens this window and
/// that a PSR hail has nowhere to land. There is no inbound message: the page carries the shipped
/// input action `ToggleUrgentAssistancePanel 0x1000000B` in attribute `0x57`, its close button
/// carries the same action in `BUTTON_INPUT_ACTION` `0x12`, and the page is
/// `catalogue::PANEL_PAGES[11]` — a toolbar panel like every other.
#[test]
fn the_window_is_in_the_shipped_tree_and_opens_from_an_input_action() {
    let (ui, _s, panels) = screen();
    let p = panel(&panels);
    let n = ui.node(p).expect("live");
    assert_eq!(n.ty(), ua::PANEL_TYPE, "the type-checked cast would fail");
    assert_eq!(n.element_id(), ua::PANEL);
    assert_eq!(
        n.merged_properties()
            .get_enum(dereth_ui::props::attr::INPUT_ACTION),
        Some(ua::TOGGLE_ACTION),
        "the window registers for ToggleUrgentAssistancePanel"
    );
    let close = ui
        .get_child_recursive(p, ua::CLOSE_BUTTON)
        .expect("the title bar's close button");
    assert_eq!(
        ui.node(close)
            .expect("live")
            .merged_properties()
            .get_enum(dereth_ui::props::attr::BUTTON_INPUT_ACTION),
        Some(ua::TOGGLE_ACTION),
        "the close button toggles the same panel"
    );
    assert!(
        dereth_ui_screens::panels::catalogue::PANEL_PAGES.contains(&ua::PANEL.0),
        "{:#010X} is one of PanelStack's sixteen pages",
        ua::PANEL.0
    );
    // And it really is the only one of its type, which is what makes the type-bind safe.
    let all = ui.element_list();
    let n = all
        .iter()
        .filter(|h| ui.node(**h).is_some_and(|n| n.ty() == ua::PANEL_TYPE))
        .count();
    assert_eq!(
        n, 1,
        "exactly one UrgentAssistancePanel in the shipped tree"
    );
}

/// The three page containers and their per-state `HIDE` flag. This is the table that makes
/// `SetState(0x1000000E/0F/10)` the whole of the page switching; without it the client's three
/// `SetState` calls would be decoration.
#[test]
fn the_three_pages_are_switched_by_the_windows_own_state() {
    let (ui, _s, panels) = screen();
    let p = panel(&panels);
    for (st, container) in ua::PAGE_CONTAINERS {
        let h = ui
            .get_child_recursive(p, container)
            .expect("the page container");
        for (other, _) in ua::PAGE_CONTAINERS {
            let hide = ui
                .node(h)
                .expect("live")
                .merged_properties_for(other)
                .get_bool(dereth_ui::props::attr::HIDE);
            assert_eq!(
                hide,
                Some(other != st),
                "{:#010X} in state {:#010X}",
                container.0,
                other.0
            );
        }
    }
}

/// The window's post-init makes two descendant lookups, which bind exactly the two
/// `ChildBinding`s `panels/catalogue.rs`'s `URGENT` table already carried and nothing consumed.
#[test]
fn post_init_binds_the_two_children_the_catalogue_names() {
    let (ui, _s, panels) = screen();
    assert!(
        panels.urgent_assistance.bound(),
        "the window and both children are bound"
    );
    let spec = dereth_ui_screens::panels::catalogue::spec("UrgentAssistancePanel")
        .expect("the catalogue row");
    let ids: Vec<u32> = spec.children.iter().map(|c| c.id.0).collect();
    assert_eq!(ids, vec![ua::ENTRY_BOX.0, ua::CONTINUE_BUTTON.0]);
    let p = panel(&panels);
    for id in [ua::ENTRY_BOX, ua::CONTINUE_BUTTON] {
        assert!(ui.get_child_recursive(p, id).is_some(), "{:#010X}", id.0);
    }
}

// -------------------------------------------------------------------------------------------
// The wizard
// -------------------------------------------------------------------------------------------

/// Behaviour: urgent-assistance.window.walks-warning-form-confirmation-and-send-needs-text
/// **The red.** Page one's *Continue* enters page two; a typed report and *Send* raise
/// a channel-broadcast request carrying channel `0x400` and the typed text, then enter page three.
///
/// `0x400` is `Channel::Help`, and this window is the client's **only** producer of it —
/// the channel-command handler excludes `0x400` by value, which is what makes `@help` a help
/// command. ACE answers it: `GameActionChatChannel.cs`'s `case Channel.Help`.
#[test]
fn the_wizard_walks_from_the_warning_to_the_form_to_the_confirmation() {
    let (mut ui, _s, mut panels) = screen();
    let p = panel(&panels);
    ui.set_state(p, ua::PAGE_WARNING);

    click(&mut ui, &mut panels, ua::NEXT_BUTTON);
    assert_eq!(
        state(&ui, p),
        Some(ua::PAGE_FORM),
        "Continue must enter the entry form"
    );

    ui.requests.clear();
    type_into_box(&mut ui, &mut panels, "stuck in a wall at 33.2N 45.1E");
    click(&mut ui, &mut panels, ua::CONTINUE_BUTTON);

    let out = ui.requests.take();
    assert!(
        out.iter().any(|r| matches!(
            r,
            UiRequest::ChannelBroadcast { channel, text }
                if *channel == ua::HELP_CHANNEL && text == "stuck in a wall at 33.2N 45.1E"
        )),
        "Send must broadcast the report on the Help channel {:#06X}; got {out:?}",
        ua::HELP_CHANNEL
    );
    assert_eq!(panels.urgent_assistance.requests_sent, 1);
    assert_eq!(
        state(&ui, p),
        Some(ua::PAGE_SENT),
        "Send must enter the confirmation page"
    );
}

/// The empty-box branch — an empty box sends nothing **and does not change the page**. Both
/// halves, because a build that advanced anyway would look right and post a blank help request.
#[test]
fn send_with_an_empty_box_does_nothing_at_all() {
    let (mut ui, _s, mut panels) = screen();
    let p = panel(&panels);
    click(&mut ui, &mut panels, ua::NEXT_BUTTON);
    assert_eq!(state(&ui, p), Some(ua::PAGE_FORM));

    ui.requests.clear();
    click(&mut ui, &mut panels, ua::CONTINUE_BUTTON);
    assert!(ui.requests.take().is_empty(), "an empty report is not sent");
    assert_eq!(panels.urgent_assistance.requests_sent, 0);
    assert_eq!(
        state(&ui, p),
        Some(ua::PAGE_FORM),
        "and the page does not advance"
    );
}

/// The handler's tail, both directions: Send's state is set from whether the box has any
/// length, state `1` or `0x0D` on the continue button. One direction alone passes
/// on a button that is always enabled.
#[test]
fn typing_enables_send_and_clearing_disables_it_again() {
    let (mut ui, _s, mut panels) = screen();
    click(&mut ui, &mut panels, ua::NEXT_BUTTON);

    type_into_box(&mut ui, &mut panels, "help");
    assert_eq!(
        button_state(&ui, &panels, ua::CONTINUE_BUTTON),
        Some(dereth_ui::widgets::button::state::NORMAL),
        "a non-empty box enables Send"
    );
    type_into_box(&mut ui, &mut panels, "");
    assert_eq!(
        button_state(&ui, &panels, ua::CONTINUE_BUTTON),
        Some(dereth_ui::widgets::button::state::DISABLED),
        "an empty box disables it again"
    );
}

/// The cancel arm — `SetVisible(false)`, `SetText("")`, `SetState(0x0D)` on Send and
/// `SetState(page one)` on the window, in that order.
///
/// This is the part that matters on the **next** open: a cancelled report must not still be in the
/// box when the player presses the toolbar button again.
#[test]
fn cancel_hides_the_window_empties_the_box_and_returns_to_page_one() {
    let (mut ui, _s, mut panels) = screen();
    let p = panel(&panels);
    click(&mut ui, &mut panels, ua::NEXT_BUTTON);
    type_into_box(&mut ui, &mut panels, "never mind");

    click(&mut ui, &mut panels, ua::CANCEL_BUTTON);

    assert!(!ui.is_visible(p), "the window is hidden");
    assert_eq!(
        state(&ui, p),
        Some(ua::PAGE_WARNING),
        "and back at page one"
    );
    let box_h = ui.get_child_recursive(p, ua::ENTRY_BOX).expect("entry box");
    assert_eq!(
        ui.text_element_mut(box_h)
            .expect("a text element")
            .glyphs
            .inq_text(false),
        "",
        "the report is gone"
    );
    assert_eq!(
        button_state(&ui, &panels, ua::CONTINUE_BUTTON),
        Some(dereth_ui::widgets::button::state::DISABLED),
        "and Send is disabled again"
    );
}
