//! Divergence: V462
//! The October 1999 asset target moves exactly the three reviewed Yanshi outpost rows into the
//! classic rooms that contain their stored positions, and refuses any source that differs.
//! Fixture: the outpost's rows as ac1999 writes them, loaded through the converter's SQL store.

use std::collections::BTreeMap;
use std::path::PathBuf;

use empyrean_content::import::mysqldump::{DumpSink, Row, TableDef};
use empyrean_content::import::sql::store::Store;
use empyrean_content::import::{self, patch};

use super::{Adaptation, Target, ADAPTER};
use crate::ac1999::convert::{self, number};

/// The start of ac1999's extended data for landblock 0xB973 at the pinned revision: the outpost
/// generator, its linked vendor, the second vendor and the door beside them.
const OUTPOST: &str = "\
DELETE FROM `landblock_instance` WHERE `landblock` = 0xB973;

INSERT INTO `landblock_instance` (`guid`, `weenie_Class_Id`, `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`, `is_Link_Child`, `last_Modified`)
VALUES (0x7B973000,  1154, 0xB9730100, 131.137, 8.92834, 44.405, 0.725515, 0, 0, -0.688206, False, '2005-02-09 10:00:00'); /* 01154 Unnamed Generator */

INSERT INTO `landblock_instance_link` (`parent_GUID`, `child_GUID`, `last_Modified`)
VALUES (0x7B973000, 0x7B973001, '2005-02-09 10:00:00') /* Lou Ei  (5064) */;

INSERT INTO `landblock_instance` (`guid`, `weenie_Class_Id`, `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`, `is_Link_Child`, `last_Modified`)
VALUES (0x7B973001,  5064, 0xB9730100, 129.502, 8.17398, 44.405, 0.067259, 0, 0, 0.997736,  True, '2005-02-09 10:00:00'); /* Lou Ei  */

INSERT INTO `landblock_instance` (`guid`, `weenie_Class_Id`, `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`, `is_Link_Child`, `last_Modified`)
VALUES (0x7B973002,  4565, 0xB9730106, 129.893, 16.8466, 40.005, -0.898448, 0, 0, 0.43908, False, '2005-02-09 10:00:00'); /* Dou In */

INSERT INTO `landblock_instance` (`guid`, `weenie_Class_Id`, `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`, `is_Link_Child`, `last_Modified`)
VALUES (0x7B973003,   720, 0xB9730000, 138.744, 11.05, 40, -1, 0, 0, 0, False, '2005-02-09 10:00:00'); /* Sliding Door */
";

