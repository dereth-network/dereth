//! Which sampler each UI draw binds. A glyph draw binds POINT/CLAMP: a text font's own material
//! asks for point filtering and clamped addressing, and the device's four shared samplers are
//! 0 linear/wrap, 1 linear/clamp, 2 point/wrap, **3 point/clamp**, so the font's pair is index 3.
//! An image blit or a flat fill binds what the client's material logic selects, which is a
//! conditional and not a constant: POINT/CLAMP at natural size, LINEAR when the surface is scaled
//! or rotated, and POINT with WRAP on the axes where an element larger than its picture repeats
//! it. No shipped element reaches the LINEAR arm, so the scaled and unscaled cases are asserted
//! separately, and the per-frame bind census is written as an equality that catches a bind from
//! nowhere. A fill samples one white texel under CLAMP, so its sampler cannot move a pixel.
//! Fixture: the character-creation wizard of the shipped layout, whose captions, title and labels
//! draw glyph runs from more than one font, rendered by a headless `App`, with
//! `Gpu::sampler_binds` as the census. The retail dats are an `expect`, never a skip.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::client_dir;
use crate::common::gpu_lock;
use dereth_client::app::App;
use dereth_client_runtime::config::Config;
use dereth_render::ui::pixel_rules::{
    ui_surface_sampler, UI_GLYPH_SAMPLER, UI_SURFACE_SAMPLER_SCALED, UI_SURFACE_SAMPLER_TILED,
    UI_SURFACE_SAMPLER_UNSCALED,
};
use dereth_ui::framework::mode;

fn require_dats() {
    let d = client_dir();
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are this file's oracle: none at {} -- set DERETH_TEST_DAT_DIR",
        d.display()
    );
}

/// The char-gen wizard: it carries captions, a title and several labels, so it draws glyph runs
/// out of more than one font as well as blits and fills.
fn app_on_wizard() -> App {
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
    app.queue_ui_mode(mode::CHAR_GEN);
    for _ in 0..8 {
        app.frame();
    }
    app
}

/// **Retail's transform choice, as a table.** Both branches of the transform conditional, and the
/// two sampler indices they select, pinned as literals.
///
/// The literals are pinned because every other reader goes through the symbol and therefore
/// cannot detect the symbol being wrong. The polarity is the load-bearing half — an
/// implementation that had it backwards would satisfy "the blit binds a sampler chosen by the
/// element's geometry" and be exactly wrong.
#[test]
fn the_conditional_polarity_is_the_one_dot_text_shows_and_is_stated_as_literals() {
    assert_eq!(
        UI_SURFACE_SAMPLER_UNSCALED, 3,
        "the unscaled arm passes filter offset 0 -> POINT (=1), \
         which is `create_samplers`' point/clamp entry, index 3"
    );
    assert_eq!(
        UI_SURFACE_SAMPLER_SCALED, 1,
        "the scaled or rotated arm passes filter offset 1 -> LINEAR (=2), \
         which is `create_samplers`' linear/clamp entry, index 1"
    );
    assert_ne!(
        UI_SURFACE_SAMPLER_SCALED, UI_SURFACE_SAMPLER_UNSCALED,
        "if the two ever resolve to one descriptor, every assertion in this file is vacuous"
    );

    // Every branch of `vw == pw && vh == ph && rot == identity`, one at a time.
    assert_eq!(
        ui_surface_sampler((64, 32), (64, 32), false, false),
        UI_SURFACE_SAMPLER_UNSCALED,
        "natural size, unrotated, untiled -> POINT/CLAMP"
    );
    assert_eq!(
        ui_surface_sampler((128, 32), (64, 32), false, false),
        UI_SURFACE_SAMPLER_SCALED,
        "surface stretched in x -> LINEAR"
    );
    assert_eq!(
        ui_surface_sampler((64, 64), (64, 32), false, false),
        UI_SURFACE_SAMPLER_SCALED,
        "surface stretched in y -> LINEAR"
    );
    assert_eq!(
        ui_surface_sampler((32, 16), (64, 32), false, false),
        UI_SURFACE_SAMPLER_SCALED,
        "surface shrunk -> LINEAR; the condition is inequality, not 'bigger than'"
    );
    assert_eq!(
        ui_surface_sampler((64, 32), (64, 32), true, false),
        UI_SURFACE_SAMPLER_SCALED,
        "rotated at natural size -> LINEAR; the exact rotation-matrix comparison with identity is the \
         third conjunct and drops out of the `&&` on its own"
    );
    // The glyph draw's sampler is a different material's setting and must not be conflated with
    // this one, even though both happen to be POINT/CLAMP at natural size.
    assert_eq!(
        UI_GLYPH_SAMPLER, UI_SURFACE_SAMPLER_UNSCALED,
        "they coincide -- a text font's material initialization sets POINT/CLAMP on the \
         font's own material -- but they are two independent readings and this records that they \
         agree rather than that one is derived from the other"
    );
}

