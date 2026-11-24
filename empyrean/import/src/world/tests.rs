//! Behaviour: none (tooling: a world of layers checked layer by layer, built at a layer into its pack and ACE's world SQL, and released with a manifest of every layer, and a layer's pack fetched from those releases)
//! A world folded at each layer is checked for what its fold finds and what it names but doesn't
//! have; built at a layer, it is the pack its rows make, and its world SQL builds that pack back.
//! Its loot is held to the swap from the source's to ACE's being whole, its records to the data
//! files of its layer, where they are given, and the points it names in rooms to the rooms' walls
//! where the files change them.
//! Fixture: small worlds and data files written to a temporary folder (whole cell files with the
//! land of every landblock, partial ones without), the content crate's synthetic source loot
//! policy; with `launch-content`, the operator's Launch pack named by
//! `EMPYREAN_TEST_LAUNCH_FIXTURE`.

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
    content(&path, Some("2000-01-dark-thaw"), None, &[]).expect("builds");
    let e = content(&path, None, None, &[]).expect_err("refused");
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
    content(&path, Some("base"), None, &[]).expect("the base builds");
    for layer in [Some("1999-12-sudden-season"), None] {
        let e = content(&path, layer, None, &[]).expect_err("refused");
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
    let base = content(&path, Some("base"), None, &[]).expect("the base");
    let mut want = sample();
    empyrean_world_db::number(&mut want);
    let built = Pack::from_bytes(pack_of(&base).expect("a pack")).expect("reads");
    let original = Pack::from_bytes(pack_of(&want).expect("a pack")).expect("reads");
    assert_eq!(built.header().content_hash, original.header().content_hash);
    // The last layer's SQL builds its pack back, every record equal, ids aside.
    let last = content(&path, None, None, &[]).expect("the last layer");
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
                era: None,
                checkpoint: Some(Box::new(CheckpointLayer {
                    snapshot,
                    kinds,
                    drift: Vec::new(),
                })),
                ..Default::default()
            },
            PatchLayer {
                id: "2005-03-later".into(),
                era: None,
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

/// `text` added to the end of the file at `path`.
fn append(path: &Path, text: &str) {
    let mut s = std::fs::read_to_string(path).expect("read");
    s.push_str(text);
    std::fs::write(path, s).expect("written");
}

/// The synthetic source loot policy the content crate's tests use.
fn source_policy() -> empyrean_content::loot::SourceLootPolicy {
    empyrean_content::loot::SourceLootPolicy::from_json(include_bytes!(
        "../../../fixtures/source-loot/synthetic.json"
    ))
    .expect("the policy")
}

/// `sample()` under the source's loot: its policy, grouped treasure of type 99, and a drudge
/// whose death treasure is that type.
fn source_loot() -> WorldContent {
    use empyrean_content::loot::{GroupedTreasureDeath, SlotGroup};
    use empyrean_content::models::world::WeeniePropertiesDID;
    use empyrean_entity::enums::PropertyDataId;
    let mut c = sample();
    c.loot_policies = vec![source_policy()];
    c.grouped_treasure = vec![GroupedTreasureDeath {
        id: 1,
        treasure_type: 99,
        tier: 1,
        quality: 0.5,
        groups: [SlotGroup {
            chance: 50,
            min: 1,
            max: 2,
        }; 6],
    }];
    let mut drudge = weenie(13, "drudge");
    drudge.weenie_properties_did = vec![WeeniePropertiesDID {
        r#type: PropertyDataId::DeathTreasureType.0,
        value: 99,
        ..Default::default()
    }];
    c.weenies.push(drudge);
    empyrean_world_db::number(&mut c);
    c
}

/// ACE's death treasure of type `ty`, as a file of rows.
fn death_row(ty: u32) -> String {
    use empyrean_content::models::world::TreasureDeath;
    let c = WorldContent {
        treasure_death: vec![TreasureDeath {
            treasure_type: ty,
            tier: 1,
            item_chance: 100,
            item_min_amount: 1,
            item_max_amount: 2,
            last_modified: default_stamp(),
            ..Default::default()
        }],
        ..WorldContent::default()
    };
    let files = empyrean_world_db::write(&c, default_stamp()).expect("written");
    files[&format!("treasure_death/{ty}.toml")].clone()
}

/// The checks of a world under the source's loot with one patch of `rows` on it.
fn swapped(name: &str, rows: &str) -> Vec<empyrean_world_db::layers::LayerCheck> {
    let dir = Scratch::new(name);
    let path = world(
        &dir.0,
        &source_loot(),
        &[("1999-12-sudden-season", Some(rows))],
    );
    empyrean_world_db::world::open(&path, None)
        .expect("opened")
        .check()
}

#[test]
fn a_layer_that_swaps_loot_to_aces_does_all_three_parts_at_once_and_each_part_alone_is_an_error() {
    let policy = "[[remove]]\nitems = [\"loot-policy\"]\n";
    let grouped = "[[remove]]\nitems = [\"grouped-treasure 99\"]\n";
    let both = "[[remove]]\nitems = [\"loot-policy\", \"grouped-treasure 99\"]\n";
    let death = death_row(99);
    // All three at once: the swap, reported, and no error.
    let checks = swapped("swap", &format!("{both}\n{death}"));
    assert!(checks[0].errors().is_empty(), "{:?}", checks[0].errors());
    assert!(checks[1].errors().is_empty(), "{:?}", checks[1].errors());
    assert_eq!(
        checks[1].reports,
        ["the loot becomes ACE's: the source's loot-policy goes, 1 grouped-treasure go and 1 death-treasure come"]
    );
    let errors = |rows: &str, name: &str| swapped(name, rows)[1].errors();
    // The policy alone leaves grouped treasure with none.
    let e = errors(policy, "policy");
    assert!(
        e.iter().any(|x| x.starts_with(
            "loot-policy: the world has none, so its loot is ACE's, and still has 1 grouped-treasure"
        )),
        "{e:?}"
    );
    // The removals alone, under the source's loot: and the drudge's treasure is now nothing.
    let e = errors(grouped, "grouped");
    assert!(
        e.iter()
            .any(|x| x.starts_with("grouped-treasure 99: removed while the loot is the source's")),
        "{e:?}"
    );
    assert!(
        e.contains(&"weenie 13 did.DeathTreasureType: treasure type 99 is in no death-treasure or grouped-treasure".to_owned()),
        "{e:?}"
    );
    // The new rows alone: a type both ACE's death treasure and the source's grouped treasure.
    let e = errors(&death, "death");
    assert_eq!(
        e,
        ["death-treasure 99 and grouped-treasure 99: a treasure type is ACE's death treasure or the source's grouped treasure, not both"]
    );
    // The removals and the new rows, the policy kept.
    let e = errors(&format!("{grouped}\n{death}"), "kept");
    assert!(
        e.iter()
            .any(|x| x.starts_with("grouped-treasure 99: removed while the loot is the source's")),
        "{e:?}"
    );
    // The policy and the removals, no new rows.
    let e = errors(both, "norows");
    assert!(
        e.iter()
            .any(|x| x.starts_with("loot-policy: the swap to ACE's loot adds no death-treasure")),
        "{e:?}"
    );
    // A build of a half-made swap is refused; of the whole swap, it is ACE's loot.
    let dir = Scratch::new("swap-build");
    let path = world(
        &dir.0,
        &source_loot(),
        &[("1999-12-sudden-season", Some(policy))],
    );
    let e = content(&path, None, None, &[]).expect_err("refused");
    assert!(e.contains("loot-policy: the world has none"), "{e}");
    let dir = Scratch::new("swap-built");
    let path = world(
        &dir.0,
        &source_loot(),
        &[("1999-12-sudden-season", Some(&format!("{both}\n{death}")))],
    );
    let c = content(&path, None, None, &[]).expect("the swap builds");
    assert!(c.loot_policies.is_empty() && c.grouped_treasure.is_empty());
    assert_eq!(c.treasure_death.len(), 1);
    pack_of(&c).expect("a pack");
}

#[test]
fn a_checkpoint_whose_source_loot_is_aces_is_the_swap_and_is_reported_not_an_error() {
    use empyrean_world_db::layers::{snapshot_of, CheckpointLayer, Folding, Layers, PatchLayer};
    let rows = |c: &WorldContent| {
        let mut files = empyrean_world_db::write(c, default_stamp()).expect("written");
        set_source(&mut files, "test-world", "r1").expect("a source");
        files
    };
    let base = source_loot();
    // The checkpoint's source: ACE's loot, its drudge's treasure ACE's type 5.
    let mut later = source_loot();
    later.loot_policies.clear();
    later.grouped_treasure.clear();
    later.treasure_death = vec![empyrean_content::models::world::TreasureDeath {
        treasure_type: 5,
        tier: 1,
        last_modified: default_stamp(),
        ..Default::default()
    }];
    later.weenies[3].weenie_properties_did[0].value = 5;
    let (snapshot, kinds) = snapshot_of(&rows(&later)).expect("a snapshot");
    let layers = Layers {
        base: rows(&base),
        patches: vec![PatchLayer {
            id: "2005-02-infiltration".into(),
            era: None,
            checkpoint: Some(Box::new(CheckpointLayer {
                snapshot,
                kinds,
                drift: Vec::new(),
            })),
            ..Default::default()
        }],
        ..Default::default()
    };
    let checks = Folding::new(layers, base.era, default_stamp())
        .expect("read")
        .check();
    assert!(checks[1].errors().is_empty(), "{:?}", checks[1].errors());
    assert!(checks[1].inherited.is_empty(), "{:?}", checks[1].inherited);
    assert_eq!(
        checks[1].reports,
        ["the loot becomes ACE's: the source's loot-policy goes, 1 grouped-treasure go and 1 death-treasure come"]
    );
}

/// A data file of `data_set` in `dir`, named `name`, at `iteration`, holding the records `ids`.
fn dat(dir: &Path, name: &str, data_set: u32, iteration: u32, ids: &[u32]) {
    let records: Vec<(u32, Vec<u8>)> = ids.iter().map(|&id| (id, b"a record".to_vec())).collect();
    dat_of(dir, name, data_set, iteration, &records);
}

/// A data file of `data_set` in `dir`, named `name`, at `iteration`, holding `records`.
fn dat_of(dir: &Path, name: &str, data_set: u32, iteration: u32, records: &[(u32, Vec<u8>)]) {
    use dereth_dat::{DatWriter, MemoryFile, ITERATION_LIST};
    use dereth_primitives::DataId;
    std::fs::create_dir_all(dir).expect("a folder");
    // Built in memory and written once, in small blocks: a whole cell file holds the land of
    // every landblock.
    let path = dir.join(name);
    let bytes = MemoryFile::default();
    let mut w = DatWriter::create_in(
        path.clone(),
        Box::new(bytes.clone()),
        0x40,
        data_set,
        0,
        0x400 + 0x40 * 1024,
    )
    .expect("created");
    for (id, record) in records {
        w.save(DataId(*id), record, 1, iteration, 1).expect("saved");
    }
    let list: Vec<u32> = (1..=iteration).collect();
    w.save(
        ITERATION_LIST,
        &dereth_dat::iteration::encode(&list),
        1,
        0,
        1,
    )
    .expect("its iterations");
    drop(w);
    std::fs::write(&path, bytes.bytes()).expect("written");
}

/// `sample()` with its chest drawn by a setup and an icon.
fn drawn() -> WorldContent {
    use empyrean_content::models::world::WeeniePropertiesDID;
    use empyrean_entity::enums::PropertyDataId;
    let mut c = sample();
    c.weenies[0].weenie_properties_did = [
        (PropertyDataId::Setup, 0x0200_0001),
        (PropertyDataId::Icon, 0x0600_0001),
    ]
    .map(|(t, value)| WeeniePropertiesDID {
        r#type: t.0,
        value,
        ..Default::default()
    })
    .to_vec();
    empyrean_world_db::number(&mut c);
    c
}

#[test]
fn a_layer_naming_a_record_its_data_files_lack_fails_and_a_layer_whose_files_are_unknown_says_so() {
    use dereth_dat::container::{CELL_DATFILE, PORTAL_DATFILE};
    let dir = Scratch::new("closure");
    let path = world(
        &dir.0,
        &drawn(),
        &[
            // Names an icon no file of its iteration has.
            (
                "1999-12-sudden-season",
                Some("[[weenie]]\nclass = 12\ndid = { Icon = 0x06000009 }\n"),
            ),
            // Its portal is a later one, without the chest's icon.
            ("2000-01-dark-thaw", None),
            // Its portal isn't among the files given.
            ("2000-02-shadows", None),
            // Names no iteration.
            (
                "2000-03-darkness",
                Some("[[weenie]]\nclass = 11\nint = { Value = 5 }\n"),
            ),
        ],
    );
    append(
        &path,
        "\n[land]\nportal_iteration = 1\ncell_iteration = 1\n",
    );
    let patch = |id: &str| dir.0.join("patches").join(id).join("patch.toml");
    append(
        &patch("1999-12-sudden-season"),
        "status = \"written\"\nportal_iteration = 1\ncell_iteration = 1\n",
    );
    append(
        &patch("2000-01-dark-thaw"),
        "status = \"written\"\nportal_iteration = 2\n",
    );
    append(&patch("2000-02-shadows"), "portal_iteration = 3\n");
    let dats = dir.0.join("dats");
    dat(
        &dats.join("first"),
        "client_portal.dat",
        PORTAL_DATFILE,
        1,
        &[0x0200_0001, 0x0600_0001],
    );
    dat(
        &dats.join("first"),
        "client_cell_1.dat",
        CELL_DATFILE,
        1,
        &[0xA9B4_FFFF],
    );
    dat(
        &dats.join("later"),
        "client_portal.dat",
        PORTAL_DATFILE,
        2,
        &[0x0200_0001, 0x0600_0009],
    );
    let given = [dats.clone()];
    let checks = super::open(&path, None, &given).expect("opened").check();
    assert!(
        checks[0].errors().is_empty() && checks[0].inherited.is_empty(),
        "{checks:?}"
    );
    assert_eq!(
        checks[1].errors(),
        ["portal 0x06000009: in no portal file of iteration 1 (named by weenie 12 did.Icon)"]
    );
    // What the files below held and the layer's lack is the layer's; what they lacked was said.
    assert_eq!(
        checks[2].errors(),
        ["portal 0x06000001: in no portal file of iteration 2 (named by weenie 10 did.Icon)"]
    );
    assert_eq!(
        checks[2].reports,
        ["closure of cell records not checked: the layer names no cell iteration"]
    );
    assert!(checks[3].errors().is_empty(), "{:?}", checks[3].errors());
    assert_eq!(
        checks[3].reports,
        [
            "closure of portal records not checked: no portal file of iteration 3 among the data files given",
            "closure of cell records not checked: the layer names no cell iteration",
        ]
    );
    assert_eq!(
        checks[4].reports,
        ["closure not checked: the layer names none of its data files' iterations"]
    );
    let (lines, errors) = check_lines(super::open(&path, None, &given).expect("opened"));
    assert_eq!(errors, 2, "{lines:?}");
    // Without data files, closure isn't checked; a build with them is refused.
    let (lines, errors) = check_lines(super::open(&path, None, &[]).expect("opened"));
    assert_eq!(errors, 0, "{lines:?}");
    content(&path, None, None, &[]).expect("builds");
    let e = content(&path, None, None, &given).expect_err("refused");
    assert!(e.contains("2000-01-dark-thaw: portal 0x06000001"), "{e}");
}

#[test]
fn a_record_the_world_below_named_where_no_files_were_checked_is_a_warning_not_the_layers_error() {
    use dereth_dat::container::PORTAL_DATFILE;
    let dir = Scratch::new("closure-below");
    let path = world(&dir.0, &drawn(), &[("1999-12-sudden-season", None)]);
    append(
        &dir.0
            .join("patches")
            .join("1999-12-sudden-season")
            .join("patch.toml"),
        "portal_iteration = 1\n",
    );
    let dats = dir.0.join("dats");
    dat(
        &dats,
        "client_portal.dat",
        PORTAL_DATFILE,
        1,
        &[0x0200_0001],
    );
    let checks = super::open(&path, None, &[dats]).expect("opened").check();
    assert!(checks[1].errors().is_empty(), "{:?}", checks[1].errors());
    assert_eq!(
        checks[1].warnings,
        ["portal 0x06000001: in no portal file of iteration 1 (named by weenie 10 did.Icon); named below, where no files were checked for it"]
    );
    assert_eq!(
        checks[0].reports,
        ["closure not checked: the layer names none of its data files' iterations"]
    );
}

/// A cell file in `dir` at `iteration` holding `records`; whole, with the land of every landblock
/// besides, or partial.
fn cell_file(dir: &Path, iteration: u32, whole: bool, mut records: Vec<(u32, Vec<u8>)>) {
    use dereth_dat::container::CELL_DATFILE;
    if whole {
        for x in 0..255u32 {
            for y in 0..255u32 {
                let land = (x << 24) | (y << 16) | 0xFFFF;
                if !records.iter().any(|(id, _)| *id == land) {
                    records.push((land, b"land".to_vec()));
                }
            }
        }
    }
    dat_of(dir, "client_cell_1.dat", CELL_DATFILE, iteration, &records);
}

/// The landblock info `0xLLLLFFFE` claiming `rooms` rooms.
fn info(landblock: u32, rooms: u32) -> (u32, Vec<u8>) {
    use dereth_assets::LandblockInfo;
    use dereth_primitives::DataId;
    let id = (landblock << 16) | 0xFFFE;
    let i = LandblockInfo {
        id: DataId(id),
        num_cells: rooms,
        objects: Vec::new(),
        pack_mask: 0,
        buildings: Vec::new(),
        restrictions: None,
    };
    (id, i.encode())
}

/// The room `id`, cell 0 of the environment `0x0D000001`, standing at `origin`.
fn room(id: u32, origin: [f32; 3]) -> (u32, Vec<u8>) {
    use dereth_assets::EnvCell;
    use dereth_primitives::{DataId, Frame, Quat, Vec3};
    let c = EnvCell {
        id: DataId(id),
        flags: 0,
        cell_id_repeat: id,
        surfaces: Vec::new(),
        environment: DataId(0x0D00_0001),
        cell_struct: 0,
        frame: Frame::new(Vec3::new(origin[0], origin[1], origin[2]), Quat::IDENTITY),
        portals: Vec::new(),
        visible_cells: Vec::new(),
        static_objects: Vec::new(),
        restriction_obj: None,
    };
    (id, c.encode())
}

/// The environment `0x0D000001`: one cell, a box from the origin to `size` on each axis, its walls
/// facing in.
fn environment(size: [f32; 3]) -> (u32, Vec<u8>) {
    use dereth_assets::geometry::CellStruct;
    use dereth_assets::{BspNode, BspTree, Environment, Plane, VertexArray};
    use dereth_primitives::{DataId, Vec3};
    let walls = [
        (Vec3::new(1.0, 0.0, 0.0), 0.0),
        (Vec3::new(-1.0, 0.0, 0.0), size[0]),
        (Vec3::new(0.0, 1.0, 0.0), 0.0),
        (Vec3::new(0.0, -1.0, 0.0), size[1]),
        (Vec3::new(0.0, 0.0, 1.0), 0.0),
        (Vec3::new(0.0, 0.0, -1.0), size[2]),
    ];
    let last = walls.len() - 1;
    let nodes = walls
        .iter()
        .enumerate()
        .map(|(i, (normal, d))| BspNode {
            // A chain of front-only nodes, the last childless.
            tag: if i == last { 0x4250_464C } else { 0x4250_6E6E },
            plane: Some(Plane {
                normal: *normal,
                d: *d,
            }),
            pos_child: (i != last).then(|| u32::try_from(i + 1).expect("small")),
            ..BspNode::default()
        })
        .collect();
    let e = Environment {
        id: DataId(0x0D00_0001),
        cells: vec![CellStruct {
            cellstruct_id: 0,
            vertex_array: VertexArray {
                vertex_type: 1,
                vertices: Vec::new(),
            },
            polygons: Vec::new(),
            portals: Vec::new(),
            cell_bsp: BspTree { nodes },
            physics_polygons: Vec::new(),
            physics_bsp: BspTree {
                nodes: vec![BspNode {
                    tag: 0x4C45_4146,
                    leaf_index: Some(0),
                    solid: Some(0),
                    sphere: Some(dereth_assets::Sphere::default()),
                    ..BspNode::default()
                }],
            },
            drawing_bsp: None,
        }],
    };
    (0x0D00_0001, e.encode())
}

/// A placement of `class` in `cell` at `at`.
fn placed(guid: u32, class: u32, cell: u32, at: [f32; 3]) -> LandblockInstance {
    LandblockInstance {
        guid,
        weenie_class_id: class,
        obj_cell_id: cell,
        origin_x: at[0],
        origin_y: at[1],
        origin_z: at[2],
        angles_w: 1.0,
        last_modified: default_stamp(),
        ..Default::default()
    }
}

/// A portal of `class` sending its user to `cell` at `at`.
fn portal_to(class: u32, cell: u32, at: [f32; 3]) -> Weenie {
    use empyrean_content::models::world::WeeniePropertiesPosition;
    let mut w = weenie(class, &format!("portal-{class}"));
    w.r#type = 7;
    w.weenie_properties_position = vec![WeeniePropertiesPosition {
        position_type: 2,
        obj_cell_id: cell,
        origin_x: at[0],
        origin_y: at[1],
        origin_z: at[2],
        angles_w: 1.0,
        ..Default::default()
    }];
    w
}

#[test]
fn a_room_is_there_by_its_landblocks_claimed_count_and_a_partial_files_whole_records_date_a_change()
{
    let dir = Scratch::new("rooms-claimed");
    let mut c = sample();
    c.landblock_instances
        .push(placed(0x7A9B_4002, 11, 0xA9B4_0101, [5.0, 5.0, 1.0]));
    empyrean_world_db::number(&mut c);
    let path = world(
        &dir.0,
        &c,
        &[
            ("1999-12-sudden-season", None),
            ("2000-01-dark-thaw", None),
            ("2000-02-shadows", None),
        ],
    );
    append(&path, "\n[land]\ncell_iteration = 1\n");
    let patch = |id: &str| dir.0.join("patches").join(id).join("patch.toml");
    for (id, n) in [
        ("1999-12-sudden-season", 2),
        ("2000-01-dark-thaw", 3),
        ("2000-02-shadows", 4),
    ] {
        append(
            &patch(id),
            &format!("status = \"written\"\ncell_iteration = {n}\n"),
        );
    }
    let dats = dir.0.join("dats");
    // Whole: the landblock claims two rooms, and holds the record of neither.
    cell_file(&dats.join("1"), 1, true, vec![info(0xA9B4, 2)]);
    // Partial, and nothing of the landblock in it: it can't say.
    cell_file(
        &dats.join("2"),
        2,
        false,
        vec![(0x0101_FFFF, b"land".to_vec())],
    );
    // Partial, but its landblock info is whole: one room now.
    cell_file(
        &dats.join("3"),
        3,
        false,
        vec![info(0xA9B4, 1), (0xA9B4_FFFF, b"land".to_vec())],
    );
    // Whole, and the room still gone; a partial file of the same iteration, found first, holds
    // fewer records and doesn't serve.
    cell_file(
        &dats.join("0"),
        4,
        false,
        vec![(0xA9B4_FFFF, b"land".to_vec())],
    );
    cell_file(&dats.join("4"), 4, true, vec![info(0xA9B4, 1)]);
    let checks = super::open(&path, None, std::slice::from_ref(&dats))
        .expect("opened")
        .check();
    assert!(
        checks[0].errors().is_empty() && checks[0].inherited.is_empty(),
        "{:?}",
        checks[0]
    );
    assert!(checks[1].errors().is_empty(), "{:?}", checks[1].errors());
    assert_eq!(
        checks[1].reports,
        ["2 cell records the partial cell file of iteration 2 can't say of: what the files below said of them stands"]
    );
    assert_eq!(
        checks[2].errors(),
        ["cell 0xA9B40101: not among the 1 rooms landblock A9B4's info claims in the cell file of iteration 3 (named by spawn 0x7A9B4002)"]
    );
    // Said where it happened, not again.
    assert!(checks[3].errors().is_empty(), "{:?}", checks[3].errors());
    assert!(checks[3].reports.is_empty(), "{:?}", checks[3].reports);

    // Until the layer is written, what its files find is a warning.
    let p = patch("2000-01-dark-thaw");
    let text = std::fs::read_to_string(&p).expect("read");
    std::fs::write(&p, text.replace("status = \"written\"\n", "")).expect("written");
    let checks = super::open(&path, None, std::slice::from_ref(&dats))
        .expect("opened")
        .check();
    assert!(checks[2].errors().is_empty(), "{:?}", checks[2].errors());
    assert_eq!(
        checks[2].warnings,
        ["cell 0xA9B40101: not among the 1 rooms landblock A9B4's info claims in the cell file of iteration 3 (named by spawn 0x7A9B4002); not an error until the layer is written"]
    );
    content(&path, None, None, &[dats]).expect("builds");
}

#[test]
fn a_placed_portals_destination_outside_its_room_after_the_room_changes_is_the_layers_error() {
    use dereth_dat::container::PORTAL_DATFILE;
    let dir = Scratch::new("rooms-changed");
    let mut c = sample();
    // A portal placed outdoors, one nothing places, and an apple, all at one point in room 0100;
    // and two portals nothing places sending their users to room 0101.
    c.weenies.push(portal_to(20, 0xA9B4_0100, [5.0, 5.0, 1.0]));
    c.weenies.push(portal_to(21, 0xA9B4_0100, [5.0, 5.0, 1.0]));
    c.weenies.push(portal_to(22, 0xA9B4_0101, [5.0, 5.0, 1.0]));
    c.weenies.push(portal_to(23, 0xA9B4_0101, [5.0, 5.0, 1.0]));
    c.landblock_instances
        .push(placed(0x7A9B_4002, 20, 0xA9B4_0021, [20.0, 20.0, 94.0]));
    c.landblock_instances
        .push(placed(0x7A9B_4003, 11, 0xA9B4_0100, [5.0, 5.0, 1.0]));
    empyrean_world_db::number(&mut c);
    let path = world(
        &dir.0,
        &c,
        &[("1999-12-sudden-season", None), ("2000-01-dark-thaw", None)],
    );
    append(
        &path,
        "\n[land]\nportal_iteration = 1\ncell_iteration = 1\n",
    );
    let patch = |id: &str| dir.0.join("patches").join(id).join("patch.toml");
    append(
        &patch("1999-12-sudden-season"),
        "status = \"written\"\ncell_iteration = 2\n",
    );
    append(
        &patch("2000-01-dark-thaw"),
        "status = \"written\"\nportal_iteration = 2\n",
    );
    let dats = dir.0.join("dats");
    let portal = |n: u32, size: [f32; 3]| {
        let mut records = vec![environment(size)];
        records.extend([0x0200_0001, 0x0600_0001].map(|id| (id, b"a record".to_vec())));
        dat_of(
            &dats.join(format!("p{n}")),
            "client_portal.dat",
            PORTAL_DATFILE,
            n,
            &records,
        );
    };
    portal(1, [10.0, 10.0, 10.0]);
    // The environment grows a hundred meters along x.
    portal(2, [200.0, 10.0, 10.0]);
    cell_file(
        &dats.join("c1"),
        1,
        true,
        vec![
            info(0xA9B4, 2),
            room(0xA9B4_0100, [0.0, 0.0, 0.0]),
            room(0xA9B4_0101, [0.0, 0.0, 0.0]),
        ],
    );
    // The room moves a hundred meters along x, and the second room is gone.
    cell_file(
        &dats.join("c2"),
        2,
        true,
        vec![info(0xA9B4, 1), room(0xA9B4_0100, [-100.0, 0.0, 0.0])],
    );
    let checks = super::open(&path, None, std::slice::from_ref(&dats))
        .expect("opened")
        .check();
    assert!(
        checks[0].errors().is_empty() && checks[0].inherited.is_empty(),
        "{:?}",
        checks[0]
    );
    let between = "between cell 1 with portal 1 and cell 2 with portal 1";
    assert_eq!(
        checks[1].errors(),
        [format!("room 0xA9B40100 changed {between} (it moved 100.00 m): weenie 20 position.Destination, a placed portal's destination, is outside it now; it was in it before")]
    );
    assert_eq!(
        checks[1].warnings,
        [
            "cell 0xA9B40101: not among the 1 rooms landblock A9B4's info claims in the cell file of iteration 2 (named by weenie 22 position.Destination and 1 more); named only by the destination of a portal nothing places".to_owned(),
            format!("room 0xA9B40100 changed {between} (it moved 100.00 m): weenie 21 position.Destination, the destination of a portal nothing places, is outside it now; it was in it before"),
        ]
    );
    assert!(
        checks[1].reports.contains(&format!(
            "landblock A9B4: 1 of the rooms the world names changed {between} (1 moved); 3 points in them, 3 of them outside their room now"
        )),
        "{:?}",
        checks[1].reports
    );
    // The room's environment grows to hold the points again: it changed, and they're in it.
    assert!(checks[2].errors().is_empty(), "{:?}", checks[2].errors());
    assert_eq!(
        checks[2].reports,
        [
            "closure of cell records not checked: the layer names no cell iteration",
            "landblock A9B4: 1 of the rooms the world names changed between cell 2 with portal 1 and cell 2 with portal 2 (1 changed shape); 3 points in them, 0 of them outside their room now",
        ]
    );
    let e = content(&path, None, None, &[dats]).expect_err("refused");
    assert!(e.contains("1999-12-sudden-season: room 0xA9B40100"), "{e}");
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
            let c = content(&path, Some(id), None, &[]).expect("builds");
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
        let c = content(&path, Some("base"), None, &[]).expect("the base");
        let found = differences(&original, &pack_from_sql(&c)).expect("compared");
        assert!(
            found.is_empty(),
            "{} differences, first {:?}",
            found.len(),
            found.first()
        );
    }
}

