//! The music files' forms: pieces (segments, `DMSG`) and their tracks, bands (`DMBD`), styles
//! (`DMST`) with their parts and patterns, and scripts (`DMSC`) with the pieces they hold.
//!
//! Times are music time: 768 ticks a quarter note. A file names another (a style, a collection, a
//! piece to start) by its file name, which the data's file table resolves.

use super::riff::{self, Chunk, Le};

/// Ticks a quarter note.
pub const PPQ: i32 = 768;

/// A file a file names: by file name, with its object name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reference {
    pub file: Option<String>,
    pub name: Option<String>,
}

fn reference(c: Chunk<'_>) -> Reference {
    Reference {
        file: c.find(b"file").map(|f| f.wide_text()),
        name: c.find(b"name").map(|f| f.wide_text()),
    }
}

/// A time signature: beats a measure, the note a beat is (4 a quarter, 8 an eighth), and grids a
/// beat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimeSig {
    pub beats: u8,
    pub beat: u8,
    pub grids: u16,
}

impl Default for TimeSig {
    fn default() -> Self {
        Self {
            beats: 4,
            beat: 4,
            grids: 4,
        }
    }
}

impl TimeSig {
    fn read(le: Le<'_>, at: usize) -> Option<Self> {
        Some(Self {
            beats: le.u8(at)?,
            beat: le.u8(at + 1)?,
            grids: le.u16(at + 2)?,
        })
    }

    /// Ticks a beat.
    #[must_use]
    pub fn beat_ticks(&self) -> i32 {
        if self.beat == 0 {
            return PPQ;
        }
        PPQ * 4 / i32::from(self.beat)
    }

    /// Ticks a grid.
    #[must_use]
    pub fn grid_ticks(&self) -> i32 {
        self.beat_ticks() / i32::from(self.grids.max(1))
    }

    /// Ticks a measure.
    #[must_use]
    pub fn measure_ticks(&self) -> i32 {
        self.beat_ticks() * i32::from(self.beats.max(1))
    }
}

/// A sequence event: a note (status `0x9n`, with its length) or any other channel message, at a
/// time and offset, on a performance channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeqEvent {
    pub time: i32,
    pub duration: i32,
    pub pchannel: u32,
    pub offset: i16,
    pub status: u8,
    pub data1: u8,
    pub data2: u8,
}

/// A curve: a controller (or the pitch wheel, or pressure) moved from one value to another over a
/// time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Curve {
    pub start: i32,
    pub duration: i32,
    pub reset_duration: i32,
    pub pchannel: u32,
    pub offset: i16,
    pub from: i16,
    pub to: i16,
    pub reset: i16,
    /// 3 pitch bend, 4 controller, 5 mono pressure, 6 poly pressure (the curve event types).
    pub kind: u8,
    /// 0 linear, 1 instant, 2 exponential, 3 logarithmic, 4 sine.
    pub shape: u8,
    pub controller: u8,
    pub flags: u8,
}

/// An instrument a band sets on a channel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BandInstrument {
    pub patch: u32,
    pub pchannel: u32,
    /// Which settings it carries: 0x1 the patch, 0x20 pan, 0x40 volume, 0x80 transpose, 0x1000 the
    /// General MIDI set, 0x2000 the pitch wheel's range.
    pub flags: u32,
    pub pan: u8,
    pub volume: u8,
    pub transpose: i16,
    pub bend_range: i16,
    /// The collection it comes from (none: the General MIDI set).
    pub collection: Option<Reference>,
}

/// A band: the instruments of the channels.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Band {
    pub instruments: Vec<BandInstrument>,
}

/// A band's place in a band track: its logical time (and the earlier time it is sent at).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BandItem {
    pub time: i32,
    pub physical: i32,
    pub band: Band,
}

/// A piece a segment trigger track starts: at its time, with its play flags.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TriggerItem {
    pub time: i32,
    pub physical: i32,
    pub play_flags: u32,
    pub flags: u32,
    pub segment: Reference,
}

