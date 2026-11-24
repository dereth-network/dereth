//! Behaviour: none (tooling: a world repository's layer edited through the pack's SQL in the world editor's session)
//!
//! Vectors: `world/row_sql_pair/`, the server's fixture world's rows and the schema of their names.

use std::path::{Path, PathBuf};

use lyceum_foundry::content::{PropertyKind, Schema, SqlDoc};
use lyceum_foundry::edit::{Op, Session};
use lyceum_foundry::export::{self, Era, Options, Target};
use lyceum_foundry::loot::LootKind;
use lyceum_foundry::repo::Repo;

use crate::common::Fixture;

fn pair() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cpu/world/row_sql_pair")
}

/// A repository whose base is the pair's rows (weenies, quests, death and wielded treasures),
/// with an empty patch after it.
fn repo(f: &Fixture) -> PathBuf {
    let dir = pair();
    for e in std::fs::read_dir(&dir).unwrap() {
        let p = e.unwrap().path();
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        if let Some(stem) = name.strip_suffix(".toml") {
            let (kind, key) = stem.rsplit_once('-').unwrap();
            f.write(
                &format!("repo/base/library/{kind}/{key}.toml"),
                &std::fs::read_to_string(&p).unwrap(),
            );
        }
    }
    f.write(
        "repo/base/library/pack.toml",
        "name = \"library\"\nversion = \"1\"\n",
    );
    f.write(
        "repo/patches/2027-01-first/patch.toml",
        "id = \"2027-01-first\"\ntitle = \"First\"\ndate = \"2027-01-01\"\nafter = \"\"\npacks = [\"main\"]\n",
    );
    f.write(
        "repo/patches/2027-01-first/packs/main/pack.toml",
        "name = \"main\"\nversion = \"1\"\n",
    );
    f.write(
        "repo/world.toml",
        "format_version = 1\ngame = \"ac1\"\npacks = [\"base/library\"]\nlineage = [\"2027-01-first\"]\n[ids]\nclasses = [2000000, 2999999]\n",
    )
}

fn session(f: &Fixture) -> (Session, PathBuf) {
    let mut s = Session::open(&f.world(), &f.dats(), None).unwrap();
    let schema =
        Schema::read(&std::fs::read_to_string(pair().join("schema.json")).unwrap()).unwrap();
    s.set_schema(Some(schema));
    let world = repo(f);
    s.open_repository(&Repo::open(&world).unwrap(), 1).unwrap();
    let patch = world
        .parent()
        .unwrap()
        .join("patches/2027-01-first/packs/main");
    (s, patch)
}

/// SQL file `file` of the pack as the session holds it, changed by `change`, written back as a
/// step.
fn change(s: &mut Session, file: &Path, change: impl FnOnce(&mut SqlDoc)) {
    let mut doc = SqlDoc::parse(&s.sql_text(file).expect("the file"));
    change(&mut doc);
    s.apply(&Op::WriteSql {
        file: file.to_path_buf(),
        text: Some(doc.render()),
    })
    .unwrap();
}

/// With a repository's patch open, the world below it is the server's: its weenies, quests and
/// loot. A server's quest or death treasure registered in the pack and changed, and a server's
/// weenie copied and changed, are written as the patch's rows of only what changed; put back, the
/// patch's row goes. Undo and redo take the rows with them; a save writes the rows and never the
/// pack's mirror of them.
#[test]
fn a_repositorys_items_changed_through_the_packs_sql_are_written_as_the_layers_rows() {
    let f = Fixture::new("mirror");
    let (mut s, patch) = session(&f);
    assert!(s.server_weenies().contains_key(&100));
    assert!(s
        .server_quests()
        .keys()
        .any(|q| q.eq_ignore_ascii_case("testquest")));
    assert!(s.server_treasures().contains_key(&5));

    // A quest: registered in the pack as the server has it, which changes nothing; then changed.
    let op = s.quest_change_here("TestQuest").unwrap();
    s.apply(&op).unwrap();
    let quest = patch.join("quest/TestQuest.toml");
    assert_eq!(s.rows_text(&quest), None, "a copy as it was is no change");
    let file = s.quest_file("TestQuest");
    change(&mut s, &file, |d| {
        let r = d.rows("quest")[0];
        d.set_cell(r, "min_Delta", "60");
    });
    let text = s.rows_text(&quest).expect("the quest's row");
    assert!(
        text.contains("min_delta = 60") && !text.contains("max_solves"),
        "{text}"
    );

    // A death treasure, the same way.
    let op = s.loot_change_here(LootKind::Death, 5).unwrap();
    s.apply(&op).unwrap();
    let file = s.loot_file(LootKind::Death, 5);
    change(&mut s, &file, |d| {
        let r = d.rows("treasure_death")[0];
        d.set_cell(r, "tier", "3");
    });
    let text = s
        .rows_text(&patch.join("treasure_death/5.toml"))
        .expect("the treasure's row");
    assert!(
        text.contains("tier = 3") && !text.contains("item_chance"),
        "{text}"
    );

    // A weenie: the server's class copied and its level changed.
    let below = s.mirror().unwrap().below_text("weenie 100").unwrap();
    let file = s.class_file(100, "drudgetest");
    s.apply(&Op::WriteSql {
        file: file.clone(),
        text: Some(below),
    })
    .unwrap();
    change(&mut s, &file, |d| {
        d.set_property(PropertyKind::Int, 25, "6", None);
    });
    let weenie = patch.join("weenie/00100.toml");
    let text = s.rows_text(&weenie).expect("the weenie's row");
    assert!(
        text.contains("Level = 6") && !text.contains("ItemType"),
        "{text}"
    );

    // Put back as the server has it: the patch's row goes; undo brings it back.
    s.apply(&Op::WriteSql {
        file: file.clone(),
        text: None,
    })
    .unwrap();
    assert_eq!(s.rows_text(&weenie), None);
    s.undo().unwrap();
    assert!(s
        .rows_text(&weenie)
        .is_some_and(|t| t.contains("Level = 6")));
    s.redo().unwrap();
    assert_eq!(s.rows_text(&weenie), None);

    // Saved: the rows, and no mirror.
    s.save().unwrap();
    assert!(std::fs::read_to_string(&quest)
        .unwrap()
        .contains("min_delta = 60"));
    assert!(!s.content_dir().exists(), "the mirror is never saved");
    assert!(!s.rows_unsaved() && !s.unsaved());
}

