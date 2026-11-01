//! Behaviour: none (checks the world builder's playtest end to end: its own Yalain and our client as programs on loopback, a world over the world database played as a recipe's character)
//!
//! The editor's playtest with the real programs: Yalain and Auberean built in this profile (beside
//! this binary's folder of dependencies, or named by `YALAIN_BIN` and `AUBEREAN_BIN`), the
//! end-of-retail install, and the world database from `YALAIN_TEST_WORLD_DB` (a checkout), else
//! the release this build pins in the per-user cache. Our client runs with no window, printing
//! where its character stands. Each step waits for what it should leave in the server's or the
//! client's log, with a time limit only against a hang.

mod stop;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use athenaeum_editor::characters::{self, Recipe, Start};
use athenaeum_editor::live::{self, Entry, Event, Live, Programs, Reloaded, Setup};
use athenaeum_foundry::edit::{NewKind, NewRow};
use athenaeum_foundry::{new, world, Session};
use auberean_dat::testing::require_target_install;
use auberean_world::chargen;
use yalain_world_db::compile::outdoor_cell;
use yalain_world_db::release::{cache_dir_for, Host, PINNED_TAG};

/// A guard against a hang, not the time a step is expected to take.
const HANG: Duration = Duration::from_secs(600);

/// The variable naming a world database checkout for the tests.
const WORLD_DB_VAR: &str = "YALAIN_TEST_WORLD_DB";

/// A person of the database in Arwic, and where the world moves it.
const PERSON: &str = "object n-334c@77B8";

fn database() -> PathBuf {
    if let Some(v) = std::env::var_os(WORLD_DB_VAR).filter(|v| !v.is_empty()) {
        return PathBuf::from(v);
    }
    let cached =
        cache_dir_for(Host::current(), |n| std::env::var_os(n)).map(|c| c.join(PINNED_TAG));
    match cached {
        Some(d) if d.join("world.toml").is_file() => d,
        _ => panic!(
            "no world database: set {WORLD_DB_VAR} to a checkout, or fetch the pinned release \
             ({PINNED_TAG}) with `yalain world fetch`"
        ),
    }
}

/// A program: the variable that names it, else the file of that name beside this test binary's
/// folder of dependencies, where cargo puts every program of the workspace it builds.
fn program(var: &str, name: &str) -> PathBuf {
    if let Some(p) = std::env::var_os(var).filter(|v| !v.is_empty()) {
        return PathBuf::from(p);
    }
    let exe = std::env::current_exe().expect("the test binary");
    let folder = exe
        .parent()
        .and_then(Path::parent)
        .expect("the test binary's profile folder");
    let path = folder.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    assert!(
        path.is_file(),
        "{name} is not built at {} (set {var}, or run `cargo xtask gate auberean-live`, which \
         builds it first)",
        path.display()
    );
    path
}

