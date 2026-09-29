// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/LootGenerationFactory_Jewelry.cs
//! Port of `Source/ACE.Server/Factories/LootGenerationFactory_Jewelry.cs`.

use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_content::models::world::TreasureDeath;
use empyrean_tables::enums::TreasureItemType;
use empyrean_tables::logic::tables::workmanship_chance;
use empyrean_tables::logic::wcids::jewelry_wcids;

use crate::factories::entity::treasure_roll::TreasureRoll;
use crate::factories::loot_generation_factory::tables_logic::tables::gem_count_chance;
use crate::factories::loot_generation_factory::{
    gem_code, get_long_desc, get_material_type, mutate_color, mutate_value, roll_gem_type,
    roll_wield_level_req_t7_t8, world_object_factory_create_new_world_object,
};
use crate::factories::loot_generation_factory_clothing::try_mutate_gear_rating;
use crate::factories::loot_generation_factory_magic::assign_magic;
use crate::world_objects::world_object::WorldObject;
use crate::World;

/// This is only called by /testlootgen command. The actual lootgen system doesn't use this.
///
/// # Panics
/// As ACE throws a `NullReferenceException`, when the rolled wcid has no weenie.
// ACE: LootGenerationFactory.CreateJewelry
pub(crate) fn create_jewelry(
    w: &mut World,
    profile: &TreasureDeath,
    is_magical: bool,
) -> Option<WorldObject> {
    let mut treasure_roll = TreasureRoll::with_item_type(TreasureItemType::Jewelry);
    treasure_roll.wcid = jewelry_wcids::roll(profile.tier);

    let mut wo =
        world_object_factory_create_new_world_object(w, treasure_roll.wcid.0.cast_unsigned())
            .expect("NullReferenceException: CreateNewWorldObject returned null");

    mutate_jewelry(w, &mut wo, profile, is_magical, &mut treasure_roll);

    Some(wo)
}

/// Material, colour, gems, workmanship, the T7+ level requirement, spells, the T8 gear rating,
/// value.
// ACE: LootGenerationFactory.MutateJewelry
pub(crate) fn mutate_jewelry(
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
        wo.set_gem_count(Some(ThreadSafeRandom::next(1, 5)));
    }

    wo.set_gem_type(Some(roll_gem_type(profile.tier)));

    // workmanship
    wo.set_item_workmanship(Some(workmanship_chance::roll(profile.tier)));

    // wield level requirement for t7+
    if profile.tier > 6 {
        roll_wield_level_req_t7_t8(wo, profile);
    }

    // assign magic
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

    // gear rating (t8)
    if profile.tier == 8 {
        try_mutate_gear_rating(wo, profile, roll);
    }

    // item value
    //  if (wo.HasMutateFilter(MutateFilter.Value))     // fixme: data
    mutate_value(w, wo, profile.tier, Some(roll));

    wo.set_long_desc(get_long_desc(wo));
}
