//! Behaviour: none (checks the world builder's playtest without starting the server: the recipes of its characters, what the server is told, the world written for it, its answers read, its folder held, its ports chosen, and the script steps that drive it)

use std::net::{Ipv4Addr, UdpSocket};
use std::path::{Path, PathBuf};

use athenaeum_editor::characters::{self, Recipe, Start};
use athenaeum_editor::live::{self, Entry, Reloaded};
use athenaeum_editor::script::{parse, Step};
use auberean_world::chargen;
use yalain_world_db::Files;

/// A folder of the test's own, empty.
fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("athenaeum-cpu-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn a_recipe_reads_back_as_written_and_refuses_what_the_server_would() {
    let mut r = Recipe::new("Ellis Vane");
    assert_eq!(
        (r.race, r.sex, &r.start),
        (chargen::HUMAN, chargen::MALE, &Start::Camera)
    );
    r.race = chargen::TUMEROK;
    r.sex = chargen::FEMALE;
    r.start = Start::Place("arwic-gate".into());
    r.hero = true;
    let text = r.text();
    assert!(text.contains("race = \"tumerok\""), "{text}");
    assert!(text.contains("start = \"place arwic-gate\""), "{text}");
    assert_eq!(Recipe::parse("Ellis Vane", &text).unwrap(), r);
    // What a recipe leaves out is a human man at the camera, no hero.
    assert_eq!(Recipe::parse("Ann", "").unwrap(), Recipe::new("Ann"));
    for (start, want) in [
        ("camera", Start::Camera),
        ("selection", Start::Selection),
        ("left", Start::Left),
        ("place inn", Start::Place("inn".into())),
    ] {
        assert_eq!(Start::parse(start).unwrap(), want);
        assert_eq!(Start::parse(&want.word()).unwrap(), want);
    }
    assert!(Start::parse("place ").is_err());
    assert!(Start::parse("north").is_err());
    assert!(Recipe::parse("Ann", "race = \"gromnie\"").is_err());
    assert!(Recipe::parse("Ann", "sex = \"both\"").is_err());
    assert!(
        Recipe::parse("Ann", "level = 50").is_err(),
        "a field the server cannot make"
    );
    assert!(Recipe::parse("Ann", "hero = \"yes\"").is_err());
    // Names as the server takes them: 2 to 16 English letters, spaces, apostrophes and hyphens,
    // a letter at each end, never two spaces together.
    for good in [
        "Al",
        "O'Neil",
        "Hand Of Doom",
        "Mary-Kate",
        "Sixteen Letters",
    ] {
        assert!(characters::check_name(good).is_ok(), "{good}");
    }
    for bad in [
        "A",
        "Seventeen Letters",
        "Zoë",
        "Two  Spaces",
        " Lead",
        "Trail-",
        "R2D2",
    ] {
        assert!(characters::check_name(bad).is_err(), "{bad}");
    }
}

#[test]
fn recipes_are_kept_beside_the_world_and_listed_by_name() {
    let world = scratch("recipes");
    let mut b = Recipe::new("Bea");
    b.sex = chargen::FEMALE;
    characters::save(&world, &b).unwrap();
    characters::save(&world, &Recipe::new("al")).unwrap();
    std::fs::write(characters::file(&world, "Broken"), "race = 3").unwrap();
    assert!(characters::save(&world, &Recipe::new("X")).is_err());
    let (list, problems) = characters::list(&world);
    let names: Vec<&str> = list.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["al", "Bea"]);
    assert_eq!(list[1], b);
    assert_eq!(problems.len(), 1, "{problems:?}");
    let playtest = world.join(live::FOLDER);
    characters::mark_made(&playtest, &b).unwrap();
    characters::delete(&world, &playtest, "Bea").unwrap();
    assert!(!characters::file(&world, "Bea").exists());
    assert!(!characters::made_dir(&playtest, "Bea").exists());
    std::fs::remove_dir_all(&world).ok();
}

