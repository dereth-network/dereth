//! The Game/Support page carries seven buttons in layout order; two carry input actions; either
//! support button raises the support-URL request; Use Mouse Turning Settings sets the mouse-turning
//! preset and nothing else;
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
    // The converse: none of the three answered ids carries one, so no button is driven twice.
    // The two support buttons are given their forms' actions instead of a class arm.
    for (id, _) in BUTTONS {
        if LAYOUT_DRIVEN.iter().any(|(l, _)| *l == id)
            || matches!(
                id,
                button::SUPPORT_TICKET_UPPER | button::SUPPORT_TICKET_LOWER
            )
        {
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

/// Behaviour: options.support.each-support-button-opens-its-in-game-form
/// Urgent Assistance and Report Abuse open the in-game forms, through the forms' own input
/// actions, and open no web page; In-Game Help is hidden and the two close up over its place.
#[test]
fn each_support_button_opens_its_in_game_form_and_help_is_hidden() {
    use dereth_ui_screens::options::gameplay::{REPORT_ABUSE_ACTION, URGENT_ASSISTANCE_ACTION};
    let (mut ui, mut s) = screen();
    let p = page(&ui);
    for (id, action) in [
        (button::SUPPORT_TICKET_UPPER, URGENT_ASSISTANCE_ACTION),
        (button::SUPPORT_TICKET_LOWER, REPORT_ABUSE_ACTION),
    ] {
        let h = ui.get_child_recursive(p, id).expect("the button");
        assert_eq!(
            ui.node(h)
                .expect("alive")
                .merged_properties()
                .get_enum(dereth_ui::props::attr::BUTTON_INPUT_ACTION),
            Some(action),
            "{:#010X} fires its form's action",
            id.0
        );
        let rs = click(&mut ui, &mut s, id);
        assert!(
            !rs.iter().any(|r| matches!(r, UiRequest::OpenUrl(_))),
            "{:#010X} opens no web page: {rs:?}",
            id.0
        );
    }
    let help = ui.get_child_recursive(p, button::HELP).expect("the button");
    assert!(!ui.is_visible(help), "In-Game Help is hidden");
    let y = |ui: &UiSystem, id: ElementId| {
        let h = ui.get_child_recursive(p, id).expect("the button");
        ui.node(h).expect("alive").region.box_.y0
    };
    assert_eq!(y(&ui, button::SUPPORT_TICKET_UPPER), 200);
    assert_eq!(y(&ui, button::SUPPORT_TICKET_LOWER), 240);
    // The address retail opened is gone; nothing here opens it.
    assert!(SUPPORT_URL.starts_with("http://support.turbine.com/"));
}

/// Behaviour: options.gameplay-page.mouse-turning-settings-sets-the-preset-and-nothing-else
/// `0x100005CC` broadcasts global message `0x0C`, whose listeners set the mouse-turning preset.
/// The press writes the six preset preferences that were off their preset values, prints one
/// chat line for each, and writes nothing else: no other Client Options row, and no character
/// option.
#[test]
fn use_mouse_turning_settings_sets_the_preset_and_nothing_else() {
    use dereth_ui_screens::options::config::{MOUSE_TURNING_CHANNEL, MOUSE_TURNING_PRESET};
    use dereth_ui_screens::PrefValue;
    let (mut ui, mut s) = screen();
    let rows = s.config_page.options.len();
    assert!(rows > 20, "the Client Options page built {rows} rows");
    // Every preset row starts off its preset value.
    for row in MOUSE_TURNING_PRESET {
        let i = s
            .config_page
            .options
            .iter()
            .position(|o| o.preference == row.preference)
            .unwrap_or_else(|| panic!("{} is on the page", row.preference));
        s.config_page.options[i].current = match row.value {
            dereth_ui_screens::options::config::PrefValueConst::Bool(b) => PrefValue::Bool(!b),
            dereth_ui_screens::options::config::PrefValueConst::Float(f) => {
                PrefValue::Float(f / 2.0)
            }
            dereth_ui_screens::options::config::PrefValueConst::Int(n) => PrefValue::Int(n + 1),
        };
    }

    let rs = click(&mut ui, &mut s, button::MOUSE_TURNING_SETTINGS);
    let prefs: Vec<(&str, PrefValue)> = rs
        .iter()
        .filter_map(|r| match r {
            UiRequest::SetPreference(name, v) => Some((*name, v.clone())),
            _ => None,
        })
        .collect();
    let want: Vec<(&str, PrefValue)> = MOUSE_TURNING_PRESET
        .iter()
        .map(|r| (r.preference, r.value.into()))
        .collect();
    assert_eq!(prefs, want, "the six preset rows and nothing else");
    assert!(
        !rs.iter().any(|r| matches!(
            r,
            UiRequest::SetPlayerOption(..) | UiRequest::SetOptionWords { .. }
        )),
        "no character option is written: {rs:?}"
    );
    let lines: Vec<&str> = rs
        .iter()
        .filter_map(|r| match r {
            UiRequest::DisplayChatText { channel, text } if *channel == MOUSE_TURNING_CHANNEL => {
                Some(text.as_str())
            }
            _ => None,
        })
        .collect();
    assert_eq!(lines.len(), 6, "a line per row moved: {lines:?}");
    assert_eq!(
        lines[0],
        "Camera Stiffness was changed from 0.475000 to the mouse turning default of 0.950000."
    );
    for row in MOUSE_TURNING_PRESET {
        assert_eq!(
            dereth_ui_screens::options::store::inq_value(row.preference),
            Some(row.value.into()),
            "{} holds the preset",
            row.preference
        );
    }

    // A second press finds every row at its preset value: nothing written, nothing printed.
    let again = click(&mut ui, &mut s, button::MOUSE_TURNING_SETTINGS);
    assert!(
        !again.iter().any(|r| matches!(
            r,
            UiRequest::SetPreference(..) | UiRequest::DisplayChatText { .. }
        )),
        "{again:?}"
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
        .get_child_recursive(p, button::EXIT_TO_CHARACTER_SELECTION)
        .expect("the button");
    let bound = GameplayOptionsPage::bind(&ui, ui.element_list()[0]);
    let msg = |id: MessageId| dereth_ui::msg::ElementMessage {
        source_id: button::EXIT_TO_CHARACTER_SELECTION,
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
        Some(GameplayOptionAction::Request(
            UiRequest::EndCharacterSession { ask: true }
        ))
    );
}
