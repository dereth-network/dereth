// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/AetheriaChance.cs, Source/ACE.Server/Factories/Tables/ArmorModVsTypeChance.cs, Source/ACE.Server/Factories/Tables/CasterSlotSpells.cs, Source/ACE.Server/Factories/Tables/CloakChance.cs, Source/ACE.Server/Factories/Tables/EquipmentSetChance.cs, Source/ACE.Server/Factories/Tables/GearRatingChance.cs, Source/ACE.Server/Factories/Tables/GemCountChance.cs, Source/ACE.Server/Factories/Tables/PetDeviceChance.cs, Source/ACE.Server/Factories/Tables/QualityChance.cs, Source/ACE.Server/Factories/Tables/ScrollLevelChance.cs
//! The 4.10 members of the classes directly in `Factories/Tables` (see the module docs of
//! `empyrean_tables::logic::tables` for the rest of each class).

/// ACE `AetheriaChance` (namespace `Tables.Wcids`, file `Tables/AetheriaChance.cs`).
pub mod aetheria_chance {
    use empyrean_content::models::world::TreasureDeath;
    use empyrean_tables::tables::aetheria_chance::ITEM_MAX_LEVELS;

    use super::super::at;

    /// 0 below tier 5, else the tier's item max level table.
    // ACE: AetheriaChance.Roll_ItemMaxLevel
    #[must_use]
    pub fn roll_item_max_level(profile: &TreasureDeath) -> i32 {
        if profile.tier < 5 {
            return 0;
        }

        let table = at(&ITEM_MAX_LEVELS, profile.tier - 5);

        table.roll(profile.loot_quality_mod)
    }
}

/// ACE `ArmorModVsTypeChance`.
pub mod armor_mod_vs_type_chance {
    use empyrean_content::models::world::TreasureDeath;
    use empyrean_tables::tables::armor_mod_vs_type_chance::QUALITY_LEVELS;

    use super::super::at;

    /// The 1-5 quality level of an ArmorModVsType bonus, for tier 2 and above.
    ///
    /// # Panics
    /// Never for tiers 2-8 (tier 1's table is `null`: ACE would throw).
    // ACE: ArmorModVsTypeChance.RollQualityLevel
    #[must_use]
    pub fn roll_quality_level(profile: &TreasureDeath) -> i32 {
        if profile.tier < 2 {
            return 0;
        }

        let table = at(&QUALITY_LEVELS, profile.tier - 1)
            .expect("NullReferenceException: no quality level table");

        // quality mod?
        table.roll(profile.loot_quality_mod)
    }
}

/// ACE `CasterSlotSpells`.
pub mod caster_slot_spells {
    use empyrean_entity::enums::{DamageType, SpellId, WeenieClassName};
    use empyrean_tables::tables::caster_slot_spells::{
        NETHER_SPELLS, ORB_SPELLS, WAND_STAFF_SPELLS,
    };

    use crate::world_objects::world_object::WorldObject;

    /// The caster's first spell: the orb, nether or wand/staff table.
    // ACE: CasterSlotSpells.Roll
    #[must_use]
    pub fn roll(wo: &WorldObject) -> SpellId {
        let table = if is_orb(wo) {
            &ORB_SPELLS
        } else if wo.w_damage_type() == DamageType::Nether {
            &NETHER_SPELLS
        } else {
            &WAND_STAFF_SPELLS
        };

        table.roll(0.0)
    }

    // ACE: CasterSlotSpells.IsOrb
    #[must_use]
    pub fn is_orb(wo: &WorldObject) -> bool {
        // todo: any other wcids for obs?
        i64::from(wo.biota.weenie_class_id) == i64::from(WeenieClassName::W_ORB_CLASS.0)
        // ACE.Entity.Enum.WeenieClassName
    }
}

/// ACE `CloakChance`.
pub mod cloak_chance {
    use empyrean_content::models::world::TreasureDeath;
    use empyrean_tables::tables::cloak_chance::CLOAK_LEVELS;

    use super::super::at;

    // ACE: CloakChance.Roll_ItemMaxLevel
    #[must_use]
    pub fn roll_item_max_level(profile: &TreasureDeath) -> i32 {
        let table = at(&CLOAK_LEVELS, profile.tier - 1);

        table.roll(profile.loot_quality_mod)
    }
}

/// ACE `EquipmentSetChance`.
pub mod equipment_set_chance {
    use empyrean_common::thread_safe_random::ThreadSafeRandom;
    use empyrean_content::models::world::TreasureDeath;
    use empyrean_entity::enums::{CoverageMask, CoverageMaskHelper, EquipmentSet};
    use empyrean_tables::tables::equipment_set_chance::{ARMOR_SETS, ARMOR_SET_CHANCE};

    use super::super::{at, count};
    use crate::factories::entity::treasure_roll::TreasureRoll;
    use crate::world_objects::world_object::WorldObject;

