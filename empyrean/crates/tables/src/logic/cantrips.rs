// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/Cantrips/*.cs

//! The methods of the `Factories/Tables/Cantrips` classes.
//!
//! In `empyrean-world`'s `factories::tables_logic` instead: `CantripChance.RollNumCantrips`/`RollCantripLevel` and
//! `NumCantrips.RollNumCantrips`/`RollCantripLevel` (they take `TreasureDeath`), and
//! `CantripChance`'s static constructor, `ApplyNumCantripsMod`, `ApplyCantripLevelsMod`,
//! `ShowTables` and `GetPercent` (they read `PropertyManager` and hold the rescaled tables).

use empyrean_entity::enums::SpellId;

use crate::logic::tables::spell_level_progression;

/// The body shared by every cantrip class's `BuildSpells`: each spell's `num_levels` levels, or a
/// row of `Undef` (and an error in the log) when its progression is missing or the wrong length.
fn build_spells(class: &str, spells: &[SpellId], num_levels: i32) -> Vec<Vec<SpellId>> {
    let n = usize::try_from(num_levels).expect("NumLevels");
    let mut table: Vec<Vec<SpellId>> = Vec::with_capacity(spells.len());
    for _ in 0..spells.len() {
        table.push(vec![SpellId::default(); n]);
    }

    for (i, &spell) in spells.iter().enumerate() {
        let Some(spell_levels) = spell_level_progression::get_spell_levels(spell) else {
            log::error!("{class} - couldn't find {spell}");
            continue;
        };

        if spell_levels.len() != n {
            log::error!(
                "{class} - expected {num_levels} levels for {spell}, found {}",
                spell_levels.len()
            );
            continue;
        }

        table[i][..n].copy_from_slice(&spell_levels[..n]);
    }
    table
}

/// ACE `ArmorCantrips`.
pub mod armor_cantrips {
    use std::sync::LazyLock;

    use empyrean_entity::enums::SpellId;

    use crate::tables::cantrips::armor_cantrips::{ARMOR_CANTRIPS, NUM_LEVELS, SPELLS};

    /// `Table` ("original api"): each spell's levels, built by the static constructor.
    pub static TABLE: LazyLock<Vec<Vec<SpellId>>> = LazyLock::new(armor_cantrips);

    // ACE: ArmorCantrips.ArmorCantrips
    fn armor_cantrips() -> Vec<Vec<SpellId>> {
        // takes ~0.3ms
        build_spells()
    }

    // ACE: ArmorCantrips.BuildSpells
    fn build_spells() -> Vec<Vec<SpellId>> {
        super::build_spells("ArmorCantrips", &SPELLS, NUM_LEVELS)
    }

    // ACE: ArmorCantrips.Roll
    pub fn roll() -> SpellId {
        ARMOR_CANTRIPS.roll(0.0)
    }
}

/// ACE `JewelryCantrips`.
pub mod jewelry_cantrips {
    use std::sync::LazyLock;

    use empyrean_entity::enums::SpellId;

    use crate::tables::cantrips::jewelry_cantrips::{JEWELRY_CANTRIPS, NUM_LEVELS, SPELLS};

    /// `Table` ("original api"): each spell's levels, built by the static constructor.
    pub static TABLE: LazyLock<Vec<Vec<SpellId>>> = LazyLock::new(jewelry_cantrips);

    // ACE: JewelryCantrips.JewelryCantrips
    fn jewelry_cantrips() -> Vec<Vec<SpellId>> {
        // takes ~0.3ms
        build_spells()
    }

    // ACE: JewelryCantrips.BuildSpells
    fn build_spells() -> Vec<Vec<SpellId>> {
        super::build_spells("JewelryCantrips", &SPELLS, NUM_LEVELS)
    }

    // ACE: JewelryCantrips.Roll
    pub fn roll() -> SpellId {
        JEWELRY_CANTRIPS.roll(0.0)
    }
}

/// ACE `MeleeCantrips`.
pub mod melee_cantrips {
    use std::sync::LazyLock;

    use empyrean_entity::enums::SpellId;

    use crate::tables::cantrips::melee_cantrips::{MELEE_CANTRIPS, NUM_LEVELS, SPELLS};

    /// `Table` ("original api"): each spell's levels, built by the static constructor.
    pub static TABLE: LazyLock<Vec<Vec<SpellId>>> = LazyLock::new(melee_cantrips);

    // ACE: MeleeCantrips.MeleeCantrips
    fn melee_cantrips() -> Vec<Vec<SpellId>> {
        // takes ~0.3ms
        build_spells()
    }

    // ACE: MeleeCantrips.BuildSpells
    fn build_spells() -> Vec<Vec<SpellId>> {
        super::build_spells("MeleeCantrips", &SPELLS, NUM_LEVELS)
    }

    // ACE: MeleeCantrips.Roll
    pub fn roll() -> SpellId {
        MELEE_CANTRIPS.roll(0.0)
    }
}

/// ACE `MissileCantrips`.
pub mod missile_cantrips {
    use std::sync::LazyLock;

    use empyrean_entity::enums::SpellId;

