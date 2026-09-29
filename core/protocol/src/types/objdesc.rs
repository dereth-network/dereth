//! `ObjDesc` — an object's visual appearance.
//!
//! Source: `docs/networking/messages/02-world-objects.md` §"The three descriptors",
//! transcribing the object description's own unpack.

use crate::archive::{Reader, Writer};
use crate::error::MessageError;

/// Base type for a palette DataID (`0x04000000`).
pub const PALETTE_BASE: u32 = 0x0400_0000;
/// Base type for a texture DataID (`0x05000000`).
pub const TEXTURE_BASE: u32 = 0x0500_0000;
/// Base type for a model/gfxobj DataID (`0x01000000`).
pub const MODEL_BASE: u32 = 0x0100_0000;

/// One palette-range replacement.
///
/// `offset` and `num_colors` are stored **shifted left by 3** by the client; the wire byte is the
/// value divided by 8, and `num_colors == 0` means 256 (i.e. 2048 after the shift). The shift is
/// applied in the client's own field, not on the wire, so the wire byte is what is kept here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Subpalette {
    /// Full DataID, i.e. the wire value plus [`PALETTE_BASE`].
    pub sub_id: u32,
    /// Wire byte; the client's `offset` field is this `<< 3`.
    pub offset: u8,
    /// Wire byte; the client's `numcolors` field is this `<< 3`, and 0 means 256.
    pub num_colors: u8,
}

impl Subpalette {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            sub_id: r.packed_data_id(PALETTE_BASE)?,
            offset: r.u8()?,
            num_colors: r.u8()?,
        })
    }

    pub fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.packed_data_id(PALETTE_BASE, self.sub_id)?;
        w.u8(self.offset);
        w.u8(self.num_colors);
        Ok(())
    }
}

/// One texture-map change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TextureMapChange {
    pub part_index: u8,
    pub old_tex_id: u32,
    pub new_tex_id: u32,
}

impl TextureMapChange {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            part_index: r.u8()?,
            old_tex_id: r.packed_data_id(TEXTURE_BASE)?,
            new_tex_id: r.packed_data_id(TEXTURE_BASE)?,
        })
    }

    pub fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u8(self.part_index);
        w.packed_data_id(TEXTURE_BASE, self.old_tex_id)?;
        w.packed_data_id(TEXTURE_BASE, self.new_tex_id)?;
        Ok(())
    }
}

/// One animation-part change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AnimPartChange {
    pub part_index: u8,
    pub part_id: u32,
}

impl AnimPartChange {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            part_index: r.u8()?,
            part_id: r.packed_data_id(MODEL_BASE)?,
        })
    }

    pub fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u8(self.part_index);
        w.packed_data_id(MODEL_BASE, self.part_id)?;
        Ok(())
    }
}

/// The object description.
///
/// The four counts come first as bytes; the palette DataID follows **only when
/// `numPalettes > 0`**. `UnPack` starts with `Wipe(this)`, so an `ObjDesc` always replaces the
/// previous one rather than merging into it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ObjDesc {
    /// Full DataID, or 0 when there are no subpalettes.
    pub palette_id: u32,
    pub subpalettes: Vec<Subpalette>,
    pub texture_changes: Vec<TextureMapChange>,
    pub anim_part_changes: Vec<AnimPartChange>,
}

