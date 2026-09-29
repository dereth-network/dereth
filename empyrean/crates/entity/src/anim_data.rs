// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/AnimData.cs
//! `AnimData`: an animation id with its frame range and rate.

/// ACE: AnimData
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AnimData {
    // ACE: AnimData.AnimId
    pub anim_id: i32,
    // ACE: AnimData.LowFrame
    pub low_frame: i32,
    // ACE: AnimData.HighFrame
    pub high_frame: i32,
    /// Negative framerates play the animation in reverse.
    // ACE: AnimData.Framerate
    pub framerate: f32,
}

impl AnimData {
    // ACE: AnimData.AnimData
    #[must_use]
    pub fn new(animation_id: i32, low_frame: i32, high_frame: i32, framerate: f32) -> Self {
        AnimData {
            anim_id: animation_id,
            low_frame,
            high_frame,
            framerate,
        }
    }
}