#[test]
fn a_character_is_made_when_new_and_made_anew_only_when_its_body_changes() {
    let playtest = scratch("made");
    let mut r = Recipe::new("Cato");
    // Never played: made.
    assert_eq!(characters::to_make(&playtest, &r), (true, false));
    // Made, with its store: plays on, whatever its start or hero become.
    characters::mark_made(&playtest, &r).unwrap();
    std::fs::write(characters::store(&playtest, "Cato"), b"").unwrap();
    r.start = Start::Left;
    r.hero = true;
    assert_eq!(characters::to_make(&playtest, &r), (false, false));
    // Another race or sex: its store taken away and the character made anew.
    r.race = chargen::LUGIAN;
    assert_eq!(characters::to_make(&playtest, &r), (true, true));
    r.race = chargen::HUMAN;
    r.sex = chargen::FEMALE;
    assert_eq!(characters::to_make(&playtest, &r), (true, true));
    // Reset: made again from its recipe at the next play.
    characters::reset(&playtest, "Cato").unwrap();
    assert_eq!(characters::to_make(&playtest, &r), (true, false));
    std::fs::remove_dir_all(&playtest).ok();
}

#[test]
fn yalain_is_started_on_loopback_over_the_worlds_base_in_the_characters_own_store() {
    let mut r = Recipe::new("Dain Rook");
    r.race = chargen::LUGIAN;
    r.sex = chargen::FEMALE;
    let entry = Entry {
        cell: 0x77B8_0005,
        at: [6.0, 86.0, 183.0],
        heading: 90.0,
    };
    let start = live::Start {
        world: Path::new("w"),
        base: Some(Path::new("db")),
        store: Path::new("store.sqlite"),
        port: 9300,
        recipe: &r,
        make: true,
        entry: Some(entry),
    };
    let a = live::server_args(Path::new("install"), &start);
    let at = |a: &[String], flag: &str| a.iter().position(|x| x == flag).map(|i| a[i + 1].clone());
    assert_eq!(at(&a, "--world-db").as_deref(), Some("w"));
    assert_eq!(at(&a, "--world-base").as_deref(), Some("db"));
    assert_eq!(at(&a, "--dats").as_deref(), Some("install"));
    assert_eq!(at(&a, "--data").as_deref(), Some("store.sqlite"));
    assert_eq!(at(&a, "--bind").as_deref(), Some("127.0.0.1"));
    assert_eq!(at(&a, "--login-port").as_deref(), Some("9300"));
    assert_eq!(at(&a, "--world-port").as_deref(), Some("9302"));
    assert_eq!(
        at(&a, "--add-character").as_deref(),
        Some("builder:Dain Rook:2:8192")
    );
    assert_eq!(
        at(&a, "--place").as_deref(),
        Some("builder:Dain Rook:77B80005:6:86:183:90")
    );
    assert!(!a.iter().any(|x| x == "--memory"), "the store is kept");
    // Made before, entering where it left: neither made nor placed.
    let again = live::server_args(
        Path::new("install"),
        &live::Start {
            make: false,
            entry: None,
            base: None,
            ..start
        },
    );
    for flag in ["--add-character", "--place", "--world-base"] {
        assert!(!again.iter().any(|x| x == flag), "{flag}: {again:?}");
    }
}