/// A command: the groove level (and fills, breaks, intros, ends) from a time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Command {
    pub time: i32,
    pub measure: u16,
    pub beat: u8,
    pub command: u8,
    pub groove: u8,
    pub groove_range: u8,
    pub repeat_mode: u8,
}

/// One chord of a chord's stack: its notes over its root, the scale and its root.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SubChord {
    pub chord_pattern: u32,
    pub scale_pattern: u32,
    pub inversion_points: u32,
    pub levels: u32,
    pub chord_root: u8,
    pub scale_root: u8,
}

/// A chord from a time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chord {
    pub time: i32,
    pub measure: u16,
    pub beat: u8,
    pub flags: u8,
    pub subchords: Vec<SubChord>,
}

/// A script routine called from a time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptCall {
    pub flags: u32,
    pub time: i32,
    pub physical: i32,
    pub script: Reference,
    pub routine: String,
}

/// A track's contents.
#[derive(Clone, Debug, PartialEq)]
pub enum TrackKind {
    Tempo(Vec<(i32, f64)>),
    TimeSig(Vec<(i32, TimeSig)>),
    Sequence {
        events: Vec<SeqEvent>,
        curves: Vec<Curve>,
    },
    Band(Vec<BandItem>),
    Trigger(Vec<TriggerItem>),
    /// Styles from their times.
    Style(Vec<(i32, Reference)>),
    Command(Vec<Command>),
    Chord {
        /// The key's root (top byte) and scale (low 24 bits).
        key: u32,
        chords: Vec<Chord>,
    },
    Marker(Vec<i32>),
    Script(Vec<ScriptCall>),
    /// A track this player does not read: its form type.
    Other([u8; 4]),
}

/// A track: its group bits, its configuration flags, and its contents.
#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    pub group: u32,
    pub config: Option<u32>,
    pub kind: TrackKind,
}

/// A piece of music (a segment).
#[derive(Clone, Debug, PartialEq)]
pub struct Segment {
    /// How many times it repeats after its first play (`u32::MAX`: for ever).
    pub repeats: u32,
    pub length: i32,
    pub play_start: i32,
    pub loop_start: i32,
    pub loop_end: i32,
    pub tracks: Vec<Track>,
}

/// Why a file did not read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormError(pub String);

impl std::fmt::Display for FormError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

fn err(s: impl Into<String>) -> FormError {
    FormError(s.into())
}

fn short(what: &str) -> FormError {
    FormError(format!("a short {what}"))
}

/// The tracks' class identifiers, by their distinguishing first word.
const TRACK_TEMPO: u32 = 0xD2AC_2885;
const TRACK_SEQUENCE: u32 = 0xD2AC_2886;
const TRACK_TIMESIG: u32 = 0xD2AC_2888;
const TRACK_CHORD: u32 = 0xD2AC_288B;
const TRACK_COMMAND: u32 = 0xD2AC_288C;
const TRACK_STYLE: u32 = 0xD2AC_288D;
const TRACK_BAND: u32 = 0xD2AC_2894;
const TRACK_SCRIPT: u32 = 0x4108_FA85;
const TRACK_MARKER: u32 = 0x55A8_FD00;
const TRACK_TRIGGER: u32 = 0xBAE4_D665;

/// Read a band (`DMBD`).
///
/// # Errors
/// An instrument entry too short.
pub fn band(c: Chunk<'_>) -> Result<Band, FormError> {
    let mut instruments = Vec::new();
    if let Some(list) = c.find(b"lbil") {
        for lbin in list.all(b"lbin") {
            let b = lbin
                .find(b"bins")
                .ok_or_else(|| err("a band instrument without settings"))?;
            let le = b.le();
            let s = || short("band instrument");
            instruments.push(BandInstrument {
                patch: le.u32(0).ok_or_else(s)?,
                pchannel: le.u32(24).ok_or_else(s)?,
                flags: le.u32(28).ok_or_else(s)?,
                pan: le.u8(32).ok_or_else(s)?,
                volume: le.u8(33).ok_or_else(s)?,
                transpose: le.i16(34).ok_or_else(s)?,
                bend_range: le.i16(40).unwrap_or(0),
                collection: lbin.find(b"DMRF").map(reference),
            });
        }
    }
    Ok(Band { instruments })
}

