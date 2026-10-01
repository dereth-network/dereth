//! The acceptance script replays first-login-walk-jump to character select and prints fourteen
//! frame steps; a named character reaches the world and a walk reaches the movement route; an
//! unknown session name stops the run.
//! Fixture: the shipped retail DAT records and recorded inputs.

use dereth_client_sdk::net::client_session::SessionState;
use dereth_headless::run::{step_lines, Options};
use dereth_headless::{parse, run};

const ACCEPTANCE: &str = include_str!("../../../scripts/first-login-walk-jump-login.script");

fn options() -> Options {
    dereth_client_sdk::dat::testing::require_dats();
    Options {
        dat_dir: dereth_client_sdk::dat::testing::dat_dir(),
        ..Options::default()
    }
}

fn go(script: &str) -> (Vec<String>, dereth_headless::Summary) {
    let commands = parse(script).expect("the script parses");
    let mut out: Vec<u8> = Vec::new();
    let summary = run(&commands, &options(), &mut out).expect("the run finishes");
    let text = String::from_utf8(out).expect("the run prints text");
    (text.lines().map(str::to_owned).collect(), summary)
}

/// Behaviour: presentation.headless.a-recorded-session-drives-the-real-app-to-character-select-and-into-the-world
/// **The acceptance.** `dereth-headless --script scripts/first-login-walk-jump-login.script` replays
/// `first-login-walk-jump`'s login to the character-select screen and prints the fourteen frame
/// steps.
///
/// Falsified by a frame that skips a step (the step list would be short), by a login that stops
/// early (`run` would answer `NotReached`), and by a client that needed a window or a device
/// (neither exists here, so it would not have started).
#[test]
fn the_acceptance_script_logs_first_login_walk_jump_in_to_character_select_and_prints_the_fourteen_steps(
) {
    let (lines, summary) = go(ACCEPTANCE);

    assert_eq!(
        summary.state,
        Some(SessionState::CharacterSelect),
        "0xF658 arrived and stopped it"
    );
    assert_eq!(
        summary.characters,
        vec!["+Aldis".to_owned(), "+Aren".to_owned()],
        "first-login-walk-jump's own Login_LoginCharacterSet (the locked corpus's stand-in names, ), in the order the shard sent it"
    );

    // The fourteen steps, in `FrameStep::ORDER`, are the last thing before `quit`.
    let steps = step_lines();
    assert_eq!(steps.len(), 14, "the frame step list");
    let tail = &lines[lines.len() - steps.len() - 1..lines.len() - 1];
    assert_eq!(
        tail,
        steps.as_slice(),
        "the fourteen frame steps the headless frame ran"
    );
    assert_eq!(
        lines.last().map(String::as_str),
        Some(format!("quit frames={}", summary.frames).as_str())
    );

    // ...and the step lines are the only step lines: a `dump steps` that also printed the other
    // events would pass the tail test above by accident.
    assert_eq!(
        lines.iter().filter(|l| l.starts_with("step ")).count(),
        steps.len(),
        "dump steps printed something that is not a step"
    );

    let login = lines
        .iter()
        .find(|l| l.starts_with("login first-login-walk-jump fed="))
        .expect("the login's own result line");
    assert!(
        login.ends_with("state=CharacterSelect characters=2"),
        "{login}"
    );
}

/// Behaviour: presentation.headless.a-recorded-session-drives-the-real-app-to-character-select-and-into-the-world
/// A named character reaches the world and a walk reaches the movement route.
#[test]
fn a_named_character_reaches_the_world_and_a_walk_reaches_the_movement_route() {
    let (lines, summary) = go("login first-login-walk-jump +Aldis\n\
         walk forward 0.5\n\
         dump events\n\
         snapshot\n\
         quit\n");

    assert_eq!(
        summary.state,
        Some(SessionState::Playable),
        "0x0013 is what puts it in the world"
    );
    assert!(
        lines
            .iter()
            .any(|l| l == "enter-world +Aldis id=0x50000002"),
        "the character the recording's own character set names, with its own id: {lines:?}"
    );
    assert!(
        lines.iter().any(|l| l == "action-routed movement"),
        "the injected MOVE_FORWARD never reached MovementCommands::on_action: {lines:?}"
    );
    assert!(
        lines.iter().any(|l| l == "snapshot player=0x50000002"),
        "GameSnapshot::from_view did not see the player the shard described: {lines:?}"
    );
    assert!(
        lines.iter().any(|l| l == "snapshot character=+Aldis"),
        "...nor its name: {lines:?}"
    );
    // The skills line is a count and not a zero: the snapshot is reading a real player.
    let skills = lines
        .iter()
        .find(|l| l.starts_with("snapshot skills="))
        .expect("a skills line");
    assert_ne!(
        skills, "snapshot skills=0",
        "the 0x0013 carried skills and the snapshot lost them"
    );
}

/// The action verbs: a turn held, a jump pressed and a camera rotation held, each named by its
/// retail action name, reach the movement, jump and camera routes of the frame's handlers -- the
/// same routes a key's action takes. The script is the one the crate ships.
#[test]
fn the_shipped_action_script_reaches_the_movement_jump_and_camera_routes() {
    let (lines, _) = go(include_str!("../../../scripts/first-login-actions.script"));
    for (verb, route) in [
        ("hold MovementTurnLeft ", "action-routed movement"),
        ("press MovementJump ", "action-routed jump"),
        ("hold CameraRotateLeft ", "action-routed camera"),
    ] {
        let at = lines
            .iter()
            .position(|l| l.starts_with(verb))
            .unwrap_or_else(|| panic!("no {verb:?} line: {lines:?}"));
        let dumped = lines[at + 1..]
            .iter()
            .take_while(|l| !l.starts_with("hold ") && !l.starts_with("press "));
        assert!(
            dumped.clone().any(|l| l == route),
            "{verb:?} did not reach {route:?}: {:?}",
            dumped.collect::<Vec<_>>()
        );
    }
}

/// Behaviour: login.enter-world.the-client-says-it-has-finished-loading-when-the-portal-fades-out
/// A client with no UI at all still tells the server it has finished loading. After the replayed
/// login reaches the world, the log-in's portal-space animation runs its course (about eight
/// simulated seconds) and the login-complete notification goes out exactly once.
///
/// Falsified by a frame that runs the teleport step only from inside a UI's own step: this client
/// has none, so the animation would never run and nothing would be sent.
#[test]
fn a_client_with_no_ui_sends_login_complete_once_the_portal_space_ends() {
    // `--world`: the portal space lasts until the body stands in a loaded scene, so the run needs
    // a world for the animation to end.
    let commands = parse(
        "login first-login-walk-jump +Aldis\n\
         tick 900\n\
         quit\n",
    )
    .expect("the script parses");
    let mut out: Vec<u8> = Vec::new();
    let opts = Options {
        world: true,
        ..options()
    };
    let summary = run(&commands, &opts, &mut out).expect("the run finishes");
    assert_eq!(summary.state, Some(SessionState::Playable));
    assert_eq!(
        summary.login_completes_sent, 1,
        "the 0x00A1 the end of the log-in's portal space owes the server"
    );
}

/// A recording that is not there is an error, not an empty replay that passes.
#[test]
fn a_session_name_with_no_recording_stops_the_run() {
    let commands = parse("login no-such-session\nquit\n").expect("it parses");
    let mut out: Vec<u8> = Vec::new();
    let e = run(&commands, &options(), &mut out).expect_err("there is no such capture");
    assert!(matches!(e, dereth_headless::RunError::Capture(_)), "{e}");
}
