//! UI blit address mode and rotation: the sampler descriptor is filter plus address; the shipped
//! layouts author no tiling and no rotation; a tiled element repeats its picture at every seam and
//! a tiling offset scrolls the repeat; the bind census moves the address bit and leaves `LINEAR`
//! unbound; and a rotated element binds `LINEAR` and turns counter-clockwise. Fixture: the retail
//! dats (layouts and pictures) and a headless `App` on the char-gen, character-management and
//! gameplay screens, with hand-built `UiDrawCmd`s driven through `Renderer::draw_ui`.
//!
//! # A: the address mode
//!
//! UI-surface material generation sets `TEXADDRESS_WRAP` for a tiling element:
//! `(tile == false) * 2 + 1`, followed by a per-axis refinement against `2.0e-4` on the tiling
//! branch. **But nothing turns the tile flag on.** It is written in exactly one place, through the
//! second argument to surface setup, and UI-object creation computes that argument as "the
//! element's attribute `0xCD` equals 2". Attribute `0xCD` is read in three places and written in
//! none, so its only source is the layout dat, and **no shipped element authors `2`**:
//! [`the_shipped_layouts_author_no_tiling_element`] counts 67 authored sites over 101 layouts and
//! 2 162 elements, 58 carrying `3` and 9 carrying `0`.
//!
//! What a player sees tiling is a **different mechanism**: region drawing hands the picture to
//! graphic drawing, which walks a modulo grid of image blits into the element's own CPU surface
//! whenever
//!
//! ```text
//! !(tiling_offset == (0,0) && box_width <= picture_width && box_height <= picture_height)
//! ```
//!
//! and the blit path takes `min(src, dst)` on both extents, so either arm of that blit **never
//! scales**. This client has no CPU surface to repeat into, so the repeat is UVs past 1 under a
//! `WRAP` sampler, and that is what makes the address mode load-bearing here. `ui_draw::quad`
//! divides the UVs by the **picture's** extent, so a 10 x 5 border strip is laid down 79.2 times
//! across a 792-pixel element rather than stretched; and the surface size handed to
//! `ui_surface_sampler` is the element's own, because UI-object creation makes each element's
//! surface at the element's own width and height, so a tiled blit binds `POINT`, one texel to one
//! pixel. Resizing sets the physical size before the virtual screen position, so a resize updates
//! both size and position.
//!
//! # B: rotation, pinned rather than produced
//!
//! No region or element rotation setter exists, `MasterProperty 0x39000001` has no rotation
//! attribute, and no construction path the client uses reaches the surface rotation setter. That
//! is a **structural** null, not a coincidence of the shipped layouts: no data could turn it on,
//! which is why the answer is "pin it" rather than "give it a producer". `UiDrawCmd` carries the
//! field anyway so the conditional the renderer evaluates is the client's whole conditional and
//! the arm can be *driven* rather than only reasoned about:
//! [`a_rotated_element_binds_linear_where_its_unrotated_twin_binds_point`] drives it through the
//! same `Renderer::draw_ui` a frame uses and reads `Gpu::sampler_binds` back.
//!
//! # What is asserted against what
//!
//! The pixel oracle is the **source bitmap's own bytes**, decoded from the retail dat and tiled by
//! hand in this file, never a golden image and never the renderer's own output. Every zero is
//! bracketed: the same instrument is required to report a large non-zero for the stretched
//! reading, so "0 differing" cannot be the differ failing to look.

#![cfg(gpu)]

use crate::common::client_dir;
use crate::common::gpu_lock;

use std::collections::BTreeMap;

use dereth_client::app::App;
use dereth_client_runtime::config::Config;
use dereth_primitives::{AssetSource, DataId};
use dereth_render::ui::pixel_rules::{
    graphic_draw_tiles, ui_surface_address_mode, ui_surface_sampler, TEXADDRESS_CLAMP,
    TEXADDRESS_WRAP, UI_SURFACE_SAMPLER_SCALED, UI_SURFACE_SAMPLER_TILED,
    UI_SURFACE_SAMPLER_TILED_SCALED, UI_SURFACE_SAMPLER_UNSCALED,
};
use dereth_ui::framework::mode;

fn store() -> dereth_dat::RetailDatStore {
    crate::common::dat_store()
}

fn app_on(m: dereth_ui::UiMode) -> App {
    let cfg = Config {
        headless: true,
        sound: false,
        ui: true,
        width: 800,
        height: 600,
        dat_dir: client_dir(),
        ..Config::default()
    };
    let mut app = App::new(cfg).expect("the application comes up");
    app.start_shell().expect("the UI comes up");
    app.queue_ui_mode(m);
    for _ in 0..8 {
        app.frame();
    }
    app
}

// -------------------------------------------------------------------------------------------
// Retail's values, pinned as literals
// -------------------------------------------------------------------------------------------

