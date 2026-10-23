//! Behaviour: none (checks formats, geometry, numeric contracts or repository tools)
//! Every record of the February 2005 files (before Throne of Destiny) of each kind a world builder
//! writes reads in that era's layout and encodes back to its own bytes in that layout, and a
//! record of the later layout is refused or written in the older one as the older files hold it.
//! Fixture: the February 2005 portal and cell files (`DERETH_TEST_PRETOD_DAT_DIR`).

use dereth_assets::motion::GfxObjDegradeInfo;
use dereth_assets::{
    CellLandblock, Decode, EnvCell, Environment, GfxObj, LandblockInfo, RenderSurface, Setup,
    Surface,
};
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::{DataId, EraId};

const ERA: EraId = EraId::Infiltration;

fn store() -> RetailDatStore {
    dereth_dat::testing::open_classic_store_or_fail()
}

/// The ids of one cell-file kind.
fn cell_ids(s: &RetailDatStore, kind: DbType) -> Vec<DataId> {
    s.cell()
        .iter_ids()
        .filter(|id| dereth_dat::classify_cell_id(*id) == Some(kind))
        .collect()
}

/// Every record in `ids` reads in the era's layout and encodes back byte for byte; answers how
/// many there were.
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

fn portal(s: &RetailDatStore, kind: DbType) -> (Vec<DataId>, impl Fn(DataId) -> Vec<u8> + '_) {
    (s.ids_of(kind), move |id| s.read_typed(kind, id).unwrap())
}

#[test]
fn every_february_2005_landblock_encodes_back_to_its_own_bytes() {
    let s = store();
    let ids = cell_ids(&s, DbType::LandBlock);
    let n = round_trip(
        &ids,
        |id| s.read_cell(id).unwrap(),
        |id, b| {
            CellLandblock::decode_payload_for(ERA, id, b)
                .unwrap()
                .encode_for(ERA)
                .unwrap()
        },
    );
    assert_eq!(n, 65_025, "every landblock of the world");
}

#[test]
fn every_february_2005_landblock_info_encodes_back_to_its_own_bytes() {
    let s = store();
    let ids = cell_ids(&s, DbType::Lbi);
    let n = round_trip(
        &ids,
        |id| s.read_cell(id).unwrap(),
        |id, b| {
            LandblockInfo::decode_payload_for(ERA, id, b)
                .unwrap()
                .encode_for(ERA)
                .unwrap()
        },
    );
    assert_eq!(n, 4_288, "every landblock that has statics or buildings");
}

#[test]
fn every_february_2005_indoor_cell_encodes_back_to_its_own_bytes() {
    let s = store();
    let ids = cell_ids(&s, DbType::Cell);
    let n = round_trip(
        &ids,
        |id| s.read_cell(id).unwrap(),
        |id, b| {
            EnvCell::decode_payload_for(ERA, id, b)
                .unwrap()
                .encode_for(ERA)
                .unwrap()
        },
    );
    assert_eq!(n, 455_641, "every cell of every building and dungeon");
}

#[test]
fn every_february_2005_surface_encodes_back_to_its_own_bytes() {
    let s = store();
    let (ids, read) = portal(&s, DbType::Surface);
    let n = round_trip(&ids, read, |id, b| {
        Surface::decode_payload_for(ERA, id, b)
            .unwrap()
            .encode_for(ERA, id)
            .unwrap()
    });
    assert_eq!(n, 4_334);
}

/// An image texture of these files holds its own pixels: it reads as the picture it is and is
/// written back from that picture.
#[test]
fn every_february_2005_image_texture_encodes_back_to_its_own_bytes() {
    let s = store();
    let (ids, read) = portal(&s, DbType::SurfaceTexture);
    let n = round_trip(&ids, read, |id, b| {
        let rs = RenderSurface::from_classic_texture(id, b).unwrap();
        rs.encode_texture_for(ERA, rs.payload(b).unwrap()).unwrap()
    });
    assert_eq!(n, 5_118);
}

#[test]
fn every_february_2005_image_encodes_back_to_its_own_bytes() {
    let s = store();
    let (ids, read) = portal(&s, DbType::RenderSurface);
    let n = round_trip(&ids, read, |id, b| {
        let rs = RenderSurface::decode_payload_for(ERA, id, b).unwrap();
        rs.encode_for(ERA, rs.payload(b).unwrap()).unwrap()
    });
    assert_eq!(n, 9_930);
}

#[test]
fn every_february_2005_model_encodes_back_to_its_own_bytes() {
    let s = store();
    let (ids, read) = portal(&s, DbType::GfxObj);
    let n = round_trip(&ids, read, |id, b| {
        GfxObj::decode_payload_for(ERA, id, b)
            .unwrap()
            .encode_for(ERA)
            .unwrap()
    });
    assert_eq!(n, 10_065, "every model");
}

