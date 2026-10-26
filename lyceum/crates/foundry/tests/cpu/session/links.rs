//! Behaviour: none (tooling: links between spawns in the world editor: what a lever, a pressure plate or a door sets off, written where the server reads it)

use std::cell::RefCell;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use dereth_primitives::Vec3;
use lyceum_foundry::edit::{Op, Session, Spawn, Weenie};
use lyceum_foundry::export::{self, Era, Importer, Options, Target};

use crate::common::{Fixture, HOME};

/// The server's own in `HOME`, as a dump writes them: two doors, the first set off by a lever;
/// a lever and a drudge on their own; a generator and the drudge it owns.
const SERVER: &str = "\
CREATE TABLE `weenie` (`class_Id` int, `class_Name` varchar(100), `type` int, `last_Modified` datetime);
INSERT INTO `weenie` VALUES (2179,'doorprisonactivated',19,'2005-02-09 10:00:00'),(2609,'leverbigswitch',26,'2005-02-09 10:00:00'),(7924,'testdrudge',10,'2005-02-09 10:00:00'),(1154,'gen',1,'2005-02-09 10:00:00');
CREATE TABLE `weenie_properties_string` (`id` int, `object_Id` int, `type` smallint, `value` text);
INSERT INTO `weenie_properties_string` VALUES (1,2179,1,'Door'),(2,2609,1,'Lever'),(3,7924,1,'Test Drudge'),(4,1154,1,'Generator');
CREATE TABLE `weenie_properties_d_i_d` (`id` int, `object_Id` int, `type` smallint, `value` int);
INSERT INTO `weenie_properties_d_i_d` VALUES (5,2179,1,33554433),(6,2609,1,33554433),(7,7924,1,33554433),(8,1154,1,33554433);
CREATE TABLE `weenie_properties_generator` (`id` int, `object_Id` int, `probability` float, `weenie_Class_Id` int);
INSERT INTO `weenie_properties_generator` VALUES (1,1154,-1,7924);
CREATE TABLE `landblock_instance` (`guid` int, `weenie_Class_Id` int, `obj_Cell_Id` int, `origin_X` float, `origin_Y` float, `origin_Z` float, `angles_W` float, `angles_X` float, `angles_Y` float, `angles_Z` float, `is_Link_Child` bit(1), `last_Modified` datetime);
INSERT INTO `landblock_instance` VALUES (0x70101000,2179,0x01010019,60,60,94,1,0,0,0,'\\0','2019-05-16 00:00:00'),(0x70101001,2609,0x01010019,62,60,94,1,0,0,0,'\u{1}','2019-05-16 00:00:00'),(0x70101002,2179,0x01010019,80,80,94,1,0,0,0,'\\0','2019-05-16 00:00:00'),(0x70101003,2609,0x01010019,64,60,94,1,0,0,0,'\\0','2019-05-16 00:00:00'),(0x70101004,7924,0x01010019,70,70,94,1,0,0,0,'\\0','2019-05-16 00:00:00'),(0x70101005,1154,0x01010019,90,10,94,1,0,0,0,'\\0','2019-05-16 00:00:00'),(0x70101006,7924,0x01010019,91,10,94,1,0,0,0,'\u{1}','2019-05-16 00:00:00');
CREATE TABLE `landblock_instance_link` (`id` int, `parent_GUID` int, `child_GUID` int, `last_Modified` datetime);
INSERT INTO `landblock_instance_link` VALUES (1,0x70101000,0x70101001,'2019-05-16 00:00:00'),(2,0x70101005,0x70101006,'2019-05-16 00:00:00');
";

const DOOR_A: &str = "0x70101000";
const LEVER_A: &str = "0x70101001";
const DOOR_B: &str = "0x70101002";
const LEVER: &str = "0x70101003";
const DRUDGE: &str = "0x70101004";
const GENERATOR: &str = "0x70101005";
const OWNED: &str = "0x70101006";
const DOOR: u32 = 2179;
const SWITCH: u32 = 2609;
const TODAY: &str = "2026-10-25 00:00:00";

