//! Divergence: V462
//! The 1999 world keeps what ac1999 chose and drops only what the 1999 files force: what places
//! content the 1999 portal has no art for, placements in rooms the 1999 cells lack with their linked
//! children, and destinations there. Fixture: synthetic rows in ac1999's schema built from the
//! reviewed lists themselves.

use std::fmt::Write as _;
use std::path::PathBuf;

use empyrean_content::import::{self, patch};

use super::{Policy, CUT, MISSING_CELLS, MISSING_DESTINATIONS};
use crate::ac1999::cells::Target;
use crate::ac1999::convert;

const STAMP: &str = "'2005-02-09 10:00:00'";

fn instance(sql: &mut String, guid: i128, class: i128, cell: i128, child: bool) {
    let _ = writeln!(
        sql,
        "INSERT INTO `landblock_instance` (`guid`, `weenie_Class_Id`, `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`, `is_Link_Child`, `last_Modified`) VALUES ({guid}, {class}, {cell}, 1, 1, 1, 1, 0, 0, 0, {}, {STAMP});",
        if child { "True" } else { "False" }
    );
}

fn link(sql: &mut String, parent: i128, child: i128) {
    let _ = writeln!(
        sql,
        "INSERT INTO `landblock_instance_link` (`parent_GUID`, `child_GUID`, `last_Modified`) VALUES ({parent}, {child}, {STAMP});"
    );
}

/// Every reviewed row the policy expects, plus the ordinary content around it.
fn world() -> String {
    let mut sql = String::new();
    let mut classes: Vec<i128> = CUT.iter().map(|c| c.0).collect();
    classes.extend(MISSING_CELLS.iter().map(|m| m.1));
    classes.extend(MISSING_DESTINATIONS.iter().map(|d| d.0));
    classes.extend([1, 2, 3953, 61001, 61700]);
    classes.sort_unstable();
    classes.dedup();
    for class in classes {
        let _ = writeln!(
            sql,
            "INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`) VALUES ({class}, 'w{class}', 1, {STAMP});"
        );
    }
    for (i, &(guid, class, cell)) in MISSING_CELLS.iter().enumerate() {
        instance(&mut sql, guid, class, cell, false);
        let child = 0x7000_0000 + i128::try_from(i).unwrap();
        instance(&mut sql, child, 2, cell & !0xFFFF, true);
        link(&mut sql, guid, child);
    }
    // A town generator of a cut class, and an ordinary placement beside it.
    instance(&mut sql, 0x7A00_0001, 8275, 0xA9B4_0000, false);
    instance(&mut sql, 0x7A00_0002, 1, 0xA9B4_0000, false);
    // A link generator whose only child is cut, as the snowmen are placed.
    instance(&mut sql, 0x7B00_0001, 3953, 0xA9B5_0000, false);
    instance(&mut sql, 0x7B00_0002, 5765, 0xA9B5_0000, true);
    link(&mut sql, 0x7B00_0001, 0x7B00_0002);
    // A region generator's edge into a seasonal generator, its ordinary edge, and a chest's
    // treasure-mode row whose number happens to be the seasonal generator's class.
    for (object, class, mode) in [(61001, 8271, 1), (61001, 2, 1), (61700, 8271, 0x48)] {
        let _ = writeln!(
            sql,
            "INSERT INTO `weenie_properties_generator` (`object_Id`, `probability`, `weenie_Class_Id`, `where_Create`) VALUES ({object}, 0.5, {class}, {mode});"
        );
    }
    // A seasonal generator's own edge, which stays with its definition.
    let _ = writeln!(
        sql,
        "INSERT INTO `weenie_properties_generator` (`object_Id`, `probability`, `weenie_Class_Id`, `where_Create`) VALUES (8271, 1, 8271, 1);"
    );
    for &(object, kind, cell) in MISSING_DESTINATIONS {
        let _ = writeln!(
            sql,
            "INSERT INTO `weenie_properties_position` (`object_Id`, `position_Type`, `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`) VALUES ({object}, {kind}, {cell}, 1, 1, 1, 1, 0, 0, 0);"
        );
    }
    let _ = writeln!(
        sql,
        "INSERT INTO `event` (`name`, `start_Time`, `end_Time`, `state`, `last_Modified`) VALUES ('stayson', -1, -1, 4, {STAMP});"
    );
    let _ = writeln!(
        sql,
        "INSERT INTO `event` (`name`, `start_Time`, `end_Time`, `state`, `last_Modified`) VALUES ('alreadyoff', -1, -1, 3, {STAMP});"
    );
    sql
}

