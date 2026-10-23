//! The Horizon interface's pixels, on every renderer this build has, on a hardware device: text is
//! its font's own glyphs, texel for texel on whole pixels; at 100% a window's frame is its own art
//! texel for texel; a picture drawn at another size than its own lands on the same pixels with the
//! same colours wherever it stands within a pixel, made at that size with the art's own filter;
//! and every window's title bar is one measure at every scale.
//!
//! Fixtures: the interface's own art and fonts as the client carries them, drawn through the
//! client's presentation off screen.
//!
//! Behaviour: none (experimental Horizon interface)

#![cfg(gpu)]

use std::sync::Arc;

use dereth_client_runtime::present::Presentation;
use dereth_client_shell::gpu::Renderer;
use dereth_horizon::art::{Art, Family};
use dereth_horizon::draw::{whole, DrawList, Overlay, Rect, WHITE};
use dereth_horizon::ui::colours::Colours;
use dereth_horizon::ui::input::InputFrame;
use dereth_horizon::ui::kit::{self, Ctx, WindowState, TITLE_BAR};
use dereth_horizon::ui::paint::{Painter, TextStyle};
use dereth_render::device::{AdapterKind, Backend};

const W: u32 = 1280;
const H: u32 = 720;
#[allow(clippy::cast_precision_loss)]
const SCREEN: (f32, f32) = (W as f32, H as f32);

const BLACK: u32 = 0xFF00_0000;

/// A renderer on `backend`, off screen, on a real GPU.
fn renderer(backend: Backend) -> Renderer {
    let r = Renderer::new_on(backend, None, W, H)
        .unwrap_or_else(|e| panic!("{backend:?}: these frames need a device and none opened: {e}"));
    assert_eq!(
        r.gpu.adapter_kind(),
        AdapterKind::Hardware,
        "{backend:?}: {} is a software rasteriser",
        r.gpu.adapter_name()
    );
    r
}

/// The interface's own art, as the client carries it.
fn art() -> Arc<Art> {
    Arc::new(Art::new(Arc::new(
        dereth_horizon::pieces::Pieces::built_in().expect("the pieces built in"),
    )))
}

/// `list` drawn through the presentation, as BGRA. It is composed a few times first, as the
/// frames before it would be, so a picture drawn at another size than its own is made.
fn drawn(r: &mut Renderer, art: &Art, list: &DrawList) -> Vec<u8> {
    let mut overlay = Overlay::default();
    for _ in 0..3 {
        overlay.compose(r, art, list, false, &[]);
    }
    Presentation::start_frame(r).expect("the frame starts");
    overlay.draw(r).expect("the interface is drawn");
    Presentation::end_frame(r).expect("the frame ends");
    let (w, h, bgra) = r.capture_bgra().expect("the frame is read back");
    assert_eq!((w, h), (W, H));
    overlay.release(r);
    bgra
}

/// The painter over `list` at `scale`, the whole screen filled with `ground` first.
fn painter<'a>(list: &'a mut DrawList, art: &'a Art, scale: f32, ground: u32) -> Painter<'a> {
    let mut p = Painter {
        list,
        art,
        scale,
        screen: SCREEN,
        fade: 1.0,
    };
    p.fill(Rect::new(0.0, 0.0, SCREEN.0, SCREEN.1), ground);
    p
}

/// The BGRA pixel at `(x, y)`.
fn px(bgra: &[u8], x: u32, y: u32) -> [u8; 4] {
    let i = ((y * W + x) * 4) as usize;
    [bgra[i], bgra[i + 1], bgra[i + 2], bgra[i + 3]]
}

/// A window's frame drawn at `scale`, its top-left at `at`, `size` layout units, over `ground`.
fn window(
    r: &mut Renderer,
    art: &Art,
    scale: f32,
    at: (f32, f32),
    size: (f32, f32),
    ground: u32,
) -> Vec<u8> {
    let mut list = DrawList::default();
    let mut p = painter(&mut list, art, scale, ground);
    let colours = Colours::default();
    let mut input = InputFrame {
        mouse: (-50.0, -50.0),
        ..InputFrame::default()
    };
    let mut drag = None;
    let mut ctx = Ctx {
        time: 10.0,
        dt: 1.0 / 60.0,
        input: &mut input,
        colours: &colours,
        hot: false,
        drag: &mut drag,
        drops: Vec::new(),
    };
    let mut state = WindowState {
        open: true,
        pos: Some(at),
        ..WindowState::default()
    };
    kit::window(&mut p, &mut ctx, &mut state, "Inventory", size, at);
    drawn(r, art, &list)
}

