// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/LootGenerationFactory_Spells.cs
//! Port of `Source/ACE.Server/Factories/LootGenerationFactory_Spells.cs`.
//!
//! ACE collects enchantments and cantrips in `HashSet<SpellId>`s and enumerates them; a fresh
//! .NET `HashSet` with no removals enumerates in insertion order, so they are `Vec`s with a
//! membership check here.

use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_content::models::world::TreasureDeath;
use empyrean_entity::enums::{CoverageMask, DamageType, Skill, SpellId, WieldRequirement};
use empyrean_tables::enums::{TreasureArmorType, TreasureItemType, WeenieClassName};
use empyrean_tables::logic::cantrips::{
    armor_cantrips, jewelry_cantrips, melee_cantrips, missile_cantrips, wand_cantrips,
};
use empyrean_tables::logic::tables::{
    spell_level_chance, spell_level_progression, spell_selection_table,
};

use crate::entity::spell::Spell;
use crate::factories::entity::treasure_roll::TreasureRoll;
use crate::factories::loot_generation_factory::tables_logic::cantrips::cantrip_chance;
use crate::factories::loot_generation_factory::tables_logic::spells::{
    armor_spells, melee_spells, missile_spells, wand_spells,
};
use crate::factories::loot_generation_factory::wo_name;
use crate::world_objects::world_object::WorldObject;
use crate::World;

/// Rolls the item's spells and adds each to its spell book (probability 2).
// ACE: LootGenerationFactory.AssignSpells
pub(crate) fn assign_spells(
    w: &World,
    wo: &mut WorldObject,
    profile: &TreasureDeath,
    roll: &mut TreasureRoll,
) {
    let spells = roll_spells(w, wo, profile, roll);

    for spell in spells {
        wo.biota.get_or_add_known_spell(spell.0.cast_signed(), 2.0);
    }
}

/// Item spells (armor with AL, weapons), enchantments and cantrips; the last two add to the roll's
/// item difficulty.
// ACE: LootGenerationFactory.RollSpells
fn roll_spells(
    w: &World,
    wo: &mut WorldObject,
    profile: &TreasureDeath,
    roll: &mut TreasureRoll,
) -> Vec<SpellId> {
    let mut spells = Vec::new();

    // crowns, which are classified as TreasureItemType.Jewelry, should also be getting item spells
    // perhaps replace this with wo.ArmorLevel check?
    //if (roll.IsArmor || roll.IsArmorClothing(wo) || roll.IsWeapon)
    if roll.has_armor_level(wo) || roll.is_weapon() {
        let item_spells = roll_item_spells(wo, profile, roll);

        if let Some(item_spells) = item_spells {
            spells.extend(item_spells);
        }
    }

    let enchantments = roll_enchantments(wo, profile, roll);

    if let Some(enchantments) = enchantments {
        spells.extend_from_slice(&enchantments);

        roll.item_difficulty += roll_enchantment_difficulty(w, &enchantments);
    }

    let cantrips = roll_cantrips(w, wo, profile, roll);

    if let Some(cantrips) = cantrips {
        spells.extend_from_slice(&cantrips);

        roll.item_difficulty += roll_cantrip_difficulty(&cantrips);
    }

    spells
}

// ACE: LootGenerationFactory.RollItemSpells
fn roll_item_spells(
    wo: &WorldObject,
    profile: &TreasureDeath,
    roll: &TreasureRoll,
) -> Option<Vec<SpellId>> {
    //if (roll.IsArmor || roll.IsArmorClothing(wo))
    let spells = if roll.has_armor_level(wo) {
        armor_spells::roll(profile)
    } else if roll.is_melee_weapon() {
        melee_spells::roll(profile)
    } else if roll.is_missile_weapon() {
        missile_spells::roll(profile)
    } else if roll.is_caster() {
        wand_spells::roll(wo, profile)
    } else {
        log::error!(
            "RollItemSpells({}) - item is not clothing / armor / weapon",
            wo_name(wo)
        );
        return None;
    };

    Some(roll_spell_levels(wo, profile, &spells))
}

