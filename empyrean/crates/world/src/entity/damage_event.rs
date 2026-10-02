// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/DamageEvent.cs
//! Port of `Source/ACE.Server/Entity/DamageEvent.cs`.
//!
//! The physical damage pipeline for one melee or missile hit. Factors, in ACE's order: lifestone
//! protection; evade (offense mod, accuracy mod, defense mod, stamina); base damage; damage rating
//! (recklessness, sneak attack, heritage); power meter; critical (chance, damage, ratings); attribute
//! mod; armor (base AL, impen / bane, life armor / imperil); elemental bonus; slayer; resistance
//! (natural, prot, vuln, resistance cleaving); damage resistance rating; shield; rending.
//!
//! Every `ThreadSafeRandom` draw is made where ACE makes it: overpower (0-2), evade, base damage
//! (a monster's attack part first, and a multi-type body part's damage type after), sneak attack,
//! critical, critical defense, and the body part.
//!
//! The virtual members ACE calls on the attacker and defender go through the `creature_combat::shim`
//! stand-ins, which pick the Player override while `player_combat.rs` is still `not_ported!`.

#![allow(clippy::cast_possible_truncation)] // C#'s `(float)` of a double, as ACE writes it

use std::collections::HashSet;
use std::sync::LazyLock;

use dereth_animation::hooks::AttackCone;
use empyrean_common::dotnet::{self, math};
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_entity::enums::{
    AttackConditions, AttackHeight, AttackType, ChatMessageType, CombatBodyPart, DamageType,
    ImbuedEffectType, ItemType, MotionCommand, Quadrant, WeenieType,
};
use empyrean_entity::models::PropertiesBodyPart;
use empyrean_entity::ObjectGuid;

use crate::entity::base_damage_mod::BaseDamageMod;
use crate::entity::body_part::{self, BodyPart};
use crate::world_objects::creature_body_part::CreatureBodyPart;
use crate::world_objects::creature_combat::shim;
use crate::world_objects::creature_combat::{self, CombatType, DebugDamageType};
use crate::world_objects::creature_equipment;
use crate::world_objects::creature_rating;
use crate::world_objects::skill_check;
use crate::world_objects::world_object::{self, WorldObject};
use crate::world_objects::world_object_weapon::{self as weapon_mod, SkillOf};
use crate::world_objects::{monster_combat, monster_melee};
use crate::World;

/// One physical attack's damage calculation and every factor that went into it.
// ACE: DamageEvent
#[derive(Debug, Clone, Default)]
pub struct DamageEvent {
    // ACE: DamageEvent.Attacker
    pub attacker: ObjectGuid,
    // ACE: DamageEvent.Defender
    pub defender: ObjectGuid,

    /// melee / missile / magic
    // ACE: DamageEvent.CombatType
    pub combat_type: CombatType,

    // ACE: DamageEvent.DamageSource
    pub damage_source: ObjectGuid,
    // ACE: DamageEvent.DamageType
    pub damage_type: DamageType,

    /// The attacker's weapon. This can be different from DamageSource: for a missile attack the
    /// missile is the DamageSource, and the buffs come from the Weapon.
    // ACE: DamageEvent.Weapon
    pub weapon: Option<ObjectGuid>,

    /// slash / thrust / punch / kick / offhand / multistrike
    // ACE: DamageEvent.AttackType
    pub attack_type: AttackType,
    // ACE: DamageEvent.AttackHeight
    pub attack_height: AttackHeight,

    // ACE: DamageEvent.LifestoneProtection
    pub lifestone_protection: bool,

    // ACE: DamageEvent.EvasionChance
    pub evasion_chance: f32,
    // ACE: DamageEvent.EffectiveAttackSkill
    pub effective_attack_skill: u32,
    // ACE: DamageEvent.EffectiveDefenseSkill
    pub effective_defense_skill: u32,
    // ACE: DamageEvent.AccuracyMod
    pub accuracy_mod: f32,

    // ACE: DamageEvent.Evaded
    pub evaded: bool,

    // ACE: DamageEvent.BaseDamageMod
    pub base_damage_mod: Option<BaseDamageMod>,
    // ACE: DamageEvent.BaseDamage
    pub base_damage: f32,

    // ACE: DamageEvent.AttributeMod
    pub attribute_mod: f32,
    // ACE: DamageEvent.PowerMod
    pub power_mod: f32,
    // ACE: DamageEvent.SlayerMod
    pub slayer_mod: f32,

    // ACE: DamageEvent.DamageRatingBaseMod
    pub damage_rating_base_mod: f32,
    // ACE: DamageEvent.RecklessnessMod
    pub recklessness_mod: f32,
    // ACE: DamageEvent.SneakAttackMod
    pub sneak_attack_mod: f32,
    // ACE: DamageEvent.HeritageMod
    pub heritage_mod: f32,
    // ACE: DamageEvent.PkDamageMod
    pub pk_damage_mod: f32,

    // ACE: DamageEvent.DamageRatingMod
    pub damage_rating_mod: f32,

