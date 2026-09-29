//! The Game/Support page carries seven buttons in layout order; two carry input actions; either
//! support button raises the support-URL request; Restore Defaults restores every option page;
//! exit-to-character-select still asks; non-message-1 is not a press.
//! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

use crate::common::layout::RegistrationOrder;

use dereth_ui::framework::Screen;
use dereth_ui::msg::Delivery;
use dereth_ui::{ElemHandle, ElementId, MessageId, UiSystem};

use dereth_ui_screens::options::gameplay::{button, GameplayOptionsPage, BUTTONS, LAYOUT_DRIVEN};
use dereth_ui_screens::options::pages::{GameplayOptionAction, SUPPORT_URL};
use dereth_ui_screens::screens::gameplay::GamePlayScreen;
use dereth_ui_screens::UiRequest;

fn env() -> UiSystem {
    let (ui, _flow, _store) =
        crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
    ui
}

/// Screen.
fn screen() -> (UiSystem, GamePlayScreen) {
    let mut ui = env();
    let mut s = GamePlayScreen::default();
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the gameplay screen builds");
    (ui, s)
}

/// Deliver the tree's element messages to the screen the way `UiShell::frame` does.
fn pump(ui: &mut UiSystem, s: &mut GamePlayScreen) -> usize {
    let mut n = 0;
    for d in ui.drain_outbox() {
        if let Delivery::Element { msg, .. } = d {
            s.on_element_message(&mut dereth_ui::framework::ScreenCx::new(ui), &msg);
            n += 1;
        }
    }
    n
}

/// The `0x10000212` page handle, with the type checked as the client's type-checked cast would.
fn page(ui: &UiSystem) -> ElemHandle {
    let all: Vec<ElemHandle> = ui
        .element_list()
        .iter()
        .copied()
        .filter(|h| {
            ui.node(*h)
                .is_some_and(|n| n.ty() == dereth_ui_screens::element_types::ty::GAMEPLAY_OPTIONS)
        })
        .collect();
    assert_eq!(
        all.len(),
        1,
        "exactly one gameplay-options panel in the shipped tree"
    );
    let h = all[0];
    assert_eq!(
        ui.node(h).expect("alive").element_id(),
        dereth_ui_screens::options::gameplay::PAGE_ELEMENT,
        "the page's element id"
    );
    h
}

/// Show the page and every ancestor, so its buttons are hit-testable. The options page is one of
/// sixteen stacked pages and starts hidden — a panel-visibility notice is what
/// shows it in the client. (Copied from `options_client_page.rs`, which needs the same thing.)
fn show(ui: &mut UiSystem, h: ElemHandle) {
    let mut cur = h;
    loop {
        ui.set_visible(cur, true);
        match ui.parent(cur) {
            Some(p) => cur = p,
            None => break,
        }
    }
    ui.drain_outbox();
}

/// A real click in the middle of `id`'s screen box, and the requests it queued.
fn click(ui: &mut UiSystem, s: &mut GamePlayScreen, id: ElementId) -> Vec<UiRequest> {
    let p = page(ui);
    let h = ui
        .get_child_recursive(p, id)
        .unwrap_or_else(|| panic!("{:#010X} is a child of the shipped Game/Support page", id.0));
    show(ui, h);
    let b = ui.node(h).expect("the button").region.box_;
    let (ox, oy) = ui.screen_origin(h);
    let (x, y) = (ox + b.width() / 2, oy + b.height() / 2);
    ui.requests.clear();
    ui.mouse_down(7, x, y);
    ui.mouse_up(7, x, y, false);
    pump(ui, s);
    ui.requests.take()
}

// -------------------------------------------------------------------------------------------
// The shipped page
// -------------------------------------------------------------------------------------------

/// The shipped page carries the seven buttons in layout order.
#[test]
fn the_shipped_page_carries_the_seven_buttons_in_layout_order() {
    let (ui, _s) = screen();
    let p = page(&ui);
    let kids: Vec<(u32, u32, i32)> = ui
        .children(p)
        .iter()
        .map(|h| {
            let n = ui.node(*h).expect("alive");
            (n.element_id().0, n.ty().0, n.original_position().y0)
        })
        .collect();
    assert_eq!(kids.len(), 7, "the page's direct children: {kids:?}");
    for (i, (id, y)) in BUTTONS.iter().enumerate() {
        assert_eq!(kids[i].0, id.0, "child {i} is {:#010X}", kids[i].0);
        assert_eq!(kids[i].1, 1, "{:#010X} is a button element", id.0);
        assert_eq!(kids[i].2, *y, "{:#010X} sits at y {y}", id.0);
    }
}

/// The two buttons with no class arm carry their input action instead.
#[test]
fn the_two_buttons_with_no_class_arm_carry_their_input_action_instead() {
    let (ui, _s) = screen();
    let p = page(&ui);
    for (id, action) in LAYOUT_DRIVEN {
        let h = ui
            .get_child_recursive(p, id)
            .expect("the button is in the tree");
        assert_eq!(
            ui.node(h)
                .expect("alive")
                .merged_properties()
                .get_enum(dereth_ui::props::attr::BUTTON_INPUT_ACTION),
            Some(action),
            "{:#010X} fires input action {action:#010X}",
            id.0
        );
        assert_eq!(
            dereth_ui_screens::options::pages::gameplay_option_action(id),
            None,
            "{:#010X} has no class arm",
            id.0
        );
    }
    // The converse: none of the five answered ids carries one, so no button is driven twice.
    for (id, _) in BUTTONS {
        if LAYOUT_DRIVEN.iter().any(|(l, _)| *l == id) {
            continue;
        }
        let h = ui
            .get_child_recursive(p, id)
            .expect("the button is in the tree");
        assert_eq!(
            ui.node(h)
                .expect("alive")
                .merged_properties()
                .get_enum(dereth_ui::props::attr::BUTTON_INPUT_ACTION),
            None,
            "{:#010X} is answered by the class and must not also fire an action",
            id.0
        );
    }
}

