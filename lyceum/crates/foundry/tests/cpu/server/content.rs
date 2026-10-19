//! Behaviour: none (tooling: the world editor's content forms, over ACE SQL)

use std::path::Path;

use lyceum_foundry::content::{self, PropertyKind, SqlDoc};
use lyceum_foundry::edit::{Op, Session};

use crate::common::Fixture;

/// The sample pack's weenie, as written by hand in ACE's export layout.
fn guide() -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../sample/holtburg/server/sql/weenies/guide.sql"),
    )
    .unwrap()
}

#[test]
fn a_document_read_and_written_again_is_the_same_text() {
    let text = guide();
    let doc = SqlDoc::parse(&text);
    assert_eq!(doc.render(), text);
    assert_eq!(doc.class().as_deref(), Some("{{wcid:guide}}"));
    assert_eq!(doc.rows("weenie_properties_int").len(), 10);
    // A comment inside a value is kept, and read past.
    let set = doc.emote_sets()[0];
    assert_eq!(doc.raw(set.emote, "category"), Some("7 /* Use */"));
    assert_eq!(doc.cell(set.emote, "category").as_deref(), Some("7"));
}

#[test]
fn a_property_set_added_and_taken_away_changes_its_row_alone() {
    let text = guide();
    let mut doc = SqlDoc::parse(&text);
    // Changed: its row is written again, its name kept and the old value's description dropped.
    assert!(doc.set_property(PropertyKind::Int, 2, "13", None));
    assert_eq!(doc.property(PropertyKind::Int, 2).as_deref(), Some("13"));
    let out = doc.render();
    assert!(
        out.contains("({{wcid:guide}},   2,         13) /* CreatureType */"),
        "{out}"
    );
    assert_eq!(
        out.replace(
            "({{wcid:guide}},   2,         13) /* CreatureType */",
            "({{wcid:guide}},   2,          3) /* CreatureType - Drudge */"
        ),
        text,
        "every other row as written"
    );
    // Added: in order of its type.
    assert!(doc.set_property(PropertyKind::Int, 18, "1", Some("UiEffects".into())));
    let ints: Vec<u32> = doc
        .properties(PropertyKind::Int)
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    assert_eq!(ints, [1, 2, 6, 7, 16, 18, 25, 93, 95, 133, 134]);
    // A type with no statement yet gets one, after the ints and before the bools.
    assert!(doc.set_property(PropertyKind::Int64, 1, "5000", None));
    let out = doc.render();
    let at = |s: &str| out.find(s).unwrap();
    assert!(at("`weenie_properties_int`") < at("`weenie_properties_int64`"));
    assert!(at("`weenie_properties_int64`") < at("`weenie_properties_bool`"));
    assert!(out.contains(";\n\nINSERT INTO `weenie_properties_int64` (`object_Id`, `type`, `value`)\nVALUES ({{wcid:guide}},   1,       5000);\n\nINSERT INTO `weenie_properties_bool`"), "{out}");
    // A value not of the type is refused.
    assert!(!doc.set_property(PropertyKind::Int, 25, "five", None));
    assert!(!doc.set_property(PropertyKind::Bool, 1, "maybe", None));
    // Taken away: the int64 statement goes with its one row, and the ints are as they were.
    assert!(doc.remove_property(PropertyKind::Int64, 1));
    assert!(doc.remove_property(PropertyKind::Int, 18));
    assert!(doc.set_property(
        PropertyKind::Int,
        2,
        "3",
        Some("CreatureType - Drudge".into())
    ));
    assert_eq!(doc.render(), text);
}

