//! The classic interface's text rasterised from the Liberation fonts it ships, for hosts without the Windows font system.
//!
//! **Depends on** `dereth-classic-dat` (the font atlas format), `dereth-primitives` (its rounding)
//! and `skrifa` (TrueType outlines and their hinting instructions). **Used by** the desktop host off
//! Windows and the web client, which hand the classic interface its fonts.
//!
//! **Must never** reach a platform or read a file: the fonts are inside it, and it builds for every
//! target, the browser's included.
//!
//! The classic interface asks for Times New Roman, Courier New and Arial by cell height, average
//! width and weight, the way a Windows program does. This crate answers with the Liberation fonts
//! (Serif, Mono and Sans), which have those faces' advance widths in font units, and draws them
//! as the Windows font system draws a grey glyph, as closely as a different face allows:
//!
//! * **The request is resolved as Windows resolves it.** The cell height picks the em size by
//!   the ascent and descent Windows gives the face at each size ([`extents`]); a width other than
//!   the face's own average at that size stretches the font sideways, by the request's proportion
//!   of width to height against the face's.
//! * **Each glyph is hinted by the font's own TrueType instructions,** fully in both directions,
//!   at the em height for its y and at the stretched em width for its x.
//! * **Each pixel's coverage** is the share of an 8 by 8 grid of sample points inside the hinted
//!   outline: 65 levels, no smoothing filter ([`raster`]).
//! * **The advances** are the design widths scaled and rounded, corrected for the interface's own
//!   requests to the advances Windows gives them ([`advances`]); the line height and baseline are
//!   Windows' ascent and descent.
//!
//! The faces' outlines are not Microsoft's, so the glyphs' pixels differ from the Windows desktop's
//! (Liberation hints only vertically at these sizes, so its vertical strokes are softer); the
//! advances, line heights and baselines are the same.
//!
//! The fonts are the Liberation 2.1.5 release's files, unmodified, under the SIL Open Font
//! Licence 1.1 (`fonts/OFL.txt`; `fonts/SOURCES.txt` names each file's origin and hash).

use dereth_classic_dat::fonts::{
    coverage, cp1252, measure_cells, FontAtlas, FontSource, FontSpec, CELL, COLUMNS, PAD, ROWS,
};
use raster::{combine, sample, stretch, Command, Recorder};
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, Engine, HintingInstance, HintingOptions, Target};
use skrifa::raw::TableProvider;
use skrifa::{FontRef, GlyphId, MetadataProvider, OutlineGlyph};

mod advances;
mod extents;
mod raster;

/// Liberation Serif, for Times New Roman.
static SERIF: &[u8] = include_bytes!("../fonts/LiberationSerif-Regular.ttf");
/// Liberation Serif Bold.
static SERIF_BOLD: &[u8] = include_bytes!("../fonts/LiberationSerif-Bold.ttf");
/// Liberation Serif Italic.
static SERIF_ITALIC: &[u8] = include_bytes!("../fonts/LiberationSerif-Italic.ttf");
/// Liberation Mono Bold, for Courier New.
static MONO_BOLD: &[u8] = include_bytes!("../fonts/LiberationMono-Bold.ttf");
/// Liberation Sans Bold, for Arial.
static SANS_BOLD: &[u8] = include_bytes!("../fonts/LiberationSans-Bold.ttf");

/// The fonts this crate carries, as a [`FontSource`].
#[derive(Debug, Default, Clone, Copy)]
pub struct ShippedFonts;

impl FontSource for ShippedFonts {
    fn rasterize(&self, spec: &FontSpec) -> Result<FontAtlas, String> {
        rasterize(spec)
    }
}

/// Rasterise `spec` with the carried fonts.
///
/// # Errors
/// A glyph does not fit its atlas cell (a request far larger than the interface's own).
pub fn rasterize(spec: &FontSpec) -> Result<FontAtlas, String> {
    rasterize_face(&Face::for_request(spec), spec)
}

