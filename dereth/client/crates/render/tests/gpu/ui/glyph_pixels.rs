//! Through the real D3D12 rasteriser and both samplers a UI glyph equals its source bitmap, ink
//! lands where pen and bearings say, a half-pixel displacement is caught only under linear, and the
//! two samplers are distinct.
//! Fixture: offscreen device rendering and pixel readback.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use dereth_primitives::{TextureData, TextureFormat};
use dereth_render::device::{Gpu, PerDrawConstants, PerFrameConstants, TextureSlot};
use dereth_render::font::{Font, FontAtlas, FontCharDesc, GlyphSheet, TextBatch};
use dereth_render::DrawConstants;

/// Opaque red makes the red readback channel equal the glyph coverage.
const RED: u32 = 0xFFFF_0000;

const SAMPLER_LINEAR: u32 = 1;
const SAMPLER_POINT: u32 = dereth_render::ui::pixel_rules::UI_GLYPH_SAMPLER;

/// The alpha cut-off for *"this pixel carries intermediate coverage"*. Stated because a count
/// produced by a threshold is not a fact until the threshold is stated, as required by the
/// test-design constraints: a pixel counts as intermediate when it is strictly inside
/// `CUT ..= 255 - CUT`.
const CUT: u8 = 8;

const SHEET_W: u32 = 37;
const SHEET_H: u32 = 23;

/// One row of the fixture font: the `FontCharDesc` fields the destination rectangle is built from.
/// The offsets are odd and the sheet is not a power of two, so a stride or a `1/256` left anywhere
/// would show.
#[derive(Debug, Clone, Copy)]
struct G {
    ch: u16,
    ox: u16,
    oy: u16,
    w: u8,
    h: u8,
    before: i8,
    after: i8,
    v_before: i8,
}

/// The eight `FontCharDesc` fields, in the order the 11-byte record carries them. Written as one
/// constructor rather than eight struct literals so the table below stays readable as a table.
#[allow(clippy::too_many_arguments)]
const fn g(ch: u8, ox: u16, oy: u16, w: u8, h: u8, before: i8, after: i8, v_before: i8) -> G {
    G {
        ch: ch as u16,
        ox,
        oy,
        w,
        h,
        before,
        after,
        v_before,
    }
}

const GLYPHS: &[G] = &[
    g(b'A', 1, 2, 6, 9, 1, 1, 2),
    // A negative left side bearing, which every retail font has some of, so the destination can be
    // to the *left* of the pen.
    g(b'B', 9, 2, 5, 9, -1, 2, 2),
    g(b'C', 17, 3, 7, 7, 0, 1, 4),
    // One tall glyph, so the vertical offset is not constant across the string.
    g(b'D', 26, 1, 4, 11, 2, 0, 0),
];

fn font() -> Font {
    Font {
        max_char_height: 12,
        max_char_width: 7,
        char_descs: GLYPHS
            .iter()
            .map(|d| FontCharDesc {
                unicode: d.ch,
                offset_x: d.ox,
                offset_y: d.oy,
                width: d.w,
                height: d.h,
                horizontal_offset_before: d.before,
                horizontal_offset_after: d.after,
                vertical_offset_before: d.v_before,
            })
            .collect(),
        num_horizontal_border_pixels: 0,
        num_vertical_border_pixels: 0,
        baseline_offset: 9,
        foreground_surface_data_id: 0x0600_0001,
        background_surface_data_id: 0,
    }
}

/// The A8 coverage sheet: hard interiors, soft edges, and a deterministic pattern so the assertion
/// is against a value that varies per texel rather than a constant a shifted read could still hit.
fn source_alpha() -> Vec<u8> {
    let mut a = vec![0u8; (SHEET_W * SHEET_H) as usize];
    for d in GLYPHS {
        for r in 0..u32::from(d.h) {
            for c in 0..u32::from(d.w) {
                let (x, y) = (u32::from(d.ox) + c, u32::from(d.oy) + r);
                let edge = c == 0 || r == 0 || c + 1 == u32::from(d.w) || r + 1 == u32::from(d.h);
                // Interiors are hard; edges take one of four intermediate levels, cycling so that
                // no two adjacent edge texels are equal.
                let v = if edge {
                    [37u8, 96, 158, 211][((c * 3 + r * 5) % 4) as usize]
                } else {
                    255
                };
                a[(y * SHEET_W + x) as usize] = v;
            }
        }
    }
    a
}