fn track(t: Chunk<'_>) -> Result<Track, FormError> {
    let h = t
        .find(b"trkh")
        .ok_or_else(|| err("a track without a header"))?;
    let hl = h.le();
    let class = hl.u32(0).ok_or_else(|| short("track header"))?;
    let group = hl.u32(20).ok_or_else(|| short("track header"))?;
    let config = t.find(b"trkx").and_then(|x| x.le().u32(0));
    // The track's data: the first chunk after the header and the configuration.
    let body = t
        .kids()
        .find(|k| !matches!(&k.tag, b"trkh" | b"trkx" | b"guid" | b"vers" | b"UNFO"));
    let kind = match (class, body) {
        (TRACK_TEMPO, Some(b)) => TrackKind::Tempo(
            b.le()
                .sized_items(16)
                .iter()
                .filter_map(|i| Some((Le(i).i32(0)?, Le(i).f64(8)?)))
                .collect(),
        ),
        (TRACK_TIMESIG, Some(b)) => {
            let items = b
                .find(b"tims")
                .ok_or_else(|| err("a time signature track without items"))?;
            TrackKind::TimeSig(
                items
                    .le()
                    .sized_items(8)
                    .iter()
                    .filter_map(|i| Some((Le(i).i32(0)?, TimeSig::read(Le(i), 4)?)))
                    .collect(),
            )
        }
        (TRACK_SEQUENCE, Some(b)) => {
            let mut events = Vec::new();
            let mut curves = Vec::new();
            for k in riff::Chunks::of(b.data) {
                match &k.tag {
                    b"evtl" => {
                        for i in k.le().sized_items(20) {
                            let l = Le(i);
                            events.push(SeqEvent {
                                time: l.i32(0).ok_or_else(|| short("sequence event"))?,
                                duration: l.i32(4).ok_or_else(|| short("sequence event"))?,
                                pchannel: l.u32(8).ok_or_else(|| short("sequence event"))?,
                                offset: l.i16(12).ok_or_else(|| short("sequence event"))?,
                                status: l.u8(14).ok_or_else(|| short("sequence event"))?,
                                data1: l.u8(15).ok_or_else(|| short("sequence event"))?,
                                data2: l.u8(16).ok_or_else(|| short("sequence event"))?,
                            });
                        }
                    }
                    b"curl" => {
                        for i in k.le().sized_items(28) {
                            curves.push(seq_curve(Le(i)).ok_or_else(|| short("sequence curve"))?);
                        }
                    }
                    _ => {}
                }
            }
            TrackKind::Sequence { events, curves }
        }
        (TRACK_BAND, Some(b)) => {
            let mut items = Vec::new();
            if let Some(list) = b.find(b"lbdl") {
                for lbnd in list.all(b"lbnd") {
                    let (time, physical) = if let Some(h2) = lbnd.find(b"bd2h") {
                        (h2.le().i32(0).unwrap_or(0), h2.le().i32(4).unwrap_or(0))
                    } else if let Some(h1) = lbnd.find(b"bdih") {
                        let t = h1.le().i32(0).unwrap_or(0);
                        (t, t)
                    } else {
                        (0, 0)
                    };
                    let bd = lbnd
                        .find(b"DMBD")
                        .ok_or_else(|| err("a band item without a band"))?;
                    items.push(BandItem {
                        time,
                        physical,
                        band: band(bd)?,
                    });
                }
            }
            TrackKind::Band(items)
        }
        (TRACK_TRIGGER, Some(b)) => {
            let mut items = Vec::new();
            if let Some(list) = b.find(b"lsgl") {
                for item in list.all(b"lseg") {
                    let h = item
                        .find(b"sgih")
                        .ok_or_else(|| err("a trigger without a header"))?;
                    let l = h.le();
                    let s = || short("trigger header");
                    items.push(TriggerItem {
                        time: l.i32(0).ok_or_else(s)?,
                        physical: l.i32(4).ok_or_else(s)?,
                        play_flags: l.u32(8).ok_or_else(s)?,
                        flags: l.u32(12).ok_or_else(s)?,
                        segment: item.find(b"DMRF").map(reference).unwrap_or_default(),
                    });
                }
            }
            TrackKind::Trigger(items)
        }
        (TRACK_STYLE, Some(b)) => {
            let mut items = Vec::new();
            for strf in b.all(b"strf") {
                let time = strf.find(b"stmp").and_then(|s| s.le().i32(0)).unwrap_or(0);
                if let Some(r) = strf.find(b"DMRF") {
                    items.push((time, reference(r)));
                }
            }
            TrackKind::Style(items)
        }
        (TRACK_COMMAND, Some(b)) => TrackKind::Command(
            b.le()
                .sized_items(11)
                .iter()
                .filter_map(|i| {
                    let l = Le(i);
                    Some(Command {
                        time: l.i32(0)?,
                        measure: l.u16(4)?,
                        beat: l.u8(6)?,
                        command: l.u8(7)?,
                        groove: l.u8(8)?,
                        groove_range: l.u8(9)?,
                        repeat_mode: l.u8(10)?,
                    })
                })
                .collect(),
        ),
        (TRACK_CHORD, Some(b)) => {
            let key = b.find(b"crdh").and_then(|h| h.le().u32(0)).unwrap_or(0);
            let mut chords = Vec::new();
            for c in b.all(b"crdb") {
                chords.push(chord(c.le()).ok_or_else(|| short("chord"))?);
            }
            TrackKind::Chord { key, chords }
        }
        (TRACK_MARKER, Some(b)) => {
            let mut marks = Vec::new();
            for k in b.kids() {
                if &k.tag == b"vals" {
                    for i in k.le().sized_items(4) {
                        if let Some(t) = Le(i).i32(0) {
                            marks.push(t);
                        }
                    }
                }
            }
            TrackKind::Marker(marks)
        }
        (TRACK_SCRIPT, Some(b)) => {
            let mut calls = Vec::new();
            if let Some(list) = b.find(b"scrl") {
                for e in list.all(b"scre") {
                    let h = e
                        .find(b"scrh")
                        .ok_or_else(|| err("a script call without a header"))?;
                    let l = h.le();
                    let s = || short("script call");
                    calls.push(ScriptCall {
                        flags: l.u32(0).ok_or_else(s)?,
                        time: l.i32(4).ok_or_else(s)?,
                        physical: l.i32(8).ok_or_else(s)?,
                        script: e.find(b"DMRF").map(reference).unwrap_or_default(),
                        routine: e.find(b"scrn").map(|n| n.wide_text()).unwrap_or_default(),
                    });
                }
            }
            TrackKind::Script(calls)
        }
        (_, b) => TrackKind::Other(b.map_or(*b"    ", |b| b.tag)),
    };
    Ok(Track {
        group,
        config,
        kind,
    })
}

