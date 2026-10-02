//! A graphics object from before Throne of Destiny reaches its detail record by its own id, so a
//! February 2005 body part draws the record's levels, nearest the denser mesh the record lists
//! first, whether it is drawn on the February 2005 world or as the older look on the end-of-retail
//! world. The level is chosen from the raw viewing distance on the older world and from the
//! distance less the 50 m degrade distance on the end-of-retail world, whichever look it draws.
//! These are the data halves; the record each reader takes is the one the scene builds its levels
//! from.
//!
//! Fixture: the February 2005 dats (`DERETH_TEST_PRETOD_DAT_DIR`) with the retail dats
//! (`DERETH_TEST_DAT_DIR`) beside them. A missing input fails.

use dereth_assets::{Decode, GfxObj, GfxObjDegradeInfo};
use dereth_client::world::world_degrade_distance;
use dereth_dat::{ContainerEra, DbType, RetailDatStore};
use dereth_primitives::DataId;
use dereth_world_render::objects::degrade::{get_degrade, DegradeGlobals, DegradeMode};

/// The male torso, the same id in every era.
const TORSO: DataId = DataId(0x0100_004E);
/// Its February 2005 record's nearest level: 44 drawing polygons against the torso's 16.
const TORSO_NEAR: u32 = 0x0100_1787;

/// The February 2005 world with the end-of-retail files beside it.
fn older_world() -> RetailDatStore {
    if let Some(msg) = dereth_dat::testing::pre_tod_shortfall() {
        panic!("{msg}");
    }
    let world = dereth_dat::testing::pre_tod_dat_dir().unwrap_or_default();
    RetailDatStore::open_pre_tod_with_later(&world, &dereth_dat::testing::dat_dir())
        .unwrap_or_else(|e| panic!("the February 2005 world did not open: {e}"))
}

/// The end-of-retail world with the February 2005 portal beside it for presentation.
fn end_of_retail_world() -> RetailDatStore {
    if let Some(msg) = dereth_dat::testing::pre_tod_shortfall() {
        panic!("{msg}");
    }
    let legacy = dereth_dat::testing::pre_tod_dat_dir().unwrap_or_default();
    dereth_dat::testing::open_store_or_fail()
        .with_legacy_portal(&legacy)
        .unwrap_or_else(|e| panic!("the February 2005 portal did not attach: {e}"))
}

/// The detail record a graphics object reaches, read the way the scene reads it.
fn record(store: &RetailDatStore, gfxobj: DataId) -> Option<(DataId, GfxObjDegradeInfo)> {
    let bytes = store.read_typed(DbType::GfxObj, gfxobj).ok()?;
    let obj = GfxObj::decode_payload_in(store.era_of(gfxobj), gfxobj, &bytes).ok()?;
    let did = obj.did_degrade?;
    let bytes = store.read_typed(DbType::DegradeInfo, did).ok()?;
    let info = GfxObjDegradeInfo::decode_payload_in(store.era_of(did), did, &bytes).ok()?;
    Some((did, info))
}

/// The mesh drawn at `distance` on a world of `world_era`'s files, with the shipped settings.
fn drawn(info: &GfxObjDegradeInfo, world_era: ContainerEra, distance: f32) -> (u32, DegradeMode) {
    let g = DegradeGlobals {
        degrade_distance: world_degrade_distance(
            world_era,
            dereth_animation::parts::S_R_DEGRADE_DISTANCE,
        ),
        ..DegradeGlobals::default()
    };
    let (level, mode) = get_degrade(info, distance, &g);
    (info.degrades[level].gfxobj_id.raw(), mode)
}

/// Behaviour: rendering.degrade.an-older-eras-part-draws-the-levels-its-own-id-reaches
/// The February 2005 torso names no record; it reaches `0x1100004E` by its own id, and up close
/// it draws that record's first level, the 44-polygon torso, on the February 2005 world and as
/// the older look on the end-of-retail world alike. The end-of-retail torso keeps the record
/// its own file names.
#[test]
fn a_february_2005_torso_draws_its_records_nearest_level_up_close_on_either_world() {
    let older = older_world();
    let eor = end_of_retail_world();
    let look = eor
        .object_files(ContainerEra::PreTod)
        .expect("the February 2005 files are beside the world");
    for (name, store, world_era) in [
        ("the February 2005 world", &older, ContainerEra::PreTod),
        ("the older look", &look, ContainerEra::Tod),
    ] {
        let (did, info) = record(store, TORSO).unwrap_or_else(|| panic!("{name}: no record"));
        assert_eq!(did, DataId(0x1100_004E), "{name}");
        assert_eq!(info.degrades.len(), 6, "{name}");
        assert_eq!(drawn(&info, world_era, 1.0).0, TORSO_NEAR, "{name}");
        assert!(
            dereth_client::models::draws_at_near_band(store, TORSO),
            "{name}: the part draws at the near band"
        );
    }
    let (did, info) = record(&eor, TORSO).expect("the end-of-retail torso names a record");
    assert_eq!(did, DataId(0x1100_06C6));
    assert_eq!(drawn(&info, ContainerEra::Tod, 1.0).0, TORSO_NEAR);
}

/// Behaviour: rendering.degrade.an-older-world-chooses-detail-from-the-raw-distance
/// On the February 2005 world the torso changes level at its record's own distances: the dense
/// torso inside 3 m, the torso itself to 5 m, coarser meshes to 7 and 15 m, an upright card to
/// 84 m and nothing beyond. On the end-of-retail world, drawing the same record as the older
/// look, each change sits 50 m further out.
#[test]
fn an_older_world_changes_the_torsos_level_at_the_raw_distance_and_the_later_world_50_m_out() {
    let older = older_world();
    let (_, info) = record(&older, TORSO).expect("the torso reaches its record");
    let pre = |d| drawn(&info, ContainerEra::PreTod, d);
    assert_eq!(pre(2.0).0, TORSO_NEAR);
    assert_eq!(pre(4.0).0, TORSO.raw());
    assert_eq!(pre(6.0).0, 0x0100_01A2);
    assert_eq!(pre(10.0).0, 0x0100_01A0);
    assert_eq!(pre(20.0), (0x0100_01F1, DegradeMode::AxisZ));
    assert_eq!(pre(100.0).0, 0, "nothing is drawn past 84 m");

    let later = |d| drawn(&info, ContainerEra::Tod, d);
    assert_eq!(later(4.0).0, TORSO_NEAR);
    assert_eq!(later(20.0).0, TORSO_NEAR);
    assert_eq!(later(54.0).0, TORSO.raw());
    assert_eq!(later(70.0), (0x0100_01F1, DegradeMode::AxisZ));
}
