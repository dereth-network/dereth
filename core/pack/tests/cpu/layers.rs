//! Behaviour: none (tooling: the pack format's patch layers, their fold and its checks)
//! A world's lineage of patch layers read, its layers merged field by field and folded, its
//! checkpoints' change computed from their snapshots and their drift acknowledged, and rows read
//! from TOML tables, in the test's own dialect.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use dereth_pack::conflict::{Item, Override, Scope};
use dereth_pack::fold::{
    fold, merge_layer, resolve, Finding, Folded, Input, Merge, Origin, Removal, Row, Severity,
    Snapshot,
};
use dereth_pack::lineage::{Ack, Lineage, Take};
use dereth_pack::pack::PackHeader;
use dereth_pack::provenance::{Provenance, Source};
use dereth_pack::records::{read_pack_file, read_pack_file_from, snapshot_row, tables, RowKinds};
use dereth_pack::store::{MemoryStore, SourceStore};
use dereth_pack::{Dialect, Error, Loc, World};

use super::{Toy, ToyEra, ToyKind};

fn at(file: &str, line: usize) -> Loc {
    Loc::new(Path::new(file), line)
}

fn class(n: u32) -> Item {
    Item::new(format!("class {n}"))
}

/// A row of `pack` setting `fields` of class `n`.
fn row(n: u32, pack: &str, line: usize, fields: &[(&str, &str)]) -> Row<String> {
    let mut r = Row::new(
        class(n),
        pack,
        &at(&format!("{pack}/rows.toml"), line),
        Provenance::AUTHORED,
    );
    for (k, v) in fields {
        r = r.set(k, (*v).to_owned());
    }
    r
}

fn removal(n: u32, pack: &str, line: usize) -> Removal {
    Removal {
        item: format!("class {n}"),
        pack: pack.to_owned(),
        at: at(&format!("{pack}/rows.toml"), line),
    }
}

fn merged(rows: &[Row<String>], removals: &[Removal]) -> Merge<String> {
    merge_layer(rows, removals, &[])
}

/// A snapshot of classes and their fields, converted from `source` at `revision`.
fn snapshot(revision: &str, items: &[(u32, &[(&str, &str)])]) -> Snapshot<String> {
    let prov: Provenance = toml::from_str::<Holder>(&format!(
        "prov = {{ from = \"source\", source = \"toy-world\", revision = \"{revision}\" }}"
    ))
    .unwrap()
    .prov;
    let mut s = Snapshot::new();
    for (n, fields) in items {
        s.push(
            class(*n),
            &prov,
            fields
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned())),
        )
        .expect("each class once");
    }
    s
}

#[derive(serde::Deserialize)]
struct Holder {
    prov: Provenance,
}

fn ack(n: u32, field: Option<&str>, take: Take) -> Ack {
    Ack {
        item: format!("class {n}"),
        field: field.map(str::to_owned),
        layer: None,
        take,
        note: None,
        at: at("drift.toml", 1),
    }
}

fn value<'a>(f: &'a Folded<String>, n: u32, field: &str) -> Option<&'a str> {
    f.world
        .get(&format!("class {n}"))
        .and_then(|e| e.fields.get(field))
        .map(|f| f.value.as_str())
}

fn names(f: &Folded<String>) -> Vec<String> {
    f.world.iter().map(|e| e.item.name.clone()).collect()
}

/// The test's lineage: a base pinned to a snapshot with a correction of its own, a delta, and a
/// checkpoint whose snapshot changed some fields since the base's and agrees on others.
fn lineage(acks: Vec<Ack>) -> Vec<Input<String>> {
    let launch = snapshot(
        "r1",
        &[
            (1, &[("name", "stone"), ("use", "old"), ("radius", "2")]),
            (2, &[("name", "cow")]),
            (3, &[("name", "bowl")]),
        ],
    );
    let fixes = merged(&[row(1, "fixes", 1, &[("use", "fixed")])], &[]);
    let patch = merged(
        &[
            row(2, "herd", 1, &[("name", "bull")]),
            row(4, "herd", 5, &[("name", "arrow")]),
            row(1, "herd", 9, &[("radius", "9")]),
        ],
        &[removal(3, "herd", 12)],
    );
    let later = snapshot(
        "r2",
        &[
            (1, &[("name", "stone"), ("use", "old"), ("radius", "3")]),
            (2, &[("name", "cow")]),
            (5, &[("name", "pumpkin")]),
        ],
    );
    vec![
        Input::checkpoint("base", launch, Vec::new(), fixes),
        Input::delta("1999-12-herd", patch),
        Input::checkpoint("2005-02-later", later, acks, merged(&[], &[])),
    ]
}

