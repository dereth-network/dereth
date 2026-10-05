//! Outlined text submits the outline pass on a real frame: the char-gen screen issues outline draw
//! calls with geometry through `Renderer::draw_ui`, takes the background-sheet (`0x7000`) arm, and
//! a hand-built command drives both glyph-outline arms, one per font kind. Fixture: a headless
//! `App` on the char-gen wizard with the retail dats, counted at the renderer's UI statistics.
//! The geometry at the pixel and the attribute reaching a `UiDrawCmd` are covered elsewhere; a
//! draw list is not a draw call, so this counts what leaves the renderer.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::client_dir;
use crate::common::gpu_lock;

use dereth_client::app::App;
use dereth_client_runtime::config::Config;
use dereth_ui::framework::mode;

fn app_on_wizard() -> App {
    let d = client_dir();
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are this file's oracle: none at {} -- set DERETH_TEST_DAT_DIR",
        d.display()
    );
    let cfg = Config {
        headless: true,
        sound: false,
        ui: true,
        width: 800,
        height: 600,
        dat_dir: d,
        ..Config::default()
    };
    let mut app = App::new(cfg).expect("the application comes up");
    app.start_shell().expect("the UI comes up");
    app.queue_ui_mode(mode::CHAR_GEN);
    for _ in 0..8 {
        app.frame();
    }
    app
}

/// Behaviour: ui.text.outlined-text-submits-the-outline-pass-on-a-real-frame
///
/// **The outline pass is submitted, on the real screen, for the elements that ask.**
///
/// Every number here is a denominator for the next: fonts with an outline sheet uploaded, outline
/// draw calls issued, outline quads inside them. A zero anywhere means the pass is not reached,
/// and each is asserted separately so the failure message says *which* link broke.
#[test]
fn a_real_frame_submits_the_outline_pass() {
    let _gpu = gpu_lock();
    let mut app = app_on_wizard();

    let before = app.renderer_mut().ui_stats;
    app.frame();
    let now = app.renderer_mut().ui_stats;

    let outline_draws = now.outline_draws - before.outline_draws;
    let outline_quads = now.outline_glyphs_drawn - before.outline_glyphs_drawn;
    let outline_skipped = now.outline_glyphs_skipped - before.outline_glyphs_skipped;
    let glyph_draws = now.glyph_draws - before.glyph_draws;
    let glyph_quads = now.glyphs_drawn - before.glyphs_drawn;

    eprintln!(
        "one char-gen frame issued {outline_draws} outline draw(s) carrying \
         {outline_quads} outline quad(s), against {glyph_draws} foreground draw(s) carrying \
         {glyph_quads} glyph quad(s); {} font(s) uploaded an outline texture; \
         {outline_skipped} outline glyph(s) skipped; font_failures {}",
        now.font_outlines_baked, now.font_failures
    );

    assert_eq!(
        now.font_failures, 0,
        "a font this screen names would not rasterise"
    );
    assert!(
        now.font_outlines_baked > 0,
        "no font on this screen uploaded an outline texture, so the outline pass had nothing to \
         sample and every later count would be zero for the wrong reason"
    );
    assert!(
        glyph_draws > 0,
        "this frame drew no text at all, so it can say nothing about the outline pass"
    );
    assert!(
        outline_draws > 0,
        "the outline pass was not submitted: {glyph_draws} foreground glyph draw(s) and \
         {outline_draws} outline draw(s); `TextBits::outline` must reach the renderer"
    );
    assert!(
        outline_quads > 0,
        "{outline_draws} outline draw call(s) submitted no geometry at all"
    );
    // The outline pass never runs alone: it precedes a foreground pass over the same run, so
    // there can never be more outline draws than foreground draws.
    assert!(
        outline_draws <= glyph_draws,
        "{outline_draws} outline draw(s) against {glyph_draws} foreground draw(s) -- the outline \
         pass is the first of two over the same run and cannot outnumber the second"
    );
    app.shutdown();
}

