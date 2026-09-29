//! `Palette`, `PaletteSet`, `Surface`, `SurfaceTexture`, `RenderSurface`,
//! `RenderTexture`.
//!
//! Record layouts are described in `docs/formats/13-palette-and-surfaces.md` and
//! `docs/formats/14-textures.md`.
//!
//! No pixel decoding happens here. `RenderSurface` exposes its payload as a byte slice for the
//! renderer.

use dereth_dat::{packobj::read_n, Cursor, DbType};
use dereth_primitives::DataId;

use crate::error::AssetError;
use crate::Decode;

// ---------------------------------------------------------------------------------------------
// 0x04 Palette
// ---------------------------------------------------------------------------------------------

/// A shipped palette holds **2,048** entries: a 256-colour table replicated eight times.
///
/// All 4,521 retail palettes have `num_colors == 2048` and are therefore exactly
/// `4 + 4 + 2048 * 4 = 8200` bytes. Sizing a palette at 256 entries silently truncates every
/// texture that uses `PFID_INDEX16`.
pub const RETAIL_PALETTE_ENTRIES: u32 = 2048;

/// A palette file is `4 (id) + 4 (count) + 2048 * 4`.
pub const RETAIL_PALETTE_BYTES: usize = 4 + 4 + 2048 * 4;

/// A decoded `0x04` Palette: 2,048 ARGB entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Palette {
    pub id: DataId,
    pub colors_argb: Vec<u32>,
}

impl Decode for Palette {
    const TYPE: DbType = DbType::Palette;

    fn declared_id(&self) -> Option<DataId> {
        Some(self.id)
    }

    fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        let id = c.data_id()?;
        let n = c.u32()? as usize;
        let colors_argb = read_n(c, n, Cursor::u32)?;
        Ok(Self { id, colors_argb })
    }
}

// ---------------------------------------------------------------------------------------------
// 0x0F PaletteSet
// ---------------------------------------------------------------------------------------------

/// A decoded `0x0F` PaletteSet: the palette ids one base may be recoloured with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteSet {
    pub id: DataId,
    pub palette_ids: Vec<DataId>,
}

impl Decode for PaletteSet {
    const TYPE: DbType = DbType::PalSet;

    fn declared_id(&self) -> Option<DataId> {
        Some(self.id)
    }

    fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        let id = c.data_id()?;
        let n = c.u32()? as usize;
        let palette_ids = read_n(c, n, Cursor::data_id)?;
        Ok(Self { id, palette_ids })
    }
}

// ---------------------------------------------------------------------------------------------
// 0x08 Surface
// ---------------------------------------------------------------------------------------------

/// A decoded `0x08` Surface.
///
/// A `0x08` file carries **no id prefix at all**. A surface record is one of the very few
/// dat types whose payload does not start with its own DataID, which is why [`Surface::declared_id`]
/// returns `None` and the echo check is skipped for it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Surface {
    /// `SurfaceType` bit field. Bits 1 and 2 (`& 6`) select the textured form.
    pub surface_type: u32,
    pub orig_texture_id: Option<DataId>,
    pub orig_palette_id: Option<DataId>,
    pub color_value: Option<u32>,
    pub translucency: f32,
    pub luminosity: f32,
    pub diffuse: f32,
}

impl Decode for Surface {
    const TYPE: DbType = DbType::Surface;

    /// No id prefix, so there is nothing to echo-check.
    fn declared_id(&self) -> Option<DataId> {
        None
    }

    fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        let surface_type = c.u32()?;
        let (orig_texture_id, orig_palette_id, color_value) = if surface_type & 6 != 0 {
            (Some(c.data_id()?), Some(c.data_id()?), None)
        } else {
            (None, None, Some(c.u32()?))
        };
        Ok(Self {
            surface_type,
            orig_texture_id,
            orig_palette_id,
            color_value,
            translucency: c.f32()?,
            luminosity: c.f32()?,
            diffuse: c.f32()?,
        })
    }
}

// ---------------------------------------------------------------------------------------------
// 0x05 image texture (`SurfaceTexture`)
// ---------------------------------------------------------------------------------------------