/// **`create_samplers`' index is two bits, and material generation supplies both.**
///
/// These independent literals keep every other reader from reaching the values through the same
/// symbol and overlooking a wrong symbol. The **polarity of the epsilon comparison** is the
/// load-bearing half here: read it the other way up and every axis's address mode inverts, and
/// the result still looks like "the address mode depends on the repeat count".
#[test]
fn the_four_sampler_descriptors_are_filter_plus_address_and_both_bits_are_read() {
    assert_eq!(UI_SURFACE_SAMPLER_UNSCALED, 3, "POINT + CLAMP");
    assert_eq!(UI_SURFACE_SAMPLER_SCALED, 1, "LINEAR + CLAMP");
    assert_eq!(UI_SURFACE_SAMPLER_TILED, 2, "POINT + WRAP");
    assert_eq!(UI_SURFACE_SAMPLER_TILED_SCALED, 0, "LINEAR + WRAP");
    assert_eq!(TEXADDRESS_WRAP, 1, "the wrap address enum value");
    assert_eq!(TEXADDRESS_CLAMP, 3, "the clamp address enum value");

    // The whole two-by-two, one cell at a time, so no pair can silently collapse.
    let s = (64u32, 32u32);
    assert_eq!(
        ui_surface_sampler(s, s, false, false),
        3,
        "natural, unrotated, untiled"
    );
    assert_eq!(
        ui_surface_sampler(s, s, false, true),
        2,
        "natural, unrotated, tiled"
    );
    assert_eq!(
        ui_surface_sampler(s, s, true, false),
        1,
        "rotated -> LINEAR, untiled"
    );
    assert_eq!(
        ui_surface_sampler(s, s, true, true),
        0,
        "rotated -> LINEAR, tiled"
    );
    assert_eq!(
        ui_surface_sampler((128, 32), s, false, false),
        1,
        "surface stretched -> LINEAR"
    );
    let mut seen: Vec<u32> = vec![
        ui_surface_sampler(s, s, false, false),
        ui_surface_sampler(s, s, false, true),
        ui_surface_sampler(s, s, true, false),
        ui_surface_sampler(s, s, true, true),
    ];
    seen.sort_unstable();
    assert_eq!(
        seen,
        vec![0, 1, 2, 3],
        "the four inputs must reach four distinct descriptors"
    );

    // Material generation's per-axis refinement. A false tile flag is CLAMP outright;
    // on the tiling branch each axis is WRAP only when its own repeat count leaves 1 by more than
    // 2.0e-4.
    assert_eq!(
        ui_surface_address_mode((64, 32), (64, 32), false),
        (TEXADDRESS_CLAMP, TEXADDRESS_CLAMP),
        "tile flag false -> `(false == false) * 2 + 1` = 3 on both axes"
    );
    assert_eq!(
        ui_surface_address_mode((64, 32), (64, 32), true),
        (TEXADDRESS_CLAMP, TEXADDRESS_CLAMP),
        "a tiling element whose surface is its own size repeats once, so the refinement puts \
         both axes back to CLAMP; inverting the comparison would give WRAP"
    );
    assert_eq!(
        ui_surface_address_mode((792, 5), (10, 5), true),
        (TEXADDRESS_WRAP, TEXADDRESS_CLAMP),
        "the shipped border strip: U repeats 79.2 times, V exactly once"
    );
    assert_eq!(
        ui_surface_address_mode((5, 592), (5, 10), true),
        (TEXADDRESS_CLAMP, TEXADDRESS_WRAP),
        "and its vertical twin, the other way round"
    );

    // The graphic-drawing fast-path predicate, negated one term at a time.
    assert!(
        !graphic_draw_tiles((10, 5), (10, 5), (0, 0)),
        "exact coverage selects the single-blit path"
    );
    assert!(
        !graphic_draw_tiles((10, 5), (4, 2), (0, 0)),
        "a larger picture selects the single-blit crop path"
    );
    assert!(
        graphic_draw_tiles((10, 5), (11, 5), (0, 0)),
        "one pixel too wide -> the grid"
    );
    assert!(
        graphic_draw_tiles((10, 5), (10, 6), (0, 0)),
        "one pixel too tall -> the grid"
    );
    assert!(
        graphic_draw_tiles((10, 5), (10, 5), (1, 0)),
        "a tiling offset takes the grid even when the picture covers the box: the offset is the \
         first term of the fast path's `&&`"
    );
}

// -------------------------------------------------------------------------------------------
// A: the shipped counts
// -------------------------------------------------------------------------------------------

/// Every authored `0xCD` in the shipped layouts, so the tile-flag arm's reachability is a
/// measurement rather than a claim.
fn attribute_cd_census() -> (BTreeMap<u32, usize>, usize, usize) {
    let store = store();
    let mid = DataId(0x3900_0001);
    let bytes = AssetSource::read(&store, mid).expect("MasterProperty 0x39000001");
    let master =
        <dereth_assets::MasterProperty as dereth_assets::Decode>::decode_payload(mid, &bytes)
            .expect("the property type table");
    let types = master.property_types();

    fn walk(d: &dereth_ui::desc::ElementDesc, hist: &mut BTreeMap<u32, usize>) {
        if let Some(v) = d.base.properties.get_enum(0xCD) {
            *hist.entry(v).or_default() += 1;
        }
        for s in d.states.values() {
            if let Some(v) = s.properties.get_enum(0xCD) {
                *hist.entry(v).or_default() += 1;
            }
        }
        for c in d.children.values() {
            walk(c, hist);
        }
    }

    let mut hist = BTreeMap::new();
    let (mut layouts, mut elements) = (0usize, 0usize);
    for id in 0x2100_0000u32..=0x2100_0075 {
        let Ok(l) = dereth_ui::desc::LayoutDesc::load(&store, DataId(id), &types) else {
            continue;
        };
        layouts += 1;
        elements += l.element_count();
        for e in l.elements.values() {
            walk(e, &mut hist);
        }
    }
    (hist, layouts, elements)
}

