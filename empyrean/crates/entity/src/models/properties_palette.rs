// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Models/PropertiesPalette.cs
//! `PropertiesPalette`: a sub-palette override.

/// ACE: PropertiesPalette. `Clone` copies every field; ACE's own `Clone()` is [`PropertiesPalette::ace_clone`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PropertiesPalette {
    // ACE: PropertiesPalette.SubPaletteId
    pub sub_palette_id: u32,
    // ACE: PropertiesPalette.Offset
    pub offset: u16,
    // ACE: PropertiesPalette.Length
    pub length: u16,
}

impl PropertiesPalette {
    /// ACE's `Clone()`: a copy of every field.
    // ACE: PropertiesPalette.Clone
    #[must_use]
    pub fn ace_clone(&self) -> PropertiesPalette {
        PropertiesPalette {
            sub_palette_id: self.sub_palette_id,
            offset: self.offset,
            length: self.length,
        }
    }
}
