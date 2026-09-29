//! Vectors: tests/fixtures/patches and local GDLE JSON roundtrip cases
//! GDLE spawn maps, quest lists and bulk documents import through empyrean-import --json and
//! round-trip unchanged through ACE's GDLE export; an unknown JSON array is refused.
//! Fixture: tests/fixtures/patches/base.sql and locally constructed edge cases.

use std::path::PathBuf;

use empyrean_content::export::json::{serialize, try_convert_landblock, try_convert_quest};
use empyrean_content::export::sql::SQLWriter;
use empyrean_content::gdle::loader;
use empyrean_content::import::patch::{InputKind, Source};
use empyrean_content::import::{build_from, default_now};
use empyrean_content::models::world::LandblockInstance;
use empyrean_content::pack::Pack;
use empyrean_content::PackContent;

const BASE: &str = include_str!("../../../../crates/content/tests/fixtures/patches/base.sql");

const SPAWNS: &str = r#"{"_version": "1.0", "landblocks": [
  {"key": 2847145984, "value": {
    "weenies": [
      {"id": 1879048193, "wcid": 100, "desc": "a", "pos": {"objcell_id": 2847146015, "frame": {"origin": {"x": 12.5, "y": -0.0, "z": 94.1}, "angles": {"w": 1, "x": 0, "y": 0, "z": 0}}}},
      {"id": 1879048194, "wcid": 200, "pos": {"objcell_id": 2847146016, "frame": {"origin": {"x": 1, "y": 2, "z": 3}, "angles": {"w": 0.70710677, "x": 0, "y": 0, "z": -0.70710677}}}},
      {"id": 1879048195, "wcid": 100, "pos": {"objcell_id": 2847146017, "frame": {"origin": {"x": 4, "y": 5, "z": 6}, "angles": {"w": 1, "x": 0, "y": 0, "z": 0}}}}
    ],
    "links": [{"target": 1879048193, "source": 1879048194}, {"target": 1879048193, "source": 1879048195}]}},
  {"key": 31064064, "value": {
    "weenies": [
      {"id": 1, "wcid": 200, "pos": {"objcell_id": 31064065, "frame": {"origin": {"x": 10, "y": 20, "z": 30}, "angles": {"w": 1, "x": 0, "y": 0, "z": 0}}}}
    ]}}
]}"#;

const QUESTS: &str = r#"[
  {"key": "GdleQuestOne", "value": {"fullname": "Quest one, the first", "maxsolves": 3, "mindelta": 72000}},
  {"key": "GdleQuestTwo", "value": {"fullname": null, "maxsolves": -1, "mindelta": 0}},
  null,
  {"key": "NoValue"}
]"#;

fn json(name: &str, text: &str) -> Source {
    Source {
        kind: InputKind::Json,
        path: PathBuf::from(name),
        bytes: text.as_bytes().to_vec(),
    }
}

fn build(sources: Vec<Source>) -> Result<Pack, String> {
    let (bytes, _) = build_from(BASE.as_bytes(), sources.into_iter().map(Ok), default_now())
        .map_err(|e| e.to_string())?;
    Ok(Pack::from_bytes(bytes).unwrap())
}

fn temp_file(name: &str, text: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "serv-content-8b3-import-{}-{name}",
        std::process::id()
    ));
    std::fs::write(&p, text).unwrap();
    p
}

/// The GDLE landblock JSON ACE's `export-json` writes for one landblock of a pack.
fn export_landblock(db: &PackContent, landblock: u16) -> String {
    let instances = db.get_cached_instances_by_landblock(landblock);
    serialize(&try_convert_landblock(&instances, None, None).unwrap()).unwrap()
}

/// The values the SQL writers write for a float (`TrimNegativeZero` with `0.######`).
fn written(v: f32) -> u32 {
    SQLWriter::trim_negative_zero(Some(v)).unwrap().to_bits()
}

