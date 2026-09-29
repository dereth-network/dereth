// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/Encounter.cs
//! `Encounter`: one row of the world-database table `encounter`.

use empyrean_common::dotnet::DotNetDateTime;

/// Encounters
// ACE: Encounter
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Encounter {
    /// Unique Id of this Encounter
    pub id: u32,
    /// Landblock for this Encounter
    pub landblock: i32,
    /// Weenie Class Id of generator/object to spawn for Encounter
    pub weenie_class_id: u32,
    /// CellX position of this Encounter
    pub cell_x: i32,
    /// CellY position of this Encounter
    pub cell_y: i32,
    pub last_modified: DotNetDateTime,
}