#[test]
fn an_emote_set_and_its_actions_are_added_ordered_and_taken_away() {
    let text = guide();
    let mut doc = SqlDoc::parse(&text);
    let sets = doc.emote_sets();
    assert_eq!(sets.len(), 1);
    assert_eq!(doc.actions(&sets[0]).len(), 1);
    // A second action before the first: the two are numbered again.
    doc.add_action(
        0,
        0,
        &[
            ("type", "5 /* Motion */".into()),
            ("delay", "0".into()),
            ("extent", "1".into()),
            ("motion", "0x10000087".into()),
        ],
    );
    let set = doc.emote_sets()[0];
    let acts = doc.actions(&set);
    let order: Vec<String> = acts
        .iter()
        .map(|a| doc.cell(*a, "order").unwrap())
        .collect();
    assert_eq!(order, ["0", "1"]);
    assert_eq!(doc.cell(acts[0], "type").as_deref(), Some("5"));
    assert_eq!(doc.cell(acts[1], "type").as_deref(), Some("10"));
    doc.move_action(0, 0, 1);
    let acts = doc.actions(&doc.emote_sets()[0]);
    assert_eq!(doc.cell(acts[0], "type").as_deref(), Some("10"));
    assert_eq!(doc.cell(acts[0], "order").as_deref(), Some("0"));
    // A new set: its actions follow it, after the `SET` that ties them.
    let i = doc.add_emote_set("1 /* Refuse */", &[]);
    assert_eq!(i, 1);
    doc.add_action(
        1,
        0,
        &[
            ("type", "10 /* Tell */".into()),
            ("message", content::quote("Not that.")),
        ],
    );
    let out = doc.render();
    let reread = SqlDoc::parse(&out);
    let sets = reread.emote_sets();
    assert_eq!(sets.len(), 2);
    assert_eq!(reread.cell(sets[1].emote, "category").as_deref(), Some("1"));
    let acts = reread.actions(&sets[1]);
    assert_eq!(acts.len(), 1);
    assert_eq!(
        content::unquote(reread.raw(acts[0], "message").unwrap()),
        "Not that."
    );
    assert_eq!(
        reread.cell(acts[0], "emote_Id").as_deref(),
        Some("@parent_id")
    );
    assert_eq!(out.matches("SET @parent_id = LAST_INSERT_ID();").count(), 2);
    // Taken back out, the file is as it was.
    doc.remove_emote_set(1);
    doc.move_action(0, 1, 0);
    doc.remove_action(0, 0);
    assert_eq!(doc.render(), text);
    // Its last action taken, a set keeps no statement of actions.
    doc.remove_action(0, 0);
    assert_eq!(doc.emote_sets()[0].actions, None);
    assert!(!doc.render().contains("SET @parent_id"));
}

/// A world database in a dump's layout: two classes, one with an emote of two actions.
const WORLD: &str = "\
CREATE TABLE `weenie` (
  `class_Id` int(10) unsigned NOT NULL,
  `class_Name` varchar(100) NOT NULL,
  `type` int(5) NOT NULL DEFAULT '0',
  `last_Modified` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP
);
INSERT INTO `weenie` VALUES (7,'drudgeskulker',10,'2005-02-09 10:00:00'),(8,'other',10,'2005-02-09 10:00:00');
CREATE TABLE `weenie_properties_bool` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `object_Id` int(10) unsigned NOT NULL,
  `type` smallint(5) unsigned NOT NULL DEFAULT '0',
  `value` bit(1) NOT NULL
);
INSERT INTO `weenie_properties_bool` VALUES (1,7,1,1),(2,8,1,0);
CREATE TABLE `weenie_properties_string` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `object_Id` int(10) unsigned NOT NULL,
  `type` smallint(5) unsigned NOT NULL DEFAULT '0',
  `value` text NOT NULL
);
INSERT INTO `weenie_properties_string` VALUES (3,7,1,'Drudge Skulker'),(4,7,16,'A drudge.');
CREATE TABLE `weenie_properties_d_i_d` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `object_Id` int(10) unsigned NOT NULL,
  `type` smallint(5) unsigned NOT NULL DEFAULT '0',
  `value` int(10) unsigned NOT NULL DEFAULT '0'
);
INSERT INTO `weenie_properties_d_i_d` VALUES (5,7,1,33556445);
CREATE TABLE `weenie_properties_emote` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `object_Id` int(10) unsigned NOT NULL,
  `category` int(10) unsigned NOT NULL,
  `probability` float NOT NULL DEFAULT '1',
  `quest` text
);
INSERT INTO `weenie_properties_emote` VALUES (90,7,7,1,NULL),(91,8,7,1,NULL);
CREATE TABLE `weenie_properties_emote_action` (
  `id` int(10) unsigned NOT NULL AUTO_INCREMENT,
  `emote_Id` int(10) unsigned NOT NULL,
  `order` int(10) unsigned NOT NULL,
  `type` int(10) unsigned NOT NULL,
  `message` text
);
INSERT INTO `weenie_properties_emote_action` VALUES (200,90,1,10,'Second.'),(201,90,0,10,'First.'),(202,91,0,10,'Not mine.');
";

