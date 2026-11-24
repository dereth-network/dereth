//! Behaviour: none (tooling: a world of layers checked layer by layer, and built at a layer into its pack and ACE's world SQL)
//! A world folded at each layer is checked for what its fold finds and what it names but doesn't
//! have; built at a layer, it is the pack its rows make, and its world SQL builds that pack back.
//! Fixture: small worlds written to a temporary folder; with `launch-content`, the operator's
//! Launch pack named by `EMPYREAN_TEST_LAUNCH_FIXTURE`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use empyrean_content::import::{self, patch};
use empyrean_content::models::world::{
    CookBook, LandblockInstance, PointsOfInterest, Quest, Recipe, Spell, TreasureGemCount,
    TreasureWielded, Version, Weenie, WeeniePropertiesCreateList, WeeniePropertiesInt,
    WeeniePropertiesSpellBook, WeeniePropertiesString,
};
use empyrean_content::pack::Pack;
use empyrean_content::WorldContent;
use empyrean_world_db::compare::differences;
use empyrean_world_db::files::{default_stamp, set_source};

use super::{check_lines, content, pack_of};

/// A folder of its own under the temporary folder, removed when dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let p = std::env::temp_dir().join(format!("empyrean-world-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        Self(p)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn weenie(class: u32, name: &str) -> Weenie {
    let stamp = default_stamp();
    Weenie {
        class_id: class,
        class_name: name.into(),
        r#type: 1,
        last_modified: stamp,
        weenie_properties_int: vec![WeeniePropertiesInt {
            r#type: 1,
            value: 128,
            ..Default::default()
        }],
        weenie_properties_string: vec![WeeniePropertiesString {
            r#type: 1,
            value: format!("A {name}"),
            ..Default::default()
        }],
        ..Weenie::default()
    }
}

/// A small world with a row of most tables.
fn sample() -> WorldContent {
    let stamp = default_stamp();
    let mut chest = weenie(10, "chest");
    chest.weenie_properties_create_list = vec![WeeniePropertiesCreateList {
        destination_type: 2,
        weenie_class_id: 11,
        stack_size: 1,
        ..Default::default()
    }];
    chest.weenie_properties_spell_book = vec![WeeniePropertiesSpellBook {
        spell: 1,
        probability: 2.0,
        ..Default::default()
    }];
    let mut c = WorldContent {
        weenies: vec![chest, weenie(11, "apple"), weenie(12, "pie")],
        landblock_instances: vec![LandblockInstance {
            guid: 0x7A9B_4001,
            weenie_class_id: 10,
            obj_cell_id: 0xA9B4_0021,
            origin_x: 12.5,
            origin_y: 33.25,
            origin_z: 94.0,
            angles_w: 1.0,
            last_modified: stamp,
            ..Default::default()
        }],
        recipes: vec![Recipe {
            id: 7,
            success_wcid: 12,
            success_amount: 1,
            success_message: Some("You bake a pie.".into()),
            last_modified: stamp,
            ..Default::default()
        }],
        cook_books: vec![CookBook {
            recipe_id: 7,
            source_wcid: 11,
            target_wcid: 11,
            last_modified: stamp,
            ..Default::default()
        }],
        spells: vec![Spell {
            id: 1,
            name: "Strength Self I".into(),
            last_modified: stamp,
            ..Default::default()
        }],
        quests: vec![Quest {
            name: "ApplePicked".into(),
            min_delta: 60,
            max_solves: -1,
            message: Some("It's a fine apple.".into()),
            last_modified: stamp,
            ..Default::default()
        }],
        treasure_wielded: vec![TreasureWielded {
            treasure_type: 3,
            weenie_class_id: 11,
            probability: 0.5,
            set_start: true,
            last_modified: stamp,
            ..Default::default()
        }],
        points_of_interest: vec![PointsOfInterest {
            name: "Orchard".into(),
            weenie_class_id: 10,
            last_modified: stamp,
            ..Default::default()
        }],
        treasure_gem_count: vec![TreasureGemCount {
            gem_code: 1,
            tier: 2,
            count: 3,
            chance: 0.25,
            ..Default::default()
        }],
        versions: vec![Version {
            base_version: Some("test".into()),
            last_modified: stamp,
            ..Default::default()
        }],
        era: empyrean_common::era::EraId::Launch,
        ..WorldContent::default()
    };
    empyrean_world_db::number(&mut c);
    c
}