/// A folder of the run's own: removed when the run ends well, kept and named when it fails.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).expect("a scratch folder");
        Self(d)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("the run's folder is kept: {}", self.0.display());
        } else {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

/// Tick the playtest until `want` answers, keeping every event; panics past the guard, with the
/// logs.
fn until<T>(
    live: &mut Live,
    setup: &Setup,
    what: &str,
    mut want: impl FnMut(&[Event], &str) -> Option<T>,
) -> T {
    let began = Instant::now();
    let mut seen = Vec::new();
    loop {
        match live.tick(setup) {
            Ok(events) => seen.extend(events),
            Err(e) => panic!("{what}: {e}\n{}", logs(setup)),
        }
        let client = std::fs::read_to_string(setup.folder.join("client.log")).unwrap_or_default();
        if let Some(t) = want(&seen, &client) {
            return t;
        }
        assert!(
            began.elapsed() < HANG,
            "{what}: no answer; events {seen:?}\n{}",
            logs(setup)
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn logs(setup: &Setup) -> String {
    let read = |f: &str| std::fs::read_to_string(setup.folder.join(f)).unwrap_or_default();
    let tail = |t: String| {
        let lines: Vec<&str> = t.lines().collect();
        lines[lines.len().saturating_sub(40)..].join("\n")
    };
    format!(
        "--- yalain.log\n{}\n--- client.log\n{}",
        tail(read("yalain.log")),
        tail(read("client.log"))
    )
}

/// A place line our client printed: its character's cell and point.
fn place_line(line: &str) -> Option<(u32, [f32; 3])> {
    let rest = &line[line.find("CELL ")? + 5..];
    let cell = u32::from_str_radix(rest.get(..8)?, 16).ok()?;
    let inner = &rest[rest.find('(')? + 1..rest.find(')')?];
    let v: Vec<f32> = inner
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    (v.len() == 3).then(|| (cell, [v[0], v[1], v[2]]))
}

/// Where our client's character entered: the first place line it printed.
fn entered(client: &str) -> Option<(u32, [f32; 3])> {
    client.lines().find_map(place_line)
}

/// Where our client's character stands still: the last two place lines it printed, when they
/// are the same.
fn still(client: &str) -> Option<(u32, [f32; 3])> {
    let mut last = client.lines().rev().filter_map(place_line);
    let (a, b) = (last.next()?, last.next()?);
    (a == b).then_some(a)
}

/// The answer to a reload among the events.
fn reloaded(events: &[Event]) -> Option<Reloaded> {
    events.iter().find_map(|e| match e {
        Event::Reloaded(r) => Some(r.clone()),
        _ => None,
    })
}

fn near(a: [f32; 3], b: [f32; 3], within: f32) -> bool {
    dereth_primitives::num::math::hypotf(a[0] - b[0], a[1] - b[1]) <= within
}

/// How far east of its place the character is sent before the server starts again, and how far
/// it must have gone (something stops it some meters on), meters.
const WALK: f32 = 12.0;
const WALKED: f32 = 2.5;

/// A world over the database: a place by an Arwic person, which the recipe starts at, and the
/// session holding it.
fn arwic_world(root: &Path, base: &Path) -> (Session, String, Entry) {
    let dir = root.join("arwic-play");
    new::new_world(&dir, None).expect("a new world");
    let mut s =
        Session::open(world::open(&dir, Some(base)).expect("opens"), None).expect("session");
    let person = s
        .entry(PERSON)
        .expect("the database places this person")
        .clone();
    let at = person.at();
    let z = at.z.expect("the person stands at a height");
    let (x, y) = (at.x + 3.0, at.y);
    let item = s
        .place(&NewRow {
            kind: NewKind::Place,
            template: 0,
            landblock: 0x77B8,
            at: [x, y, z],
            heading: 90.0,
        })
        .expect("a place placed");
    s.save().expect("saved");
    let key = item
        .strip_prefix("place ")
        .expect("a place's item")
        .to_string();
    let entry = Entry {
        cell: outdoor_cell(0x77B8, x, y),
        at: [x, y, z + 0.5],
        heading: 90.0,
    };
    (s, key, entry)
}

/// A recipe's character made by the editor's own Yalain and played by our client where its
/// recipe starts it; a change the server takes without a restart published to it, and one to a
/// placed person that needs one started again with the character where it stood; stopped as the
/// operator stops it, which keeps the character; and played again, entering where it left.
#[test]
fn a_recipes_character_plays_the_world_live_and_is_kept_where_it_stood() {
    let install = require_target_install();
    let base = database();
    let scratch = Scratch::new("athenaeum-live");
    let (mut session, place, entry) = arwic_world(&scratch.0, &base);
    let setup = Setup {
        programs: Programs {
            yalain: Some(program("YALAIN_BIN", "yalain")),
            server_dats: Some(install.clone()),
            client: Some(program("AUBEREAN_BIN", "auberean")),
            client_dats: Some(install),
            retail: None,
            client_args: vec!["--no-window".into(), "1200".into()],
            port: 41_300,
        },
        folder: session.dir().join(live::FOLDER),
        base: Some(base),
    };
    let mut recipe = Recipe::new("Tester");
    recipe.race = chargen::TUMEROK;
    recipe.sex = chargen::FEMALE;
    recipe.start = Start::Place(place.clone());
    recipe.hero = true;
    characters::save(session.dir(), &recipe).expect("the recipe");
    // Our client walks its character east of the place once it is in the world, the first time.
    let walk_to = [entry.at[0] + WALK, entry.at[1], entry.at[2]];
    let script = scratch.0.join("walk.txt");
    std::fs::write(
        &script,
        format!(
            "until world\nwait 5\ngoto 77B8 {} {} 60\n",
            walk_to[0], walk_to[1]
        ),
    )
    .expect("the client's script");
    let mut walking = setup.clone();
    walking
        .programs
        .client_args
        .extend(["--dev-script".into(), script.display().to_string()]);
    let mut live = Live::default();

    // Made from its recipe, a hero, and in the world at its place.
    live.start(&setup, session.files(), &recipe, Some(entry))
        .expect("Yalain starts");
    assert!(
        live::lock(&setup.folder).is_err(),
        "the playtest folder is held while it plays"
    );
    live.play_when_ready(false);
    until(&mut live, &walking, "the server up", |e, _| {
        e.contains(&Event::Up).then_some(())
    });
    let log = std::fs::read_to_string(setup.log()).unwrap();
    assert!(log.contains("character Tester made on builder"), "{log}");
    assert!(log.contains("Tester placed at"), "{log}");
    let made = characters::made(&setup.folder, "Tester").expect("written down as made");
    assert_eq!(
        (made.race, made.sex, made.hero),
        (chargen::TUMEROK, chargen::FEMALE, true)
    );
    let (cell, at) = until(&mut live, &setup, "our client in the world", |_, c| {
        entered(c)
    });
    assert_eq!(cell >> 16, 0x77B8, "{cell:08X}");
    assert!(near(at, entry.at, 2.0), "{at:?} against {:?}", entry.at);
    let log = std::fs::read_to_string(setup.log()).unwrap();
    assert!(log.contains("Tester is now a hero"), "{log}");
    let was = until(&mut live, &setup, "the character walked away", |_, c| {
        still(c).filter(|(_, at)| !near(*at, entry.at, WALKED))
    })
    .1;

    // A change no table the server plays holds: taken, nothing waiting.
    session
        .turn(&format!("place {place}"), 180.0)
        .expect("the place turned");
    assert!(live.publish(&setup, session.files()).expect("published"));
    let answer = until(&mut live, &setup, "the first reload", |e, _| reloaded(e));
    assert_eq!(
        answer,
        Reloaded::Taken {
            applied: vec![],
            restart: vec![]
        }
    );

    // A person moved: it waits for a restart, which keeps the character where it stands.
    let pa = session.entry(PERSON).unwrap().at();
    session
        .move_to(PERSON, 0x77B8, [pa.x + 1.0, pa.y, pa.z.unwrap_or_default()])
        .expect("moved");
    assert!(live.publish(&setup, session.files()).expect("published"));
    let answer = until(&mut live, &setup, "the second reload", |e, _| reloaded(e));
    let Reloaded::Taken { restart, .. } = answer else {
        panic!("refused: {answer:?}\n{}", logs(&setup));
    };
    assert_eq!(restart, ["placed objects"]);
    live.restart(&setup, session.files())
        .expect("started again");
    let before = std::fs::read_to_string(setup.previous_log()).unwrap();
    assert!(before.contains("stopping at the system's word"), "{before}");
    assert!(
        before.contains("stopping: 1 characters in the world kept"),
        "{before}"
    );
    until(&mut live, &setup, "the server up again", |e, _| {
        e.contains(&Event::Up).then_some(())
    });
    let log = std::fs::read_to_string(setup.log()).unwrap();
    assert!(
        !log.contains("character Tester made"),
        "not made again: {log}"
    );
    assert!(!log.contains("Tester placed at"), "not placed again: {log}");
    let (cell, at) = until(
        &mut live,
        &setup,
        "our client in the world again",
        |_, c| entered(c),
    );
    assert_eq!(cell >> 16, 0x77B8);
    assert!(near(at, was, 1.0), "{at:?} where it stood, {was:?}");
    assert!(
        !near(at, entry.at, WALKED - 0.5),
        "not back at its place: {at:?}"
    );

    // Stopped as its operator stops it: it ends by itself, keeping the character.
    live.stop_client();
    assert!(
        live.stop_server(),
        "the server ended by itself\n{}",
        logs(&setup)
    );
    let log = std::fs::read_to_string(setup.log()).unwrap();
    assert!(log.contains("stopping at the system's word"), "{log}");
    assert!(
        log.contains("stopping: 1 characters in the world kept"),
        "{log}"
    );
    live.stop_all();
    assert!(live::lock(&setup.folder).is_ok(), "the folder let go");
}
