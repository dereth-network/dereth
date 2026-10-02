// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/LootGenerationFactory_Dinnerware.cs
//! Port of `Source/ACE.Server/Factories/LootGenerationFactory_Dinnerware.cs`.

use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_content::models::world::TreasureDeath;
use empyrean_entity::enums::MutateFilter;
use empyrean_tables::enums::{TreasureItemType, WeenieClassName};
use empyrean_tables::logic::tables::workmanship_chance;
use empyrean_tables::logic::wcids::generic_wcids;

use crate::factories::entity::treasure_roll::TreasureRoll;
use crate::factories::loot_generation_factory::tables_logic::tables::gem_count_chance;
use crate::factories::loot_generation_factory::{
    gem_code, get_long_desc_in, get_material_type, has_mutate_filter, mutate_color, mutate_value,
    roll_gem_type, world_object_factory_create_new_world_object,
};
use crate::factories::loot_generation_factory_magic::assign_magic;
use crate::world_objects::world_object::WorldObject;
use crate::World;

/// This is only called by /testlootgen command. The actual lootgen system doesn't use this.
///
/// # Panics
/// As ACE throws a `NullReferenceException`, when the rolled wcid has no weenie.
// ACE: LootGenerationFactory.CreateDinnerware
pub(crate) fn create_dinnerware(
    w: &mut World,
    profile: &TreasureDeath,
    is_magical: bool,
) -> Option<WorldObject> {
    let mut treasure_roll = TreasureRoll::with_item_type(TreasureItemType::ArtObject);
    treasure_roll.wcid = generic_wcids::roll(profile.tier);

    let mut wo =
        world_object_factory_create_new_world_object(w, treasure_roll.wcid.0.cast_unsigned())
            .expect("NullReferenceException: CreateNewWorldObject returned null");
    mutate_dinnerware(w, &mut wo, profile, is_magical, &mut treasure_roll);

    Some(wo)
}

/// Material, colour, gems, workmanship, spells (not on the empty flask), value.
// ACE: LootGenerationFactory.MutateDinnerware
pub(crate) fn mutate_dinnerware(
    w: &World,
    wo: &mut WorldObject,
    profile: &TreasureDeath,
    is_magical: bool,
    roll: &mut TreasureRoll,
) {
    // dinnerware did not have its Damage / DamageVariance / WeaponSpeed mutated

    // material type
    wo.set_material_type(Some(get_material_type(w, wo, profile.tier)));

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

    // "Empty Flask" was the only dinnerware that never received spells
    if is_magical
        && i64::from(wo.biota.weenie_class_id) != i64::from(WeenieClassName::flasksimple.0)
    {
        assign_magic(w, wo, profile, roll, false);
    }

    // item value
    if has_mutate_filter(wo, MutateFilter::Value) {
        mutate_value(w, wo, profile.tier, Some(roll));
    }

    // long desc
    wo.set_long_desc(get_long_desc_in(w.era.loot_rules, wo));
}
