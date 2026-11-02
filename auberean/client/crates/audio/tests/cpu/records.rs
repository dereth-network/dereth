//! The records a sound plays from: waves and encoded tracks.

use auberean_assets::sound::Wave;
use auberean_audio::records;

use crate::wave_file;

/// Behaviour: audio.records.a-track-reads-as-far-as-its-bytes-go
#[test]
fn an_encoded_track_reads_its_wave_file_as_far_as_its_bytes_go() {
    let file = wave_file(16000, 1, &[16384, -16384], &[]);
    let s = records::track_source(1, &file).expect("plain samples");
    assert_eq!(s.rate(), 16000);
    assert_eq!(s.frames(), &[0.5, 0.5, -0.5, -0.5]);
    // A chunk running past the file's own size, and past the bytes: read as far as they go.
    let mut tail = b"LIST".to_vec();
    tail.extend_from_slice(&9u32.to_le_bytes());
    tail.extend_from_slice(&[0; 4]);
    let file = wave_file(16000, 2, &[16384, -16384], &tail);
    let s = records::track_source(1, &file).expect("plain samples");
    assert_eq!(s.frames(), &[0.5, -0.5]);
    assert!(records::track_source(1, b"RIFX....WAVE").is_err());
}

/// Behaviour: audio.records.a-waves-length-is-its-frames-over-its-rate
#[test]
fn a_waves_length_is_its_frames_over_its_rate() {
    let mut format = Vec::new();
    for v in [1u16, 2] {
        format.extend_from_slice(&v.to_le_bytes());
    }
    format.extend_from_slice(&22050u32.to_le_bytes());
    format.extend_from_slice(&(22050u32 * 4).to_le_bytes());
    format.extend_from_slice(&4u16.to_le_bytes());
    format.extend_from_slice(&16u16.to_le_bytes());
    let w = Wave {
        format,
        data: vec![0; 22050 * 4 + 3],
    };
    assert_eq!(records::wave_seconds(&w), 1.0);
    let s = records::wave_source(1, &w).expect("plain samples");
    assert_eq!(s.len_frames(), 22050);
}