/// **The char-gen screen takes the `0x7000` arm**, and the quad count says so arithmetically.
///
/// The sheet arm draws **one** dilated quad per glyph and the neighbourhood arm **eight**, so the
/// ratio of outline quads to glyph quads identifies the arm without trusting the counter that
/// names it.
///
/// `Renderer::prepare_ui_fonts` *moves* the outline sheet's pixels out of the atlas to upload
/// them as a texture, so the arm is selected from `FontAtlas::has_outline_sheet`, a fact that
/// survives `take_outline_pixels`; selecting from the pixels after the move would make every font
/// look sheetless and send every outline down the eight-draw arm.
#[test]
fn the_char_gen_screen_takes_the_background_sheet_arm() {
    let _gpu = gpu_lock();
    let mut app = app_on_wizard();

    let before = app.renderer_mut().ui_stats;
    app.frame();
    let now = app.renderer_mut().ui_stats;
    let sheet = now.outline_draws_sheet - before.outline_draws_sheet;
    let neighbourhood = now.outline_draws_neighbourhood - before.outline_draws_neighbourhood;
    let outline_draws = now.outline_draws - before.outline_draws;
    let outline_quads = now.outline_glyphs_drawn - before.outline_glyphs_drawn;
    let glyph_quads = now.glyphs_drawn - before.glyphs_drawn;

    eprintln!(
        "one char-gen frame took the 0x7000 arm {sheet} time(s) and the 0x9000 arm \
         {neighbourhood} time(s), for {outline_quads} outline quad(s) over {glyph_quads} glyph \
         quad(s)"
    );
    assert_eq!(
        sheet + neighbourhood,
        outline_draws,
        "every outline draw took exactly one arm"
    );
    assert!(sheet > 0, "the screen must exercise the arm it selects");
    assert_eq!(
        neighbourhood, 0,
        "every outlined element on this screen names a font with a background sheet, so none may \
         take the eight-draw arm; if that ever changes this assertion is where to notice it"
    );
    // One quad per glyph is the sheet arm's arithmetic signature, and it is an inequality rather
    // than an equality for a faithful reason: the dilated destination rectangle is 4 px wider on
    // every side for this font, so a glyph against the edge of its element's visible box has its
    // outline clipped away entirely. Rectangle-pair construction clamps the pair against the
    // destination window and glyph drawing emits nothing when the result is empty, so the
    // client loses the same ones.
    assert!(
        outline_quads <= glyph_quads,
        "{outline_quads} outline quad(s) for {glyph_quads} glyph quad(s): the 0x7000 arm draws \
         one per glyph, so more than one apiece means the eight-draw arm was taken somewhere"
    );
    let clipped = glyph_quads - outline_quads;
    #[allow(clippy::cast_precision_loss)]
    let pct = 100.0 * clipped as f64 / glyph_quads as f64;
    eprintln!(
        "{outline_quads} of {glyph_quads} glyphs got an outline quad; {clipped} \
         ({pct:.1}%) sit against the edge of their element's box and have their dilated rectangle \
         clipped away, as the client clips them"
    );
    assert!(
        pct < 25.0,
        "{pct:.1}% of the outline quads were clipped away -- too many to be edge effects, which \
         points at the clip box rather than at the dilation"
    );
    app.shutdown();
}