/// Behaviour: rendering.ui-sampler.blits-and-fills-bind-the-sampler-the-material-selects
/// **The census on a real frame: the unscaled case and the scaled case, separately.**
///
/// The three counts are one claim each:
///
/// 1. `blits_unscaled > 0` and every one of them bound POINT.
/// 2. `blits_scaled` bound LINEAR — the *other* arm. Asserted separately, because a frame of
///    unscaled elements alone would satisfy "everything binds POINT" and could not tell a
///    conditional from a constant.
/// 3. The whole bind census adds up: POINT binds equal the glyph draws plus the unscaled blits
///    plus the fills, and LINEAR binds equal the scaled blits. An equality rather than an
///    inequality, so that a bind from somewhere unaccounted for reddens instead of hiding.
#[test]
fn each_blit_and_fill_binds_the_sampler_the_material_logic_selects() {
    let _gpu = gpu_lock();
    require_dats();
    let mut app = app_on_wizard();

    let before = app.renderer_mut().ui_stats;
    app.renderer_mut().clear_sampler_binds();
    app.frame();
    let binds = app.renderer_mut().sampler_binds();
    let now = app.renderer_mut().ui_stats;

    let glyph_draws = now.glyph_draws - before.glyph_draws;
    let outline_draws = now.outline_draws - before.outline_draws;
    let unscaled = now.blits_unscaled - before.blits_unscaled;
    let scaled = now.blits_scaled - before.blits_scaled;
    let fills = now.fills_drawn - before.fills_drawn;
    let tiled = now.blits_tiled - before.blits_tiled;

    eprintln!(
        "one char-gen frame bound samplers {binds:?} (0 linear/wrap, 1 linear/clamp, \
         2 point/wrap, 3 point/clamp) over {glyph_draws} glyph draw(s), {outline_draws} outline \
         draw(s), {unscaled} unscaled blit(s) ({tiled} of them tiled), {scaled} scaled blit(s) \
         and {fills} fill(s)"
    );

    // The denominator first: a frame that drew nothing scores a broken setup as a pass.
    assert!(
        unscaled + scaled > 0,
        "this frame issued no image blit at all, so it cannot say which sampler one binds"
    );
    assert!(
        unscaled > 0,
        "no blit in this frame was at its source surface's own size, so the POINT arm of the \
         conditional was never exercised: {unscaled} unscaled, {scaled} scaled"
    );

    // `binds[0]` and `binds[2]` are the two WRAP-both descriptors, and `binds[2]` is not zero:
    // surface-material generation sets CLAMP unless the element tiles, and this build expresses
    // the picture repeat as a wrapping sampler. `binds[0]` stays zero because it is LINEAR/WRAP
    // and no conjunct of the filter condition has a producer. The two address modes are
    // independent, so a tiled blit lands on descriptor 2, 6 or 7 depending on which axes cross a
    // seam.
    assert!(
        tiled > 0,
        "no char-gen blit tiles, so the WRAP arm says nothing: {binds:?}"
    );
    assert_eq!(
        binds[0], 0,
        "nothing binds LINEAR/WRAP on both axes: {binds:?}"
    );
    let point_wrap_any = binds[UI_SURFACE_SAMPLER_TILED as usize]
        + binds[dereth_render::ui::pixel_rules::UI_SURFACE_SAMPLER_TILED_U as usize]
        + binds[dereth_render::ui::pixel_rules::UI_SURFACE_SAMPLER_TILED_V as usize];
    assert_eq!(
        point_wrap_any, tiled,
        "the POINT descriptors that wrap at least one axis (2, 6, 7) are exactly the tiled \
         blits: {binds:?}"
    );

    // The whole census, as an equality.
    let point_expected = glyph_draws + outline_draws + (unscaled - tiled) + fills;
    assert_eq!(
        binds[UI_SURFACE_SAMPLER_UNSCALED as usize],
        point_expected,
        "POINT/CLAMP binds must be exactly the glyph draws ({glyph_draws}) + outline draws \
         ({outline_draws}) + untiled blits ({}) + fills ({fills}) = {point_expected}; \
         the census is {binds:?}",
        unscaled - tiled
    );
    assert_eq!(
        binds[UI_SURFACE_SAMPLER_SCALED as usize], scaled,
        "LINEAR binds must be exactly the scaled blits ({scaled}); the census is {binds:?}. \
         Nothing binds it, because neither conjunct of `vw == pw && rot == I` has a producer"
    );
    app.shutdown();
}

