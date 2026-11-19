//! The game before the world as Horizon follows it from the view.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use auberean_client_sdk::contract::session::{
    Character, CharacterList, LoginProgress, NetError, Phase, SessionEnd,
};
use auberean_client_sdk::contract::text::TextRef;
use auberean_client_sdk::contract::view::{CreationAnswer, Refusal, View};
use auberean_horizon::pregame::{download, Download, Made, PreFrame, Pregame};
use auberean_world::chargen;

/// One frame of `view` in `phase` at `now`, the game's messages saying `"<name>"`.
fn follow(p: &mut Pregame, phase: Phase, view: Option<&View>, now: f64) {
    follow_ended(p, phase, view, now, None);
}

fn follow_ended(
    p: &mut Pregame,
    phase: Phase,
    view: Option<&View>,
    now: f64,
    ended: Option<SessionEnd>,
) {
    let write = |t: &TextRef| t.literal.as_ref().map(|l| String::from_utf16_lossy(l));
    let message = |name: &str| Some(format!("<{name}>"));
    p.follow(&PreFrame {
        phase,
        view,
        ended,
        stopped: false,
        now,
        portal_space: false,
        write: &write,
        message: &message,
    });
}

fn list(expansion: bool) -> CharacterList {
    CharacterList {
        characters: vec![
            Character {
                id: 1,
                name: "Rowan".into(),
                seconds_greyed_out: 0,
            },
            Character {
                id: 2,
                name: "Elm".into(),
                seconds_greyed_out: 120,
            },
        ],
        slots: 6,
        account: "tester".into(),
        expansion,
        third_flag: false,
    }
}

#[test]
fn the_list_gives_the_characters_the_room_and_the_races_the_account_may_make() {
    let mut p = Pregame::default();
    let mut v = View::default();
    v.pregame.characters = Some(list(true));
    follow(&mut p, Phase::Characters, Some(&v), 10.0);
    let names: Vec<_> = p
        .characters
        .iter()
        .flatten()
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(names, ["Rowan", "Elm"]);
    assert_eq!(p.slots, 6);
    assert!(p.has_room());
    assert_eq!(p.may_make, chargen::EMPYREAN);
    // A deleted character's time runs from when the list came.
    let elm = p.character(2).expect("listed").clone();
    assert!((p.seconds_left(&elm, 40.0) - 90.0).abs() < 1e-9);
    // The same list again does not restart it.
    follow(&mut p, Phase::Characters, Some(&v), 40.0);
    assert!((p.seconds_left(&elm, 40.0) - 90.0).abs() < 1e-9);
}

#[test]
fn the_download_s_events_give_how_far_it_has_come() {
    let mut p = Pregame::default();
    let mut v = View::default();
    v.pregame.download = vec![(download::LOOKING, 0), (download::WAITING, 3)];
    follow(&mut p, Phase::Connecting, Some(&v), 0.0);
    assert_eq!(p.download, Download::Waiting(3));
    assert_eq!(p.download.share(), None);
    v.pregame
        .download
        .extend([(download::FETCHING, 400), (download::PROGRESS, 100)]);
    follow(&mut p, Phase::Connecting, Some(&v), 0.0);
    assert_eq!(p.download.share(), Some(0.25));
    // Each event is taken once.
    follow(&mut p, Phase::Connecting, Some(&v), 0.0);
    assert_eq!(p.download.share(), Some(0.25));
    v.pregame.download.push((download::DONE, 0));
    follow(&mut p, Phase::Connecting, Some(&v), 0.0);
    assert_eq!(p.download, Download::Done);
}

#[test]
fn the_login_s_progress_shows_only_while_connecting() {
    let mut p = Pregame::default();
    let mut v = View::default();
    v.pregame.login = Some(LoginProgress::Asking { attempt: 1, of: 5 });
    follow(&mut p, Phase::Connecting, Some(&v), 0.0);
    assert_eq!(p.login, Some(LoginProgress::Asking { attempt: 1, of: 5 }));
    follow(&mut p, Phase::Characters, Some(&v), 0.0);
    assert_eq!(p.login, None);
}