/// A world's key set, as the world repositories' files set it.
fn keyed(path: &Path) {
    let text = std::fs::read_to_string(path).expect("world.toml");
    std::fs::write(
        path,
        text.replacen(
            "format_version = 1\n",
            "format_version = 1\nkey = \"test-world\"\n",
            1,
        ),
    )
    .expect("written");
}

/// A release of the world at `path` as `release`, after `previous`, into `out`.
fn released(
    path: &Path,
    release: &str,
    previous: Option<&super::release::Manifest>,
    out: &Path,
) -> Result<super::release::Manifest, String> {
    let open = || super::open(path, None, &[]);
    super::release::release(&open, "test-world", release, previous, out)
}

fn file_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("the folder")
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    names.sort();
    names
}

/// A release packs each written layer whose world isn't packed already, and its manifest names
/// every layer with its world's dataset id and the pack holding that world: a layer that changes
/// nothing names the pack of the world below it, a world no written layer has names none, and the
/// next release names the earlier release's packs and packs only what is new. Each pack is the
/// one `world build` makes at its layer.
#[test]
fn a_release_packs_the_written_layers_once_and_its_manifest_names_every_layers_world() {
    use super::release::Manifest;
    let dir = Scratch::new("release");
    let path = world(
        &dir.0,
        &sample(),
        &[
            (
                "1999-12-sudden-season",
                Some("[[weenie]]\nclass = 11\nint = { Value = 5 }\n"),
            ),
            ("2000-01-dark-thaw", None),
            (
                "2000-02-shadows",
                Some("[[weenie]]\nclass = 12\nint = { Value = 9 }\n"),
            ),
            ("2000-03-darkness", None),
        ],
    );
    keyed(&path);
    let patch = |id: &str| dir.0.join("patches").join(id).join("patch.toml");
    append(&patch("2000-02-shadows"), "status = \"written\"\n");
    let first = dir.0.join("v0.1.0");
    let m = released(&path, "v0.1.0", None, &first).expect("released");
    assert_eq!(m.world, "test-world");
    assert_eq!(
        m.layers.iter().map(|l| l.id.as_str()).collect::<Vec<_>>(),
        [
            "base",
            "1999-12-sudden-season",
            "2000-01-dark-thaw",
            "2000-02-shadows",
            "2000-03-darkness"
        ]
    );
    let asset = |n: usize| {
        m.layers[n]
            .pack
            .as_ref()
            .map(|p| (p.release.as_str(), p.asset.as_str()))
    };
    assert_eq!(asset(0), Some(("v0.1.0", "base.pack")));
    assert_eq!(asset(1), None, "a world no written layer has");
    assert_eq!(asset(2), None);
    assert_eq!(
        m.layers[2].dataset_id, m.layers[1].dataset_id,
        "nothing changed"
    );
    assert_eq!(asset(3), Some(("v0.1.0", "2000-02-shadows.pack")));
    assert_eq!(
        asset(4),
        asset(3),
        "a layer that changes nothing has the world below it"
    );
    assert_ne!(m.layers[1].dataset_id, m.layers[0].dataset_id);
    assert_eq!(
        file_names(&first),
        [
            "2000-02-shadows.empyrean.sql",
            "2000-02-shadows.pack",
            "2000-02-shadows.world.sql",
            "SHA256SUMS",
            "base.empyrean.sql",
            "base.pack",
            "base.world.sql",
            "manifest.json",
        ]
    );
    let text = std::fs::read_to_string(first.join("manifest.json")).expect("the manifest");
    assert_eq!(Manifest::parse(&text).expect("reads"), m);
    // Each pack is what `world build` makes at its layer: its bytes, its hash and its dataset id.
    for (n, layer) in [(0, "base"), (3, "2000-02-shadows")] {
        let bytes = std::fs::read(first.join(format!("{layer}.pack"))).expect("the pack");
        let built =
            pack_of(&content(&path, Some(layer), None, &[]).expect("builds")).expect("packs");
        assert!(
            bytes == built,
            "{layer}: the pack is what world build makes"
        );
        let p = m.layers[n].pack.as_ref().expect("a pack");
        assert_eq!(
            p.sha256,
            super::release::hex(&<sha2::Sha256 as sha2::Digest>::digest(&bytes))
        );
        let header = Pack::from_bytes(bytes).expect("a pack").header().dataset_id;
        assert_eq!(super::release::hex(&header), m.layers[n].dataset_id);
    }
    let sums = std::fs::read_to_string(first.join("SHA256SUMS")).expect("sums");
    assert_eq!(sums.lines().count(), 7, "{sums}");
    // The next release: the last patch is written with rows of its own. Only its world is new.
    std::fs::write(
        patch("2000-03-darkness"),
        std::fs::read_to_string(patch("2000-03-darkness"))
            .expect("patch.toml")
            .replace("packs = []", "packs = [\"fixes\"]")
            + "status = \"written\"\n",
    )
    .expect("written");
    let fixes = dir
        .0
        .join("patches")
        .join("2000-03-darkness")
        .join("packs")
        .join("fixes");
    std::fs::create_dir_all(&fixes).expect("pack folder");
    std::fs::write(
        fixes.join("pack.toml"),
        "name = \"fixes\"\nversion = \"1\"\n",
    )
    .expect("pack.toml");
    std::fs::write(
        fixes.join("rows.toml"),
        "[[weenie]]\nclass = 12\nint = { Value = 10 }\n",
    )
    .expect("rows");
    let second = dir.0.join("v0.2.0");
    let m2 = released(&path, "v0.2.0", Some(&m), &second).expect("released");
    assert_eq!(
        m2.layers[..4],
        m.layers[..4],
        "the earlier layers name the earlier packs"
    );
    let last = m2.layers[4].pack.as_ref().expect("a pack");
    assert_eq!(
        (last.release.as_str(), last.asset.as_str()),
        ("v0.2.0", "2000-03-darkness.pack")
    );
    assert_eq!(
        file_names(&second),
        [
            "2000-03-darkness.empyrean.sql",
            "2000-03-darkness.pack",
            "2000-03-darkness.world.sql",
            "SHA256SUMS",
            "manifest.json",
        ]
    );
    // A previous manifest of another world, or of this release itself, is refused.
    let mut other = m.clone();
    other.world = "another".into();
    let e = released(&path, "v0.2.1", Some(&other), &dir.0.join("x")).expect_err("refused");
    assert!(e.contains("is of another"), "{e}");
    let e = released(&path, "v0.1.0", Some(&m), &dir.0.join("y")).expect_err("refused");
    assert!(e.contains("v0.1.0 itself"), "{e}");
}

