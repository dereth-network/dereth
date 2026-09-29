// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/WorldObject_Weapon.cs
//! Port of `Source/ACE.Server/WorldObjects/WorldObject_Weapon.cs`.
//!
//! The property wrappers (`WeaponSkill`, `W_DamageType`, `CriticalFrequency`, ...) are generated in
//! `props/world_object_weapon.rs`. The statics and instance helpers are free functions here.
//! ACE's `CreatureSkill` arguments carry their creature; here they are a
//! [`SkillOf`] (the creature's guid and the skill handle).
//!
//! Every `weapon.EnchantmentManager` / `wielder.EnchantmentManager` read goes through the caching
//! manager (`WorldObject.EnchantmentManager` is an `EnchantmentManagerWithCaching` for every
//! object), so these take `&mut World`.

#![allow(clippy::cast_possible_truncation)] // C#'s `(float)` of a double, as ACE writes it

use empyrean_common::dotnet::{math, CsCast};
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_entity::enums::{
    AttackType, ChatMessageType, CombatMode, DamageType, ImbuedEffectType, ItemType, MotionStance,
    PropertyFloat, PropertyInt, PropertyString, Skill, SkillAdvancementClass,
};
use empyrean_entity::ObjectGuid;

use crate::entity::spell::Spell;
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::world_objects::creature_combat::{self, shim};
use crate::world_objects::creature_equipment;
use crate::world_objects::creature_rating;
use crate::world_objects::entity::creature_skill::CreatureSkill;
use crate::world_objects::managers::enchantment_manager_with_caching as emc;
use crate::world_objects::world_object::WorldObject;
use crate::World;

/// Non-property fields declared in `WorldObject_Weapon.cs`.
#[derive(Debug, Default)]
pub struct WorldObjectWeaponFields {
    /// The cached answer of [`is_masterable`].
    // ACE: WorldObject.isMasterable
    pub is_masterable: Option<bool>,
}

// ============================================================================== helpers

fn object(w: &World, g: ObjectGuid) -> &WorldObject {
    w.objects.get(g).unwrap_or_else(|| {
        panic!(
            "System.NullReferenceException: object 0x{:08X} is not in World.objects",
            g.full()
        )
    })
}

/// ACE's `CreatureSkill` argument: one skill of one creature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SkillOf {
    pub creature: ObjectGuid,
    pub skill: CreatureSkill,
}

impl SkillOf {
    /// `Creature.GetCreatureSkill(skill)` (adding an untrained record when the biota lacks one).
    ///
    /// # Panics
    /// When the creature is not in the world (ACE: `NullReferenceException`).
    pub fn get(w: &mut World, creature: ObjectGuid, skill: Skill) -> SkillOf {
        let o = w
            .objects
            .get_mut(creature)
            .unwrap_or_else(|| panic!("System.NullReferenceException: creature {creature:?}"));
        let skill = o
            .get_creature_skill(skill, true)
            .expect("GetCreatureSkill(add: true) always answers");
        SkillOf { creature, skill }
    }

    /// `CreatureSkill.Base`.
    #[must_use]
    pub fn base(self, w: &World) -> u32 {
        self.skill.base(w, object(w, self.creature))
    }

    /// `CreatureSkill.Current`.
    #[must_use]
    pub fn current(self, w: &mut World) -> u32 {
        self.skill.current(w, self.creature)
    }

    /// `CreatureSkill.AdvancementClass`.
    #[must_use]
    pub fn advancement_class(self, w: &World) -> SkillAdvancementClass {
        self.skill.advancement_class(object(w, self.creature))
    }

    /// `CreatureSkill.InitLevel`.
    #[must_use]
    pub fn init_level(self, w: &World) -> u32 {
        self.skill.init_level(object(w, self.creature))
    }
}

// ============================================================================== members

/// Returns the primary weapon equipped by a creature (melee, missile, or wand).
/// ACE's default: `force_main_hand = false`.
// ACE: WorldObject.GetWeapon
fn get_weapon(w: &World, wielder: Option<ObjectGuid>, force_main_hand: bool) -> Option<ObjectGuid> {
    let wielder = wielder?;

    let mut weapon = creature_equipment::get_equipped_weapon(w, wielder, force_main_hand);

    if weapon.is_none() {
        weapon = creature_equipment::get_equipped_wand(w, wielder);
    }

    weapon
}

/// `wielder as Player`: the guid when it is a Player, else `null`.
fn as_player(w: &World, wielder: Option<ObjectGuid>) -> Option<ObjectGuid> {
    wielder.filter(|&g| w.objects.get(g).is_some_and(WorldObject::is_player))
}

// ACE: WorldObject.defaultModifier
const DEFAULT_MODIFIER: f32 = 1.0;

/// Returns the Melee Defense skill modifier for the current weapon.
// ACE: WorldObject.GetWeaponMeleeDefenseModifier
pub fn get_weapon_melee_defense_modifier(w: &mut World, wielder: Option<ObjectGuid>) -> f32 {
    // creatures only receive defense bonus in combat mode
    let Some(wielder) = wielder else {
        return DEFAULT_MODIFIER;
    };
    if creature_combat::combat_mode(w, wielder) == CombatMode::NonCombat {
        return DEFAULT_MODIFIER;
    }

    let mainhand = get_weapon(w, Some(wielder), true);
    let offhand = creature_equipment::get_dual_wield_weapon(w, wielder);

    match offhand {
        None => get_weapon_melee_defense_modifier_of(w, wielder, mainhand),
        Some(offhand) => {
            let mainhand_defense_mod = get_weapon_melee_defense_modifier_of(w, wielder, mainhand);
            let offhand_defense_mod =
                get_weapon_melee_defense_modifier_of(w, wielder, Some(offhand));

            math::max_f32(mainhand_defense_mod, offhand_defense_mod)
        }
    }
}

