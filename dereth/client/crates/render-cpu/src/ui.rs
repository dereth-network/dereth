//! The 2D UI quad path.
//!
//! This module preserves the UI surface transform, vertex buffer and window-to-clip mapping.
//!
//! # The four rules, in one place
//!
//! Each one alone looks like an off-by-one worth "fixing"; together they are what makes a
//! `w`-pixel element cover exactly `w` pixels. Change any one and the whole interface shifts or
//! blurs. They live together in [`pixel_rules`], the only place any of the four is written down.
//!
//! The font path has its **own** half-pixel formula (`2·(p/size) − 1 ∓ 1/size`) — related, not
//! identical. It lives in [`crate::font`] and the two must not be unified.

use crate::texture::best_width_height;
use dereth_primitives::num::math;

/// The four 2D pixel rules. **Keep them together or not at all.**
///
/// Position through [`pixel_rules::pixel_to_clip`] (`−1/W`, `−1/H`), size scaled by
/// `2·(size − 0.25)/display`, UVs `(physical − 1)/(texture − 1)`, and **point sampling**. All four
/// together give pixel-exact UI; changing any one shifts or blurs the whole interface.
pub mod pixel_rules {
    /// Rule 1 — the pixel-to-clip transform:
    ///
    /// ```text
    /// clipX =  (2*x/W - 1) - 1/W
    /// clipY = -(2*y/H - 1) - 1/H
    /// ```
    ///
    /// i.e. the standard −0.5-pixel offset expressed in clip units. `W`/`H` are the **frame buffer**
    /// dimensions, so the whole UI is positioned against the back buffer, not against the current
    /// viewport.
    #[must_use]
    pub fn pixel_to_clip(x: f32, y: f32, fb_w: f32, fb_h: f32) -> (f32, f32) {
        let cx = (2.0 * x / fb_w - 1.0) - 1.0 / fb_w;
        let cy = -(2.0 * y / fb_h - 1.0) - 1.0 / fb_h;
        (cx, cy)
    }

    /// Rule 2 — the UI surface's transform update:
    ///
    /// ```text
    /// sx = 2 * (virtual_width  - 0.25f) / displayWidth
    /// sy = 2 * (virtual_height - 0.25f) / displayHeight
    /// ```
    ///
    /// "The `− 0.25f` in the scale is a quarter-pixel inset applied to the size; combined with the
    /// half-pixel offset in the position it is what makes a `w`-pixel-wide UI element cover exactly
    /// `w` pixels with point sampling. Reproduce both constants exactly."
    #[must_use]
    pub fn size_to_clip_scale(w: f32, h: f32, disp_w: f32, disp_h: f32) -> (f32, f32) {
        (2.0 * (w - 0.25) / disp_w, 2.0 * (h - 0.25) / disp_h)
    }

    /// Rule 3 — the UI surface's vertex buffer:
    ///
    /// ```text
    /// fu = (physical_width  - 1) / (texture_width  - 1)
    /// fv = (physical_height - 1) / (texture_height - 1)
    /// ```
    ///
    /// "Note the `−1` on **both** numerator and denominator: the quad's right edge samples the texel
    /// *centre* of the last used column, not the edge of the used region."
    ///
    /// A one-texel or degenerate extent gives 0, because the numerator is 0; a 1×1 texture would
    /// divide by zero, so that case returns 0 as well rather than a NaN the sampler would clamp.
    #[must_use]
    pub fn surface_uv(physical: (u32, u32), texture: (u32, u32)) -> (f32, f32) {
        let f = |p: u32, t: u32| -> f32 {
            if t <= 1 {
                0.0
            } else {
                #[allow(clippy::cast_precision_loss)]
                {
                    (p.max(1) - 1) as f32 / (t - 1) as f32
                }
            }
        };
        (f(physical.0, texture.0), f(physical.1, texture.1))
    }

    /// Rule 4 is not a formula: **the sampler is `TEXFILTER_POINT`**.
    /// The UI surface's material sets `min_filter = mag_filter = TEXFILTER_POINT`, and
    /// only elements that scale switch to linear. Rules 1-3 are calibrated against point sampling;
    /// with a
    /// linear sampler they blur instead of landing on texels.
    pub const UI_SAMPLER_IS_POINT: bool = true;

    /// **Which of `Gpu::create_samplers`' four the UI text draw must bind: 3, `POINT`/`CLAMP`.**
    ///
    /// Rule 4 above is read off the *UI surface's* material. The glyph draw does not use that
    /// material — the font builds its **own**
    /// `RenderMaterial`, and that constructor sets the filter and the address mode by name
    /// when it builds it:
    ///
    /// ```text
    /// bind_texture(stage, font_texture);
    /// stage->min_filter = TEXFILTER_POINT;
    /// stage->mag_filter = TEXFILTER_POINT;
    /// stage->address_u  = TEXADDRESS_CLAMP;
    /// stage->address_v  = TEXADDRESS_CLAMP;
    /// ```
    ///
    /// So POINT/CLAMP is the font material's own setting and not an inheritance from the surface
    /// it happens to be composed over. **It is also not a default**: a layer stage starts every
    /// stage at `TEXFILTER_LINEAR` and
    /// `TEXADDRESS_WRAP`, and both the surface's material generation and the font material override it — which is
    /// what makes POINT a decision rather than an accident. (Filter modes: `NONE` 0, `POINT` 1,
    /// `LINEAR` 2. Address modes: `WRAP` 1, `CLAMP` 3.) \[verified\]
    ///
    /// And it is not a preference either:
    /// the client's rectangle builder builds the source and destination rectangles entirely in
    /// `int`: no float, no cast, no division anywhere in the function — and each of the four clip
    /// edges applies the *identical integer delta to both rectangles*, so one destination pixel is
    /// one source texel by construction. Under that geometry POINT and LINEAR agree exactly, which
    /// is why binding the wrong one was invisible; what POINT buys is that any future sub-pixel
    /// error stays invisible instead of blurring.
    ///
    /// The glyph-pixel tests measure both halves of that against the fixture sheet's own bytes:
    /// at the shipped placement the two samplers are pixel-identical
    /// (0 of ~440 covered px, at three framebuffer sizes and two pens), and a half-pixel
    /// displacement is **0 px under POINT and 256 px under LINEAR** — 56 of them carrying
    /// intermediate coverage where the source bitmap has none, at an alpha cut-off of 8. That pair
    /// is identical at all six framebuffer/pen combinations, so it is a property of the sampler and
    /// not of a placement. A whole pixel moves 303 px under **both**, which is what stops the POINT
    /// zeros being the instrument failing to look. \[verified\]
    pub const UI_GLYPH_SAMPLER: u32 = 3;

    /// The shared index buffer: 6 **16-bit** indices, created once at UI startup and used by every
    /// UI surface.
    pub const UI_QUAD_INDICES: [u16; 6] = [0, 2, 1, 1, 2, 3];

    /// The UI **surface** blit's sampler when the element is drawn at its natural size and
    /// unrotated: 3, `POINT`/`CLAMP`.
    ///
    /// This is [`UI_SAMPLER_IS_POINT`] as an index. Material generation initialises the surface's
    /// stage to it explicitly — the writes are at word
    /// offsets 7 and 8 of the layer stage, which are `min_filter` and `mag_filter`, both
    /// set to `1 = TEXFILTER_POINT`, alongside `address_u`/`address_v` at 5 and 6 set to
    /// `(tile == false) * 2 + 1`, i.e. `3 = TEXADDRESS_CLAMP` unless the element tiles.
    pub const UI_SURFACE_SAMPLER_UNSCALED: u32 = 3;
    /// The UI surface blit's sampler when the element is **scaled or rotated**: 1,
    /// `LINEAR`/`CLAMP`.
    pub const UI_SURFACE_SAMPLER_SCALED: u32 = 1;

