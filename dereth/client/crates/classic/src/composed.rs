//! Interface images the early-2005 portal does not have, composed from pieces it does have when
//! the client's own records do not hold them either, so the classic interface can show what later
//! worlds added in its own style without reading any later data.
//!
//! Each one is asked for by an id the early-2005 portal does not use: the later files' own id for
//! the same picture where there is one, else an id in a range no portal uses ([`BASE`] up).
//! [`crate::art`] reads the portal first, with the client's records over it, and composes only
//! what neither holds, so a record at one of these ids takes the composed picture's place.
//!
//! A composed paper-doll slot is a bevelled slot frame with a grey silhouette, as the classic
//! slots are: the frame and its colours are taken from the helmet slot, and the silhouette's shape
//! from an item icon of the portal.
use crate::art::Image;

/// The first id of the range no portal uses. The portal's interface images stay well below it.
pub const BASE: u32 = 0x06F0_0000;
/// The cloak slot: the client's own picture, else a slot frame with a garment's outline.
pub const CLOAK_SLOT: u32 = 0x0600_708F;
/// The trinket slot: the client's own picture, else a slot frame with a flask's outline.
pub const TRINKET_SLOT: u32 = 0x0600_6A6C;
/// The three aetheria sigil slots, blue, yellow and red: the client's own pictures, else a slot
/// frame with a crystal's outline.
pub const SIGIL_SLOTS: [u32; 3] = [0x0600_6BEF, 0x0600_6BF0, 0x0600_6BF1];

/// The classic helmet slot, whose bevel and colours every composed slot borrows.
const SLOT_FRAME: u32 = 0x0600_0f68;
/// The portal item icons whose outlines the composed slots show.
const GARMENT_ICON: u32 = 0x0600_32bf;
const FLASK_ICON: u32 = 0x0600_32c8;
const CRYSTAL_ICON: u32 = 0x0600_32d4;

/// The paper doll's accessories button, plain and lit (a slot behind it holds an item): ids of
/// Dereth's own in the range no portal uses. Composed, when no record holds them, as the shield
/// slot's bevelled frame with its inside cleared and three dots across it, grey on the plain
/// button and gold on the lit one, whose ground is warmed too.
pub const ACCESSORIES_BUTTON: [u32; 2] = [BASE + 0x20, BASE + 0x21];
/// The classic shield slot, whose frame the accessories button borrows.
const SHIELD_SLOT: u32 = 0x0600_0f6c;

/// The journal's toolbar button, normal, lit (its page shown) and pressed: the later files' ids
/// for the quest button's normal and lit pictures, the pressed one showing the lit picture as the
/// map's and the options' do. Composed, when no record holds them, as the map button's triangle
/// with its compass taken out and filled in from the triangle around it, and a quill on it in the
/// toolbar glyphs' white with a black edge, shaped from the portal's feather icon.
pub const JOURNAL_BUTTON: [u32; 3] = [0x0600_69AE, 0x0600_69AF, 0x0600_69AF];
/// Narrowed toolbar pictures: this, plus 64 for each picture of [`TOOLBAR_PICTURES`] before it,
/// plus the width.
const NARROWED: u32 = BASE + 0x100;
/// The toolbar's panel button pictures that can be narrowed, each cut evenly from both sides so
/// one more button fits on the row.
const TOOLBAR_PICTURES: [u32; 17] = [
    0x0600_111f,
    0x0600_1120,
    0x0600_1121,
    0x0600_1119,
    0x0600_111a,
    0x0600_111b,
    0x0600_1122,
    0x0600_1123,
    0x0600_1124,
    0x0600_1116,
    0x0600_1117,
    0x0600_1118,
    0x0600_111c,
    0x0600_111d,
    0x0600_111e,
    JOURNAL_BUTTON[0],
    JOURNAL_BUTTON[1],
];
/// The map button's pictures the journal's are made from, in the same order.
const TRIANGLES: [u32; 3] = [0x0600_1116, 0x0600_1117, 0x0600_1118];
/// The feather icon the quill is shaped from: 32 pixels square on black.
const FEATHER: u32 = 0x0600_216B;

/// The id of toolbar picture `id` cut to `width` pixels; `id` itself for a picture that is not
/// one of the toolbar's.
#[must_use]
pub fn narrowed(id: u32, width: u32) -> u32 {
    TOOLBAR_PICTURES
        .iter()
        .position(|p| *p == id)
        .map_or(id, |i| {
            NARROWED + 64 * crate::int::u32_from(i) + width.clamp(1, 63)
        })
}

