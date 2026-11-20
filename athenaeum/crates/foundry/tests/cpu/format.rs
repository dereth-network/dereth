//! Behaviour: none (checks the world builder's format reference against the rows the server reads, and that its sample world reads)

use std::collections::BTreeSet;
use std::path::PathBuf;

use athenaeum_foundry::world;
use yalain_world_db::rows::{read_partly, FIELDS, KINDS, TIER_FIELDS};

/// The format reference, as Athenaeum's tree holds it.
const REFERENCE: &str = include_str!("../../../../FORMAT.md");

/// Each kind's section of `reference`: its kind, and the fields named in the first column of each
/// of its tables, table by table.
fn sections(reference: &str) -> Vec<(String, Vec<Vec<String>>)> {
    let mut out: Vec<(String, Vec<Vec<String>>)> = Vec::new();
    let mut in_table = false;
    for line in reference.lines() {
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

/// The fields `what` shows and the server doesn't read, and the ones it reads and `what` doesn't
/// show, into `gaps`.
fn field_differences(gaps: &mut Vec<String>, what: &str, shown: &[String], read: &[&str]) {
    for f in read.iter().filter(|f| !shown.iter().any(|s| s == *f)) {
        gaps.push(format!("{what} lacks `{f}`"));
    }
    for f in shown.iter().filter(|s| !read.contains(&s.as_str())) {
        gaps.push(format!(
            "{what} names `{f}`, which the server does not read"
        ));
    }
}

/// Every way `reference`'s sections differ from the rows the server reads (`kinds`, each one's
/// `fields`, and a treasure family's `tier_fields`), one line each: a kind written twice, one the
/// server reads with no section, a section of a kind it doesn't read, and each field a kind's first
/// table (a treasure family's second, for a tier) leaves out or names but the server doesn't read.
fn field_gaps(
    reference: &str,
    kinds: &[&str],
    fields: &[(&str, &[&str])],
    tier_fields: &[&str],
) -> Vec<String> {
    let sections = sections(reference);
    let mut gaps = Vec::new();
    let mut documented = BTreeSet::new();
    for (kind, _) in &sections {
        if !documented.insert(kind.as_str()) {
            gaps.push(format!("[[{kind}]] is written twice"));
        }
    }
    for kind in kinds.iter().filter(|k| !documented.contains(*k)) {
        gaps.push(format!("[[{kind}]] has no section"));
    }
    for (kind, tables) in &sections {
        if !kinds.contains(&kind.as_str()) {
            gaps.push(format!("[[{kind}]] is no kind the server reads"));
            continue;
        }
        let read = fields
            .iter()
            .find(|(k, _)| k == kind)
            .map_or(&[][..], |(_, f)| *f);
        let table = |i: usize| tables.get(i).map_or(&[][..], Vec::as_slice);
        field_differences(&mut gaps, &format!("[[{kind}]]"), table(0), read);
        if kind == "treasure_family" {
            field_differences(&mut gaps, "a treasure family's tier", table(1), tier_fields);
        }
    }
    gaps
}

/// Each kind of `kinds` but `[[remove]]`, which any file takes, that `reference`'s list of files
/// puts in no file, one line each; or that it has no list.
fn file_gaps(reference: &str, kinds: &[&str]) -> Vec<String> {
    let Some(files) = reference
        .split("## Files")
        .nth(1)
        .and_then(|r| r.split("```").nth(1))
    else {
        return vec!["no list of files".to_string()];
    };
    kinds
        .iter()
        .filter(|k| **k != "remove" && !files.contains(&format!("[[{k}]]")))
        .map(|k| format!("[[{k}]] is in no file"))
        .collect()
}

/// `gaps` as an assertion's message: how many, then one a line.
fn listed(gaps: &[String]) -> String {
    format!("{} gap(s):\n  {}", gaps.len(), gaps.join("\n  "))
}

/// The reference has a section for every kind of row the server reads and for no other, and each
/// section's first table names exactly the fields the server reads of its kind.
#[test]
fn the_format_reference_names_every_kind_and_every_field_the_server_reads() {
    let gaps = field_gaps(REFERENCE, KINDS, FIELDS, &TIER_FIELDS);
    assert!(
        gaps.is_empty(),
        "the format reference's kinds and fields: {}",
        listed(&gaps)
    );
}

/// The reference's list of files names a file for every kind of row but `[[remove]]`, which any
/// file takes.
#[test]
fn the_format_reference_says_where_each_kind_is_written() {
    let gaps = file_gaps(REFERENCE, KINDS);
    assert!(
        gaps.is_empty(),
        "the format reference's files: {}",
        listed(&gaps)
    );
}

/// A small reference, with one of each gap the checks look for.
const SHORT_REFERENCE: &str = r#"## Files

```text
<pack>/server/a.toml    [[a]]
<pack>/server/t.toml    [[treasure_family]]
```

## Rows

#### `[[a]]`

| field | | meaning |
|---|---|---|
| `x` | required | read |
| `z` | | not read |

#### `[[c]]`

| field | | meaning |
|---|---|---|
| `x` | | |

#### `[[a]]`

#### `[[treasure_family]]`

| field | | meaning |
|---|---|---|
| `sort` | required | |

| field | | meaning |
|---|---|---|
| `low`, `odd` | | |

#### `[[remove]]`
"#;

/// **The checks list every gap of a reference in one run**, each kind and each field it lacks or
/// names in error, not the first alone; and none of a reference that matches.
#[test]
fn the_format_checks_list_every_gap_at_once() {
    let kinds = ["a", "b", "treasure_family", "remove"];
    let fields: [(&str, &[&str]); 4] = [
        ("a", &["x", "y"]),
        ("b", &["w"]),
        ("treasure_family", &["sort"]),
        ("remove", &[]),
    ];
    let tier = ["low", "high"];
    assert_eq!(
        field_gaps(SHORT_REFERENCE, &kinds, &fields, &tier),
        [
            "[[a]] is written twice",
            "[[b]] has no section",
            "[[a]] lacks `y`",
            "[[a]] names `z`, which the server does not read",
            "[[c]] is no kind the server reads",
            "[[a]] lacks `x`",
            "[[a]] lacks `y`",
            "a treasure family's tier lacks `high`",
            "a treasure family's tier names `odd`, which the server does not read",
        ]
    );
    assert_eq!(file_gaps(SHORT_REFERENCE, &kinds), ["[[b]] is in no file"]);
    assert_eq!(file_gaps("## Rows\n", &kinds), ["no list of files"]);

    // Negative control: what the reference shows, read, has no gaps.
    let kinds = ["a", "c", "treasure_family", "remove"];
    let shown: [(&str, &[&str]); 4] = [
        ("a", &["x", "z"]),
        ("c", &["x"]),
        ("treasure_family", &["sort"]),
        ("remove", &[]),
    ];
    let matching = SHORT_REFERENCE
        .replacen(
            "#### `[[a]]`\n\n#### `[[treasure_family]]`",
            "#### `[[treasure_family]]`",
            1,
        )
        .replace("[[a]]\n", "[[a]]\n<pack>/server/c.toml    [[c]]\n");
    assert_eq!(
        field_gaps(&matching, &kinds, &shown, &["low", "odd"]),
        Vec::<String>::new()
    );
    assert_eq!(file_gaps(&matching, &kinds), Vec::<String>::new());
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