#[test]
fn a_lineage_folds_each_layer_on_the_one_before_and_a_checkpoint_takes_only_what_its_source_changed(
) {
    let acks = vec![
        ack(1, Some("use"), Take::Keep),
        ack(2, Some("name"), Take::Snapshot),
        ack(4, None, Take::Keep),
    ];
    let f = fold(lineage(acks.clone()));
    assert!(f.is_clean(), "{:?}", f.errors().collect::<Vec<_>>());
    // The base's correction flows past the checkpoint, whose snapshots agree on the field.
    assert_eq!(value(&f, 1, "use"), Some("fixed"));
    // Where the checkpoint's source changed a field, its value takes it.
    assert_eq!(value(&f, 1, "radius"), Some("3"));
    // A drift given to the snapshot takes the snapshot's value.
    assert_eq!(value(&f, 2, "name"), Some("cow"));
    // An item the snapshot makes new comes after the rest; one removed below stays removed.
    assert_eq!(names(&f), ["class 1", "class 2", "class 4", "class 5"]);

    // Each field keeps where it came from.
    let one = f.world.get("class 1").unwrap();
    let radius = &one.fields["radius"];
    assert_eq!((radius.layer, &radius.origin), (2, &Origin::Snapshot));
    assert_eq!(radius.prov.source, Source::Snapshot);
    assert_eq!(radius.prov.revision.as_deref(), Some("r2"));
    let fix = &one.fields["use"];
    assert_eq!(fix.layer, 0);
    assert_eq!(fix.origin, Origin::pack("fixes", &at("fixes/rows.toml", 1)));
    assert_eq!(fix.prov.source, Source::Authored);

    // The world at the delta, folded up to it.
    let mid = resolve(lineage(acks), 1);
    assert_eq!(mid.layers.len(), 2);
    assert_eq!(value(&mid, 2, "name"), Some("bull"));
    assert_eq!(value(&mid, 1, "radius"), Some("9"));
    assert_eq!(names(&mid), ["class 1", "class 2", "class 4"]);

    // The delta's radius never reaches the checkpoint: reported in its own layer.
    let shadowed: Vec<&Finding> = f.layers[1]
        .findings
        .iter()
        .filter(|x| x.severity() == Severity::Report)
        .collect();
    // So does its bull, which the drift gave to the snapshot.
    let shadow = |n: u32, field: &str, line: usize| Finding::Shadowed {
        item: format!("class {n}"),
        field: Some(field.into()),
        at: at("herd/rows.toml", line),
        by: "2005-02-later".into(),
        by_at: None,
    };
    assert_eq!(shadowed, [&shadow(1, "radius", 9), &shadow(2, "name", 1)]);
}

/// A layer's acknowledgement gives the snapshot every drift that layer made: the field it set, the
/// item it made that the snapshot lacks, and the item it took away that the snapshot keeps; an
/// item's own acknowledgement still comes first, a drift another layer made is still a finding,
/// and a layer that made none is a stale acknowledgement.
#[test]
fn a_layers_acknowledgement_gives_the_snapshot_every_drift_that_layer_made() {
    let layer = |id: &str| Ack {
        item: format!("layer {id}"),
        field: None,
        layer: Some(id.to_owned()),
        take: Take::Snapshot,
        note: None,
        at: at("drift.toml", 3),
    };
    let lineage = |acks: Vec<Ack>| {
        let launch = snapshot(
            "r1",
            &[
                (1, &[("name", "stone"), ("use", "old")]),
                (2, &[("name", "cow")]),
                (3, &[("name", "bowl")]),
            ],
        );
        let fixes = merged(&[row(1, "fixes", 1, &[("use", "fixed")])], &[]);
        let herd = merged(
            &[
                row(2, "herd", 1, &[("name", "bull")]),
                row(4, "herd", 5, &[("name", "arrow")]),
            ],
            &[removal(3, "herd", 12)],
        );
        // The later release kept the base's cow, bowl and old use, and made no arrow.
        let later = snapshot(
            "r2",
            &[
                (1, &[("name", "stone"), ("use", "old")]),
                (2, &[("name", "cow")]),
                (3, &[("name", "bowl")]),
            ],
        );
        vec![
            Input::checkpoint("base", launch, Vec::new(), fixes),
            Input::delta("1999-12-herd", herd),
            Input::checkpoint("2005-02-later", later, acks, merged(&[], &[])),
        ]
    };
    // The layer's acknowledgement alone: the base's fix is still drift, nobody's but the base's.
    let f = fold(lineage(vec![layer("1999-12-herd")]));
    let errors: Vec<String> = f.errors().map(|(_, e)| e.to_string()).collect();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("class 1 use"), "{errors:?}");
    // With the item's own acknowledgement first, clean: everything the herd did is the snapshot's.
    let f = fold(lineage(vec![
        ack(1, Some("use"), Take::Keep),
        layer("1999-12-herd"),
    ]));
    assert!(f.is_clean(), "{:?}", f.errors().collect::<Vec<_>>());
    assert_eq!(value(&f, 1, "use"), Some("fixed"));
    assert_eq!(value(&f, 2, "name"), Some("cow"));
    assert_eq!(value(&f, 3, "name"), Some("bowl"));
    assert_eq!(names(&f), ["class 1", "class 2", "class 3"]);
    // A layer that made no drift: its acknowledgement is stale.
    let f = fold(lineage(vec![
        ack(1, Some("use"), Take::Keep),
        layer("1999-12-herd"),
        layer("base"),
    ]));
    let stale: Vec<&Finding> = f.layers[2]
        .findings
        .iter()
        .filter(|x| matches!(x, Finding::StaleAck { .. }))
        .collect();
    assert_eq!(stale.len(), 1, "{stale:?}");
}