    /// t7 and t8 armor has a ~1/3 chance of having an equipment set.
    // ACE: EquipmentSetChance.Roll
    #[must_use]
    pub fn roll(
        wo: &WorldObject,
        profile: &TreasureDeath,
        roll: &TreasureRoll,
    ) -> Option<EquipmentSet> {
        if profile.tier < 6 || !roll.has_armor_level(wo) {
            return None;
        }

        match wo.clothing_priority() {
            None => return None,
            Some(p) if p.bits() & CoverageMask(CoverageMaskHelper::Outerwear.0).bits() == 0 => {
                return None
            }
            Some(_) => {}
        }

        // loot quality mod?
        if !ARMOR_SET_CHANCE.roll(profile.loot_quality_mod) {
            return None;
        }

        // each armor set has an even chance of being selected
        let rng = ThreadSafeRandom::next(0, count(&ARMOR_SETS) - 1);

        Some(at(&ARMOR_SETS, rng))
    }
}

/// ACE `GearRatingChance`.
pub mod gear_rating_chance {
    use empyrean_content::models::world::TreasureDeath;
    use empyrean_tables::tables::gear_rating_chance::{
        ARMOR_RATING, CLOTHING_JEWELRY_RATING, RATING_CHANCE,
    };

    use super::super::wo_name;
    use crate::factories::entity::treasure_roll::TreasureRoll;
    use crate::world_objects::world_object::WorldObject;

    /// A T8 gear rating: 0 when the initial chance fails.
    // ACE: GearRatingChance.Roll
    #[must_use]
    pub fn roll(wo: &WorldObject, profile: &TreasureDeath, roll: &TreasureRoll) -> i32 {
        // initial roll for rating chance
        if !RATING_CHANCE.roll(profile.loot_quality_mod) {
            return 0;
        }

        // roll for the actual rating
        let rating = if roll.has_armor_level(wo) {
            &ARMOR_RATING
        } else if roll.is_clothing() || roll.is_jewelry() || roll.is_cloak() {
            &CLOTHING_JEWELRY_RATING
        } else {
            log::error!(
                "GearRatingChance.Roll({}, {}, {}): unknown item type",
                wo_name(wo),
                profile.treasure_type,
                roll.item_type
            );
            return 0;
        };

        rating.roll(profile.loot_quality_mod)
    }
}

/// ACE `GemCountChance`: gem code -> tier -> (count, chance), from the world database's
/// `treasure_gem_count` rows.
///
/// ACE's static constructor queries the world database on first use; here the table is built on
/// the first roll from the world's `WorldDatabase` (`get_all_treasure_gem_count`, the rows in
/// primary-key order) and kept in `World.loot_tables`.
pub mod gem_count_chance {
    use std::collections::HashMap;

    use empyrean_content::models::world::TreasureGemCount;
    use empyrean_tables::entity::ChanceTable;

    use crate::World;

    // gem code -> tier -> (count, chance)

    // gem code is (byte)(TsysMutationData >> 8)

    pub(crate) type GemCodes = HashMap<u8, HashMap<i32, ChanceTable<i32>>>;

    /// The static constructor over `ctx.TreasureGemCount.Where(i => i.Chance > 0)`: `gem_counts`
    /// are every row of the table, in primary-key order. Replaces the world's table (a test
    /// installs its own rows this way; a roll otherwise builds it from the world's content).
    // ACE: GemCountChance.GemCountChance
    pub fn gem_count_chance(w: &mut World, gem_counts: &[TreasureGemCount]) {
        w.loot_tables.gem_codes = std::sync::OnceLock::from(build(gem_counts));
    }

    fn build(gem_counts: &[TreasureGemCount]) -> GemCodes {
        let mut gem_codes: HashMap<u8, HashMap<i32, Vec<(i32, f32)>>> = HashMap::new();

        for gem_count in gem_counts.iter().filter(|i| i.chance > 0.0) {
            let tiers = gem_codes.entry(gem_count.gem_code).or_default();

            let chances = tiers.entry(gem_count.tier).or_default();

            chances.push((gem_count.count, gem_count.chance));
        }

        gem_codes
            .into_iter()
            .map(|(code, tiers)| {
                (
                    code,
                    tiers
                        .into_iter()
                        .map(|(t, c)| (t, ChanceTable::from_vec(c)))
                        .collect(),
                )
            })
            .collect()
    }

    /// Whether the constructor has run for this world.
    #[must_use]
    pub fn is_initialized(w: &World) -> bool {
        w.loot_tables.gem_codes.get().is_some()
    }

    /// Runs `f` with the table for a gem code and tier (tiers above 6 use tier 6's).
    // ACE: GemCountChance.GetChanceTable
    fn get_chance_table<R>(
        w: &World,
        gem_code: u8,
        mut tier: i32,
        f: impl FnOnce(Option<&ChanceTable<i32>>) -> R,
    ) -> R {
        // temporary code
        if tier > 6 {
            tier = 6;
        }

        // the static constructor, on first use
        let gem_codes = w
            .loot_tables
            .gem_codes
            .get_or_init(|| build(&w.content.get_all_treasure_gem_count()));

        f(gem_codes.get(&gem_code).and_then(|tiers| tiers.get(&tier)))
    }

