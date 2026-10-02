// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Structure/WeaponProfile.cs
//! Port of `Source/ACE.Server/Network/Structure/WeaponProfile.cs`.
//!
//! C# arithmetic is kept in its own types: the enchantment modifiers are `float` sums (the
//! enchantment manager returns `float`), stored into `double` fields, and the `Get*` helpers
//! return `float` values that are widened to `double` when stored.

#![allow(clippy::cast_possible_truncation)] // `(float)` of a double, as C# writes it

use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::{DamageType, PropertyFloat, PropertyInt, Skill, WeenieType};
use empyrean_entity::ObjectGuid;

use crate::network::game_messages::game_message::write_record;
use crate::world_objects::world_object_networking::shims;
use crate::World;

// ACE: WeaponProfile
/// Handles the info for the weapon appraisal panel.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct WeaponProfile {
    // ACE: WeaponProfile.Weapon
    pub weapon: ObjectGuid,

    // ACE: WeaponProfile.DamageType
    pub damage_type: DamageType,
    // ACE: WeaponProfile.WeaponTime
    /// the weapon speed
    pub weapon_time: u32,
    // ACE: WeaponProfile.WeaponSkill
    pub weapon_skill: Skill,
    // ACE: WeaponProfile.Damage
    /// max damage
    pub damage: u32,
    // ACE: WeaponProfile.DamageVariance
    /// damage range %
    pub damage_variance: f64,
    // ACE: WeaponProfile.DamageMod
    /// the damage modifier of the weapon
    pub damage_mod: f64,
    // ACE: WeaponProfile.WeaponLength
    pub weapon_length: f64,
    // ACE: WeaponProfile.MaxVelocity
    /// the power of the weapon (this affects range)
    pub max_velocity: f64,
    // ACE: WeaponProfile.WeaponOffense
    /// the attack skill bonus of the weapon
    pub weapon_offense: f64,
    // ACE: WeaponProfile.WeaponDefense
    pub weapon_defense: f64,
    // ACE: WeaponProfile.MaxVelocityEstimated
    pub max_velocity_estimated: u32,

    // ACE: WeaponProfile.Enchantment_WeaponTime
    pub enchantment_weapon_time: i32,
    // ACE: WeaponProfile.Enchantment_Damage
    pub enchantment_damage: i32,
    // ACE: WeaponProfile.Enchantment_DamageVariance
    pub enchantment_damage_variance: f64,
    // ACE: WeaponProfile.Enchantment_DamageMod
    pub enchantment_damage_mod: f64,
    // ACE: WeaponProfile.Enchantment_WeaponOffense
    pub enchantment_weapon_offense: f64,

    // ACE: WeaponProfile.Enchantment_WeaponDefense
    /// gets sent elsewhere, calculating here for consistency
    pub enchantment_weapon_defense: f64,
}

// ACE: WeaponProfile.WeaponProfile
pub fn weapon_profile_new(w: &World, weapon: ObjectGuid) -> WeaponProfile {
    let mut p = WeaponProfile {
        weapon,
        ..Default::default()
    };

    p.weapon_defense = f64::from(p.get_weapon_defense(w, weapon));

    let wo = w.objects.get(weapon).expect("ACE: weapon is null");
    if wo.is_caster() {
        return p;
    }

    p.damage_type = DamageType(wo.get_property(PropertyInt::DamageType).unwrap_or(0));
    //if (DamageType == 0)
    //Console.WriteLine($"Warning: WeaponProfile undefined damage type for {weapon.Name} ({weapon.Guid})");

    p.weapon_time = p.get_weapon_speed(w, weapon);
    p.weapon_skill = Skill(wo.get_property(PropertyInt::WeaponSkill).unwrap_or(0));
    p.damage = p.get_damage(w, weapon);
    p.damage_variance = f64::from(p.get_damage_variance(w, weapon));
    p.damage_mod = f64::from(p.get_damage_multiplier(w, weapon));
    p.weapon_length = wo.get_property(PropertyFloat::WeaponLength).unwrap_or(1.0);
    p.max_velocity = wo.maximum_velocity().unwrap_or(1.0);
    p.weapon_offense = f64::from(p.get_weapon_offense(w, weapon));
    //MaxVelocityEstimated = (uint)Math.Round(MaxVelocity);   // not found in pcaps?
    p
}

