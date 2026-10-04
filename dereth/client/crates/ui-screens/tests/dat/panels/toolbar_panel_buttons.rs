//! The seven toolbar panel buttons draw a lit picture and stop their own click; opening a panel
//! over another unlights the first one's button; the notice alone moves the button both ways; a
//! notice for no button moves none.
//! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

use crate::common::layout::RegistrationOrder;

use dereth_primitives::DataId;
use dereth_ui::{Delivery, ElemHandle, Screen, StateId, UiSystem};
use dereth_ui_screens::screens::gameplay::GamePlayScreen;
use dereth_ui_screens::toolbar::{STATE_PANEL_CLOSED, STATE_PANEL_OPEN};

/// Every fixture path here is an `expect`, never a skip — the stated testability rule says *"a test that skips is
/// a test that passes"*.
fn env() -> UiSystem {
    let (ui, _flow, _store) =
        crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
    ui
}

fn screen() -> (UiSystem, GamePlayScreen) {
    let mut ui = env();
    let mut s = GamePlayScreen::default();
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the gameplay screen builds from the shipped layout");
    // `UiShell::frame` ends in `UiFlow::deliver`. `setup_children`'s trailing "hide every
    // registered page" loop raises `0x18` while the screen is being built, so flush that backlog
    // here rather than let it arrive in the middle of the first click.
    pump(&mut ui, &mut s);
    ui.requests.clear();
    (ui, s)
}

/// `UiFlow::deliver`, bounded: a handler may raise more messages (`recv_set_panel_visibility` hides
/// the page it covers, and that hide raises another `0x18`).
fn pump(ui: &mut UiSystem, s: &mut GamePlayScreen) -> usize {
    let mut n = 0;
    for _ in 0..16 {
        let batch = ui.drain_outbox();
        if batch.is_empty() {
            break;
        }
        for d in batch {
            if let Delivery::Element { msg, .. } = d {
                s.on_element_message(&mut dereth_ui::framework::ScreenCx::new(ui), &msg);
                n += 1;
            }
        }
    }
    n
}

/// The `DataId` the layout's own `StateDesc` for `state` installs — the oracle, read out of the
/// shipped data and not out of anything this crate decided.
fn state_image(ui: &UiSystem, h: ElemHandle, state: StateId) -> DataId {
    use dereth_assets::ui::MediaFields;
    let n = ui.node(h).expect("alive");
    let sd = n
        .desc
        .access_state(state)
        .unwrap_or_else(|| panic!("the button declares state {:#X}", state.0));
    for m in &sd.media {
        if let MediaFields::Image { file, .. } = m.fields {
            return file;
        }
    }
    panic!("state {:#X} has no image media", state.0)
}

/// What the button is drawing: the region's image, which supplies the draw-list surface.
fn drawn(ui: &UiSystem, h: ElemHandle) -> Option<DataId> {
    ui.node(h)
        .expect("alive")
        .region
        .image
        .as_ref()
        .map(|g| g.did)
}

/// A real click on an element: move the pointer onto it (a toggle button only flips while
/// the pointer is over it), press, release, then deliver everything that came out.
fn click(ui: &mut UiSystem, s: &mut GamePlayScreen, h: ElemHandle) {
    let (ox, oy) = ui.screen_origin(h);
    let b = ui.node(h).expect("alive").region.box_;
    let (x, y) = (ox + b.width() / 2, oy + b.height() / 2);
    ui.mouse_move(dereth_primitives::LocalTime(1.0), x, y);
    pump(ui, s);
    ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, x, y);
    pump(ui, s);
    ui.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, x, y, false);
    pump(ui, s);
}

/// The seven buttons, as `(element id, panel id, handle)`, read off the live toolbar rather than
/// paired by hand.
fn buttons(_ui: &UiSystem, s: &GamePlayScreen) -> Vec<(u32, u32, ElemHandle)> {
    assert_eq!(
        s.toolbar.buttons.len(),
        7,
        "toolbar initialization binds seven panel buttons"
    );
    s.toolbar
        .buttons
        .iter()
        .map(|b| (b.element.0, b.panel_id, b.handle))
        .collect()
}

