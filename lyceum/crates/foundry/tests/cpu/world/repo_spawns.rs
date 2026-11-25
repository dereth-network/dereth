//! Behaviour: none (tooling: a world repository's placements edited as the world builder's spawns, and published)
//!
//! Vectors: `row_sql_pair/`, the server's fixture world's weenie 100 and the schema of its rows.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use dereth_primitives::Vec3;
use lyceum_foundry::content::Schema;
use lyceum_foundry::edit::{Op, Session, Weenie};
use lyceum_foundry::export::Importer;
use lyceum_foundry::publish::{publish, Publish};
use lyceum_foundry::repo::{Repo, RowSchema};
use lyceum_foundry::repo_world::world_of;

use crate::common::{Fixture, HOME};

fn pair() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cpu/world/row_sql_pair")
}

fn schema_text() -> String {
    std::fs::read_to_string(pair().join("schema.json")).unwrap()
}

/// The layer's own placement: a creature of class 100 in [`HOME`]'s outdoor cell 9.
const OWN: &str = "# The guard.\n[[placement]]\nguid = 0x70101800\nclass = 100\ncell = 0x01010009\nat = [30.5, 40.25, 12.0]\nrotation = [1.0, 0.0, 0.0, 0.0]\n";

/// A repository whose base is a library of rows (weenie 100, and one placement of it) and the
/// pack edited, which places weenie 100 once more ([`OWN`]).
fn repo(f: &Fixture) -> PathBuf {
    f.write(
        "repo/base/library/pack.toml",
        "name = \"library\"\nversion = \"1\"\n",
    );
    f.write(
        "repo/base/library/weenie/00100.toml",
        &std::fs::read_to_string(pair().join("weenie-100.toml")).unwrap(),
    );
    f.write(
        "repo/base/library/landblock/0101.toml",
        "[[placement]]\nguid = 0x70101005\nclass = 100\ncell = 0x01010001\nat = [5.0, 6.0, 7.0]\nrotation = [1.0, 0.0, 0.0, 0.0]\n",
    );
    f.write(
        "repo/base/emergence/pack.toml",
        "name = \"emergence\"\nversion = \"1\"\n",
    );
    f.write("repo/base/emergence/landblock/0101.toml", OWN);
    f.write(
        "repo/world.toml",
        "format_version = 1\ngame = \"ac1\"\nkey = \"test-repo\"\nera = \"newdereth\"\npacks = [\"base/library\", \"base/emergence\"]\nlineage = []\n[base]\nmonth = \"2026-12\"\n[land]\nledger = \"land/generations.tsv\"\n[ids]\nclasses = [2000000, 2999999]\n",
    )
}

/// The repository opened as the world, its newest layer edited, as Lyceum opens it.
fn session(f: &Fixture, world: &Path) -> Session {
    let repo = Repo::open(world).unwrap();
    let layer = repo.shown_layers() - 1;
    let w = world_of(&repo, layer, None, &f.path().join("build")).unwrap();
    let mut s = Session::open_world(w, &f.dats(), None, None).unwrap();
    s.set_schema(Some(Schema::read(&schema_text()).unwrap()));
    s.open_repository(&repo, layer).unwrap();
    s
}

/// [`repo`] with a patch after its base, whose pack edits go into.
fn repo_with_patch(f: &Fixture) -> PathBuf {
    repo(f);
    f.write(
        "repo/patches/2027-01-fixes/patch.toml",
        "id = \"2027-01-fixes\"\ntitle = \"Fixes\"\ndate = \"2027-01-01\"\nafter = \"\"\npacks = [\"fixes\"]\n",
    );
    f.write(
        "repo/patches/2027-01-fixes/packs/fixes/pack.toml",
        "name = \"fixes\"\nversion = \"1\"\n",
    );
    f.write(
        "repo/world.toml",
        "format_version = 1\ngame = \"ac1\"\nkey = \"test-repo\"\nera = \"newdereth\"\npacks = [\"base/library\", \"base/emergence\"]\nlineage = [\"2027-01-fixes\"]\n[base]\nmonth = \"2026-12\"\n[land]\nledger = \"land/generations.tsv\"\n[ids]\nclasses = [2000000, 2999999]\n",
    )
}

