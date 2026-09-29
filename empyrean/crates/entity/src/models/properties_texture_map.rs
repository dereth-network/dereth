// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Models/PropertiesTextureMap.cs
//! `PropertiesTextureMap`: a texture override on one part.

/// ACE: PropertiesTextureMap. `Clone` copies every field; ACE's own `Clone()` is [`PropertiesTextureMap::ace_clone`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PropertiesTextureMap {
    // ACE: PropertiesTextureMap.PartIndex
    pub part_index: u8,
    // ACE: PropertiesTextureMap.OldTexture
    pub old_texture: u32,
    // ACE: PropertiesTextureMap.NewTexture
    pub new_texture: u32,
}

impl PropertiesTextureMap {
    /// ACE's `Clone()`: a copy of every field.
    // ACE: PropertiesTextureMap.Clone
    #[must_use]
    pub fn ace_clone(&self) -> PropertiesTextureMap {
        PropertiesTextureMap {
            part_index: self.part_index,
            old_texture: self.old_texture,
            new_texture: self.new_texture,
        }
    }
}