/// **The shipped count of tile-flag elements: zero, and the denominators that make that a
/// measurement.**
///
/// UI-object creation reads attribute `0xCD`, keeps the value only when it is `1` or `2`, and
/// passes `mode == 2` to surface setup as the tile flag. The other two consumers, the
/// current-mode query and the ownership-mode update, also only read it. So the population is the
/// layout dat's, and it contains no `2`.
///
/// A zero with no denominator is not a measurement, so the layout and element counts are asserted
/// too, so a decoder that returned nothing would redden here rather than report a clean zero.
#[test]
fn the_shipped_layouts_author_no_tiling_element() {
    let (hist, layouts, elements) = attribute_cd_census();
    let total: usize = hist.values().sum();
    eprintln!(
        "tiling: attribute 0xCD over {layouts} layouts / {elements} elements: {total} authored \
         site(s), {hist:?}"
    );
    assert_eq!(
        layouts, 101,
        "the shipped layout range is 0x21000000..=0x21000075"
    );
    assert_eq!(elements, 2162, "and its element count");
    assert!(
        total >= 60,
        "a reader that found no attributes at all would score a false zero"
    );
    assert_eq!(
        hist.get(&2).copied().unwrap_or(0),
        0,
        "no shipped element sets the tile flag: {hist:?}"
    );
    assert_eq!(
        hist.get(&1).copied().unwrap_or(0),
        0,
        "and none sizes from the state desc either"
    );
    assert_eq!(
        hist.get(&3).copied().unwrap_or(0),
        58,
        "58 sites take the element's own size"
    );
    assert_eq!(
        hist.get(&0).copied().unwrap_or(0),
        9,
        "and 9 leave the mode at 0"
    );
}

/// **The count that does matter: how many shipped blits graphic drawing repeats.**
///
/// Both arms of the predicate must appear, on real screens, or the address mode is being asserted
/// over a population of one kind.
#[test]
fn the_shipped_screens_exercise_both_arms_of_graphic_draw() {
    let _gpu = gpu_lock();
    let store = store();
    let tex = dereth_scene::textures::TextureStore::new(&store);
    let mut totals = (0usize, 0usize, 0usize);
    for (name, m) in [
        ("char-gen", mode::CHAR_GEN),
        ("character-management", mode::CHARACTER_MANAGEMENT),
        ("gameplay", mode::GAME_PLAY),
    ] {
        let mut app = app_on(m);
        app.frame();
        let (mut blits, mut tiled, mut offset) = (0usize, 0usize, 0usize);
        for c in app.ui_draw_list() {
            let Some(id) = c.image else { continue };
            let Ok(d) = tex.texture_data(id) else {
                continue;
            };
            blits += 1;
            let box_ = (
                u32::try_from(c.screen.x1 - c.screen.x0 + 1).unwrap_or(0),
                u32::try_from(c.screen.y1 - c.screen.y0 + 1).unwrap_or(0),
            );
            if graphic_draw_tiles((d.width, d.height), box_, c.tiling_offset) {
                tiled += 1;
            }
            if c.tiling_offset != (0, 0) {
                offset += 1;
            }
        }
        eprintln!(
            "tiling: {name}: {blits} image blit(s), {tiled} of them repeated by the \
             picture-grid rule, {offset} carrying a live tiling offset"
        );
        assert!(
            blits > 0,
            "{name} drew no picture at all, so it measures nothing"
        );
        totals.0 += blits;
        totals.1 += tiled;
        totals.2 += offset;
        app.shutdown();
    }
    eprintln!(
        "tiling: {} image blit(s) over three screens, {} tiled, {} with a tiling offset",
        totals.0, totals.1, totals.2
    );
    assert!(
        totals.1 > 0,
        "no shipped blit tiles, so the WRAP arm has no exerciser at all"
    );
    assert!(
        totals.0 - totals.1 > 0,
        "every shipped blit tiles, so the CLAMP arm has no exerciser -- a population of one kind \
         cannot show a conditional"
    );
}

// -------------------------------------------------------------------------------------------
// A: the pixels
// -------------------------------------------------------------------------------------------

