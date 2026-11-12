//! Behaviour: none (checks the world builder's playtest in its own world view end to end: our client run in the editor's process, logged in to the editor's Yalain, entering, running and stopped)
//!
//! The game played in the editor's own world view, with the real server: our client run in the
//! editor's process (its frames framed with no picture), logged in to the editor's Yalain as the
//! recipe's character, walking where its keys take it, and stopped as our client stops on exit.

use std::time::{Duration, Instant};

use athenaeum_editor::characters::Recipe;
use athenaeum_editor::embedded::Embedded;
use athenaeum_editor::live::{self, Client, Event, Live, Programs, Said, Setup};
use auberean_dat::testing::require_target_install;
use auberean_shell::interface::Phase;

use super::{arwic_world, database, logs, near, program, until, Scratch, HANG};

/// How far the character must have run forward, meters.
const RAN: f32 = 2.0;

/// Frame the game (and tick the playtest) until `want` answers; panics past the guard against a
/// hang, with the logs.
fn frames<T>(
    game: &mut Embedded,
    live: &mut Live,
    setup: &Setup,
    what: &str,
    mut want: impl FnMut(&Embedded) -> Option<T>,
) -> T {
    let began = Instant::now();
    loop {
        live.tick(setup).unwrap_or_else(|e| panic!("{what}: {e}"));
        game.tick();
        if let Some(t) = want(game) {
            return t;
        }
        assert!(
            began.elapsed() < HANG,
            "{what}: not within {HANG:?} ({:?})\n{}",
            game.phase(),
            logs(setup)
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Played in the editor, nothing is started: the editor is told how to log in, the game logs in
/// as the recipe's character and enters at its start, W runs it forward, and stopping takes it
/// out of the world while the server plays on.
#[test]
fn a_character_played_in_the_editor_logs_in_enters_and_runs_where_its_keys_take_it() {
    let install = require_target_install();
    let base = database();
    let scratch = Scratch::new("athenaeum-in-editor");
    let (session, place, entry) = arwic_world(&scratch.0, &base);
    let setup = Setup {
        programs: Programs {
            yalain: Some(program("YALAIN_BIN", "yalain")),
            server_dats: Some(install.clone()),
            client_dats: Some(install),
            port: 41_900,
            ..Programs::default()
        },
        folder: session.dir().join(live::FOLDER),
        base: Some(base),
        key: String::new(),
        clearings: Vec::new(),
        shops: Vec::new(),
    };
    let mut recipe = Recipe::new("Tester");
    recipe.start = athenaeum_editor::characters::Start::Place(place);
    let mut live = Live::default();
    live.start(&setup, session.files()).expect("Yalain starts");
    live.prepare(&setup, &recipe, Some(entry)).expect("asked");
    live.play_when_ready("Tester", Client::Editor);
    let launch = until(&mut live, &setup, "the editor told to play", |e, _| {
        e.iter().find_map(|x| match x {
            Event::InEditor(l) => Some(l.clone()),
            _ => None,
        })
    });
    assert!(live.in_editor() && live.playing());
    assert_eq!(live.playing_as().as_deref(), Some("Tester"));
    assert_eq!(launch.server.port(), live.port().expect("the login port"));
    assert_eq!(launch.character, "Tester");
    assert!(
        !setup.folder.join("client.log").exists(),
        "no client program started"
    );

    let mut game = Embedded::start(&launch, false).expect("the game starts in the editor");
    assert!(game.focused(), "the game takes the keyboard and the mouse");
    let (cell, at) = frames(
        &mut game,
        &mut live,
        &setup,
        "in the world",
        Embedded::place,
    );
    assert_eq!(cell >> 16, entry.cell >> 16, "entered at its start's block");
    assert!(near(at, entry.at, 5.0), "{at:?} {:?}", entry.at);
    assert!(
        live.in_world("Tester"),
        "the server says it entered: {}",
        logs(&setup)
    );

    // W held runs it forward, as the default keys do; let go, it stops.
    game.key("W", true).expect("a key of the game's");
    let ran = frames(&mut game, &mut live, &setup, "running", |g| {
        g.place().filter(|(_, p)| !near(*p, at, RAN))
    });
    game.key("W", false).expect("a key of the game's");
    assert_eq!(game.phase(), Phase::World);
    assert!(!near(ran.1, at, RAN), "{:?} from {at:?}", ran.1);

    // Stopped: the game says goodbye, and the server takes the character out of the world while
    // it plays on.
    game.stop();
    live.stop_client();
    assert!(!live.in_editor() && !live.playing());
    until(&mut live, &setup, "out of the world", |_, _| {
        (!live_in_world(&setup)).then_some(())
    });
    assert!(live.serving(), "the server plays on");
    live.stop_all();
}

/// Whether the server's log last said the character entered rather than left.
fn live_in_world(setup: &Setup) -> bool {
    let log = std::fs::read_to_string(setup.log()).unwrap_or_default();
    let mut inside = false;
    for line in log.lines() {
        match live::said(line) {
            Some(Said::Entered(n)) if n.eq_ignore_ascii_case("Tester") => inside = true,
            Some(Said::Left(n)) if n.eq_ignore_ascii_case("Tester") => inside = false,
            _ => {}
        }
    }
    inside
}