/// The whole toolbar's rendering, as `(element id, lit)`, where `lit` is decided by the picture.
fn toolbar_drawn(ui: &UiSystem, s: &GamePlayScreen) -> Vec<(u32, bool)> {
    buttons(ui, s)
        .into_iter()
        .map(|(id, _, h)| {
            let got = drawn(ui, h);
            let lit = state_image(ui, h, STATE_PANEL_OPEN);
            let unlit = state_image(ui, h, STATE_PANEL_CLOSED);
            assert!(
                got == Some(lit) || got == Some(unlit),
                "{id:#010X} draws {got:?}, which is neither its lit {lit:?} nor its unlit {unlit:?}"
            );
            (id, got == Some(lit))
        })
        .collect()
}

/// Which page the stack thinks is current — the *other* half, asserted separately.
fn current_panel(s: &GamePlayScreen) -> Option<u32> {
    let e = s.panels.current?;
    s.panels
        .pages
        .iter()
        .find(|p| p.element == e)
        .map(|p| p.panel_id)
}

/// **Calibration.** Prove the instrument can tell the two pictures
/// apart before believing any match. If a button's state `6` and state `1` installed the same
/// surface, every assertion in this file would pass over a toolbar that never changed.
#[test]
fn the_seven_buttons_draw_a_different_picture_when_lit() {
    // **The two numbers, as literals.** Everything else in this file reads them through the
    // symbols, and a test that reads a constant through the same symbol it writes it through
    // cannot detect a wrong constant under the stated testability rule. These are the two states
    // the toolbar's panel-visibility notice handler sets: 6 when the panel becomes visible and 1
    // when it stops being visible.
    assert_eq!(STATE_PANEL_OPEN, StateId(6));
    assert_eq!(STATE_PANEL_CLOSED, StateId(1));
    let (ui, s) = screen();
    for (id, panel, h) in buttons(&ui, &s) {
        assert_ne!(
            panel, 0,
            "{id:#010X} carries a real panel id in attribute 0x10000029"
        );
        let lit = state_image(&ui, h, STATE_PANEL_OPEN);
        let unlit = state_image(&ui, h, STATE_PANEL_CLOSED);
        assert_ne!(
            lit, unlit,
            "{id:#010X}: states 6 and 1 must not blit the same surface"
        );
        // And the button comes up drawing the unlit one, which is the "assert the negative case"
        // rule: a toolbar that is always lit passes a lit-only test.
        assert_eq!(drawn(&ui, h), Some(unlit), "{id:#010X} comes up unlit");
    }
    assert_eq!(
        current_panel(&s),
        None,
        "and no page is current before anything is clicked"
    );
}

/// The seven panel buttons stop their own click.
#[test]
fn the_seven_panel_buttons_stop_their_own_click() {
    use dereth_ui::props::attr::BUTTON_INPUT_ACTION;

    // The number as a literal, once, so a wrong constant cannot hide behind the symbol every
    // other line reads it through, as required by the stated testability rule.
    assert_eq!(BUTTON_INPUT_ACTION, 0x12);

    let (mut ui, mut s) = screen();
    for (id, _panel, h) in buttons(&ui, &s) {
        let action = ui
            .node(h)
            .expect("alive")
            .merged_properties()
            .get_enum(BUTTON_INPUT_ACTION);
        assert!(
            matches!(action, Some(a) if a != 1),
            "{id:#010X} must carry a live attribute 0x12, or handle_button_click returns false and \
             its message 1 bubbles to the screen again; it carries {action:?}"
        );
        assert_eq!(
            clicks_seen_by_the_screen(&mut ui, &mut s, h),
            0,
            "{id:#010X}'s message 1 must stop on the widget"
        );
    }

    // ---- the negative control: an element with no 0x12 does reach the screen ------------------
    let exit = ui
        .get_child_recursive(
            s.root().expect("the screen root"),
            dereth_ui::ElementId(0x1000_00FA),
        )
        .expect("the lamp row's log-out button 0x100000FA is in the shipped tree");
    assert_eq!(
        ui.node(exit)
            .expect("alive")
            .merged_properties()
            .get_enum(BUTTON_INPUT_ACTION),
        None,
        "0x100000FA carries no input action -- that is why the indicators panel hears its click"
    );
    assert_eq!(
        clicks_seen_by_the_screen(&mut ui, &mut s, exit),
        1,
        "and the probe can see a BUTTON_CLICKED when one is really broadcast"
    );
}

