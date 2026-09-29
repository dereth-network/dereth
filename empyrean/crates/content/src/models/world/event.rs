// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/Event.cs
//! `Event`: one row of the world-database table `event`.

use empyrean_common::dotnet::DotNetDateTime;

/// Events
// ACE: Event
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Event {
    /// Unique Id of this Event
    pub id: u32,
    /// Unique Event of Quest
    pub name: String,
    /// Unixtime of Event Start
    pub start_time: i32,
    /// Unixtime of Event End
    pub end_time: i32,
    /// State of Event (GameEventState)
    pub state: i32,
    pub last_modified: DotNetDateTime,
}