impl WeaponProfile {
    // ACE: WeaponProfile.GetDamage
    /// The weapon max damage, with enchantments factored in.
    pub fn get_damage(&mut self, w: &World, weapon: ObjectGuid) -> u32 {
        let wo = w.objects.get(weapon).expect("ACE: weapon is null");
        let base_damage = wo.get_property(PropertyInt::Damage).unwrap_or(0);
        let damage_bonus = shims::enchantment_manager_get_damage_bonus(w, weapon);
        let aura_damage_bonus = match wo.wielder {
            Some(wielder)
                if wo.biota.weenie_type != WeenieType::Ammunition
                    || shims::property_manager_get_bool(w, "show_ammo_buff", false) =>
            {
                shims::enchantment_manager_get_damage_bonus(w, wielder)
            }
            _ => 0,
        };
        self.enchantment_damage = if shims::is_enchantable(w, weapon) {
            damage_bonus.wrapping_add(aura_damage_bonus)
        } else {
            damage_bonus
        };
        base_damage
            .wrapping_add(self.enchantment_damage)
            .max(0)
            .cast_unsigned()
    }

    // ACE: WeaponProfile.GetWeaponSpeed
    /// The weapon speed, with enchantments factored in.
    pub fn get_weapon_speed(&mut self, w: &World, weapon: ObjectGuid) -> u32 {
        let wo = w.objects.get(weapon).expect("ACE: weapon is null");
        let base_speed = wo.get_property(PropertyInt::WeaponTime).unwrap_or(0); // safe to assume defaults here?
        let speed_mod = shims::enchantment_manager_get_weapon_speed_mod(w, weapon);
        let aura_speed_mod = wo.wielder.map_or(0, |wielder| {
            shims::enchantment_manager_get_weapon_speed_mod(w, wielder)
        });
        // DIVERGE: an era whose multiplicative speed enchantments count
        // (`EraFormulas::multiplicative_weapon_speed`) adds their change to the base speed,
        // rounded half to even (ClassicACE's `GetWeaponSpeed` at its older rulesets).
        // Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/Network/Structure/WeaponProfile.cs
        let mult_speed_bonus: i32 = if w.era.formulas.multiplicative_weapon_speed {
            let mult_speed_mod =
                crate::world_objects::managers::enchantment_manager::get_weapon_multiplicative_speed_mod(
                    w, weapon,
                );
            let base = base_speed as f32;
            let bonus: i32 =
                empyrean_common::dotnet::math::round(f64::from(base - base * mult_speed_mod))
                    .cs_cast();
            bonus.wrapping_neg()
        } else {
            0
        };
        self.enchantment_weapon_time = if shims::is_enchantable(w, weapon) {
            speed_mod
                .wrapping_add(aura_speed_mod)
                .wrapping_add(mult_speed_bonus)
        } else {
            speed_mod.wrapping_add(mult_speed_bonus)
        };
        base_speed
            .wrapping_add(self.enchantment_weapon_time)
            .max(0)
            .cast_unsigned()
    }

    // ACE: WeaponProfile.GetDamageVariance
    /// The weapon damage variance, with enchantments factored in.
    pub fn get_damage_variance(&mut self, w: &World, weapon: ObjectGuid) -> f32 {
        let wo = w.objects.get(weapon).expect("ACE: weapon is null");
        // are there any spells which modify damage variance?
        let base_variance = wo
            .get_property(PropertyFloat::DamageVariance)
            .unwrap_or(0.0); // safe to assume defaults here?
        let variance_mod = shims::enchantment_manager_get_variance_mod(w, weapon);
        let aura_variance_mod = wo.wielder.map_or(1.0, |wielder| {
            shims::enchantment_manager_get_variance_mod(w, wielder)
        });
        self.enchantment_damage_variance = f64::from(if shims::is_enchantable(w, weapon) {
            variance_mod * aura_variance_mod
        } else {
            variance_mod
        });
        (base_variance * self.enchantment_damage_variance) as f32
    }