/// The values of a folded world, item by item and field by field.
fn values(f: &Folded<String>) -> Vec<(String, Vec<(String, String)>)> {
    f.world
        .iter()
        .map(|e| {
            let fields = e
                .fields
                .iter()
                .map(|(k, v)| (k.to_string(), v.value.clone()))
                .collect();
            (e.item.name.clone(), fields)
        })
        .collect()
}

#[test]
fn a_checkpoint_over_a_world_nothing_has_changed_folds_as_one_over_a_changed_world_does() {
    let launch = || {
        snapshot(
            "r1",
            &[
                (1, &[("name", "stone"), ("use", "old")]),
                (2, &[("name", "cow")]),
                (3, &[("name", "bowl")]),
            ],
        )
    };
    let later = || {
        snapshot(
            "r2",
            &[
                (1, &[("name", "stone"), ("radius", "3")]),
                (4, &[("name", "arrow")]),
                (2, &[("name", "cow")]),
            ],
        )
    };
    let stale = vec![ack(4, None, Take::Keep)];
    // Nothing changes the world between the two snapshots.
    let quiet = fold([
        Input::checkpoint("base", launch(), Vec::new(), merged(&[], &[])),
        Input::delta("2000-01-quiet", merged(&[], &[])),
        Input::checkpoint("2005-02-later", later(), stale.clone(), merged(&[], &[])),
    ]);
    // A patch sets a field to what it already is: the world is the same, but it has been changed.
    let touched = fold([
        Input::checkpoint("base", launch(), Vec::new(), merged(&[], &[])),
        Input::delta(
            "2000-01-quiet",
            merged(&[row(2, "same", 1, &[("name", "cow")])], &[]),
        ),
        Input::checkpoint("2005-02-later", later(), stale, merged(&[], &[])),
    ]);
    assert_eq!(values(&quiet), values(&touched));
    assert_eq!(
        values(&quiet),
        [
            (
                "class 1".to_owned(),
                vec![
                    ("name".into(), "stone".into()),
                    ("radius".into(), "3".into())
                ]
            ),
            ("class 2".to_owned(), vec![("name".into(), "cow".into())]),
            ("class 4".to_owned(), vec![("name".into(), "arrow".into())]),
        ]
    );
    // An unchanged field keeps the layer that set it; a changed one is the checkpoint's.
    for f in [&quiet, &touched] {
        let one = f.world.get("class 1").unwrap();
        assert_eq!(one.fields["name"].layer, 0);
        assert_eq!(one.fields["radius"].layer, 2);
        assert_eq!(
            f.layers[2].findings,
            [Finding::StaleAck {
                item: "class 4".into(),
                field: None,
                at: at("drift.toml", 1),
            }]
        );
    }
    // The last checkpoint's snapshot let go changes nothing folded.
    let last = fold([
        Input::checkpoint("base", launch(), Vec::new(), merged(&[], &[])),
        Input::checkpoint("2005-02-later", later(), Vec::new(), merged(&[], &[])).last_checkpoint(),
    ]);
    assert_eq!(values(&last), values(&quiet));
}

#[test]
fn every_disagreement_with_a_checkpoint_s_snapshot_is_drift_until_acknowledged() {
    let f = fold(lineage(vec![ack(5, None, Take::Keep)]));
    let mut drift: Vec<String> = f.layers[2]
        .findings
        .iter()
        .map(|x| match x {
            Finding::Drift(d) => {
                format!("{} {:?} {:?} {:?}", d.item, d.field, d.folded, d.snapshot)
            }
            Finding::StaleAck { item, .. } => format!("stale {item}"),
            other => panic!("{other}"),
        })
        .collect();
    drift.sort();
    assert_eq!(
        drift,
        [
            "class 1 Some(\"use\") Some(\"\\\"fixed\\\"\") Some(\"\\\"old\\\"\")",
            "class 2 Some(\"name\") Some(\"\\\"bull\\\"\") Some(\"\\\"cow\\\"\")",
            "class 4 None Some(\"it\") None",
            "stale class 5",
        ]
    );
    assert_eq!(f.errors().count(), 3);
    assert!(f.layers[2]
        .findings
        .iter()
        .any(|x| matches!(x, Finding::StaleAck { .. }) && x.severity() == Severity::Warning));
    // Unacknowledged, the folded world stands.
    assert_eq!(value(&f, 1, "use"), Some("fixed"));
    assert_eq!(value(&f, 2, "name"), Some("bull"));
    // A whole item given to the snapshot is taken away where the snapshot has none of it.
    let f = fold(lineage(vec![
        ack(1, None, Take::Snapshot),
        ack(2, Some("name"), Take::Keep),
        ack(4, None, Take::Snapshot),
    ]));
    assert!(f.is_clean());
    assert_eq!(value(&f, 1, "use"), Some("old"));
    assert_eq!(names(&f), ["class 1", "class 2", "class 5"]);
}

