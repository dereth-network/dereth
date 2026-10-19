//! Behaviour: none (tooling: the building footprint warning and the SQL's data ids, on the retail files)

use dereth_assets::world::LandblockInfo;
use dereth_assets::Decode;
use dereth_primitives::DataId;
use lyceum_foundry::dats::DatSet;
use lyceum_foundry::footprint::{statics_inside, Environments, Footprints};
use lyceum_foundry::validate::{validate, Scope};
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
