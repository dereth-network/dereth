// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/Cantrips/CantripChance.cs, Source/ACE.Server/Factories/Tables/Cantrips/NumCantrips.cs
//! The 4.10 members of the `Factories/Tables/Cantrips` classes (see
//! `empyrean_tables::logic::cantrips` for the rest).

/// ACE `CantripChance`: the per-tier cantrip count and level tables, rescaled by the
/// `*cantrip_drop_rate` server properties.
///
/// `numCantrips` and `cantripLevels` are process-wide statics in ACE, filled by the static
/// constructor on first use (from `PropertyManager`) and replaced by `ApplyNumCantripsMod` /
/// `ApplyCantripLevelsMod` when a property changes. Here they are the world's
/// (`World.loot_tables`), built from its properties on the first roll (or apply).
pub mod cantrip_chance {
    use std::sync::PoisonError;

    use empyrean_common::dotnet::math::{round_digits_mode, MidpointRounding};
    use empyrean_content::models::world::TreasureDeath;
    use empyrean_tables::entity::ChanceTable;
    use empyrean_tables::logic::cantrips::cantrip_chance::{
        scale_cantrip_levels, scale_num_cantrips,
    };
    use empyrean_tables::tables::cantrips::cantrip_chance as ace_tables;

    type TierTables = [&'static ChanceTable<i32>; 8];

    /// Not ACE: the cantrip counts by tier the world's era rolls: ClassicACE's for the
    /// Infiltration era (V416), else ACE's.
    fn num_cantrips_source(w: &World) -> &'static TierTables {
        if w.era.loot_rules == empyrean_common::era::LootRules::Infiltration {
            &empyrean_tables::era::infiltration::cantrip_chance::_NUM_CANTRIPS
        } else {
            &ace_tables::_NUM_CANTRIPS
        }
    }

