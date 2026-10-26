//! Behaviour: none (tooling: the building footprint warning and the SQL's data ids, on the retail files; the blanks a new weenie starts from, and every wielded treasure read as the server draws it and written back the same, in both eras' world databases)

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use dereth_assets::world::LandblockInfo;
use dereth_assets::Decode;
use dereth_primitives::DataId;
use lyceum_foundry::content::{self, PropertyKind, SqlDoc, BLANKS};
use lyceum_foundry::content_check::{self, Known};
use lyceum_foundry::dats::DatSet;
use lyceum_foundry::export::{database_cache, newest_database_for, Era};
use lyceum_foundry::footprint::{statics_inside, Environments, Footprints};
use lyceum_foundry::sql_ids::{self, number};
use lyceum_foundry::validate::{validate, Scope};
use lyceum_foundry::wielded;
use lyceum_foundry::{compile, Error};

/// Retail stands statics inside its own buildings on purpose (lights, furniture, a ring of stones
/// in a courtyard), so the warning is for what a pack places, and the files alone warn of nothing.
/// The count pins the footprint's geometry: its margin behind the walls, its reach below the
/// floor and the cells a building is walked through.
#[test]
fn retail_stands_1658_of_its_own_statics_inside_its_buildings_and_warns_of_none() {
    let set = DatSet::open(&dereth_dat::testing::dat_dir()).unwrap();
    let mut environments = Environments::default();
    let (mut inside, mut landblocks) = (0, 0);
    let ids: Vec<u32> = set
        .cell
        .iter_ids()
        .map(DataId::raw)
        .filter(|id| id & 0xFFFF == 0xFFFE)
        .collect();
    for id in ids {
        let info =
            LandblockInfo::decode_payload(DataId(id), &set.cell.read(DataId(id)).unwrap()).unwrap();
        #[allow(clippy::cast_possible_truncation)]
        let fp = Footprints::of(&set, (id >> 16) as u16, &info, &mut environments);
        let n = statics_inside(&info, &fp).len();
        inside += n;
        landblocks += usize::from(n > 0);
    }
    assert_eq!((inside, landblocks), (1658, 358));
    let report = validate(&set, &Scope::All);
    assert!(
        report.warnings.is_empty(),
        "{:?}",
        &report.warnings[..5.min(report.warnings.len())]
    );
}

/// The fault the first sample stones made: a stone placed on the ground at the middle of a
/// Holtburg house.
#[test]
fn a_static_a_pack_places_inside_a_holtburg_house_is_warned_of() {
    let (dir, world) = super::sample::sample("pack-footprint");
    let dats = dereth_dat::testing::dat_dir();
    let built = compile::build(&world, &dats, None, false).unwrap();
    assert!(built.warnings.is_empty(), "{:?}", built.warnings);
    let edits = dir.path().join("holtburg/edits/landblocks/A9B4.toml");
    let mut text = std::fs::read_to_string(&edits).unwrap();
    text.push_str("\n[[static]]\nmodel = 0x0200040B\nat = [79.5, 37.5, \"ground\"]\n");
    std::fs::write(&edits, text).unwrap();
    let built = compile::build(&world, &dats, None, false).unwrap();
    assert_eq!(built.warnings.len(), 1, "{:?}", built.warnings);
    assert!(
        built.warnings[0].starts_with("0xA9B4FFFE: static 117 (0x0200040B at 79.5, 37.5")
            && built.warnings[0].ends_with("stands inside building 8 (0x01000BC3)"),
        "{:?}",
        built.warnings
    );
}

/// The sample's NPC as first written: two of its parts transcribed one digit wrong, which drew
/// him as nothing in both clients.
#[test]
fn the_sample_npc_names_records_the_files_hold_and_a_mistyped_part_stops_the_build() {
    let (dir, world) = super::sample::sample("pack-sql-retail");
    let dats = dereth_dat::testing::dat_dir();
    compile::build(&world, &dats, None, false).unwrap();
    let weenie = dir.path().join("holtburg/server/sql/weenies/guide.sql");
    let text = std::fs::read_to_string(&weenie)
        .unwrap()
        .replace("0x01001B82", "0x01001A82");
    std::fs::write(&weenie, text).unwrap();
    let e = compile::build(&world, &dats, None, false).unwrap_err();
    let Error::Invalid(faults) = e else {
        panic!("{e}")
    };
    assert_eq!(faults.len(), 1, "{faults:?}");
    assert!(
        faults[0].contains("weenie_properties_anim_part animation_Id names GfxObj 0x01001A82, which the files do not hold"),
        "{faults:?}"
    );
}

// The blanks a new weenie starts from: each a class of its kind, drawn with a model, in both eras'
// world databases (`EMPYREAN_WORLD_SQL` and `EMPYREAN_INFILTRATION_SQL`, else the newest of each
// in the importer's cache), and each blank passing the checks a pack's content passes.

