// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/TreasureGemCount.cs
//! `TreasureGemCount`: one row of the world-database table `treasure_gem_count`.

// ACE: TreasureGemCount
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TreasureGemCount {
    pub id: u32,
    pub gem_code: u8,
    pub tier: i32,
    pub count: i32,
    pub chance: f32,
}