#[test]
fn a_world_spawn_map_imports_and_round_trips_through_the_gdle_landblock_export() {
    let db = PackContent::new(build(vec![json("worldspawns.json", SPAWNS)]).unwrap());

    // What GDLELoader converts the map to (guids sanitised to 0x7LBID###).
    let file = temp_file("spawns.json", SPAWNS);
    let (expected, links) = loader::try_load_world_spawns_converted(&file, 0).unwrap();
    let _ = std::fs::remove_file(&file);
    assert_eq!(expected.len(), 4);
    assert_eq!(links.len(), 2);

    for landblock in [0xA9B4u16, 0x01DA] {
        let mut want: Vec<_> = expected
            .iter()
            .filter(|i| i.obj_cell_id >> 16 == u32::from(landblock))
            .collect();
        want.sort_by_key(|i| i.guid);
        let got = db.get_cached_instances_by_landblock(landblock);
        let mut got: Vec<_> = got.iter().collect();
        got.sort_by_key(|i| i.guid);
        assert_eq!(got.len(), want.len(), "landblock {landblock:04X}");
        for (g, w) in got.iter().zip(&want) {
            let key = |i: &LandblockInstance| {
                let mut l: Vec<_> = i
                    .landblock_instance_link
                    .iter()
                    .map(|l| (l.parent_guid, l.child_guid))
                    .collect();
                l.sort_unstable();
                let f = [
                    i.origin_x, i.origin_y, i.origin_z, i.angles_w, i.angles_x, i.angles_y,
                    i.angles_z,
                ]
                .map(written);
                (
                    i.guid,
                    i.weenie_class_id,
                    i.obj_cell_id,
                    f,
                    i.is_link_child,
                    l,
                )
            };
            assert_eq!(key(g), key(w));
            assert_eq!(g.last_modified, default_now());
        }
    }

    // Round trip: the pack's landblocks exported as GDLE landblock JSON (ACE's export-json),
    // imported as per-object landblock files over the spawn map, export identically.
    let exports: Vec<String> = [0xA9B4u16, 0x01DA]
        .iter()
        .map(|&lb| export_landblock(&db, lb))
        .collect();
    let mut sources = vec![json("worldspawns.json", SPAWNS)];
    for (e, lb) in exports.iter().zip(["A9B4", "01DA"]) {
        sources.push(json(&format!("landblocks/{lb}.json"), e));
    }
    let again = PackContent::new(build(sources).unwrap());
    for (e, lb) in exports.iter().zip([0xA9B4u16, 0x01DA]) {
        assert_eq!(&export_landblock(&again, lb), e);
    }
}

#[test]
fn a_gdle_quest_list_imports_and_round_trips_through_the_gdle_quest_export() {
    let db = PackContent::new(build(vec![json("quests.json", QUESTS)]).unwrap());

    let one = db.get_cached_quest("GdleQuestOne").unwrap();
    assert_eq!(
        (one.min_delta, one.max_solves, one.message.as_deref()),
        (72000, 3, Some("Quest one, the first"))
    );
    let two = db.get_cached_quest("GdleQuestTwo").unwrap();
    assert_eq!(
        (two.min_delta, two.max_solves, two.message.as_deref()),
        (0, -1, None)
    );
    // The null entry and the entry without a value convert to nothing (TryConvert returns false).
    assert!(db.get_cached_quest("NoValue").is_none());

    let exports: Vec<(String, String)> = [one, two]
        .iter()
        .map(|q| {
            (
                q.name.clone(),
                serialize(&try_convert_quest(q).unwrap()).unwrap(),
            )
        })
        .collect();
    let mut sources = vec![json("quests.json", QUESTS)];
    for (name, e) in &exports {
        sources.push(json(&format!("quests/{name}.json"), e));
    }
    let again = PackContent::new(build(sources).unwrap());
    for (name, e) in &exports {
        let q = again.get_cached_quest(name).unwrap();
        assert_eq!(&serialize(&try_convert_quest(&q).unwrap()).unwrap(), e);
    }
}

