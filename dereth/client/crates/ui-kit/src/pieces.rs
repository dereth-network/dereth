//! A skin's own art as files: pieces packed into atlases at several scales, and one manifest that
//! names each piece, gives its size in layout units and says where it sits in each scale's atlas;
//! and the skin's fonts as glyph pages with their table ([`FontTable`]). Where a piece is in the
//! manifest the kit draws it; where it is not, the kit draws plainly in its place. An interface
//! hands the files to the art ([`crate::art::ArtFiles`]).

use std::collections::BTreeMap;

use serde::Deserialize;

/// The manifest's name.
pub const MANIFEST: &str = "manifest.json";

/// The fonts' table's name.
pub const FONTS: &str = "fonts.json";

/// One size of one font family: its glyphs, each `[char, page, x, y, w, h, advance_adjust,
/// y_offset]` in that size's pixels, and its kerning pairs, `[left, right, pixels]`.
#[derive(Debug, Clone, Deserialize)]
pub struct FontSize {
    pub size: f32,
    pub line_height: i32,
    pub ascent: i32,
    pub glyphs: Vec<[i32; 8]>,
    pub kerning: Vec<[i32; 3]>,
}

impl FontSize {
    /// This size as the font table the interface's text drawing reads, for a family drawn
    /// `factor` times larger than the size asked for: the table answers as the size asked for,
    /// so text asked for at `size / factor` is drawn from these glyphs unscaled.
    #[must_use]
    pub fn font(&self, factor: f32) -> crate::font::Font {
        let narrow = |v: i32| u16::try_from(v).unwrap_or(0);
        let glyphs = self
            .glyphs
            .iter()
            .filter_map(|g| {
                let ch = char::from_u32(u32::try_from(g[0]).ok()?)?;
                Some((
                    ch,
                    crate::font::Glyph {
                        ch,
                        page: narrow(g[1]),
                        x: narrow(g[2]),
                        y: narrow(g[3]),
                        w: u8::try_from(g[4]).unwrap_or(0),
                        h: u8::try_from(g[5]).unwrap_or(0),
                        advance_adjust: i8::try_from(g[6]).unwrap_or(0),
                        y_offset: i8::try_from(g[7]).unwrap_or(0),
                        x_offset: 0,
                    },
                ))
            })
            .collect();
        let kerning = self
            .kerning
            .iter()
            .filter_map(|k| {
                let a = char::from_u32(u32::try_from(k[0]).ok()?)?;
                let b = char::from_u32(u32::try_from(k[1]).ok()?)?;
                Some(((a, b), k[2]))
            })
            .collect();
        crate::font::Font {
            point_size: self.size / factor.max(f32::EPSILON),
            pixel_size: self.size,
            line_height: self.line_height,
            ascent: self.ascent,
            glyphs,
            kerning,
        }
    }
}

/// One family: how much larger than the asked size its face is drawn (so it fills the room the
/// interface's layout gives its text), and the sizes it was rendered at.
#[derive(Debug, Clone, Deserialize)]
pub struct FontFamily {
    pub factor: f32,
    pub sizes: Vec<FontSize>,
}

/// The interface's fonts: each family (named as the interface names it), and how many glyph
/// pages they share.
#[derive(Debug, Clone, Deserialize)]
pub struct FontTable {
    pub pages: u32,
    pub families: BTreeMap<String, FontFamily>,
}

impl FontTable {
    /// The table in `bytes`.
    ///
    /// # Errors
    /// When the bytes are not a font table.
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        serde_json::from_slice(bytes).map_err(|e| format!("the pieces' fonts: {e}"))
    }

    /// The size of `family` to draw text asked for at `pixels`: the one nearest `pixels` times
    /// the family's factor, with the factor.
    #[must_use]
    pub fn nearest(&self, family: &str, pixels: f32) -> Option<(&FontSize, f32)> {
        let family = self.families.get(family)?;
        let want = pixels * family.factor;
        family
            .sizes
            .iter()
            .min_by(|a, b| (a.size - want).abs().total_cmp(&(b.size - want).abs()))
            .map(|s| (s, family.factor))
    }
}

/// The name of glyph page `page`.
#[must_use]
pub fn font_page_path(page: u16) -> String {
    format!("font-{page}.png")
}

/// One piece: its size in layout units, and for each scale the atlas it is in and its rectangle
/// there, in that atlas's pixels.
#[derive(Debug, Clone, Deserialize)]
pub struct Piece {
    pub size: [f32; 2],
    pub at: BTreeMap<String, [u32; 5]>,
}

/// The manifest: the scales the atlases come in, the pieces and the values (insets, wells,
/// channels) the interface lays the pieces out by, in layout units.
#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub scales: Vec<u32>,
    pub pieces: BTreeMap<String, Piece>,
    #[serde(default)]
    pub values: BTreeMap<String, Vec<f32>>,
}

