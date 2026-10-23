//! Lyceum over the February 2005 files, before Throne of Destiny: where the tests find them, the
//! sample world built on their era, and the checks a world over them passes.
//! Fixture: the February 2005 portal and cell files (`DERETH_TEST_PRETOD_DAT_DIR`), the sample
//! world, and the February 2005 world database when the importer's cache holds it.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use dereth_assets::world::EnvCell;
use dereth_assets::Decode;
use dereth_dat::{ContainerEra, ModernDat};
use dereth_primitives::{DataId, EraId};
use lyceum_foundry::compile;
use lyceum_foundry::dats::DatSet;
use lyceum_foundry::edit::Session;
use lyceum_foundry::pieces::Piece;
use lyceum_foundry::validate::{validate, Scope};

pub const ERA: EraId = EraId::Infiltration;

/// The February 2005 files, protected for the run.
pub fn dir() -> PathBuf {
    if let Some(why) = dereth_dat::testing::classic_shortfall() {
        panic!("{why}");
    }
    let dir = dereth_dat::testing::classic_dat_dir().unwrap_or_default();
    dereth_dat::protect_install(&dir);
    dir
}

pub fn files() -> &'static DatSet {
    static SET: OnceLock<DatSet> = OnceLock::new();
    SET.get_or_init(|| DatSet::open_era(&dir(), ERA).unwrap())
}

/// The February 2005 world database in the importer's per-user cache, when it is there.
pub fn world_sql() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("EMPYREAN_INFILTRATION_SQL").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(p));
    }
    lyceum_foundry::export::database_cache().and_then(|d| {
        lyceum_foundry::export::newest_database_for(&d, lyceum_foundry::export::Era::Infiltration)
    })
}

/// The sample world built on Infiltration's era, as `lyceum new --era infiltration` makes it.
pub fn infiltration_sample(tag: &str) -> (dereth_dat::testing::ScratchDir, PathBuf) {
    let (scratch, world) = super::sample::sample(tag);
    let text = std::fs::read_to_string(&world).unwrap();
    std::fs::write(&world, format!("era = \"infiltration\"\n{text}")).unwrap();
    for (rel, bytes) in lyceum_foundry::sample::FILES {
        let path = scratch.path().join(rel);
        if path.is_file() {
            std::fs::write(&path, lyceum_foundry::sample::file_for(ERA, rel, bytes)).unwrap();
        }
    }
    (scratch, world)
}

/// The sample world, built on Infiltration's era, open for editing over the February 2005 files.
pub fn session(tag: &str) -> (dereth_dat::testing::ScratchDir, Session) {
    let (scratch, world) = infiltration_sample(tag);
    let mut s = Session::open(&world, &dir(), None).unwrap();
    s.set_today("2026-10-22 00:00:00");
    (scratch, s)
}

/// The first landblock of the open sea floor: heights all 0, and no statics record or cells.
pub fn empty_landblock(set: &DatSet) -> u16 {
    (0x0101u16..)
        .find(|lb| {
            let first = u32::from(*lb) << 16;
            let Ok(b) = set.cell.read(DataId(first | 0xFFFF)) else {
                return false;
            };
            let land = set
                .decode::<dereth_assets::world::CellLandblock>(DataId(first | 0xFFFF), &b)
                .unwrap();
            land.lbi_exists == 0
                && land.height.iter().all(|h| *h == 0)
                && !set.cell.contains(DataId(first | 0xFFFE))
                && !set.cell.contains(DataId(first | 0x0100))
        })
        .expect("an empty landblock")
}

/// The commonest piece with two portals: a corridor.
pub fn corridor(pieces: &[Piece]) -> Piece {
    pieces
        .iter()
        .filter(|p| p.portals == 2)
        .max_by_key(|p| p.cells)
        .cloned()
        .unwrap()
}

/// Write the session's world out as a build does and check it over the February 2005 files: the
/// records it changes are in their layout and pass every check.
pub fn check(s: &Session) {
    let out = tempdir(s.world().path.parent().unwrap());
    compile::write_overlay(s.compiled(), s.base(), &s.world().key, &out).unwrap();
    let layered = s.base().with_overlay(&out, Some(&s.world().key)).unwrap();
    let report = validate(&layered, &Scope::of(s.compiled()));
    assert!(report.problems.is_empty(), "{:?}", report.problems);
    // Every cell the overlay holds reads in the older layout, and only in it.
    let layer = layered.cell.layer().unwrap();
    let mut cells = 0;
    for (id, _) in layer.records() {
        if dereth_dat::classify_cell_id(id) != Some(dereth_dat::DbType::Cell) {
            continue;
        }
        let bytes = layer.read(id).unwrap();
        EnvCell::decode_payload_for(ERA, id, &bytes).unwrap();
        assert!(EnvCell::decode_payload(id, &bytes).is_err(), "{id}");
        cells += 1;
    }
    assert!(cells > 0);
    assert_eq!(layered.portal.era(), ContainerEra::Classic);
    assert!(layered.file(ModernDat::Local).is_none());
}

fn tempdir(beside: &Path) -> PathBuf {
    let out = beside.join("check-overlay");
    let _ = std::fs::remove_dir_all(&out);
    out
}