// ACE: WorldObject.GetWeaponMeleeDefenseModifier
fn get_weapon_melee_defense_modifier_of(
    w: &mut World,
    wielder: ObjectGuid,
    weapon: Option<ObjectGuid>,
) -> f32 {
    let Some(weapon) = weapon else {
        return DEFAULT_MODIFIER;
    };

    //var defenseMod = (float)(weapon.WeaponDefense ?? defaultModifier) + weapon.EnchantmentManager.GetDefenseMod();

    // TODO: Resolve this issue a better way?
    // Because of the way ACE handles default base values in recipe system (or rather the lack thereof)
    // we need to check the following weapon properties to see if they're below expected minimum and adjust accordingly
    // The issue is that the recipe system likely added 0.01 to 0 instead of 1, which is what *should* have happened.
    let wo = object(w, weapon);
    let weapon_defense = wo.weapon_defense();
    let mut base_wep_def = weapon_defense.unwrap_or(f64::from(DEFAULT_MODIFIER)) as f32;
    if weapon_defense.is_some_and(|d| d > 0.0)
        && weapon_defense.is_some_and(|d| d < 1.0)
        && (wo.get_property(PropertyInt::ImbueStackingBits).unwrap_or(0) & 4) != 0
    {
        base_wep_def += 1.0;
    }
    let enchantable = is_enchantable(wo);

    let mut defense_mod = base_wep_def + emc::get_defense_mod(w, weapon);

    if enchantable {
        defense_mod += emc::get_defense_mod(w, wielder);
    }

    defense_mod
}

/// Returns the Missile Defense skill modifier for the current weapon.
// ACE: WorldObject.GetWeaponMissileDefenseModifier
pub fn get_weapon_missile_defense_modifier(w: &mut World, wielder: ObjectGuid) -> f32 {
    let weapon = get_weapon(w, as_player(w, Some(wielder)), false);

    let Some(weapon) = weapon else {
        return DEFAULT_MODIFIER;
    };
    if creature_combat::combat_mode(w, wielder) == CombatMode::NonCombat {
        return DEFAULT_MODIFIER;
    }

    //// no enchantments?
    //return (float)(weapon.WeaponMissileDefense ?? 1.0f);

    let wo = object(w, weapon);
    let d = wo.weapon_missile_defense();
    let mut base_wep_def = d.unwrap_or(1.0) as f32;
    // TODO: Resolve this issue a better way? (the recipe system added 0.005 to 0 instead of 1)
    if d.is_some_and(|d| d > 0.0)
        && d.is_some_and(|d| d < 1.0)
        && (wo.get_property(PropertyInt::ImbueStackingBits).unwrap_or(0) & 1) == 1
    {
        base_wep_def += 1.0;
    }

    // no enchantments?
    base_wep_def
}

/// Returns the Magic Defense skill modifier for the current weapon.
// ACE: WorldObject.GetWeaponMagicDefenseModifier
pub fn get_weapon_magic_defense_modifier(w: &mut World, wielder: ObjectGuid) -> f32 {
    let weapon = get_weapon(w, as_player(w, Some(wielder)), false);

    let Some(weapon) = weapon else {
        return DEFAULT_MODIFIER;
    };
    if creature_combat::combat_mode(w, wielder) == CombatMode::NonCombat {
        return DEFAULT_MODIFIER;
    }

    //// no enchantments?
    //return (float)(weapon.WeaponMagicDefense ?? 1.0f);

    let wo = object(w, weapon);
    let d = wo.weapon_magic_defense();
    let mut base_wep_def = d.unwrap_or(1.0) as f32;
    // TODO: Resolve this issue a better way? (the recipe system added 0.005 to 0 instead of 1)
    if d.is_some_and(|d| d > 0.0)
        && d.is_some_and(|d| d < 1.0)
        && (wo.get_property(PropertyInt::ImbueStackingBits).unwrap_or(0) & 1) == 1
    {
        base_wep_def += 1.0;
    }

    // no enchantments?
    base_wep_def
}

/// Returns the attack skill modifier for the current weapon.
// ACE: WorldObject.GetWeaponOffenseModifier
pub fn get_weapon_offense_modifier(w: &mut World, wielder: Option<ObjectGuid>) -> f32 {
    // creatures only receive offense bonus in combat mode
    let Some(wielder) = wielder else {
        return DEFAULT_MODIFIER;
    };
    if creature_combat::combat_mode(w, wielder) == CombatMode::NonCombat {
        return DEFAULT_MODIFIER;
    }

    let mainhand = get_weapon(w, Some(wielder), true);
    let offhand = creature_equipment::get_dual_wield_weapon(w, wielder);

    match offhand {
        None => get_weapon_offense_modifier_of(w, wielder, mainhand),
        Some(offhand) => {
            let mainhand_attack_mod = get_weapon_offense_modifier_of(w, wielder, mainhand);
            let offhand_attack_mod = get_weapon_offense_modifier_of(w, wielder, Some(offhand));

            math::max_f32(mainhand_attack_mod, offhand_attack_mod)
        }
    }
}

// ACE: WorldObject.GetWeaponOffenseModifier
fn get_weapon_offense_modifier_of(
    w: &mut World,
    wielder: ObjectGuid,
    weapon: Option<ObjectGuid>,
) -> f32 {
    // HeartSeeker does not affect missile launchers (Letter to the Players, 2002/07 Repercussions):
    // launchers get defensive bonuses only.
    let Some(weapon) = weapon else {
        return DEFAULT_MODIFIER;
    };
    let wo = object(w, weapon);
    if wo.is_ranged()
    /* see note above */
    {
        return DEFAULT_MODIFIER;
    }

    let base = wo.weapon_offense().unwrap_or(f64::from(DEFAULT_MODIFIER)) as f32;
    let enchantable = is_enchantable(wo);
    let mut offense_mod = base + emc::get_attack_mod(w, weapon);

    if enchantable {
        offense_mod += emc::get_attack_mod(w, wielder);
    }

    offense_mod
}

