//! Writing records back: every record of a type with a writer, decoded and written again, gives
//! the bytes it was read from.

use auberean_assets::entity::Entity;
use auberean_assets::landblock::{Cell, LandblockInfo, LightInfo};
use auberean_assets::property::{PhysicsMaterial, PropertyList};
use auberean_assets::region::{
    BlockMap, FogDesc, Landscape, Region, SceneDesc, TerrainDesc, WaterDesc,
};
use auberean_assets::script::ScriptState;
use auberean_assets::strings::{EnumMap, StringState, StringTable};
use auberean_assets::tables::{FileIdTable, IdMap};
use auberean_assets::verify::type_in;
use auberean_assets::{decode, encode, Decode, Encode};
use auberean_dat::RecordType;

use crate::common::installs;

/// Every record of `ty` in the checked set decodes and encodes to its own bytes; returns how many
/// there were.
fn round_trip<T: Decode + Encode>(ty: RecordType) -> usize {
    let mut n = 0;
    let mut differ = Vec::new();
    for i in installs() {
        for (_, dat) in &i.dats {
            for e in dat.entries() {
                if type_in(dat, e.id) != Some(ty) {
                    continue;
                }
                let bytes = dat.read_entry(e).expect("read the record");
                let rec: T = decode(e.id, &bytes, &i.ctx)
                    .unwrap_or_else(|err| panic!("{ty:?} {:08X}: {err}", e.id));
                n += 1;
                if encode(e.id, &rec) != bytes {
                    differ.push(e.id);
                }
            }
        }
    }
    assert!(
        differ.is_empty(),
        "{ty:?}: {} of {n} records encode to other bytes, first {:08X?}",
        differ.len(),
        &differ[..differ.len().min(8)]
    );
    assert!(n > 0, "no record of {ty:?}");
    n
}

/// Behaviour: data.records.encode-to-their-own-bytes
#[test]
fn every_record_with_a_writer_writes_back_to_its_own_bytes() {
    let counts = [
        round_trip::<PropertyList>(RecordType::PROPERTY_LIST),
        round_trip::<PhysicsMaterial>(RecordType::PHYSICS_MATERIAL),
        round_trip::<Entity>(RecordType::ENTITY),
        round_trip::<EnumMap>(RecordType::ENUM_MAP),
        round_trip::<IdMap>(RecordType::ID_MAP),
        round_trip::<StringTable>(RecordType::STRING_TABLE),
        round_trip::<StringState>(RecordType::STRING_STATE),
        round_trip::<LandblockInfo>(RecordType::LANDBLOCK_INFO),
        round_trip::<LightInfo>(RecordType::LIGHT_INFO),
        round_trip::<Cell>(RecordType::CELL),
        round_trip::<FileIdTable>(RecordType::FILE_ID_TABLE),
        round_trip::<ScriptState>(RecordType::SCRIPT_STATE),
        round_trip::<Region>(RecordType::REGION),
        round_trip::<SceneDesc>(RecordType::SCENE_DESC),
        round_trip::<TerrainDesc>(RecordType::TERRAIN_DESC),
        round_trip::<WaterDesc>(RecordType::WATER_DESC),
        round_trip::<FogDesc>(RecordType::FOG_DESC),
        round_trip::<BlockMap>(RecordType::BLOCK_MAP),
        round_trip::<Landscape>(RecordType::LANDSCAPE),
    ];
    // Entities, string tables, cells and saved script states number in the thousands at the end
    // of retail.
    assert!(
        counts[2] > 10_000 && counts[5] > 10_000 && counts[9] > 10_000 && counts[11] > 10_000,
        "{counts:?}"
    );
}
