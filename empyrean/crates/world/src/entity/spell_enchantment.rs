// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/SpellEnchantment.cs
//! Port of `Source/ACE.Server/Entity/SpellEnchantment.cs`.

use empyrean_entity::models::PropertiesEnchantmentRegistry;

use crate::entity::spell::Spell;
use crate::World;

/// Wrapper class for linking Enchantments and Spells. `enchantment` is a copy of the registry
/// entry ACE references.
// ACE: SpellEnchantment
#[derive(Debug, Clone)]
pub struct SpellEnchantment {
    // ACE: SpellEnchantment.Enchantment
    pub enchantment: PropertiesEnchantmentRegistry,
    // ACE: SpellEnchantment.Spell
    pub spell: Spell,
}

impl SpellEnchantment {
    // ACE: SpellEnchantment.SpellEnchantment
    #[must_use]
    pub fn new(w: &World, enchantment: PropertiesEnchantmentRegistry) -> Self {
        let spell = Spell::from_int(w, enchantment.spell_id, true);
        Self { enchantment, spell }
    }
}
