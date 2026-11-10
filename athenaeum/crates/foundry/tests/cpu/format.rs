//! Behaviour: none (checks the world builder's format reference against the rows the server reads, and that its sample world reads)

use std::collections::BTreeSet;
use std::path::PathBuf;

use athenaeum_foundry::world;
use yalain_world_db::rows::{read_partly, FIELDS, KINDS, TIER_FIELDS};

/// The format reference, as Athenaeum's tree holds it.
const REFERENCE: &str = include_str!("../../../../FORMAT.md");

/// Each kind's section of the reference: its kind, and the fields named in the first column of
/// each of its tables, table by table.
fn sections() -> Vec<(String, Vec<Vec<String>>)> {
    let mut out: Vec<(String, Vec<Vec<String>>)> = Vec::new();
    let mut in_table = false;
    for line in REFERENCE.lines() {
        if let Some(kind) = line
            .strip_prefix("#### `[[")
            .and_then(|r| r.strip_suffix("]]`"))
        {
            out.push((kind.to_string(), Vec::new()));
            in_table = false;
            continue;
        }
        if line.starts_with("## ") || line.starts_with("### ") {
            in_table = false;
            continue;
        }
        let Some((_, tables)) = out.last_mut() else {
            continue;
        };
        if !line.starts_with('|') {
            in_table = false;
            continue;
        }
        if !in_table {
            tables.push(Vec::new());
            in_table = true;
        }
        let first = line.split('|').nth(1).unwrap_or("");
        let names = first
            .split('`')
            .skip(1)
            .step_by(2)
            .filter(|n| n.chars().all(|c| c.is_ascii_lowercase() || c == '_'))
            .map(str::to_string);
        tables.last_mut().unwrap().extend(names);
    }
    out
}

/// The reference has a section for every kind of row the server reads and for no other, and each
/// section's first table names exactly the fields the server reads of its kind.
#[test]
fn the_format_reference_names_every_kind_and_every_field_the_server_reads() {
    let sections = sections();
    let documented: BTreeSet<&str> = sections.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(documented.len(), sections.len(), "a kind written twice");
    let kinds: BTreeSet<&str> = KINDS.iter().copied().collect();
    assert_eq!(documented, kinds, "the reference's kinds and the server's");
    for (kind, tables) in &sections {
        let fields = FIELDS.iter().find(|(k, _)| k == kind).unwrap().1;
        let shown: BTreeSet<&str> = tables
            .first()
            .map(|t| t.iter().map(String::as_str).collect())
            .unwrap_or_default();
        let read: BTreeSet<&str> = fields.iter().copied().collect();
        assert_eq!(shown, read, "[[{kind}]]'s fields");
    }
    let family = &sections
        .iter()
        .find(|(k, _)| k == "treasure_family")
        .unwrap()
        .1;
    let tiers: BTreeSet<&str> = family[1].iter().map(String::as_str).collect();
    assert_eq!(
        tiers,
        TIER_FIELDS.iter().copied().collect(),
        "a tier's fields"
    );
}

/// The reference's list of files names a file for every kind of row but `[[remove]]`, which any
/// file takes.
#[test]
fn the_format_reference_says_where_each_kind_is_written() {
    let files = REFERENCE
        .split("## Files")
        .nth(1)
        .and_then(|r| r.split("```").nth(1))
        .expect("the list of files");
    for kind in KINDS.iter().filter(|k| **k != "remove") {
        assert!(files.contains(&format!("[[{kind}]]")), "[[{kind}]]");
    }
}

/// The sample world, as Athenaeum's tree holds it.
pub fn sample_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../sample")
}

/// Every row of the sample world reads, in a pack of its own over the database.
#[test]
fn the_sample_world_reads_every_row() {
    let files = world::load(&sample_dir()).expect("the sample's files");
    let (db, problems) = read_partly(&files);
    assert!(problems.is_empty(), "{problems:?}");
    let db = db.expect("the sample reads");
    assert_eq!(db.world.packs, ["sample-arwic"]);
    assert!(db.world.base.is_some(), "it lies over the database");
    let kinds: BTreeSet<&str> = db.rows().map(|l| l.row.kind()).collect();
    assert_eq!(
        kinds,
        ["object", "camp", "place", "talk"].into_iter().collect(),
        "a person, a portal, a camp, a place and a line"
    );
}
