//! Behaviour: none (tooling: an item's row written into the layer being edited, read back by the fold)

use std::path::{Path, PathBuf};

use lyceum_foundry::layer_rows::{delta, file_of, over, row_in, write_removal, write_row};
use lyceum_foundry::repo::{Repo, RowSchema};

/// Weenies with maps of numbers, names and data ids (written in hexadecimal) and a create list.
fn schema() -> RowSchema {
    RowSchema::of_schema(&serde_json::json!({
        "rows": {
            "weenie": {
                "file": "weenie/{class:05}.toml",
                "item": "weenie {class}",
                "key": ["class"],
                "fields": { "class": "int", "class_name": "text" },
                "maps": {
                    "int": { "key": { "property": "int" }, "value": "int" },
                    "string": { "key": { "property": "string" }, "value": "text" },
                    "did": { "key": { "property": "did" }, "value": "hex" },
                },
                "lists": {
                    "create_list": { "fields": { "weenie_class_id": "int", "stack_size": "int" } },
                },
            },
        },
    }))
    .expect("reads")
    .expect("has rows")
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// A repository of a base, with a drudge, and an empty patch after it.
fn repo(tag: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!("lyceum-layer-rows-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    write(
        &dir.join("base/library/pack.toml"),
        "name = \"library\"\nversion = \"1\"\n",
    );
    write(
        &dir.join("base/library/weenie/00509.toml"),
        "[[weenie]]\nclass = 509\nclass_name = \"drudge\"\n\n[weenie.int]\nItemType = 16\nLevel = 5\n\n\
         [weenie.string]\nName = \"Drudge\"\n\n[[weenie.create_list]]\nweenie_class_id = 273\nstack_size = 1\n",
    );
    let patch = dir.join("patches/2027-01-first");
    write(
        &patch.join("patch.toml"),
        "id = \"2027-01-first\"\ntitle = \"First\"\ndate = \"2027-01-01\"\nafter = \"\"\npacks = [\"main\"]\n",
    );
    write(
        &patch.join("packs/main/pack.toml"),
        "name = \"main\"\nversion = \"1\"\n",
    );
    let world = dir.join("world.toml");
    write(
        &world,
        "format_version = 1\ngame = \"ac1\"\npacks = [\"base/library\"]\nlineage = [\"2027-01-first\"]\n",
    );
    (dir, world)
}

/// The weenie as the world holds it at layer `at`, as one row.
fn weenie_at(world: &Path, at: usize) -> Option<toml::Table> {
    let schema = schema();
    let repo = Repo::open(world).expect("opens");
    let w = repo.fold(&schema, at).expect("folds");
    w.items.get("weenie 509").map(|i| i.row(&schema))
}

/// A change made in a patch is written as the one row the patch needs: the item's key, the fields
/// that differ from the base, and in `remove_fields` those it takes away; folded, the world at the
/// patch holds the item as it was wanted. A field set back to the base's value is no longer the
/// patch's (the rest of the file, its comment among it, kept), and nothing changed takes the row
/// (and the file, left empty) away. A data id is written in hexadecimal.
#[test]
fn an_edit_is_written_as_the_layers_row_and_folds_into_the_item_wanted() {
    let (dir, world) = repo("edit");
    let schema = schema();
    let below = weenie_at(&world, 0).expect("the base's drudge");
    let mut wanted = below.clone();
    let t = |s: &str| -> toml::Table { toml::from_str(s).unwrap() };
    wanted.insert("int".into(), t("ItemType = 16\nLevel = 6").into());
    wanted.insert("did".into(), t("Setup = 0x02000001").into());
    wanted.remove("string");
    wanted.insert(
        "create_list".into(),
        t("x = [{ weenie_class_id = 273, stack_size = 5 }]")["x"].clone(),
    );
    let row = delta(&schema, "weenie", Some(&below), &wanted).expect("a change");
    assert_eq!(
        row,
        t(
            "class = 509\ncreate_list = [{ weenie_class_id = 273, stack_size = 5 }]\n\
           remove_fields = [\"string.Name\"]\n[did]\nSetup = 0x02000001\n[int]\nLevel = 6"
        ),
    );
    let file = dir
        .join("patches/2027-01-first/packs/main")
        .join(file_of(&schema, "weenie", &row).unwrap());
    let text = write_row(&schema, "weenie", None, "weenie 509", Some(&row))
        .unwrap()
        .expect("a file");
    assert!(text.contains("Setup = 0x02000001"), "{text}");
    write(&file, &format!("# Fixes to the drudge.\n{text}"));
    assert_eq!(weenie_at(&world, 1).as_ref(), Some(&wanted));
    assert_eq!(over(&schema, "weenie", Some(&below), &row), wanted);

    // The level set back: the patch no longer sets it, and the comment stays.
    let mut back = wanted.clone();
    back.insert("int".into(), t("ItemType = 16\nLevel = 5").into());
    let row = delta(&schema, "weenie", Some(&below), &back).expect("still a change");
    let old = std::fs::read_to_string(&file).unwrap();
    let text = write_row(&schema, "weenie", Some(&old), "weenie 509", Some(&row))
        .unwrap()
        .expect("a file");
    assert!(text.starts_with("# Fixes to the drudge.\n"), "{text}");
    assert!(!text.contains("Level"), "{text}");
    write(&file, &text);
    assert_eq!(weenie_at(&world, 1).as_ref(), Some(&back));
    assert_eq!(
        row_in(&schema, "weenie", &text, "weenie 509").unwrap(),
        Some(row)
    );

    // Nothing changed: no row, and no file.
    assert_eq!(delta(&schema, "weenie", Some(&below), &below), None);
    assert_eq!(
        write_row(&schema, "weenie", Some(&text), "weenie 509", None).unwrap(),
        None,
        "the file's one row, and the comment over it, taken away"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A patch takes an item away with a `[[remove]]` row, and puts it back by taking that away.
#[test]
fn a_patch_takes_an_item_away_with_a_remove_row() {
    let (dir, world) = repo("remove");
    let file = dir.join("patches/2027-01-first/packs/main/removed.toml");
    let text = write_removal(None, "weenie 509", true)
        .unwrap()
        .expect("a file");
    assert_eq!(
        write_removal(Some(&text), "weenie 509", true).unwrap(),
        Some(text.clone()),
        "taken away once"
    );
    write(&file, &text);
    assert_eq!(weenie_at(&world, 1), None);
    assert!(weenie_at(&world, 0).is_some());
    assert_eq!(
        write_removal(Some(&text), "weenie 509", false).unwrap(),
        None
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// New Patch writes a patch after the last layer, with one pack to write in, and adds it to the
/// world file's lineage, the file's comments kept; an id not of the form, or one the world has, is
/// refused.
#[test]
fn new_patch_adds_a_patch_with_a_pack_after_the_last_layer() {
    let (dir, world) = repo("new-patch");
    let text = std::fs::read_to_string(&world).unwrap();
    write(&world, &format!("# The test world.\n{text}"));
    let repo = Repo::open(&world).unwrap();
    assert!(repo.new_patch("2027-02", "Second", "2027-02-01").is_err());
    assert!(repo
        .new_patch("2027-02-Second", "Second", "2027-02-01")
        .is_err());
    assert!(repo
        .new_patch("2027-01-first", "First", "2027-01-01")
        .is_err());
    for (path, text) in repo
        .new_patch("2027-02-second", "Second", "2027-02-01")
        .unwrap()
    {
        write(&path, &text);
    }
    let repo = Repo::open(&world).unwrap();
    assert_eq!(repo.ids(), ["base", "2027-01-first", "2027-02-second"]);
    let last = &repo.layers[2];
    assert_eq!(last.packs.len(), 1);
    assert_eq!(last.packs[0].name, "main");
    assert!(std::fs::read_to_string(&world)
        .unwrap()
        .starts_with("# The test world.\n"));
    let patch = std::fs::read_to_string(dir.join("patches/2027-02-second/patch.toml")).unwrap();
    assert!(patch.contains("after = \"2027-01-first\""), "{patch}");
    assert!(dir.join("patches/2027-02-second/NOTES.md").is_file());
    let _ = std::fs::remove_dir_all(&dir);
}