    /// The gem count for a gem code and tier, or 0 when there is no table.
    // ACE: GemCountChance.Roll
    #[must_use]
    pub fn roll(w: &World, gem_code: u8, tier: i32) -> i32 {
        get_chance_table(w, gem_code, tier, |chance_table| match chance_table {
            Some(chance_table) => chance_table.roll(0.0),
            None => 0,
        })
    }
}

/// ACE `PetDeviceChance`.
pub mod pet_device_chance {
    use empyrean_content::models::world::TreasureDeath;
    use empyrean_tables::tables::pet_device_chance::PET_LEVEL_CHANCES;

    use super::super::at;

    /// Rolls for a CombatPet level for a PetDevice
    // ACE: PetDeviceChance.Roll
    #[must_use]
    pub fn roll(profile: &TreasureDeath) -> i32 {
        let table = at(&PET_LEVEL_CHANCES, profile.tier - 1);

        table.roll(profile.loot_quality_mod)
    }
}

/// ACE `QualityChance`.
pub mod quality_chance {
    use empyrean_common::thread_safe_random::ThreadSafeRandom;
    use empyrean_content::models::world::TreasureDeath;
    use empyrean_tables::logic::tables::quality_chance::get_quality_chances_for_tier;
    use empyrean_tables::tables::quality_chance::QUALITY_CHANCE_PER_TIER;

    use super::super::at;

    /// Rolls for the initial chance of getting a quality bonus for an item. The chances are based
    /// on the tier, and can be increased with `LootQualityMod`.
    // ACE: QualityChance.RollTierChance
    fn roll_tier_chance(treasure_death: &TreasureDeath) -> bool {
        let tier_chance = at(&QUALITY_CHANCE_PER_TIER, treasure_death.tier - 1);

        // use for initial roll? logic seems backwards here...
        let rng = ThreadSafeRandom::next_interval(treasure_death.loot_quality_mod);

        rng < f64::from(tier_chance)
    }

    /// Returns a quality level between 1 - 12
    // ACE: QualityChance.Roll
    #[must_use]
    pub fn roll(treasure_death: &TreasureDeath) -> i32 {
        // roll for the initial chance for any quality modification -- based on tier
        if !roll_tier_chance(treasure_death) {
            return 0;
        }

        // if the initial roll succeeds, roll for the actual quality level -- also based on tier
        let chances = get_quality_chances_for_tier(treasure_death.tier);

        //var rng = ThreadSafeRandom.NextIntervalMax(treasureDeath.LootQualityMod);
        let rng = ThreadSafeRandom::next_float(0.0, 1.0);

        for (i, &cur_chance) in chances.iter().enumerate() {
            if rng < f64::from(cur_chance) && cur_chance >= treasure_death.loot_quality_mod {
                return i32::try_from(i + 1).expect("12 levels");
            }
        }

        log::error!(
            "QualityTables.Roll({}, {}) - this shouldn't happen",
            treasure_death.tier,
            empyrean_common::dotnet::to_string(treasure_death.loot_quality_mod)
        );
        0
    }

    /// Performs a weighted RNG roll, linearly interpolating between discrete values: a quality
    /// level between 0.0 and 1.0, higher values better.
    // ACE: QualityChance.RollInterval
    #[must_use]
    pub fn roll_interval(treasure_death: &TreasureDeath) -> f32 {
        // roll for the initial chance for any quality modification -- based on tier
        if !roll_tier_chance(treasure_death) {
            return 0.0;
        }

        // if the initial roll succeeds, roll for the actual quality level -- also based on tier
        let chances = get_quality_chances_for_tier(treasure_death.tier);

        //var rng = ThreadSafeRandom.NextIntervalMax(treasureDeath.LootQualityMod);
        let rng = ThreadSafeRandom::next_float(0.0, 1.0);

        for (i, &cur_chance) in chances.iter().enumerate() {
            if rng < f64::from(cur_chance) && cur_chance >= treasure_death.loot_quality_mod {
                let prev_chance = if i > 0 { chances[i - 1] } else { 0.0 };

                let dx = cur_chance - prev_chance;
                #[allow(clippy::cast_precision_loss)]
                let dy = 1.0f32 / chances.len() as f32;

                let interval = (rng - f64::from(prev_chance)) / f64::from(dx);

                #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
                return (f64::from(dy) * (interval + i as f64)) as f32;
            }
        }

        log::error!(
            "QualityTables.RollInterval({}, {}) - this shouldn't happen",
            treasure_death.tier,
            empyrean_common::dotnet::to_string(treasure_death.loot_quality_mod)
        );
        0.0
    }
}

/// ACE `ScrollLevelChance`.
pub mod scroll_level_chance {
    use empyrean_content::models::world::TreasureDeath;
    use empyrean_tables::tables::scroll_level_chance::SCROLL_LEVEL_CHANCES;

    use super::super::at;

    // ACE: ScrollLevelChance.Roll
    #[must_use]
    pub fn roll(profile: &TreasureDeath) -> i32 {
        // shortcut
        if profile.tier >= 5 {
            return 7;
        }

        let table = at(&SCROLL_LEVEL_CHANCES, profile.tier - 1);

        table.roll(profile.loot_quality_mod)
    }
}
