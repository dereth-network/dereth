// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/TreasureDeath.cs
//! `TreasureDeath`: one row of the world-database table `treasure_death`.

use empyrean_common::dotnet::DotNetDateTime;

/// Death Treasure
// ACE: TreasureDeath
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TreasureDeath {
    /// Unique Id of this Treasure
    pub id: u32,
    /// Type of Treasure for this instance
    pub treasure_type: u32,
    pub tier: i32,
    pub loot_quality_mod: f32,
    pub unknown_chances: i32,
    pub item_chance: i32,
    pub item_min_amount: i32,
    pub item_max_amount: i32,
    pub item_treasure_type_selection_chances: i32,
    pub magic_item_chance: i32,
    pub magic_item_min_amount: i32,
    pub magic_item_max_amount: i32,
    pub magic_item_treasure_type_selection_chances: i32,
    pub mundane_item_chance: i32,
    pub mundane_item_min_amount: i32,
    pub mundane_item_max_amount: i32,
    pub mundane_item_type_selection_chances: i32,
    pub last_modified: DotNetDateTime,
}
