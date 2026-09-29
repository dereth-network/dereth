//! The talk-focus menu builds thirteen real rows in client order with shipped ids; disabled rows
//! are in state 13; empty menu does not open; popup is 2x7 above the button; choosing a row raises
//! message 7 and moves focus; squelch row asks the shard; the sweep clears a distant target.
//! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

use crate::common::layout::RegistrationOrder;

use dereth_primitives::DataId;
use dereth_ui::framework::Screen;
use dereth_ui::{ElemHandle, ElementId, UiSystem};

use dereth_ui_screens::chat::mainchat::{
    AutoTarget, AutoTargetWorld, MenuRowChoice, SpeakableTarget, STATE_DISABLED, STATE_ENABLED,
    STATE_SELECTED,
};
use dereth_ui_screens::screens::gameplay::GamePlayScreen;
use dereth_ui_screens::view::UiRequest;

// ---------------------------------------------------------------------------------------------
// The five ids builds the menu out of, **as literals**.
//
// The stated testability rule: *"a test that reads a constant through the same symbol it writes it through
// cannot detect a wrong constant"*. Every one of these is repeated as a symbol somewhere in the
// crate; these are the copies that came off the shipped `client_local_English.dat` and off
// retail, and they are what makes a transcription slip visible.
// ---------------------------------------------------------------------------------------------

/// The talk-focus menu itself, which the chat UI looks up by this id as it builds it.
const MENU: ElementId = ElementId(0x1000_0014);
/// The menu's attribute **7** and attribute **10** — `make_popup`'s and `insert_text_item`'s layout.
const POPUP_LAYOUT: DataId = DataId(0x2100_0006);
/// The menu's attribute **6** — the popup root creates.
const POPUP_ROOT: ElementId = ElementId(0x1000_001C);
/// The menu's attribute **2** — the list-box element inside the popup.
const POPUP_LIST: ElementId = ElementId(0x1000_001D);
/// The menu's attribute **9** — the text element every one of the fourteen rows is built from.
const ROW_TEMPLATE: ElementId = ElementId(0x1000_001E);

/// The thirteen values of enum attribute `0x1000000b`, in the order the fourteen text items
/// are added — monarch, selected, patron, all,
/// vassals, fellows, allegiance, then the six Turbine channels.
const ADD_ORDER: [u32; 13] = [5, 2, 4, 1, 6, 3, 7, 8, 9, 10, 11, 12, 13];

/// The attribute a focus row is tagged with, and the one the squelch row deliberately lacks.
const ATTR_TALK_FOCUS: u32 = 0x1000_000B;

/// The three states the shipped row template `0x1000001E` declares, and nothing else.
const ROW_STATES: [u32; 3] = [0x1, 0x0D, 0x1000_0001];

/// The retail dat. An `expect`, not a skip — the stated testability rule says *"a test that skips is a test that
/// passes"*.
fn env() -> UiSystem {
    let (ui, _flow, _store) =
        crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
    ui
}

/// The shipped gameplay screen with `GamePlayScreen::create` run, which reaches
/// the chat UI's post-init and therefore the construction of the talk-focus menu.
fn screen() -> (UiSystem, GamePlayScreen) {
    let mut ui = env();
    let mut s = GamePlayScreen::default();
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the gameplay screen builds");
    ui.requests.clear();
    (ui, s)
}

fn attr_enum(ui: &UiSystem, h: ElemHandle, id: u32) -> Option<u32> {
    ui.node(h)?.merged_properties().get_enum(id)
}

fn state(ui: &UiSystem, h: ElemHandle) -> u32 {
    ui.node(h).expect("live row").state.0
}

// ---------------------------------------------------------------------------------------------

