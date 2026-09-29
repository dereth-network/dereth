// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/TreasureMaterialGroups.cs
//! `TreasureMaterialGroups`: one row of the world-database table `treasure_material_groups`.

// ACE: TreasureMaterialGroups
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TreasureMaterialGroups {
    pub id: u32,
    /// MaterialType Group
    pub material_group: u32,
    /// Loot Tier
    pub tier: u32,
    pub probability: f32,
    /// MaterialType
    pub material_id: u32,
}
