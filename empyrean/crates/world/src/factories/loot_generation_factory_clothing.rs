// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/LootGenerationFactory_Clothing.cs
//! Port of `Source/ACE.Server/Factories/LootGenerationFactory_Clothing.cs`.

use empyrean_common::dotnet::CsCast;
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_content::models::world::TreasureDeath;
use empyrean_entity::enums::{
    CoverageMaskHelper, MutateFilter, PropertyFloat, Skill, SpellId, WieldRequirement,
};
use empyrean_tables::enums::{TreasureArmorType, TreasureItemType};
use empyrean_tables::logic::tables::cloak_chance as cloak_chance_rolls;
use empyrean_tables::logic::tables::{
    armor_mod_vs_type_chance as armor_mod_vs_type_roll, armor_type_chance, workmanship_chance,
};
use empyrean_tables::logic::wcids::cloak_wcids;

use crate::entity::mutations::mutation_cache;
use crate::factories::entity::treasure_roll::TreasureRoll;
use crate::factories::loot_generation_factory::tables_logic::tables::{
    armor_mod_vs_type_chance, cloak_chance, equipment_set_chance, gear_rating_chance,
    gem_count_chance,
};
use crate::factories::loot_generation_factory::tables_logic::wcids::{armor_wcids, clothing_wcids};
use crate::factories::loot_generation_factory::{
    gem_code, get_long_desc_in, get_material_type, has_mutate_filter, mutate_burden, mutate_color,
    mutate_value, roll_gem_type, roll_wield_level_req_t7_t8, wo_name,
    world_object_factory_create_new_world_object,
};
use crate::factories::loot_generation_factory_aetheria::ICON_OVERLAY_ITEM_MAX_LEVEL;
use crate::factories::loot_generation_factory_magic::assign_magic;
use crate::world_objects::world_object::WorldObject;
use crate::World;

/// This is only called by /testlootgen command. The actual lootgen system doesn't use this.
///
/// # Panics
/// As ACE throws a `NullReferenceException`, when the rolled wcid has no weenie.
// ACE: LootGenerationFactory.CreateArmor
pub(crate) fn create_armor(
    w: &mut World,
    profile: &TreasureDeath,
    is_magical: bool,
    is_armor: bool,
) -> Option<WorldObject> {
    let item_type = if is_armor {
        TreasureItemType::Armor
    } else {
        TreasureItemType::Clothing
    };
    let mut treasure_roll = TreasureRoll::with_item_type(item_type);

    if is_armor {
        treasure_roll.armor_type = armor_type_chance::roll(profile.tier);
        treasure_roll.wcid = armor_wcids::roll(profile, &mut treasure_roll.armor_type);
    } else {
        treasure_roll.wcid = clothing_wcids::roll(profile);
    }

    let mut wo =
        world_object_factory_create_new_world_object(w, treasure_roll.wcid.0.cast_unsigned())
            .expect("NullReferenceException: CreateNewWorldObject returned null");
    treasure_roll.base_armor_level = wo.armor_level().unwrap_or(0);

    mutate_armor(w, &mut wo, profile, is_magical, &mut treasure_roll);

    Some(wo)
}

