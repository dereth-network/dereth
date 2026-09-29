// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/PointsOfInterest.cs
//! `PointsOfInterest`: one row of the world-database table `points_of_interest`.

use empyrean_common::dotnet::DotNetDateTime;

/// Points of Interest for @telepoi command
// ACE: PointsOfInterest
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PointsOfInterest {
    /// Unique Id of this POI
    pub id: u32,
    /// Name for POI
    pub name: String,
    /// Weenie Class Id of portal weenie to reference for destination of POI
    pub weenie_class_id: u32,
    pub last_modified: DotNetDateTime,
}
