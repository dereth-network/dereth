//! Materials, material modifiers and material instances.

use auberean_dat::RecordType;

use crate::common::assert_every_record_decodes;

/// Behaviour: data.material.every-record-decodes
#[test]
fn every_material_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::MATERIAL]);
}

/// Behaviour: data.material-modifier.every-record-decodes
#[test]
fn every_material_modifier_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::MATERIAL_MODIFIER]);
}

/// Behaviour: data.material-instance.every-record-decodes
#[test]
fn every_material_instance_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::MATERIAL_INSTANCE]);
}
