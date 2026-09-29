// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/Spells/ArmorSpells.cs, Source/ACE.Server/Factories/Tables/Spells/MeleeSpells.cs, Source/ACE.Server/Factories/Tables/Spells/MissileSpells.cs, Source/ACE.Server/Factories/Tables/Spells/WandSpells.cs
//! The `Roll` members of the `Factories/Tables/Spells` classes (see `empyrean_tables::logic::spells`
//! for their static constructors). Each item spell of the class's list is kept when one
//! `NextInterval(LootQualityMod)` draw is below its chance.

use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_content::models::world::TreasureDeath;
use empyrean_entity::enums::SpellId;

/// The shared body of the four `Roll`s; `skip` is WandSpells' extra filter.
fn roll_list(
    list: &[(SpellId, f32)],
    treasure_death: &TreasureDeath,
    skip: impl Fn(SpellId) -> bool,
) -> Vec<SpellId> {
    let mut spells = Vec::new();

    for &(spell_id, chance) in list {
        if skip(spell_id) {
            continue;
        }

        let rng = ThreadSafeRandom::next_interval(treasure_death.loot_quality_mod);

        if rng < f64::from(chance) {
            spells.push(spell_id);
        }
    }

    spells
}

/// ACE `ArmorSpells`.
pub mod armor_spells {
    use empyrean_content::models::world::TreasureDeath;
    use empyrean_entity::enums::SpellId;
    use empyrean_tables::tables::spells::armor_spells::ARMOR_SPELLS;

    /// This roll also applies to clothing w/ AL: shirts and pants would never have item spells on
    /// them, but cloth gloves would.
    // ACE: ArmorSpells.Roll
    #[must_use]
    pub fn roll(treasure_death: &TreasureDeath) -> Vec<SpellId> {
        // thanks to Sapphire Knight and Butterflygolem for helping to figure this part out!
        super::roll_list(&ARMOR_SPELLS, treasure_death, |_| false)
    }
}

/// ACE `MeleeSpells`.
pub mod melee_spells {
    use empyrean_content::models::world::TreasureDeath;
    use empyrean_entity::enums::SpellId;
    use empyrean_tables::tables::spells::melee_spells::WEAPON_MELEE_SPELLS;

    // ACE: MeleeSpells.Roll
    #[must_use]
    pub fn roll(treasure_death: &TreasureDeath) -> Vec<SpellId> {
        super::roll_list(&WEAPON_MELEE_SPELLS, treasure_death, |_| false)
    }
}

/// ACE `MissileSpells`.
pub mod missile_spells {
    use empyrean_content::models::world::TreasureDeath;
    use empyrean_entity::enums::SpellId;
    use empyrean_tables::tables::spells::missile_spells::WEAPON_MISSILE_SPELLS;

    // ACE: MissileSpells.Roll
    #[must_use]
    pub fn roll(treasure_death: &TreasureDeath) -> Vec<SpellId> {
        super::roll_list(&WEAPON_MISSILE_SPELLS, treasure_death, |_| false)
    }
}

/// ACE `WandSpells`.
pub mod wand_spells {
    use empyrean_content::models::world::TreasureDeath;
    use empyrean_entity::enums::{DamageType, SpellId};
    use empyrean_tables::tables::spells::wand_spells::WAND_SPELLS;

    use crate::world_objects::world_object::WorldObject;

    /// Spirit Drinker is skipped (with no draw) on a caster without an elemental damage type.
    // ACE: WandSpells.Roll
    #[must_use]
    pub fn roll(wo: &WorldObject, treasure_death: &TreasureDeath) -> Vec<SpellId> {
        let w_damage_type = wo.w_damage_type();
        // retail didn't have this logic, but...
        super::roll_list(&WAND_SPELLS, treasure_death, |spell_id| {
            spell_id == SpellId::SpiritDrinkerSelf1 && w_damage_type == DamageType::Undef
        })
    }
}
