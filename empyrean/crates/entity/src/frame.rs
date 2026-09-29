// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Frame.cs
//! `Frame`: an origin and an orientation.

use crate::numerics::{Quaternion, Vector3};

/// ACE: Frame
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    // ACE: Frame.Origin
    pub origin: Vector3,
    // ACE: Frame.Orientation
    pub orientation: Quaternion,
}

impl Default for Frame {
    fn default() -> Self {
        Frame::new()
    }
}

impl Frame {
    /// Origin zero, identity orientation.
    // ACE: Frame.Frame
    #[must_use]
    pub fn new() -> Self {
        Frame {
            origin: Vector3::ZERO,
            orientation: Quaternion::IDENTITY,
        }
    }
}