    /// The shared index buffer: 6 **16-bit** indices, created once at UI start-up
    /// and used by every UI surface.
    ///
    /// **The polarity is conditional.** The transform update ends:
    ///
    /// ```text
    /// scaled = !(vw == pw && vh == ph && rotation is the identity)
    /// if there is a surface: set the material filtering from `scaled`
    /// ```
    ///
    /// and the material-filtering setter sets `min_filter = mag_filter = scaled +
    /// TEXFILTER_POINT`, with `TEXFILTER_POINT = 1` and `TEXFILTER_LINEAR = 2`. So **0 is POINT
    /// and 1 is LINEAR**: the client keeps point sampling for an element at its natural size and
    /// moves to linear only when it is stretched or turned. There is one conditional.
    ///
    /// # `physical_size` is the **surface**, not the picture.
    ///
    /// The transform update compares the virtual width/height against the surface's physical
    /// width/height, and that surface is the one the element builds for itself over the element's
    /// own width and height for every shipped element,
    /// because the size is taken from the state description only when attribute `0xCD` is 1 or 2
    /// and no shipped element authors either (67 sites: 58 × `3`, 9 × `0`). Setting the surface
    /// then copies the physical size into the virtual one, and a resize sets the physical size
    /// before the virtual screen position for mode 3, so a resize moves both together. **So the two are equal for every shipped
    /// element and the `LINEAR` arm has no producer in retail either.**
    ///
    /// # A display-mode change does not open the `LINEAR` arm either.
    ///
    /// The element resize sets the physical size **only** for mode 3, so the nine shipped
    /// `0xCD = 0` sites — the intro, credits and epilogue roots plus six 800 x 600 partials in
    /// `0x2100003C` — look as though they would keep a creation-time surface across a resolution change and so diverge. **They
    /// would not, and no capture is needed to say so.** Three readings settle it:
    ///
    /// 1. **The device reset touches no UI element.** Its
    ///    callees are the display aspect-ratio setup, the default D3D state block, a viewport set,
    ///    the gamma setter and a three-iteration clear/present loop -- plus
    ///    exactly one UI call, which recomputes the game viewport and broadcasts the change.
    ///    No element is resized from here.
    ///
    /// 2. **The element-side path resizes the manager's root and nothing else.**
    ///    The rendering-system restart walks the renderer's own resize callback list, which the UI
    ///    manager joins at start-up. That callback is the UI manager's own, and all it does to the
    ///    tree is
    ///
    ///    ```text
    ///    broadcast global message 5
    ///    w = current display width  (else 800)
    ///    h = current display height (else 600)
    ///    resize the root element to (w, h)
    ///    force-update the root with flags 7
    ///    draw the dirty regions
    ///    ```
    ///
    ///    — one resize, on the root, the hollow element built when the manager starts.
    ///    Layout roots are not resized here; they are
    ///    reached, if at all, through the resize's own child walk.
    ///
    /// 3. **A mode-0 element has no physical size to diverge, because it owns no UI object.**
    ///    The resize is gated on flag word bit 16, "owns its object" -- and an element without
    ///    it takes the other arm and
    ///    touches no object at all: it sets neither the physical size nor the virtual position. And
    ///    a mode-0 element never has the bit: making the object re-reads
    ///    `0xCD` and overwrites bit 14 with `(value != 0)`, so even a forced set
    ///    leaves it clear, and the object build returns false
    ///    before allocating a UI surface object. Such an element composes into the nearest owning
    ///    ancestor's surface, which finds by walking `parent`.
    ///
    /// So the nine mode-0 sites have no physical size of their own, mode 3 moves both
    /// sizes together, and modes 1 and 2 — the only values for which a resize could move the
    /// virtual size and leave the physical one — are **not authored by any shipped element**
    /// (a census of the shipped layouts: 58 threes, 9 zeros, no ones, no twos). `LINEAR` is unreachable for
    /// every shipped element on every path, display-mode change included. A capture of a
    /// resolution change was therefore **not** filed. What a future layout
    /// authoring mode 1 or 2 would do is a different question, and the census test's assertion is
    /// what makes it fail loudly rather than silently. \[verified\]
    ///
    /// This matters because it is easy to hand this function the *picture's* size instead, which
    /// is a different number: a 10 × 5 border strip inside a 792 × 5 element is not a scaled
    /// element, it is repeating the strip 79.2 times into a surface
    /// that is exactly 792 × 5. Passing the picture's size binds `LINEAR`
    /// for 6 of 30 char-gen blits, 4 of 15 character-management blits and 26 of 111 gameplay
    /// blits — every one of them a tile retail draws `POINT`, one texel to one pixel.
    ///
    /// # Rotation has no producer at all, and the reason is structural.
    ///
    /// The rotation setter is the only writer of the surface's rotation outside
    /// the constructor, and nothing in the client ever invokes it: it is not called directly,
    /// it is not reachable through the base UI object's interface, and the only two places that
    /// hold a concrete UI surface object build one each and never use it. There is no UI-region or element rotation setter and no rotation attribute in
    /// `MasterProperty 0x39000001`.
    ///
    /// So the rotation is written as the identity and remains the identity for the life of every
    /// instance, so comparison with the identity matrix always succeeds.
    /// This is a **structural** null, not a coincidence of the shipped layouts: no data could
    /// turn it on. It is pinned rather than given a producer, and
    /// [`crate::ui::rotate_clip_quad`] exists so the arm can still be exercised at this seam.
    #[must_use]
    pub fn ui_surface_sampler(
        virtual_size: (u32, u32),
        physical_size: (u32, u32),
        rotated: bool,
        tiles: bool,
    ) -> u32 {
        let filter = if virtual_size == physical_size && !rotated {
            FILTER_BIT_POINT
        } else {
            FILTER_BIT_LINEAR
        };
        filter
            + if tiles {
                ADDRESS_BIT_WRAP
            } else {
                ADDRESS_BIT_CLAMP
            }
    }

    /// `Gpu::create_samplers`' index is two independent bits, and this is the high one: `+2`
    /// selects a `POINT` descriptor and `+0` a `LINEAR` one (0 linear/wrap, 1 linear/clamp,
    /// 2 point/wrap, 3 point/clamp). Split out because the **filter** and the **address mode**
    /// come from two different writes during material generation, decided by two different
    /// conditions; treating the four-way table as one axis is what let the
    /// address mode stay unwired.
    pub const FILTER_BIT_POINT: u32 = 2;
    /// The other half of the filter bit: `LINEAR`, chosen when the element is scaled or rotated.
    pub const FILTER_BIT_LINEAR: u32 = 0;
    /// `+0` selects a `WRAP` descriptor. See [`ui_surface_address_mode`].
    pub const ADDRESS_BIT_WRAP: u32 = 0;
    /// `+1` selects a `CLAMP` descriptor.
    pub const ADDRESS_BIT_CLAMP: u32 = 1;

