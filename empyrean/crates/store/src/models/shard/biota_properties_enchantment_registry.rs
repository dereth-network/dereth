// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/Shard/BiotaPropertiesEnchantmentRegistry.cs
//! `BiotaPropertiesEnchantmentRegistry`: a row of the `shard` database (Entity Framework model).

// ACE: BiotaPropertiesEnchantmentRegistry
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BiotaPropertiesEnchantmentRegistry {
    // ACE: BiotaPropertiesEnchantmentRegistry.ObjectId
    pub object_id: u32,
    // ACE: BiotaPropertiesEnchantmentRegistry.EnchantmentCategory
    pub enchantment_category: u32,
    // ACE: BiotaPropertiesEnchantmentRegistry.SpellId
    pub spell_id: i32,
    // ACE: BiotaPropertiesEnchantmentRegistry.LayerId
    pub layer_id: u16,
    // ACE: BiotaPropertiesEnchantmentRegistry.HasSpellSetId
    pub has_spell_set_id: bool,
    // ACE: BiotaPropertiesEnchantmentRegistry.SpellCategory
    pub spell_category: u16,
    // ACE: BiotaPropertiesEnchantmentRegistry.PowerLevel
    pub power_level: u32,
    // ACE: BiotaPropertiesEnchantmentRegistry.StartTime
    pub start_time: f64,
    // ACE: BiotaPropertiesEnchantmentRegistry.Duration
    pub duration: f64,
    // ACE: BiotaPropertiesEnchantmentRegistry.CasterObjectId
    pub caster_object_id: u32,
    // ACE: BiotaPropertiesEnchantmentRegistry.DegradeModifier
    pub degrade_modifier: f32,
    // ACE: BiotaPropertiesEnchantmentRegistry.DegradeLimit
    pub degrade_limit: f32,
    // ACE: BiotaPropertiesEnchantmentRegistry.LastTimeDegraded
    pub last_time_degraded: f64,
    // ACE: BiotaPropertiesEnchantmentRegistry.StatModType
    pub stat_mod_type: u32,
    // ACE: BiotaPropertiesEnchantmentRegistry.StatModKey
    pub stat_mod_key: u32,
    // ACE: BiotaPropertiesEnchantmentRegistry.StatModValue
    pub stat_mod_value: f32,
    // ACE: BiotaPropertiesEnchantmentRegistry.SpellSetId
    pub spell_set_id: u32,
}