/// Each spell's level-1 id to a rolled level.
///
/// # Panics
/// As ACE throws a `NullReferenceException`, when a spell has no level progression.
// ACE: LootGenerationFactory.RollSpellLevels
fn roll_spell_levels(
    wo: &WorldObject,
    profile: &TreasureDeath,
    spells: &[SpellId],
) -> Vec<SpellId> {
    let mut final_spells = Vec::new();

    for &spell in spells {
        let spell_level = spell_level_chance::roll(profile.tier);

        let spell_levels = spell_level_progression::get_spell_levels(spell)
            .expect("NullReferenceException: no spell level progression");

        if spell_levels.len() != 8 {
            log::error!(
                "RollSpellLevels({}, {spell}) - spell level progression returned {}, expected 8",
                wo_name(wo),
                spell_levels.len()
            );
            continue;
        }

        final_spells.push(spell_levels[usize::try_from(spell_level - 1).expect("levels 1-8")]);
    }

    final_spells
}

/// Up to `numEnchantments` distinct spells from the item's spell selection code (three attempts
/// per enchantment), at rolled levels.
// ACE: LootGenerationFactory.RollEnchantments
fn roll_enchantments(
    wo: &WorldObject,
    profile: &TreasureDeath,
    roll: &TreasureRoll,
) -> Option<Vec<SpellId>> {
    /*if (wo.SpellSelectionCode == null)
    {
        log.Warn($"RollEnchantments({wo.Name}) - missing spell selection code / PropertyInt.TsysMutationData");
        return null;
    }*/

    // test method: determine spell selection code dynamically
    let spell_selection_code = get_spell_selection_code_dynamic(wo, roll);

    if spell_selection_code == 0 {
        return None;
    }

    //Console.WriteLine($"Using spell selection code {spellSelectionCode} for {wo.Name}");

    let num_enchantments = roll_num_enchantments(wo, profile, roll);

    if num_enchantments <= 0 {
        return None;
    }

    let num_attempts = num_enchantments * 3;

    let mut spells: Vec<SpellId> = Vec::new();

    let mut i = 0;
    while i < num_attempts && i32::try_from(spells.len()).expect("small") < num_enchantments {
        let spell = spell_selection_table::roll(spell_selection_code);

        if spell != SpellId::Undef && !spells.contains(&spell) {
            spells.push(spell);
        }
        i += 1;
    }

    Some(roll_spell_levels(wo, profile, &spells))
}

// ACE: LootGenerationFactory.RollNumEnchantments
fn roll_num_enchantments(wo: &WorldObject, profile: &TreasureDeath, roll: &TreasureRoll) -> i32 {
    if roll.is_armor() || roll.is_weapon() {
        roll_num_enchantments_armor_weapon(wo, profile, roll)
    }
    // confirmed:
    // - crowns (classified as TreasureItemType.Jewelry) used this table
    // - clothing w/ al also used this table
    else if roll.is_clothing() || roll.is_jewelry() || roll.is_dinnerware() {
        roll_num_enchantments_clothing_jewelry_dinnerware(wo, profile, roll)
    } else {
        log::warn!(
            "RollNumEnchantments({}, {}, {}) - unknown item type",
            wo_name(wo),
            profile.treasure_type,
            roll.item_type
        );
        1 // gems?
    }
}

const ENCHANTMENT_CHANCES_ARMOR_MELEE_MISSILE_WEAPON: [f32; 8] = [
    0.00, // T1
    0.05, // T2
    0.10, // T3
    0.20, // T4
    0.40, // T5
    0.60, // T6
    0.60, // T7
    0.60, // T8
];

const ENCHANTMENT_CHANCES_CASTER: [f32; 8] = [
    0.60, // T1
    0.60, // T2
    0.60, // T3
    0.60, // T4
    0.60, // T5
    0.75, // T6
    0.75, // T7
    0.75, // T8
];

// ACE: LootGenerationFactory.RollNumEnchantments_Armor_Weapon
fn roll_num_enchantments_armor_weapon(
    _wo: &WorldObject,
    profile: &TreasureDeath,
    roll: &TreasureRoll,
) -> i32 {
    let tier_chances = if roll.is_caster() {
        &ENCHANTMENT_CHANCES_CASTER
    } else {
        &ENCHANTMENT_CHANCES_ARMOR_MELEE_MISSILE_WEAPON
    };

    let chance =
        crate::factories::loot_generation_factory::tables_logic::at(tier_chances, profile.tier - 1);

    let rng = ThreadSafeRandom::next_interval(profile.loot_quality_mod);

    i32::from(rng < f64::from(chance))
}