/// `c` as a world's base in `dir`, with a patch of each of `patches` (its id, and its one pack's
/// rows, or none), and the world's own classes from 1,000,000.
fn world(dir: &Path, c: &WorldContent, patches: &[(&str, Option<&str>)]) -> PathBuf {
    let mut files = empyrean_world_db::write(c, default_stamp()).expect("written");
    set_source(&mut files, "test-world", "r1").expect("a source");
    empyrean_world_db::write_dir(&dir.join("base"), &files).expect("base");
    let mut lineage = Vec::new();
    for (n, (id, rows)) in patches.iter().enumerate() {
        let pdir = dir.join("patches").join(id);
        std::fs::create_dir_all(&pdir).expect("patch folder");
        let after = if n == 0 {
            String::new()
        } else {
            patches[n - 1].0.to_owned()
        };
        let packs = if rows.is_some() { "[\"fixes\"]" } else { "[]" };
        let date = format!("{}-01", &id[..7]);
        std::fs::write(
            pdir.join("patch.toml"),
            format!("id = \"{id}\"\ntitle = \"{id}\"\ndate = \"{date}\"\nafter = \"{after}\"\npacks = {packs}\n"),
        )
        .expect("patch.toml");
        if let Some(rows) = rows {
            let pack = pdir.join("packs").join("fixes");
            std::fs::create_dir_all(&pack).expect("pack folder");
            std::fs::write(
                pack.join("pack.toml"),
                "name = \"fixes\"\nversion = \"1\"\n",
            )
            .expect("pack.toml");
            std::fs::write(pack.join("rows.toml"), rows).expect("rows");
        }
        lineage.push(format!("\"{id}\""));
    }
    let path = dir.join("world.toml");
    std::fs::write(
        &path,
        format!(
            "format_version = 1\ngame = \"ac1\"\npacks = [\"base\"]\nlineage = [{}]\n[ids]\nclasses = [1000000, 1999999]\n",
            lineage.join(", ")
        ),
    )
    .expect("world.toml");
    path
}

/// The pack `world.sql` and `empyrean.sql` build, as the importer builds content over an empty
/// database.
fn pack_from_sql(c: &WorldContent) -> Pack {
    let sql = crate::world_sql::write(c).expect("the SQL");
    let source = |name: &str, text: String| {
        Ok(patch::Source {
            kind: patch::InputKind::Sql,
            path: PathBuf::from(name),
            bytes: text.into_bytes(),
        })
    };
    let (bytes, imported) = import::build_from_for(
        &b""[..],
        [
            source("world.sql", sql.world),
            source("empyrean.sql", sql.empyrean),
        ],
        import::default_now(),
        c.era,
    )
    .expect("the SQL imports");
    imported
        .verify(import::Allow::default())
        .expect("nothing unread");
    assert!(imported.orphans.is_empty(), "{:?}", imported.orphans);
    Pack::from_bytes(bytes).expect("a pack")
}