/// **How much of this is visible in the shipped data, split into the half that moves pixels and
/// the half that cannot.**
///
/// A repeat and a stretch produce identical pixels when the picture is uniform along the axis
/// being repeated, and most of the client's tiled art is exactly that: plain border strips whose
/// variation runs across the strip, not along it.
///
/// Some do not. Both halves are asserted non-zero, because a partition with an empty side is
/// the failure mode this file is otherwise built to avoid.
#[test]
fn some_shipped_repeats_move_pixels_and_the_rest_are_uniform_along_their_axis() {
    let _gpu = gpu_lock();
    let store = store();
    let tex = dereth_scene::textures::TextureStore::new(&store);
    let mut tiled = 0usize;
    let mut moves: Vec<String> = Vec::new();
    let mut pictures: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    for m in [mode::CHAR_GEN, mode::CHARACTER_MANAGEMENT, mode::GAME_PLAY] {
        let mut app = app_on(m);
        app.frame();
        for c in app.ui_draw_list() {
            let Some(id) = c.image else { continue };
            let Ok(d) = tex.texture_data(id) else {
                continue;
            };
            let box_ = (
                u32::try_from(c.screen.x1 - c.screen.x0 + 1).unwrap_or(0),
                u32::try_from(c.screen.y1 - c.screen.y0 + 1).unwrap_or(0),
            );
            if !graphic_draw_tiles((d.width, d.height), box_, c.tiling_offset) {
                continue;
            }
            tiled += 1;
            pictures.insert(id.0);
            let (w, h) = (d.width as usize, d.height as usize);
            let px = &d.levels[0];
            let at = |x: usize, y: usize| &px[(y * w + x) * 4..][..4];
            let varies_x = (0..h).any(|y| (1..w).any(|x| at(x, y) != at(0, y)));
            let varies_y = (0..w).any(|x| (1..h).any(|y| at(x, y) != at(x, 0)));
            // Which axes this placement actually repeats along. A tiling offset repeats an axis
            // even when the picture covers the box, because it is the first term of the
            // graphic-drawing fast-path `&&`.
            let repeats_x = box_.0 > d.width || c.tiling_offset.0 != 0;
            let repeats_y = box_.1 > d.height || c.tiling_offset.1 != 0;
            if (repeats_x && varies_x) || (repeats_y && varies_y) {
                moves.push(format!(
                    "{id:?} {}x{} in {}x{} off {:?}",
                    d.width, d.height, box_.0, box_.1, c.tiling_offset
                ));
            }
        }
        app.shutdown();
    }
    eprintln!(
        "tiling: {tiled} tiled blit(s) over three screens, {} distinct picture(s); {} of them \
         would move a pixel, {} are uniform along the axis they repeat",
        pictures.len(),
        moves.len(),
        tiled - moves.len()
    );
    for v in &moves {
        eprintln!("   moves: {v}");
    }
    assert!(tiled > 0, "no tiled blit at all, so this measures nothing");
    assert!(
        pictures.len() >= 4,
        "too few distinct pictures to call this a sweep: {pictures:?}"
    );
    assert!(
        !moves.is_empty(),
        "not one shipped repeat moves a pixel, so the whole change is unobservable outside a \
         synthetic placement and this file's pixel test is its only evidence"
    );
    assert!(
        moves.len() < tiled,
        "every shipped repeat moves a pixel, so none is uniform along its axis: {} of \
         {tiled}",
        moves.len()
    );
}

/// A shipped 5 x 10 border strip whose **columns differ**: `0x21000005`'s left window
/// edge, one of the pictures the live gameplay screen tiles.
///
/// It is drawn *horizontally* by this file, which is not how the shipped screen places it, and
/// that is deliberate: most pictures the shipped screens repeat are uniform along the axis they
/// repeat along, so they cannot show a seam. The oracle is still
/// the retail bytes; only the placement is this file's, because a placement that cannot
/// distinguish a repeat from a stretch measures nothing.
const STRIP: DataId = DataId(0x0600_612B);

/// The frame the **source bitmap** says a `w x h` element at `(x, y)` showing `image` should
/// carry, built by hand from the decoded texels with picture-grid drawing's modulo, never from
/// the renderer's output.
///
/// `wrap` false is the stretched reading: the picture stretched to the element's extent,
/// nearest-texel. It is here so the differ is required to *separate* the two, which is what stops
/// a zero being the instrument failing to look.
fn expected(
    src: &dereth_primitives::TextureData,
    rect: (i32, i32, i32, i32),
    offset: (i32, i32),
    fb: (u32, u32),
    wrap: bool,
) -> Vec<[u8; 4]> {
    let (x, y, w, h) = rect;
    let mut out = vec![[0u8; 4]; (fb.0 * fb.1) as usize];
    let px = &src.levels[0];
    let (iw, ih) = (src.width as i32, src.height as i32);
    for r in 0..h {
        for c in 0..w {
            let (dx, dy) = (x + c, y + r);
            if dx < 0 || dy < 0 || dx >= fb.0 as i32 || dy >= fb.1 as i32 {
                continue;
            }
            let (sx, sy) = if wrap {
                ((c + offset.0).rem_euclid(iw), (r + offset.1).rem_euclid(ih))
            } else {
                ((c * iw / w).min(iw - 1), (r * ih / h).min(ih - 1))
            };
            let o = ((sy * iw + sx) * 4) as usize;
            out[(dy * fb.0 as i32 + dx) as usize] = [px[o], px[o + 1], px[o + 2], px[o + 3]];
        }
    }
    out
}

/// Compare a captured frame with an expectation over one rectangle, on RGB only (the back buffer
/// is opaque; the picture's alpha rides in the blend, not in the read-back byte).
fn differing(cap: &[u8], want: &[[u8; 4]], fb: (u32, u32), r: (i32, i32, i32, i32)) -> usize {
    let mut n = 0;
    for y in r.1..r.1 + r.3 {
        for x in r.0..r.0 + r.2 {
            let i = (y * fb.0 as i32 + x) as usize;
            let got = [cap[i * 4], cap[i * 4 + 1], cap[i * 4 + 2]];
            let exp = [want[i][0], want[i][1], want[i][2]];
            if got != exp {
                n += 1;
            }
        }
    }
    n
}

