// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/SpellProperties.cs
//! Port of `Source/ACE.Server/Entity/SpellProperties.cs`.
//!
//! Accessors of the dat half (`_spellBase`) panic as ACE's `NullReferenceException` when the dat
//! has no such spell; those of the database half (`_spell`) when the row is missing.

use empyrean_common::dotnet::CsCast;
use empyrean_dat::file_types::spell_table::SpellBaseExt;
use empyrean_entity::enums::properties::PropertyAttribute2nd;
use empyrean_entity::enums::{
    CreatureType, DamageType, DispelType, EnchantmentTypeFlags, ItemType, MagicSchool, PlayScript,
    SpellCategory, SpellFlags, SpellType, TransferFlags,
};
use empyrean_entity::{Position, Quaternion, Vector3};

use crate::entity::spell::Spell;

impl Spell {
    // ======================================
    // SpellBase fields - from the client DAT
    // ======================================

    /// The spell ID.
    // ACE: Spell.Id
    #[must_use]
    pub fn id(&self) -> u32 {
        self.base().meta_spell_id
    }

    /// The spell name.
    // ACE: Spell.Name
    #[must_use]
    pub fn name(&self) -> &str {
        &self.base().name
    }

    /// The spell description that appears in client.
    // ACE: Spell.Description
    #[must_use]
    pub fn description(&self) -> &str {
        &self.base().description
    }

    /// The magic school this spell belongs to.
    // ACE: Spell.School
    #[must_use]
    pub fn school(&self) -> MagicSchool {
        MagicSchool(self.base().school.cs_cast())
    }

    /// The spell icon ID for display in client.
    // ACE: Spell.IconID
    #[must_use]
    pub fn icon_id(&self) -> u32 {
        self.base().icon
    }

    /// Used for spell stacking, ie. Strength Self I and Strength Self VI will be the same category.
    // ACE: Spell.Category
    #[must_use]
    pub fn category(&self) -> SpellCategory {
        SpellCategory(self.base().category)
    }

    /// bit flags for the spell.
    // ACE: Spell.Flags
    #[must_use]
    pub fn flags(&self) -> SpellFlags {
        SpellFlags(self.base().bitfield.cs_cast())
    }

    /// The base mana cost required for casting the spell.
    ///
    /// This and [`Self::power`], [`Self::display_order`] and [`Self::mana_mod`] are signed in the
    /// shipped record; they are handed on as the unsigned numbers ACE reads them as, bit for bit.
    // ACE: Spell.BaseMana
    #[must_use]
    pub fn base_mana(&self) -> u32 {
        u32::from_ne_bytes(self.base().base_mana.to_ne_bytes())
    }

    /// The base maximum distance for casting the spell.
    // ACE: Spell.BaseRangeConstant
    #[must_use]
    pub fn base_range_constant(&self) -> f32 {
        self.base().base_range_constant
    }

    /// An additive multiplier to BaseRangeConstant based on caster's skill level.
    // ACE: Spell.BaseRangeMod
    #[must_use]
    pub fn base_range_mod(&self) -> f32 {
        self.base().base_range_mod
    }

    /// The difficulty of casting the spell.
    // ACE: Spell.Power
    #[must_use]
    pub fn power(&self) -> u32 {
        u32::from_ne_bytes(self.base().power.to_ne_bytes())
    }

    /// Returns the minimum spell power required for some calculations, such as mana conversion
    /// and proficiency points.
    // ACE: Spell.PowerMod
    #[must_use]
    pub fn power_mod(&self) -> u32 {
        self.power().max(25)
    }

    /// The modifier for the original spell economy. A legacy of a bygone era.
    // ACE: Spell.SpellEconomyMod
    #[must_use]
    pub fn spell_economy_mod(&self) -> f32 {
        self.base().spell_economy_mod
    }

    /// The version # of the spell formula.
    // ACE: Spell.FormulaVersion
    #[must_use]
    pub fn formula_version(&self) -> u32 {
        self.base().formula_version
    }