// ACE: LootGenerationFactory.RollNumEnchantments_Clothing_Jewelry_Dinnerware
fn roll_num_enchantments_clothing_jewelry_dinnerware(
    _wo: &WorldObject,
    profile: &TreasureDeath,
    _roll: &TreasureRoll,
) -> i32 {
    let chance = 0.1f32;

    let rng = ThreadSafeRandom::next_interval(profile.loot_quality_mod);

    if rng >= f64::from(chance) {
        return 1;
    } else if profile.tier < 6 {
        return 2;
    }

    // tier 6+ has a chance for 3 enchantments
    let rng = ThreadSafeRandom::next_interval(profile.loot_quality_mod * 0.1);

    if rng >= f64::from(chance * 0.5) {
        2
    } else {
        3
    }
}

/// Every spell but the highest (by client level) adds `level * 5 * rng(0.5, 1.5)`.
// ACE: LootGenerationFactory.RollEnchantmentDifficulty
fn roll_enchantment_difficulty(w: &World, spell_ids: &[SpellId]) -> f32 {
    let mut spells = Vec::new();

    for &spell_id in spell_ids {
        let spell = Spell::from_spell_id(w, spell_id, true);
        spells.push(spell);
    }

    // OrderBy is a stable sort
    spells.sort_by_key(|s| s.formula_ref().level());

    let mut item_difficulty = 0.0f32;

    // exclude highest spell
    for spell in spells.iter().take(spells.len().saturating_sub(1)) {
        #[allow(clippy::cast_possible_truncation)]
        let rng = ThreadSafeRandom::next_float(0.5, 1.5) as f32;

        #[allow(clippy::cast_precision_loss)]
        let level = spell.formula_ref().level() as f32;
        item_difficulty += level * 5.0 * rng;
    }

    item_difficulty
}

/// Up to `numCantrips` distinct cantrips for the item type (three attempts per cantrip), at
/// rolled levels; a legendary raises a level requirement below 180 to 180.
// ACE: LootGenerationFactory.RollCantrips
fn roll_cantrips(
    w: &World,
    wo: &mut WorldObject,
    profile: &TreasureDeath,
    roll: &TreasureRoll,
) -> Option<Vec<SpellId>> {
    // no cantrips on dinnerware?
    if roll.item_type == TreasureItemType::ArtObject {
        return None;
    }

    let num_cantrips = cantrip_chance::roll_num_cantrips(w, profile);

    if num_cantrips == 0 {
        return None;
    }

    let num_attempts = num_cantrips * 3;

    let mut cantrips: Vec<SpellId> = Vec::new();

    let mut i = 0;
    while i < num_attempts && i32::try_from(cantrips.len()).expect("small") < num_cantrips {
        let cantrip = roll_cantrip(wo, profile, roll);

        if cantrip != SpellId::Undef && !cantrips.contains(&cantrip) {
            cantrips.push(cantrip);
        }
        i += 1;
    }

    let mut final_cantrips = Vec::new();

    let mut has_legendary = false;

    for &cantrip in &cantrips {
        let cantrip_level = cantrip_chance::roll_cantrip_level(w, profile);

        let cantrip_levels = spell_level_progression::get_spell_levels(cantrip)
            .expect("NullReferenceException: no cantrip level progression");

        if cantrip_levels.len() != 4 {
            log::error!(
                "RollCantrips({}, {}, {}) - {cantrip} has {} cantrip levels, expected 4",
                wo_name(wo),
                profile.treasure_type,
                roll.item_type,
                cantrip_levels.len()
            );
            continue;
        }

        final_cantrips
            .push(cantrip_levels[usize::try_from(cantrip_level - 1).expect("levels 1-4")]);

        if cantrip_level == 4 {
            has_legendary = true;
        }
    }

    // if a legendary cantrip dropped on this item
    if has_legendary && roll.armor_type != TreasureArmorType::Society {
        // and if the item has a level requirement, ensure the level requirement is at least 180
        // if the item does not already contain a level requirement, don't add one?

        if wo.wield_requirements() == WieldRequirement::Level
            && wo.wield_difficulty().is_some_and(|d| d < 180)
        {
            wo.set_wield_difficulty(Some(180));
        }

        if wo.wield_requirements2() == WieldRequirement::Level
            && wo.wield_difficulty2().is_some_and(|d| d < 180)
        {
            wo.set_wield_difficulty2(Some(180));
        }
    }

    Some(final_cantrips)
}

