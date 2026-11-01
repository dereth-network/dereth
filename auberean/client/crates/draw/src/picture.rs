//! Pictures as drawing needs them: a picture list's levels, expanded to 8-bit blue-green-red-alpha
//! texels, the largest first; a texture is made of them, one level of detail per stored level.

use auberean_assets::picture::{Picture, Texture};
use auberean_world::records::WorldRecords;
use dereth_render_cpu::pixel_format::PixelFormatId;

/// A picture expanded to texels: `width * height` of blue, green, red, alpha.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
    /// The picture list's smaller levels as stored, each half the one before; a texture's levels
    /// of detail are these, never made by averaging. Empty for a single picture.
    pub smaller: Vec<Image>,
}

/// The stored formats a picture can have.
mod format {
    pub const DXT1: u32 = 0x3154_5844;
    pub const DXT3: u32 = 0x3354_5844;
    pub const DXT5: u32 = 0x3554_5844;
    pub const R8G8B8: u32 = 0x14;
    pub const A8R8G8B8: u32 = 0x15;
    pub const X8R8G8B8: u32 = 0x16;
    pub const A8: u32 = 0x1C;
    pub const JPEG: u32 = 0x1F4;
}

/// Expand one picture's texels, or `None` for a format or size that does not read.
#[must_use]
pub fn expand(p: &Picture) -> Option<Image> {
    let (w, h) = (p.width, p.height);
    let n = usize::try_from(w).ok()? * usize::try_from(h).ok()?;
    let bgra = match p.format {
        format::DXT1 | format::DXT3 | format::DXT5 => {
            let id = match p.format {
                format::DXT1 => PixelFormatId::Dxt1,
                format::DXT3 => PixelFormatId::Dxt3,
                _ => PixelFormatId::Dxt5,
            };
            dereth_render_cpu::dxt::decode(id, &p.bytes, w, h).ok()?
        }
        format::R8G8B8 => {
            let src = p.bytes.get(..n * 3)?;
            src.as_chunks::<3>()
                .0
                .iter()
                .flat_map(|c| [c[0], c[1], c[2], 0xFF])
                .collect()
        }
        format::A8R8G8B8 => p.bytes.get(..n * 4)?.to_vec(),
        format::X8R8G8B8 => p
            .bytes
            .get(..n * 4)?
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|c| [c[0], c[1], c[2], 0xFF])
            .collect(),
        format::A8 => p
            .bytes
            .get(..n)?
            .iter()
            .flat_map(|&a| [0xFF, 0xFF, 0xFF, a])
            .collect(),
        format::JPEG => {
            let j = dereth_render_cpu::jpeg::decode(&p.bytes).ok()?;
            return Some(Image {
                width: j.width,
                height: j.height,
                bgra: j
                    .bgr
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .flat_map(|c| [c[0], c[1], c[2], 0xFF])
                    .collect(),
                smaller: Vec::new(),
            });
        }
        _ => return None,
    };
    (bgra.len() == n * 4).then_some(Image {
        width: w,
        height: h,
        bgra,
        smaller: Vec::new(),
    })
}

/// How many of a picture list's `levels` largest levels a texture leaves out at texture detail
/// `detail` (0 lowest to 4 highest): four, three, two or one, none at the highest when
/// `highest_kept`; never every level.
// D2-10018
#[must_use]
pub fn dropped_levels(detail: u32, highest_kept: bool, levels: usize) -> usize {
    let drop = match detail {
        0 => 4,
        1 => 3,
        2 => 2,
        3 => 1,
        4 if highest_kept => return 0,
        4 => 1,
        _ => return 0,
    };
    if levels <= 1 {
        0
    } else {
        drop.min(levels - 1)
    }
}

/// Picture list `id`'s levels, expanded, at the highest texture detail: the first the install holds
/// and the smaller ones after it while each is present and half the one before; a picture id
/// (`0x41...`) is taken as a list of one.
// D2-10004, D2-10005
#[must_use]
pub fn load(records: &WorldRecords, id: u32) -> Option<Image> {
    if id >> 24 == 0x41 {
        return expand(&records.get::<Picture>(id)?);
    }
    let list: Texture = records.get(id)?;
    let skip = dropped_levels(4, true, list.levels.len());
    let mut levels = list.levels[skip..]
        .iter()
        .map(|level| records.get::<Picture>(level.0).and_then(|p| expand(&p)))
        .skip_while(Option::is_none);
    let mut top = levels.next()??;
    let (mut w, mut h) = (top.width, top.height);
    for level in levels {
        let Some(img) = level else {
            break;
        };
        if (w, h) == (1, 1) || img.width != (w / 2).max(1) || img.height != (h / 2).max(1) {
            break;
        }
        (w, h) = (img.width, img.height);
        top.smaller.push(img);
    }
    Some(top)
}