/// A world with an error is not released, and nothing is packed.
#[test]
fn a_world_with_an_error_is_not_released() {
    let dir = Scratch::new("release-refused");
    let path = world(
        &dir.0,
        &sample(),
        &[(
            "1999-12-sudden-season",
            Some("[[remove]]\nitem = \"quest nothing\"\n"),
        )],
    );
    keyed(&path);
    let out = dir.0.join("out");
    let e = released(&path, "v0.1.0", None, &out).expect_err("refused");
    assert!(e.contains("nothing released"), "{e}");
    assert!(!out.join("manifest.json").exists());
    assert!(
        !out.join("base.pack").exists(),
        "packs are made after the check"
    );
}

/// Releases kept in a folder, `<tag>/<asset>`, served as GitHub serves a repository's: an asset at
/// its download address, and the releases API's answer for a tag or the newest, with each manifest's
/// published digest. It counts the downloads.
struct Releases {
    root: PathBuf,
    newest: String,
    downloads: std::cell::RefCell<Vec<String>>,
}

impl crate::fetch_repo::Get for Releases {
    fn json(&self, url: &str) -> Result<String, String> {
        let tag = url
            .rsplit_once("/tags/")
            .map_or(self.newest.clone(), |(_, t)| t.to_owned());
        let manifest = std::fs::read(self.root.join(&tag).join("manifest.json"))
            .map_err(|_| format!("{url} was not found (HTTP 404)"))?;
        Ok(serde_json::json!({
            "tag_name": tag,
            "assets": [{
                "name": "manifest.json",
                "browser_download_url": format!("https://github.com/x/test-world/releases/download/{tag}/manifest.json"),
                "digest": format!("sha256:{}", crate::fetch::sha256_of(&manifest)),
            }],
        })
        .to_string())
    }

