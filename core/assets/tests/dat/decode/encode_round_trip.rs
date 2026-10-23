//! Behaviour: none (checks formats, geometry, numeric contracts or repository tools)
//! Every retail record of each kind a world builder writes encodes back to its own bytes.
//! Fixture: the shipped retail DAT records.

use dereth_assets::{
    CellLandblock, Decode, EnvCell, Environment, GfxObj, LandblockInfo, RenderSurface, Setup,
    Surface, SurfaceTexture,
};
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::DataId;

fn store() -> RetailDatStore {
    let s = dereth_dat::testing::open_store_or_fail();
    assert!(
        s.grant_highres().expect("client_highres.dat opens"),
        "client_highres.dat is present"
    );
    s
}

/// The ids of one cell-file kind.
fn cell_ids(s: &RetailDatStore, kind: DbType) -> Vec<DataId> {
    s.cell()
        .iter_ids()
        .filter(|id| dereth_dat::classify_cell_id(*id) == Some(kind))
        .collect()
}

/// Every record in `ids` decodes and encodes back byte for byte; answers how many there were.
fn round_trip(
    ids: &[DataId],
    read: impl Fn(DataId) -> Vec<u8>,
    encode: impl Fn(DataId, &[u8]) -> Vec<u8>,
) -> usize {
    let mut differ = Vec::new();
    for id in ids {
        let bytes = read(*id);
        if encode(*id, &bytes) != bytes {
            differ.push(*id);
        }
    }
    assert!(
        differ.is_empty(),
        "{} of {} records encode to other bytes, the first {:?}",
        differ.len(),
        ids.len(),
        &differ[..differ.len().min(8)]
    );
    ids.len()
}

#[test]
fn every_landblock_encodes_back_to_its_own_bytes() {
    let s = store();
    let ids = cell_ids(&s, DbType::LandBlock);
    let n = round_trip(
        &ids,
        |id| s.read_cell(id).unwrap(),
        |id, b| CellLandblock::decode_payload(id, b).unwrap().encode(),
    );
    assert_eq!(n, 65_025, "every landblock of the world");
}

#[test]
fn every_landblock_info_encodes_back_to_its_own_bytes() {
    let s = store();
    let ids = cell_ids(&s, DbType::Lbi);
    let n = round_trip(
        &ids,
        |id| s.read_cell(id).unwrap(),
        |id, b| LandblockInfo::decode_payload(id, b).unwrap().encode(),
    );
    assert_eq!(n, 5_346, "every landblock that has statics or buildings");
}

#[test]
fn every_indoor_cell_encodes_back_to_its_own_bytes() {
    let s = store();
    let ids = cell_ids(&s, DbType::Cell);
    let n = round_trip(
        &ids,
        |id| s.read_cell(id).unwrap(),
        |id, b| EnvCell::decode_payload(id, b).unwrap().encode(),
    );
    assert_eq!(n, 734_976, "every cell of every building and dungeon");
}

#[test]
fn every_surface_encodes_back_to_its_own_bytes() {
    let s = store();
    let ids = s.ids_of(DbType::Surface);
    let n = round_trip(
        &ids,
        |id| s.read_typed(DbType::Surface, id).unwrap(),
        |id, b| Surface::decode_payload(id, b).unwrap().encode(),
    );
    assert_eq!(n, 6_152);
}

#[test]
fn every_surface_texture_encodes_back_to_its_own_bytes() {
    let s = store();
    let ids = s.ids_of(DbType::SurfaceTexture);
    let n = round_trip(
        &ids,
        |id| s.read_typed(DbType::SurfaceTexture, id).unwrap(),
        |id, b| SurfaceTexture::decode_payload(id, b).unwrap().encode(),
    );
    assert_eq!(n, 7_221);
}

#[test]
fn every_render_surface_encodes_back_to_its_own_bytes() {
    let s = store();
    let ids = s.ids_of(DbType::RenderSurface);
    let n = round_trip(
        &ids,
        |id| s.read_typed(DbType::RenderSurface, id).unwrap(),
        |id, b| {
            let rs = RenderSurface::decode_payload(id, b).unwrap();
            rs.encode(rs.payload(b).unwrap())
        },
    );
    // The portal file's and the high-res file's.
    assert_eq!(n, 20_684 + 2_294);
}

#[test]
fn every_model_encodes_back_to_its_own_bytes() {
    let s = store();
    let ids = s.ids_of(DbType::GfxObj);
    let n = round_trip(
        &ids,
        |id| s.read_typed(DbType::GfxObj, id).unwrap(),
        |id, b| GfxObj::decode_payload(id, b).unwrap().encode(),
    );
    assert_eq!(n, 15_318, "every model");
}

#[test]
fn every_setup_encodes_back_to_its_own_bytes() {
    let s = store();
    let ids = s.ids_of(DbType::Setup);
    let n = round_trip(
        &ids,
        |id| s.read_typed(DbType::Setup, id).unwrap(),
        |id, b| Setup::decode_payload(id, b).unwrap().encode(),
    );
    assert_eq!(n, 5_935, "every setup");
}

#[test]
fn every_environment_encodes_back_to_its_own_bytes() {
    let s = store();
    let ids = s.ids_of(DbType::Environment);
    let n = round_trip(
        &ids,
        |id| s.read_typed(DbType::Environment, id).unwrap(),
        |id, b| Environment::decode_payload(id, b).unwrap().encode(),
    );
    assert_eq!(n, 772, "every environment");
}

#[test]
fn every_detail_record_encodes_back_to_its_own_bytes() {
    let s = store();
    let ids = s.ids_of(DbType::DegradeInfo);
    let n = round_trip(
        &ids,
        |id| s.read_typed(DbType::DegradeInfo, id).unwrap(),
        |id, b| {
            dereth_assets::motion::GfxObjDegradeInfo::decode_payload(id, b)
                .unwrap()
                .encode()
        },
    );
    assert_eq!(n, 4_131, "every detail record");
}