#[test]
fn a_layer_s_packs_merge_field_by_field_and_conflict_only_on_one_field_set_differently() {
    let rows = [
        row(1, "a", 1, &[("name", "x")]),
        row(1, "b", 1, &[("use", "y")]),
        row(2, "a", 4, &[("name", "p")]),
        row(2, "b", 4, &[("name", "q")]),
        row(1, "a", 8, &[("radius", "5")]),
        row(3, "a", 10, &[("name", "same")]),
        row(3, "b", 10, &[("name", "same")]),
    ];
    let m = merge_layer(&rows, &[], &[]);
    let fields: Vec<&String> = m.change.edits[0].set.keys().collect();
    assert_eq!(fields, ["name", "use"], "each pack gives its own field");
    let kinds: Vec<String> = m
        .findings
        .iter()
        .map(|f| match f {
            Finding::Conflict(c) => format!("conflict {}", c.what),
            Finding::SetTwice { item, pack, .. } => format!("twice {item} {pack}"),
            other => panic!("{other}"),
        })
        .collect();
    assert_eq!(kinds, ["twice class 1 a", "conflict class 2 name"]);
    // The layer's override settles it, naming the item.
    let over = Override {
        pack: "b".into(),
        over: "a".into(),
        items: Some(vec![Scope::parse::<Toy>("class 2").unwrap()]),
        at: at("patch.toml", 3),
    };
    let m = merge_layer(&rows[2..4], &[], std::slice::from_ref(&over));
    assert!(m.findings.is_empty());
    assert_eq!(m.overridden[0].what, "class 2 name");
    assert_eq!(m.change.edits[0].set["name"].value, "q");
    // Unsetting a field another pack sets is a conflict too.
    let m = merge_layer(
        &[
            row(1, "a", 1, &[("name", "x")]),
            row(1, "b", 2, &[]).unset("name"),
        ],
        &[],
        &[],
    );
    assert!(matches!(&m.findings[..], [Finding::Conflict(c)] if c.what == "class 1 name"));
}

#[test]
fn removals_take_items_away_last_and_a_stale_one_or_a_change_to_nothing_is_found() {
    let base = Input::delta(
        "base",
        merged(
            &[
                row(1, "a", 1, &[("name", "x")]),
                row(2, "a", 2, &[("name", "y")]),
                row(4, "a", 3, &[("name", "w")]),
            ],
            &[],
        ),
    );
    let patch = Input::delta(
        "2000-01-p",
        merged(
            &[
                row(1, "b", 1, &[("name", "x")]),
                row(1, "b", 2, &[]).unset("use"),
                row(3, "b", 3, &[("name", "z")]),
            ],
            &[removal(2, "b", 7), removal(3, "b", 8), removal(9, "b", 9)],
        ),
    );
    let readd = Input::delta(
        "2000-02-q",
        merged(&[row(2, "c", 1, &[("name", "back")])], &[]),
    );
    let f = fold([base, patch, readd]);
    let found: Vec<String> = f.layers[1]
        .findings
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(
        found,
        [
            format!(
                "{}: class 1 is set twice in pack b (also {})",
                at("b/rows.toml", 2),
                at("b/rows.toml", 1)
            ),
            format!(
                "{}: class 1 name: changes nothing below",
                at("b/rows.toml", 1)
            ),
            format!(
                "{}: remove: nothing below or in this layer sets class 9",
                at("b/rows.toml", 9)
            ),
        ]
    );
    assert_eq!(
        f.errors().count(),
        2,
        "the pack's twice and the stale removal"
    );
    // An item set again after its removal comes after the rest; one set and removed in one layer
    // is gone.
    assert_eq!(names(&f), ["class 1", "class 4", "class 2"]);
    assert_eq!(value(&f, 2, "name"), Some("back"));
    // The base's edit of the removed item is reported as never reaching the end.
    assert!(f.layers[0].findings.iter().any(|x| matches!(
        x,
        Finding::Shadowed { item, field: None, by, .. } if item == "class 2" && by == "2000-01-p"
    )));
    // Unsetting a field that is not set changes nothing.
    let f = fold([
        Input::delta("base", merged(&[row(1, "a", 1, &[("name", "x")])], &[])),
        Input::delta(
            "2000-01-p",
            merged(&[row(1, "b", 2, &[]).unset("use")], &[]),
        ),
    ]);
    assert_eq!(
        f.layers[1].findings,
        [Finding::NoOp {
            item: "class 1".into(),
            field: "use".into(),
            at: at("b/rows.toml", 2),
        }]
    );
}

