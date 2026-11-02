//! Behaviour: none (checks the world builder's playtest without starting the server: the recipes of its characters and their builds, what the server is told at its start and its console, the world written for it, its answers read by their tags, its folder held, its ports chosen, a character file read into a recipe, and the script steps that drive it)

use std::net::{Ipv4Addr, UdpSocket};
use std::path::{Path, PathBuf};

use athenaeum_editor::app::play::{matching, number, pick};
use athenaeum_editor::characters::{self, Item, Making, Quest, QuestAt, Recipe, Skill, Start};
use athenaeum_editor::live::{self, Ask, Entry, Reloaded, Said};
use athenaeum_editor::script::{parse, Step};
use auberean_protocol::types::Qualities;
use auberean_protocol::wire::{Field as _, Writer};
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
        Recipe::parse("Ann", "height = 50").is_err(),
        "a field no recipe has"
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
fn a_character_is_made_when_new_given_what_changed_of_its_build_and_made_anew_for_another_body() {
    let playtest = scratch("made");
    let store = playtest.join(live::STORE);
    let mut r = Recipe::new("Cato");
    // Never played: made.
    assert_eq!(characters::making(&playtest, &r), Making::Make);
    // Written down as made, but no store holds it: made.
    characters::mark_made(&playtest, &r).unwrap();
    assert_eq!(characters::making(&playtest, &r), Making::Make);
    // Made, in the store: plays on, whatever its start or hero become.
    std::fs::write(&store, b"").unwrap();
    let made = r.clone();
    assert_eq!(
        characters::making(&playtest, &r),
        Making::Same(made.clone())
    );
    r.start = Start::Left;
    r.hero = true;
    assert_eq!(
        characters::making(&playtest, &r),
        Making::Same(made.clone())
    );
    // Another build: given what changed.
    for change in [
        "level = 2",
        "experience = 900",
        "money = 1",
        "skills = [{ skill = 7, rating = 2 }]",
        "items = [{ template = 0x4700051C }]",
        "quests = [{ quest = 0x400001CA, phase = \"done\" }]",
    ] {
        let changed = r.with_line(change).unwrap();
        assert_eq!(
            characters::making(&playtest, &changed),
            Making::Build(made.clone()),
            "{change}"
        );
    }
    // Another race or sex: made anew.
    r.race = chargen::LUGIAN;
    assert_eq!(characters::making(&playtest, &r), Making::Make);
    r.race = chargen::HUMAN;
    r.sex = chargen::FEMALE;
    assert_eq!(characters::making(&playtest, &r), Making::Make);
    r.sex = chargen::MALE;
    // Reset: made anew from its recipe at its next play.
    characters::unmark(&playtest, "Cato").unwrap();
    assert_eq!(characters::making(&playtest, &r), Making::Make);
    // One an earlier editor kept in a store of its own is made anew in the one store, and that
    // store goes; so does every record of what was made once the one store is gone.
    characters::mark_made(&playtest, &r).unwrap();
    let own = characters::made_dir(&playtest, "Cato").join("yalain.sqlite");
    std::fs::write(&own, b"").unwrap();
    assert_eq!(characters::making(&playtest, &r), Making::Make);
    characters::forget_unstored(&playtest).unwrap();
    assert!(!characters::made_dir(&playtest, "Cato").exists());
    characters::mark_made(&playtest, &r).unwrap();
    characters::forget_unstored(&playtest).unwrap();
    assert!(
        characters::made(&playtest, "Cato").is_some(),
        "kept while the store holds it"
    );
    std::fs::remove_file(&store).unwrap();
    characters::forget_unstored(&playtest).unwrap();
    assert!(characters::made(&playtest, "Cato").is_none());
    std::fs::remove_dir_all(&playtest).ok();
}

