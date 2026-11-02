//! Instrument collections (DLS, the Downloadable Sounds format, levels 1 and 2): instruments by bank
//! and program, each a set of regions over key and velocity ranges, each region playing a wave with
//! its root note, tuning, attenuation and loops, shaped by articulation (connections from sources
//! such as an envelope or the key's velocity to destinations such as the pitch or the level).

use std::sync::Arc;

use super::riff::{self, Chunk};

/// A connection's source: none (a constant).
pub const SRC_NONE: u16 = 0x0000;
/// The modulation LFO.
pub const SRC_LFO: u16 = 0x0001;
/// The note's velocity.
pub const SRC_VELOCITY: u16 = 0x0002;
/// The note's key number.
pub const SRC_KEY: u16 = 0x0003;
/// The volume envelope.
pub const SRC_EG1: u16 = 0x0004;
/// The modulation envelope.
pub const SRC_EG2: u16 = 0x0005;
/// The pitch wheel.
pub const SRC_PITCH_WHEEL: u16 = 0x0006;
/// The channel's pressure.
pub const SRC_CHANNEL_PRESSURE: u16 = 0x0008;
/// The vibrato LFO.
pub const SRC_VIBRATO: u16 = 0x0009;
/// Controller 1, the modulation wheel.
pub const SRC_CC1: u16 = 0x0081;

/// Destinations.
pub mod dst {
    pub const ATTENUATION: u16 = 0x0001;
    pub const PITCH: u16 = 0x0003;
    pub const PAN: u16 = 0x0004;
    pub const LFO_FREQUENCY: u16 = 0x0104;
    pub const LFO_DELAY: u16 = 0x0105;
    pub const VIB_FREQUENCY: u16 = 0x0114;
    pub const VIB_DELAY: u16 = 0x0115;
    pub const EG1_ATTACK: u16 = 0x0206;
    pub const EG1_DECAY: u16 = 0x0207;
    pub const EG1_RELEASE: u16 = 0x0209;
    pub const EG1_SUSTAIN: u16 = 0x020A;
    pub const EG1_DELAY: u16 = 0x020B;
    pub const EG1_HOLD: u16 = 0x020C;
    pub const EG1_SHUTDOWN: u16 = 0x020D;
    pub const EG2_ATTACK: u16 = 0x030A;
    pub const EG2_DECAY: u16 = 0x030B;
    pub const EG2_RELEASE: u16 = 0x030D;
    pub const EG2_SUSTAIN: u16 = 0x030E;
    pub const EG2_DELAY: u16 = 0x030F;
    pub const EG2_HOLD: u16 = 0x0310;
    pub const FILTER_CUTOFF: u16 = 0x0500;
    pub const FILTER_Q: u16 = 0x0501;
}

/// The bank bit that marks a drum kit.
pub const DRUM_BANK: u32 = 0x8000_0000;

/// One connection: a source (times a control) times a scale, added to a destination.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Connection {
    pub source: u16,
    pub control: u16,
    pub destination: u16,
    pub transform: u16,
    /// The amount, in the destination's units times 65,536 (time cents, pitch cents, tenths of a
    /// percent, tenths of a decibel).
    pub scale: i32,
}

/// A set of connections (an `art1` or `art2` list).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Articulation {
    pub connections: Vec<Connection>,
    /// Written as level 2 (`art2`), whose defaults are level 2's.
    pub level2: bool,
}

/// A wave's loop: its type (0 forward), start and length in samples.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Loop {
    pub kind: u32,
    pub start: u32,
    pub length: u32,
}

/// How a wave plays: the key it plays at its own pitch, a fine tuning in cents, an attenuation
/// (in hundredths of a decibel times 6,553.6, as stored), and its loops.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sampler {
    pub unity_note: u16,
    pub fine_tune: i16,
    pub attenuation: i32,
    pub options: u32,
    pub loops: Vec<Loop>,
}

/// A region: the keys and velocities it answers, its wave and how it plays.
#[derive(Clone, Debug, PartialEq)]
pub struct Region {
    pub keys: (u16, u16),
    pub velocities: (u16, u16),
    /// A note on a key already sounding in this region does not stop the one sounding.
    pub self_non_exclusive: bool,
    /// Notes of the same non-zero group stop one another.
    pub key_group: u16,
    /// The wave, by its index in the collection's pool table.
    pub wave: u32,
    /// The region's own sampler settings, which win over the wave's.
    pub sampler: Option<Sampler>,
    /// The region's own articulation, which wins over the instrument's.
    pub articulation: Option<Arc<Articulation>>,
}

