//! [`PixelFormatId`] and [`PixelFormatDesc`] — the source-side pixel format table.
//!
//! This module contains the complete pixel-format table, raw enum values and decoding rules.
//!
//! The pixel format id is D3D9's `D3DFORMAT` for values 20-102, extended with five Turbine formats
//! and a "raw JPEG" marker. Because values 20-102 are literally `D3DFORMAT`, the client passes them
//! to D3D unconverted; the five `PFID_CUSTOM_*` values and `PFID_CUSTOM_RAW_JPEG` are **never**
//! given to D3D — they only appear as *source* formats.

/// A source or device pixel format. Only the values the client can actually meet are named; the
/// rest reach the format lookup and come back `None`, which corresponds to returning
/// false.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
#[allow(non_camel_case_types)]
pub enum PixelFormatId {
    Unknown = 0,
    R8G8B8 = 20,
    A8R8G8B8 = 21,
    X8R8G8B8 = 22,
    R5G6B5 = 23,
    X1R5G5B5 = 24,
    A1R5G5B5 = 25,
    A4R4G4B4 = 26,
    A8 = 28,
    X4R4G4B4 = 30,
    A2B10G10R10 = 31,
    A8B8G8R8 = 32,
    X8B8G8R8 = 33,
    A2R10G10B10 = 35,
    P8 = 41,
    V8U8 = 60,
    D16Lockable = 70,
    D32 = 71,
    D15S1 = 73,
    D24S8 = 75,
    D24X8 = 77,
    D24X4S4 = 79,
    D16 = 80,
    Index16 = 101,
    CustomR8G8B8A8 = 240,
    CustomA8B8G8R8 = 241,
    CustomB8G8R8 = 242,
    CustomLscapeR8G8B8 = 243,
    CustomLscapeAlpha = 244,
    CustomRawJpeg = 500,
    Dxt1 = 0x3154_5844,
    Dxt2 = 0x3254_5844,
    Dxt3 = 0x3354_5844,
    Dxt4 = 0x3454_5844,
    Dxt5 = 0x3554_5844,
    /// Any value rejects, carried so a decoder can report it.
    Other(u32),
}

impl PixelFormatId {
    #[must_use]
    pub const fn from_raw(v: u32) -> Self {
        match v {
            0 => Self::Unknown,
            20 => Self::R8G8B8,
            21 => Self::A8R8G8B8,
            22 => Self::X8R8G8B8,
            23 => Self::R5G6B5,
            24 => Self::X1R5G5B5,
            25 => Self::A1R5G5B5,
            26 => Self::A4R4G4B4,
            28 => Self::A8,
            30 => Self::X4R4G4B4,
            31 => Self::A2B10G10R10,
            32 => Self::A8B8G8R8,
            33 => Self::X8B8G8R8,
            35 => Self::A2R10G10B10,
            41 => Self::P8,
            60 => Self::V8U8,
            70 => Self::D16Lockable,
            71 => Self::D32,
            73 => Self::D15S1,
            75 => Self::D24S8,
            77 => Self::D24X8,
            79 => Self::D24X4S4,
            80 => Self::D16,
            101 => Self::Index16,
            240 => Self::CustomR8G8B8A8,
            241 => Self::CustomA8B8G8R8,
            242 => Self::CustomB8G8R8,
            243 => Self::CustomLscapeR8G8B8,
            244 => Self::CustomLscapeAlpha,
            500 => Self::CustomRawJpeg,
            0x3154_5844 => Self::Dxt1,
            0x3254_5844 => Self::Dxt2,
            0x3354_5844 => Self::Dxt3,
            0x3454_5844 => Self::Dxt4,
            0x3554_5844 => Self::Dxt5,
            other => Self::Other(other),
        }
    }