    use crate::tables::cantrips::missile_cantrips::{MISSILE_CANTRIPS, NUM_LEVELS, SPELLS};

    /// `Table` ("original api"): each spell's levels, built by the static constructor.
    pub static TABLE: LazyLock<Vec<Vec<SpellId>>> = LazyLock::new(missile_cantrips);

    // ACE: MissileCantrips.MissileCantrips
    fn missile_cantrips() -> Vec<Vec<SpellId>> {
        // takes ~0.3ms
        build_spells()
    }

    // ACE: MissileCantrips.BuildSpells
    fn build_spells() -> Vec<Vec<SpellId>> {
        super::build_spells("MissileCantrips", &SPELLS, NUM_LEVELS)
    }

    // ACE: MissileCantrips.Roll
    pub fn roll() -> SpellId {
        MISSILE_CANTRIPS.roll(0.0)
    }
}

/// ACE `WandCantrips`.
pub mod wand_cantrips {
    use std::sync::LazyLock;

    use empyrean_entity::enums::SpellId;

    use crate::tables::cantrips::wand_cantrips::{CASTER_CANTRIPS, NUM_LEVELS, SPELLS};

    /// `Table` ("original api"): each spell's levels, built by the static constructor.
    pub static TABLE: LazyLock<Vec<Vec<SpellId>>> = LazyLock::new(wand_cantrips);

    // ACE: WandCantrips.WandCantrips
    fn wand_cantrips() -> Vec<Vec<SpellId>> {
        // takes ~0.3ms
        build_spells()
    }

    // ACE: WandCantrips.BuildSpells
    fn build_spells() -> Vec<Vec<SpellId>> {
        super::build_spells("WandCantrips", &SPELLS, NUM_LEVELS)
    }

    // ACE: WandCantrips.Roll
    pub fn roll() -> SpellId {
        CASTER_CANTRIPS.roll(0.0)
    }
}

/// ACE `CantripChance`: the pure table arithmetic behind the `cantrip_drop_rate` properties.
pub mod cantrip_chance {
    use crate::entity::ChanceTable;

    /// `Enumerable.Sum(i => i.chance)`: .NET sums a `float` selector in `double` and returns the
    /// total as `float`.
    #[allow(clippy::cast_possible_truncation)] // the (float) of .NET's Sum
    fn sum_chances(table: &[(i32, f32)]) -> f32 {
        let mut sum = 0.0f64;
        for entry in table {
            sum += f64::from(entry.1);
        }
        sum as f32
    }

    /// Scales every non-zero cantrip count by `cantrip_drop_rate` (rescaling to 1 if that
    /// overflows) and puts the remainder on zero cantrips, first.
    // ACE: CantripChance.ScaleNumCantrips
    pub fn scale_num_cantrips(
        num_cantrips: &ChanceTable<i32>,
        cantrip_drop_rate: f32,
    ) -> ChanceTable<i32> {
        let mut new_table = Vec::new();

        for entry in num_cantrips.entries() {
            if entry.0 != 0 {
                new_table.push((entry.0, entry.1 * cantrip_drop_rate));
            }
        }

        let mut new_table = ChanceTable::from_vec(new_table);

        let mut total_chance = sum_chances(new_table.entries());

        if total_chance > 1.0 {
            new_table = rescale(new_table, 1.0);
            total_chance = 1.0;
        }

        let mut final_table = vec![(0, 1.0 - total_chance)];
        final_table.extend_from_slice(new_table.entries());

        ChanceTable::from_vec(final_table)
    }

    /// Scales each cantrip level (1 minor, 2 major, 3 epic, 4 legendary) by its rate, then
    /// rescales the table to 1.
    // ACE: CantripChance.ScaleCantripLevels
    pub fn scale_cantrip_levels(
        cantrip_level: &ChanceTable<i32>,
        minor_cantrip_drop_rate: f32,
        major_cantrip_drop_rate: f32,
        epic_cantrip_drop_rate: f32,
        legendary_cantrip_drop_rate: f32,
    ) -> ChanceTable<i32> {
        let mut new_table = Vec::new();

        for entry in cantrip_level.entries() {
            let modifier = match entry.0 {
                1 => minor_cantrip_drop_rate,
                2 => major_cantrip_drop_rate,
                3 => epic_cantrip_drop_rate,
                4 => legendary_cantrip_drop_rate,
                _ => 1.0,
            };
            new_table.push((entry.0, entry.1 * modifier));
        }

        rescale(ChanceTable::from_vec(new_table), 1.0)
    }

    /// Scales the chances to sum to `target` (ACE's default is `1.0f`); returns `table` itself if
    /// they already do.
    // ACE: CantripChance.Rescale
    #[allow(clippy::float_cmp)] // ACE compares the float sum exactly
    pub fn rescale(table: ChanceTable<i32>, target: f32) -> ChanceTable<i32> {
        let total = sum_chances(table.entries());

        if total == target {
            return table;
        }

        // get scalar
        let scalar = target / total;

        let rescaled = table
            .entries()
            .iter()
            .map(|entry| (entry.0, entry.1 * scalar))
            .collect();

        ChanceTable::from_vec(rescaled)
    }
}