/// A session of the fixture's world over the server's own in `HOME`, shown.
fn session(f: &Fixture) -> Session {
    let mut s = Session::open(&f.world(), &f.dats(), None).unwrap();
    s.set_today(TODAY);
    s.learn_weenies(SERVER);
    s.show_server_spawns([HOME]);
    s
}

fn find(s: &Session, guid: &str) -> Spawn {
    s.spawns()
        .into_iter()
        .find(|x| x.guid == guid)
        .unwrap_or_else(|| panic!("no spawn {guid}"))
}

fn guids(list: &[Spawn]) -> Vec<String> {
    list.iter().map(|x| x.guid.clone()).collect()
}

/// `child` made to set off `target`, or nothing.
fn link(
    s: &mut Session,
    child: &Spawn,
    target: Option<&Spawn>,
) -> Result<lyceum_foundry::edit::Applied, lyceum_foundry::error::Error> {
    s.apply(&Op::Link {
        file: child.file.clone(),
        guid: child.guid.clone(),
        target: target.map(|t| (t.file.clone(), t.guid.clone())),
    })
}

/// The spawn `child` (by its guid) made to set off the spawn `target` (by its guid), or nothing.
fn link_by(
    s: &mut Session,
    child: &str,
    target: Option<&str>,
) -> Result<lyceum_foundry::edit::Applied, lyceum_foundry::error::Error> {
    let c = find(s, child);
    let t = target.map(|t| find(s, t));
    link(s, &c, t.as_ref())
}

/// A spawn of server class `class` placed by the pack in `HOME` at `x`, `y`.
fn place(s: &mut Session, class: u32, x: f32, y: f32) -> Spawn {
    let file = s
        .apply(&Op::PlaceSpawn {
            landblock: HOME,
            weenie: Weenie::Class(class),
            origin: Vec3::new(x, y, 94.0),
            heading: 0.0,
        })
        .unwrap()
        .spawned
        .unwrap();
    s.spawns().into_iter().find(|x| x.file == file).unwrap()
}

#[test]
fn a_server_doors_lever_is_what_sets_it_off_not_a_spawn_it_owns() {
    let f = Fixture::new("links-server");
    let mut s = session(&f);
    let door = find(&s, DOOR_A);
    assert_eq!(guids(&s.set_off_by(&door)), [LEVER_A]);
    assert_eq!(
        s.sets_off(&find(&s, LEVER_A)).map(|x| x.guid),
        Some(DOOR_A.into())
    );
    assert!(s.sets_off(&find(&s, LEVER)).is_none());
    // The door owns nothing; the generator owns its drudge, which sets nothing off.
    assert!(s.generator(&door).is_none());
    let g = s.generator(&find(&s, GENERATOR)).unwrap();
    assert_eq!(g.children, [OWNED]);
    assert!(s.set_off_by(&find(&s, GENERATOR)).is_empty());
    assert!(s.sets_off(&find(&s, OWNED)).is_none());
    assert_eq!(
        s.link_owner(&find(&s, OWNED)).map(|x| x.guid),
        Some(GENERATOR.into())
    );
    assert!(
        s.link_owner(&find(&s, LEVER_A)).is_none(),
        "the door owns it not"
    );
    // Doors and levers set things off and are set off; a creature (through its emotes) sets
    // things off; a generator does neither.
    for (guid, sets, set) in [
        (DOOR_A, true, true),
        (LEVER, true, true),
        (DRUDGE, true, false),
        (GENERATOR, false, false),
    ] {
        let x = find(&s, guid);
        assert_eq!(
            (s.sets_things_off(&x), s.can_be_set_off(&x)),
            (sets, set),
            "{guid}"
        );
    }
    // What the free lever could set off: the landblock's doors and levers, nearest first.
    assert_eq!(
        guids(&s.link_targets(&find(&s, LEVER))),
        [LEVER_A, DOOR_A, DOOR_B]
    );
}

