//! The interface's own art: pieces packed into atlases at several scales, and one manifest that
//! names each piece, gives its size in layout units and says where it sits in each scale's atlas.
//! Where a piece is in the manifest the interface draws it; where it is not, the interface draws
//! plainly in its place.
//!
//! The art is a set of files ([`FILES`]): the manifest and the atlases, and the interface's fonts
//! as glyph pages with their table ([`FontTable`]). The host hands them to the interface as
//! [`Pieces`]. A desktop client carries them built in ([`Pieces::built_in`]); the browser's module
//! does not, and its page fetches them from beside the module instead.

use std::borrow::Cow;
use std::collections::BTreeMap;

use serde::Deserialize;

/// The manifest's name.
pub const MANIFEST: &str = "manifest.json";

/// The fonts' table's name.
pub const FONTS: &str = "fonts.json";

/// The art's files, as one list: [`FILES`], and in a build that carries them, their bytes.
macro_rules! files {
    ($($name:literal),* $(,)?) => {
        /// Every file of the art, by its name: the manifest, the atlases, the fonts' table and
        /// the glyph pages. A host hands over each of them, and nothing else.
        pub const FILES: &[&str] = &[$($name),*];

        /// The files built in, by name.
        #[cfg(not(target_arch = "wasm32"))]
        const BUILT_IN: &[(&str, &[u8])] =
            &[$(($name, include_bytes!(concat!("../pieces/", $name)))),*];
    };
}

files!(
    "manifest.json",
    "atlas-1-0.png",
    "atlas-2-0.png",
    "atlas-3-0.png",
    "fonts.json",
    "font-0.png",
    "font-1.png",
    "font-2.png",
    "font-3.png",
    "font-4.png",
    "font-5.png",
);

/// The interface's art as a host hands it over: each of [`FILES`] by its name.
#[derive(Clone, Default)]
pub struct Pieces {
    files: BTreeMap<String, Cow<'static, [u8]>>,
}

impl std::fmt::Debug for Pieces {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_map()
            .entries(self.files.iter().map(|(name, bytes)| (name, bytes.len())))
            .finish()
    }
}

