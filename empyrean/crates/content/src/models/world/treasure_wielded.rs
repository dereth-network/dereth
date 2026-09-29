// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/TreasureWielded.cs
//! `TreasureWielded`: one row of the world-database table `treasure_wielded`.

use empyrean_common::dotnet::DotNetDateTime;

/// Wielded Treasure
// ACE: TreasureWielded
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TreasureWielded {
    /// Unique Id of this Treasure
    pub id: u32,
    /// Type of Treasure for this instance
    pub treasure_type: u32,
    /// Weenie Class Id of Treasure to Generate
    pub weenie_class_id: u32,
    /// Palette Color of Object Generated
    pub palette_id: u32,
    /// Always 0 in cache.bin
    pub unknown_1: u32,
    /// Shade of Object generated&apos;s Palette
    pub shade: f32,
    /// Stack Size of object to create (-1 = infinite)
    pub stack_size: i32,
    pub stack_size_variance: f32,
    pub probability: f32,
    /// Always 0 in cache.bin
    pub unknown_3: u32,
    /// Always 0 in cache.bin
    pub unknown_4: u32,
    /// Always 0 in cache.bin
    pub unknown_5: u32,
    pub set_start: bool,
    pub has_sub_set: bool,
    pub continues_previous_set: bool,
    /// Always 0 in cache.bin
    pub unknown_9: u32,
    /// Always 0 in cache.bin
    pub unknown_10: u32,
    /// Always 0 in cache.bin
    pub unknown_11: u32,
    /// Always 0 in cache.bin
    pub unknown_12: u32,
    pub last_modified: DotNetDateTime,
}