impl ObjDesc {
    /// The version byte the client insists on. Anything else fails the unpack.
    pub const VERSION: u8 = 0x11;

    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let version = r.u8()?;
        if version != Self::VERSION {
            return Err(MessageError::InvalidValue {
                field: "object-description version",
                value: u64::from(version),
            });
        }
        let num_palettes = r.u8()?;
        let num_textures = r.u8()?;
        let num_models = r.u8()?;
        let mut d = Self::default();
        if num_palettes > 0 {
            d.palette_id = r.packed_data_id(PALETTE_BASE)?;
            for _ in 0..num_palettes {
                d.subpalettes.push(Subpalette::read(r)?);
            }
        }
        for _ in 0..num_textures {
            d.texture_changes.push(TextureMapChange::read(r)?);
        }
        for _ in 0..num_models {
            d.anim_part_changes.push(AnimPartChange::read(r)?);
        }
        r.align4()?;
        Ok(d)
    }

    pub fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        let counts = [
            self.subpalettes.len(),
            self.texture_changes.len(),
            self.anim_part_changes.len(),
        ];
        for (n, field) in counts.iter().zip([
            "object-description palette count",
            "object-description texture-map-change count",
            "object-description animation-part-change count",
        ]) {
            if *n > 255 {
                if !w.wraps_counts() {
                    return Err(MessageError::Unencodable {
                        field,
                        reason: "the count is a single byte",
                    });
                }
                w.note_count_wrapped();
            }
        }
        w.u8(Self::VERSION);
        // Guarded above, or deliberately wrapped (`Writer::with_wrapping_counts`).
        #[allow(clippy::cast_possible_truncation)]
        {
            w.u8(counts[0] as u8);
            w.u8(counts[1] as u8);
            w.u8(counts[2] as u8);
        }
        if !self.subpalettes.is_empty() {
            w.packed_data_id(PALETTE_BASE, self.palette_id)?;
            for s in &self.subpalettes {
                s.write(w)?;
            }
        }
        for t in &self.texture_changes {
            t.write(w)?;
        }
        for a in &self.anim_part_changes {
            a.write(w)?;
        }
        w.align4();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: `docs/networking/messages/02-world-objects.md` §3.1. The bytes here are laid
    /// out by hand from that table; they are a test of the documented layout, since no capture
    /// corpus exists yet (see the track report).
    #[test]
    fn objdesc_round_trips_with_a_palette_and_both_change_lists() {
        let d = ObjDesc {
            palette_id: PALETTE_BASE + 0x0123,
            subpalettes: vec![Subpalette {
                sub_id: PALETTE_BASE + 0x0456,
                offset: 2,
                num_colors: 8,
            }],
            texture_changes: vec![TextureMapChange {
                part_index: 3,
                old_tex_id: TEXTURE_BASE + 1,
                new_tex_id: TEXTURE_BASE + 2,
            }],
            anim_part_changes: vec![AnimPartChange {
                part_index: 4,
                part_id: MODEL_BASE + 9,
            }],
        };
        let mut w = Writer::new();
        d.write(&mut w).unwrap();
        let bytes = w.into_inner();
        assert_eq!(bytes[0], 0x11, "the version byte");
        assert_eq!(&bytes[1..4], &[1, 1, 1]);
        assert_eq!(bytes.len() % 4, 0, "the trailing align to 4");
        let mut r = Reader::new(&bytes);
        assert_eq!(ObjDesc::read(&mut r).unwrap(), d);
        r.expect_exhausted().unwrap();
    }

    /// With no palettes the palette DataID is absent entirely, which is the one place the layout is
    /// conditional on a count rather than on a flag.
    #[test]
    fn objdesc_omits_the_palette_id_when_there_are_no_subpalettes() {
        let d = ObjDesc::default();
        let mut w = Writer::new();
        d.write(&mut w).unwrap();
        assert_eq!(w.as_slice(), &[0x11, 0, 0, 0]);
        let bytes = w.into_inner();
        let mut r = Reader::new(&bytes);
        assert_eq!(ObjDesc::read(&mut r).unwrap(), d);
        r.expect_exhausted().unwrap();
    }

    /// A count past 255 is refused, or cut to its byte and tallied by a wrapping writer.
    #[test]
    fn a_wrapping_writer_tallies_an_objdesc_count_it_cuts() {
        let part = AnimPartChange {
            part_index: 0,
            part_id: MODEL_BASE + 1,
        };
        let d = ObjDesc {
            anim_part_changes: vec![part; 257],
            ..ObjDesc::default()
        };
        assert!(d.write(&mut Writer::new()).is_err());
        let mut w = Writer::new().with_wrapping_counts();
        d.write(&mut w).unwrap();
        assert_eq!(&w.as_slice()[..4], &[0x11, 0, 0, 1]);
        assert_eq!(w.counts_wrapped(), 1);
    }

    #[test]
    fn objdesc_rejects_a_wrong_version_byte() {
        let bytes = [0x10u8, 0, 0, 0];
        let mut r = Reader::new(&bytes);
        assert!(matches!(
            ObjDesc::read(&mut r),
            Err(MessageError::InvalidValue {
                field: "object-description version",
                ..
            })
        ));
    }
}