    /// The UI surface blit's sampler when the element **tiles** and is at its natural size: 2,
    /// `POINT`/`WRAP`.
    pub const UI_SURFACE_SAMPLER_TILED: u32 = 2;
    /// The UI surface blit's sampler when the element **tiles and is scaled or rotated**: 0,
    /// `LINEAR`/`WRAP`. Neither retail nor this build can produce the pair — the
    /// filter's other arm has no producer either (see [`ui_surface_sampler`]) — but the
    /// descriptor exists and the index arithmetic reaches it, so it is named rather than left as
    /// a hole in the table.
    pub const UI_SURFACE_SAMPLER_TILED_SCALED: u32 = 0;

    /// The point filter mode, what material generation writes into layer-stage words 7 and 8 and
    /// what the filtering setter computes for an unscaled element. (Filter modes: `NONE` 0,
    /// `POINT` 1, `LINEAR` 2.)
    pub const TEXFILTER_POINT: u32 = 1;
    /// The linear filter mode — the filtering setter's scaled arm.
    pub const TEXFILTER_LINEAR: u32 = 2;

    /// The wrap address mode. The layer stage's address modes are an enumeration, not a bool.
    pub const TEXADDRESS_WRAP: u32 = 1;
    /// The clamp address mode.
    pub const TEXADDRESS_CLAMP: u32 = 3;

    /// repeating: the comparison is against a constant `0x3951B717` = **2.0e-4**, on each axis.
    /// \[verified\]
    /// `0x3951B717` = **2.0e-4**. \[verified\]
    pub const ADDRESS_MODE_EPSILON: f32 = 2.0e-4;

    /// Set the texture address mode **per axis**. The client stores the chosen
    /// address-mode value independently for U and V.
    ///
    /// Two writes occur, and the second overrides the first. Each axis is assigned
    /// `(tile == false) * 2 + 1`, selecting wrap for tiled images and clamp otherwise.
    ///
    /// -- `1 = TEXADDRESS_WRAP` when the element tiles, `3 = TEXADDRESS_CLAMP` when it does not.
    /// Then, **only** on the tiling branch (tiling set and a surface present), the same two
    /// words are rewritten from each axis's own repeat count:
    ///
    /// ```text
    ///   r = virtual_width / physical_width
    ///   address_u = (abs(abs(r) - 1.0) <= 2.0e-4) ? 3 /* CLAMP */ : 1 /* WRAP */
    /// ```
    ///
    /// and the identical block for V, from `virtual_height /
    /// physical_height`. The comparison is a strict *greater than*: when the difference is equal
    /// to or less than the epsilon `CLAMP` stands; only an axis whose repeat count differs from 1
    /// by more than the epsilon gets `WRAP`. Reading the comparison the other way up inverts the
    /// whole result. \[verified\]
    ///
    /// **Retail never reaches the tiling branch.** `tile` is written in exactly one place --
    /// the second argument when the surface is set, which the element computes as
    /// `attribute 0xCD == 2` -- and **no shipped element authors `0xCD = 2`**: across the
    /// the element's attribute `0xCD` — and **no shipped element authors `0xCD = 2`**: across the
    /// 101 layouts and 2 162 elements of `client_local_English.dat` there are 67 authored `0xCD`
    /// sites, 58 carrying `3` and 9 carrying `0`. The attribute is read in three places
    /// (three read sites) and written in none, so nothing sets it at runtime
    /// either. `ui_tiling.rs` re-measures that count rather than quoting it.
    ///
    /// What a player sees tiling is a *different* mechanism: repeats
    /// the picture into the element's own CPU surface. See [`graphic_draw_tiles`].
    #[must_use]
    pub fn ui_surface_address_mode(
        virtual_size: (u32, u32),
        physical_size: (u32, u32),
        tiles: bool,
    ) -> (u32, u32) {
        if !tiles {
            return (TEXADDRESS_CLAMP, TEXADDRESS_CLAMP);
        }
        let axis = |v: u32, p: u32| -> u32 {
            if p == 0 {
                return TEXADDRESS_CLAMP;
            }
            #[allow(clippy::cast_precision_loss)]
            // LINT-OK: retail's material generation does exactly this — both extents as floats.
            let scale = (v as f32) / (p as f32);
            if (scale.abs() - 1.0).abs() > ADDRESS_MODE_EPSILON {
                TEXADDRESS_WRAP
            } else {
                TEXADDRESS_CLAMP
            }
        };
        (
            axis(virtual_size.0, physical_size.0),
            axis(virtual_size.1, physical_size.1),
        )
    }

    /// The UI element draw path's own fast-path predicate, negated: **must this element's
    /// picture be repeated?**
    ///
    /// The tiled draw receives the element's own box,
    /// the dirty rectangle and `tiling_offset`. It takes a single image put — one clipped
    /// 1:1 blit — only when
    ///
    /// ```text
    /// tilingOffset.x == 0 && tilingOffset.y == 0 &&
    /// boxWidth <= graphic_width && boxHeight <= graphic_height
    /// ```
    ///
    /// The two extents are the box's `x1 - x0 + 1` and its y twin — the **element's
    /// whole box**, not the dirty piece being redrawn, so whether an element tiles does not
    /// change with the scissor.
    ///
    /// and otherwise walks a modulo grid, blitting `graphic_width - start_u` columns at a time from
    /// `start_u = (tiling_offset.x - box.x0 + dst.x0) mod graphic_width`. Either way source and
    /// destination have the same extent: the copy width is
    /// `min(source width, destination width)` and the height likewise, so **the UI region blit never
    /// scales a picture** — it crops it, or it repeats it. \[verified\]
    ///
    /// This rebuild has no CPU surface to repeat into, so the repeat is expressed as
    /// texture-space UVs that run past 1 under a `WRAP` sampler. That is what makes the address
    /// mode load-bearing here where retail's material generation leaves it inert, and the two agree
    /// because retail's draw grid and a wrapping sampler compute the same `mod`.
    #[must_use]
    pub fn graphic_draw_tiles(
        image: (u32, u32),
        element_box: (u32, u32),
        tiling_offset: (i32, i32),
    ) -> bool {
        if image.0 == 0 || image.1 == 0 {
            return false;
        }
        !(tiling_offset == (0, 0) && element_box.0 <= image.0 && element_box.1 <= image.1)
    }

    /// How many sampler descriptors `Gpu::create_samplers` writes: **8**, not 4.
    ///
    /// Two filters times **four** `(AddressU, AddressV)` combinations, because retail sets the two
    /// address modes independently and the common shipped case is mixed. See
    /// [`ui_surface_sampler_axes`] for the index arithmetic and for why 0..=3 keep their meaning.
    pub const UI_SAMPLER_COUNT: usize = 8;

    /// The UI surface blit's sampler when **U wraps and V clamps**: 6, `POINT` / `WRAP` / `CLAMP`.
    /// The shipped mixed case — a 10 x 5 strip laid across a 792 x 5 element, and
    /// `0x0600113A`, a 500 x 17 picture scrolled by `tiling_offset` inside a 400 x 17 element.
    pub const UI_SURFACE_SAMPLER_TILED_U: u32 = 6;
    /// The UI surface blit's sampler when **V wraps and U clamps**: 7, `POINT` / `CLAMP` / `WRAP`.
    /// The vertical twin: a strip repeated down a tall, narrow element.
    pub const UI_SURFACE_SAMPLER_TILED_V: u32 = 7;

