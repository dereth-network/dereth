//! A request to play: the sound or music setting, who makes the sound, and how the caller wants it
//! played where it does not leave that to the setting.

/// Whether a sound loops: as its setting says, or forced off or on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Looping {
    #[default]
    Inherit = 0,
    ForceOff = 1,
    ForceOn = 2,
}

/// How a sound shares the speakers: as its setting says; alongside anything (and counted against
/// the budget of sounds, or not); one at a time of the same setting (counted or not); or alone,
/// silencing every other sound and the music while it plays.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlayType {
    #[default]
    Inherit = 0,
    NonExclusive = 1,
    NonExclusiveBudgeted = 2,
    SelfExclusive = 3,
    SelfExclusiveBudgeted = 4,
    AllExclusive = 5,
}

/// Which music track a piece plays on: as its setting says, the primary track (one piece at a
/// time) or a secondary one (over it).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Track {
    #[default]
    Inherit = 0,
    Primary = 1,
    Secondary = 2,
}

/// The volume slider a sound follows: the sound effects', the music's, the player music's or the
/// ambient sounds'.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Channel {
    #[default]
    Main = 0,
    Music = 1,
    Player = 2,
    Ambient = 3,
}

/// When a piece starts or stops, relative to the music playing: at its end, on the next grid,
/// beat, marker or measure, or at once.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Timing {
    OnEnd = 0,
    OnGrid = 1,
    OnBeat = 2,
    OnMarker = 3,
    OnMeasure = 4,
    #[default]
    Immediate = 5,
}

/// The four kinds of path a sound plays on: music heard everywhere alike, music placed in the
/// world, a sound heard everywhere alike, a sound placed in the world.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathKind {
    FlatMusic = 0,
    PlacedMusic = 1,
    FlatSound = 2,
    PlacedSound = 3,
}

impl PathKind {
    /// Whether it is placed in the world (heard from where its maker stands).
    #[must_use]
    pub fn placed(self) -> bool {
        matches!(self, Self::PlacedMusic | Self::PlacedSound)
    }
}

/// One request to play: the setting (a sound setting, `.sif`, or a music setting, `.mif`), the
/// object making the sound (none: heard where the listener is), its volume (0 to 1) and frequency
/// (1 plays it as recorded), and the looping, type, track and channel asked for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayRequest {
    pub id: u32,
    pub emitter: Option<u64>,
    pub volume: f32,
    pub frequency: f32,
    pub looping: Looping,
    pub kind: PlayType,
    pub track: Track,
    pub channel: Channel,
}

impl PlayRequest {
    /// A request for `id` with every choice left to its setting: full volume, as recorded, on the
    /// sound effects' channel, heard where the listener is.
    // D2-01714
    #[must_use]
    pub fn new(id: u32) -> Self {
        Self {
            id,
            emitter: None,
            volume: 1.0,
            frequency: 1.0,
            looping: Looping::Inherit,
            kind: PlayType::Inherit,
            track: Track::Inherit,
            channel: Channel::Main,
        }
    }
}
