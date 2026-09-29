// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/LootGenerationFactory_Scroll.cs
//! Port of `Source/ACE.Server/Factories/LootGenerationFactory_Scroll.cs`.

use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_content::models::world::TreasureDeath;
use empyrean_entity::enums::SpellId;
use empyrean_tables::logic::spells::scroll_spells;

use crate::factories::entity::treasure_roll::TreasureRoll;
use crate::factories::loot_generation_factory::tables_logic::tables::scroll_level_chance;
use crate::factories::loot_generation_factory::world_object_factory_create_new_world_object;
use crate::world_objects::world_object::WorldObject;
use crate::World;

/// A spell scroll: a level for the tier, then random spells of the scroll table until one exists
/// at that level, then that spell's scroll weenie.
// ACE: LootGenerationFactory.CreateRandomScroll
pub(crate) fn create_random_scroll(
    w: &mut World,
    profile: &TreasureDeath,
    _roll: Option<&TreasureRoll>,
) -> Option<WorldObject> {
    let spell_level = scroll_level_chance::roll(profile);

    // todo: switch to SpellLevelProgression
    let table = &*scroll_spells::TABLE;
    let spell_id = loop {
        let spell_idx = ThreadSafeRandom::next(0, scroll_spells::num_spells() - 1);

        let row = &table[usize::try_from(spell_idx).expect("an index into the table")];
        let spell_id = row[usize::try_from(spell_level - 1).expect("levels 1-7")];
        if spell_id != SpellId::Undef {
            break spell_id; // simple way of handling spells that start at level 3 (blasts, volleys)
        }
    };

    let Some(weenie) = w.content.get_scroll_weenie(spell_id.0) else {
        log::debug!(
            "CreateRandomScroll for tier {} and spellID of {} returned null from the database.",
            profile.tier,
            spell_id.0
        );
        return None;
    };

    world_object_factory_create_new_world_object(w, weenie.weenie_class_id)
}
