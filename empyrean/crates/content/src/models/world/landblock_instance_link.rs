// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/LandblockInstanceLink.cs
//! `LandblockInstanceLink`: one row of the world-database table `landblock_instance_link`.

use empyrean_common::dotnet::DotNetDateTime;

/// Weenie Instance Links
// ACE: LandblockInstanceLink
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LandblockInstanceLink {
    /// Unique Id of this Instance Link
    pub id: u32,
    /// GUID of parent instance
    pub parent_guid: u32,
    /// GUID of child instance
    pub child_guid: u32,
    pub last_modified: DotNetDateTime,
    // ACE: LandblockInstanceLink.Parent navigates back to the parent row, not carried.
}
