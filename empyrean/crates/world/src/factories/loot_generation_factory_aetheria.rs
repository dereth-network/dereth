// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/LootGenerationFactory_Aetheria.cs
//! Port of `Source/ACE.Server/Factories/LootGenerationFactory_Aetheria.cs`.

use empyrean_content::models::world::TreasureDeath;
use empyrean_tables::logic::wcids::aetheria_wcids;

use crate::factories::loot_generation_factory::tables_logic::tables::aetheria_chance;
use crate::factories::loot_generation_factory::tables_logic::wcids::coalesced_mana_wcids;
use crate::factories::loot_generation_factory::world_object_factory_create_new_world_object;
use crate::world_objects::world_object::WorldObject;
use crate::World;

/// An aetheria of a random colour, mutated unless `mutate` is false.
///
/// # Panics
/// As ACE throws a `NullReferenceException`, when the wcid has no weenie and `mutate` is set.
// ACE: LootGenerationFactory.CreateAetheria
pub(crate) fn create_aetheria(
    w: &mut World,
    profile: &TreasureDeath,
    mutate: bool,
) -> Option<WorldObject> {
    let wcid = aetheria_wcids::roll(profile.tier);

    let mut wo = world_object_factory_create_new_world_object(w, wcid.0.cast_unsigned());

    if mutate {
        mutate_aetheria(
            wo.as_mut()
                .expect("NullReferenceException: CreateNewWorldObject returned null"),
            profile,
        );
    }

    wo
}

/// The item max level for the tier and its icon overlay.
// ACE: LootGenerationFactory.MutateAetheria
pub(crate) fn mutate_aetheria(wo: &mut WorldObject, profile: &TreasureDeath) {
    wo.set_item_max_level(Some(aetheria_chance::roll_item_max_level(profile)));

    let item_max_level = wo.item_max_level().expect("set above");
    wo.set_icon_overlay_id(Some(
        crate::factories::loot_generation_factory::tables_logic::at(
            &ICON_OVERLAY_ITEM_MAX_LEVEL,
            item_max_level - 1,
        ),
    ));
}

// ACE: LootGenerationFactory.CreateCoalescedMana
pub(crate) fn create_coalesced_mana(w: &mut World, profile: &TreasureDeath) -> Option<WorldObject> {
    let wcid = coalesced_mana_wcids::roll(profile);

    world_object_factory_create_new_world_object(w, wcid.0.cast_unsigned())
}

/// `IconOverlay_ItemMaxLevel`: the overlay icon for item max levels 1-5.
/// (ACE: `LootGenerationFactory.IconOverlay_ItemMaxLevel`, an auto-property.)
pub static ICON_OVERLAY_ITEM_MAX_LEVEL: [u32; 5] = [
    0x6006C34, // 1
    0x6006C35, // 2
    0x6006C36, // 3
    0x6006C37, // 4
    0x6006C38, // 5
];
