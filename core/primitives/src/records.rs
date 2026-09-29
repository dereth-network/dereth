//! Small fixed-layout records that the dat decoders produce and the animation runtime consumes
//! unchanged.
//!
//! Each of these is one entry inside a larger decoded object (a motion table, a setup, a physics
//! script table, a degrade table) whose decoded and runtime forms are field-for-field the same.
//! They are defined once, here, so the decoder crate and the animation crate share them without
//! either depending on the other. The containing objects are not here: their runtime forms differ
//! from the decoded ones (resolved hooks, keyed maps, derived values) and stay in their crates.

use crate::ids::DataId;
use crate::space::Frame;

/// One animation reference inside a motion entry: the animation and the frame range and rate it
/// plays at. 16 bytes on disk.
///
/// `high_frame == -1` means "to the end of the animation" and a negative `framerate` plays
/// backwards; both are resolved by the animation sequence node, not here.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnimData {
    pub anim_id: DataId,
    pub low_frame: i32,
    pub high_frame: i32,
    pub framerate: f32,
}

impl Default for AnimData {
    /// id 0, `low_frame = 0`, `high_frame = -1`,
    /// **`framerate = 30.0`**.
    fn default() -> Self {
        Self {
            anim_id: DataId(0),
            low_frame: 0,
            high_frame: -1,
            framerate: 30.0,
        }
    }
}

impl AnimData {
    /// Scale the framerate by the motion speed. This is the only place speed reaches the
    /// player, so its float rounding is contract: the expansion gate compares framerates
    /// as `f32::to_bits`.
    #[must_use]
    pub fn scaled(self, speed: f32) -> Self {
        Self {
            framerate: self.framerate * speed,
            ..self
        }
    }
}

/// A light attached to a setup.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LightInfo {
    pub frame: Frame,
    pub color_argb: u32,
    pub intensity: f32,
    pub falloff: f32,
    pub cone_angle: f32,
}

/// One `{ part_id, Frame }` entry of a setup's holding locations or connection points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LocationEntry {
    pub part_id: u32,
    pub frame: Frame,
}

/// One row of a physics script table: a script and the modifier threshold that selects it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScriptAndMod {
    pub modifier: f32,
    pub script_id: DataId,
}

/// One level of a graphics object's degrade table: exactly 20 bytes, no alignment and no
/// trailing data.
///
/// The last level's `max_dist` is `FLT_MAX`, which is a terminator sentinel and is preserved
/// rather than dropped.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GfxObjInfo {
    pub gfxobj_id: DataId,
    pub degrade_mode: i32,
    pub min_dist: f32,
    pub ideal_dist: f32,
    pub max_dist: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Defaults to a framerate of 30, not 0.
    #[test]
    fn anim_data_defaults_to_thirty_fps_and_high_frame_minus_one() {
        let a = AnimData::default();
        assert_eq!(a.framerate, 30.0);
        assert_eq!(a.high_frame, -1);
        assert_eq!(a.low_frame, 0);
        assert_eq!(a.scaled(0.5).framerate, 15.0);
        assert_eq!(a.scaled(-1.0).framerate, -30.0);
    }
}
