// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Managers/EnchantmentManagerWithCaching.cs
//! Port of `Source/ACE.Server/WorldObjects/Managers/EnchantmentManagerWithCaching.cs`.
//!
//! The manager every ACE `WorldObject` has (`WorldObject.EnchantmentManager`). Its caches live
//! in [`EnchantmentManagerWithCaching`], held by the object
//! (`WorldObjectFields::enchantment_manager`); the overrides are free functions `(w, this, ..)`
//! that call the base implementations in [`super::enchantment_manager`] and clear the caches
//! exactly where ACE does. **These are the entry points for callers** for every member this
//! class overrides; the rest of `EnchantmentManager` is called in the base module.
//!
//! The vital-keyed caches are keyed by `CreatureVital.Vital` (`PropertyAttribute2nd`): ACE keys
//! them by the `CreatureVital` object, and each creature holds one object per vital.

use empyrean_common::dotnet::DotNetDict;
use empyrean_entity::enums::properties::{PropertyAttribute, PropertyAttribute2nd, PropertyInt};
use empyrean_entity::enums::{DamageType, Skill, SpellCategory};
use empyrean_entity::models::PropertiesEnchantmentRegistry;
use empyrean_entity::ObjectGuid;

use crate::entity::add_enchantment_result::AddEnchantmentResult;
use crate::entity::spell::Spell;
use crate::world_objects::managers::enchantment_manager::{self as base, SPELL_CATEGORY_COOLDOWN};
use crate::World;

/// The caches of `EnchantmentManagerWithCaching` (its only state beyond the base class's
/// `WorldObject` / `Player`, which are the owning object here).
// ACE: EnchantmentManagerWithCaching
#[derive(Debug, Default, Clone)]
pub struct EnchantmentManagerWithCaching {
    has_enchantments: Option<bool>,
    has_vitae: Option<bool>,

    attribute_mod_additive_cache: DotNetDict<PropertyAttribute, i32>,
    attribute_mod_multiplier_cache: DotNetDict<PropertyAttribute, f32>,

    vital_mod_additive_cache: DotNetDict<PropertyAttribute2nd, f32>,
    vital_mod_multiplier_cache: DotNetDict<PropertyAttribute2nd, f32>,

    skill_mod_additive_cache: DotNetDict<Skill, i32>,
    skill_mod_multiplier_cache: DotNetDict<Skill, f32>,

    body_armor_mod_cache: Option<i32>,
    resistance_mod_cache: DotNetDict<DamageType, f32>,
    protection_resistance_mod_cache: DotNetDict<DamageType, f32>,
    vulnerability_resistance_mod_cache: DotNetDict<DamageType, f32>,
    regeneration_mod_cache: DotNetDict<PropertyAttribute2nd, f32>,

    damage_bonus_cache: Option<i32>,
    damage_mod_cache: Option<f32>,
    attack_mod_cache: Option<f32>,
    weapon_speed_mod_cache: Option<i32>,
    defense_mod_cache: Option<f32>,
    mana_conv_mod_cache: Option<f32>,
    elemental_damage_mod_cache: Option<f32>,
    variance_mod_cache: Option<f32>,
    armor_mod_cache: Option<i32>,
    armor_mod_vs_type_mod_cache: DotNetDict<DamageType, f32>,
    rating_cache: DotNetDict<PropertyInt, i32>,
    nether_dot_damage_rating_cache: Option<i32>,
    xp_bonus_cache: Option<f32>,
    resist_lockpick_cache: Option<i32>,
}

