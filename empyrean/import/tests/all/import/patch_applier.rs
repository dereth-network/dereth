//! Vectors: tests/fixtures/patches, synthetic SQL and JSON content inputs
//! Per-object SQL/JSON content files over a base dump: rows as MySQL would hold them, JSON and
//! SQL give identical records, cascading deletes, --check reports changed records, deterministic
//! bytes, sorted directory order, AUTO_INCREMENT/LAST_INSERT_ID, keys and strict mode as MySQL.
//! Fixture: tests/fixtures/patches/base.sql and locally constructed edge cases.

use std::path::PathBuf;

use empyrean_common::dotnet::DotNetDateTime;
use empyrean_content::import::patch::{expand, Input, InputKind, RecordCounts, Source};
use empyrean_content::import::{build_from, check, default_now, import, Imported};
use empyrean_content::models::world::*;
use empyrean_content::pack::{Pack, TableId};
use empyrean_content::ImportError;

const BASE: &str = include_str!("../../../../crates/content/tests/fixtures/patches/base.sql");

fn dir() -> PathBuf {
    crate::patch_fixtures()
}

fn file(kind: InputKind, rel: &str) -> Source {
    Source::read(kind, &dir().join(rel)).unwrap()
}

fn sql(name: &str, text: &str) -> Source {
    Source {
        kind: InputKind::Sql,
        path: PathBuf::from(name),
        bytes: text.as_bytes().to_vec(),
    }
}

fn try_build_at(
    sources: Vec<Source>,
    now: DotNetDateTime,
) -> Result<(Vec<u8>, Imported), ImportError> {
    build_from(BASE.as_bytes(), sources.into_iter().map(Ok), now)
}

fn build(sources: Vec<Source>) -> (Pack, Imported) {
    let (bytes, imp) = try_build_at(sources, default_now()).unwrap();
    (Pack::from_bytes(bytes).unwrap(), imp)
}

fn fails(sources: Vec<Source>) -> String {
    match try_build_at(sources, default_now()) {
        Ok(_) => panic!("expected the import to fail"),
        Err(e) => e.to_string(),
    }
}

fn plain() -> Pack {
    Pack::from_bytes(import(BASE.as_bytes()).unwrap().0).unwrap()
}

fn weenie(p: &Pack, wcid: u64) -> Option<Weenie> {
    p.get::<Weenie>(TableId::WEENIE, wcid).unwrap()
}

