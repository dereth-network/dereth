// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Creature_Rating.cs
//! Port of `Source/ACE.Server/WorldObjects/Creature_Rating.cs`.
//!
//! The rating arithmetic (`GetPositiveRatingMod`, `GetNegativeRatingMod`, `ModToRating`, ...) is
//! pure and takes no creature. The rating getters read the creature's own property, its enchantment
//! registry (through the caching `EnchantmentManager`), its equipped items' rating cache and, for a
//! player, the augmentations; they are `(w, this)` free functions because the enchantment reads
//! fill caches.
//!
//! ACE overloads four names: `GetDamageRating()` and `GetDamageRating(int)`, and likewise
//! `GetHealingBoostRating`, `GetManaChargeRating` and `GetPKDamageRating`. The getter keeps the
//! plain name; the `(int)` modifier overload takes an `_int` suffix. The other `(int)` modifier
//! wrappers are instance methods in ACE that never read the creature, so they take no `this`.

use empyrean_common::dotnet::{math, CsCast};
use empyrean_entity::enums::{PropertyInt, Skill, SkillAdvancementClass};
use empyrean_entity::ObjectGuid;

use crate::world_objects::creature_combat::{self, CombatType};
use crate::world_objects::creature_equipment;
use crate::world_objects::managers::enchantment_manager_with_caching as emc;
use crate::world_objects::world_object::WorldObject;
use crate::world_objects::world_object_weapon::SkillOf;
use crate::World;

/// Non-property fields declared in `Creature_Rating.cs`.
#[derive(Debug, Default)]
pub struct CreatureRatingFields {}

// Ratings: http://asheron.wikia.com/wiki/Rating

// ACE: Creature.GetRatingMod
/// A rating as a multiplier: 1.0 for 0, [`get_positive_rating_mod`] above, the negative mod of
/// `-rating` below.
#[must_use]
pub fn get_rating_mod(rating: i32) -> f32 {
    if rating == 0 {
        return 1.0;
    }

    if rating >= 0 {
        get_positive_rating_mod(rating)
    } else {
        get_negative_rating_mod(rating.wrapping_neg(), false)
    }
}

// ACE: Creature.GetPositiveRatingMod
// ACE-BUG: for int.MinValue, -rating wraps back to int.MinValue and GetPositiveRatingMod and GetNegativeRatingMod recurse into each other until the stack overflows (kept).
/// Returns a 1.xx rating modifier by default, or a 0.xx rating modifier if negative.
#[must_use]
pub fn get_positive_rating_mod(rating: i32) -> f32 {
    if rating < 0 {
        return get_negative_rating_mod(rating.wrapping_neg(), false);
    }

    // formula: (100 + rating) / 100 = 1.xx modifier
    100i32.wrapping_add(rating) as f32 / 100.0f32
}

// ACE: Creature.GetNegativeRatingMod
/// Returns a 0.xx rating modifier by default, or a 1.xx rating modifier if negative.
///
/// With `allow_bug`, a negative rating is clamped to -99 and produces the unbalanced (> 1)
/// modifier that ACE keeps for void DRR reduction.
#[must_use]
pub fn get_negative_rating_mod(rating: i32, allow_bug: bool) -> f32 {
    let mut rating = rating;

    if rating < 0 && !allow_bug {
        return get_positive_rating_mod(rating.wrapping_neg());
    }

    if allow_bug {
        // with the bug allowed for DRR reduction from void dots,
        // this method will produce unbalanced modifiers for negative ratings
        // as negative rating approaches -100, it ramps up in a curve to infinity, eventually getting a divide by 0 crash for -100
        // values less than -100 would produce negative multipliers, which would result in undefined behavior throughout the system,
        // such as negative damage numbers. even with the bug enabled, we still limit to -99 on the lower end to prevent system failure

        rating = rating.max(-99);
    }

    // formula: 100 / (100 + rating) = 0.xx modifier
    100.0f32 / 100i32.wrapping_add(rating) as f32
}

// - Damage rating - increases all damage done to a target, including critical hits
// Formula: (100 + <total damage rating>) / 100 = 1.xx modifier to damage
// ACE: Creature.GetDamageRating
/// `GetDamageRating(int damageRating)`.
#[must_use]
pub fn get_damage_rating_int(damage_rating: i32) -> f32 {
    get_positive_rating_mod(damage_rating)
}