#[test]
fn a_changed_build_is_given_as_what_changed_and_nothing_it_holds_already() {
    let mut made = Recipe::new("Hand Of Doom");
    made.race = chargen::TUMEROK;
    made.sex = chargen::FEMALE;
    made.level = Some(10);
    made.money = Some(500);
    made.skills = vec![Skill {
        skill: 7,
        rating: 2,
    }];
    made.items = vec![Item {
        template: 0x4700_051C,
        quantity: 1,
    }];
    made.quests = vec![Quest {
        quest: 0x4000_01CA,
        at: QuestAt::Done,
    }];
    let read = |text: &str| -> toml::Table { text.parse().expect("TOML") };
    // Nothing changed but where it enters: the character and its place alone.
    let t = read(&made.changes_text(&made, Some((0x77B8_0006, [4.4, 100.8, 182.5], 90.0))));
    assert_eq!(t["account"].as_str(), Some("hand_of_doom"));
    assert_eq!(t["name"].as_str(), Some("Hand Of Doom"));
    assert_eq!(t["race"].as_integer(), Some(i64::from(chargen::TUMEROK)));
    assert!(t.contains_key("place"));
    for k in [
        "level",
        "experience",
        "money",
        "hero",
        "skills",
        "items",
        "quests",
    ] {
        assert!(!t.contains_key(k), "{k}: {t:?}");
    }
    // Its level up, a skill raised, another item, more of one, and a hero: those alone.
    let mut r = made.clone();
    r.level = Some(12);
    r.hero = true;
    r.skills.push(Skill {
        skill: 12,
        rating: 1,
    });
    r.items[0].quantity = 3;
    r.items.push(Item {
        template: 0x4700_0001,
        quantity: 1,
    });
    let t = read(&r.changes_text(&made, None));
    assert_eq!(t["level"].as_integer(), Some(12));
    assert_eq!(t["hero"].as_bool(), Some(true));
    let skills = t["skills"].as_array().unwrap();
    assert_eq!(skills.len(), 1, "the raised one alone: {skills:?}");
    assert_eq!(skills[0]["skill"].as_integer(), Some(12));
    let items = t["items"].as_array().unwrap();
    assert_eq!(items.len(), 2, "{items:?}");
    assert_eq!(items[0]["quantity"].as_integer(), Some(3));
    for k in ["money", "quests", "place", "experience"] {
        assert!(!t.contains_key(k), "{k}: {t:?}");
    }
    // A level given as experience now: the experience.
    let t = read(
        &made
            .with_line("experience = 250000")
            .unwrap()
            .changes_text(&made, None),
    );
    assert_eq!(t["experience"].as_integer(), Some(250_000));
    assert!(!t.contains_key("level"));
}