const TEXT: &str = "Aurelia Vale says, \"Tithes are up, 1,250!\"";

/// Text at 100%, 125% and 150% is its font's glyphs, each texel on one pixel: the frame's pixels
/// are the glyphs' own coverage laid on whole pixels from where the line starts, pen step by pen
/// step, and nothing smeared between them.
#[test]
fn text_is_its_glyphs_texel_for_texel_on_whole_pixels() {
    let _gpu = crate::common::gpu_lock();
    let art = art();
    for backend in Backend::compiled() {
        let mut r = renderer(backend);
        for scale in [1.0, 1.25, 1.5] {
            let style = TextStyle::new(Family::Body, 13.0, WHITE);
            let (x, y) = (40.3, 60.6);
            let mut list = DrawList::default();
            let mut p = painter(&mut list, &art, scale, BLACK);
            p.text(&style, x, y, TEXT);
            let bgra = drawn(&mut r, &art, &list);
            // The glyphs laid out from the font's own table: white over black, by coverage.
            let face = art.font(Family::Body, 13.0 * scale).expect("the body face");
            let mut want = vec![0.0_f32; (W * H) as usize];
            let mut pen = x;
            let mut prev = None;
            let mut drawn_px = 0;
            for ch in TEXT.chars() {
                let g = *face.font.glyph(ch).expect("the glyph");
                if let Some(p) = prev {
                    #[allow(clippy::cast_precision_loss)]
                    let kern = face.font.kern(p, ch) as f32;
                    pen += kern;
                }
                let page = art
                    .image(art.font_page(&face, g.page).expect("the page"))
                    .expect("its pixels");
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let (gx, gy) = (whole(pen) as u32, whole(y + f32::from(g.y_offset)) as u32);
                for v in 0..u32::from(g.h) {
                    for u in 0..u32::from(g.w) {
                        let a = f32::from(page.pixel(u32::from(g.x) + u, u32::from(g.y) + v)[3])
                            / 255.0;
                        let i = ((gy + v) * W + gx + u) as usize;
                        want[i] = a * 255.0 + (1.0 - a) * want[i];
                    }
                }
                #[allow(clippy::cast_precision_loss)]
                let adv = g.advance() as f32;
                pen += adv;
                prev = Some(ch);
            }
            let mut worst = (0.0_f32, (0, 0));
            for yy in 0..H {
                for xx in 0..W {
                    let got = f32::from(px(&bgra, xx, yy)[2]);
                    let w = want[(yy * W + xx) as usize];
                    if w > 0.0 {
                        drawn_px += 1;
                    }
                    if (got - w).abs() > worst.0 {
                        worst = ((got - w).abs(), (xx, yy));
                    }
                }
            }
            assert!(drawn_px > 300, "{backend:?} at {scale}: the text is drawn");
            assert!(
                worst.0 <= 2.0,
                "{backend:?} at {scale}: the text differs from its glyphs by {} at {:?}",
                worst.0,
                worst.1
            );
        }
    }
}

/// At 100% a window's frame is its own art, texel for texel: every opaque texel of the frame's
/// top-left corner is the very pixel at its place.
#[test]
fn at_100_percent_the_window_frame_is_its_own_art_texel_for_texel() {
    let _gpu = crate::common::gpu_lock();
    let art = art();
    let (x0, y0) = (40.0, 30.0);
    let frame = kit::frame_scale(&Painter {
        list: &mut DrawList::default(),
        art: &art,
        scale: 1.0,
        screen: SCREEN,
        fade: 1.0,
    });
    let corner = art.piece("window.tl", frame).expect("the corner piece");
    let image = art.image(corner.tex).expect("its atlas");
    for backend in Backend::compiled() {
        let mut r = renderer(backend);
        let bgra = window(&mut r, &art, 1.0, (x0, y0), (400.0, 240.0), BLACK);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (sx, sy, w, h) = (
            corner.src.x as u32,
            corner.src.y as u32,
            corner.src.w as u32,
            corner.src.h as u32,
        );
        let mut opaque = 0;
        for v in 0..h {
            for u in 0..w {
                let texel = image.pixel(sx + u, sy + v);
                if texel[3] != 255 {
                    continue;
                }
                opaque += 1;
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let got = px(&bgra, x0 as u32 + u, y0 as u32 + v);
                for c in 0..3 {
                    assert!(
                        got[c].abs_diff(texel[c]) <= 1,
                        "{backend:?}: the corner's texel ({u}, {v}) is {:?}, drawn {:?}",
                        &texel[..3],
                        &got[..3]
                    );
                }
            }
        }
        assert!(opaque > 300, "{backend:?}: the corner's opaque texels");
    }
}

