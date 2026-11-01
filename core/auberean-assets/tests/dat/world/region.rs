//! The region's records: the region itself, its descriptions of sound, scenery, terrain, surfaces,
//! encounters, sky, days, water, fog and properties, block maps, the calendar, the landscape
//! constants, detail maps and map notes.

use auberean_dat::RecordType;

use crate::common::assert_every_record_decodes;

/// Behaviour: data.block-map.every-record-decodes
#[test]
fn every_block_map_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::BLOCK_MAP]);
}

/// Behaviour: data.region.every-record-decodes
#[test]
fn every_region_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::REGION]);
}

/// Behaviour: data.sound-desc.every-record-decodes
#[test]
fn every_region_sound_description_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::SOUND_DESC]);
}

/// Behaviour: data.scene-desc.every-record-decodes
#[test]
fn every_region_scenery_description_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::SCENE_DESC]);
}

/// Behaviour: data.terrain-desc.every-record-decodes
#[test]
fn every_region_terrain_description_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::TERRAIN_DESC]);
}

/// Behaviour: data.surface-desc.every-record-decodes
#[test]
fn every_region_surface_description_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::SURFACE_DESC]);
}

/// Behaviour: data.encounter-desc.every-record-decodes
#[test]
fn every_region_encounter_description_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::ENCOUNTER_DESC]);
}

/// Behaviour: data.sky-desc.every-record-decodes
#[test]
fn every_region_sky_description_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::SKY_DESC]);
}

/// Behaviour: data.property-desc.every-record-decodes
#[test]
fn every_region_property_description_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::PROPERTY_DESC]);
}

/// Behaviour: data.water-desc.every-record-decodes
#[test]
fn every_region_water_description_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::WATER_DESC]);
}

/// Behaviour: data.fog-desc.every-record-decodes
#[test]
fn every_region_fog_description_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::FOG_DESC]);
}

/// Behaviour: data.day-desc.every-record-decodes
#[test]
fn every_day_of_the_sky_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::DAY_DESC]);
}

/// Behaviour: data.game-time.every-record-decodes
#[test]
fn every_calendar_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::GAME_TIME]);
}

/// Behaviour: data.landscape.every-record-decodes
#[test]
fn every_landscape_constants_record_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::LANDSCAPE]);
}

/// Behaviour: data.detail-map.every-record-decodes
#[test]
fn every_detail_map_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::DETAIL_MAP]);
}

/// Behaviour: data.map-notes.every-record-decodes
#[test]
fn every_set_of_map_notes_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::MAP_NOTES]);
}