/// A face: its font file, the name an atlas reports, and the ascent and descent Windows gives the
/// face it stands in for at each em size from 1 pixel.
#[doc(hidden)]
#[derive(Debug, Clone, Copy)]
pub struct Face<'a> {
    pub data: &'a [u8],
    pub name: &'a str,
    pub extents: &'a [(u8, u8)],
}

impl Face<'static> {
    /// The carried face `spec` draws with. Only the styles the interface asks for are carried:
    /// Courier New and Arial in bold, Times New Roman in regular, bold and italic; another style
    /// of a family draws with the family's carried one.
    #[must_use]
    pub fn for_request(spec: &FontSpec) -> Self {
        let lower = spec.face.to_ascii_lowercase();
        // A face's full name names its style, as the Windows font system reads it.
        let italic = spec.italic || lower.ends_with(" italic");
        // Windows draws a weight of 600 and above with a face's bold.
        let bold = spec.weight >= 600 || lower.ends_with(" bold");
        let (data, name, extents): (_, _, &[(u8, u8)]) = if lower.starts_with("courier") {
            (MONO_BOLD, "Liberation Mono", &extents::MONO_BOLD)
        } else if lower.starts_with("arial") {
            (SANS_BOLD, "Liberation Sans", &extents::SANS_BOLD)
        } else if italic {
            (SERIF_ITALIC, "Liberation Serif", &extents::SERIF_ITALIC)
        } else if bold {
            (SERIF_BOLD, "Liberation Serif", &extents::SERIF_BOLD)
        } else {
            (SERIF, "Liberation Serif", &extents::SERIF)
        };
        Self {
            data,
            name,
            extents,
        }
    }
}

/// A face's design metrics, in font units.
#[derive(Debug, Clone, Copy)]
struct Design {
    units_per_em: f64,
    ascent: f64,
    descent: f64,
    /// The average character width the Windows font system takes for these faces: the advances
    /// of the lower-case letters and the space, weighted by their frequency in English text.
    average: f64,
}

/// The frequency, per thousand characters of English text, of each lower-case letter and the
/// space: the weights of a TrueType face's classic average character width.
const LETTER_WEIGHTS: [(char, f64); 27] = [
    ('a', 64.0),
    ('b', 14.0),
    ('c', 27.0),
    ('d', 35.0),
    ('e', 100.0),
    ('f', 20.0),
    ('g', 14.0),
    ('h', 42.0),
    ('i', 63.0),
    ('j', 3.0),
    ('k', 6.0),
    ('l', 35.0),
    ('m', 20.0),
    ('n', 56.0),
    ('o', 56.0),
    ('p', 17.0),
    ('q', 4.0),
    ('r', 49.0),
    ('s', 56.0),
    ('t', 71.0),
    ('u', 31.0),
    ('v', 10.0),
    ('w', 18.0),
    ('x', 3.0),
    ('y', 18.0),
    ('z', 2.0),
    (' ', 166.0),
];

fn design(font: &FontRef) -> Result<Design, String> {
    let head = font.head().map_err(|e| e.to_string())?;
    let os2 = font.os2().map_err(|e| e.to_string())?;
    let charmap = font.charmap();
    let metrics = font.glyph_metrics(Size::unscaled(), LocationRef::default());
    let mut sum = 0.0;
    for (c, weight) in LETTER_WEIGHTS {
        let glyph = charmap.map(c).ok_or("the font has no lower-case letters")?;
        sum += weight * f64::from(metrics.advance_width(glyph).unwrap_or(0.0));
    }
    Ok(Design {
        units_per_em: f64::from(head.units_per_em()),
        ascent: f64::from(os2.us_win_ascent()),
        descent: f64::from(os2.us_win_descent()),
        // Whole font units, the thousandths dropped.
        average: (sum / 1000.0).floor(),
    })
}

