// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/LootGenerationFactory_Magic.cs
//! Port of `Source/ACE.Server/Factories/LootGenerationFactory_Magic.cs`.

use empyrean_common::dotnet::CsCast;
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_content::models::world::TreasureDeath;
use empyrean_entity::enums::{Skill, UiEffects};
use empyrean_tables::logic::tables::workmanship_chance;

use crate::entity::spell::Spell;
use crate::factories::entity::treasure_roll::TreasureRoll;
use crate::factories::loot_generation_factory::wo_name;
use crate::factories::loot_generation_factory_spells::assign_spells;
use crate::world_objects::world_object::WorldObject;
use crate::World;

/// Spells, then mana (rate, max, current), spellcraft and the activation requirements. ACE's
/// `isArmor` (default false) is unused.
// ACE: LootGenerationFactory.AssignMagic
pub(crate) fn assign_magic(
    w: &World,
    wo: &mut WorldObject,
    profile: &TreasureDeath,
    roll: &mut TreasureRoll,
    _is_armor: bool,
) {
    assign_spells(w, wo, profile, roll);

    wo.set_ui_effects(Some(UiEffects::Magical));

    let max_base_mana = get_max_base_mana(w, wo);

    wo.set_mana_rate(Some(f64::from(calculate_mana_rate(max_base_mana))));

    let mut max_spell_mana = max_base_mana;

    if let Some(spell_did) = wo.spell_did() {
        let spell = Spell::new(w, spell_did, true);

        let castable_mana = CsCast::<i32>::cs_cast(spell.base_mana()).wrapping_mul(5);

        if castable_mana > max_spell_mana {
            max_spell_mana = castable_mana;
        }
    }

    wo.set_item_max_mana(Some(roll_item_max_mana(wo, roll, max_spell_mana)));
    wo.set_item_cur_mana(wo.item_max_mana());

    wo.set_item_spellcraft(Some(roll_spellcraft(w, wo, roll)));

    add_activation_requirements(wo, roll);
}

/// Returns the maximum BaseMana from the spells in item's spellbook
// ACE: LootGenerationFactory.GetMaxBaseMana
fn get_max_base_mana(w: &World, wo: &WorldObject) -> i32 {
    let mut max_base_mana = 0;

    if let Some(book) = wo.biota.properties_spell_book.as_ref() {
        for &spell_id in book.keys() {
            let spell = Spell::from_int(w, spell_id, true);

            let base_mana = spell.base_mana();
            if i64::from(base_mana) > i64::from(max_base_mana) {
                max_base_mana = base_mana.cs_cast();
            }
        }
    }
    max_base_mana
}

/// Rolls the ItemMaxMana for an object
// ACE: LootGenerationFactory.RollItemMaxMana
pub(crate) fn roll_item_max_mana(
    wo: &WorldObject,
    roll: &TreasureRoll,
    max_spell_mana: i32,
) -> i32 {
    // verified matches up with magloot eor logs

    let workmanship =
        workmanship_chance::get_modifier(wo.item_workmanship().map(|v| v.wrapping_sub(1)));

    let range: (i32, i32) =
        if roll.is_clothing() || roll.is_armor() || roll.is_weapon() || roll.is_dinnerware() {
            (6, 15)
        } else if roll.is_jewelry() {
            // includes crowns
            (12, 20)
        } else if roll.is_gem() {
            (1, 1)
        } else {
            log::error!(
                "RollItemMaxMana({}, {}, {max_spell_mana}) - unknown item type",
                wo_name(wo),
                roll.item_type
            );
            return 1;
        };

    let rng = ThreadSafeRandom::next(range.0, range.1);

    #[allow(clippy::cast_precision_loss)]
    let mana = max_spell_mana as f32 * workmanship * rng as f32;
    f64::from(mana).ceil().cs_cast()
}

/// Calculates the ManaRate for an item
// ACE: LootGenerationFactory.CalculateManaRate
fn calculate_mana_rate(mut max_base_mana: i32) -> f32 {
    if max_base_mana <= 0 {
        max_base_mana = 1;
    }

    // verified with eor data
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    let ceiling = f64::from(1200.0f32 / max_base_mana as f32).ceil() as f32;
    -1.0f32 / ceiling
}

// ACE: LootGenerationFactory.RollSpellcraft
fn roll_spellcraft(w: &World, wo: &WorldObject, roll: &TreasureRoll) -> i32 {
    let max_spell_power = get_max_spell_power(w, wo);

    let mut range: (f32, f32) = (1.0, 1.0);

    if roll.is_clothing()
        || roll.is_armor()
        || roll.is_weapon()
        || roll.is_jewelry()
        || roll.is_dinnerware()
    {
        range.0 = 0.9;
        range.1 = 1.1;
    } else if !roll.is_gem() {
        log::error!(
            "RollSpellcraft({}, {}) - unknown item type",
            wo_name(wo),
            roll.item_type
        );
    }

    let rng = ThreadSafeRandom::next_float(range.0, range.1);

    let spellcraft: i32 = (f64::from(max_spell_power) * rng).ceil().cs_cast();

    // retail was capped at 370
    spellcraft.min(370)
}