/// The server's import tool, as a test has it: its world pack is the world's placements at the
/// layer, folded from the repository's rows as the server's build folds them, one line each.
struct Tool;

impl Importer for Tool {
    fn run(&self, args: &[OsString]) -> Result<String, String> {
        let arg = |name: &str| {
            args.iter()
                .position(|a| a == name)
                .and_then(|i| args.get(i + 1))
                .ok_or(format!("no {name}"))
        };
        let repo = Repo::open(Path::new(arg("--world")?))?;
        let layer = arg("--layer")?;
        let at = repo
            .layers
            .iter()
            .position(|l| l.id.as_str() == layer.to_string_lossy())
            .ok_or("no such layer")?;
        let rows = RowSchema::read(&schema_text())?.ok_or("no rows")?;
        let world = repo.fold(&rows, at)?;
        let mut pack = String::new();
        for (name, item) in &world.items {
            if item.kind == "placement" {
                let row = item.row(&rows);
                pack.push_str(&format!(
                    "{name} cell={:#010X} at={}\n",
                    row["cell"].as_integer().unwrap_or(0),
                    row["at"]
                ));
            }
        }
        std::fs::write(arg("--out")?, pack).map_err(|e| e.to_string())?;
        Ok("built".into())
    }
}

/// The world pack `lyceum publish` builds from the repository as saved.
fn published(f: &Fixture, world: &Path) -> String {
    let out = f.path().join("served");
    let p = Publish {
        layer: None,
        land: None,
        number: true,
        importer: &Tool,
    };
    publish(world, &f.dats(), &out, &p).unwrap();
    std::fs::read_to_string(out.join("world.pack")).unwrap()
}

/// A world repository's placements are the world builder's spawns: the layer's own, as the
/// layer's pack writes them, and those of the world below it as the server's own.
#[test]
fn a_repository_worlds_placement_rows_are_listed_as_its_spawns() {
    let f = Fixture::new("repo-spawns-listed");
    let world = repo(&f);
    let mut s = session(&f, &world);
    let target = s.packs().len() - 1;
    let spawns = s.spawns();
    let own = spawns
        .iter()
        .find(|s| s.guid == "0x70101800")
        .expect("the layer's placement is a spawn");
    assert!(!own.server && own.pack == target, "{own:?}");
    assert_eq!((own.weenie.as_str(), own.cell), ("100", 0x0101_0009));
    let at = own.frame.origin;
    assert!(
        (at.x - 30.5).abs() < 1e-4 && (at.y - 40.25).abs() < 1e-4 && (at.z - 12.0).abs() < 1e-4,
        "{at:?}"
    );

    s.show_server_spawns([HOME]);
    let below = s
        .spawns()
        .into_iter()
        .find(|s| s.guid == "0x70101005")
        .expect("the library's placement is the server's own spawn");
    assert!(below.server, "{below:?}");
    assert_eq!(below.cell, 0x0101_0001);
}

