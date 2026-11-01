//! Effect tables, effect scripts, camera effects and particle systems.

use auberean_dat::RecordType;

use crate::common::assert_every_record_decodes;

/// Behaviour: data.fx-table.every-record-decodes
#[test]
fn every_effect_table_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::FX_TABLE]);
}

/// Behaviour: data.fx-script.every-record-decodes
#[test]
fn every_effect_script_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::FX_SCRIPT]);
}

/// Behaviour: data.camera-fx.every-record-decodes
#[test]
fn every_camera_effect_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::CAMERA_FX]);
}

/// Behaviour: data.particles.every-record-decodes
#[test]
fn every_particle_system_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::PARTICLES]);
}
