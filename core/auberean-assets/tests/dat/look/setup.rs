//! Setups: meshes, skeleton, poses, attachment points, collision shapes and detail distances.

use auberean_dat::RecordType;

use crate::common::assert_every_record_decodes;

/// Behaviour: data.setup.every-record-decodes
#[test]
fn every_setup_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::SETUP]);
}
