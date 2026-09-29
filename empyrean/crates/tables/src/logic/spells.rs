// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/Spells/*.cs

//! The methods of the `Factories/Tables/Spells` classes: the static constructors that expand each
//! class's spell list into per-tier tables.
//!
//! In `empyrean-world`'s `factories::tables_logic` instead: `ArmorSpells.Roll`, `MeleeSpells.Roll`, `MissileSpells.Roll` (they take
//! `TreasureDeath`) and `WandSpells.Roll` (`WorldObject`, `TreasureDeath`).

use empyrean_entity::enums::SpellId;

use crate::logic::tables::spell_level_progression;

/// A class's `Table` and, for the weapon and armor classes, its `CreatureLifeTable`.
#[derive(Debug, Default)]
pub struct SpellTables {
    /// `Table`: row i is spell i's levels, one per tier (all `Undef` if its progression is bad).
    pub table: Vec<Vec<SpellId>>,
    /// `CreatureLifeTable`: the spells not in the class's exclusion list, in list order.
    pub creature_life_table: Vec<SpellId>,
}

/// The body shared by `ArmorSpells`/`JewelrySpells`/`MeleeSpells`/`MissileSpells`/`WandSpells`
/// `.BuildSpells`. `not_creature_life` is the class's `switch` of spells kept out of
/// `CreatureLifeTable`; `None` for `JewelrySpells`, which has no such table.
fn build_spells(
    class: &str,
    spells: &[SpellId],
    num_tiers: i32,
    not_creature_life: Option<&[SpellId]>,
) -> SpellTables {
    let n = usize::try_from(num_tiers).expect("NumTiers");
    let mut out = SpellTables::default();
    for _ in 0..spells.len() {
        out.table.push(vec![SpellId::default(); n]);
    }

    for (i, &spell) in spells.iter().enumerate() {
        let Some(spell_levels) = spell_level_progression::get_spell_levels(spell) else {
            log::error!("{class} - couldn't find {spell}");
            continue;
        };

        if spell_levels.len() != n {
            log::error!(
                "{class} - expected {num_tiers} levels for {spell}, found {}",
                spell_levels.len()
            );
            continue;
        }

        out.table[i][..n].copy_from_slice(&spell_levels[..n]);

        if let Some(excluded) = not_creature_life {
            if !excluded.contains(&spell) {
                out.creature_life_table.push(spell);
            }
        }
    }
    out
}

/// ACE `ArmorSpells`.
pub mod armor_spells {
    use std::sync::LazyLock;

    use empyrean_entity::enums::SpellId;

    use super::SpellTables;
    use crate::tables::spells::armor_spells::{NUM_TIERS, SPELLS};

    /// `Table` and `CreatureLifeTable`, built by the static constructor.
    pub static TABLES: LazyLock<SpellTables> = LazyLock::new(armor_spells);

    // ACE: ArmorSpells.ArmorSpells
    fn armor_spells() -> SpellTables {
        build_spells()
    }

    // ACE: ArmorSpells.BuildSpells
    fn build_spells() -> SpellTables {
        let not_creature_life = [
            SpellId::Impenetrability1,
            SpellId::BladeBane1,
            SpellId::PiercingBane1,
            SpellId::BludgeonBane1,
            SpellId::FlameBane1,
            SpellId::FrostBane1,
            SpellId::AcidBane1,
            SpellId::LightningBane1,
        ];
        super::build_spells("ArmorSpells", &SPELLS, NUM_TIERS, Some(&not_creature_life))
    }
}

/// ACE `GemSpells`.
pub mod gem_spells {
    use std::sync::LazyLock;

    use empyrean_entity::enums::SpellId;

    use crate::logic::tables::spell_level_progression;
    use crate::tables::spells::gem_spells::{CREATURE_SPELLS, LIFE_SPELLS, NUM_TIERS};

    /// `GemCreatureSpellMatrix` and `GemLifeSpellMatrix`: row j is tier j+1, column i is spell i.
    #[derive(Debug)]
    pub struct GemMatrices {
        pub gem_creature_spell_matrix: Vec<Vec<SpellId>>,
        pub gem_life_spell_matrix: Vec<Vec<SpellId>>,
    }