#[test]
fn a_lever_the_pack_places_set_to_open_a_server_door_writes_the_door_again_with_it() {
    let f = Fixture::new("links-pack-lever");
    let mut s = session(&f);
    let lever = place(&mut s, SWITCH, 58.0, 60.0);
    let steps = s.history().0;
    link_by(&mut s, &lever.guid, Some(DOOR_A)).unwrap();
    assert_eq!(s.history().0, steps + 1, "one step");
    let lever = s.spawn(&lever.file, &lever.guid).unwrap();
    let door = find(&s, DOOR_A);
    assert!(!door.server && door.file == s.override_file(HOME));
    assert_eq!(guids(&s.set_off_by(&door)), [LEVER_A, lever.guid.as_str()]);
    assert_eq!(s.sets_off(&lever).map(|x| x.guid), Some(DOOR_A.into()));
    // Built, the server reads the link by the lever's number.
    let number = s.spawn_number(&lever).unwrap();
    let row = format!("(0x70101000, 0x{number:08X}, '{TODAY}')");
    assert!(s.compiled().sql.values().any(|t| t.contains(&row)), "{row}");
    s.save().unwrap();
    let doors = std::fs::read_to_string(s.override_file(HOME)).unwrap();
    assert!(
        doors.contains(&format!(
            "VALUES (0x70101000, 0x70101001, '{TODAY}')\n     , (0x70101000, {}, '{TODAY}');",
            lever.guid
        )),
        "{doors}"
    );
    let levers = std::fs::read_to_string(&lever.file).unwrap();
    assert!(levers.contains(&format!(", True, '{TODAY}');")), "{levers}");
    // Undone, the door is the server's again and the lever stands alone.
    s.undo().unwrap();
    assert_eq!(guids(&s.set_off_by(&find(&s, DOOR_A))), [LEVER_A]);
    assert!(find(&s, DOOR_A).server);
    assert!(s.sets_off(&lever).is_none());
    assert!(s
        .sql_text(&lever.file)
        .unwrap()
        .contains(&format!(", False, '{TODAY}');")));
}

#[test]
fn a_levers_target_changed_or_taken_away_is_one_step_each() {
    let f = Fixture::new("links-change");
    let mut s = session(&f);
    let file = s.override_file(HOME);
    let steps = s.history().0;
    link_by(&mut s, LEVER_A, Some(DOOR_B)).unwrap();
    assert_eq!(s.history().0, steps + 1);
    assert_eq!(
        s.sets_off(&find(&s, LEVER_A)).map(|x| x.guid),
        Some(DOOR_B.into())
    );
    assert!(s.set_off_by(&find(&s, DOOR_A)).is_empty());
    assert_eq!(guids(&s.set_off_by(&find(&s, DOOR_B))), [LEVER_A]);
    let text = s.sql_text(&file).unwrap();
    assert!(
        text.contains(&format!("VALUES (0x70101002, 0x70101001, '{TODAY}');")),
        "{text}"
    );
    assert!(!text.contains("(0x70101000, 0x70101001"), "{text}");
    assert!(
        !text.contains("`guid` = 0x70101001;"),
        "the lever is a link's child already: {text}"
    );
    // Taken away: the lever stands alone, written so.
    link_by(&mut s, LEVER_A, None).unwrap();
    assert_eq!(s.history().0, steps + 2);
    assert!(s.sets_off(&find(&s, LEVER_A)).is_none());
    assert!(s.set_off_by(&find(&s, DOOR_B)).is_empty());
    let text = s.sql_text(&file).unwrap();
    assert!(
        text.contains(&format!(
            "VALUES (0x70101001, 2609, 0x01010019, 62.0, 60.0, 94.0, 1.0, 0.0, 0.0, 0.0, False, '{TODAY}') /* Lever */;"
        )),
        "{text}"
    );
    assert!(
        !text.contains("INSERT INTO `landblock_instance_link`"),
        "{text}"
    );
    // Each undone on its own.
    s.undo().unwrap();
    assert_eq!(
        s.sets_off(&find(&s, LEVER_A)).map(|x| x.guid),
        Some(DOOR_B.into())
    );
    s.undo().unwrap();
    assert_eq!(
        s.sets_off(&find(&s, LEVER_A)).map(|x| x.guid),
        Some(DOOR_A.into())
    );
    assert!(find(&s, DOOR_A).server && find(&s, LEVER_A).server);
}