    // ACE: DamageEvent.IsCritical
    pub is_critical: bool,

    // ACE: DamageEvent.CriticalChance
    pub critical_chance: f32,
    // ACE: DamageEvent.CriticalDamageMod
    pub critical_damage_mod: f32,

    // ACE: DamageEvent.CriticalDamageRatingMod
    pub critical_damage_rating_mod: f32,
    // ACE: DamageEvent.CriticalDamageResistanceRatingMod
    pub critical_damage_resistance_rating_mod: f32,

    // ACE: DamageEvent.DamageBeforeMitigation
    pub damage_before_mitigation: f32,

    // ACE: DamageEvent.ArmorMod
    pub armor_mod: f32,
    // ACE: DamageEvent.ResistanceMod
    pub resistance_mod: f32,
    // ACE: DamageEvent.ShieldMod
    pub shield_mod: f32,
    // ACE: DamageEvent.WeaponResistanceMod
    pub weapon_resistance_mod: f32,

    // ACE: DamageEvent.DamageResistanceRatingBaseMod
    pub damage_resistance_rating_base_mod: f32,
    // ACE: DamageEvent.DamageResistanceRatingMod
    pub damage_resistance_rating_mod: f32,
    // ACE: DamageEvent.PkDamageResistanceMod
    pub pk_damage_resistance_mod: f32,

    // ACE: DamageEvent.DamageMitigated
    pub damage_mitigated: f32,

    // creature attacker
    // ACE: DamageEvent.AttackMotion
    pub attack_motion: Option<MotionCommand>,
    // ACE: DamageEvent.AttackHook
    pub attack_hook: Option<AttackCone>,
    /// The body part this monster is attacking with (ACE's default `KeyValuePair` is `None`).
    // ACE: DamageEvent.AttackPart
    pub attack_part: Option<(CombatBodyPart, PropertiesBodyPart)>,

    // creature defender
    // ACE: DamageEvent.Quadrant
    pub quadrant: Quadrant,

    // ACE: DamageEvent.Overpower
    pub overpower: bool,

    // player defender
    // ACE: DamageEvent.BodyPart
    pub body_part: BodyPart,
    // ACE: DamageEvent.Armor
    pub armor: Option<Vec<ObjectGuid>>,

    // creature defender
    /// The defender's body part (its biota's record, `None` when the biota lacks it).
    // ACE: DamageEvent.PropertiesBodyPart
    pub properties_body_part: Option<(CombatBodyPart, Option<PropertiesBodyPart>)>,
    // ACE: DamageEvent.CreaturePart
    pub creature_part: Option<CreatureBodyPart>,

    // ACE: DamageEvent.Damage
    pub damage: f32,

    // ACE: DamageEvent.GeneralFailure
    pub general_failure: bool,

    // ACE: DamageEvent.CriticalDefended
    pub critical_defended: bool,
}

// ACE: DamageEvent.AllowDamageTypeUndef
pub static ALLOW_DAMAGE_TYPE_UNDEF: LazyLock<HashSet<u32>> = LazyLock::new(|| {
    HashSet::from([
        22545, // Obsidian Spines
        35191, // Thunder Chicken
        38406, // Blessed Moar
        38587, // Ardent Moar
        38588, // Blessed Moar
        38586, // Verdant Moar
        40298, // Ardent Moar
        40300, // Blessed Moar
        40301, // Verdant Moar
    ])
});

// ACE: DamageEvent.LeftRight
pub const LEFT_RIGHT: Quadrant = Quadrant(Quadrant::Left.0 | Quadrant::Right.0);
// ACE: DamageEvent.FrontBack
pub const FRONT_BACK: Quadrant = Quadrant(Quadrant::Front.0 | Quadrant::Back.0);

fn object(w: &World, g: ObjectGuid) -> &WorldObject {
    w.objects.get(g).unwrap_or_else(|| {
        panic!(
            "System.NullReferenceException: object 0x{:08X} is not in World.objects",
            g.full()
        )
    })
}

impl DamageEvent {
    /// Ignores impen / banes.
    // ACE: DamageEvent.IgnoreMagicArmor
    #[must_use]
    pub fn ignore_magic_armor(&self, w: &World) -> bool {
        self.weapon
            .is_some_and(|g| object(w, g).ignore_magic_armor())
            || w.objects
                .get(self.attacker)
                .is_some_and(WorldObject::ignore_magic_armor)
    }

    /// Ignores life armor / prots.
    // ACE: DamageEvent.IgnoreMagicResist
    #[must_use]
    pub fn ignore_magic_resist(&self, w: &World) -> bool {
        self.weapon
            .is_some_and(|g| object(w, g).ignore_magic_resist())
            || w.objects
                .get(self.attacker)
                .is_some_and(WorldObject::ignore_magic_resist)
    }

    // ACE: DamageEvent.HasDamage
    #[must_use]
    pub fn has_damage(&self) -> bool {
        !self.evaded && !self.lifestone_protection
    }