/// **An element bigger than its picture changes the address bit, not the filter bit.**
///
/// The element's UI surface is created at the *element's* own size, so its virtual and physical
/// sizes match and the filter stays POINT; the picture no longer covers the box, so picture
/// drawing repeats it and the **address mode** goes to WRAP. One element at three sizes goes
/// through the real `Renderer::draw_ui`, with the census read back each time, so a
/// *conditional* is visible rather than a constant.
#[test]
fn an_element_wider_than_its_picture_binds_wrap_where_the_same_element_at_its_own_size_binds_clamp()
{
    let _gpu = gpu_lock();
    require_dats();
    let mut app = app_on_wizard();

    // Find an image this screen actually uploaded and the size it was uploaded at, by asking the
    // frame's own draw list rather than by naming a DataID this test would then have to keep
    // true.
    let cmds = app.ui_draw_list();
    let (id, natural) = cmds
        .iter()
        .find_map(|c| {
            let id = c.image?;
            let w = c.screen.x1 - c.screen.x0 + 1;
            let h = c.screen.y1 - c.screen.y0 + 1;
            // The doubled element has to stay on the back buffer. The address mode follows the
            // picture draw's *clipped* run -- the grid walks the intersection with the
            // destination, not the whole box -- so an element whose second copy is entirely off
            // screen crosses no seam and correctly keeps CLAMP. The frame's first blit is the
            // 800 x 600 background, whose double is clipped back to 790 and never reaches the
            // seam.
            (w > 1 && h > 1 && 10 + 2 * w <= 800 && 10 + h <= 600).then_some((id, (w, h)))
        })
        .expect("the char-gen screen blits at least one image");

    let mut at = |w: i32,
                  h: i32|
     -> (
        [u64; dereth_render::ui::pixel_rules::UI_SAMPLER_COUNT],
        u64,
        u64,
    ) {
        let cmd = dereth_ui::UiDrawCmd {
            who: dereth_ui::ElemHandle::for_test(0),
            screen: dereth_ui::region::Box2D::new(10, 10, 10 + w - 1, 10 + h - 1),
            clip: dereth_ui::region::Box2D::new(0, 0, 799, 599),
            image: Some(id),
            image_op: None,
            image_source: dereth_ui::ImageSource::Interface,
            blit_mode: dereth_ui::BlitMode::default(),
            alpha_blend_mod: 1.0,
            tiling_offset: (0, 0),
            rotation_z_degrees: 0,
            color: 0xFFFF_FFFF,
            glyphs: Vec::new(),
            text_outline: None,
            invert: Vec::new(),
            fills: Vec::new(),
        };
        let before = app.renderer_mut().ui_stats;
        app.renderer_mut().clear_sampler_binds();
        // The same `Renderer::draw_ui` a frame runs, inside the same frame bracket, with one
        // command: `prepare_ui` first because a texture upload runs a command list of its own and
        // cannot happen between `start_frame` and `end_frame`.
        let r = app.renderer_mut();
        r.start_frame().expect("start_frame");
        r.draw_ui(&[cmd]).expect("draw_ui");
        r.end_frame().expect("end_frame");
        let now = app.renderer_mut().ui_stats;
        (
            app.renderer_mut().sampler_binds(),
            now.blits_unscaled - before.blits_unscaled,
            now.blits_tiled - before.blits_tiled,
        )
    };

    // 1. At the picture's own size: POINT/CLAMP. Picture drawing takes its single blit.
    let (binds, unscaled, tiled) = at(natural.0, natural.1);
    assert_eq!(
        (unscaled, tiled),
        (1, 0),
        "at its own {natural:?} the picture covers the box: {unscaled} unscaled, {tiled} tiled"
    );
    assert_eq!(
        binds[UI_SURFACE_SAMPLER_UNSCALED as usize], 1,
        "the untiled blit must bind POINT/CLAMP: {binds:?}"
    );
    assert_eq!(
        binds[UI_SURFACE_SAMPLER_TILED as usize], 0,
        "and not WRAP: {binds:?}"
    );
    assert_eq!(
        binds[dereth_render::ui::pixel_rules::UI_SURFACE_SAMPLER_TILED_U as usize],
        0,
        "and not the mixed WRAP-U descriptor either: {binds:?}"
    );

    // 2. The same image, same code path, twice as wide: the grid arm, POINT/**WRAP**.
    let (binds, unscaled, tiled) = at(natural.0 * 2, natural.1);
    assert_eq!(
        (unscaled, tiled),
        (1, 1),
        "twice as wide the element tiles and is still unscaled: {unscaled} unscaled, {tiled} tiled"
    );
    // Only **U** crosses a seam here -- the element is twice as wide as its picture and exactly
    // as tall -- so the descriptor is 6, POINT/WRAP-U/CLAMP-V, and not 2.
    assert_eq!(
        binds[dereth_render::ui::pixel_rules::UI_SURFACE_SAMPLER_TILED_U as usize],
        1,
        "the tiled blit must bind POINT/WRAP-U/CLAMP-V -- the arm no shipped screen \
         exercises, and not LINEAR: {binds:?}"
    );
    assert_eq!(
        binds[UI_SURFACE_SAMPLER_TILED as usize], 0,
        "and must not wrap V, which does not cross a seam at this geometry: {binds:?}"
    );
    assert_eq!(
        binds[UI_SURFACE_SAMPLER_UNSCALED as usize], 0,
        "and not CLAMP on both: {binds:?}"
    );
    assert_eq!(
        binds[UI_SURFACE_SAMPLER_SCALED as usize], 0,
        "and never LINEAR: {binds:?}"
    );

    // 3. Smaller than the picture, which is the *other* side of the predicate: picture drawing
    //    crops with one image blit, so it is back to CLAMP. The condition is coverage, not
    //    inequality: a shrunk element is not scaled either.
    let (binds, unscaled, tiled) = at(natural.0 / 2 + 1, natural.1);
    assert_eq!(
        (unscaled, tiled),
        (1, 0),
        "smaller than its picture is a crop, not a repeat"
    );
    assert_eq!(
        binds[UI_SURFACE_SAMPLER_UNSCALED as usize], 1,
        "a cropped blit binds POINT/CLAMP: {binds:?}"
    );

    eprintln!(
        "ui sampler: {id:?} at its own {natural:?} binds POINT/CLAMP; the same image in an \
         element {}x{} binds POINT/WRAP-U/CLAMP-V; cropped to {}x{} it is back to POINT/CLAMP",
        natural.0 * 2,
        natural.1,
        natural.0 / 2 + 1,
        natural.1
    );
    app.shutdown();
}

