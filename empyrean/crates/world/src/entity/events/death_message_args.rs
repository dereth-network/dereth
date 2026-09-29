// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Events/DeathMessageArgs.cs
//! Port of `Source/ACE.Server/Entity/Events/DeathMessageArgs.cs`.

use empyrean_entity::ObjectGuid;

// ACE: DeathMessageArgs
#[derive(Debug, Clone, PartialEq)]
pub struct DeathMessageArgs {
    // ACE: DeathMessageArgs.Message
    pub message: String,
    // ACE: DeathMessageArgs.Victim
    pub victim: ObjectGuid,
    // ACE: DeathMessageArgs.Killer
    pub killer: ObjectGuid,
}

impl DeathMessageArgs {
    // ACE: DeathMessageArgs.DeathMessageArgs
    #[must_use]
    pub fn new(message: &str, victim_id: ObjectGuid, killer_id: ObjectGuid) -> Self {
        Self {
            message: message.to_owned(),
            victim: victim_id,
            killer: killer_id,
        }
    }
}