/// Whether `id` is one this module composes when no record holds it.
#[must_use]
pub fn is_composed(id: u32) -> bool {
    (BASE..BASE + 0x1_0000).contains(&id)
        || JOURNAL_BUTTON.contains(&id)
        || [CLOAK_SLOT, TRINKET_SLOT].contains(&id)
        || SIGIL_SLOTS.contains(&id)
}

/// Compose image `id` from the portal images `portal` returns; `None` for an id this module does
/// not know or a piece the portal lacks.
pub fn compose(id: u32, portal: &dyn Fn(u32) -> Option<Image>) -> Option<Image> {
    if let Some(variant) = JOURNAL_BUTTON.iter().position(|j| *j == id) {
        // The normal picture's triangle is green; the lit and pressed ones' red.
        let mut image = blank(&portal(TRIANGLES[variant])?, usize::from(variant == 0));
        quill(&mut image, &portal(FEATHER)?, 9, 7);
        return Some(image);
    }
    if let Some(lit) = ACCESSORIES_BUTTON.iter().position(|b| *b == id) {
        return Some(dots(&portal(SHIELD_SLOT)?, lit == 1));
    }
    let narrowing = NARROWED..NARROWED + 64 * crate::int::u32_from(TOOLBAR_PICTURES.len());
    if narrowing.contains(&id) {
        let k = id - NARROWED;
        return Some(narrow(
            &portal(TOOLBAR_PICTURES[(k / 64) as usize])?,
            k % 64,
        ));
    }
    let icon = match id {
        CLOAK_SLOT => GARMENT_ICON,
        TRINKET_SLOT => FLASK_ICON,
        _ if SIGIL_SLOTS.contains(&id) => CRYSTAL_ICON,
        _ => return None,
    };
    Some(slot(&portal(SLOT_FRAME)?, &portal(icon)?))
}

fn pixel(image: &Image, x: u32, y: u32) -> [u8; 3] {
    let i = ((y * image.width + x) * 4) as usize;
    [image.rgba[i], image.rgba[i + 1], image.rgba[i + 2]]
}

fn luminance([r, g, b]: [u8; 3]) -> u32 {
    (u32::from(r) * 3 + u32::from(g) * 6 + u32::from(b)) / 10
}

/// The frame's bevel kept, its inside cleared to its own ground colour, and the icon's outline
/// (its pixels that are not the icon's black ground) painted in the frame's silhouette grey, shaded
/// by the icon's own brightness.
fn slot(frame: &Image, icon: &Image) -> Image {
    let (w, h) = (frame.width, frame.height);
    const BEVEL: u32 = 3;
    let ground = pixel(frame, BEVEL + 1, BEVEL + 1);
    // The silhouette grey: the middle of the frame's lit pixels inside its bevel, the tone the
    // classic slots draw their outlines in.
    let mut lit: Vec<[u8; 3]> = (BEVEL..h.saturating_sub(BEVEL))
        .flat_map(|y| (BEVEL..w.saturating_sub(BEVEL)).map(move |x| (x, y)))
        .map(|(x, y)| pixel(frame, x, y))
        .filter(|p| luminance(*p) > 30)
        .collect();
    lit.sort_by_key(|p| luminance(*p));
    let silhouette = lit.get(lit.len() / 2).copied().unwrap_or([68, 68, 68]);
    let mut out = frame.clone();
    let inside = |x: u32, y: u32| x >= BEVEL && y >= BEVEL && x + BEVEL < w && y + BEVEL < h;
    // The outline at three quarters of the frame's inside, centred.
    let span = (w - 2 * BEVEL) * 3 / 4;
    let (ox, oy) = ((w - span) / 2, (h - span) / 2);
    for y in 0..h {
        for x in 0..w {
            if !inside(x, y) {
                continue;
            }
            let mut colour = ground;
            if (ox..ox + span).contains(&x) && (oy..oy + span).contains(&y) {
                let ix = (x - ox) * icon.width / span;
                let iy = (y - oy) * icon.height / span;
                let p = pixel(icon, ix.min(icon.width - 1), iy.min(icon.height - 1));
                let lum = luminance(p);
                if lum > 24 {
                    // Shaded from three quarters to a fifth over the silhouette grey by the icon's
                    // brightness, as the classic outlines are lit.
                    let shade = 192 + lum.min(255) / 2;
                    colour =
                        silhouette.map(|c| u8::try_from(u32::from(c) * shade / 255).unwrap_or(c));
                }
            }
            let i = ((y * w + x) * 4) as usize;
            out.rgba[i..i + 3].copy_from_slice(&colour);
        }
    }
    out
}

