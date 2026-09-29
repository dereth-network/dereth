// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/SubPalette.cs
//! `SubPalette` (fields only).

/// ACE: SubPalette
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SubPalette {
    // ACE: SubPalette.SubID
    pub sub_id: u32,
    // ACE: SubPalette.Offset
    pub offset: u32,
    // ACE: SubPalette.NumColors
    pub num_colors: u32,
}
