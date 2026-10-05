//! The outline sheet is pixel identical to its source; the neighbourhood pass draws eight glyphs at
//! their offsets behind an unmoved foreground; the destination dilates by the field border counts;
//! a displaced outline is caught; the atlas carries both border counts.
//! Fixture: offscreen device rendering and pixel readback.

#![cfg(gpu)]

use dereth_primitives::{TextureData, TextureFormat};
use dereth_render::device::{Gpu, PerDrawConstants, PerFrameConstants, TextureSlot};
use dereth_render::font::{
    Font, FontAtlas, FontCharDesc, GlyphSheet, TextBatch, OUTLINE_NEIGHBOURHOOD,
};
use dereth_render::DrawConstants;

const RED: u32 = 0xFFFF_0000;
/// Opaque black, the outline colour's default — used where the *outline* needs to
/// be told apart from the foreground in one frame, in the blue channel.
const BLUE: u32 = 0xFF00_00FF;

const SAMPLER_POINT: u32 = dereth_render::ui::pixel_rules::UI_GLYPH_SAMPLER;
/// `create_samplers`' linear/clamp entry.
const SAMPLER_LINEAR: u32 = 1;

/// The alpha cut-off for *"this pixel carries intermediate coverage"*, stated because a count
/// produced by a threshold is not a fact until the threshold is stated.
const CUT: u8 = 8;

const SHEET_W: u32 = 48;
const SHEET_H: u32 = 32;
const HB: u32 = 3;
const VB: u32 = 1;

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
    g(b'A', 4, 4, 6, 9, 1, 7, 2),
    g(b'B', 14, 4, 5, 9, -1, 7, 2),
    g(b'C', 24, 5, 7, 7, 0, 7, 4),
    g(b'D', 35, 3, 4, 11, 2, 7, 0),
];

const TEXT: &str = "ABCDABDC";

/// A font with the two border counts and, when `background`, a background sheet.
fn font(hb: u32, vb: u32, background: bool) -> Font {
    Font {
        max_char_height: 14,
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
        num_horizontal_border_pixels: hb,
        num_vertical_border_pixels: vb,
        baseline_offset: 11,
        foreground_surface_data_id: 0x0600_0001,
        background_surface_data_id: u32::from(background) * 0x0600_0002,
    }
}