/// The accessories button: the frame's bevel kept, its inside cleared to its own ground, and
/// three round dots across its middle; lit, the dots are gold and the ground is warmed towards
/// them.
fn dots(frame: &Image, lit: bool) -> Image {
    const BEVEL: u32 = 3;
    let (w, h) = (frame.width, frame.height);
    let ground = pixel(frame, BEVEL + 1, BEVEL + 1);
    let (ground, dot) = if lit {
        (
            [
                ground[0].saturating_add(34),
                ground[1].saturating_add(24),
                ground[2].saturating_add(4),
            ],
            [236, 196, 96],
        )
    } else {
        (ground, [150, 150, 150])
    };
    let mut out = frame.clone();
    // Dots four pixels across, a quarter of the width apart.
    let step = w / 4;
    let centres = [w / 2 - step, w / 2, w / 2 + step];
    let middle = h / 2;
    for y in BEVEL..h.saturating_sub(BEVEL) {
        for x in BEVEL..w.saturating_sub(BEVEL) {
            let on = centres.iter().any(|&c| {
                // The 4x4 square around the centre, less its corners.
                let (Some(lx), Some(ly)) = ((x + 2).checked_sub(c), (y + 2).checked_sub(middle))
                else {
                    return false;
                };
                lx < 4 && ly < 4 && !((lx == 0 || lx == 3) && (ly == 0 || ly == 3))
            });
            let i = ((y * w + x) * 4) as usize;
            out.rgba[i..i + 3].copy_from_slice(if on { &dot } else { &ground });
        }
    }
    out
}

fn narrow(source: &Image, width: u32) -> Image {
    let width = width.min(source.width);
    let left = (source.width - width) / 2;
    let mut rgba = Vec::with_capacity((width * source.height * 4) as usize);
    for y in 0..source.height {
        let row = ((y * source.width + left) * 4) as usize;
        rgba.extend_from_slice(&source.rgba[row..row + (width * 4) as usize]);
    }
    Image {
        width,
        height: source.height,
        rgba,
    }
}

/// Whether a pixel is of the triangle: channel `tint` (0 red, 1 green) clearly above the others.
fn tinted(p: [u8; 3], tint: usize) -> bool {
    let c = p[tint];
    c > 16 && (0..3).all(|k| k == tint || c > p[k].saturating_add(8))
}

/// Whether a pixel is of a glyph: black, or a near-grey white.
fn glyph(p: [u8; 3]) -> bool {
    let (lo, hi) = (*p.iter().min().unwrap_or(&0), *p.iter().max().unwrap_or(&0));
    hi <= 24 || (lo >= 150 && hi - lo <= 40)
}

/// The button's triangle with its glyph taken out: each glyph pixel between the triangle's
/// edges on its row is filled from the triangle around it, each in turn becoming the mean of its
/// neighbours until the fill settles.
fn blank(source: &Image, tint: usize) -> Image {
    let (w, h) = (source.width, source.height);
    let mut colours: Vec<[f32; 3]> = (0..w * h)
        .map(|i| pixel(source, i % w, i / w).map(f32::from))
        .collect();
    let mut holes = vec![];
    let mut is_hole = vec![false; (w * h) as usize];
    // The last row is the picture's black foot, no part of the triangle.
    for y in 0..h.saturating_sub(1) {
        let row: Vec<u32> = (0..w)
            .filter(|&x| tinted(pixel(source, x, y), tint))
            .collect();
        let (Some(&first), Some(&last)) = (row.first(), row.last()) else {
            continue;
        };
        for x in first..=last {
            if glyph(pixel(source, x, y)) {
                holes.push((x, y));
                is_hole[(y * w + x) as usize] = true;
                colours[(y * w + x) as usize] = [0.0; 3];
            }
        }
    }
    let known = |x: u32, y: u32, is_hole: &[bool]| {
        is_hole[(y * w + x) as usize] || tinted(pixel(source, x, y), tint)
    };
    for _ in 0..300 {
        for &(x, y) in &holes {
            let mut sum = [0.0f32; 3];
            let mut n = 0.0f32;
            for (dx, dy) in [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)] {
                let (nx, ny) = (x.wrapping_add_signed(dx), y.wrapping_add_signed(dy));
                if nx < w && ny < h.saturating_sub(1) && known(nx, ny, &is_hole) {
                    let c = colours[(ny * w + nx) as usize];
                    for k in 0..3 {
                        sum[k] += c[k];
                    }
                    n += 1.0;
                }
            }
            if n > 0.0 {
                colours[(y * w + x) as usize] = sum.map(|s| s / n);
            }
        }
    }
    let mut rgba = source.rgba.clone();
    for &(x, y) in &holes {
        let i = (y * w + x) as usize;
        for k in 0..3 {
            rgba[i * 4 + k] =
                u8::try_from(dereth_primitives::num::to_i32(colours[i][k].round())).unwrap_or(255);
        }
    }
    Image {
        width: w,
        height: h,
        rgba,
    }
}