impl Manifest {
    /// The manifest in `bytes`.
    ///
    /// # Errors
    /// When the bytes are not a manifest.
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        serde_json::from_slice(bytes).map_err(|e| format!("the pieces' manifest: {e}"))
    }

    /// Where piece `name` is best drawn from when a layout unit is `px_per_unit` screen pixels:
    /// the smallest scale whose pixels per unit reach it, else the largest. Answers the scale,
    /// the atlas, the rectangle and that scale's pixels per unit.
    #[must_use]
    pub fn place(&self, name: &str, px_per_unit: f32) -> Option<(u32, u32, [u32; 4], f32)> {
        let piece = self.pieces.get(name)?;
        let mut options: Vec<(u32, u32, [u32; 4], f32)> = piece
            .at
            .iter()
            .filter_map(|(scale, at)| {
                let scale: u32 = scale.parse().ok()?;
                #[allow(clippy::cast_precision_loss)]
                let ratio = at[3] as f32 / piece.size[0].max(f32::EPSILON);
                Some((scale, at[0], [at[1], at[2], at[3], at[4]], ratio))
            })
            .collect();
        options.sort_by(|a, b| a.3.total_cmp(&b.3));
        options
            .iter()
            .find(|o| o.3 >= px_per_unit * 0.98)
            .or_else(|| options.last())
            .copied()
    }
}

/// The name of the atlas `index` at `scale`.
#[must_use]
pub fn atlas_path(scale: u32, index: u32) -> String {
    format!("atlas-{scale}-{index}.png")
}

/// An atlas's PNG decoded to the art's pixel order (blue, green, red, alpha).
///
/// # Errors
/// When the bytes are not an 8-bit RGBA or RGB PNG.
pub fn decode_png(bytes: &[u8]) -> Result<crate::art::Image, String> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    let pixels = &buf[..info.buffer_size()];
    let mut bgra = Vec::with_capacity((info.width * info.height * 4) as usize);
    match info.color_type {
        png::ColorType::Rgba => {
            for p in pixels.as_chunks::<4>().0 {
                bgra.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
            }
        }
        png::ColorType::Rgb => {
            for p in pixels.as_chunks::<3>().0 {
                bgra.extend_from_slice(&[p[2], p[1], p[0], 255]);
            }
        }
        // A glyph page: its grey is the glyphs' coverage.
        png::ColorType::Grayscale => {
            for &g in pixels {
                bgra.extend_from_slice(&[g, g, g, 255]);
            }
        }
        png::ColorType::GrayscaleAlpha => {
            for p in pixels.as_chunks::<2>().0 {
                bgra.extend_from_slice(&[p[0], p[0], p[0], p[1]]);
            }
        }
        other => return Err(format!("an atlas in {other:?}")),
    }
    Ok(crate::art::Image {
        width: info.width,
        height: info.height,
        bgra,
    })
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (this client's own widget kit)
    use super::*;

    fn manifest() -> Manifest {
        Manifest::parse(
            br#"{"scales": [1, 2, 3],
                 "pieces": {"button.normal.left": {"size": [10, 28],
                   "at": {"1": [0, 0, 0, 10, 28], "2": [0, 4, 4, 20, 56], "3": [1, 0, 0, 30, 84]}}},
                 "values": {"slot.icon": [4, 5, 40]}}"#,
        )
        .unwrap()
    }

    #[test]
    fn a_piece_is_drawn_from_the_smallest_scale_that_is_sharp_enough_else_the_largest() {
        let m = manifest();
        let at = |k: f32| m.place("button.normal.left", k).map(|p| (p.0, p.1));
        assert_eq!(at(1.0), Some((1, 0)));
        assert_eq!(at(1.5), Some((2, 0)));
        assert_eq!(at(2.0), Some((2, 0)));
        assert_eq!(at(2.5), Some((3, 1)));
        assert_eq!(at(6.0), Some((3, 1)), "past every scale, the largest");
        assert_eq!(m.place("nothing", 1.0), None);
        assert_eq!(m.values["slot.icon"], [4.0, 5.0, 40.0]);
    }

    #[test]
    fn an_atlas_decodes_to_the_art_s_pixel_order() {
        let mut bytes = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut bytes, 2, 1);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut w = enc.write_header().unwrap();
            w.write_image_data(&[10, 20, 30, 40, 50, 60, 70, 80])
                .unwrap();
        }
        let image = decode_png(&bytes).unwrap();
        assert_eq!((image.width, image.height), (2, 1));
        assert_eq!(image.bgra, [30, 20, 10, 40, 70, 60, 50, 80]);
    }
}