#[test]
fn a_creature_is_linked_to_the_door_its_emotes_open_as_a_lever_is() {
    let f = Fixture::new("links-creature");
    let mut s = session(&f);
    link_by(&mut s, DRUDGE, Some(DOOR_B)).unwrap();
    assert_eq!(
        s.sets_off(&find(&s, DRUDGE)).map(|x| x.guid),
        Some(DOOR_B.into())
    );
    assert_eq!(guids(&s.set_off_by(&find(&s, DOOR_B))), [DRUDGE]);
    let text = s.sql_text(&s.override_file(HOME)).unwrap();
    assert!(
        text.contains(&format!(
            "VALUES (0x70101004, 7924, 0x01010019, 70.0, 70.0, 94.0, 1.0, 0.0, 0.0, 0.0, True, '{TODAY}') /* Test Drudge */;"
        )),
        "{text}"
    );
    // The one a generator makes is its generator's, and is not linked.
    let e = link_by(&mut s, OWNED, Some(DOOR_A))
        .unwrap_err()
        .to_string();
    assert!(e.contains("is made by 0x70101005"), "{e}");
}

#[test]
fn a_spawn_of_a_file_that_places_several_is_linked_in_that_file() {
    let f = Fixture::new("links-several");
    let mut s = session(&f);
    // A building's doors, both in one file, as a dropped house's basement writes them.
    let file = f.path().join("sample/server/sql/landblocks/0101 pair.sql");
    let row = |name: &str, x: u32| {
        format!(
            "({{{{guid:{name}@0101}}}}, {DOOR}, 0x01010019, {x}.0, 30.0, 94.0, 1.0, 0.0, 0.0, 0.0, False, '{TODAY}')"
        )
    };
    s.apply(&Op::WriteSql {
        file: file.clone(),
        text: Some(format!(
            "DELETE FROM `landblock_instance` WHERE `guid` = {{{{guid:left@0101}}}};
             DELETE FROM `landblock_instance` WHERE `guid` = {{{{guid:right@0101}}}};

             INSERT INTO `landblock_instance` (`guid`, `weenie_Class_Id`, `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`, `is_Link_Child`, `last_Modified`)
             VALUES {}
     , {};
",
            row("left", 30),
            row("right", 34)
        )),
    })
    .unwrap();
    // The free lever opens the right one; the right one opens the left.
    link_by(&mut s, LEVER, Some("{{guid:right@0101}}")).unwrap();
    link_by(&mut s, "{{guid:right@0101}}", Some("{{guid:left@0101}}")).unwrap();
    let text = s.sql_text(&file).unwrap();
    assert!(
        text.contains(&format!(
            "VALUES ({{{{guid:right@0101}}}}, 0x70101003, '{TODAY}');"
        )) && text.contains(&format!(
            "VALUES ({{{{guid:left@0101}}}}, {{{{guid:right@0101}}}}, '{TODAY}');"
        )),
        "{text}"
    );
    assert!(text.contains(&format!(
        "34.0, 30.0, 94.0, 1.0, 0.0, 0.0, 0.0, True, '{TODAY}')"
    )));
    assert!(text.contains(&format!(
        "30.0, 30.0, 94.0, 1.0, 0.0, 0.0, 0.0, False, '{TODAY}')"
    )));
    assert_eq!(
        guids(&s.set_off_by(&find(&s, "{{guid:left@0101}}"))),
        ["{{guid:right@0101}}"]
    );
    // Undone, the right one stands alone again in its file.
    s.undo().unwrap();
    assert!(s.sets_off(&find(&s, "{{guid:right@0101}}")).is_none());
    assert!(s.sql_text(&file).unwrap().contains(&format!(
        "34.0, 30.0, 94.0, 1.0, 0.0, 0.0, 0.0, False, '{TODAY}')"
    )));
}