    fn bytes(&self, url: &str, _limit: u64) -> Result<Vec<u8>, String> {
        let rest = url
            .strip_prefix("https://github.com/x/test-world/releases/download/")
            .ok_or_else(|| format!("{url} is not this repository's"))?;
        self.downloads.borrow_mut().push(rest.to_owned());
        std::fs::read(self.root.join(rest)).map_err(|_| format!("{url} was not found (HTTP 404)"))
    }
}

/// A line of the repository `x/test-world`, pinned to `tag` and its manifest's `sha256` (empty:
/// the newest release), serving `layer`.
fn line(
    tag: &str,
    sha256: &str,
    layer: &str,
) -> &'static empyrean_common::world_release::WorldRepo {
    let leak = |s: &str| -> &'static str { Box::leak(s.to_owned().into_boxed_str()) };
    Box::leak(Box::new(empyrean_common::world_release::WorldRepo {
        id: "test-world-line",
        repository: "x/test-world",
        pinned_tag: leak(tag),
        pinned_manifest_sha256: leak(sha256),
        layer: leak(layer),
    }))
}

/// A layer's pack fetched from a world repository's releases: the manifest the line pins (or the
/// newest release's, by the digest GitHub publishes), the one pack the layer names, in whichever
/// release holds it, checked against the manifest's SHA-256, dataset id and era, and cached by its
/// dataset id, so a layer of the same world in another release is fetched from the cache. A layer
/// whose world no release packs is refused with how to build it; a pack or a manifest that isn't
/// the one it must be is refused, and nothing is cached.
#[test]
fn a_layers_pack_is_fetched_from_the_release_its_manifest_names_and_cached_by_its_dataset_id() {
    use crate::fetch::Version;
    use crate::fetch_repo::{fetch, RepoOptions};
    let dir = Scratch::new("fetch-repo");
    let path = world(
        &dir.0,
        &sample(),
        &[
            (
                "1999-12-sudden-season",
                Some("[[weenie]]\nclass = 11\nint = { Value = 5 }\n"),
            ),
            ("2000-01-dark-thaw", None),
            (
                "2000-02-shadows",
                Some("[[weenie]]\nclass = 12\nint = { Value = 9 }\n"),
            ),
        ],
    );
    keyed(&path);
    let patch = |id: &str| dir.0.join("patches").join(id).join("patch.toml");
    append(&patch("2000-02-shadows"), "status = \"written\"\n");
    let root = dir.0.join("releases");
    let m1 = released(&path, "v0.1.0", None, &root.join("v0.1.0")).expect("released");
    let m2 = released(&path, "v0.2.0", Some(&m1), &root.join("v0.2.0")).expect("released");
    assert!(
        m2.layers.iter().all(|l| l.era == "launch"),
        "{:?}",
        m2.layers
    );
    let sha = |tag: &str| {
        crate::fetch::sha256_of(&std::fs::read(root.join(tag).join("manifest.json")).expect("read"))
    };
    let net = Releases {
        root: root.clone(),
        newest: "v0.2.0".into(),
        downloads: std::cell::RefCell::new(Vec::new()),
    };
    let cache = dir.0.join("cache");
    let opts =
        |line: &'static empyrean_common::world_release::WorldRepo, layer: &str| RepoOptions {
            line,
            layer: layer.into(),
            version: Version::Pinned,
            dir: Some(cache.clone()),
            out: None,
        };
    // Pinned to v0.2.0, whose base names v0.1.0's pack: the pack comes from v0.1.0.
    let base = fetch(&opts(line("v0.2.0", &sha("v0.2.0"), "base"), "base"), &net).expect("fetched");
    assert_eq!(
        base,
        cache.join(format!("test-world-{}.pack", m2.layers[0].dataset_id))
    );
    assert_eq!(
        std::fs::read(&base).expect("cached"),
        std::fs::read(root.join("v0.1.0").join("base.pack")).expect("released")
    );
    assert_eq!(
        *net.downloads.borrow(),
        ["v0.2.0/manifest.json", "v0.1.0/base.pack"]
    );
    // The newest release, with no pin; its written patch's world, cached once.
    let newest = line("", "", "base");
    let shadows = fetch(&opts(newest, "2000-02-shadows"), &net).expect("fetched");
    assert!(net.downloads.borrow().ends_with(&[
        "v0.2.0/manifest.json".to_owned(),
        "v0.1.0/2000-02-shadows.pack".to_owned()
    ]));
    // The same world at v0.1.0's manifest: the cache serves it, nothing but the manifest is fetched.
    net.downloads.borrow_mut().clear();
    let again = fetch(
        &RepoOptions {
            version: Version::Tag("v0.1.0".into()),
            ..opts(newest, "2000-02-shadows")
        },
        &net,
    )
    .expect("fetched");
    assert_eq!(again, shadows);
    assert_eq!(*net.downloads.borrow(), ["v0.1.0/manifest.json"]);
    // A cached file that isn't the layer's world is refused, and said how to mend.
    let wrong = dir.0.join("wrong");
    std::fs::create_dir_all(&wrong).expect("folder");
    std::fs::copy(
        &shadows,
        wrong.join(format!("test-world-{}.pack", m2.layers[0].dataset_id)),
    )
    .expect("copied");
    let e = fetch(
        &RepoOptions {
            dir: Some(wrong),
            ..opts(newest, "base")
        },
        &net,
    )
    .expect_err("refused");
    assert!(
        e.contains("the cached pack isn't the layer's") && e.contains("is dataset"),
        "{e}"
    );
    // A layer whose world no release packs, and a layer the world doesn't have.
    let e = fetch(&opts(newest, "1999-12-sudden-season"), &net).expect_err("refused");
    assert!(
        e.contains("has no pack in any release") && e.contains("world build"),
        "{e}"
    );
    let e = fetch(&opts(newest, "2099-01-none"), &net).expect_err("refused");
    assert!(e.contains("has no layer") && e.contains("base"), "{e}");
    // A manifest that isn't the pinned one, a manifest of another world, and a pack that isn't
    // the one the manifest hashes: refused, and nothing is cached.
    let e =
        fetch(&opts(line("v0.2.0", &"0".repeat(64), "base"), "base"), &net).expect_err("refused");
    assert!(
        e.contains("does not match the SHA-256 committed with the pin"),
        "{e}"
    );
    let mut other = m2.clone();
    other.world = "another-world".into();
    std::fs::create_dir_all(root.join("v0.3.0")).expect("folder");
    std::fs::write(root.join("v0.3.0").join("manifest.json"), other.to_json()).expect("written");
    let e = fetch(
        &RepoOptions {
            version: Version::Tag("v0.3.0".into()),
            ..opts(newest, "base")
        },
        &net,
    )
    .expect_err("refused");
    assert!(e.contains("is of another-world"), "{e}");
    let fresh = dir.0.join("fresh");
    let mut tampered = std::fs::read(root.join("v0.1.0").join("base.pack")).expect("read");
    let last = tampered.len() - 1;
    tampered[last] ^= 1;
    std::fs::write(root.join("v0.1.0").join("base.pack"), &tampered).expect("written");
    let e = fetch(
        &RepoOptions {
            dir: Some(fresh.clone()),
            ..opts(newest, "base")
        },
        &net,
    )
    .expect_err("refused");
    assert!(
        e.contains("does not match the SHA-256 manifest.json gives"),
        "{e}"
    );
    assert!(
        !fresh.exists() || std::fs::read_dir(&fresh).expect("read").next().is_none(),
        "nothing cached"
    );
}