fn project(target: Target, sql: &str) -> Result<(String, Vec<serde_json::Value>), String> {
    let source = patch::Source {
        kind: patch::InputKind::Sql,
        path: PathBuf::from("world.sql"),
        bytes: sql.as_bytes().to_vec(),
    };
    let source = import::load_store(convert::SOURCE_SCHEMA, [Ok(source)], import::default_now())
        .map_err(|e| e.to_string())?
        .store;
    let canonical = import::load_store(convert::SCHEMA, std::iter::empty(), import::default_now())
        .expect("canonical schema")
        .store;
    let census = convert::Census::read(&source).map_err(|e| e.to_string())?;
    let mut policy = Policy::new(target);
    policy
        .settle(&census.placements)
        .map_err(|e| e.to_string())?;
    let mut projection = convert::Projection::new(Vec::new(), &canonical, census);
    projection.world = policy;
    source.emit(&mut projection).map_err(|e| e.to_string())?;
    projection.finish().map_err(|e| e.to_string())?;
    projection.world.complete().map_err(|e| e.to_string())?;
    Ok((
        String::from_utf8(projection.output).expect("projected SQL"),
        projection.ledger,
    ))
}

fn quarantined<'a>(ledger: &'a [serde_json::Value], table: &str) -> Vec<&'a serde_json::Value> {
    ledger
        .iter()
        .filter(|r| r["table"] == table && r["action"] == "quarantine")
        .collect()
}

#[test]
fn placements_in_missing_cells_go_with_their_linked_children_and_links() {
    let (_, ledger) = project(Target::Launch, &world()).expect("projection");
    let instances = quarantined(&ledger, "landblock_instance");
    for &(guid, _, _) in MISSING_CELLS {
        assert!(instances
            .iter()
            .any(|r| r["values"]["guid"] == guid.to_string()));
    }
    let children = instances
        .iter()
        .filter(|r| {
            r["reason"]
                .as_str()
                .unwrap()
                .starts_with("linked child of a dropped placement")
        })
        .count();
    assert_eq!(children, MISSING_CELLS.len());
    assert_eq!(
        quarantined(&ledger, "landblock_instance_link").len(),
        MISSING_CELLS.len() + 1
    );
    assert_eq!(quarantined(&ledger, "weenie_properties_position").len(), 1);
}

#[test]
fn what_places_cut_content_goes_and_its_definitions_stay() {
    let (sql, ledger) = project(Target::Launch, &world()).expect("projection");
    let instances = quarantined(&ledger, "landblock_instance");
    let guid = |g: i128| {
        instances
            .iter()
            .find(|r| r["values"]["guid"] == g.to_string())
    };
    assert!(guid(0x7A00_0001).is_some(), "the seasonal town generator");
    assert!(guid(0x7A00_0002).is_none(), "the ordinary placement");
    assert!(guid(0x7B00_0002).is_some(), "the cut child");
    assert_eq!(
        guid(0x7B00_0001).unwrap()["reason"],
        "generator whose every linked child is dropped"
    );
    let edges = quarantined(&ledger, "weenie_properties_generator");
    assert_eq!(edges.len(), 1, "only the region's seasonal edge");
    assert_eq!(edges[0]["values"]["object_Id"], "61001");
    assert_eq!(edges[0]["values"]["weenie_Class_Id"], "8271");
    // Every cut class is still defined.
    for (class, _) in CUT {
        assert!(sql.contains(&format!("({class},'w{class}',")), "{class}");
    }
}

