//! Behaviour: none (checks the world builder's worlds: made over the database, read and checked over their base, and counted)

use std::path::PathBuf;

use athenaeum_foundry::new::{key_from, new_world};
use athenaeum_foundry::{facts, world};
use yalain_world_db::layer;

use crate::{base, base_files, world_files};

/// A folder of the test's own, empty.
fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("athenaeum-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn write_all(dir: &std::path::Path, files: &yalain_world_db::Files) {
    for (rel, text) in files {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
}

#[test]
fn a_base_that_names_no_release_is_refused() {
    for tag in ["../v0.1.0", "..", "v0.1.0/../../x", "v1"] {
        let why = world::base_dir(Some(tag), None).expect_err(tag);
        assert!(why.contains("is not a release"), "{why}");
    }
}

#[test]
fn a_new_worlds_key_is_made_from_its_folders_name() {
    assert_eq!(key_from("My World"), "my-world");
    assert_eq!(key_from("Arwic (fill) 2"), "arwic-fill-2");
    assert_eq!(key_from("--"), "world");
}

#[test]
fn a_new_world_lies_over_the_database_with_an_empty_pack_to_fill() {
    let root = scratch("new");
    let base_dir = root.join("db");
    write_all(&base_dir, &base_files());
    let dir = root.join("Hearthstead");
    let made = new_world(&dir, None).unwrap();
    assert_eq!(made.key, "hearthstead");
    assert_eq!(made.pack, "fill-hearthstead");
    assert!(
        new_world(&dir, None).is_err(),
        "a world is never made over another"
    );
    // Its repository keeps out what playing and building it leaves beside it.
    let ignored = std::fs::read_to_string(dir.join(".gitignore")).unwrap();
    let lines: Vec<&str> = ignored.lines().filter(|l| !l.starts_with('#')).collect();
    assert_eq!(lines, [".playtest/", "build/", "overlay/"]);
    let opened = world::open(&dir, Some(&base_dir)).unwrap();
    assert_eq!(opened.base_dir.as_deref(), Some(base_dir.as_path()));
    assert!(opened.check().is_empty(), "{:?}", opened.check());
    let (db, _) = opened.read();
    let db = db.unwrap();
    assert_eq!(db.world.base.unwrap().db, world::DATABASE);
    assert_eq!(db.packs.len(), 1);
    assert!(db.packs[0].rows.is_empty());
    let f = facts::facts(&opened);
    assert_eq!(f.problems, 0);
    assert_eq!(f.base_rows, 4);
    assert_eq!(f.merged_rows, 4);
    assert_eq!(
        f.to_json(),
        "{\"world\":\"hearthstead\",\"base\":{\"db\":\"yalain-world\",\"version\":null},\"packs\":1,\"rows\":{},\"from\":{},\"replaced\":0,\"removed\":0,\"base_rows\":4,\"merged_rows\":4,\"problems\":0}"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_worlds_counts_say_what_it_changes() {
    let root = scratch("facts");
    let base_dir = root.join("db");
    write_all(&base_dir, &base_files());
    let dir = root.join("w");
    let mut files = world_files();
    files.insert(
        "fill-test/server/landblocks/77B8.toml".into(),
        "[[object]]\nkey = \"n-334c\"\nkind = \"npc\"\ntemplate = 0x4700334C\nat = [1.0, 2.0, 3.0]\nprov = { from = \"authored\" }\n\n[[object]]\nkey = \"person-1\"\nkind = \"npc\"\ntemplate = 0x470033D0\nat = [1.0, 2.0, 3.0]\nprov = { from = \"authored\" }\n\n[[remove]]\nitems = [\"camp c-4586@77B8\"]\n".into(),
    );
    write_all(&dir, &files);
    let opened = world::open(&dir, Some(&base_dir)).unwrap();
    let f = facts::facts(&opened);
    assert_eq!(f.problems, 0);
    assert_eq!(f.rows.get("object"), Some(&2));
    assert_eq!(f.rows.get("remove"), Some(&1));
    assert_eq!(f.from.get("authored"), Some(&2));
    assert_eq!((f.replaced, f.removed), (1, 1));
    assert_eq!(
        f.merged_rows, 4,
        "4 of the base, one replaced, one taken, one added"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_world_naming_another_database_or_a_bases_pack_name_has_a_problem() {
    let base = base();
    let mut files = world_files();
    files.insert(
        "world.toml".into(),
        files["world.toml"].replace("db = \"yalain-world\"", "db = \"another-world\""),
    );
    let problems = layer::check(&base, &files);
    assert!(
        problems.iter().any(|p| p.message.contains("another-world")),
        "{problems:?}"
    );
    let mut files = world_files();
    files.insert(
        "world.toml".into(),
        files["world.toml"].replace("packs = [\"fill-test\"]", "packs = [\"seed\"]"),
    );
    files.insert(
        "seed/pack.toml".into(),
        files["fill-test/pack.toml"].replace("fill-test", "seed"),
    );
    let problems = layer::check(&base, &files);
    assert!(
        problems
            .iter()
            .any(|p| p.message.contains("the base has a pack seed")),
        "{problems:?}"
    );
}

#[test]
fn two_world_packs_that_set_one_base_row_differently_conflict_until_one_wins() {
    let base = base();
    let mut files = world_files();
    files.insert(
        "world.toml".into(),
        files["world.toml"].replace(
            "packs = [\"fill-test\"]",
            "packs = [\"fill-test\", \"fix-test\"]",
        ),
    );
    files.insert(
        "fix-test/pack.toml".into(),
        files["fill-test/pack.toml"].replace("fill-test", "fix-test"),
    );
    let row = |x: f32| {
        format!("[[object]]\nkey = \"n-334c\"\nkind = \"npc\"\ntemplate = 0x4700334C\nat = [{x:.1}, 2.0, 3.0]\nprov = {{ from = \"authored\" }}\n")
    };
    files.insert("fill-test/server/landblocks/77B8.toml".into(), row(1.0));
    files.insert("fix-test/server/landblocks/77B8.toml".into(), row(9.0));
    let problems = layer::check(&base, &files);
    assert!(
        problems
            .iter()
            .any(|p| p.message.contains("object n-334c@77B8 is set by")),
        "{problems:?}"
    );
    files.insert(
        "world.toml".into(),
        format!(
            "{}\n[[override]]\npack = \"fix-test\"\nover = \"fill-test\"\n",
            files["world.toml"]
        ),
    );
    let problems = layer::check(&base, &files);
    assert!(problems.is_empty(), "{problems:?}");
    let db = yalain_world_db::rows::read(&files).unwrap();
    let merged = layer::merge_over(&base, &db).unwrap();
    let row = merged
        .rows
        .iter()
        .find(|l| yalain_world_db::merge::item_key(l).as_deref() == Some("object n-334c@77B8"))
        .unwrap();
    assert_eq!(row.pack, "fix-test");
}