    /// Not ACE: the cantrip levels by tier the world's era rolls (minor and major only in the
    /// Infiltration era; V416).
    fn cantrip_levels_source(w: &World) -> &'static TierTables {
        if w.era.loot_rules == empyrean_common::era::LootRules::Infiltration {
            &empyrean_tables::era::infiltration::cantrip_chance::_CANTRIP_LEVELS
        } else {
            &ace_tables::_CANTRIP_LEVELS
        }
    }

    use crate::managers::property_manager;
    use crate::World;

    /// One entry of `List<ChanceTable<int>>`: one of the literal tables, or a rescaled copy.
    #[derive(Debug)]
    pub enum TierTable {
        Literal(&'static ChanceTable<i32>),
        Scaled(ChanceTable<i32>),
    }

    impl TierTable {
        fn table(&self) -> &ChanceTable<i32> {
            match self {
                TierTable::Literal(t) => t,
                TierTable::Scaled(t) => t,
            }
        }
    }

    #[derive(Debug)]
    pub(crate) struct Tables {
        num_cantrips: Vec<TierTable>,
        cantrip_levels: Vec<TierTable>,
    }

    fn literal(tables: &[&'static ChanceTable<i32>; 8]) -> Vec<TierTable> {
        tables.iter().map(|&t| TierTable::Literal(t)).collect()
    }

    /// Runs the static constructor if nothing has built the tables yet.
    fn ensure_initialized(w: &World) {
        if w.loot_tables
            .cantrips
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .is_none()
        {
            cantrip_chance(w);
        }
    }

    /// The static constructor: both tables from the current property values, without logging.
    // ACE: CantripChance.CantripChance
    pub fn cantrip_chance(w: &World) {
        {
            let mut tables = w
                .loot_tables
                .cantrips
                .write()
                .unwrap_or_else(PoisonError::into_inner);
            if tables.is_none() {
                *tables = Some(Tables {
                    num_cantrips: literal(num_cantrips_source(w)),
                    cantrip_levels: literal(cantrip_levels_source(w)),
                });
            }
        }
        apply_num_cantrips_mod(w, false);
        apply_cantrip_levels_mod(w, false);
    }

    // ACE: CantripChance.RollNumCantrips
    #[must_use]
    pub fn roll_num_cantrips(w: &World, profile: &TreasureDeath) -> i32 {
        ensure_initialized(w);
        let tables = w
            .loot_tables
            .cantrips
            .read()
            .unwrap_or_else(PoisonError::into_inner);
        let num_cantrips = &tables.as_ref().expect("initialized").num_cantrips;
        tier_table(num_cantrips, profile.tier).roll(profile.loot_quality_mod)
    }

    // ACE: CantripChance.RollCantripLevel
    #[must_use]
    pub fn roll_cantrip_level(w: &World, profile: &TreasureDeath) -> i32 {
        ensure_initialized(w);
        let tables = w
            .loot_tables
            .cantrips
            .read()
            .unwrap_or_else(PoisonError::into_inner);
        let cantrip_levels = &tables.as_ref().expect("initialized").cantrip_levels;
        tier_table(cantrip_levels, profile.tier).roll(profile.loot_quality_mod)
    }

    fn tier_table(tables: &[TierTable], tier: i32) -> &ChanceTable<i32> {
        match usize::try_from(tier - 1) {
            Ok(i) if i < tables.len() => tables[i].table(),
            _ => panic!(
                "ArgumentOutOfRangeException: index {}, count {}",
                tier - 1,
                tables.len()
            ),
        }
    }

    /// `(float)Math.Max(0.0f, PropertyManager.GetDouble(key).Item)`.
    #[allow(clippy::cast_possible_truncation)]
    fn drop_rate(w: &World, key: &str) -> f32 {
        empyrean_common::dotnet::math::max(
            0.0,
            property_manager::get_double(w, key, 0.0, true).item,
        ) as f32
    }

    /// Scales NumCantrips (no chance vs. chance, relative to each other) by `cantrip_drop_rate`.
    // ACE: CantripChance.ApplyNumCantripsMod
    #[allow(clippy::float_cmp)]
    pub fn apply_num_cantrips_mod(w: &World, show_results: bool) {
        // scales NumCantrips, no chance vs. chance, relative to each other
        let cantrip_drop_rate = drop_rate(w, "cantrip_drop_rate");

        let new_tables = if cantrip_drop_rate != 1.0 {
            let mut new_table = Vec::new();
            for entry in num_cantrips_source(w) {
                let new_entry = scale_num_cantrips(entry, cantrip_drop_rate);
                new_table.push(TierTable::Scaled(new_entry));
            }
            new_table
        } else {
            literal(num_cantrips_source(w))
        };

        let mut tables = w
            .loot_tables
            .cantrips
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        let tables = tables.get_or_insert_with(|| Tables {
            num_cantrips: Vec::new(),
            cantrip_levels: literal(cantrip_levels_source(w)),
        });
        tables.num_cantrips = new_tables;

        if show_results {
            log::info!(
                "ApplyNumCantripsMod({})",
                empyrean_common::dotnet::to_string(cantrip_drop_rate)
            );
            log::info!("");
            show_tables(&tables.num_cantrips);
        }
    }

    /// Scales CantripLevels (relative to each other) by the four level drop rates.
    // ACE: CantripChance.ApplyCantripLevelsMod
    #[allow(clippy::float_cmp)]
    pub fn apply_cantrip_levels_mod(w: &World, show_results: bool) {
        // scales CantripLevels, relative to each other
        let minor_cantrip_drop_rate = drop_rate(w, "minor_cantrip_drop_rate");
        let major_cantrip_drop_rate = drop_rate(w, "major_cantrip_drop_rate");
        let epic_cantrip_drop_rate = drop_rate(w, "epic_cantrip_drop_rate");
        let legendary_cantrip_drop_rate = drop_rate(w, "legendary_cantrip_drop_rate");

        let new_tables = if minor_cantrip_drop_rate != 1.0
            || major_cantrip_drop_rate != 1.0
            || epic_cantrip_drop_rate != 1.0
            || legendary_cantrip_drop_rate != 1.0
        {
            let mut new_table = Vec::new();
            for entry in cantrip_levels_source(w) {
                let new_entry = scale_cantrip_levels(
                    entry,
                    minor_cantrip_drop_rate,
                    major_cantrip_drop_rate,
                    epic_cantrip_drop_rate,
                    legendary_cantrip_drop_rate,
                );
                new_table.push(TierTable::Scaled(new_entry));
            }
            new_table
        } else {
            literal(cantrip_levels_source(w))
        };

        let mut tables = w
            .loot_tables
            .cantrips
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        let tables = tables.get_or_insert_with(|| Tables {
            num_cantrips: literal(num_cantrips_source(w)),
            cantrip_levels: Vec::new(),
        });
        tables.cantrip_levels = new_tables;

        if show_results {
            log::info!(
                "ApplyCantripLevelsMod({}, {}, {}, {})",
                empyrean_common::dotnet::to_string(minor_cantrip_drop_rate),
                empyrean_common::dotnet::to_string(major_cantrip_drop_rate),
                empyrean_common::dotnet::to_string(epic_cantrip_drop_rate),
                empyrean_common::dotnet::to_string(legendary_cantrip_drop_rate)
            );
            log::info!("");
            show_tables(&tables.cantrip_levels);
        }
    }

    // ACE: CantripChance.ShowTables
    fn show_tables(tables: &[TierTable]) {
        for (i, table) in tables.iter().enumerate() {
            log::info!("Tier {}:", i + 1);
            log::info!("");
            for entry in table.table().entries() {
                log::info!("{}: {}", entry.0, get_percent(entry.1));
            }
            log::info!("");
        }
    }

    /// `$"{Math.Round(pct * 100, 2, MidpointRounding.AwayFromZero)}%"`.
    // ACE: CantripChance.GetPercent
    #[must_use]
    pub fn get_percent(pct: f32) -> String {
        let rounded = round_digits_mode(f64::from(pct * 100.0), 2, MidpointRounding::AwayFromZero);
        format!("{}%", empyrean_common::dotnet::to_string(rounded))
    }

    /// Each tier's `(result, chance)` entries.
    pub type TierEntries = Vec<Vec<(i32, f32)>>;

    /// The current tables (tier order), for tests: `(numCantrips, cantripLevels)` entries.
    #[must_use]
    pub fn current_tables(w: &World) -> (TierEntries, TierEntries) {
        ensure_initialized(w);
        let tables = w
            .loot_tables
            .cantrips
            .read()
            .unwrap_or_else(PoisonError::into_inner);
        let t = tables.as_ref().expect("initialized");
        let dump = |v: &[TierTable]| v.iter().map(|t| t.table().entries().to_vec()).collect();
        (dump(&t.num_cantrips), dump(&t.cantrip_levels))
    }
}

/// ACE `NumCantrips` (an older copy of `CantripChance`'s tables, still in ACE).
pub mod num_cantrips {
    use empyrean_content::models::world::TreasureDeath;
    use empyrean_tables::tables::cantrips::num_cantrips::{CANTRIP_LEVELS, NUM_CANTRIPS};

    use super::super::at;

    // ACE: NumCantrips.RollNumCantrips
    #[must_use]
    pub fn roll_num_cantrips(profile: &TreasureDeath) -> i32 {
        at(&NUM_CANTRIPS, profile.tier - 1).roll(profile.loot_quality_mod)
    }

    // ACE: NumCantrips.RollCantripLevel
    #[must_use]
    pub fn roll_cantrip_level(profile: &TreasureDeath) -> i32 {
        at(&CANTRIP_LEVELS, profile.tier - 1).roll(profile.loot_quality_mod)
    }
}