/// The pack's mirror of a repository's layer rides into the world the editor serves, but no
/// export carries it.
#[test]
fn an_export_with_a_repository_open_holds_none_of_its_items() {
    let f = Fixture::new("mirror-export");
    let (mut s, _) = session(&f);
    let op = s.quest_change_here("TestQuest").unwrap();
    s.apply(&op).unwrap();
    assert!(
        s.compiled()
            .sql
            .keys()
            .any(|p| p.components().any(|c| c.as_os_str() == ".repository")),
        "served"
    );
    let overlay = f.path().join("overlay");
    lyceum_foundry::compile::write_overlay(s.compiled(), s.base(), "test", &overlay).unwrap();
    let out = f.path().join("for-ace");
    export::export_compiled(
        &overlay,
        &s.compiled().sql,
        "test",
        &f.dats(),
        &Options {
            target: Target::Ace,
            out: &out,
            era: Era::Eor,
            world_sql: None,
            importer: None,
            gdle_quests: None,
        },
    )
    .unwrap();
    let mut texts = Vec::new();
    let mut todo = vec![out.join("sql")];
    while let Some(d) = todo.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                todo.push(p);
            } else if let Ok(t) = std::fs::read_to_string(&p) {
                texts.push(t);
            }
        }
    }
    assert!(!texts.iter().any(|t| t.contains("TestQuest")), "{texts:?}");
}

/// A row file of the repository changed on the disk since it was read (another editor, a pull)
/// refuses a change of an item read from it: the mirror never writes over the rows.
#[test]
fn a_change_is_refused_once_the_repositorys_rows_changed_on_the_disk() {
    let f = Fixture::new("mirror-fresh");
    let (mut s, _) = session(&f);
    let op = s.quest_change_here("TestQuest").unwrap();
    s.apply(&op).unwrap();
    let library = f.path().join("repo/base/library/quest/TestQuest.toml");
    let text = std::fs::read_to_string(&library).unwrap();
    std::fs::write(&library, text.replace("3600", "7200")).unwrap();
    let file = s.quest_file("TestQuest");
    let mut doc = SqlDoc::parse(&s.sql_text(&file).unwrap());
    let r = doc.rows("quest")[0];
    doc.set_cell(r, "max_Solves", "3");
    let e = s
        .apply(&Op::WriteSql {
            file,
            text: Some(doc.render()),
        })
        .unwrap_err();
    assert!(e.to_string().contains("changed on the disk"), "{e}");
}

/// A new weenie of the layer, made from the world below's weenie as the editor makes one, takes
/// the repository's first free class and is written whole as the patch's row; it is the layer's
/// own, deleted by taking its row away.
#[test]
fn a_new_weenie_of_the_layer_takes_the_repositorys_first_free_class() {
    let f = Fixture::new("mirror-new");
    let (mut s, patch) = session(&f);
    let m = s.mirror().unwrap();
    let class = m.free_class(&Default::default()).unwrap();
    assert_eq!(class, 2_000_000);
    let text = lyceum_foundry::content::template(
        &m.below_text("weenie 100").unwrap(),
        100,
        "scout",
        false,
    )
    .unwrap()
    .replace("{{wcid:scout}}", &class.to_string());
    let (file, _) = s.new_weenie_file("scout").unwrap();
    assert!(s.mirror().unwrap().holds(&file));
    s.apply(&Op::WriteSql {
        file,
        text: Some(text),
    })
    .unwrap();
    let row = patch.join("weenie/2000000.toml");
    let text = s.rows_text(&row).expect("the new weenie's row");
    assert!(
        text.contains("class = 2000000")
            && text.contains("class_name = \"scout\"")
            && text.contains("ItemType"),
        "{text}"
    );
    assert!(s.layer_makes(class));
    assert_eq!(
        s.mirror().unwrap().free_class(&Default::default()),
        Some(2_000_001)
    );
    let op = s
        .weenie_removal(&lyceum_foundry::edit::Weenie::Class(class))
        .unwrap();
    s.apply(&op).unwrap();
    assert_eq!(s.rows_text(&row), None);
}