/// A creature placed in a world repository is the layer's placement row, published in the world
/// pack; moved, the row and the pack hold where it stands now; taken away, it is gone from both.
/// The layer's other placement and its comment stay as written, and nothing is written as SQL.
#[test]
fn a_creature_placed_moved_and_removed_in_a_repository_world_reaches_its_published_world() {
    let f = Fixture::new("repo-spawns-published");
    let world = repo(&f);
    let pack_dir = world.parent().unwrap().join("base/emergence");
    let rows = pack_dir.join("landblock/0101.toml");
    let mut s = session(&f, &world);

    let applied = s
        .apply(&Op::PlaceSpawn {
            landblock: HOME,
            weenie: Weenie::Class(100),
            origin: Vec3::new(50.0, 60.0, 8.0),
            heading: 0.0,
        })
        .unwrap();
    let file = applied.spawned.expect("a spawn placed");
    let placed = s
        .spawns()
        .into_iter()
        .find(|s| s.file == file)
        .expect("listed");
    assert_eq!(
        placed.guid, "0x70101801",
        "the first free of the landblock's own"
    );
    let written = s.save().unwrap();
    assert!(
        written
            .iter()
            .all(|w| !w.starts_with(pack_dir.join("server"))),
        "{written:?}"
    );
    let text = std::fs::read_to_string(&rows).unwrap();
    assert!(text.starts_with(OWN), "{text}");
    assert!(
        text.contains("guid = 0x70101801") && text.contains("class = 100"),
        "{text}"
    );
    let pack = published(&f, &world);
    assert!(
        pack.contains("spawn 0x70101801 cell=0x01010013 at=[50.0, 60.0, 8.0]"),
        "{pack}"
    );
    assert!(pack.contains("spawn 0x70101800 "), "{pack}");

    s.apply(&Op::MoveSpawn {
        file: file.clone(),
        guid: placed.guid.clone(),
        origin: Vec3::new(70.0, 80.0, 9.0),
        heading: 90.0,
    })
    .unwrap();
    s.save().unwrap();
    let pack = published(&f, &world);
    assert!(
        pack.contains("spawn 0x70101801 cell=0x01010014 at=[70.0, 80.0, 9.0]"),
        "{pack}"
    );

    s.apply(&Op::RemoveSpawn { file }).unwrap();
    s.save().unwrap();
    let pack = published(&f, &world);
    assert!(!pack.contains("spawn 0x70101801"), "{pack}");
    assert!(pack.contains("spawn 0x70101800 "), "{pack}");
    assert_eq!(std::fs::read_to_string(&rows).unwrap(), OWN);
    assert!(!pack_dir.join("server").exists());
}

/// A placement of the world below the layer, moved, is the layer's row of what changed: its
/// cell, place and turn, over the library's.
#[test]
fn a_placement_below_the_layer_moved_is_the_layers_change_of_it() {
    let f = Fixture::new("repo-spawns-below");
    let world = repo(&f);
    let mut s = session(&f, &world);
    s.show_server_spawns([HOME]);
    let below = s
        .spawns()
        .into_iter()
        .find(|s| s.guid == "0x70101005")
        .unwrap();
    s.apply(&Op::MoveSpawn {
        file: below.file,
        guid: below.guid,
        origin: Vec3::new(100.0, 110.0, 3.0),
        heading: 0.0,
    })
    .unwrap();
    s.save().unwrap();
    let text = std::fs::read_to_string(
        world
            .parent()
            .unwrap()
            .join("base/emergence/landblock/0101.toml"),
    )
    .unwrap();
    let t: toml::Table = toml::from_str(&text).unwrap();
    let moved = t["placement"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["guid"].as_integer() == Some(0x7010_1005))
        .expect("the layer's row of it");
    assert_eq!(moved.get("class"), None, "{text}");
    assert_eq!(moved["at"].as_array().unwrap().len(), 3, "{text}");
    let pack = published(&f, &world);
    assert!(
        pack.contains("spawn 0x70101005 cell=0x01010025 at=[100.0, 110.0, 3.0]"),
        "{pack}"
    );
}

