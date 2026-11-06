//! Divergence: V462
//! Treasure and emote-spell references: a death type naming a wielded list becomes the creature's
//! treasure create list, reviewed dangling references are quarantined, and any other dangling
//! reference refuses the conversion. Fixture: synthetic rows in ac1999's schema.

use std::collections::BTreeSet;
use std::path::PathBuf;

use empyrean_content::import::sql::store::Store;
use empyrean_content::import::{self, patch};

use super::{Index, DEATH_LIST_REASON, EMOTE_SPELL_ABSENT_REASON, WIELDED_ABSENT_REASON};
use crate::ac1999::convert;

/// A practice dummy whose death type names a wielded list, the six reviewed creatures with absent
/// wielded types, and the two reviewed emote spells.
const BASE: &str = "
INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`) VALUES
(2442, 'dummy', 10, '2005-02-09 10:00:00'), (5088, 'bread', 18, '2005-02-09 10:00:00'),
(5089, 'cheese', 18, '2005-02-09 10:00:00'), (227, 'a', 10, '2005-02-09 10:00:00'),
(619, 'b', 10, '2005-02-09 10:00:00'), (1404, 'c', 10, '2005-02-09 10:00:00'),
(1484, 'd', 10, '2005-02-09 10:00:00'), (1608, 'e', 10, '2005-02-09 10:00:00'),
(1610, 'f', 10, '2005-02-09 10:00:00'), (2568, 'rabbit', 10, '2005-02-09 10:00:00'),
(4201, 'madman', 10, '2005-02-09 10:00:00'), (900, 'drudge', 10, '2005-02-09 10:00:00');

INSERT INTO `treasure_wielded` (`id`, `treasure_Type`, `weenie_Class_Id`, `palette_Id`, `unknown_1`, `shade`, `stack_Size`, `stack_Size_Variance`, `probability`, `unknown_3`, `unknown_4`, `unknown_5`, `set_Start`, `has_Sub_Set`, `continues_Previous_Set`, `unknown_9`, `unknown_10`, `unknown_11`, `unknown_12`, `last_Modified`)
VALUES (127, 73, 5088, 0, 0, 0, 1, 0, 0.16, 0, 0, 0, True, False, False, 0, 0, 0, 0, '2005-02-09 10:00:00')
     , (128, 73, 5089, 0, 0, 0, 1, 0, 0.16, 0, 0, 0, False, False, False, 0, 0, 0, 0, '2005-02-09 10:00:00');

INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`) VALUES
(2442, 35, 73), (227, 32, 220), (619, 32, 250), (1404, 32, 250), (1484, 32, 300),
(1608, 32, 71), (1610, 32, 78), (900, 32, 73);

INSERT INTO `weenie_properties_create_list` (`id`, `object_Id`, `destination_Type`, `weenie_Class_Id`, `stack_Size`, `palette`, `shade`, `try_To_Bond`) VALUES
(40, 900, 2, 5088, 1, 0, 0, False);

INSERT INTO `weenie_properties_emote` (`id`, `object_Id`, `category`, `probability`) VALUES (1536, 2568, 3, 1), (705, 4201, 6, 1);
INSERT INTO `weenie_properties_emote_action` (`id`, `emote_Id`, `order`, `type`, `spell_Id`) VALUES
(3205, 1536, 0, 17, NULL), (3206, 1536, 1, 19, 2017), (1360, 705, 8, 14, 2648);
";

fn load(sql: &str) -> Store {
    let source = patch::Source {
        kind: patch::InputKind::Sql,
        path: PathBuf::from("references.sql"),
        bytes: sql.as_bytes().to_vec(),
    };
    let loaded = import::load_store(convert::SOURCE_SCHEMA, [Ok(source)], import::default_now())
        .expect("synthetic rows load");
    assert!(loaded.skipped_statements.is_empty());
    loaded.store
}

fn index(sql: &str) -> Index {
    let mut index = Index::default();
    load(sql).emit(&mut index).expect("index");
    index
}

/// Project the rows with the settled references, as the converter does.
fn project(sql: &str) -> (String, Vec<serde_json::Value>) {
    let source = load(sql);
    let canonical = import::load_store(convert::SCHEMA, std::iter::empty(), import::default_now())
        .expect("canonical schema")
        .store;
    let census = convert::Census::read(&source).expect("census");
    let settled = census.references.settle().expect("references settle");
    let mut projection = convert::Projection::new(Vec::new(), &canonical, census);
    projection.references = settled;
    source.emit(&mut projection).expect("projection");
    projection.finish().expect("finish");
    projection
        .references
        .write(&mut projection.output, &mut projection.ledger)
        .expect("restored rows");
    (
        String::from_utf8(projection.output).expect("projected SQL"),
        projection.ledger,
    )
}