#[test]
fn a_row_converted_from_a_snapshot_names_its_source_and_revision() {
    let p: Provenance = toml::from_str::<Holder>(
        "prov = { from = \"source\", source = \"ACE-World-16PY\", revision = \"v0.8.8\" }",
    )
    .unwrap()
    .prov;
    assert_eq!((p.source, p.source.letter()), (Source::Snapshot, 'S'));
    assert_eq!(p.to_string(), "source: ACE-World-16PY v0.8.8");
    let e = toml::from_str::<Holder>("prov = { from = \"source\", source = \"x\" }")
        .err()
        .unwrap()
        .to_string();
    assert!(e.contains("from source names its revision"), "{e}");
}

#[test]
fn fields_that_say_the_same_provenance_share_one_copy_of_it() {
    let s = snapshot(
        "r1",
        &[
            (1, &[("name", "stone"), ("use", "old")]),
            (2, &[("name", "cow")]),
        ],
    );
    let one = s.get("class 1").unwrap();
    let two = s.get("class 2").unwrap();
    assert!(std::sync::Arc::ptr_eq(
        &one.fields["name"].prov,
        &one.fields["use"].prov
    ));
    assert!(std::sync::Arc::ptr_eq(
        &one.fields["name"].prov,
        &two.fields["name"].prov
    ));
    // A field whose own provenance differs from its row's keeps its own.
    let mut s = Snapshot::new();
    let row: Provenance = toml::from_str::<Holder>(
        "prov = { from = \"dat\", record = 1, fields = { use = { from = \"authored\" } } }",
    )
    .unwrap()
    .prov;
    s.push(
        class(3),
        &row,
        [
            ("name".to_owned(), "a".to_owned()),
            ("use".to_owned(), "b".to_owned()),
        ],
    )
    .unwrap();
    let three = s.get("class 3").unwrap();
    assert_eq!(three.fields["name"].prov.source, Source::Dat);
    assert!(three.fields["name"].prov.fields.is_empty());
    assert_eq!(three.fields["use"].prov.source, Source::Authored);
    let m = merge_layer(
        &[
            row_with(1, "a", 1, &row, &[("name", "x")]),
            row_with(2, "a", 2, &row, &[("name", "y")]),
        ],
        &[],
        &[],
    );
    assert!(std::sync::Arc::ptr_eq(
        &m.change.edits[0].set["name"].prov,
        &m.change.edits[1].set["name"].prov
    ));
}

/// A row of `pack` from `prov` setting `fields` of class `n`.
fn row_with(
    n: u32,
    pack: &str,
    line: usize,
    prov: &Provenance,
    fields: &[(&str, &str)],
) -> Row<String> {
    let mut r = row(n, pack, line, fields);
    r.prov = prov.clone();
    r
}

/// A dialect of the test's game whose worlds have no layers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Flat;

impl Dialect for Flat {
    const GAME: &'static str = "toy";
    const PRODUCT: &'static str = "Flatbox";
    const BUILD_COMMAND: &'static str = "flatbox build";
    const ITEM_FORMS: &'static str = "class <n>";
    type Kind = ToyKind;
    type Era = ToyEra;

    fn era(name: Option<&str>) -> Result<ToyEra, String> {
        Toy::era(name)
    }

    fn item(text: &str, words: &[&str]) -> Option<String> {
        Toy::item(text, words)
    }
}

fn w(rel: &str) -> PathBuf {
    rel.split('/')
        .fold(PathBuf::from("w"), |p, part| p.join(part))
}