// ACE: LootGenerationFactory.RollCantrip
fn roll_cantrip(wo: &WorldObject, profile: &TreasureDeath, roll: &TreasureRoll) -> SpellId {
    if roll.has_armor_level(wo) || roll.is_clothing() {
        // armor / clothing cantrip
        // this table also applies to crowns (treasureitemtype.jewelry w/ al)
        return armor_cantrips::roll();
    }
    if roll.is_melee_weapon() {
        // melee cantrip
        let mut melee_cantrip = melee_cantrips::roll();

        // adjust for weapon skill
        if melee_cantrip == SpellId::CANTRIPLIGHTWEAPONSAPTITUDE1 {
            melee_cantrip = adjust_for_weapon_mastery(wo);
        }

        melee_cantrip
    } else if roll.is_missile_weapon() {
        // missile cantrip
        missile_cantrips::roll()
    } else if roll.is_caster() {
        // caster cantrip
        let mut caster_cantrip = wand_cantrips::roll();

        if caster_cantrip == SpellId::CANTRIPWARMAGICAPTITUDE1 {
            caster_cantrip = adjust_for_damage_type(wo, caster_cantrip);
        }

        caster_cantrip
    } else if roll.is_jewelry() {
        // jewelry cantrip
        jewelry_cantrips::roll()
    } else {
        log::error!(
            "RollCantrip({}, {}, {}) - unknown item type",
            wo_name(wo),
            profile.treasure_type,
            roll.item_type
        );
        SpellId::Undef
    }
}

/// A minor cantrip adds `rng(5, 10)`, a higher one `rng(10, 20)`.
// ACE: LootGenerationFactory.RollCantripDifficulty
fn roll_cantrip_difficulty(cantrip_ids: &[SpellId]) -> f32 {
    let mut item_difficulty = 0.0f32;

    for &cantrip_id in cantrip_ids {
        let cantrip_levels = spell_level_progression::get_spell_levels(cantrip_id);

        let Some(cantrip_levels) = cantrip_levels.filter(|l| l.len() == 4) else {
            log::error!("RollCantripDifficulty({cantrip_id}) - unknown cantrip");
            continue;
        };

        let cantrip_level = cantrip_levels.iter().position(|&c| c == cantrip_id);

        #[allow(clippy::cast_possible_truncation)]
        if cantrip_level == Some(0) {
            item_difficulty += ThreadSafeRandom::next_float(5.0, 10.0) as f32;
        } else {
            item_difficulty += ThreadSafeRandom::next_float(10.0, 20.0) as f32;
        }
    }
    item_difficulty
}

// ACE: LootGenerationFactory.AdjustForWeaponMastery
fn adjust_for_weapon_mastery(wo: &WorldObject) -> SpellId {
    // handle two-handed weapons
    if wo.weapon_skill() == Skill::TwoHandedCombat {
        return SpellId::CANTRIPTWOHANDEDAPTITUDE1;
    }

    // 10% chance to adjust to dual wielding
    let rng = ThreadSafeRandom::next_float(0.0, 1.0);

    if rng < f64::from(0.1f32) {
        return SpellId::CantripDualWieldAptitude1;
    }

    // heavy/light/finesse weapons
    match wo.weapon_skill() {
        Skill::HeavyWeapons => SpellId::CANTRIPHEAVYWEAPONSAPTITUDE1,
        Skill::LightWeapons => SpellId::CANTRIPLIGHTWEAPONSAPTITUDE1,
        Skill::FinesseWeapons => SpellId::CANTRIPFINESSEWEAPONSAPTITUDE1,
        _ => SpellId::Undef,
    }
}

// ACE: LootGenerationFactory.AdjustForDamageType
fn adjust_for_damage_type(wo: &WorldObject, _spell: SpellId) -> SpellId {
    if wo.w_damage_type() == DamageType::Nether {
        return SpellId::CantripVoidMagicAptitude1;
    }

    if wo.w_damage_type() != DamageType::Undef {
        return SpellId::CANTRIPWARMAGICAPTITUDE1;
    }

    // even split? retail was broken here
    let rng = ThreadSafeRandom::next_float(0.0, 1.0);

    if rng < f64::from(0.5f32) {
        SpellId::CANTRIPWARMAGICAPTITUDE1
    } else {
        SpellId::CantripVoidMagicAptitude1
    }
}