/// Returns the maximum power from the spells in item's SpellDID / spellbook
// ACE: LootGenerationFactory.GetMaxSpellPower
fn get_max_spell_power(w: &World, wo: &WorldObject) -> i32 {
    let mut max_spell_power = 0;

    if let Some(spell_did) = wo.spell_did() {
        let spell = Spell::new(w, spell_did, true);

        if i64::from(spell.power()) > i64::from(max_spell_power) {
            max_spell_power = spell.power().cs_cast();
        }
    }

    if let Some(book) = wo.biota.properties_spell_book.as_ref() {
        for &spell_id in book.keys() {
            let spell = Spell::from_int(w, spell_id, true);

            if i64::from(spell.power()) > i64::from(max_spell_power) {
                max_spell_power = spell.power().cs_cast();
            }
        }
    }
    max_spell_power
}

// ACE: LootGenerationFactory.AddActivationRequirements
fn add_activation_requirements(wo: &mut WorldObject, roll: &TreasureRoll) {
    // ItemSkill/LevelLimit
    try_mutate_item_skill_limit(wo, roll);

    // Arcane Lore / ItemDifficulty
    wo.set_item_difficulty(Some(calculate_arcane_lore(wo, roll)));
}

// ACE: LootGenerationFactory.TryMutate_ItemSkillLimit
fn try_mutate_item_skill_limit(wo: &mut WorldObject, roll: &TreasureRoll) -> bool {
    if !roll_item_skill_limit(roll) {
        return false;
    }

    wo.set_item_skill_level_limit(wo.item_spellcraft().map(|s| s.wrapping_add(20)));

    let skill;

    if roll.is_melee_weapon() || roll.is_missile_weapon() {
        skill = wo.weapon_skill();
    } else if roll.is_armor() {
        let rng = ThreadSafeRandom::next_float(0.0, 1.0);

        if rng < f64::from(0.5f32) {
            skill = Skill::MeleeDefense;
        } else {
            skill = Skill::MissileDefense;
            #[allow(clippy::cast_precision_loss)]
            let limit = wo
                .item_skill_level_limit()
                .map(|l| (l as f32 * 0.7f32).cs_cast());
            wo.set_item_skill_level_limit(limit);
        }
    } else {
        log::error!(
            "RollItemSkillLimit({}, {}) - unknown item type",
            wo_name(wo),
            roll.item_type
        );
        return false;
    }

    wo.set_item_skill_limit(Some(skill));
    true
}

// ACE: LootGenerationFactory.RollItemSkillLimit
fn roll_item_skill_limit(roll: &TreasureRoll) -> bool {
    if roll.is_melee_weapon() || roll.is_missile_weapon() {
        return true;
    } else if roll.is_armor() {
        let rng = ThreadSafeRandom::next_float(0.0, 1.0);

        return rng < f64::from(0.55f32);
    }
    false
}

/// Calculates the Arcane Lore requirement / ItemDifficulty:
/// `spellcraft - (itemSkillLevelLimit / 2.0f) + creatureLifeEnchantments + cantrips`.
///
/// # Panics
/// As ACE throws `InvalidOperationException`, when the item has no spellcraft.
// ACE: LootGenerationFactory.CalculateArcaneLore
fn calculate_arcane_lore(wo: &WorldObject, roll: &TreasureRoll) -> i32 {
    // spellcraft - (itemSkillLevelLimit / 2.0f) + creatureLifeEnchantments + cantrips

    let spellcraft = wo
        .item_spellcraft()
        .expect("InvalidOperationException: Nullable object must have a value");

    // - mutates 100% of the time for melee / missile weapons
    // - mutates 55% of the time for armor
    // - mutates 0% of the time for all other item types
    let mut item_skill_level_factor = 0.0f32;

    if let Some(limit) = wo.item_skill_level_limit().filter(|&l| l > 0) {
        #[allow(clippy::cast_precision_loss)]
        {
            item_skill_level_factor = limit as f32 / 2.0f32;
        }
    }

    #[allow(clippy::cast_precision_loss)]
    let mut f_arcane = spellcraft as f32 - item_skill_level_factor;

    if f_arcane < 0.0 {
        f_arcane = 0.0;
    }

    f64::from(f_arcane + roll.item_difficulty).floor().cs_cast()
}