/// The thirteen rows are real elements in the clients own order.
#[test]
fn the_thirteen_rows_are_real_elements_in_the_clients_own_order() {
    let (ui, s) = screen();
    let rows = &s.main_chat.talk_focus_buttons;
    assert_eq!(rows.len(), 13, "talk-focus buttons after the menu is built");

    let ids: Vec<u32> = rows
        .iter()
        .map(|h| attr_enum(&ui, *h, ATTR_TALK_FOCUS).expect("0x1000000B"))
        .collect();
    assert_eq!(
        ids,
        ADD_ORDER.to_vec(),
        "the fourteen additions' order, not id order"
    );

    // Every one is a real, live text element built from the shipped template.
    for h in rows {
        let n = ui.node(*h).expect("live");
        assert_eq!(
            n.element_id(),
            ROW_TEMPLATE,
            "all fourteen share one description"
        );
        assert_eq!(n.ty(), dereth_ui::ElementType(0x0C), "text element");
        assert!(
            n.is_mouse_visible,
            "inserting a menu item sets it mouse-visible"
        );
    }

    // …and they are the rows of the menu's own list box, in the same order, with the squelch
    // toggle in front of them: fourteen items for thirteen focuses.
    let menu = s.main_chat.menu.expect("0x10000014");
    assert_eq!(
        dereth_ui::widgets::menu::num_items(&ui, menu),
        14,
        "fourteen items, not thirteen"
    );
    assert_eq!(
        dereth_ui::widgets::menu::get_item(&ui, menu, 0),
        s.main_chat.squelch_toggle
    );
    for (i, h) in rows.iter().enumerate() {
        assert_eq!(
            dereth_ui::widgets::menu::get_item(&ui, menu, i + 1),
            Some(*h),
            "row {i} of the talk-focus buttons is item {} of the item list",
            i + 1
        );
    }

    // The by-attribute lookup finds all thirteen, and it is the lookup
    // every other function in the main-chat panel goes through.
    for id in ADD_ORDER {
        assert!(
            s.main_chat.menu_item(&ui, id).is_some(),
            "the talk-focus menu item for {id}"
        );
    }
    assert_eq!(s.main_chat.menu_item(&ui, 99), None, "there is no focus 99");
}

/// The shipped menu names the popup the rows and the list by these exact ids.
#[test]
fn the_shipped_menu_names_the_popup_the_rows_and_the_list_by_these_exact_ids() {
    let (ui, s) = screen();
    let menu = s.main_chat.menu.expect("0x10000014");
    assert_eq!(ui.node(menu).expect("live").element_id(), MENU);
    assert_eq!(
        ui.node(menu).expect("live").ty(),
        dereth_ui::ElementType(6),
        "menu element"
    );

    let p = ui.node(menu).expect("live").merged_properties();
    assert_eq!(
        p.get_enum(2),
        Some(POPUP_LIST.0),
        "attribute 2 — the list box's id"
    );
    assert_eq!(
        p.get_enum(6),
        Some(POPUP_ROOT.0),
        "attribute 6 — the popup root"
    );
    assert_eq!(
        p.get_data_id(7),
        Some(POPUP_LAYOUT),
        "attribute 7 — the popup's layout"
    );
    assert_eq!(
        p.get_enum(9),
        Some(ROW_TEMPLATE.0),
        "attribute 9 — the row template"
    );
    assert_eq!(
        p.get_data_id(10),
        Some(POPUP_LAYOUT),
        "attribute 10 — the row's layout"
    );
    assert_eq!(
        p.get_bool(5),
        Some(true),
        "attribute 5 — the popup opens *above* the button"
    );
    assert_eq!(
        p.get_bool(3),
        Some(false),
        "attribute 3 — and is left-aligned, not centred"
    );

    let popup = dereth_ui::widgets::menu::popup_handle(&ui, menu).expect("popup");
    assert_eq!(ui.node(popup).expect("live").element_id(), POPUP_ROOT);
    assert_eq!(ui.node(popup).expect("live").layout_did, POPUP_LAYOUT);
    assert_ne!(
        ui.parent(popup),
        Some(menu),
        "the popup is a root element, not a child"
    );

    let list = dereth_ui::widgets::menu::list_box_handle(&ui, menu).expect("list box");
    assert_eq!(ui.node(list).expect("live").element_id(), POPUP_LIST);
    assert_eq!(
        ui.node(list).expect("live").ty(),
        dereth_ui::ElementType(5),
        "list-box element"
    );
    assert_eq!(
        ui.parent(list),
        Some(popup),
        "found as a descendant of the popup"
    );

    let row = s.main_chat.talk_focus_buttons[0];
    let mut declared: Vec<u32> = ui
        .node(row)
        .expect("live")
        .desc
        .states
        .keys()
        .map(|s| s.0)
        .collect();
    declared.sort_unstable();
    assert_eq!(
        declared,
        ROW_STATES.to_vec(),
        "0x1000001E's own state table"
    );
}