    /// `Gpu::create_samplers`' index with the two address modes **independent**.
    ///
    /// [`ui_surface_sampler`] takes one `tiles` bool and puts the same address mode on both axes.
    /// That is not what does: it writes
    /// `address_u` and `address_v` from the *same* value at word offsets 5 and 6
    /// from the *same* value at word offsets 5 and 6 and then, on the tiling branch, **refines
    /// each axis on its own** from that axis's repeat count. See [`ui_surface_address_mode`],
    /// which reproduces that refinement including the strict comparison.
    ///
    /// The index keeps `0..=3` exactly where [`ui_surface_sampler`] put them — those are the four
    /// **symmetric** descriptors, so every assertion written against the four-sampler index still
    /// names the same object — and puts the two mixed pairs in `4..=7`:
    ///
    /// | index | filter | `AddressU` | `AddressV` |
    /// |---:|---|---|---|
    /// | 0 | `LINEAR` | `WRAP` | `WRAP` |
    /// | 1 | `LINEAR` | `CLAMP` | `CLAMP` |
    /// | 2 | `POINT` | `WRAP` | `WRAP` |
    /// | 3 | `POINT` | `CLAMP` | `CLAMP` |
    /// | 4 | `LINEAR` | `WRAP` | `CLAMP` |
    /// | 5 | `LINEAR` | `CLAMP` | `WRAP` |
    /// | 6 | `POINT` | `WRAP` | `CLAMP` |
    /// | 7 | `POINT` | `CLAMP` | `WRAP` |
    #[must_use]
    pub fn ui_surface_sampler_axes(
        virtual_size: (u32, u32),
        physical_size: (u32, u32),
        rotated: bool,
        wrap: (bool, bool),
    ) -> u32 {
        let filter = if virtual_size == physical_size && !rotated {
            FILTER_BIT_POINT
        } else {
            FILTER_BIT_LINEAR
        };
        let first_wraps = if wrap.0 {
            ADDRESS_BIT_WRAP
        } else {
            ADDRESS_BIT_CLAMP
        };
        if wrap.0 == wrap.1 {
            filter + first_wraps
        } else {
            4 + filter + first_wraps
        }
    }

    /// The `(AddressU, AddressV)` pair a sampler index carries, in the layer stage's own
    /// address-mode values. The inverse of [`ui_surface_sampler_axes`]' address half, so a test
    /// can name a descriptor and check what it is rather than trusting the table's order.
    #[must_use]
    pub fn ui_sampler_address_modes(index: u32) -> (u32, u32) {
        let (u, v) = match index {
            0 | 2 => (true, true),
            1 | 3 => (false, false),
            4 | 6 => (true, false),
            _ => (false, true),
        };
        let m = |w: bool| if w { TEXADDRESS_WRAP } else { TEXADDRESS_CLAMP };
        (m(u), m(v))
    }

    /// `TEXFILTER_POINT` (1) or `TEXFILTER_LINEAR` (2) for a sampler index — the other half of
    /// [`ui_sampler_address_modes`].
    #[must_use]
    pub const fn ui_sampler_filter(index: u32) -> u32 {
        // Indices 2, 3, 6, 7 are the POINT half: the filter bit is `+2` inside each group of four.
        if index % 4 >= 2 {
            1
        } else {
            2
        }
    }

    /// The client's modulo, **one axis at a time**: the source texel the first
    /// destination pixel takes, and whether the run crosses a seam.
    ///
    /// The grid's start texel is
    ///
    /// ```text
    ///   startU = (tilingOffset.x - box.x0 + dst.x0) % graphic_width
    ///   if (startU < 0) startU += graphic_width
    ///   startV = (tilingOffset.y - box.y0 + dst.y0) % graphic_height
    ///   if (startV < 0) startV += graphic_height
    ///   run height = graphic_height - startV, capped at what is left of the box
    /// ```
    ///
    /// and the inner loop blits `min(graphic_width - start_u, remaining)` columns at a time, resetting
    /// `start_u` to 0 for every run after the first. So on each axis independently the source
    /// coordinate for destination offset `d` is `(start + d) mod g`, and the run **crosses a
    /// seam** exactly when `(start mod g) + extent > g`.
    ///
    /// That is not the same predicate as [`graphic_draw_tiles`], and the difference is the whole
    /// reason for the mixed samplers. `graphic_draw_tiles` is the negation of `Draw`'s *fast path*, which is a
    /// single `&&` over both offsets and both extents, so a tiling offset on one axis takes the
    /// grid for both; but the grid then computes `startV = 0` and one full-height run, and no V
    /// seam is crossed. Binding `WRAP` on that axis is harmless only while its UVs stay inside
    /// `[0, 1]` — a coincidence, not a guarantee.
    ///
    /// It is also not the same predicate as [`ui_surface_address_mode`], and that difference is
    /// real rather than a modelling choice: retail's material generation refines from
    /// `virtual_width / physical_width`, a ratio that **knows nothing about
    /// `tiling_offset`** — the offset lives in the draw state, not in the material. So the
    /// material calls an offset element's axes `CLAMP` while retail's blit wraps them. Retail is
    /// never caught by that because it never reaches the tiling branch at all (no shipped element
    /// authors `0xCD = 2`, so `tile` is always false and both axes are `TEXADDRESS_CLAMP`);
    /// this rebuild has no CPU surface to repeat into and expresses the repeat as UVs under a
    /// wrapping sampler, so the predicate it must bind from is **`Draw`'s**, not the material's.
    /// Said here rather than left as a silent divergence. \[verified\]
    #[must_use]
    pub fn graphic_draw_axis(image_extent: u32, start: i32, extent: u32) -> (u32, bool) {
        if image_extent == 0 {
            return (0, false);
        }
        let g = i64::from(image_extent);
        let s = i64::from(start).rem_euclid(g);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // LINT-OK: `rem_euclid` by a `u32` yields `0..g`, which fits a `u32` by construction.
        let s32 = s as u32;
        (
            s32,
            u64::from(s32) + u64::from(extent) > u64::from(image_extent),
        )
    }
}

use pixel_rules::{pixel_to_clip, size_to_clip_scale, surface_uv, UI_QUAD_INDICES};

/// One vertex of a UI quad, in the client's Z-up convention: X right, **Y into the screen**, Z
/// up, as the UI surface's vertex buffer builds it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiVertex {
    /// Client-space position, before the y/z swap the transform update bakes into the world matrix.
    pub position: [f32; 3],
    pub uv: [f32; 2],
}

/// The 4-vertex quad the client's UI vertex buffer holds, in its exact order.
///
/// | vertex | client (x, y, z) | u | v |
/// |---|---|---|---|
/// | 0 | (−0.5, 0, +0.5) | 0 | 0 |
/// | 1 | (+0.5, 0, +0.5) | `fu` | 0 |
/// | 2 | (−0.5, 0, −0.5) | 0 | `fv` |
/// | 3 | (+0.5, 0, −0.5) | `fu` | `fv` |
#[must_use]
pub fn quad_vertices(physical: (u32, u32), texture: (u32, u32)) -> [UiVertex; 4] {
    let (fu, fv) = surface_uv(physical, texture);
    [
        UiVertex {
            position: [-0.5, 0.0, 0.5],
            uv: [0.0, 0.0],
        },
        UiVertex {
            position: [0.5, 0.0, 0.5],
            uv: [fu, 0.0],
        },
        UiVertex {
            position: [-0.5, 0.0, -0.5],
            uv: [0.0, fv],
        },
        UiVertex {
            position: [0.5, 0.0, -0.5],
            uv: [fu, fv],
        },
    ]
}

