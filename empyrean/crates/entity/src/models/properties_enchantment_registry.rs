// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Models/PropertiesEnchantmentRegistry.cs
//! `PropertiesEnchantmentRegistry`: one active enchantment on a biota.

use crate::enums::{EnchantmentTypeFlags, EquipmentSet, SpellCategory};

/// ACE: PropertiesEnchantmentRegistry
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PropertiesEnchantmentRegistry {
    // ACE: PropertiesEnchantmentRegistry.EnchantmentCategory
    pub enchantment_category: u32,
    // ACE: PropertiesEnchantmentRegistry.SpellId
    pub spell_id: i32,
    // ACE: PropertiesEnchantmentRegistry.LayerId
    pub layer_id: u16,
    // ACE: PropertiesEnchantmentRegistry.HasSpellSetId
    pub has_spell_set_id: bool,
    // ACE: PropertiesEnchantmentRegistry.SpellCategory
    pub spell_category: SpellCategory,
    // ACE: PropertiesEnchantmentRegistry.PowerLevel
    pub power_level: u32,
    // ACE: PropertiesEnchantmentRegistry.StartTime
    pub start_time: f64,
    // ACE: PropertiesEnchantmentRegistry.Duration
    pub duration: f64,
    // ACE: PropertiesEnchantmentRegistry.CasterObjectId
    pub caster_object_id: u32,
    // ACE: PropertiesEnchantmentRegistry.DegradeModifier
    pub degrade_modifier: f32,
    // ACE: PropertiesEnchantmentRegistry.DegradeLimit
    pub degrade_limit: f32,
    // ACE: PropertiesEnchantmentRegistry.LastTimeDegraded
    pub last_time_degraded: f64,
    // ACE: PropertiesEnchantmentRegistry.StatModType
    pub stat_mod_type: EnchantmentTypeFlags,
    // ACE: PropertiesEnchantmentRegistry.StatModKey
    pub stat_mod_key: u32,
    // ACE: PropertiesEnchantmentRegistry.StatModValue
    pub stat_mod_value: f32,
    // ACE: PropertiesEnchantmentRegistry.SpellSetId
    pub spell_set_id: EquipmentSet,
}