/// A class only the server's database has, placed in a world repository, is written into the
/// layer with its placement, as a weenie row of its own: the published world has both. What the
/// database is learned for beside a repository is its weenies, to place, and never its spawns.
#[test]
fn a_class_the_repository_lacks_is_written_into_the_layer_with_its_placement() {
    let f = Fixture::new("repo-spawns-weenie");
    let world = repo(&f);
    let mut s = session(&f, &world);
    let database = std::fs::read_to_string(pair().join("weenie-200.sql")).unwrap();
    let mut content = lyceum_foundry::sql_ids::server_content(&format!(
        "{database}\nINSERT INTO `landblock_instance` (`guid`, `weenie_Class_Id`, `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`, `is_Link_Child`, `last_Modified`)\nVALUES (0x70101006, 200, 0x01010001, 1, 2, 3, 1, 0, 0, 0, False, '2005-02-09 10:00:00');\n"
    ));
    assert_eq!(content.instances.len(), 1);
    content.weenies.entry(100).or_default().name = Some("the database's own".into());
    s.learn_server_beside(content);
    s.show_server_spawns([HOME]);
    assert!(s.server_weenies().contains_key(&200));
    assert_ne!(
        s.server_weenies()[&100].name.as_deref(),
        Some("the database's own"),
        "the repository's weenie stays its own"
    );
    assert!(!s.spawns().iter().any(|s| s.guid == "0x70101006"));

    assert!(!s.repository_lacks(100));
    assert!(s.repository_lacks(200));
    let file = s.class_file(200, "Drudge");
    s.apply(&Op::Batch(vec![
        Op::WriteSql {
            file,
            text: Some(database),
        },
        Op::PlaceSpawn {
            landblock: HOME,
            weenie: Weenie::Class(200),
            origin: Vec3::new(50.0, 60.0, 8.0),
            heading: 0.0,
        },
    ]))
    .unwrap();
    assert!(!s.repository_lacks(200));
    s.save().unwrap();
    let pack_dir = world.parent().unwrap().join("base/emergence");
    let weenie = std::fs::read_to_string(pack_dir.join("weenie/00200.toml")).unwrap();
    assert!(weenie.contains("class = 200"), "{weenie}");
    let rows = std::fs::read_to_string(pack_dir.join("landblock/0101.toml")).unwrap();
    assert!(rows.contains("class = 200"), "{rows}");
    assert!(published(&f, &world).contains("spawn 0x70101801 "));
}

/// A placement's links are the children its generator or lever sets off, which the server's SQL
/// writes as statements of their own: read into the placement's `links`, and written back from
/// them. A placement viewed and read back unchanged is no change.
#[test]
fn a_placements_links_are_its_link_statements_both_ways() {
    let f = Fixture::new("repo-spawns-links");
    let world = repo(&f);
    f.write(
        "repo/base/emergence/landblock/0101.toml",
        &format!("{OWN}\n[[placement]]\nguid = 0x70101801\nclass = 100\ncell = 0x01010009\nat = [31.0, 40.0, 12.0]\nrotation = [1.0, 0.0, 0.0, 0.0]\nlinks = [0x70101800]\n"),
    );
    let mut s = session(&f, &world);
    assert!(!s.rows_unsaved(), "a placement viewed is no change of it");
    let file = s.mirror().unwrap().file_of("placement", "spawn 0x70101801");
    let text = s.sql_text(&file).expect("its view");
    assert!(
        text.contains("`landblock_instance_link`") && text.contains("(0x70101801, 0x70101800, "),
        "{text}"
    );
    // Another child linked, by a statement of its own.
    let linked = format!(
        "{text}\nINSERT INTO `landblock_instance_link` (`parent_GUID`, `child_GUID`, `last_Modified`)\nVALUES (0x70101801, 0x70101005, '2026-11-25 00:00:00');\n"
    );
    s.apply(&Op::WriteSql {
        file,
        text: Some(linked),
    })
    .unwrap();
    let rows_file = world
        .parent()
        .unwrap()
        .join("base/emergence/landblock/0101.toml");
    let rows = s.rows_text(&rows_file).unwrap();
    assert!(rows.contains("links = [0x70101800, 0x70101005]"), "{rows}");
}