impl EnchantmentManagerWithCaching {
    /// Constructs a new EnchantmentManager for a WorldObject (`new
    /// EnchantmentManagerWithCaching(this)`): every cache empty. The base constructor's
    /// `WorldObject` and `Player` are the owning object.
    // ACE: EnchantmentManagerWithCaching.EnchantmentManagerWithCaching, EnchantmentManager.EnchantmentManager
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    // ACE: EnchantmentManagerWithCaching.ClearCache
    fn clear_cache(&mut self) {
        self.has_enchantments = None;
        self.has_vitae = None;

        self.attribute_mod_additive_cache.clear();
        self.attribute_mod_multiplier_cache.clear();

        self.vital_mod_additive_cache.clear();
        self.vital_mod_multiplier_cache.clear();

        self.skill_mod_additive_cache.clear();
        self.skill_mod_multiplier_cache.clear();

        self.body_armor_mod_cache = None;
        self.resistance_mod_cache.clear();
        self.protection_resistance_mod_cache.clear();
        self.vulnerability_resistance_mod_cache.clear();
        self.regeneration_mod_cache.clear();

        self.damage_bonus_cache = None;
        self.damage_mod_cache = None;
        self.attack_mod_cache = None;
        self.weapon_speed_mod_cache = None;
        self.defense_mod_cache = None;
        self.mana_conv_mod_cache = None;
        self.elemental_damage_mod_cache = None;
        self.variance_mod_cache = None;
        self.armor_mod_cache = None;
        self.armor_mod_vs_type_mod_cache.clear();
        self.rating_cache.clear();
        self.nether_dot_damage_rating_cache = None;
        self.xp_bonus_cache = None;
        self.resist_lockpick_cache = None;
    }
}

/// `WorldObject.EnchantmentManager`. A missing object is ACE's `NullReferenceException`.
fn state(w: &mut World, this: ObjectGuid) -> &mut EnchantmentManagerWithCaching {
    &mut w
        .objects
        .get_mut(this)
        .expect("ACE: WorldObject is null (NullReferenceException)")
        .wo
        .world_object
        .enchantment_manager
}

/// `ClearCache()` on `this`'s manager.
fn clear_cache(w: &mut World, this: ObjectGuid) {
    state(w, this).clear_cache();
}

/// `Player != null`.
fn is_player(w: &World, this: ObjectGuid) -> bool {
    w.objects
        .get(this)
        .is_some_and(crate::world_objects::world_object::WorldObject::is_player)
}

// ------------------------------------------------------------------------------ flags

// ACE: EnchantmentManagerWithCaching.HasEnchantments
pub fn has_enchantments(w: &mut World, this: ObjectGuid) -> bool {
    if let Some(v) = state(w, this).has_enchantments {
        return v;
    }
    let v = base::has_enchantments(w, this);
    state(w, this).has_enchantments = Some(v);
    v
}

// ACE: EnchantmentManagerWithCaching.HasVitae
pub fn has_vitae(w: &mut World, this: ObjectGuid) -> bool {
    if let Some(v) = state(w, this).has_vitae {
        return v;
    }
    let v = base::has_vitae(w, this);
    state(w, this).has_vitae = Some(v);
    v
}

// ------------------------------------------------------------------------------ mutations

/// Add/update an enchantment in this object's registry. See [`base::add`] for the arguments.
// ACE: EnchantmentManagerWithCaching.Add
pub fn add(
    w: &mut World,
    this: ObjectGuid,
    spell: &Spell,
    caster: Option<ObjectGuid>,
    weapon: Option<ObjectGuid>,
    equip: bool,
    is_weapon_spell: bool,
) -> AddEnchantmentResult {
    let result = base::add(w, this, spell, caster, weapon, equip, is_weapon_spell);

    clear_cache(w, this);

    result
}

/// Removes a spell from the enchantment registry, and sends the relevant network messages for
/// spell removal (`sound` defaults to true).
// ACE: EnchantmentManagerWithCaching.Remove
pub fn remove(
    w: &mut World,
    this: ObjectGuid,
    entry: Option<&PropertiesEnchantmentRegistry>,
    sound: bool,
) {
    base::remove(w, this, entry, sound);

    let Some(entry) = entry else { return };

    clear_cache(w, this);

    if is_player(w, this)
        && entry.spell_category != SpellCategory(u32::from(SPELL_CATEGORY_COOLDOWN))
    {
        let spell = Spell::from_int(w, entry.spell_id, true);
        player_handle_spell_hooks(w, this, &spell);
    }
}