    #[must_use]
    pub const fn raw(self) -> u32 {
        match self {
            Self::Other(v) => v,
            Self::Unknown => 0,
            Self::R8G8B8 => 20,
            Self::A8R8G8B8 => 21,
            Self::X8R8G8B8 => 22,
            Self::R5G6B5 => 23,
            Self::X1R5G5B5 => 24,
            Self::A1R5G5B5 => 25,
            Self::A4R4G4B4 => 26,
            Self::A8 => 28,
            Self::X4R4G4B4 => 30,
            Self::A2B10G10R10 => 31,
            Self::A8B8G8R8 => 32,
            Self::X8B8G8R8 => 33,
            Self::A2R10G10B10 => 35,
            Self::P8 => 41,
            Self::V8U8 => 60,
            Self::D16Lockable => 70,
            Self::D32 => 71,
            Self::D15S1 => 73,
            Self::D24S8 => 75,
            Self::D24X8 => 77,
            Self::D24X4S4 => 79,
            Self::D16 => 80,
            Self::Index16 => 101,
            Self::CustomR8G8B8A8 => 240,
            Self::CustomA8B8G8R8 => 241,
            Self::CustomB8G8R8 => 242,
            Self::CustomLscapeR8G8B8 => 243,
            Self::CustomLscapeAlpha => 244,
            Self::CustomRawJpeg => 500,
            Self::Dxt1 => 0x3154_5844,
            Self::Dxt2 => 0x3254_5844,
            Self::Dxt3 => 0x3354_5844,
            Self::Dxt4 => 0x3454_5844,
            Self::Dxt5 => 0x3554_5844,
        }
    }
}

/// Pixel-format flags stored as a bitfield.
pub mod pf_flags {
    /// Has RGB.
    pub const RGB: u32 = 1;
    /// Has alpha.
    pub const ALPHA: u32 = 2;
    /// Compressed; the FourCC code is valid.
    pub const COMPRESSED: u32 = 4;
    /// "Raw" — the JPEG passthrough.
    pub const RAW: u32 = 0x10;
    /// Palettised / index.
    pub const INDEXED: u32 = 0x40;
}

/// What the client's pixel-format descriptor fills in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelFormatDesc {
    pub format: PixelFormatId,
    pub flags: u32,
    pub bits_per_pixel: u32,
    pub red_mask: u32,
    pub green_mask: u32,
    pub blue_mask: u32,
    pub alpha_mask: u32,
}

impl PixelFormatDesc {
    /// The observed pixel-format-id to descriptor table. An unlisted format returns `None`,
    /// corresponding to a failed format selection; it is unsupported.
    #[must_use]
    pub fn for_format(format: PixelFormatId) -> Option<Self> {
        use pf_flags as f;
        let d = |flags, bpp, r, g, b, a| Self {
            format,
            flags,
            bits_per_pixel: bpp,
            red_mask: r,
            green_mask: g,
            blue_mask: b,
            alpha_mask: a,
        };
        Some(match format {
            PixelFormatId::R8G8B8 | PixelFormatId::CustomLscapeR8G8B8 => {
                d(f::RGB, 24, 0x00FF_0000, 0x0000_FF00, 0x0000_00FF, 0)
            }
            PixelFormatId::A8R8G8B8 => d(
                f::RGB | f::ALPHA,
                32,
                0x00FF_0000,
                0x0000_FF00,
                0x0000_00FF,
                0xFF00_0000,
            ),
            PixelFormatId::X8R8G8B8 => d(f::RGB, 32, 0x00FF_0000, 0x0000_FF00, 0x0000_00FF, 0),
            PixelFormatId::R5G6B5 => d(f::RGB, 16, 0xF800, 0x07E0, 0x001F, 0),
            PixelFormatId::X1R5G5B5 => d(f::RGB, 16, 0x7C00, 0x03E0, 0x001F, 0),
            PixelFormatId::A1R5G5B5 => d(f::RGB | f::ALPHA, 16, 0x7C00, 0x03E0, 0x001F, 0x8000),
            PixelFormatId::A4R4G4B4 => d(f::RGB | f::ALPHA, 16, 0x0F00, 0x00F0, 0x000F, 0xF000),
            PixelFormatId::X4R4G4B4 => d(f::RGB, 16, 0x0F00, 0x00F0, 0x000F, 0),
            PixelFormatId::A8 | PixelFormatId::CustomLscapeAlpha => d(f::ALPHA, 8, 0, 0, 0, 0xFF),
            PixelFormatId::A2B10G10R10 => d(
                f::RGB | f::ALPHA,
                32,
                0x0000_03FF,
                0x000F_FC00,
                0x3FF0_0000,
                0xC000_0000,
            ),
            PixelFormatId::A8B8G8R8 => d(
                f::RGB | f::ALPHA,
                32,
                0x0000_00FF,
                0x0000_FF00,
                0x00FF_0000,
                0xFF00_0000,
            ),
            PixelFormatId::X8B8G8R8 => d(f::RGB, 32, 0x0000_00FF, 0x0000_FF00, 0x00FF_0000, 0),
            PixelFormatId::A2R10G10B10 => d(
                f::RGB | f::ALPHA,
                32,
                0x3FF0_0000,
                0x000F_FC00,
                0x0000_03FF,
                0xC000_0000,
            ),
            PixelFormatId::P8 => d(f::INDEXED, 8, 0, 0, 0, 0),
            PixelFormatId::V8U8 => d(f::RGB | 8, 16, 0xFF00, 0x00FF, 0, 0),
            PixelFormatId::D16Lockable | PixelFormatId::D16 => {
                d(f::ALPHA, 16, 0, 0, 0, 0x0000_FFFF)
            }
            PixelFormatId::D32 => d(f::ALPHA, 32, 0, 0, 0, 0xFFFF_FFFF),
            PixelFormatId::D15S1 => d(f::ALPHA, 16, 0, 0, 0, 0x0000_FFFE),
            PixelFormatId::D24S8 | PixelFormatId::D24X8 | PixelFormatId::D24X4S4 => {
                d(f::ALPHA, 32, 0, 0, 0, 0xFFFF_FF00)
            }
            PixelFormatId::Index16 => d(f::INDEXED, 16, 0, 0, 0, 0),
            PixelFormatId::CustomR8G8B8A8 => d(
                f::RGB | f::ALPHA,
                32,
                0xFF00_0000,
                0x00FF_0000,
                0x0000_FF00,
                0x0000_00FF,
            ),
            PixelFormatId::CustomA8B8G8R8 => d(
                f::RGB | f::ALPHA,
                32,
                0x0000_00FF,
                0x0000_FF00,
                0x00FF_0000,
                0xFF00_0000,
            ),
            PixelFormatId::CustomB8G8R8 => d(f::RGB, 24, 0x0000_00FF, 0x0000_FF00, 0x00FF_0000, 0),
            PixelFormatId::CustomRawJpeg => d(f::RAW | f::RGB, 0, 0, 0, 0, 0),
            // DXT1 is 4 bits per pixel; DXT2-5 are 8.
            PixelFormatId::Dxt1 => d(f::COMPRESSED, 4, 0, 0, 0, 0),
            PixelFormatId::Dxt2
            | PixelFormatId::Dxt3
            | PixelFormatId::Dxt4
            | PixelFormatId::Dxt5 => d(f::COMPRESSED, 8, 0, 0, 0, 0),
            _ => return None,
        })
    }