/// **The row's acceptance, as one assertion.** A row a player cannot pick is refused because the
/// **element** is in state `0x0D`, and for no other reason.
///
/// The selection handler's guard returns when the item is in state `0xD` — it never consults
/// whether the talk focus is enabled. Both directions are checked, including the one that separates
/// a state from a flag: with the mask saying *yes* and the element saying `0x0D`, the element wins.
#[test]
fn a_disabled_row_is_disabled_because_its_element_is_in_state_thirteen() {
    let (mut ui, mut s) = screen();
    let m = &mut s.main_chat;

    // Post-init's initial selection of row 1 has already run: exactly one row is selected, the
    // two `ChatState::new` enables are lit, and the other eleven are down.
    let lit: Vec<u32> = ADD_ORDER
        .iter()
        .copied()
        .filter(|id| state(&ui, m.menu_item(&ui, *id).expect("row")) != 0x0D)
        .collect();
    // Only row 1. `ChatState::new` enables 1 **and** 2, but runs
    // a clear of the selected target between building the menu and selecting row 1, and with no
    // speakable target that arm puts row 2 in state `0x0D` and disables talk focus 2.
    // So a freshly built chat window offers exactly one row, which is Say — and
    // that is what retail shows before anything is selected.
    assert_eq!(
        lit,
        vec![1],
        "1 alone: clearing the selection has just taken row 2 down"
    );
    assert_eq!(
        state(&ui, m.menu_item(&ui, 1).expect("Say")),
        0x1000_0001,
        "…and it is chosen"
    );
    assert!(m.is_talk_focus_enabled(1) && !m.is_talk_focus_enabled(2));

    let row8 = m.menu_item(&ui, 8).expect("General");
    assert_eq!(state(&ui, row8), 0x0D, "the literal the client pushes ");
    assert_eq!(state(&ui, row8), STATE_DISABLED.0);
    assert!(
        m.handle_selection(&mut ui, 8).is_none(),
        "state 0x0D refuses"
    );
    assert_eq!(m.talk_focus, 1, "…so the focus did not move");

    m.set_talk_focus_enabled(8, true);
    m.reset_all_talk_focus_menu_buttons(&mut ui);
    assert_eq!(
        state(&ui, row8),
        0x1,
        "the literal the client pushes for an enabled row"
    );
    assert_eq!(state(&ui, row8), STATE_ENABLED.0);
    assert!(m.handle_selection(&mut ui, 8).is_some(), "state 1 admits");
    assert_eq!(
        state(&ui, row8),
        0x1000_0001,
        "the chosen row, `push 0x10000001` "
    );
    assert_eq!(state(&ui, row8), STATE_SELECTED.0);

    // **The discriminating direction.** Make the mask and the element disagree — which is what
    // `enable_selection`'s Olthoi arm and `set_selected`'s row-2 arm both do — and the element wins.
    // A guard written against the mask would admit here, and a guard written against a *modelled*
    // state would not notice that the element had been driven behind its back.
    m.set_talk_focus_enabled(9, true);
    let row9 = m.menu_item(&ui, 9).expect("Trade");
    ui.set_state(row9, dereth_ui::StateId(0x0D));
    assert!(m.is_talk_focus_enabled(9), "the flag says yes");
    assert!(
        m.handle_selection(&mut ui, 9).is_none(),
        "and the element says no, which wins"
    );
    assert_eq!(m.talk_focus, 8, "…so the focus stayed where it was");
}

/// The menu adds `ID_Chat_SquelchSelectedNoSelection` **first** and gives it
/// no enum attribute `0x1000000b`, which is the whole reason
/// the by-attribute lookup cannot see it and the reason the chat UI's
/// element-message handler has to match it by pointer.
#[test]
fn the_squelch_row_is_row_zero_and_carries_no_focus_attribute() {
    let (ui, s) = screen();
    let menu = s.main_chat.menu.expect("0x10000014");
    let toggle = s.main_chat.squelch_toggle.expect("squelch toggle button");
    assert_eq!(
        dereth_ui::widgets::menu::get_item(&ui, menu, 0),
        Some(toggle),
        "row 0"
    );
    assert_eq!(
        attr_enum(&ui, toggle, ATTR_TALK_FOCUS),
        None,
        "and no 0x1000000B on it"
    );
    assert!(
        !s.main_chat.talk_focus_buttons.contains(&toggle),
        "it is never added to the talk-focus buttons"
    );
    for id in 1..=13u32 {
        assert_ne!(s.main_chat.menu_item(&ui, id), Some(toggle));
    }
}