/// A request resolved to the font's sizes in pixels.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resolved {
    /// The em height.
    pub ppem_y: i32,
    /// The em width: the em height unless the request stretches the font.
    pub ppem_x: i32,
    pub ascent: i32,
    pub descent: i32,
}

/// Round half away from zero.
fn round(v: f64) -> i32 {
    dereth_primitives::num::to_i32_f64(v.round())
}

fn resolve(d: &Design, extents: &[(u8, u8)], spec: &FontSpec) -> Resolved {
    let cell = d.ascent + d.descent;
    let scale = |p: i32| f64::from(p) / d.units_per_em;
    // The ascent and descent at an em size: Windows' where known, else the design's, rounded.
    let at = |p: i32| -> (i32, i32) {
        usize::try_from(p - 1)
            .ok()
            .and_then(|i| extents.get(i))
            .map_or_else(
                || (round(d.ascent * scale(p)), round(d.descent * scale(p))),
                |&(a, b)| (i32::from(a), i32::from(b)),
            )
    };
    let ppem_y = match spec.height {
        // A cell height picks the first em whose cell is exactly that tall, or else the largest
        // whose cell is shorter.
        h if h > 0 => {
            let mut best = 1;
            for p in 1..=4 * h {
                let (a, b) = at(p);
                if a + b > h {
                    break;
                }
                best = p;
                if a + b == h {
                    break;
                }
            }
            best
        }
        // A negative height is the em itself.
        h if h < 0 => -h,
        _ => round(12.0 * d.units_per_em / cell).max(1),
    };
    let (ascent, descent) = at(ppem_y);
    let natural = round(d.average * scale(ppem_y));
    let ppem_x = if spec.width <= 0 || spec.width == natural {
        ppem_y
    } else {
        // The width stretches the font sideways by the request's proportion, width over cell
        // height, against the face's own: its average width over its cell.
        let height = if spec.height > 0 {
            f64::from(spec.height)
        } else {
            cell * scale(ppem_y)
        };
        round(f64::from(ppem_y) * f64::from(spec.width) * cell / (d.average * height)).max(1)
    };
    Resolved {
        ppem_y,
        ppem_x,
        ascent,
        descent,
    }
}

/// Resolve `spec` against `face`.
///
/// # Errors
/// The font cannot be read.
#[doc(hidden)]
pub fn resolve_spec(face: &Face, spec: &FontSpec) -> Result<Resolved, String> {
    let font = FontRef::new(face.data).map_err(|e| e.to_string())?;
    Ok(resolve(&design(&font)?, face.extents, spec))
}

/// The advance of `c` for `spec` before any measured correction: its design width at the
/// request's em width, rounded.
///
/// # Errors
/// The font cannot be read.
#[doc(hidden)]
pub fn design_advance(spec: &FontSpec, c: char) -> Result<i32, String> {
    let face = Face::for_request(spec);
    let font = FontRef::new(face.data).map_err(|e| e.to_string())?;
    let d = design(&font)?;
    let r = resolve(&d, face.extents, spec);
    let glyph = font.charmap().map(c).unwrap_or(GlyphId::NOTDEF);
    let width = font
        .glyph_metrics(Size::unscaled(), LocationRef::default())
        .advance_width(glyph)
        .unwrap_or(0.0);
    Ok(round(
        f64::from(width) * f64::from(r.ppem_x) / d.units_per_em,
    ))
}

/// Full hinting by the font's own instructions at `ppem`.
fn hinting(font: &FontRef, ppem: i32) -> Result<HintingInstance, String> {
    #[allow(clippy::cast_precision_loss)]
    let size = Size::new(ppem as f32);
    HintingInstance::new(
        &font.outline_glyphs(),
        size,
        LocationRef::default(),
        HintingOptions {
            engine: Engine::Interpreter,
            target: Target::Mono,
        },
    )
    .map_err(|e| e.to_string())
}