    #[must_use]
    pub const fn has_rgb(&self) -> bool {
        self.flags & pf_flags::RGB != 0
    }
    #[must_use]
    pub const fn has_alpha(&self) -> bool {
        self.flags & pf_flags::ALPHA != 0
    }
    #[must_use]
    pub const fn is_compressed(&self) -> bool {
        self.flags & pf_flags::COMPRESSED != 0
    }
    #[must_use]
    pub const fn is_indexed(&self) -> bool {
        self.flags & pf_flags::INDEXED != 0
    }
    #[must_use]
    pub const fn is_raw(&self) -> bool {
        self.flags & pf_flags::RAW != 0
    }

    /// The number of bytes one row of `width` pixels occupies, uncompressed.
    #[must_use]
    pub const fn row_bytes(&self, width: u32) -> usize {
        (self.bits_per_pixel as usize * width as usize).div_ceil(8)
    }

    /// The whole image's byte size, handling the 4×4 block layout of the compressed formats.
    #[must_use]
    pub fn image_bytes(&self, width: u32, height: u32) -> usize {
        if self.is_compressed() {
            let bw = width.div_ceil(4) as usize;
            let bh = height.div_ceil(4) as usize;
            // 4 bpp -> 8 bytes per block; 8 bpp -> 16.
            let block = if self.bits_per_pixel == 4 { 8 } else { 16 };
            bw * bh * block
        } else {
            self.row_bytes(width) * height as usize
        }
    }
}

