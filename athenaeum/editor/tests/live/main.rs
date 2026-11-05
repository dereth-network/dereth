//! Behaviour: none (checks the world builder's playtest end to end: its own Yalain and our client as programs on loopback, a world over the world database played as recipes' characters made, built and placed at the server's console, the server moved past ports another program holds, two playtests at once each on ports of its own, and stopped by the end of its console)
//!
//! The editor's playtest with the real programs: Yalain and Auberean built in this profile (beside
//! this binary's folder of dependencies, or named by `YALAIN_BIN` and `AUBEREAN_BIN`), the
//! end-of-retail install, and the world database from `YALAIN_TEST_WORLD_DB` (a checkout), else
//! the release this build pins in the per-user cache. Our client runs with no window, printing
//! where its character stands. Each step waits for what it should leave in the server's or the
//! client's log, with a time limit only against a hang.

mod stop;

use std::net::{Ipv4Addr, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use athenaeum_editor::characters::{self, Item, Making, Quest, QuestAt, Recipe, Skill, Start};
use athenaeum_editor::live::{self, Ask, Entry, Event, Live, Programs, Reloaded, Said, Setup};
use athenaeum_editor::stop::windowless;
use athenaeum_foundry::edit::{ClearingShape, NewKind, NewRow};
use athenaeum_foundry::{new, world, Session};
use auberean_dat::overlay::WORLD_FOLDER;
use auberean_dat::testing::require_target_install;
use auberean_world::chargen;
use yalain_world_db::compile::outdoor_cell;
use yalain_world_db::release::{pinned_in_cache, PINNED_TAG};
use yalain_world_db::Files;

/// A guard against a hang, not the time a step is expected to take.
const HANG: Duration = Duration::from_secs(600);

/// The variable naming a world database checkout for the tests.
const WORLD_DB_VAR: &str = "YALAIN_TEST_WORLD_DB";

/// A person of the database in Arwic, and where the world moves it.
const PERSON: &str = "object n-334c@77B8";
/// A hoard's vein (a craft node anyone may mine).
const VEIN: u32 = 0x4700_12C0;
/// A camp of the database in Arwic, which the world moves.
const CAMP: &str = "camp c-4586@77B8";
/// A recipe's build: a skill a tumerok trains at level 10, an item of the world's catalog (a
/// race's starting pack) and a quest of the world's.
const SKILL: u32 = 7;
const ITEM: u32 = 0x4700_051C;
const QUEST: u32 = 0x4000_01CA;

fn database() -> PathBuf {
    if let Some(v) = std::env::var_os(WORLD_DB_VAR).filter(|v| !v.is_empty()) {
        return PathBuf::from(v);
    }
    pinned_in_cache().unwrap_or_else(|why| {
        panic!(
            "no world database: {why}. Set {WORLD_DB_VAR} to a checkout, or fetch the pinned release ({PINNED_TAG}) with `yalain world fetch`"
        )
    })
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

/// Tick the playtest until `want` answers, keeping every event; panics once the server has
/// stopped by itself, or past the guard, with the logs.
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
        if let Some(Event::Stopped(tail)) = seen.iter().find(|e| matches!(e, Event::Stopped(_))) {
            panic!(
                "{what}: the server stopped: {tail}\n{}\n--- yalain-previous.log\n{}",
                logs(setup),
                std::fs::read_to_string(setup.previous_log()).unwrap_or_default()
            );
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

/// The answer to a command asked at the console among the events.
fn answered(events: &[Event], ask: &Ask) -> Option<Result<String, String>> {
    events.iter().find_map(|e| match e {
        Event::Answered(a, r) if a == ask => Some(r.clone()),
        _ => None,
    })
}

/// Whether the answer to `ask` among the events is what was done, in words holding `words`.
fn done_with(events: &[Event], ask: &Ask, words: &str) -> bool {
    answered(events, ask).is_some_and(|r| r.is_ok_and(|s| s.contains(words)))
}

/// Tick until a client the editor waited to start has started, keeping every event; panics once
/// a step it waited on was refused (a delete of a name the store does not hold aside).
fn played(live: &mut Live, setup: &Setup, what: &str) -> Vec<Event> {
    until(live, setup, what, |e, _| {
        if let Some(refused) = e
            .iter()
            .find(|x| matches!(x, Event::Answered(a, Err(_)) if !matches!(a, Ask::Delete(_))))
        {
            panic!("{what}: {refused:?}\n{}", logs(setup));
        }
        e.contains(&Event::Playing).then(|| e.to_vec())
    })
}

/// The server's log.
fn log_of(setup: &Setup) -> String {
    std::fs::read_to_string(setup.log()).unwrap_or_default()
}

/// Recipes' characters played on one server, each made, given its build and put at its start at
/// the server's console: the first made with a build and played where its recipe starts it; a
/// change no table holds, a placed person moved, a vein placed and a camp moved published, each
/// taken without a restart; started again with the character where it stood; sent to its place
/// through portal space; written to a file; its recipe's build changed, given what changed once
/// it has left the world, keeping where it stood; put at another place before it enters; a second
/// character made and played on the same server; that one made anew while it plays; the first no
/// longer a hero; one brought in from the file with a recipe of what it holds; and the server
/// stopped as its operator stops it, keeping them.
#[test]
#[allow(clippy::too_many_lines)] // one playtest, step by step
fn recipes_characters_play_the_world_live_on_one_server_made_built_and_placed_at_its_console() {
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
            // The client runs without its window and its sound: a test makes no noise.
            client_args: vec!["--no-window".into(), "1200".into(), "--no-sound".into()],
            port: 41_300,
        },
        folder: session.dir().join(live::FOLDER),
        base: Some(base),
        key: String::new(),
        clearings: Vec::new(),
    };
    let mut recipe = Recipe::new("Tester");
    recipe.race = chargen::TUMEROK;
    recipe.sex = chargen::FEMALE;
    recipe.start = Start::Place(place.clone());
    recipe.hero = true;
    recipe.level = Some(10);
    recipe.money = Some(500);
    recipe.skills = vec![Skill {
        skill: SKILL,
        rating: 2,
    }];
    recipe.items = vec![Item {
        template: ITEM,
        quantity: 1,
    }];
    recipe.quests = vec![Quest {
        quest: QUEST,
        at: QuestAt::Done,
    }];
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
    let tester = || "Tester".to_string();

    // Made at the console from its recipe with its build, a hero, and in the world at its place.
    live.start(&setup, session.files()).expect("Yalain starts");
    assert!(
        live::lock(&setup.folder).is_err(),
        "the playtest folder is held while it plays"
    );
    live.prepare(&setup, &recipe, Some(entry)).expect("asked");
    live.play_when_ready("Tester", false);
    let seen = played(&mut live, &walking, "Tester made and played");
    assert!(seen.contains(&Event::Up), "{seen:?}");
    assert!(
        answered(&seen, &Ask::Delete(tester())).is_some_and(|r| r.is_err()),
        "no Tester to delete before it is made: {seen:?}"
    );
    assert!(
        done_with(
            &seen,
            &Ask::Make(tester()),
            "character Tester made on tester with its build"
        ),
        "{seen:?}\n{}",
        logs(&setup)
    );
    assert_eq!(
        characters::made(&setup.folder, "Tester"),
        Some(recipe.clone()),
        "written down as made as its recipe says"
    );
    let (cell, at) = until(&mut live, &setup, "our client in the world", |_, c| {
        entered(c)
    });
    assert_eq!(cell >> 16, 0x77B8, "{cell:08X}");
    assert!(near(at, entry.at, 2.0), "{at:?} against {:?}", entry.at);
    let was = until(&mut live, &setup, "the character walked away", |_, c| {
        still(c).filter(|(_, at)| !near(*at, entry.at, WALKED))
    })
    .1;
    assert!(live.in_world("Tester"), "the log says it entered");

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

    // A person moved: taken at once, the person made again where it now stands.
    let pa = session.entry(PERSON).unwrap().at();
    session
        .move_to(PERSON, 0x77B8, [pa.x + 1.0, pa.y, pa.z.unwrap_or_default()])
        .expect("moved");
    assert!(live.publish(&setup, session.files()).expect("published"));
    let answer = until(&mut live, &setup, "the second reload", |e, _| reloaded(e));
    assert_eq!(
        answer,
        Reloaded::Taken {
            applied: vec!["placed objects".into()],
            restart: vec![]
        },
        "{}",
        logs(&setup)
    );
    assert!(
        log_of(&setup).contains("placed objects: 0 new, 1 moved, 0 changed, 0 taken away"),
        "{}",
        logs(&setup)
    );

    // A vein placed beside the person: it stands at once.
    session
        .place(&NewRow {
            kind: NewKind::Static,
            template: VEIN,
            landblock: 0x77B8,
            at: [pa.x - 3.0, pa.y, pa.z.unwrap_or_default()],
            heading: 0.0,
        })
        .expect("a vein placed");
    assert!(live.publish(&setup, session.files()).expect("published"));
    let answer = until(&mut live, &setup, "the third reload", |e, _| reloaded(e));
    let Reloaded::Taken { applied, restart } = answer else {
        panic!("refused: {answer:?}\n{}", logs(&setup));
    };
    assert!(applied.contains(&"veins".to_string()), "{applied:?}");
    assert!(restart.is_empty(), "{restart:?}");
    assert!(
        log_of(&setup).contains("veins: 1 new, 0 taken away"),
        "{}",
        logs(&setup)
    );

    // A camp of the database moved: taken at once, its creatures made again where it now is.
    let ca = session.entry(CAMP).expect("the database's camp").at();
    session
        .move_to(CAMP, 0x77B8, [ca.x + 4.0, ca.y, ca.z.unwrap_or_default()])
        .expect("the camp moved");
    assert!(live.publish(&setup, session.files()).expect("published"));
    let answer = until(&mut live, &setup, "the fourth reload", |e, _| reloaded(e));
    let Reloaded::Taken { applied, restart } = answer else {
        panic!("refused: {answer:?}\n{}", logs(&setup));
    };
    assert_eq!(applied, ["camps"], "{}", logs(&setup));
    assert!(restart.is_empty(), "{restart:?}");
    assert!(
        log_of(&setup).contains("camps: 0 new, 1 changed, 0 taken away"),
        "{}",
        logs(&setup)
    );

    // Restart: the server starts again, keeping the character where it stands.
    live.restart(&setup, session.files())
        .expect("started again");
    let before = std::fs::read_to_string(setup.previous_log()).unwrap();
    assert!(before.contains("stopping now [ok stop-now]"), "{before}");
    assert!(
        before.contains("stopping: 1 characters in the world kept"),
        "{before}"
    );
    let seen = played(&mut live, &setup, "the client again");
    assert!(seen.contains(&Event::Up), "{seen:?}");
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
    let log = log_of(&setup);
    for asked in ["[ok make-character]", "[ok place]", "[ok apply-build]"] {
        assert!(!log.contains(asked), "nothing asked of it again: {log}");
    }

    // Sent back to its place through portal space, as To camera sends it.
    live.teleport("Tester", entry);
    let said = until(&mut live, &setup, "the teleport answered", |e, _| {
        answered(e, &Ask::Teleport(tester()))
    });
    assert!(
        said.as_ref()
            .is_ok_and(|s| s.contains("Tester is on its way to 77B8")),
        "{said:?}"
    );
    until(&mut live, &setup, "the character at its place", |_, c| {
        still(c).filter(|(_, at)| near(*at, entry.at, 2.0))
    });

    // Written to a file for another world, with what it holds.
    let file = characters::export_file(session.dir(), "Tester");
    live.export("Tester", &file).expect("asked");
    let said = until(&mut live, &setup, "the export answered", |e, _| {
        answered(e, &Ask::Export(file.clone()))
    });
    assert!(said.is_ok(), "{said:?}");
    let held = characters::exported(&file).expect("a character file");
    assert_eq!(
        (held.name.as_str(), held.race, held.sex),
        ("Tester", chargen::TUMEROK, chargen::FEMALE)
    );
    // Its build as the file gives it, in a recipe's words: the skill at its rating, a hero,
    // starting where it stood.
    assert_eq!(
        (held.level, held.money, held.hero),
        (Some(10), Some(500), true),
        "{held:?}"
    );
    assert!(
        held.skills.contains(&Skill {
            skill: SKILL,
            rating: 2
        }),
        "{:?}",
        held.skills
    );
    assert_eq!(held.start, Start::Left);
    assert!(
        held.items.iter().any(|i| i.template == ITEM),
        "{:?}",
        held.items
    );
    assert!(
        held.quests.contains(&Quest {
            quest: QUEST,
            at: QuestAt::Done
        }),
        "{:?}",
        held.quests
    );

    // Its build changed while it plays: its client goes, it is taken out of the world at once,
    // and is given what changed, keeping where it stood; nothing is made anew.
    let as_made = recipe.clone();
    recipe.level = Some(12);
    recipe.items[0].quantity = 2;
    characters::save(session.dir(), &recipe).expect("the recipe");
    assert_eq!(
        characters::making(&setup.folder, &recipe),
        Making::Build(as_made)
    );
    live.stop_client();
    live.prepare(&setup, &recipe, None).expect("asked");
    live.play_when_ready("Tester", false);
    let seen = played(&mut live, &setup, "the build given and Tester played");
    assert!(
        done_with(
            &seen,
            &Ask::Logoff(tester()),
            "Tester taken out of the world"
        ),
        "{seen:?}\n{}",
        logs(&setup)
    );
    assert!(
        done_with(
            &seen,
            &Ask::Build(tester()),
            "Tester brought up to its build"
        ),
        "{seen:?}\n{}",
        logs(&setup)
    );
    let log = log_of(&setup);
    let left = log.find("[left \"Tester\"").expect("it left the world");
    let given = log.find("[ok apply-build]").expect("given its build");
    assert!(left < given, "given out of the world: {log}");
    assert!(!log.contains("[ok make-character]"), "not made anew: {log}");
    let changes = std::fs::read_to_string(characters::server_recipe(&setup.folder, "Tester"))
        .expect("its server recipe");
    let t: toml::Table = changes.parse().expect("TOML");
    assert_eq!(t["level"].as_integer(), Some(12), "{changes}");
    assert_eq!(t["items"][0]["quantity"].as_integer(), Some(2), "{changes}");
    for k in ["money", "skills", "quests", "hero", "place"] {
        assert!(
            !t.contains_key(k),
            "what did not change is not given: {changes}"
        );
    }
    assert_eq!(
        characters::made(&setup.folder, "Tester"),
        Some(recipe.clone())
    );
    let (_, at) = until(&mut live, &setup, "Tester where it stood", |_, c| {
        entered(c)
    });
    assert!(
        near(at, entry.at, 2.0),
        "{at:?} where it stood, {:?}",
        entry.at
    );

    // Put elsewhere before it enters: placed at the console once it has left, never teleported.
    let there = Entry {
        cell: outdoor_cell(0x77B8, was[0], was[1]),
        at: was,
        heading: 90.0,
    };
    live.stop_client();
    live.prepare(&setup, &recipe, Some(there)).expect("asked");
    live.play_when_ready("Tester", false);
    let seen = played(&mut live, &setup, "Tester placed and played");
    assert!(
        done_with(&seen, &Ask::Place(tester()), "Tester enters at"),
        "{seen:?}\n{}",
        logs(&setup)
    );
    assert!(answered(&seen, &Ask::Teleport(tester())).is_none());
    let (_, at) = until(&mut live, &setup, "Tester where it was put", |_, c| {
        entered(c)
    });
    assert!(near(at, was, 1.5), "{at:?} where it was put, {was:?}");

    // A second character on the same server: made and played, no restart.
    let port = live.port();
    let mut two = Recipe::new("Tester Two");
    two.start = Start::Place(place.clone());
    characters::save(session.dir(), &two).expect("its recipe");
    live.stop_client();
    live.prepare(&setup, &two, Some(entry)).expect("asked");
    live.play_when_ready("Tester Two", false);
    let seen = played(&mut live, &setup, "Tester Two made and played");
    assert!(
        done_with(
            &seen,
            &Ask::Make("Tester Two".into()),
            "character Tester Two made on tester_two"
        ),
        "{seen:?}\n{}",
        logs(&setup)
    );
    assert_eq!(live.port(), port, "the same server");
    assert!(
        log_of(&setup).contains("[ok apply-build]"),
        "the same log: no restart"
    );
    let (_, at) = until(&mut live, &setup, "Tester Two in the world", |_, c| {
        entered(c)
    });
    assert!(near(at, entry.at, 2.0), "{at:?} against {:?}", entry.at);

    // Made anew while it plays: deleted at once (taken out of the world), made again, played.
    live.reset(&setup, &two, Some(entry)).expect("asked");
    let seen = until(&mut live, &setup, "Tester Two made anew", |e, _| {
        answered(e, &Ask::Make("Tester Two".into())).map(|_| e.to_vec())
    });
    assert!(
        done_with(&seen, &Ask::Delete("Tester Two".into()), "deleted at once"),
        "{seen:?}\n{}",
        logs(&setup)
    );
    assert!(
        done_with(&seen, &Ask::Make("Tester Two".into()), "made on tester_two"),
        "{seen:?}"
    );
    live.play_when_ready("Tester Two", false);
    played(&mut live, &setup, "Tester Two played again");
    until(
        &mut live,
        &setup,
        "Tester Two in the world again",
        |_, c| entered(c),
    );

    // Its recipe no longer a hero: unmade at the console, out of the world, nothing made anew.
    recipe.hero = false;
    characters::save(session.dir(), &recipe).expect("the recipe");
    live.stop_client();
    live.prepare(&setup, &recipe, None).expect("asked");
    let said = until(&mut live, &setup, "the hero unmade", |e, _| {
        answered(e, &Ask::Hero(tester()))
    });
    assert!(
        said.as_ref()
            .is_ok_and(|s| s.contains("Tester is no longer a hero")),
        "{said:?}"
    );
    assert!(!characters::made(&setup.folder, "Tester").unwrap().hero);

    // Brought in from its file as another character, with a recipe of what it holds, and played.
    let mut three = held;
    three.name = "Tester Three".into();
    characters::save(session.dir(), &three).expect("its recipe");
    live.import(&three, &file);
    live.play_when_ready("Tester Three", false);
    let seen = played(&mut live, &setup, "Tester Three brought in and played");
    assert!(
        done_with(
            &seen,
            &Ask::Import("Tester Three".into()),
            "character Tester Three brought from Athenaeum onto tester_three"
        ),
        "{seen:?}\n{}",
        logs(&setup)
    );
    assert_eq!(
        characters::made(&setup.folder, "Tester Three"),
        Some(three.clone())
    );
    assert_eq!(
        characters::making(&setup.folder, &three),
        Making::Same(three.clone()),
        "plays on as it came in"
    );
    until(
        &mut live,
        &setup,
        "the one brought in, in the world",
        |_, c| entered(c),
    );

    // Stopped as its operator stops it: it ends by itself, keeping its characters.
    live.stop_client();
    assert!(
        live.stop_server(),
        "the server ended by itself\n{}",
        logs(&setup)
    );
    let log = log_of(&setup);
    assert!(log.contains("stopping now [ok stop-now]"), "{log}");
    assert!(log.contains("characters in the world kept"), "{log}");
    live.stop_all();
    assert!(live::lock(&setup.folder).is_ok(), "the folder let go");
}

/// A new world over the database, and the session holding it.
fn plain_world(root: &Path, base: &Path, name: &str) -> Session {
    let dir = root.join(name);
    new::new_world(&dir, None).expect("a new world");
    Session::open(world::open(&dir, Some(base)).expect("opens"), None).expect("session")
}

/// Started on ports another program holds (as a second editor or test may take them between the
/// look and the start), the server says it cannot take them and ends, and is started again on the
/// next free ones by itself.
#[test]
fn a_server_started_on_ports_another_program_holds_moves_on_to_free_ones() {
    let install = require_target_install();
    let base = database();
    let scratch = Scratch::new("athenaeum-ports");
    let session = plain_world(&scratch.0, &base, "ports");
    let setup = Setup {
        programs: Programs {
            yalain: Some(program("YALAIN_BIN", "yalain")),
            server_dats: Some(install),
            port: 41_500,
            ..Programs::default()
        },
        folder: session.dir().join(live::FOLDER),
        base: Some(base),
        key: String::new(),
        clearings: Vec::new(),
    };
    let p = live::free_port(live::first_port(41_500, std::process::id())).expect("free ports");
    let held: Vec<UdpSocket> = (0..4)
        .map(|i| UdpSocket::bind((Ipv4Addr::LOCALHOST, p + i)).expect("a port held"))
        .collect();
    let mut live = Live::default();
    live.start_at(&setup, session.files(), p)
        .expect("Yalain starts");
    let seen = until(&mut live, &setup, "the server up on other ports", |e, _| {
        e.contains(&Event::Up).then(|| e.to_vec())
    });
    let moved = seen
        .iter()
        .find_map(|e| match e {
            Event::Moved(q) => Some(*q),
            _ => None,
        })
        .expect("moved on");
    assert!(moved > p + 3 && moved.is_multiple_of(2), "{p} {moved}");
    assert_eq!(live.port(), Some(moved));
    let before = std::fs::read_to_string(setup.previous_log()).unwrap();
    assert!(live::ports_taken(None, &before), "{before}");
    assert!(
        log_of(&setup).contains(&format!("127.0.0.1:{moved}/")),
        "{}",
        logs(&setup)
    );
    drop(held);
    live.stop_all();
}

/// Two playtests started at once on one machine (two editors, or two test runs) whose looks for
/// ports found the same free ones: the server that binds them second says they are taken and is
/// started again on the next free ones, and both serve, each on ports of its own.
#[test]
fn two_playtests_started_at_once_on_the_same_free_ports_each_serve_on_their_own() {
    let install = require_target_install();
    let base = database();
    let scratch = Scratch::new("athenaeum-two");
    let sessions = [
        plain_world(&scratch.0, &base, "one"),
        plain_world(&scratch.0, &base, "two"),
    ];
    let setups: Vec<Setup> = sessions
        .iter()
        .map(|s| Setup {
            programs: Programs {
                yalain: Some(program("YALAIN_BIN", "yalain")),
                server_dats: Some(install.clone()),
                port: 41_900,
                ..Programs::default()
            },
            folder: s.dir().join(live::FOLDER),
            base: Some(base.clone()),
            key: String::new(),
            clearings: Vec::new(),
        })
        .collect();
    let p = live::free_port(live::first_port(41_900, std::process::id())).expect("free ports");
    let mut lives = [Live::default(), Live::default()];
    for ((live, setup), session) in lives.iter_mut().zip(&setups).zip(&sessions) {
        live.start_at(setup, session.files(), p)
            .expect("Yalain starts");
    }
    let began = Instant::now();
    let mut up = [false, false];
    let mut moved = [None, None];
    while !(up[0] && up[1]) {
        for (i, (live, setup)) in lives.iter_mut().zip(&setups).enumerate() {
            for e in live.tick(setup).expect("ticks") {
                match e {
                    Event::Up => up[i] = true,
                    Event::Moved(q) => moved[i] = Some(q),
                    Event::Stopped(tail) => panic!("server {i} stopped: {tail}\n{}", logs(setup)),
                    _ => {}
                }
            }
        }
        assert!(
            began.elapsed() < HANG,
            "both up: {up:?}\n{}\n---\n{}",
            logs(&setups[0]),
            logs(&setups[1])
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    let (a, b) = (lives[0].port().unwrap(), lives[1].port().unwrap());
    assert!(a.abs_diff(b) >= 4, "ports of their own: {a} {b}");
    assert!(moved.iter().any(Option::is_some), "one moved on: {moved:?}");
    for (live, setup) in lives.iter_mut().zip(&setups) {
        let port = live.port().unwrap();
        assert!(
            log_of(setup).contains(&format!("127.0.0.1:{port}/")),
            "{}",
            logs(setup)
        );
        live.stop_all();
    }
}

/// The server the editor starts stops by itself, keeping everyone, once its console's input
/// ends: an editor gone without stopping it (a crash, a kill) leaves nothing holding the ports.
#[test]
fn a_server_whose_console_ends_stops_by_itself_keeping_everyone() {
    let install = require_target_install();
    let base = database();
    let scratch = Scratch::new("athenaeum-console-end");
    let session = plain_world(&scratch.0, &base, "console-end");
    let folder = session.dir().join(live::FOLDER);
    let world = folder.join("world");
    live::write_world(&world, session.files(), &mut Files::new()).expect("the world written");
    let store = folder.join(live::STORE);
    let port = live::free_port(live::first_port(41_700, std::process::id())).expect("free ports");
    let args = live::server_args(
        &install,
        &live::Start {
            world: &world,
            base: Some(&base),
            store: &store,
            port,
        },
    );
    let path = folder.join("yalain.log");
    let log = std::fs::File::create(&path).expect("the log");
    let err = log.try_clone().expect("the log");
    let mut cmd = Command::new(program("YALAIN_BIN", "yalain"));
    cmd.args(&args)
        .current_dir(&folder)
        .stdin(Stdio::piped())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(err));
    windowless(&mut cmd);
    let mut child = cmd.spawn().expect("Yalain starts");
    let console = child.stdin.take().expect("its console");
    let read = || std::fs::read_to_string(&path).unwrap_or_default();
    let began = Instant::now();
    while !read()
        .lines()
        .any(|l| live::said(l) == Some(Said::Listening))
    {
        assert!(
            matches!(child.try_wait(), Ok(None)),
            "it ended before it listened: {}",
            read()
        );
        assert!(began.elapsed() < HANG, "never listened: {}", read());
        std::thread::sleep(Duration::from_millis(100));
    }
    // The editor goes: its end of the console closes.
    drop(console);
    let began = Instant::now();
    let ended = loop {
        if let Ok(Some(status)) = child.try_wait() {
            break status;
        }
        if began.elapsed() > HANG {
            let _ = child.kill();
            panic!("it went on without its console: {}", read());
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    assert!(ended.success(), "{ended:?}: {}", read());
    let log = read();
    assert!(log.contains("stopping: the console's input ended"), "{log}");
    assert!(
        log.contains("stopping: 0 characters in the world kept"),
        "{log}"
    );
}

/// A world with a clearing, played: the server and our client read the one overlay the editor made
/// for the world, and a clearing changed while the server runs is in the folder when it has
/// reloaded.
#[test]
fn a_clearing_is_read_by_the_server_and_by_the_client_play_starts() {
    let install = require_target_install();
    let base = database();
    let scratch = Scratch::new("athenaeum-clearing");
    let mut session = plain_world(&scratch.0, &base, "clearing");
    let item = session
        .place_clearing(0xB590, [75.0, 45.0], ClearingShape::Round { radius: 25.0 })
        .expect("the clearing");
    let setup_of = |session: &Session| Setup {
        programs: Programs {
            yalain: Some(program("YALAIN_BIN", "yalain")),
            server_dats: Some(install.clone()),
            client: Some(program("AUBEREAN_BIN", "auberean")),
            client_dats: Some(install.clone()),
            retail: None,
            client_args: vec!["--no-window".into(), "1200".into(), "--no-sound".into()],
            port: 41_700,
        },
        folder: session.dir().join(live::FOLDER),
        base: Some(base.clone()),
        key: session.key().to_owned(),
        clearings: session
            .clearings()
            .into_iter()
            .map(|(c, l)| (c.clone(), l))
            .collect(),
    };
    let setup = setup_of(&session);
    let mut live = Live::default();
    live.start(&setup, session.files()).expect("Yalain starts");
    until(&mut live, &setup, "the server up", |e, _| {
        e.contains(&Event::Up).then_some(())
    });
    assert!(
        log_of(&setup).contains("overlay"),
        "the server reads it: {}",
        logs(&setup)
    );
    let folder = setup.world().join(WORLD_FOLDER);
    let container = folder.join("overlay_cell.dat");
    let first = std::fs::read(&container).expect("the overlay");

    // Played, the client is told the folder.
    let recipe = Recipe::new("Tester");
    live.prepare(&setup, &recipe, None).expect("the character");
    live.play_when_ready("Tester", false);
    played(&mut live, &setup, "Tester made and played");
    let client_log = setup.folder.join("client.log");
    until(&mut live, &setup, "the client reading it", |_, _| {
        std::fs::read_to_string(&client_log)
            .ok()
            .filter(|t| t.contains("through a world's overlay"))
    });

    // Changed as it runs: the folder is replaced under the server and the client, and the server
    // takes it up.
    session
        .set(&item, "radius", "40.0")
        .expect("a bigger clearing");
    let setup = setup_of(&session);
    assert!(live.publish(&setup, session.files()).expect("published"));
    until(&mut live, &setup, "the reload", |e, _| {
        reloaded(e).map(|_| ())
    });
    let second = std::fs::read(&container).expect("the overlay");
    assert_ne!(first, second);
    let world = auberean_dat::overlay::WorldOverlay::of_world(&setup.world())
        .expect("reads")
        .expect("an overlay");
    assert_eq!(world.world(), session.key());
    live.stop_all();
}