/// A decoded `0x05` SurfaceTexture. A categorized type: the DataID is followed by the data
/// category.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceTexture {
    pub id: DataId,
    pub data_category: u32,
    /// The serializer writes the constant byte `2` here and reads it into
    /// nothing. Every `0x05` file has it; `RenderTexture` writes a real `TextureType` in the same
    /// slot, where 2 is `TEXTURETYPE_2D`. Decoded and kept rather than skipped blind.
    pub unknown_byte: u8,
    pub source_levels: Vec<DataId>,
}

impl Decode for SurfaceTexture {
    const TYPE: DbType = DbType::SurfaceTexture;

    fn declared_id(&self) -> Option<DataId> {
        Some(self.id)
    }

    fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        let id = c.data_id()?;
        let data_category = c.u32()?;
        let unknown_byte = c.u8()?;
        let n = c.u32()? as usize;
        let source_levels = read_n(c, n, Cursor::data_id)?;
        Ok(Self {
            id,
            data_category,
            unknown_byte,
            source_levels,
        })
    }
}

// ---------------------------------------------------------------------------------------------
// 0x15 RenderTexture
// ---------------------------------------------------------------------------------------------

/// A decoded `0x15` RenderTexture. Same shape as an image texture, except that the
/// byte in the same slot is a real `TextureType` (2 = `TEXTURETYPE_2D`).
///
/// Two placeholder files ship; the type belongs to the unused engine-2 renderer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderTexture {
    pub id: DataId,
    pub data_category: u32,
    pub texture_type: u8,
    pub source_levels: Vec<DataId>,
}

impl Decode for RenderTexture {
    const TYPE: DbType = DbType::RenderTexture;

    fn declared_id(&self) -> Option<DataId> {
        Some(self.id)
    }

    fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        let id = c.data_id()?;
        let data_category = c.u32()?;
        let texture_type = c.u8()?;
        let n = c.u32()? as usize;
        let source_levels = read_n(c, n, Cursor::data_id)?;
        Ok(Self {
            id,
            data_category,
            texture_type,
            source_levels,
        })
    }
}

// ---------------------------------------------------------------------------------------------
// 0x06 RenderSurface
// ---------------------------------------------------------------------------------------------

/// `PFID_P8`, the 8-bit paletted format; it carries a default palette id after the payload.
pub const PFID_P8: u32 = 41;
/// `PFID_INDEX16`, the 16-bit paletted format, the other one with a trailing palette id. (The two
/// names are easy to swap, and nothing but the symmetric test below would notice.)
pub const PFID_INDEX16: u32 = 101;
/// `PFID_CUSTOM_RAW_JPEG` — 79 records in the portal dat, six of them progressive.
pub const PFID_CUSTOM_RAW_JPEG: u32 = 0x1F4;

/// The six `PFID_CUSTOM_RAW_JPEG` records that are **progressive** (SOF2) rather than baseline
/// (SOF0). A baseline-only decoder passes 73 images and silently fails these.
/// A baseline-only decoder therefore passes most records but silently fails these six.
pub const PROGRESSIVE_JPEG_IDS: [u32; 6] = [
    0x0600_66AB,
    0x0600_66CE,
    0x0600_6701,
    0x0600_67CD,
    0x0600_67F2,
    0x0600_681C,
];

/// A decoded `0x06` RenderSurface: the header, plus the payload as a byte range.
///
/// No pixel decoding: DXT, JPEG and palette expansion are the renderer's. The payload starts at
/// `data_offset` (`+0x18` from the start of the record) and is `image_size` bytes long.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderSurface {
    pub id: DataId,
    /// Takes 0–10; only 6 and 10 are interpreted by the client, and
    /// the high-res dat uses 1,2,4,5,7,8,9 where the portal dat uses 6. Passed through untouched.
    pub data_category: u32,
    pub width: u32,
    pub height: u32,
    /// A pixel-format id; see `docs/formats/14-textures.md`.
    pub format: u32,
    pub image_size: u32,
    /// Offset of the pixel payload within the record.
    pub data_offset: usize,
    /// `format == 41 || format == 101`.
    pub default_palette_id: Option<DataId>,
}