/// **The live symptom, as an assertion.** A menu's `Open` returns without doing
/// anything when the item list is empty, so a menu with no rows opens nothing at all — which is
/// what a click on the Chat tab did in this build, and what the sweep photographed.
#[test]
fn a_menu_with_no_rows_does_not_open_and_one_with_fourteen_does() {
    let (mut ui, s) = screen();
    let menu = s.main_chat.menu.expect("0x10000014");
    let popup = dereth_ui::widgets::menu::popup_handle(&ui, menu).expect("popup");
    assert!(
        !ui.node(popup).expect("live").region.flags.visible,
        "building the popup leaves it hidden"
    );

    assert!(
        dereth_ui::widgets::menu::open(&mut ui, menu),
        "fourteen rows: it opens"
    );
    assert!(ui.node(popup).expect("live").region.flags.visible);
    assert!(
        !dereth_ui::widgets::menu::open(&mut ui, menu),
        "…and does not re-open while open"
    );
    assert!(dereth_ui::widgets::menu::close(&mut ui, menu));
    assert!(!ui.node(popup).expect("live").region.flags.visible);

    dereth_ui::widgets::menu::flush(&mut ui, menu);
    assert_eq!(dereth_ui::widgets::menu::num_items(&ui, menu), 0);
    assert!(
        !dereth_ui::widgets::menu::open(&mut ui, menu),
        "no rows: `Open` returns at its guard"
    );
    assert!(
        !ui.node(popup).expect("live").region.flags.visible,
        "and nothing is shown"
    );
}

/// Behaviour: chat.talk-to-menu.the-menu-is-real-rows-in-two-columns-that-opens-above-the-button
/// The popup's shape, which is entirely in the shipped data and is **two columns of seven**.
///
/// The list box `0x1000001D` carries `0x5C = true` (`horizontal layout`, so it fills
/// row-major) and `0x5F = 2` (`maximum columns`), and puts
/// fourteen 191x17 rows into a 382x119 grid. The popup's own resize then grows the
/// popup from its design 382x104 to 382x121 — the list's paper plus the 0x2 border
/// the list box latched at initialize — because the list is pinned at both ends on both axes.
///
/// Attribute **5** is `true`, so puts the popup **above** the button: the chat
/// window sits on the bottom edge of the screen and a menu opening downwards would be off it.
///
/// A failure here means the grid arithmetic or the popup resize has drifted, which is the
/// difference between fourteen readable rows and fourteen rows stacked on one line.
#[test]
fn the_popup_is_two_columns_of_seven_and_opens_above_the_button() {
    let (mut ui, s) = screen();
    let menu = s.main_chat.menu.expect("0x10000014");
    let list = dereth_ui::widgets::menu::list_box_handle(&ui, menu).expect("list box");
    let p = ui.node(list).expect("live").merged_properties();
    assert_eq!(
        p.get_bool(0x5C),
        Some(true),
        "horizontal layout — row-major"
    );
    assert_eq!(p.get_int(0x5F), Some(2), "maximum columns");
    let edges = ui.node(list).expect("live").desc.edges;
    use dereth_ui::layout::EdgeMode::AnchorStart;
    assert_eq!(
        (edges.left, edges.top, edges.right, edges.bottom),
        (AnchorStart, AnchorStart, AnchorStart, AnchorStart),
        "pinned at both ends on both axes, which is what lets the popup follow the paper"
    );

    assert_eq!(
        dereth_ui::widgets::menu::layout_items(&mut ui, menu),
        (2, 7),
        "columns, rows"
    );
    assert!(dereth_ui::widgets::menu::open(&mut ui, menu));

    let button = ui.screen_box(menu);
    let popup = ui.screen_box(dereth_ui::widgets::menu::popup_handle(&ui, menu).expect("popup"));
    assert_eq!(
        (popup.width(), popup.height()),
        (382, 121),
        "382x119 of rows plus the 0x2 border"
    );
    assert_eq!(
        popup.x0, button.x0,
        "attribute 3 is false: left-aligned, not centred"
    );
    assert_eq!(
        popup.y1 + 1,
        button.y0,
        "attribute 5 is true: it opens upwards"
    );

    // Fourteen rows, two per line, seven lines, none overlapping its neighbour.
    let rows: Vec<_> = (0..14)
        .map(|i| ui.screen_box(dereth_ui::widgets::menu::get_item(&ui, menu, i).expect("item")))
        .collect();
    for (i, b) in rows.iter().enumerate() {
        assert_eq!((b.width(), b.height()), (191, 17), "row {i}");
        let col = i % 2;
        let line = i / 2;
        assert_eq!(
            b.x0,
            popup.x0 + i32::try_from(col).unwrap() * 191,
            "row {i} column"
        );
        assert_eq!(
            b.y0,
            popup.y0 + 2 + i32::try_from(line).unwrap() * 17,
            "row {i} line"
        );
    }
}