    /// The burn rate for casting the spell.
    // ACE: Spell.ComponentLoss
    #[must_use]
    pub fn component_loss(&self) -> f32 {
        self.base().component_loss
    }

    /// A subtype for the spell.
    // ACE: Spell.MetaSpellType
    #[must_use]
    pub fn meta_spell_type(&self) -> SpellType {
        SpellType(self.base().meta_spell_type.cs_cast())
    }

    /// The amount of time the spell lasts, usually for EnchantmentSpell /
    /// FellowshipEnchantmentSpells.
    ///
    /// If spell has DotDuration, this returns DotDuration + 5, to handle the odd way retail did
    /// DoT ticks (a 15 second DoT performed 4 ticks on the default 5 second heartbeat).
    // ACE: Spell.Duration
    #[must_use]
    pub fn duration(&self) -> f64 {
        if let Some(dot_duration) = self.spell.as_ref().and_then(|s| s.dot_duration) {
            if self.has_extra_tick() {
                dot_duration + f64::from(5.0f32)
            } else {
                dot_duration
            }
        } else {
            // `SpellBase.Duration`: unpacked only for (Fellow)Enchantment spells, else 0.
            self.base().duration.map_or(0.0, |(d, _, _)| d)
        }
    }

    /// The DoT (damage over time) duration for the spell.
    // ACE: Spell.DotDuration
    #[must_use]
    pub fn dot_duration(&self) -> f64 {
        self.db().dot_duration.unwrap_or(0.0)
    }

    /// Returns the number of ticks for a DoT spell. `heartbeat` is the PropertyFloat
    /// .HeartbeatInterval of the Creature with the enchantment (ACE's default argument: 5.0f).
    // ACE: Spell.GetNumTicks
    #[must_use]
    pub fn get_num_ticks(&self, heartbeat: f32) -> i32 {
        if self.dot_duration() == 0.0 {
            return 0;
        }

        // For spell like Surge of Regeneration, which is listed as 19 seconds,
        // this would be the equivalent of 20 seconds, ie. Ceil((19 + 5) / 5) = 5 ticks
        (self.duration() / f64::from(heartbeat)).ceil().cs_cast()
    }

    /// Returns the damage per tick for a DoT spell (ACE's default `heartbeat`: 5.0f).
    // ACE: Spell.GetDamagePerTick
    #[must_use]
    #[allow(clippy::float_cmp, clippy::cast_precision_loss)]
    pub fn get_damage_per_tick(&self, heartbeat: f32) -> f32 {
        if self.dot_duration() == 0.0 {
            return 0.0;
        }

        // normally we can just use the value in StatModVal for this,
        // however it assumes that heartbeat is 5
        if heartbeat == 5.0 {
            return self.stat_mod_val();
        }

        // calculate the total damage w/ default 5 heartbeats
        let total_damage = self.stat_mod_val() * self.get_num_ticks(5.0) as f32;

        // divide totalDamage by the actual # of heartbeats
        let num_ticks = self.get_num_ticks(heartbeat);

        total_damage / num_ticks as f32
    }

    // ACE: Spell.DegradeModifier
    #[must_use]
    pub fn degrade_modifier(&self) -> f32 {
        self.base().duration.map_or(0.0, |(_, m, _)| m)
    }

    // ACE: Spell.DegradeLimit
    #[must_use]
    pub fn degrade_limit(&self) -> f32 {
        self.base().duration.map_or(0.0, |(_, _, l)| l)
    }

    /// The duration for PortalSummon_SpellType (`SpellBase.PortalLifetime`: unpacked only for
    /// PortalSummon spells, else 0).
    // ACE: Spell.PortalLifetime
    #[must_use]
    pub fn portal_lifetime(&self) -> f64 {
        self.base().portal_lifetime.unwrap_or(0.0)
    }

    /// uint values correspond to the SpellComponentsTable (`_spellBase.Formula`, decrypted by
    /// empyrean-dat's `SpellBase.Formula`).
    // ACE: Spell._formula
    #[must_use]
    pub(crate) fn formula_list(&self) -> Vec<u32> {
        self.base().formula()
    }

