//! The log window's input line is the game's chat entry: it opens on the game's draft, tells the
//! game every edit, sends to the destination the game holds, steps through the destinations the
//! game lets the player use, and takes the game's replies and recalled lines.
//!
//! Behaviour: none (experimental Horizon interface)

use dereth_client_contract::chat::entry::{EntryAction, EntryUpdate};
use dereth_client_contract::chat::interface::window::MAIN;
use dereth_client_contract::chat::mainchat::ChatFocusView;
use dereth_client_contract::UiRequest;
use dereth_horizon::ui::game::GameState;
use dereth_horizon::ui::input::vk;

use crate::Harness;

fn in_world() -> GameState {
    let mut focus = ChatFocusView {
        focus: 1,
        ..ChatFocusView::default()
    };
    for f in [1, 3, 8] {
        focus.enabled[f] = true;
        focus.selectable[f] = true;
    }
    GameState {
        in_world: true,
        name: "Tester".into(),
        chat_focus: focus,
        chat_draft: "half a line".into(),
        ..GameState::default()
    }
}

fn key(h: &mut Harness, key: usize) {
    h.input.keys.push(key);
}

/// Enter pressed as the key map binds it: the key, and the game's enter-chat action.
fn open_line(h: &mut Harness) {
    h.input.keys.push(vk::ENTER);
    h.input
        .actions
        .push(dereth_client_contract::actions::chat_entry::BEGIN_CHAT_MODE.0);
}

#[test]
fn enter_opens_the_line_on_the_game_s_draft_and_each_edit_is_told_to_the_game() {
    let state = in_world();
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    open_line(&mut h);
    let opened = h.frame(&state);
    assert!(h.ui.hud.log.typing());
    assert!(
        opened.requests.is_empty(),
        "opening on the draft changes nothing: {:?}",
        opened.requests
    );
    h.input.chars = vec!['!'];
    let typed = h.frame(&state);
    assert_eq!(
        typed.requests,
        [UiRequest::ChatEntry {
            window: MAIN,
            text: "half a line!".into(),
            action: EntryAction::Draft,
        }]
    );
}

#[test]
fn enter_sends_the_line_to_the_main_window_for_the_game_to_route_by_its_destination() {
    let state = in_world();
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    open_line(&mut h);
    h.frame(&state);
    key(&mut h, vk::ENTER);
    let sent = h.frame(&state);
    assert_eq!(
        sent.requests,
        [UiRequest::ChatLine {
            text: "half a line".into(),
            window: MAIN,
        }]
    );
    assert!(!h.ui.hud.log.typing());
}

#[test]
fn tab_asks_for_the_next_destination_and_up_asks_the_game_for_the_line_before() {
    let state = in_world();
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    open_line(&mut h);
    h.frame(&state);
    key(&mut h, vk::TAB);
    key(&mut h, vk::UP);
    let out = h.frame(&state);
    // After All in the game's menu order come Vassals, Fellowship (3), ...
    assert!(out.requests.contains(&UiRequest::SetTalkFocus { focus: 3 }));
    assert!(out.requests.contains(&UiRequest::ChatEntry {
        window: MAIN,
        text: "half a line".into(),
        action: EntryAction::Previous,
    }));
}

#[test]
fn a_reply_the_game_begins_opens_the_line_with_its_text() {
    let mut state = in_world();
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    state.chat_entry = vec![EntryUpdate {
        window: MAIN,
        text: "@tell Borin, ".into(),
        cursor: 13,
        focus: true,
    }];
    h.frame(&state);
    state.chat_entry.clear();
    assert!(h.ui.hud.log.typing());
    key(&mut h, vk::ENTER);
    let sent = h.frame(&state);
    assert_eq!(
        sent.requests,
        [UiRequest::ChatLine {
            text: "@tell Borin, ".into(),
            window: MAIN,
        }]
    );
}

#[test]
fn escape_closes_the_line_and_leaves_the_draft_with_the_game() {
    let state = in_world();
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    open_line(&mut h);
    h.frame(&state);
    key(&mut h, vk::ESCAPE);
    let out = h.frame(&state);
    assert!(!h.ui.hud.log.typing());
    assert!(
        !out.requests
            .iter()
            .any(|r| matches!(r, UiRequest::ChatLine { .. })),
        "nothing is sent"
    );
}