/// Era `era`'s world database: the one `var` names, else the newest of the era in the importer's
/// cache.
fn world_sql(var: &str, era: Era) -> PathBuf {
    std::env::var_os(var)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| database_cache().and_then(|d| newest_database_for(&d, era)))
        .unwrap_or_else(|| {
            panic!(
                "the {} world database: set {var}, or fetch it (empyrean-import fetch)",
                era.word()
            )
        })
}

#[test]
fn every_blank_is_a_class_of_its_kind_drawn_with_a_model_in_both_eras_world_databases() {
    // The two databases read side by side.
    std::thread::scope(|s| {
        for (var, era) in [
            ("EMPYREAN_WORLD_SQL", Era::Eor),
            ("EMPYREAN_INFILTRATION_SQL", Era::Infiltration),
        ] {
            s.spawn(move || blanks_in(&world_sql(var, era)));
        }
    });
}

/// Every blank checked in the world database at `path`.
fn blanks_in(path: &Path) {
    let world = String::from_utf8_lossy(&std::fs::read(path).unwrap()).into_owned();
    let known = Known {
        world_classes: Some(content::classes(&world).into_keys().collect()),
        ..Known::default()
    };
    for b in BLANKS {
        let at = format!("{} {} in {}", b.kind, b.class, path.display());
        let text = content::template(&world, b.class, "blank", true)
            .unwrap_or_else(|| panic!("no class: {at}"));
        let doc = SqlDoc::parse(&text);
        let ty = doc
            .rows("weenie")
            .first()
            .and_then(|r| doc.cell(*r, "type"))
            .and_then(|v| number(&v));
        assert_eq!(ty, Some(b.weenie_type), "its weenie type: {at}");
        if let Some(item) = b.item_type {
            let kind = doc
                .property(PropertyKind::Int, 1)
                .and_then(|v| number(&content::bare(&v)));
            assert_eq!(kind, Some(item), "its item type: {at}");
        }
        assert!(
            doc.property(PropertyKind::DataId, 1).is_some(),
            "a model: {at}"
        );
        assert!(
            doc.property(PropertyKind::String, 1).is_some(),
            "a name: {at}"
        );
        for t in [
            "weenie_properties_emote",
            "weenie_properties_create_list",
            "weenie_properties_generator",
            "weenie_properties_book",
        ] {
            assert!(doc.rows(t).is_empty(), "{t} left out: {at}");
        }
        let mut findings = Vec::new();
        content_check::check(
            Path::new("blank.sql"),
            "pack",
            &text,
            &known,
            &mut BTreeSet::new(),
            &mut findings,
        );
        assert!(findings.is_empty(), "{at}: {findings:#?}");
    }
}

/// What each entry of `w` is and how likely it is made at all, in order.
fn odds(w: &wielded::Wielded) -> Vec<(String, usize, f64)> {
    w.entries()
        .into_iter()
        .zip(w.odds())
        .map(|((at, e), p)| (e.item.weenie.clone(), at.len(), p))
        .collect()
}

#[test]
fn every_wielded_treasure_of_both_eras_is_written_back_so_it_draws_the_same() {
    std::thread::scope(|s| {
        for (var, era) in [
            ("EMPYREAN_WORLD_SQL", Era::Eor),
            ("EMPYREAN_INFILTRATION_SQL", Era::Infiltration),
        ] {
            s.spawn(move || {
                let path = world_sql(var, era);
                let text = String::from_utf8_lossy(&std::fs::read(&path).unwrap()).into_owned();
                let content = sql_ids::server_content(&text);
                assert!(
                    content.wielded.len() > 200 && content.treasure.len() > 200,
                    "{}: {} wielded, {} death",
                    path.display(),
                    content.wielded.len(),
                    content.treasure.len()
                );
                let (mut deep, mut cut) = (0, 0);
                for (id, rows) in &content.wielded {
                    let w = wielded::read(rows);
                    let back = wielded::write(&w);
                    let again = wielded::read(&back);
                    let (a, b) = (odds(&w), odds(&again));
                    assert_eq!(a.len(), b.len(), "{id} in {}", path.display());
                    for (x, y) in a.iter().zip(&b) {
                        assert!(
                            x.0 == y.0 && x.1 == y.1 && (x.2 - y.2).abs() < 1e-6,
                            "{id}: {x:?} against {y:?}"
                        );
                    }
                    assert!(again.unread.is_empty(), "{id}: all of it read");
                    deep += usize::from(a.iter().any(|(_, d, _)| *d > 1));
                    cut += usize::from(!w.unread.is_empty());
                }
                // Some have sub-sets, and a few end before their last rows.
                assert!(deep > 0 && cut > 0, "{deep} deep, {cut} cut");
            });
        }
    });
}