/// Material, colour, gems, workmanship, burden, the T7+ level requirement, armor level, the
/// elemental ArmorModVsType bonuses, spells, the T7+ equipment set, the T8 gear rating, value.
// ACE: LootGenerationFactory.MutateArmor
pub(crate) fn mutate_armor(
    w: &World,
    wo: &mut WorldObject,
    profile: &TreasureDeath,
    is_magical: bool,
    roll: &mut TreasureRoll,
) {
    // material type
    let material_type = get_material_type(w, wo, profile.tier);
    if material_type.0 > 0 {
        wo.set_material_type(Some(material_type));
    }

    // item color
    mutate_color(w, wo);

    // gem count / gem material
    if let Some(gem_code) = gem_code(wo) {
        wo.set_gem_count(Some(gem_count_chance::roll(w, gem_code, profile.tier)));
    } else {
        wo.set_gem_count(Some(ThreadSafeRandom::next(1, 6)));
    }

    wo.set_gem_type(Some(roll_gem_type(profile.tier)));

    // workmanship
    wo.set_item_workmanship(Some(workmanship_chance::roll(profile.tier)));

    // burden
    if has_mutate_filter(wo, MutateFilter::EncumbranceVal) {
        // fixme: data
        mutate_burden(wo, profile, false);
    }

    if profile.tier > 6 && !wo.has_armor_level() {
        // normally this is handled in the mutation script for armor
        // for clothing, just calling the generic method here
        roll_wield_level_req_t7_t8(wo, profile);
    }

    assign_armor_level(w, wo, profile, roll);

    if has_mutate_filter(wo, MutateFilter::ArmorModVsType) {
        mutate_armor_mod_vs_type(wo, profile);
    }

    if is_magical {
        assign_magic(w, wo, profile, roll, true);
    } else {
        wo.set_item_mana_cost(None);
        wo.set_item_max_mana(None);
        wo.set_item_cur_mana(None);
        wo.set_item_spellcraft(None);
        wo.set_item_difficulty(None);
    }

    if profile.tier > 6 && !roll.armor_type.is_society_armor() {
        let equipment_set_id = equipment_set_chance::roll(wo, profile, roll);
        wo.set_equipment_set_id(equipment_set_id);
    }

    if profile.tier == 8 {
        try_mutate_gear_rating(wo, profile, roll);
    }

    // item value
    //if (wo.HasMutateFilter(MutateFilter.Value))   // fixme: data
    mutate_value(w, wo, profile.tier, Some(roll));

    wo.set_long_desc(get_long_desc_in(w.era.loot_rules, wo));
}

/// Runs the armor level mutation script for the item's coverage (society armor keeps its wield
/// requirements).
///
/// # Panics
/// As ACE throws a `NullReferenceException`, when the mutation script is missing.
// ACE: LootGenerationFactory.AssignArmorLevel
fn assign_armor_level(
    w: &World,
    wo: &mut WorldObject,
    profile: &TreasureDeath,
    roll: &TreasureRoll,
) -> bool {
    // retail was only divied up into a few different mutation scripts here
    // anything with ArmorLevel ran these mutation scripts
    // anything that covered extremities (head / hand / foot wear) started with a slightly higher base AL,
    // but otherwise used the same mutation as anything that covered non-extremities
    // shields also had their own mutation script

    // only exceptions found: covenant armor, olthoi armor, metal cap

    if !roll.has_armor_level(wo) {
        return false;
    }

    let Some(script_name) = get_mutation_script_armor_level(wo, roll, w.era.loot_rules) else {
        log::error!(
            "AssignArmorLevel({}, {}, {}) - unknown item type",
            wo_name(wo),
            profile.treasure_type,
            roll.item_type
        );
        return false;
    };

    // persist original values for society armor
    let wield_requirements = wo.wield_requirements();
    let wield_skill_type = wo.wield_skill_type();
    let wield_difficulty = wo.wield_difficulty();

    //Console.WriteLine($"Mutating {wo.Name} with {scriptName}");

    let mutation_filter = mutation_cache::get_mutation(w, script_name)
        .expect("NullReferenceException: no mutation script");

    let success = mutation_filter.try_mutate(wo, profile.tier);

    if roll.armor_type.is_society_armor() {
        wo.set_wield_requirements(wield_requirements);
        wo.set_wield_skill_type(wield_skill_type);
        wo.set_wield_difficulty(wield_difficulty);
    }

    success
}