/// Behaviour: ui.draw.a-tiled-element-repeats-its-picture-at-every-seam
///
/// **The seam: a tiled element repeats its picture, and the repeat is the client's `mod`.**
///
/// One element, three widths, all drawn through the same `Renderer::draw_ui` a frame uses:
///
/// 1. at the picture's own 10 x 5: the calibration positive. Picture-grid drawing takes its
///    single image blit here, one texel to one pixel, and the frame must equal the source bitmap
///    exactly. A differ that cannot produce a zero on this cannot be believed anywhere else.
/// 2. at 80 x 5: eight whole repeats. Every seam at x = 10, 20, ... must show texel 0 again.
/// 3. at 77 x 5: seven repeats and a **partial** eighth, because a whole number of copies is the
///    one case where wrapping and clamping could still be confused with a fencepost.
///
/// Each is measured twice: against the tiled expectation *and* against the stretched one. The
/// tiled reading must be 0 and the stretched reading must be large, so the instrument is shown to
/// discriminate rather than merely to agree.
#[test]
fn a_tiled_element_repeats_its_picture_at_every_seam() {
    let _gpu = gpu_lock();
    let store = store();
    let tex = dereth_scene::textures::TextureStore::new(&store);
    let src = tex
        .texture_data(STRIP)
        .expect("the shipped border strip decodes");
    assert_eq!(
        src.format,
        dereth_primitives::TextureFormat::Bgra8,
        "this file reads the source texels directly, so it needs them uncompressed"
    );
    assert_eq!((src.width, src.height), (5, 10), "the strip is 5 x 10");
    // **The premise, and it is the whole reason this file lays a vertical strip out
    // horizontally.** A picture that is uniform along the axis being repeated makes a repeat and
    // a stretch produce identical pixels, so a differ over it reports 0 either way and proves
    // nothing: a differential is blind to the subject being absent from both arms. Require the
    // columns to differ before believing anything below.
    let col = |x: usize| -> Vec<u8> {
        (0..src.height as usize)
            .flat_map(|y| src.levels[0][(y * src.width as usize + x) * 4..][..4].to_vec())
            .collect()
    };
    let distinct_cols: std::collections::BTreeSet<Vec<u8>> =
        (0..src.width as usize).map(col).collect();
    assert!(
        distinct_cols.len() > 1,
        "this picture is uniform along x, so tiling it horizontally cannot be told from \
         stretching it: {} distinct column(s)",
        distinct_cols.len()
    );

    let mut app = app_on(mode::GAME_PLAY);
    let fb = (800u32, 600u32);
    let (x, y) = (100i32, 100i32);

    let mut shot = |w: i32, h: i32, offset: (i32, i32)| -> Vec<u8> {
        let cmd = dereth_ui::UiDrawCmd {
            who: dereth_ui::ElemHandle::for_test(0),
            screen: dereth_ui::region::Box2D::new(x, y, x + w - 1, y + h - 1),
            clip: dereth_ui::region::Box2D::new(0, 0, 799, 599),
            image: Some(STRIP),
            image_op: None,
            image_source: dereth_ui::ImageSource::Interface,
            blit_mode: dereth_ui::BlitMode::default(),
            alpha_blend_mod: 1.0,
            tiling_offset: offset,
            rotation_z_degrees: 0,
            color: 0xFFFF_FFFF,
            glyphs: Vec::new(),
            text_outline: None,
            invert: Vec::new(),
            fills: Vec::new(),
        };
        let r = app.renderer_mut();
        r.prepare_ui(&store, std::slice::from_ref(&cmd));
        r.start_frame().expect("start_frame");
        r.draw_ui(std::slice::from_ref(&cmd)).expect("draw_ui");
        r.end_frame().expect("end_frame");
        let (cw, ch, bgra) = app.renderer_mut().capture_bgra().expect("capture");
        assert_eq!(
            (cw, ch),
            fb,
            "the capture must be the back buffer this test placed against"
        );
        bgra
    };

    for (w, h, note) in [
        (5i32, 10i32, "one copy"),
        (25, 10, "five whole"),
        (23, 10, "four and a bit"),
    ] {
        let cap = shot(w, h, (0, 0));
        let rect = (x, y, w, h);
        let tiled = expected(&src, rect, (0, 0), fb, true);
        let stretched = expected(&src, rect, (0, 0), fb, false);
        let d_tiled = differing(&cap, &tiled, fb, rect);
        let d_stretched = differing(&cap, &stretched, fb, rect);
        eprintln!(
            "tiling: {w}x{h} ({note}): {d_tiled} px differ from the tiled source, {d_stretched} \
             from the stretched reading, of {} covered",
            w * h
        );
        assert_eq!(
            d_tiled, 0,
            "{w}x{h} must be the source bitmap repeated: {d_tiled} px differ"
        );
        if w > src.width as i32 {
            assert!(
                d_stretched > 0,
                "{w}x{h}: the differ cannot tell a repeat from a stretch, so its zero above says \
                 nothing"
            );
        }
    }

    // The seam itself, named rather than folded into a count: the first column of the second
    // copy must be the first column of the picture, and it must differ from the last column of
    // the first copy -- otherwise "the seam wrapped" is satisfied by a picture whose two edges
    // happen to be equal.
    let cap = shot(25, 10, (0, 0));
    let at = |cx: i32, cy: i32| -> [u8; 3] {
        let i = ((y + cy) * fb.0 as i32 + x + cx) as usize;
        [cap[i * 4], cap[i * 4 + 1], cap[i * 4 + 2]]
    };
    let texel = |tx: i32, ty: i32| -> [u8; 3] {
        let o = ((ty * src.width as i32 + tx) * 4) as usize;
        [src.levels[0][o], src.levels[0][o + 1], src.levels[0][o + 2]]
    };
    // The columns a CLAMP reading could not have produced: this picture's first and last columns
    // happen to agree on every row, so "the seam restarted" alone would be satisfied by a
    // continuation. What separates the two readings is the *interior* of each repeat -- a
    // clamping sampler shows the picture's last column for every pixel past `u = 1`, so any
    // column whose texel differs from column 4 is a column only a wrap can produce.
    let discriminating: Vec<(i32, i32)> = (0..5)
        .flat_map(|c| (0..10).map(move |row| (c, row)))
        .filter(|&(c, row)| texel(c, row) != texel(4, row))
        .collect();
    assert!(
        discriminating.len() >= 10,
        "too few texels differ from this picture's last column for a wrap and a clamp to be \
         distinguishable here: {} of 50",
        discriminating.len()
    );
    let mut seams_checked = 0;
    for copy in 1..5 {
        let sx = copy * 5;
        for &(c, row) in &discriminating {
            assert_eq!(
                at(sx + c, row),
                texel(c, row),
                "copy {copy} column {c} row {row} must be the picture's own texel"
            );
            assert_ne!(
                at(sx + c, row),
                texel(4, row),
                "and must not be the last column smeared, which is what CLAMP gives past u = 1"
            );
        }
        seams_checked += 1;
    }
    assert_eq!(seams_checked, 4, "four interior seams in five copies");
    eprintln!(
        "tiling: 4 seam(s) checked over {} texel position(s) a clamping sampler could not produce",
        discriminating.len()
    );

    app.shutdown();
}