#[test]
fn the_world_is_written_where_it_changed_and_its_files_gone_go() {
    let dir = scratch("world");
    let mut files = Files::new();
    files.insert("world.toml".into(), "key = \"w\"\n".into());
    files.insert("fill-w/server/places.toml".into(), "[[place]]\n".into());
    let mut written = Files::new();
    assert_eq!(live::write_world(&dir, &files, &mut written).unwrap(), 2);
    assert_eq!(
        std::fs::read_to_string(dir.join("fill-w/server/places.toml")).unwrap(),
        "[[place]]\n"
    );
    assert_eq!(
        live::write_world(&dir, &files, &mut written).unwrap(),
        0,
        "nothing changed"
    );
    files.insert(
        "fill-w/server/landblocks/77B8.toml".into(),
        "[[object]]\n".into(),
    );
    files.remove("fill-w/server/places.toml");
    assert_eq!(live::write_world(&dir, &files, &mut written).unwrap(), 2);
    assert!(!dir.join("fill-w/server/places.toml").exists());
    assert!(dir.join("fill-w/server/landblocks/77B8.toml").exists());
    assert_eq!(written, files);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_reload_is_answered_by_the_first_whole_line_that_answers_it() {
    let taken = "2026-11-01T10:00:00Z  INFO yalain: reloading the world database\n\
                 2026-11-01T10:00:01Z  INFO yalain: world database C:\\w\\.playtest\\world: \
                 reloaded: talk, quests; needs a restart: placed objects, camps; unchanged: \
                 loot\n";
    assert_eq!(
        live::reload_answer(taken),
        Some(Reloaded::Taken {
            applied: vec!["talk".into(), "quests".into()],
            restart: vec!["placed objects".into(), "camps".into()],
        })
    );
    let nothing = "x: world database w: reloaded: nothing; needs a restart: nothing; unchanged: \
                   templates\n";
    assert_eq!(
        live::reload_answer(nothing),
        Some(Reloaded::Taken {
            applied: vec![],
            restart: vec![],
        })
    );
    let refused = " WARN yalain: the world database did not compile; the world is unchanged: \
                   fill-w/server/places.toml:3: not TOML\nmore of the report\n";
    assert_eq!(
        live::reload_answer(refused),
        Some(Reloaded::Refused(
            "fill-w/server/places.toml:3: not TOML".into()
        ))
    );
    // A line still being written answers nothing yet; nor does a log with no answer.
    let half = &taken[..taken.len() - 10];
    assert_eq!(live::reload_answer(half), None);
    assert_eq!(live::reload_answer("INFO yalain: reloading\n"), None);
}

#[test]
fn a_second_playtest_of_one_world_is_refused_while_the_first_holds_it() {
    let folder = scratch("lock");
    let first = live::lock(&folder).unwrap();
    let second = live::lock(&folder).unwrap_err();
    assert!(second.contains("another editor"), "{second}");
    drop(first);
    assert!(live::lock(&folder).is_ok(), "free once the first lets go");
    std::fs::remove_dir_all(&folder).ok();
}

#[test]
fn the_login_port_taken_is_the_first_even_one_whose_ports_are_all_free() {
    let p = live::free_port(41_000).unwrap();
    assert!(p.is_multiple_of(2));
    assert!(p >= 41_000);
    // Another program holding the world's port above it moves the server on.
    let held = UdpSocket::bind((Ipv4Addr::LOCALHOST, p + 3)).unwrap();
    let q = live::free_port(p).unwrap();
    assert!(q > p + 3 && q.is_multiple_of(2), "{p} {q}");
    drop(held);
    // An odd port asked for starts at the even one below it.
    assert!(live::free_port(p + 1).unwrap().is_multiple_of(2));
}

#[test]
fn a_script_makes_chooses_resets_and_plays_characters() {
    let text = "character Ann_Bo tumerok female place arwic-gate hero\n\
                character Cy human male\nas Ann_Bo\nlive\nrestart\nreset Ann_Bo\nstop\nserve\n\
                characters\n";
    let steps = parse(text, Path::new("shots")).unwrap();
    let Step::Character(r) = &steps[0] else {
        panic!("{:?}", steps[0]);
    };
    assert_eq!(r.name, "Ann Bo");
    assert_eq!((r.race, r.sex), (chargen::TUMEROK, chargen::FEMALE));
    assert_eq!(r.start, Start::Place("arwic-gate".into()));
    assert!(r.hero);
    let Step::Character(r) = &steps[1] else {
        panic!("{:?}", steps[1]);
    };
    assert_eq!((&r.start, r.hero), (&Start::Camera, false));
    assert_eq!(
        steps[2..],
        [
            Step::As("Ann Bo".into()),
            Step::Serve,
            Step::Restart,
            Step::Reset("Ann Bo".into()),
            Step::Stop,
            Step::Serve,
            Step::Characters,
        ]
    );
    assert!(parse("character Ann gromnie male camera", Path::new("."))
        .unwrap_err()
        .contains("line 1"));
}