/// Returns the Mana Conversion skill modifier for the current weapon.
// ACE: WorldObject.GetWeaponManaConversionModifier
pub fn get_weapon_mana_conversion_modifier(w: &mut World, wielder: ObjectGuid) -> f32 {
    let weapon = get_weapon(w, as_player(w, Some(wielder)), false);

    let Some(weapon) = weapon else {
        return DEFAULT_MODIFIER;
    };

    if creature_combat::combat_mode(w, wielder) != CombatMode::NonCombat {
        // hermetic link / void

        // base mod starts at 0
        let wo = object(w, weapon);
        let base_mod = wo.mana_conversion_mod().unwrap_or(0.0) as f32;
        let enchantable = is_enchantable(wo);

        // enchantments are multiplicative, so they are only effective if there is a base mod
        let mana_conv_mod = emc::get_mana_conv_mod(w, weapon);

        let mut aura_mana_conv_mod = 1.0f32;

        if enchantable {
            aura_mana_conv_mod = emc::get_mana_conv_mod(w, wielder);
        }

        let enchantment_mod = mana_conv_mod * aura_mana_conv_mod;

        return 1.0 + base_mod * enchantment_mod;
    }

    DEFAULT_MODIFIER
}

// ACE: WorldObject.defaultSpeed
const DEFAULT_SPEED: u32 = 40; // TODO: find default speed

/// Returns the weapon speed, with enchantments factored in.
// ACE: WorldObject.GetWeaponSpeed
pub fn get_weapon_speed(w: &mut World, wielder: Option<ObjectGuid>) -> u32 {
    let weapon = get_weapon(w, as_player(w, wielder), false);

    let base_speed: i32 = weapon
        .and_then(|g| object(w, g).weapon_time())
        .unwrap_or(DEFAULT_SPEED.cs_cast());

    let speed_mod = match weapon {
        Some(g) => emc::get_weapon_speed_mod(w, g),
        None => 0,
    };
    let aura_speed_mod = match wielder {
        Some(g) => emc::get_weapon_speed_mod(w, g),
        None => 0,
    };

    0i32.max(
        base_speed
            .wrapping_add(speed_mod)
            .wrapping_add(aura_speed_mod),
    )
    .cs_cast()
}

// ACE: WorldObject.defaultPhysicalCritFrequency
const DEFAULT_PHYSICAL_CRIT_FREQUENCY: f32 = 0.1; // 10% base chance

/// Returns the critical chance for the attack weapon.
///
/// # Panics
/// With no target (ACE: `NullReferenceException` on `target.GetCritResistRating()`).
// ACE: WorldObject.GetWeaponCriticalChance
pub fn get_weapon_critical_chance(
    w: &mut World,
    weapon: Option<ObjectGuid>,
    wielder: Option<ObjectGuid>,
    skill: Option<SkillOf>,
    target: ObjectGuid,
) -> f32 {
    let mut crit_rate = weapon
        .and_then(|g| object(w, g).critical_frequency())
        .unwrap_or(f64::from(DEFAULT_PHYSICAL_CRIT_FREQUENCY)) as f32;

    if weapon.is_some_and(|g| has_imbued_effect(object(w, g), ImbuedEffectType::CriticalStrike)) {
        let critical_strike_bonus = get_critical_strike_mod(w, skill, false);

        crit_rate = math::max_f32(crit_rate, critical_strike_bonus);
    }

    if let Some(wielder) = wielder {
        crit_rate += creature_rating::get_crit_rating(w, wielder) as f32 * 0.01f32;
    }

    // mitigation
    let crit_resist_rating_mod = creature_rating::get_negative_rating_mod(
        creature_rating::get_crit_resist_rating(w, target),
        false,
    );
    crit_rate *= crit_resist_rating_mod;

    crit_rate
}

// The 2002/08 Atonement letter gave 2% originally; the 2002/11 Iron Coast notes raised magic crits,
// probably to 5%, the minimum that CS magic scales from.

// ACE: WorldObject.defaultMagicCritFrequency
const DEFAULT_MAGIC_CRIT_FREQUENCY: f32 = 0.05;

/// Returns the critical chance for the caster weapon.
///
/// # Panics
/// With a weapon and no target (ACE: `NullReferenceException`).
// ACE: WorldObject.GetWeaponMagicCritFrequency
pub fn get_weapon_magic_crit_frequency(
    w: &mut World,
    weapon: Option<ObjectGuid>,
    wielder: ObjectGuid,
    skill: Option<SkillOf>,
    target: ObjectGuid,
) -> f32 {
    // TODO : merge with above function

    let Some(weapon) = weapon else {
        return DEFAULT_MAGIC_CRIT_FREQUENCY;
    };

    let mut crit_rate = object(w, weapon)
        .get_property(PropertyFloat::CriticalFrequency)
        .unwrap_or(f64::from(DEFAULT_MAGIC_CRIT_FREQUENCY)) as f32;

    if has_imbued_effect(object(w, weapon), ImbuedEffectType::CriticalStrike) {
        let is_pvp = object(w, wielder).is_player() && object(w, target).is_player();

        let critical_strike_mod = get_critical_strike_mod(w, skill, is_pvp);

        crit_rate = math::max_f32(crit_rate, critical_strike_mod);
    }

    crit_rate += creature_rating::get_crit_rating(w, wielder) as f32 * 0.01f32;

    // mitigation
    let crit_resist_rating_mod = creature_rating::get_negative_rating_mod(
        creature_rating::get_crit_resist_rating(w, target),
        false,
    );
    crit_rate *= crit_resist_rating_mod;

    crit_rate
}

// ACE: WorldObject.defaultCritDamageMultiplier
const DEFAULT_CRIT_DAMAGE_MULTIPLIER: f32 = 1.0;

/// Returns the critical damage multiplier for the attack weapon.
// ACE: WorldObject.GetWeaponCritDamageMod
#[must_use]
pub fn get_weapon_crit_damage_mod(
    w: &World,
    weapon: Option<ObjectGuid>,
    _wielder: Option<ObjectGuid>,
    skill: Option<SkillOf>,
    _target: ObjectGuid,
) -> f32 {
    let mut crit_damage_mod = weapon
        .and_then(|g| object(w, g).get_property(PropertyFloat::CriticalMultiplier))
        .unwrap_or(f64::from(DEFAULT_CRIT_DAMAGE_MULTIPLIER)) as f32;

    if weapon.is_some_and(|g| has_imbued_effect(object(w, g), ImbuedEffectType::CripplingBlow)) {
        let crippling_blow_mod = get_crippling_blow_mod(w, skill);

        crit_damage_mod = math::max_f32(crit_damage_mod, crippling_blow_mod);
    }
    crit_damage_mod
}