#[test]
fn yalain_is_started_on_loopback_over_the_worlds_base_in_the_playtest_store_to_stop_when_its_console_ends(
) {
    let start = live::Start {
        world: Path::new("w"),
        base: Some(Path::new("db")),
        store: Path::new("store.sqlite"),
        port: 9300,
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
    // An editor gone without stopping it leaves no server holding the ports.
    assert!(a.iter().any(|x| x == "--stop-at-console-end"), "{a:?}");
    // Its characters are asked for at its console, never at its start.
    for flag in ["--place", "--add-character", "--make-character", "--memory"] {
        assert!(!a.iter().any(|x| x == flag), "{flag}: {a:?}");
    }
    let bare = live::server_args(
        Path::new("install"),
        &live::Start {
            base: None,
            ..start
        },
    );
    assert!(!bare.iter().any(|x| x == "--world-base"), "{bare:?}");
}

#[test]
fn each_playtest_character_is_played_from_an_account_of_its_own() {
    assert_eq!(live::account_of("Hand Of Doom"), "hand_of_doom");
    assert_eq!(live::account_of("O'Neil"), "o'neil");
    assert_ne!(live::account_of("Ann Bo"), live::account_of("Ann-Bo"));
    assert_ne!(live::account_of("Ann Bo"), live::account_of("Annbo"));
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

/// A line of the server's log as it writes one: its time, a level, where it came from, the words.
fn line(level: &str, from: &str, words: &str) -> String {
    format!("2026-11-01T14:06:47.884225Z  {level} {from}: {words}")
}

#[test]
fn a_reload_is_answered_by_its_tag_its_tables_listed_where_the_words_name_them() {
    let taken = line(
        "INFO",
        "yalain",
        "world database C:\\w\\.playtest\\world: reloaded: talk, quests; needs a restart: \
         placed objects, camps; unchanged: loot [ok reload]",
    );
    let Some(Said::Done { command, words }) = live::said(&taken) else {
        panic!("{taken}");
    };
    assert_eq!(command, "reload");
    assert_eq!(
        live::reloaded(true, &words),
        Reloaded::Taken {
            applied: vec!["talk".into(), "quests".into()],
            restart: vec!["placed objects".into(), "camps".into()],
        }
    );
    assert_eq!(
        live::reloaded(
            true,
            "world database w: reloaded: nothing; needs a restart: nothing; unchanged: templates"
        ),
        Reloaded::Taken {
            applied: vec![],
            restart: vec![],
        }
    );
    // Words that no longer read as they did still answer by the tag: taken.
    assert_eq!(
        live::reloaded(true, "the world database is new"),
        Reloaded::Taken {
            applied: vec![],
            restart: vec![],
        }
    );
    let refused = line(
        "WARN",
        "yalain",
        "the world database did not compile; the world is unchanged: \
         fill-w/server/places.toml:3: not TOML [refused reload]",
    );
    let Some(Said::Refused { command, words }) = live::said(&refused) else {
        panic!("{refused}");
    };
    assert_eq!(command, "reload");
    assert_eq!(
        live::reloaded(false, &words),
        Reloaded::Refused("fill-w/server/places.toml:3: not TOML".into())
    );
    assert_eq!(
        live::reloaded(false, "a reload is already running"),
        Reloaded::Refused("a reload is already running".into())
    );
    // Its start, and the lines after it, answer nothing.
    for nothing in [
        line("INFO", "yalain", "reloading the world database"),
        line("INFO", "yalain", "camps: 0 new, 1 changed, 0 taken away"),
    ] {
        assert_eq!(live::said(&nothing), None, "{nothing}");
    }
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

/// Two editors (or test runs) look for ports first from places of their own, by their process
/// ids: a step apart for neighboring ids, even, within the steps above the port asked for, and
/// the same each time for one process.
#[test]
fn each_process_looks_for_its_ports_first_from_a_step_of_its_own() {
    let from = 9300;
    let span = u16::try_from(live::PROCESS_SLOTS).unwrap() * live::PROCESS_STEP;
    let looks: Vec<u16> = (4000..4000 + live::PROCESS_SLOTS)
        .map(|id| live::first_port(from, id))
        .collect();
    for (i, p) in looks.iter().enumerate() {
        assert!(p.is_multiple_of(2) && *p >= from && *p < from + span, "{p}");
        for q in &looks[i + 1..] {
            assert!(p.abs_diff(*q) >= live::PROCESS_STEP, "{p} {q}");
        }
    }
    assert_eq!(live::first_port(from, 4321), live::first_port(from, 4321));
    assert_eq!(
        live::first_port(9301, 0),
        9300,
        "an odd port begins at the even one below"
    );
}

#[test]
fn a_server_that_could_not_take_its_ports_says_so_by_its_exit_code_or_its_tag() {
    let taken = line(
        "ERROR",
        "yalain",
        "binding the ports: 127.0.0.1:9300-9301 and 9302-9303: Only one usage of each socket \
         address is normally permitted. (os error 10048) [ports-taken]",
    );
    let other = line("INFO", "yalain", "stopping: 0 characters in the world kept");
    // Its exit code says it, whatever the log's words.
    assert!(live::ports_taken(Some(live::PORTS_TAKEN), &other));
    assert!(!live::ports_taken(Some(1), &taken), "another exit");
    assert!(!live::ports_taken(Some(0), &other));
    // With no exit code (ended by a signal), its log's tag.
    assert!(live::ports_taken(None, &format!("{other}\n{taken}\n")));
    assert!(!live::ports_taken(None, &other));
    // The words alone, as an older server wrote them, are not the tag.
    assert!(!live::ports_taken(
        None,
        "yalain: binding the ports: Only one usage of each socket address is normally permitted.\n"
    ));
    assert_eq!(live::said(&taken), Some(Said::PortsTaken));
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
    let steps = parse(
        "recipe Ann_Bo level = 10\nrecipe Ann_Bo items = [{ template = 0x4700051C }]\n\
         export Ann_Bo\nimport w/kara.json\nimport w/kara.json Kara_Two\nteleport\n",
        Path::new("shots"),
    )
    .unwrap();
    assert_eq!(
        steps,
        [
            Step::Recipe("Ann Bo".into(), "level = 10".into()),
            Step::Recipe(
                "Ann Bo".into(),
                "items = [{ template = 0x4700051C }]".into()
            ),
            Step::Export("Ann Bo".into()),
            Step::Import(PathBuf::from("w/kara.json"), String::new()),
            Step::Import(PathBuf::from("w/kara.json"), "Kara Two".into()),
            Step::Teleport,
        ]
    );
}

#[test]
fn a_recipes_build_reads_back_as_written_and_refuses_what_no_player_could_hold() {
    let mut r = Recipe::new("Hand Of Doom");
    r.level = Some(20);
    r.money = Some(5000);
    r.skills = vec![Skill {
        skill: 15,
        rating: 10,
    }];
    r.items = vec![Item {
        template: 0x4700_051C,
        quantity: 2,
    }];
    r.quests = vec![
        Quest {
            quest: 0x8000_00E0,
            at: QuestAt::Done,
        },
        Quest {
            quest: 0x8000_00E1,
            at: QuestAt::Phase(2),
        },
    ];
    let text = r.text();
    assert!(text.contains("level = 20\n"), "{text}");
    assert!(
        text.contains("items = [{ template = 0x4700051C, quantity = 2 }]"),
        "{text}"
    );
    assert!(
        text.contains(
            "quests = [{ quest = 0x800000E0, phase = \"done\" }, { quest = 0x800000E1, phase = 2 }]"
        ),
        "{text}"
    );
    assert_eq!(Recipe::parse("Hand Of Doom", &text).unwrap(), r);
    // A quantity left out is one; a rating left out trains the skill without raising it.
    let short = Recipe::parse(
        "Ann",
        "items = [{ template = 0x47000001 }]\nskills = [{ skill = 3 }]",
    )
    .unwrap();
    assert_eq!(short.items[0].quantity, 1);
    assert_eq!(short.skills[0].rating, 0);
    for bad in [
        "level = 0",
        "level = 5\nexperience = 100",
        "money = -1",
        "skills = [{ skill = 3, level = 2 }]",
        "skills = [{ rating = 2 }]",
        "items = [{ template = 0x47000001, quantity = 0 }]",
        "items = 3",
        "quests = [{ quest = 0x80000001, phase = \"half\" }]",
        "quests = [{ quest = 0x80000001, phase = 0 }]",
        "quests = [{ quest = 0x80000001 }]",
    ] {
        assert!(Recipe::parse("Ann", bad).is_err(), "{bad}");
    }
    // A line sets a field; a level takes an experience away, and an experience a level.
    let with = r.with_line("experience = 250000").unwrap();
    assert_eq!((with.level, with.experience), (None, Some(250_000)));
    let with = with.with_line("level = 3").unwrap();
    assert_eq!((with.level, with.experience), (Some(3), None));
    assert_eq!(with.items, r.items, "the rest kept");
    assert!(r.with_line("level = ").is_err());
}

#[test]
fn the_servers_recipe_is_the_character_on_its_own_account_entering_where_it_starts() {
    let mut r = Recipe::new("Hand Of Doom");
    r.race = chargen::TUMEROK;
    r.sex = chargen::FEMALE;
    r.hero = true;
    r.level = Some(10);
    r.skills = vec![Skill {
        skill: 7,
        rating: 2,
    }];
    r.items = vec![Item {
        template: 0x4700_051C,
        quantity: 1,
    }];
    r.quests = vec![Quest {
        quest: 0x4000_01CA,
        at: QuestAt::Done,
    }];
    let text = r.server_text(Some((0x77B8_0006, [4.4, 100.8, 182.5], 90.0)));
    let t: toml::Table = text.parse().expect("TOML");
    let int = |k: &str| t[k].as_integer().unwrap();
    assert_eq!(t["account"].as_str(), Some("hand_of_doom"));
    assert_eq!(t["name"].as_str(), Some("Hand Of Doom"));
    assert_eq!(
        (int("race"), int("sex"), int("level")),
        (i64::from(chargen::TUMEROK), i64::from(chargen::FEMALE), 10)
    );
    assert_eq!(t["hero"].as_bool(), Some(true));
    let place = t["place"].as_table().unwrap();
    assert_eq!(place["cell"].as_integer(), Some(0x77B8_0006));
    let f = |k: &str| place[k].as_float().unwrap();
    assert!((f("x") - 4.4).abs() < 1e-4 && (f("y") - 100.8).abs() < 1e-4);
    assert!((f("z") - 182.5).abs() < 1e-4 && (f("heading") - 90.0).abs() < 1e-4);
    assert_eq!(t["skills"][0]["skill"].as_integer(), Some(7));
    assert_eq!(t["items"][0]["template"].as_integer(), Some(0x4700_051C));
    assert_eq!(t["quests"][0]["phase"].as_str(), Some("done"));
    for k in ["start", "experience", "money"] {
        assert!(!t.contains_key(k), "{k}: {text}");
    }
    // Entering where it was put: no place.
    let t: toml::Table = r.server_text(None).parse().unwrap();
    assert!(!t.contains_key("place"));
}

#[test]
fn the_console_is_told_names_and_files_whole() {
    let to = Entry {
        cell: 0x77B8_0006,
        at: [4.5, 100.25, 182.0],
        heading: 90.0,
    };
    assert_eq!(
        live::teleport_line("Hand Of Doom", &to),
        "teleport \"Hand Of Doom\" to 77B80006 4.5 100.25 182 90"
    );
    assert_eq!(
        live::place_line("Hand Of Doom", &to),
        "place \"Hand Of Doom\" to 77B80006 4.5 100.25 182 90"
    );
    assert_eq!(live::hero_line("Kara Off", true), "hero \"Kara Off\"");
    assert_eq!(live::hero_line("Kara", false), "hero \"Kara\" off");
    assert_eq!(
        live::make_line(Path::new("my world/.playtest/characters/Kara/recipe.toml")),
        "make-character \"my world/.playtest/characters/Kara/recipe.toml\""
    );
    assert_eq!(
        live::build_line(Path::new("w/recipe.toml")),
        "apply-build \"w/recipe.toml\""
    );
    assert_eq!(
        live::delete_line("Now Then"),
        "deletecharacter \"Now Then\" now"
    );
    assert_eq!(live::logoff_line("Sir Kara"), "forcelogoff \"Sir Kara\"");
    assert_eq!(
        live::export_line("Hand Of Doom", Path::new("my worlds/hand.json")),
        "export-character \"Hand Of Doom\" \"my worlds/hand.json\""
    );
    assert_eq!(
        live::import_line(Path::new("hand.json"), "Hand Of Glory"),
        "import-character \"hand.json\" hand_of_glory \"Hand Of Glory\""
    );
    // Each answer is read under its command's own name.
    for (ask, command) in [
        (Ask::Make("K".into()), "make-character"),
        (Ask::Build("K".into()), "apply-build"),
        (Ask::Place("K".into()), "place"),
        (Ask::Hero("K".into()), "hero"),
        (Ask::Delete("K".into()), "deletecharacter"),
        (Ask::Logoff("K".into()), "forcelogoff"),
        (Ask::Teleport("K".into()), "teleport"),
        (Ask::Export(PathBuf::from("k.json")), "export-character"),
        (Ask::Import("K".into()), "import-character"),
    ] {
        assert_eq!(ask.command(), command);
    }
}

#[test]
fn the_servers_answers_are_read_by_the_tags_that_end_them() {
    let said = |level: &str, from: &str, words: &str| live::said(&line(level, from, words));
    assert_eq!(
        said(
            "INFO",
            "yalain",
            "Kara is on its way to 77B80006 [4.5, 100.25, 182.0] [ok teleport]"
        ),
        Some(Said::Done {
            command: "teleport".into(),
            words: "Kara is on its way to 77B80006 [4.5, 100.25, 182.0]".into(),
        }),
        "the words' own brackets are the words'"
    );
    assert_eq!(
        said(
            "WARN",
            "yalain",
            "place: Kara is in the world: teleport it instead [refused place]"
        ),
        Some(Said::Refused {
            command: "place".into(),
            words: "place: Kara is in the world: teleport it instead".into(),
        })
    );
    assert_eq!(
        said(
            "INFO",
            "yalain",
            "in the world at 77B80005 [1.2, 3.4, 5.6] [entered \"Kara Vey\" 0x2130000000000001]"
        ),
        Some(Said::Entered("Kara Vey".into()))
    );
    assert_eq!(
        said(
            "INFO",
            "yalain",
            "left the world at 77B80005 [left \"Kara Vey\" 0x2130000000000001]"
        ),
        Some(Said::Left("Kara Vey".into()))
    );
    assert_eq!(
        said(
            "INFO",
            "yalain",
            "listening on 127.0.0.1:9300/9301 (login) and 127.0.0.1:9302/9303 (world, referred \
             as 127.0.0.1) [listening]"
        ),
        Some(Said::Listening)
    );
    // A line of the world's crates, untagged words, a line still being written, a tag that is
    // none of these, and a line written straight to the log say nothing.
    for nothing in [
        line(
            "INFO",
            "yalain_world::world",
            "in the world id=0x2130000000000001 [ok teleport]",
        ),
        line(
            "INFO",
            "yalain",
            "Kara is on its way to 77B80006 [4.5, 100.25, 182.0]",
        ),
        line("INFO", "yalain", "Kara deleted at once [ok deletecharacter"),
        line("INFO", "yalain", "who knows [maybe so]"),
        line("INFO", "yalain", "in the world [entered \"Kara\" 2130]"),
        "yalain: binding the ports: in use [ok stop-now]".to_string(),
    ] {
        assert_eq!(live::said(&nothing), None, "{nothing}");
    }
}

/// A character file's text as the server writes one (version 2): the character, its state in the
/// server's own encodings (which the editor never reads), and its build in a recipe's own words.
fn character_file_v2(name: &str, build: &str) -> String {
    format!(
        r#"{{"format":"yalain-character","version":2,"world":"Local World",
           "character":{{"id":1,"name":"{name}","race":4,"sex":8192,"class":0}},
           "state":{{"qualities":"00","play":"63"}}{build}}}"#
    )
}

#[test]
fn a_character_file_gives_a_recipe_of_its_build_field_for_field() {
    let dir = scratch("exported");
    let file = dir.join("kara.json");
    let build = r#","build":{"name":"Kara Vey","race":4,"sex":8192,"level":12,"money":750,
        "hero":true,"place":{"cell":2008547334,"x":4.4,"y":100.8,"z":182.5,"heading":90.0},
        "skills":[{"skill":7,"rating":10},{"skill":26,"rating":3}],
        "items":[{"template":1191183644,"quantity":3},{"template":1191182337,"quantity":1}],
        "quests":[{"quest":1073742282,"phase":"done"},{"quest":1073742283,"phase":2}]}"#;
    std::fs::write(&file, character_file_v2("Kara Vey", build)).unwrap();
    let r = characters::exported(&file).unwrap();
    assert_eq!(r.name, "Kara Vey");
    assert_eq!((r.race, r.sex), (chargen::TUMEROK, chargen::FEMALE));
    assert_eq!(
        (r.level, r.experience, r.money, r.hero),
        (Some(12), None, Some(750), true)
    );
    assert_eq!(r.start, Start::Left, "it starts where it stood");
    assert_eq!(
        r.skills,
        [
            Skill {
                skill: 7,
                rating: 10
            },
            Skill {
                skill: 26,
                rating: 3
            }
        ],
        "each skill with its rating"
    );
    assert_eq!(
        r.items,
        [
            Item {
                template: 0x4700_051C,
                quantity: 3
            },
            Item {
                template: 0x4700_0001,
                quantity: 1
            }
        ]
    );
    assert_eq!(
        r.quests,
        [
            Quest {
                quest: 0x4000_01CA,
                at: QuestAt::Done
            },
            Quest {
                quest: 0x4000_01CB,
                at: QuestAt::Phase(2)
            }
        ]
    );
    // It reads back as a recipe, and a recipe of it is what it holds: given to it, nothing.
    assert_eq!(Recipe::parse("Kara Vey", &r.text()).unwrap(), r);
    let t: toml::Table = r.changes_text(&r, None).parse().unwrap();
    for k in ["level", "money", "skills", "items", "quests", "hero"] {
        assert!(!t.contains_key(k), "{k}: {t:?}");
    }
    // An experience in place of a level, no place: at the camera.
    let build = r#","build":{"name":"Kara Vey","race":4,"sex":8192,"experience":250000}"#;
    std::fs::write(&file, character_file_v2("Kara Vey", build)).unwrap();
    let r = characters::exported(&file).unwrap();
    assert_eq!((r.level, r.experience), (None, Some(250_000)));
    assert_eq!((r.start, r.hero), (Start::Camera, false));
    // A build that does not read is said, not passed over.
    let build =
        r#","build":{"name":"Kara Vey","race":4,"sex":8192,"quests":[{"quest":1,"phase":0}]}"#;
    std::fs::write(&file, character_file_v2("Kara Vey", build)).unwrap();
    assert!(characters::exported(&file).is_err());
    // A file with no build, whose state does not read either, gives its body alone.
    std::fs::write(&file, character_file_v2("Kara Vey", "")).unwrap();
    let bare = characters::exported(&file).unwrap();
    assert_eq!((bare.race, bare.sex), (chargen::TUMEROK, chargen::FEMALE));
    assert_eq!((bare.level, bare.money), (None, None));
    assert!(bare.items.is_empty() && bare.skills.is_empty() && bare.quests.is_empty());
    std::fs::write(&file, r#"{"format":"something-else"}"#).unwrap();
    assert!(characters::exported(&file).is_err());
    assert!(characters::exported(&dir.join("none.json")).is_err());
    assert_eq!(
        characters::export_file(Path::new("w"), "Kara Vey"),
        Path::new("w")
            .join("build")
            .join("characters")
            .join("Kara Vey.json")
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// A character file's text as the server writes one: the character, its qualities and what it
/// holds in play, each in the server's own encoding as hexadecimal text.
fn character_file_v1(name: &str, qualities: &[u8], play: &[u8]) -> String {
    let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
    format!(
        r#"{{"format":"yalain-character","version":1,"world":"Local World",
           "character":{{"id":1,"name":"{name}","race":4,"sex":8192,"class":0}},
           "state":{{"qualities":"{}","play":"{}"}}}}"#,
        hex(qualities),
        hex(play)
    )
}

#[test]
fn a_first_version_character_file_gives_a_recipe_of_its_body_and_of_what_it_holds() {
    let dir = scratch("exported-v1");
    let file = dir.join("kara.json");
    let mut q = Qualities {
        header: Qualities::INTS,
        ..Qualities::default()
    };
    // Its level and its money.
    q.ints.entries = vec![(302, 12), (3100, 750), (1, 99)];
    let mut w = Writer::default();
    q.write(&mut w);
    let qualities = w.finish();
    let mut play = Vec::new();
    let w32 = |p: &mut Vec<u8>, v: u32| p.extend_from_slice(&v.to_le_bytes());
    let w64 = |p: &mut Vec<u8>, v: u64| p.extend_from_slice(&v.to_le_bytes());
    w32(&mut play, 10);
    w64(&mut play, 0);
    w32(&mut play, 3);
    // Two stacks of one template, and a worn item of another.
    w32(&mut play, 3);
    for (id, template, slot, worn, quantity) in [
        (1, 0x4700_051C, 0, 0, 1),
        (2, 0x4700_051C, 1, 0, 2),
        (3, 0x4700_0001, 0, 0x10, 1),
    ] {
        w64(&mut play, id);
        for v in [template, slot, worn, quantity] {
            w32(&mut play, v);
        }
    }
    // A quest done, one under way at its second phase, and one failed.
    w32(&mut play, 3);
    for (quest, status, phase) in [
        (0x4000_01CA, 0x3000_0000, 0),
        (0x4000_01CB, 0x1000_0000, 2),
        (0x4000_01CC, 0x2000_0000, 0),
    ] {
        for v in [quest, status, phase] {
            w32(&mut play, v);
        }
        w64(&mut play, 1);
        w64(&mut play, 2);
    }
    // A skill it trained, one it was given, and one it untrained.
    w32(&mut play, 3);
    for (skill, granted, mask) in [(7, 0, 1), (8, 1, 1), (9, 0, 0)] {
        for v in [skill, granted, mask, 4, 0, 0] {
            w32(&mut play, v);
        }
        w64(&mut play, 1000);
    }
    // What follows the skills is not read.
    play.extend_from_slice(&[0xFF; 9]);
    std::fs::write(&file, character_file_v1("Kara Vey", &qualities, &play)).unwrap();
    let r = characters::exported(&file).unwrap();
    assert_eq!(r.name, "Kara Vey");
    assert_eq!((r.race, r.sex), (chargen::TUMEROK, chargen::FEMALE));
    assert_eq!(
        (r.level, r.experience, r.money),
        (Some(12), None, Some(750))
    );
    assert_eq!(
        r.items,
        [
            Item {
                template: 0x4700_051C,
                quantity: 3
            },
            Item {
                template: 0x4700_0001,
                quantity: 1
            }
        ]
    );
    assert_eq!(
        r.quests,
        [
            Quest {
                quest: 0x4000_01CA,
                at: QuestAt::Done
            },
            Quest {
                quest: 0x4000_01CB,
                at: QuestAt::Phase(2)
            }
        ]
    );
    assert_eq!(
        r.skills,
        [Skill {
            skill: 7,
            rating: 0
        }],
        "trained, as far as it raised it"
    );
    // It reads back as a recipe, and a recipe of it is what it holds: given to it, nothing.
    assert_eq!(Recipe::parse("Kara Vey", &r.text()).unwrap(), r);
    let t: toml::Table = r.changes_text(&r, None).parse().unwrap();
    for k in ["level", "money", "skills", "items", "quests", "hero"] {
        assert!(!t.contains_key(k), "{k}: {t:?}");
    }
    // A file whose state does not read gives its body alone.
    std::fs::write(&file, character_file_v1("Kara Vey", b"\x01", b"\x63\x00")).unwrap();
    let bare = characters::exported(&file).unwrap();
    assert_eq!((bare.level, bare.money), (None, None));
    assert!(bare.items.is_empty() && bare.skills.is_empty() && bare.quests.is_empty());
    std::fs::write(&file, r#"{"format":"something-else"}"#).unwrap();
    assert!(characters::exported(&file).is_err());
    assert!(characters::exported(&dir.join("none.json")).is_err());
    assert_eq!(
        characters::export_file(Path::new("w"), "Kara Vey"),
        Path::new("w")
            .join("build")
            .join("characters")
            .join("Kara Vey.json")
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_skill_or_quest_is_picked_by_number_name_or_the_one_name_holding_what_is_typed() {
    let names = vec![
        (6, "Com_Me_AutoAttack".to_string()),
        (7, "Com_Me_MeleeAttack".to_string()),
        (12, "Com_Mi_MissileAttack".to_string()),
    ];
    assert_eq!(number("0x4700051C"), Some(0x4700_051C));
    assert_eq!(number(" 15 "), Some(15));
    assert_eq!(number("sword"), None);
    assert_eq!(pick(&names, "12"), Some(12));
    assert_eq!(pick(&names, "com_me_meleeattack"), Some(7));
    assert_eq!(pick(&names, "missile"), Some(12), "the one name holding it");
    assert_eq!(pick(&names, "attack"), None, "more than one holds it");
    assert_eq!(pick(&names, ""), None);
    let offered: Vec<u32> = matching(&names, "Me_", 5).iter().map(|(n, _)| *n).collect();
    assert_eq!(offered, [6, 7]);
}