#[test]
fn a_death_type_naming_a_wielded_list_becomes_the_treasure_create_list() {
    let (sql, ledger) = project(BASE);
    let restored: Vec<_> = ledger.iter().filter(|r| r["action"] == "restore").collect();
    assert_eq!(restored.len(), 2);
    for (r, class) in restored.iter().zip(["5088", "5089"]) {
        assert_eq!(r["values"]["object_Id"], "2442");
        assert_eq!(r["values"]["weenie_Class_Id"], class);
        assert_eq!(r["values"]["destination_Type"], "9");
        let shade: f32 = r["values"]["shade"].as_str().unwrap().parse().unwrap();
        assert_eq!(
            shade, 0.16_f32,
            "the list's probability, as the source's float"
        );
        assert_eq!(r["treasure_type"], 73);
    }
    // New ids follow the source's highest create-list id.
    assert!(sql.contains("(41,2442,9,5088,1,0,"), "{sql}");
    assert!(sql.contains("(42,2442,9,5089,1,0,"), "{sql}");
    let did = ledger
        .iter()
        .find(|r| r["table"] == "weenie_properties_d_i_d" && r["values"]["object_Id"] == "2442")
        .expect("the death type row is ledgered");
    assert_eq!(did["action"], "quarantine");
    assert_eq!(did["reason"], DEATH_LIST_REASON);
}

#[test]
fn the_reviewed_absent_wielded_types_and_emote_spells_are_quarantined_with_their_reasons() {
    let (_, ledger) = project(BASE);
    let wielded: BTreeSet<_> = ledger
        .iter()
        .filter(|r| r["reason"] == WIELDED_ABSENT_REASON)
        .map(|r| r["values"]["object_Id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        wielded,
        ["1404", "1484", "1608", "1610", "227", "619"]
            .map(str::to_owned)
            .into()
    );
    let spells: BTreeSet<_> = ledger
        .iter()
        .filter(|r| r["reason"] == EMOTE_SPELL_ABSENT_REASON)
        .map(|r| r["values"]["spell_Id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(spells, ["2017", "2648"].map(str::to_owned).into());
    // The rabbit's broadcast before the cast stays.
    assert!(!ledger
        .iter()
        .any(|r| r["values"]["id"] == "3205" && r["table"] == "weenie_properties_emote_action"));
}

#[test]
fn an_unreviewed_dangling_wielded_type_refuses_the_conversion() {
    let sql = BASE.replace("(900, 32, 73)", "(900, 32, 74)");
    let error = index(&sql).settle().expect_err("refused").to_string();
    assert!(
        error.contains("weenie 900 names wielded treasure 74"),
        "{error}"
    );
}

#[test]
fn an_unreviewed_dangling_death_type_refuses_the_conversion() {
    let sql = BASE.replace("(2442, 35, 73)", "(2442, 35, 75)");
    let error = index(&sql).settle().expect_err("refused").to_string();
    assert!(
        error.contains("weenie 2442 names death treasure 75"),
        "{error}"
    );
}

#[test]
fn an_unreviewed_emote_spell_refuses_the_conversion() {
    let sql = BASE.replace("(3206, 1536, 1, 19, 2017)", "(3206, 1536, 1, 19, 2018)");
    let error = index(&sql).settle().expect_err("refused").to_string();
    assert!(error.contains("casts spell 2018"), "{error}");
}

#[test]
fn a_death_list_on_a_creature_that_already_rolls_treasure_refuses_the_conversion() {
    let sql = BASE.replace(
        "(40, 900, 2, 5088, 1, 0, 0, False)",
        "(40, 2442, 9, 5088, 1, 0, 0.5, False)",
    );
    let error = index(&sql).settle().expect_err("refused").to_string();
    assert!(error.contains("cannot express"), "{error}");
}

#[test]
fn the_written_content_check_refuses_any_dangling_reference() {
    let index = index(BASE);
    let error = index
        .verify(&BTreeSet::new())
        .expect_err("refused")
        .to_string();
    for part in [
        "weenie 2442 DID 35 -> 73",
        "weenie 227 DID 32 -> 220",
        "weenie 2568 emote spell 2017",
    ] {
        assert!(error.contains(part), "{part}: {error}");
    }
    let clean = BASE
        .replace("(2442, 35, 73), ", "")
        .replace(
            "(227, 32, 220), (619, 32, 250), (1404, 32, 250), (1484, 32, 300),\n(1608, 32, 71), (1610, 32, 78), ",
            "",
        )
        .replace(", (3206, 1536, 1, 19, 2017), (1360, 705, 8, 14, 2648)", "");
    index_ok(&clean);
}

fn index_ok(sql: &str) {
    index(sql)
        .verify(&BTreeSet::new())
        .expect("no dangling reference");
}

#[test]
fn an_emote_giving_an_undefined_item_is_quarantined() {
    let sql = format!(
        "{BASE}\nINSERT INTO `weenie_properties_emote_action` (`id`, `emote_Id`, `order`, `type`, `weenie_Class_Id`) VALUES (3300, 1536, 2, 3, 27436), (3301, 1536, 3, 3, 5088);\n"
    );
    let (_, ledger) = project(&sql);
    let gives: Vec<_> = ledger
        .iter()
        .filter(|r| {
            r["table"] == "weenie_properties_emote_action"
                && r["reason"] == "referenced weenie is absent"
        })
        .map(|r| r["values"]["id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(gives, ["3300"]);
}