impl Pieces {
    /// No files yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The files the client carries built in; `None` in a build that carries none (the
    /// browser's module, whose page hands the files over).
    #[must_use]
    pub fn built_in() -> Option<Self> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut pieces = Self::new();
            for (name, bytes) in BUILT_IN {
                pieces.insert(name, *bytes);
            }
            Some(pieces)
        }
        #[cfg(target_arch = "wasm32")]
        {
            None
        }
    }

    /// File `name`'s bytes, in place of any it had.
    pub fn insert(&mut self, name: &str, bytes: impl Into<Cow<'static, [u8]>>) {
        self.files.insert(name.to_ascii_lowercase(), bytes.into());
    }

    /// File `name`, its case aside.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&[u8]> {
        self.files
            .get(&name.to_ascii_lowercase())
            .map(std::convert::AsRef::as_ref)
    }

    /// Whether every file of the art is here and looks like what it should be: each picture a
    /// PNG and each table JSON text, so a page a server sends in a missing file's place is not
    /// taken for it.
    ///
    /// # Errors
    /// Each file that is missing, not one of [`FILES`], or not what its name says, named.
    pub fn check(&self) -> Result<(), String> {
        const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        let mut findings = Vec::new();
        for name in FILES {
            match self.get(name) {
                None => findings.push(format!("{name} is missing")),
                Some(bytes) if name.ends_with(".png") && !bytes.starts_with(PNG) => {
                    findings.push(format!("{name} is not a PNG picture"));
                }
                Some(bytes)
                    if name.ends_with(".json")
                        && bytes.iter().find(|b| !b.is_ascii_whitespace()) != Some(&b'{') =>
                {
                    findings.push(format!("{name} is not a JSON table"));
                }
                Some(_) => {}
            }
        }
        for name in self.files.keys() {
            if !FILES.iter().any(|f| f.eq_ignore_ascii_case(name)) {
                findings.push(format!("{name} is not one of the art's files"));
            }
        }
        if findings.is_empty() {
            Ok(())
        } else {
            Err(findings.join("; "))
        }
    }
}

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
    //! Behaviour: none (experimental Horizon interface)
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
    fn the_built_in_pieces_hold_every_atlas_their_manifest_names_and_each_decodes() {
        let pieces = Pieces::built_in().expect("a desktop build carries its pieces");
        let manifest = Manifest::parse(pieces.get(MANIFEST).unwrap()).unwrap();
        assert!(!manifest.pieces.is_empty());
        for (name, piece) in &manifest.pieces {
            for (scale, at) in &piece.at {
                let path = atlas_path(scale.parse().unwrap(), at[0]);
                assert!(pieces.get(&path).is_some(), "{name}: {path} is built in");
            }
        }
        for path in FILES.iter().filter(|p| p.starts_with("atlas-")) {
            let image =
                decode_png(pieces.get(path).unwrap()).unwrap_or_else(|e| panic!("{path}: {e}"));
            for piece in manifest.pieces.values() {
                for (scale, at) in &piece.at {
                    if atlas_path(scale.parse().unwrap(), at[0]) == *path {
                        assert!(at[1] + at[3] <= image.width && at[2] + at[4] <= image.height);
                    }
                }
            }
        }
    }

    /// The art's files are exactly the files of its folder, each built in and each passing the
    /// check a host's files pass: what a host is told to hand over is what there is.
    #[test]
    fn the_art_s_files_are_its_folder_s_files_and_the_built_in_ones_pass_the_check() {
        let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("pieces");
        let mut on_disk: Vec<String> = std::fs::read_dir(&folder)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        on_disk.sort();
        let mut listed: Vec<String> = FILES.iter().map(|f| (*f).to_owned()).collect();
        listed.sort();
        assert_eq!(listed, on_disk, "FILES lists {}", folder.display());
        let pieces = Pieces::built_in().unwrap();
        assert_eq!(pieces.check(), Ok(()));
        for name in FILES {
            assert_eq!(
                pieces.get(name).map(<[u8]>::len),
                Some(std::fs::read(folder.join(name)).unwrap().len()),
                "{name}"
            );
        }
    }

    /// Files a host hands over are the art once all of them are there and each is what its name
    /// says; a missing one, or a page sent in a file's place, is named.
    #[test]
    fn handed_over_files_are_checked_for_each_one_missing_or_not_what_its_name_says() {
        let mut pieces = Pieces::new();
        let check = pieces.check().unwrap_err();
        for name in FILES {
            assert!(check.contains(&format!("{name} is missing")), "{check}");
        }
        let png = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0];
        for name in FILES {
            if name.ends_with(".png") {
                pieces.insert(name, png.to_vec());
            } else {
                pieces.insert(name, b" {}".to_vec());
            }
        }
        assert_eq!(pieces.check(), Ok(()));
        assert_eq!(
            pieces.get("ATLAS-1-0.PNG"),
            Some(&png[..]),
            "its case aside"
        );
        pieces.insert("atlas-2-0.png", b"<!doctype html>".to_vec());
        pieces.insert(FONTS, b"<html>".to_vec());
        pieces.insert("notes.txt", b"x".to_vec());
        let check = pieces.check().unwrap_err();
        assert!(check.contains("atlas-2-0.png is not a PNG"), "{check}");
        assert!(check.contains("fonts.json is not a JSON"), "{check}");
        assert!(check.contains("notes.txt is not one of"), "{check}");
        assert!(!check.contains("missing"), "{check}");
    }

    /// The art behind the interface is the files the host handed over: drawn from them when
    /// they are there, and plainly where they are not.
    #[test]
    fn the_art_draws_from_the_files_the_host_hands_over() {
        let built_in = Pieces::built_in().unwrap();
        let art = crate::art::Art::new(std::sync::Arc::new(built_in.clone()));
        assert!(art.has_piece("window.tl"));
        let sprite = art.piece("window.tl", 1.0).expect("the piece");
        assert!(art.image(sprite.tex).is_some(), "its atlas decoded");
        assert!(art.font(crate::art::Family::Body, 13.0).is_some());
        // Only the manifest: the piece is named, and its atlas is not there to draw it from.
        let mut manifest_only = Pieces::new();
        manifest_only.insert(MANIFEST, built_in.get(MANIFEST).unwrap().to_vec());
        let art = crate::art::Art::new(std::sync::Arc::new(manifest_only));
        assert!(art.has_piece("window.tl"));
        assert_eq!(art.piece("window.tl", 1.0), None);
        assert!(art.font(crate::art::Family::Body, 13.0).is_none());
        let empty = crate::art::Art::new(std::sync::Arc::new(Pieces::new()));
        assert!(!empty.has_piece("window.tl"));
    }

    #[test]
    fn the_built_in_fonts_cover_every_family_the_interface_draws_and_their_glyphs_fit_their_pages()
    {
        let pieces = Pieces::built_in().unwrap();
        let table = FontTable::parse(pieces.get(FONTS).unwrap()).unwrap();
        let mut pages = Vec::new();
        for page in 0..table.pages {
            let path = font_page_path(u16::try_from(page).unwrap());
            pages.push(decode_png(pieces.get(&path).unwrap()).unwrap());
        }
        for family in crate::art::Family::ALL {
            let fam = &table.families[family.file_stem()];
            assert!(!fam.sizes.is_empty(), "{family:?}");
            for size in &fam.sizes {
                let font = size.font(fam.factor);
                for ch in ['A', 'g', '0', '?'] {
                    assert!(font.glyph(ch).is_some(), "{family:?} {}: {ch}", size.size);
                }
                for g in font.glyphs.values() {
                    let page = &pages[usize::from(g.page)];
                    assert!(u32::from(g.x) + u32::from(g.w) <= page.width);
                    assert!(u32::from(g.y) + u32::from(g.h) <= page.height);
                }
            }
        }
        // Text asked for at 8.5 is drawn from the size nearest 8.5 times the factor, and that
        // size's table answers as 8.5 asks: drawn unscaled.
        let (size, factor) = table.nearest("Body", 8.53).unwrap();
        assert!((size.size - (8.53 * factor).round()).abs() < f32::EPSILON);
        let font = size.font(factor);
        assert!((font.point_size * factor - size.size).abs() < 1e-3);
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