/// A checkpoint that names an era changes the world's from there on: each layer folds and builds
/// in its own era, its pack records it, and the release's manifest says it, a layer that names
/// none keeping the era of the one below.
#[test]
fn each_layer_builds_in_the_era_its_last_checkpoint_names() {
    use empyrean_common::era::EraId;
    use empyrean_world_db::layers::{snapshot_of, CheckpointLayer, Folding, Layers, PatchLayer};
    let rows = |c: &WorldContent| {
        let mut files = empyrean_world_db::write(c, default_stamp()).expect("written");
        set_source(&mut files, "test-world", "r1").expect("a source");
        files
    };
    // A checkpoint's world: the sample moved along x, and with the apple's value the patch below
    // the second sets (`valued`), so it takes nothing back.
    let checkpoint = |id: &str, era: EraId, x: f32, valued: bool| {
        let mut c = sample();
        c.landblock_instances[0].origin_x = x;
        if valued {
            let apple = c
                .weenies
                .iter_mut()
                .find(|w| w.class_id == 11)
                .expect("the apple");
            apple.weenie_properties_int.push(WeeniePropertiesInt {
                r#type: 19,
                value: 5,
                ..Default::default()
            });
        }
        let (snapshot, kinds) = snapshot_of(&rows(&c)).expect("a snapshot");
        PatchLayer {
            id: id.into(),
            era: Some(era),
            checkpoint: Some(Box::new(CheckpointLayer {
                snapshot,
                kinds,
                drift: Vec::new(),
            })),
            ..Default::default()
        }
    };
    let layers = Layers {
        base: rows(&sample()),
        patches: vec![
            checkpoint("2005-02-infiltration", EraId::Infiltration, 11.0, false),
            PatchLayer {
                id: "2005-03-later".into(),
                era: None,
                packs: vec![(
                    "fixes".into(),
                    BTreeMap::from([(
                        "rows.toml".to_owned(),
                        "[[weenie]]\nclass = 11\nint = { Value = 5 }\n".to_owned(),
                    )]),
                )],
                ..Default::default()
            },
            checkpoint("2014-05-may-patch", EraId::Eor, 12.0, true),
        ],
        ..Default::default()
    };
    let folding = Folding::new(layers, EraId::Launch, default_stamp()).expect("read");
    let want = [
        EraId::Launch,
        EraId::Infiltration,
        EraId::Infiltration,
        EraId::Eor,
    ];
    assert_eq!(folding.eras(), want);
    for (n, era) in want.iter().enumerate() {
        let c = folding.clone().content_at(n).expect("builds");
        assert_eq!(c.era, *era, "layer {n}");
    }
    let dir = Scratch::new("eras");
    let open = || Ok(folding.clone());
    let m = super::release::release(&open, "test-world", "v0.1.0", None, &dir.0).expect("released");
    assert_eq!(
        m.layers.iter().map(|l| l.era.as_str()).collect::<Vec<_>>(),
        ["launch", "infiltration", "infiltration", "eor"]
    );
    for (asset, era) in [
        ("base.pack", EraId::Launch),
        ("2005-02-infiltration.pack", EraId::Infiltration),
        ("2014-05-may-patch.pack", EraId::Eor),
    ] {
        let p = Pack::open(&dir.0.join(asset)).expect("a pack");
        assert_eq!(p.era(), era, "{asset}");
    }
}

