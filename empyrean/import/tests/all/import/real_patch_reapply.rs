//! Vectors: the configured ACE world dump and ACE-World-16PY-Patches checkout
//! Re-applying every ACE-World-16PY-Patches file over the dump gives back the dump's records
//! modulo surrogate ids.
//! Fixture: the configured ACE world dump and the explicitly enabled patch checkout.

use std::collections::BTreeMap;
use std::path::PathBuf;

use empyrean_content::import::patch::{Input, InputKind};
use empyrean_content::import::{self, Build};
use empyrean_content::models::world::*;
use empyrean_content::pack::{Pack, TableId};

fn dump() -> PathBuf {
    empyrean_common::test_paths::world_sql()
}

macro_rules! zero_ids {
    ($v:expr) => {
        for r in &mut $v {
            r.id = 0;
        }
    };
}

fn norm_weenie(mut w: Weenie) -> Weenie {
    zero_ids!(w.weenie_properties_anim_part);
    zero_ids!(w.weenie_properties_attribute);
    zero_ids!(w.weenie_properties_attribute_2nd);
    zero_ids!(w.weenie_properties_body_part);
    zero_ids!(w.weenie_properties_book_page_data);
    zero_ids!(w.weenie_properties_bool);
    zero_ids!(w.weenie_properties_create_list);
    zero_ids!(w.weenie_properties_did);
    zero_ids!(w.weenie_properties_event_filter);
    zero_ids!(w.weenie_properties_float);
    zero_ids!(w.weenie_properties_generator);
    zero_ids!(w.weenie_properties_iid);
    zero_ids!(w.weenie_properties_int);
    zero_ids!(w.weenie_properties_int64);
    zero_ids!(w.weenie_properties_palette);
    zero_ids!(w.weenie_properties_position);
    zero_ids!(w.weenie_properties_skill);
    zero_ids!(w.weenie_properties_spell_book);
    zero_ids!(w.weenie_properties_string);
    zero_ids!(w.weenie_properties_texture_map);
    if let Some(b) = &mut w.weenie_properties_book {
        b.id = 0;
    }
    for e in &mut w.weenie_properties_emote {
        e.id = 0;
        for a in &mut e.weenie_properties_emote_action {
            a.id = 0;
            a.emote_id = 0;
        }
    }
    w
}

fn norm_recipe(mut r: Recipe) -> Recipe {
    for m in &mut r.recipe_mod {
        m.id = 0;
        for x in &mut m.recipe_mods_bool {
            x.id = 0;
            x.recipe_mod_id = 0;
        }
        for x in &mut m.recipe_mods_did {
            x.id = 0;
            x.recipe_mod_id = 0;
        }
        for x in &mut m.recipe_mods_float {
            x.id = 0;
            x.recipe_mod_id = 0;
        }
        for x in &mut m.recipe_mods_iid {
            x.id = 0;
            x.recipe_mod_id = 0;
        }
        for x in &mut m.recipe_mods_int {
            x.id = 0;
            x.recipe_mod_id = 0;
        }
        for x in &mut m.recipe_mods_string {
            x.id = 0;
            x.recipe_mod_id = 0;
        }
    }
    zero_ids!(r.recipe_requirements_bool);
    zero_ids!(r.recipe_requirements_did);
    zero_ids!(r.recipe_requirements_float);
    zero_ids!(r.recipe_requirements_iid);
    zero_ids!(r.recipe_requirements_int);
    zero_ids!(r.recipe_requirements_string);
    r
}

