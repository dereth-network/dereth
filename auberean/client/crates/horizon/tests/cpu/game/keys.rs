//! The keys' actions Horizon carries out, and the keys Horizon's built-in layout gives them.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use auberean_horizon::keys::{action, keyed, Keyed};
use auberean_shell::input::Key;
use auberean_shell::interface_keys::{own, HORIZON_KEYS};

/// The action Horizon's built-in layout gives `key` held with `meta` (none 0, Shift 1, Ctrl 4).
fn bound(key: Key, meta: u32) -> Option<u32> {
    HORIZON_KEYS
        .iter()
        .find(|(input, m, _)| *input == key.input_word() && *m == meta)
        .map(|(_, _, a)| *a)
}

#[test]
fn horizons_layout_gives_each_action_horizon_carries_out_its_key() {
    assert_eq!(bound(Key::Enter, 0), Some(action::CHAT_ENTRY));
    assert_eq!(bound(Key::NumpadEnter, 0), Some(action::CHAT_ENTRY));
    assert_eq!(bound(Key::Slash, 0), Some(action::SLASH_COMMAND));
    assert_eq!(bound(Key::K, 0), Some(action::OPTIONS));
    assert_eq!(bound(Key::F12, 0), Some(action::SCREENSHOT));
    assert_eq!(bound(Key::Escape, 1), Some(action::LOG_OUT));
}

#[test]
fn each_action_horizon_carries_out_does_its_thing_and_the_games_own_are_left_to_the_world() {
    assert_eq!(
        keyed(action::CHAT_ENTRY),
        Some(Keyed::Chat { slash: false })
    );
    assert_eq!(
        keyed(action::SLASH_COMMAND),
        Some(Keyed::Chat { slash: true })
    );
    assert_eq!(keyed(action::OPTIONS), Some(Keyed::Settings));
    assert_eq!(keyed(action::LOG_OUT), Some(Keyed::LogOut));
    assert_eq!(keyed(action::SCREENSHOT), Some(Keyed::Screenshot));
    // Selecting and using are the world's controls' to carry out; the look at the front too.
    for other in [0x4100_001A, 0x4100_000D, own::LOOK_AT_FRONT] {
        assert_eq!(keyed(other), None, "{other:#x}");
    }
    // Every action Horizon carries out is the game's own: it has no own-range action yet.
    for a in [
        action::CHAT_ENTRY,
        action::SLASH_COMMAND,
        action::OPTIONS,
        action::LOG_OUT,
    ] {
        assert!(!own::is_own(a));
    }
}

#[test]
fn horizons_layout_gives_the_hotbar_the_stance_and_the_windows_their_keys() {
    assert_eq!(bound(Key::Digit1, 0), Some(action::SHORTCUT_1));
    assert_eq!(bound(Key::Digit0, 0), Some(action::SHORTCUT_0));
    assert_eq!(bound(Key::Digit1, 4), Some(action::SECOND_ROW_1));
    assert_eq!(bound(Key::Minus, 0), Some(action::SET_PREVIOUS));
    assert_eq!(bound(Key::Equal, 0), Some(action::SET_NEXT));
    assert_eq!(bound(Key::Backquote, 0), Some(action::AUTOATTACK));
    assert_eq!(bound(Key::C, 0), Some(action::SKILLS));
    assert_eq!(bound(Key::G, 0), Some(action::EXAMINATION));
}

#[test]
fn the_hotbars_the_stances_and_the_windows_actions_are_horizons_to_carry_out() {
    assert_eq!(
        keyed(action::SHORTCUT_1 + 4),
        Some(Keyed::Shortcut { row: 0, column: 4 })
    );
    assert_eq!(
        keyed(action::SECOND_ROW_0),
        Some(Keyed::Shortcut { row: 1, column: 9 })
    );
    assert_eq!(keyed(action::SET_1), Some(Keyed::Set(1)));
    assert_eq!(keyed(action::SET_0), Some(Keyed::Set(10)));
    assert_eq!(keyed(action::SET_NEXT), Some(Keyed::StepSet { next: true }));
    assert_eq!(
        keyed(action::SET_PREVIOUS),
        Some(Keyed::StepSet { next: false })
    );
    assert_eq!(keyed(action::AUTOATTACK), Some(Keyed::Attack));
    assert_eq!(keyed(action::SKILLS), Some(Keyed::Skills));
    assert_eq!(keyed(action::EXAMINATION), Some(Keyed::Examine));
    assert_eq!(keyed(action::NEXT_TAB), Some(Keyed::StepTab { next: true }));
    assert_eq!(
        keyed(action::PREVIOUS_TAB),
        Some(Keyed::StepTab { next: false })
    );
}

#[test]
fn the_social_window_and_the_fellows_have_their_keys_and_horizon_carries_them_out() {
    assert_eq!(bound(Key::O, 0), Some(action::SOCIAL));
    assert_eq!(bound(Key::F1, 0), Some(action::FELLOW_1));
    assert_eq!(bound(Key::F9, 0), Some(action::FELLOW_9));
    assert_eq!(keyed(action::SOCIAL), Some(Keyed::Social));
    assert_eq!(keyed(action::FELLOW_1), Some(Keyed::Fellow(0)));
    assert_eq!(keyed(action::FELLOW_9), Some(Keyed::Fellow(8)));
}

#[test]
fn shift_f2_to_f4_show_the_second_to_the_fourth_chat_window() {
    assert_eq!(bound(Key::F2, 1), Some(action::CHAT_WINDOW_1 + 1));
    assert_eq!(bound(Key::F3, 1), Some(action::CHAT_WINDOW_1 + 2));
    assert_eq!(bound(Key::F4, 1), Some(action::CHAT_WINDOW_4));
    assert_eq!(keyed(action::CHAT_WINDOW_1), Some(Keyed::ChatTab(0)));
    assert_eq!(keyed(action::CHAT_WINDOW_4), Some(Keyed::ChatTab(3)));
}

#[test]
fn t_replies_to_the_last_tell_and_no_key_of_the_layout_does_two_things() {
    assert_eq!(bound(Key::T, 0), Some(action::REPLY));
    assert_eq!(keyed(action::REPLY), Some(Keyed::Reply));
    for (i, (input, meta, a)) in HORIZON_KEYS.iter().enumerate() {
        assert!(
            !HORIZON_KEYS[..i]
                .iter()
                .any(|(k, m, _)| k == input && m == meta),
            "{input:#x} with {meta} bound twice, the second time to {a:#x}"
        );
    }
}