// - Critical damage rating - increases critical hit damage done to a target
// Formula: (100 + <total critical damage rating>) / 100 = 1.xx modifier to critical damage
// ACE: Creature.GetCriticalDamageRating
#[must_use]
pub fn get_critical_damage_rating(critical_dmg_rating: i32) -> f32 {
    get_positive_rating_mod(critical_dmg_rating)
}

// - Damage resistance rating - decreases the amount of all incoming damage, including critical hits
// Formula: 100 / (100 + <total damage resistance rating>) = 0.xxx modifier to incoming damage
// ACE: Creature.GetDamageResistanceRating
#[must_use]
pub fn get_damage_resistance_rating(damage_resistance_rating: i32) -> f32 {
    get_negative_rating_mod(damage_resistance_rating, false)
}

// - Critical damage resistance rating - decreases critical hit damage received
// Formula: 100 / (100 + <total critical damage resistance rating>) = 0.xxx modifier to critical hit damage received
// ACE: Creature.GetCriticalDamageResistanceRating
#[must_use]
pub fn get_critical_damage_resistance_rating(critical_dmg_resistance_rating: i32) -> f32 {
    get_negative_rating_mod(critical_dmg_resistance_rating, false)
}

// - DoT resistance rating - decreases the effectiveness of damage over time (DoT) spells cast upon the user
// Formula: 100 / (100 + <total DoT reduction rating> = 0.xxx modifier to outgoing incoming DoT attacks
// ACE: Creature.GetDamageOverTimeResistanceRating
#[must_use]
pub fn get_damage_over_time_resistance_rating(dot_resistance_rating: i32) -> f32 {
    get_negative_rating_mod(dot_resistance_rating, false)
}

// - Health drain resistance rating - decreases the effect of drain spells cast upon the target
// Formula: 100 / (100 + <total health drain resistance rating>) = 0.xxx modifier to incoming health drains
// ACE: Creature.GetHealthDrainResistanceRating
#[must_use]
pub fn get_health_drain_resistance_rating(health_drain_resist_rating: i32) -> f32 {
    get_negative_rating_mod(health_drain_resist_rating, false)
}

// - Healing boost rating - increases the amount of health received from consumables, healing kits, and life magic
// Formula: (100 + <total healing rating>) / 100 = 1.xx modifier to healing
// ACE: Creature.GetHealingBoostRating
/// `GetHealingBoostRating(int healingBoostRating)`.
#[must_use]
pub fn get_healing_boost_rating_int(healing_boost_rating: i32) -> f32 {
    get_positive_rating_mod(healing_boost_rating)
}

// - Aetheria Surge rating - increases the chance that Aetheria will surge
// Formula: (100 + <aetheria surge rating>) / 100 = 1.xx modifier to Aetheria Surges
// ACE: Creature.GetAetheriaSurgeRating
#[must_use]
pub fn get_aetheria_surge_rating(aetheria_surge_rating: i32) -> f32 {
    get_positive_rating_mod(aetheria_surge_rating)
}

// - Mana charge rating - increases the amount of mana released from mana stones into magical items
// Formula: (100 + <mana charge rating>) / 100 = 1.xx modifier to mana charges
// ACE: Creature.GetManaChargeRating
/// `GetManaChargeRating(int manaChargeRating)`.
#[must_use]
pub fn get_mana_charge_rating_int(mana_charge_rating: i32) -> f32 {
    get_positive_rating_mod(mana_charge_rating)
}

// - Mana reduction rating - decreases the amount of mana consumed in equipped magical items
// Formula: 100 / (100 + <mana reduction rating>) = 0.xxx modifier to magical item mana consumed
// ACE: Creature.GetManaReductionRating
#[must_use]
pub fn get_mana_reduction_rating(mana_reduction_rating: i32) -> f32 {
    get_negative_rating_mod(mana_reduction_rating, false)
}

// - Damage reduction rating - debuff applied to decrease the effective damage rating of the target
// Formula: 100 / (100 + <total damage reduction rating>) = 0.xxx modifier to outgoing damage
// ACE: Creature.GetDamageReductionRating
#[must_use]
pub fn get_damage_reduction_rating(damage_reduction_rating: i32) -> f32 {
    get_negative_rating_mod(damage_reduction_rating, false)
}

// - Healing reduction rating - debuff applied to decrease the effective healing rate of the target
// Formula: 100 / (100 + <total healing reduction rating>) = 0.xxx modifier to healing
// ACE: Creature.GetHealingReductionRating
#[must_use]
pub fn get_healing_reduction_rating(healing_reduction_rating: i32) -> f32 {
    get_negative_rating_mod(healing_reduction_rating, false)
}

