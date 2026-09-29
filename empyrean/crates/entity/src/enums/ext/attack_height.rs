// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/AttackHeight.cs

use crate::enums::{AttackHeight, Quadrant};

impl AttackHeight {
    // ACE: AttackHeightExtensions.GetString
    pub fn get_string(self) -> Option<&'static str> {
        match self {
            AttackHeight::High => Some("High"),
            AttackHeight::Medium => Some("Med"),
            AttackHeight::Low => Some("Low"),
            _ => None,
        }
    }

    // ACE: AttackHeightExtensions.ToQuadrant
    pub fn to_quadrant(self) -> Quadrant {
        match self {
            AttackHeight::High => Quadrant::High,
            AttackHeight::Medium => Quadrant::Medium,
            AttackHeight::Low => Quadrant::Low,
            _ => Quadrant::None,
        }
    }
}