#[test]
fn every_layer_is_checked_and_a_layer_that_changes_nothing_has_the_world_below_it() {
    let dir = Scratch::new("check");
    let path = world(
        &dir.0,
        &sample(),
        &[
            ("1999-12-sudden-season", Some("[[weenie]]\nclass = 11\nint = { Value = 5 }\n")),
            ("2000-01-dark-thaw", None),
            (
                "2000-02-shadows",
                Some("[[placement]]\nguid = 0x7A9B4002\nclass = 4000\ncell = 0xA9B40021\nat = [1.0, 2.0, 3.0]\nrotation = [1.0, 0.0, 0.0, 0.0]\n\n[[weenie]]\nclass = 11\nint = { Value = 5 }\n\n[[remove]]\nitem = \"quest nothing\"\n"),
            ),
            ("2000-03-darkness", None),
            ("2000-04-thorns", Some("[[weenie]]\nclass = 12\nint = { Value = 9 }\n")),
        ],
    );
    let f = empyrean_world_db::world::open(&path, None).expect("opened");
    assert!(
        f.changes(1) && !f.changes(2) && !f.changes(4) && f.changes(5),
        "only a layer with rows changes the world"
    );
    let checks = f.clone().check();
    assert_eq!(checks.len(), 6);
    assert!(
        checks[..3].iter().all(|l| l.errors().is_empty()),
        "{:?}",
        checks[1].errors()
    );
    // The third patch names a class nobody has, removes what nothing sets, and sets a value
    // already there; the one above it changes nothing and has its world.
    let third = checks[3].errors();
    assert!(
        third
            .iter()
            .any(|e| e == "spawn 0x7A9B4002: class 4000 is in no weenie"),
        "{third:?}"
    );
    assert!(
        third
            .iter()
            .any(|e| e.contains("nothing below or in this layer sets quest nothing")),
        "{third:?}"
    );
    assert!(checks[3].findings.iter().any(|x| x
        .to_string()
        .contains("weenie 11 int.Value: changes nothing below")));
    // Above it, what it broke stays broken, and is said where it broke.
    assert!(
        checks[4].problems.is_empty() && checks[5].problems.is_empty(),
        "{:?}",
        checks[5].problems
    );
    let (lines, errors) = check_lines(f);
    assert_eq!(errors, 2, "{lines:?}");
    assert_eq!(
        lines.last().map(String::as_str),
        Some("6 layers, 5 with nothing to say; 2 errors")
    );
    // A patch row's own key is no change.
    assert!(
        !lines.iter().any(|l| l.contains("weenie 11 class")),
        "{lines:?}"
    );
    // The world up to the second patch builds; the third, which breaks a reference, doesn't, and
    // nor does any above it.
    content(&path, Some("2000-01-dark-thaw"), None).expect("builds");
    let e = content(&path, None, None).expect_err("refused");
    assert!(
        e.contains("nothing below or in this layer sets quest nothing"),
        "{e}"
    );
}

#[test]
fn a_build_refuses_a_layer_that_breaks_a_reference_and_every_layer_above_it() {
    let dir = Scratch::new("refuse");
    let path = world(
        &dir.0,
        &sample(),
        &[
            (
                "1999-12-sudden-season",
                Some(
                    "[[placement]]
guid = 0x7A9B4002
class = 4000
cell = 0xA9B40021
at = [1.0, 2.0, 3.0]
rotation = [1.0, 0.0, 0.0, 0.0]
",
                ),
            ),
            ("2000-01-dark-thaw", None),
        ],
    );
    content(&path, Some("base"), None).expect("the base builds");
    for layer in [Some("1999-12-sudden-season"), None] {
        let e = content(&path, layer, None).expect_err("refused");
        assert_eq!(
            e,
            "1999-12-sudden-season: spawn 0x7A9B4002: class 4000 is in no weenie"
        );
    }
}