/// The weenies the outpost rows name, so the projection keeps the rows instead of quarantining them.
const WEENIES: &str = "
INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`) VALUES
(720, 'door', 1, '2005-02-09 10:00:00'),
(1154, 'generator', 1, '2005-02-09 10:00:00'),
(4565, 'vendor-a', 1, '2005-02-09 10:00:00'),
(5064, 'vendor-b', 1, '2005-02-09 10:00:00');
";

fn load(sql: &str) -> Store {
    let source = patch::Source {
        kind: patch::InputKind::Sql,
        path: PathBuf::from("outpost.sql"),
        bytes: sql.as_bytes().to_vec(),
    };
    let loaded = import::load_store(convert::SOURCE_SCHEMA, [Ok(source)], import::default_now())
        .expect("outpost rows load");
    assert!(loaded.skipped_statements.is_empty());
    loaded.store
}

/// Each placed instance's adapted cell, by guid, as the converter's row pass would see it.
struct Cells {
    adaptation: Adaptation,
    cells: BTreeMap<i128, Option<u32>>,
}

impl DumpSink for Cells {
    fn create_table(&mut self, _: &TableDef) -> Result<(), empyrean_content::error::ImportError> {
        Ok(())
    }

    fn row(&mut self, row: &Row<'_>) -> Result<(), empyrean_content::error::ImportError> {
        let cell = self.adaptation.cell(row)?;
        if row.table == "landblock_instance" {
            self.cells.insert(number(row, "guid")?, cell);
        } else {
            assert_eq!(cell, None, "only placed instances move");
        }
        Ok(())
    }
}

fn adapt(
    target: Target,
    sql: &str,
) -> Result<BTreeMap<i128, Option<u32>>, empyrean_content::error::ImportError> {
    let mut sink = Cells {
        adaptation: Adaptation::new(target),
        cells: BTreeMap::new(),
    };
    load(sql).emit(&mut sink)?;
    sink.adaptation.complete()?;
    Ok(sink.cells)
}

#[test]
fn both_1999_targets_move_the_yanshi_outpost_rows_into_their_classic_rooms() {
    // Both play the retail CD's cell.dat, whose numbering the rooms were matched against.
    for target in [Target::Launch, Target::ClassicOctober1999] {
        let cells = adapt(target, OUTPOST).expect("reviewed rows adapt");
        assert_eq!(cells[&0x7B97_3000], Some(0xB973_0107));
        assert_eq!(cells[&0x7B97_3001], Some(0xB973_0107));
        assert_eq!(cells[&0x7B97_3002], Some(0xB973_010D));
    }
}

#[test]
fn rows_outside_the_reviewed_set_keep_their_cells() {
    let cells = adapt(Target::ClassicOctober1999, OUTPOST).expect("reviewed rows adapt");
    assert_eq!(cells[&0x7B97_3003], None);
}

#[test]
fn the_source_target_leaves_every_cell_as_the_source_wrote_it() {
    let cells = adapt(Target::Source, OUTPOST).expect("no adaptation");
    assert_eq!(cells.len(), 4);
    assert!(cells.values().all(Option::is_none), "{cells:X?}");
}

#[test]
fn the_launch_target_is_the_default() {
    assert_eq!(Target::default(), Target::Launch);
}

#[test]
fn a_reviewed_row_changed_in_any_field_refuses_the_adaptation() {
    for (from, to) in [
        // A coordinate.
        ("129.893, 16.8466", "129.894, 16.8466"),
        // The timestamp.
        (
            "0.43908, False, '2005-02-09 10:00:00'",
            "0.43908, False, '2005-02-10 10:00:00'",
        ),
        // The class.
        ("0x7B973002,  4565", "0x7B973002,  4566"),
        // The cell itself.
        (
            "0x7B973002,  4565, 0xB9730106",
            "0x7B973002,  4565, 0xB9730105",
        ),
    ] {
        assert!(OUTPOST.contains(from), "{from}");
        let error = adapt(Target::ClassicOctober1999, &OUTPOST.replace(from, to))
            .expect_err("a changed row is refused");
        let text = error.to_string();
        assert!(
            text.contains(ADAPTER) && text.contains("0x7B973002"),
            "{from}: {text}"
        );
    }
}

#[test]
fn a_reviewed_row_missing_from_the_source_refuses_the_adaptation() {
    let start = OUTPOST.find("VALUES (0x7B973001").unwrap();
    let insert = OUTPOST[..start].rfind("INSERT INTO").unwrap();
    let end = start + OUTPOST[start..].find('\n').unwrap();
    let without = format!("{}{}", &OUTPOST[..insert], &OUTPOST[end..]);
    let error = adapt(Target::ClassicOctober1999, &without).expect_err("a missing row is refused");
    let text = error.to_string();
    assert!(
        text.contains("missing required source row 0x7B973001"),
        "{text}"
    );
}

#[test]
fn only_the_three_named_asset_targets_are_accepted() {
    assert_eq!(Target::parse("source"), Ok(Target::Source));
    assert_eq!(Target::parse("launch"), Ok(Target::Launch));
    assert_eq!(
        Target::parse("classic-1999-10-09"),
        Ok(Target::ClassicOctober1999)
    );
    assert!(Target::parse("classic").is_err());
    assert!(Target::parse("Source").is_err());
    assert_eq!(
        Target::Source.manifest()["adapter"],
        serde_json::Value::Null
    );
    assert_eq!(Target::ClassicOctober1999.manifest()["adapter"], ADAPTER);
    assert_eq!(Target::Launch.manifest()["adapter"], ADAPTER);
}

#[test]
fn the_manifest_names_each_target_s_dats_by_iteration_and_hash() {
    let launch = Target::Launch.manifest();
    assert_eq!(launch["dats"]["portal.dat"]["capture"], "1999-10-31");
    assert_eq!(launch["dats"]["portal.dat"]["iteration"], 105);
    assert_eq!(launch["dats"]["cell.dat"]["capture"], "1999-10-09");
    assert_eq!(launch["dats"]["cell.dat"]["iteration"], 67);
    let classic = Target::ClassicOctober1999.manifest();
    assert_eq!(classic["dats"]["portal.dat"]["iteration"], 102);
    assert_eq!(classic["dats"]["cell.dat"], launch["dats"]["cell.dat"]);
    for d in [&launch["dats"]["portal.dat"], &launch["dats"]["cell.dat"]] {
        let sha = d["sha256"].as_str().unwrap();
        assert_eq!(sha.len(), 64);
        assert!(sha.bytes().all(|b| b.is_ascii_hexdigit()));
    }
    assert_eq!(Target::Source.manifest()["dats"], serde_json::Value::Null);
}

fn project(sql: &str) -> Result<(String, Vec<serde_json::Value>), String> {
    let source = load(sql);
    let canonical = import::load_store(convert::SCHEMA, std::iter::empty(), import::default_now())
        .expect("canonical schema")
        .store;
    let census = convert::Census::read(&source).map_err(|e| e.to_string())?;
    let mut projection = convert::Projection::new(Vec::new(), &canonical, census);
    projection.cells = Adaptation::new(Target::Launch);
    source.emit(&mut projection).map_err(|e| e.to_string())?;
    projection.finish().map_err(|e| e.to_string())?;
    projection.cells.complete().map_err(|e| e.to_string())?;
    let sql = String::from_utf8(projection.output).expect("projected SQL");
    Ok((sql, projection.ledger))
}

#[test]
fn the_projection_writes_the_classic_cell_and_records_the_source_cell() {
    let (sql, ledger) = project(&format!("{WEENIES}{OUTPOST}")).expect("projection");
    // Projected rows are written in decimal, one tuple per row.
    let tuple = |guid: u32| -> &str {
        let at = sql
            .find(&format!("({guid},"))
            .expect("placed instance projected");
        &sql[at..at + sql[at..].find(')').unwrap()]
    };
    for (guid, cell) in [
        (0x7B97_3000_u32, 0xB973_0107_u32),
        (0x7B97_3001, 0xB973_0107),
        (0x7B97_3002, 0xB973_010D),
        (0x7B97_3003, 0xB973_0000),
    ] {
        assert!(
            tuple(guid).contains(&format!(",{cell},")),
            "{guid:X} is written in {cell:X}: {}",
            tuple(guid)
        );
    }
    let adapted: Vec<_> = ledger.iter().filter(|r| r["action"] == "adapt").collect();
    assert_eq!(adapted.len(), 3);
    for record in adapted {
        assert_eq!(record["target"], "launch");
        assert_eq!(record["adapter"], ADAPTER);
        let before = record["changes"]["obj_Cell_Id"]["before"].as_i64().unwrap();
        let after = record["changes"]["obj_Cell_Id"]["after"].as_i64().unwrap();
        assert!(
            matches!(
                (before, after),
                (0xB973_0100, 0xB973_0107) | (0xB973_0106, 0xB973_010D)
            ),
            "{before:X} -> {after:X}"
        );
    }
}

#[test]
fn a_reviewed_row_the_projection_would_quarantine_refuses_the_adaptation() {
    // Without its weenies every placed instance names an absent class.
    let error = project(OUTPOST).expect_err("quarantined target rows are refused");
    assert!(
        error.contains("required target instance would be quarantined"),
        "{error}"
    );
}