/// An alternate method to using the SpellSelectionCode from PropertyInt.TSysMutationdata
// ACE: LootGenerationFactory.GetSpellSelectionCode_Dynamic
fn get_spell_selection_code_dynamic(wo: &WorldObject, roll: &TreasureRoll) -> i32 {
    if wo.is_gem() {
        1
    } else if roll.item_type == TreasureItemType::Jewelry {
        if roll.has_armor_level(wo) {
            3
        } else {
            2
        }
    } else if roll.wcid == WeenieClassName::orb {
        4
    } else if roll.is_caster() && wo.w_damage_type() != DamageType::Nether {
        5
    } else if roll.is_melee_weapon() && wo.weapon_skill() != Skill::TwoHandedCombat {
        6
    } else if (roll.is_armor() || roll.is_clothing()) && !wo.is_shield() {
        get_spell_code_dynamic_clothing_armor(wo, roll)
    } else if wo.is_shield() {
        8
    } else if roll.is_dinnerware() {
        if roll.wcid == WeenieClassName::flasksimple {
            0
        } else {
            16
        }
    } else if roll.is_missile_weapon() || wo.weapon_skill() == Skill::TwoHandedCombat {
        17
    } else if roll.is_caster() && wo.w_damage_type() == DamageType::Nether {
        19
    } else {
        log::error!(
            "GetSpellCode_Dynamic({}) - couldn't determine spell selection code",
            wo_name(wo)
        );

        0
    }
}

const UPPER_ARMOR: u32 = CoverageMask::OuterwearChest.0
    | CoverageMask::OuterwearUpperArms.0
    | CoverageMask::OuterwearLowerArms.0
    | CoverageMask::OuterwearAbdomen.0;
const LOWER_ARMOR: u32 = CoverageMask::OuterwearUpperLegs.0 | CoverageMask::OuterwearLowerLegs.0; // check abdomen

const CLOTHING: u32 = CoverageMask::UnderwearChest.0
    | CoverageMask::UnderwearUpperArms.0
    | CoverageMask::UnderwearLowerArms.0
    | CoverageMask::UnderwearAbdomen.0
    | CoverageMask::UnderwearUpperLegs.0
    | CoverageMask::UnderwearLowerLegs.0;

// ACE: LootGenerationFactory.GetSpellCode_Dynamic_ClothingArmor
fn get_spell_code_dynamic_clothing_armor(wo: &WorldObject, roll: &TreasureRoll) -> i32 {
    // special cases
    match roll.wcid {
        WeenieClassName::glovescloth => return 14,
        WeenieClassName::capleather => return 20,
        _ => {}
    }

    let coverage_mask = wo.clothing_priority().map_or(0, |c| c.0);
    let is_armor = roll.is_armor();

    if (coverage_mask & UPPER_ARMOR) != 0
        && (coverage_mask & CoverageMask::OuterwearLowerLegs.0) == 0
    {
        return 7;
    }

    if coverage_mask == CoverageMask::Hands.0 && is_armor {
        return 9;
    }

    if coverage_mask == CoverageMask::Head.0 && roll.base_armor_level > 20 {
        return 10;
    }

    // base weenie armorLevel > 20
    if (coverage_mask & CoverageMask::Feet.0) != 0 && roll.base_armor_level > 20 {
        return 11;
    }

    if (coverage_mask & CLOTHING) != 0 {
        return 12;
    }

    // metal cap?
    if coverage_mask == CoverageMask::Head.0 && !is_armor {
        return 13;
    }

    if coverage_mask == CoverageMask::Hands.0 && !is_armor {
        return 14;
    }

    // leggings
    if (coverage_mask & LOWER_ARMOR) != 0 {
        return 15;
    }

    if coverage_mask == CoverageMask::Feet.0 {
        return 18;
    }

    log::error!(
        "GetSpellCode_Dynamic_ClothingArmor({}) - couldn't determine spell selection code for {}, {}",
        wo_name(wo),
        CoverageMask(coverage_mask),
        if is_armor { "True" } else { "False" }
    );
    0
}
