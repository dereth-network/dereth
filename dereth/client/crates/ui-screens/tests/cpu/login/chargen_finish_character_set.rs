//! On the character-set notice after Finish the wizard logs on the new character (plussed/unplussed
//! name match, greyed row skipped, no-op with nothing outstanding) or falls back to character
//! select; the character screen list rebuild includes the new name.
//! Fixture: synthetic views and UI state; the catalogue check also reads production source files.

use dereth_primitives::ObjectId;
use dereth_ui::persist::{CharacterIdentity, CharacterSet};
use dereth_ui::UiSystem;
use dereth_ui_screens::screens::chargen::{CharGenAction, CharGenScreen};
use dereth_ui_screens::screens::charmgmt::CharacterManagementScreen;

const LARK: u32 = 0x5000_0001;
const NEW: u32 = 0x5000_0002;

fn identity(id: u32, name: &str, grace: u32) -> CharacterIdentity {
    CharacterIdentity {
        id: ObjectId(id),
        name: name.into(),
        seconds_grace_period: grace,
    }
}

/// The set as `0xF658` delivered it: one character.
fn before() -> CharacterSet {
    CharacterSet {
        set: vec![identity(LARK, "Lark", 0)],
        num_allowed_characters: 5,
        account: "ac01".into(),
        ..CharacterSet::default()
    }
}

/// The set after appended `0xF643`'s identity.
fn after(new_name: &str) -> CharacterSet {
    let mut s = before();
    s.set.push(identity(NEW, new_name, 0));
    s
}

/// A wizard that has pressed Finish and had the server say OK.
fn wizard_after_ok(name: &str) -> CharGenScreen {
    let mut s = CharGenScreen::default();
    s.state.name = name.into();
    // The character-generation verification response, the OK arm.
    s.on_chargen_verification_response(1);
    assert!(
        s.awaiting_char_set_for_login,
        "the OK arm sets awaiting_char_set_for_login and closes the please-wait dialog"
    );
    assert_eq!(s.open_dialog, None);
    s
}

/// Behaviour: login.enter-world.reaches-the-hud-from-the-wizard-as-well-as-from-character-select
/// **Symptom 1, and retail's real answer to it.** The notice moves the wizard, and where it moves
/// to is *into the world as the new character*, not to character select.
///
/// Oracle: the case-insensitive name match over the character set, the no-grey-out and non-zero
/// gid guards, and the log-on that follows them.
#[test]
fn the_notice_takes_the_wizard_into_the_world_as_the_new_character() {
    let mut s = wizard_after_ok("Tarinell");
    let mode = s.character_set_arrived(&after("Tarinell"));

    assert_eq!(
        mode, None,
        "a successful match queues no mode: the update returns before the tail"
    );
    assert_eq!(
        std::mem::take(&mut s.actions),
        vec![CharGenAction::LogOn(ObjectId(NEW))],
        "log on the character that was just created"
    );
    assert!(
        !s.awaiting_char_set_for_login,
        "the latch does not survive the set it waited for"
    );
}

/// Behaviour: chargen.finish.a-set-without-the-new-name-falls-back-to-character-select
/// A set without the new name falls through to character select.
#[test]
fn a_set_without_the_new_name_falls_through_to_character_select() {
    let mut s = wizard_after_ok("Tarinell");
    let mode = s.character_set_arrived(&after("SomebodyElse"));
    assert_eq!(mode, Some(dereth_ui::framework::mode::CHARACTER_MANAGEMENT));
    assert!(s.actions.is_empty(), "nothing is logged on");
}

/// The grey-out guard, which the success arm's "row not greyed out" test is: a character on the
/// server's delete timer is skipped even when the name matches.
#[test]
fn a_greyed_out_row_is_not_logged_on() {
    let mut s = wizard_after_ok("Tarinell");
    let mut set = before();
    set.set.push(identity(NEW, "Tarinell", 3600));
    assert_eq!(
        s.character_set_arrived(&set),
        Some(dereth_ui::framework::mode::CHARACTER_MANAGEMENT)
    );
    assert!(s.actions.is_empty());
}