fn seq_curve(l: Le<'_>) -> Option<Curve> {
    Some(Curve {
        start: l.i32(0)?,
        duration: l.i32(4)?,
        reset_duration: l.i32(8)?,
        pchannel: l.u32(12)?,
        offset: l.i16(16)?,
        from: l.i16(18)?,
        to: l.i16(20)?,
        reset: l.i16(22)?,
        kind: l.u8(24)?,
        shape: l.u8(25)?,
        controller: l.u8(26)?,
        flags: l.u8(27)?,
    })
}

fn chord(l: Le<'_>) -> Option<Chord> {
    // The chord's own size first, then the chord, then the sub-chords' size, count and items.
    let size = l.u32(0)? as usize;
    let at = 4;
    let time = l.i32(at + 32)?;
    let measure = l.u16(at + 36)?;
    let beat = l.u8(at + 38)?;
    let flags = l.u8(at + 39)?;
    let mut o = 4 + size;
    let count = l.u32(o)? as usize;
    o += 4;
    let sub = l.u32(o)? as usize;
    o += 4;
    let mut subchords = Vec::with_capacity(count);
    for k in 0..count {
        let b = o + k * sub;
        subchords.push(SubChord {
            chord_pattern: l.u32(b)?,
            scale_pattern: l.u32(b + 4)?,
            inversion_points: l.u32(b + 8)?,
            levels: l.u32(b + 12)?,
            chord_root: l.u8(b + 16)?,
            scale_root: l.u8(b + 17)?,
        });
    }
    Some(Chord {
        time,
        measure,
        beat,
        flags,
        subchords,
    })
}