impl RenderSurface {
    /// The pixel payload, as a slice of the record this header was decoded from.
    #[must_use]
    pub fn payload<'a>(&self, record: &'a [u8]) -> Option<&'a [u8]> {
        record.get(self.data_offset..self.data_offset + self.image_size as usize)
    }
}

impl Decode for RenderSurface {
    const TYPE: DbType = DbType::RenderSurface;

    fn declared_id(&self) -> Option<DataId> {
        Some(self.id)
    }

    fn decode(c: &mut Cursor<'_>) -> Result<Self, AssetError> {
        let id = c.data_id()?;
        let data_category = c.u32()?;
        let width = c.u32()?;
        let height = c.u32()?;
        let format = c.u32()?;
        let image_size = c.u32()?;
        let data_offset = c.position();
        debug_assert_eq!(data_offset, 0x18, "the payload starts at +0x18");
        c.skip(image_size as usize)?;
        let default_palette_id = if format == PFID_INDEX16 || format == PFID_P8 {
            Some(c.data_id()?)
        } else {
            None
        };
        Ok(Self {
            id,
            data_category,
            width,
            height,
            format,
            image_size,
            data_offset,
            default_palette_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A retail palette is exactly 8200 bytes.
    #[test]
    fn a_retail_palette_is_exactly_8200_bytes() {
        assert_eq!(RETAIL_PALETTE_BYTES, 8200);
        assert_eq!(RETAIL_PALETTE_ENTRIES, 2048);
        assert_eq!(RETAIL_PALETTE_ENTRIES, 256 * 8);
    }

    /// Contract 9.11: a `0x08` file has no id prefix, so the first dword is the surface type.
    #[test]
    fn a_surface_starts_with_its_type_not_an_id() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&0u32.to_le_bytes()); // type 0 -> colour form
        buf.extend_from_slice(&0xFF00_00FFu32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&1.0f32.to_le_bytes());
        let s = Surface::decode_payload(DataId(0x0800_0000), &buf).unwrap();
        assert_eq!(s.surface_type, 0);
        assert_eq!(s.color_value, Some(0xFF00_00FF));
        assert_eq!(s.orig_texture_id, None);
        assert_eq!(s.declared_id(), None);
    }

    /// The progressive jpeg ids are the documented six.
    #[test]
    fn the_progressive_jpeg_ids_are_the_documented_six() {
        assert_eq!(PROGRESSIVE_JPEG_IDS.len(), 6);
        assert!(PROGRESSIVE_JPEG_IDS.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(PROGRESSIVE_JPEG_IDS[0], 0x0600_66AB);
        assert_eq!(PROGRESSIVE_JPEG_IDS[5], 0x0600_681C);
    }
}

// ---------------------------------------------------------------------------------------------
// 0x16 / 0x17 / 0x18 — the unused engine-2 material system
// ---------------------------------------------------------------------------------------------

/// `RenderMaterial` (`0x16`), `MaterialModifier` (`0x17`) and `MaterialInstance` (`0x18`): one
/// placeholder file each, read by nothing in the shipped renderer.
///
/// The wire layouts are not worth a day's work for three unused files, so this keeps the DataID
/// (which all three do carry) and the rest of the payload verbatim. That
/// is enough for the exhaustive gate to account for every byte without inventing a layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterialBlob {
    pub id: DataId,
    pub body: Vec<u8>,
}

impl MaterialBlob {
    /// Decode a blob of a given type. There is no `Decode` impl because the three types share one
    /// body and `Decode::TYPE` can only name one.
    pub fn decode_payload(id: DataId, bytes: &[u8]) -> Result<Self, AssetError> {
        let mut c = Cursor::new(bytes);
        let found = c.data_id()?;
        crate::dbobj::check_id_echo(id, found)?;
        let body = c.bytes(c.remaining())?.to_vec();
        c.expect_end()?;
        Ok(Self { id: found, body })
    }
}