/// A 16-pixel quill at `(left, top)`: the feather icon at half its size, its light parts white and
/// its dark ones grey, edged in black, with its shaft drawn across it in black.
fn quill(image: &mut Image, feather: &Image, left: u32, top: u32) {
    const SIZE: u32 = 16;
    let mut inside = [[false; SIZE as usize]; SIZE as usize];
    let mut light = [[false; SIZE as usize]; SIZE as usize];
    for gy in 0..SIZE {
        for gx in 0..SIZE {
            let (mut cover, mut lum) = (0u32, 0u32);
            for (x, y) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let (fx, fy) = (gx * 2 + x, gy * 2 + y);
                if fx >= feather.width || fy >= feather.height {
                    continue;
                }
                let p = pixel(feather, fx, fy);
                if p.iter().max().copied().unwrap_or(0) >= 8 {
                    cover += 1;
                    lum +=
                        (u32::from(p[0]) * 30 + u32::from(p[1]) * 59 + u32::from(p[2]) * 11) / 100;
                }
            }
            inside[gy as usize][gx as usize] = cover >= 2;
            light[gy as usize][gx as usize] = cover > 0 && lum / cover > 80;
        }
    }
    let at = |x: i32, y: i32| {
        (0..SIZE as i32).contains(&x)
            && (0..SIZE as i32).contains(&y)
            && inside[y as usize][x as usize]
    };
    let mut put = |x: i32, y: i32, c: [u8; 3]| {
        let (px, py) = (left as i32 + x, top as i32 + y);
        if px >= 0 && py >= 0 && (px as u32) < image.width && (py as u32) < image.height {
            let i = ((py as u32 * image.width + px as u32) * 4) as usize;
            image.rgba[i..i + 3].copy_from_slice(&c);
        }
    };
    for y in -1..=SIZE as i32 {
        for x in -1..=SIZE as i32 {
            if at(x, y) {
                let v = if light[y as usize][x as usize] {
                    255
                } else {
                    150
                };
                put(x, y, [v; 3]);
            } else if (-1..=1).any(|dy| (-1..=1).any(|dx| at(x + dx, y + dy))) {
                put(x, y, [0; 3]);
            }
        }
    }
    for i in 3..12 {
        put(i, 14 - i, [0; 3]);
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (art composed for the classic interface from its own portal's pieces).
    use super::*;

    fn flat(w: u32, h: u32, rgb: [u8; 3]) -> Image {
        Image {
            width: w,
            height: h,
            rgba: (0..w * h)
                .flat_map(|_| [rgb[0], rgb[1], rgb[2], 255])
                .collect(),
        }
    }

    #[test]
    fn a_composed_slot_keeps_the_frame_and_draws_the_icon_outline_in_the_silhouette_grey() {
        let mut frame = flat(32, 32, [60, 60, 60]);
        // A bevel pixel and a bright silhouette pixel inside.
        frame.rgba[0..3].copy_from_slice(&[200, 0, 0]);
        let at = ((10 * 32 + 10) * 4) as usize;
        frame.rgba[at..at + 3].copy_from_slice(&[150, 150, 150]);
        // An icon: black ground, a white block in its middle.
        let mut icon = flat(32, 32, [0, 0, 0]);
        for y in 12..20 {
            for x in 12..20 {
                let i = ((y * 32 + x) * 4) as usize;
                icon.rgba[i..i + 3].copy_from_slice(&[255, 255, 255]);
            }
        }
        let out = slot(&frame, &icon);
        assert_eq!(pixel(&out, 0, 0), [200, 0, 0], "the bevel is the frame's");
        assert_eq!(
            pixel(&out, 5, 5),
            [60, 60, 60],
            "the inside is the frame's ground"
        );
        let centre = pixel(&out, 16, 16);
        assert!(
            centre[0] > 60,
            "the outline is drawn lighter than the ground, in the silhouette grey: {centre:?}"
        );
        assert!(SIGIL_SLOTS.iter().all(|s| is_composed(*s)) && !is_composed(SLOT_FRAME));
        assert!(compose(BASE + 99, &|_| None).is_none());
    }

    fn solid(width: u32, height: u32, f: impl Fn(u32, u32) -> [u8; 3]) -> Image {
        let mut rgba = vec![];
        for y in 0..height {
            for x in 0..width {
                let [r, g, b] = f(x, y);
                rgba.extend_from_slice(&[r, g, b, 255]);
            }
        }
        Image {
            width,
            height,
            rgba,
        }
    }

    #[test]
    fn the_accessories_button_is_the_shield_frame_with_three_dots_grey_or_gold() {
        let mut frame = flat(32, 32, [20, 22, 26]);
        frame.rgba[0..3].copy_from_slice(&[200, 0, 0]);
        // Something of the shield's outline inside, which the button clears.
        let at = ((8 * 32 + 16) * 4) as usize;
        frame.rgba[at..at + 3].copy_from_slice(&[150, 150, 150]);
        let shield = |p: u32| (p == SHIELD_SLOT).then(|| frame.clone());
        let plain = compose(ACCESSORIES_BUTTON[0], &shield).unwrap();
        let lit = compose(ACCESSORIES_BUTTON[1], &shield).unwrap();
        for button in [&plain, &lit] {
            assert_eq!(pixel(button, 0, 0), [200, 0, 0], "the bevel is the frame's");
            assert_eq!(
                pixel(button, 16, 8),
                pixel(button, 5, 5),
                "the inside is cleared"
            );
            // Three dots across the middle, with ground between them.
            for x in [8, 16, 24] {
                assert_ne!(pixel(button, x, 16), pixel(button, 5, 5), "a dot at {x}");
            }
            assert_eq!(pixel(button, 12, 16), pixel(button, 5, 5));
            assert_eq!(pixel(button, 16, 12), pixel(button, 5, 5));
        }
        assert_eq!(pixel(&plain, 5, 5), [20, 22, 26]);
        assert!(pixel(&lit, 5, 5)[0] > 20, "the lit ground is warmed");
        assert!(
            pixel(&lit, 16, 16)[0] > pixel(&plain, 16, 16)[0],
            "the lit dots are gold"
        );
        assert!(ACCESSORIES_BUTTON.iter().all(|b| is_composed(*b)));
        assert!(compose(ACCESSORIES_BUTTON[0], &|_| None).is_none());
    }

    #[test]
    fn narrowing_cuts_evenly_from_both_sides() {
        let source = solid(34, 2, |x, _| [u8::try_from(x).unwrap_or(0), 0, 0]);
        let cut = narrow(&source, 30);
        assert_eq!((cut.width, cut.height), (30, 2));
        assert_eq!(cut.rgba[0], 2);
        assert_eq!(cut.rgba[(29 * 4) as usize], 31);
    }

    #[test]
    fn narrowed_ids_name_their_source_and_width() {
        let id = narrowed(0x0600_1116, 31);
        assert!(is_composed(id));
        assert_ne!(id, narrowed(0x0600_1117, 31));
        assert_ne!(id, narrowed(0x0600_1116, 30));
        // A picture that is not the toolbar's is left alone.
        assert_eq!(narrowed(0x0600_0f68, 31), 0x0600_0f68);
        let picture = flat(34, 2, [1, 2, 3]);
        let made = compose(id, &|p| (p == 0x0600_1116).then(|| picture.clone())).unwrap();
        assert_eq!((made.width, made.height), (31, 2));
    }

    #[test]
    fn a_blanked_triangle_has_no_glyph_left_between_its_edges() {
        // A green triangle row with a white and black glyph in the middle.
        let source = solid(12, 3, |x, y| {
            if y == 2 {
                [0, 0, 0]
            } else if (5..7).contains(&x) {
                if y == 0 {
                    [255, 255, 255]
                } else {
                    [0, 0, 0]
                }
            } else {
                [10, 100 + u8::try_from(x).unwrap_or(0), 10]
            }
        });
        let out = blank(&source, 1);
        for y in 0..2 {
            for x in 0..12 {
                assert!(
                    tinted(pixel(&out, x, y), 1),
                    "({x},{y}) {:?}",
                    pixel(&out, x, y)
                );
            }
        }
        // The foot stays black.
        assert_eq!(pixel(&out, 5, 2), [0, 0, 0]);
    }
}
