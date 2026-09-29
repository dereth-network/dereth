// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/ObjDesc.cs
//! `ObjDesc`: a visual description (palette, sub-palettes, texture and part changes).

use crate::models::properties_anim_part::PropertiesAnimPart;
use crate::models::properties_palette::PropertiesPalette;
use crate::models::properties_texture_map::PropertiesTextureMap;

/// ACE: ObjDesc
#[derive(Debug, Clone, Default)]
pub struct ObjDesc {
    // ACE: ObjDesc.PaletteID
    pub palette_id: u32,
    // ACE: ObjDesc.SubPalettes
    pub sub_palettes: Vec<PropertiesPalette>,
    // ACE: ObjDesc.TextureChanges
    pub texture_changes: Vec<PropertiesTextureMap>,
    // ACE: ObjDesc.AnimPartChanges
    pub anim_part_changes: Vec<PropertiesAnimPart>,
}

impl ObjDesc {
    /// Adds `tm` unless an identical change (same part, old and new texture) is present.
    // ACE: ObjDesc.AddTextureChange
    pub fn add_texture_change(&mut self, tm: PropertiesTextureMap) {
        let e = self.texture_changes.iter().find(|c| {
            c.part_index == tm.part_index
                && c.old_texture == tm.old_texture
                && c.new_texture == tm.new_texture
        });
        if e.is_none() {
            self.texture_changes.push(tm);
        }
    }

    /// Removes the first identical part change (same index and animation id), then appends `ap`,
    /// so the change moves to the end of the list.
    // ACE: ObjDesc.AddAnimPartChange
    pub fn add_anim_part_change(&mut self, ap: PropertiesAnimPart) {
        let p = self
            .anim_part_changes
            .iter()
            .position(|c| c.index == ap.index && c.animation_id == ap.animation_id);
        if let Some(p) = p {
            self.anim_part_changes.remove(p);
        }
        self.anim_part_changes.push(ap);
    }
}