/// Removes all enchantments except for vitae and enchantments from items. Called on player death.
// ACE: EnchantmentManagerWithCaching.RemoveAllEnchantments
pub fn remove_all_enchantments(w: &mut World, this: ObjectGuid) {
    base::remove_all_enchantments(w, this);

    clear_cache(w, this);
}

/// Removes all enchantments except for beneficial enchantments, vitae and enchantments from
/// items. Called on player death.
// ACE: EnchantmentManagerWithCaching.RemoveAllBadEnchantments
pub fn remove_all_bad_enchantments(w: &mut World, this: ObjectGuid) {
    base::remove_all_bad_enchantments(w, this);

    clear_cache(w, this);
}

/// Called on player death.
// ACE: EnchantmentManagerWithCaching.UpdateVitae
pub fn update_vitae(w: &mut World, this: ObjectGuid) -> f32 {
    let result = base::update_vitae(w, this);

    clear_cache(w, this);

    result
}

/// Called when player crosses the VitaeCPPool threshold.
// ACE: EnchantmentManagerWithCaching.ReduceVitae
pub fn reduce_vitae(w: &mut World, this: ObjectGuid) -> f32 {
    let result = base::reduce_vitae(w, this);

    clear_cache(w, this);

    result
}

/// Silently removes a spell from the enchantment registry, and sends the relevant network
/// message for dispel.
// ACE: EnchantmentManagerWithCaching.Dispel
pub fn dispel(w: &mut World, this: ObjectGuid, entry: Option<&PropertiesEnchantmentRegistry>) {
    base::dispel(w, this, entry);

    let Some(entry) = entry else { return };

    clear_cache(w, this);

    if is_player(w, this) {
        let spell = Spell::from_int(w, entry.spell_id, true);
        player_handle_spell_hooks(w, this, &spell);
    }
}

/// Silently removes multiple spells from the enchantment registry, and sends the relevant
/// network messages for dispel (the `List` overload of `Dispel`).
///
/// # Panics
/// For a player and `None` (ACE: `foreach` over `null` after the base's early return).
// ACE: EnchantmentManagerWithCaching.Dispel
pub fn dispel_list(
    w: &mut World,
    this: ObjectGuid,
    entries: Option<&[PropertiesEnchantmentRegistry]>,
) {
    base::dispel_list(w, this, entries);

    clear_cache(w, this);

    if is_player(w, this) {
        let entries = entries.expect("ACE: foreach over a null list (NullReferenceException)");
        for entry in entries {
            let spell = Spell::from_int(w, entry.spell_id, true);
            player_handle_spell_hooks(w, this, &spell);
        }
    }
}

// ACE: EnchantmentManagerWithCaching.StartCooldown
pub fn start_cooldown(w: &mut World, this: ObjectGuid, item: ObjectGuid) -> bool {
    let result = base::start_cooldown(w, this, item);

    if result {
        clear_cache(w, this);
    }

    result
}

// ------------------------------------------------------------------------------ cached getters

/// Returns the additive modifiers to an attribute from enchantments.
// ACE: EnchantmentManagerWithCaching.GetAttributeMod_Additive
pub fn get_attribute_mod_additive(
    w: &mut World,
    this: ObjectGuid,
    attribute: PropertyAttribute,
) -> i32 {
    if let Some(&value) = state(w, this).attribute_mod_additive_cache.get(&attribute) {
        return value;
    }

    let value = base::get_attribute_mod_additive(w, this, attribute);

    state(w, this)
        .attribute_mod_additive_cache
        .insert(attribute, value);

    value
}

/// Returns the multiplicative modifiers to an attribute from enchantments.
// ACE: EnchantmentManagerWithCaching.GetAttributeMod_Multiplier
pub fn get_attribute_mod_multiplier(
    w: &mut World,
    this: ObjectGuid,
    attribute: PropertyAttribute,
) -> f32 {
    if let Some(&value) = state(w, this)
        .attribute_mod_multiplier_cache
        .get(&attribute)
    {
        return value;
    }

    let value = base::get_attribute_mod_multiplier(w, this, attribute);

    state(w, this)
        .attribute_mod_multiplier_cache
        .insert(attribute, value);

    value
}