/// One measurement: what the rendered frame's red channel is, per pixel.
struct Shot {
    red: Vec<u8>,
    fb: (u32, u32),
}

impl Shot {
    fn at(&self, x: i32, y: i32) -> u8 {
        if x < 0 || y < 0 || x >= self.fb.0 as i32 || y >= self.fb.1 as i32 {
            return 0;
        }
        self.red[y as usize * self.fb.0 as usize + x as usize]
    }
}

/// Draw `text` at `(pen_x, pen_y)` with `sampler`, optionally displacing every vertex by
/// `shift_px` **destination pixels** in x, and read the frame back.
#[allow(clippy::too_many_arguments)]
fn draw(
    gpu: &mut Gpu,
    atlas: &FontAtlas,
    slot: TextureSlot,
    text: &str,
    pen_x: i32,
    pen_y: i32,
    sampler: u32,
    fb: (u32, u32),
    shift_px: f32,
) -> Shot {
    let mut batch = TextBatch::new();
    batch.begin();
    let clip = (0, 0, fb.0 as i32 - 1, fb.1 as i32 - 1);
    let mut pen = pen_x;
    for ch in text.encode_utf16() {
        assert!(
            batch.draw_ui_glyph(atlas, ch, pen, pen_y, RED, clip, fb),
            "glyph {ch:#06X} was dropped: the fixture must draw every character it names"
        );
        pen += advance(ch);
    }
    let (mut verts, _) = batch.end();
    assert_eq!(
        verts.len(),
        text.chars().count() * 6,
        "six vertices per glyph, unindexed"
    );
    if shift_px != 0.0 {
        // Clip x spans 2 units over `fb.0` pixels, so one destination pixel is `2/fb.0`.
        #[allow(clippy::cast_precision_loss)]
        let d = shift_px * 2.0 / fb.0 as f32;
        for v in &mut verts {
            v.origin[0] += d;
        }
    }
    let per_frame = PerFrameConstants {
        view_proj: identity16(),
        view: identity16(),
        fog_params: [0.0; 4],
        fog_color: [0.0; 4],
        ambient: [0.0; 4],
        screen: [0.0; 4],
    };
    gpu.begin_frame().expect("begin_frame");
    gpu.bind_texture(slot, sampler);
    gpu.draw_dynamic(
        &TextBatch::new().pipeline_key(),
        &DrawConstants::default(),
        &per_frame,
        &PerDrawConstants::identity(),
        &pack(&verts),
    )
    .expect("draw_dynamic");
    gpu.end_frame().expect("end_frame");
    let cap = gpu.capture().expect("capture");
    // BGRA: index 2 is red.
    let red = cap.bgra.as_chunks::<4>().0.iter().map(|p| p[2]).collect();
    Shot { red, fb }
}

/// The character's advance width, from the **fixture's own table** rather than from the atlas,
/// so the pen walk is independent of the code under test.
fn advance(ch: u16) -> i32 {
    let d = GLYPHS
        .iter()
        .find(|d| d.ch == ch)
        .expect("a glyph the fixture declares");
    i32::from(d.before) + i32::from(d.w) + i32::from(d.after)
}

