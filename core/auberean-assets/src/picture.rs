//! Pictures (`.dds`, portal and high-resolution files `0x41000000`-`0x410FFFFF`, local files
//! `0x41100000`+) and textures, the lists of a picture's levels of detail (`.texture`,
//! `0x40000000`-`0x400FFFFF`, local `0x40100000`+).

use auberean_dat::reader::Result;
use auberean_dat::Reader;
use auberean_primitives::DataId;

use crate::{Context, Decode};

/// One picture: its size, its pixel format and its bytes.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Picture {
    /// The category word every picture carries after its id.
    pub category: u32,
    pub width: u32,
    pub height: u32,
    /// The pixel format: a Direct3D format number (`0x15` A8R8G8B8, `0x14` R8G8B8, `0x17` R5G6B5,
    /// `0x1C` 8-bit alpha, `0x29` 8-bit palette, `0x32` 8-bit luminance...), a four-character code (`DXT1`, `DXT5`...) or `0x1F4`
    /// for JPEG.
    pub format: u32,
    pub bytes: Vec<u8>,
}

impl Picture {
    /// The pixel format's usual name.
    #[must_use]
    pub fn format_name(&self) -> String {
        match self.format {
            0x14 => "R8G8B8".into(),
            0x15 => "A8R8G8B8".into(),
            0x16 => "X8R8G8B8".into(),
            0x17 => "R5G6B5".into(),
            0x19 => "A1R5G5B5".into(),
            0x1A => "A4R4G4B4".into(),
            0x1C => "A8".into(),
            0x29 => "P8".into(),
            0x32 => "L8".into(),
            0x1F4 => "JPEG".into(),
            f => {
                let b = f.to_le_bytes();
                if b.iter().all(u8::is_ascii_alphanumeric) {
                    b.iter().map(|c| char::from(*c)).collect()
                } else {
                    format!("{f:#x}")
                }
            }
        }
    }
}

impl Decode for Picture {
    /// The id, the category word, width, height, format, a byte count and the bytes.
    // D2-09943, D2-12693
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        let category = r.u32()?;
        let width = r.u32()?;
        let height = r.u32()?;
        let format = r.u32()?;
        let n = r.count(1)?;
        let bytes = r.bytes(n)?.to_vec();
        Ok(Self {
            category,
            width,
            height,
            format,
            bytes,
        })
    }
}

/// The pictures of one texture, a picture per level of detail, largest first.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Texture {
    pub kind: u32,
    pub levels: Vec<DataId>,
}

impl Decode for Texture {
    /// The id, a kind word, then a counted list of picture ids.
    // D2-10022
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        let kind = r.u32()?;
        let levels = r.list(4, Reader::data_id)?;
        Ok(Self { kind, levels })
    }
}