/// **The fills, stated as the guard rail they are and not as a repair.**
///
/// A fill's texture is one white texel under CLAMP. Every sample of it returns the same value
/// whichever filter is bound, so moving the fill from sampler 1 to sampler 3 **cannot change a
/// pixel** — and saying otherwise would be a claim without a measurable denominator.
///
/// What *is* true and worth pinning: the client's own choice for a fill is POINT, because a fill
/// is written into the element's own UI surface (created at the element's `(w, h)`, so virtual equals physical by construction) and that surface's material
/// is surface-material generation's POINT/CLAMP. The census now agrees with it.
#[test]
fn a_fill_binds_point_because_its_surface_is_never_scaled_and_it_could_not_look_different() {
    // The condition a fill always meets, evaluated rather than asserted from prose.
    assert_eq!(
        ui_surface_sampler((1, 1), (1, 1), false, false),
        UI_SURFACE_SAMPLER_UNSCALED,
        "a fill's surface is created at the element's own size, so the sizes are equal by \
         construction and the transform's unscaled condition always answers POINT"
    );
    let _gpu = gpu_lock();
    require_dats();
    let mut app = app_on_wizard();

    let before = app.renderer_mut().ui_stats;
    app.renderer_mut().clear_sampler_binds();
    app.frame();
    let now = app.renderer_mut().ui_stats;
    let fills = now.fills_drawn - before.fills_drawn;
    eprintln!("ui sampler: {fills} fill(s) in one char-gen frame");
    // Not asserted non-zero: a screen with no visible fill is an ordinary screen, and inventing a
    // denominator that is not there would be worse than saying it is absent. The equality in
    // `each_blit_and_fill_binds_the_sampler_the_material_logic_selects` is where the fills are
    // held to account when the frame does have some.
    app.shutdown();
}