    /// `CalculateDamage(Creature attacker, Creature defender, WorldObject damageSource,
    /// MotionCommand? attackMotion = null, AttackHook attackHook = null)`; a null damage source is
    /// the attacker.
    // ACE: DamageEvent.CalculateDamage
    pub fn calculate_damage(
        w: &mut World,
        attacker: ObjectGuid,
        defender: ObjectGuid,
        damage_source: Option<ObjectGuid>,
        attack_motion: Option<MotionCommand>,
        attack_hook: Option<AttackCone>,
    ) -> DamageEvent {
        let mut damage_event = DamageEvent {
            attack_motion,
            attack_hook,
            ..DamageEvent::default()
        };
        let damage_source = damage_source.unwrap_or(attacker);

        let _damage = damage_event.do_calculate_damage(w, attacker, defender, damage_source);

        damage_event.handle_logging(w, Some(attacker), Some(defender));

        damage_event
    }

    // ACE: DamageEvent.DoCalculateDamage
    #[allow(clippy::too_many_lines)]
    fn do_calculate_damage(
        &mut self,
        w: &mut World,
        attacker: ObjectGuid,
        defender: ObjectGuid,
        damage_source: ObjectGuid,
    ) -> f32 {
        let player_attacker = object(w, attacker).is_player();
        let player_defender = object(w, defender).is_player();

        let pk_battle = player_attacker && player_defender;

        self.attacker = attacker;
        self.defender = defender;

        let projectile = object(w, damage_source)
            .projectile
            .as_ref()
            .map(|p| (p.source, p.launcher, p.ammo));
        let projectile_source = projectile.and_then(|p| p.0);

        self.combat_type = if projectile_source.is_none() {
            CombatType::Melee
        } else {
            CombatType::Missile
        };

        self.damage_source = damage_source;

        self.weapon = if projectile_source.is_none() {
            creature_equipment::get_equipped_melee_weapon(w, attacker, false)
        } else {
            let p = projectile.expect("checked");
            p.1.or(p.2)
        };

        self.attack_type = creature_combat::attack_type(w, attacker);
        self.attack_height =
            creature_combat::attack_height(w, attacker).unwrap_or(AttackHeight::Medium);

        // check lifestone protection
        if player_defender && object(w, defender).under_lifestone_protection() {
            self.lifestone_protection = true;
            crate::world_objects::player_death::handle_lifestone_protection(w, defender);
            return 0.0;
        }

        if object(w, defender).invincible() {
            return 0.0;
        }

        // overpower
        if object(w, attacker).overpower().is_some() {
            self.overpower = creature_combat::get_overpower(w, attacker, defender);
        }

        // evasion chance
        if !self.overpower {
            self.evasion_chance = self.get_evade_chance(w, attacker, defender);
            if f64::from(self.evasion_chance) > ThreadSafeRandom::next_float(0.0, 1.0) {
                self.evaded = true;
                return 0.0;
            }
        }

        // get base damage
        if player_attacker {
            self.get_base_damage_player(w, attacker);
        } else {
            self.get_base_damage_creature(
                w,
                attacker,
                self.attack_motion.unwrap_or(MotionCommand::Invalid),
                self.attack_hook,
            );
        }

        if self.damage_type == DamageType::Undef
            && (attacker.is_player() || damage_source.is_player())
        {
            log::error!(
                "DamageEvent.DoCalculateDamage({} ({}), {} ({}), {} ({})) - DamageType == DamageType.Undef",
                shim::name(w, attacker),
                attacker,
                shim::name(w, defender),
                defender,
                shim::name(w, damage_source),
                damage_source
            );
            self.general_failure = true;
        }

        if self.general_failure {
            return 0.0;
        }

        // get damage modifiers
        self.power_mod = shim::virt_get_power_mod(w, attacker, self.weapon);
        self.attribute_mod = creature_combat::get_attribute_mod(w, attacker, self.weapon);
        self.slayer_mod = weapon_mod::get_weapon_creature_slayer_modifier(
            w,
            self.weapon,
            Some(attacker),
            Some(defender),
        );

        // ratings
        self.damage_rating_base_mod = creature_rating::get_positive_rating_mod(
            creature_rating::get_damage_rating(w, attacker),
        );
        self.recklessness_mod = creature_combat::get_recklessness_mod(w, attacker, defender);
        self.sneak_attack_mod = creature_combat::get_sneak_attack_mod(w, attacker, defender);
        self.heritage_mod = if shim::virt_get_heritage_bonus(w, attacker, self.weapon) {
            1.05
        } else {
            1.0
        };

        self.damage_rating_mod = creature_rating::additive_combine(&[
            self.damage_rating_base_mod,
            self.recklessness_mod,
            self.sneak_attack_mod,
            self.heritage_mod,
        ]);

        if pk_battle {
            self.pk_damage_mod = creature_rating::get_positive_rating_mod(
                creature_rating::get_pk_damage_rating(w, attacker),
            );
            self.damage_rating_mod =
                creature_rating::additive_combine(&[self.damage_rating_mod, self.pk_damage_mod]);
        }

        // damage before mitigation
        self.damage_before_mitigation = self.base_damage
            * self.attribute_mod
            * self.power_mod
            * self.slayer_mod
            * self.damage_rating_mod;

        // critical hit?
        let weapon_skill =
            crate::dispatch::get_current_weapon_skill::get_current_weapon_skill(w, attacker);
        let attack_skill = SkillOf::get(w, attacker, weapon_skill);
        self.critical_chance = weapon_mod::get_weapon_critical_chance(
            w,
            self.weapon,
            Some(attacker),
            Some(attack_skill),
            defender,
        );

        // https://asheron.fandom.com/wiki/Announcements_-_2002/08_-_Atonement
        // It should be noted that any time a character is logging off, PK or not, all physical attacks against them become automatically critical.
        // (Note that spells do not share this behavior.) We hope this will stress the need to log off in a safe place.

        let player_fields = |w: &World| {
            w.objects
                .get(defender)
                .and_then(|o| o.player.as_ref())
                .map(|p| (p.player.is_logging_out, p.player.pk_logout))
        };
        if player_defender
            && player_fields(w).is_some_and(|(logging_out, pk_logout)| logging_out || pk_logout)
        {
            self.critical_chance = 1.0;
        }

        if f64::from(self.critical_chance) > ThreadSafeRandom::next_float(0.0, 1.0) {
            if player_defender && object(w, defender).augmentation_critical_defense() > 0 {
                let critical_defense_mod = if player_attacker { 0.05f32 } else { 0.25f32 };
                let critical_defense_chance = object(w, defender).augmentation_critical_defense()
                    as f32
                    * critical_defense_mod;

                if f64::from(critical_defense_chance) > ThreadSafeRandom::next_float(0.0, 1.0) {
                    self.critical_defended = true;
                }
            }

            if !self.critical_defended {
                self.is_critical = true;

                // verify: CriticalMultiplier only applied to the additional crit damage,
                // whereas CD/CDR applied to the total damage (base damage + additional crit damage)
                self.critical_damage_mod = 1.0
                    + weapon_mod::get_weapon_crit_damage_mod(
                        w,
                        self.weapon,
                        Some(attacker),
                        Some(attack_skill),
                        defender,
                    );

                self.critical_damage_rating_mod = creature_rating::get_positive_rating_mod(
                    creature_rating::get_crit_damage_rating(w, attacker),
                );

                // recklessness excluded from crits
                self.recklessness_mod = 1.0;
                self.damage_rating_mod = creature_rating::additive_combine(&[
                    self.damage_rating_base_mod,
                    self.critical_damage_rating_mod,
                    self.sneak_attack_mod,
                    self.heritage_mod,
                ]);

                if pk_battle {
                    self.damage_rating_mod = creature_rating::additive_combine(&[
                        self.damage_rating_mod,
                        self.pk_damage_mod,
                    ]);
                }

                let max_damage = self
                    .base_damage_mod
                    .as_ref()
                    .expect("System.NullReferenceException: BaseDamageMod")
                    .max_damage();
                self.damage_before_mitigation = max_damage
                    * self.attribute_mod
                    * self.power_mod
                    * self.slayer_mod
                    * self.damage_rating_mod
                    * self.critical_damage_mod;
            }
        }

        // armor rending and cleaving
        let mut armor_rending_mod = 1.0f32;
        if self.weapon.is_some_and(|g| {
            weapon_mod::has_imbued_effect(object(w, g), ImbuedEffectType::ArmorRending)
        }) {
            armor_rending_mod = weapon_mod::get_armor_rending_mod(w, Some(attack_skill));
        }

        let armor_cleaving_mod = weapon_mod::get_armor_cleaving_mod(w, attacker, self.weapon);

        let ignore_armor_mod = math::min_f32(armor_rending_mod, armor_cleaving_mod);

        // get body part / armor pieces / armor modifier
        if player_defender {
            // select random body part @ current attack height
            self.get_body_part_player(self.attack_height);

            // get player armor pieces
            let armor = monster_melee::get_armor_layers(w, defender, self.body_part);

            // get armor modifiers
            self.armor_mod = monster_melee::get_armor_mod_layers(
                w,
                attacker,
                defender,
                self.damage_type,
                &armor,
                self.weapon,
                ignore_armor_mod,
            );
            self.armor = Some(armor);
        } else {
            // determine height quadrant
            self.quadrant =
                self.get_quadrant(w, defender, attacker, self.attack_height, damage_source);

            // select random body part @ current attack height
            self.get_body_part_creature(w, defender, self.quadrant);
            if self.evaded {
                return 0.0;
            }

            let part = self.creature_part.clone().expect("set by GetBodyPart");
            let key = self
                .properties_body_part
                .as_ref()
                .expect("set by GetBodyPart")
                .0;
            let armor = part.get_armor_layers(w, key);

            // get target armor
            self.armor_mod = part.get_armor_mod(
                w,
                self.damage_type,
                &armor,
                Some(attacker),
                self.weapon,
                ignore_armor_mod,
            );
            self.armor = Some(armor);
        }

        if self.weapon.is_some_and(|g| {
            weapon_mod::has_imbued_effect(object(w, g), ImbuedEffectType::IgnoreAllArmor)
        }) {
            self.armor_mod = 1.0;
        }

        // get resistance modifiers
        self.weapon_resistance_mod = weapon_mod::get_weapon_resistance_modifier(
            w,
            self.weapon,
            Some(attacker),
            Some(attack_skill),
            self.damage_type,
        );

        if player_defender {
            self.resistance_mod =
                crate::world_objects::creature_properties::get_resistance_mod_damage(
                    w,
                    defender,
                    self.damage_type,
                    Some(attacker),
                    self.weapon,
                    self.weapon_resistance_mod,
                );
        } else {
            let resistance_type = creature_combat::get_resistance_type(self.damage_type);
            let r = crate::world_objects::creature_properties::get_resistance_mod(
                w,
                defender,
                resistance_type,
                Some(attacker),
                self.weapon,
                self.weapon_resistance_mod,
            );
            self.resistance_mod = math::max(f64::from(0.0f32), r) as f32;
        }

        // damage resistance rating
        self.damage_resistance_rating_base_mod = creature_rating::get_damage_resist_rating_mod(
            w,
            defender,
            Some(self.combat_type),
            true,
        );
        self.damage_resistance_rating_mod = self.damage_resistance_rating_base_mod;

        if self.is_critical {
            self.critical_damage_resistance_rating_mod = creature_rating::get_negative_rating_mod(
                creature_rating::get_crit_damage_resist_rating(w, defender),
                false,
            );

            self.damage_resistance_rating_mod = creature_rating::additive_combine(&[
                self.damage_resistance_rating_base_mod,
                self.critical_damage_resistance_rating_mod,
            ]);
        }

        if pk_battle {
            self.pk_damage_resistance_mod = creature_rating::get_negative_rating_mod(
                creature_rating::get_pk_damage_resist_rating(w, defender),
                false,
            );

            self.damage_resistance_rating_mod = creature_rating::additive_combine(&[
                self.damage_resistance_rating_mod,
                self.pk_damage_resistance_mod,
            ]);
        }

        // get shield modifier
        self.shield_mod =
            creature_combat::get_shield_mod(w, defender, attacker, self.damage_type, self.weapon);

        // calculate final output damage
        self.damage = self.damage_before_mitigation
            * self.armor_mod
            * self.shield_mod
            * self.resistance_mod
            * self.damage_resistance_rating_mod;

        self.damage_mitigated = self.damage_before_mitigation - self.damage;

        self.damage
    }