/// The whole frame the source bitmap says should be there, built from `GLYPHS` and the pen —
/// The glyph rectangle builder's destination rectangle, in integers, exactly as the client builds it.
fn expected(a8: &[u8], text: &str, pen_x: i32, pen_y: i32, fb: (u32, u32)) -> Vec<u8> {
    let mut out = vec![0u8; (fb.0 * fb.1) as usize];
    let mut pen = pen_x;
    for ch in text.encode_utf16() {
        let d = GLYPHS
            .iter()
            .find(|d| d.ch == ch)
            .expect("a glyph the fixture declares");
        let left = pen + i32::from(d.before);
        let top = pen_y + i32::from(d.v_before);
        for r in 0..i32::from(d.h) {
            for c in 0..i32::from(d.w) {
                let (dx, dy) = (left + c, top + r);
                if dx < 0 || dy < 0 || dx >= fb.0 as i32 || dy >= fb.1 as i32 {
                    continue;
                }
                let a = a8[(i32::from(d.oy) + r) as usize * SHEET_W as usize
                    + (i32::from(d.ox) + c) as usize];
                out[dy as usize * fb.0 as usize + dx as usize] = a;
            }
        }
        pen += advance(ch);
    }
    out
}

/// `(pixels differing from the source, worst channel difference, pixels carrying intermediate
/// coverage where the source bitmap has none)`.
fn compare(shot: &Shot, want: &[u8]) -> (u64, u8, u64) {
    crate::common::text_pixels::compare(&shot.red, want, CUT)
}