// - Damage resistance reduction rating - debuff applied to decrease the effective damage resistance rating of the target
// Formula: 100 / (100 + <total damage resistance reduction rating>) = 0.xxx modifier to incoming damage
// ACE: Creature.GetDamageResistanceReductionRating
#[must_use]
pub fn get_damage_resistance_reduction_rating(damage_resist_reduce_rating: i32) -> f32 {
    get_negative_rating_mod(damage_resist_reduce_rating, false)
}

// - Player Killer damage rating - increases all damage done to other players, including critical hits
// Formula: (100 + <total PK damage rating> / 100 = 1.xx modifier to PK damage
// ACE: Creature.GetPKDamageRating
/// `GetPKDamageRating(int pkDamageRating)`.
#[must_use]
pub fn get_pk_damage_rating_int(pk_damage_rating: i32) -> f32 {
    get_positive_rating_mod(pk_damage_rating)
}

// - Player Killer damage resistance rating - decreases all incoming damage done by other players, including critical hits
// Formula: 100 / (100 + <total PK damage resistance rating>) = 0.xxx modifier to incoming PK damage
// ACE: Creature.GetPKDamageResistanceRating
#[must_use]
pub fn get_pk_damage_resistance_rating(pk_damage_resist_rating: i32) -> f32 {
    get_negative_rating_mod(pk_damage_resist_rating, false)
}

