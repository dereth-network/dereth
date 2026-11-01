//! Sounds, encoded sounds, the sound and music settings, music and sound environments.

use auberean_dat::RecordType;

use crate::common::assert_every_record_decodes;

/// Behaviour: data.wave.every-record-decodes
#[test]
fn every_wave_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::WAVE]);
}

/// Behaviour: data.encoded-wave.every-record-decodes
#[test]
fn every_encoded_wave_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::ENCODED_WAVE]);
}

/// Behaviour: data.sound-info.every-record-decodes
#[test]
fn every_sound_info_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::SOUND_INFO]);
}

/// Behaviour: data.music-info.every-record-decodes
#[test]
fn every_music_info_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::MUSIC_INFO]);
}

/// Behaviour: data.music.every-record-decodes
#[test]
fn every_piece_of_music_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::MUSIC]);
}

/// Behaviour: data.sound-environment.every-record-decodes
#[test]
fn every_sound_environment_decodes_exactly() {
    assert_every_record_decodes(&[RecordType::SOUND_ENVIRONMENT]);
}
