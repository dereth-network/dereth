//! Behaviour: none (tooling: a world repository read and folded by the vocabulary the server gives)

use std::path::{Path, PathBuf};

use lyceum_foundry::repo::{Repo, RowSchema};

/// A vocabulary of two kinds, as `empyrean-import schema` gives it: weenies, whose `int` folds key
/// by key, and quests by name.
fn schema() -> RowSchema {
    let doc = serde_json::json!({
        "properties": {},
        "enums": {},
        "rows": {
            "weenie": {
                "file": "weenie/{class:05}.toml",
                "item": "weenie {class}",
                "key": ["class"],
                "fields": { "class": "int", "class_name": "text" },
                "maps": { "int": { "key": { "property": "int" }, "value": "int" } },
            },
            "quest": {
                "file": "quest/{name}.toml",
                "item": "quest {name:lower}",
                "key": ["name"],
                "fields": { "name": "text", "max_solves": "int" },
            },
        },
    });
    RowSchema::of_schema(&doc)
        .expect("reads")
        .expect("has rows")
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// A repository: a base of two packs (rows at their roots, beside a world builder's own folders),
/// a patch that sets a field and takes a quest away, and a checkpoint after it.
fn repo(dir: &Path) -> PathBuf {
    write(
        &dir.join("base/library/pack.toml"),
        "name = \"library\"\nversion = \"1\"\nprov = { from = \"source\", source = \"ACE-World-Database\", revision = \"v0.9.296\" }\n",
    );
    write(
        &dir.join("base/library/weenie/00509.toml"),
        "[[weenie]]\nclass = 509\nclass_name = \"drudge\"\nint = { ItemType = 16, Value = 5 }\n",
    );
    write(
        &dir.join("base/library/quest/drudgekills.toml"),
        "[[quest]]\nname = \"DrudgeKills\"\nmax_solves = 3\n",
    );
    write(&dir.join("base/library/world-db.toml"), "format = 1\n");
    write(
        &dir.join("base/library/records/rules.toml"),
        "[[skill]]\nskill = \"Axe\"\n",
    );
    write(
        &dir.join("base/emergence/pack.toml"),
        "name = \"emergence\"\nversion = \"1\"\n",
    );
    write(
        &dir.join("base/emergence/weenie/00509.toml"),
        "[[weenie]]\nclass = 509\nint = { Value = 25 }\n",
    );
    write(
        &dir.join("base/emergence/edits/landblocks/A9B4.toml"),
        "x = 1\n",
    );
    let patch = dir.join("patches/2027-01-first");
    write(
        &patch.join("patch.toml"),
        "id = \"2027-01-first\"\ntitle = \"First\"\ndate = \"2027-01-01\"\nafter = \"\"\npacks = [\"fixes\"]\n",
    );
    write(
        &patch.join("packs/fixes/pack.toml"),
        "name = \"fixes\"\nversion = \"1\"\n",
    );
    write(
        &patch.join("packs/fixes/rows.toml"),
        "[[weenie]]\nclass = 509\nint = { ItemType = 1 }\nprov = { from = \"authored\", by = \"an-author\" }\n\n[[remove]]\nitem = \"quest  DRUDGEKILLS\"\n",
    );
    let checkpoint = dir.join("patches/2027-02-pinned");
    write(
        &checkpoint.join("patch.toml"),
        "id = \"2027-02-pinned\"\ntitle = \"Pinned\"\ndate = \"2027-02-01\"\nafter = \"2027-01-first\"\npacks = []\n",
    );
    write(
        &checkpoint.join("checkpoint.toml"),
        "source = \"elsewhere\"\nrevision = \"v1\"\nurl = \"https://example.org/w.zip\"\nsha256 = \"00\"\n",
    );
    let world = dir.join("world.toml");
    write(
        &world,
        "format_version = 1\ngame = \"ac1\"\nera = \"newdereth\"\ntitle = \"A test world\"\npacks = [\"base/library\", \"base/emergence\"]\nlineage = [\"2027-01-first\", \"2027-02-pinned\"]\n[ids]\nclasses = [2000000, 2999999]\n",
    );
    world
}

fn folder(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lyceum-repo-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// A repository's world is shown folded at each layer: the base's packs in order (a later pack's
/// field over an earlier's, each field with its pack's provenance), then a patch's changes over it,
/// each field naming the layer that set it and where; a patch's removal, written another way,
/// takes its item away. The world builder's own folders hold no rows; a row's key, which every row
/// of its item names, is no finding. The world after a checkpoint isn't shown.
#[test]
fn a_repository_is_folded_at_each_layer_with_each_fields_layer_and_provenance() {
    let dir = folder("fold");
    let repo = Repo::open(&repo(&dir)).expect("opens");
    assert_eq!(repo.ids(), ["base", "2027-01-first", "2027-02-pinned"]);
    assert_eq!(repo.era, lyceum_foundry::world::EraId::NewDereth);
    assert_eq!(repo.title.as_deref(), Some("A test world"));
    assert_eq!(repo.shown_layers(), 2);
    let schema = schema();

    let base = repo.fold(&schema, 0).expect("the base");
    let w = &base.items["weenie 509"];
    let field = |name: &str| w.fields.iter().find(|f| f.name == name).expect(name);
    assert_eq!(
        field("int.Value").value,
        toml::Value::Integer(25),
        "emergence over the library"
    );
    assert_eq!(field("int.ItemType").value, toml::Value::Integer(16));
    assert_eq!(
        field("class_name").prov.source,
        dereth_pack::provenance::Source::Snapshot
    );
    assert!(field("int.Value").prov.is_plain_authored());
    assert!(base.items.contains_key("quest drudgekills"));
    assert_eq!(base.items.len(), 2, "no rows from records/ or edits/");
    // Emergence's row names the library's weenie by its class again: no change, and no finding.
    assert_eq!(base.findings, Vec::<String>::new());

    let first = repo.fold(&schema, 1).expect("the patch");
    let w = &first.items["weenie 509"];
    let field = |name: &str| w.fields.iter().find(|f| f.name == name).expect(name);
    assert_eq!(field("int.ItemType").value, toml::Value::Integer(1));
    assert_eq!(field("int.ItemType").layer, "2027-01-first");
    assert!(field("int.ItemType")
        .at
        .as_ref()
        .is_some_and(|at| at.to_string().contains("rows.toml")));
    assert_eq!(field("int.ItemType").prov.by.as_deref(), Some("an-author"));
    assert_eq!(field("int.Value").layer, "base");
    assert!(!first.items.contains_key("quest drudgekills"), "taken away");

    let e = repo.fold(&schema, 2).expect_err("a checkpoint");
    assert!(
        e.contains("2027-02-pinned") && e.contains("pinned release"),
        "{e}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A row of a kind the vocabulary doesn't know, or that names no item, is refused with its file.
#[test]
fn a_row_the_vocabulary_doesnt_know_is_refused() {
    let dir = folder("unknown");
    let world = repo(&dir);
    write(
        &dir.join("patches/2027-01-first/packs/fixes/more.toml"),
        "[[recipe]]\nid = 3\n",
    );
    let e = Repo::open(&world)
        .unwrap()
        .fold(&schema(), 1)
        .expect_err("refused");
    assert!(e.contains("more.toml") && e.contains("recipe"), "{e}");
    let _ = std::fs::remove_dir_all(&dir);
}