/// Every record of both packs, surrogate ids zeroed; tables keyed by an auto id are re-keyed by
/// their natural key (quest/event/POI name, treasure type).
fn compare(old: &Pack, new: &Pack) -> BTreeMap<String, Vec<String>> {
    let mut diffs: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut note = |t: &str, what: String| diffs.entry(t.to_owned()).or_default().push(what);

    let (a, b) = (
        old.all::<Weenie>(TableId::WEENIE).unwrap(),
        new.all::<Weenie>(TableId::WEENIE).unwrap(),
    );
    let b: BTreeMap<u64, Weenie> = b.into_iter().collect();
    assert_eq!(a.len(), b.len(), "weenie count");
    for (k, w) in a {
        if norm_weenie(w) != norm_weenie(b[&k].clone()) {
            note("weenie", k.to_string());
        }
    }
    let (a, b) = (
        old.all::<Vec<LandblockInstance>>(TableId::LANDBLOCK_INSTANCE)
            .unwrap(),
        new.all::<Vec<LandblockInstance>>(TableId::LANDBLOCK_INSTANCE)
            .unwrap(),
    );
    let b: BTreeMap<u64, Vec<LandblockInstance>> = b.into_iter().collect();
    assert_eq!(a.len(), b.len(), "landblock count");
    for (k, v) in a {
        let z = |v: Vec<LandblockInstance>| -> Vec<LandblockInstance> {
            v.into_iter()
                .map(|mut i| {
                    zero_ids!(i.landblock_instance_link);
                    i
                })
                .collect()
        };
        if z(v) != z(b[&k].clone()) {
            note("landblock_instance", format!("0x{k:04X}"));
        }
    }
    let (a, b) = (
        old.all::<Vec<Encounter>>(TableId::ENCOUNTER).unwrap(),
        new.all::<Vec<Encounter>>(TableId::ENCOUNTER).unwrap(),
    );
    let b: BTreeMap<u64, Vec<Encounter>> = b.into_iter().collect();
    for (k, mut v) in a {
        let mut w = b[&k].clone();
        zero_ids!(v);
        zero_ids!(w);
        if v != w {
            note("encounter", format!("0x{k:04X}"));
        }
    }
    let (a, b) = (
        old.all::<Vec<CookBook>>(TableId::COOK_BOOK).unwrap(),
        new.all::<Vec<CookBook>>(TableId::COOK_BOOK).unwrap(),
    );
    let b: BTreeMap<u64, Vec<CookBook>> = b.into_iter().collect();
    assert_eq!(a.len(), b.len(), "cook book pairs");
    for (k, mut v) in a {
        let mut w = b[&k].clone();
        zero_ids!(v);
        zero_ids!(w);
        if v != w {
            note("cook_book", format!("{k:#x}"));
        }
    }
    let (a, b) = (
        old.all::<Recipe>(TableId::RECIPE).unwrap(),
        new.all::<Recipe>(TableId::RECIPE).unwrap(),
    );
    let b: BTreeMap<u64, Recipe> = b.into_iter().collect();
    for (k, r) in a {
        if norm_recipe(r) != norm_recipe(b[&k].clone()) {
            note("recipe", k.to_string());
        }
    }
    macro_rules! by_natural_key {
        ($ty:ty, $table:expr, $name:literal, |$r:ident| $key:expr $(, $fix:expr)?) => {{
            let group = |p: &Pack| {
                let mut m: BTreeMap<String, Vec<$ty>> = BTreeMap::new();
                for (_, mut $r) in p.all::<$ty>($table).unwrap() {
                    let k = $key;
                    $r.id = 0;
                    $( $fix(&mut $r); )?
                    m.entry(k).or_default().push($r);
                }
                m
            };
            let (a, b) = (group(old), group(new));
            if a.len() != b.len() {
                note($name, format!("{} keys before, {} after", a.len(), b.len()));
            }
            for (k, v) in &a {
                if b.get(k) != Some(v) {
                    note($name, k.clone());
                }
            }
        }};
    }
    by_natural_key!(Quest, TableId::QUEST, "quest", |r| r.name.to_lowercase());
    by_natural_key!(Event, TableId::EVENT, "event", |r| r.name.to_lowercase());
    // `TN POIs.sql` REPLACEs 51 rows without `last_Modified`, so MySQL stamps CURRENT_TIMESTAMP:
    // the dump holds its compile time, the rebuild `--now`. Compared apart below.
    by_natural_key!(
        PointsOfInterest,
        TableId::POINTS_OF_INTEREST,
        "points_of_interest",
        |r| r.name.to_lowercase(),
        |r: &mut PointsOfInterest| r.last_modified = Default::default()
    );
    let stamped = new
        .all::<PointsOfInterest>(TableId::POINTS_OF_INTEREST)
        .unwrap()
        .into_iter()
        .filter(|(_, p)| p.last_modified == import::default_now())
        .count();
    if stamped != 51 {
        note(
            "points_of_interest",
            format!("{stamped} rows stamped with --now, expected 51"),
        );
    }
    by_natural_key!(
        TreasureDeath,
        TableId::TREASURE_DEATH,
        "treasure_death",
        |r| r.treasure_type.to_string()
    );
    by_natural_key!(
        TreasureWielded,
        TableId::TREASURE_WIELDED,
        "treasure_wielded",
        |r| r.treasure_type.to_string()
    );
    for t in [
        TableId::SPELL,
        TableId::HOUSE_PORTAL,
        TableId::TREASURE_GEM_COUNT,
        TableId::TREASURE_MATERIAL_BASE,
        TableId::TREASURE_MATERIAL_COLOR,
        TableId::TREASURE_MATERIAL_GROUPS,
        TableId::VERSION,
    ] {
        let (ka, kb) = (old.keys(t), new.keys(t));
        if ka != kb
            || ka
                .iter()
                .any(|&k| old.raw(t, k).unwrap() != new.raw(t, k).unwrap())
        {
            note(t.name(), "differs".into());
        }
    }
    diffs
}