    /// The attack height's quadrant plus the direction of the defender from the damage source (or
    /// the attacker, when the source is not on a landblock).
    // ACE: DamageEvent.GetQuadrant
    #[must_use]
    pub fn get_quadrant(
        &self,
        w: &World,
        defender: ObjectGuid,
        attacker: ObjectGuid,
        attack_height: AttackHeight,
        damage_source: ObjectGuid,
    ) -> Quadrant {
        let mut quadrant = attack_height.to_quadrant();

        let wo = if object(w, damage_source).current_landblock.is_some() {
            damage_source
        } else {
            attacker
        };

        quadrant |= world_object::get_relative_dir(w, wo, defender);

        quadrant
    }

    /// Returns the chance for creature to avoid monster attack.
    // ACE: DamageEvent.GetEvadeChance
    pub fn get_evade_chance(
        &mut self,
        w: &mut World,
        attacker: ObjectGuid,
        defender: ObjectGuid,
    ) -> f32 {
        self.accuracy_mod = shim::virt_get_accuracy_mod(w, attacker, self.weapon);

        self.effective_attack_skill =
            crate::dispatch::get_effective_attack_skill::get_effective_attack_skill(w, attacker);

        //var attackType = attacker.GetCombatType();

        self.effective_defense_skill =
            creature_combat::get_effective_defense_skill(w, defender, self.combat_type);

        let evade_chance = 1.0
            - skill_check::get_skill_chance_uint(
                self.effective_attack_skill,
                self.effective_defense_skill,
                skill_check::DEFAULT_FACTOR,
            );
        evade_chance as f32
    }