/// Choosing a row raises seven with the row in p2 and moves the focus.
#[test]
fn choosing_a_row_raises_seven_with_the_row_in_p2_and_moves_the_focus() {
    assert_eq!(dereth_ui::msg::element::id::MENU_CHOSEN.0, 7);
    assert_eq!(dereth_ui::msg::element::id::MENU_OPENED.0, 8);
    assert_eq!(dereth_ui::msg::element::id::MENU_CLOSED.0, 9);

    let (mut ui, mut s) = screen();
    let m = &mut s.main_chat;
    let menu = m.menu.expect("0x10000014");
    // Row 5 (Monarch) has to be available before it can be picked, exactly as in the client.
    m.set_talk_focus_enabled(5, true);
    m.reset_all_talk_focus_menu_buttons(&mut ui);
    let row5 = m.menu_item(&ui, 5).expect("Monarch");

    // Watch for message 7 on the menu, as the main-chat panel does.
    ui.register_for_element_message(
        MENU,
        dereth_ui::msg::element::id::MENU_CHOSEN,
        dereth_ui::msg::ListenerId::External(139),
    );
    let _ = ui.drain_outbox();

    dereth_ui::widgets::menu::open(&mut ui, menu);
    {
        let b = ui.screen_box(row5);
        let (cx, cy) = ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2);
        assert_eq!(
            ui.hit_test_screen(cx, cy),
            Some(row5),
            "a menu row is mouse-visible, so the press lands on the row itself"
        );
        ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, cx, cy);
    }

    let seven: Vec<_> = ui
        .drain_outbox()
        .into_iter()
        .filter_map(|d| match d {
            // Only this test's own listener: `GamePlayScreen` is registered for the same message
            // and gets its own copy, which is the production path and is asserted below.
            dereth_ui::Delivery::Element { to, msg }
                if to == dereth_ui::msg::ListenerId::External(139)
                    && msg.id == dereth_ui::msg::element::id::MENU_CHOSEN =>
            {
                Some(msg)
            }
            _ => None,
        })
        .collect();
    assert_eq!(seven.len(), 1, "one message 7 for one pick");
    assert_eq!(seven[0].source, menu, "raised by the menu, not by the row");
    assert_eq!(
        dereth_ui::ElemHandle::from_raw(seven[0].p2),
        row5,
        "p2 is the chosen item — the one whose enum attribute 0x1000000B is read"
    );
    assert_eq!(
        seven[0].p1, ROW_TEMPLATE.0,
        "p1 is the row's element id, shared by all 14"
    );
    // `new_selection(true)` then a close — the menu shuts on a pick.
    assert!(
        !dereth_ui::widgets::menu::close(&mut ui, menu),
        "already closed by the pick"
    );

    // And the screen's arm, driven by the element the way the client drives it.
    ui.requests.clear();
    let choice = s.chat_target_menu_item(&mut ui, row5);
    match choice {
        MenuRowChoice::Focus(c) => {
            assert_eq!(c.focus, 5);
            assert_eq!(c.caption, "ID_Chat_ChatTargetMenuMonarch");
        }
        other => panic!("expected a focus change, got {other:?}"),
    }
    assert_eq!(s.main_chat.talk_focus, 5);
    assert_eq!(
        ui.requests.take(),
        vec![UiRequest::SetTalkFocus { focus: 5 }],
        " is the only thing the menu selection asks the host for"
    );
}