#[test]
fn the_line_s_caret_moves_with_the_arrows_and_typing_goes_in_at_it() {
    let state = in_world();
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    open_line(&mut h);
    h.frame(&state);
    key(&mut h, vk::HOME);
    h.frame(&state);
    h.input.chars = "a ".chars().collect();
    let typed = h.frame(&state);
    assert_eq!(
        typed.requests,
        [UiRequest::ChatEntry {
            window: MAIN,
            text: "a half a line".into(),
            action: EntryAction::Draft,
        }]
    );
}

#[test]
fn control_a_then_c_copies_the_whole_line_and_control_v_pastes_over_it() {
    let state = in_world();
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    open_line(&mut h);
    h.frame(&state);
    h.input.ctrl = true;
    key(&mut h, 0x41);
    key(&mut h, 0x43);
    h.frame(&state);
    assert_eq!(h.input.copied.take().as_deref(), Some("half a line"));
    h.input.paste = Some("a whole line".into());
    key(&mut h, 0x56);
    let pasted = h.frame(&state);
    assert_eq!(
        pasted.requests,
        [UiRequest::ChatEntry {
            window: MAIN,
            text: "a whole line".into(),
            action: EntryAction::Draft,
        }]
    );
}

#[test]
fn a_drag_over_the_log_selects_its_lines_and_control_c_copies_them() {
    use dereth_horizon::ui::game::ChatLine;
    let mut state = in_world();
    state.new_chat = vec![
        ChatLine {
            chat_type: 0,
            text: "You say, \"hello\"".into(),
            window: MAIN,
        },
        ChatLine {
            chat_type: 0,
            text: "Cora Vell says, \"well met\"".into(),
            window: MAIN,
        },
    ];
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    state.new_chat.clear();
    let dock = h.ui.hud.log.dock;
    // Pressed above the lines (they sit at the bottom of the log) and dragged below the window.
    h.input.mouse = (dock.x + 20.0, dock.y + 10.0);
    h.input.down[0] = true;
    h.input.pressed[0] = true;
    h.frame(&state);
    h.input.mouse = (dock.x + 20.0, dock.bottom() + 40.0);
    h.frame(&state);
    h.input.down[0] = false;
    h.input.released[0] = true;
    h.frame(&state);
    h.input.ctrl = true;
    key(&mut h, 0x43);
    h.frame(&state);
    assert_eq!(
        h.input.copied.take().as_deref(),
        Some("You say, \"hello\"\nCora Vell says, \"well met\"")
    );
    // While the lines hold a selection, Control-C is the interface's: the game's C key (the
    // character window) never sees it. Without Control, C goes on to the game.
    {
        use dereth_input::host::HostEvent;
        use dereth_input::keys::Key;
        let mut front = dereth_horizon::runtime::HorizonFrontEnd::new(
            std::sync::Arc::new(dereth_horizon::art::Art::empty()),
            Default::default(),
            None,
        );
        std::mem::swap(&mut front.ui, &mut h.ui);
        let key = |vk, scan, pressed| HostEvent::KeyboardInput {
            key: Key::new(vk, scan),
            pressed,
            text: None,
        };
        assert!(front.host_event(&key(0x43, 0x2E, true), 0).forward);
        front.host_event(&key(0x11, 0x1D, true), 0);
        assert!(!front.host_event(&key(0x43, 0x2E, true), 0).forward);
        std::mem::swap(&mut front.ui, &mut h.ui);
    }
    // A click that is not dragged lets the selection go.
    h.input.ctrl = false;
    h.input.mouse = (dock.x + 20.0, dock.y + 10.0);
    h.input.down[0] = true;
    h.input.pressed[0] = true;
    h.frame(&state);
    h.input.down[0] = false;
    h.frame(&state);
    h.input.ctrl = true;
    key(&mut h, 0x43);
    h.frame(&state);
    assert!(h.input.copied.is_none());
}