// -------------------------------------------------------------------------------------------
// The two arms that reached nothing
// -------------------------------------------------------------------------------------------

/// Behaviour: options.support.pressing-either-support-button-opens-the-page-in-the-players-browser
/// Clicking either support button raises the support url request.
#[test]
fn clicking_either_support_button_raises_the_support_url_request() {
    let (mut ui, mut s) = screen();
    for id in [button::SUPPORT_TICKET_UPPER, button::SUPPORT_TICKET_LOWER] {
        let rs = click(&mut ui, &mut s, id);
        assert!(
            rs.iter()
                .any(|r| matches!(r, UiRequest::OpenUrl(u) if *u == SUPPORT_URL)),
            "{:#010X} must raise OpenUrl({SUPPORT_URL}); got {rs:?}",
            id.0
        );
    }
    // The URL is the client's own literal, not a re-typed one.
    assert_eq!(
        SUPPORT_URL,
        "http://support.turbine.com/ics/support/ticketnewwizard.asp?style=classic"
    );
}

/// Behaviour: options.gameplay-page.restore-defaults-restores-every-option-pages-defaults
/// **The second red.** `0x100005CC` broadcasts global message `0x0C`, and `0x0C`
/// is `RESTORE_DEFAULTS` — every client options page is registered for it. Nothing in this
/// build reached it.
///
/// The assertion is on the **writes**, not on a counter: a restore must put `SetPreference` on the
/// queue for the Client Options page's rows, which is the same production path each page's own
/// *Defaults* button takes (`GamePlayScreen::on_config_page_button`).
#[test]
fn clicking_restore_defaults_restores_every_option_pages_defaults() {
    let (mut ui, mut s) = screen();
    let rows = s.config_page.options.len();
    assert!(
        rows > 20,
        "the Client Options page built {rows} rows; option initialization builds 27"
    );

    let rs = click(&mut ui, &mut s, button::RESTORE_DEFAULTS);
    let prefs: Vec<&str> = rs
        .iter()
        .filter_map(|r| match r {
            UiRequest::SetPreference(name, _) => Some(*name),
            _ => None,
        })
        .collect();
    assert!(
        prefs.len() >= rows,
        "Restore Defaults must write every one of the {rows} Client Options rows; wrote \
         {} ({prefs:?})",
        prefs.len()
    );
    assert!(
        prefs.contains(&"Sound.SoundVolume"),
        "the sound volume row is one of them; got {prefs:?}"
    );
}

/// The read-back half: after the press, the control the player reopens shows the default, not the
/// value that was there. `restore_default_values` writes `current` from `default` and refreshes the
/// element, so the page's own array is the read-back the client would show.
#[test]
fn restore_defaults_is_visible_when_the_client_options_page_is_read_back() {
    let (mut ui, mut s) = screen();
    // Move one row away from its default first, so the restore has something to undo.
    let i = s
        .config_page
        .options
        .iter()
        .position(|o| o.preference == "Sound.SoundVolume")
        .expect("the sound volume row");
    let default = s.config_page.options[i].default.clone();
    s.config_page.options[i].current = dereth_ui_screens::PrefValue::Float(0.125);
    assert_ne!(
        s.config_page.options[i].current, default,
        "the row is off its default"
    );

    click(&mut ui, &mut s, button::RESTORE_DEFAULTS);

    assert_eq!(
        s.config_page.options[i].current, default,
        "the reopened row reads back the default"
    );
    assert_eq!(
        dereth_ui_screens::options::store::inq_value("Sound.SoundVolume"),
        Some(default),
        "and so does the preference registry the page wrote through"
    );
}

// -------------------------------------------------------------------------------------------
// Controls
// -------------------------------------------------------------------------------------------

/// Clicking exit to character selection still asks.
#[test]
fn clicking_exit_to_character_selection_still_asks() {
    let (mut ui, mut s) = screen();
    click(&mut ui, &mut s, button::EXIT_TO_CHARACTER_SELECTION);
    assert!(
        s.logout_dialog().is_some(),
        "the asking form of EndCharacterSession raises the confirmation dialog"
    );
}

/// The handler answers element message **1** and
/// nothing else. A page that answered every message would act on a rollover.
#[test]
fn a_message_that_is_not_element_message_1_is_not_a_press() {
    let (ui, _s) = screen();
    let p = page(&ui);
    let h = ui
        .get_child_recursive(p, button::SUPPORT_TICKET_UPPER)
        .expect("the button");
    let bound = GameplayOptionsPage::bind(&ui, ui.element_list()[0]);
    let msg = |id: MessageId| dereth_ui::msg::ElementMessage {
        source_id: button::SUPPORT_TICKET_UPPER,
        source: h,
        id,
        p1: 0,
        p2: 0,
        point: dereth_ui::msg::MessagePoint::default(),
        serial: 0,
    };
    assert!(
        bound.on_element_message(&ui, &msg(MessageId(1))).is_some(),
        "message 1 is a press"
    );
    for other in [0x0A_u32, 0x15, 0x18, 0x19] {
        assert_eq!(
            bound.on_element_message(&ui, &msg(MessageId(other))),
            None,
            "message {other:#X} is not a press"
        );
    }
    // And the action the press resolves to is the one the bytes give.
    assert_eq!(
        bound.on_element_message(&ui, &msg(MessageId(1))),
        Some(GameplayOptionAction::Request(UiRequest::OpenUrl(
            SUPPORT_URL
        )))
    );
}