/// A world of two patches, the second a checkpoint, in memory.
fn store() -> MemoryStore {
    let s = MemoryStore::default();
    let put = |rel: &str, text: &str| s.write(&w(rel), text.as_bytes()).unwrap();
    put(
        "world.toml",
        "key = \"w\"\ngame = \"toy\"\npacks = [\"seed\"]\nlineage = [\"1999-12-thaw\", \"2005-02-later\"]\n",
    );
    put(
        "patches/1999-12-thaw/patch.toml",
        "id = \"1999-12-thaw\"\ntitle = \"Thaw\"\ndate = \"1999-12-20\"\nafter = \"\"\npacks = [\"fixes\", \"herd\"]\nportal_iteration = 112\ncell_iteration = 70\n\n[[override]]\npack = \"herd\"\nover = \"fixes\"\nitems = [\"class 4\"]\n",
    );
    put(
        "patches/2005-02-later/patch.toml",
        "id = \"2005-02-later\"\ntitle = \"Later\"\ndate = \"2005-02-01\"\nafter = \"1999-12-thaw\"\n",
    );
    put(
        "patches/2005-02-later/checkpoint.toml",
        "source = \"toy-world\"\nrevision = \"v0.8.8\"\nsha256 = \"ab\"\nkeys = \"keys.toml\"\n",
    );
    put(
        "patches/2005-02-later/drift.toml",
        "[[drift]]\nitem = \"class 1\"\nfield = \"use\"\ntake = \"keep\"\nnote = \"ours\"\n\n[[drift]]\nitem = \"class 4\"\ntake = \"snapshot\"\n",
    );
    s
}

fn read(s: &MemoryStore) -> Result<Lineage, Error> {
    Lineage::read_world_file::<Toy>(s, &w("world.toml"))
}

#[test]
fn a_world_lists_its_patches_in_order_each_saying_what_it_is_and_a_checkpoint_its_pin_and_drift() {
    let s = store();
    let l = read(&s).unwrap();
    let ids: Vec<&str> = l.layers.iter().map(|l| l.id.as_str()).collect();
    assert_eq!(ids, ["base", "1999-12-thaw", "2005-02-later"]);
    assert_eq!(l.layers[0].packs, [w("seed")]);
    let thaw = &l.layers[1];
    assert_eq!(
        thaw.packs,
        [
            w("patches/1999-12-thaw/packs/fixes"),
            w("patches/1999-12-thaw/packs/herd")
        ]
    );
    let p = thaw.patch.as_ref().unwrap();
    assert_eq!(
        (p.title.as_str(), p.date.as_str(), p.after.as_str()),
        ("Thaw", "1999-12-20", "")
    );
    assert_eq!(
        p.iterations,
        BTreeMap::from([("cell".into(), 70), ("portal".into(), 112)])
    );
    assert_eq!(thaw.overrides.len(), 1);
    assert!(!thaw.is_checkpoint());
    // A patch whose status says nothing isn't written; a checkpoint always is.
    assert!(!p.written);
    let later = &l.layers[2];
    assert!(later.patch.as_ref().unwrap().written);
    let pin = later.checkpoint.as_ref().unwrap();
    assert_eq!(
        (pin.source.as_str(), pin.revision.as_str()),
        ("toy-world", "v0.8.8")
    );
    assert_eq!(
        pin.rest.get("keys").and_then(|v| v.as_str()),
        Some("keys.toml")
    );
    assert_eq!(
        later
            .drift
            .iter()
            .map(|a| (a.item.as_str(), a.field.as_deref(), a.take))
            .collect::<Vec<_>>(),
        [
            ("class 1", Some("use"), Take::Keep),
            ("class 4", None, Take::Snapshot)
        ]
    );
    assert_eq!(l.position("2005-02-later"), Some(2));

    // The same through the world file as the game's world reads it.
    let world = World::<Toy>::read_from(&s, &w("world.toml")).unwrap();
    assert_eq!(world.lineage, ["1999-12-thaw", "2005-02-later"]);
    assert_eq!(Lineage::of_world_in(&s, &world).unwrap(), l);
    // A game whose worlds have no layers refuses one that lists some.
    let e = World::<Flat>::read_from(&s, &w("world.toml"))
        .unwrap_err()
        .to_string();
    assert!(e.contains("a Flatbox world lists no patch layers"), "{e}");
}

#[test]
fn a_patch_is_written_when_its_status_says_so_and_unwritten_otherwise() {
    let status = |line: &str| {
        let s = store();
        s.write(
            &w("patches/1999-12-thaw/patch.toml"),
            format!(
                "id = \"1999-12-thaw\"
title = \"T\"
date = \"1999-12-20\"
{line}"
            )
            .as_bytes(),
        )
        .unwrap();
        read(&s).map(|l| l.layers[1].patch.as_ref().unwrap().written)
    };
    assert!(!status("").unwrap());
    assert!(!status(
        "status = \"unwritten\"
"
    )
    .unwrap());
    assert!(status(
        "status = \"written\"
"
    )
    .unwrap());
    let e = status(
        "status = \"done\"
",
    )
    .unwrap_err()
    .to_string();
    assert!(e.contains("a patch is \"written\" or \"unwritten\""), "{e}");
}

