//! Device-free tests of the sound manager: the laws it plays by, and what it does with requests
//! and the sounds playing, over records made in memory and a mixer pulled by hand.

mod ambient;
mod laws;
mod manager;
mod music;
mod records;

use std::collections::HashMap;

use auberean_audio::SoundFiles;

/// Records made in memory, by id.
#[derive(Debug, Default)]
pub struct Files(pub HashMap<u32, Vec<u8>>);

impl SoundFiles for Files {
    fn read(&self, id: u32) -> Option<Vec<u8>> {
        self.0.get(&id).cloned()
    }
}

fn le(words: &[u32]) -> Vec<u8> {
    words.iter().flat_map(|w| w.to_le_bytes()).collect()
}

impl Files {
    /// A wave of 16-bit mono `samples` at `rate`.
    pub fn wave(&mut self, id: u32, rate: u32, samples: &[i16]) {
        let mut format = Vec::new();
        format.extend_from_slice(&1u16.to_le_bytes());
        format.extend_from_slice(&1u16.to_le_bytes());
        format.extend_from_slice(&rate.to_le_bytes());
        format.extend_from_slice(&(rate * 2).to_le_bytes());
        format.extend_from_slice(&2u16.to_le_bytes());
        format.extend_from_slice(&16u16.to_le_bytes());
        format.extend_from_slice(&0u16.to_le_bytes());
        let data: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        let mut r = le(&[id, 18, u32::try_from(data.len()).expect("small")]);
        r.extend_from_slice(&format);
        r.extend_from_slice(&data);
        self.0.insert(id, r);
    }

    /// A sound setting: its flags, priority and minimum distance, and its wave.
    pub fn sound(&mut self, id: u32, flags: u32, priority: f32, min_distance: f32, wave: u32) {
        let r = le(&[
            id,
            flags,
            0f32.to_bits(),
            0f32.to_bits(),
            priority.to_bits(),
            min_distance.to_bits(),
            wave,
        ]);
        self.0.insert(id, r);
    }

    /// A music setting: its flags, kind and what it plays.
    pub fn music(&mut self, id: u32, flags: u32, kind: u32, music: u32) {
        let r = le(&[id, flags, 1f32.to_bits(), 1f32.to_bits(), kind, music]);
        self.0.insert(id, r);
    }

    /// An encoded track: a wave file's bytes, as a language file keeps them.
    pub fn track(&mut self, id: u32, file: &[u8]) {
        let mut r = le(&[id, u32::try_from(file.len()).expect("small")]);
        r.extend_from_slice(file);
        self.0.insert(id, r);
    }
}

/// A RIFF wave file of 16-bit `channels`-channel samples at `rate`, with `tail` after its data.
#[must_use]
pub fn wave_file(rate: u32, channels: u16, samples: &[i16], tail: &[u8]) -> Vec<u8> {
    let data: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
    let mut fmt = Vec::new();
    fmt.extend_from_slice(&1u16.to_le_bytes());
    fmt.extend_from_slice(&channels.to_le_bytes());
    fmt.extend_from_slice(&rate.to_le_bytes());
    fmt.extend_from_slice(&(rate * 2 * u32::from(channels)).to_le_bytes());
    fmt.extend_from_slice(&(2 * channels).to_le_bytes());
    fmt.extend_from_slice(&16u16.to_le_bytes());
    let mut body = b"WAVE".to_vec();
    body.extend_from_slice(b"fmt ");
    body.extend_from_slice(&16u32.to_le_bytes());
    body.extend_from_slice(&fmt);
    body.extend_from_slice(b"data");
    body.extend_from_slice(&u32::try_from(data.len()).expect("small").to_le_bytes());
    body.extend_from_slice(&data);
    let mut file = b"RIFF".to_vec();
    file.extend_from_slice(&u32::try_from(body.len()).expect("small").to_le_bytes());
    file.extend_from_slice(&body);
    file.extend_from_slice(tail);
    file
}