// ACE: Creature.ModToRating
/// Converts a 1.xx modifier to a +x rating, or a 0.xx modifier to a -x rating.
#[must_use]
pub fn mod_to_rating(r#mod: f32) -> i32 {
    if r#mod >= 1.0 {
        math::round(f64::from(r#mod * 100.0 - 100.0)).cs_cast()
    } else {
        math::round(f64::from(-100.0 / r#mod + 100.0)).cs_cast()
    }
}

// ACE: Creature.NegativeModToRating
/// Returns a modifier to an implicitly negative rating (ie. damage resistance).
#[must_use]
pub fn negative_mod_to_rating(r#mod: f32) -> i32 {
    mod_to_rating(r#mod).wrapping_neg()
}

// ACE: Creature.AdditiveCombine
/// Combines rating modifiers additively.
#[must_use]
pub fn additive_combine(mods: &[f32]) -> f32 {
    let mut total_rating: i32 = 0;

    for &m in mods {
        total_rating = total_rating.wrapping_add(mod_to_rating(m));
    }

    get_rating_mod(total_rating)
}

// ============================================================================== the getters

fn object(w: &World, g: ObjectGuid) -> &WorldObject {
    w.objects.get(g).unwrap_or_else(|| {
        panic!(
            "System.NullReferenceException: object 0x{:08X} is not in World.objects",
            g.full()
        )
    })
}

/// `this is Player`.
fn is_player(w: &World, this: ObjectGuid) -> bool {
    object(w, this).is_player()
}

/// `GetEquippedItemsRatingSum(rating)` (`Creature_Equipment.cs`).
fn equipment(w: &World, this: ObjectGuid, rating: PropertyInt) -> i32 {
    creature_equipment::get_equipped_items_rating_sum(w, this, rating)
}

// ACE: Creature.GetDamageRating
/// The creature's total damage rating: its own, equipment, additive enchantments less weakness,
/// and a player's augmentations (3 per Aug) and luminance augmentation.
pub fn get_damage_rating(w: &mut World, this: ObjectGuid) -> i32 {
    // get from base properties (monsters)?
    let damage_rating = object(w, this).damage_rating().unwrap_or(0);

    // additive enchantments
    let enchantments = emc::get_rating(w, this, PropertyInt::DamageRating);

    // equipment ratings
    let equipment = equipment(w, this, PropertyInt::GearDamage);

    // weakness as negative damage rating?
    // TODO: this should be factored in as a separate weakness rating...
    let weakness_rating = emc::get_rating(w, this, PropertyInt::WeaknessRating);

    let mut aug_bonus = 0i32;
    let mut lum_aug_bonus = 0i32;

    if is_player(w, this) {
        let player = object(w, this);
        aug_bonus = player.augmentation_damage_bonus().wrapping_mul(3);
        lum_aug_bonus = player.lum_aug_damage_rating();
    }

    // heritage / weapon type bonus factored in elsewhere?
    damage_rating
        .wrapping_add(equipment)
        .wrapping_add(enchantments)
        .wrapping_sub(weakness_rating)
        .wrapping_add(aug_bonus)
        .wrapping_add(lum_aug_bonus)
}

// ACE: Creature.GetDamageResistRating
/// `GetDamageResistRating(CombatType? combatType = null, bool directDamage = true)`: nether DoTs
/// count as negative DRR on direct damage; a player adds augmentations, luminance and the
/// specialized-defense bonus.
pub fn get_damage_resist_rating(
    w: &mut World,
    this: ObjectGuid,
    combat_type: Option<CombatType>,
    direct_damage: bool,
) -> i32 {
    // get from base properties (monsters)?
    let damage_resist_rating = object(w, this).damage_resist_rating().unwrap_or(0);

    // additive enchantments
    let enchantments = emc::get_rating(w, this, PropertyInt::DamageResistRating);

    // equipment ratings
    let equipment = equipment(w, this, PropertyInt::GearDamageResist);

    // nether DoTs as negative DRR?
    // TODO: this should be factored in as a separate nether damage rating...
    let nether_dot_damage_rating = if direct_damage {
        emc::get_nether_dot_damage_rating(w, this)
    } else {
        0
    };

    let mut aug_bonus = 0i32;
    let mut lum_aug_bonus = 0i32;
    let mut spec_bonus = 0i32;

    if is_player(w, this) {
        let player = object(w, this);
        aug_bonus = player.augmentation_damage_reduction().wrapping_mul(3);
        lum_aug_bonus = player.lum_aug_damage_reduction_rating();
        spec_bonus = get_spec_defense_bonus(w, this, combat_type);
    }

    damage_resist_rating
        .wrapping_add(equipment)
        .wrapping_add(enchantments)
        .wrapping_sub(nether_dot_damage_rating)
        .wrapping_add(aug_bonus)
        .wrapping_add(lum_aug_bonus)
        .wrapping_add(spec_bonus)
}

// ACE: Creature.GetDamageResistRatingMod
/// `GetDamageResistRatingMod(CombatType? combatType = null, bool directDamage = true)`: the total
/// DRR as a modifier, on the unbalanced curve when `allow_negative_rating_curve` is set.
pub fn get_damage_resist_rating_mod(
    w: &mut World,
    this: ObjectGuid,
    combat_type: Option<CombatType>,
    direct_damage: bool,
) -> f32 {
    let damage_resist_rating = get_damage_resist_rating(w, this, combat_type, direct_damage);

    let allow_bug =
        crate::managers::property_manager::get_bool(w, "allow_negative_rating_curve", false, true)
            .item;

    get_negative_rating_mod(damage_resist_rating, allow_bug)
}

// ACE: Creature.GetSpecDefenseBonus
/// Specialized defenses add 1 damage resist rating per 60 base points of Melee Defense against
/// melee, and per 50 of Missile or Magic Defense against missile or magic (players only).
pub fn get_spec_defense_bonus(
    w: &mut World,
    this: ObjectGuid,
    combat_type: Option<CombatType>,
) -> i32 {
    // https://asheron.fandom.com/wiki/Announcements_-_2013/02_-_Balance_of_Power

    // New bonus added to specialized defenses against damage of their respective attack type. (Applied in both PvE & PvP)

    // - Specialized Melee Defense skill now adds 1 Damage Rating Resist for every 60 pts against melee attacks
    // - Specialized Missile Defense skill now adds 1 Damage Rating Resist for every 50 pts against missile attacks
    // - Specialized Magic Defense skill now adds 1 Damage Rating Resist for every 50 pts against magic attacks

    // only applies to players
    let Some(combat_type) = combat_type else {
        return 0;
    };
    if !is_player(w, this) {
        return 0;
    }

    let skill = creature_combat::get_defense_skill(combat_type);
    let creature_skill = SkillOf::get(w, this, skill);

    // ensure defense skill is specialized
    if creature_skill.advancement_class(w) != SkillAdvancementClass::Specialized {
        return 0;
    }

    let divisor: i32 = if skill == Skill::MeleeDefense { 60 } else { 50 };

    // floor?
    let base: i32 = creature_skill.base(w).cs_cast();
    base / divisor
}

// ACE: Creature.GetCritRating
/// Crit chance rating: its own, enchantments, equipment and a player's Critical Expertise aug.
pub fn get_crit_rating(w: &mut World, this: ObjectGuid) -> i32 {
    // crit chance

    // get from base properties (monsters)?
    let crit_chance_rating = object(w, this).crit_rating().unwrap_or(0);

    // additive enchantments
    let enchantments = emc::get_rating(w, this, PropertyInt::CritRating);

    // equipment ratings
    let equipment = equipment(w, this, PropertyInt::GearCrit);

    // augmentations
    let mut aug_bonus = 0i32;

    if is_player(w, this) {
        aug_bonus = object(w, this).augmentation_critical_expertise();
    }

    crit_chance_rating
        .wrapping_add(enchantments)
        .wrapping_add(equipment)
        .wrapping_add(aug_bonus)
}

// ACE: Creature.GetCritDamageRating
pub fn get_crit_damage_rating(w: &mut World, this: ObjectGuid) -> i32 {
    // get from base properties (monsters)?
    let crit_damage_rating = object(w, this).crit_damage_rating().unwrap_or(0);

    // additive enchantments
    let enchantments = emc::get_rating(w, this, PropertyInt::CritDamageRating);

    // equipment ratings
    let equipment = equipment(w, this, PropertyInt::GearCritDamage);

    // augmentations
    let mut aug_bonus = 0i32;
    let mut lum_aug_bonus = 0i32;

    if is_player(w, this) {
        let player = object(w, this);
        aug_bonus = player.augmentation_critical_power().wrapping_mul(3);
        lum_aug_bonus = player.lum_aug_crit_damage_rating();
    }

    crit_damage_rating
        .wrapping_add(equipment)
        .wrapping_add(enchantments)
        .wrapping_add(aug_bonus)
        .wrapping_add(lum_aug_bonus)
}

// ACE: Creature.GetCritResistRating
pub fn get_crit_resist_rating(w: &mut World, this: ObjectGuid) -> i32 {
    // crit resist chance

    // get from base properties (monsters)?
    let crit_resist_rating = object(w, this).crit_resist_rating().unwrap_or(0);

    // additive enchantments
    let enchantments = emc::get_rating(w, this, PropertyInt::CritResistRating);

    // equipment ratings
    let equipment = equipment(w, this, PropertyInt::GearCritResist);

    // no augs / lum augs?
    crit_resist_rating
        .wrapping_add(enchantments)
        .wrapping_add(equipment)
}

// ACE: Creature.GetCritDamageResistRating
pub fn get_crit_damage_resist_rating(w: &mut World, this: ObjectGuid) -> i32 {
    // get from base properties (monsters)?
    let crit_damage_resist_rating = object(w, this).crit_damage_resist_rating().unwrap_or(0);

    // additive enchantments
    let enchantments = emc::get_rating(w, this, PropertyInt::CritDamageResistRating);

    // equipment ratings
    let equipment = equipment(w, this, PropertyInt::GearCritDamageResist);

    let mut lum_aug_bonus = 0i32;
    if is_player(w, this) {
        lum_aug_bonus = object(w, this).lum_aug_crit_reduction_rating();
    }

    crit_damage_resist_rating
        .wrapping_add(equipment)
        .wrapping_add(enchantments)
        .wrapping_add(lum_aug_bonus)
}

// ACE: Creature.GetHealingBoostRating
pub fn get_healing_boost_rating(w: &mut World, this: ObjectGuid) -> i32 {
    // get from base properties (monsters)?
    let heal_boost_rating = object(w, this).healing_boost_rating().unwrap_or(0);

    // additive enchantments
    let enchantments = emc::get_rating(w, this, PropertyInt::HealingBoostRating);

    // equipment ratings
    let equipment = equipment(w, this, PropertyInt::GearHealingBoost);

    let mut lum_aug_bonus = 0i32;
    if is_player(w, this) {
        lum_aug_bonus = object(w, this).lum_aug_healing_rating();
    }

    heal_boost_rating
        .wrapping_add(equipment)
        .wrapping_add(enchantments)
        .wrapping_add(lum_aug_bonus)
}

// ACE: Creature.GetHealingResistRating
pub fn get_healing_resist_rating(w: &mut World, this: ObjectGuid) -> i32 {
    // debuff?
    let heal_resist_rating = object(w, this).healing_resist_rating().unwrap_or(0);

    // additive enchantments
    let enchantments = emc::get_rating(w, this, PropertyInt::HealingResistRating);

    heal_resist_rating.wrapping_add(enchantments)
}

// ACE: Creature.GetHealingRatingMod
/// The healing boost modifier times the healing resist modifier.
pub fn get_healing_rating_mod(w: &mut World, this: ObjectGuid) -> f32 {
    let boost_mod = get_positive_rating_mod(get_healing_boost_rating(w, this));
    let resist_mod = get_negative_rating_mod(get_healing_resist_rating(w, this), false);

    boost_mod * resist_mod
}

// ACE: Creature.GetLifeResistRating
pub fn get_life_resist_rating(w: &mut World, this: ObjectGuid) -> i32 {
    // only affects health drain?
    // only cast by Sigil of Perserverance (Aetheria)?

    // get from base properties (monsters)?
    let life_resist_rating = object(w, this).life_resist_rating().unwrap_or(0);

    // additive enchantments
    let enchantments = emc::get_rating(w, this, PropertyInt::LifeResistRating);

    life_resist_rating.wrapping_add(enchantments)
}

// ACE: Creature.GetLifeResistRatingMod
pub fn get_life_resist_rating_mod(w: &mut World, this: ObjectGuid) -> f32 {
    get_negative_rating_mod(get_life_resist_rating(w, this), false)
}

// ACE: Creature.GetDotResistanceRating
pub fn get_dot_resistance_rating(w: &mut World, this: ObjectGuid) -> i32 {
    // get from base properties (monsters)?
    let dot_resist_rating = object(w, this).dot_resist_rating().unwrap_or(0);

    // additive enchantments
    let enchantments = emc::get_rating(w, this, PropertyInt::DotResistRating);

    dot_resist_rating.wrapping_add(enchantments)
}

// ACE: Creature.GetNetherResistRating
pub fn get_nether_resist_rating(w: &mut World, this: ObjectGuid) -> i32 {
    // there is a property defined for this,
    // but does anything use this?

    // get from base properties (monsters)?
    let nether_resist_rating = object(w, this).nether_resist_rating().unwrap_or(0);

    // additive enchantments
    let enchantments = emc::get_rating(w, this, PropertyInt::NetherResistRating);

    nether_resist_rating.wrapping_add(enchantments)
}

// ACE: Creature.GetGearMaxHealth
#[must_use]
pub fn get_gear_max_health(w: &World, this: ObjectGuid) -> i32 {
    equipment(w, this, PropertyInt::GearMaxHealth)
}

// ACE: Creature.GetPKDamageRating
pub fn get_pk_damage_rating(w: &mut World, this: ObjectGuid) -> i32 {
    let pk_damage_rating = object(w, this).pk_damage_rating().unwrap_or(0);

    // additive enchantments?
    let enchantments = emc::get_rating(w, this, PropertyInt::PKDamageRating);

    // equipment ratings
    let equipment = equipment(w, this, PropertyInt::GearPKDamageRating);

    pk_damage_rating
        .wrapping_add(equipment)
        .wrapping_add(enchantments)
}

// ACE: Creature.GetPKDamageResistRating
pub fn get_pk_damage_resist_rating(w: &mut World, this: ObjectGuid) -> i32 {
    let pk_damage_resist_rating = object(w, this).pk_damage_resist_rating().unwrap_or(0);

    // additive enchantments?
    let enchantments = emc::get_rating(w, this, PropertyInt::PKDamageResistRating);

    // equipment ratings
    let equipment = equipment(w, this, PropertyInt::GearPKDamageResistRating);

    pk_damage_resist_rating
        .wrapping_add(equipment)
        .wrapping_add(enchantments)
}

// ACE: Creature.GetGearPKDamageRating
#[must_use]
pub fn get_gear_pk_damage_rating(w: &World, this: ObjectGuid) -> i32 {
    equipment(w, this, PropertyInt::GearPKDamageRating)
}

// ACE: Creature.GetGearPKDamageResistRating
#[must_use]
pub fn get_gear_pk_damage_resist_rating(w: &World, this: ObjectGuid) -> i32 {
    equipment(w, this, PropertyInt::GearPKDamageResistRating)
}

// ACE: Creature.GetItemManaReductionRating
/// Only from a player's luminance augmentation.
#[must_use]
pub fn get_item_mana_reduction_rating(w: &World, this: ObjectGuid) -> i32 {
    // only comes from luminance aug?
    let mut lum_aug_bonus = 0i32;

    if is_player(w, this) {
        lum_aug_bonus = object(w, this).lum_aug_item_mana_usage();
    }

    lum_aug_bonus
}

// ACE: Creature.GetManaChargeRating
/// Only from a player's luminance augmentation.
#[must_use]
pub fn get_mana_charge_rating(w: &World, this: ObjectGuid) -> i32 {
    // only comes from luminance aug?
    let mut lum_aug_bonus = 0i32;

    if is_player(w, this) {
        lum_aug_bonus = object(w, this).lum_aug_item_mana_gain();
    }

    lum_aug_bonus
}