    /// Returns the base damage for a player attacker.
    // ACE: DamageEvent.GetBaseDamage
    pub fn get_base_damage_player(&mut self, w: &mut World, attacker: ObjectGuid) {
        let source = object(w, self.damage_source);
        if source.item_type() == ItemType::MissileWeapon {
            self.damage_type = source.w_damage_type();

            // handle prismatic arrows
            if self.damage_type == DamageType::Base {
                self.damage_type = match self.weapon.map(|g| object(w, g).w_damage_type()) {
                    Some(t) if t != DamageType::Undef => t,
                    _ => DamageType::Pierce,
                };
            }
        } else {
            self.damage_type =
                shim::virt_get_damage_type(w, attacker, false, Some(CombatType::Melee));
        }

        // TODO: combat maneuvers for player?
        let mut base_damage_mod = crate::world_objects::player_combat::get_base_damage_mod(
            w,
            attacker,
            self.damage_source,
        );

        // DIVERGE: the Unarmed Combat skill's damage (`EraFormulas::older_melee_damage`).
        // Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/Entity/DamageEvent.cs
        base_damage_mod.base_damage.max_damage = base_damage_mod
            .base_damage
            .max_damage
            .wrapping_add(creature_combat::get_unarmed_skill_damage_bonus(w, attacker));

        // some quest bows can have built-in damage bonus
        if let Some(weapon) = self
            .weapon
            .filter(|&g| object(w, g).biota.weenie_type == WeenieType::MissileLauncher)
        {
            base_damage_mod.damage_bonus += object(w, weapon).damage().unwrap_or(0) as f32;
        }

        if object(w, self.damage_source).item_type() == ItemType::MissileWeapon {
            base_damage_mod.elemental_bonus = weapon_mod::get_missile_elemental_damage_bonus(
                w,
                self.weapon,
                Some(attacker),
                self.damage_type,
            );
        }

        self.base_damage = ThreadSafeRandom::next_float(
            base_damage_mod.min_damage(),
            base_damage_mod.max_damage(),
        ) as f32;
        self.base_damage_mod = Some(base_damage_mod);
    }