fn draw(glyph: &OutlineGlyph, hinting: &HintingInstance) -> Result<Vec<Command>, String> {
    let mut pen = Recorder::default();
    glyph
        .draw(DrawSettings::hinted(hinting, false), &mut pen)
        .map_err(|e| e.to_string())?;
    Ok(pen.0)
}

/// [`rasterize`] with `face`.
///
/// # Errors
/// The font cannot be read or hinted, or a glyph does not fit its atlas cell.
#[doc(hidden)]
pub fn rasterize_face(face: &Face, spec: &FontSpec) -> Result<FontAtlas, String> {
    let font = FontRef::new(face.data).map_err(|e| e.to_string())?;
    let d = design(&font)?;
    let r = resolve(&d, face.extents, spec);
    let outlines = font.outline_glyphs();
    let charmap = font.charmap();
    let design_advances = font.glyph_metrics(Size::unscaled(), LocationRef::default());
    let hint_y = hinting(&font, r.ppem_y)?;
    let hint_x = if r.ppem_x == r.ppem_y {
        None
    } else {
        Some(hinting(&font, r.ppem_x)?)
    };
    let measured = advances::measured(spec);

    let (aw, ah) = (COLUMNS * CELL, ROWS * CELL);
    let mut alpha = vec![0u8; usize::try_from(aw * ah).map_err(|e| e.to_string())?];
    let mut cells = Vec::new();
    // Each character's place in the measured advances: the printable characters in order.
    let mut slot = 0;
    for (index, byte) in (32u8..=255).enumerate() {
        if byte == 127 {
            continue;
        }
        let Some(c) = cp1252(byte) else { continue };
        let index = i32::try_from(index).map_err(|e| e.to_string())?;
        let (cx, cy) = (index % COLUMNS * CELL, index / COLUMNS * CELL);
        let glyph_id = charmap.map(c).unwrap_or(GlyphId::NOTDEF);
        let glyph = outlines
            .get(glyph_id)
            .ok_or_else(|| format!("the font has no outline for U+{:04X}", u32::from(c)))?;
        let ys = draw(&glyph, &hint_y)?;
        let commands = match &hint_x {
            None => ys,
            Some(hint_x) => {
                let xs = draw(&glyph, hint_x)?;
                combine(&xs, &ys)
                    .unwrap_or_else(|| stretch(&ys, f64::from(r.ppem_x) / f64::from(r.ppem_y)))
            }
        };
        let width = f64::from(design_advances.advance_width(glyph_id).unwrap_or(0.0));
        let mut advance = round(width * f64::from(r.ppem_x) / d.units_per_em);
        if let Some(deltas) = measured {
            advance += advances::delta(deltas, slot);
        }
        slot += 1;
        cells.push((u32::from(c), cx, cy, advance));
        // The cell, with the pen at its margin on the baseline.
        let Some(counts) = sample(&commands, -PAD, PAD + r.ascent, CELL) else {
            continue;
        };
        for (y, row) in (0..).zip(counts.chunks(usize::try_from(CELL).unwrap_or(1))) {
            for (x, &n) in (0..).zip(row) {
                if n == 0 {
                    continue;
                }
                if x == 0 || y == 0 || x == CELL - 1 || y == CELL - 1 {
                    return Err(format!(
                        "the glyph for U+{:04X} does not fit its cell",
                        u32::from(c)
                    ));
                }
                if let Some(a) = usize::try_from((cy + y) * aw + cx + x)
                    .ok()
                    .and_then(|i| alpha.get_mut(i))
                {
                    *a = coverage(n);
                }
            }
        }
    }
    let mut rgba = vec![0xFF; alpha.len() * 4];
    for (px, a) in rgba.as_chunks_mut::<4>().0.iter_mut().zip(&alpha) {
        px[3] = *a;
    }
    let glyphs = measure_cells(&alpha, aw, &cells, r.ascent)?;
    Ok(FontAtlas {
        width: aw.unsigned_abs(),
        height: ah.unsigned_abs(),
        rgba,
        glyphs,
        line_height: r.ascent + r.descent,
        baseline: r.ascent,
        face: face.name.into(),
    })
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the carried fonts' reading of a request; the Windows comparison is in
    //! tests/cpu).
    use super::*;

    fn spec(height: i32, width: i32, weight: i32, face: &str) -> FontSpec {
        FontSpec {
            height,
            width,
            weight,
            italic: false,
            face: face.into(),
        }
    }

    fn resolved(height: i32, width: i32, weight: i32, face: &str) -> (i32, i32, i32, i32) {
        let s = spec(height, width, weight, face);
        let r = resolve_spec(&Face::for_request(&s), &s).unwrap();
        (r.ppem_y, r.ppem_x, r.ascent, r.descent)
    }

    /// Times New Roman's ascent and descent at 12 and 13 pixels to the em are both 12 and 3, so a
    /// 15-pixel cell is the smaller em; a 13-pixel cell, which no em fills exactly, is the largest
    /// shorter one (10, a 12-pixel cell).
    #[test]
    fn a_cell_height_picks_the_first_em_that_fills_it_or_the_largest_that_fits() {
        assert_eq!(resolved(15, 0, 400, "Times New Roman"), (12, 12, 12, 3));
        assert_eq!(resolved(13, 0, 400, "Times New Roman"), (10, 10, 10, 2));
        assert_eq!(resolved(-13, 0, 400, "Times New Roman"), (13, 13, 12, 3));
        // The bold's extents differ: an 18-pixel cell is a 15-pixel em, 17 tall.
        assert_eq!(resolved(18, 0, 700, "Times New Roman"), (15, 15, 14, 3));
    }

    /// Times New Roman's average character width is 821 of 2048 units: 5 pixels at a 12-pixel em,
    /// so a width of 5 leaves the font as designed and 6 stretches it to a 13-pixel em width.
    #[test]
    fn a_width_other_than_the_face_s_own_average_stretches_the_font_sideways() {
        assert_eq!(resolved(15, 5, 500, "Times New Roman").1, 12);
        assert_eq!(resolved(15, 6, 500, "Times New Roman").1, 13);
        assert_eq!(resolved(35, 16, 500, "Times New Roman").1, 39);
        assert_eq!(resolved(35, 16, 900, "Times New Roman").1, 36);
        assert_eq!(resolved(14, 7, 700, "Courier New").1, 11);
    }

    #[test]
    fn a_face_s_full_name_or_its_weight_picks_the_carried_style() {
        let name = |face: &str, weight| Face::for_request(&spec(15, 6, weight, face)).data.as_ptr();
        assert_eq!(name("Times New Roman Italic", 500), SERIF_ITALIC.as_ptr());
        assert_eq!(name("Times New Roman", 900), SERIF_BOLD.as_ptr());
        assert_eq!(name("Times New Roman", 500), SERIF.as_ptr());
        assert_eq!(name("Courier New", 700), MONO_BOLD.as_ptr());
        assert_eq!(name("Arial", 700), SANS_BOLD.as_ptr());
        assert_eq!(name("Garamond", 400), SERIF.as_ptr());
    }

    #[test]
    fn the_carried_faces_average_width_is_the_one_windows_takes_for_the_faces_they_replace() {
        let average = |data: &[u8]| design(&FontRef::new(data).unwrap()).unwrap().average;
        assert!((average(SERIF) - 821.0).abs() < f64::EPSILON);
        assert!((average(SERIF_BOLD) - 874.0).abs() < f64::EPSILON);
        assert!((average(SERIF_ITALIC) - 823.0).abs() < f64::EPSILON);
        assert!((average(MONO_BOLD) - 1229.0).abs() < f64::EPSILON);
        assert!((average(SANS_BOLD) - 980.0).abs() < f64::EPSILON);
    }
}
