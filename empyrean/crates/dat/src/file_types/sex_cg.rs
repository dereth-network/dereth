// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.DatLoader/Entity/SexCG.cs
//! Character-creation appearance lookups on one heritage's sex: ACE's `SexCG` getters.
//!
//! Each takes the style index the client sent. ACE indexes its lists with `Convert.ToInt32(x)`,
//! which throws for an index past the list (or past `int.MaxValue`); those arms answer
//! [`AceThrow::IndexOutOfRange`] here. The shared decoder's texture changes are
//! `(part, old, new)`.

use dereth_assets::tables::{GearItem, ObjDesc, SexCg};

use super::AceThrow;

fn index<T>(list: &[T], i: u32) -> Result<&T, AceThrow> {
    // Convert.ToInt32(uint) throws OverflowException above int.MaxValue; the indexer throws past
    // the end. Both are the request failing.
    if i32::try_from(i).is_err() {
        return Err(AceThrow::IndexOutOfRange(u64::from(i)));
    }
    usize::try_from(i)
        .ok()
        .and_then(|i| list.get(i))
        .ok_or(AceThrow::IndexOutOfRange(u64::from(i)))
}

/// `ObjDesc.TextureChanges[0]` as `(OldTexture, NewTexture)`.
fn first_texture(desc: &ObjDesc) -> Result<(u32, u32), AceThrow> {
    desc.texture_changes
        .first()
        .map(|(_, old, new)| (old.0, new.0))
        .ok_or(AceThrow::IndexOutOfRange(0))
}

fn gear(list: &[GearItem], i: u32) -> Result<&GearItem, AceThrow> {
    index(list, i)
}

/// ACE's `SexCG` getters. Every one fails with [`AceThrow::IndexOutOfRange`] where ACE throws: the
/// style index is past its list, or a strip has no texture change.
#[allow(clippy::missing_errors_doc)]
pub trait SexCgExt {
    fn get_eye_texture(&self, eyes_strip: u32, is_bald: bool) -> Result<u32, AceThrow>;
    fn get_default_eye_texture(&self, eyes_strip: u32, is_bald: bool) -> Result<u32, AceThrow>;
    fn get_nose_texture(&self, nose_strip: u32) -> Result<u32, AceThrow>;
    fn get_default_nose_texture(&self, nose_strip: u32) -> Result<u32, AceThrow>;
    fn get_mouth_texture(&self, mouth_strip: u32) -> Result<u32, AceThrow>;
    fn get_default_mouth_texture(&self, mouth_strip: u32) -> Result<u32, AceThrow>;
    /// `Ok(None)` when the style has other than exactly one part change (Gear Knights, Olthoi).
    fn get_head_object(&self, hair_style: u32) -> Result<Option<u32>, AceThrow>;
    /// `Ok(None)` when the style has no texture change (Olthoi acid).
    fn get_hair_texture(&self, hair_style: u32) -> Result<Option<u32>, AceThrow>;
    fn get_default_hair_texture(&self, hair_style: u32) -> Result<Option<u32>, AceThrow>;
    fn get_headgear_weenie(&self, headgear_style: u32) -> Result<u32, AceThrow>;
    fn get_headgear_clothing_table(&self, headgear_style: u32) -> Result<u32, AceThrow>;
    fn get_shirt_weenie(&self, shirt_style: u32) -> Result<u32, AceThrow>;
    fn get_shirt_clothing_table(&self, shirt_style: u32) -> Result<u32, AceThrow>;
    fn get_pants_weenie(&self, pants_style: u32) -> Result<u32, AceThrow>;
    fn get_pants_clothing_table(&self, pants_style: u32) -> Result<u32, AceThrow>;
    fn get_footwear_weenie(&self, footwear_style: u32) -> Result<u32, AceThrow>;
    fn get_footwear_clothing_table(&self, footwear_style: u32) -> Result<u32, AceThrow>;
}

