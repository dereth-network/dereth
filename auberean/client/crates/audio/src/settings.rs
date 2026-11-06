//! What a request becomes for its setting: whether it loops, its type and track, its priority and
//! the distance it starts fading at, its length, when it starts and stops, and the path it plays
//! on. The flag bits are the settings' own.

use auberean_assets::sound::{MusicInfo, SoundInfo};

use crate::request::{Channel, Looping, PathKind, PlayRequest, PlayType, Timing, Track};

/// A sound setting's bit: it loops.
pub const SOUND_LOOPS: u32 = 0x1;
/// A sound setting's bit: one at a time, counted against the budget (else alongside anything,
/// counted).
pub const SOUND_SELF_EXCLUSIVE: u32 = 0x2;
/// A sound setting's bit: placed in the world.
pub const SOUND_PLACED: u32 = 0x4;

/// A music setting's bit: it loops (when it does not play on the primary track).
pub const MUSIC_LOOPS: u32 = 0x1;
/// A music setting's bit: the primary track (never looped by the music's player; the primary
/// track replays it).
pub const MUSIC_PRIMARY: u32 = 0x2;
/// A music setting's bit: one at a time (else alongside anything).
pub const MUSIC_SELF_EXCLUSIVE: u32 = 0x4;
/// A music setting's bit: placed in the world.
pub const MUSIC_PLACED: u32 = 0x10;
/// A music setting's bit: played by an effect, it follows the music slider (else the player
/// music's).
pub const MUSIC_DEFAULT_CHANNEL_MUSIC: u32 = 0x4_0000;

/// A music setting's kind: an encoded track (an MP3, or one PCM wave, in a language file).
pub const KIND_TRACK: u32 = 0x4000_0001;
/// A music setting's kind: a piece of synthesized music (a segment of instruments).
pub const KIND_SEGMENT: u32 = 0x8000_0001;

/// The setting a request plays.
#[derive(Clone, Copy, Debug)]
pub enum Setting<'a> {
    Sound(&'a SoundInfo),
    Music(&'a MusicInfo),
}

/// What a request becomes: how the playing sound is made.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlaySettings {
    pub looping: bool,
    pub track: Track,
    pub kind: PlayType,
    pub channel: Channel,
    pub priority: f32,
    pub min_distance: f32,
    /// Seconds it lasts (a wave's length); `None` for music, which lasts until it ends.
    pub length: Option<f32>,
    pub volume: f32,
    pub frequency: f32,
    pub emitter: Option<u64>,
    pub start: Timing,
    pub stop: Timing,
    /// It plays as a control piece (one that changes the music playing rather than sounding).
    pub control: bool,
}

/// A sound setting's request: looping as the setting says unless forced; a type of one at a time
/// (counted) for the self-exclusive bit, else alongside anything (counted); the secondary track;
/// the wave's length; it starts and stops at once, never as a control piece.
// D2-07804
#[must_use]
pub fn from_sound_info(req: &PlayRequest, info: &SoundInfo, wave_seconds: f32) -> PlaySettings {
    let looping = match req.looping {
        Looping::Inherit => info.test_bit_mask(SOUND_LOOPS),
        Looping::ForceOn => true,
        Looping::ForceOff => false,
    };
    let kind = match req.kind {
        PlayType::Inherit if info.test_bit_mask(SOUND_SELF_EXCLUSIVE) => {
            PlayType::SelfExclusiveBudgeted
        }
        PlayType::Inherit => PlayType::NonExclusiveBudgeted,
        k => k,
    };
    let track = match req.track {
        Track::Inherit => Track::Secondary,
        t => t,
    };
    PlaySettings {
        looping,
        track,
        kind,
        channel: req.channel,
        priority: info.priority,
        min_distance: info.min_distance,
        length: Some(wave_seconds),
        volume: req.volume,
        frequency: req.frequency,
        emitter: req.emitter,
        start: Timing::Immediate,
        stop: Timing::Immediate,
        control: false,
    }
}

/// A music setting's request: looping as the setting says (never for the primary track) unless
/// forced; a primary piece alongside anything, else one at a time for the self-exclusive bit;
/// the primary or the secondary track by the primary bit; no length; the start and stop timings
/// from their bits (at the end, on the grid, the beat or a marker, at once, else on the measure;
/// the stop at once unless a bit says otherwise); a secondary piece plays as a control piece for
/// its bit.
// D2-07792
#[must_use]
pub fn from_music_info(req: &PlayRequest, info: &MusicInfo) -> PlaySettings {
    let bit = |b: u32| info.test_bit_mask(b);
    let looping = match req.looping {
        Looping::Inherit => !bit(MUSIC_PRIMARY) && bit(MUSIC_LOOPS),
        Looping::ForceOn => true,
        Looping::ForceOff => false,
    };
    let kind = match req.kind {
        PlayType::Inherit if bit(MUSIC_PRIMARY) => PlayType::NonExclusive,
        PlayType::Inherit if bit(MUSIC_SELF_EXCLUSIVE) => PlayType::SelfExclusive,
        PlayType::Inherit => PlayType::NonExclusive,
        k => k,
    };
    let track = match req.track {
        Track::Inherit if bit(MUSIC_PRIMARY) => Track::Primary,
        Track::Inherit => Track::Secondary,
        t => t,
    };
    let start = if bit(0x20) {
        Timing::OnEnd
    } else if bit(0x80) {
        Timing::OnGrid
    } else if bit(0x40) {
        Timing::OnBeat
    } else if bit(0x200) {
        Timing::OnMarker
    } else if bit(0x100) {
        Timing::Immediate
    } else {
        Timing::OnMeasure
    };
    let stop = if bit(0x800) {
        Timing::OnEnd
    } else if bit(0x2000) {
        Timing::OnGrid
    } else if bit(0x1000) {
        Timing::OnBeat
    } else if bit(0x8000) {
        Timing::OnMarker
    } else if bit(0x4000) {
        Timing::OnMeasure
    } else {
        Timing::Immediate
    };
    PlaySettings {
        looping,
        track,
        kind,
        channel: req.channel,
        priority: info.priority,
        min_distance: info.min_distance,
        length: None,
        volume: req.volume,
        frequency: req.frequency,
        emitter: req.emitter,
        start,
        stop,
        control: track == Track::Secondary && bit(0x400),
    }
}

/// The path a setting plays on: a sound setting placed or not by its bit; a music setting placed
/// for its bit, else an encoded track as a sound heard everywhere and a segment as music heard
/// everywhere.
// D2-07770
#[must_use]
pub fn path_kind(setting: Setting<'_>) -> PathKind {
    match setting {
        Setting::Sound(s) if s.test_bit_mask(SOUND_PLACED) => PathKind::PlacedSound,
        Setting::Sound(_) => PathKind::FlatSound,
        Setting::Music(m) if m.test_bit_mask(MUSIC_PLACED) => PathKind::PlacedMusic,
        Setting::Music(m) if m.kind == KIND_TRACK => PathKind::FlatSound,
        Setting::Music(_) => PathKind::FlatMusic,
    }
}
