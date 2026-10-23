//! The pictures a pack makes, as the files of each era hold them.
//!
//! A pack describes a picture (`[[render-surface]]`), an image texture that names it
//! (`[[surface-texture]]`) and a surface laying that texture (`[[surface]]`), in every era alike.
//! The end of retail's files keep them so: the picture is an image record in the pack's format,
//! and the image texture lists its images. Infiltration's files keep the pixels in the image
//! texture itself: the texture holds its first image's pixels in one of that era's 16-bit
//! formats (`A4R4G4B4` when any pixel is transparent, `R5G6B5` otherwise), and the picture is
//! that era's image record, three bytes a pixel.

use dereth_assets::material::PFID_CUSTOM_B8G8R8;
use dereth_assets::{AssetError, Decode, RenderSurface};
use dereth_dat::ContainerEra;
use dereth_primitives::{DataId, EraId};

/// A picture as a pack's source holds it: its size and four bytes a pixel, red, green, blue,
/// alpha, top row first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Rgba {
    /// A picture from pixels four bytes each in blue, green, red, alpha order (`A8R8G8B8`).
    #[must_use]
    pub fn from_bgra(width: u32, height: u32, bgra: &[u8]) -> Self {
        Self {
            width,
            height,
            pixels: bgra
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|p| [p[2], p[1], p[0], p[3]])
                .collect(),
        }
    }
}

/// `A4R4G4B4`'s pixel-format id.
const PFID_A4R4G4B4: u32 = 26;
/// `R5G6B5`'s pixel-format id.
const PFID_R5G6B5: u32 = 23;

/// The image record `id` holding `picture` in the files of `era`: in the older files, three
/// bytes a pixel, red, green, blue; in the later, what `later` gives: the pixel format's id, the
/// category and the pixels in that format.
///
/// # Errors
/// The era's encoder refuses the picture.
pub fn image_record(
    era: EraId,
    id: DataId,
    picture: &Rgba,
    later: impl FnOnce() -> (u32, u32, Vec<u8>),
) -> Result<Vec<u8>, AssetError> {
    let (format, data_category, data_offset, px) = match era.container_era() {
        ContainerEra::Classic => (
            PFID_CUSTOM_B8G8R8,
            0,
            12,
            picture
                .pixels
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|p| [p[0], p[1], p[2]])
                .collect(),
        ),
        ContainerEra::Modern => {
            let (format, category, px) = later();
            (format, category, 0x18, px)
        }
    };
    RenderSurface {
        id,
        data_category,
        width: picture.width,
        height: picture.height,
        format,
        image_size: u32::try_from(px.len()).unwrap_or(u32::MAX),
        data_offset,
        default_palette_id: None,
    }
    .encode_for(era, &px)
}

/// The image texture `id` holding `picture` itself, for an era whose image textures carry their
/// pixels: `A4R4G4B4` when any of its pixels is transparent, else `R5G6B5`.
///
/// # Errors
/// The era's image textures name their images instead, or its encoder refuses the picture.
pub fn texture_holding(era: EraId, id: DataId, picture: &Rgba) -> Result<Vec<u8>, AssetError> {
    let quads = picture.pixels.as_chunks::<4>().0;
    let alpha = quads.iter().any(|p| p[3] != 255);
    let px: Vec<u8> = if alpha {
        quads
            .iter()
            .flat_map(|p| {
                let v = (u16::from(p[3] >> 4) << 12)
                    | (u16::from(p[0] >> 4) << 8)
                    | (u16::from(p[1] >> 4) << 4)
                    | u16::from(p[2] >> 4);
                v.to_le_bytes()
            })
            .collect()
    } else {
        quads
            .iter()
            .flat_map(|p| {
                let v = (u16::from(p[0] >> 3) << 11)
                    | (u16::from(p[1] >> 2) << 5)
                    | u16::from(p[2] >> 3);
                v.to_le_bytes()
            })
            .collect()
    };
    RenderSurface {
        id,
        data_category: 0,
        width: picture.width,
        height: picture.height,
        format: if alpha { PFID_A4R4G4B4 } else { PFID_R5G6B5 },
        image_size: u32::try_from(px.len()).unwrap_or(u32::MAX),
        data_offset: 16,
        default_palette_id: None,
    }
    .encode_texture_for(era, &px)
}

/// The picture an older image record holds (three bytes a pixel), as a pack's picture, opaque.
///
/// # Errors
/// The bytes are not an image record of `era`'s files in that layout.
pub fn picture_of_image(era: EraId, id: DataId, bytes: &[u8]) -> Result<Rgba, AssetError> {
    let rs = RenderSurface::decode_payload_for(era, id, bytes)?;
    if rs.format != PFID_CUSTOM_B8G8R8 {
        return Err(AssetError::Unsupported {
            what: "pixel format of an image a texture is made from",
            value: rs.format,
        });
    }
    let px = rs.payload(bytes).ok_or(AssetError::Unsupported {
        what: "image size",
        value: rs.image_size,
    })?;
    Ok(Rgba {
        width: rs.width,
        height: rs.height,
        pixels: px
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: a pack's pictures in each era's records)
    use super::*;

    fn picture(alpha: u8) -> Rgba {
        Rgba {
            width: 2,
            height: 1,
            pixels: vec![0xFF, 0x80, 0x00, alpha, 0x10, 0x20, 0x30, 255],
        }
    }

    #[test]
    fn an_older_texture_holds_its_picture_in_sixteen_bits() {
        let era = EraId::Infiltration;
        let id = DataId(0x0500_4000);
        let opaque = texture_holding(era, id, &picture(255)).unwrap();
        let rs = RenderSurface::from_classic_texture(id, &opaque).unwrap();
        assert_eq!(rs.format, PFID_R5G6B5);
        // Orange: red 31, green 32, blue 0.
        assert_eq!(&rs.payload(&opaque).unwrap()[..2], &0xFC00u16.to_le_bytes());
        let clear = texture_holding(era, id, &picture(0x80)).unwrap();
        let rs = RenderSurface::from_classic_texture(id, &clear).unwrap();
        assert_eq!(rs.format, PFID_A4R4G4B4);
        assert_eq!(&rs.payload(&clear).unwrap()[..2], &0x8F80u16.to_le_bytes());
        assert!(texture_holding(EraId::Eor, id, &picture(255)).is_err());
    }

    #[test]
    fn an_older_image_is_its_picture_three_bytes_a_pixel_and_reads_back() {
        let era = EraId::Infiltration;
        let id = DataId(0x0600_4000);
        let b = image_record(era, id, &picture(255), || unreachable!()).unwrap();
        assert_eq!(b.len(), 12 + 6);
        assert_eq!(&b[12..], [0xFF, 0x80, 0x00, 0x10, 0x20, 0x30]);
        assert_eq!(picture_of_image(era, id, &b).unwrap(), picture(255));
        let later = image_record(EraId::Eor, id, &picture(255), || (20, 4, vec![0; 6])).unwrap();
        let rs = RenderSurface::decode_payload(id, &later).unwrap();
        assert_eq!((rs.format, rs.data_category), (20, 4));
    }
}
