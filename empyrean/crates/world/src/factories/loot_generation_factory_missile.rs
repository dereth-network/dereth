// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/LootGenerationFactory_Missile.cs
//! Port of `Source/ACE.Server/Factories/LootGenerationFactory_Missile.cs`.

use empyrean_common::dotnet::CsCast;
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_content::models::world::TreasureDeath;
use empyrean_entity::enums::DamageType;
use empyrean_tables::enums::{TreasureItemType, TreasureWeaponType};
use empyrean_tables::logic::tables::workmanship_chance;
use empyrean_tables::tables::weapon_type_chance::MISSILE_CHANCES;

use crate::entity::mutations::mutation_cache;
use crate::factories::entity::missile_magic_defense;
use crate::factories::entity::treasure_roll::TreasureRoll;
use crate::factories::loot_generation_factory::tables_logic::tables::gem_count_chance;
use crate::factories::loot_generation_factory::tables_logic::wcids::weapon_wcids;
use crate::factories::loot_generation_factory::{
    gem_code, get_long_desc_in, get_material_type, mutate_burden, mutate_color, mutate_value,
    roll_gem_type, world_object_factory_create_new_world_object,
};
use crate::factories::loot_generation_factory_magic::assign_magic;
use crate::factories::loot_generation_factory_weapon::roll_weapon_speed_mod;
use crate::world_objects::world_object::WorldObject;
use crate::World;

/// This function is only used by test methods, and is not part of regular lootgen. ACE's
/// `mutate` parameter (default true) is unused.
///
/// # Panics
/// As ACE throws a `NullReferenceException`, when the rolled wcid has no weenie.
// ACE: LootGenerationFactory.CreateMissileWeapon
pub fn create_missile_weapon(
    w: &mut World,
    profile: &TreasureDeath,
    is_magical: bool,
    _mutate: bool,
) -> Option<WorldObject> {
    let mut treasure_roll = TreasureRoll::with_item_type(TreasureItemType::Weapon);
    treasure_roll.weapon_type = MISSILE_CHANCES.roll(0.0);
    treasure_roll.wcid = weapon_wcids::roll(profile, &mut treasure_roll.weapon_type);

    let mut wo =
        world_object_factory_create_new_world_object(w, treasure_roll.wcid.0.cast_unsigned())
            .expect("NullReferenceException: CreateNewWorldObject returned null");

    mutate_missile_weapon(w, &mut wo, profile, is_magical, &mut treasure_roll);

    Some(wo)
}

/// DamageMod / ElementalDamageBonus / WieldRequirements and WeaponDefense from the mutation
/// scripts, then speed, material, colour, gems, workmanship, burden, defenses, spells, value.
///
/// # Panics
/// As ACE throws a `NullReferenceException`, when a mutation script is missing.
// ACE: LootGenerationFactory.MutateMissileWeapon
pub(crate) fn mutate_missile_weapon(
    w: &World,
    wo: &mut WorldObject,
    profile: &TreasureDeath,
    is_magical: bool,
    roll: &mut TreasureRoll,
) {
    // new method / mutation scripts
    let is_elemental = wo.w_damage_type() != DamageType::Undef;

    // DIVERGE: a weapon from an earlier era's tables (`LootRules::Infiltration`) takes that era's
    // script for its kind (`bow_short`, `crossbow_light`, `atlatl_regular`, ...), ClassicACE's
    // `GetMissileScript` at its Infiltration ruleset.
    // Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/Factories/LootGenerationFactory_Missile.cs
    let script_name = match roll.era_script {
        Some(script) => format!(
            "MissileWeapons.Infiltration.{script}_{}.txt",
            if is_elemental {
                "elemental"
            } else {
                "non_elemental"
            }
        ),
        None => get_missile_script(roll.weapon_type, is_elemental),
    };

    // mutate DamageMod / ElementalDamageBonus / WieldRequirements
    let mutation_filter = mutation_cache::get_mutation(w, &script_name)
        .expect("NullReferenceException: no mutation script");

    mutation_filter.try_mutate(wo, profile.tier);

    // mutate WeaponDefense
    let mutation_filter = mutation_cache::get_mutation(w, "MissileWeapons.weapon_defense.txt")
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
    // DIVERGE: an earlier era's weapon rolls neither (ClassicACE at its Infiltration ruleset).
    if roll.era_script.is_none() {
        wo.set_weapon_missile_defense(missile_magic_defense::roll(profile.tier).map(f64::from));
        wo.set_weapon_magic_defense(missile_magic_defense::roll(profile.tier).map(f64::from));
    }

    // spells
    if is_magical {
        assign_magic(w, wo, profile, roll, false);
    } else {
        wo.set_item_mana_cost(None);
        wo.set_item_max_mana(None);
        wo.set_item_cur_mana(None);
        wo.set_item_spellcraft(None);
        wo.set_item_difficulty(None);
        wo.set_mana_rate(None);
    }

    // item value
    //if (wo.HasMutateFilter(MutateFilter.Value))   // fixme: data
    mutate_value(w, wo, profile.tier, Some(roll));

    // long description
    wo.set_long_desc(get_long_desc_in(w.era.loot_rules, wo));
}

// ACE: LootGenerationFactory.GetMissileScript
fn get_missile_script(weapon_type: TreasureWeaponType, is_elemental: bool) -> String {
    let elemental_str = if is_elemental {
        "elemental"
    } else {
        "non_elemental"
    };

    format!(
        "MissileWeapons.{}_{elemental_str}.txt",
        weapon_type.get_script_name().unwrap_or_default()
    )
}
