// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/Version.cs
//! `Version`: one row of the world-database table `version`.

use empyrean_common::dotnet::DotNetDateTime;

/// Version Information
// ACE: Version
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Version {
    pub id: u32,
    pub base_version: Option<String>,
    pub patch_version: Option<String>,
    pub last_modified: DotNetDateTime,
}
