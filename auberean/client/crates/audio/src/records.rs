//! The records a sound plays from: a sound setting's wave (portal `0x0C`) and a music setting's
//! encoded track (a language file's `0x61`, a wave file holding an MP3 or plain samples), read
//! through the host's [`SoundFiles`] and decoded to mixer sources.

use auberean_assets::region::SoundDesc;
use auberean_assets::sound::{LocalSound, MusicInfo, SoundInfo, Wave};
use auberean_assets::{decode, Context};
use dereth_audio::voices::Source;
use dereth_primitives::DataId;

/// The data files, as the sound manager reads them: a record's bytes from whichever file holds it.
pub trait SoundFiles {
    /// Record `id`'s bytes, or `None` when no file holds it.
    fn read(&self, id: u32) -> Option<Vec<u8>>;
}

/// The type a setting's id names, from its top byte: a sound setting or a music setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingType {
    Sound,
    Music,
}

/// The type of setting `id` names, if it names one.
#[must_use]
pub fn setting_type(id: u32) -> Option<SettingType> {
    match id >> 24 {
        0x2A => Some(SettingType::Sound),
        0x2C => Some(SettingType::Music),
        _ => None,
    }
}

/// A record decoded as `T`.
fn get<T: auberean_assets::Decode>(files: &dyn SoundFiles, id: u32) -> Option<T> {
    decode::<T>(id, &files.read(id)?, &Context::default()).ok()
}

/// A sound setting.
#[must_use]
pub fn sound_info(files: &dyn SoundFiles, id: u32) -> Option<SoundInfo> {
    get(files, id)
}

/// A music setting.
#[must_use]
pub fn music_info(files: &dyn SoundFiles, id: u32) -> Option<MusicInfo> {
    get(files, id)
}

/// The region's ambient sound table: each kind of place's sounds.
#[must_use]
pub fn ambient_table(files: &dyn SoundFiles, id: u32) -> Option<SoundDesc> {
    get(files, id)
}

/// A wave's format block: its tag, channels, rate and sample size.
fn format(block: &[u8]) -> Option<(u16, u16, u32, u16)> {
    let u16_at = |o: usize| {
        block
            .get(o..o + 2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
    };
    let rate = block
        .get(4..8)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))?;
    Some((u16_at(0)?, u16_at(2)?, rate, u16_at(14)?))
}

/// A wave's length in seconds: its frames (the data's bytes over the sample size and the
/// channels) over its rate.
// D2-07739
#[must_use]
pub fn wave_seconds(w: &Wave) -> f32 {
    let Some((_, channels, rate, bits)) = format(&w.format) else {
        return 0.0;
    };
    let bytes = u64::try_from(w.data.len()).unwrap_or(u64::MAX);
    let per = u64::from(bits >> 3).max(1) * u64::from(channels).max(1);
    let frames = u32::try_from(bytes / per).unwrap_or(u32::MAX);
    if rate == 0 {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss)]
    // LINT-OK: the client divides the two as floats.
    let seconds = frames as f32 / rate as f32;
    seconds
}

/// A wave as a mixer source, at its own rate.
///
/// # Errors
/// A format the wave does not hold, or not plain samples.
pub fn wave_source(id: u32, w: &Wave) -> Result<Source, String> {
    let (tag, channels, rate, bits) = format(&w.format).ok_or("no format block")?;
    if tag != 1 {
        return Err(format!("format {tag:#x} is not plain samples"));
    }
    dereth_audio::wave::pcm_source(DataId(id), &w.data, channels, bits, rate)
        .map_err(|e| e.to_string())
}

/// A sound setting's wave, as a source, with its length in seconds.
///
/// # Errors
/// The wave is missing or does not decode.
pub fn sound_source(files: &dyn SoundFiles, info: &SoundInfo) -> Result<(Source, f32), String> {
    let w: Wave = get(files, info.sound).ok_or_else(|| format!("no wave {:08X}", info.sound))?;
    let s = wave_source(info.sound, &w)?;
    Ok((s, wave_seconds(&w)))
}

/// An encoded track's bytes (a RIFF wave file) as a source: an MP3's frames decoded, or plain
/// samples. The chunks are read as far as the bytes go, whatever the file's own size says.
///
/// # Errors
/// No format or data chunk, or a format that is neither.
pub fn track_source(id: u32, bytes: &[u8]) -> Result<Source, String> {
    if bytes.get(0..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"WAVE") {
        return Err("not a RIFF wave".to_owned());
    }
    let mut at = 12usize;
    let mut fmt: Option<&[u8]> = None;
    let mut data: Option<&[u8]> = None;
    while at + 8 <= bytes.len() {
        let size = u32::from_le_bytes([bytes[at + 4], bytes[at + 5], bytes[at + 6], bytes[at + 7]])
            as usize;
        let start = at + 8;
        let end = start.saturating_add(size).min(bytes.len());
        match &bytes[at..at + 4] {
            b"fmt " => fmt = Some(&bytes[start..end]),
            b"data" => data = Some(&bytes[start..end]),
            _ => {}
        }
        at = start.saturating_add(size).saturating_add(size & 1);
    }
    let (tag, channels, rate, bits) = fmt.and_then(format).ok_or("no format chunk")?;
    let data = data.ok_or("no data chunk")?;
    match tag {
        1 => dereth_audio::wave::pcm_source(DataId(id), data, channels, bits, rate)
            .map_err(|e| e.to_string()),
        0x55 => dereth_audio::wave::mp3_source(DataId(id), data).map_err(|e| e.to_string()),
        t => Err(format!("format {t:#x} is neither MP3 nor plain samples")),
    }
}

/// A music setting's encoded track as a source: `None` for a setting of synthesized music, which
/// has no track.
///
/// # Errors
/// The track is missing or does not decode.
pub fn music_source(files: &dyn SoundFiles, info: &MusicInfo) -> Result<Option<Source>, String> {
    if info.kind != crate::settings::KIND_TRACK {
        return Ok(None);
    }
    let t: LocalSound =
        get(files, info.music).ok_or_else(|| format!("no track {:08X}", info.music))?;
    track_source(info.music, &t.bytes).map(Some)
}