#[test]
fn a_world_built_at_a_layer_is_its_rows_pack_and_its_world_sql_builds_the_same_pack() {
    let dir = Scratch::new("build");
    let path = world(
        &dir.0,
        &sample(),
        &[(
            "1999-12-sudden-season",
            Some("[[weenie]]\nclass = 11\nint = { Value = 5 }\n"),
        )],
    );
    let base = content(&path, Some("base"), None).expect("the base");
    let mut want = sample();
    empyrean_world_db::number(&mut want);
    let built = Pack::from_bytes(pack_of(&base).expect("a pack")).expect("reads");
    let original = Pack::from_bytes(pack_of(&want).expect("a pack")).expect("reads");
    assert_eq!(built.header().content_hash, original.header().content_hash);
    // The last layer's SQL builds its pack back, every record equal, ids aside.
    let last = content(&path, None, None).expect("the last layer");
    assert!(last.weenies[1]
        .weenie_properties_int
        .iter()
        .any(|p| p.r#type == 19 && p.value == 5));
    let rebuilt = Pack::from_bytes(pack_of(&last).expect("a pack")).expect("reads");
    let found = differences(&rebuilt, &pack_from_sql(&last)).expect("compared");
    assert!(found.is_empty(), "{found:?}");
    let sql = crate::world_sql::write(&last).expect("the SQL");
    assert!(sql.world.starts_with(
        "-- A world folded from its rows, in ACE's schema (WorldBase.sql at 47edade3"
    ));
    assert!(sql.world.contains("INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)\nVALUES (10, 'chest', 1, '2005-02-09 10:00:00')"), "{}", sql.world);
}

#[test]
fn a_world_with_vital_recipes_writes_them_where_stock_ace_cant_read_them() {
    use empyrean_content::models::world::RecipeVitalRequirement;
    let mut c = sample();
    c.recipe_vital_requirements = vec![RecipeVitalRequirement {
        id: 7,
        health: 5,
        stamina: 0,
        mana: 0,
    }];
    empyrean_world_db::number(&mut c);
    let sql = crate::world_sql::write(&c).expect("the SQL");
    assert!(
        !sql.world.contains("INSERT INTO `recipe`"),
        "a vital recipe stays out of ACE's SQL"
    );
    assert!(sql.empyrean.contains("INSERT INTO `recipe` ("));
    assert!(sql
        .empyrean
        .contains("INSERT INTO `recipe_requirements_vital` VALUES (7,5,0,0);"));
    let original = Pack::from_bytes(pack_of(&c).expect("a pack")).expect("reads");
    let found = differences(&original, &pack_from_sql(&c)).expect("compared");
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn a_motion_past_a_signed_int_is_held_by_world_sqls_unsigned_column_and_builds_back() {
    use empyrean_content::models::world::{WeeniePropertiesEmote, WeeniePropertiesEmoteAction};
    let mut c = sample();
    c.weenies[0].weenie_properties_emote = vec![WeeniePropertiesEmote {
        category: 7,
        probability: 1.0,
        weenie_properties_emote_action: vec![
            WeeniePropertiesEmoteAction {
                r#type: 5,
                motion: Some(0x8000_003C),
                ..Default::default()
            },
            WeeniePropertiesEmoteAction {
                r#type: 5,
                order: 1,
                motion: Some(0x1300_0085),
                ..Default::default()
            },
        ],
        ..Default::default()
    }];
    empyrean_world_db::number(&mut c);
    let sql = crate::world_sql::write(&c).expect("the SQL");
    assert!(sql.world.contains("0x8000003C"));
    assert!(sql.world.contains("`motion` int unsigned DEFAULT NULL"));
    let original = Pack::from_bytes(pack_of(&c).expect("a pack")).expect("reads");
    let found = differences(&original, &pack_from_sql(&c)).expect("compared");
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn a_placements_links_go_in_their_childrens_order_as_aces_writer_writes_them() {
    use empyrean_content::models::world::LandblockInstanceLink;
    let mut c = sample();
    let mut child = c.landblock_instances[0].clone();
    for guid in [0x7A9B_4003, 0x7A9B_4002] {
        child.guid = guid;
        c.landblock_instances.push(child.clone());
    }
    c.landblock_instances[0].landblock_instance_link = [0x7A9B_4003, 0x7A9B_4002]
        .map(|g| LandblockInstanceLink {
            child_guid: g,
            last_modified: default_stamp(),
            ..Default::default()
        })
        .to_vec();
    empyrean_world_db::number(&mut c);
    let links: Vec<u32> = c.landblock_instances[0]
        .landblock_instance_link
        .iter()
        .map(|l| l.child_guid)
        .collect();
    assert_eq!(links, [0x7A9B_4002, 0x7A9B_4003]);
    let original = Pack::from_bytes(pack_of(&c).expect("a pack")).expect("reads");
    let found = differences(&original, &pack_from_sql(&c)).expect("compared");
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn a_checkpoints_own_references_to_nothing_are_its_sources_and_a_patchs_are_errors() {
    use empyrean_world_db::layers::{snapshot_of, CheckpointLayer, Folding, Layers, PatchLayer};
    let rows = |c: &WorldContent| {
        let mut files = empyrean_world_db::write(c, default_stamp()).expect("written");
        set_source(&mut files, "test-world", "r1").expect("a source");
        files
    };
    let base = sample();
    let mut later = sample();
    later.landblock_instances[0].weenie_class_id = 4000;
    let (snapshot, kinds) = snapshot_of(&rows(&later)).expect("a snapshot");
    let patch = BTreeMap::from([(
        "rows.toml".to_owned(),
        "[[placement]]\nguid = 0x7A9B4002\nclass = 4001\ncell = 0xA9B40021\nat = [1.0, 2.0, 3.0]\nrotation = [1.0, 0.0, 0.0, 0.0]\n".to_owned(),
    )]);
    let layers = Layers {
        base: rows(&base),
        patches: vec![
            PatchLayer {
                id: "2005-02-infiltration".into(),
                checkpoint: Some(Box::new(CheckpointLayer {
                    snapshot,
                    kinds,
                    drift: Vec::new(),
                })),
                ..Default::default()
            },
            PatchLayer {
                id: "2005-03-later".into(),
                packs: vec![("fixes".into(), patch)],
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let folding = Folding::new(layers, base.era, default_stamp()).expect("read");
    let checks = folding.clone().check();
    assert!(
        checks.iter().all(|l| l
            .findings
            .iter()
            .all(|f| f.severity() != empyrean_world_db::layers::Severity::Error)),
        "{checks:?}"
    );
    let class = |n: u32| format!("spawn 0x7A9B4001: class {n} is in no weenie");
    assert_eq!(checks[1].inherited, [class(4000)]);
    assert!(checks[1].problems.is_empty(), "{:?}", checks[1].problems);
    assert_eq!(
        checks[2].problems,
        ["spawn 0x7A9B4002: class 4001 is in no weenie"]
    );
    // The checkpoint builds, what it brings no error; the patch that breaks a reference doesn't.
    let at = folding
        .clone()
        .checked_content_at(1)
        .expect("the checkpoint builds");
    assert!(at
        .landblock_instances
        .iter()
        .any(|i| i.weenie_class_id == 4000));
    let e = folding.checked_content_at(2).expect_err("refused");
    assert_eq!(
        e,
        "2005-03-later: spawn 0x7A9B4002: class 4001 is in no weenie"
    );
}

#[cfg(feature = "launch-content")]
mod launch {
    //! The operator's Launch pack as a world's base, under empty patches as Dereth's are.
    use super::*;

    fn pack() -> Pack {
        let path = std::env::var_os("EMPYREAN_TEST_LAUNCH_FIXTURE")
            .expect("set EMPYREAN_TEST_LAUNCH_FIXTURE to the hash-pinned fixture manifest");
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).expect("manifest")).expect("json");
        Pack::open(Path::new(
            manifest["world_pack"].as_str().expect("world_pack"),
        ))
        .expect("the Launch pack")
    }

    #[test]
    fn the_launch_base_checks_builds_at_every_layer_and_its_world_sql_builds_the_converters_records(
    ) {
        let original = pack();
        let mut base = empyrean_world_db::from_pack(&original).expect("read");
        empyrean_world_db::number(&mut base);
        let dir = Scratch::new("launch");
        let path = world(
            &dir.0,
            &base,
            &[
                ("1999-12-sudden-season", None),
                ("2000-01-dark-thaw", None),
                ("2000-02-shadows", None),
            ],
        );
        let f = empyrean_world_db::world::open(&path, None).expect("opened");
        let ids = f.ids.clone();
        let (lines, errors) = check_lines(f);
        assert_eq!(errors, 0, "{lines:?}");
        let mut hashes = BTreeMap::new();
        for id in &ids {
            let c = content(&path, Some(id), None).expect("builds");
            let p = Pack::from_bytes(pack_of(&c).expect("a pack")).expect("reads");
            hashes.insert(id.clone(), p.header().content_hash);
        }
        assert_eq!(
            hashes
                .values()
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            1,
            "empty layers change nothing"
        );
        let c = content(&path, Some("base"), None).expect("the base");
        let found = differences(&original, &pack_from_sql(&c)).expect("compared");
        assert!(
            found.is_empty(),
            "{} differences, first {:?}",
            found.len(),
            found.first()
        );
    }
}
