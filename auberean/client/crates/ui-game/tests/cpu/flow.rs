//! The game flow's rules.

use auberean_ui_game::flow::{character_error, mode};

/// Each reason the server gives for refusing a character leads to its screen with its message:
/// a deleted character and the reasons an entry fails (9 to 18, and a locked character) back to
/// the character list; a login refused, a crash, a log-off, the account's troubles, a full server
/// and a booted character to the disconnected screen; any other reason to none.
///
/// Behaviour: ui.flow.each-refusal-reason-leads-to-its-screen
#[test]
fn each_refusal_reason_leads_to_its_screen() {
    let list = mode::CHARACTER_SELECTION;
    let gone = mode::DISCONNECTED;
    for reason in 0..=30 {
        let want = match reason {
            5 | 9..=18 | 21 => Some(list),
            2..=4 | 6..=8 | 19 | 20 => Some(gone),
            _ => None,
        };
        assert_eq!(
            character_error(reason).map(|(m, _)| m),
            want,
            "reason {reason}"
        );
    }
    assert_eq!(
        character_error(11),
        Some((list, "ID_CHAR_ERROR_ENTER_GAME_CHARACTER_IN_WORLD"))
    );
    assert_eq!(character_error(4), Some((gone, "ID_CHAR_ERROR_LOGOFF")));
}
