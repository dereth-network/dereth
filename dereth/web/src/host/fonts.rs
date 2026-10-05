//! The classic interface's text, drawn with the browser's own fonts.
//!
//! The classic interface asks for each font the way a Windows program does: a cell height and an
//! average character width in pixels, a weight and a face name ([`FontSpec`]). The desktop asks
//! the Windows font system; a browser has no such call, so the page draws each character into a
//! canvas with the font the browser picks for the face, and the result goes into the same coverage
//! atlas ([`FontAtlas`]). Nothing is shipped: the faces are the player's system's, as on the
//! desktop.
//!
//! The request is turned into the canvas's terms as the Windows font system reads it:
//! - **The height** is the whole cell, ascent and descent together, so the em size is the height
//!   times the face's em over its ascent and descent.
//! - **The width** is the face's average character width; the glyphs are stretched or squeezed
//!   sideways until that average is the width asked for. For the three faces the interface names,
//!   the average is the one that makes the advances come out as the Windows font system's do at the
//!   interface's own sizes ([`calibrated_average`]); for any other face, the face's own weighted
//!   average of the lowercase letters and the space, measured in the browser.
//! - **The face** names a family, or a family and a style (`Times New Roman Italic`); each family
//!   falls back to its metric-compatible free equivalents and then to the generic family, so a
//!   system without the Microsoft faces still draws text of the same widths, or close.
//!
//! The canvas draws grey coverage, without the Windows font system's hinting, so the glyphs are
//! slightly softer than the desktop's and an advance can differ from it by a pixel.
//!
//! [`FontAtlas`]: dereth_classic_dat::fonts::FontAtlas

use dereth_classic_dat::fonts::FontSpec;

/// The size the faces are measured at: the em of the faces' own design units, so their metrics
/// come out whole.
pub const MEASURE_PX: f64 = 2048.0;

/// A request as the canvas reads it: the families, the style, the weight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Family {
    /// The face the request names, without a style word.
    pub face: String,
    pub italic: bool,
    pub bold: bool,
    pub weight: i32,
}

/// What `spec` names: a family, with `Bold` or `Italic` after it taken as the style, as the
/// Windows font system takes a face's full name.
#[must_use]
pub fn family(spec: &FontSpec) -> Family {
    let mut face = spec.face.trim();
    let (mut italic, mut bold) = (spec.italic, false);
    loop {
        if let Some(rest) = face.strip_suffix(" Italic") {
            italic = true;
            face = rest.trim_end();
        } else if let Some(rest) = face.strip_suffix(" Bold") {
            bold = true;
            face = rest.trim_end();
        } else {
            break;
        }
    }
    let weight = if bold {
        spec.weight.max(700)
    } else {
        spec.weight
    };
    Family {
        face: face.to_owned(),
        italic,
        bold: weight >= 600,
        weight: weight.clamp(1, 1000),
    }
}

/// The families the canvas is asked for, in order: the face, its metric-compatible free
/// equivalents, then the generic family.
#[must_use]
pub fn families(face: &str) -> Vec<String> {
    let quoted = |f: &str| format!("\"{}\"", f.replace(['"', '\\'], ""));
    let mut out = vec![quoted(face)];
    let lower = face.to_ascii_lowercase();
    let (equivalents, generic): (&[&str], &str) = if lower.contains("courier") {
        (&["Liberation Mono", "Cousine"], "monospace")
    } else if lower.contains("arial") || lower.contains("helvetica") {
        (&["Liberation Sans", "Arimo"], "sans-serif")
    } else {
        (&["Liberation Serif", "Tinos"], "serif")
    };
    out.extend(equivalents.iter().map(|f| quoted(f)));
    out.push(generic.to_owned());
    out
}

/// The canvas's `font` for `family` at `px` pixels to the em.
#[must_use]
pub fn css_font(family: &Family, px: f64) -> String {
    format!(
        "{}{} {px}px {}",
        if family.italic { "italic " } else { "" },
        family.weight,
        families(&family.face).join(", ")
    )
}

/// The average character width, as a fraction of the em, that makes the canvas's advances come out
/// as the Windows font system's do for the faces and weights the classic interface names, at its
/// own sizes (fitted to them; the Windows font system's hinting makes its widths no simple function
/// of the request). `None` for any other face or weight.
#[must_use]
pub fn calibrated_average(face: &str, bold: bool) -> Option<f64> {
    let units = match (face.to_ascii_lowercase().as_str(), bold) {
        ("times new roman", false) => 858.0,
        ("times new roman", true) => 908.0,
        ("arial", true) => 1131.0,
        ("courier new", _) => 1229.0,
        _ => return None,
    };
    Some(units / 2048.0)
}