/// Read a piece.
///
/// # Errors
/// Not a `DMSG` file, or a part of it that does not read.
pub fn segment(bytes: &[u8]) -> Result<Segment, FormError> {
    let root = riff::root(bytes).ok_or_else(|| err("not a RIFF file"))?;
    segment_of(root)
}

/// A piece from its `DMSG` chunk (a file's root, or one a script holds).
///
/// # Errors
/// Not a `DMSG` chunk, or a part of it that does not read.
pub fn segment_of(root: Chunk<'_>) -> Result<Segment, FormError> {
    if &root.tag != b"DMSG" {
        return Err(err("not a piece"));
    }
    let h = root
        .find(b"segh")
        .ok_or_else(|| err("a piece without a header"))?;
    let l = h.le();
    let s = || short("piece header");
    let mut tracks = Vec::new();
    if let Some(list) = root.find(b"trkl") {
        for t in list.all(b"DMTK") {
            tracks.push(track(t)?);
        }
    }
    Ok(Segment {
        repeats: l.u32(0).ok_or_else(s)?,
        length: l.i32(4).ok_or_else(s)?,
        play_start: l.i32(8).ok_or_else(s)?,
        loop_start: l.i32(12).ok_or_else(s)?,
        loop_end: l.i32(16).ok_or_else(s)?,
        tracks,
    })
}

/// A style part's note: at a grid (and an offset), in some variations, with its length, its
/// music value (a key under the fixed play mode, else a place relative to the chord and scale),
/// its velocity and the ranges each may be varied by, and a play mode of its own (16: the part's).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StyleNote {
    pub grid: i32,
    pub variations: u32,
    pub duration: i32,
    pub offset: i16,
    pub music_value: u16,
    pub velocity: u8,
    pub time_range: u8,
    pub duration_range: u8,
    pub velocity_range: u8,
    pub inversion: u8,
    pub play_mode: u8,
    pub note_flags: u8,
}

/// A style part's curve.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StyleCurve {
    pub grid: i32,
    pub variations: u32,
    pub duration: i32,
    pub reset_duration: i32,
    pub offset: i16,
    pub from: i16,
    pub to: i16,
    pub reset: i16,
    pub kind: u8,
    pub shape: u8,
    pub controller: u8,
    pub flags: u8,
}

/// A style part: the notes of one instrument's line, in up to 32 variations.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub timesig: TimeSig,
    /// For each variation, the chords it may play over (its bit 31 set: disabled).
    pub variation_choices: [u32; 32],
    pub guid: [u8; 16],
    pub measures: u16,
    pub play_mode: u8,
    pub invert_upper: u8,
    pub invert_lower: u8,
    pub flags: u32,
    pub notes: Vec<StyleNote>,
    pub curves: Vec<StyleCurve>,
}

/// A pattern's use of a part: on a performance channel, with its variation choice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PartRef {
    pub guid: [u8; 16],
    pub lock: u8,
    pub subchord_level: u8,
    pub priority: u8,
    /// 0 in order, 1 at random, 2 a random start then in order, 3 at random without repeating,
    /// 4 a random row.
    pub variation_mode: u8,
    pub pchannel: u32,
}