    /// Returns the base damage for a non-player attacker.
    // ACE: DamageEvent.GetBaseDamage
    pub fn get_base_damage_creature(
        &mut self,
        w: &mut World,
        attacker: ObjectGuid,
        motion_command: MotionCommand,
        attack_hook: Option<AttackCone>,
    ) {
        self.attack_part = monster_melee::get_attack_part(w, attacker, motion_command, attack_hook);
        let Some((_, part)) = self.attack_part.clone() else {
            self.general_failure = true;
            return;
        };

        let mut base_damage_mod = monster_melee::get_base_damage(w, attacker, &part);

        // DIVERGE: the Unarmed Combat skill's damage (`EraFormulas::older_melee_damage`).
        // Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/Entity/DamageEvent.cs
        base_damage_mod.base_damage.max_damage = base_damage_mod
            .base_damage
            .max_damage
            .wrapping_add(creature_combat::get_unarmed_skill_damage_bonus(w, attacker));

        self.base_damage = ThreadSafeRandom::next_float(
            base_damage_mod.min_damage(),
            base_damage_mod.max_damage(),
        ) as f32;
        self.base_damage_mod = Some(base_damage_mod);

        self.damage_type =
            monster_combat::get_damage_type_for_part(w, attacker, &part, Some(self.combat_type));
    }

    /// Returns a body part for a player defender.
    // ACE: DamageEvent.GetBodyPart
    pub fn get_body_part_player(&mut self, attack_height: AttackHeight) {
        // select random body part @ current attack height
        self.body_part = body_part::get_body_part(attack_height);
    }

    /// Returns a body part for a creature defender.
    // ACE: DamageEvent.GetBodyPart
    pub fn get_body_part_creature(&mut self, w: &World, defender: ObjectGuid, quadrant: Quadrant) {
        // get cached body parts table
        let Some(body_parts) =
            monster_combat::get_body_parts(w, object(w, defender).biota.weenie_class_id)
        else {
            self.evaded = true;
            return;
        };

        // rng roll for body part
        let body_part = body_parts.roll_body_part(quadrant);

        if body_part == CombatBodyPart::Undefined {
            log::debug!(
                "DamageEvent.GetBodyPart({} ({}) ) - couldn't find body part for wcid {}, Quadrant {}",
                shim::name(w, defender),
                defender,
                object(w, defender).biota.weenie_class_id,
                quadrant.to_dotnet_string()
            );
            self.evaded = true;
            return;
        }

        //Console.WriteLine($"AttackHeight: {AttackHeight}, Quadrant: {quadrant & FrontBack}{quadrant & LeftRight}, AttackPart: {bodyPart}");

        let value = object(w, defender)
            .biota
            .properties_body_part
            .as_ref()
            .and_then(|d| d.get(&body_part))
            .cloned();
        self.properties_body_part = Some((body_part, value.clone()));

        // select random body part @ current attack height
        /*BiotaPropertiesBodyPart = BodyParts.GetBodyPart(defender, attackHeight);

        if (BiotaPropertiesBodyPart == null)
        {
            Evaded = true;
            return;
        }*/

        self.creature_part = Some(CreatureBodyPart::new(defender, (body_part, value)));
    }

