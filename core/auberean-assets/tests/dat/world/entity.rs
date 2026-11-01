//! Standalone entities.

use auberean_dat::RecordType;

use crate::common::assert_every_record_decodes;

/// Behaviour: data.entity.every-record-decodes
#[test]
fn every_entity_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::ENTITY]);
}