    /// Built by the static constructor.
    pub static MATRICES: LazyLock<GemMatrices> = LazyLock::new(gem_spells);

    // ACE: GemSpells.GemSpells
    fn gem_spells() -> GemMatrices {
        // takes ~0 ms
        let mut gem_creature_spell_matrix = Vec::new();
        let mut gem_life_spell_matrix = Vec::new();
        build_spells(&CREATURE_SPELLS, &mut gem_creature_spell_matrix);
        build_spells(&LIFE_SPELLS, &mut gem_life_spell_matrix);
        GemMatrices {
            gem_creature_spell_matrix,
            gem_life_spell_matrix,
        }
    }

    // ACE: GemSpells.BuildSpells
    fn build_spells(spells: &[SpellId], matrix: &mut Vec<Vec<SpellId>>) {
        let n = usize::try_from(NUM_TIERS).expect("NumTiers");
        for _ in 0..n {
            matrix.push(vec![SpellId::default(); spells.len()]);
        }

        for (i, &spell) in spells.iter().enumerate() {
            let Some(spell_levels) = spell_level_progression::get_spell_levels(spell) else {
                log::error!("GemSpells - couldn't find {spell}");
                continue;
            };

            if spell_levels.len() != n {
                log::error!(
                    "GemSpells - expected {NUM_TIERS} levels for {spell}, found {}",
                    spell_levels.len()
                );
                continue;
            }

            // matrix[j][i] = spellLevels[j] for each tier j
            for (row, &level) in matrix.iter_mut().zip(spell_levels) {
                row[i] = level;
            }
        }
    }
}

/// ACE `JewelrySpells`.
pub mod jewelry_spells {
    use std::sync::LazyLock;

    use empyrean_entity::enums::SpellId;

    use crate::tables::spells::jewelry_spells::{NUM_TIERS, SPELLS};

    /// `Table`, built by the static constructor.
    pub static TABLE: LazyLock<Vec<Vec<SpellId>>> = LazyLock::new(jewelry_spells);

    // ACE: JewelrySpells.JewelrySpells
    fn jewelry_spells() -> Vec<Vec<SpellId>> {
        build_spells()
    }

    // ACE: JewelrySpells.BuildSpells
    fn build_spells() -> Vec<Vec<SpellId>> {
        super::build_spells("JewelrySpells", &SPELLS, NUM_TIERS, None).table
    }
}

/// ACE `MeleeSpells`.
pub mod melee_spells {
    use std::sync::LazyLock;

    use empyrean_entity::enums::SpellId;

    use super::SpellTables;
    use crate::tables::spells::melee_spells::{NUM_TIERS, SPELLS};

    /// `Table` and `CreatureLifeTable`, built by the static constructor.
    pub static TABLES: LazyLock<SpellTables> = LazyLock::new(melee_spells);

    // ACE: MeleeSpells.MeleeSpells
    fn melee_spells() -> SpellTables {
        build_spells()
    }

    // ACE: MeleeSpells.BuildSpells
    fn build_spells() -> SpellTables {
        let not_creature_life = [
            SpellId::BloodDrinkerSelf1,
            SpellId::DefenderSelf1,
            SpellId::HeartSeekerSelf1,
            SpellId::SwiftKillerSelf1,
        ];
        super::build_spells("MeleeSpells", &SPELLS, NUM_TIERS, Some(&not_creature_life))
    }
}

/// ACE `MissileSpells`.
pub mod missile_spells {
    use std::sync::LazyLock;

    use empyrean_entity::enums::SpellId;

    use super::SpellTables;
    use crate::tables::spells::missile_spells::{NUM_TIERS, SPELLS};

    /// `Table` and `CreatureLifeTable`, built by the static constructor.
    pub static TABLES: LazyLock<SpellTables> = LazyLock::new(missile_spells);

    // ACE: MissileSpells.MissileSpells
    fn missile_spells() -> SpellTables {
        build_spells()
    }

    // ACE: MissileSpells.BuildSpells
    fn build_spells() -> SpellTables {
        let not_creature_life = [
            SpellId::BloodDrinkerSelf1,
            SpellId::HeartSeekerSelf1,
            SpellId::DefenderSelf1,
            SpellId::SwiftKillerSelf1,
        ];
        super::build_spells(
            "MissileSpells",
            &SPELLS,
            NUM_TIERS,
            Some(&not_creature_life),
        )
    }
}