/// A set arriving with no creation outstanding moves nothing.
#[test]
fn a_set_arriving_with_no_creation_outstanding_moves_nothing() {
    let mut s = CharGenScreen::default();
    s.state.name = "Tarinell".into();
    assert_eq!(s.character_set_arrived(&after("Tarinell")), None);
    assert!(s.actions.is_empty());
}

/// ACE writes the raw name into `GameMessageCharacterCreateResponse` even on a plussed account,
/// while `GameMessageCharacterList` prefixes it with `+`. The wizard asks for `+Name` when
/// `create_as_admin` is set, so the `+`-insensitive second pass is what makes the appended identity
/// match — and it is exactly the case a create-then-log-on on an admin account hits.
#[test]
fn an_admin_creation_matches_the_unplussed_name_ace_sends_back() {
    let mut s = wizard_after_ok("Tarinell");
    s.state.create_as_admin = true;
    let mode = s.character_set_arrived(&after("Tarinell"));
    assert_eq!(mode, None);
    assert_eq!(
        std::mem::take(&mut s.actions),
        vec![CharGenAction::LogOn(ObjectId(NEW))]
    );
}

/// A plussed name in the set matches the bare one the wizard asked for.
#[test]
fn a_plussed_name_in_the_set_matches_the_bare_one_the_wizard_asked_for() {
    let mut s = wizard_after_ok("Tarinell");
    assert!(!s.state.create_as_admin, "the wizard asked for a bare name");
    let mode = s.character_set_arrived(&after("+Tarinell"));
    assert_eq!(
        mode, None,
        "the + is the server's, not a different character"
    );
    assert_eq!(
        std::mem::take(&mut s.actions),
        vec![CharGenAction::LogOn(ObjectId(NEW))]
    );
}

/// Behaviour: character-select.list.the-new-character-appears-after-finish
/// **Symptom 2, asserted on its own**: the same notice, delivered to the character screen, puts
/// the new name in the list.
///
/// Oracle: the list is sorted by name with the
/// pending-delete rows moved to the end.
#[test]
fn the_notice_puts_the_new_character_in_the_list() {
    let mut ui = UiSystem::new((800, 600));
    let mut s = CharacterManagementScreen::default();

    // The list as `0xF658` left it. This is the calibration: without it a two-row list below would
    // not distinguish "the rebuild ran" from "the rebuild ran twice".
    s.rebuild_character_list(&mut ui, &before());
    assert_eq!(
        s.rows.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
        vec!["Lark"],
        "the account starts with one character"
    );

    s.rebuild_character_list(&mut ui, &after("Tarinell"));
    assert_eq!(
        s.rows.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
        vec!["Lark", "Tarinell"],
        "the created character is in the list, sorted by name, with no relaunch"
    );
    assert_eq!(s.rows[1].id, ObjectId(NEW));
    assert!(!s.rows[1].greyed_out);
    assert_eq!(
        s.char_set.set.len(),
        2,
        "the screen's own copy of the set was replaced"
    );
}

/// And the edge the host tests on before rebuilding (`ui.rs`: `received_set && s.char_set != set`)
/// really is an edge: the *appended* set differs from the one the screen holds, so the rebuild
/// runs. A `PartialEq` that ignored `set` would make the whole fix invisible.
#[test]
fn the_appended_set_compares_unequal_to_the_one_the_screen_holds() {
    let mut ui = UiSystem::new((800, 600));
    let mut s = CharacterManagementScreen::default();
    s.rebuild_character_list(&mut ui, &before());
    assert_ne!(
        s.char_set,
        after("Tarinell"),
        "otherwise the host's edge never fires"
    );
    assert_eq!(
        s.char_set,
        before(),
        "and an identical set is correctly not an edge"
    );
}