#[test]
fn a_patch_is_named_by_its_month_comes_after_the_one_before_it_and_is_dated_in_its_month() {
    let bad = |rel: &str, text: &str, why: &str| {
        let s = store();
        s.write(&w(rel), text.as_bytes()).unwrap();
        let e = read(&s).unwrap_err().to_string();
        assert!(e.contains(why), "{rel}: {e}");
    };
    let thaw = "patches/1999-12-thaw/patch.toml";
    bad(
        "world.toml",
        "key = \"w\"\ngame = \"toy\"\nlineage = [\"0001-launch\"]\n",
        "is not a patch: <yyyy-mm>-<name>",
    );
    bad(
        "world.toml",
        "key = \"w\"\ngame = \"toy\"\nlineage = [\"1999-12-thaw\", \"1999-12-thaw\"]\n",
        "listed twice",
    );
    bad(
        thaw,
        "id = \"1999-12-other\"\ntitle = \"T\"\ndate = \"1999-12-20\"\n",
        "but the patch's folder is 1999-12-thaw",
    );
    bad(
        thaw,
        "id = \"1999-12-thaw\"\ntitle = \"T\"\ndate = \"1999-12-20\"\nafter = \"x\"\n",
        "the layer before it in the lineage is the base",
    );
    bad(
        thaw,
        "id = \"1999-12-thaw\"\ntitle = \"T\"\ndate = \"2000-01-02\"\n",
        "is not in 1999-12",
    );
    bad(
        thaw,
        "id = \"1999-12-thaw\"\ntitle = \"T\"\ndate = \"1999-12-20\"\nnotes = 1\n",
        "unknown field `notes`",
    );
    bad(
        thaw,
        "id = \"1999-12-thaw\"\ntitle = \"T\"\ndate = \"1999-12-20\"\nportal_iteration = \"x\"\n",
        "an iteration",
    );
    bad(
        "patches/1999-12-thaw/drift.toml",
        "",
        "drift is acknowledged only at a checkpoint",
    );
    bad(
        "patches/2005-02-later/drift.toml",
        "[[drift]]\nitem = \"class 1\"\ntake = \"keep\"\n[[drift]]\nitem = \"class 1\"\ntake = \"snapshot\"\n",
        "acknowledged already",
    );
    bad(
        "patches/2005-02-later/drift.toml",
        "[[drift]]\nitem = \"thing 1\"\ntake = \"keep\"\n",
        "is not an item",
    );
    // A layer's acknowledgement names a layer before the checkpoint, and no item or field.
    bad(
        "patches/2005-02-later/drift.toml",
        "[[drift]]\nlayer = \"2005-02-later\"\ntake = \"snapshot\"\n",
        "is no layer before this checkpoint",
    );
    bad(
        "patches/2005-02-later/drift.toml",
        "[[drift]]\nlayer = \"1999-12-thaw\"\nfield = \"use\"\ntake = \"snapshot\"\n",
        "names no field",
    );
    bad(
        "patches/2005-02-later/drift.toml",
        "[[drift]]\nitem = \"class 1\"\nlayer = \"1999-12-thaw\"\ntake = \"snapshot\"\n",
        "an item or a layer",
    );
    bad(
        "patches/2005-02-later/checkpoint.toml",
        "source = \"x\"\nrevision = \"\"\n",
        "names its snapshot's revision",
    );
    bad(
        "patches/2005-02-later/patch.toml",
        "id = \"2005-02-later\"\ntitle = \"L\"\ndate = \"2005-02-01\"\nafter = \"1999-12-thaw\"\n[[override]]\npack = \"a\"\nover = \"a\"\n",
        "cannot win over itself",
    );
}

/// The test's rows: `[[class]]` by its `id`, its `int` a map.
struct Classes;

impl RowKinds for Classes {
    fn item(&self, kind: &str, row: &toml::Table) -> Result<Item, String> {
        match (kind, row.get("id").and_then(toml::Value::as_integer)) {
            ("class", Some(n)) => Ok(Item::new(format!("class {n}"))),
            _ => Err(format!("a [[{kind}]] row names no class")),
        }
    }

    fn is_map(&self, kind: &str, field: &str) -> bool {
        kind == "class" && field == "int"
    }
}

