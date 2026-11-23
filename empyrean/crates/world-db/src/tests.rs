//! Behaviour: none (tooling: a world's rows, written and read back into the world they came from)
//! A world written as rows and read back is the same world, its ids numbered by rule; the same
//! world always writes the same files. Fixture: small worlds built in memory; with
//! `launch-content`, the operator's Launch pack named by `EMPYREAN_TEST_LAUNCH_FIXTURE`.

use std::collections::BTreeMap;

use empyrean_content::models::world::{
    Encounter, LandblockInstance, LandblockInstanceLink, Quest, TreasureWielded, Weenie,
    WeeniePropertiesCreateList, WeeniePropertiesDID, WeeniePropertiesEmote,
    WeeniePropertiesEmoteAction, WeeniePropertiesFloat, WeeniePropertiesInt,
    WeeniePropertiesPosition, WeeniePropertiesSpellBook, WeeniePropertiesString,
};
use empyrean_content::WorldContent;

use crate::files::default_stamp;
use crate::{number, world, write};

fn sample() -> WorldContent {
    let stamp = default_stamp();
    WorldContent {
        weenies: vec![Weenie {
            class_id: 509,
            class_name: "lifestone".into(),
            r#type: 25,
            last_modified: stamp,
            weenie_properties_int: vec![
                WeeniePropertiesInt {
                    id: 9,
                    object_id: 509,
                    r#type: 93,
                    value: 1040,
                },
                WeeniePropertiesInt {
                    id: 4,
                    object_id: 509,
                    r#type: 1,
                    value: 268_435_456,
                },
                WeeniePropertiesInt {
                    id: 5,
                    object_id: 509,
                    r#type: 9999,
                    value: -3,
                },
            ],
            weenie_properties_float: vec![WeeniePropertiesFloat {
                id: 2,
                object_id: 509,
                r#type: 54,
                value: 2.0,
            }],
            weenie_properties_did: vec![WeeniePropertiesDID {
                id: 7,
                object_id: 509,
                r#type: 1,
                value: 0x0200_02EE,
            }],
            weenie_properties_string: vec![WeeniePropertiesString {
                id: 3,
                object_id: 509,
                r#type: 1,
                value: "Life \"Stone\"\nsecond line".into(),
            }],
            weenie_properties_position: vec![WeeniePropertiesPosition {
                id: 1,
                object_id: 509,
                position_type: 1,
                obj_cell_id: 0xA9B4_0011,
                origin_x: 0.17,
                origin_y: -12.5,
                origin_z: 94.0,
                angles_w: 0.746_783,
                angles_x: 0.0,
                angles_y: 0.0,
                angles_z: -0.665_067,
            }],
            weenie_properties_spell_book: vec![
                WeeniePropertiesSpellBook {
                    id: 1,
                    object_id: 509,
                    spell: 1000,
                    probability: 2.0,
                },
                WeeniePropertiesSpellBook {
                    id: 2,
                    object_id: 509,
                    spell: 47,
                    probability: 0.5,
                },
            ],
            weenie_properties_create_list: vec![
                WeeniePropertiesCreateList {
                    id: 8,
                    object_id: 509,
                    destination_type: 9,
                    weenie_class_id: 3703,
                    shade: 0.15,
                    ..Default::default()
                },
                WeeniePropertiesCreateList {
                    id: 6,
                    object_id: 509,
                    destination_type: 9,
                    ..Default::default()
                },
            ],
            weenie_properties_emote: vec![WeeniePropertiesEmote {
                id: 11,
                object_id: 509,
                category: 7,
                probability: 1.0,
                quest: Some(String::new()),
                weenie_properties_emote_action: vec![
                    WeeniePropertiesEmoteAction {
                        id: 30,
                        emote_id: 11,
                        r#type: 10,
                        message: Some("Hello".into()),
                        ..Default::default()
                    },
                    WeeniePropertiesEmoteAction {
                        id: 31,
                        emote_id: 11,
                        r#type: 5,
                        order: 1,
                        motion: Some(0x1300_0085),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }],
            ..Weenie::default()
        }],
        landblock_instances: vec![LandblockInstance {
            guid: 0x7A9B_400A,
            weenie_class_id: 509,
            obj_cell_id: 0xA9B4_0121,
            origin_x: 100.56,
            angles_w: 1.0,
            last_modified: stamp,
            landblock_instance_link: vec![LandblockInstanceLink {
                id: 3,
                parent_guid: 0x7A9B_400A,
                child_guid: 0x7A9B_400B,
                last_modified: stamp,
            }],
            ..Default::default()
        }],
        encounters: vec![Encounter {
            id: 5,
            landblock: 0xA9A0,
            weenie_class_id: 61023,
            cell_x: 2,
            cell_y: 0,
            last_modified: stamp,
        }],
        quests: vec![
            Quest {
                id: 4,
                name: "Pickup/Drop: Skull".into(),
                max_solves: -1,
                min_delta: 72000,
                message: Some("A flag".into()),
                last_modified: stamp,
            },
            Quest {
                id: 2,
                name: "Pickup:Drop/ Skull".into(),
                max_solves: 1,
                last_modified: stamp,
                ..Default::default()
            },
        ],
        treasure_wielded: vec![
            TreasureWielded {
                id: 10,
                treasure_type: 364,
                weenie_class_id: 2606,
                probability: 0.4,
                set_start: true,
                last_modified: stamp,
                ..Default::default()
            },
            TreasureWielded {
                id: 11,
                treasure_type: 364,
                weenie_class_id: 115,
                probability: 0.3,
                last_modified: stamp,
                ..Default::default()
            },
        ],
        ..WorldContent::default()
    }
}

/// Every table of two worlds, compared one by one.
fn same(a: &WorldContent, b: &WorldContent) {
    assert_eq!(a.weenies, b.weenies);
    assert_eq!(a.landblock_instances, b.landblock_instances);
    assert_eq!(a.encounters, b.encounters);
    assert_eq!(a.cook_books, b.cook_books);
    assert_eq!(a.recipes, b.recipes);
    assert_eq!(a.recipe_vital_requirements, b.recipe_vital_requirements);
    assert_eq!(a.events, b.events);
    assert_eq!(a.house_portals, b.house_portals);
    assert_eq!(a.points_of_interest, b.points_of_interest);
    assert_eq!(a.quests, b.quests);
    assert_eq!(a.spells, b.spells);
    assert_eq!(a.treasure_death, b.treasure_death);
    assert_eq!(a.grouped_treasure, b.grouped_treasure);
    assert_eq!(a.loot_policies, b.loot_policies);
    assert_eq!(a.treasure_gem_count, b.treasure_gem_count);
    assert_eq!(a.treasure_material_base, b.treasure_material_base);
    assert_eq!(a.treasure_material_color, b.treasure_material_color);
    assert_eq!(a.treasure_material_groups, b.treasure_material_groups);
    assert_eq!(a.treasure_wielded, b.treasure_wielded);
    assert_eq!(a.versions, b.versions);
    assert_eq!(a.era, b.era);
}

#[test]
fn a_world_written_as_rows_reads_back_as_the_same_world_numbered_by_rule() {
    let mut original = sample();
    number(&mut original);
    let files = write(&original, default_stamp()).expect("written");
    let back = world(&files).expect("read back");
    same(&original, &back);
    // Written again, the same bytes.
    assert_eq!(files, write(&back, default_stamp()).expect("written again"));
}

#[test]
fn a_weenie_is_one_file_by_its_class_with_its_properties_by_their_type_names() {
    let mut c = sample();
    number(&mut c);
    let files = write(&c, default_stamp()).expect("written");
    let w = &files["weenie/00509.toml"];
    for want in [
        "[[weenie]]\nclass = 509\nclass_name = \"lifestone\"\ntype = \"LifeStone\"\n",
        "[weenie.int]\nItemType = 268435456\n",
        "PhysicsState = 1040\n",
        "9999 = -3\n",
        "[weenie.did]\nSetup = 0x020002EE\n",
        "Name = \"Life \\\"Stone\\\"\\nsecond line\"",
        "Location = { cell = 0xA9B40011, at = [0.17, -12.5, 94.0], rotation = [0.746783, 0.0, 0.0, -0.665067] }",
        "[weenie.spell_book]\n47 = 0.5\n1000 = 2.0\n",
        "[[weenie.emote]]\ncategory = \"Use\"",
        "[[weenie.emote.action]]\ntype = \"Tell\"\nmessage = \"Hello\"\n",
        "motion = 0x13000085",
    ] {
        assert!(w.contains(want), "{want:?} in\n{w}");
    }
    // No ids, no stamp, no field at its default.
    assert!(
        !w.contains("object_id") && !w.contains("last_modified") && !w.contains("try_to_bond"),
        "{w}"
    );
}

#[test]
fn placements_encounters_and_named_rows_go_by_their_keys() {
    let mut c = sample();
    number(&mut c);
    let files = write(&c, default_stamp()).expect("written");
    let names: Vec<&str> = files.keys().map(String::as_str).collect();
    assert_eq!(
        names,
        [
            "encounter/A9xx.toml",
            "landblock/A9B4.toml",
            "quest/Pickup_Drop__Skull.toml",
            "treasure_wielded/00364.toml",
            "weenie/00509.toml",
            "world-db.toml",
        ]
    );
    assert_eq!(
        files["landblock/A9B4.toml"],
        "[[placement]]\nguid = 0x7A9B400A\nclass = 509\ncell = 0xA9B40121\nat = [100.56, 0.0, 0.0]\nrotation = [1.0, 0.0, 0.0, 0.0]\nlinks = [0x7A9B400B]\n"
    );
    assert_eq!(
        files["encounter/A9xx.toml"],
        "[[encounter]]\nlandblock = 0xA9A0\ncell = [2, 0]\nclass = 61023\n"
    );
    // Names a file name spells alike share a file.
    assert_eq!(
        files["quest/Pickup_Drop__Skull.toml"]
            .matches("[[quest]]")
            .count(),
        2
    );
    let wielded = &files["treasure_wielded/00364.toml"];
    assert!(
        wielded.find("class_id = 2606").unwrap_or(usize::MAX)
            < wielded.find("class_id = 115").unwrap_or(0),
        "{wielded}"
    );
}

#[test]
fn numbering_by_rule_ignores_the_ids_and_order_a_dump_gave() {
    let mut a = sample();
    let mut b = sample();
    b.weenies[0].weenie_properties_int.reverse();
    b.quests.reverse();
    for (i, p) in b.weenies[0].weenie_properties_int.iter_mut().enumerate() {
        p.id = 100 + u32::try_from(i).expect("small");
    }
    number(&mut a);
    number(&mut b);
    same(&a, &b);
    assert_eq!(
        a.weenies[0]
            .weenie_properties_int
            .iter()
            .map(|p| (p.id, p.r#type))
            .collect::<Vec<_>>(),
        [(1, 1), (2, 93), (3, 9999)]
    );
}

#[test]
fn two_rows_of_one_item_are_refused_names_matching_without_case() {
    let mut c = sample();
    c.quests[1].name = "pickup/drop: skull".into();
    let e = world(&write(&c, default_stamp()).expect("written")).expect_err("one quest twice");
    assert!(e.contains("quest pickup/drop: skull is given twice"), "{e}");
}

#[test]
fn a_patch_layer_sets_unsets_and_removes_over_its_base_field_by_field() {
    use crate::layers::{fold_layers, Layers, PatchLayer};
    let mut c = sample();
    number(&mut c);
    let mut base = write(&c, default_stamp()).expect("written");
    base.remove(crate::files::META);
    let patch = r#"
[[weenie]]
class = 509
int = { ItemType = 1, Value = 25 }
remove_fields = ["int.PhysicsState", "create_list"]

[[quest]]
name = "PICKUP/DROP: SKULL"
max_solves = 3

[[remove]]
items = ["spawn 0x7a9b400a", "wielded-treasure 364"]
"#;
    let layers = Layers {
        base,
        patches: vec![PatchLayer {
            id: "1999-12-dark-thaw".into(),
            packs: vec![(
                "fixes".into(),
                BTreeMap::from([("fixes.toml".into(), patch.into())]),
            )],
            overrides: Vec::new(),
        }],
        ..Default::default()
    };
    let mut got = fold_layers(&layers, c.era, default_stamp()).expect("folded");
    number(&mut got);
    let mut want = c.clone();
    let w = &mut want.weenies[0];
    w.weenie_properties_int.retain(|p| p.r#type != 93);
    w.weenie_properties_int[0].value = 1;
    w.weenie_properties_int.push(WeeniePropertiesInt {
        r#type: 19,
        value: 25,
        ..Default::default()
    });
    w.weenie_properties_create_list.clear();
    want.quests[0].max_solves = 3;
    want.quests[0].name = "PICKUP/DROP: SKULL".into();
    want.landblock_instances.clear();
    want.treasure_wielded.clear();
    number(&mut want);
    same(&want, &got);
}

#[test]
fn a_patch_that_removes_what_no_layer_has_or_names_no_item_is_refused() {
    use crate::layers::{fold_layers, Layers, PatchLayer};
    let mut base = write(&sample(), default_stamp()).expect("written");
    base.remove(crate::files::META);
    let layers = |text: &str| Layers {
        base: base.clone(),
        patches: vec![PatchLayer {
            id: "1999-12-dark-thaw".into(),
            packs: vec![(
                "fixes".into(),
                BTreeMap::from([("fixes.toml".into(), text.to_owned())]),
            )],
            overrides: Vec::new(),
        }],
        authored_classes: Some((1_000_000, 1_999_999)),
        ..Default::default()
    };
    let e = fold_layers(
        &layers("[[remove]]\nitem = \"spawn banana\"\n"),
        Default::default(),
        default_stamp(),
    )
    .expect_err("no item");
    assert!(e.contains("names no item"), "{e}");
    let e = fold_layers(
        &layers("[[spell]]\nname = \"no id\"\n"),
        Default::default(),
        default_stamp(),
    )
    .expect_err("no id");
    assert!(e.contains("no id"), "{e}");
    // A new weenie takes one of the world's own classes.
    let new = |class: u32| {
        format!("[[weenie]]\nclass = {class}\nclass_name = \"a new thing\"\ntype = \"Generic\"\n")
    };
    let e = fold_layers(&layers(&new(4000)), Default::default(), default_stamp())
        .expect_err("not ours");
    assert!(
        e.contains(
            "weenie 4000: a class the base doesn't have is the world's own, 1000000 to 1999999"
        ),
        "{e}"
    );
    let ok = fold_layers(
        &layers(&new(1_000_000)),
        Default::default(),
        default_stamp(),
    )
    .expect("ours");
    assert!(ok.weenies.iter().any(|w| w.class_id == 1_000_000));
}

#[test]
fn a_world_file_folds_its_base_pack_and_each_patch_in_its_lineage() {
    let dir = std::env::temp_dir().join(format!("world-db-lineage-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut c = sample();
    number(&mut c);
    let mut base = write(&c, default_stamp()).expect("written");
    crate::files::set_source(&mut base, "ac1999", "183e74f8").expect("a source");
    base.insert(
        "pack.toml".into(),
        "name = \"base\"\nversion = \"0.1.0\"\n".into(),
    );
    crate::write_dir(&dir.join("base"), &base).expect("base written");
    let patch = dir.join("patches").join("1999-12-dark-thaw");
    let files = BTreeMap::from([
        (
            "pack.toml".to_owned(),
            "name = \"fixes\"\nversion = \"0.1.0\"\n".to_owned(),
        ),
        (
            "weenie/00509.toml".to_owned(),
            "[[weenie]]\nclass = 509\nint = { Value = 25 }\n".to_owned(),
        ),
    ]);
    crate::write_dir(&patch.join("packs").join("fixes"), &files).expect("patch written");
    std::fs::write(
        patch.join("patch.toml"),
        "id = \"1999-12-dark-thaw\"\ntitle = \"Dark Thaw\"\ndate = \"1999-12-20\"\nafter = \"\"\npacks = [\"fixes\"]\n",
    )
    .expect("patch.toml");
    let world_toml = dir.join("world.toml");
    std::fs::write(
        &world_toml,
        "format_version = 1\ngame = \"ac1\"\npacks = [\"base\"]\nlineage = [\"1999-12-dark-thaw\"]\n[ids]\nclasses = [1000000, 1999999]\n",
    )
    .expect("world.toml");
    let got = crate::world::fold_world(&world_toml);
    let _ = std::fs::remove_dir_all(&dir);
    let mut got = got.expect("folded");
    number(&mut got);
    let mut want = c;
    want.weenies[0]
        .weenie_properties_int
        .push(WeeniePropertiesInt {
            r#type: 19,
            value: 25,
            ..Default::default()
        });
    number(&mut want);
    same(&want, &got);
}

#[test]
fn a_base_row_that_says_nothing_of_its_source_is_the_snapshots() {
    use dereth_pack::provenance::Source;
    let mut c = sample();
    number(&mut c);
    let mut files = write(&c, default_stamp()).expect("written");
    crate::files::set_source(&mut files, "ac1999", "183e74f8").expect("a source");
    // One row says it was written by a person.
    let quest = files
        .get_mut("quest/Pickup_Drop__Skull.toml")
        .expect("the quests");
    *quest = quest.replacen(
        "[[quest]]\n",
        "[[quest]]\nprov = { from = \"authored\" }\n",
        1,
    );
    // A patch's pack says so the same way.
    let patch = BTreeMap::from([
        (
            "pack.toml".to_owned(),
            "name = \"fixes\"\nversion = \"1\"\nprov = { from = \"dat\", record = 0x6000 }\n"
                .to_owned(),
        ),
        (
            "fixes.toml".to_owned(),
            "[[weenie]]\nclass = 509\nint = { Value = 25 }\n".to_owned(),
        ),
    ]);
    let (merge, _) = crate::layers::delta_of(&[("fixes".into(), patch)], &[]).expect("merged");
    assert_eq!(
        merge.change.edits[0].set["int.Value"].prov.source,
        Source::Dat
    );
    let (snapshot, _) = crate::layers::snapshot_of(&files).expect("a snapshot");
    for e in snapshot.iter() {
        let want = if e.item.name == "quest pickup/drop: skull" {
            Source::Authored
        } else {
            Source::Snapshot
        };
        for (name, f) in &e.fields {
            assert_eq!(f.prov.source, want, "{} {name}", e.item.name);
        }
    }
}

#[test]
fn a_file_of_rows_no_kind_lives_in_is_refused() {
    let mut files: BTreeMap<String, String> =
        write(&WorldContent::default(), default_stamp()).expect("written");
    files.insert("weenies/1.toml".into(), String::new());
    let e = world(&files).expect_err("unknown folder");
    assert!(e.contains("weenies/1.toml"), "{e}");
}

#[cfg(feature = "launch-content")]
mod launch {
    //! The operator's Launch pack: its rows, written and read back, rebuild its pack, and that pack
    //! holds every record of the converter's, field for field, its ids aside.
    use super::*;
    use crate::compare::{dataset_of, differences};
    use empyrean_content::pack::Pack;

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    /// The fixture's pack, its bytes checked against the manifest's hash.
    fn pack() -> Pack {
        use sha2::{Digest, Sha256};
        let path = std::env::var_os("EMPYREAN_TEST_LAUNCH_FIXTURE")
            .expect("set EMPYREAN_TEST_LAUNCH_FIXTURE to the hash-pinned fixture manifest");
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).expect("manifest")).expect("json");
        let world = manifest["world_pack"].as_str().expect("world_pack");
        let want = manifest["world_pack_sha256"]
            .as_str()
            .expect("world_pack_sha256");
        let bytes = std::fs::read(world).expect("the Launch pack");
        assert_eq!(
            hex(&Sha256::digest(&bytes)),
            want.to_ascii_lowercase(),
            "fixture hash mismatch: {world}"
        );
        let p = Pack::from_bytes(bytes).expect("the Launch pack");
        assert_eq!(
            p.era(),
            empyrean_common::era::EraId::Launch,
            "never run this fixture with a later pack"
        );
        p
    }

    #[test]
    fn the_launch_worlds_rows_rebuild_its_pack_with_every_record_the_converter_wrote() {
        let original = pack();
        let mut content = crate::from_pack(&original).expect("read");
        number(&mut content);
        let mut files = write(&content, default_stamp()).expect("written");
        crate::files::set_source(&mut files, "ac1999", "183e74f8").expect("a source");
        let back = world(&files).expect("read back");
        same(&content, &back);
        // Two folds of the same rows make the same pack, byte for byte.
        let dataset = dataset_of(&files);
        let (a, _) = back.to_pack(dataset).expect("a pack");
        let (b, _) = world(&files)
            .expect("read again")
            .to_pack(dataset)
            .expect("a pack");
        assert!(a == b, "two folds differ");
        let rebuilt = Pack::from_bytes(a).expect("reads");
        eprintln!(
            "{} files; converter's pack {}, rows' pack {}",
            files.len(),
            hex(&original.header().content_hash),
            hex(&rebuilt.header().content_hash)
        );
        let found = differences(&original, &rebuilt).expect("compared");
        assert!(
            found.is_empty(),
            "{} differences, first {:?}",
            found.len(),
            found.first()
        );
    }
}
