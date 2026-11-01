//! Pictures and textures in every file that holds them.

use auberean_dat::RecordType;

use crate::common::assert_every_record_decodes;

/// Behaviour: data.picture.every-record-decodes
#[test]
fn every_picture_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::PICTURE, RecordType::PICTURE_LOCAL]);
}

/// Behaviour: data.texture.every-record-decodes
#[test]
fn every_texture_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::TEXTURE, RecordType::TEXTURE_LOCAL]);
}