/// **The tiling offset scrolls the repeat rather than dragging an edge row.**
///
/// Picture-grid drawing starts at `(tiling_offset.x - box.x0 + dst.x0) mod picture_width`, so an
/// offset of 3 puts texel 3 at the element's left edge and wraps texels 0..2 round to the right of
/// each copy. Under `CLAMP`, `u` would run from 0.3 to 1.3 and the last 30% of the element would
/// be the picture's final column smeared.
#[test]
fn a_tiling_offset_scrolls_the_repeat_rather_than_smearing_the_last_column() {
    let _gpu = gpu_lock();
    let store = store();
    let tex = dereth_scene::textures::TextureStore::new(&store);
    let src = tex.texture_data(STRIP).expect("the strip decodes");
    let mut app = app_on(mode::GAME_PLAY);
    let fb = (800u32, 600u32);
    let (x, y, w, h) = (100i32, 100i32, 40i32, 10i32);
    let offset = (3, 0);

    let cmd = dereth_ui::UiDrawCmd {
        who: dereth_ui::ElemHandle::for_test(0),
        screen: dereth_ui::region::Box2D::new(x, y, x + w - 1, y + h - 1),
        clip: dereth_ui::region::Box2D::new(0, 0, 799, 599),
        image: Some(STRIP),
        image_op: None,
        image_source: dereth_ui::ImageSource::Interface,
        blit_mode: dereth_ui::BlitMode::default(),
        alpha_blend_mod: 1.0,
        tiling_offset: offset,
        rotation_z_degrees: 0,
        color: 0xFFFF_FFFF,
        glyphs: Vec::new(),
        text_outline: None,
        invert: Vec::new(),
        fills: Vec::new(),
    };
    let r = app.renderer_mut();
    r.prepare_ui(&store, std::slice::from_ref(&cmd));
    r.start_frame().expect("start_frame");
    r.draw_ui(std::slice::from_ref(&cmd)).expect("draw_ui");
    r.end_frame().expect("end_frame");
    let (_, _, cap) = app.renderer_mut().capture_bgra().expect("capture");

    let want = expected(&src, (x, y, w, h), offset, fb, true);
    let d = differing(&cap, &want, fb, (x, y, w, h));
    let unshifted = expected(&src, (x, y, w, h), (0, 0), fb, true);
    let d_unshifted = differing(&cap, &unshifted, fb, (x, y, w, h));
    eprintln!(
        "tiling: {w}x{h} at tiling offset {offset:?}: {d} px differ from the shifted source, \
         {d_unshifted} from the unshifted one, of {} covered",
        w * h
    );
    assert_eq!(
        d, 0,
        "the offset must shift the phase of the repeat: {d} px differ"
    );
    assert!(
        d_unshifted > 0,
        "the offset changed nothing, so this test would pass with tiling-offset handling deleted"
    );
    app.shutdown();
}

// -------------------------------------------------------------------------------------------
// A: the bind census
// -------------------------------------------------------------------------------------------

