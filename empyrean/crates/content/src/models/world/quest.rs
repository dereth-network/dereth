// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/Quest.cs
//! `Quest`: one row of the world-database table `quest`.

use empyrean_common::dotnet::DotNetDateTime;

/// Quests
// ACE: Quest
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Quest {
    /// Unique Id of this Quest
    pub id: u32,
    /// Unique Name of Quest
    pub name: String,
    /// Minimum time between Quest completions
    pub min_delta: u32,
    /// Maximum number of times Quest can be completed
    pub max_solves: i32,
    /// Quest solved text - unused?
    pub message: Option<String>,
    pub last_modified: DotNetDateTime,
}
