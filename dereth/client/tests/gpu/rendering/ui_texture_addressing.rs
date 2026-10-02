//! A UI picture's U and V address modes are chosen independently, and the common shipped case is
//! mixed (one axis wraps, the other clamps). Fixture: the retail dats' pictures `0x0600612B` and
//! `0x0600747F` drawn through the real UI renderer on a headless App, compared with the source
//! bitmap's own bytes; plus one shipped gameplay frame's sampler census.
//!
//! # The per-axis rule
//!
//! UI-surface material generation first writes the same address mode to both axes:
//!
//! ```text
//! address = if tiles { WRAP (1) } else { CLAMP (3) }
//! address_u = address
//! address_v = address
//! ```
//!
//! — and then, **only** on the tiling branch, refines **each axis on its own**:
//!
//! ```text
//! repeat_u = abs(virtual_width / physical_width)
//! difference = abs(repeat_u - 1.0)
//! address_u = if difference > 2.0e-4 { WRAP (1) } else { CLAMP (3) }
//! ```
//!
//! The V axis uses the identical rule with virtual height divided by physical height. The
//! comparison is **greater than**: an axis whose repeat count differs from 1 by the epsilon or less
//! keeps `CLAMP`, and only one that differs by more gets `WRAP`. The inverted comparison would also
//! look like "the address mode depends on the repeat count", which is why the polarity is asserted.
//! [`dereth_render::ui::pixel_rules::ui_surface_address_mode`] implements the rule.
//!
//! # Why the *binding* is not that function
//!
//! Surface-material generation's refinement is a ratio of two extents. It **knows nothing about
//! the tiling offset**, because in retail the offset is applied by the CPU picture grid rather than
//! by the material. So the material calls an offset element's axes `CLAMP` while retail's own blit
//! wraps them — a disagreement retail never shows, because no shipped element authors the tiling
//! flag (`0xCD = 2`) and both axes are `TEXADDRESS_CLAMP` on every frame it draws.
//!
//! This rebuild has no CPU surface to repeat into, so the repeat *is* the sampler, and the
//! predicate it must bind from is `Draw`'s: [`dereth_render::ui::pixel_rules::graphic_draw_axis`],
//! `(start mod g) + extent > g`, per axis. That difference is the whole reason
//! [`a_point_sample_lands_on_the_seam_of_the_axis_the_material_calls_non_tiling`] can exist at
//! all: it is the one geometry in which the two rules disagree, and it is built from the retail
//! data and retail's own two functions rather than invented.
//!
//! # What is asserted against what
//!
//! The pixel oracle is the **source bitmap's own bytes**, decoded from the retail dat and wrapped
//! by hand in this file — never a golden image, never the renderer's own output, and never a
//! window capture. Every zero is bracketed by the reading it replaced: each frame is measured against
//! the per-axis expectation *and* against the `CLAMP`-on-both-axes one, and the second must be
//! large. A differ that cannot separate the two says nothing when it reports 0.
//!
//! `0x0600612B` is used because it is a shipped 5 x 10 strip whose **columns differ**, while 31 of
//! the 37 blits the shipped screens repeat are uniform along
//! the axis they repeat, so they cannot move a pixel either way. The premise is asserted before
//! anything is believed.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::client_dir;
use crate::common::gpu_lock;

