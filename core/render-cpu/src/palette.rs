//! Palettes and sub-palettes.
//!
//! Palette records are described in `docs/formats/13-palette-and-surfaces.md`.
//!
//! **A palette is always 2048 entries, never 256**. The loader expands a 256-entry
//! table by replicating each colour eight times, so the game's index images are
//! 11-bit (`0…2047`) with eight *shades* per palette slot, and a sub-palette swap can replace a
//! contiguous range without disturbing the rest. Sizing the buffer at 256 truncates every indexed
//! texture in the game.

use std::fmt;

/// The expanded palette. Always 2048 ARGB entries.
#[derive(Clone, PartialEq, Eq)]
pub struct ExpandedPalette(pub [u32; 2048]);

impl fmt::Debug for ExpandedPalette {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // 2048 entries is not a useful Debug body; show the shape and the first slot instead.
        f.debug_struct("ExpandedPalette")
            .field("entries", &2048)
            .field("[0]", &self.0[0])
            .finish()
    }
}

impl Default for ExpandedPalette {
    fn default() -> Self {
        Self([0; 2048])
    }
}

/// Minimum lighting, the third field of the dat object. Carried for completeness; the
/// draw path does not read it.
pub const DEFAULT_MIN_LIGHTING: f32 = 0.1;

/// The number of entries in an expanded palette.
pub const PALETTE_ENTRIES: usize = 2048;
/// How many times a 256-colour table is replicated to fill one.
pub const PALETTE_REPLICATION: usize = 8;
/// Palette indices `0..=7` are the transparent range for clip-mapped surfaces.
/// The expansion is `dst32[x] = (bClipMap && idx <= 7) ? 0 : palette->ARGB[idx]`.
pub const CLIP_MAP_TRANSPARENT_MAX_INDEX: u16 = 7;

impl ExpandedPalette {
    /// The palette load:
    ///
    /// ```text
    /// if (num_colors == 256) {
    ///     new = new ulong[2048]
    ///     for (i = 0; i < 256; i++) for (k = 0; k < 8; k++) new[i*8 + k] = old[i]
    ///     num_colors = 2048; ARGB = new
    /// }
    /// ```
    ///
    /// A table that is already 2048 long is taken as-is; anything else is a malformed palette and
    /// comes back `None`, because the client's own `Modify` refuses to write outside `num_colors`.
    #[must_use]
    pub fn from_dat(colors: &[u32]) -> Option<Self> {
        match colors.len() {
            256 => {
                let mut out = [0u32; PALETTE_ENTRIES];
                for i in 0..256 {
                    for k in 0..PALETTE_REPLICATION {
                        out[i * PALETTE_REPLICATION + k] = colors[i];
                    }
                }
                Some(Self(out))
            }
            PALETTE_ENTRIES => {
                let mut out = [0u32; PALETTE_ENTRIES];
                out.copy_from_slice(colors);
                Some(Self(out))
            }
            _ => None,
        }
    }

    /// One colour by index.
    #[must_use]
    pub fn get_color32(&self, index: u16) -> u32 {
        self.0[usize::from(index) & (PALETTE_ENTRIES - 1)]
    }

    /// Apply a sub-palette: copies `count` ARGB
    /// entries into `ARGB[offset .. offset+count)`, **refusing when `offset + count > num_colors`**.
    ///
    /// Returns `false` on that refusal, leaving the palette untouched, exactly as the client does.
    pub fn modify(&mut self, offset: usize, colors: &[u32]) -> bool {
        if offset + colors.len() > PALETTE_ENTRIES {
            return false;
        }
        self.0[offset..offset + colors.len()].copy_from_slice(colors);
        true
    }

    /// Apply a list of sub-palettes, as the client does when walking an array of
    /// `Subpalette` records.
    ///
    /// Sub-palette offsets and lengths are in **8-entry units**, and a length byte of 0 means 256
    /// (the whole palette). This
    /// helper takes the raw dat units and does that conversion; `modify` takes entries.
    pub fn apply_subpalette(
        &mut self,
        offset_units: u16,
        length_units: u16,
        colors: &[u32],
    ) -> bool {
        let count_units = if length_units == 0 {
            256
        } else {
            usize::from(length_units)
        };
        let offset = usize::from(offset_units) * PALETTE_REPLICATION;
        let count = count_units * PALETTE_REPLICATION;
        if colors.len() < count {
            return false;
        }
        self.modify(offset, &colors[..count])
    }

    /// Clone before modifying, so the base palette
    /// in the cache is never mutated. A palette produced this way has no DataID, which is why every
    /// dyed item gets an *uncached* combined texture.
    #[must_use]
    pub fn make_modified(&self) -> Self {
        self.clone()
    }