/// **The bind census on a real frame, with the address bit now moving.**
///
/// Four claims, one per counter, and an equality rather than an inequality so a bind from
/// somewhere unaccounted for reddens instead of hiding:
///
/// * every glyph, outline, fill and untiled blit binds `POINT`/`CLAMP` (3);
/// * every tiled blit binds `POINT`/`WRAP` (2), not `LINEAR`/`CLAMP`;
/// * nothing binds `LINEAR` at all, either arm of it, because neither conjunct has a producer;
/// * the tiled count is non-zero, so `binds[2]` reading 6 is the frame tiling rather than the
///   counter being stuck.
#[test]
fn the_bind_census_moves_the_address_bit_and_leaves_linear_unbound() {
    let _gpu = gpu_lock();
    let mut app = app_on(mode::CHAR_GEN);

    let before = app.renderer_mut().ui_stats;
    app.renderer_mut().clear_sampler_binds();
    app.frame();
    let binds = app.renderer_mut().sampler_binds();
    let now = app.renderer_mut().ui_stats;

    let glyphs = now.glyph_draws - before.glyph_draws;
    let outlines = now.outline_draws - before.outline_draws;
    let unscaled = now.blits_unscaled - before.blits_unscaled;
    let scaled = now.blits_scaled - before.blits_scaled;
    let tiled = now.blits_tiled - before.blits_tiled;
    let fills = now.fills_drawn - before.fills_drawn;
    eprintln!(
        "tiling: one char-gen frame bound samplers {binds:?} (0 linear/wrap, 1 linear/clamp, \
         2 point/wrap, 3 point/clamp) over {glyphs} glyph draw(s), {outlines} outline draw(s), \
         {unscaled} unscaled blit(s), {scaled} scaled blit(s), {tiled} tiled blit(s) and {fills} \
         fill(s)"
    );

    assert!(
        unscaled + scaled > 0,
        "this frame issued no image blit, so it says nothing"
    );
    assert!(
        tiled > 0,
        "no blit in this frame tiles, so the WRAP arm was never exercised and `binds[2] == 0` \
         would be a silence rather than a result"
    );
    assert_eq!(
        scaled, 0,
        "nothing in this client or in retail scales a UI blit: {binds:?}"
    );

    let point_clamp = glyphs + outlines + fills + (unscaled - tiled);
    assert_eq!(
        binds[UI_SURFACE_SAMPLER_UNSCALED as usize],
        point_clamp,
        "POINT/CLAMP binds must be glyphs ({glyphs}) + outlines ({outlines}) + fills ({fills}) + \
         untiled blits ({}) = {point_clamp}; census {binds:?}",
        unscaled - tiled
    );
    // There are eight counters, and the equality is over the three POINT/WRAP descriptors
    // together: a repeat can wrap one axis, the other, or both, and which of the three a given
    // blit takes is a property of that blit's geometry rather than of the frame. The claim is
    // "every tiled blit binds POINT and wraps"; the per-axis split is asserted elsewhere.
    let point_wrap_any = binds[UI_SURFACE_SAMPLER_TILED as usize]
        + binds[dereth_render::ui::pixel_rules::UI_SURFACE_SAMPLER_TILED_U as usize]
        + binds[dereth_render::ui::pixel_rules::UI_SURFACE_SAMPLER_TILED_V as usize];
    assert_eq!(
        point_wrap_any, tiled,
        "the POINT descriptors that wrap at least one axis (2, 6, 7) must be exactly the tiled \
         blits ({tiled}); census {binds:?}"
    );
    for linear in [
        UI_SURFACE_SAMPLER_SCALED,
        UI_SURFACE_SAMPLER_TILED_SCALED,
        4,
        5,
    ] {
        assert_eq!(
            binds[linear as usize], 0,
            "descriptor {linear} is a LINEAR one and LINEAR has no producer: census {binds:?}"
        );
    }
    assert_eq!(
        binds.iter().sum::<u64>(),
        point_clamp + tiled,
        "every bind in the frame must fall in one of the two groups: census {binds:?}"
    );
    app.shutdown();
}

// -------------------------------------------------------------------------------------------
// B: rotation
// -------------------------------------------------------------------------------------------

