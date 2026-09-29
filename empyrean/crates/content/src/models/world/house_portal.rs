// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/HousePortal.cs
//! `HousePortal`: one row of the world-database table `house_portal`.

use empyrean_common::dotnet::DotNetDateTime;

/// House Portal Destinations
// ACE: HousePortal
#[derive(Debug, Clone, PartialEq, Default)]
pub struct HousePortal {
    /// Unique Id of this House Portal
    pub id: u32,
    /// Unique Id of House
    pub house_id: u32,
    pub obj_cell_id: u32,
    pub origin_x: f32,
    pub origin_y: f32,
    pub origin_z: f32,
    pub angles_w: f32,
    pub angles_x: f32,
    pub angles_y: f32,
    pub angles_z: f32,
    pub last_modified: DotNetDateTime,
}