impl SexCgExt for SexCg {
    // ACE: SexCG.GetEyeTexture
    fn get_eye_texture(&self, eyes_strip: u32, is_bald: bool) -> Result<u32, AceThrow> {
        let strip = index(&self.eye_strips, eyes_strip)?;
        let eyes = if is_bald {
            &strip.objdesc_bald
        } else {
            &strip.objdesc
        };
        Ok(first_texture(eyes)?.1)
    }
    // ACE: SexCG.GetDefaultEyeTexture
    fn get_default_eye_texture(&self, eyes_strip: u32, is_bald: bool) -> Result<u32, AceThrow> {
        let strip = index(&self.eye_strips, eyes_strip)?;
        let eyes = if is_bald {
            &strip.objdesc_bald
        } else {
            &strip.objdesc
        };
        Ok(first_texture(eyes)?.0)
    }
    // ACE: SexCG.GetNoseTexture
    fn get_nose_texture(&self, nose_strip: u32) -> Result<u32, AceThrow> {
        Ok(first_texture(&index(&self.nose_strips, nose_strip)?.1)?.1)
    }
    // ACE: SexCG.GetDefaultNoseTexture
    fn get_default_nose_texture(&self, nose_strip: u32) -> Result<u32, AceThrow> {
        Ok(first_texture(&index(&self.nose_strips, nose_strip)?.1)?.0)
    }
    // ACE: SexCG.GetMouthTexture
    fn get_mouth_texture(&self, mouth_strip: u32) -> Result<u32, AceThrow> {
        Ok(first_texture(&index(&self.mouth_strips, mouth_strip)?.1)?.1)
    }
    // ACE: SexCG.GetDefaultMouthTexture
    fn get_default_mouth_texture(&self, mouth_strip: u32) -> Result<u32, AceThrow> {
        Ok(first_texture(&index(&self.mouth_strips, mouth_strip)?.1)?.0)
    }
    // ACE: SexCG.GetHeadObject
    fn get_head_object(&self, hair_style: u32) -> Result<Option<u32>, AceThrow> {
        let hairstyle = index(&self.hair_styles, hair_style)?;
        // Gear Knights, both Olthoi types have multiple anim part changes.
        match hairstyle.objdesc.anim_part_changes.as_slice() {
            [(_, part_id)] => Ok(Some(part_id.0)),
            _ => Ok(None),
        }
    }
    // ACE: SexCG.GetHairTexture
    fn get_hair_texture(&self, hair_style: u32) -> Result<Option<u32>, AceThrow> {
        let hairstyle = index(&self.hair_styles, hair_style)?;
        // OlthoiAcid has no TextureChanges
        Ok(hairstyle
            .objdesc
            .texture_changes
            .first()
            .map(|(_, _, new)| new.0))
    }
    // ACE: SexCG.GetDefaultHairTexture
    fn get_default_hair_texture(&self, hair_style: u32) -> Result<Option<u32>, AceThrow> {
        let hairstyle = index(&self.hair_styles, hair_style)?;
        Ok(hairstyle
            .objdesc
            .texture_changes
            .first()
            .map(|(_, old, _)| old.0))
    }
    // ACE: SexCG.GetHeadgearWeenie
    fn get_headgear_weenie(&self, headgear_style: u32) -> Result<u32, AceThrow> {
        Ok(gear(&self.headgear, headgear_style)?.weenie_default)
    }
    // ACE: SexCG.GetHeadgearClothingTable
    fn get_headgear_clothing_table(&self, headgear_style: u32) -> Result<u32, AceThrow> {
        Ok(gear(&self.headgear, headgear_style)?.clothing_table.0)
    }
    // ACE: SexCG.GetShirtWeenie
    fn get_shirt_weenie(&self, shirt_style: u32) -> Result<u32, AceThrow> {
        Ok(gear(&self.shirts, shirt_style)?.weenie_default)
    }
    // ACE: SexCG.GetShirtClothingTable
    fn get_shirt_clothing_table(&self, shirt_style: u32) -> Result<u32, AceThrow> {
        Ok(gear(&self.shirts, shirt_style)?.clothing_table.0)
    }
    // ACE: SexCG.GetPantsWeenie
    fn get_pants_weenie(&self, pants_style: u32) -> Result<u32, AceThrow> {
        Ok(gear(&self.pants, pants_style)?.weenie_default)
    }
    // ACE: SexCG.GetPantsClothingTable
    fn get_pants_clothing_table(&self, pants_style: u32) -> Result<u32, AceThrow> {
        Ok(gear(&self.pants, pants_style)?.clothing_table.0)
    }
    // ACE: SexCG.GetFootwearWeenie
    fn get_footwear_weenie(&self, footwear_style: u32) -> Result<u32, AceThrow> {
        Ok(gear(&self.footwear, footwear_style)?.weenie_default)
    }
    // ACE: SexCG.GetFootwearClothingTable
    fn get_footwear_clothing_table(&self, footwear_style: u32) -> Result<u32, AceThrow> {
        Ok(gear(&self.footwear, footwear_style)?.clothing_table.0)
    }
}