/// PvP damaged is halved, automatically displayed in the client.
///
/// ACE's value, kept for its anchor and **no longer applied**: the client's factor is 0.25 and the
/// server now uses it (V381; see [`get_caster_elemental_damage_modifier`]).
// ACE: WorldObject.ElementalDamageBonusPvPReduction
pub const ELEMENTAL_DAMAGE_BONUS_PVP_REDUCTION: f32 = 0.5;

/// Returns a multiplicative elemental damage modifier for the magic caster weapon type.
// ACE: WorldObject.GetCasterElementalDamageModifier
pub fn get_caster_elemental_damage_modifier(
    w: &mut World,
    weapon: Option<ObjectGuid>,
    wielder: Option<ObjectGuid>,
    target: Option<ObjectGuid>,
    damage_type: DamageType,
) -> f32 {
    let (Some(wielder), Some(weapon)) = (wielder, weapon) else {
        return 1.0;
    };
    let wo = object(w, weapon);
    if !wo.is_caster() || wo.w_damage_type() != damage_type {
        return 1.0;
    }

    let elemental_damage_mod = wo.elemental_damage_mod().unwrap_or(1.0);

    // additive to base multiplier
    let wielder_enchantments = emc::get_elemental_damage_mod(w, wielder);
    let weapon_enchantments = emc::get_elemental_damage_mod(w, weapon);

    let enchantments = wielder_enchantments + weapon_enchantments;

    let mut modifier = (elemental_damage_mod + f64::from(enchantments)) as f32;

    // **Retail's factor, not ACE's (V381).** ACE halved the bonus against
    // players; the client's examine panel shows a quarter, from the shared-tree function the
    // server now calls (`dereth_rules::combat::elemental_mod_pk_modifier`, in double, returned as
    // ACE's float). ACE's guard (only a bonus above 1, only against a player) is kept.
    if modifier > 1.0 && target.is_some_and(|t| object(w, t).is_player()) {
        #[allow(clippy::cast_possible_truncation)]
        let pk = dereth_rules::combat::elemental_mod_pk_modifier(f64::from(modifier)) as f32;
        modifier = pk;
    }

    modifier
}

/// Returns an additive elemental damage bonus for the missile launcher weapon type.
// ACE: WorldObject.GetMissileElementalDamageBonus
#[must_use]
pub fn get_missile_elemental_damage_bonus(
    w: &World,
    weapon: Option<ObjectGuid>,
    _wielder: Option<ObjectGuid>,
    damage_type: DamageType,
) -> i32 {
    if let Some(wo) = weapon.map(|g| object(w, g)) {
        if wo.is_missile_launcher() {
            if let Some(bonus) = wo.elemental_damage_bonus() {
                let elemental_damage_type = wo.w_damage_type();

                if elemental_damage_type != DamageType::Undef
                    && elemental_damage_type == damage_type
                {
                    return bonus;
                }
            }
        }
    }
    0
}

/// Returns the slayer damage multiplier for the attack weapon against a particular creature type.
// ACE: WorldObject.GetWeaponCreatureSlayerModifier
#[must_use]
pub fn get_weapon_creature_slayer_modifier(
    w: &World,
    weapon: Option<ObjectGuid>,
    _wielder: Option<ObjectGuid>,
    target: Option<ObjectGuid>,
) -> f32 {
    if let (Some(wo), Some(target)) = (weapon.map(|g| object(w, g)), target) {
        if let (Some(slayer), Some(bonus)) = (wo.slayer_creature_type(), wo.slayer_damage_bonus()) {
            if Some(slayer) == object(w, target).creature_type() {
                // TODO: scale with base weapon skill?
                return bonus as f32;
            }
        }
    }
    DEFAULT_MODIFIER
}

/// Returns the resistance modifier or rending modifier.
// ACE: WorldObject.GetWeaponResistanceModifier
#[must_use]
pub fn get_weapon_resistance_modifier(
    w: &World,
    weapon: Option<ObjectGuid>,
    wielder: Option<ObjectGuid>,
    skill: Option<SkillOf>,
    damage_type: DamageType,
) -> f32 {
    let mut resist_mod = DEFAULT_MODIFIER;

    let (Some(wielder), Some(weapon)) = (wielder, weapon) else {
        return DEFAULT_MODIFIER;
    };
    let wo = object(w, weapon);

    // handle quest weapon fixed resistance cleaving
    if wo
        .resistance_modifier_type()
        .is_some_and(|t| t == damage_type)
    {
        // 1.0 in the data, equivalent to a level 5 vuln
        resist_mod = 1.0
            + wo.resistance_modifier()
                .unwrap_or(f64::from(DEFAULT_MODIFIER)) as f32;
    }

    // handle elemental resistance rending
    let rend_damage_type = get_rend_damage_type(damage_type);

    if rend_damage_type == ImbuedEffectType::Undef {
        log::debug!(
            "{}.GetRendDamageType({}) unexpected damage type for {} ({})",
            shim::name(w, wielder),
            damage_type.to_dotnet_string(),
            shim::name(w, weapon),
            weapon.full()
        );
    }

    if rend_damage_type != ImbuedEffectType::Undef && has_imbued_effect(wo, rend_damage_type) {
        if let Some(skill) = skill {
            let rending_mod = get_rending_mod(w, Some(skill));

            resist_mod = math::max_f32(resist_mod, rending_mod);
        }
    }

    resist_mod
}

// ACE: WorldObject.GetImbuedEffects
#[must_use]
pub fn get_imbued_effects(o: &WorldObject) -> ImbuedEffectType {
    let get = |p| o.get_property(p).unwrap_or(0);
    ImbuedEffectType(
        (get(PropertyInt::ImbuedEffect)
            | get(PropertyInt::ImbuedEffect2)
            | get(PropertyInt::ImbuedEffect3)
            | get(PropertyInt::ImbuedEffect4)
            | get(PropertyInt::ImbuedEffect5))
        .cast_unsigned(),
    )
}

// ACE: WorldObject.HasImbuedEffect
#[must_use]
pub fn has_imbued_effect(o: &WorldObject, type_: ImbuedEffectType) -> bool {
    o.imbued_effect().contains(type_)
}