/// A pattern: the groove range it plays in, its length, and the parts it plays.
#[derive(Clone, Debug, PartialEq)]
pub struct Pattern {
    pub timesig: TimeSig,
    pub groove: (u8, u8),
    pub embellishment: u16,
    pub measures: u16,
    pub flags: u32,
    pub rhythm: Vec<u32>,
    pub parts: Vec<PartRef>,
}

/// A style.
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    pub timesig: TimeSig,
    pub tempo: f64,
    pub bands: Vec<Band>,
    pub parts: Vec<Part>,
    pub patterns: Vec<Pattern>,
}

impl Style {
    /// A part by its identifier.
    #[must_use]
    pub fn part(&self, guid: &[u8; 16]) -> Option<&Part> {
        self.parts.iter().find(|p| &p.guid == guid)
    }
}

fn part(c: Chunk<'_>) -> Result<Part, FormError> {
    let h = c
        .find(b"prth")
        .ok_or_else(|| err("a part without a header"))?;
    let l = h.le();
    let s = || short("part header");
    let mut variation_choices = [0u32; 32];
    for (k, v) in variation_choices.iter_mut().enumerate() {
        *v = l.u32(4 + 4 * k).ok_or_else(s)?;
    }
    let mut notes = Vec::new();
    if let Some(n) = c.find(b"note") {
        for i in n.le().sized_items(23) {
            let l = Le(i);
            let s = || short("style note");
            notes.push(StyleNote {
                grid: l.i32(0).ok_or_else(s)?,
                variations: l.u32(4).ok_or_else(s)?,
                duration: l.i32(8).ok_or_else(s)?,
                offset: l.i16(12).ok_or_else(s)?,
                music_value: l.u16(14).ok_or_else(s)?,
                velocity: l.u8(16).ok_or_else(s)?,
                time_range: l.u8(17).ok_or_else(s)?,
                duration_range: l.u8(18).ok_or_else(s)?,
                velocity_range: l.u8(19).ok_or_else(s)?,
                inversion: l.u8(20).ok_or_else(s)?,
                play_mode: l.u8(21).ok_or_else(s)?,
                note_flags: l.u8(22).unwrap_or(0),
            });
        }
    }
    let mut curves = Vec::new();
    if let Some(n) = c.find(b"crve") {
        for i in n.le().sized_items(28) {
            let l = Le(i);
            let s = || short("style curve");
            curves.push(StyleCurve {
                grid: l.i32(0).ok_or_else(s)?,
                variations: l.u32(4).ok_or_else(s)?,
                duration: l.i32(8).ok_or_else(s)?,
                reset_duration: l.i32(12).ok_or_else(s)?,
                offset: l.i16(16).ok_or_else(s)?,
                from: l.i16(18).ok_or_else(s)?,
                to: l.i16(20).ok_or_else(s)?,
                reset: l.i16(22).ok_or_else(s)?,
                kind: l.u8(24).ok_or_else(s)?,
                shape: l.u8(25).ok_or_else(s)?,
                controller: l.u8(26).ok_or_else(s)?,
                flags: l.u8(27).ok_or_else(s)?,
            });
        }
    }
    Ok(Part {
        timesig: TimeSig::read(l, 0).ok_or_else(s)?,
        variation_choices,
        guid: l.guid(132).ok_or_else(s)?,
        measures: l.u16(148).ok_or_else(s)?,
        play_mode: l.u8(150).ok_or_else(s)?,
        invert_upper: l.u8(151).ok_or_else(s)?,
        invert_lower: l.u8(152).ok_or_else(s)?,
        flags: l.u32(156).unwrap_or(0),
        notes,
        curves,
    })
}

