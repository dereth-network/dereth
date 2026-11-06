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

/// How long a character stays deleted reads nothing for a row past the number the account may have
/// (when it may have any), and a row holding the invalid id has no character.
///
/// Behaviour: ui.charselect.a-row-past-the-accounts-limit-is-not-a-deleted-one
#[test]
fn a_row_past_the_accounts_limit_is_not_a_deleted_one() {
    use auberean_ui_game::pregame::{Character, CharacterSet, INVALID_ID};
    let c = |id, greyed| Character {
        id,
        name: "Firstlight".into(),
        greyed,
    };
    let set = |slots| CharacterSet {
        characters: vec![c(1, 60), c(2, 60), c(INVALID_ID, 60)],
        slots,
        account: "tester".into(),
    };
    let two = set(2);
    assert_eq!(two.greyed_out_for(1), 60);
    assert_eq!(two.greyed_out_for(2), 0, "past the limit");
    assert_eq!(two.greyed_out_for(9), 0);
    let one = set(1);
    assert_eq!(one.greyed_out_for(0), 60);
    assert_eq!(one.greyed_out_for(1), 0);
    assert_eq!(one.identity(1).map(|c| c.id), Some(2), "still listed");
    for no_limit in [0, -1] {
        assert_eq!(set(no_limit).greyed_out_for(1), 60, "no limit");
        assert_eq!(set(no_limit).greyed_out_for(2), 0, "the invalid id");
    }
    assert!(
        set(3).identity(2).is_none(),
        "the invalid id is no character"
    );
}