// ACE: WorldObject.GetRendDamageType
/// The rending imbue for a damage type; `Undef` for anything else (including combined flags).
#[must_use]
pub fn get_rend_damage_type(damage_type: DamageType) -> ImbuedEffectType {
    match damage_type {
        DamageType::Slash => ImbuedEffectType::SlashRending,
        DamageType::Pierce => ImbuedEffectType::PierceRending,
        DamageType::Bludgeon => ImbuedEffectType::BludgeonRending,
        DamageType::Fire => ImbuedEffectType::FireRending,
        DamageType::Cold => ImbuedEffectType::ColdRending,
        DamageType::Acid => ImbuedEffectType::AcidRending,
        DamageType::Electric => ImbuedEffectType::ElectricRending,
        DamageType::Nether => ImbuedEffectType::NetherRending,
        //log.DebugFormat("GetRendDamageType({0}) unexpected damage type", damageType);
        _ => ImbuedEffectType::Undef,
    }
}

/// Returns TRUE if this item is enchantable, as per the client formula.
// ACE: WorldObject.IsEnchantable
#[must_use]
pub fn is_enchantable(o: &WorldObject) -> bool {
    o.resist_magic().unwrap_or(0) < 9999
}

// Rending gives the weapon the ability to make its opponent vulnerable to attacks of a certain
// specific element; it scales with the wielder's base skill. (Anon's formula docs and the 2004/07
// Treaties in Stone letter give the imbue formulas and caps used below.)

// ACE: WorldObject.MaxCriticalStrikeMod
pub const MAX_CRITICAL_STRIKE_MOD: f32 = 0.5;

// ACE: WorldObject.GetCriticalStrikeMod
/// ACE's default: `is_pvp = false`.
#[must_use]
pub fn get_critical_strike_mod(w: &World, skill: Option<SkillOf>, is_pvp: bool) -> f32 {
    let skill_type = get_imbued_skill_type(skill);

    let base_skill = get_base_skill_imbued(w, skill);

    let mut base_mod = match skill_type {
        ImbuedSkillType::Melee => 0i32.max(base_skill.wrapping_sub(100)) as f32 / 600.0f32,

        ImbuedSkillType::Missile | ImbuedSkillType::Magic => {
            0i32.max(base_skill.wrapping_sub(60)) as f32 / 600.0f32
        }

        ImbuedSkillType::Undef => return 0.0,
    };

    // For PvE only: Critical Strike for War Magic scales from 5% to 25% (raised to 50% in July 2004).

    if skill_type == ImbuedSkillType::Magic && is_pvp {
        base_mod *= 0.5;
    }

    /*var criticalStrikeMod = skillType == ImbuedSkillType.Magic ? defaultMagicCritFrequency : defaultPhysicalCritFrequency;

    var minEffective = skillType == ImbuedSkillType.Magic ? MinCriticalStrikeMagicMod : defaultPhysicalCritFrequency;

    if (baseMod >= minEffective)
        criticalStrikeMod = baseMod;*/

    let default_crit_frequency = if skill_type == ImbuedSkillType::Magic {
        DEFAULT_MAGIC_CRIT_FREQUENCY
    } else {
        DEFAULT_PHYSICAL_CRIT_FREQUENCY
    };

    //Console.WriteLine($"CriticalStrikeMod: {criticalStrikeMod}");

    math::max_f32(default_crit_frequency, base_mod)
}

// ACE: WorldObject.MaxCripplingBlowMod
pub const MAX_CRIPPLING_BLOW_MOD: f32 = 6.0;

// ACE: WorldObject.GetCripplingBlowMod
#[must_use]
pub fn get_crippling_blow_mod(w: &World, skill: Option<SkillOf>) -> f32 {
    // increases the critical damage multiplier, additive
    // (PvP only, 2004/07: Crippling Blow for War Magic adds up to 500% of the spell's damage,
    // which sounds like a 6.0 multiplier)

    let base_skill = get_base_skill_imbued(w, skill);

    let base_mod = match get_imbued_skill_type(skill) {
        ImbuedSkillType::Melee => 0i32.max(base_skill.wrapping_sub(40)) as f32 / 60.0f32,

        ImbuedSkillType::Missile | ImbuedSkillType::Magic => base_skill as f32 / 60.0f32,

        ImbuedSkillType::Undef => 1.0f32,
    };

    //Console.WriteLine($"CripplingBlowMod: {cripplingBlowMod}");

    math::max_f32(1.0, base_mod)
}

// elemental rending cap, equivalent to level 6 vuln
// ACE: WorldObject.MaxRendingMod
pub const MAX_RENDING_MOD: f32 = 2.5;

// ACE: WorldObject.GetRendingMod
#[must_use]
pub fn get_rending_mod(w: &World, skill: Option<SkillOf>) -> f32 {
    let base_skill = get_base_skill_imbued(w, skill);

    let rending_mod = match get_imbued_skill_type(skill) {
        ImbuedSkillType::Melee => base_skill as f32 / 160.0f32,

        ImbuedSkillType::Missile | ImbuedSkillType::Magic => base_skill as f32 / 144.0f32,

        ImbuedSkillType::Undef => 1.0f32,
    };

    //Console.WriteLine($"RendingMod: {rendingMod}");

    math_clamp(rending_mod, 1.0, MAX_RENDING_MOD)
}

// ACE: WorldObject.MaxArmorRendingMod
pub const MAX_ARMOR_RENDING_MOD: f32 = 0.6;

// ACE: WorldObject.GetArmorRendingMod
#[must_use]
pub fn get_armor_rending_mod(w: &World, skill: Option<SkillOf>) -> f32 {
    // % of armor ignored, min 0%, max 60%

    let base_skill = get_base_skill_imbued(w, skill);

    let mut armor_rending_mod = 1.0f32;

    match get_imbued_skill_type(skill) {
        ImbuedSkillType::Melee => {
            armor_rending_mod -= 0i32.max(base_skill.wrapping_sub(160)) as f32 / 400.0f32
        }

        ImbuedSkillType::Missile => {
            armor_rending_mod -= 0i32.max(base_skill.wrapping_sub(144)) as f32 / 360.0f32
        }

        _ => {}
    }

    //Console.WriteLine($"ArmorRendingMod: {armorRendingMod}");

    armor_rending_mod
}

