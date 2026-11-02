//! The volume law: a channel's slider and a sound's own volume to the level its path plays at.
//!
//! The level is in millibels (hundredths of a decibel), linear in the product of the two: full
//! volume on a full slider plays at 0, and the product's fall to nothing is a fall of 25 decibels,
//! not to silence; only a volume or a slider of 0 is silent, at -96 decibels. The music slider
//! counts for 0.4 of itself, and a path placed in the world for four times its slider (so a placed
//! sound is at full level until the product falls under a quarter).

use dereth_primitives::num::to_i32_f64;

use crate::request::Channel;

/// The level of a silent sound, in millibels.
pub const SILENT_MB: i32 = -9600;

/// The level the product of slider and volume falls from, in millibels: the product's 1 plays at
/// 0, its fall to 0 is this far down.
pub const RANGE_MB: f32 = -2500.0;

/// The music slider's share of itself.
pub const MUSIC_SHARE: f32 = 0.4;

/// A placed path's multiple of its slider.
pub const PLACED_BOOST: f32 = 4.0;

/// The four volume sliders, each 0 to 1 (1 by default).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChannelVolumes {
    pub main: f32,
    pub music: f32,
    pub player: f32,
    pub ambient: f32,
}

impl Default for ChannelVolumes {
    fn default() -> Self {
        Self {
            main: 1.0,
            music: 1.0,
            player: 1.0,
            ambient: 1.0,
        }
    }
}

impl ChannelVolumes {
    /// A channel's slider.
    #[must_use]
    pub fn of(&self, channel: Channel) -> f32 {
        match channel {
            Channel::Main => self.main,
            Channel::Music => self.music,
            Channel::Player => self.player,
            Channel::Ambient => self.ambient,
        }
    }

    /// Set a channel's slider; a value outside 0 to 1 is refused, as the device refuses it.
    // D2-07794
    pub fn set(&mut self, channel: Channel, value: f32) -> bool {
        if !(0.0..=1.0).contains(&value) {
            return false;
        }
        match channel {
            Channel::Main => self.main = value,
            Channel::Music => self.music = value,
            Channel::Player => self.player = value,
            Channel::Ambient => self.ambient = value,
        }
        true
    }
}

/// The level a sound's channel gives it: the slider (the music's at its share), four times that
/// on a placed path.
#[must_use]
pub fn channel_level(volumes: &ChannelVolumes, channel: Channel, placed: bool) -> f32 {
    let mut level = match channel {
        Channel::Music => volumes.music * MUSIC_SHARE,
        c => volumes.of(c),
    };
    if placed {
        level *= PLACED_BOOST;
    }
    level
}

/// The millibels a sound plays at for its channel's `level` and its own `volume` (held to 0 to 1):
/// silent for either at 0 or below, else `-2500 - trunc(-2500 * level * volume)`, never above 0.
// D2-07773
#[must_use]
pub fn millibels(level: f32, volume: f32) -> i32 {
    let volume = volume.clamp(0.0, 1.0);
    if volume <= 0.0 || level <= 0.0 {
        return SILENT_MB;
    }
    let product = f64::from(level) * f64::from(volume) * f64::from(RANGE_MB);
    (-2500 - to_i32_f64(product)).min(0)
}