/// The lowercase letters and the space, with the weights the classic average character width is
/// taken over (per thousand).
pub const AVERAGE_WEIGHTS: [(char, f64); 27] = [
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

/// How a request is drawn: the em size, the sideways stretch, and the cell's metrics.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scale {
    /// Pixels to the em, upright.
    pub em: f64,
    /// The sideways stretch: 1 draws the face as designed.
    pub stretch: f64,
    /// From a line's top to its baseline, whole pixels.
    pub ascent: i32,
    /// From one line's top to the next's, whole pixels.
    pub line_height: i32,
}

/// The [`Scale`] for `spec`, from the face's ascent and descent and its average character width,
/// each a fraction of the em.
#[must_use]
pub fn scale(spec: &FontSpec, ascent: f64, descent: f64, average: f64) -> Scale {
    let height = f64::from(spec.height.max(1));
    let cell = (ascent + descent).max(f64::EPSILON);
    let em = height / cell;
    let natural = average * em;
    let stretch = if spec.width > 0 && natural > 0.0 {
        f64::from(spec.width) / natural
    } else {
        1.0
    };
    Scale {
        em,
        stretch,
        ascent: dereth_primitives::num::to_i32_f64((ascent * em).round()),
        line_height: spec.height.max(1),
    }
}

/// An advance as the atlas keeps it: whole pixels, from the advance at [`MEASURE_PX`].
#[must_use]
pub fn advance(measured: f64, scale: &Scale) -> i32 {
    dereth_primitives::num::to_i32_f64((measured / MEASURE_PX * scale.em * scale.stretch).round())
}

#[cfg(target_arch = "wasm32")]
pub use page::PageFonts;

#[cfg(target_arch = "wasm32")]
mod page {
    use dereth_classic_dat::fonts::{
        cp1252, measure_cells, FontAtlas, FontSource, FontSpec, CELL, COLUMNS, PAD, ROWS,
    };
    use wasm_bindgen::JsCast;
    use web_sys::{OffscreenCanvas, OffscreenCanvasRenderingContext2d};

    use super::{
        advance, calibrated_average, css_font, family, scale, AVERAGE_WEIGHTS, MEASURE_PX,
    };

    /// The browser's fonts, drawn into a canvas of the worker's own.
    #[derive(Debug, Default, Clone, Copy)]
    pub struct PageFonts;

    impl FontSource for PageFonts {
        fn rasterize(&self, spec: &FontSpec) -> Result<FontAtlas, String> {
            rasterize(spec).map_err(|e| format!("the browser would not draw {:?}: {e}", spec.face))
        }
    }

    fn js(e: wasm_bindgen::JsValue) -> String {
        e.as_string().unwrap_or_else(|| format!("{e:?}"))
    }

    fn context(width: u32, height: u32) -> Result<OffscreenCanvasRenderingContext2d, String> {
        OffscreenCanvas::new(width, height)
            .map_err(js)?
            .get_context("2d")
            .map_err(js)?
            .ok_or("no 2d canvas")?
            .dyn_into::<OffscreenCanvasRenderingContext2d>()
            .map_err(|_| "not a 2d canvas".to_owned())
    }

