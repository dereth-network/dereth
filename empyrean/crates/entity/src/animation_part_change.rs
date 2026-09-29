// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/AnimationPartChange.cs
//! `AnimationPartChange` (fields only).

/// ACE: AnimationPartChange
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AnimationPartChange {
    // ACE: AnimationPartChange.PartIndex
    pub part_index: u8,
    // ACE: AnimationPartChange.PartID
    pub part_id: u32,
}