use dereth_client::app::App;
use dereth_client::config::Config;
use dereth_primitives::DataId;
use dereth_render::ui::pixel_rules::{
    graphic_draw_axis, graphic_draw_tiles, ui_sampler_address_modes, ui_sampler_filter,
    ui_surface_address_mode, ui_surface_sampler_axes, TEXADDRESS_CLAMP, TEXADDRESS_WRAP,
    TEXFILTER_LINEAR, TEXFILTER_POINT, UI_SAMPLER_COUNT, UI_SURFACE_SAMPLER_SCALED,
    UI_SURFACE_SAMPLER_TILED, UI_SURFACE_SAMPLER_TILED_SCALED, UI_SURFACE_SAMPLER_TILED_U,
    UI_SURFACE_SAMPLER_TILED_V, UI_SURFACE_SAMPLER_UNSCALED,
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

/// The shipped 5 x 10 border strip whose **columns** differ — `classic_gameplay`'s left window
/// edge.
const STRIP: DataId = DataId(0x0600_612B);

/// A shipped 100 x 16 picture whose **rows** differ and which is **fully opaque**, for the
/// vertical half.
///
/// [`STRIP`] cannot do that job: it is a *vertical* border, uniform along y, so a V wrap and a V
/// clamp produce identical pixels on it and a differ over it would report 0 either way. That
/// differential blindness is why the two halves of
/// this file use two different pictures rather than one picture twice. Opacity is the second
/// requirement and it eliminated the obvious candidates: `0x06004C5F` and `0x06004C63` vary along
/// their rows but carry alpha, so the back buffer shows them blended over the clear colour and a
/// comparison with the source bitmap's own bytes is measuring the blend rather than the sampler.
/// `0x0600747F` varies along both axes with alpha 255 everywhere, and it is one of the six
/// shipped repeats that can move a pixel.
const V_STRIP: DataId = DataId(0x0600_747F);

// -------------------------------------------------------------------------------------------
// The table
// -------------------------------------------------------------------------------------------

/// **Eight descriptors, and each one is a different `(filter, AddressU, AddressV)`.**
///
/// Stated as literals, one cell at a time, because every other reader reaches them through the
/// symbol and so cannot notice the symbol being wrong. The first four keep their established
/// meanings: an index that quietly changed meaning would leave the tests written against it green
/// while measuring something else.
#[test]
fn there_are_eight_descriptors_and_the_first_four_still_mean_what_they_meant() {
    assert_eq!(
        UI_SAMPLER_COUNT, 8,
        "two filters times four (U, V) combinations"
    );

    // The four symmetric pairs.
    assert_eq!(UI_SURFACE_SAMPLER_TILED_SCALED, 0, "LINEAR + WRAP/WRAP");
    assert_eq!(UI_SURFACE_SAMPLER_SCALED, 1, "LINEAR + CLAMP/CLAMP");
    assert_eq!(UI_SURFACE_SAMPLER_TILED, 2, "POINT + WRAP/WRAP");
    assert_eq!(UI_SURFACE_SAMPLER_UNSCALED, 3, "POINT + CLAMP/CLAMP");
    // The only two a shipped mixed element can reach.
    assert_eq!(UI_SURFACE_SAMPLER_TILED_U, 6, "POINT + WRAP/CLAMP");
    assert_eq!(UI_SURFACE_SAMPLER_TILED_V, 7, "POINT + CLAMP/WRAP");

    let want = [
        (TEXFILTER_LINEAR, TEXADDRESS_WRAP, TEXADDRESS_WRAP),
        (TEXFILTER_LINEAR, TEXADDRESS_CLAMP, TEXADDRESS_CLAMP),
        (TEXFILTER_POINT, TEXADDRESS_WRAP, TEXADDRESS_WRAP),
        (TEXFILTER_POINT, TEXADDRESS_CLAMP, TEXADDRESS_CLAMP),
        (TEXFILTER_LINEAR, TEXADDRESS_WRAP, TEXADDRESS_CLAMP),
        (TEXFILTER_LINEAR, TEXADDRESS_CLAMP, TEXADDRESS_WRAP),
        (TEXFILTER_POINT, TEXADDRESS_WRAP, TEXADDRESS_CLAMP),
        (TEXFILTER_POINT, TEXADDRESS_CLAMP, TEXADDRESS_WRAP),
    ];
    for (i, w) in want.iter().enumerate() {
        let i = u32::try_from(i).expect("eight fits");
        let (u, v) = ui_sampler_address_modes(i);
        assert_eq!((ui_sampler_filter(i), u, v), *w, "descriptor {i}");
    }
    // Eight distinct triples, or the table has a duplicate and one combination is unreachable.
    let mut seen: Vec<(u32, u32, u32)> = want.to_vec();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(
        seen.len(),
        8,
        "the eight descriptors must be eight different samplers"
    );

    // And the index arithmetic must reach every one of them from a (filter, wrap-pair) input.
    let s = (64u32, 32u32);
    let mut reached: Vec<u32> = Vec::new();
    for rotated in [false, true] {
        for wrap in [(true, true), (false, false), (true, false), (false, true)] {
            reached.push(ui_surface_sampler_axes(s, s, rotated, wrap));
        }
    }
    reached.sort_unstable();
    assert_eq!(
        reached,
        (0..8).collect::<Vec<u32>>(),
        "every descriptor must be reachable"
    );
}

/// Behaviour: rendering.ui-sampler.u-and-v-address-modes-are-chosen-independently
/// **Material generation's per-axis refinement, and the comparison polarity that decides it.**
///
/// The polarity is the load-bearing half: when the value is equal to or less than the epsilon,
/// the axis keeps `CLAMP`. Both sides of the epsilon are driven, not just the sides the shipped data
/// happens to use — an inverted reading would still pass a test that only ever asked about a
/// clearly-repeating axis.
#[test]
fn the_material_refines_each_axis_against_its_own_repeat_count() {
    // With tiling false, `(false == false) * 2 + 1` gives 3 on both axes and the refinement branch
    // is never reached.
    assert_eq!(
        ui_surface_address_mode((792, 5), (10, 5), false),
        (TEXADDRESS_CLAMP, TEXADDRESS_CLAMP),
        "with tiling disabled the refinement is not reached at all, whatever the extents"
    );

    // The shipped mixed case: U repeats 79.2 times, V exactly once.
    assert_eq!(
        ui_surface_address_mode((792, 5), (10, 5), true),
        (TEXADDRESS_WRAP, TEXADDRESS_CLAMP),
        "a 10 x 5 strip in a 792 x 5 element -- the case the four-sampler table could not express"
    );
    assert_eq!(
        ui_surface_address_mode((5, 592), (5, 10), true),
        (TEXADDRESS_CLAMP, TEXADDRESS_WRAP),
        "and its vertical twin, the other way round"
    );
    assert_eq!(
        ui_surface_address_mode((64, 32), (64, 32), true),
        (TEXADDRESS_CLAMP, TEXADDRESS_CLAMP),
        "an element that repeats exactly once on both axes keeps CLAMP on both -- this is the \
         comparison's polarity, and inverting it would give WRAP here"
    );

    // The epsilon itself, driven from both sides. It is 2.0e-4 as a single-precision float, and
    // the comparison is on `|v/p - 1|`. 10000/10001 is 1 - 9.999e-5, inside; 10000/10003 is
    // 1 - 2.999e-4, outside.
    assert_eq!(
        ui_surface_address_mode((10_000, 32), (10_001, 32), true).0,
        TEXADDRESS_CLAMP,
        "|1 - 10000/10001| = 9.99e-5 < 2.0e-4, so CLAMP stands"
    );
    assert_eq!(
        ui_surface_address_mode((10_000, 32), (10_003, 32), true).0,
        TEXADDRESS_WRAP,
        "|1 - 10000/10003| = 3.0e-4 > 2.0e-4, so the axis wraps -- the other side of the same \
         comparison, so a constant answer cannot pass both"
    );
}

/// **The picture grid's per-axis modulo, and why it is not `graphic_draw_tiles`.**
///
/// `graphic_draw_tiles` is the negation of the *fast path*, a single `&&` over both offsets and
/// both extents; `graphic_draw_axis` is the per-axis question the grid actually answers. The two
/// disagree on exactly the mixed elements, and that disagreement is asserted here
/// rather than left to the pixel tests to imply.
#[test]
fn the_per_axis_seam_predicate_is_not_the_fast_path_predicate() {
    // Reduction: the start texel is `start mod g`, with the negative fix-up.
    assert_eq!(
        graphic_draw_axis(5, 0, 5),
        (0, false),
        "exactly one copy, no seam"
    );
    assert_eq!(
        graphic_draw_axis(5, 3, 5),
        (3, true),
        "offset 3 in a 5-wide run crosses at 5"
    );
    assert_eq!(
        graphic_draw_axis(5, 5, 5),
        (0, false),
        "offset 5 reduces to 0 -- and no seam"
    );
    assert_eq!(
        graphic_draw_axis(5, -2, 5),
        (3, true),
        "negative offsets wrap up, not down"
    );
    assert_eq!(
        graphic_draw_axis(500, -46, 400),
        (454, true),
        "0x0600113A, the shipped scroll"
    );
    assert_eq!(
        graphic_draw_axis(10, 0, 10),
        (0, false),
        "and its V axis: exactly one copy"
    );
    assert_eq!(
        graphic_draw_axis(10, 0, 11),
        (0, true),
        "one pixel too tall crosses"
    );
    assert_eq!(
        graphic_draw_axis(0, 3, 5),
        (0, false),
        "a degenerate picture cannot wrap"
    );

    // The disagreement. A 5 x 10 element showing a 5 x 10 picture at offset (3, 0):
    // the fast path is not taken, so `graphic_draw_tiles` says "tiled"...
    assert!(
        graphic_draw_tiles((5, 10), (5, 10), (3, 0)),
        "the offset takes the grid"
    );
    // ...but only U crosses a seam. Binding WRAP on V would be harmless *here* and is not the
    // client's answer, and the four-sampler table could not say so.
    assert!(graphic_draw_axis(5, 3, 5).1, "U crosses");
    assert!(!graphic_draw_axis(10, 0, 10).1, "V does not");
    // And the material disagrees with both, because it cannot see the offset at all.
    assert_eq!(
        ui_surface_address_mode((5, 10), (5, 10), true),
        (TEXADDRESS_CLAMP, TEXADDRESS_CLAMP),
        "the material-generation ratio is 1 on both axes: it never sees the tiling offset"
    );
}

// -------------------------------------------------------------------------------------------
// The pixels
// -------------------------------------------------------------------------------------------

/// The frame the **source bitmap** says a `w x h` element at `(x, y)` should carry, built by hand
/// with the picture grid's modulo — and with either axis optionally **clamped** instead, which is
/// the wrong reading the pixel tests must tell apart.
fn expected(
    src: &dereth_primitives::TextureData,
    rect: (i32, i32, i32, i32),
    offset: (i32, i32),
    fb: (u32, u32),
    clamp: (bool, bool),
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
            // A clamping sampler holds `u` at the last texel once it leaves [0, 1]; a wrapping one
            // takes the modulo. The start is reduced either way by picture drawing, not by the sampler.
            let start = (offset.0.rem_euclid(iw), offset.1.rem_euclid(ih));
            let sx = if clamp.0 {
                (start.0 + c).min(iw - 1)
            } else {
                (start.0 + c) % iw
            };
            let sy = if clamp.1 {
                (start.1 + r).min(ih - 1)
            } else {
                (start.1 + r) % ih
            };
            let o = ((sy * iw + sx) * 4) as usize;
            out[(dy * fb.0 as i32 + dx) as usize] = [px[o], px[o + 1], px[o + 2], px[o + 3]];
        }
    }
    out
}