// ACE: LootGenerationFactory.GetMutationScript_ArmorLevel
fn get_mutation_script_armor_level(
    wo: &WorldObject,
    roll: &TreasureRoll,
    loot_rules: empyrean_common::era::LootRules,
) -> Option<&'static str> {
    // DIVERGE: the Infiltration era's armour levels are its own scripts: covenant armour and
    // shields, other shields, and one for every other piece (ClassicACE's
    // `GetMutationScript_ArmorLevel` at its Infiltration ruleset).
    // Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/Factories/LootGenerationFactory_Clothing.cs
    if loot_rules == empyrean_common::era::LootRules::Infiltration {
        return Some(match (roll.armor_type, wo.is_shield()) {
            (TreasureArmorType::Covenant, true) => {
                "ArmorLevel.Infiltration.covenant_shield_level.txt"
            }
            (TreasureArmorType::Covenant, false) => {
                "ArmorLevel.Infiltration.covenant_armor_level.txt"
            }
            (_, true) => "ArmorLevel.Infiltration.shield_level.txt",
            (_, false) => "ArmorLevel.Infiltration.armor_level.txt",
        });
    }
    match roll.armor_type {
        TreasureArmorType::Covenant => {
            return Some(if wo.is_shield() {
                "ArmorLevel.covenant_shield.txt"
            } else {
                "ArmorLevel.covenant_armor.txt"
            });
        }

        TreasureArmorType::Olthoi => {
            return Some(if wo.is_shield() {
                "ArmorLevel.olthoi_shield.txt"
            } else {
                "ArmorLevel.olthoi_armor.txt"
            });
        }
        _ => {}
    }

    if wo.is_shield() {
        return Some("ArmorLevel.shield_level.txt");
    }

    let coverage = wo.clothing_priority().map_or(0, |c| c.0);

    if coverage & CoverageMaskHelper::Extremities.0 != 0 {
        Some("ArmorLevel.armor_level_extremity.txt")
    } else if coverage & CoverageMaskHelper::Outerwear.0 != 0 {
        Some("ArmorLevel.armor_level_non_extremity.txt")
    } else {
        None
    }
}

// ACE: LootGenerationFactory.MutateArmorModVsType
fn mutate_armor_mod_vs_type(wo: &mut WorldObject, profile: &TreasureDeath) {
    // for the PropertyInt.MutateFilters found in py16 data,
    // items either had all of these, or none of these

    // only the elemental types could mutate
    try_mutate_armor_mod_vs_type(wo, profile, PropertyFloat::ArmorModVsFire);
    try_mutate_armor_mod_vs_type(wo, profile, PropertyFloat::ArmorModVsCold);
    try_mutate_armor_mod_vs_type(wo, profile, PropertyFloat::ArmorModVsAcid);
    try_mutate_armor_mod_vs_type(wo, profile, PropertyFloat::ArmorModVsElectric);
}

/// A tier chance, then a quality level bonus of `level * 0.15 + rng(-0.05, 0.15)`, clamped to
/// [-2, 2].
// ACE: LootGenerationFactory.TryMutateArmorModVsType
fn try_mutate_armor_mod_vs_type(
    wo: &mut WorldObject,
    profile: &TreasureDeath,
    prop: PropertyFloat,
) -> bool {
    let Some(mut armor_mod_vs_type) = wo.get_property(prop) else {
        return false;
    };

    // perform the initial roll to determine if this ArmorModVsType will mutate
    let mutate = armor_mod_vs_type_roll::roll(profile.tier);

    if !mutate {
        return false;
    }

    // get quality level 1-5 for tier
    let quality_level = armor_mod_vs_type_chance::roll_quality_level(profile);

    // add in rng
    // for t6+ / max quality level 5, the highest bonus found in eor data was ~0.9
    let rng = ThreadSafeRandom::next_float(-0.05, 0.15);

    #[allow(clippy::cast_precision_loss)]
    let bonus_rl = f64::from(quality_level as f32 * 0.15f32) + rng;

    //Console.WriteLine($"Boosting {wo.Name}.{prop} by {bonusRL}");

    armor_mod_vs_type += bonus_rl;

    // ensure between -2.0 / 2.0?
    armor_mod_vs_type = armor_mod_vs_type.clamp(-2.0, 2.0);

    wo.set_property(prop, armor_mod_vs_type);

    true
}