/// A patch takes away a placement of the world below it with a `[[remove]]` row: it is no longer
/// among the spawns, and the published world lacks it. The base takes nothing away.
#[test]
fn a_patch_takes_away_a_placement_below_it_with_a_remove_row() {
    let f = Fixture::new("repo-spawns-remove-below");
    let world = repo_with_patch(&f);
    let mut s = session(&f, &world);
    s.show_server_spawns([HOME]);
    let below = s
        .spawns()
        .into_iter()
        .find(|s| s.guid == "0x70101800")
        .expect("the base's placement, below the patch");
    assert!(below.server);
    s.apply(&Op::RemoveServerSpawn {
        file: below.file,
        guid: below.guid,
    })
    .unwrap();
    assert!(!s.spawns().iter().any(|s| s.guid == "0x70101800"));
    s.save().unwrap();
    let removal = std::fs::read_to_string(
        world
            .parent()
            .unwrap()
            .join("patches/2027-01-fixes/packs/fixes/remove.toml"),
    )
    .unwrap();
    assert!(removal.contains("item = \"spawn 0x70101800\""), "{removal}");
    let pack = published(&f, &world);
    assert!(!pack.contains("spawn 0x70101800"), "{pack}");
    assert!(pack.contains("spawn 0x70101005 "), "{pack}");

    // In the base, the library's placement stays.
    let f = Fixture::new("repo-spawns-remove-in-base");
    let world = repo(&f);
    let mut s = session(&f, &world);
    s.show_server_spawns([HOME]);
    let below = s
        .spawns()
        .into_iter()
        .find(|s| s.guid == "0x70101005")
        .unwrap();
    let why = s
        .apply(&Op::RemoveServerSpawn {
            file: below.file,
            guid: below.guid,
        })
        .unwrap_err()
        .to_string();
    assert!(why.contains("the base takes nothing away"), "{why}");
}

/// Spawns a pack writes as SQL beside a repository's rows (a dropped building's doors) are not
/// its placements: the checks say a published world won't have them.
#[test]
fn spawns_written_as_sql_beside_a_repositorys_rows_are_warned_of() {
    let f = Fixture::new("repo-spawns-beside");
    let world = repo(&f);
    f.write(
        "repo/base/emergence/server/sql/landblocks/0101 door.sql",
        "INSERT INTO `landblock_instance` (`guid`, `weenie_Class_Id`, `obj_Cell_Id`) VALUES (0x70101FFF, 100, 0x01010001);\n",
    );
    let s = session(&f, &world);
    let (_scratch, first) =
        lyceum_foundry::scratch::Scratch::create(&f.path().join("scratch"), &s).unwrap();
    let faults = s.check(&first.dir).unwrap();
    assert!(
        faults.iter().any(|x| x.warning
            && x.text.contains("0101 door.sql")
            && x.text.contains("won't have them")),
        "{faults:?}"
    );
}

/// The classes of a world repository are the world's to the checks: one the layer edited makes,
/// and one of the world below, named by the layer's weenie, are no fault; a class nobody has
/// still is.
#[test]
fn a_repositorys_own_classes_are_no_fault_where_the_layer_names_them() {
    let f = Fixture::new("repo-classes-checked");
    let world = repo(&f);
    // The layer's weenie 2000001, whose generators make its weenie 2000002 and the base's 100,
    // and whose create list names 273, which no one has.
    let own = std::fs::read_to_string(pair().join("weenie-100.toml"))
        .unwrap()
        .replace("class = 100\n", "class = 2000001\n")
        .replace("weenie_class_id = 300\n", "weenie_class_id = 2000002\n")
        .replace("weenie_class_id = 200\n", "weenie_class_id = 100\n");
    f.write("repo/base/emergence/weenie/2000001.toml", &own);
    f.write(
        "repo/base/emergence/weenie/2000002.toml",
        &std::fs::read_to_string(pair().join("weenie-300.toml"))
            .unwrap()
            .replace("class = 300\n", "class = 2000002\n"),
    );
    let mut s = session(&f, &world);
    // The server's own classes, beside the repository: one it doesn't name.
    s.learn_server_beside(lyceum_foundry::sql_ids::server_content(
        "INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`) VALUES (7, 'seven', 1, '2021-11-01 00:00:00');\n",
    ));
    let (_scratch, first) =
        lyceum_foundry::scratch::Scratch::create(&f.path().join("scratch"), &s).unwrap();
    let faults: Vec<String> = s
        .check(&first.dir)
        .unwrap()
        .into_iter()
        .filter(|x| !x.warning)
        .map(|x| x.text)
        .collect();
    let names = |class: u32| {
        faults
            .iter()
            .any(|t| t.contains(&format!("names class {class},")))
    };
    assert!(names(273), "the check runs: {faults:?}");
    assert!(!names(2_000_002), "{faults:?}");
    assert!(!names(100), "{faults:?}");
}