#[test]
fn a_link_that_cannot_stand_is_refused_and_nothing_changes() {
    let f = Fixture::new("links-refused");
    let mut s = session(&f);
    let elsewhere = {
        let file = s
            .apply(&Op::PlaceSpawn {
                landblock: 0x0102,
                weenie: Weenie::Class(DOOR),
                origin: Vec3::new(20.0, 20.0, 94.0),
                heading: 0.0,
            })
            .unwrap()
            .spawned
            .unwrap();
        s.spawns().into_iter().find(|x| x.file == file).unwrap()
    };
    let steps = s.history().0;
    let lever = find(&s, LEVER);
    let said = |r: Result<lyceum_foundry::edit::Applied, lyceum_foundry::error::Error>| {
        r.unwrap_err().to_string()
    };
    let e = said(link(&mut s, &lever, Some(&elsewhere)));
    assert!(e.contains("within its landblock"), "{e}");
    let e = said(link_by(&mut s, &lever.guid, Some(DRUDGE)));
    assert!(e.contains("nothing sets it off"), "{e}");
    let e = said(link(&mut s, &lever, Some(&lever.clone())));
    assert!(e.contains("itself"), "{e}");
    let e = said(link_by(&mut s, OWNED, Some(DOOR_A)));
    assert!(e.contains("is made by 0x70101005"), "{e}");
    let e = said(link(&mut s, &lever, None));
    assert!(e.contains("sets nothing off"), "{e}");
    let e = said(link_by(&mut s, LEVER_A, Some(DOOR_A)));
    assert!(e.contains("already"), "{e}");
    assert_eq!(s.history().0, steps, "nothing changed");
    // A door may open the next; the next may not open the first.
    link_by(&mut s, DOOR_A, Some(DOOR_B)).unwrap();
    let e = said(link_by(&mut s, DOOR_B, Some(DOOR_A)));
    assert!(e.contains("would set itself off"), "{e}");
    assert!(!guids(&s.link_targets(&find(&s, DOOR_B))).contains(&DOOR_A.to_owned()));
    // The chain: the lever opens the first door, which opens the second.
    assert_eq!(guids(&s.set_off_by(&find(&s, DOOR_A))), [LEVER_A]);
    assert_eq!(guids(&s.set_off_by(&find(&s, DOOR_B))), [DOOR_A]);
}

#[test]
fn two_of_the_packs_spawns_link_in_the_doors_file_and_either_taken_away_lets_go_of_the_other() {
    let f = Fixture::new("links-pack");
    let mut s = session(&f);
    let door = place(&mut s, DOOR, 30.0, 30.0);
    let lever = place(&mut s, SWITCH, 32.0, 30.0);
    link(&mut s, &lever, Some(&door)).unwrap();
    let text = s.sql_text(&door.file).unwrap();
    assert!(
        text.ends_with(&format!(
            ";\n\nINSERT INTO `landblock_instance_link` (`parent_GUID`, `child_GUID`, `last_Modified`)\n\
             VALUES ({}, {}, '{TODAY}');\n",
            door.guid, lever.guid
        )),
        "{text}"
    );
    assert!(s
        .sql_text(&lever.file)
        .unwrap()
        .contains(&format!(", True, '{TODAY}');")));
    assert_eq!(guids(&s.set_off_by(&door)), [lever.guid.as_str()]);
    let (d, l) = (
        s.spawn_number(&door).unwrap(),
        s.spawn_number(&lever).unwrap(),
    );
    let row = format!("(0x{d:08X}, 0x{l:08X}, '{TODAY}')");
    assert!(s.compiled().sql.values().any(|t| t.contains(&row)), "{row}");
    // The door taken away: the lever stands alone.
    s.apply(&Op::RemoveSpawn {
        file: door.file.clone(),
    })
    .unwrap();
    assert!(s
        .sql_text(&lever.file)
        .unwrap()
        .contains(&format!(", False, '{TODAY}');")));
    s.undo().unwrap();
    // The lever taken away: the door links nothing.
    s.apply(&Op::RemoveSpawn {
        file: lever.file.clone(),
    })
    .unwrap();
    let text = s.sql_text(&door.file).unwrap();
    assert!(!text.contains("landblock_instance_link"), "{text}");
    s.undo().unwrap();
    // The server's door taken away: its lever is written standing alone.
    s.apply(&Op::RemoveServerSpawn {
        file: s.override_file(HOME),
        guid: DOOR_A.into(),
    })
    .unwrap();
    let text = s.sql_text(&s.override_file(HOME)).unwrap();
    assert!(
        text.contains(
            "VALUES (0x70101001, 2609, 0x01010019, 62.0, 60.0, 94.0, 1.0, 0.0, 0.0, 0.0, False,"
        ),
        "{text}"
    );
    assert!(s.sets_off(&find(&s, LEVER_A)).is_none());
}