    /// Sends the damage breakdown to the observer's debug target.
    // ACE: DamageEvent.ShowInfo
    #[allow(clippy::too_many_lines)]
    pub fn show_info(&self, w: &mut World, creature: ObjectGuid) {
        let target = creature_combat::fields(object(w, creature)).debug_damage_target;
        let Some(target_info) =
            crate::managers::player_manager::get_online_player(w, target.full())
        else {
            creature_combat::fields_mut(w.objects.get_mut(creature).expect("in the world"))
                .debug_damage = DebugDamageType::None;
            return;
        };

        let f = |v: f32| dotnet::to_string(v);
        let name = |g: ObjectGuid| shim::name(w, g);

        // setup
        let mut info = format!("Attacker: {} ({})\n", name(self.attacker), self.attacker);
        info += &format!("Defender: {} ({})\n", name(self.defender), self.defender);

        info += &format!("CombatType: {}\n", self.combat_type.name());

        info += &format!(
            "DamageSource: {} ({})\n",
            name(self.damage_source),
            self.damage_source
        );
        info += &format!("DamageType: {}\n", self.damage_type.to_dotnet_string());

        let weapon_name = match self.weapon {
            Some(g) => format!("{} ({})", name(g), g),
            None => "None\n".to_owned(),
        };
        info += &format!("Weapon: {weapon_name}\n");

        info += &format!("AttackType: {}\n", self.attack_type.to_dotnet_string());
        info += &format!("AttackHeight: {}\n", self.attack_height.to_dotnet_string());

        // lifestone protection
        if self.lifestone_protection {
            info += &format!("LifestoneProtection: {}\n", self.lifestone_protection);
        }

        #[allow(clippy::float_cmp)]
        let not_neutral = |v: f32| v != 0.0 && v != 1.0;

        // evade
        if not_neutral(self.accuracy_mod) {
            info += &format!("AccuracyMod: {}\n", f(self.accuracy_mod));
        }

        info += &format!("EffectiveAttackSkill: {}\n", self.effective_attack_skill);
        info += &format!("EffectiveDefenseSkill: {}\n", self.effective_defense_skill);

        if object(w, self.attacker).overpower().is_some() {
            info += &format!(
                "Overpower: {} ({})\n",
                bool_string(self.overpower),
                f(creature_combat::get_overpower_chance(
                    w,
                    self.attacker,
                    self.defender
                ))
            );
        }

        info += &format!("EvasionChance: {}\n", f(self.evasion_chance));
        info += &format!("Evaded: {}\n", bool_string(self.evaded));

        if !object(w, self.attacker).is_player() {
            if let Some(m) = self.attack_motion {
                info += &format!("AttackMotion: {}\n", m.to_dotnet_string());
            }
            if let Some((key, _)) = &self.attack_part {
                info += &format!("AttackPart: {}\n", key.to_dotnet_string());
            }
        }

        // base damage
        if let Some(bdm) = &self.base_damage_mod {
            info += &format!("BaseDamageRange: {}\n", bdm.range());
        }

        info += &format!("BaseDamage: {}\n", f(self.base_damage));

        // damage modifiers
        info += &format!("AttributeMod: {}\n", f(self.attribute_mod));

        if not_neutral(self.power_mod) {
            info += &format!("PowerMod: {}\n", f(self.power_mod));
        }

        if not_neutral(self.slayer_mod) {
            info += &format!("SlayerMod: {}\n", f(self.slayer_mod));
        }

        if let Some(bdm) = &self.base_damage_mod {
            if bdm.damage_bonus != 0.0 {
                info += &format!("DamageBonus: {}\n", f(bdm.damage_bonus));
            }

            if not_neutral(bdm.damage_mod) {
                info += &format!("DamageMod: {}\n", f(bdm.damage_mod));
            }

            if bdm.elemental_bonus != 0 {
                info += &format!("ElementalDamageBonus: {}\n", bdm.elemental_bonus);
            }
        }

        // critical hit
        info += &format!("CriticalChance: {}\n", f(self.critical_chance));
        info += &format!("CriticalHit: {}\n", bool_string(self.is_critical));

        if self.critical_defended {
            info += &format!(
                "CriticalDefended: {}\n",
                bool_string(self.critical_defended)
            );
        }

        if not_neutral(self.critical_damage_mod) {
            info += &format!("CriticalDamageMod: {}\n", f(self.critical_damage_mod));
        }

        if not_neutral(self.critical_damage_rating_mod) {
            info += &format!(
                "CriticalDamageRatingMod: {}\n",
                f(self.critical_damage_rating_mod)
            );
        }

        // damage ratings
        if not_neutral(self.damage_rating_base_mod) {
            info += &format!("DamageRatingBaseMod: {}\n", f(self.damage_rating_base_mod));
        }

        if not_neutral(self.heritage_mod) {
            info += &format!("HeritageMod: {}\n", f(self.heritage_mod));
        }

        if not_neutral(self.recklessness_mod) {
            info += &format!("RecklessnessMod: {}\n", f(self.recklessness_mod));
        }

        if not_neutral(self.sneak_attack_mod) {
            info += &format!("SneakAttackMod: {}\n", f(self.sneak_attack_mod));
        }

        if not_neutral(self.pk_damage_mod) {
            info += &format!("PkDamageMod: {}\n", f(self.pk_damage_mod));
        }

        if not_neutral(self.damage_rating_mod) {
            info += &format!("DamageRatingMod: {}\n", f(self.damage_rating_mod));
        }

        if self.body_part.0 != 0 {
            // player body part
            info += &format!("BodyPart: {}\n", body_part_string(self.body_part));
        }
        if let Some(armor) = self.armor.as_ref().filter(|a| !a.is_empty()) {
            let names: Vec<String> = armor.iter().map(|&g| name(g)).collect();
            info += &format!("Armors: {}\n", names.join(", "));
        }

        if let Some(part) = &self.creature_part {
            // creature body part
            let key = self
                .properties_body_part
                .as_ref()
                .map_or(CombatBodyPart::Head, |p| p.0);
            info += &format!("BodyPart: {}\n", key.to_dotnet_string());
            let base_armor = part
                .biota
                .1
                .as_ref()
                .expect("System.NullReferenceException: CreaturePart.Biota.Value")
                .base_armor;
            info += &format!("BaseArmor: {base_armor}\n");
        }

        // damage mitigation
        if not_neutral(self.armor_mod) {
            info += &format!("ArmorMod: {}\n", f(self.armor_mod));
        }

        if not_neutral(self.resistance_mod) {
            info += &format!("ResistanceMod: {}\n", f(self.resistance_mod));
        }

        if not_neutral(self.shield_mod) {
            info += &format!("ShieldMod: {}\n", f(self.shield_mod));
        }

        if not_neutral(self.weapon_resistance_mod) {
            info += &format!("WeaponResistanceMod: {}\n", f(self.weapon_resistance_mod));
        }

        if not_neutral(self.damage_resistance_rating_base_mod) {
            info += &format!(
                "DamageResistanceRatingBaseMod: {}\n",
                f(self.damage_resistance_rating_base_mod)
            );
        }

        if not_neutral(self.critical_damage_resistance_rating_mod) {
            info += &format!(
                "CriticalDamageResistanceRatingMod: {}\n",
                f(self.critical_damage_resistance_rating_mod)
            );
        }

        if not_neutral(self.pk_damage_resistance_mod) {
            info += &format!(
                "PkDamageResistanceMod: {}\n",
                f(self.pk_damage_resistance_mod)
            );
        }

        if not_neutral(self.damage_resistance_rating_mod) {
            info += &format!(
                "DamageResistanceRatingMod: {}\n",
                f(self.damage_resistance_rating_mod)
            );
        }

        if self.ignore_magic_armor(w) {
            info += "IgnoreMagicArmor: True\n";
        }
        if self.ignore_magic_resist(w) {
            info += "IgnoreMagicResist: True\n";
        }

        // final damage
        info += &format!(
            "DamageBeforeMitigation: {}\n",
            f(self.damage_before_mitigation)
        );
        info += &format!("DamageMitigated: {}\n", f(self.damage_mitigated));
        info += &format!("Damage: {}\n", f(self.damage));

        info += "----";

        let msg = crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat(&info, ChatMessageType::Broadcast);
        crate::world_objects::player_skills::send(w, target_info, [msg]);
    }

