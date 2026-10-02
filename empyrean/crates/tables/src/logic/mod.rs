//! The methods of ACE's `Factories/Tables` classes, hand-ported over the generated data in
//! [`crate::tables`]. One module per ACE class (named as its data module is), grouped in one file
//! per ACE folder.
//!
//! Static fields a C# static constructor fills (`_combined`, `Table`, `spellProgression`, …) are
//! `LazyLock`s built by the ported constructor on first use, as .NET runs a static constructor on
//! first access. Draws go through [`crate::rng`] (ACE's `ThreadSafeRandom`), in ACE's order.
//!
//! Methods that take `TreasureDeath`, `WorldObject` or `TreasureRoll`, read `PropertyManager` or
//! query the world database are loot generation proper and are ported in `empyrean-world`'s
//! `factories::tables_logic`, against this data (every table item is `pub` for that reason).

pub mod cantrips;
pub mod era;
pub mod legacy;
pub mod spells;
pub mod tables;
pub mod wcids;
pub mod weapons;

/// C#'s `list[i]` for an `int` index. .NET throws `ArgumentOutOfRangeException` when `i` is out of
/// range; a panic stands in for it.
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