/// One real click, counting only the `BUTTON_CLICKED` deliveries **for that element** that reach
/// a listener this crate does not own — which is the screen. The batch is handed on to the screen
/// afterwards, so the click is not swallowed by the measurement.
fn clicks_seen_by_the_screen(ui: &mut UiSystem, s: &mut GamePlayScreen, h: ElemHandle) -> usize {
    let (ox, oy) = ui.screen_origin(h);
    let b = ui.node(h).expect("alive").region.box_;
    let (x, y) = (ox + b.width() / 2, oy + b.height() / 2);
    ui.mouse_move(dereth_primitives::LocalTime(1.0), x, y);
    pump(ui, s);
    ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, x, y);
    pump(ui, s);
    ui.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, x, y, false);
    let batch = ui.drain_outbox();
    let seen = batch
        .iter()
        .filter(|d| {
            matches!(d, Delivery::Element { msg, .. }
                if msg.id == dereth_ui::msg::element::id::BUTTON_CLICKED && msg.source == h)
        })
        .count();
    for d in &batch {
        if let Delivery::Element { msg, .. } = d {
            s.on_element_message(&mut dereth_ui::framework::ScreenCx::new(ui), msg);
        }
    }
    pump(ui, s);
    seen
}

/// Behaviour: toolbar.buttons.a-panel-opened-over-another-takes-the-covered-panels-button-down
/// A panel opened over another takes the first ones button down with it.
#[test]
fn a_panel_opened_over_another_takes_the_first_ones_button_down_with_it() {
    let (mut ui, mut s) = screen();
    let bs = buttons(&ui, &s);
    // Two different panels, taken off the live toolbar in post-init order. Nothing here names a
    // panel by hand.
    let (a_id, a_panel, a_h) = bs[0];
    let (b_id, b_panel, b_h) = bs[1];
    assert_ne!(a_panel, b_panel);

    // ---- station 0: nothing open ------------------------------------------------------------
    assert_eq!(current_panel(&s), None);
    assert_eq!(
        toolbar_drawn(&ui, &s)
            .iter()
            .filter(|(_, lit)| *lit)
            .count(),
        0,
        "station 0: no button is lit"
    );

    // ---- station A: open A ------------------------------------------------------------------
    click(&mut ui, &mut s, a_h);
    assert_eq!(
        current_panel(&s),
        Some(a_panel),
        "station A: A's page is current"
    );
    assert_eq!(
        toolbar_drawn(&ui, &s),
        bs.iter()
            .map(|(id, _, _)| (*id, *id == a_id))
            .collect::<Vec<_>>(),
        "station A: A's button lit and every other one dark"
    );

    // ---- station B: open B over A -- the reported defect --------------------------------------
    click(&mut ui, &mut s, b_h);
    assert_eq!(
        current_panel(&s),
        Some(b_panel),
        "station B: B's page replaced A's"
    );
    assert_eq!(
        toolbar_drawn(&ui, &s),
        bs.iter()
            .map(|(id, _, _)| (*id, *id == b_id))
            .collect::<Vec<_>>(),
        "station B: B's button lit and **A's button back down**"
    );

    // ---- station C: close B ------------------------------------------------------------------
    click(&mut ui, &mut s, b_h);
    assert_eq!(current_panel(&s), None, "station C: nothing is current");
    assert_eq!(
        toolbar_drawn(&ui, &s)
            .iter()
            .filter(|(_, lit)| *lit)
            .count(),
        0,
        "station C: every button is back down"
    );

    // ---- station D: open A again, to prove the edge runs in both directions twice -------------
    click(&mut ui, &mut s, a_h);
    assert_eq!(current_panel(&s), Some(a_panel), "station D: A again");
    assert_eq!(
        toolbar_drawn(&ui, &s),
        bs.iter()
            .map(|(id, _, _)| (*id, *id == a_id))
            .collect::<Vec<_>>(),
        "station D: A's button lit again"
    );
}

