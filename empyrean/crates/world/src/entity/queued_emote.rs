// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/QueuedEmote.cs
//! Port of `Source/ACE.Server/Entity/QueuedEmote.cs`.

use empyrean_common::dotnet::DotNetDateTime;
use empyrean_entity::{Emote, ObjectGuid};

// ACE: QueuedEmote
#[derive(Debug, Clone, Default)]
pub struct QueuedEmote {
    // ACE: QueuedEmote.Data
    pub data: Option<Emote>,
    // ACE: QueuedEmote.Target
    pub target: Option<ObjectGuid>,
    // ACE: QueuedEmote.ExecuteTime
    pub execute_time: DotNetDateTime,
}

impl QueuedEmote {
    // ACE: QueuedEmote.QueuedEmote
    #[must_use]
    pub fn new(data: Emote, target: Option<ObjectGuid>, execute_time: DotNetDateTime) -> Self {
        Self {
            data: Some(data),
            target,
            execute_time,
        }
    }
}
