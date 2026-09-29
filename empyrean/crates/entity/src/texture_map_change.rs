// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/TextureMapChange.cs
//! `TextureMapChange` (fields only).

/// ACE: TextureMapChange
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextureMapChange {
    // ACE: TextureMapChange.PartIndex
    pub part_index: u8,
    // ACE: TextureMapChange.OldTexture
    pub old_texture: u32,
    // ACE: TextureMapChange.NewTexture
    pub new_texture: u32,
}