fn ints(w: &Weenie) -> Vec<(u32, u16, i32)> {
    w.weenie_properties_int
        .iter()
        .map(|r| (r.id, r.r#type, r.value))
        .collect()
}

fn dt(y: i32, mo: i32, d: i32, h: i32, mi: i32, s: i32) -> DotNetDateTime {
    DotNetDateTime::new_hms(y, mo, d, h, mi, s)
}

#[test]
fn the_store_path_without_files_holds_what_the_plain_import_holds() {
    let (p, imp) = build(Vec::new());
    assert!(check::diff(&plain(), &p).unwrap().is_empty());
    assert!(
        imp.orphans.is_empty() && imp.unknown_tables.is_empty() && imp.missing_tables.is_empty()
    );
}

#[test]
fn replacing_a_weenie_yields_the_rows_mysql_would_hold() {
    let (p, imp) = build(vec![file(
        InputKind::Sql,
        "sql/1 weenies/00100 Test Drudge.sql",
    )]);
    let w = weenie(&p, 100).unwrap();
    assert_eq!(w.class_name, "ace100-testdrudge");
    assert_eq!(w.last_modified, dt(2000, 1, 1, 0, 0, 0));
    // New rows take ids from the AUTO_INCREMENT counter (606854 in the dump), in insert order.
    assert_eq!(ints(&w), [(606_854, 1, 16), (606_855, 25, 7)]);
    assert_eq!(
        w.weenie_properties_float
            .iter()
            .map(|f| (f.id, f.value))
            .collect::<Vec<_>>(),
        [(396_085, 1.25)]
    );
    let e = &w.weenie_properties_emote;
    assert_eq!(e.len(), 1);
    assert_eq!(e[0].id, 90_717);
    // `SET @parent_id = LAST_INSERT_ID()` gave the actions their emote's id.
    let a: Vec<(u32, u32, u32, Option<&str>)> = e[0]
        .weenie_properties_emote_action
        .iter()
        .map(|a| (a.id, a.emote_id, a.order, a.message.as_deref()))
        .collect();
    assert_eq!(
        a,
        [
            (196_625, 90_717, 0, Some("Hello again")),
            (196_626, 90_717, 1, Some("Bye now"))
        ]
    );
    // The other weenie is untouched, and the report names the one record.
    assert_eq!(weenie(&p, 200), weenie(&plain(), 200));
    assert_eq!(imp.applied.len(), 1);
    assert_eq!(
        imp.applied[0].records["weenie"],
        RecordCounts {
            added: 0,
            replaced: 1,
            deleted: 0
        }
    );
    let rows = &imp.applied[0].rows;
    assert_eq!((rows["weenie"].deleted, rows["weenie"].inserted), (1, 1));
    assert_eq!(
        rows["weenie_properties_int"].deleted, 2,
        "the cascade removed the old rows"
    );
    assert_eq!(rows["weenie_properties_emote_action"].deleted, 2);
    assert_eq!(rows["weenie_properties_emote_action"].inserted, 2);
}

#[test]
fn the_same_change_as_json_gives_the_identical_record() {
    let (by_sql, _) = build(vec![file(
        InputKind::Sql,
        "sql/1 weenies/00100 Test Drudge.sql",
    )]);
    let (by_json, imp) = build(vec![file(
        InputKind::Json,
        "json/weenies/00100 Test Drudge.json",
    )]);
    assert_eq!(
        by_sql.raw(TableId::WEENIE, 100).unwrap(),
        by_json.raw(TableId::WEENIE, 100).unwrap()
    );
    assert!(check::diff(&by_sql, &by_json).unwrap().is_empty());
    assert_eq!(imp.applied[0].kind, InputKind::Json);
}

#[test]
fn deleting_a_landblock_cascades_to_its_links() {
    assert_eq!(
        plain()
            .get::<Vec<LandblockInstance>>(TableId::LANDBLOCK_INSTANCE, 0xA9B4)
            .unwrap()
            .unwrap()
            .len(),
        2
    );
    let (p, imp) = build(vec![file(InputKind::Sql, "sql/2 landblocks/A9B4.sql")]);
    assert_eq!(
        p.get::<Vec<LandblockInstance>>(TableId::LANDBLOCK_INSTANCE, 0xA9B4)
            .unwrap(),
        None
    );
    assert_eq!(
        p.get::<Vec<LandblockInstance>>(TableId::LANDBLOCK_INSTANCE, 0x0007)
            .unwrap()
            .unwrap()
            .len(),
        1
    );
    assert!(
        p.get::<Vec<Encounter>>(TableId::ENCOUNTER, 0xA9B4)
            .unwrap()
            .is_some(),
        "other tables keep the landblock"
    );
    let r = &imp.applied[0];
    assert_eq!(
        r.records["landblock_inst"],
        RecordCounts {
            added: 0,
            replaced: 0,
            deleted: 1
        }
    );
    assert_eq!(r.rows["landblock_instance"].deleted, 2);
    assert_eq!(r.rows["landblock_instance_link"].deleted, 1);
}

#[test]
fn adding_a_weenie_and_updating_rows() {
    let (p, imp) = build(vec![
        file(InputKind::Sql, "sql/1 weenies/90001 New Thing.sql"),
        file(InputKind::Sql, "sql/3 update.sql"),
    ]);
    let w = weenie(&p, 90_001).unwrap();
    assert_eq!(w.class_name, "newthing");
    assert_eq!(
        w.weenie_properties_bool
            .iter()
            .map(|b| (b.r#type, b.value))
            .collect::<Vec<_>>(),
        [(22, true)]
    );
    let long: Vec<&str> = w
        .weenie_properties_string
        .iter()
        .map(|s| s.value.as_str())
        .collect();
    assert_eq!(long, ["New Thing", "It's new.\nTwo lines."]);
    assert_eq!(
        imp.applied[0].records["weenie"],
        RecordCounts {
            added: 1,
            replaced: 0,
            deleted: 0
        }
    );
    // UPDATE: the row keeps its id; the quest is found case-insensitively and its
    // `ON UPDATE CURRENT_TIMESTAMP` column takes the build clock; a no-op UPDATE changes nothing.
    assert_eq!(ints(&weenie(&p, 200).unwrap()), [(3, 1, 1), (4, 5, 250)]);
    let q = p.get::<Quest>(TableId::QUEST, 1).unwrap().unwrap();
    assert_eq!(
        (q.max_solves, q.last_modified),
        (5, dt(2000, 1, 1, 0, 0, 0))
    );
    let poi = p
        .get::<PointsOfInterest>(TableId::POINTS_OF_INTEREST, 1)
        .unwrap()
        .unwrap();
    assert_eq!(poi.last_modified, dt(2021, 11, 1, 0, 0, 0));
    let u = &imp.applied[1];
    assert_eq!(u.rows["weenie_properties_int"].updated, 1);
    assert!(!u.rows.contains_key("points_of_interest"));
    assert_eq!(
        u.records["weenie"],
        RecordCounts {
            added: 0,
            replaced: 1,
            deleted: 0
        }
    );
    assert_eq!(
        u.records["quest"],
        RecordCounts {
            added: 0,
            replaced: 1,
            deleted: 0
        }
    );
}

#[test]
fn check_reports_exactly_the_changed_records() {
    let (p, _) = build(vec![
        file(InputKind::Sql, "sql/1 weenies/00100 Test Drudge.sql"),
        file(InputKind::Sql, "sql/1 weenies/90001 New Thing.sql"),
        file(InputKind::Sql, "sql/2 landblocks/A9B4.sql"),
        file(InputKind::Sql, "sql/3 update.sql"),
    ]);
    let base = plain();
    let d = check::diff(&base, &p).unwrap();
    type Row<'a> = (&'a str, Vec<u64>, Vec<u64>, Vec<u64>);
    let got: Vec<Row> = d
        .iter()
        .map(|t| {
            (
                t.table,
                t.added.clone(),
                t.changed.clone(),
                t.removed.clone(),
            )
        })
        .collect();
    assert_eq!(
        got,
        [
            ("weenie", vec![90_001], vec![100, 200], vec![]),
            // The index record follows its weenie: 100's class name changed, 200's did not.
            ("weenie_index", vec![90_001], vec![100], vec![]),
            ("landblock_inst", vec![], vec![], vec![0xA9B4]),
            ("quest", vec![], vec![1], vec![]),
        ]
    );
    let text = check::render(&d, &base, &p);
    assert!(
        text.contains("  + weenie 90001 newthing (New Thing)\n"),
        "{text}"
    );
    assert!(text.contains("  - landblock_inst 0xA9B4\n"), "{text}");
    assert!(check::diff(&p, &p).unwrap().is_empty());
}

#[test]
fn the_pack_is_the_same_bytes_every_run() {
    let files = || {
        vec![
            file(InputKind::Sql, "sql/1 weenies/00100 Test Drudge.sql"),
            file(InputKind::Json, "json/weenies/00100 Test Drudge.json"),
            file(InputKind::Sql, "sql/3 update.sql"),
        ]
    };
    let a = try_build_at(files(), default_now()).unwrap().0;
    let b = try_build_at(files(), default_now()).unwrap().0;
    assert_eq!(a, b);
    // The clock is an input: another `--now` stamps other times, so another pack.
    let c = try_build_at(files(), dt(2001, 1, 1, 0, 0, 0)).unwrap().0;
    assert_ne!(a, c);
}

#[test]
fn a_directory_is_applied_in_sorted_path_order() {
    let files = expand(&Input {
        kind: InputKind::Sql,
        path: dir().join("sql"),
    })
    .unwrap();
    let rel: Vec<String> = files
        .iter()
        .map(|p| {
            p.strip_prefix(dir().join("sql"))
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();
    assert_eq!(
        rel,
        [
            "1 weenies/00100 Test Drudge.sql",
            "1 weenies/90001 New Thing.sql",
            "2 landblocks/A9B4.sql",
            "3 update.sql"
        ]
    );
    let json = expand(&Input {
        kind: InputKind::Json,
        path: dir(),
    })
    .unwrap();
    assert_eq!(json.len(), 1);
    // Later files override earlier ones: the same weenie twice keeps the second.
    let (p, _) = build(vec![
        sql("a.sql", "DELETE FROM `weenie` WHERE `class_Id` = 200; INSERT INTO `weenie` VALUES (200, 'first', 1, '2020-01-01 00:00:00');"),
        sql("b.sql", "DELETE FROM `weenie` WHERE `class_Id` = 200; INSERT INTO `weenie` VALUES (200, 'second', 1, '2020-01-01 00:00:00');"),
    ]);
    assert_eq!(weenie(&p, 200).unwrap().class_name, "second");
}

#[test]
fn auto_increment_never_goes_back_and_last_insert_id_is_the_first_of_a_statement() {
    let (p, _) = build(vec![sql(
        "a.sql",
        "INSERT INTO `quest` (`name`, `min_Delta`, `max_Solves`) VALUES ('a', 1, 1), ('b', 1, 1);\n\
         SET @first = LAST_INSERT_ID();\n\
         DELETE FROM `quest` WHERE `name` = 'b';\n\
         INSERT INTO `quest` (`name`, `min_Delta`, `max_Solves`, `message`) VALUES ('c', 1, 1, @first);\n\
         INSERT INTO `quest` (`id`, `name`, `min_Delta`, `max_Solves`) VALUES (9000, 'd', 1, 1);\n\
         INSERT INTO `quest` (`id`, `name`, `min_Delta`, `max_Solves`) VALUES (NULL, 'e', 1, 1);\n\
         INSERT INTO `quest` (`id`, `name`, `min_Delta`, `max_Solves`) VALUES (0, 'f', 1, 1);",
    )]);
    let q: Vec<(u64, String, Option<String>)> = p
        .all::<Quest>(TableId::QUEST)
        .unwrap()
        .into_iter()
        .map(|(k, q)| (k, q.name, q.message))
        .collect();
    let names: Vec<(u64, &str)> = q.iter().map(|(k, n, _)| (*k, n.as_str())).collect();
    // The dump's AUTO_INCREMENT=5301; 'b' (5302) is not reused; an explicit 9000 moves the
    // counter; NULL and 0 both generate.
    assert_eq!(
        names,
        [
            (1, "TestQuest"),
            (5301, "a"),
            (5303, "c"),
            (9000, "d"),
            (9001, "e"),
            (9002, "f")
        ]
    );
    assert_eq!(
        q[2].2.as_deref(),
        Some("5301"),
        "LAST_INSERT_ID() is the multi-row insert's first id"
    );
    // With NO_AUTO_VALUE_ON_ZERO a 0 is stored as 0.
    let (p, _) = build(vec![sql(
        "z.sql",
        "SET SQL_MODE = \"NO_AUTO_VALUE_ON_ZERO\";\n\
         INSERT INTO `points_of_interest` (`id`, `name`, `weenie_Class_Id`) VALUES (0, 'Zero', 1);",
    )]);
    assert_eq!(
        p.get::<PointsOfInterest>(TableId::POINTS_OF_INTEREST, 0)
            .unwrap()
            .unwrap()
            .name,
        "Zero"
    );
}

#[test]
fn keys_are_enforced_as_mysql_enforces_them() {
    // Duplicate primary key, duplicate UNIQUE (object_Id, type), a child without its parent.
    assert!(fails(vec![sql(
        "a.sql",
        "INSERT INTO `weenie` VALUES (200, 'x', 1, '2020-01-01 00:00:00');"
    )])
    .contains("duplicate entry"));
    assert!(fails(vec![sql(
        "a.sql",
        "INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`) VALUES (200, 5, 1);"
    )])
    .contains("duplicate entry"));
    let e = fails(vec![sql("dir/b.sql", "\n\nINSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`) VALUES (777, 1, 1);")]);
    assert!(
        e.contains("line 3") && e.contains("dir/b.sql") && e.contains("no `weenie` row"),
        "{e}"
    );
    // …unless the file turns the check off, as mysqldump's header does.
    let (p, _) = build(vec![sql(
        "c.sql",
        "/*!40014 SET @OLD_FOREIGN_KEY_CHECKS=@@FOREIGN_KEY_CHECKS, FOREIGN_KEY_CHECKS=0 */;\n\
         INSERT INTO `weenie_properties_emote` (`object_Id`, `category`) VALUES (100, 3);\n\
         /*!40014 SET FOREIGN_KEY_CHECKS=IF(@OLD_FOREIGN_KEY_CHECKS IS NULL, 1, @OLD_FOREIGN_KEY_CHECKS) */;",
    )]);
    let w = weenie(&p, 100).unwrap();
    assert_eq!(
        w.weenie_properties_emote
            .last()
            .map(|e| (e.category, e.probability)),
        Some((3, 1.0)),
        "column default 1"
    );
    // INSERT IGNORE skips the duplicate; REPLACE deletes the row a unique key collides with and
    // inserts with a new id.
    let (p, imp) = build(vec![sql(
        "d.sql",
        "INSERT IGNORE INTO `weenie` VALUES (200, 'x', 1, '2020-01-01 00:00:00');\n\
         REPLACE INTO `points_of_interest` (`name`, `weenie_Class_Id`) VALUES ('HOLTBURG ', 42);",
    )]);
    assert_eq!(weenie(&p, 200).unwrap().class_name, "testsword");
    let pois = p
        .all::<PointsOfInterest>(TableId::POINTS_OF_INTEREST)
        .unwrap();
    assert_eq!(
        pois.iter()
            .map(|(k, x)| (*k, x.weenie_class_id))
            .collect::<Vec<_>>(),
        [(106, 42)]
    );
    assert_eq!(
        imp.applied[0].records["poi"],
        RecordCounts {
            added: 1,
            replaced: 0,
            deleted: 1
        }
    );
}

#[test]
fn strict_mode_and_its_restoration() {
    let long = "x".repeat(256);
    assert!(fails(vec![sql(
        "a.sql",
        &format!(
            "INSERT INTO `quest` (`name`, `min_Delta`, `max_Solves`) VALUES ('{long}', 1, 1);"
        )
    )])
    .contains("data too long"));
    // Without STRICT_TRANS_TABLES the value is cut to the column's 255 characters…
    let (p, _) = build(vec![sql(
        "b.sql",
        &format!("SET SQL_MODE = '';\nINSERT INTO `quest` (`name`, `min_Delta`, `max_Solves`) VALUES ('{long}', 1, 1);"),
    )]);
    assert_eq!(
        p.get::<Quest>(TableId::QUEST, 5301)
            .unwrap()
            .unwrap()
            .name
            .len(),
        255
    );
    // …and a file that saves and restores @@SQL_MODE (HeidiSQL's header) is strict again after.
    assert!(fails(vec![sql(
        "c.sql",
        &format!(
            "/*!40101 SET @OLD_SQL_MODE=@@SQL_MODE, SQL_MODE='NO_AUTO_VALUE_ON_ZERO' */;\n\
             /*!40101 SET SQL_MODE=IFNULL(@OLD_SQL_MODE, '') */;\n\
             INSERT INTO `quest` (`name`, `min_Delta`, `max_Solves`) VALUES ('{long}', 1, 1);"
        )
    )])
    .contains("data too long"));
    // A NOT NULL column without a default must be given.
    assert!(fails(vec![sql(
        "d.sql",
        "INSERT INTO `quest` (`name`, `max_Solves`) VALUES ('q', 1);"
    )])
    .contains("default value"));
}

#[test]
fn rows_come_back_in_primary_key_order() {
    // Explicit ids below the ones already stored land before them, as a clustered index keeps them.
    let (p, _) = build(vec![sql(
        "a.sql",
        "DELETE FROM `weenie_properties_int` WHERE `id` = 1;
         INSERT INTO `weenie_properties_int` (`id`, `object_Id`, `type`, `value`) VALUES (2000000, 200, 7, 1), (1, 200, 9, 1);",
    )]);
    assert_eq!(
        ints(&weenie(&p, 200).unwrap()),
        [(1, 9, 1), (3, 1, 1), (4, 5, 100), (2_000_000, 7, 1)]
    );
}

#[test]
fn file_text_is_read_as_ace_reads_it() {
    // CRLF becomes LF inside strings too, and a Lifestoned changelog comment ends the file.
    let (p, _) = build(vec![sql(
        "a.sql",
        "UPDATE `weenie_properties_string` SET `value` = 'one\r\ntwo' WHERE `object_Id` = 100;\r\n\
         /* Lifestoned Changelog:\r\n{ \"changelog\": [] }\r\n*/\r\nDELETE FROM `weenie`;",
    )]);
    let w = weenie(&p, 100).unwrap();
    assert_eq!(w.weenie_properties_string[0].value, "one\ntwo");
    assert!(
        weenie(&p, 200).is_some(),
        "nothing after the changelog marker ran"
    );
    // Statements the writer never emits are refused, not skipped.
    assert!(fails(vec![sql("b.sql", "SELECT 1;")]).contains("unsupported statement"));
}

#[test]
fn a_table_can_be_dropped_and_recreated() {
    let (p, imp) = build(vec![sql(
        "loot.sql",
        "DROP TABLE IF EXISTS `treasure_gem_count`;\n\
         CREATE TABLE `treasure_gem_count` (\n  `id` int(10) unsigned NOT NULL,\n  `gem_Code` tinyint(3) unsigned NOT NULL,\n  \
         `tier` int(11) NOT NULL,\n  `count` int(11) NOT NULL,\n  `chance` float NOT NULL\n) ENGINE=InnoDB DEFAULT CHARSET=utf16;\n\
         INSERT INTO `treasure_gem_count` (`id`, `gem_Code`, `tier`, `count`, `chance`) VALUES\n(2, 1, 1, 0, 0.5),\n(1, 1, 1, 1, 0.25);\n\
         ALTER TABLE `treasure_gem_count`\n  ADD PRIMARY KEY (`id`);\n\
         ALTER TABLE `treasure_gem_count`\n  MODIFY `id` int(10) UNSIGNED NOT NULL AUTO_INCREMENT, AUTO_INCREMENT=3;",
    )]);
    let g = p
        .all::<TreasureGemCount>(TableId::TREASURE_GEM_COUNT)
        .unwrap();
    assert_eq!(
        g.iter().map(|(k, t)| (*k, t.chance)).collect::<Vec<_>>(),
        [(1, 0.25), (2, 0.5)]
    );
    assert_eq!(
        imp.applied[0].records["treasure_gem"],
        RecordCounts {
            added: 2,
            replaced: 0,
            deleted: 0
        }
    );
}

/// Every `content_json` vector: the SQL ACE's own writers produced for a JSON document, applied
/// here, holds exactly what applying the JSON document holds.
#[test]
fn ace_written_sql_and_its_json_build_the_same_pack() {
    let mut checked = 0;
    let mut both_fail = Vec::new();
    for (area, folder) in [
        ("weenie", "weenies"),
        ("recipe", "recipes"),
        ("landblock", "landblocks"),
        ("quest", "quests"),
    ] {
        let set = empyrean_common::vectors::load_named("content_json", area);
        for case in &set.cases {
            let Some(ace_sql) = case.output.get("sql").and_then(|s| s.as_str()) else {
                continue;
            };
            let name = case.input["name"].as_str().unwrap();
            let json = case.input["json"].as_str().unwrap();
            let now = dt(2026, 1, 2, 3, 4, 5);
            let from_sql = try_build_at(vec![sql(&format!("{name}.sql"), ace_sql)], now);
            let from_json = try_build_at(
                vec![Source {
                    kind: InputKind::Json,
                    path: PathBuf::from(format!("json/{folder}/{name}.json")),
                    bytes: json.as_bytes().to_vec(),
                }],
                now,
            );
            match (from_sql, from_json) {
                (Ok((a, _)), Ok((b, _))) => {
                    let (a, b) = (Pack::from_bytes(a).unwrap(), Pack::from_bytes(b).unwrap());
                    // V364 (a fix): ACE's SQL splices "NULL" into text holding ", ," or ", )";
                    // the JSON import (ours) keeps the text
                    let same = check::diff(&a, &b).unwrap().is_empty();
                    let spliced = [
                        "weenie/export_creature",
                        "weenie/export_book",
                        "weenie/emotes_sparse",
                        "recipe/export_full",
                        "quest/export",
                    ];
                    assert_eq!(
                        same,
                        !spliced.contains(&format!("{area}/{name}").as_str()),
                        "{area}/{name}"
                    );
                    checked += 1;
                }
                // What MySQL itself rejects fails both ways, at the same line for the same reason.
                (Err(a), Err(b)) => {
                    let (a, b) = (a.to_string(), b.to_string());
                    let reason = |e: &str| {
                        e.split_once(": ").map(|(l, r)| {
                            (l.to_owned(), r.split_once(": ").map(|x| x.1.to_owned()))
                        })
                    };
                    assert_eq!(reason(&a), reason(&b), "{area}/{name}");
                    both_fail.push(format!("{area}/{name}"));
                }
                (a, b) => panic!("{area}/{name}: SQL {:?} but JSON {:?}", a.err(), b.err()),
            }
        }
    }
    assert!(checked > 0, "recorded SQL/JSON comparisons are present");
    // NULL into a NOT NULL column (a null string stat, a page without an author), the writers'
    // `∞`/`Infinity` float text (not SQL), an empty quest key (not SQL), and a spawn map whose
    // guid collides with a base row of another landblock (a duplicate primary key).
    assert_eq!(
        both_fail,
        [
            "weenie/duplicates_and_nulls",
            "weenie/create_list_quirks",
            "weenie/book_pages",
            "weenie/skills_spells_positions",
            "weenie/float_overflow_is_infinity",
            "landblock/no_links",
            "quest/key_null",
        ]
    );
}