/// Adds `AL^2 / 10 * rng(min(bulk, size), max(bulk, size))`.
// ACE: LootGenerationFactory.MutateValue_Armor
pub(crate) fn mutate_value_armor(
    wo: &mut WorldObject,
    loot_rules: empyrean_common::era::LootRules,
) {
    let bulk_mod = wo.bulk_mod().unwrap_or(1.0);
    let size_mod = wo.size_mod().unwrap_or(1.0);

    let armor_level = wo.armor_level().unwrap_or(0);

    // DIVERGE: the Infiltration era's armour adds its armour level times its bulk and size to its
    // value, with no draw (ClassicACE's `MutateValue_Armor` outside its end-of-retail ruleset).
    // Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/Factories/LootGenerationFactory_Clothing.cs
    if loot_rules == empyrean_common::era::LootRules::Infiltration {
        let add: i32 = (f64::from(armor_level) * bulk_mod * size_mod).cs_cast();
        let value = wo.value().map(|v| v.wrapping_add(add));
        wo.set_value(value);
        return;
    }

    // from the py16 mutation scripts
    //wo.Value += (int)(armorLevel * armorLevel / 10.0f * bulkMod * sizeMod);

    // still probably not how retail did it
    // modified for armor values to match closer to retail pcaps
    #[allow(clippy::cast_possible_truncation)]
    let min_rng = empyrean_common::dotnet::math::min(bulk_mod, size_mod) as f32;
    #[allow(clippy::cast_possible_truncation)]
    let max_rng = empyrean_common::dotnet::math::max(bulk_mod, size_mod) as f32;

    let rng = ThreadSafeRandom::next_float(min_rng, max_rng);

    #[allow(clippy::cast_precision_loss)]
    let add: i32 =
        (f64::from(armor_level.wrapping_mul(armor_level) as f32 / 10.0f32) * rng).cs_cast();
    let value = wo.value().map(|v| v.wrapping_add(add));
    wo.set_value(value);
}

/// T8 only, not on shields: a gear rating on the crit-damage (armor), damage (clothing, cloaks)
/// or healing-boost / max-health (jewelry) pair, and the level 180 wield requirement.
// ACE: LootGenerationFactory.TryMutateGearRating
pub(crate) fn try_mutate_gear_rating(
    wo: &mut WorldObject,
    profile: &TreasureDeath,
    roll: &TreasureRoll,
) -> bool {
    if profile.tier != 8 {
        return false;
    }

    // shields don't have gear ratings
    if wo.is_shield() {
        return false;
    }

    let gear_rating = gear_rating_chance::roll(wo, profile, roll);

    if gear_rating == 0 {
        return false;
    }

    //Console.WriteLine($"TryMutateGearRating({wo.Name}, {profile.TreasureType}, {roll.ItemType}): rolled gear rating {gearRating}");

    let rng = ThreadSafeRandom::next(0, 1);

    if roll.has_armor_level(wo) {
        // clothing w/ al, and crowns would be included in this group
        if rng == 0 {
            wo.set_gear_crit_damage(Some(gear_rating));
        } else {
            wo.set_gear_crit_damage_resist(Some(gear_rating));
        }
    } else if roll.is_clothing() || roll.is_cloak() {
        if rng == 0 {
            wo.set_gear_damage(Some(gear_rating));
        } else {
            wo.set_gear_damage_resist(Some(gear_rating));
        }
    } else if roll.is_jewelry() {
        if rng == 0 {
            wo.set_gear_healing_boost(Some(gear_rating));
        } else {
            wo.set_gear_max_health(Some(gear_rating));
        }
    } else {
        log::error!(
            "TryMutateGearRating({}, {}, {}): unknown item type",
            wo_name(wo),
            profile.treasure_type,
            roll.item_type
        );
        return false;
    }

    // ensure wield requirement is level 180?
    if roll.armor_type != TreasureArmorType::Society {
        set_wield_level_req(wo, 180);
    }

    true
}