    /// Effect that plays on the caster for this spell (ie. for buffs, protects, etc.)
    // ACE: Spell.CasterEffect
    #[must_use]
    pub fn caster_effect(&self) -> PlayScript {
        PlayScript(self.base().caster_effect)
    }

    /// Effect that plays on the target for this spell (ie. for debuffs, vulns, etc.)
    // ACE: Spell.TargetEffect
    #[must_use]
    pub fn target_effect(&self) -> PlayScript {
        PlayScript(self.base().target_effect)
    }

    /// is always zero - all spells have the same fizzle effect
    // ACE: Spell.FizzleEffect
    #[must_use]
    pub fn fizzle_effect(&self) -> PlayScript {
        PlayScript(self.base().fizzle_effect)
    }

    /// is always zero
    // ACE: Spell.RecoveryInterval
    #[must_use]
    pub fn recovery_interval(&self) -> f64 {
        self.base().recovery_interval
    }

    /// is always zero
    // ACE: Spell.RecoveryAmount
    #[must_use]
    pub fn recovery_amount(&self) -> f32 {
        self.base().recovery_amount
    }

    /// For sorting in the spell list in the client UI.
    // ACE: Spell.DisplayOrder
    #[must_use]
    pub fn display_order(&self) -> u32 {
        u32::from_ne_bytes(self.base().display_order.to_ne_bytes())
    }

    /// The allowed target types for this spell.
    // ACE: Spell.NonComponentTargetType
    #[must_use]
    pub fn non_component_target_type(&self) -> ItemType {
        ItemType(self.base().non_component_target_type)
    }

    /// Additional mana cost per target (e.g. "Incantation of Acid Bane" Mana Cost = 80 + 14 per
    /// target).
    // ACE: Spell.ManaMod
    #[must_use]
    pub fn mana_mod(&self) -> u32 {
        u32::from_ne_bytes(self.base().mana_mod.to_ne_bytes())
    }

    //==================================
    // Spell fields - from the server DB
    //==================================

    /// The stat modifier type, usually EnchantmentTypeFlags.
    // ACE: Spell.StatModType
    #[must_use]
    pub fn stat_mod_type(&self) -> EnchantmentTypeFlags {
        EnchantmentTypeFlags(self.db().stat_mod_type.unwrap_or(0).cs_cast())
    }

    /// The stat modifier key, used for lookup in the enchantment registry.
    // ACE: Spell.StatModKey
    #[must_use]
    pub fn stat_mod_key(&self) -> u32 {
        self.db().stat_mod_key.unwrap_or(0)
    }

    /// The amount to modify a stat.
    // ACE: Spell.StatModVal
    #[must_use]
    pub fn stat_mod_val(&self) -> f32 {
        self.db().stat_mod_val.unwrap_or(0.0)
    }

    /// The damage type for this spell.
    // ACE: Spell.DamageType
    #[must_use]
    pub fn damage_type(&self) -> DamageType {
        DamageType(self.db().e_type.unwrap_or(0).cs_cast())
    }

    /// The base amount of damage for this spell.
    // ACE: Spell.BaseIntensity
    #[must_use]
    pub fn base_intensity(&self) -> i32 {
        self.db().base_intensity.unwrap_or(0)
    }

    // ACE: Spell.MinDamage
    #[must_use]
    pub fn min_damage(&self) -> i32 {
        self.base_intensity()
    }

    /// The maximum additional damage for this spell.
    // ACE: Spell.Variance
    #[must_use]
    pub fn variance(&self) -> i32 {
        self.db().variance.unwrap_or(0)
    }

    /// `BaseIntensity + Variance` (unchecked `int` addition).
    // ACE: Spell.MaxDamage
    #[must_use]
    pub fn max_damage(&self) -> i32 {
        self.base_intensity().wrapping_add(self.variance())
    }