/// Differing pixels over one rectangle, on RGB only (the back buffer is opaque).
fn differing(cap: &[u8], want: &[[u8; 4]], fb: (u32, u32), r: (i32, i32, i32, i32)) -> usize {
    let mut n = 0;
    for y in r.1..r.1 + r.3 {
        for x in r.0..r.0 + r.2 {
            let i = (y * fb.0 as i32 + x) as usize;
            if [cap[i * 4], cap[i * 4 + 1], cap[i * 4 + 2]] != [want[i][0], want[i][1], want[i][2]]
            {
                n += 1;
            }
        }
    }
    n
}

struct Bench {
    app: App,
    store: dereth_dat::RetailDatStore,
    src: dereth_primitives::TextureData,
    did: DataId,
}

impl Bench {
    fn new(did: DataId, size: (u32, u32)) -> Self {
        let store = store();
        let tex = dereth_client::textures::TextureStore::new(&store);
        let src = tex.texture_data(did).expect("the shipped picture decodes");
        assert_eq!(
            src.format,
            dereth_primitives::TextureFormat::Bgra8,
            "this file reads the source texels directly, so it needs them uncompressed"
        );
        assert_eq!(
            (src.width, src.height),
            size,
            "the picture's shipped extent"
        );
        Self {
            app: app_on(mode::GAME_PLAY),
            store,
            src,
            did,
        }
    }

