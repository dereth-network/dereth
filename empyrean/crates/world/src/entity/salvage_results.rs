// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/SalvageResults.cs
//! Port of `Source/ACE.Server/Entity/SalvageResults.cs`.
//!
//! `SalvageMessage` is a class in ACE: `GetMessage` hands back a reference its callers mutate. Here
//! the messages live in [`SalvageResults::messages`] and [`SalvageResults::get_message`] returns the
//! key they are held under, which the caller mutates through [`SalvageResults::message_mut`].

use empyrean_common::dotnet::DotNetDict;
use empyrean_entity::enums::{MaterialType, Skill};

/// ACE's `SalvageMessage` (its fields; 2.2b's writer, `network::structure::salvage_result`, takes
/// this type).
// You obtain <amount> <material> (ws <workmanship>) using your knowledge of <skill>.
// Your augmentation has given you a return bonus of <augMod>!
pub use crate::network::structure::salvage_result::SalvageMessage;

// ACE: SalvageMessage.SalvageMessage
/// `new SalvageMessage(materialType, skill)`: every other field is its default.
#[must_use]
pub fn salvage_message_new(material_type: MaterialType, skill: Skill) -> SalvageMessage {
    SalvageMessage {
        material_type,
        skill,
        ..SalvageMessage::default()
    }
}

// ACE: SalvageResults
#[derive(Debug, Clone, Default)]
pub struct SalvageResults {
    /// `Dictionary<uint, SalvageMessage>`, keyed `(uint)materialType << 16 | (uint)skill`; its
    /// enumeration order is observable (the order of the result messages).
    pub messages: DotNetDict<u32, SalvageMessage>,
}

impl SalvageResults {
    // ACE: SalvageResults.SalvageResults
    #[must_use]
    pub fn new() -> Self {
        Self {
            messages: DotNetDict::new(),
        }
    }

    // ACE: SalvageResults.GetMessage
    /// The message for `(material_type, skill)`, added if absent; returns its key.
    pub fn get_message(&mut self, material_type: MaterialType, skill: Skill) -> u32 {
        // `(uint)materialType << 16 | (uint)skill`: the shift wraps.
        let key = (material_type.0 << 16) | skill.0.cast_unsigned();

        if !self.messages.contains_key(&key) {
            let message = salvage_message_new(material_type, skill);
            self.messages.add(key, message);
        }
        key
    }

    /// The message held under `key` (from [`Self::get_message`]).
    ///
    /// # Panics
    /// When no message is held under `key`.
    pub fn message_mut(&mut self, key: u32) -> &mut SalvageMessage {
        self.messages
            .get_mut(&key)
            .expect("a key returned by GetMessage")
    }

    // ACE: SalvageResults.GetMessages
    /// The messages grouped by skill, each group in message order, the groups in the order their
    /// skills first appear.
    #[must_use]
    pub fn get_messages(&self) -> DotNetDict<Skill, Vec<SalvageMessage>> {
        let mut skill_groups: DotNetDict<Skill, Vec<SalvageMessage>> = DotNetDict::new();
        for (_, message) in self.messages.iter() {
            skill_groups
                .get_or_insert_with(message.skill, Vec::new)
                .push(*message);
        }
        skill_groups
    }
}