#[test]
#[ignore = "needs EMPYREAN_ACE_PATCHES: a checkout of ACE-World-16PY-Patches (tag v0.9.295) Database/Patches"]
fn reapply_every_patch_file_gives_back_the_dump() {
    let patches =
        PathBuf::from(std::env::var_os("EMPYREAN_ACE_PATCHES").expect("set EMPYREAN_ACE_PATCHES"));
    // Every folder and file in the repository's own order, except `Z Misc/POIs.sql`: its plain
    // INSERTs of names the dump already holds fail on the unique key, in MySQL as here (the
    // repository's compile runs it only against a fresh 16PY base).
    let mut inputs = Vec::new();
    for d in [
        "1 RegionDescExtendedData",
        "2 SpellTableExtendedData",
        "3 TreasureTable",
        "4 CraftTable",
        "6 LandBlockExtendedData",
        "8 QuestDefDB",
        "9 WeenieDefaults",
        "B GameEventDefDB",
        "Z Misc/LootGenColors.sql",
        "Z Misc/LootGenGem.sql",
        "Z Misc/TN POIs.sql",
        "zz-Cleanup",
    ] {
        inputs.push(Input {
            kind: InputKind::Sql,
            path: patches.join(d),
        });
    }
    let t0 = std::time::Instant::now();
    let (base, _) = import::build(&Build {
        sql: dump(),
        inputs: Vec::new(),
        now: import::default_now(),
    })
    .unwrap();
    let t1 = std::time::Instant::now();
    let (patched, imp) = import::build(&Build {
        sql: dump(),
        inputs,
        now: import::default_now(),
    })
    .unwrap();
    let t2 = std::time::Instant::now();
    eprintln!(
        "plain {:.1}s, {} files re-applied {:.1}s",
        (t1 - t0).as_secs_f64(),
        imp.applied.len(),
        (t2 - t1).as_secs_f64()
    );
    let diffs = compare(
        &Pack::from_bytes(base).unwrap(),
        &Pack::from_bytes(patched).unwrap(),
    );
    for (t, v) in &diffs {
        eprintln!("{t}: {} differ, e.g. {:?}", v.len(), &v[..v.len().min(10)]);
    }
    // The only difference: `zz-Cleanup` deletes a PropertyBool 58 row of weenie 31718 and a
    // PropertyString 38 row of weenie 33517 that the released dump still holds (neither weenie
    // has a file of its own, so nothing re-inserts them).
    let expected: BTreeMap<String, Vec<String>> = [(
        "weenie".to_owned(),
        vec!["31718".to_owned(), "33517".to_owned()],
    )]
    .into_iter()
    .collect();
    assert_eq!(diffs, expected);
}