fn foreground_alpha() -> Vec<u8> {
    let mut a = vec![0u8; (SHEET_W * SHEET_H) as usize];
    for d in GLYPHS {
        for r in 0..u32::from(d.h) {
            for c in 0..u32::from(d.w) {
                let (x, y) = (u32::from(d.ox) + c, u32::from(d.oy) + r);
                let edge = c == 0 || r == 0 || c + 1 == u32::from(d.w) || r + 1 == u32::from(d.h);
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

fn background_alpha() -> Vec<u8> {
    let mut a = vec![0u8; (SHEET_W * SHEET_H) as usize];
    for d in GLYPHS {
        let hb = i32::try_from(HB).expect("small");
        let vb = i32::try_from(VB).expect("small");
        for r in -vb..(i32::from(d.h) + vb) {
            for c in -hb..(i32::from(d.w) + hb) {
                let x = i32::from(d.ox) + c;
                let y = i32::from(d.oy) + r;
                if x < 0 || y < 0 {
                    continue;
                }
                // A value that varies with **both** axes and repeats with period 7 in x and 5 in
                // y, so no shift by 1, 2 or 3 in either axis maps the pattern onto itself.
                #[allow(clippy::cast_sign_loss)]
                let v = 23 + ((x * 29 + y * 53) % 197) as u8;
                a[(y as u32 * SHEET_W + x as u32) as usize] = v;
            }
        }
    }
    a
}

/// One read-back frame, kept as **two** colour channels so a two-pass frame can be separated:
/// the outline is drawn in blue and the foreground in red, and the text material's
/// `TEXOP_SELECTARG2(DIFFUSE)` colour keeps them in their own channels.
struct Shot {
    red: Vec<u8>,
    blue: Vec<u8>,
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

/// Which pass, or passes, to submit — and in which order, because a text batch's only ordering is
/// submission order.
///
/// The `0x9000` arm is absent on purpose: it is asserted **per offset**, one draw to a frame, in
/// [`the_neighbourhood_outline_draws_eight_glyphs_each_exactly_where_its_offset_says`], because
/// the union of eight overlapping draws is a blob that a wrong offset, a duplicated offset and a
/// missing one could all still produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pass {
    /// The outline pass alone, `0x7000` arm.
    OutlineSheet,
    /// The foreground pass alone, flags `0x1000`.
    Foreground,
    /// Outline (`0x7000`) then foreground, which is what the loop does for an outlined element.
    Both,
}

/// Submit `pass` and read the frame back.
///
/// `shift_px` displaces every vertex by that many **destination pixels** in x, which is the
/// calibration's injected defect and is zero everywhere else.
#[allow(clippy::too_many_arguments)]
fn draw(
    gpu: &mut Gpu,
    atlas: &FontAtlas,
    fg_tex: TextureSlot,
    outline_tex: TextureSlot,
    pass: Pass,
    pen: (i32, i32),
    sampler: u32,
    fb: (u32, u32),
    shift_px: f32,
) -> Shot {
    let clip = (0, 0, fb.0 as i32 - 1, fb.1 as i32 - 1);

    // Each element of `steps` is one draw call: its texture, its colour, and the glyph submission
    // that fills it.
    let mut steps: Vec<(TextureSlot, u32, Pass)> = Vec::new();
    match pass {
        Pass::OutlineSheet => steps.push((outline_tex, RED, Pass::OutlineSheet)),
        Pass::Foreground => steps.push((fg_tex, RED, Pass::Foreground)),
        Pass::Both => {
            // Outline first, in blue, then the foreground over it in red.
            steps.push((outline_tex, BLUE, Pass::OutlineSheet));
            steps.push((fg_tex, RED, Pass::Foreground));
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
    for (tex, colour, what) in steps {
        let mut batch = TextBatch::new();
        batch.begin();
        let mut x = pen.0;
        for ch in TEXT.encode_utf16() {
            match what {
                Pass::OutlineSheet => assert!(
                    batch.draw_ui_outline_glyph(atlas, ch, x, pen.1, colour, clip, fb),
                    "outline glyph {ch:#06X} was dropped: the fixture must draw every character"
                ),
                Pass::Foreground => assert!(
                    batch.draw_ui_glyph(atlas, ch, x, pen.1, colour, clip, fb),
                    "glyph {ch:#06X} was dropped"
                ),
                Pass::Both => unreachable!("`Both` is expanded into two steps above"),
            }
            x += advance(ch);
        }
        let (mut verts, _) = batch.end();
        if shift_px != 0.0 {
            #[allow(clippy::cast_precision_loss)]
            let d = shift_px * 2.0 / fb.0 as f32;
            for v in &mut verts {
                v.origin[0] += d;
            }
        }
        gpu.bind_texture(tex, sampler);
        gpu.draw_dynamic(
            &TextBatch::new().pipeline_key(),
            &DrawConstants::default(),
            &per_frame,
            &PerDrawConstants::identity(),
            &pack(&verts),
        )
        .expect("draw_dynamic");
    }
    gpu.end_frame().expect("end_frame");
    let cap = gpu.capture().expect("capture");
    let px = cap.bgra.as_chunks::<4>().0;
    Shot {
        red: px.iter().map(|p| p[2]).collect(),
        blue: px.iter().map(|p| p[0]).collect(),
    }
}

/// The frame the **foreground** source bitmap says should be there: the glyph rectangle builder's
/// destination rectangle, in integers, exactly as the client builds it, with `offset` added to
/// every pen (which is how the eight neighbourhood draws are expressed).
fn expect_foreground(a8: &[u8], pen: (i32, i32), offset: (i32, i32), fb: (u32, u32)) -> Vec<u8> {
    let mut out = vec![0u8; (fb.0 * fb.1) as usize];
    let mut x = pen.0;
    for ch in TEXT.encode_utf16() {
        let d = GLYPHS.iter().find(|d| d.ch == ch).expect("declared");
        let left = x + i32::from(d.before) + offset.0;
        let top = pen.1 + i32::from(d.v_before) + offset.1;
        for r in 0..i32::from(d.h) {
            for c in 0..i32::from(d.w) {
                let (dx, dy) = (left + c, top + r);
                if dx < 0 || dy < 0 || dx >= fb.0 as i32 || dy >= fb.1 as i32 {
                    continue;
                }
                let a = a8[(i32::from(d.oy) + r) as usize * SHEET_W as usize
                    + (i32::from(d.ox) + c) as usize];
                let o = &mut out[dy as usize * fb.0 as usize + dx as usize];
                *o = (*o).max(a);
            }
        }
        x += advance(ch);
    }
    out
}

/// The frame the **background** source bitmap says should be there.
///
/// This is the whole point of the file, so it is written from the retail rectangle rule rather than by
/// calling anything in `dereth-render`:
///
/// ```text
/// src = (ox - hb, oy - vb, ox + w + hb, oy + h + vb)
/// dst = (dx - hb, dy - vb, dx + w + hb, dy + h + hb)     <- bottom uses hb
/// ```
fn expect_outline(a8: &[u8], pen: (i32, i32), hb: i32, vb: i32, fb: (u32, u32)) -> Vec<u8> {
    let mut out = vec![0u8; (fb.0 * fb.1) as usize];
    let mut x = pen.0;
    for ch in TEXT.encode_utf16() {
        let d = GLYPHS.iter().find(|d| d.ch == ch).expect("declared");
        let left = x + i32::from(d.before);
        let top = pen.1 + i32::from(d.v_before);
        // Destination rows run from `-vb` to `h + hb - 1`; source rows follow one for one from
        // `oy - vb`, which is what a 1:1 blit does and what the two rectangles agree on whenever
        // `hb == vb` — true of every shipped font.
        for r in -vb..(i32::from(d.h) + hb) {
            for c in -hb..(i32::from(d.w) + hb) {
                let (dx, dy) = (left + c, top + r);
                if dx < 0 || dy < 0 || dx >= fb.0 as i32 || dy >= fb.1 as i32 {
                    continue;
                }
                let (sx, sy) = (i32::from(d.ox) + c, i32::from(d.oy) + r);
                if sx < 0 || sy < 0 || sx >= SHEET_W as i32 || sy >= SHEET_H as i32 {
                    continue;
                }
                let a = a8[sy as usize * SHEET_W as usize + sx as usize];
                let o = &mut out[dy as usize * fb.0 as usize + dx as usize];
                *o = (*o).max(a);
            }
        }
        x += advance(ch);
    }
    out
}

fn compare(got: &[u8], want: &[u8]) -> (u64, u8, u64) {
    crate::common::text_pixels::compare(got, want, CUT)
}

const FRAMEBUFFERS: &[(u32, u32)] = &[(800, 600), (801, 601), (813, 617)];
const PENS: &[(i32, i32)] = &[(100, 50), (103, 57)];

/// The fixture, built once per framebuffer: the atlas, and the two uploaded glyph textures.
struct Fixture {
    atlas: FontAtlas,
    fg: TextureSlot,
    outline: TextureSlot,
}

fn fixture(gpu: &mut Gpu, hb: u32, vb: u32, background: bool) -> Fixture {
    let fg_a8 = foreground_alpha();
    let bg_a8 = background_alpha();
    let fg_bgra = bgra(&fg_a8);
    let bg_bgra = bgra(&bg_a8);
    let f = font(hb, vb, background);
    let atlas = FontAtlas::from_glyph_sheets(
        &f,
        GlyphSheet {
            width: SHEET_W,
            height: SHEET_H,
            bgra: &fg_bgra,
        },
        background.then_some(GlyphSheet {
            width: SHEET_W,
            height: SHEET_H,
            bgra: &bg_bgra,
        }),
    )
    .expect("the fixture sheets build an atlas");
    assert_eq!(
        atlas.has_outline_sheet(),
        background,
        "the atlas must carry the outline sheet exactly when the font names one"
    );
    let up = |gpu: &mut Gpu, b: &[u8]| {
        gpu.upload_texture(&TextureData {
            width: SHEET_W,
            height: SHEET_H,
            format: TextureFormat::Bgra8,
            levels: vec![b.to_vec()],
        })
        .expect("the sheet uploads")
    };
    let fg = up(gpu, &fg_bgra);
    let outline = up(gpu, &bg_bgra);
    Fixture { atlas, fg, outline }
}

// -------------------------------------------------------------------------------------------
// The two arms, each asserted at the pixel, on its own.
// -------------------------------------------------------------------------------------------

/// **Arm 1 of 2 — `0x7000`.** The outline pass of a font that *has* a background surface is one
/// draw per glyph, and every pixel of it is the background sheet's own byte at the dilated
/// source rectangle.
#[test]
fn the_background_sheet_outline_is_pixel_identical_to_its_source_bitmap() {
    require_device();
    let bg_a8 = background_alpha();
    let (hb, vb) = (i32::try_from(HB).unwrap(), i32::try_from(VB).unwrap());
    let mut measured = 0u32;
    for &fb in FRAMEBUFFERS {
        let mut gpu = warp(fb.0, fb.1);
        let fx = fixture(&mut gpu, HB, VB, true);
        for &pen in PENS {
            let want = expect_outline(&bg_a8, pen, hb, vb, fb);
            let covered = want.iter().filter(|a| **a != 0).count();
            let fg_covered = expect_foreground(&foreground_alpha(), pen, (0, 0), fb)
                .iter()
                .filter(|a| **a != 0)
                .count();
            assert!(
                covered > fg_covered,
                "the fixture's outline must cover more than its foreground: {covered} vs {fg_covered}"
            );
            for sampler in [SAMPLER_LINEAR, SAMPLER_POINT] {
                let shot = draw(
                    &mut gpu,
                    &fx.atlas,
                    fx.fg,
                    fx.outline,
                    Pass::OutlineSheet,
                    pen,
                    sampler,
                    fb,
                    0.0,
                );
                let (differ, worst, soft) = compare(&shot.red, &want);
                assert_eq!(
                    (differ, worst, soft),
                    (0, 0, 0),
                    "{}x{} pen {pen:?} sampler {sampler}: {differ} of {covered} covered outline \
                     pixels differ from the background sheet (worst {worst}), {soft} carry \
                     intermediate coverage where the source has none at cut-off {CUT}",
                    fb.0,
                    fb.1
                );
                measured += 1;
            }
        }
    }
    assert_eq!(
        measured as usize,
        FRAMEBUFFERS.len() * PENS.len() * 2,
        "every configuration must have been measured"
    );
    eprintln!("the 0x7000 outline arm is exact in {measured} configurations");
}

/// Behaviour: ui.text.an-outlined-label-draws-eight-offset-glyphs-behind-the-foreground
/// **Arm 2 of 2 — `0x9000`.** A font with **no** background surface outlines by drawing its
/// *foreground* sheet eight times over the 3x3 neighbourhood, and each of the eight lands exactly
/// where its offset says.
///
/// Asserted per offset rather than on the union: the union of eight overlapping draws is a blob
/// that a wrong offset, a duplicated offset or a missing one could all still produce. Each of the
/// eight is drawn into a frame of its own and compared with the foreground bitmap displaced by
/// that offset, and then the count is asserted so a ninth or a seventh would show.
#[test]
fn the_neighbourhood_outline_draws_eight_glyphs_each_exactly_where_its_offset_says() {
    require_device();
    let fg_a8 = foreground_alpha();
    const EXPECTED_NEIGHBOURHOOD: [(i32, i32); 8] = [
        (-1, -1),
        (0, -1),
        (1, -1),
        (-1, 0),
        (1, 0),
        (-1, 1),
        (0, 1),
        (1, 1),
    ];
    assert_eq!(
        OUTLINE_NEIGHBOURHOOD, EXPECTED_NEIGHBOURHOOD,
        "the eight offsets of the 0x9000 arm, in the client's own dy-outer/dx-inner order with the centre skipped"
    );
    assert_eq!(
        OUTLINE_NEIGHBOURHOOD.len(),
        8,
        "the 3x3 neighbourhood less its centre"
    );
    assert!(
        !OUTLINE_NEIGHBOURHOOD.contains(&(0, 0)),
        "the centre is the foreground pass's own draw and the client skips it"
    );
    // Every offset is within the 3x3 box, and no two are equal -- properties the literal above
    // already implies, asserted because they are what "3x3 neighbourhood" *means* and a future
    // edit to both the constant and the literal would otherwise pass unexamined.
    for (dx, dy) in OUTLINE_NEIGHBOURHOOD {
        assert!(
            (-1..=1).contains(&dx) && (-1..=1).contains(&dy),
            "({dx},{dy}) is outside 3x3"
        );
    }
    let mut sorted = OUTLINE_NEIGHBOURHOOD;
    sorted.sort_unstable();
    let mut distinct = sorted.to_vec();
    distinct.dedup();
    assert_eq!(
        distinct.len(),
        8,
        "the eight offsets must be distinct: {sorted:?}"
    );
    let mut measured = 0u32;
    for &fb in FRAMEBUFFERS {
        let mut gpu = warp(fb.0, fb.1);
        // A font with no background sheet, and — as all 12 such shipped fonts have — no borders.
        let fx = fixture(&mut gpu, 0, 0, false);
        assert!(
            !fx.atlas.has_outline_sheet(),
            "this arm's precondition is that the font has no background surface"
        );
        for &pen in PENS {
            for (dx, dy) in OUTLINE_NEIGHBOURHOOD {
                // One offset at a time, through the ordinary foreground draw the client itself
                // uses for this arm: flags `0x9000` differ from `0x1000` only in the colour
                // source, and the geometry is `draw_ui_glyph`'s.
                let want = expect_foreground(&fg_a8, pen, (dx, dy), fb);
                let mut batch = TextBatch::new();
                batch.begin();
                let mut x = pen.0;
                for ch in TEXT.encode_utf16() {
                    assert!(batch.draw_ui_glyph(
                        &fx.atlas,
                        ch,
                        x + dx,
                        pen.1 + dy,
                        RED,
                        (0, 0, fb.0 as i32 - 1, fb.1 as i32 - 1),
                        fb
                    ));
                    x += advance(ch);
                }
                let (verts, _) = batch.end();
                let per_frame = PerFrameConstants {
                    view_proj: identity16(),
                    view: identity16(),
                    fog_params: [0.0; 4],
                    fog_color: [0.0; 4],
                    ambient: [0.0; 4],
                    screen: [0.0; 4],
                };
                gpu.begin_frame().expect("begin_frame");
                gpu.bind_texture(fx.fg, SAMPLER_POINT);
                gpu.draw_dynamic(
                    &TextBatch::new().pipeline_key(),
                    &DrawConstants::default(),
                    &per_frame,
                    &PerDrawConstants::identity(),
                    &pack(&verts),
                )
                .expect("draw");
                gpu.end_frame().expect("end_frame");
                let cap = gpu.capture().expect("capture");
                let red: Vec<u8> = cap.bgra.as_chunks::<4>().0.iter().map(|p| p[2]).collect();
                let (differ, worst, soft) = compare(&red, &want);
                assert_eq!(
                    (differ, worst, soft),
                    (0, 0, 0),
                    "{}x{} pen {pen:?} offset ({dx},{dy}): {differ} pixels differ from the \
                     foreground bitmap displaced by that offset (worst {worst}, {soft} soft)",
                    fb.0,
                    fb.1
                );
                measured += 1;
            }
            // And the batch itself makes exactly eight draws per glyph, in the client's order.
            let mut batch = TextBatch::new();
            batch.begin();
            assert_eq!(
                batch.draw_ui_outline_neighbourhood(
                    &fx.atlas,
                    u16::from(b'A'),
                    pen.0,
                    pen.1,
                    RED,
                    (0, 0, fb.0 as i32 - 1, fb.1 as i32 - 1),
                    fb
                ),
                8,
                "eight draws, one per neighbour"
            );
            let (verts, _) = batch.end();
            assert_eq!(
                verts.len(),
                8 * 6,
                "six vertices per draw, eight draws, unindexed"
            );
        }
    }
    assert_eq!(
        measured as usize,
        FRAMEBUFFERS.len() * PENS.len() * 8,
        "every framebuffer, pen and offset must have been measured"
    );
    eprintln!("the 0x9000 outline arm is exact in {measured} offset configurations");
}

/// Behaviour: ui.text.an-outlined-label-draws-eight-offset-glyphs-behind-the-foreground
/// The foreground pass is unmoved by the outline sheet.
#[test]
fn the_foreground_pass_is_unmoved_by_the_outline_sheet() {
    require_device();
    let fg_a8 = foreground_alpha();
    let mut measured = 0u32;
    for &fb in FRAMEBUFFERS {
        let mut gpu = warp(fb.0, fb.1);
        let fx = fixture(&mut gpu, HB, VB, true);
        for &pen in PENS {
            let want = expect_foreground(&fg_a8, pen, (0, 0), fb);
            for sampler in [SAMPLER_LINEAR, SAMPLER_POINT] {
                let shot = draw(
                    &mut gpu,
                    &fx.atlas,
                    fx.fg,
                    fx.outline,
                    Pass::Foreground,
                    pen,
                    sampler,
                    fb,
                    0.0,
                );
                assert_eq!(
                    compare(&shot.red, &want),
                    (0, 0, 0),
                    "{}x{} pen {pen:?} sampler {sampler}: the foreground pass moved when the \
                     atlas gained an outline sheet",
                    fb.0,
                    fb.1
                );
                measured += 1;
            }
        }
    }
    assert_eq!(measured as usize, FRAMEBUFFERS.len() * PENS.len() * 2);
    eprintln!("the foreground pass is unmoved in {measured} configurations");
}

/// Behaviour: ui.text.an-outlined-label-draws-eight-offset-glyphs-behind-the-foreground
/// **The two passes together, and in the right order.** The outline goes *behind*.
///
/// Asserted without any dependence on blend rounding: with the outline drawn in blue and the
/// foreground in red over it,
///
/// * wherever the foreground is **fully opaque**, the blue channel must be exactly **0** — the
///   outline is completely covered. Were the passes submitted the other way round, every one of
///   those pixels would still be blue, so this is the ordering assertion;
/// * wherever the foreground has **no ink at all** but the outline does, the blue channel must be
///   exactly the outline's own source byte — the halo that extends past the glyph, which is the
///   thing the row says is missing.
///
/// Both are exact equalities on pixels chosen so no partial blend occurs, and both denominators
/// are asserted non-trivial so neither can pass vacuously.
#[test]
fn the_outline_is_drawn_behind_the_foreground() {
    require_device();
    let fg_a8 = foreground_alpha();
    let bg_a8 = background_alpha();
    let (hb, vb) = (i32::try_from(HB).unwrap(), i32::try_from(VB).unwrap());
    let mut cases = 0u32;
    for &fb in FRAMEBUFFERS {
        let mut gpu = warp(fb.0, fb.1);
        let fx = fixture(&mut gpu, HB, VB, true);
        for &pen in PENS {
            let fg = expect_foreground(&fg_a8, pen, (0, 0), fb);
            let ol = expect_outline(&bg_a8, pen, hb, vb, fb);
            let shot = draw(
                &mut gpu,
                &fx.atlas,
                fx.fg,
                fx.outline,
                Pass::Both,
                pen,
                SAMPLER_POINT,
                fb,
                0.0,
            );

            let (mut covered, mut halo) = (0u64, 0u64);
            let (mut covered_bad, mut halo_bad) = (0u64, 0u64);
            for i in 0..fg.len() {
                if fg[i] == 255 && ol[i] != 0 {
                    covered += 1;
                    if shot.blue[i] != 0 {
                        covered_bad += 1;
                    }
                } else if fg[i] == 0 && ol[i] != 0 {
                    halo += 1;
                    if shot.blue[i] != ol[i] {
                        halo_bad += 1;
                    }
                }
            }
            assert!(
                covered > 100 && halo > 100,
                "{}x{} pen {pen:?}: both denominators must be non-trivial, got covered={covered} \
                 halo={halo}",
                fb.0,
                fb.1
            );
            assert_eq!(
                (covered_bad, halo_bad),
                (0, 0),
                "{}x{} pen {pen:?}: {covered_bad} of {covered} pixels under fully-opaque \
                 foreground still show the outline (the passes are in the wrong order), and \
                 {halo_bad} of {halo} halo pixels do not carry the background sheet's own byte",
                fb.0,
                fb.1
            );
            // The halo is the point: it must be a large fraction of the glyph's own ink.
            let ink = fg.iter().filter(|a| **a != 0).count() as u64;
            assert!(
                halo > ink,
                "{}x{} pen {pen:?}: the outline's halo ({halo} px) must exceed the foreground's \
                 own ink ({ink} px) — that is what 'dark backing' means",
                fb.0,
                fb.1
            );
            eprintln!(
                "{}x{} pen {pen:?}: foreground ink {ink} px, outline halo beyond it {halo} px \
                 ({:.2}x), {covered} px of outline hidden under opaque foreground",
                fb.0,
                fb.1,
                halo as f64 / ink as f64
            );
            cases += 1;
        }
    }
    assert_eq!(cases as usize, FRAMEBUFFERS.len() * PENS.len());
}

#[test]
fn the_destination_rectangle_dilates_by_the_field_text_says() {
    require_device();
    assert_ne!(
        HB, VB,
        "the fixture must be able to tell the two border fields apart"
    );
    let (hb, vb) = (i32::try_from(HB).unwrap(), i32::try_from(VB).unwrap());
    let mut cases = 0u32;
    for &fb in FRAMEBUFFERS {
        let mut gpu = warp(fb.0, fb.1);
        let fx = fixture(&mut gpu, HB, VB, true);
        for &pen in PENS {
            // Where the outline's ink must be, from `GLYPHS` and the pen alone.
            let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
            let mut x = pen.0;
            for ch in TEXT.encode_utf16() {
                let d = GLYPHS.iter().find(|d| d.ch == ch).expect("declared");
                let left = x + i32::from(d.before);
                let top = pen.1 + i32::from(d.v_before);
                x0 = x0.min(left - hb);
                y0 = y0.min(top - vb);
                x1 = x1.max(left + i32::from(d.w) + hb - 1);
                // The quirk: `hb`, not `vb`.
                y1 = y1.max(top + i32::from(d.h) + hb - 1);
                x += advance(ch);
            }
            let shot = draw(
                &mut gpu,
                &fx.atlas,
                fx.fg,
                fx.outline,
                Pass::OutlineSheet,
                pen,
                SAMPLER_POINT,
                fb,
                0.0,
            );
            let mut got = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
            for y in 0..fb.1 as i32 {
                for x in 0..fb.0 as i32 {
                    if shot.red[y as usize * fb.0 as usize + x as usize] != 0 {
                        got.0 = got.0.min(x);
                        got.1 = got.1.min(y);
                        got.2 = got.2.max(x);
                        got.3 = got.3.max(y);
                    }
                }
            }
            assert_eq!(
                got,
                (x0, y0, x1, y1),
                "{}x{} pen {pen:?}: the outline's bounding box is not the dilated destination \
                 rectangle hb={hb} vb={vb} names (the bottom edge dilates by hb, not vb)",
                fb.0,
                fb.1
            );
            cases += 1;
        }
    }
    assert_eq!(cases as usize, FRAMEBUFFERS.len() * PENS.len());
    eprintln!("the anisotropic dilation is pinned in {cases} configurations");
}

/// **The calibration, in both directions, for the outline pass specifically.**
///
/// §7.8: the zeros above are worth nothing until this instrument has produced a non-zero on
/// something known to be non-zero. §7.14: a difference is worth nothing until it has produced a
/// zero on something known to be zero — which the tests above are.
///
/// It is also aimed **through** the fixture, following the test-design rule to *"mutate your calibrations
/// too"*: a calibration whose subject never moves would pass against any renderer. Half a pixel
/// under `LINEAR` must blur the outline; the same displacement under `POINT` must be invisible,
/// because the sample point stays inside its own texel; a whole pixel must move both, which is
/// what stops the `POINT` zeros being the instrument being deaf.
#[test]
fn a_displaced_outline_reddens_this_instrument() {
    require_device();
    let bg_a8 = background_alpha();
    let (hb, vb) = (i32::try_from(HB).unwrap(), i32::try_from(VB).unwrap());
    let mut cases = 0u32;
    for &fb in FRAMEBUFFERS {
        let mut gpu = warp(fb.0, fb.1);
        let fx = fixture(&mut gpu, HB, VB, true);
        for &pen in PENS {
            let want = expect_outline(&bg_a8, pen, hb, vb, fb);
            let shot = |g: &mut Gpu, s: u32, d: f32| {
                let sh = draw(
                    g,
                    &fx.atlas,
                    fx.fg,
                    fx.outline,
                    Pass::OutlineSheet,
                    pen,
                    s,
                    fb,
                    d,
                );
                compare(&sh.red, &want)
            };
            let half_linear = shot(&mut gpu, SAMPLER_LINEAR, 0.5);
            assert!(
                half_linear.0 > 200,
                "{}x{} pen {pen:?}: half a pixel under LINEAR must redden the outline differ, \
                 got {half_linear:?}",
                fb.0,
                fb.1
            );
            let half_point = shot(&mut gpu, SAMPLER_POINT, 0.5);
            // The same texel-boundary tie o274 records: a half-pixel displacement puts the POINT
            // sample exactly on a boundary, which the Vulkan specification leaves to the
            // implementation (software rasterisers take the near side; hardware lands a ULP either
            // way). Asserted where it is defined -- on a software device -- and reported elsewhere.
            // (Restructure step 2.0: the xplat branch guarded o274 and not this instrument.)
            if gpu.adapter_kind() != dereth_render::device::AdapterKind::Software {
                eprintln!(
                    "{}x{} pen {pen:?} POINT at half a pixel on hardware: {half_point:?} (tie, not asserted)",
                    fb.0, fb.1
                );
            } else {
                assert_eq!(
                    half_point,
                    (0, 0, 0),
                    "{}x{} pen {pen:?}: POINT cannot see half a pixel",
                    fb.0,
                    fb.1
                );
            }
            let whole_linear = shot(&mut gpu, SAMPLER_LINEAR, 1.0);
            let whole_point = shot(&mut gpu, SAMPLER_POINT, 1.0);
            assert!(
                whole_linear.0 > 200 && whole_point.0 > 200,
                "{}x{} pen {pen:?}: a whole pixel must move both samplers, got {whole_linear:?} \
                 and {whole_point:?}",
                fb.0,
                fb.1
            );
            eprintln!(
                "{}x{} pen {pen:?}: outline displaced HALF a pixel reads {} differing / {} \
                 intermediate under LINEAR and {half_point:?} under POINT; a WHOLE pixel reads {} \
                 under LINEAR and {} under POINT",
                fb.0, fb.1, half_linear.0, half_linear.2, whole_linear.0, whole_point.0
            );
            cases += 1;
        }
    }
    assert_eq!(cases as usize, FRAMEBUFFERS.len() * PENS.len());
    eprintln!("the outline differ is calibrated both ways at {cases} pairs");
}

/// The atlas carries the two border counts.
#[test]
fn the_atlas_carries_the_two_border_counts() {
    let f = font(HB, VB, true);
    let fg = bgra(&foreground_alpha());
    let bg = bgra(&background_alpha());
    let atlas = FontAtlas::from_glyph_sheets(
        &f,
        GlyphSheet {
            width: SHEET_W,
            height: SHEET_H,
            bgra: &fg,
        },
        Some(GlyphSheet {
            width: SHEET_W,
            height: SHEET_H,
            bgra: &bg,
        }),
    )
    .expect("atlas");
    assert_eq!(
        atlas.num_horizontal_border_pixels, HB,
        "horizontal border pixels at +0x40"
    );
    assert_eq!(
        atlas.num_vertical_border_pixels, VB,
        "vertical border pixels at +0x44"
    );
    assert!(atlas.has_outline_sheet());
    // An outline sheet of the wrong size is refused rather than silently mis-sampled: the two
    // sheets are addressed by the same `(offset_x, offset_y)`.
    let small = bgra(&vec![0u8; ((SHEET_W - 1) * SHEET_H) as usize]);
    assert!(
        FontAtlas::from_glyph_sheets(
            &f,
            GlyphSheet {
                width: SHEET_W,
                height: SHEET_H,
                bgra: &fg
            },
            Some(GlyphSheet {
                width: SHEET_W - 1,
                height: SHEET_H,
                bgra: &small
            }),
        )
        .is_err(),
        "a mismatched outline sheet must be refused"
    );
    let plain = FontAtlas::from_glyph_sheet(
        &f,
        GlyphSheet {
            width: SHEET_W,
            height: SHEET_H,
            bgra: &fg,
        },
    )
    .expect("atlas");
    assert!(!plain.has_outline_sheet());
    assert_eq!(
        plain.num_horizontal_border_pixels, HB,
        "the counts are carried either way"
    );
}

/// The fixture's own precondition, asserted rather than assumed: **no two glyphs' dilated
/// destination rectangles overlap**, at either pen.
///
/// Without this the per-pixel oracles above silently become approximations — overlapping quads
/// composite under the text material's `SRCALPHA`/`INVSRCALPHA` blend where the expectation folds
/// with `max` — and they would fail with a confusing "N pixels differ" rather than saying why.
/// It is the same hazard the test-design constraints describe as a fixture that tests the one case where
/// the bug cancels, in the opposite direction: here the tidy case is the *honest* one and this
/// assertion is what keeps it tidy.
#[test]
fn the_fixture_rectangles_do_not_overlap() {
    let hb = i32::try_from(HB).expect("small");
    for &pen in PENS {
        let mut spans: Vec<(i32, i32)> = Vec::new();
        let mut x = pen.0;
        for ch in TEXT.encode_utf16() {
            let d = GLYPHS.iter().find(|d| d.ch == ch).expect("declared");
            let left = x + i32::from(d.before) - hb;
            let right = x + i32::from(d.before) + i32::from(d.w) + hb - 1;
            spans.push((left, right));
            x += advance(ch);
        }
        for w in spans.windows(2) {
            assert!(
                w[0].1 < w[1].0,
                "pen {pen:?}: dilated rectangles {:?} and {:?} overlap, so the per-pixel oracle \
                 in this file is no longer exact -- widen the right-hand bearings in `GLYPHS`",
                w[0],
                w[1]
            );
        }
    }
}

use crate::common::text_pixels::{bgra, identity16, pack, require_device, warp};
