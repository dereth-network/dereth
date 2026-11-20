//! The overlay any UI draws over the world: the shared overlay's textures, batches and preview
//! spaces ([`dereth_ui_overlay`], which names no game), and the pictures this game lays on the
//! ground under its world objects.

pub use dereth_ui_overlay::*;

/// A picture laid on the ground under one of this game's world objects (the shared overlay's
/// [`dereth_ui_overlay::GroundMarker`], by the object's id).
pub type GroundMarker = dereth_ui_overlay::GroundMarker<dereth_primitives::ObjectId>;

/// A picture laid on the ground at one of this game's places (the shared overlay's
/// [`dereth_ui_overlay::GroundMark`], by its position).
pub type GroundMark = dereth_ui_overlay::GroundMark<dereth_primitives::Position>;

/// A band drawn in the world through this game's places (the shared overlay's
/// [`dereth_ui_overlay::WorldBand`], by position).
pub type WorldBand = dereth_ui_overlay::WorldBand<dereth_primitives::Position>;