    /// The weenie class ID associated for this spell, ie. the projectile weenie class id.
    // ACE: Spell.WeenieClassId
    #[must_use]
    pub fn weenie_class_id(&self) -> u32 {
        self.db().wcid.unwrap_or(0)
    }

    // ACE: Spell.Wcid
    #[must_use]
    pub fn wcid(&self) -> u32 {
        self.db().wcid.unwrap_or(0)
    }

    /// The total # of projectiles launched for this spell.
    // ACE: Spell.NumProjectiles
    #[must_use]
    pub fn num_projectiles(&self) -> i32 {
        self.db().num_projectiles.unwrap_or(0)
    }

    /// The maximum # of additional projectiles possibly launched.
    // ACE: Spell.NumProjectilesVariance
    #[must_use]
    pub fn num_projectiles_variance(&self) -> i32 {
        self.db().num_projectiles_variance.unwrap_or(0)
    }

    /// The total angle for multi-projectile spells, ie. 90 degrees for 3-5 projectiles, or 360
    /// degrees for ring spells.
    // ACE: Spell.SpreadAngle
    #[must_use]
    pub fn spread_angle(&self) -> f32 {
        self.db().spread_angle.unwrap_or(0.0)
    }

    /// The vertical angle to launch this spell.
    // ACE: Spell.VerticalAngle
    #[must_use]
    pub fn vertical_angle(&self) -> f32 {
        self.db().vertical_angle.unwrap_or(0.0)
    }

    /// The default angle to launch this spell (relative to player, or global?)
    // ACE: Spell.DefaultLaunchAngle
    #[must_use]
    pub fn default_launch_angle(&self) -> f32 {
        self.db().default_launch_angle.unwrap_or(0.0)
    }

    /// If this is on then projectile spells won't lead a target. Arc spells have this set to true.
    // ACE: Spell.NonTracking
    #[must_use]
    pub fn non_tracking(&self) -> bool {
        self.db().non_tracking.unwrap_or(false)
    }

    /// The offset to apply to the spawn position.
    // ACE: Spell.CreateOffset
    #[must_use]
    pub fn create_offset(&self) -> Vector3 {
        let s = self.db();
        Vector3::new(
            s.create_offset_origin_x.unwrap_or(0.0),
            s.create_offset_origin_y.unwrap_or(0.0),
            s.create_offset_origin_z.unwrap_or(0.0),
        )
    }

    /// The minimum amount of padding to ensure for the spell to spawn.
    // ACE: Spell.Padding
    #[must_use]
    pub fn padding(&self) -> Vector3 {
        let s = self.db();
        Vector3::new(
            s.padding_origin_x.unwrap_or(0.0),
            s.padding_origin_y.unwrap_or(0.0),
            s.padding_origin_z.unwrap_or(0.0),
        )
    }

    /// The dimensions of the origin, used for Volley spells?
    // ACE: Spell.Dims
    #[must_use]
    pub fn dims(&self) -> Vector3 {
        let s = self.db();
        Vector3::new(
            s.dims_origin_x.unwrap_or(0.0),
            s.dims_origin_y.unwrap_or(0.0),
            s.dims_origin_z.unwrap_or(0.0),
        )
    }

    /// The maximum variation for spawn position.
    // ACE: Spell.Peturbation
    #[must_use]
    pub fn peturbation(&self) -> Vector3 {
        let s = self.db();
        Vector3::new(
            s.peturbation_origin_x.unwrap_or(0.0),
            s.peturbation_origin_y.unwrap_or(0.0),
            s.peturbation_origin_z.unwrap_or(0.0),
        )
    }

    /// The imbued effect for this spell.
    // ACE: Spell.ImbuedEffect
    #[must_use]
    pub fn imbued_effect(&self) -> u32 {
        self.db().imbued_effect.unwrap_or(0)
    }

    /// The creature class for a slayer spell.
    // ACE: Spell.SlayerCreatureType
    #[must_use]
    pub fn slayer_creature_type(&self) -> CreatureType {
        CreatureType(self.db().slayer_creature_type.unwrap_or(0).cast_unsigned())
    }