/// An import tool that keeps the text of every SQL file it is handed with `--patches`, and
/// writes what it is asked to.
#[derive(Default)]
struct Tool {
    handed: RefCell<Vec<(String, String)>>,
}

fn sql_under(dir: &Path, out: &mut Vec<String>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            sql_under(&p, out);
        } else {
            out.push(std::fs::read_to_string(&p).unwrap());
        }
    }
}

impl Importer for Tool {
    fn run(&self, args: &[OsString]) -> Result<String, String> {
        let args: Vec<String> = args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        if args[0] == "--version" {
            return Ok("empyrean-import 9.9.9".into());
        }
        let at = |flag: &str| {
            args.iter()
                .position(|a| a == flag)
                .map(|i| PathBuf::from(&args[i + 1]))
        };
        if let Some(dir) = at("--patches") {
            let mut texts = Vec::new();
            sql_under(&dir, &mut texts);
            for t in texts {
                self.handed.borrow_mut().push((args[0].clone(), t));
            }
        }
        let out = at("--out").unwrap();
        if args[0] == "gdle" {
            std::fs::create_dir_all(&out).unwrap();
            std::fs::write(out.join("quests.json"), "[]").unwrap();
        } else {
            std::fs::write(&out, b"pack").unwrap();
        }
        Ok("wrote".into())
    }
}

#[test]
fn a_link_reaches_each_server_the_world_is_exported_for_by_its_numbers() {
    let f = Fixture::new("links-export");
    let mut s = session(&f);
    let door = place(&mut s, DOOR, 30.0, 30.0);
    let lever = place(&mut s, SWITCH, 32.0, 30.0);
    link(&mut s, &lever, Some(&door)).unwrap();
    link_by(&mut s, LEVER, Some(DOOR_B)).unwrap();
    s.save().unwrap();
    let (d, l) = (
        s.spawn_number(&door).unwrap(),
        s.spawn_number(&lever).unwrap(),
    );
    let ours = format!("(0x{d:08X}, 0x{l:08X}, '{TODAY}')");
    let servers = format!("(0x70101002, 0x70101003, '{TODAY}')");
    let marked = format!("0x{l:08X}, {SWITCH}, ");
    let dump = f.path().join("ACE-World-Database-v0.9.295.sql");
    std::fs::write(&dump, "-- the world database").unwrap();
    let tool = Tool::default();
    for target in [Target::Empyrean, Target::Ace, Target::Gdle] {
        let out = f.path().join(format!("for-{}", target.word()));
        export::export(
            &f.path().join("world.toml"),
            &f.dats(),
            &Options {
                target,
                out: &out,
                era: Era::Eor,
                world_sql: Some(&dump),
                importer: Some(&tool),
                gdle_quests: None,
            },
        )
        .unwrap();
        // ACE's content folder holds the files; the import tool is handed them for the others.
        let texts: Vec<String> = match target {
            Target::Ace => {
                let mut t = Vec::new();
                sql_under(&out.join("sql").join("landblocks"), &mut t);
                t
            }
            _ => tool
                .handed
                .borrow()
                .iter()
                .filter(|(run, _)| (run == "gdle") == (target == Target::Gdle))
                .map(|(_, t)| t.clone())
                .collect(),
        };
        for want in [&ours, &servers, &marked] {
            assert!(
                texts.iter().any(|t| t.contains(want.as_str())),
                "{}: {want}",
                target.name()
            );
        }
        assert!(
            texts
                .iter()
                .any(|t| t.contains(&marked) && t.contains(", True, '")),
            "{}: the lever marked a link's child",
            target.name()
        );
        assert!(!texts.iter().any(|t| t.contains("{{")), "{}", target.name());
    }
}
