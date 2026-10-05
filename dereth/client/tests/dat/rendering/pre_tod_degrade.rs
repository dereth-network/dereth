//! A graphics object from before Throne of Destiny reaches its detail record by its own id, so a
//! February 2005 body part draws the record's levels, nearest the denser mesh the record lists
//! first, whether it is drawn on the February 2005 world or as the older look on the end-of-retail
//! world. On either world the level is chosen from the viewing distance less the Degrade Distance
//! setting, so at 0 the levels change at the record's own distances and at the default 50 each
//! change sits 50 m further out. These are the data halves; the record each reader takes is the
//! one the scene builds its levels from.
//!
//! Fixture: the February 2005 dats (`DERETH_TEST_PRETOD_DAT_DIR`) with the retail dats
//! (`DERETH_TEST_DAT_DIR`) beside them. A missing input fails.

use dereth_assets::{Decode, GfxObj, GfxObjDegradeInfo};
use dereth_dat::{ContainerEra, DbType, RetailDatStore};
use dereth_primitives::DataId;
use dereth_world_render::objects::degrade::{get_degrade, DegradeGlobals, DegradeMode};

/// The shipped Degrade Distance setting.
const DEFAULT: f32 = dereth_animation::parts::S_R_DEGRADE_DISTANCE;

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

/// The mesh drawn at `distance` with the Degrade Distance setting at `degrade_distance` and the
/// shipped bias.
fn drawn(info: &GfxObjDegradeInfo, degrade_distance: f32, distance: f32) -> (u32, DegradeMode) {
    let g = DegradeGlobals {
        degrade_distance,
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
    for (name, store) in [
        ("the February 2005 world", &older),
        ("the older look", &look),
    ] {
        let (did, info) = record(store, TORSO).unwrap_or_else(|| panic!("{name}: no record"));
        assert_eq!(did, DataId(0x1100_004E), "{name}");
        assert_eq!(info.degrades.len(), 6, "{name}");
        assert_eq!(drawn(&info, DEFAULT, 1.0).0, TORSO_NEAR, "{name}");
        assert!(
            dereth_client_runtime::models::draws_at_near_band(store, TORSO),
            "{name}: the part draws at the near band"
        );
    }
    let (did, info) = record(&eor, TORSO).expect("the end-of-retail torso names a record");
    assert_eq!(did, DataId(0x1100_06C6));
    assert_eq!(drawn(&info, DEFAULT, 1.0).0, TORSO_NEAR);
}

/// Behaviour: rendering.degrade.the-degrade-distance-applies-on-an-older-world
/// On the February 2005 world the Degrade Distance setting moves the torso's detail changes as it
/// does on the end-of-retail world. At 0 they sit at the record's own distances: the dense torso
/// inside 3 m, the torso itself to 5 m, coarser meshes to 7 and 15 m, an upright card to 84 m and
/// nothing beyond. At the default 50 each change sits 50 m further out, and at 100, 100 m.
#[test]
fn the_degrade_distance_moves_an_older_worlds_torso_levels_out_by_its_own_value() {
    let older = older_world();
    let (_, info) = record(&older, TORSO).expect("the torso reaches its record");
    for offset in [0.0, DEFAULT, 100.0] {
        let at = |d: f32| drawn(&info, offset, d + offset);
        assert_eq!(at(2.0).0, TORSO_NEAR, "Degrade Distance {offset}");
        assert_eq!(at(4.0).0, TORSO.raw(), "Degrade Distance {offset}");
        assert_eq!(at(6.0).0, 0x0100_01A2, "Degrade Distance {offset}");
        assert_eq!(at(10.0).0, 0x0100_01A0, "Degrade Distance {offset}");
        assert_eq!(
            at(20.0),
            (0x0100_01F1, DegradeMode::AxisZ),
            "Degrade Distance {offset}"
        );
        assert_eq!(
            at(100.0).0,
            0,
            "nothing is drawn past 84 m beyond the setting"
        );
    }
    // At 49 m the default keeps the nearest level, where 0 has reached the upright card.
    assert_eq!(drawn(&info, DEFAULT, 49.0).0, TORSO_NEAR);
    assert_eq!(drawn(&info, 0.0, 49.0), (0x0100_01F1, DegradeMode::AxisZ));
}

/// Behaviour: rendering.degrade.an-older-eras-part-draws-the-levels-its-own-id-reaches
/// A step of the Holtburg cottage stair has no record of its own in the February 2005 files, so on
/// the February 2005 world it reaches none and draws itself: the end-of-retail record kept under
/// the id its own id implies is another object's levels and is not read in its place. A record the
/// later files hold for an object the older files lack is still read from them.
#[test]
fn an_older_part_without_its_own_record_draws_itself_and_not_the_later_record_under_that_id() {
    const STEP: DataId = DataId(0x0100_081A);
    const IMPLIED: DataId = DataId(0x1100_081A);
    let older = older_world();
    assert!(
        dereth_dat::testing::open_store_or_fail()
            .portal()
            .contains(IMPLIED),
        "the end-of-retail files hold a record under the implied id"
    );
    assert_eq!(record(&older, STEP).map(|(did, _)| did), None);
    assert!(older.read_typed(DbType::DegradeInfo, IMPLIED).is_err());
    assert_eq!(older.era_of(IMPLIED), ContainerEra::PreTod);
    assert!(!older.ids_of(DbType::DegradeInfo).contains(&IMPLIED));
    // A later record no older object implies stays readable beside the older world.
    let later_only = older
        .ids_of(DbType::DegradeInfo)
        .into_iter()
        .find(|id| !older.portal().contains(*id))
        .expect("a later detail record whose object the older files lack");
    assert!(!older
        .portal()
        .contains(DataId(0x0100_0000 | (later_only.raw() & 0x00FF_FFFF))));
    assert!(older.read_typed(DbType::DegradeInfo, later_only).is_ok());
    assert_eq!(older.era_of(later_only), ContainerEra::Tod);
}

/// Behaviour: rendering.degrade.an-older-eras-part-draws-the-levels-its-own-id-reaches
/// The portal-space tunnel's model has no detail record in the February 2005 files, so on the
/// February 2005 world it draws itself through log-in and log-out rather than the end-of-retail
/// record under its implied id, whose levels are other models (with them the tunnel drew black).
#[test]
fn the_portal_space_tunnel_draws_its_own_model_on_the_february_2005_world() {
    /// The tunnel's two parts, setup `0x02000306` in both eras.
    const TUNNEL: DataId = DataId(0x0100_080B);
    let older = older_world();
    assert!(dereth_dat::testing::open_store_or_fail()
        .portal()
        .contains(DataId(0x1100_080B)));
    assert_eq!(record(&older, TUNNEL).map(|(did, _)| did), None);
    assert!(dereth_client_runtime::models::draws_at_near_band(
        &older, TUNNEL
    ));
}