#[test]
fn gdle_events_spells_wielded_treasure_recipes_and_precursors_import() {
    let events = r#"[{"key": "GdleEvent", "value": {"startTime": 1600000000, "endTime": -1, "eventState": 2}}, {"key": "NoValue"}]"#;
    let spells = r#"{"table": {"spellBaseHash": [
        {"key": 70001, "value": {"name": "Gdle Spell", "meta_spell": {"sp_type": 1, "spell": {"spell_id": 70001,
          "smod": {"key": 1, "type": 36865, "val": 10.5}, "numProjectiles": 2, "numProjectilesVariance": 2.7,
          "createOffset": {"x": 0.1, "y": 0.2, "z": 0.3}, "critFreq": 2,
          "pos": {"objcell_id": 2847146015, "frame": {"origin": {"x": 1.5, "y": 2.5, "z": 3.5}, "angles": {"w": 1, "x": 0, "y": 0, "z": 0}}}}}}},
        {"key": 70002, "value": {"name": "No meta"}}]}}"#;
    let wielded = r#"[{"key": 900, "value": [
        {"setStart": true, "probability": 0.5, "stackSize": 5, "weenieClassId": 100},
        {"setStart": false, "probability": 0.25, "paletteId": 3, "shade": 0.5, "weenieClassId": 200, "unknown12": 12}]}]"#;
    let recipes = r#"[{"RecipeID": 600, "Skill": 18, "Difficulty": 250, "SuccessWcid": 100, "SuccessAmount": 1,
        "SuccessMessage": "made", "FailMessage": "failed",
        "Requirements": [{"IntRequirements": [{"Stat": 25, "Value": 10, "OperationType": 2, "Message": "too low"}]}],
        "Mods": [{"ModifyHealth": 5, "IntRequirements": [{"Stat": 1, "Value": 2, "OperationType": 3, "Unknown": 4}]}, null, null, null, null, null, null, null]}]"#;
    let precursors = r#"[{"Tool": 100, "Target": 200, "RecipeId": 600}, {"Tool": 200, "Target": 100, "RecipeId": 600}]"#;

    let pack = build(vec![
        json("events.json", events),
        json("spells.json", spells),
        json("wielded.json", wielded),
        json("recipes.json", recipes),
        json("precursors.json", precursors),
    ])
    .unwrap();
    let db = PackContent::new(pack);

    let e = db.get_cached_event("GdleEvent").unwrap();
    assert_eq!((e.start_time, e.end_time, e.state), (1_600_000_000, -1, 2));
    assert!(db.get_cached_event("NoValue").is_none());

    let s = db.get_cached_spell(70001).unwrap();
    assert_eq!(s.name, "Gdle Spell");
    assert_eq!(
        (s.stat_mod_type, s.stat_mod_key, s.stat_mod_val),
        (Some(36865), Some(1), Some(10.5))
    );
    // (int?)2.7 truncates.
    assert_eq!(
        (s.num_projectiles, s.num_projectiles_variance),
        (Some(2), Some(2))
    );
    assert_eq!(s.create_offset_origin_x, Some(0.1));
    assert_eq!(s.crit_freq, Some(2.0));
    assert_eq!(
        (s.position_obj_cell_id, s.position_origin_z),
        (Some(2_847_146_015), Some(3.5))
    );
    assert!(db.get_cached_spell(70002).is_none());

    let all = db.get_all_treasure_wielded();
    let t = all.get(&900).unwrap();
    assert_eq!(t.len(), 2);
    assert_eq!(
        (
            t[0].weenie_class_id,
            t[0].probability,
            t[0].set_start,
            t[0].stack_size
        ),
        (100, 0.5, true, 5)
    );
    assert_eq!(
        (
            t[1].weenie_class_id,
            t[1].palette_id,
            t[1].shade,
            t[1].unknown_12
        ),
        (200, 3, 0.5, 12)
    );

    let r = db.get_recipe(600).unwrap();
    assert_eq!(
        (
            r.skill,
            r.difficulty,
            r.success_wcid,
            r.success_message.as_deref()
        ),
        (18, 250, 100, Some("made"))
    );
    assert_eq!(r.recipe_requirements_int.len(), 1);
    assert_eq!(r.recipe_mod.len(), 1);
    let cook_books = db.get_cookbooks_by_recipe_id(600);
    let mut pairs: Vec<_> = cook_books
        .iter()
        .flatten()
        .map(|c| (c.source_wcid, c.target_wcid))
        .collect();
    pairs.sort_unstable();
    assert_eq!(pairs, [(100, 200), (200, 100)]);
}

#[test]
fn a_json_array_of_no_known_kind_is_refused() {
    let err = build(vec![json("odd.json", r#"[{"a": 1}]"#)]).unwrap_err();
    assert!(err.contains("GDLE bulk file"), "{err}");
    let err = build(vec![json("empty.json", "[]")]).unwrap_err();
    assert!(err.contains("GDLE bulk file"), "{err}");
}