#[test]
fn a_class_of_the_world_database_is_copied_with_its_emotes_tied_to_their_actions() {
    let t = content::template(WORLD, 7, "skulker", false).unwrap();
    let doc = SqlDoc::parse(&t);
    assert_eq!(doc.render(), t, "a template reads back as written");
    assert_eq!(doc.class().as_deref(), Some("{{wcid:skulker}}"));
    assert!(t.starts_with("DELETE FROM `weenie` WHERE `class_Id` = {{wcid:skulker}};\n"));
    assert!(t.contains("VALUES ({{wcid:skulker}}, 'skulker', 10, '2005-02-09 10:00:00');"));
    // The database's own row ids are gone; a bit is True or False, a data id hexadecimal.
    assert_eq!(doc.property(PropertyKind::Bool, 1).as_deref(), Some("True"));
    assert_eq!(
        doc.property(PropertyKind::DataId, 1).as_deref(),
        Some("0x020007DD")
    );
    assert_eq!(
        doc.property(PropertyKind::String, 16).as_deref(),
        Some("'A drudge.'")
    );
    let sets = doc.emote_sets();
    assert_eq!(sets.len(), 1, "only its own emote");
    let acts = doc.actions(&sets[0]);
    let said: Vec<String> = acts
        .iter()
        .map(|a| content::unquote(doc.raw(*a, "message").unwrap()))
        .collect();
    assert_eq!(said, ["First.", "Second."], "in their order");
    // A blank leaves the emotes and every string but the name.
    let b = content::template(WORLD, 7, "blank-creature", true).unwrap();
    let doc = SqlDoc::parse(&b);
    assert!(doc.emote_sets().is_empty());
    assert_eq!(doc.properties(PropertyKind::String).len(), 1);
    assert!(content::template(WORLD, 9, "none", false).is_none());
    let classes = content::classes(WORLD);
    assert_eq!(classes.get(&7).map(String::as_str), Some("drudgeskulker"));
    assert_eq!(classes.len(), 2);
}

#[test]
fn a_weenie_written_by_a_form_is_numbered_and_filled_as_a_build_fills_it_and_undone() {
    let f = Fixture::new("content-write");
    let world = f.world();
    let mut s = Session::open(&world, &f.dats(), None).unwrap();
    let file = f.path().join("sample/server/sql/weenies/skulker.sql");
    let text = content::template(WORLD, 7, "skulker", false).unwrap();
    let applied = s
        .apply(&Op::WriteSql {
            file: file.clone(),
            text: Some(text.clone()),
        })
        .unwrap();
    assert!(applied.touched.sql);
    let filled = &s.compiled().sql[Path::new("sample/weenies/skulker.sql")];
    let id = s
        .compiled()
        .lock
        .of(lyceum_foundry::symbol::Kind::Wcid)
        .find(|(n, _)| *n == "sample:skulker")
        .map(|(_, id)| id)
        .expect("numbered");
    assert!(
        filled.contains(&format!("DELETE FROM `weenie` WHERE `class_Id` = {id};")),
        "{filled}"
    );
    // Not an SQL file of the pack: refused.
    assert!(s
        .apply(&Op::WriteSql {
            file: f.path().join("sample/edits/landblocks/0101.toml"),
            text: Some(String::new()),
        })
        .is_err());
    s.undo().unwrap();
    assert!(s.compiled().sql.is_empty());
    s.redo().unwrap();
    s.save().unwrap();
    assert_eq!(std::fs::read_to_string(&file).unwrap(), text);
}

#[test]
fn actions_after_an_emote_statement_of_several_rows_are_its_first_rows() {
    // `LAST_INSERT_ID()` after a statement of several rows is its first row's id.
    let doc = SqlDoc::parse(
        "INSERT INTO `weenie_properties_emote` (`object_Id`, `category`, `probability`)\n\
         VALUES (5, 1, 1)\n     , (5, 7, 1);\n\n\
         SET @parent_id = LAST_INSERT_ID();\n\n\
         INSERT INTO `weenie_properties_emote_action` (`emote_Id`, `order`, `type`, `delay`, `extent`)\n\
         VALUES (@parent_id, 0, 10, 0, 1)\n     , (@parent_id, 1, 10, 0, 1);\n",
    );
    let sets = doc.emote_sets();
    assert_eq!(sets.len(), 2);
    assert_eq!(doc.actions(&sets[0]).len(), 2);
    assert!(doc.actions(&sets[1]).is_empty());
}