/// A world on disk whose checkpoint's `checkpoint.toml` names an era: the checkpoint and the patch
/// above it are in that era, the base in its own; one naming none keeps the era below, and one
/// naming no era there is doesn't read. (The world is read before a checkpoint's dump is: each layer
/// builds in its era as `each_layer_builds_in_the_era_its_last_checkpoint_names` shows.)
#[test]
fn a_checkpoint_that_names_an_era_changes_the_worlds_from_there_on() {
    use empyrean_common::era::EraId;
    use empyrean_common::world_release::SIXTEEN_PY;
    let dir = Scratch::new("checkpoint-era");
    let path = world(
        &dir.0,
        &sample(),
        &[
            ("2005-02-infiltration", None),
            (
                "2005-03-later",
                Some("[[weenie]]\nclass = 11\nint = { Value = 5 }\n"),
            ),
        ],
    );
    let dumps = dir.0.join("dumps");
    std::fs::create_dir_all(&dumps).expect("folder");
    std::fs::write(dumps.join("ACE-World-16PY-db-v0.8.7.sql"), "").expect("written");
    let checkpoint = dir
        .0
        .join("patches")
        .join("2005-02-infiltration")
        .join("checkpoint.toml");
    let pin = |era: &str| {
        std::fs::write(
            &checkpoint,
            format!(
                "world = \"16py\"\nsource = \"ACE-World-16PY\"\nrevision = \"v0.8.7\"\nurl = \"{}\"\nsha256 = \"{}\"\ndataset_id = \"{}\"\n{era}",
                SIXTEEN_PY.download_url("v0.8.7"),
                "0".repeat(64),
                "0".repeat(32),
            ),
        )
        .expect("written");
        empyrean_world_db::world::open(&path, Some(&dumps))
    };
    let f = pin("era = \"infiltration\"\n").expect("read");
    let base = f.eras()[0];
    assert_ne!(base, EraId::Infiltration);
    assert_eq!(f.eras(), [base, EraId::Infiltration, EraId::Infiltration]);
    assert_eq!(pin("").expect("read").eras(), [base, base, base]);
    let e = pin("era = \"medieval\"\n").expect_err("refused");
    assert!(
        e.contains("2005-02-infiltration") && e.contains("is not an era"),
        "{e}"
    );
}

