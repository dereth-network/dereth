//! Plain payloads shared by decoded and executable animation hooks.

use crate::{DataId, Frame};

/// Shared fields for the Attack payload.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AttackCone {
    pub part_index: u32,
    pub left: (f32, f32),
    pub right: (f32, f32),
    pub radius: f32,
    pub height: f32,
}

/// Shared fields for the PartRamp payload.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HookPartRamp {
    pub part: u32,
    pub start: f32,
    pub end: f32,
    pub time: f32,
}

/// Shared fields for the Ramp payload.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HookRamp {
    pub start: f32,
    pub end: f32,
    pub time: f32,
}

/// Shared fields for the Scale payload.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HookScale {
    pub end: f32,
    pub time: f32,
}

/// Shared fields for the CreateParticle payload.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HookCreateParticle {
    pub emitter_info_id: DataId,
    pub part_index: u32,
    pub offset: Frame,
    pub emitter_id: u32,
}

/// Shared fields for the CallPes payload.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HookCallPes {
    pub pes: DataId,
    pub pause: f32,
}

/// Shared fields for the SoundTweaked payload.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HookSoundTweaked {
    pub sound_id: DataId,
    pub probability: f32,
    pub priority: f32,
    pub volume: f32,
}

/// Shared fields for the TextureVelocity payload.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HookTextureVelocity {
    pub u_speed: f32,
    pub v_speed: f32,
}

/// Shared fields for the TextureVelocityPart payload.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HookTextureVelocityPart {
    pub part_index: u32,
    pub u_speed: f32,
    pub v_speed: f32,
}
