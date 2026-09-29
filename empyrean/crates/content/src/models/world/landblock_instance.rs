// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/LandblockInstance.cs
//! `LandblockInstance`: one row of the world-database table `landblock_instance`.

use empyrean_common::dotnet::DotNetDateTime;

use super::LandblockInstanceLink;

/// Weenie Instances for each Landblock
// ACE: LandblockInstance
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LandblockInstance {
    /// Unique Id of this Instance
    pub guid: u32,
    pub landblock: Option<i32>,
    /// Weenie Class Id of object to spawn
    pub weenie_class_id: u32,
    pub obj_cell_id: u32,
    pub origin_x: f32,
    pub origin_y: f32,
    pub origin_z: f32,
    pub angles_w: f32,
    pub angles_x: f32,
    pub angles_y: f32,
    pub angles_z: f32,
    /// Is this a child link for any other instances?
    pub is_link_child: bool,
    pub last_modified: DotNetDateTime,
    pub landblock_instance_link: Vec<LandblockInstanceLink>,
}