/// Gets the additive modifiers to a vital / secondary attribute (`vital` is `CreatureVital.Vital`).
// ACE: EnchantmentManagerWithCaching.GetVitalMod_Additives
pub fn get_vital_mod_additives(
    w: &mut World,
    this: ObjectGuid,
    vital: PropertyAttribute2nd,
) -> f32 {
    if let Some(&value) = state(w, this).vital_mod_additive_cache.get(&vital) {
        return value;
    }

    let value = base::get_vital_mod_additives(w, this, vital);

    state(w, this).vital_mod_additive_cache.insert(vital, value);

    value
}

/// Gets the multiplicative modifiers to a vital / secondary attribute.
// ACE: EnchantmentManagerWithCaching.GetVitalMod_Multiplier
pub fn get_vital_mod_multiplier(
    w: &mut World,
    this: ObjectGuid,
    vital: PropertyAttribute2nd,
) -> f32 {
    if let Some(&value) = state(w, this).vital_mod_multiplier_cache.get(&vital) {
        return value;
    }

    let value = base::get_vital_mod_multiplier(w, this, vital);

    state(w, this)
        .vital_mod_multiplier_cache
        .insert(vital, value);

    value
}

/// Returns the additive modifiers to a skill from enchantments.
// ACE: EnchantmentManagerWithCaching.GetSkillMod_Additives
pub fn get_skill_mod_additives(w: &mut World, this: ObjectGuid, skill: Skill) -> i32 {
    if let Some(&value) = state(w, this).skill_mod_additive_cache.get(&skill) {
        return value;
    }

    let value = base::get_skill_mod_additives(w, this, skill);

    state(w, this).skill_mod_additive_cache.insert(skill, value);

    value
}

/// Returns the multiplicative modifiers to a skill from enchantments.
// ACE: EnchantmentManagerWithCaching.GetSkillMod_Multiplier
pub fn get_skill_mod_multiplier(w: &mut World, this: ObjectGuid, skill: Skill) -> f32 {
    if let Some(&value) = state(w, this).skill_mod_multiplier_cache.get(&skill) {
        return value;
    }

    let value = base::get_skill_mod_multiplier(w, this, skill);

    state(w, this)
        .skill_mod_multiplier_cache
        .insert(skill, value);

    value
}

/// Returns the base armor modifier from enchantments (the parameterless `GetBodyArmorMod`; the
/// `bool` overload is not cached: [`base::get_body_armor_mod_positive`]).
// ACE: EnchantmentManagerWithCaching.GetBodyArmorMod
pub fn get_body_armor_mod(w: &mut World, this: ObjectGuid) -> i32 {
    if let Some(value) = state(w, this).body_armor_mod_cache {
        return value;
    }

    let value = base::get_body_armor_mod(w, this);
    state(w, this).body_armor_mod_cache = Some(value);

    value
}

/// Gets the resistance modifier for a damage type.
// ACE: EnchantmentManagerWithCaching.GetResistanceMod
pub fn get_resistance_mod(w: &mut World, this: ObjectGuid, damage_type: DamageType) -> f32 {
    if let Some(&value) = state(w, this).resistance_mod_cache.get(&damage_type) {
        return value;
    }

    let value = base::get_resistance_mod(w, this, damage_type);

    state(w, this)
        .resistance_mod_cache
        .insert(damage_type, value);

    value
}

/// Gets the resistance modifier for a damage type (protections only).
// ACE: EnchantmentManagerWithCaching.GetProtectionResistanceMod
pub fn get_protection_resistance_mod(
    w: &mut World,
    this: ObjectGuid,
    damage_type: DamageType,
) -> f32 {
    if let Some(&value) = state(w, this)
        .protection_resistance_mod_cache
        .get(&damage_type)
    {
        return value;
    }

    let value = base::get_protection_resistance_mod(w, this, damage_type);

    state(w, this)
        .protection_resistance_mod_cache
        .insert(damage_type, value);

    value
}

