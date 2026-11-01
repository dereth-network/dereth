//! The cell files: landblocks, their placed objects, their light and their interior cells.

use auberean_dat::RecordType;

use crate::common::assert_every_record_decodes;

/// Behaviour: data.landblock.every-record-decodes
#[test]
fn every_landblock_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::LANDBLOCK]);
}

/// Behaviour: data.landblock-info.every-record-decodes
#[test]
fn every_landblock_info_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::LANDBLOCK_INFO]);
}

/// Behaviour: data.light-info.every-record-decodes
#[test]
fn every_light_info_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::LIGHT_INFO]);
}

/// Behaviour: data.cell.every-record-decodes
#[test]
fn every_interior_cell_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::CELL]);
}
