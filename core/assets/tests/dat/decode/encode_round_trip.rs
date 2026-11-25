//! Behaviour: none (checks formats, geometry, numeric contracts or repository tools)
//! Every retail record of each kind a world builder writes encodes back to its own bytes.
//! Fixture: the shipped retail DAT records.

use dereth_assets::{
    Animation, CellLandblock, CombatManeuverTable, Decode, EnvCell, Environment, GfxObj,
    LandblockInfo, MotionTable, ParticleEmitterInfo, PhysicsScript, Region, RenderSurface, Setup,
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
fn every_animation_encodes_back_to_its_own_bytes() {
    let s = store();
    let ids = s.ids_of(DbType::Anim);
    let n = round_trip(
        &ids,
        |id| s.read_typed(DbType::Anim, id).unwrap(),
        |id, b| Animation::decode_payload(id, b).unwrap().encode(),
    );
    assert_eq!(n, 2_066, "every animation");
}

#[test]
fn every_particle_emitter_encodes_back_to_its_own_bytes() {
    let s = store();
    let ids = s.ids_of(DbType::ParticleEmitter);
    let n = round_trip(
        &ids,
        |id| s.read_typed(DbType::ParticleEmitter, id).unwrap(),
        |id, b| ParticleEmitterInfo::decode_payload(id, b).unwrap().encode(),
    );
    assert_eq!(n, 2_051, "every particle emitter");
}

#[test]
fn every_physics_script_encodes_back_to_its_own_bytes() {
    let s = store();
    let ids = s.ids_of(DbType::PhysicsScript);
    let n = round_trip(
        &ids,
        |id| s.read_typed(DbType::PhysicsScript, id).unwrap(),
        |id, b| PhysicsScript::decode_payload(id, b).unwrap().encode(),
    );
    assert_eq!(n, 4_248, "every physics script");
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

#[test]
fn the_character_creation_table_encodes_back_to_its_own_bytes() {
    let s = store();
    let id = DataId(0x0E00_0002);
    let bytes = s.read_portal(id).unwrap();
    let cg = dereth_assets::tables::CharGen::decode_payload(id, &bytes).unwrap();
    assert_eq!(cg.starter_areas.len(), 5, "retail's five starter areas");
    let got = cg.encode();
    let first = got.iter().zip(&bytes).position(|(a, b)| a != b);
    assert!(
        got == bytes,
        "the table encodes to other bytes: {} against {}, first difference at {first:?}: {:02X?} against {:02X?}",
        got.len(),
        bytes.len(),
        first.map(|i| &got[i.saturating_sub(8)..(i + 8).min(got.len())]),
        first.map(|i| &bytes[i.saturating_sub(8)..(i + 8).min(bytes.len())]),
    );
}

#[test]
fn the_region_encodes_back_to_its_own_bytes() {
    let s = store();
    let ids = s.ids_of(DbType::Region);
    let n = round_trip(
        &ids,
        |id| s.read_typed(DbType::Region, id).unwrap(),
        |id, b| Region::decode_payload(id, b).unwrap().encode().unwrap(),
    );
    assert_eq!(n, 1, "the one region");
}

/// A sky of another size moves everything after it, and the record still reads back whole: the
/// region with its first day group dropped, one more sky object and one more replacement.
#[test]
fn the_region_with_another_sky_reads_back_as_written() {
    let s = store();
    let id = DataId(0x1300_0000);
    let mut r = Region::decode_payload(id, &s.read_portal(id).unwrap()).unwrap();
    let sky = r.sky_info.as_mut().expect("the region has a sky");
    let first = sky.day_groups.remove(0);
    let g = &mut sky.day_groups[0];
    g.day_name = "Odd".into();
    g.sky_objects.push(first.sky_objects[0].clone());
    let row = first.sky_time[0].sky_obj_replace[0].clone();
    g.sky_time[0].sky_obj_replace.push(row);
    let bytes = r.encode().unwrap();
    let back = Region::decode_payload(id, &bytes).unwrap();
    assert_eq!(back, r);
    assert_eq!(back.encode().unwrap(), bytes);
}

/// One portal-file record, read, decoded and encoded back, against its own bytes.
fn portal_record_round_trips(id: u32, encode: impl Fn(DataId, &[u8]) -> Vec<u8>) {
    let s = store();
    let id = DataId(id);
    let bytes = s.read_portal(id).unwrap();
    let got = encode(id, &bytes);
    let first = got.iter().zip(&bytes).position(|(a, b)| a != b);
    assert!(
        got == bytes,
        "{id:?} encodes to other bytes: {} against {}, first difference at {first:?}: {:02X?} against {:02X?}",
        got.len(),
        bytes.len(),
        first.map(|i| &got[i.saturating_sub(8)..(i + 8).min(got.len())]),
        first.map(|i| &bytes[i.saturating_sub(8)..(i + 8).min(bytes.len())]),
    );
}

#[test]
fn the_skill_table_encodes_back_to_its_own_bytes() {
    portal_record_round_trips(0x0E00_0004, |id, b| {
        dereth_assets::tables::SkillTable::decode_payload(id, b)
            .unwrap()
            .encode()
    });
}

#[test]
fn the_experience_table_encodes_back_to_its_own_bytes() {
    portal_record_round_trips(0x0E00_0018, |id, b| {
        dereth_assets::tables::XpTable::decode_payload(id, b)
            .unwrap()
            .encode()
    });
}

#[test]
fn the_spell_table_encodes_back_to_its_own_bytes() {
    portal_record_round_trips(0x0E00_000E, |id, b| {
        dereth_assets::tables::SpellTable::decode_payload(id, b)
            .unwrap()
            .encode()
    });
}

/// A skill table a world changes reads back as changed: a skill's costs, a skill added, a skill
/// gone; the skills it keeps stay in their stored order and the added one follows them.
#[test]
fn an_edited_skill_table_reads_back_as_edited() {
    use dereth_assets::tables::SkillTable;
    let s = store();
    let id = DataId(0x0E00_0004);
    let mut t = SkillTable::decode_payload(id, &s.read_portal(id).unwrap()).unwrap();
    let stored = t.order.clone();
    let melee_defense = t.skills.get_mut(&6).expect("Melee Defense");
    melee_defense.trained_cost = 4;
    melee_defense.specialized_cost = 8;
    let mut axe = t.skills[&6].clone();
    axe.name = "Axe".into();
    axe.description = "Helps you fight with axes.".into();
    t.skills.insert(1, axe);
    t.skills.remove(&54);
    let back = SkillTable::decode_payload(id, &t.encode()).unwrap();
    assert_eq!(back.skills, t.skills);
    let kept: Vec<u32> = stored.into_iter().filter(|k| *k != 54).collect();
    assert_eq!(back.order[..kept.len()], kept[..]);
    assert_eq!(back.order[kept.len()..], [1]);
}

/// An experience table a world changes reads back as changed: a shorter curve with its own
/// numbers.
#[test]
fn an_edited_experience_table_reads_back_as_edited() {
    use dereth_assets::tables::XpTable;
    let s = store();
    let id = DataId(0x0E00_0018);
    let mut t = XpTable::decode_payload(id, &s.read_portal(id).unwrap()).unwrap();
    t.level_xp.truncate(21);
    t.level_credits.truncate(21);
    for (n, v) in t.level_xp.iter_mut().enumerate() {
        *v = 1000 * (n as u64) * (n as u64);
    }
    assert_eq!(XpTable::decode_payload(id, &t.encode()).unwrap(), t);
}

/// A spell a world renames keeps its components, now keyed by its new name, and a component it
/// changes reads back changed; every other spell reads back as it was.
#[test]
fn a_renamed_spell_keeps_its_components() {
    use dereth_assets::tables::SpellTable;
    let s = store();
    let id = DataId(0x0E00_000E);
    let mut t = SpellTable::decode_payload(id, &s.read_portal(id).unwrap()).unwrap();
    let heal = t.spells.get_mut(&6).expect("Heal Self I");
    let comps = heal.comps.clone();
    heal.name = "Mend Self I".into();
    heal.comps[1] = comps[2];
    let back = SpellTable::decode_payload(id, &t.encode()).unwrap();
    let mended = &back.spells[&6];
    assert_eq!(mended.name, "Mend Self I");
    assert_eq!(mended.comps[0], comps[0]);
    assert_eq!(mended.comps[1], comps[2]);
    assert_eq!(mended.comps[2..], comps[2..]);
    assert_ne!(
        mended.comp_key, t.spells[&6].comp_key,
        "keyed by the new name"
    );
    for (k, v) in &t.spells {
        if *k != 6 {
            assert_eq!(&back.spells[k], v, "spell {k}");
        }
    }
    assert_eq!(back.spellsets, t.spellsets);
}

/// The end of retail's spell table and experience table don't fit the February 2005 files: the
/// older files have no spell sets, and no level past 32 bits of experience.
#[test]
fn the_later_spell_sets_and_levels_are_refused_for_the_older_files() {
    use dereth_assets::tables::{SpellTable, XpTable};
    let s = store();
    let spells = DataId(0x0E00_000E);
    let t = SpellTable::decode_payload(spells, &s.read_portal(spells).unwrap()).unwrap();
    assert!(t
        .encode_for(dereth_primitives::EraId::Infiltration)
        .is_err());
    let mut no_sets = t.clone();
    no_sets.spellsets.clear();
    assert!(no_sets
        .encode_for(dereth_primitives::EraId::Infiltration)
        .is_ok());
    let xp = DataId(0x0E00_0018);
    let x = XpTable::decode_payload(xp, &s.read_portal(xp).unwrap()).unwrap();
    assert!(x.level_xp.last().copied().unwrap_or(0) > u64::from(u32::MAX));
    assert!(x
        .encode_for(dereth_primitives::EraId::Infiltration)
        .is_err());
    assert_eq!(
        x.encode_for(dereth_primitives::EraId::Eor).unwrap(),
        x.encode()
    );
}

#[test]
fn every_motion_table_written_again_reads_back_the_same() {
    // A retail table keeps its keyed tables in hash order, written here in key order, so the
    // check is on what the record says, not its bytes.
    let s = store();
    let ids = s.ids_of(DbType::MTable);
    let mut differ = Vec::new();
    for id in &ids {
        let t =
            MotionTable::decode_payload(*id, &s.read_typed(DbType::MTable, *id).unwrap()).unwrap();
        if MotionTable::decode_payload(*id, &t.encode()).ok().as_ref() != Some(&t) {
            differ.push(*id);
        }
    }
    assert!(
        differ.is_empty(),
        "{} differ: {:?}",
        differ.len(),
        &differ[..differ.len().min(8)]
    );
    assert!(ids.len() > 200, "{} motion tables", ids.len());
}

#[test]
fn every_combat_table_encodes_back_to_its_own_bytes() {
    let s = store();
    let ids = s.ids_of(DbType::CombatTable);
    let n = round_trip(
        &ids,
        |id| s.read_typed(DbType::CombatTable, id).unwrap(),
        |id, b| CombatManeuverTable::decode_payload(id, b).unwrap().encode(),
    );
    assert!(n > 20, "{n} combat tables");
}