/// The bounding box of everything the frame drew, so placement can be asserted without going
/// through the atlas the renderer read.
fn ink_box(shot: &Shot) -> Option<(i32, i32, i32, i32)> {
    let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    for y in 0..shot.fb.1 as i32 {
        for x in 0..shot.fb.0 as i32 {
            if shot.at(x, y) != 0 {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    (x0 <= x1).then_some((x0, y0, x1, y1))
}

/// Every framebuffer this runs at. `800x600` is the client's own; the other two are deliberately
/// odd and non-power-of-two, because a tidy size is where a half-pixel error cancels (§7.19).
const FRAMEBUFFERS: &[(u32, u32)] = &[(800, 600), (801, 601), (813, 617)];
/// An even pen and an odd one.
const PENS: &[(i32, i32)] = &[(100, 50), (103, 57)];

const TEXT: &str = "ABCDABDC";

/// Behaviour: ui.text.a-ui-glyph-is-drawn-pixel-identical-to-its-source-bitmap
#[test]
fn a_ui_glyph_is_pixel_identical_to_its_source_bitmap_under_both_samplers() {
    require_device();
    let a8 = source_alpha();
    let bgra = source_bgra(&a8);
    let f = font();
    let atlas = FontAtlas::from_glyph_sheet(
        &f,
        GlyphSheet {
            width: SHEET_W,
            height: SHEET_H,
            bgra: &bgra,
        },
    )
    .expect("the fixture sheet builds an atlas");

    let mut measured = 0u32;
    for &fb in FRAMEBUFFERS {
        let mut gpu = warp(fb.0, fb.1);
        let tex = TextureData {
            width: SHEET_W,
            height: SHEET_H,
            format: TextureFormat::Bgra8,
            levels: vec![bgra.clone()],
        };
        let slot = gpu.upload_texture(&tex).expect("the sheet uploads");
        for &(px, py) in PENS {
            let want = expected(&a8, TEXT, px, py, fb);
            let covered = want.iter().filter(|a| **a != 0).count();
            assert!(
                covered > 100,
                "the fixture must actually draw something: {covered} px"
            );

            for sampler in [SAMPLER_LINEAR, SAMPLER_POINT] {
                let shot = draw(&mut gpu, &atlas, slot, TEXT, px, py, sampler, fb, 0.0);
                let (differ, worst, soft) = compare(&shot, &want);
                assert_eq!(
                    (differ, worst, soft),
                    (0, 0, 0),
                    "{}x{} pen ({px},{py}) sampler {sampler}: {differ} of {covered} covered pixels \
                     differ from the source bitmap (worst {worst}), {soft} carry intermediate \
                     coverage where the source has none at cut-off {CUT}",
                    fb.0,
                    fb.1
                );
                measured += 1;
            }
        }
    }
    assert_eq!(
        usize::try_from(measured).expect("a small count"),
        FRAMEBUFFERS.len() * PENS.len() * 2,
        "every configuration must have been measured"
    );
    eprintln!("{measured} pixel-exactness configurations measured, all equal to the source");
}

/// Placement, asserted **without** consulting the atlas: the ink's bounding box is where the
/// fixture's own metrics say the glyph rectangle builder's destination rectangle is.
///
/// The pixel-equality test above computes its expectation from the same bearings the renderer
/// reads, so a *systematic* placement error would cancel between the two. This one closes that:
/// the box is derived from `GLYPHS` and the pen alone.
#[test]
fn the_ink_lands_where_the_pen_and_the_bearings_say_and_nowhere_else() {
    require_device();
    let a8 = source_alpha();
    let bgra = source_bgra(&a8);
    let f = font();
    let atlas = FontAtlas::from_glyph_sheet(
        &f,
        GlyphSheet {
            width: SHEET_W,
            height: SHEET_H,
            bgra: &bgra,
        },
    )
    .expect("atlas");

    let mut placed = 0u32;
    for &fb in FRAMEBUFFERS {
        let mut gpu = warp(fb.0, fb.1);
        let tex = TextureData {
            width: SHEET_W,
            height: SHEET_H,
            format: TextureFormat::Bgra8,
            levels: vec![bgra.clone()],
        };
        let slot = gpu.upload_texture(&tex).expect("upload");
        for &(px, py) in PENS {
            // Where the string's ink must be, from the fixture's table only.
            let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
            let mut pen = px;
            for ch in TEXT.encode_utf16() {
                let d = GLYPHS.iter().find(|d| d.ch == ch).expect("declared");
                let left = pen + i32::from(d.before);
                let top = py + i32::from(d.v_before);
                x0 = x0.min(left);
                y0 = y0.min(top);
                x1 = x1.max(left + i32::from(d.w) - 1);
                y1 = y1.max(top + i32::from(d.h) - 1);
                pen += advance(ch);
            }
            let shot = draw(
                &mut gpu,
                &atlas,
                slot,
                TEXT,
                px,
                py,
                SAMPLER_LINEAR,
                fb,
                0.0,
            );
            assert_eq!(
                ink_box(&shot),
                Some((x0, y0, x1, y1)),
                "{}x{} pen ({px},{py}): the drawn ink's bounding box is not the destination \
                 rectangle the pen and the side bearings name",
                fb.0,
                fb.1
            );
            placed += 1;
        }
    }
    assert_eq!(
        usize::try_from(placed).expect("a small count"),
        FRAMEBUFFERS.len() * PENS.len(),
        "every configuration must have been measured"
    );
    eprintln!("ink box asserted in {placed} configurations");
}

/// **The calibration, in both directions and from one harness.**
///
/// §7.8: a zero is worth nothing until the instrument has produced a non-zero on something known
/// to be non-zero. §7.14: a difference is worth nothing until it has produced a zero on something
/// known to be zero. The test above is the zero; this is the non-zero, at three displacements, and
/// it also exercises the glyph alpha composition:
///
/// * displaced half a pixel, `LINEAR` blends two texels and the frame stops matching the source —
///   which is what "blurry" *is*, expressed as pixels;
/// * displaced half a pixel, `POINT` is unmoved, because the sample point is still inside the same
///   texel. A half-pixel error is therefore **invisible** under the client's own sampler and
///   visible under ours;
/// * displaced a whole pixel, **both** samplers move, which is what stops the `POINT` zeros above
///   from being vacuous.
#[test]
fn a_displaced_glyph_reddens_this_instrument_and_only_linear_sees_half_a_pixel() {
    require_device();
    let a8 = source_alpha();
    let bgra = source_bgra(&a8);
    let f = font();
    let atlas = FontAtlas::from_glyph_sheet(
        &f,
        GlyphSheet {
            width: SHEET_W,
            height: SHEET_H,
            bgra: &bgra,
        },
    )
    .expect("atlas");

    let mut cases = 0u32;
    for &fb in FRAMEBUFFERS {
        let mut gpu = warp(fb.0, fb.1);
        let tex = TextureData {
            width: SHEET_W,
            height: SHEET_H,
            format: TextureFormat::Bgra8,
            levels: vec![bgra.clone()],
        };
        let slot = gpu.upload_texture(&tex).expect("upload");
        for &(px, py) in PENS {
            let want = expected(&a8, TEXT, px, py, fb);

            // Half a pixel under LINEAR: the defect the row named, injected on purpose.
            let half_linear = compare(
                &draw(
                    &mut gpu,
                    &atlas,
                    slot,
                    TEXT,
                    px,
                    py,
                    SAMPLER_LINEAR,
                    fb,
                    0.5,
                ),
                &want,
            );
            assert!(
                half_linear.0 > 200,
                "{}x{}: half a pixel under LINEAR must redden this instrument, got {half_linear:?}",
                fb.0,
                fb.1
            );
            assert!(
                half_linear.2 > 50,
                "{}x{}: half a pixel under LINEAR must put intermediate coverage where the source \
             bitmap has none (cut-off {CUT}), got {half_linear:?}",
                fb.0,
                fb.1
            );

            // The same displacement under POINT: nothing moves, because the sample point stays in its
            // own texel. This is why rule 4 is not decorative.
            let half_point = compare(
                &draw(&mut gpu, &atlas, slot, TEXT, px, py, SAMPLER_POINT, fb, 0.5),
                &want,
            );
            // A half-pixel displacement puts the POINT sample exactly on a texel boundary, and
            // which texel an exact boundary belongs to is a tie the Vulkan specification leaves
            // to the implementation (WARP and lavapipe take the near side; a hardware
            // rasteriser's interpolated coordinate lands a ULP either way). The claim is asserted
            // where it is defined -- on a software device -- and reported elsewhere.
            if gpu.adapter_kind() == dereth_render::device::AdapterKind::Software {
                assert_eq!(
                    half_point,
                    (0, 0, 0),
                    "{}x{}: POINT cannot see half a pixel",
                    fb.0,
                    fb.1
                );
            } else {
                eprintln!(
                    "{}x{} POINT at half a pixel on hardware: {half_point:?} (tie, not asserted)",
                    fb.0, fb.1
                );
            }

            // A whole pixel: both samplers move, so the POINT zeros are not the instrument being deaf.
            let (mut whole_linear, mut whole_point) = (0u64, 0u64);
            for sampler in [SAMPLER_LINEAR, SAMPLER_POINT] {
                let whole = compare(
                    &draw(&mut gpu, &atlas, slot, TEXT, px, py, sampler, fb, 1.0),
                    &want,
                );
                assert!(
                    whole.0 > 200,
                    "{}x{} sampler {sampler}: a whole pixel must redden it, got {whole:?}",
                    fb.0,
                    fb.1
                );
                if sampler == SAMPLER_LINEAR {
                    whole_linear = whole.0;
                } else {
                    whole_point = whole.0;
                }
            }
            eprintln!(
                "/{}x{} pen ({px},{py}): a HALF pixel reads {} differing and {} \
             intermediate-coverage px under LINEAR (sampler {SAMPLER_LINEAR}) and {half_point:?} \
             under POINT (sampler {SAMPLER_POINT}); a WHOLE pixel reads {whole_linear} under \
             LINEAR and {whole_point} under POINT",
                fb.0, fb.1, half_linear.0, half_linear.2,
            );
            cases += 1;
        }
    }
    assert_eq!(
        usize::try_from(cases).expect("a small count"),
        FRAMEBUFFERS.len() * PENS.len(),
        "every framebuffer and pen must have been calibrated"
    );
    eprintln!("calibrated in both directions at {cases} framebuffer/pen pairs");
}

/// The two samplers this file compares are distinct.
#[test]
fn the_two_samplers_this_file_compares_are_distinct() {
    assert_ne!(
        SAMPLER_POINT, SAMPLER_LINEAR,
        "the POINT and LINEAR arms resolved to the same sampler descriptor, so every \
         'invisible under POINT' measurement in this file is comparing LINEAR with itself"
    );
    assert_eq!(SAMPLER_LINEAR, 1, "`create_samplers`' linear/clamp entry");
}

use crate::common::text_pixels::{bgra as source_bgra, identity16, pack, require_device, warp};