/// Which events run at launch is ac1999's to say: they are written as the source starts them.
#[test]
fn events_keep_the_state_the_source_gives_them() {
    let (sql, ledger) = project(Target::Launch, &world()).expect("projection");
    assert!(!ledger.iter().any(|r| r["table"] == "event"), "{ledger:#?}");
    assert!(sql.contains("'stayson',-1,-1,4,"));
    assert!(sql.contains("'alreadyoff',-1,-1,3,"));
}

#[test]
fn a_reviewed_placement_that_changed_refuses_the_conversion() {
    let (guid, class, cell) = MISSING_CELLS[0];
    let sql = world().replace(
        &format!("VALUES ({guid}, {class}, {cell},"),
        &format!("VALUES ({guid}, {class}, {},", cell + 1),
    );
    let error = project(Target::Launch, &sql).expect_err("refused");
    assert!(error.contains("does not match the reviewed row"), "{error}");
}

/// The world as the policy leaves it, with what it reviewed that the source no longer has.
fn gone(sql: &str) -> Vec<String> {
    let source = patch::Source {
        kind: patch::InputKind::Sql,
        path: PathBuf::from("world.sql"),
        bytes: sql.as_bytes().to_vec(),
    };
    let source = import::load_store(convert::SOURCE_SCHEMA, [Ok(source)], import::default_now())
        .expect("source")
        .store;
    let canonical = import::load_store(convert::SCHEMA, std::iter::empty(), import::default_now())
        .expect("canonical schema")
        .store;
    let census = convert::Census::read(&source).expect("census");
    let mut policy = Policy::new(Target::Launch);
    policy.settle(&census.placements).expect("settled");
    let mut projection = convert::Projection::new(Vec::new(), &canonical, census);
    projection.world = policy;
    source.emit(&mut projection).expect("emitted");
    projection.finish().expect("finished");
    projection.world.complete().expect("complete");
    projection.world.gone.into_iter().collect()
}

/// A later source that removed what the policy cuts leaves less to cut, so a new revision is not
/// refused for it; the manifest says what went, for the next review.
#[test]
fn what_the_source_has_since_removed_is_noted_and_not_refused() {
    assert!(
        gone(&world()).is_empty(),
        "the reviewed revision has it all"
    );
    let (class, _) = *CUT.last().unwrap();
    let (guid, _, _) = MISSING_CELLS[0];
    let (object, _, _) = MISSING_DESTINATIONS[0];
    let sql: String = world()
        .lines()
        .filter(|l| {
            !l.contains(&format!("VALUES ({class}, 'w{class}'"))
                && !l.contains(&format!("VALUES ({guid}, "))
                && !l.contains(&format!("({guid}, "))
                && !l.contains(&format!("VALUES ({object}, 'w{object}'"))
                && !(l.contains("`weenie_properties_position`")
                    && l.contains(&format!("({object},")))
        })
        .collect::<Vec<_>>()
        .join(
            "
",
        );
    let noted = gone(&sql);
    for expected in [
        format!("cut class {class}"),
        format!("placement {guid:#X}"),
        format!("destination of {object}"),
    ] {
        assert!(noted.contains(&expected), "{expected} in {noted:?}");
    }
}

/// A reviewed destination whose object is still there but points elsewhere has changed, not gone.
#[test]
fn a_reviewed_destination_that_changed_refuses_the_conversion() {
    let (object, _, cell) = MISSING_DESTINATIONS[0];
    let sql: String = world()
        .lines()
        .map(|l| {
            if l.contains("`weenie_properties_position`") && l.contains(&format!("({object},")) {
                l.replace(&cell.to_string(), &(cell + 1).to_string())
            } else {
                l.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(
            "
",
        );
    let error = project(Target::Launch, &sql).expect_err("refused");
    assert!(error.contains("reviewed destination"), "{error}");
}

#[test]
fn the_source_target_keeps_every_row() {
    let (_, ledger) = project(Target::Source, &world()).expect("projection");
    assert!(ledger.is_empty(), "{ledger:#?}");
}

#[test]
fn the_classic_target_applies_the_same_world() {
    let (_, launch) = project(Target::Launch, &world()).expect("launch");
    let (_, classic) = project(Target::ClassicOctober1999, &world()).expect("classic");
    assert_eq!(launch, classic);
}