/// **The rotated arm, driven through the frame's own path, against an unrotated twin.**
///
/// This is the only way the `LINEAR` descriptor is reached at all, since the scaled conjunct has
/// no producer either. The two commands differ in **one field**, so the census difference is
/// attributable.
#[test]
fn a_rotated_element_binds_linear_where_its_unrotated_twin_binds_point() {
    let _gpu = gpu_lock();
    let store = store();
    let mut app = app_on(mode::GAME_PLAY);

    let mut at = |z: i32| -> (
        [u64; dereth_render::ui::pixel_rules::UI_SAMPLER_COUNT],
        u64,
        u64,
    ) {
        let cmd = dereth_ui::UiDrawCmd {
            who: dereth_ui::ElemHandle::for_test(0),
            screen: dereth_ui::region::Box2D::new(200, 200, 204, 209),
            clip: dereth_ui::region::Box2D::new(0, 0, 799, 599),
            image: Some(STRIP),
            image_op: None,
            image_source: dereth_ui::ImageSource::Interface,
            blit_mode: dereth_ui::BlitMode::default(),
            alpha_blend_mod: 1.0,
            tiling_offset: (0, 0),
            rotation_z_degrees: z,
            color: 0xFFFF_FFFF,
            glyphs: Vec::new(),
            text_outline: None,
            invert: Vec::new(),
            fills: Vec::new(),
        };
        let before = app.renderer_mut().ui_stats;
        let r = app.renderer_mut();
        r.prepare_ui(&store, std::slice::from_ref(&cmd));
        r.clear_sampler_binds();
        r.start_frame().expect("start_frame");
        r.draw_ui(std::slice::from_ref(&cmd)).expect("draw_ui");
        r.end_frame().expect("end_frame");
        let now = app.renderer_mut().ui_stats;
        (
            app.renderer_mut().sampler_binds(),
            now.blits_unscaled - before.blits_unscaled,
            now.blits_scaled - before.blits_scaled,
        )
    };

    // The twin: the element at its picture's own size, unrotated. POINT/CLAMP.
    let (binds, unscaled, scaled) = at(0);
    assert_eq!(
        (unscaled, scaled),
        (1, 0),
        "the unrotated twin is unscaled: {binds:?}"
    );
    assert_eq!(
        binds[UI_SURFACE_SAMPLER_UNSCALED as usize], 1,
        "and binds POINT: {binds:?}"
    );
    assert_eq!(
        binds[UI_SURFACE_SAMPLER_SCALED as usize], 0,
        "and not LINEAR: {binds:?}"
    );

    // The same element, same size, same picture, turned 30 degrees. LINEAR/CLAMP.
    let (binds, unscaled, scaled) = at(30);
    assert_eq!(
        (unscaled, scaled),
        (0, 1),
        "a rotated element is on the LINEAR arm: {binds:?}"
    );
    assert_eq!(
        binds[UI_SURFACE_SAMPLER_SCALED as usize], 1,
        "the rotated blit must bind LINEAR: the rotation-matrix identity check selects it on its \
         own: {binds:?}"
    );
    assert_eq!(
        binds[UI_SURFACE_SAMPLER_UNSCALED as usize], 0,
        "and not POINT: {binds:?}"
    );

    // 360 is the identity again: rotation setup reduces modulo 360 before building the
    // matrix, so a full turn leaves the rotation matrix at identity and returns to the POINT arm.
    let (binds, _, scaled) = at(360);
    assert_eq!(
        scaled, 0,
        "360 degrees reduces to 0 modulo 360, so POINT: {binds:?}"
    );
    assert_eq!(binds[UI_SURFACE_SAMPLER_UNSCALED as usize], 1, "{binds:?}");
    app.shutdown();
}

/// **Rotation is pinned, not produced: no shipped element can carry one.**
///
/// The structural argument is in this file's header and in
/// `dereth_render::ui::pixel_rules::ui_surface_sampler`; what is *measurable* from here is its
/// consequence: the whole shipped draw list, over every screen this client can raise, carries
/// `rotation_z_degrees == 0`, because no attribute and no code path sets it.
///
/// Asserted with a denominator, so an empty draw list cannot read as agreement.
#[test]
fn no_shipped_element_carries_a_rotation() {
    let _gpu = gpu_lock();
    let mut total = 0usize;
    for m in [mode::CHAR_GEN, mode::CHARACTER_MANAGEMENT, mode::GAME_PLAY] {
        let mut app = app_on(m);
        app.frame();
        for c in app.ui_draw_list() {
            total += 1;
            assert_eq!(
                c.rotation_z_degrees, 0,
                "element {:?} carries a rotation, which nothing in the client can produce",
                c.who
            );
        }
        app.shutdown();
    }
    eprintln!("tiling: {total} draw command(s) over three screens, all unrotated");
    assert!(
        total > 100,
        "too few commands to call this a sweep: {total}"
    );
}

/// **The rotation geometry, at the two angles that can be checked in closed form.**
///
/// `rotate_clip_quad` must be free at 0 (the same corners the unrotated path emits, bit for bit,
/// because the local offsets are `+-0.5` and the two multipliers are exactly `0.0` and `1.0`), and
/// a quarter turn must permute the corners. The Z-rotation handedness is `_11 = cos`,
/// `_12 = sin`, `_21 = -sin`, `_22 = cos` under the row-vector convention established by
/// translation components `_41` through `_43`.
#[test]
fn the_rotation_is_free_at_zero_and_turns_counter_clockwise() {
    use dereth_render::ui::{rotate_clip_quad, ClipRect};
    // A square in clip units, so a quarter turn is a permutation rather than a shear.
    let r = ClipRect {
        x: -0.5,
        y: -0.5,
        sx: 1.0,
        sy: 1.0,
    };
    let flat = rotate_clip_quad(r, 0);
    assert_eq!(
        flat,
        [(-0.5, 0.5), (-0.5, -0.5), (0.5, -0.5), (0.5, 0.5)],
        "0 degrees must be the unrotated quad exactly"
    );
    assert_eq!(
        rotate_clip_quad(r, 360),
        flat,
        "reducing modulo 360 makes a full turn free too"
    );
    assert_eq!(
        rotate_clip_quad(r, -360),
        flat,
        "and so is a full turn the other way"
    );

    let q = rotate_clip_quad(r, 90);
    let near = |a: (f32, f32), b: (f32, f32)| (a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6;
    // (x, y) -> (x cos - y sin, x sin + y cos) = (-y, x) at 90 degrees.
    assert!(
        near(q[0], (-0.5, -0.5)),
        "top-left goes to bottom-left: {:?}",
        q[0]
    );
    assert!(
        near(q[1], (0.5, -0.5)),
        "bottom-left goes to bottom-right: {:?}",
        q[1]
    );
    assert!(
        near(q[2], (0.5, 0.5)),
        "bottom-right goes to top-right: {:?}",
        q[2]
    );
    assert!(
        near(q[3], (-0.5, 0.5)),
        "top-right goes to top-left: {:?}",
        q[3]
    );
}
