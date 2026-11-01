//! The script library, saved script states, qualities and quality filters.

use auberean_dat::RecordType;

use crate::common::assert_every_record_decodes;

/// Behaviour: data.script-library.every-record-decodes
#[test]
fn the_script_library_decodes_exactly_section_by_section() {
    assert_every_record_decodes(&[RecordType::SCRIPT_LIBRARY]);
}

/// Behaviour: data.script-state.every-record-decodes
#[test]
fn every_saved_script_state_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::SCRIPT_STATE]);
}

/// Behaviour: data.qualities.every-record-decodes
#[test]
fn every_qualities_record_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::QUALITIES]);
}

/// Behaviour: data.quality-filter.every-record-decodes
#[test]
fn every_quality_filter_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::QUALITY_FILTER]);
}