/// A checkpoint layer gives its pin, the pinned dump in the folder named, and its `keys.toml`'s
/// keys; a layer that is no checkpoint gives none, and a layer the world hasn't is an error.
#[test]
fn a_checkpoint_layer_gives_its_dump_and_keys_for_a_parity_check() {
    use empyrean_common::world_release::SIXTEEN_PY;
    let dir = Scratch::new("checkpoint-of");
    let path = world(&dir.0, &sample(), &[("2005-02-infiltration", None)]);
    let dumps = dir.0.join("dumps");
    std::fs::create_dir_all(&dumps).expect("folder");
    std::fs::write(dumps.join("ACE-World-16PY-db-v0.8.7.sql"), "").expect("written");
    let layer = dir.0.join("patches").join("2005-02-infiltration");
    std::fs::write(
        layer.join("checkpoint.toml"),
        format!(
            "world = \"16py\"\nsource = \"ACE-World-16PY\"\nrevision = \"v0.8.7\"\nurl = \"{}\"\nsha256 = \"{}\"\ndataset_id = \"{}\"\n",
            SIXTEEN_PY.download_url("v0.8.7"),
            "0".repeat(64),
            "0".repeat(32),
        ),
    )
    .expect("written");
    let of = |l: &str| empyrean_world_db::world::checkpoint_of(&path, l, Some(&dumps));
    let (pin, dump, keys) = of("2005-02-infiltration")
        .expect("reads")
        .expect("a checkpoint");
    assert_eq!(
        (pin.tag.as_str(), dump),
        ("v0.8.7", dumps.join("ACE-World-16PY-db-v0.8.7.sql"))
    );
    assert!(keys.is_none());
    std::fs::write(
        layer.join("keys.toml"),
        "format = 1\nmatched = { same = 0, near = 0, new_kept = 0, new_renumbered = 1 }\nkeys = [\n  [0x70001000, 0x70001FFF],\n]\n",
    )
    .expect("written");
    let (_, _, keys) = of("2005-02-infiltration")
        .expect("reads")
        .expect("a checkpoint");
    assert_eq!(
        keys.expect("keys").map.into_iter().collect::<Vec<_>>(),
        [(0x7000_1000, 0x7000_1FFF)]
    );
    assert!(of("base").expect("reads").is_none());
    assert!(of("2006-01-nowhere")
        .expect_err("no layer")
        .contains("no layer"));
}
