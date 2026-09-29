// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/GemCountChance.cs, Source/ACE.Server/Factories/Tables/Cantrips/CantripChance.cs, Source/ACE.Server/Factories/Entity/SpellLevelCache.cs
//! The `Factories/Tables` members that need the world: every table method that takes
//! a `TreasureDeath`, a `WorldObject` or a `TreasureRoll`, reads `PropertyManager` or the world
//! database. They are ported here, in empyrean-world, over the literal data empyrean-tables generated
//! (`empyrean_tables::tables`), so empyrean-tables stays a leaf crate. One module per ACE class, grouped
//! in one file per ACE folder, as `empyrean_tables::logic` is; the other members of each class are in
//! `empyrean_tables::logic`.
//!
//! Draws go straight to `ThreadSafeRandom` (`empyrean_common`), in ACE's order; the `ChanceTable`
//! rolls draw through empyrean-tables' `rng`, which forwards to the same generator.

pub mod cantrips;
pub mod spells;
pub mod tables;
pub mod wcids;

/// ACE's process-wide statics of `Factories/Tables` and `Factories/Entity/SpellLevelCache`, kept
/// per `World`: each is built on first use from the world it is used with, as ACE's static
/// constructors are built from the one server's database, properties and dats. A test process
/// builds many worlds over different content, so a process-wide copy made the first world's
/// tables everyone's.
#[derive(Debug, Default)]
pub struct LootTablesState {
    // ACE: GemCountChance.gemCodes
    pub(crate) gem_codes: std::sync::OnceLock<tables::gem_count_chance::GemCodes>,
    // ACE: CantripChance.numCantrips, CantripChance.cantripLevels
    pub(crate) cantrips: std::sync::RwLock<Option<cantrips::cantrip_chance::Tables>>,
    // ACE: SpellLevelCache.spellLevels
    pub(crate) spell_levels: std::sync::Mutex<std::collections::HashMap<i32, i32>>,
}

/// C#'s `list[i]` for an `int` index; .NET throws `ArgumentOutOfRangeException` when `i` is out
/// of range, and a panic stands in for it.
pub(crate) fn at<T: Copy>(list: &[T], i: i32) -> T {
    match usize::try_from(i) {
        Ok(u) if u < list.len() => list[u],
        _ => panic!(
            "ArgumentOutOfRangeException: index {i}, count {}",
            list.len()
        ),
    }
}

/// C#'s `list.Count`.
pub(crate) fn count<T>(list: &[T]) -> i32 {
    i32::try_from(list.len()).expect("a C# list count fits an int")
}

/// `wo.Name`, for log lines (the `PropertyString.Name` behind the virtual getter).
pub(crate) fn wo_name(wo: &crate::world_objects::world_object::WorldObject) -> String {
    wo.get_property(empyrean_entity::enums::PropertyString::Name)
        .unwrap_or_default()
}