/// Behaviour: toolbar.buttons.the-panel-notice-alone-moves-the-button
/// The panel-visibility notice updates the button, so opening a panel through any other route
/// must update its toolbar button too. Driven with no click at all.
#[test]
fn the_notice_alone_moves_the_button_in_both_directions() {
    let (mut ui, mut s) = screen();
    let bs = buttons(&ui, &s);
    let (a_id, a_panel, a_h) = bs[0];
    let (b_id, b_panel, b_h) = bs[1];

    assert_eq!(
        drawn(&ui, a_h),
        Some(state_image(&ui, a_h, STATE_PANEL_CLOSED))
    );

    s.recv_set_panel_visibility(&mut ui, a_panel, true);
    pump(&mut ui, &mut s);
    assert_eq!(current_panel(&s), Some(a_panel));
    assert_eq!(
        toolbar_drawn(&ui, &s),
        bs.iter()
            .map(|(id, _, _)| (*id, *id == a_id))
            .collect::<Vec<_>>(),
        "the notice lit A"
    );

    s.recv_set_panel_visibility(&mut ui, b_panel, true);
    pump(&mut ui, &mut s);
    assert_eq!(current_panel(&s), Some(b_panel));
    assert_eq!(
        toolbar_drawn(&ui, &s),
        bs.iter()
            .map(|(id, _, _)| (*id, *id == b_id))
            .collect::<Vec<_>>(),
        "the panel wrapper's re-sent notice for the covered page took A's button down"
    );
    let _ = b_h;

    s.recv_set_panel_visibility(&mut ui, b_panel, false);
    pump(&mut ui, &mut s);
    assert_eq!(current_panel(&s), None);
    assert_eq!(
        toolbar_drawn(&ui, &s)
            .iter()
            .filter(|(_, lit)| *lit)
            .count(),
        0,
        "and hiding the last page leaves nothing lit"
    );
}

/// The visibility handler's loop matches on the panel id, so a notice for
/// a panel no button carries must move nothing. This is the negative case that stops
/// "every notice lights every button" passing the tests above.
#[test]
fn a_notice_for_a_panel_no_button_carries_moves_no_button() {
    let (mut ui, mut s) = screen();
    let before = toolbar_drawn(&ui, &s);
    // Panel id 2 is `0x10000182`'s -- a page in the stack that no toolbar button carries.
    let orphan = 2;
    assert!(
        s.panels.pages.iter().any(|p| p.panel_id == orphan),
        "the page exists, so this is a notice the stack acts on"
    );
    assert!(
        !s.toolbar.buttons.iter().any(|b| b.panel_id == orphan),
        "and no toolbar button carries it"
    );
    assert_eq!(
        s.toolbar.on_set_panel_visibility(&mut ui, orphan, true),
        None
    );
    s.recv_set_panel_visibility(&mut ui, orphan, true);
    pump(&mut ui, &mut s);
    assert_eq!(current_panel(&s), Some(orphan), "the page did open");
    assert_eq!(toolbar_drawn(&ui, &s), before, "and not one button moved");
}