/// An instrument.
#[derive(Clone, Debug, PartialEq)]
pub struct Instrument {
    /// The bank (MSB and LSB, with [`DRUM_BANK`] for a drum kit).
    pub bank: u32,
    pub program: u32,
    pub regions: Vec<Region>,
    pub articulation: Option<Arc<Articulation>>,
}

impl Instrument {
    /// The patch number a band names it by: the bank's MSB and LSB and the program.
    #[must_use]
    pub fn patch(&self) -> u32 {
        ((self.bank & 0x7F00) << 8)
            | ((self.bank & 0x7F) << 8)
            | (self.program & 0x7F)
            | (self.bank & DRUM_BANK)
    }
}

/// A wave of the pool: mono samples in -1..1 at their rate, and its own sampler settings.
#[derive(Clone, Debug, PartialEq)]
pub struct Wave {
    pub rate: u32,
    pub samples: Arc<[f32]>,
    pub sampler: Option<Sampler>,
}

/// A collection.
#[derive(Clone, Debug, PartialEq)]
pub struct Collection {
    pub instruments: Vec<Instrument>,
    /// The pool's waves, in pool-table order.
    pub waves: Vec<Wave>,
}

impl Collection {
    /// The instrument a band's patch names.
    #[must_use]
    pub fn instrument(&self, patch: u32) -> Option<&Instrument> {
        self.instruments.iter().find(|i| i.patch() == patch)
    }
}

/// Why a collection did not read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DlsError(pub String);

impl std::fmt::Display for DlsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

fn err(s: &str) -> DlsError {
    DlsError(s.to_owned())
}

fn articulation(list: Chunk<'_>) -> Option<Arc<Articulation>> {
    let (c, level2) = match list.find(b"art2") {
        Some(c) => (c, true),
        None => (list.find(b"art1")?, false),
    };
    let le = c.le();
    let header = le.u32(0)? as usize;
    let n = le.u32(4)? as usize;
    let mut connections = Vec::with_capacity(n);
    for k in 0..n {
        let at = header + 12 * k;
        connections.push(Connection {
            source: le.u16(at)?,
            control: le.u16(at + 2)?,
            destination: le.u16(at + 4)?,
            transform: le.u16(at + 6)?,
            scale: le.i32(at + 8)?,
        });
    }
    Some(Arc::new(Articulation {
        connections,
        level2,
    }))
}

fn sampler(c: Chunk<'_>) -> Option<Sampler> {
    let le = c.le();
    let header = le.u32(0)? as usize;
    let n = le.u32(16)? as usize;
    let mut loops = Vec::with_capacity(n);
    for k in 0..n {
        let at = header + 16 * k;
        loops.push(Loop {
            kind: le.u32(at + 4)?,
            start: le.u32(at + 8)?,
            length: le.u32(at + 12)?,
        });
    }
    Some(Sampler {
        unity_note: le.u16(4)?,
        fine_tune: le.i16(6)?,
        attenuation: le.i32(8)?,
        options: le.u32(12)?,
        loops,
    })
}

/// An articulation list of an instrument or a region: `lar2` (level 2) or `lart` (level 1).
fn articulation_of(c: Chunk<'_>) -> Option<Arc<Articulation>> {
    c.find(b"lar2")
        .and_then(articulation)
        .or_else(|| c.find(b"lart").and_then(articulation))
}

fn region(c: Chunk<'_>) -> Result<Region, DlsError> {
    let h = c
        .find(b"rgnh")
        .ok_or_else(|| err("a region without a header"))?;
    let le = h.le();
    let bad = || err("a short region header");
    let link = c
        .find(b"wlnk")
        .ok_or_else(|| err("a region without a wave link"))?;
    Ok(Region {
        keys: (le.u16(0).ok_or_else(bad)?, le.u16(2).ok_or_else(bad)?),
        velocities: (le.u16(4).ok_or_else(bad)?, le.u16(6).ok_or_else(bad)?),
        self_non_exclusive: le.u16(8).ok_or_else(bad)? & 1 != 0,
        key_group: le.u16(10).ok_or_else(bad)?,
        wave: link.le().u32(8).ok_or_else(|| err("a short wave link"))?,
        sampler: c.find(b"wsmp").and_then(sampler),
        articulation: articulation_of(c),
    })
}