/// Armor Cleaving: the smaller of the creature's own and the weapon's.
// ACE: WorldObject.GetArmorCleavingMod
pub fn get_armor_cleaving_mod(w: &mut World, this: ObjectGuid, weapon: Option<ObjectGuid>) -> f32 {
    // investigate: should this value be on creatures directly?
    let creature_mod = get_armor_cleaving_mod_self(w, this);
    let weapon_mod = match weapon {
        Some(weapon) => get_armor_cleaving_mod_self(w, weapon),
        None => 1.0,
    };

    math::min_f32(creature_mod, weapon_mod)
}

// ACE: WorldObject.GetArmorCleavingMod
pub fn get_armor_cleaving_mod_self(w: &mut World, this: ObjectGuid) -> f32 {
    if object(w, this).ignore_armor().is_none() {
        return 1.0;
    }

    // FIXME: data
    let max_spell_level = crate::world_objects::world_object_magic::get_max_spell_level(w, this);

    // thanks to moro for this formula
    1.0 - (0.1 + max_spell_level as f32 * 0.05f32)
}

// ACE: WorldObject.GetIgnoreShieldMod
#[must_use]
pub fn get_ignore_shield_mod(w: &World, this: ObjectGuid, weapon: Option<ObjectGuid>) -> f32 {
    let creature_mod = object(w, this).ignore_shield().unwrap_or(0.0);
    let weapon_mod = weapon
        .and_then(|g| object(w, g).ignore_shield())
        .unwrap_or(0.0);

    1.0 - math::max(creature_mod, weapon_mod) as f32
}

// ACE: WorldObject.GetBaseSkillImbued
/// # Panics
/// With a null skill (ACE: `NullReferenceException` on `skill.Base`).
#[must_use]
pub fn get_base_skill_imbued(w: &World, skill: Option<SkillOf>) -> i32 {
    let base = |w: &World| {
        skill
            .expect("System.NullReferenceException: skill.Base")
            .base(w)
    };
    match get_imbued_skill_type(skill) {
        ImbuedSkillType::Melee => base(w).min(400).cs_cast(),

        _ => base(w).min(360).cs_cast(),
    }
}

/// ACE's nested `WorldObject.ImbuedSkillType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImbuedSkillType {
    #[default]
    Undef,
    Melee,
    Missile,
    Magic,
}

// ACE: WorldObject.GetImbuedSkillType
#[must_use]
pub fn get_imbued_skill_type(skill: Option<SkillOf>) -> ImbuedSkillType {
    match skill.map(|s| s.skill.skill) {
        Some(
            Skill::LightWeapons
            | Skill::HeavyWeapons
            | Skill::FinesseWeapons
            | Skill::DualWield
            | Skill::TwoHandedCombat
            // legacy
            | Skill::Axe
            | Skill::Dagger
            | Skill::Mace
            | Skill::Spear
            | Skill::Staff
            | Skill::Sword
            | Skill::UnarmedCombat,
        ) => ImbuedSkillType::Melee,

        Some(
            Skill::MissileWeapons
            // legacy
            | Skill::Bow
            | Skill::Crossbow
            | Skill::Sling
            | Skill::ThrownWeapon,
        ) => ImbuedSkillType::Missile,

        Some(Skill::WarMagic | Skill::VoidMagic | Skill::LifeMagic /* Martyr's Hecatomb */) => ImbuedSkillType::Magic,

        other => {
            log::debug!(
                "WorldObject_Weapon.GetImbuedSkillType({}): unexpected skill",
                other.map(Skill::to_dotnet_string).unwrap_or_default()
            );
            ImbuedSkillType::Undef
        }
    }
}

/// Returns the base skill multiplier to the maximum bonus. ACE's default: `use_min = true`.
// ACE: WorldObject.GetImbuedInterval
#[must_use]
pub fn get_imbued_interval(w: &World, skill: Option<SkillOf>, use_min: bool) -> f32 {
    let skill_type = get_imbued_skill_type(skill);

    let mut min = 0;
    if use_min {
        min = if skill_type == ImbuedSkillType::Melee {
            150
        } else {
            125
        };
    }
    let max = if skill_type == ImbuedSkillType::Melee {
        400
    } else {
        360
    };

    let base = skill
        .expect("System.NullReferenceException: skill.Base")
        .base(w);
    get_interval(base.cs_cast(), min, max)
}

// ACE: WorldObject.GetInterval
/// Returns an interval between 0-1.
#[must_use]
pub fn get_interval(num: i32, min: i32, max: i32) -> f32 {
    if num <= min {
        return 0.0;
    }
    if num >= max {
        return 1.0;
    }

    let range = max.wrapping_sub(min);

    num.wrapping_sub(min) as f32 / range as f32
}

// ACE: WorldObject.SetInterval
/// Projects a 0-1 interval between min and max.
#[must_use]
pub fn set_interval(interval: f32, min: f32, max: f32) -> f32 {
    let range = max - min;

    interval * range + min
}

/// Returns TRUE if this item has a proc / 'cast on strike' spell.
// ACE: WorldObject.HasProc
#[must_use]
pub fn has_proc(o: &WorldObject) -> bool {
    o.proc_spell().is_some()
}

/// Returns TRUE if this item has a proc spell that matches the input spell.
// ACE: WorldObject.HasProcSpell
#[must_use]
pub fn has_proc_spell(o: &WorldObject, spell_id: u32) -> bool {
    has_proc(o) && o.proc_spell() == Some(spell_id)
}

/// `Aetheria.CalcProcRate(this, wielder)` (`Entity/Aetheria.cs`), a float.
fn aetheria_calc_proc_rate(w: &World, this: ObjectGuid, wielder: ObjectGuid) -> f64 {
    f64::from(crate::entity::aetheria::calc_proc_rate(w, this, wielder))
}