/// The client's "is this a D3D format" test.
///
/// Returns **false** for exactly four formats — `P8`, `INDEX16`, `CUSTOM_LSCAPE_R8G8B8`,
/// `CUSTOM_LSCAPE_ALPHA` — and true for everything else. The four false cases need CPU decoding.
#[must_use]
pub const fn is_d3d_format(format: PixelFormatId) -> bool {
    !matches!(
        format,
        PixelFormatId::P8
            | PixelFormatId::Index16
            | PixelFormatId::CustomLscapeR8G8B8
            | PixelFormatId::CustomLscapeAlpha
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `X8R8G8B8` has the colour masks of `A8R8G8B8` and no alpha mask, so its unused top byte
    /// never makes a texel transparent.
    #[test]
    fn x8r8g8b8_has_the_colour_masks_and_no_alpha_mask() {
        let d = PixelFormatDesc::for_format(PixelFormatId::X8R8G8B8).expect("a known format");
        let a = PixelFormatDesc::for_format(PixelFormatId::A8R8G8B8).expect("a known format");
        assert_eq!(d.bits_per_pixel, 32);
        assert_eq!(
            (d.red_mask, d.green_mask, d.blue_mask),
            (a.red_mask, a.green_mask, a.blue_mask)
        );
        assert_eq!(d.alpha_mask, 0);
    }

    // The verified format test returns false for exactly four formats: `PixelFormatId::P8`,
    // INDEX16, CUSTOM_LSCAPE_R8G8B8, CUSTOM_LSCAPE_ALPHA -- and true for everything else.
    #[test]
    fn is_d3d_format_is_false_for_exactly_four_formats() {
        let false_set = [
            PixelFormatId::P8,
            PixelFormatId::Index16,
            PixelFormatId::CustomLscapeR8G8B8,
            PixelFormatId::CustomLscapeAlpha,
        ];
        for f in false_set {
            assert!(!is_d3d_format(f), "{f:?} must not be a D3D format");
        }
        // Every other named value in the enum must be true. Walk the whole enum rather than a
        // hand-picked sample.
        let everything_else = [
            PixelFormatId::Unknown,
            PixelFormatId::R8G8B8,
            PixelFormatId::A8R8G8B8,
            PixelFormatId::X8R8G8B8,
            PixelFormatId::R5G6B5,
            PixelFormatId::X1R5G5B5,
            PixelFormatId::A1R5G5B5,
            PixelFormatId::A4R4G4B4,
            PixelFormatId::A8,
            PixelFormatId::X4R4G4B4,
            PixelFormatId::A2B10G10R10,
            PixelFormatId::A8B8G8R8,
            PixelFormatId::X8B8G8R8,
            PixelFormatId::A2R10G10B10,
            PixelFormatId::V8U8,
            PixelFormatId::D16Lockable,
            PixelFormatId::D32,
            PixelFormatId::D15S1,
            PixelFormatId::D24S8,
            PixelFormatId::D24X8,
            PixelFormatId::D24X4S4,
            PixelFormatId::D16,
            PixelFormatId::CustomR8G8B8A8,
            PixelFormatId::CustomA8B8G8R8,
            PixelFormatId::CustomB8G8R8,
            PixelFormatId::CustomRawJpeg,
            PixelFormatId::Dxt1,
            PixelFormatId::Dxt2,
            PixelFormatId::Dxt3,
            PixelFormatId::Dxt4,
            PixelFormatId::Dxt5,
            PixelFormatId::Other(1234),
        ];
        for f in everything_else {
            assert!(is_d3d_format(f), "{f:?} must be a D3D format");
        }
    }

    // Oracle: the raw enum values.
    #[test]
    fn the_enum_values_round_trip() {
        for raw in [
            0u32,
            20,
            21,
            22,
            23,
            24,
            25,
            26,
            28,
            30,
            31,
            32,
            33,
            35,
            41,
            60,
            70,
            71,
            73,
            75,
            77,
            79,
            80,
            101,
            240,
            241,
            242,
            243,
            244,
            500,
            0x3154_5844,
            0x3554_5844,
        ] {
            assert_eq!(PixelFormatId::from_raw(raw).raw(), raw);
        }
        assert_eq!(PixelFormatId::from_raw(0x1234).raw(), 0x1234);
        // The DXT fourCCs really are 'DXT1'..'DXT5' little-endian.
        assert_eq!(&PixelFormatId::Dxt1.raw().to_le_bytes(), b"DXT1");
        assert_eq!(&PixelFormatId::Dxt5.raw().to_le_bytes(), b"DXT5");
    }

    // Check the observed pixel-format flags, bit depths and channel masks.
    // Spot-checked across every flag combination the table uses, including the two shared rows
    // (R8G8B8 with CUSTOM_LSCAPE_R8G8B8, A8 with CUSTOM_LSCAPE_ALPHA) and the deliberately odd
    // 4-bpp DXT1 entry.
    #[test]
    fn the_format_table_matches_the_documented_flags_and_masks() {
        let d = PixelFormatDesc::for_format(PixelFormatId::A8R8G8B8).unwrap();
        assert_eq!(d.flags, 3);
        assert_eq!(d.bits_per_pixel, 32);
        assert_eq!(d.alpha_mask, 0xFF00_0000);
        assert!(d.has_rgb() && d.has_alpha() && !d.is_compressed());

        // The landscape pair shares its descriptor with the format it mimics.
        let a = PixelFormatDesc::for_format(PixelFormatId::R8G8B8).unwrap();
        let b = PixelFormatDesc::for_format(PixelFormatId::CustomLscapeR8G8B8).unwrap();
        assert_eq!(
            (a.flags, a.bits_per_pixel, a.red_mask),
            (b.flags, b.bits_per_pixel, b.red_mask)
        );
        let a = PixelFormatDesc::for_format(PixelFormatId::A8).unwrap();
        let b = PixelFormatDesc::for_format(PixelFormatId::CustomLscapeAlpha).unwrap();
        assert_eq!(
            (a.flags, a.bits_per_pixel, a.alpha_mask),
            (b.flags, b.bits_per_pixel, b.alpha_mask)
        );

        // Palettised formats carry flag 0x40 and no masks.
        for f in [PixelFormatId::P8, PixelFormatId::Index16] {
            let d = PixelFormatDesc::for_format(f).unwrap();
            assert_eq!(d.flags, pf_flags::INDEXED);
            assert!(d.is_indexed());
        }
        assert_eq!(
            PixelFormatDesc::for_format(PixelFormatId::P8)
                .unwrap()
                .bits_per_pixel,
            8
        );
        assert_eq!(
            PixelFormatDesc::for_format(PixelFormatId::Index16)
                .unwrap()
                .bits_per_pixel,
            16
        );

        // Raw JPEG: flags 0x11, bpp 0.
        let d = PixelFormatDesc::for_format(PixelFormatId::CustomRawJpeg).unwrap();
        assert_eq!(d.flags, 0x11);
        assert_eq!(d.bits_per_pixel, 0);
        assert!(d.is_raw());

        // DXT1 is 4 bpp, DXT2-5 are 8, and all five are flagged compressed.
        assert_eq!(
            PixelFormatDesc::for_format(PixelFormatId::Dxt1)
                .unwrap()
                .bits_per_pixel,
            4
        );
        for f in [
            PixelFormatId::Dxt2,
            PixelFormatId::Dxt3,
            PixelFormatId::Dxt4,
            PixelFormatId::Dxt5,
        ] {
            let d = PixelFormatDesc::for_format(f).unwrap();
            assert_eq!(d.bits_per_pixel, 8);
            assert!(d.is_compressed());
        }

        // "Anything not in this table returns false from SetFormat."
        assert!(PixelFormatDesc::for_format(PixelFormatId::Other(27)).is_none());
        assert!(PixelFormatDesc::for_format(PixelFormatId::Unknown).is_none());
    }

    // Oracle: the block layout of S3TC, which is what the 4/8 bpp entries encode; cross-checked
    // against the texture resource-size formula bitsPerPixel * w * h / 8.
    #[test]
    fn image_bytes_handles_blocks_and_odd_widths() {
        let dxt1 = PixelFormatDesc::for_format(PixelFormatId::Dxt1).unwrap();
        assert_eq!(dxt1.image_bytes(4, 4), 8);
        assert_eq!(dxt1.image_bytes(8, 8), 32);
        // A 5x5 image still needs 2x2 blocks.
        assert_eq!(dxt1.image_bytes(5, 5), 32);
        let dxt5 = PixelFormatDesc::for_format(PixelFormatId::Dxt5).unwrap();
        assert_eq!(dxt5.image_bytes(4, 4), 16);

        let argb = PixelFormatDesc::for_format(PixelFormatId::A8R8G8B8).unwrap();
        assert_eq!(argb.image_bytes(7, 3), 7 * 4 * 3);
        let rgb24 = PixelFormatDesc::for_format(PixelFormatId::R8G8B8).unwrap();
        assert_eq!(rgb24.row_bytes(7), 21);
    }
}