    /// Expand one index the way the client does.
    ///
    /// `dst32[x] = (bClipMap && idx <= 7) ? 0x00000000 : palette->ARGB[idx]`. For non-clip-mapped
    /// surfaces every index goes through the palette unchanged, so **index 0 is an ordinary colour**.
    #[must_use]
    pub fn expand(&self, index: u16, clip_map: bool) -> u32 {
        if clip_map && index <= CLIP_MAP_TRANSPARENT_MAX_INDEX {
            0x0000_0000
        } else {
            self.get_color32(index)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The verified palette load shows that all 4,521 shipped palettes have
    // num_colors == 2048".
    #[test]
    fn a_256_entry_table_is_replicated_eight_times() {
        let src: Vec<u32> = (0..256).map(|i| 0xFF00_0000 | i).collect();
        let p = ExpandedPalette::from_dat(&src).unwrap();
        assert_eq!(p.0.len(), 2048);
        for (i, colour) in src.iter().enumerate().take(256) {
            for k in 0..8usize {
                assert_eq!(p.0[i * 8 + k], *colour, "entry {}", i * 8 + k);
            }
        }
        // The replication is what makes the 11-bit index meaningful: indices 0..7 are all colour 0,
        // 8..15 are all colour 1, and so on. This is the "sub-palette shift" the spec names.
        assert_eq!(p.get_color32(0), src[0]);
        assert_eq!(p.get_color32(7), src[0]);
        assert_eq!(p.get_color32(8), src[1]);
        assert_eq!(p.get_color32(2047), src[255]);
    }

    // Oracle: same section -- a palette that already has 2048 entries is used as-is.
    #[test]
    fn an_already_expanded_table_is_taken_verbatim() {
        let src: Vec<u32> = (0..2048).map(|i| i ^ 0xDEAD).collect();
        let p = ExpandedPalette::from_dat(&src).unwrap();
        assert_eq!(&p.0[..], &src[..]);
        // Nothing else is a palette.
        assert!(ExpandedPalette::from_dat(&[0; 128]).is_none());
        assert!(ExpandedPalette::from_dat(&[]).is_none());
    }

    // Oracle: the sub-palette apply -- "copies count ARGB entries into
    // ARGB[offset ... offset+count), refusing when offset + count > num_colors".
    #[test]
    fn modify_writes_a_range_and_refuses_to_overrun() {
        let mut p = ExpandedPalette::from_dat(&vec![0u32; 2048]).unwrap();
        assert!(p.modify(16, &[1, 2, 3, 4]));
        assert_eq!(&p.0[15..21], &[0, 1, 2, 3, 4, 0]);
        // Exactly filling the tail is allowed.
        assert!(p.modify(2044, &[9, 9, 9, 9]));
        assert_eq!(p.0[2047], 9);
        // One past the end is refused, and nothing is written.
        let before = p.0[2047];
        assert!(!p.modify(2045, &[7, 7, 7, 7]));
        assert_eq!(p.0[2047], before);
    }

    // Sub-palette offsets and lengths are in 8-entry units; a length byte of 0 means 256
    // entries, the whole palette.
    #[test]
    fn subpalette_offsets_and_lengths_are_in_eight_entry_units() {
        let mut p = ExpandedPalette::from_dat(&vec![0u32; 2048]).unwrap();
        // Offset 2 units = entry 16; length 3 units = 24 entries.
        let colors: Vec<u32> = (1..=24).collect();
        assert!(p.apply_subpalette(2, 3, &colors));
        assert_eq!(p.0[15], 0);
        assert_eq!(p.0[16], 1);
        assert_eq!(p.0[39], 24);
        assert_eq!(p.0[40], 0);

        // Length 0 means 256 units, i.e. the whole 2048-entry palette.
        let mut p = ExpandedPalette::from_dat(&vec![0u32; 2048]).unwrap();
        let all: Vec<u32> = (0..2048u32).map(|i| i | 0x8000_0000).collect();
        assert!(p.apply_subpalette(0, 0, &all));
        assert_eq!(p.0[2047], 2047 | 0x8000_0000);

        // A short colour list is refused rather than partially applied.
        let mut p = ExpandedPalette::from_dat(&vec![0u32; 2048]).unwrap();
        assert!(!p.apply_subpalette(0, 4, &[1, 2, 3]));
        assert_eq!(p.0[0], 0);
    }

    // Oracle: the expansion -- "Palette indices 0...7 are the
    // transparent range for clip-mapped surfaces... For non-clip-mapped surfaces every index goes
    // through the palette unchanged, so index 0 is an ordinary colour."
    #[test]
    fn indices_zero_to_seven_are_transparent_only_for_clip_maps() {
        let src: Vec<u32> = (0..256).map(|i| 0xFF00_0000 | (i * 7)).collect();
        let p = ExpandedPalette::from_dat(&src).unwrap();
        for idx in 0..=7u16 {
            assert_eq!(p.expand(idx, true), 0x0000_0000, "clip map index {idx}");
            assert_eq!(p.expand(idx, false), src[0], "non-clip-map index {idx}");
        }
        // Index 8 is the first opaque one even under a clip map.
        assert_eq!(p.expand(8, true), src[1]);
        assert_eq!(p.expand(8, false), src[1]);
    }

    // Oracle: the modified-palette path -- "allocates a fresh 2048-entry palette ... so the
    // base palette in the cache is never mutated."
    #[test]
    fn a_modified_palette_does_not_disturb_its_base() {
        let base = ExpandedPalette::from_dat(&vec![0x1111_1111u32; 2048]).unwrap();
        let mut dyed = base.make_modified();
        assert!(dyed.modify(0, &[0x2222_2222; 8]));
        assert_eq!(base.0[0], 0x1111_1111);
        assert_eq!(dyed.0[0], 0x2222_2222);
    }
}
