// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/LootGenerationFactory_Melee.cs
//! Port of `Source/ACE.Server/Factories/LootGenerationFactory_Melee.cs`.

use empyrean_common::dotnet::CsCast;
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_content::models::world::TreasureDeath;
use empyrean_tables::enums::ext::to_melee_weapon_skill;
use empyrean_tables::enums::{
    MeleeWeaponSkill, TreasureItemType, TreasureWeaponType, WeenieClassName,
};
use empyrean_tables::logic::tables::workmanship_chance;
use empyrean_tables::tables::weapon_type_chance::MELEE_CHANCES;

use crate::entity::mutations::mutation_cache;
use crate::factories::entity::missile_magic_defense;
use crate::factories::entity::treasure_roll::TreasureRoll;
use crate::factories::loot_generation_factory::tables_logic::tables::gem_count_chance;
use crate::factories::loot_generation_factory::tables_logic::wcids::weapon_wcids;
use crate::factories::loot_generation_factory::{
    gem_code, get_long_desc, get_material_type, mutate_burden, mutate_color, mutate_value,
    roll_gem_type, world_object_factory_create_new_world_object,
};
use crate::factories::loot_generation_factory_magic::assign_magic;
use crate::factories::loot_generation_factory_weapon::roll_weapon_speed_mod;
use crate::world_objects::world_object::WorldObject;
use crate::World;

/// This function is only used by test methods, and is not part of regular lootgen.
///
/// # Panics
/// As ACE throws a `NullReferenceException`, when the rolled wcid has no weenie.
// ACE: LootGenerationFactory.CreateMeleeWeapon
pub fn create_melee_weapon(
    w: &mut World,
    profile: &TreasureDeath,
    is_magical: bool,
) -> Option<WorldObject> {
    let mut treasure_roll = TreasureRoll::with_item_type(TreasureItemType::Weapon);
    treasure_roll.weapon_type = MELEE_CHANCES.roll(0.0);
    treasure_roll.wcid = weapon_wcids::roll(profile, &mut treasure_roll.weapon_type);

    let mut wo =
        world_object_factory_create_new_world_object(w, treasure_roll.wcid.0.cast_unsigned())
            .expect("NullReferenceException: CreateNewWorldObject returned null");

    mutate_melee_weapon(w, &mut wo, profile, is_magical, &mut treasure_roll);

    Some(wo)
}

/// Damage / wield difficulty / variance and offense / defense from the mutation scripts, then
/// speed, material, colour, gems, workmanship, burden, missile and magic defense, spells, value.
///
/// # Panics
/// As ACE throws a `NullReferenceException`, when a mutation script is missing.
// ACE: LootGenerationFactory.MutateMeleeWeapon
pub(crate) fn mutate_melee_weapon(
    w: &World,
    wo: &mut WorldObject,
    profile: &TreasureDeath,
    is_magical: bool,
    roll: &mut TreasureRoll,
) {
    // thanks to 4eyebiped for helping with the data analysis of magloot retail logs
    // that went into reversing these mutation scripts

    let mut weapon_skill = to_melee_weapon_skill(wo.weapon_skill());
    // DIVERGE: under the era's `LootTables::PackOnly` rule a weapon whose weenie still carries a
    // skill retired in 2012 (Axe, Sword, ...) is mutated as the loot table that named it (heavy,
    // light, finesse or two-handed); ACE finds no mutation script for it and throws.
    if weapon_skill == MeleeWeaponSkill::Undef
        && w.era.loot == empyrean_common::era::LootTables::PackOnly
    {
        weapon_skill = melee_table_skill(WeenieClassName(wo.weenie_class_id().cast_signed()));
    }

    // mutate Damage / WieldDifficulty / Variance
    let script_name = get_damage_script(weapon_skill, roll.weapon_type);

    let mutation_filter = mutation_cache::get_mutation(w, &script_name)
        .expect("NullReferenceException: no mutation script");

    mutation_filter.try_mutate(wo, profile.tier);

    // mutate WeaponOffense / WeaponDefense
    let script_name = get_offense_defense_script(weapon_skill, roll.weapon_type);

    let mutation_filter = mutation_cache::get_mutation(w, &script_name)
        .expect("NullReferenceException: no mutation script");

    mutation_filter.try_mutate(wo, profile.tier);

    // weapon speed
    if let Some(weapon_time) = wo.weapon_time() {
        let weapon_speed_mod = roll_weapon_speed_mod(profile);
        #[allow(clippy::cast_precision_loss)]
        wo.set_weapon_time(Some((weapon_time as f32 * weapon_speed_mod).cs_cast()));
    }

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
        wo.set_gem_count(Some(ThreadSafeRandom::next(1, 5)));
    }

    wo.set_gem_type(Some(roll_gem_type(profile.tier)));

    // workmanship
    wo.set_item_workmanship(Some(workmanship_chance::roll(profile.tier)));

    // burden
    mutate_burden(wo, profile, true);

    // missile / magic defense
    wo.set_weapon_missile_defense(missile_magic_defense::roll(profile.tier).map(f64::from));
    wo.set_weapon_magic_defense(missile_magic_defense::roll(profile.tier).map(f64::from));

    // spells
    if is_magical {
        assign_magic(w, wo, profile, roll, false);
    } else {
        // clear base
        wo.set_item_mana_cost(None);
        wo.set_item_max_mana(None);
        wo.set_item_cur_mana(None);
        wo.set_item_spellcraft(None);
        wo.set_item_difficulty(None);
    }

    // item value
    //if (wo.HasMutateFilter(MutateFilter.Value))   // fixme: data
    mutate_value(w, wo, profile.tier, Some(roll));

    // long description
    wo.set_long_desc(get_long_desc(wo));
}

// ACE: LootGenerationFactory.GetDamageScript
fn get_damage_script(weapon_skill: MeleeWeaponSkill, weapon_type: TreasureWeaponType) -> String {
    format!(
        "MeleeWeapons.Damage_WieldDifficulty_DamageVariance.{}_{}.txt",
        weapon_skill.get_script_name_combined().unwrap_or_default(),
        weapon_type.get_script_name().unwrap_or_default()
    )
}

/// Not ACE: the melee skill of the loot table that names `wcid`; `Undef` when none does.
fn melee_table_skill(wcid: WeenieClassName) -> MeleeWeaponSkill {
    use empyrean_tables::logic::weapons::{
        finesse_weapon_wcids, heavy_weapon_wcids, light_weapon_wcids, two_handed_weapon_wcids,
    };
    if heavy_weapon_wcids::try_get_value(wcid).is_some() {
        MeleeWeaponSkill::HeavyWeapons
    } else if light_weapon_wcids::try_get_value(wcid).is_some() {
        MeleeWeaponSkill::LightWeapons
    } else if finesse_weapon_wcids::try_get_value(wcid).is_some() {
        MeleeWeaponSkill::FinesseWeapons
    } else if two_handed_weapon_wcids::try_get_value(wcid).is_some() {
        MeleeWeaponSkill::TwoHandedCombat
    } else {
        MeleeWeaponSkill::Undef
    }
}

// ACE: LootGenerationFactory.GetOffenseDefenseScript
fn get_offense_defense_script(
    _weapon_skill: MeleeWeaponSkill,
    weapon_type: TreasureWeaponType,
) -> String {
    format!(
        "MeleeWeapons.WeaponOffense_WeaponDefense.{}_offense_defense.txt",
        weapon_type.get_script_short_name().unwrap_or_default()
    )
}
