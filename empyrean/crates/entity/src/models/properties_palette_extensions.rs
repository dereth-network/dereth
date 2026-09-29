// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Models/PropertiesPaletteExtensions.cs
//! `PropertiesPaletteExtensions`: helpers over a biota's `IList<PropertiesPalette>`, which may be `null` (`None`).
//! ACE's `ReaderWriterLockSlim` parameter is dropped (see [`crate::models`]).

use crate::models::properties_palette::PropertiesPalette;

/// `Count`, or 0 for a null list.
// ACE: PropertiesPaletteExtensions.GetCount
#[must_use]
pub fn get_count(value: Option<&Vec<PropertiesPalette>>) -> i32 {
    let Some(value) = value else { return 0 };

    i32::try_from(value.len()).unwrap_or(i32::MAX)
}

/// A new list with the same entries, or `None` for a null list. ACE's copy shares the entry
/// objects; this one copies them.
// ACE: PropertiesPaletteExtensions.Clone
#[must_use]
pub fn clone(value: Option<&Vec<PropertiesPalette>>) -> Option<Vec<PropertiesPalette>> {
    let value = value?;

    Some(value.clone())
}

/// Appends every entry to `destination`, in order; nothing for a null list.
// ACE: PropertiesPaletteExtensions.CopyTo
pub fn copy_to(value: Option<&Vec<PropertiesPalette>>, destination: &mut Vec<PropertiesPalette>) {
    let Some(value) = value else { return };

    for entry in value {
        destination.push(entry.clone());
    }
}
