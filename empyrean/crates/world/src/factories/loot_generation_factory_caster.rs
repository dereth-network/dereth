// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/LootGenerationFactory_Caster.cs
//! Port of `Source/ACE.Server/Factories/LootGenerationFactory_Caster.cs`.

use empyrean_common::dotnet::CsCast;
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_content::models::world::TreasureDeath;
use empyrean_entity::enums::{DamageType, Skill, Usable, WieldRequirement};
use empyrean_tables::enums::{TreasureItemType, TreasureWeaponType};
use empyrean_tables::logic::tables::{spell_level_progression, workmanship_chance};
use empyrean_tables::logic::weapons::caster_wcids;

use crate::entity::mutations::mutation_cache;
use crate::entity::spell::Spell;
use crate::factories::entity::missile_magic_defense;
use crate::factories::entity::treasure_roll::TreasureRoll;
use crate::factories::loot_generation_factory::tables_logic::tables::{
    caster_slot_spells, gem_count_chance,
};
use crate::factories::loot_generation_factory::{
    gem_code, get_long_desc_in, get_material_type, mutate_color, mutate_value, roll_gem_type,
    world_object_factory_create_new_world_object,
};
use crate::factories::loot_generation_factory_magic::assign_magic;
use crate::world_objects::world_object::WorldObject;
use crate::World;

/// This function is only used by test methods, and is not part of regular lootgen.
///
/// # Panics
/// As ACE throws a `NullReferenceException`, when the rolled wcid has no weenie.
// ACE: LootGenerationFactory.CreateCaster
pub fn create_caster(
    w: &mut World,
    profile: &TreasureDeath,
    is_magical: bool,
) -> Option<WorldObject> {
    let mut treasure_roll = TreasureRoll::with_item_type(TreasureItemType::Caster);
    treasure_roll.weapon_type = TreasureWeaponType::Caster;
    treasure_roll.wcid = caster_wcids::roll(profile.tier);

    let mut wo =
        world_object_factory_create_new_world_object(w, treasure_roll.wcid.0.cast_unsigned())
            .expect("NullReferenceException: CreateNewWorldObject returned null");
    mutate_caster(w, &mut wo, profile, is_magical, &mut treasure_roll);

    Some(wo)
}

/// ManaConversionMod, ElementalDamageMod / WieldRequirements and WeaponDefense from the mutation
/// scripts, then material, colour, gems, workmanship, defenses, the SpellDID and spells, value.
///
/// # Panics
/// As ACE throws a `NullReferenceException`, when a mutation script is missing.
// ACE: LootGenerationFactory.MutateCaster
pub(crate) fn mutate_caster(
    w: &World,
    wo: &mut WorldObject,
    profile: &TreasureDeath,
    is_magical: bool,
    roll: &mut TreasureRoll,
) {
    // mutate ManaConversionMod
    let mutation_filter = mutation_cache::get_mutation(w, "Casters.caster.txt")
        .expect("NullReferenceException: no mutation script");
    mutation_filter.try_mutate(wo, profile.tier);

    // mutate ElementalDamageMod / WieldRequirements
    let is_elemental = wo.w_damage_type() != DamageType::Undef;
    // DIVERGE: a caster from an earlier era's tables (`LootRules::Infiltration`) takes that era's
    // elemental script (ClassicACE's `GetCasterScript` at its Infiltration ruleset).
    // Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/Factories/LootGenerationFactory_Caster.cs
    let script_name = if roll.era_script.is_some() {
        format!(
            "Casters.Infiltration.caster_{}.txt",
            if is_elemental {
                "elemental"
            } else {
                "non_elemental"
            }
        )
    } else {
        get_caster_script(is_elemental)
    };

    let mutation_filter = mutation_cache::get_mutation(w, &script_name)
        .expect("NullReferenceException: no mutation script");
    mutation_filter.try_mutate(wo, profile.tier);

    // this part was not handled by mutation filter
    if wo.wield_requirements() == WieldRequirement::RawSkill {
        if wo.w_damage_type() == DamageType::Nether {
            wo.set_wield_skill_type(Some(Skill::VoidMagic.0));
        } else {
            wo.set_wield_skill_type(Some(Skill::WarMagic.0));
        }
    }

    // mutate WeaponDefense
    let mutation_filter = mutation_cache::get_mutation(w, "Casters.weapon_defense.txt")
        .expect("NullReferenceException: no mutation script");
    mutation_filter.try_mutate(wo, profile.tier);

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

    // burden?

    // missile defense / magic defense
    // DIVERGE: an earlier era's caster rolls neither (ClassicACE at its Infiltration ruleset).
    if roll.era_script.is_none() {
        wo.set_weapon_missile_defense(missile_magic_defense::roll(profile.tier).map(f64::from));
        wo.set_weapon_magic_defense(missile_magic_defense::roll(profile.tier).map(f64::from));
    }

    // spells
    if is_magical {
        // if a caster was from a MagicItem profile, it always had a SpellDID
        mutate_caster_spell_did(w, wo, profile);

        assign_magic(w, wo, profile, roll, false);
    } else {
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
    wo.set_long_desc(get_long_desc_in(w.era.loot_rules, wo));
}

/// The caster's slot spell at a rolled level, its mana cost and its useability.
// ACE: LootGenerationFactory.MutateCaster_SpellDID
fn mutate_caster_spell_did(w: &World, wo: &mut WorldObject, profile: &TreasureDeath) {
    let first_spell = caster_slot_spells::roll(wo);

    let Some(spell_levels) = spell_level_progression::get_spell_levels(first_spell) else {
        log::error!("MutateCaster_SpellDID: couldn't find {first_spell}");
        return;
    };

    if spell_levels.len() != 8 {
        log::error!(
            "MutateCaster_SpellDID: found {} spell levels for {first_spell}, expected 8",
            spell_levels.len()
        );
        return;
    }

    let spell_level =
        crate::factories::loot_generation_factory_spells::roll_spell_level(w, profile.tier);

    let spell_did = spell_levels[usize::try_from(spell_level - 1).expect("levels 1-8")].0;
    wo.set_spell_did(Some(spell_did));

    let spell = Spell::new(w, spell_did, true);

    let castable_mod = if caster_slot_spells::is_orb(wo) {
        5.0f32
    } else {
        2.5f32
    };

    let base_mana: f32 = spell.base_mana().cs_cast();
    wo.set_item_mana_cost(Some((base_mana * castable_mod).cs_cast()));

    wo.set_item_useable(Some(Usable::SourceWieldedTargetRemoteNeverWalk));
}

// ACE: LootGenerationFactory.GetCasterScript
fn get_caster_script(is_elemental: bool) -> String {
    let elemental_str = if is_elemental {
        "elemental"
    } else {
        "non_elemental"
    };

    format!("Casters.caster_{elemental_str}.txt")
}