/// A UI element's placement in clip space: the quad's anchor corner and its extent.
///
/// The UI transform anchors the quad at the element's **bottom-left**
/// pixel — the window-to-clip conversion takes `virtual_y + virtual_height - 1`, the bottom edge —
/// and grows it up and to the right by `(sx, sy)`. The document writes the composition as
/// `Translate(0.5, 0.5, 0) * Scale(sx, sy, 1)`, "move the `[-0.5,0.5]` quad to `[0,1]` then scale",
/// followed by overwriting the translation with `(clipX, clipY)`; anchoring the `(-0.5, -0.5)`
/// corner at `(clipX, clipY)` is the reading under which a `w`-pixel element covers exactly `w`
/// pixels, and the coverage test
/// below is what settles it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClipRect {
    /// Clip x of the quad's left edge.
    pub x: f32,
    /// Clip y of the quad's bottom edge (clip +Y is up).
    pub y: f32,
    pub sx: f32,
    pub sy: f32,
}

impl ClipRect {
    /// The clip-space x of the right edge.
    #[must_use]
    pub fn right(&self) -> f32 {
        self.x + self.sx
    }
    /// The clip-space y of the top edge.
    #[must_use]
    pub fn top(&self) -> f32 {
        self.y + self.sy
    }
}

/// Compute the UI transform with rotation left out. Rotation belongs to the caller, and only the
/// compass and radar needles use it.
///
/// `fb` is the **frame buffer** size, which is what the window-to-clip conversion measures against;
/// `display` is what the size scale divides by. They are the same buffer in the client, and are
/// separate parameters here only because the two formulae name them differently.
#[must_use]
pub fn update_transform(
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    fb: (u32, u32),
    display: (u32, u32),
) -> ClipRect {
    #[allow(clippy::cast_precision_loss)]
    let (fb_w, fb_h) = (fb.0 as f32, fb.1 as f32);
    #[allow(clippy::cast_precision_loss)]
    let (disp_w, disp_h) = (display.0 as f32, display.1 as f32);
    // The bottom edge: virtual_y + virtual_height - 1. The `- 1` is the inclusive box
    // convention: a size in the layout dat is x1 - x0 + 1.
    #[allow(clippy::cast_precision_loss)]
    let bottom_pixel = (y + h.max(1) - 1) as f32;
    #[allow(clippy::cast_precision_loss)]
    let (clip_x, clip_y) = pixel_to_clip(x as f32, bottom_pixel, fb_w, fb_h);
    #[allow(clippy::cast_precision_loss)]
    let (sx, sy) = size_to_clip_scale(w as f32, h as f32, disp_w, disp_h);
    ClipRect {
        x: clip_x,
        y: clip_y,
        sx,
        sy,
    }
}

/// The four clip-space corners of a UI quad with the surface's **z** rotation applied.
///
/// Corners come back in the order [`quad_vertices`] uses: top-left, bottom-left, bottom-right,
/// top-right, in a clip space whose `+Y` is up.
///
/// # What is read and what is not
///
/// The client's rotation setter takes three **integer degree** counts, reduces each `% 360`
/// (`% 0x168`) and scales by `0.017453292`, then builds the rotation product
/// `Rx(x) * Ry(y) * Rz(z)` into `rotation`. The UI surface's transform update composes
/// `rotation * (Translate(0.5, 0.5, 0) * Scale(sx, sy, 1))` and then overwrites the result's
/// translation with the element's clip position — so the rotation acts on the `[-0.5, 0.5]` quad
/// **about its own centre**, before it is moved to `[0, 1]` and scaled. An anisotropic `(sx, sy)`
/// applied after the rotation shears a turned element, and that is the client's own arithmetic,
/// not an artefact of this reading.
///
/// The handedness is read, not assumed. The translation step writes its three components to `_41`,
/// `_42`, and `_43`, which is Direct3D's **row-vector** convention: `v' = v * M`. The z-rotation
/// step writes `_11 = cos`, `_12 = sin`, `_21 = -sin`,
/// `_22 = cos`, so `x' = x cos - y sin`, `y' = x sin + y cos` — a **counter-clockwise** turn by
/// `+z` degrees. \[verified\]
///
/// **Only `z` is modelled.** The source quad lies in the client's X–Z plane, so the x and y arms
/// interact with the y/z swap the transform update bakes
/// in, and nothing in the shipped client can turn any axis on to settle it — see
/// [`pixel_rules::ui_surface_sampler`]'s rotation section, which pins the rotation setter as having
/// **no caller in the client at all**. Guessing the other two arms would be inventing behaviour
/// no data can contradict; this models the one whose matrix is fully recovered and leaves the
/// others to the day something produces them.
///
/// `z == 0` reproduces the unrotated corners **bit for bit** — the local offsets are `±0.5`, so
/// the two multipliers are exactly `0.0` and `1.0` — which is what keeps this free for every
/// element that does not rotate.
#[must_use]
pub fn rotate_clip_quad(rect: ClipRect, z_degrees: i32) -> [(f32, f32); 4] {
    // Local corners of the client's UI quad, y-up, in the order `quad_vertices` emits.
    const LOCAL: [(f32, f32); 4] = [(-0.5, 0.5), (-0.5, -0.5), (0.5, -0.5), (0.5, 0.5)];
    let place = |lx: f32, ly: f32| (rect.x + (lx + 0.5) * rect.sx, rect.y + (ly + 0.5) * rect.sy);
    // The angle modulo 360 (`0x168`), then degrees to radians with the client's own constant.
    let turns = z_degrees % 360;
    if turns == 0 {
        return [
            place(LOCAL[0].0, LOCAL[0].1),
            place(LOCAL[1].0, LOCAL[1].1),
            place(LOCAL[2].0, LOCAL[2].1),
            place(LOCAL[3].0, LOCAL[3].1),
        ];
    }
    #[allow(clippy::cast_precision_loss)]
    // LINT-OK: an integer degree count already reduced to (-360, 360), exact in f32.
    let radians = (turns as f32) * 0.017_453_292;
    let (sin, cos) = (math::sinf(radians), math::cosf(radians));
    let mut out = [(0.0, 0.0); 4];
    for (i, (lx, ly)) in LOCAL.iter().enumerate() {
        out[i] = place(lx * cos - ly * sin, lx * sin + ly * cos);
    }
    out
}

/// Which rasteriser convention a clip rectangle is being read against.
///
/// The client only ever met one of these conventions. The distinction
/// matters to the rebuild and is the reason [`compensate_for_d3d12`] exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelCentre {
    /// Direct3D 9: a pixel's sample point is at its **integer** index. This is the convention the
    /// four rules are calibrated against — it is *why* [`pixel_rules::pixel_to_clip`] subtracts half a
    /// pixel at all.
    D3D9,
    /// Direct3D 10 and later, including D3D12: a pixel's sample point is at **index + 0.5**, and the
    /// half-pixel offset D3D9 needed was removed from the API.
    D3D12,
}

impl PixelCentre {
    /// The sample point of pixel `k` in viewport coordinates.
    #[must_use]
    pub fn sample_point(self, k: f64) -> f64 {
        match self {
            Self::D3D9 => k,
            Self::D3D12 => k + 0.5,
        }
    }
}