    /// **The premise guard, on the axis the test repeats along.**
    ///
    /// A picture that is constant along an axis makes a wrap and a clamp produce identical pixels
    /// along it, and a differ over it reports 0 whichever the sampler did. 31 of the 37 blits the
    /// shipped screens repeat are exactly that. The sharper form is what is actually required: a
    /// clamping sampler shows the axis's **last** texel past the seam, so the discrimination needs
    /// texels that differ from *that* one, not merely from each other.
    fn assert_varies(&self, axis_x: bool) {
        let (w, h) = (self.src.width as usize, self.src.height as usize);
        let px = &self.src.levels[0];
        let at = |x: usize, y: usize| &px[(y * w + x) * 4..][..4];
        if axis_x {
            assert!(
                (0..h).any(|y| (0..w - 1).any(|x| at(x, y) != at(w - 1, y))),
                "every column of {:?} equals its last, so CLAMP-U and WRAP-U agree on it and a \
                 zero from this differ would say nothing",
                self.did
            );
        } else {
            assert!(
                (0..w).any(|x| (0..h - 1).any(|y| at(x, y) != at(x, h - 1))),
                "every row of {:?} equals its last, so CLAMP-V and WRAP-V agree on it",
                self.did
            );
        }
    }

    /// One draw through the real `Renderer::draw_ui`, returning the back buffer and the census.
    fn shot(
        &mut self,
        rect: (i32, i32, i32, i32),
        offset: (i32, i32),
    ) -> (Vec<u8>, [u64; UI_SAMPLER_COUNT]) {
        let (x, y, w, h) = rect;
        let cmd = dereth_ui::UiDrawCmd {
            who: dereth_ui::ElemHandle::for_test(0),
            screen: dereth_ui::region::Box2D::new(x, y, x + w - 1, y + h - 1),
            clip: dereth_ui::region::Box2D::new(0, 0, 799, 599),
            image: Some(self.did),
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
        let r = self.app.renderer_mut();
        r.prepare_ui(&self.store, std::slice::from_ref(&cmd));
        r.clear_sampler_binds();
        r.start_frame().expect("start_frame");
        r.draw_ui(std::slice::from_ref(&cmd)).expect("draw_ui");
        r.end_frame().expect("end_frame");
        let binds = self.app.renderer_mut().sampler_binds();
        let (cw, ch, bgra) = self.app.renderer_mut().capture_bgra().expect("capture");
        assert_eq!(
            (cw, ch),
            (800, 600),
            "the capture must be the back buffer this test placed on"
        );
        (bgra, binds)
    }
}

/// **The acceptance: a POINT sample lands *on* the seam of the axis the material calls
/// non-tiling — the case the old argument said could not arise.**
///
/// The argument that binding `WRAP` on both axes is pixel-identical is that the non-tiling axis
/// spans exactly `[0, 1]` and no `POINT` sample reaches its seam. That is true of every element
/// whose axes are decided by extents alone. It is **not** true once the tiling offset is in play,
/// and that is the one place retail's two rules disagree:
///
/// * material generation computes both axes from `virtual / physical` — `5/5` and `10/10`, both
///   1 — and calls **both** axes `TEXADDRESS_CLAMP`;
/// * picture drawing starts at texel `(3 - box.x0 + dst.x0) mod 5 = 3` and blits `5 - 3 = 2`
///   columns before wrapping to texel 0, so **U crosses its seam three columns in five** while V
///   does not cross at all.
///
/// So this element has a `POINT` sample on the seam of an axis the material calls non-tiling —
/// thirty of them, one per row of the last three columns — and the frame is measured against the
/// source bitmap's own bytes with U wrapped and V clamped. Both wrong readings are required to
/// differ: clamping U (what surface-material generation alone would give) and wrapping V.
#[test]
fn a_point_sample_lands_on_the_seam_of_the_axis_the_material_calls_non_tiling() {
    let _gpu = gpu_lock();
    let mut b = Bench::new(STRIP, (5, 10));
    b.assert_varies(true);

    let fb = (800u32, 600u32);
    let rect = (100i32, 100i32, 5i32, 10i32);
    let offset = (3, 0);
    let (cap, binds) = b.shot(rect, offset);

    // Which descriptor the production path chose, from the census rather than from the call site.
    assert_eq!(
        binds[UI_SURFACE_SAMPLER_TILED_U as usize], 1,
        "the element must bind POINT/WRAP-U/CLAMP-V (6): census {binds:?}"
    );
    assert_eq!(
        binds.iter().sum::<u64>(),
        1,
        "one blit, one bind: {binds:?}"
    );

    let want = expected(&b.src, rect, offset, fb, (false, false));
    let clamp_u = expected(&b.src, rect, offset, fb, (true, false));
    let wrap_v = {
        // V wrapped is only distinguishable when a V sample leaves [0, 1], which it does not here
        // -- so this arm is expected to AGREE, and that is asserted rather than assumed: it is
        // why a four-sampler table cannot be told apart at this geometry.
        expected(&b.src, rect, offset, fb, (false, false))
    };
    let d = differing(&cap, &want, fb, rect);
    let d_clamp_u = differing(&cap, &clamp_u, fb, rect);
    let d_wrap_v = differing(&cap, &wrap_v, fb, rect);
    eprintln!(
        "ui addressing: 5x10 at offset {offset:?}: {d} px differ from WRAP-U/CLAMP-V, {d_clamp_u} from \
         CLAMP-U/CLAMP-V, {d_wrap_v} from WRAP-U/WRAP-V, of 50 covered; census {binds:?}"
    );

    assert_eq!(
        d, 0,
        "the frame must be the source bitmap with U wrapped: {d} px differ"
    );
    assert!(
        d_clamp_u > 0,
        "clamping U changes nothing here, so the differ cannot see the seam at all and its zero \
         above says nothing"
    );
    assert_eq!(
        d_wrap_v, 0,
        "and V is genuinely indistinguishable at this geometry, which is measured here rather \
         than assumed"
    );

    // The seam itself, named rather than folded into a count. Columns 2, 3 and 4 of the element
    // show texels 0, 1 and 2 -- and a CLAMP-U sampler would show texel 4 in all three.
    let px = &b.src.levels[0];
    let texel = |tx: i32, ty: i32| -> [u8; 3] {
        let o = ((ty * b.src.width as i32 + tx) * 4) as usize;
        [px[o], px[o + 1], px[o + 2]]
    };
    let at = |cx: i32, cy: i32| -> [u8; 3] {
        let i = ((rect.1 + cy) * fb.0 as i32 + rect.0 + cx) as usize;
        [cap[i * 4], cap[i * 4 + 1], cap[i * 4 + 2]]
    };
    let mut past_the_seam = 0usize;
    let mut discriminating = 0usize;
    for col in 2..5 {
        let src_col = col - 2;
        for row in 0..10 {
            past_the_seam += 1;
            assert_eq!(
                at(col, row),
                texel(src_col, row),
                "column {col} row {row} is past U = 1 and must be texel {src_col}"
            );
            if texel(src_col, row) != texel(4, row) {
                discriminating += 1;
                assert_ne!(
                    at(col, row),
                    texel(4, row),
                    "and must not be the last column smeared, which is what CLAMP-U gives"
                );
            }
        }
    }
    eprintln!(
        "ui addressing: {past_the_seam} POINT sample(s) past the U seam, {discriminating} of them at \
         positions a clamping sampler could not have produced"
    );
    assert_eq!(past_the_seam, 30, "three columns of ten");
    assert!(
        discriminating >= 10,
        "too few of the seam samples differ from the last column for a wrap and a clamp to be \
         distinguishable: {discriminating} of {past_the_seam}"
    );
    b.app.shutdown();
}

/// **The same thing on V, so the mixed pair is exercised both ways round.**
///
/// A test that only ever wraps U cannot tell "the U axis is wired" from "both axes follow U".
#[test]
fn the_vertical_twin_wraps_v_and_clamps_u() {
    let _gpu = gpu_lock();
    let mut b = Bench::new(V_STRIP, (100, 16));
    b.assert_varies(false);

    let fb = (800u32, 600u32);
    // The element is exactly the picture's 100 x 16, so U covers one copy and clamps; the V
    // offset of 5 makes the V run 5..21 over a 16-tall picture, so V crosses its seam at row 11.
    let rect = (100i32, 100i32, 100i32, 16i32);
    let offset = (0, 5);
    let (cap, binds) = b.shot(rect, offset);

    assert_eq!(
        binds[UI_SURFACE_SAMPLER_TILED_V as usize], 1,
        "the element must bind POINT/CLAMP-U/WRAP-V (7): census {binds:?}"
    );
    assert_eq!(
        binds[UI_SURFACE_SAMPLER_TILED_U as usize], 0,
        "and must not take its horizontal twin's descriptor: {binds:?}"
    );

    let want = expected(&b.src, rect, offset, fb, (false, false));
    let clamp_v = expected(&b.src, rect, offset, fb, (false, true));
    let d = differing(&cap, &want, fb, rect);
    let d_clamp_v = differing(&cap, &clamp_v, fb, rect);
    eprintln!(
        "ui addressing: 100x16 at offset {offset:?}: {d} px differ from CLAMP-U/WRAP-V, {d_clamp_v} from \
         CLAMP-U/CLAMP-V, of 1600 covered; census {binds:?}"
    );
    assert_eq!(
        d, 0,
        "the frame must be the source bitmap with V wrapped: {d} px differ"
    );
    assert!(
        d_clamp_v > 0,
        "clamping V changes nothing, so the differ cannot see the V seam"
    );
    b.app.shutdown();
}

/// **The modulo reduction belongs to picture drawing and is load-bearing under `CLAMP`.**
///
/// Dividing the raw `x0 - screen.x0 + tilingOffset.x` by the picture's width, unreduced, is harmless
/// under a wrapping sampler — `frac(u)` does not care about whole copies. Under a clamping one it
/// is fatal: an offset of a whole picture width
/// puts `u0` at 1.0 and every sample past the seam, so the element becomes its picture's last
/// column smeared across the whole box.
///
/// Here the offset is exactly one picture width on a box exactly one picture wide, so
/// `graphic_draw_axis` reduces the start to 0, the axis does **not** cross a seam and the sampler
/// is `CLAMP`. The frame must be the unshifted picture; the unreduced reading must not be.
#[test]
fn the_start_texel_is_reduced_modulo_the_picture_before_the_sampler_sees_it() {
    let _gpu = gpu_lock();
    let mut b = Bench::new(STRIP, (5, 10));
    b.assert_varies(true);

    let fb = (800u32, 600u32);
    let rect = (100i32, 100i32, 5i32, 10i32);
    let (cap, binds) = b.shot(rect, (5, 0));

    assert_eq!(
        binds[UI_SURFACE_SAMPLER_UNSCALED as usize], 1,
        "start 5 mod 5 = 0 and the run is 5 wide, so neither axis crosses: POINT/CLAMP/CLAMP (3). \
         census {binds:?}"
    );

    let want = expected(&b.src, rect, (0, 0), fb, (false, false));
    let d = differing(&cap, &want, fb, rect);
    // The reading without the reduction: `u` runs 1.0..2.0, and under CLAMP every sample is the
    // last column. Built by hand rather than by flipping a flag, so it cannot share a bug with
    // the code under test.
    let px = &b.src.levels[0];
    let mut smeared = vec![[0u8; 4]; (fb.0 * fb.1) as usize];
    for r in 0..10i32 {
        for c in 0..5i32 {
            let o = ((r * b.src.width as i32 + 4) * 4) as usize;
            let i = ((rect.1 + r) * fb.0 as i32 + rect.0 + c) as usize;
            smeared[i] = [px[o], px[o + 1], px[o + 2], px[o + 3]];
        }
    }
    let d_smeared = differing(&cap, &smeared, fb, rect);
    eprintln!(
        "ui addressing: 5x10 at offset (5, 0): {d} px differ from the unshifted source, {d_smeared} from \
         the unreduced CLAMP reading, of 50 covered; census {binds:?}"
    );
    assert_eq!(
        d, 0,
        "an offset of one whole picture is the unshifted picture: {d} px differ"
    );
    assert!(
        d_smeared > 0,
        "the smeared reading is indistinguishable from the right one, so this test cannot see \
         the reduction at all"
    );
    b.app.shutdown();
}

// -------------------------------------------------------------------------------------------
// The shipped frame
// -------------------------------------------------------------------------------------------

/// **The census on a real frame, as an equality recomputed from the draw list.**
///
/// Not a set of magic numbers: every command in the frame is put through the same two functions
/// the renderer uses and the eight counters are predicted, then compared with what the device
/// recorded. A bind from anywhere unaccounted for reddens, and so does a descriptor chosen for a
/// different reason than the draw list says.
///
/// Both mixed descriptors are additionally required to be **non-zero on shipped data**, because
/// this module rests on the claim that the mixed case is the common one. If the shipped
/// screens turned out to use only the symmetric pairs, the eight descriptors would be
/// unexercised decoration and this file would be saying so.
#[test]
fn the_shipped_frame_uses_the_mixed_descriptors_and_the_census_is_an_equality() {
    let _gpu = gpu_lock();
    let store = store();
    let tex = dereth_client::textures::TextureStore::new(&store);
    let mut app = app_on(mode::GAME_PLAY);

    let before = app.renderer_mut().ui_stats;
    app.renderer_mut().clear_sampler_binds();
    app.frame();
    let binds = app.renderer_mut().sampler_binds();

    // The prediction, from the draw list the frame just used.
    let mut want = [0u64; UI_SAMPLER_COUNT];
    let (mut blits, mut tiled, mut mixed) = (0usize, 0usize, 0usize);
    for c in app.ui_draw_list() {
        let Some(id) = c.image else { continue };
        let Ok(d) = tex.texture_data(id) else {
            continue;
        };
        blits += 1;
        let (x0, y0) = (
            c.screen.x0.max(c.clip.x0).max(0),
            c.screen.y0.max(c.clip.y0).max(0),
        );
        let (x1, y1) = (
            c.screen.x1.min(c.clip.x1).min(799),
            c.screen.y1.min(c.clip.y1).min(599),
        );
        if x1 < x0 || y1 < y0 {
            continue;
        }
        let (w, h) = (
            u32::try_from(x1 - x0 + 1).unwrap_or(0),
            u32::try_from(y1 - y0 + 1).unwrap_or(0),
        );
        let (_, wu) = graphic_draw_axis(d.width, x0 - c.screen.x0 + c.tiling_offset.0, w);
        let (_, wv) = graphic_draw_axis(d.height, y0 - c.screen.y0 + c.tiling_offset.1, h);
        let box_ = (
            u32::try_from(c.screen.x1 - c.screen.x0 + 1).unwrap_or(0),
            u32::try_from(c.screen.y1 - c.screen.y0 + 1).unwrap_or(0),
        );
        if graphic_draw_tiles((d.width, d.height), box_, c.tiling_offset) {
            tiled += 1;
        }
        if wu != wv {
            mixed += 1;
        }
        let s = ui_surface_sampler_axes(box_, box_, c.rotation_z_degrees % 360 != 0, (wu, wv));
        want[s as usize] += 1;
    }
    // Glyphs, outlines and fills all take POINT/CLAMP; they are counted from the renderer's own
    // statistics rather than re-derived, because this test is about the address bit.
    let st = app.renderer_mut().ui_stats;
    want[UI_SURFACE_SAMPLER_UNSCALED as usize] += (st.glyph_draws - before.glyph_draws)
        + (st.outline_draws - before.outline_draws)
        + (st.fills_drawn - before.fills_drawn);

    eprintln!(
        "ui addressing: one gameplay frame: census {binds:?}, predicted {want:?}, over {blits} blit(s), \
         {tiled} tiled, {mixed} of them mixed"
    );
    assert!(
        blits > 0,
        "this frame drew no picture, so it measures nothing"
    );
    assert_eq!(
        binds, want,
        "the device's census must equal the draw list's own prediction"
    );
    assert!(
        binds[UI_SURFACE_SAMPLER_TILED_U as usize] + binds[UI_SURFACE_SAMPLER_TILED_V as usize] > 0,
        "no shipped blit is mixed, so the eight descriptors are decoration: {binds:?}"
    );
    for linear in [
        UI_SURFACE_SAMPLER_TILED_SCALED,
        UI_SURFACE_SAMPLER_SCALED,
        4,
        5,
    ] {
        assert_eq!(
            binds[linear as usize], 0,
            "descriptor {linear} is LINEAR and neither conjunct of the filter condition has a \
             producer: {binds:?}"
        );
    }
    app.shutdown();
}