    /// The amount of additional damage for a slayer spell (0 = additive, or 1 = multiplier?).
    // ACE: Spell.SlayerDamageBonus
    #[must_use]
    pub fn slayer_damage_bonus(&self) -> f32 {
        self.db().slayer_damage_bonus.unwrap_or(0.0)
    }

    /// The critical chance frequency for this spell (default: 0, 1, or 0.03?).
    // ACE: Spell.CritFrequency
    #[must_use]
    pub fn crit_frequency(&self) -> f64 {
        self.db().crit_freq.unwrap_or(0.0)
    }

    /// The critical damage multiplier for this spell (verify default multiplier?).
    // ACE: Spell.CritMultiplier
    #[must_use]
    pub fn crit_multiplier(&self) -> f64 {
        self.db().crit_multiplier.unwrap_or(1.0)
    }

    /// If TRUE, ignores magic resistance (`Convert.ToBoolean(int)`: non-zero).
    // ACE: Spell.IgnoreMagicResist
    #[must_use]
    pub fn ignore_magic_resist(&self) -> bool {
        self.db().ignore_magic_resist.unwrap_or(0) != 0
    }

    /// The elemental damage multiplier for this spell (verify default multiplier?).
    // ACE: Spell.ElementalModifier
    #[must_use]
    pub fn elemental_modifier(&self) -> f64 {
        self.db().elemental_modifier.unwrap_or(1.0)
    }

    /// The amount of source vital to drain for a life spell.
    // ACE: Spell.DrainPercentage
    #[must_use]
    pub fn drain_percentage(&self) -> f32 {
        self.db().drain_percentage.unwrap_or(0.0)
    }

    /// The percentage of DrainPercentage to damage a target for life projectiles.
    // ACE: Spell.DamageRatio
    #[must_use]
    pub fn damage_ratio(&self) -> f32 {
        self.db().damage_ratio.unwrap_or(1.0)
    }

    /// DamageType used by LifeMagic spells that specifies Health, Mana, or Stamina for the Boost
    /// type spells.
    // ACE: Spell.VitalDamageType
    #[must_use]
    pub fn vital_damage_type(&self) -> DamageType {
        DamageType(self.db().damage_type.unwrap_or(0))
    }

    /// The minimum amount of vital boost from a life spell.
    // ACE: Spell.Boost
    #[must_use]
    pub fn boost(&self) -> i32 {
        self.db().boost.unwrap_or(0)
    }

    /// Boost + BoostVariance = the maximum amount of vital boost from a life spell.
    // ACE: Spell.BoostVariance
    #[must_use]
    pub fn boost_variance(&self) -> i32 {
        self.db().boost_variance.unwrap_or(0)
    }

    /// `Boost + BoostVariance` (unchecked `int` addition).
    // ACE: Spell.MaxBoost
    #[must_use]
    pub fn max_boost(&self) -> i32 {
        self.boost().wrapping_add(self.boost_variance())
    }

    /// The source vital for a life spell (`(PropertyAttribute2nd)int`: the enum is `ushort`).
    // ACE: Spell.Source
    #[must_use]
    pub fn source(&self) -> PropertyAttribute2nd {
        PropertyAttribute2nd(self.db().source.unwrap_or(0).cs_cast())
    }

    /// The destination vital for a life spell.
    // ACE: Spell.Destination
    #[must_use]
    pub fn destination(&self) -> PropertyAttribute2nd {
        PropertyAttribute2nd(self.db().destination.unwrap_or(0).cs_cast())
    }

    /// The proportion of source vital to transfer to destination vital.
    // ACE: Spell.Proportion
    #[must_use]
    pub fn proportion(&self) -> f32 {
        self.db().proportion.unwrap_or(1.0)
    }

    /// The percent of source vital loss for a life magic transfer spell.
    // ACE: Spell.LossPercent
    #[must_use]
    pub fn loss_percent(&self) -> f32 {
        self.db().loss_percent.unwrap_or(0.0)
    }