/// Gets the resistance modifier for a damage type (vulnerabilities only).
// ACE: EnchantmentManagerWithCaching.GetVulnerabilityResistanceMod
pub fn get_vulnerability_resistance_mod(
    w: &mut World,
    this: ObjectGuid,
    damage_type: DamageType,
) -> f32 {
    if let Some(&value) = state(w, this)
        .vulnerability_resistance_mod_cache
        .get(&damage_type)
    {
        return value;
    }

    let value = base::get_vulnerability_resistance_mod(w, this, damage_type);

    state(w, this)
        .vulnerability_resistance_mod_cache
        .insert(damage_type, value);

    value
}

/// Gets the regeneration modifier for a vital type (`vital` is `CreatureVital.Vital`).
// ACE: EnchantmentManagerWithCaching.GetRegenerationMod
pub fn get_regeneration_mod(w: &mut World, this: ObjectGuid, vital: PropertyAttribute2nd) -> f32 {
    if let Some(&value) = state(w, this).regeneration_mod_cache.get(&vital) {
        return value;
    }

    let value = base::get_regeneration_mod(w, this, vital);

    state(w, this).regeneration_mod_cache.insert(vital, value);

    value
}

/// Returns the weapon damage modifier, ie. Blood Drinker.
// ACE: EnchantmentManagerWithCaching.GetDamageBonus
pub fn get_damage_bonus(w: &mut World, this: ObjectGuid) -> i32 {
    if let Some(value) = state(w, this).damage_bonus_cache {
        return value;
    }

    let value = base::get_damage_bonus(w, this);
    state(w, this).damage_bonus_cache = Some(value);

    value
}

/// Returns the DamageMod for bow / crossbow.
// ACE: EnchantmentManagerWithCaching.GetDamageMod
pub fn get_damage_mod(w: &mut World, this: ObjectGuid) -> f32 {
    if let Some(value) = state(w, this).damage_mod_cache {
        return value;
    }

    let value = base::get_damage_mod(w, this);
    state(w, this).damage_mod_cache = Some(value);

    value
}

/// Returns the attack skill modifier, ie. Heart Seeker.
// ACE: EnchantmentManagerWithCaching.GetAttackMod
pub fn get_attack_mod(w: &mut World, this: ObjectGuid) -> f32 {
    if let Some(value) = state(w, this).attack_mod_cache {
        return value;
    }

    let value = base::get_attack_mod(w, this);
    state(w, this).attack_mod_cache = Some(value);

    value
}

/// Returns the weapon speed modifier, ie. Swift Killer.
// ACE: EnchantmentManagerWithCaching.GetWeaponSpeedMod
pub fn get_weapon_speed_mod(w: &mut World, this: ObjectGuid) -> i32 {
    if let Some(value) = state(w, this).weapon_speed_mod_cache {
        return value;
    }

    let value = base::get_weapon_speed_mod(w, this);
    state(w, this).weapon_speed_mod_cache = Some(value);

    value
}

/// Returns the defense skill modifier, ie. Defender.
// ACE: EnchantmentManagerWithCaching.GetDefenseMod
pub fn get_defense_mod(w: &mut World, this: ObjectGuid) -> f32 {
    if let Some(value) = state(w, this).defense_mod_cache {
        return value;
    }

    let value = base::get_defense_mod(w, this);
    state(w, this).defense_mod_cache = Some(value);

    value
}

/// Returns the mana conversion bonus modifier, ie. Hermetic Link / Void.
// ACE: EnchantmentManagerWithCaching.GetManaConvMod
pub fn get_mana_conv_mod(w: &mut World, this: ObjectGuid) -> f32 {
    if let Some(value) = state(w, this).mana_conv_mod_cache {
        return value;
    }

    let value = base::get_mana_conv_mod(w, this);
    state(w, this).mana_conv_mod_cache = Some(value);

    value
}