/// ACE `WandSpells`.
pub mod wand_spells {
    use std::sync::LazyLock;

    use empyrean_entity::enums::SpellId;

    use super::SpellTables;
    use crate::tables::spells::wand_spells::{NUM_TIERS, SPELLS};

    /// `Table` and `CreatureLifeTable`, built by the static constructor.
    pub static TABLES: LazyLock<SpellTables> = LazyLock::new(wand_spells);

    // ACE: WandSpells.WandSpells
    fn wand_spells() -> SpellTables {
        build_spells()
    }

    // ACE: WandSpells.BuildSpells
    fn build_spells() -> SpellTables {
        let not_creature_life = [
            SpellId::DefenderSelf1,
            SpellId::HermeticLinkSelf1,
            SpellId::SpiritDrinkerSelf1,
        ];
        super::build_spells("WandSpells", &SPELLS, NUM_TIERS, Some(&not_creature_life))
    }
}

/// ACE `ScrollSpells`.
pub mod scroll_spells {
    use std::sync::LazyLock;

    use empyrean_entity::enums::SpellId;

    use crate::logic::count;
    use crate::logic::tables::spell_level_progression;
    use crate::tables::spells::scroll_spells::{
        CREATURE_SPELLS, ITEM_SPELLS, LIFE_SPELLS, MAX_LEVELS, NUM_LEVELS, VOID_SPELLS, WAR_SPELLS,
    };

    /// `NumSpells`: the five lists' counts summed.
    pub fn num_spells() -> i32 {
        count(&CREATURE_SPELLS)
            + count(&LIFE_SPELLS)
            + count(&ITEM_SPELLS)
            + count(&WAR_SPELLS)
            + count(&VOID_SPELLS)
    }

    /// `Table` ("original api"), built by the static constructor: creature, life, item, war and
    /// void spells in that order, `NumLevels` (7) levels each.
    pub static TABLE: LazyLock<Vec<Vec<SpellId>>> = LazyLock::new(scroll_spells);

    // ACE: ScrollSpells.ScrollSpells
    fn scroll_spells() -> Vec<Vec<SpellId>> {
        // ~0.5ms
        build_spells()
    }

    // ACE: ScrollSpells.BuildSpells
    fn build_spells() -> Vec<Vec<SpellId>> {
        let num_spells = num_spells();
        let levels = usize::try_from(NUM_LEVELS).expect("NumLevels");
        let mut table = Vec::new();
        for _ in 0..num_spells {
            table.push(vec![SpellId::default(); levels]);
        }

        let mut start_idx = 0;

        start_idx += add_spells(&mut table, &CREATURE_SPELLS, start_idx);
        start_idx += add_spells(&mut table, &LIFE_SPELLS, start_idx);
        start_idx += add_spells(&mut table, &ITEM_SPELLS, start_idx);
        start_idx += add_spells(&mut table, &WAR_SPELLS, start_idx);
        start_idx += add_spells(&mut table, &VOID_SPELLS, start_idx);

        if start_idx != num_spells {
            log::error!("ScrollSpells - startIdx {start_idx}, expected {num_spells}");
        }
        table
    }

    // ACE: ScrollSpells.AddSpells
    fn add_spells(table: &mut [Vec<SpellId>], spells: &[SpellId], start_idx: i32) -> i32 {
        let start = usize::try_from(start_idx).expect("startIdx");
        let max_levels = usize::try_from(MAX_LEVELS).expect("MaxLevels");
        let levels = usize::try_from(NUM_LEVELS).expect("NumLevels");
        for (i, &spell) in spells.iter().enumerate() {
            let Some(spell_levels) = spell_level_progression::get_spell_levels(spell) else {
                log::error!("ScrollSpells - couldn't find {spell}");
                continue;
            };

            if spell_levels.len() != max_levels {
                log::error!(
                    "ScrollSpells - expected {MAX_LEVELS} levels for {spell}, found {}",
                    spell_levels.len()
                );
                continue;
            }

            table[start + i][..levels].copy_from_slice(&spell_levels[..levels]);
        }
        count(spells)
    }
}
