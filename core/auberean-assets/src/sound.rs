//! Sounds: waves (`.wav`, `0x0C000000`+), encoded waves (`.wv3`, local `0x61000000`+), sound and
//! music settings (`.sif`, `.mif`), music (`.mid`) and sound environments (`.snv`).

use auberean_dat::reader::Result;
use auberean_dat::Reader;

use crate::{Context, Decode};

/// A sound: a wave format block and the sample data.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Wave {
    /// The format block as stored (a Windows wave format: tag, channels, rate, byte rate, block
    /// alignment, bits, and any extra bytes).
    pub format: Vec<u8>,
    pub data: Vec<u8>,
}

impl Wave {
    /// The format tag (1 PCM, 2 ADPCM, 0x55 MP3...).
    #[must_use]
    pub fn tag(&self) -> u16 {
        self.format
            .get(..2)
            .map_or(0, |b| u16::from_le_bytes([b[0], b[1]]))
    }
}

impl Decode for Wave {
    /// The id, the format block's size, the data size, the format block, the data. The block's
    /// size comes from the record, not from the block's own extra-size field.
    // D2-07745
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        let format_size = r.u32()? as usize;
        let data_size = r.u32()? as usize;
        let format = r.bytes(format_size)?.to_vec();
        r.ensure(format.len() >= 16, || {
            format!("format block of {format_size} bytes")
        })?;
        let data = r.bytes(data_size)?.to_vec();
        Ok(Self { format, data })
    }
}

/// An encoded sound kept in a language file: a sized blob.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct LocalSound {
    pub bytes: Vec<u8>,
}

impl Decode for LocalSound {
    /// The id, a byte count, the bytes.
    // D2-07733
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        let n = r.count(1)?;
        Ok(Self {
            bytes: r.bytes(n)?.to_vec(),
        })
    }
}

/// How a sound plays: flags, a random pitch range, priority, the distance it starts fading at,
/// and the wave it plays.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct SoundInfo {
    pub flags: u32,
    pub pitch_high: f32,
    pub pitch_low: f32,
    pub priority: f32,
    pub min_distance: f32,
    pub sound: u32,
}

impl Decode for SoundInfo {
    // D2-16813
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        Ok(Self {
            flags: r.u32()?,
            pitch_high: r.f32()?,
            pitch_low: r.f32()?,
            priority: r.f32()?,
            min_distance: r.f32()?,
            sound: r.u32()?,
        })
    }
}

/// How a piece of music plays: flags, priority, fade distance, kind, and the music it plays.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct MusicInfo {
    pub flags: u32,
    pub priority: f32,
    pub min_distance: f32,
    pub kind: u32,
    pub music: u32,
}

impl Decode for MusicInfo {
    // D2-16805
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        Ok(Self {
            flags: r.u32()?,
            priority: r.f32()?,
            min_distance: r.f32()?,
            kind: r.u32()?,
            music: r.u32()?,
        })
    }
}

/// A piece of music (`.mid`, `0x24000000`+): the music file's bytes, its kind, and the records it
/// uses.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Music {
    pub bytes: Vec<u8>,
    pub kind: u32,
    pub references: Vec<u32>,
}

impl Decode for Music {
    /// The id, a byte count, the bytes, the kind word, then a counted list of record ids.
    // D2-07747
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        let n = r.count(1)?;
        let bytes = r.bytes(n)?.to_vec();
        let kind = r.u32()?;
        let references = r.u32_list()?;
        Ok(Self {
            bytes,
            kind,
            references,
        })
    }
}

/// A sound environment (`.snv`, `0x29000000`+): how a place colors the sounds in it, as the
/// listener settings of the EAX reverb interface. A mask says which settings are stored; the rest
/// keep their defaults.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct SoundEnvironment {
    /// Which settings follow: one bit per setting (see [`SOUND_ENVIRONMENT_FIELDS`]).
    pub mask: u32,
    /// The stored settings, in stored order: their mask bit and their raw word (an integer or a
    /// float by the setting).
    pub settings: Vec<(u32, u32)>,
}

/// The settings a sound environment can store, in stored order: the mask bit and the setting.
pub const SOUND_ENVIRONMENT_FIELDS: [(u32, &str); 22] = [
    (0x1, "room"),
    (0x2, "room high frequencies"),
    (0x80_0000, "room low frequencies"),
    (0x4, "room rolloff factor"),
    (0x8, "decay time"),
    (0x10, "decay high-frequency ratio"),
    (0x20, "decay low-frequency ratio"),
    (0x40, "reflections"),
    (0x80, "reflections delay"),
    (0x200, "reverb"),
    (0x400, "reverb delay"),
    (0x1000, "echo time"),
    (0x2000, "echo depth"),
    (0x4000, "modulation time"),
    (0x8000, "modulation depth"),
    (0x1_0000, "environment"),
    (0x2_0000, "environment size"),
    (0x4_0000, "environment diffusion"),
    (0x8_0000, "air absorption high frequencies"),
    (0x20_0000, "low-frequency reference"),
    (0x10_0000, "high-frequency reference"),
    (0x40_0000, "flags"),
];

impl SoundEnvironment {
    /// The raw word of a setting, by its mask bit, when stored.
    #[must_use]
    pub fn setting(&self, bit: u32) -> Option<u32> {
        self.settings
            .iter()
            .find(|(b, _)| *b == bit)
            .map(|(_, v)| *v)
    }
}

impl Decode for SoundEnvironment {
    /// The id, the mask, then one word for each set bit, in the order of
    /// [`SOUND_ENVIRONMENT_FIELDS`].
    // D2-07964
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        let mask = r.u32()?;
        let mut settings = Vec::new();
        for (bit, _) in SOUND_ENVIRONMENT_FIELDS {
            if mask & bit != 0 {
                settings.push((bit, r.u32()?));
            }
        }
        Ok(Self { mask, settings })
    }
}