/// Behaviour: chat.talk-to-menu.the-squelch-row-is-a-toggle-and-its-message-names-the-speaker
/// Choosing the squelch row asks the shard to toggle the squelch.
#[test]
fn choosing_the_squelch_row_asks_the_shard_to_toggle_the_squelch() {
    let (mut ui, mut s) = screen();
    let toggle = s.main_chat.squelch_toggle.expect("squelch toggle button");

    // Nothing selected: with no last speakable target the handler returns — the row is inert.
    ui.requests.clear();
    assert_eq!(
        s.chat_target_menu_item(&mut ui, toggle),
        MenuRowChoice::Squelch
    );
    assert!(ui.requests.take().is_empty(), "no target, no event");

    let t = SpeakableTarget {
        id: 0x5000_1234,
        name: "Alba".into(),
        talkable: true,
        squelched: false,
    };
    s.main_chat.set_selected(&mut ui, Some(&t));
    s.chat_target_squelched = false;
    ui.requests.clear();
    assert_eq!(
        s.chat_target_menu_item(&mut ui, toggle),
        MenuRowChoice::Squelch
    );
    assert_eq!(
        ui.requests.take(),
        vec![UiRequest::ModifyCharacterSquelch {
            object: dereth_primitives::ObjectId(0x5000_1234),
            add: true,
            account: String::new(),
            message_type: 1,
        }],
        "the character-squelch event: inverse of the current state, id, empty account, type 1"
    );

    // …and it is a toggle: an already-squelched target is un-squelched.
    s.chat_target_squelched = true;
    ui.requests.clear();
    s.chat_target_menu_item(&mut ui, toggle);
    let sent = ui.requests.take();
    assert!(
        matches!(
            sent.first(),
            Some(UiRequest::ModifyCharacterSquelch { add: false, .. })
        ),
        "got {sent:?}"
    );
}

/// Behaviour: chat.talk-to-menu.the-chat-target-follows-what-is-selected-while-it-is-near
/// **The chat UI's once-a-second sweep — its first caller.**
///
/// The throttle is a file-scope static in the client: once a second, and the deadline is
/// pushed forward on every path out.
#[test]
fn the_once_a_second_sweep_clears_a_target_that_walked_away() {
    let (mut ui, mut s) = screen();
    let t = SpeakableTarget {
        id: 40,
        name: "Alba".into(),
        talkable: true,
        squelched: false,
    };
    s.main_chat.set_selected(&mut ui, Some(&t));
    assert!(
        s.main_chat.is_talk_focus_enabled(2),
        "the first arm is the one under test"
    );

    // In range: nothing happens, and the second is spent.
    let near = AutoTargetWorld {
        in_range_of_player: vec![40],
        ..AutoTargetWorld::default()
    };
    assert_eq!(
        s.chat_use_time(&mut ui, 100.0, &near, |_| None),
        Some(AutoTarget::Unchanged)
    );
    assert_eq!(
        s.chat_use_time(&mut ui, 100.5, &near, |_| None),
        None,
        "the throttle refuses"
    );

    // Out of range: the selected target is cleared, and focus 2 goes down with it.
    let far = AutoTargetWorld::default();
    assert_eq!(
        s.chat_use_time(&mut ui, 101.0, &far, |_| None),
        Some(AutoTarget::Clear)
    );
    assert_eq!(s.main_chat.last_speakable_target, 0);
    assert!(
        !s.main_chat.is_talk_focus_enabled(2),
        "SetTalkFocusEnabled(2, false)"
    );
    let row2 = s.main_chat.menu_item(&ui, 2).expect("Tell to <selected>");
    assert_eq!(
        state(&ui, row2),
        0x0D,
        "and its element is disabled, not a flag"
    );

    // The second arm: nothing speakable, so a talkable selection in range is adopted.
    let adopt = AutoTargetWorld {
        selected_id: 12,
        player_id: 9,
        selected_talkable: true,
        in_range_of_player: vec![12],
        selected_name: "Hoshino".into(),
        ..AutoTargetWorld::default()
    };
    assert_eq!(
        s.chat_use_time(&mut ui, 102.0, &adopt, |id| Some(adopt.adopted(id))),
        Some(AutoTarget::Adopt(12))
    );
    assert_eq!(s.main_chat.last_speakable_target, 12);
    assert_eq!(state(&ui, row2), 0x1, "…and the row comes back up");
}