fn pattern(c: Chunk<'_>) -> Result<Pattern, FormError> {
    let h = c
        .find(b"ptnh")
        .ok_or_else(|| err("a pattern without a header"))?;
    let l = h.le();
    let s = || short("pattern header");
    let rhythm = c
        .find(b"rhtm")
        .map(|r| {
            (0..r.data.len() / 4)
                .filter_map(|k| r.le().u32(4 * k))
                .collect()
        })
        .unwrap_or_default();
    let mut parts = Vec::new();
    for pref in c.all(b"pref") {
        let p = pref
            .find(b"prfc")
            .ok_or_else(|| err("a part reference without its settings"))?;
        let l = p.le();
        let s = || short("part reference");
        parts.push(PartRef {
            guid: l.guid(0).ok_or_else(s)?,
            lock: l.u8(18).ok_or_else(s)?,
            subchord_level: l.u8(19).ok_or_else(s)?,
            priority: l.u8(20).ok_or_else(s)?,
            variation_mode: l.u8(21).ok_or_else(s)?,
            pchannel: l.u32(24).ok_or_else(s)?,
        });
    }
    Ok(Pattern {
        timesig: TimeSig::read(l, 0).ok_or_else(s)?,
        groove: (l.u8(4).ok_or_else(s)?, l.u8(5).ok_or_else(s)?),
        embellishment: l.u16(6).ok_or_else(s)?,
        measures: l.u16(8).ok_or_else(s)?,
        flags: l.u32(12).unwrap_or(0),
        rhythm,
        parts,
    })
}

/// Read a style.
///
/// # Errors
/// Not a `DMST` file, or a part of it that does not read.
pub fn style(bytes: &[u8]) -> Result<Style, FormError> {
    let root = riff::root(bytes).ok_or_else(|| err("not a RIFF file"))?;
    if &root.tag != b"DMST" {
        return Err(err("not a style"));
    }
    let h = root
        .find(b"styh")
        .ok_or_else(|| err("a style without a header"))?;
    let l = h.le();
    let mut bands = Vec::new();
    let mut parts = Vec::new();
    let mut patterns = Vec::new();
    for k in root.kids() {
        match &k.tag {
            b"DMBD" => bands.push(band(k)?),
            b"part" => parts.push(part(k)?),
            b"pttn" => patterns.push(pattern(k)?),
            _ => {}
        }
    }
    Ok(Style {
        timesig: TimeSig::read(l, 0).ok_or_else(|| short("style header"))?,
        tempo: l.f64(4).ok_or_else(|| short("style header"))?,
        bands,
        parts,
        patterns,
    })
}

/// A piece a script holds, by the name the script calls it.
#[derive(Clone, Debug, PartialEq)]
pub struct ScriptObject {
    pub alias: String,
    pub segment: Option<Segment>,
    pub reference: Option<Reference>,
}

/// A script: its language, its text, and the pieces it holds.
#[derive(Clone, Debug, PartialEq)]
pub struct Script {
    pub language: String,
    pub source: String,
    pub objects: Vec<ScriptObject>,
}

/// Read a script.
///
/// # Errors
/// Not a `DMSC` file, or a piece it holds that does not read.
pub fn script(bytes: &[u8]) -> Result<Script, FormError> {
    let root = riff::root(bytes).ok_or_else(|| err("not a RIFF file"))?;
    if &root.tag != b"DMSC" {
        return Err(err("not a script"));
    }
    let mut objects = Vec::new();
    if let Some(list) = root.find(b"DMCN").and_then(|c| c.find(b"cosl")) {
        for o in list.all(b"cobl") {
            let alias = o.find(b"coba").map(|a| a.wide_text()).unwrap_or_default();
            let segment = match o.find(b"DMSG") {
                Some(s) => Some(segment_of(s)?),
                None => None,
            };
            objects.push(ScriptObject {
                alias,
                segment,
                reference: o.find(b"DMRF").map(reference),
            });
        }
    }
    Ok(Script {
        language: root
            .find(b"scla")
            .map(|c| c.wide_text())
            .unwrap_or_default(),
        source: root
            .find(b"scsr")
            .map(|c| c.wide_text())
            .unwrap_or_default(),
        objects,
    })
}
