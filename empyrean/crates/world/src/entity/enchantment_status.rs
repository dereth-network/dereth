// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/EnchantmentStatus.cs
//! Port of `Source/ACE.Server/Entity/EnchantmentStatus.cs`.

use crate::entity::spell::Spell;
use crate::network::game_messages::game_message::GameMessage;
use crate::world_objects::managers::enchantment_manager::StackType;
use crate::World;

/// Tracks the status of applying an enchantment.
// ACE: EnchantmentStatus
#[derive(Debug, Clone, Default)]
pub struct EnchantmentStatus {
    // ACE: EnchantmentStatus.Spell
    pub spell: Option<Spell>,
    // ACE: EnchantmentStatus.StackType
    pub stack_type: StackType,
    /// A `GameMessageSystemChat`.
    // ACE: EnchantmentStatus.Message
    pub message: Option<GameMessage>,
    // ACE: EnchantmentStatus.Success
    pub success: bool,
}

impl EnchantmentStatus {
    /// `new EnchantmentStatus(Spell spell)`.
    // ACE: EnchantmentStatus.EnchantmentStatus
    #[must_use]
    pub fn from_spell(spell: Spell) -> Self {
        Self {
            spell: Some(spell),
            ..Self::default()
        }
    }

    /// `new EnchantmentStatus(uint spellID)`: `Spell = new Spell(spellID)`.
    #[must_use]
    pub fn from_spell_id(w: &World, spell_id: u32) -> Self {
        Self {
            spell: Some(Spell::new(w, spell_id, true)),
            ..Self::default()
        }
    }

    /// `new EnchantmentStatus(bool success)`.
    #[must_use]
    pub fn from_success(success: bool) -> Self {
        Self {
            success,
            ..Self::default()
        }
    }
}
