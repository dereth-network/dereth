// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/LootGenerationFactory_Gem.cs
//! Port of `Source/ACE.Server/Factories/LootGenerationFactory_Gem.cs`.

use empyrean_common::dotnet::CsCast;
use empyrean_content::models::world::TreasureDeath;
use empyrean_entity::enums::{MutateFilter, UiEffects, Usable};
use empyrean_tables::enums::TreasureItemType;
use empyrean_tables::logic::tables::{
    gem_class_chance, gem_material_chance, material_table, spell_level_progression,
    workmanship_chance,
};

use crate::entity::spell::Spell;
use crate::factories::entity::treasure_roll::TreasureRoll;
use crate::factories::loot_generation_factory::{
    get_long_desc_in, has_mutate_filter, mutate_color, mutate_value, wo_name,
    world_object_factory_create_new_world_object,
};
use crate::factories::loot_generation_factory_magic::roll_item_max_mana;
use crate::world_objects::world_object::WorldObject;
use crate::World;

/// This is only called by /testlootgen command. The actual lootgen system doesn't use this.
///
/// # Panics
/// As ACE throws a `NullReferenceException`, when the rolled wcid has no weenie.
// ACE: LootGenerationFactory.CreateGem
pub(crate) fn create_gem(
    w: &mut World,
    profile: &TreasureDeath,
    is_magical: bool,
) -> Option<WorldObject> {
    let mut treasure_roll = TreasureRoll::with_item_type(TreasureItemType::Gem);

    let gem_class = gem_class_chance::roll(profile.tier);
    let gem_result = gem_material_chance::roll(gem_class);

    treasure_roll.wcid = gem_result.class_name;

    let mut wo =
        world_object_factory_create_new_world_object(w, treasure_roll.wcid.0.cast_unsigned())
            .expect("NullReferenceException: CreateNewWorldObject returned null");

    mutate_gem(w, &mut wo, profile, is_magical, &mut treasure_roll);

    Some(wo)
}

/// Workmanship and colour, then a gem spell (magical) or cleared mana properties, value.
// ACE: LootGenerationFactory.MutateGem
pub(crate) fn mutate_gem(
    w: &World,
    wo: &mut WorldObject,
    profile: &TreasureDeath,
    is_magical: bool,
    roll: &mut TreasureRoll,
) {
    // workmanship
    wo.set_item_workmanship(Some(workmanship_chance::roll(profile.tier)));

    // item color
    mutate_color(w, wo);

    if is_magical {
        assign_magic_gem(w, wo, profile, roll);

        wo.set_ui_effects(Some(UiEffects::Magical));
        wo.set_item_useable(Some(Usable::Contained));
    } else {
        // TODO: verify if this is needed
        wo.set_item_useable(Some(Usable::No));
        wo.set_spell_did(None);
        wo.set_item_mana_cost(None);
        wo.set_item_max_mana(None);
        wo.set_item_cur_mana(None);
        wo.set_item_spellcraft(None);
        wo.set_item_difficulty(None);
        wo.set_item_skill_level_limit(None);
        wo.set_mana_rate(None);
    }

    // item value
    if has_mutate_filter(wo, MutateFilter::Value) {
        mutate_value(w, wo, profile.tier, Some(roll));
    }

    // long desc
    wo.set_long_desc(get_long_desc_in(w.era.loot_rules, wo));
}

/// A spell from selection code 1 at a rolled level, its spellcraft, mana and mana cost.
// ACE: LootGenerationFactory.AssignMagic_Gem
fn assign_magic_gem(
    w: &World,
    wo: &mut WorldObject,
    profile: &TreasureDeath,
    roll: &TreasureRoll,
) -> bool {
    // TODO: move to standard AssignMagic() pipeline

    let spell = crate::factories::loot_generation_factory_spells::roll_spell_selection(w, 1);

    let spell_level =
        crate::factories::loot_generation_factory_spells::roll_spell_level(w, profile.tier);

    let spell_levels = spell_level_progression::get_spell_levels(spell);

    let Some(spell_levels) = spell_levels.filter(|l| l.len() == 8) else {
        log::error!(
            "AssignMagic_Gem({}, {}, {}) - unknown spell {spell}",
            wo_name(wo),
            profile.treasure_type,
            roll.item_type
        );
        return false;
    };

    let final_spell_id = spell_levels[usize::try_from(spell_level - 1).expect("levels 1-8")];

    wo.set_spell_did(Some(final_spell_id.0));

    let spell = Spell::from_spell_id(w, final_spell_id, true);

    // retail spellcraft was capped at 370
    wo.set_item_spellcraft(Some(CsCast::<i32>::cs_cast(spell.power()).min(370)));

    let castable_mana = CsCast::<i32>::cs_cast(spell.base_mana()).wrapping_mul(5);

    wo.set_item_max_mana(Some(roll_item_max_mana(wo, roll, castable_mana)));
    wo.set_item_cur_mana(wo.item_max_mana());

    // verified
    wo.set_item_mana_cost(Some(castable_mana));

    true
}

/// `Value = (int)(Value * materialMod * workmanshipMod)`.
// ACE: LootGenerationFactory.MutateValue_Gem
pub(crate) fn mutate_value_gem(wo: &mut WorldObject) {
    let material_mod = material_table::get_value_mod(wo.material_type());

    let workmanship_mod = workmanship_chance::get_modifier(wo.item_workmanship());

    #[allow(clippy::cast_precision_loss)]
    let value = wo
        .value()
        .map(|v| (v as f32 * material_mod * workmanship_mod).cs_cast());
    wo.set_value(value);
}
