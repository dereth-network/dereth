//! The sound section of the client's preferences, as the sound manager puts them in: the four
//! volume sliders (sound effects, music, player music, ambient sounds), the sound features, the
//! number of sounds that may play at once, sound off and music off.
//!
//! The sliders and the number of sounds take effect as they change; sound off, music off and the
//! features when they are applied ([`crate::SoundPlayer::handle_change`],
//! [`crate::SoundPlayer::handle_apply`]).

/// The section the sound preferences are in.
pub const SECTION: u32 = 0x4000_0007;

/// The sound preferences' keys.
pub mod key {
    pub const SOUND_VOLUME: u32 = 0x4000_0040;
    pub const MUSIC_VOLUME: u32 = 0x4000_0041;
    pub const PLAYER_MUSIC_VOLUME: u32 = 0x4000_003D;
    pub const AMBIENT_VOLUME: u32 = 0x4000_0056;
    pub const FEATURES: u32 = 0x4000_0057;
    pub const PLAYING_SOUNDS: u32 = 0x4000_0058;
    pub const SOUND_OFF: u32 = 0x4000_0033;
    pub const MUSIC_OFF: u32 = 0x4000_0034;
}

/// The enumeration the sound features are named in.
pub const FEATURE_NAMES: u32 = 0x4000_004A;

/// The sound features: each one's number, label, and value in [`FEATURE_NAMES`]. Hardware is the
/// default.
pub const FEATURES: [(i32, &str, u32); 3] = [
    (0, "ID_Sound_Software", 0x4000_000F),
    (1, "ID_Sound_Hardware", 0x4000_0010),
    (2, "ID_Sound_EAX", 0x4000_0011),
];

/// What a sound preference holds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    /// A number between two bounds, with its default.
    Number {
        least: f32,
        greatest: f32,
        default: f32,
    },
    /// One of the [`FEATURES`], by number.
    Features { default: i32 },
    /// Yes or no.
    Flag { default: bool },
}

/// A sound preference: its key, label and help (the preference strings' names), and kind.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SoundPreference {
    pub key: u32,
    pub label: &'static str,
    pub help: &'static str,
    pub kind: Kind,
}

const VOLUME: Kind = Kind::Number {
    least: 0.0,
    greatest: 1.0,
    default: 1.0,
};

/// The section's preferences, in the order the sound manager puts them in.
// D2-07685
pub const PREFERENCES: [SoundPreference; 8] = [
    SoundPreference {
        key: key::SOUND_VOLUME,
        label: "ID_Sound_FXVolume",
        help: "ID_Sound_FXVolume_Help",
        kind: VOLUME,
    },
    SoundPreference {
        key: key::MUSIC_VOLUME,
        label: "ID_Sound_MusicVolume",
        help: "ID_Sound_MusicVolume_Help",
        kind: VOLUME,
    },
    SoundPreference {
        key: key::PLAYER_MUSIC_VOLUME,
        label: "ID_Sound_Player_Music",
        help: "ID_Sound_Player_Music_Help",
        kind: VOLUME,
    },
    SoundPreference {
        key: key::AMBIENT_VOLUME,
        label: "ID_Sound_AmbientVolume",
        help: "ID_Sound_AmbientVolume_Help",
        kind: VOLUME,
    },
    SoundPreference {
        key: key::FEATURES,
        label: "ID_Sound_SoundFeatures",
        help: "ID_Sound_SoundFeatures_Help",
        kind: Kind::Features { default: 1 },
    },
    SoundPreference {
        key: key::PLAYING_SOUNDS,
        label: "ID_Sound_NumPlayingSounds",
        help: "ID_Sound_NumPlayingSounds_Help",
        kind: Kind::Number {
            least: 8.0,
            greatest: 64.0,
            default: 32.0,
        },
    },
    SoundPreference {
        key: key::SOUND_OFF,
        label: "ID_Sound_DisableSound",
        help: "ID_Sound_DisableSound_Help",
        kind: Kind::Flag { default: false },
    },
    SoundPreference {
        key: key::MUSIC_OFF,
        label: "ID_Sound_DisableMusic",
        help: "ID_Sound_DisableMusic_Help",
        kind: Kind::Flag { default: false },
    },
];

/// The keys whose values the sound manager takes as it starts, after its device: the four
/// sliders and the number of sounds (taken as changes), then sound off, music off and the features
/// (taken as applied).
// D2-07680
pub const AT_START: [(u32, bool); 8] = [
    (key::SOUND_VOLUME, false),
    (key::MUSIC_VOLUME, false),
    (key::PLAYER_MUSIC_VOLUME, false),
    (key::AMBIENT_VOLUME, false),
    (key::PLAYING_SOUNDS, false),
    (key::SOUND_OFF, true),
    (key::MUSIC_OFF, true),
    (key::FEATURES, true),
];