    // ACE: DamageEvent.HandleLogging
    pub fn handle_logging(
        &self,
        w: &mut World,
        attacker: Option<ObjectGuid>,
        defender: Option<ObjectGuid>,
    ) {
        if let Some(attacker) = attacker {
            if (creature_combat::fields(object(w, attacker)).debug_damage.0
                & DebugDamageType::Attacker.0)
                != 0
            {
                self.show_info(w, attacker);
            }
        }
        if let Some(defender) = defender {
            if (creature_combat::fields(object(w, defender)).debug_damage.0
                & DebugDamageType::Defender.0)
                != 0
            {
                self.show_info(w, defender);
            }
        }
    }

    // ACE: DamageEvent.AttackConditions
    #[must_use]
    pub fn attack_conditions(&self) -> AttackConditions {
        let mut attack_conditions = AttackConditions::None;

        if self.critical_defended {
            attack_conditions |= AttackConditions::CriticalProtectionAugmentation;
        }
        if self.recklessness_mod > 1.0 {
            attack_conditions |= AttackConditions::Recklessness;
        }
        if self.sneak_attack_mod > 1.0 {
            attack_conditions |= AttackConditions::SneakAttack;
        }
        if self.overpower {
            attack_conditions |= AttackConditions::Overpower;
        }

        attack_conditions
    }
}

/// C#'s `bool.ToString()`.
fn bool_string(b: bool) -> &'static str {
    if b {
        "True"
    } else {
        "False"
    }
}

/// `BodyPart.ToString()`: the member name, else the number.
fn body_part_string(p: BodyPart) -> String {
    const NAMES: [(i32, &str); 9] = [
        (0x1, "Head"),
        (0x2, "Chest"),
        (0x4, "Abdomen"),
        (0x8, "UpperArm"),
        (0x10, "LowerArm"),
        (0x20, "Hand"),
        (0x40, "UpperLeg"),
        (0x80, "LowerLeg"),
        (0x100, "Foot"),
    ];
    NAMES
        .iter()
        .find(|(v, _)| *v == p.0)
        .map_or_else(|| p.0.to_string(), |(_, n)| (*n).to_owned())
}