    // ACE: WeaponProfile.GetDamageMultiplier
    /// The weapon damage multiplier, with enchantments factored in.
    pub fn get_damage_multiplier(&mut self, w: &World, weapon: ObjectGuid) -> f32 {
        let wo = w.objects.get(weapon).expect("ACE: weapon is null");
        let base_multiplier = wo.get_property(PropertyFloat::DamageMod).unwrap_or(1.0);
        let damage_mod = shims::enchantment_manager_get_damage_mod(w, weapon);
        let aura_damage_mod = wo.wielder.map_or(0.0, |wielder| {
            shims::enchantment_manager_get_damage_mod(w, wielder)
        });
        self.enchantment_damage_mod = f64::from(if shims::is_enchantable(w, weapon) {
            damage_mod + aura_damage_mod
        } else {
            damage_mod
        });
        (base_multiplier + self.enchantment_damage_mod) as f32
    }

    // ACE: WeaponProfile.GetWeaponOffense
    /// The attack bonus %, with enchantments factored in.
    pub fn get_weapon_offense(&mut self, w: &World, weapon: ObjectGuid) -> f32 {
        let wo = w.objects.get(weapon).expect("ACE: weapon is null");
        if wo.is_ammunition() {
            return 1.0;
        }

        let base_offense = wo.get_property(PropertyFloat::WeaponOffense).unwrap_or(1.0);
        let offense_mod = if wo.is_ranged() {
            0.0
        } else {
            shims::enchantment_manager_get_attack_mod(w, weapon)
        };
        let aura_offense_mod = match wo.wielder {
            Some(wielder) if !wo.is_ranged() => {
                shims::enchantment_manager_get_attack_mod(w, wielder)
            }
            _ => 0.0,
        };
        self.enchantment_weapon_offense = f64::from(if shims::is_enchantable(w, weapon) {
            offense_mod + aura_offense_mod
        } else {
            offense_mod
        });
        (base_offense + self.enchantment_weapon_offense) as f32
    }

    // ACE: WeaponProfile.GetWeaponDefense
    /// The defense bonus %, with enchantments factored in.
    pub fn get_weapon_defense(&mut self, w: &World, weapon: ObjectGuid) -> f32 {
        let wo = w.objects.get(weapon).expect("ACE: weapon is null");
        if wo.is_ammunition() {
            return 1.0;
        }

        let mut base_defense = wo.get_property(PropertyFloat::WeaponDefense).unwrap_or(1.0);

        // TODO: Resolve this issue a better way?
        // Because of the way ACE handles default base values in recipe system (or rather the lack thereof)
        // we need to check the following weapon properties to see if they're below expected minimum and adjust accordingly
        // The issue is that the recipe system likely added 0.01 to 0 instead of 1, which is what *should* have happened.
        if let Some(weapon_defense) = wo.weapon_defense() {
            if weapon_defense > 0.0
                && weapon_defense < 1.0
                && (wo.get_property(PropertyInt::ImbueStackingBits).unwrap_or(0) & 4) != 0
            {
                base_defense += 1.0;
            }
        }

        let defense_mod = shims::enchantment_manager_get_defense_mod(w, weapon);
        let aura_defense_mod = wo.wielder.map_or(0.0, |wielder| {
            shims::enchantment_manager_get_defense_mod(w, wielder)
        });
        self.enchantment_weapon_defense = f64::from(if shims::is_enchantable(w, weapon) {
            defense_mod + aura_defense_mod
        } else {
            defense_mod
        });
        (base_defense + self.enchantment_weapon_defense) as f32
    }
}

// ACE: WeaponProfileExtensions.Write
/// Writes the weapon appraisal info to the network stream.
pub fn write(writer: &mut Vec<u8>, profile: &WeaponProfile) {
    write_record(writer, &[], |w| record(profile).write(w));
}

/// The dereth-protocol record the `Write` extension below writes, field for field.
#[must_use]
pub fn record(profile: &WeaponProfile) -> dereth_protocol::types::WeaponProfile {
    dereth_protocol::types::WeaponProfile {
        damage_type: profile.damage_type.0.cast_unsigned(),
        weapon_time: profile.weapon_time.cast_signed(),
        weapon_skill: profile.weapon_skill.0.cast_unsigned(),
        weapon_damage: profile.damage.cast_signed(),
        damage_variance: profile.damage_variance,
        damage_mod: profile.damage_mod,
        weapon_length: profile.weapon_length,
        max_velocity: profile.max_velocity,
        weapon_offense: profile.weapon_offense,
        max_velocity_estimated: profile.max_velocity_estimated.cast_signed(),
    }
}
