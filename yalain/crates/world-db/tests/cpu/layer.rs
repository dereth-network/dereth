//! Behaviour: none (checks the world database's tooling: a world's packs read, checked and merged over a base database)

use yalain_world_db::layer::{self, Base};
use yalain_world_db::merge::item_key;
use yalain_world_db::rows::read;
use yalain_world_db::Files;

fn pack(name: &str) -> String {
    format!("format_version = 1\nname = \"{name}\"\nversion = \"0.1.0\"\ngame = \"ac2\"\n")
}

fn person(key: &str, x: f32) -> String {
    format!("[[object]]\nkey = \"{key}\"\nkind = \"npc\"\ntemplate = 0x470033D0\nat = [{x:.1}, 2.0, 3.0]\nprov = {{ from = \"authored\" }}\n")
}

/// A base of two people and a place, and an empty world over it.
fn files() -> (Files, Files) {
    let mut base = Files::new();
    base.insert(
        "world.toml".into(),
        "format_version = 1\nkey = \"db\"\ngame = \"ac2\"\nera = \"eor\"\npacks = [\"seed\"]\n"
            .into(),
    );
    base.insert("seed/pack.toml".into(), pack("seed"));
    base.insert(
        "seed/server/landblocks/77B8.toml".into(),
        format!("{}\n{}", person("a", 1.0), person("b", 2.0)),
    );
    base.insert(
        "seed/server/places.toml".into(),
        "[[place]]\nkey = \"inn\"\ncell = 0x77B80003\nat = [1.0, 2.0, 3.0]\nprov = { from = \"authored\" }\n".into(),
    );
    let mut world = Files::new();
    world.insert(
        "world.toml".into(),
        "format_version = 1\nkey = \"w\"\ngame = \"ac2\"\nera = \"eor\"\npacks = [\"fill\"]\n\n[base]\ndb = \"db\"\n".into(),
    );
    world.insert("fill/pack.toml".into(), pack("fill"));
    (base, world)
}

fn items(base: &Base, world: &Files) -> Vec<String> {
    let db = read(world).unwrap();
    layer::merge_over(base, &db)
        .unwrap()
        .rows
        .iter()
        .filter_map(item_key)
        .collect()
}

#[test]
fn a_world_row_replaces_the_base_row_of_its_key_where_it_stood() {
    let (b, mut w) = files();
    let base = Base::new(&b).unwrap();
    w.insert(
        "fill/server/landblocks/77B8.toml".into(),
        format!("{}\n{}", person("a", 9.0), person("c", 4.0)),
    );
    assert!(
        layer::check(&base, &w).is_empty(),
        "{:?}",
        layer::check(&base, &w)
    );
    let db = read(&w).unwrap();
    let merged = layer::merge_over(&base, &db).unwrap();
    let a = &merged.rows[0];
    assert_eq!(
        (a.pack.as_str(), item_key(a).unwrap().as_str()),
        ("fill", "object a@77B8")
    );
    assert_eq!(
        items(&base, &w),
        [
            "object a@77B8",
            "object b@77B8",
            "place inn",
            "object c@77B8"
        ]
    );
    assert_eq!(merged.overridden, ["object a@77B8"]);
}

#[test]
fn a_world_removal_takes_a_base_row_away_and_a_removal_of_nothing_warns() {
    let (b, mut w) = files();
    let base = Base::new(&b).unwrap();
    w.insert(
        "fill/server/landblocks/77B8.toml".into(),
        "[[remove]]\nitems = [\"object b@77B8\", \"object z@77B8\"]\n".into(),
    );
    assert!(layer::check(&base, &w).is_empty());
    assert_eq!(items(&base, &w), ["object a@77B8", "place inn"]);
    let merged = layer::merge_over(&base, &read(&w).unwrap()).unwrap();
    assert_eq!(merged.warnings.len(), 1);
    assert!(merged.warnings[0].message.contains("object z@77B8"));
}

#[test]
fn a_world_row_may_refer_to_the_bases_rows_by_their_pack() {
    let (b, mut w) = files();
    let base = Base::new(&b).unwrap();
    let portal = |to: &str| {
        format!("[[object]]\nkey = \"door\"\nkind = \"static\"\ntemplate = 0x4700115B\nat = [1.0, 2.0, 3.0]\nto = \"{to}\"\nprov = {{ from = \"authored\" }}\n")
    };
    w.insert(
        "fill/server/landblocks/77B8.toml".into(),
        portal("seed:inn"),
    );
    assert!(layer::check(&base, &w).is_empty());
    w.insert("fill/server/landblocks/77B8.toml".into(), portal("inn"));
    let problems = layer::check(&base, &w);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].message.contains("no place inn in pack fill"));
}

#[test]
fn a_bases_problems_are_named_under_the_base() {
    let (mut b, _) = files();
    b.insert(
        "seed/server/landblocks/77B8.toml".into(),
        "[[object]]\nkey = \"a\"\n".into(),
    );
    let problems = Base::new(&b).unwrap_err();
    assert!(
        problems.iter().all(|p| p.file.starts_with("base/")),
        "{problems:?}"
    );
}