// ACE: LootGenerationFactory.SetWieldLevelReq
fn set_wield_level_req(wo: &mut WorldObject, level: i32) {
    if wo.wield_requirements() == WieldRequirement::Invalid {
        wo.set_wield_requirements(WieldRequirement::Level);
        wo.set_wield_skill_type(Some(Skill::Axe.0)); // set from examples in pcap data
        wo.set_wield_difficulty(Some(level));
    } else if wo.wield_requirements() == WieldRequirement::Level {
        if wo.wield_difficulty().is_some_and(|d| d < level) {
            wo.set_wield_difficulty(Some(level));
        }
    } else {
        // this can either be empty, or in the case of covenant / olthoi armor,
        // it could already contain a level requirement of 180, or possibly 150 in tier 8

        // we want to set this level requirement to 180, in all cases

        // magloot logs indicated that even if covenant / olthoi armor was not upgraded to 180 in its mutation script,
        // a gear rating could still drop on it, and would "upgrade" the 150 to a 180

        wo.set_wield_requirements2(WieldRequirement::Level);
        wo.set_wield_skill_type2(Some(Skill::Axe.0)); // set from examples in pcap data
        wo.set_wield_difficulty2(Some(level));
    }
}

/// This is only called by /testlootgen command. The actual lootgen system doesn't use this.
// ACE: LootGenerationFactory.CreateCloak
pub(crate) fn create_cloak(
    w: &mut World,
    profile: &TreasureDeath,
    mutate: bool,
) -> Option<WorldObject> {
    let cloak_weenie = cloak_wcids::roll();

    let mut wo = world_object_factory_create_new_world_object(w, cloak_weenie.0.cast_unsigned());

    if let Some(o) = wo.as_mut() {
        if mutate {
            mutate_cloak(w, o, profile, None);
        }
    }

    wo
}

/// Item max level and its wield difficulty and icon overlay, a set, a proc spell (or damage
/// reduction), material, workmanship, the T8 gear rating (lootgen rolls only), value.
// ACE: LootGenerationFactory.MutateCloak
pub(crate) fn mutate_cloak(
    w: &World,
    wo: &mut WorldObject,
    profile: &TreasureDeath,
    roll: Option<&mut TreasureRoll>,
) {
    wo.set_item_max_level(Some(cloak_chance::roll_item_max_level(profile)));

    // wield difficulty, based on ItemMaxLevel
    match wo.item_max_level() {
        Some(1) => wo.set_wield_difficulty(Some(30)),
        Some(2) => wo.set_wield_difficulty(Some(60)),
        Some(3) => wo.set_wield_difficulty(Some(90)),
        Some(4) => wo.set_wield_difficulty(Some(120)),
        Some(5) => wo.set_wield_difficulty(Some(150)),
        _ => {}
    }

    let item_max_level = wo
        .item_max_level()
        .expect("InvalidOperationException: Nullable object must have a value");
    wo.set_icon_overlay_id(Some(super::loot_generation_factory::tables_logic::at(
        &ICON_OVERLAY_ITEM_MAX_LEVEL,
        item_max_level - 1,
    )));

    // equipment set
    wo.set_equipment_set_id(Some(cloak_chance_rolls::roll_equipment_set()));

    // proc spell
    let surge_spell = cloak_chance_rolls::roll_proc_spell();

    if surge_spell == SpellId::Undef {
        // Damage Reduction proc
        wo.set_cloak_weave_proc(Some(2));
    } else {
        wo.set_proc_spell(Some(surge_spell.0));

        // Cloaked In Skill is the only self-targeted spell
        wo.set_proc_spell_self_targeted(wo.proc_spell() == Some(SpellId::CloakAllSkill.0));

        wo.set_cloak_weave_proc(Some(1));
    }

    // material type
    wo.set_material_type(Some(get_material_type(w, wo, profile.tier)));

    // workmanship
    #[allow(clippy::cast_precision_loss)]
    wo.set_workmanship(Some(workmanship_chance::roll(profile.tier) as f32));

    if let Some(roll) = roll.as_deref() {
        if profile.tier == 8 {
            try_mutate_gear_rating(wo, profile, roll);
        }
    }

    // item value
    //if (wo.HasMutateFilter(MutateFilter.Value))
    mutate_value(w, wo, profile.tier, roll.as_deref());
}