    /// A static amount of source vital loss for a life magic transfer spell? Unused / unknown?
    // ACE: Spell.SourceLoss
    #[must_use]
    pub fn source_loss(&self) -> i32 {
        self.db().source_loss.unwrap_or(0)
    }

    /// The maximum amount of vital transferred by a life magic spell.
    // ACE: Spell.TransferCap
    #[must_use]
    pub fn transfer_cap(&self) -> i32 {
        self.db().transfer_cap.unwrap_or(0)
    }

    /// The maximum destination vital boost for a life magic transfer spell? Unused / unknown?
    // ACE: Spell.MaxBoostAllowed
    #[must_use]
    pub fn max_boost_allowed(&self) -> i32 {
        self.db().max_boost_allowed.unwrap_or(0)
    }

    /// Indicates the source and destination for life magic transfer spells.
    // ACE: Spell.TransferFlags
    #[must_use]
    pub fn transfer_flags(&self) -> TransferFlags {
        TransferFlags(self.db().transfer_bitfield.unwrap_or(0).cast_signed())
    }

    /// Unknown index?
    // ACE: Spell.Index
    #[must_use]
    pub fn index(&self) -> i32 {
        self.db().index.unwrap_or(0)
    }

    /// For SpellType.PortalSummon spells, Link is set to either 1 for LinkedPortalOneDID or 2 for
    /// LinkedPortalTwoDID.
    // ACE: Spell.Link
    #[must_use]
    pub fn link(&self) -> i32 {
        self.db().link.unwrap_or(0)
    }

    /// A destination location for a spell (a new `Position` on every read).
    // ACE: Spell.Position
    #[must_use]
    pub fn position(&self) -> Position {
        let s = self.db();
        Position::from_vectors(
            s.position_obj_cell_id.unwrap_or(0),
            Vector3::new(
                s.position_origin_x.unwrap_or(0.0),
                s.position_origin_y.unwrap_or(0.0),
                s.position_origin_z.unwrap_or(0.0),
            ),
            Quaternion::new(
                s.position_angles_x.unwrap_or(0.0),
                s.position_angles_y.unwrap_or(0.0),
                s.position_angles_z.unwrap_or(0.0),
                s.position_angles_w.unwrap_or(0.0),
            ),
        )
    }

    /// The minimum spell power to dispel (unused?).
    // ACE: Spell.MinPower
    #[must_use]
    pub fn min_power(&self) -> i32 {
        self.db().min_power.unwrap_or(0)
    }

    /// The maximum spell power to dispel.
    // ACE: Spell.MaxPower
    #[must_use]
    pub fn max_power(&self) -> i32 {
        self.db().max_power.unwrap_or(0)
    }

    /// Possible RNG for spell power to dispel (unused?).
    // ACE: Spell.PowerVariance
    #[must_use]
    pub fn power_variance(&self) -> f32 {
        self.db().power_variance.unwrap_or(0.0)
    }

    /// The magic school to dispel, or undefined for all schools.
    // ACE: Spell.DispelSchool
    #[must_use]
    pub fn dispel_school(&self) -> MagicSchool {
        MagicSchool(self.db().dispel_school.unwrap_or(0))
    }

    /// The type of spells to dispel: 0 = all spells, 1 = positive, 2 = negative.
    // ACE: Spell.Align
    #[must_use]
    pub fn align(&self) -> DispelType {
        DispelType(self.db().align.unwrap_or(0))
    }

    /// The maximum # of spells to dispel.
    // ACE: Spell.Number
    #[must_use]
    pub fn number(&self) -> i32 {
        self.db().number.unwrap_or(0)
    }

    /// Number * NumberVariance = the minimum # of spells to dispel.
    // ACE: Spell.NumberVariance
    #[must_use]
    pub fn number_variance(&self) -> f32 {
        self.db().number_variance.unwrap_or(0.0)
    }
}