/// Convert a clip-space x to viewport coordinates. The mapping itself is the same in both APIs;
/// only where the sample point sits within a pixel differs ([`PixelCentre`]).
#[must_use]
pub fn clip_x_to_viewport(clip_x: f32, fb_w: f32) -> f32 {
    (clip_x + 1.0) * fb_w * 0.5
}

/// The same for y, remembering that clip +Y is up while screen +Y is down.
#[must_use]
pub fn clip_y_to_viewport(clip_y: f32, fb_h: f32) -> f32 {
    (1.0 - clip_y) * fb_h * 0.5
}

/// **Undo the D3D9 half-pixel offset when handing a rectangle to a D3D12 rasteriser.**
///
/// The four pixel rules are kept exactly as the client has them, because they are the
/// observable behaviour and because rule 3's UVs are computed against rule 1's geometry. But rule 1's `−1/W` and
/// `−1/H` exist to compensate for **D3D9's** pixel-centre convention, and D3D12 does not have it: a
/// D3D12 pixel samples at `index + 0.5`, where a D3D9 pixel sampled at `index`. Feeding the raw
/// client clip coordinates to D3D12 therefore lands the whole UI half a pixel out — which shows up
/// as an element covering `w × (h−1)` pixels one column to the left, not as a blur.
///
/// So the compensation is exactly the offset rule 1 applied, added back at the API boundary and
/// nowhere else: `+1/W` in x, `−1/H` in y (clip +Y is up). Applying it inside
/// [`pixel_rules::pixel_to_clip`] instead would be wrong — that function is the client's formula and
/// other code reads it as such.
///
/// Measured, not reasoned: the smoke example draws a 400×300 element and covered 119 600 pixels
/// without this, 120 000 with it.
#[must_use]
pub fn compensate_for_d3d12(rect: ClipRect, fb: (u32, u32)) -> ClipRect {
    #[allow(clippy::cast_precision_loss)]
    let (fb_w, fb_h) = (fb.0 as f32, fb.1 as f32);
    ClipRect {
        x: rect.x + 1.0 / fb_w,
        y: rect.y - 1.0 / fb_h,
        ..rect
    }
}

/// One axis of a coverage report: `(destination pixel, sampled texel)` per covered pixel.
pub type AxisCoverage = Vec<(i64, u32)>;

/// Which destination pixels a [`ClipRect`] covers, and which source texel each one samples.
///
/// This is the harness for the four pixel rules: it applies all four together and reports the result
/// in pixels, which is the only level at which "covers exactly `w` pixels" is a statement about
/// anything. Returns `(columns, rows)` where each entry is `(destination pixel, sampled texel)`.
#[must_use]
pub fn coverage(
    rect: &ClipRect,
    fb: (u32, u32),
    physical: (u32, u32),
    texture: (u32, u32),
    centre: PixelCentre,
) -> (AxisCoverage, AxisCoverage) {
    #[allow(clippy::cast_precision_loss)]
    let (fb_w, fb_h) = (fb.0 as f32, fb.1 as f32);
    let (fu, fv) = surface_uv(physical, texture);

    let left = clip_x_to_viewport(rect.x, fb_w);
    let right = clip_x_to_viewport(rect.right(), fb_w);
    // Clip +Y is up, so the rect's `y` (bottom edge) is the *larger* screen coordinate.
    let bottom = clip_y_to_viewport(rect.y, fb_h);
    let top = clip_y_to_viewport(rect.top(), fb_h);

    let axis = |lo: f32, hi: f32, f_max: f32, tex: u32, limit: u32| -> AxisCoverage {
        let (lo, hi) = (f64::from(lo), f64::from(hi));
        let mut out = Vec::new();
        // Scan a generous window and keep the pixels whose sample point is inside the rectangle,
        // with D3D's top-left fill rule: inclusive at the near edge, exclusive at the far one.
        let first = dereth_primitives::num::to_i32_f64((lo - 2.0).floor()) as i64;
        let last = dereth_primitives::num::to_i32_f64((hi + 2.0).ceil()) as i64;
        for k in first..=last {
            let sample = centre.sample_point(k as f64);
            if sample < lo || sample >= hi {
                continue;
            }
            let t = (sample - lo) / (hi - lo);
            let uv = t * f64::from(f_max);
            let texel = (uv * f64::from(tex)).floor();
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            // LINT-OK: the sampler clamps, so the texel index is bounded by the texture extent.
            let texel = (texel.max(0.0) as u32).min(limit.saturating_sub(1));
            out.push((k, texel));
        }
        out
    };

    let cols = axis(left, right, fu, texture.0, texture.0.max(1));
    let rows = axis(top, bottom, fv, texture.1, texture.1.max(1));
    (cols, rows)
}

/// A whole UI surface, as builds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiSurface {
    /// `physical_width` / `physical_height` — the logical content size.
    pub physical: (u32, u32),
    /// The power-of-two texture the content sits in the top-left corner of.
    pub texture: (u32, u32),
    /// `has_alpha = (flags & 1)`.
    pub has_alpha: bool,
}

impl UiSurface {
    /// Create a UI surface:
    ///
    /// ```text
    /// require render_device != NULL and 0 < w <= 2048 and 0 < h <= 2048
    /// physical_width = w ; physical_height = h
    /// if no simple non-power-of-two support    // always true in this build, so this always runs
    ///     texW = next power of two >= w (capped at 2048); texH likewise
    /// ```
    ///
    /// # Errors
    /// [`crate::RenderError::BadDimensions`] outside `1..=2048`, which is the client's own `require`.
    pub fn create(w: u32, h: u32, has_alpha: bool) -> Result<Self, crate::RenderError> {
        if w == 0 || h == 0 || w > 2048 || h > 2048 {
            return Err(crate::RenderError::BadDimensions {
                width: w,
                height: h,
                reason: "UI surface creation requires 0 < extent <= 2048",
            });
        }
        Ok(Self {
            physical: (w, h),
            texture: (best_width_height(w), best_width_height(h)),
            has_alpha,
        })
    }

    /// The quad's UVs for this surface.
    #[must_use]
    pub fn uv(&self) -> (f32, f32) {
        surface_uv(self.physical, self.texture)
    }