// ACE: WorldObject.TryProcItem
/// # Panics
/// With no `ProcSpell` (ACE: `InvalidOperationException` on `ProcSpell.Value`), or a player
/// attacker without a session.
pub fn try_proc_item(
    w: &mut World,
    this: ObjectGuid,
    attacker: ObjectGuid,
    target: Option<ObjectGuid>,
    self_target: bool,
) {
    // roll for a chance of casting spell
    let mut chance = object(w, this).proc_spell_rate().unwrap_or(0.0);

    // special handling for aetheria
    if crate::entity::aetheria::is_aetheria(object(w, this).biota.weenie_class_id)
        && object(w, attacker).is_creature()
    {
        chance = aetheria_calc_proc_rate(w, this, attacker);
    }

    let rng = ThreadSafeRandom::next_float(0.0, 1.0);
    if rng >= chance {
        return;
    }

    let proc_spell = object(w, this)
        .proc_spell()
        .expect("System.InvalidOperationException: Nullable object must have a value.");
    let spell = Spell::new(w, proc_spell, true);

    if spell.not_found() {
        if object(w, attacker).is_player() {
            let msg = if spell.spell_base.is_none() {
                game_message_system_chat(
                    &format!("SpellId {proc_spell} Invalid."),
                    ChatMessageType::System,
                )
            } else {
                game_message_system_chat(
                    &format!("{} spell not implemented, yet!", spell.name()),
                    ChatMessageType::System,
                )
            };
            crate::world_objects::player_skills::send(w, attacker, [msg]);
        }
        return;
    }

    // not sure if this should go before or after the resist check
    // after would match Player_Magic, but would require changing the signature of TryCastSpell yet again
    // starting with the simpler check here
    if !self_target {
        if let Some(target) = target {
            if object(w, target).non_projectile_magic_immune() && !spell.is_projectile() {
                if object(w, attacker).is_player() {
                    let msg = game_message_system_chat(
                        &format!(
                            "You fail to affect {} with {}",
                            shim::name(w, target),
                            spell.name()
                        ),
                        ChatMessageType::Magic,
                    );
                    crate::world_objects::player_skills::send(w, attacker, [msg]);
                }

                return;
            }
        }
    }

    let item_caster = if object(w, this).is_creature() {
        None
    } else {
        Some(this)
    };

    use crate::world_objects::world_object_magic::{try_cast_spell, try_cast_spell_with_redirects};
    if spell.non_component_target_type() == ItemType::None {
        try_cast_spell(
            w,
            attacker,
            &spell,
            None,
            item_caster,
            item_caster,
            true,
            true,
            true,
        );
    } else if spell.non_component_target_type() == ItemType::Vestements {
        // TODO: spell.NonComponentTargetType should probably always go through TryCastSpell_WithItemRedirects,
        // however i don't feel like testing every possible known type of item procspell in the current db to ensure there are no regressions
        // current test case: 33990 Composite Bow casting Tattercoat
        let _ = try_cast_spell_with_redirects(
            w,
            attacker,
            &spell,
            target,
            item_caster,
            item_caster,
            true,
            true,
            true,
        );
    } else {
        try_cast_spell(
            w,
            attacker,
            &spell,
            target,
            item_caster,
            item_caster,
            true,
            true,
            true,
        );
    }
}

/// The weapon heritage-mastery test (cached per object, as ACE does).
// ACE: WorldObject.IsMasterable
pub fn is_masterable(o: &mut WorldObject) -> bool {
    // should be based on this, but a bunch of the weapon data probably needs to be updated...
    //return W_WeaponType != WeaponType.Undef;

    // cache this?
    if o.wo.world_object_weapon.is_masterable.is_none() {
        let v = match o.get_property(PropertyString::LongDesc) {
            None => true,
            Some(d) => !contains_ordinal_ignore_case(&d, "This weapon seems tough to master."),
        };
        o.wo.world_object_weapon.is_masterable = Some(v);
    }

    o.wo.world_object_weapon.is_masterable.unwrap_or(true)
}

/// `string.Contains(value, StringComparison.OrdinalIgnoreCase)` for an ASCII needle.
fn contains_ordinal_ignore_case(haystack: &str, needle: &str) -> bool {
    haystack.to_uppercase().contains(&needle.to_uppercase())
}

// from the Dark Majesty strategy guide, page 150:

// -   0 - 1/3 sec. Power-up Time = High Stab
// - 1/3 - 2/3 sec. Power-up Time = High Backhand
// -       2/3 sec+ Power-up Time = High Slash

// ACE: WorldObject.ThrustThreshold
pub const THRUST_THRESHOLD: f32 = 0.33;

/// Returns TRUE if this is a thrust/slash weapon, or if this weapon uses 2 different attack types
/// based on the ThrustThreshold.
// ACE: WorldObject.IsThrustSlash
#[must_use]
pub fn is_thrust_slash(o: &WorldObject) -> bool {
    let t = o.w_attack_type();
    t.contains(AttackType::Slash | AttackType::Thrust)
        || t.contains(AttackType::DoubleSlash | AttackType::DoubleThrust)
        || t.contains(AttackType::TripleSlash | AttackType::TripleThrust)
        || t.contains(AttackType::DoubleSlash) // stiletto
}

/// `if power_level >= ThrustThreshold [|| extra] { a } else { b }`.
fn by_power(power_level: f32, or: bool, a: AttackType, b: AttackType) -> AttackType {
    if power_level >= THRUST_THRESHOLD || or {
        a
    } else {
        b
    }
}