    fn rasterize(spec: &FontSpec) -> Result<FontAtlas, String> {
        let (aw, ah) = (COLUMNS * CELL, ROWS * CELL);
        let family = family(spec);
        let ctx = context(aw.unsigned_abs(), ah.unsigned_abs())?;
        // The face's metrics at its design em.
        ctx.set_font(&css_font(&family, MEASURE_PX));
        let probe = ctx.measure_text("Hg").map_err(js)?;
        let (ascent, descent) = (
            probe.font_bounding_box_ascent() / MEASURE_PX,
            probe.font_bounding_box_descent() / MEASURE_PX,
        );
        let mut chars = Vec::new();
        for (index, byte) in (32u8..=255).enumerate() {
            if byte == 127 {
                continue;
            }
            let Some(c) = cp1252(byte) else { continue };
            let width = ctx.measure_text(&c.to_string()).map_err(js)?.width();
            chars.push((index, c, width));
        }
        let average = match calibrated_average(&family.face, family.bold) {
            Some(a) => a,
            None => {
                let mut sum = 0.0;
                for (c, weight) in AVERAGE_WEIGHTS {
                    sum += weight * ctx.measure_text(&c.to_string()).map_err(js)?.width();
                }
                sum / 1000.0 / MEASURE_PX
            }
        };
        let scale = scale(spec, ascent, descent, average);
        ctx.set_font(&css_font(&family, scale.em));
        ctx.set_fill_style_str("#ffffff");
        ctx.set_text_baseline("alphabetic");
        let mut cells = Vec::with_capacity(chars.len());
        for &(index, c, measured) in &chars {
            let index = i32::try_from(index).unwrap_or(0);
            let (cx, cy) = (index % COLUMNS * CELL, index / COLUMNS * CELL);
            cells.push((u32::from(c), cx, cy, advance(measured, &scale)));
            ctx.set_transform(
                scale.stretch,
                0.0,
                0.0,
                1.0,
                f64::from(cx + PAD),
                f64::from(cy + PAD + scale.ascent),
            )
            .map_err(js)?;
            ctx.fill_text(&c.to_string(), 0.0, 0.0).map_err(js)?;
        }
        let image = ctx
            .get_image_data(0.0, 0.0, f64::from(aw), f64::from(ah))
            .map_err(js)?;
        let mut rgba = image.data().0;
        let alpha: Vec<u8> = rgba.iter().skip(3).step_by(4).copied().collect();
        for px in rgba.as_chunks_mut::<4>().0 {
            px[0] = 0xFF;
            px[1] = 0xFF;
            px[2] = 0xFF;
        }
        let glyphs = measure_cells(&alpha, aw, &cells, scale.ascent)?;
        Ok(FontAtlas {
            width: aw.unsigned_abs(),
            height: ah.unsigned_abs(),
            rgba,
            glyphs,
            line_height: scale.line_height,
            baseline: scale.ascent,
            face: spec.face.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the browser host's reading of a font request; the pixels are the browser's).
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

    #[test]
    fn a_face_s_full_name_names_its_family_and_style() {
        let f = family(&spec(15, 6, 500, "Times New Roman Italic"));
        assert_eq!(
            (f.face.as_str(), f.italic, f.bold, f.weight),
            ("Times New Roman", true, false, 500)
        );
        let f = family(&spec(14, 6, 400, "Arial Bold"));
        assert_eq!((f.face.as_str(), f.bold, f.weight), ("Arial", true, 700));
        let f = family(&spec(35, 16, 900, "Times New Roman"));
        assert_eq!(
            (f.face.as_str(), f.italic, f.bold),
            ("Times New Roman", false, true)
        );
    }

    #[test]
    fn each_face_falls_back_to_its_free_equivalents_and_its_generic_family() {
        assert_eq!(
            css_font(&family(&spec(15, 6, 500, "Times New Roman Italic")), 13.5),
            "italic 500 13.5px \"Times New Roman\", \"Liberation Serif\", \"Tinos\", serif"
        );
        assert_eq!(
            css_font(&family(&spec(14, 7, 700, "Courier New")), 12.0),
            "700 12px \"Courier New\", \"Liberation Mono\", \"Cousine\", monospace"
        );
        assert_eq!(
            families("Arial"),
            [
                "\"Arial\"",
                "\"Liberation Sans\"",
                "\"Arimo\"",
                "sans-serif"
            ]
        );
        // A quote in a face name cannot end the family list early.
        assert_eq!(families("x\" y")[0], "\"x y\"");
    }

    /// The cell height is ascent and descent together; the average width stretches the face
    /// sideways. Times New Roman's design metrics: ascent 1825 and descent 443 of 2048.
    #[test]
    fn the_height_is_the_whole_cell_and_the_width_the_average_character() {
        let (ascent, descent) = (1825.0 / 2048.0, 443.0 / 2048.0);
        let average = calibrated_average("Times New Roman", false).unwrap();
        let s = scale(
            &spec(15, 6, 500, "Times New Roman"),
            ascent,
            descent,
            average,
        );
        assert!((s.em - 15.0 * 2048.0 / 2268.0).abs() < 1e-9);
        assert_eq!((s.ascent, s.line_height), (12, 15));
        assert!((s.stretch * average * s.em - 6.0).abs() < 1e-9);
        // The em-wide advance at the measuring size is the em, stretched.
        assert_eq!(
            advance(MEASURE_PX, &s),
            dereth_primitives::num::to_i32_f64((s.em * s.stretch).round())
        );
        // No width asked for: the face as designed.
        let natural = scale(
            &spec(15, 0, 500, "Times New Roman"),
            ascent,
            descent,
            average,
        );
        assert!((natural.stretch - 1.0).abs() < f64::EPSILON);
        assert_eq!(calibrated_average("Garamond", false), None);
    }
}