/// Every window's title bar is one measure, [`TITLE_BAR`] layout units, at every scale: the
/// opaque band along the top of the window, which the ground under a window does not show
/// through, reaches that many pixels down at the scale (give or take the pixel a part-scale
/// rounds to).
#[test]
fn every_window_s_title_bar_is_one_measure_at_every_scale() {
    let _gpu = crate::common::gpu_lock();
    let art = art();
    let (x0, y0) = (40.0, 30.0);
    for backend in Backend::compiled() {
        let mut r = renderer(backend);
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let size = (300.0, 200.0);
            let green = window(&mut r, &art, scale, (x0, y0), size, 0xFF00_FF00);
            let red = window(&mut r, &art, scale, (x0, y0), size, 0xFFFF_0000);
            // Down a column a third of the way along, clear of the title and the corners.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let column = (x0 + size.0 * scale / 3.0) as u32;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let top = y0 as u32;
            // How much of the ground shows through a row: none where the bar is opaque.
            let through = |y: u32| {
                let (g, r) = (px(&green, column, y), px(&red, column, y));
                (0..3).map(|c| g[c].abs_diff(r[c])).max().unwrap_or(0)
            };
            let first = (top..top + 3)
                .find(|y| through(*y) <= 12)
                .unwrap_or_else(|| panic!("{backend:?} at {scale}: no bar at the top"));
            let last = (first..H)
                .take_while(|y| through(*y) <= 12)
                .last()
                .unwrap_or(first);
            #[allow(clippy::cast_precision_loss)]
            let bar = (last + 1 - top) as f32;
            let want = whole(TITLE_BAR * scale);
            assert!(
                (bar - want).abs() <= 1.0,
                "{backend:?} at {scale}: the title bar is {bar} pixels tall, not {want}"
            );
        }
    }
}

/// A picture drawn at another size than its own (an icon's slot at 150%) is made at that size with
/// the art's own filter and drawn texel for texel: wherever its edges fall within the pixels they
/// stand nearest, it lands on those pixels with the same colours, and they are the copy's.
#[test]
fn a_picture_drawn_at_another_size_is_the_same_wherever_its_edges_fall_within_a_pixel() {
    let _gpu = crate::common::gpu_lock();
    let art = art();
    let scale = 1.5;
    let slot = art.piece("slot.empty", scale).expect("the slot");
    let (w, h) = (slot.w * scale, slot.h * scale);
    assert!(
        (w - slot.src.w).abs() > 1.0,
        "the slot is drawn at another size than its texels"
    );
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let size = (whole(w) as u32, whole(h) as u32);
    let copy = dereth_horizon::resample::resample(
        &art.image(slot.tex).expect("its atlas"),
        slot.src,
        size,
    );
    for backend in Backend::compiled() {
        let mut r = renderer(backend);
        // Its left edge and its width each a little off the whole pixel, both ways.
        let frames: Vec<Vec<u8>> = [(100.0, 0.0), (100.3, -0.1), (99.8, 0.4)]
            .iter()
            .map(|(x, wider)| {
                let mut list = DrawList::default();
                let mut p = painter(&mut list, &art, scale, BLACK);
                p.sprite(&slot, Rect::new(*x, 80.4, w + wider, h), WHITE);
                drawn(&mut r, &art, &list)
            })
            .collect();
        assert!(
            frames.iter().all(|f| *f == frames[0]),
            "{backend:?}: the same pixels wherever its edges fall"
        );
        for v in 0..size.1 {
            for u in 0..size.0 {
                let texel = copy.pixel(u, v);
                let got = px(&frames[0], 100 + u, 80 + v);
                for c in 0..3 {
                    let want = u32::from(texel[c]) * u32::from(texel[3]) / 255;
                    assert!(
                        u32::from(got[c]).abs_diff(want) <= 2,
                        "{backend:?}: ({u}, {v}) is {:?}, the copy's {:?}",
                        &got[..3],
                        texel
                    );
                }
            }
        }
    }
}
