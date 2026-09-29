// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/TreasureMaterialBase.cs
//! `TreasureMaterialBase`: one row of the world-database table `treasure_material_base`.

// ACE: TreasureMaterialBase
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TreasureMaterialBase {
    pub id: u32,
    /// Derived from PropertyInt.TsysMutationData
    pub material_code: u32,
    /// Loot Tier
    pub tier: u32,
    pub probability: f32,
    /// MaterialType
    pub material_id: u32,
}