#[test]
fn a_refusal_back_to_the_list_says_why_once_and_one_that_ends_the_session_is_its_end() {
    let mut p = Pregame::default();
    let mut v = View::default();
    v.pregame.characters = Some(list(false));
    v.pregame.refused = Some(Refusal {
        reason: 11,
        count: 1,
        text: None,
    });
    follow(&mut p, Phase::Characters, Some(&v), 0.0);
    let r = p.refused.clone().expect("refused");
    assert!(r.to_list);
    assert_eq!(r.line, "<ID_CHAR_ERROR_ENTER_GAME_CHARACTER_IN_WORLD>");
    assert_eq!(p.ended, None);
    follow(&mut p, Phase::Characters, Some(&v), 0.0);
    assert_eq!(p.refused, None, "told once");
    // Booted, with the server's own line: the session is over, and that is why.
    v.pregame.refused = Some(Refusal {
        reason: 20,
        count: 2,
        text: Some(TextRef {
            literal: Some("Booted for testing.".encode_utf16().collect()),
            ..TextRef::default()
        }),
    });
    follow(&mut p, Phase::Characters, Some(&v), 0.0);
    assert!(!p.refused.as_ref().expect("refused").to_list);
    assert_eq!(p.ended.as_deref(), Some("Booted for testing."));
}

#[test]
fn the_server_s_answer_to_a_character_made_is_told_once() {
    let mut p = Pregame::default();
    let mut v = View::default();
    v.pregame.characters = Some(list(false));
    v.pregame.creation = Some(CreationAnswer {
        response: 3,
        character: 0,
        count: 1,
        error: 0,
    });
    follow(&mut p, Phase::Characters, Some(&v), 0.0);
    assert_eq!(p.made, Some(Made::Refused("That name is taken.".into())));
    follow(&mut p, Phase::Characters, Some(&v), 0.0);
    assert_eq!(p.made, None);
    v.pregame.creation = Some(CreationAnswer {
        response: 1,
        character: 9,
        count: 2,
        error: 0,
    });
    follow(&mut p, Phase::Characters, Some(&v), 0.0);
    assert_eq!(p.made, Some(Made::Made(9)));
}

#[test]
fn a_session_that_ends_says_why_and_a_new_login_forgets_it() {
    let mut p = Pregame::default();
    let mut v = View::default();
    v.pregame.characters = Some(list(false));
    follow(&mut p, Phase::Characters, Some(&v), 0.0);
    follow(&mut p, Phase::Disconnected, None, 1.0);
    assert_eq!(
        p.ended.as_deref(),
        Some("The connection to the server was lost.")
    );
    assert_eq!(p.characters, None);
    follow(&mut p, Phase::Connecting, None, 2.0);
    assert_eq!(p.ended, None, "logging in again");
    let gone = SessionEnd {
        server_died: true,
        ..SessionEnd::default()
    };
    follow_ended(&mut p, Phase::Disconnected, None, 3.0, Some(gone));
    assert_eq!(p.ended.as_deref(), Some("The server went down."));
    // A login refused before it began says so as soon as the shell knows.
    let mut q = Pregame::default();
    let bad = SessionEnd {
        connect_failed: Some(Some(NetError::BAD_SERVER_ADDRESS)),
        ..SessionEnd::default()
    };
    follow_ended(&mut q, Phase::Disconnected, None, 0.0, Some(bad));
    assert_eq!(
        q.ended.as_deref(),
        Some("The server's address can't be used.")
    );
}

#[test]
fn a_login_that_timed_out_says_the_server_can_t_be_reached_and_other_errors_say_their_own() {
    let failed = |id: &'static str| {
        let mut p = Pregame::default();
        let end = SessionEnd {
            connect_failed: Some(Some(NetError(id))),
            login_failed: true,
            ..SessionEnd::default()
        };
        follow_ended(&mut p, Phase::Disconnected, None, 0.0, Some(end));
        p.ended.expect("a line")
    };
    assert_eq!(
        failed("ID_ConnectionError_ClientTimedOutServer"),
        "The server can't be reached."
    );
    assert_eq!(
        failed("ID_ConnectionError_AccountAuthenticationFailed"),
        "The server refused the login."
    );
    assert_eq!(
        failed("ID_ConnectionError_ServerFull"),
        "The server is full."
    );
    assert_eq!(
        failed(NetError::BAD_SERVER_ADDRESS.id_string()),
        "The server's address can't be used."
    );
    assert_eq!(
        failed("ID_ConnectionError_NetVersionMismatch"),
        "This client's version doesn't match the server's."
    );
    assert_eq!(
        failed("ID_ConnectionError_PlayerAlreadyLoggedOn"),
        "This account is already logged in."
    );
    // A failed login with no error named was refused.
    let mut p = Pregame::default();
    let end = SessionEnd {
        login_failed: true,
        ..SessionEnd::default()
    };
    follow_ended(&mut p, Phase::Disconnected, None, 0.0, Some(end));
    assert_eq!(p.ended.as_deref(), Some("The server refused the login."));
}