// ACE: WorldObject.GetAttackType
#[must_use]
pub fn get_attack_type(
    w: &World,
    this: ObjectGuid,
    stance: MotionStance,
    power_level: f32,
    offhand: bool,
) -> AttackType {
    if offhand {
        return get_offhand_attack_type(w, this, stance, power_level);
    }

    let mut attack_type = object(w, this).w_attack_type();

    if (attack_type & AttackType::Offhand) != AttackType::Undef {
        log::warn!(
            "{} ({}, {}).GetAttackType(): {}",
            shim::name(w, this),
            this.full(),
            object(w, this).biota.weenie_class_id,
            attack_type.to_dotnet_string()
        );
        attack_type &= !AttackType::Offhand;
    }

    if stance == MotionStance::DualWieldCombat {
        if attack_type.contains(AttackType::TripleThrust | AttackType::TripleSlash) {
            attack_type = by_power(
                power_level,
                false,
                AttackType::TripleSlash,
                AttackType::TripleThrust,
            );
        } else if attack_type.contains(AttackType::DoubleThrust | AttackType::DoubleSlash) {
            attack_type = by_power(
                power_level,
                false,
                AttackType::DoubleSlash,
                AttackType::DoubleThrust,
            );
        }
        // handle old bugged stilettos that only have DoubleThrust
        // handle old bugged rapiers w/ Thrust, DoubleThrust
        else if attack_type.contains(AttackType::DoubleThrust) {
            attack_type = by_power(
                power_level,
                !attack_type.contains(AttackType::Thrust),
                AttackType::DoubleThrust,
                AttackType::Thrust,
            );
        }
        // handle old bugged poniards and newer tachis
        else if attack_type.contains(AttackType::Thrust | AttackType::DoubleSlash) {
            attack_type = by_power(
                power_level,
                false,
                AttackType::DoubleSlash,
                AttackType::Thrust,
            );
        }
        // gaerlan sword / py16 (iasparailaun)
        else if attack_type.contains(AttackType::Thrust | AttackType::TripleSlash) {
            attack_type = by_power(
                power_level,
                false,
                AttackType::TripleSlash,
                AttackType::Thrust,
            );
        }
    } else if stance == MotionStance::SwordShieldCombat {
        // force thrust animation when using a shield with a multi-strike weapon
        if attack_type.contains(AttackType::TripleThrust) {
            attack_type = by_power(
                power_level,
                !attack_type.contains(AttackType::Thrust),
                AttackType::TripleThrust,
                AttackType::Thrust,
            );
        } else if attack_type.contains(AttackType::DoubleThrust) {
            attack_type = by_power(
                power_level,
                !attack_type.contains(AttackType::Thrust),
                AttackType::DoubleThrust,
                AttackType::Thrust,
            );
        }
        // handle old bugged poniards and newer tachis w/ Thrust, DoubleSlash
        // and gaerlan sword / py16 (iasparailaun) w/ Thrust, TripleSlash
        else if attack_type.contains(AttackType::Thrust)
            && (attack_type & (AttackType::DoubleSlash | AttackType::TripleSlash))
                != AttackType::Undef
        {
            attack_type = AttackType::Thrust;
        }
    } else if stance == MotionStance::SwordCombat {
        // force slash animation when using no shield with a multi-strike weapon
        if attack_type.contains(AttackType::TripleSlash) {
            attack_type = by_power(
                power_level,
                !attack_type.contains(AttackType::Thrust),
                AttackType::TripleSlash,
                AttackType::Thrust,
            );
        } else if attack_type.contains(AttackType::DoubleSlash) {
            attack_type = by_power(
                power_level,
                !attack_type.contains(AttackType::Thrust),
                AttackType::DoubleSlash,
                AttackType::Thrust,
            );
        }
        // handle old bugged stilettos that only have DoubleThrust
        else if attack_type.contains(AttackType::DoubleThrust) {
            attack_type = AttackType::Thrust;
        }
    }

    if attack_type.contains(AttackType::Thrust | AttackType::Slash) {
        attack_type = by_power(power_level, false, AttackType::Slash, AttackType::Thrust);
    }

    attack_type
}

// ACE: WorldObject.GetOffhandAttackType
#[must_use]
pub fn get_offhand_attack_type(
    w: &World,
    this: ObjectGuid,
    _stance: MotionStance,
    power_level: f32,
) -> AttackType {
    let mut attack_type = object(w, this).w_attack_type();

    if (attack_type & AttackType::Offhand) != AttackType::Undef {
        log::warn!(
            "{} ({}, {}).GetOffhandAttackType(): {}",
            shim::name(w, this),
            this.full(),
            object(w, this).biota.weenie_class_id,
            attack_type.to_dotnet_string()
        );
        attack_type &= !AttackType::Offhand;
    }

    if attack_type.contains(AttackType::TripleThrust | AttackType::TripleSlash) {
        attack_type = by_power(
            power_level,
            false,
            AttackType::OffhandTripleSlash,
            AttackType::OffhandTripleThrust,
        );
    } else if attack_type.contains(AttackType::DoubleThrust | AttackType::DoubleSlash) {
        attack_type = by_power(
            power_level,
            false,
            AttackType::OffhandDoubleSlash,
            AttackType::OffhandDoubleThrust,
        );
    }
    // handle old bugged stilettos that only have DoubleThrust
    // handle old bugged rapiers w/ Thrust, DoubleThrust
    else if attack_type.contains(AttackType::DoubleThrust) {
        attack_type = by_power(
            power_level,
            !attack_type.contains(AttackType::Thrust),
            AttackType::OffhandDoubleThrust,
            AttackType::OffhandThrust,
        );
    }
    // handle old bugged poniards and newer tachis w/ Thrust, DoubleSlash
    else if attack_type.contains(AttackType::Thrust | AttackType::DoubleSlash) {
        attack_type = by_power(
            power_level,
            false,
            AttackType::OffhandDoubleSlash,
            AttackType::OffhandThrust,
        );
    }
    // gaerlan sword / py16 (iasparailaun) w/ Thrust, TripleSlash
    else if attack_type.contains(AttackType::Thrust | AttackType::TripleSlash) {
        attack_type = by_power(
            power_level,
            false,
            AttackType::OffhandTripleSlash,
            AttackType::OffhandThrust,
        );
    } else if attack_type.contains(AttackType::Thrust | AttackType::Slash) {
        attack_type = by_power(
            power_level,
            false,
            AttackType::OffhandSlash,
            AttackType::OffhandThrust,
        );
    } else {
        match attack_type {
            AttackType::Thrust => attack_type = AttackType::OffhandThrust,

            AttackType::Slash => attack_type = AttackType::OffhandSlash,

            AttackType::Punch => attack_type = AttackType::OffhandPunch,
            _ => {}
        }
    }
    attack_type
}

/// `Math.Clamp(float, float, float)` (min <= max here): a NaN passes through.
fn math_clamp(value: f32, min: f32, max: f32) -> f32 {
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}