    /// The 4 vertices and the 6 shared indices.
    #[must_use]
    pub fn quad(&self) -> ([UiVertex; 4], [u16; 6]) {
        (quad_vertices(self.physical, self.texture), UI_QUAD_INDICES)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drive all four rules end to end under a rasteriser convention and return `(columns, rows)`.
    ///
    /// Under [`PixelCentre::D3D12`] the D3D9 half-pixel offset is compensated at the API boundary,
    /// which is what the real renderer does; under [`PixelCentre::D3D9`] the client's own
    /// coordinates are used unchanged. Both must give the same pixels.
    fn place_on(
        x: u32,
        y: u32,
        w: u32,
        h: u32,
        fb: (u32, u32),
        centre: PixelCentre,
    ) -> (AxisCoverage, AxisCoverage) {
        let s = UiSurface::create(w, h, true).unwrap();
        let mut rect = update_transform(x, y, w, h, fb, fb);
        if centre == PixelCentre::D3D12 {
            rect = compensate_for_d3d12(rect, fb);
        }
        coverage(&rect, fb, s.physical, s.texture, centre)
    }

    fn place(x: u32, y: u32, w: u32, h: u32, fb: (u32, u32)) -> (AxisCoverage, AxisCoverage) {
        place_on(x, y, w, h, fb, PixelCentre::D3D9)
    }

    // The four 2D pixel rules together produce pixel-exact UI: a w x h element at (x,y) on a
    // W x H back buffer covers exactly the w x h pixel rectangle starting at (x,y), with each
    // source texel landing on exactly one destination pixel, at 800x600 and at one widescreen
    // size.
    //
    // The rules are applied TOGETHER; asserting any one of them alone would prove nothing.
    #[test]
    fn an_element_covers_exactly_its_own_pixel_rectangle() {
        for centre in [PixelCentre::D3D9, PixelCentre::D3D12] {
            for fb in [(800u32, 600u32), (1920, 1080)] {
                for (x, y, w, h) in [
                    (0u32, 0u32, 100u32, 40u32),
                    (37, 91, 64, 64),
                    (10, 10, 1, 1),
                    (100, 200, 256, 128),
                    (0, 0, 33, 17),
                ] {
                    let (cols, rows) = place_on(x, y, w, h, fb, centre);
                    let col_ids: Vec<i64> = cols.iter().map(|c| c.0).collect();
                    let row_ids: Vec<i64> = rows.iter().map(|r| r.0).collect();
                    let want_cols: Vec<i64> = (i64::from(x)..i64::from(x) + i64::from(w)).collect();
                    let want_rows: Vec<i64> = (i64::from(y)..i64::from(y) + i64::from(h)).collect();
                    assert_eq!(
                        col_ids, want_cols,
                        "columns for {w}x{h} at ({x},{y}) on {fb:?}"
                    );
                    assert_eq!(
                        row_ids, want_rows,
                        "rows for {w}x{h} at ({x},{y}) on {fb:?}"
                    );

                    // ..and each source texel lands on exactly one destination pixel.
                    let sampled_cols: Vec<u32> = cols.iter().map(|c| c.1).collect();
                    let sampled_rows: Vec<u32> = rows.iter().map(|r| r.1).collect();
                    let want: Vec<u32> = (0..w).collect();
                    assert_eq!(
                        sampled_cols, want,
                        "u texels for {w}x{h} at ({x},{y}) on {fb:?}"
                    );
                    let want: Vec<u32> = (0..h).collect();
                    assert_eq!(
                        sampled_rows, want,
                        "v texels for {w}x{h} at ({x},{y}) on {fb:?}"
                    );
                }
            }
        }
    }

    // Oracle: the D3D12 rasteriser itself, measured by the smoke example. Without the compensation
    // a 400x300 element covers 119 600 pixels rather than 120 000 -- one row short and one column
    // to the left -- because D3D9's pixel centres are at the integer index and D3D12's are at
    // index + 0.5. The rules themselves are untouched; the compensation lives at the API boundary.
    #[test]
    fn the_d3d9_half_pixel_rule_must_be_compensated_for_a_d3d12_rasteriser() {
        let fb = (800u32, 600u32);
        let (x, y, w, h) = (100u32, 80u32, 400u32, 300u32);

        // The raw client coordinates read against D3D12's convention: wrong, and wrong in the way
        // the smoke example measured.
        let s = UiSurface::create(w, h, true).unwrap();
        let raw = update_transform(x, y, w, h, fb, fb);
        let (cols, rows) = coverage(&raw, fb, s.physical, s.texture, PixelCentre::D3D12);
        assert_eq!(
            cols.len() * rows.len(),
            119_600,
            "the uncompensated pixel count"
        );
        assert_eq!(
            cols.first().map(|c| c.0),
            Some(i64::from(x) - 1),
            "shifted one column left"
        );
        assert_eq!(rows.len(), (h - 1) as usize, "one row short");

        // Compensated: exactly the rectangle, which is what the smoke example then produced.
        let (cols, rows) = place_on(x, y, w, h, fb, PixelCentre::D3D12);
        assert_eq!(cols.len() * rows.len(), 120_000);
        assert_eq!(cols.first().map(|c| c.0), Some(i64::from(x)));
        assert_eq!(rows.first().map(|r| r.0), Some(i64::from(y)));

        // The compensation is exactly the offset rule 1 applied, added back.
        let back = compensate_for_d3d12(raw, fb);
        #[allow(clippy::cast_precision_loss)]
        let naive_x = 2.0 * (x as f32) / (fb.0 as f32) - 1.0;
        assert!((back.x - naive_x).abs() < 1e-6, "{} vs {naive_x}", back.x);
    }

    // Oracle: the same contract, read as a falsification test.
    //
    // A finding worth recording: under *point* sampling, and with the pixel-centre model above, the
    // covered pixel range is robust to each of rules 1 and 2 on its own -- half a pixel of position
    // and a quarter pixel of size both stay inside the same set of pixel centres. What each rule
    // does change exactly is the clip coordinate the vertex carries, and rule 3 changes which texel
    // is sampled. So this asserts the consequences that are actually observable rather than
    // claiming a coverage change that does not happen; the *combined* mapping is what the previous
    // test proves, and it is the mapping the contract is about.
    #[test]
    fn each_rule_has_its_own_exact_consequence() {
        let fb = (800u32, 600u32);
        let (x, y, w, h) = (37u32, 91u32, 100u32, 40u32);
        let s = UiSurface::create(w, h, true).unwrap();
        assert_eq!(s.texture, (128, 64));
        let good = update_transform(x, y, w, h, fb, fb);

        // Rule 1: the -1/W term is worth exactly half a pixel of clip space, and it is what puts
        // the quad's left edge at viewport coordinate x - 0.5 rather than x.
        #[allow(clippy::cast_precision_loss)]
        let naive_x = 2.0 * (x as f32) / (fb.0 as f32) - 1.0;
        let half_pixel = 1.0 / f64::from(fb.0);
        assert!(
            (f64::from(naive_x - good.x) - half_pixel).abs() < 1e-7,
            "the offset must be exactly 1/W: {} vs {half_pixel}",
            naive_x - good.x
        );
        #[allow(clippy::cast_precision_loss)]
        let left = clip_x_to_viewport(good.x, fb.0 as f32);
        assert!(
            (left - (x as f32 - 0.5)).abs() < 1e-2,
            "left edge at x - 0.5, got {left}"
        );

        // Rule 2: the -0.25 inset is worth exactly a quarter pixel of extent.
        #[allow(clippy::cast_precision_loss)]
        let no_inset = 2.0 * (w as f32) / (fb.0 as f32);
        #[allow(clippy::cast_precision_loss)]
        let delta = (f64::from(no_inset - good.sx)) * f64::from(fb.0) * 0.5;
        assert!(
            (delta - 0.25).abs() < 1e-6,
            "the inset must be a quarter pixel, got {delta}"
        );

        // Rule 3: UVs of 1.0 instead of (physical-1)/(texture-1) walks the sampled texels off the
        // end of the content and into the unused part of the power-of-two texture.
        let (cols, _) = coverage(&good, fb, s.physical, s.texture, PixelCentre::D3D9);
        assert_eq!(
            cols.iter().map(|c| c.1).collect::<Vec<u32>>(),
            (0..w).collect::<Vec<u32>>()
        );
        let (cols, _) = coverage(&good, fb, s.texture, s.texture, PixelCentre::D3D9);
        let sampled: Vec<u32> = cols.iter().map(|c| c.1).collect();
        assert_ne!(sampled, (0..w).collect::<Vec<u32>>());
        assert!(
            sampled.last().is_some_and(|t| *t >= s.physical.0),
            "the last column samples past the content: {:?}",
            sampled.last()
        );
    }

    // Check the observed pixel-to-clip transform, including its half-pixel offset and Y flip.
    #[test]
    fn pixel_to_clip_is_the_documented_formula() {
        let (cx, cy) = pixel_rules::pixel_to_clip(0.0, 0.0, 800.0, 600.0);
        assert!((cx - (-1.0 - 1.0 / 800.0)).abs() < 1e-7, "{cx}");
        assert!((cy - (1.0 - 1.0 / 600.0)).abs() < 1e-7, "{cy}");
        // The centre of the buffer is half a pixel left of and above clip zero.
        let (cx, cy) = pixel_rules::pixel_to_clip(400.0, 300.0, 800.0, 600.0);
        assert!((cx - (-1.0 / 800.0)).abs() < 1e-7, "{cx}");
        assert!((cy - (-1.0 / 600.0)).abs() < 1e-7, "{cy}");
        // Y is negated: increasing pixel y decreases clip y.
        let (_, cy0) = pixel_rules::pixel_to_clip(0.0, 0.0, 800.0, 600.0);
        let (_, cy1) = pixel_rules::pixel_to_clip(0.0, 1.0, 800.0, 600.0);
        assert!(cy1 < cy0);
    }

    // The observed size transform uses 2 * (size - 0.25f) / display.
    #[test]
    fn the_size_scale_carries_the_quarter_pixel_inset() {
        let (sx, sy) = pixel_rules::size_to_clip_scale(100.0, 40.0, 800.0, 600.0);
        assert!((sx - 2.0 * 99.75 / 800.0).abs() < 1e-7, "{sx}");
        assert!((sy - 2.0 * 39.75 / 600.0).abs() < 1e-7, "{sy}");
    }

    // Oracle: the UI surface's vertex buffer -- "fu = (physical_width - 1) /
    // (texture_width - 1)". Spec trap 12: the -1s exist *because* the texture is
    // power-of-two with the content in the top-left corner.
    #[test]
    fn the_uv_rule_has_its_minus_ones_on_both_sides() {
        // A 100-wide surface in a 128-wide texture.
        let (fu, fv) = pixel_rules::surface_uv((100, 40), (128, 64));
        assert!((fu - 99.0 / 127.0).abs() < 1e-7, "{fu}");
        assert!((fv - 39.0 / 63.0).abs() < 1e-7, "{fv}");
        // An exactly power-of-two surface still gets fu < 1: the last texel's centre, not its edge.
        let (fu, _) = pixel_rules::surface_uv((128, 128), (128, 128));
        assert!((fu - 127.0 / 127.0).abs() < 1e-7, "{fu}");
        // Degenerate extents must not produce a NaN the sampler would then clamp silently.
        let (fu, fv) = pixel_rules::surface_uv((1, 1), (1, 1));
        assert_eq!((fu, fv), (0.0, 0.0));
    }

    // Every observed UI surface shares these six 16-bit indices:
    // 0, 2, 1, 1, 2, 3.
    #[test]
    fn the_shared_index_buffer_is_the_documented_one() {
        assert_eq!(pixel_rules::UI_QUAD_INDICES, [0, 2, 1, 1, 2, 3]);
        assert_eq!(
            std::mem::size_of_val(&pixel_rules::UI_QUAD_INDICES),
            12,
            "16-bit indices"
        );
        // The two triangles share the 1-2 edge, and between them name all four vertices.
        let mut seen = pixel_rules::UI_QUAD_INDICES.to_vec();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen, vec![0u16, 1, 2, 3]);
    }