#[test]
fn every_february_2005_setup_encodes_back_to_its_own_bytes() {
    let s = store();
    let (ids, read) = portal(&s, DbType::Setup);
    let n = round_trip(&ids, read, |id, b| {
        Setup::decode_payload_for(ERA, id, b)
            .unwrap()
            .encode_for(ERA)
            .unwrap()
    });
    assert_eq!(n, 3_622, "every setup");
}

#[test]
fn every_february_2005_environment_encodes_back_to_its_own_bytes() {
    let s = store();
    let (ids, read) = portal(&s, DbType::Environment);
    let n = round_trip(&ids, read, |id, b| {
        Environment::decode_payload_for(ERA, id, b)
            .unwrap()
            .encode_for(ERA)
            .unwrap()
    });
    assert_eq!(n, 686, "every environment");
}

#[test]
fn every_february_2005_detail_record_encodes_back_to_its_own_bytes() {
    let s = store();
    let (ids, read) = portal(&s, DbType::DegradeInfo);
    let n = round_trip(&ids, read, |id, b| {
        GfxObjDegradeInfo::decode_payload_for(ERA, id, b)
            .unwrap()
            .encode_for(ERA)
            .unwrap()
    });
    assert_eq!(n, 3_134, "every detail record");
}

/// A model, a cell and an environment of the later files written for the older ones read back in
/// the older layout as the same values; a surface carries its id there, and a later image texture
/// (which names its images) and a picture in a later format are refused rather than written as
/// something else.
#[test]
fn later_records_written_for_february_2005_read_back_in_its_layout() {
    let later = dereth_dat::testing::open_store_or_fail();
    let read = |kind, id: u32| later.read_typed(kind, DataId(id)).unwrap();
    let eor = EraId::Eor;

    let id = DataId(0x0100_0A2B);
    let g = GfxObj::decode_payload_for(eor, id, &read(DbType::GfxObj, id.raw())).unwrap();
    let mut older = g.clone();
    older.did_degrade = None;
    let back = GfxObj::decode_payload_for(ERA, id, &older.encode_for(ERA).unwrap()).unwrap();
    assert_eq!(back.polygons, g.polygons);
    assert_eq!(back.vertex_array, g.vertex_array);
    assert_eq!(back.drawing_bsp, g.drawing_bsp);
    assert_eq!(
        back.did_degrade,
        Some(DataId(0x1100_0A2B)),
        "found by its id"
    );
    let mut elsewhere = g;
    elsewhere.did_degrade = Some(DataId(0x1100_0001));
    assert!(
        elsewhere.encode_for(ERA).is_err(),
        "a detail record the older files cannot reach"
    );

    let cell = DataId(0xA9B4_0100);
    let c = EnvCell::decode_payload_for(eor, cell, &later.read_cell(cell).unwrap()).unwrap();
    let bytes = c.encode_for(ERA).unwrap();
    assert_eq!(&bytes[..4], &c.flags.to_le_bytes(), "the flags come first");
    assert_eq!(EnvCell::decode_payload_for(ERA, cell, &bytes).unwrap(), c);

    let env = DataId(0x0D00_0170);
    let e =
        Environment::decode_payload_for(eor, env, &read(DbType::Environment, env.raw())).unwrap();
    let bytes = e.encode_for(ERA).unwrap();
    assert!(bytes.len() > e.encode().len(), "the older layout pads");
    assert_eq!(
        Environment::decode_payload_for(ERA, env, &bytes).unwrap(),
        e
    );

    let surface = DataId(0x0800_0001);
    let s =
        Surface::decode_payload_for(eor, surface, &read(DbType::Surface, surface.raw())).unwrap();
    let bytes = s.encode_for(ERA, surface).unwrap();
    assert_eq!(&bytes[..4], &surface.raw().to_le_bytes());
    assert_eq!(
        Surface::decode_payload_for(ERA, surface, &bytes).unwrap(),
        s
    );

    let texture = later.ids_of(DbType::SurfaceTexture)[0];
    let t = dereth_assets::SurfaceTexture::decode_payload_for(
        eor,
        texture,
        &read(DbType::SurfaceTexture, texture.raw()),
    )
    .unwrap();
    assert!(t.encode_for(ERA).is_err(), "it names images");
    let picture = later.ids_of(DbType::RenderSurface)[0];
    let bytes = read(DbType::RenderSurface, picture.raw());
    let rs = RenderSurface::decode_payload_for(eor, picture, &bytes).unwrap();
    assert!(rs.encode_for(ERA, rs.payload(&bytes).unwrap()).is_err());
}
