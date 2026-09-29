// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.DatLoader/FileTypes/PaletteSet.cs
//! `PaletteSet.GetPaletteID`: a palette from a set by shade.

use dereth_assets::PaletteSet;

/// ACE's `PaletteSet` helpers.
pub trait PaletteSetExt {
    /// The palette at `hue` (0..=1) through the set, or 0 for an empty set or a hue out of range.
    fn get_palette_id(&self, hue: f64) -> u32;
}

impl PaletteSetExt for PaletteSet {
    // ACE: PaletteSet.GetPaletteID
    fn get_palette_id(&self, hue: f64) -> u32 {
        let list = &self.palette_ids;
        // Make sure the PaletteList has valid data and the hue is within valid ranges
        if list.is_empty() || hue < 0.0 || hue > 1.0 {
            return 0;
        }
        #[allow(clippy::cast_precision_loss)] // C# promotes Count to double here too
        let count = list.len() as f64;
        // C#'s (int) of a double truncates toward zero. Past the checks above, hue is in 0..=1 or
        // NaN (which C# casts to int.MinValue and Rust to 0; both clamp to index 0 below).
        #[allow(clippy::cast_possible_truncation)]
        let mut pal_index = ((count - 0.000_001) * hue) as i64;
        if pal_index < 0 {
            pal_index = 0;
        }
        let last = i64::try_from(list.len()).unwrap_or(i64::MAX) - 1;
        if pal_index > last {
            pal_index = last;
        }
        usize::try_from(pal_index)
            .ok()
            .and_then(|i| list.get(i))
            .map_or(0, |d| d.0)
    }
}
