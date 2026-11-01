//! Animations, animation maps, motion descriptions, behavior tables, character templates, valid
//! modes.

use auberean_dat::RecordType;

use crate::common::assert_every_record_decodes;

/// Behaviour: data.animation.every-record-decodes
#[test]
fn every_animation_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::ANIMATION]);
}

/// Behaviour: data.anim-map.every-record-decodes
#[test]
fn every_animation_map_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::ANIM_MAP]);
}

/// Behaviour: data.motion-desc.every-record-decodes
#[test]
fn every_motion_description_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::MOTION_DESC]);
}

/// Behaviour: data.behavior-table.every-record-decodes
#[test]
fn every_behavior_table_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::BEHAVIOR_TABLE]);
}

/// Behaviour: data.character-template.every-record-decodes
#[test]
fn the_character_template_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::CHARACTER_TEMPLATE]);
}

/// Behaviour: data.valid-modes.every-record-decodes
#[test]
fn every_valid_modes_list_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::VALID_MODES]);
}