/// **The `0x7000` arm, driven through the real renderer with the strip's own font.**
///
/// Both glyph-drawing arms are exercised by a font of each kind. The shipped char-gen screen
/// supplies only one of them, so the other is driven here with a `UiDrawCmd` built by hand,
/// through the same `Renderer::prepare_ui` / `Renderer::draw_ui` a frame runs, with **font
/// `0x40000001`**: the font inherited by the red diagnostic strip. It has a background sheet
/// (`0x06005EE8`) and four border pixels on each side.
///
/// Both arms are then asserted from one harness in one run: same code, two fonts, two arms, and
/// the quad arithmetic that tells them apart.
#[test]
fn the_background_sheet_arm_is_driven_by_the_strips_own_font() {
    let _gpu = gpu_lock();
    let mut app = app_on_wizard();
    let store = dereth_dat::RetailDatStore::open_dir(&client_dir()).expect("the retail dats open");

    /// `0x40000001`: the spew strip's font, and one of the 37 that carry a background sheet.
    const SHEET_FONT: dereth_primitives::DataId = dereth_primitives::DataId(0x4000_0001);
    /// `0x40000017`: one of the 12 with no background sheet and no border pixels.
    const NO_SHEET_FONT: dereth_primitives::DataId = dereth_primitives::DataId(0x4000_0017);

    let run = |app: &mut App, font: dereth_primitives::DataId| -> (u64, u64, u64) {
        let glyphs: Vec<dereth_ui::text::PlacedGlyph> = "ABC"
            .encode_utf16()
            .enumerate()
            .map(|(i, ch)| dereth_ui::text::PlacedGlyph {
                x: 40 + 20 * i32::try_from(i).expect("small"),
                y: 40,
                ch,
                color: 0xFFFF_0000,
                font,
            })
            .collect();
        let cmd = dereth_ui::UiDrawCmd {
            who: dereth_ui::ElemHandle::for_test(0),
            screen: dereth_ui::region::Box2D::new(0, 0, 799, 599),
            clip: dereth_ui::region::Box2D::new(0, 0, 799, 599),
            image: None,
            image_op: None,
            image_source: dereth_ui::ImageSource::Interface,
            blit_mode: dereth_ui::BlitMode::default(),
            alpha_blend_mod: 1.0,
            tiling_offset: (0, 0),
            rotation_z_degrees: 0,
            color: 0xFFFF_FFFF,
            glyphs,
            // The element asks for an outline in opaque black: the outline colour's own default
            // and the colour the shipped strip resolves to.
            text_outline: Some(0xFF00_0000),
            invert: Vec::new(),
            fills: Vec::new(),
        };
        let before = app.renderer_mut().ui_stats;
        let r = app.renderer_mut();
        // `prepare_ui` outside the frame bracket, as a real frame does: a texture upload runs a
        // command list of its own.
        r.prepare_ui(&store, std::slice::from_ref(&cmd));
        r.start_frame().expect("start_frame");
        r.draw_ui(std::slice::from_ref(&cmd)).expect("draw_ui");
        r.end_frame().expect("end_frame");
        let now = app.renderer_mut().ui_stats;
        (
            now.outline_draws_sheet - before.outline_draws_sheet,
            now.outline_draws_neighbourhood - before.outline_draws_neighbourhood,
            now.outline_glyphs_drawn - before.outline_glyphs_drawn,
        )
    };

    // The 0x7000 arm: one dilated quad per glyph, out of the font's background sheet.
    let (sheet, neighbourhood, quads) = run(&mut app, SHEET_FONT);
    assert_eq!(
        (sheet, neighbourhood),
        (1, 0),
        "font {SHEET_FONT:?} has a background sheet and must take the 0x7000 arm"
    );
    assert_eq!(
        quads, 3,
        "one outline quad per glyph for the three glyphs drawn, not eight"
    );

    // The 0x9000 arm: eight quads per glyph, out of the foreground sheet.
    let (sheet2, neighbourhood2, quads2) = run(&mut app, NO_SHEET_FONT);
    assert_eq!(
        (sheet2, neighbourhood2),
        (0, 1),
        "font {NO_SHEET_FONT:?} has no background sheet and must take the 0x9000 arm"
    );
    assert_eq!(quads2, 24, "eight outline quads per glyph for three glyphs");

    // And the two arms are genuinely different work, from the same code, in the same run --
    // which is what stops either measurement being a property of the harness.
    assert_eq!(
        quads2,
        8 * quads,
        "the neighbourhood arm draws eight times what the sheet arm does"
    );
    assert_eq!(
        now_font_failures(&mut app),
        0,
        "neither font failed to rasterise"
    );
    eprintln!(
        "through the real renderer, {SHEET_FONT:?} takes the 0x7000 arm ({quads} quad(s) \
         for 3 glyphs) and {NO_SHEET_FONT:?} takes the 0x9000 arm ({quads2} quad(s) for 3)"
    );
    app.shutdown();
}

fn now_font_failures(app: &mut App) -> u64 {
    app.renderer_mut().ui_stats.font_failures
}