#[test]
fn rows_read_from_tables_merge_a_map_by_its_keys_and_come_back_as_tables() {
    let base_text = "[[class]]\nid = 1\nname = \"stone\"\nint = { a = 1, b = 2 }\nlist = [1, 2]\n\n[[class]]\nid = 7\nname = \"gone\"\n";
    let (base, problems) =
        read_pack_file(&Classes, "seed", Path::new("seed/classes.toml"), base_text);
    assert!(problems.is_empty(), "{problems:?}");
    let patch_text = "[[class]]\nid = 1\nint = { b = 3 }\nlist = [4]\nremove_fields = [\"int.a\"]\nprov = { from = \"dat\", record = 5 }\n\n[[remove]]\nitems = [\"class 7\"]\n";
    let (patch, problems) = read_pack_file(
        &Classes,
        "fixes",
        Path::new("fixes/classes.toml"),
        patch_text,
    );
    assert!(problems.is_empty(), "{problems:?}");
    assert_eq!(patch.rows[0].at, at("fixes/classes.toml", 1));
    assert_eq!(patch.removals[0].item, "class 7");
    let mut kinds = base.kinds.clone();
    kinds.extend(patch.kinds.clone());
    let f = fold([
        Input::delta("base", merge_layer(&base.rows, &base.removals, &[])),
        Input::delta(
            "2000-01-fixes",
            merge_layer(&patch.rows, &patch.removals, &[]),
        ),
    ]);
    assert!(f.is_clean(), "{:?}", f.errors().collect::<Vec<_>>());
    let t = tables(&Classes, &f.world, &kinds);
    assert_eq!(t.keys().collect::<Vec<_>>(), ["class 1"]);
    let one = &t["class 1"];
    assert_eq!(one.kind, "class");
    let expect: toml::Table = "id = 1\nname = \"stone\"\nint = { b = 3 }\nlist = [4]\n"
        .parse()
        .unwrap();
    assert_eq!(one.table, expect);
    assert_eq!(one.prov["int.b"].source, Source::Dat);
    assert_eq!(one.prov["name"].source, Source::Authored);

    // What does not read is a problem, and its row is left out.
    let (bad, problems) = read_pack_file(
        &Classes,
        "x",
        Path::new("x/c.toml"),
        "[[class]]\nname = \"no id\"\n\n[[class]]\nid = 2\nremove_fields = \"int.a\"\n\n[[remove]]\nitems = [\"class 1\"]\nwhy = 1\n",
    );
    assert!(bad.rows.is_empty() && bad.removals.is_empty());
    let problems: Vec<String> = problems.iter().map(ToString::to_string).collect();
    assert_eq!(problems.len(), 3, "{problems:?}");
    assert!(problems[0].ends_with("a [[class]] row names no class"));
    assert!(problems[1].ends_with("remove_fields: a list of names"));
    assert!(problems[2].ends_with("unknown field `why` of a [[remove]]"));
}

#[test]
fn a_row_that_says_nothing_of_its_source_comes_from_where_its_packs_header_says() {
    let header = PackHeader::parse::<Toy>(
        Path::new("base/pack.toml"),
        "name = \"base\"\nversion = \"1\"\ngame = \"toy\"\nprov = { from = \"source\", source = \"toy-world\", revision = \"r1\" }\n",
    )
    .unwrap();
    let default = header.prov.expect("the header's source");
    let text = "[[class]]\nid = 1\nname = \"stone\"\n\n[[class]]\nid = 2\nname = \"fixed\"\nprov = { from = \"authored\" }\n";
    let (file, problems) = read_pack_file_from(
        &Classes,
        "base",
        Path::new("base/classes.toml"),
        text,
        &default,
    );
    assert!(problems.is_empty(), "{problems:?}");
    assert_eq!(file.rows[0].prov.source, Source::Snapshot);
    assert_eq!(file.rows[1].prov.source, Source::Authored);
    // Without the header's word, a person.
    let (file, _) = read_pack_file(&Classes, "base", Path::new("base/classes.toml"), text);
    assert_eq!(file.rows[0].prov.source, Source::Authored);
    // A snapshot's rows take it too.
    let mut snap = Snapshot::new();
    let mut kinds = BTreeMap::new();
    let row: toml::Table = "id = 1\nname = \"stone\"\n".parse().unwrap();
    snapshot_row(
        &Classes,
        "class",
        &row,
        &at("base/classes.toml", 1),
        &default,
        &mut snap,
        &mut kinds,
    )
    .unwrap();
    assert_eq!(
        snap.get("class 1").unwrap().fields["name"].prov.source,
        Source::Snapshot
    );
}

#[test]
fn a_snapshot_refuses_an_item_given_twice_and_keeps_the_first() {
    let mut snap = Snapshot::new();
    let mut kinds = BTreeMap::new();
    let first: toml::Table = "id = 1\nname = \"stone\"\n".parse().unwrap();
    let again: toml::Table = "id = 1\nname = \"other\"\n".parse().unwrap();
    let here = at("base/classes.toml", 4);
    snapshot_row(
        &Classes,
        "class",
        &first,
        &here,
        &Provenance::AUTHORED,
        &mut snap,
        &mut kinds,
    )
    .unwrap();
    let e = snapshot_row(
        &Classes,
        "class",
        &again,
        &here,
        &Provenance::AUTHORED,
        &mut snap,
        &mut kinds,
    )
    .unwrap_err()
    .to_string();
    assert!(e.ends_with("class 1 is given twice"), "{e}");
    assert_eq!(snap.len(), 1);
    assert_eq!(
        snap.get("class 1").unwrap().fields["name"].value,
        toml::Value::from("stone")
    );
}