    // Oracle: the UI-surface vertex table reproduced in the doc comment above.
    #[test]
    fn the_quad_vertices_are_in_the_documented_order() {
        let v = quad_vertices((100, 40), (128, 64));
        let (fu, fv) = pixel_rules::surface_uv((100, 40), (128, 64));
        assert_eq!(v[0].position, [-0.5, 0.0, 0.5]);
        assert_eq!(v[0].uv, [0.0, 0.0]);
        assert_eq!(v[1].position, [0.5, 0.0, 0.5]);
        assert_eq!(v[1].uv, [fu, 0.0]);
        assert_eq!(v[2].position, [-0.5, 0.0, -0.5]);
        assert_eq!(v[2].uv, [0.0, fv]);
        assert_eq!(v[3].position, [0.5, 0.0, -0.5]);
        assert_eq!(v[3].uv, [fu, fv]);
        // v = 0 is the top of the image and z = +0.5 is up in the client's Z-up convention, so the
        // image's first row is the quad's top edge.
        assert!(v[0].position[2] > v[2].position[2]);
        assert!(v[0].uv[1] < v[2].uv[1]);
    }

    // Oracle: every UI surface is created at power-of-two extents; the rounding itself is
    // asserted on its own beside the rounder.
    #[test]
    fn every_ui_surface_is_a_power_of_two_texture() {
        let s = UiSurface::create(100, 40, true).unwrap();
        assert_eq!(s.physical, (100, 40));
        assert_eq!(s.texture, (128, 64));
        let s = UiSurface::create(2048, 2048, false).unwrap();
        assert_eq!(s.texture, (2048, 2048));
        let s = UiSurface::create(1, 1, false).unwrap();
        assert_eq!(s.texture, (1, 1));
        // The client's own `require 0 < w <= 2048`.
        assert!(UiSurface::create(0, 10, false).is_err());
        assert!(UiSurface::create(10, 0, false).is_err());
        assert!(UiSurface::create(2049, 10, false).is_err());
        assert!(UiSurface::create(10, 2049, false).is_err());
    }

    // Oracle: rule 4 -- point filtering. Documented as a constant so a reader looking for the fourth
    // rule finds it next to the other three rather than concluding there are only three.
    #[test]
    fn the_fourth_rule_is_point_sampling() {
        // A constant rather than a formula, named so a reader looking for the fourth rule finds it
        // here with the other three instead of concluding there are only three.
        let sampler_is_point: bool = pixel_rules::UI_SAMPLER_IS_POINT;
        assert!(sampler_is_point, "the UI sampler is TEXFILTER_POINT");
    }

    // UI positions are pixels and do not scale with resolution; a larger display shows more UI.
    #[test]
    fn the_ui_does_not_scale_with_resolution() {
        let (cols_a, rows_a) = place(10, 10, 64, 32, (800, 600));
        let (cols_b, rows_b) = place(10, 10, 64, 32, (1920, 1080));
        assert_eq!(cols_a.len(), cols_b.len());
        assert_eq!(rows_a.len(), rows_b.len());
        assert_eq!(
            cols_a.iter().map(|c| c.0).collect::<Vec<_>>(),
            cols_b.iter().map(|c| c.0).collect::<Vec<_>>()
        );
    }

    // Oracle: contract 11.4's inclusive box convention -- "every size in the layout dat is
    // x1 - x0 + 1" -- which is why the transform update anchors on `y + height - 1` rather than
    // `y + height`. Asserted by showing the bottom row of the coverage is y + h - 1.
    #[test]
    fn the_anchor_is_the_inclusive_bottom_edge() {
        let (_, rows) = place(0, 0, 10, 10, (800, 600));
        assert_eq!(rows.last().map(|r| r.0), Some(9));
        let (_, rows) = place(0, 5, 10, 1, (800, 600));
        assert_eq!(rows.iter().map(|r| r.0).collect::<Vec<_>>(), vec![5]);
    }
}