fn wave(c: Chunk<'_>) -> Result<Wave, DlsError> {
    let f = c
        .find(b"fmt ")
        .ok_or_else(|| err("a wave without a format"))?;
    let d = c.find(b"data").ok_or_else(|| err("a wave without data"))?;
    let le = f.le();
    let bad = || err("a short wave format");
    let tag = le.u16(0).ok_or_else(bad)?;
    let channels = usize::from(le.u16(2).ok_or_else(bad)?.max(1));
    let rate = le.u32(4).ok_or_else(bad)?;
    let bits = le.u16(14).ok_or_else(bad)?;
    if tag != 1 {
        return Err(DlsError(format!(
            "wave format {tag:#x} is not plain samples"
        )));
    }
    // The first channel of each frame.
    let samples: Vec<f32> = match bits {
        8 => d
            .data
            .chunks_exact(channels)
            .map(|f| (f32::from(f[0]) - 128.0) / 128.0)
            .collect(),
        16 => d
            .data
            .chunks_exact(2 * channels)
            .map(|f| f32::from(i16::from_le_bytes([f[0], f[1]])) / 32768.0)
            .collect(),
        b => return Err(DlsError(format!("{b}-bit samples"))),
    };
    Ok(Wave {
        rate,
        samples: samples.into(),
        sampler: c.find(b"wsmp").and_then(sampler),
    })
}

/// Read a collection.
///
/// # Errors
/// Not a `DLS ` RIFF file, or a part of it that does not read.
pub fn parse(bytes: &[u8]) -> Result<Collection, DlsError> {
    let root = riff::root(bytes).ok_or_else(|| err("not a RIFF file"))?;
    if &root.tag != b"DLS " {
        return Err(err("not an instrument collection"));
    }
    let mut instruments = Vec::new();
    if let Some(lins) = root.find(b"lins") {
        for ins in lins.all(b"ins ") {
            let h = ins
                .find(b"insh")
                .ok_or_else(|| err("an instrument without a header"))?;
            let le = h.le();
            let bad = || err("a short instrument header");
            let mut regions = Vec::new();
            if let Some(lrgn) = ins.find(b"lrgn") {
                for r in lrgn
                    .kids()
                    .filter(|k| &k.tag == b"rgn " || &k.tag == b"rgn2")
                {
                    regions.push(region(r)?);
                }
            }
            instruments.push(Instrument {
                bank: le.u32(4).ok_or_else(bad)?,
                program: le.u32(8).ok_or_else(bad)?,
                regions,
                articulation: articulation_of(ins),
            });
        }
    }
    // The pool table's cues are offsets into the wave pool; the waves are read in pool order and
    // each cue resolved to the wave at its offset.
    let mut waves = Vec::new();
    let pool = root.find(b"wvpl").ok_or_else(|| err("no wave pool"))?;
    let mut offsets = Vec::new();
    {
        let base = pool.data.as_ptr() as usize;
        for w in pool.all(b"wave") {
            // A list's data starts 12 bytes after its header (code, size, form type).
            offsets.push(w.data.as_ptr() as usize - base - 12);
            waves.push(wave(w)?);
        }
    }
    let table = root.find(b"ptbl").ok_or_else(|| err("no pool table"))?;
    let le = table.le();
    let header = le.u32(0).ok_or_else(|| err("a short pool table"))? as usize;
    let cues = le.u32(4).ok_or_else(|| err("a short pool table"))? as usize;
    let mut ordered = Vec::with_capacity(cues);
    for k in 0..cues {
        let off = le
            .u32(header + 4 * k)
            .ok_or_else(|| err("a short pool table"))? as usize;
        let i = offsets
            .iter()
            .position(|&o| o == off)
            .ok_or_else(|| DlsError(format!("pool cue {k} names no wave")))?;
        ordered.push(waves[i].clone());
    }
    Ok(Collection {
        instruments,
        waves: ordered,
    })
}