/// Index 3 is the only sampler the glyph draw may bind, and it is written down once.
///
/// The literal pins the constant directly because every other reader of this value, including
/// the render crate's glyph pixel measurements, goes through the symbol and so cannot see the symbol
/// being wrong.
///
#[test]
fn the_glyph_sampler_constant_is_the_point_clamp_index_and_is_stated_as_a_literal() {
    assert_eq!(
        UI_GLYPH_SAMPLER, 3,
        "`Gpu::create_samplers` writes {{linear/wrap, linear/clamp, point/wrap, point/clamp}} in \
         that order, and a text font's own material asks for POINT + CLAMP, which \
         is index 3"
    );
    // `assertions_on_constants` is exactly the point here: pinning a constant is an assertion
    // whose value is knowable at compile time, and clippy cannot tell that from a tautology.
    #[allow(clippy::assertions_on_constants)]
    {
        assert!(
            dereth_render::ui::pixel_rules::UI_SAMPLER_IS_POINT,
            "the UI glyph sampler is a point filter"
        );
    }
    // That index 3 *behaves* as a point filter is not asserted here from a comment: the render
    // crate's glyph pixel test establishes it through the device, by measuring that a half-pixel displacement moves 0 pixels under this
    // constant and hundreds under sampler 1.
}

/// Behaviour: rendering.ui-sampler.every-glyph-draw-binds-point-clamp
/// **The measurement.** Every glyph draw of a real frame bound the POINT sampler, and the frame
/// really had glyph draws in it.
///
/// Three claims:
///
/// 1. `glyph_draws > 0` — the denominator. A frame that drew no text would satisfy "no glyph draw
///    bound LINEAR" trivially.
/// 2. `binds[POINT/CLAMP]` equals every draw the material logic puts on it: glyph, outline,
///    untiled blit and fill draws. Fewer means a draw is on a linear sampler; more means something
///    unaccounted for took POINT/CLAMP too.
/// 3. The counter is calibrated the other way: the wrapping POINT descriptors 2, 6 and 7 are bound
///    by exactly the tiled blits, and LINEAR/CLAMP by exactly the scaled blits, so a counter stuck
///    at "everything is 3" cannot pass.
#[test]
fn every_glyph_draw_of_a_real_frame_binds_the_point_sampler() {
    let _gpu = gpu_lock();
    require_dats();
    let mut app = app_on_wizard();

    // Count one frame's binds on their own, from a settled screen.
    let before = app.renderer_mut().ui_stats;
    app.renderer_mut().clear_sampler_binds();
    app.frame();
    let binds = app.renderer_mut().sampler_binds();
    let stats = app.renderer_mut().ui_stats;
    let glyph_draws = stats.glyph_draws - before.glyph_draws;
    // POINT/CLAMP is bound by the glyph draw, by the text element's outline pass (drawn ahead of
    // the foreground out of the font's background sheet), and by every unscaled image blit and
    // fill; LINEAR only by a scaled or rotated element. The address bit is a second, independent
    // choice: a blit whose picture does not cover its element repeats it and binds a wrapping
    // POINT descriptor instead, so the equality is written over the whole census and a POINT
    // bind from anywhere unaccounted for still fails.
    let outline_draws = stats.outline_draws - before.outline_draws;
    let unscaled_blits = stats.blits_unscaled - before.blits_unscaled;
    let scaled_blits = stats.blits_scaled - before.blits_scaled;
    let tiled_blits = stats.blits_tiled - before.blits_tiled;
    let fills = stats.fills_drawn - before.fills_drawn;
    let point_expected = glyph_draws + outline_draws + (unscaled_blits - tiled_blits) + fills;

    eprintln!(
        "one char-gen frame bound samplers {binds:?} (0 linear/wrap, 1 linear/clamp, \
         2 point/wrap, 3 point/clamp) over {glyph_draws} glyph draw(s) carrying \
         {} glyph quad(s); font_failures {}",
        stats.glyphs_drawn, stats.font_failures
    );

    assert_eq!(
        stats.font_failures, 0,
        "a font this screen names would not rasterise"
    );
    assert!(
        glyph_draws > 0,
        "this frame issued no glyph draw at all, so it cannot say which sampler one binds"
    );
    assert_eq!(
        binds[UI_GLYPH_SAMPLER as usize],
        point_expected,
        "of {glyph_draws} glyph draw(s), {outline_draws} outline draw(s), {} untiled blit(s) and \
         {fills} fill(s) -- every one of which the client's own material logic puts on \
         POINT/CLAMP -- {} bound that sampler where {point_expected} should have. The whole bind \
         census is {binds:?}. Fewer means a draw is still on a linear sampler \
         more means something unaccounted for took POINT/CLAMP too",
        unscaled_blits - tiled_blits,
        binds[UI_GLYPH_SAMPLER as usize]
    );
    // The calibration in the other direction: the counter must be shown able to report an index
    // that is **not** the one under test, or a census stuck at "everything is 3" reads identically
    // to a correct one. The frame's discriminating indices are the wrapping POINT descriptors,
    // bound by its repeating blits: the address mode is per axis, so a tiled blit lands on 2
    // (both axes wrap), 6 (U only) or 7 (V only), and the discriminator is the group.
    let point_wrap_any = binds[2]
        + binds[dereth_render::ui::pixel_rules::UI_SURFACE_SAMPLER_TILED_U as usize]
        + binds[dereth_render::ui::pixel_rules::UI_SURFACE_SAMPLER_TILED_V as usize];
    assert!(
        point_wrap_any > 0,
        "no draw in this frame bound a sampler other than {UI_GLYPH_SAMPLER}, so the census \
         cannot distinguish 'the glyph draw chose it' from 'this counter only ever reports it': \
         {binds:?}"
    );
    assert_eq!(
        point_wrap_any, tiled_blits,
        "the only thing that binds a wrapping POINT descriptor is a **tiled** blit, and this \
         frame has {tiled_blits} of them: {binds:?}"
    );
    assert_eq!(
        binds[1], scaled_blits,
        "LINEAR/CLAMP is bound by a scaled or rotated blit, and this frame has \
         {scaled_blits} of them: {binds:?}"
    );
    app.shutdown();
}