/// Returns the elemental damage bonus modifier, ie. Spirit Drinker / Loather.
// ACE: EnchantmentManagerWithCaching.GetElementalDamageMod
pub fn get_elemental_damage_mod(w: &mut World, this: ObjectGuid) -> f32 {
    if let Some(value) = state(w, this).elemental_damage_mod_cache {
        return value;
    }

    let value = base::get_elemental_damage_mod(w, this);
    state(w, this).elemental_damage_mod_cache = Some(value);

    value
}

/// Returns the weapon damage variance modifier.
// ACE: EnchantmentManagerWithCaching.GetVarianceMod
pub fn get_variance_mod(w: &mut World, this: ObjectGuid) -> f32 {
    if let Some(value) = state(w, this).variance_mod_cache {
        return value;
    }

    let value = base::get_variance_mod(w, this);
    state(w, this).variance_mod_cache = Some(value);

    value
}

/// Returns the additive armor level modifier, ie. Impenetrability.
// ACE: EnchantmentManagerWithCaching.GetArmorMod
pub fn get_armor_mod(w: &mut World, this: ObjectGuid) -> i32 {
    if let Some(value) = state(w, this).armor_mod_cache {
        return value;
    }

    let value = base::get_armor_mod(w, this);
    state(w, this).armor_mod_cache = Some(value);

    value
}

/// Gets the additive armor level vs type modifier, ie. banes.
// ACE: EnchantmentManagerWithCaching.GetArmorModVsType
pub fn get_armor_mod_vs_type(w: &mut World, this: ObjectGuid, damage_type: DamageType) -> f32 {
    if let Some(&value) = state(w, this).armor_mod_vs_type_mod_cache.get(&damage_type) {
        return value;
    }

    let value = base::get_armor_mod_vs_type(w, this, damage_type);

    state(w, this)
        .armor_mod_vs_type_mod_cache
        .insert(damage_type, value);

    value
}

// ACE: EnchantmentManagerWithCaching.GetResistLockpick
pub fn get_resist_lockpick(w: &mut World, this: ObjectGuid) -> i32 {
    if let Some(value) = state(w, this).resist_lockpick_cache {
        return value;
    }

    let value = base::get_resist_lockpick(w, this);
    state(w, this).resist_lockpick_cache = Some(value);

    value
}

// ACE: EnchantmentManagerWithCaching.GetRating
pub fn get_rating(w: &mut World, this: ObjectGuid, property: PropertyInt) -> i32 {
    if let Some(&value) = state(w, this).rating_cache.get(&property) {
        return value;
    }

    let value = base::get_rating(w, this, property);

    state(w, this).rating_cache.insert(property, value);

    value
}

// ACE: EnchantmentManagerWithCaching.GetNetherDotDamageRating
pub fn get_nether_dot_damage_rating(w: &mut World, this: ObjectGuid) -> i32 {
    if let Some(value) = state(w, this).nether_dot_damage_rating_cache {
        return value;
    }
    let value = base::get_nether_dot_damage_rating(w, this);
    state(w, this).nether_dot_damage_rating_cache = Some(value);

    value
}

// ACE: EnchantmentManagerWithCaching.GetXPBonus
pub fn get_xp_bonus(w: &mut World, this: ObjectGuid) -> f32 {
    if let Some(value) = state(w, this).xp_bonus_cache {
        return value;
    }
    let value = base::get_xp_bonus(w, this);
    state(w, this).xp_bonus_cache = Some(value);

    value
}

// ---------------------------------------------------------------------------------------------
// Not ACE: pointer to a member of another ACE file.
// ---------------------------------------------------------------------------------------------

/// `Player.HandleSpellHooks(spell)` (Player_Spells.cs: max-vital and run-rate updates).
fn player_handle_spell_hooks(w: &mut World, player: ObjectGuid, spell: &Spell) {
    crate::world_objects::player_spells::handle_spell_hooks(w, player, spell);
}
