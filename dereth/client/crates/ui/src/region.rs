//! The UI region — the rectangle, the child list, the blit mode and the draw order.
//!
//! This module models region fields, coordinate helpers, the nine-step draw, and
//! the inclusive box convention.
//!
//! Two things here are load-bearing and easy to get wrong:
//!
//! * **Boxes are inclusive.** Width is `x1 - x0 + 1`. Centring uses the integer
//!   halving `(x1 - x0 + 1) / 2`. An off-by-one moves every widget in the interface by a pixel.
//! * **`children` is insertion-ordered**: head = bottom-most = drawn first, tail = top-most for
//!   both drawing and hit testing. `bring_to_front` is a *list move*, not a z-level change. And
//!   **`z_level` sorts descending**: a higher z-level is
//!   further *back*. See [`Region::add_child`].

use dereth_primitives::DataId;

use crate::props::UiObjectMode;
use crate::ElemHandle;

/// The inclusive rectangle. It is valid when `x1 >= x0 && y1 >= y0`.
///
/// It lives here rather than in `dereth_primitives` because this is where the inclusive
/// convention is documented and used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Box2D {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}

impl Box2D {
    #[must_use]
    pub const fn new(x0: i32, y0: i32, x1: i32, y1: i32) -> Self {
        Self { x0, y0, x1, y1 }
    }

    /// From a position and a size, as is called from the element
    /// constructor with the desc's x, y, width and height.
    #[must_use]
    pub const fn from_xywh(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self {
            x0: x,
            y0: y,
            x1: x + w - 1,
            y1: y + h - 1,
        }
    }

    /// The empty box returns when a region is fully clipped away.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            x0: 0,
            y0: 0,
            x1: -1,
            y1: -1,
        }
    }

    /// Whether the box is valid.
    #[must_use]
    pub const fn is_valid(&self) -> bool {
        self.x1 >= self.x0 && self.y1 >= self.y0
    }

    /// The region's width — inclusive.
    #[must_use]
    pub const fn width(&self) -> i32 {
        self.x1 - self.x0 + 1
    }

    /// The region's height — inclusive.
    #[must_use]
    pub const fn height(&self) -> i32 {
        self.y1 - self.y0 + 1
    }

    #[must_use]
    pub const fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x0 && x <= self.x1 && y >= self.y0 && y <= self.y1
    }

    /// Intersection; an empty result uses the empty-box value, not a negative-width box, because that is
    /// what the client's clip-box query produces and what callers test with `is_valid`.
    #[must_use]
    pub fn intersect(&self, o: &Self) -> Self {
        let r = Self {
            x0: self.x0.max(o.x0),
            y0: self.y0.max(o.y0),
            x1: self.x1.min(o.x1),
            y1: self.y1.min(o.y1),
        };
        if r.is_valid() {
            r
        } else {
            Self::empty()
        }
    }

    /// Translate by a parent origin, i.e. child-relative -> parent-relative.
    #[must_use]
    pub const fn offset(&self, dx: i32, dy: i32) -> Self {
        Self {
            x0: self.x0 + dx,
            y0: self.y0 + dy,
            x1: self.x1 + dx,
            y1: self.y1 + dy,
        }
    }
}

/// The region's blitting mode.
///
/// Kept even though the dirty-rectangle machinery around it is dropped: draw-after-children,
/// `BlitMode` and the alpha blend modifier are all *visible*.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BlitMode {
    #[default]
    Normal,
    Alpha3,
    Alpha4,
    Colorize,
    Multiply,
    Screen,
    Grayscale,
    Nop,
}

impl BlitMode {
    #[must_use]
    pub const fn from_u32(v: u32) -> Self {
        match v {
            1 => Self::Alpha3,
            2 => Self::Alpha4,
            3 => Self::Colorize,
            4 => Self::Multiply,
            5 => Self::Screen,
            6 => Self::Grayscale,
            7 => Self::Nop,
            _ => Self::Normal,
        }
    }
}

/// A **runtime-generated** UI surface: a dat image with one pixel operation applied to it before it
/// is shown.
///
/// One half of the "generated surface" support — `UiFill` is the other. Two of the
/// client's UI surfaces are not a picture out of the dat and are not a flat fill either: they are a
/// *derivation* of a picture, made in a local render surface at run time.
///
/// For each appearance-page color spot, the client creates a local surface in the UI format,
/// copies the template onto it, replaces black with the spot color, and installs the generated
/// image on the region. The gradient disk instead copies the shade ring with the spot color as a
/// multiply tint.
///
/// The client makes a page of pixels; this rebuild has no CPU page and uploads a texture, so the
/// operation rides on the image reference and the host derives the texture once per distinct
/// `(DataId, SurfaceOp)` pair. That is the same caching the combined-texture builder
/// does for a `(palette, image)` pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SurfaceOp {
    /// Behavior: every texel whose **whole 32-bit ARGB value**
    /// equals `from` becomes `to`. An exact match, not a tolerance and not a hue shift: the loop is
    /// `if (*p == from) *p = to;` over the locked surface.
    ReplaceColor { from: u32, to: u32 },
    /// Multiply every channel of every texel
    /// multiplied by `colour`'s, which is what turns the grey shade ring into a coloured one.
    Multiply(u32),
    /// The target indicator's image copy, selector 4:
    /// Normal copy, then an unmasked colorize blit at full strength. Retains the source
    /// alpha/value and replaces hue/saturation using retail's integer colour conversion.
    Colorize(u32),
    /// **Several** dat surfaces blitted into one local surface, which is what an inventory slot's
    /// picture actually is. See [`IconRecipe`].
    Icon(IconRecipe),
}

/// The surface the weenie's icon accessor answers, described rather than drawn.
///
/// The icon accessor does not return a dat image directly. It builds a **32 × 32** local surface
/// in the UI's `PFID_A8R8G8B8` format, so the
/// working format is `PFID_A8R8G8B8`) that the icon renderer composites out of
/// **six** blits into **two** surfaces. A spell slot's picture is the same idea through a different
/// function, the magic system's spell-icon compositor.
///
/// For an object, the client first builds a drag surface from the icon, alpha overlay, and effect
/// color replacement. It then builds the final surface from the item-type background, underlay,
/// and the completed drag surface. For a spell, it draws the power-level background, alpha icon,
/// reversed or normal tint, and an optional fellowship or self overlay in that order.
///
/// # Why this is a recipe and not a flat layer list
///
/// The drag icon is a **separate surface** and the three blits that build it happen *before* it is
/// alpha-blended onto the icon surface as one image. Flattening the six into one ordered list would put
/// the icon's normal blit straight onto the background tile and erase it — the tile is the opaque
/// base, and it is only visible at all because the icon reaches it through the drag surface's own
/// alpha. Two stages is not an implementation detail; it is the picture.
///
/// # Why this rides on [`SurfaceOp`]
///
/// A region carries **one** `GraphicRef`, and the client's own answer to that is not a second
/// region — it is a second *surface*. The seam for a runtime-generated UI
/// surface (`GraphicRef::op` → `UiDrawCmd::image_op` → a host texture cache keyed on
/// `(DataId, Option<SurfaceOp>)`), so this is the same seam with more inputs rather than a parallel
/// one. The host composites once per distinct recipe, exactly as the client's icon data is built once per
/// object and rebuilt by the icon-data-changed hook only when one of the five fields
/// the icon update compares has moved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IconRecipe {
    /// The object icon, as the icon renderer builds it.
    Object {
        /// The `0x10000004` icon-background row selected by the lowest set item-type bit plus one —
        /// the opaque base.
        background: Option<DataId>,
        /// The `0x10000005` effect-icon row selected by the lowest set effect bit plus one, with
        /// the client's row `0x21` fallback already applied.
        /// Replaces the drag icon's opaque-white pixels; x86.
        effects: Option<DataId>,
        /// The object's public icon id.
        icon: Option<DataId>,
        /// The object's public icon-overlay id.
        overlay: Option<DataId>,
        /// The object's public icon-underlay id.
        /// Blitted over the type tile, below the drag icon; x86.
        underlay: Option<DataId>,
    },
    /// The spell icon, as the magic system's compositor builds it.
    Spell {
        /// The `0x10000006` spell-background row selected by the spell's power level.
        background: Option<DataId>,
        /// The spell's icon id.
        icon: Option<DataId>,
        /// Row 1 or 2 of spell-overlay table `0x10000007`, selected by bit `0x10` — the
        /// **reversed / non-reversed** wash, applied through `ReplaceColor` and not blitted.
        tint: Option<DataId>,
        /// `0x10000007` row **4** for `FellowshipSpell (0x2000)`, row **3** for
        /// `SelfTargeted (0x8)`, and nothing otherwise.
        overlay: Option<DataId>,
    },
}

impl IconRecipe {
    /// The id this composite **is about**, which is what [`GraphicRef::did`] carries.
    ///
    /// **The object's own icon, not the surface the composite happens to start from.** The two are
    /// different — the bottom blit is the item-type tile — and the icon is the right answer for
    /// both of `did`'s jobs: a consumer that does not understand the recipe draws the object's
    /// picture rather than a bare coloured square, and a reader of a draw list (or of a failing
    /// assertion) sees the id that names the thing in the slot. Dozens of assertions across this
    /// workspace already read `did` off the icon element and mean *"this slot draws that object's
    /// icon"*; two of the quickbar's caught this the moment it was the tile instead.
    ///
    /// The **pixels** do not come from here at all: the host builds them from the whole recipe, so
    /// nothing about the composite depends on which layer this names.
    #[must_use]
    pub const fn base(self) -> Option<DataId> {
        match self {
            Self::Object {
                background, icon, ..
            }
            | Self::Spell {
                background, icon, ..
            } => match icon {
                Some(d) => Some(d),
                None => background,
            },
        }
    }

    /// The *first* of the two surfaces the icon renderer builds,
    /// as a recipe of its own.
    ///
    /// The client makes two 32×32 surfaces and keeps both. The drag surface gets the icon (blit
    /// mode 0), then the overlay (mode 2), then the effects replacement of white. The icon surface
    /// gets the item-type tile (mode 0), then the underlay (mode 1), then the finished drag surface
    /// (mode 1). Each becomes a graphic of its own.
    ///
    /// The item list's drag-icon preparation asks for the object's drag icon by name, so what
    /// follows the cursor is the icon, its overlay and the effects replacement, and
    /// **not** the item-type tile or the custom underlay. Dropping those two is therefore not an
    /// approximation of the drag surface: `composite`'s `based == false` arm blits the finished
    /// drag buffer onto an untouched local with a normal blit, which reproduces the drag surface exactly.
    ///
    /// A `Spell` recipe has no second surface in the client — the spell-icon compositor
    /// makes one — and is returned unchanged.
    #[must_use]
    pub const fn drag_surface(self) -> Self {
        match self {
            Self::Object {
                effects,
                icon,
                overlay,
                ..
            } => Self::Object {
                background: None,
                effects,
                icon,
                overlay,
                underlay: None,
            },
            Self::Spell { .. } => self,
        }
    }

    /// How many of the recipe's surfaces are present. **One** is exactly the plain image and the
    /// host may skip the compositor — but it must still key on the recipe, because the icon alone
    /// and the icon under a tile are different pictures. **Zero** is the client's
    /// null image: nothing at all.
    #[must_use]
    pub fn layer_count(self) -> usize {
        let present = |v: Option<DataId>| usize::from(v.is_some());
        match self {
            Self::Object {
                background,
                effects,
                icon,
                overlay,
                underlay,
            } => {
                present(background)
                    + present(effects)
                    + present(icon)
                    + present(overlay)
                    + present(underlay)
            }
            Self::Spell {
                background,
                icon,
                tint,
                overlay,
            } => present(background) + present(icon) + present(tint) + present(overlay),
        }
    }
}

impl SurfaceOp {
    /// The packed ARGB value of RGBA `(0, 0, 0, 1)` — the colour the colour-spot pass replaces.
    ///
    /// The `from` is built on the stack as four floats and converted, so the alpha byte is **0xFF**
    /// and the match is against opaque black rather than against transparent black.
    pub const OPAQUE_BLACK: u32 = 0xFF00_0000;

    /// The packed ARGB value of RGBA `(1, 1, 1, 1)` — the colour **both** icon composites use
    /// as the replacement source. Opaque white, by the same reasoning as
    /// [`Self::OPAQUE_BLACK`]: the four floats are converted, so the alpha byte is `0xFF`.
    pub const OPAQUE_WHITE: u32 = 0xFFFF_FFFF;

    /// The 32 × 32 `Create(0x20, 0x20, ...)` both composites make. Kept here because it is the
    /// client's constant and not the icon art's: a source surface of another size is clipped to it
    /// ( takes the smaller of the two extents on each axis).
    pub const ICON_EXTENT: u32 = 32;

    /// Apply the operation to one 32-bit ARGB texel.
    ///
    /// Little-endian BGRA8 bytes and a `u32` ARGB are the same four bytes in the same order, which
    /// is why the client's `*(ulong*)p == from` and this agree texel for texel.
    #[must_use]
    pub fn apply(self, texel: u32) -> u32 {
        match self {
            Self::ReplaceColor { from, to } => {
                if texel == from {
                    to
                } else {
                    texel
                }
            }
            Self::Multiply(c) => {
                let ch = |sh: u32| ((texel >> sh) & 0xFF) * ((c >> sh) & 0xFF) / 255;
                (ch(24) << 24) | (ch(16) << 16) | (ch(8) << 8) | ch(0)
            }
            Self::Colorize(c) => Self::colorize(texel, c),
            // A composite is a function of **several** surfaces and cannot be a function of one
            // texel. The host recognises this variant before it reaches here; see
            // `dereth_client::ui_draw::composite`. Returning the texel unchanged keeps a caller that
            // does not from corrupting the base image, which is what a `todo!()` here would not.
            Self::Icon(_) => texel,
        }
    }

    /// Full-strength, unmasked ARGB colorize. The blit-and-color entry point performs the
    /// preceding Normal copy.
    /// The 1..256 channel representation, integer hue/saturation, and signed nearest
    /// division (ties toward zero) are observable on grey antialiased bracket texels.
    #[must_use]
    pub fn colorize(texel: u32, color: u32) -> u32 {
        let channel = |c: u32, shift: u32| {
            let v = ((c >> shift) & 255u32) as i32;
            v + i32::from(v != 0)
        };
        let (r, g, b) = (channel(color, 16), channel(color, 8), channel(color, 0));
        let max = r.max(g).max(b);
        let delta = max - r.min(g).min(b);
        let value = channel(texel, 16)
            .max(channel(texel, 8))
            .max(channel(texel, 0));
        let (rr, gg, bb) = if delta == 0 {
            (value, value, value)
        } else {
            let hue = if r == max {
                (g - b) * 4096 / delta + if g < b { 24576 } else { 0 }
            } else if g == max {
                (b - r) * 4096 / delta + 8192
            } else {
                (r - g) * 4096 / delta + 16384
            };
            let saturation = delta * 256 / max;
            let chroma = value * saturation / 256;
            let low = value - chroma;
            let sector = hue / 4096;
            let product = (hue - ((sector + 1) / 2) * 8192) * chroma;
            let mut middle = product / 4096;
            if product % 4096 > 2048 {
                middle += 1;
            }
            if product % 4096 < -2048 {
                middle -= 1;
            }
            match sector {
                0 => (value, low + middle, low),
                1 => (low - middle, value, low),
                2 => (low, value, low + middle),
                3 => (low, low - middle, value),
                4 => (low + middle, low, value),
                _ => (value, low, low - middle),
            }
        };
        let pack = |v: i32| u32::try_from(v - i32::from(v != 0)).unwrap_or(0);
        (texel & 0xFF00_0000) | (pack(rr) << 16) | (pack(gg) << 8) | pack(bb)
    }

    /// The pair a per-texel [`Self::apply`] cannot answer: the recipe, when this operation is one.
    #[must_use]
    pub const fn icon_recipe(self) -> Option<IconRecipe> {
        match self {
            Self::Icon(r) => Some(r),
            _ => None,
        }
    }

    /// Normal-copy behavior for `A8R8G8B8 -> A8R8G8B8` at full opacity.
    ///
    /// A source copy from ARGB to ARGB is a plain 32-bit store — all four channels, alpha
    /// included. That is why the drag surface never needs clearing: the icon's own normal blit
    /// initialises every texel of it.
    #[must_use]
    pub const fn blit_normal(_dst: u32, src: u32) -> u32 {
        src
    }

    /// The three-channel alpha blit.
    ///
    /// ```text
    /// a = src >> 24;                       // the alpha modifier is 1.0 at every call site here
    /// if (a == 0) leave dst alone;
    /// if (a == 255) dst = (dst & 0xFF000000) | (src & 0x00FFFFFF);   // three channels, not four
    /// else per channel c:  d - (((d * (a+1)) >> 8) - ((s * (a+1)) >> 8))   saturated to 0..255,
    ///      and dst's own alpha byte is left where it is.
    /// ```
    ///
    /// The two separate shifts are the client's and are **not** algebraically `d + ((s-d)*k >> 8)`;
    /// they round apart by one on many texels, which is why they are transcribed rather than
    /// simplified. This is the mode that lets the background tile show around an icon's edge.
    #[must_use]
    pub fn blit_3alpha(dst: u32, src: u32) -> u32 {
        let a = src >> 24;
        if a == 0 {
            return dst;
        }
        if a == 255 {
            return (dst & 0xFF00_0000) | (src & 0x00FF_FFFF);
        }
        let k = i64::from(a + 1);
        let ch = |sh: u32| -> u32 {
            let d = i64::from((dst >> sh) & 0xFF);
            let s = i64::from((src >> sh) & 0xFF);
            let v = d - (((d * k) >> 8) - ((s * k) >> 8));
            u32::try_from(v.clamp(0, 255)).unwrap_or(0)
        };
        (dst & 0xFF00_0000) | (ch(16) << 16) | (ch(8) << 8) | ch(0)
    }

    /// The four-channel alpha blit.
    ///
    /// The same lerp when the destination is opaque, the source outright when the destination is
    /// transparent, and the non-premultiplied *over* when it is neither:
    ///
    /// ```text
    /// a = src >> 24;  if (a == 0) leave dst alone;  if (a == 255) dst = src;
    /// da = dst >> 24; if (da == 0) dst = src;
    /// else if (da == 255) the blit_3alpha lerp, and the alpha byte stays 255;
    /// else { k = a + 1; dk = da + 1;
    ///        outA = dk + k - ((dk * k) >> 8);
    ///        per channel: round(d - ((d - s) * k) / outA), saturated;
    ///        alpha byte = outA - 1; }
    /// ```
    #[must_use]
    pub fn blit_4alpha(dst: u32, src: u32) -> u32 {
        let a = src >> 24;
        if a == 0 {
            return dst;
        }
        if a == 255 {
            return src;
        }
        let da = dst >> 24;
        if da == 0 {
            return src;
        }
        if da == 255 {
            // The alpha lane of the same lerp with d = 255 cannot move below 255 for any s, so the
            // result is opaque and only the three colour channels change.
            return 0xFF00_0000 | (Self::blit_3alpha(dst, src) & 0x00FF_FFFF);
        }
        let k = a + 1;
        let dk = da + 1;
        let out_a = dk + k - ((dk * k) >> 8);
        let ch = |sh: u32| -> u32 {
            let d = f64::from((dst >> sh) & 0xFF);
            let s = f64::from((src >> sh) & 0xFF);
            let v = (d - (d - s) * f64::from(k) / f64::from(out_a)).round();
            u32::try_from(dereth_primitives::num::to_i32_f64(v).clamp(0, 255)).unwrap_or(0)
        };
        ((out_a - 1) << 24) | (ch(16) << 16) | (ch(8) << 8) | ch(0)
    }
}

/// A reference to a dat surface plus, when the caller has one, its decoded alpha coverage.
///
/// `Graphic` itself belongs to the render track. What the hit test needs from it is
/// the graphic's own point test: whether the texel at a point is transparent. That is modelled
/// here as an optional bitmask so the UI can be tested with no renderer at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphicRef {
    pub did: DataId,
    /// Which files the picture is read from: the interface's own, or the world's. See
    /// [`ImageSource`].
    pub source: ImageSource,
    pub width: i32,
    pub height: i32,
    /// One entry per texel, row-major, `true` = opaque. `None` = "no mask, every texel hits".
    pub opaque: Option<Vec<bool>>,
    /// The pixel operation this element's picture is a *derivation* under, or `None` for the dat
    /// image itself. See [`SurfaceOp`].
    pub op: Option<SurfaceOp>,
}

/// Which files an element's picture is read from.
///
/// An interface draws two kinds of picture. Its **chrome** -- the art its layouts name: panel
/// frames, buttons, bars, the colours of its windows -- is the interface's own and comes from the
/// interface's files. Its **content** -- the pictures a world names: an item's icon, a spell's, a
/// skill's, a component's -- comes from the world's files. Both kinds share one id space, and beside
/// an older world the two sets of files answer the same id with different pictures, so a picture
/// says which it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum ImageSource {
    /// The interface's own files: every picture a layout names, and every one a screen sets
    /// unless it says otherwise.
    #[default]
    Interface,
    /// The world's files: a picture a world's records name.
    World,
}

impl GraphicRef {
    /// [`Self::opaque_surface`] for a picture a world's records name ([`ImageSource::World`]).
    #[must_use]
    pub const fn world_surface(did: DataId, width: i32, height: i32) -> Self {
        let mut g = Self::opaque_surface(did, width, height);
        g.source = ImageSource::World;
        g
    }

    #[must_use]
    pub const fn opaque_surface(did: DataId, width: i32, height: i32) -> Self {
        Self {
            did,
            source: ImageSource::Interface,
            width,
            height,
            opaque: None,
            op: None,
        }
    }

    /// Returns whether the texel at this *region-relative* point is opaque.
    ///
    /// A point outside the surface misses, which matters because a region can be larger than its
    /// background image.
    ///
    /// # The mask's polarity is inverted with respect to the client, and deliberately left so
    ///
    /// The client's graphic point test returns true for a **painted** texel (the colour with the
    /// alpha channel masked off is non-zero), and the region hit test treats true as a *miss*. So
    /// the alpha image is a **cut-out** mask: the point
    /// falls through where the mask is painted. This type models the opposite — `opaque` is a
    /// coverage mask, `true` = hit — and every caller and test in this crate is consistent with
    /// that reading. The difference is **unobservable in this build**, because no alpha surface's
    /// pixels are decoded anywhere: `MediaEffect::SetAlphaImage` records the `DataId` and nothing
    /// fills `opaque`. Whatever first decodes one must flip it, and this note is here so that the
    /// inversion does not have to be rediscovered.
    ///
    /// **The "no mask" case is decided first, and that ordering is load-bearing.** The graphic
    /// point test returns false when its image is null, and its one caller
    /// reads false as "this graphic does not refuse the point." A [`GraphicRef`] with
    /// `opaque: None` **is** that null surface: nothing has been
    /// decoded, so there is nothing to refuse with. Testing the extent first would make every such
    /// reference a 0×0 rectangle that answered "miss" everywhere, and
    /// [`MediaEffect::SetAlphaImage`](crate::MediaEffect::SetAlphaImage) builds exactly that —
    /// `GraphicRef::opaque_surface(did, 0, 0)`, because the alpha surface's pixels belong to the
    /// render track and are not loaded here.
    ///
    /// The consequence would be total and silent: all six buttons of the shipped
    /// character-management layout carry an alpha media step, so all six would be transparent to
    /// the mouse however mouse-visible they were, and `hit_test_screen` would return `None` over
    /// every one of them.
    #[must_use]
    pub fn point_test(&self, x: i32, y: i32) -> bool {
        let Some(mask) = &self.opaque else {
            return true;
        };
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return false;
        }
        let idx = y as usize * self.width as usize + x as usize;
        mask.get(idx).copied().unwrap_or(false)
    }
}

/// The original UI-region layout packs these flags in bits 0 through 7.
///
/// `visible` defaults **true** and everything else false, matching.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegionFlags {
    pub mouse_over_top: bool,
    pub visible: bool,
    pub transparent: bool,
    pub erase_background: bool,
    pub mouse_over: bool,
    pub tooltip: bool,
    pub block_clicks: bool,
    pub draw_after_children: bool,
}

impl Default for RegionFlags {
    fn default() -> Self {
        Self {
            mouse_over_top: false,
            visible: true,
            transparent: false,
            erase_background: false,
            mouse_over: false,
            tooltip: false,
            block_clicks: false,
            draw_after_children: false,
        }
    }
}

/// The UI region.
#[derive(Debug, Clone, Default)]
pub struct Region {
    /// **Inclusive**, parent-relative. `w = x1 - x0 + 1`.
    pub box_: Box2D,
    /// Texture-space offset for a tiled background.
    pub tiling_offset: (i32, i32),
    /// Sort key inside the parent.
    pub z_level: i32,
    /// The region's image.
    pub image: Option<GraphicRef>,
    /// The per-pixel hit mask.
    pub alpha_image: Option<GraphicRef>,
    /// The alpha blend modifier, 0..1.
    pub alpha_blend_mod: f32,
    /// The blit mode.
    pub blit_mode: BlitMode,
    pub flags: RegionFlags,
    /// Insertion-ordered: head = bottom-most, tail = top-most.
    pub children: Vec<ElemHandle>,
    /// The parent region's element.
    pub parent: Option<ElemHandle>,
    /// The original layout's object field selects **which** UI object this region gets and how its
    /// surface is sized.
    ///
    /// It is a mode rather than a bool, because a bool is only the `!= 0` half of attribute
    /// `0xCD`. The default is [`UiObjectMode::ElementSize`] because an **absent** `0xCD` means 3
    /// at all three of retail's readers, not 0 — see [`UiObjectMode`].
    pub object_mode: UiObjectMode,
    /// The surface object's **layer diffuse alpha**, written by material-opacity updates. It is *not*
    /// [`Self::alpha_blend_mod`].
    ///
    /// The client has two alphas and a rebuild that collapses them loses the only one a player
    /// ever sees move:
    ///
    /// * the alpha blend modifier is **per region** and rides on the CPU blit this region makes
    ///   *into* a surface — this region's own picture, and
    ///   nothing else;
    /// * the material's alpha is **per UI object**, and a UI object is shared: only an element
    ///   with the should-own-object flag has one, and every descendant without one composes into the
    ///   nearest owning ancestor's surface, which is exactly what the object's ancestor walk
    ///   finds. So the material alpha
    ///   multiplies *the whole composed window*.
    ///
    /// That difference is why the chat opacity must not be wired onto `alpha_blend_mod`, which
    /// is a faithful place for a picture's own blend and the wrong place for this: the chat window root
    /// `0x10000601` draws **no picture and no fill of its own** — measured, its `UiDrawCmd` has
    /// `image: None`, `fills: []`, `glyphs: 0` — while its background `0x10000010`, its eight
    /// frame pieces `0x1000069B`…`0x100006A2`, the log and the scrollbar are all children, all
    /// owning no object, so they would all draw at alpha 1 and the slider would move a number
    /// that reached zero pixels.
    ///
    /// `1.0` for everything; [`crate::UiSystem::set_material_opacity`] is the only writer and
    /// [`crate::UiSystem::draw`] carries it down the subtree as a product.
    pub material_opacity: f32,
    /// The pixels this element wrote into its own UI surface with flat fills,
    /// which no `Graphic` produced and no `DataID` names.
    ///
    /// The other half of the "generated surface" support. Two callers in the client put pixels
    /// in a UI surface without a picture, and both use the same primitive:
    /// the generated-image path fills the whole clip box with the null colour, and the radar's
    /// eight blip shapes fill 1×1 areas. Every shape is drawn with 1×1 fills into the element's
    /// surface; there is no texture and no anti-aliasing.
    ///
    /// Coordinates are **element-relative**, matching the client's own `sx`/`sy`,
    /// which are measured from the element's screen origin.
    pub surface_fills: Vec<UiFill>,
}

/// One filled `(w, h, colour)` rectangle with no source image.
///
/// The client's UI has exactly one such primitive and it is the whole of the "generated surface"
/// that the draw list has to express. `w`/`h` are **counts**, not an inclusive
/// far edge, because the client's fill takes a width and a height.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiFill {
    /// Element-relative, top-left.
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    /// Packed ARGB order, alpha in the high byte.
    pub color: u32,
}

impl UiFill {
    /// The null RGBA colour — the colour the region's background erase fills with.
    ///
    /// In retail this colour is all zero and nothing ever writes it, so the erase is a clear to
    /// fully transparent black, not a paint.
    pub const NULL: u32 = 0x0000_0000;

    /// A 1×1 fill, which is what every radar blip pixel is.
    #[must_use]
    pub const fn point(x: i32, y: i32, color: u32) -> Self {
        Self {
            x,
            y,
            w: 1,
            h: 1,
            color,
        }
    }

    /// True when this fill can change no pixel under `SRCALPHA`/`INVSRCALPHA`.
    ///
    /// The null colour is exactly this case, which is why the erase step is free here: the client
    /// clears a CPU page that is then blended over the frame, and this rebuild has no CPU page —
    /// blending a zero-alpha source leaves the destination bit for bit.
    #[must_use]
    pub const fn is_invisible(&self) -> bool {
        self.color >> 24 == 0 || self.w <= 0 || self.h <= 0
    }
}

impl Region {
    #[must_use]
    pub fn new(box_: Box2D) -> Self {
        // Both alphas start opaque: the region starts the alpha blend modifier at 1, and the material
        // layer starts with diffuse color `(1,1,1,1)`.
        Self {
            box_,
            alpha_blend_mod: 1.0,
            material_opacity: 1.0,
            ..Self::default()
        }
    }

    /// The point-is-over-region test — the inclusive box test **plus** the alpha-mask
    /// test. `x`/`y` are in the region's *parent* coordinates, as at the recursion site in
    /// the mouse hit tester.
    ///
    /// Dropping the mask makes every irregular panel frame a
    /// solid rectangle for the mouse.
    #[must_use]
    pub fn point_is_over(&self, x: i32, y: i32) -> bool {
        if !self.box_.contains(x, y) {
            return false;
        }
        match &self.alpha_image {
            None => true,
            Some(g) => g.point_test(x - self.box_.x0, y - self.box_.y0),
        }
    }

    /// Add a child — insert by `z_level`, **descending from head to tail**.
    ///
    /// **A higher `z_level` is further back, not further forward.** That is the opposite of what
    /// the name suggests, and the transcription is
    /// worth spelling out because it is the whole of it: the z comparison returns -1, 0 or 1 as
    /// the first z is less than, equal to or greater than the second. Adding walks the children
    /// from the **tail** backwards and inserts the new child after the first existing child whose
    /// z is `>=` the new one's; if there is none, the new child goes at the head. The draw
    /// iterates from the **head**.
    ///
    /// Insert into `[A]` with `A.z >= B.z` and you get `[A, B]`; with `A.z < B.z` you get `[B, A]`.
    /// So the list runs highest z at the head to lowest z at the tail, the head is drawn first, and
    /// the **lowest** z-level ends up on top. The shipped `patch` layout is the proof: its black
    /// background panel `0x1000041B` carries z-level 10 while the Cancel button and the two
    /// status labels carry 5 and the two meters carry 0. Sorted the other way the background covers
    /// the whole screen and the interface is invisible.
    ///
    /// The sort must be **stable** so that equal z-levels keep insertion order, which is what the
    /// client's "insert after the last existing with an equal z" does.
    ///
    /// # The tie-break is `read_order`, not insertion order
    ///
    /// The original walk dispatches a comparison that elements extend. The element behavior calls
    /// the region comparison first and, **only when that returns
    /// 0**, compares the description's `read_order`: it returns 1 (i.e. "insert the
    /// new child after this one") when the existing child's read order is the *smaller*. So on an
    /// equal z the list runs in **ascending authoring order**, and that is a real ordering, not a
    /// tolerance — the recursive element builder walks
    /// the children in hash order, so insertion order carries no meaning at all and this is the only
    /// thing that decides which sibling paints on top.
    ///
    /// The vitals bars are the proof. Every
    /// `Meter` in `classic_gameplay` holds three z-0 children: the empty trough
    /// `0x100000E7` (read order 1), the coloured fill `0x00000002` (read order 2) and the label
    /// (read order 3). This crate's child-description table is a `BTreeMap` keyed by element id, so
    /// without the tie-break creation order is `2, 0xE7, 0xEB` — the fill first, the trough painted
    /// straight over it — and **every vial draws empty at every value**. With the tie-break the order is trough, fill,
    /// label, which is what the retail client shows.
    pub fn add_child(&mut self, h: ElemHandle, key_of: impl Fn(ElemHandle) -> (i32, u32)) {
        self.children.retain(|c| *c != h);
        self.children.push(h);
        self.children.sort_by_key(|c| {
            let (z, read_order) = key_of(*c);
            (std::cmp::Reverse(z), read_order)
        });
    }

    pub fn remove_child(&mut self, h: ElemHandle) {
        self.children.retain(|c| *c != h);
    }

    /// Move a child to the list tail, **not** to a new z-level.
    pub fn bring_child_to_front(&mut self, h: ElemHandle) {
        if let Some(i) = self.children.iter().position(|c| *c == h) {
            let c = self.children.remove(i);
            self.children.push(c);
        }
    }
}

/// The region draw routine's nine steps, as an ordered trace.
///
/// The steps are emitted rather than executed so that a test can assert the order without a
/// renderer. The screens and the client bind [`DrawStep::DrawSelf`] to an actual blit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawStep {
    /// step 2 — `EraseSelf`, only when `erase_background`.
    EraseSelf,
    /// steps 3 and 8 — `DrawChildren`; which one runs is `draw_after_children`.
    DrawChildren,
    /// step 4 — `DrawStart`.
    DrawStart,
    /// step 5 — `PreBlit`.
    PreBlit,
    /// step 6 — `DrawSelf`: with the blit mode, the alpha
    /// blend modifier and the tiling offset.
    DrawSelf,
    /// step 7 — `PostBlit`.
    PostBlit,
    /// step 9 — `DrawDone`.
    DrawDone,
}

/// One entry of a draw trace: which element, which step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DrawEvent {
    pub who: ElemHandle,
    pub step: DrawStep,
}

/// The fixed step sequence for one region, given `draw_after_children` and `erase_background`.
///
/// The per-element draw's step 1 is "intersect the dirty rects with the clip box and bail out if empty"; that
/// is the caller's job (see `crate::UiSystem::draw_trace`), because it decides whether this
/// sequence runs at all.
#[must_use]
pub fn draw_steps(erase_background: bool, draw_after_children: bool) -> Vec<DrawStep> {
    let mut v = Vec::with_capacity(7);
    if erase_background {
        v.push(DrawStep::EraseSelf);
    }
    if draw_after_children {
        v.push(DrawStep::DrawChildren);
    }
    v.push(DrawStep::DrawStart);
    v.push(DrawStep::PreBlit);
    v.push(DrawStep::DrawSelf);
    v.push(DrawStep::PostBlit);
    if !draw_after_children {
        v.push(DrawStep::DrawChildren);
    }
    v.push(DrawStep::DrawDone);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the inclusive box convention and the region's own width and height accessors.
    #[test]
    fn inclusive_box_width_is_x1_minus_x0_plus_one() {
        let b = Box2D::new(0, 0, 799, 599);
        assert_eq!(b.width(), 800);
        assert_eq!(b.height(), 600);
        assert_eq!(Box2D::from_xywh(10, 20, 30, 40), Box2D::new(10, 20, 39, 59));
        assert_eq!(Box2D::from_xywh(10, 20, 30, 40).width(), 30);
    }

    /// Pinned behavior: it returns the empty box `(0,0,-1,-1)` when fully clipped.
    #[test]
    fn fully_clipped_intersection_is_the_documented_empty_box() {
        let a = Box2D::new(0, 0, 10, 10);
        let b = Box2D::new(50, 50, 60, 60);
        assert_eq!(a.intersect(&b), Box2D::new(0, 0, -1, -1));
        assert!(!a.intersect(&b).is_valid());
        // A one-pixel overlap is still valid, because the box is inclusive.
        assert_eq!(
            a.intersect(&Box2D::new(10, 10, 20, 20)),
            Box2D::new(10, 10, 10, 10)
        );
    }

    /// Oracle: the nine steps of the region's draw.
    #[test]
    fn draw_after_children_moves_the_children_step_before_drawself() {
        assert_eq!(
            draw_steps(true, false),
            vec![
                DrawStep::EraseSelf,
                DrawStep::DrawStart,
                DrawStep::PreBlit,
                DrawStep::DrawSelf,
                DrawStep::PostBlit,
                DrawStep::DrawChildren,
                DrawStep::DrawDone,
            ]
        );
        assert_eq!(
            draw_steps(false, true),
            vec![
                DrawStep::DrawChildren,
                DrawStep::DrawStart,
                DrawStep::PreBlit,
                DrawStep::DrawSelf,
                DrawStep::PostBlit,
                DrawStep::DrawDone,
            ]
        );
    }

    /// Oracle: the region's point-is-over test plus the graphic's point test.
    #[test]
    fn a_point_over_a_transparent_texel_misses() {
        // A 4x4 frame whose corners are transparent, like AC's irregular panel frames.
        let mut mask = vec![true; 16];
        for (x, y) in [(0, 0), (3, 0), (0, 3), (3, 3)] {
            mask[y * 4 + x] = false;
        }
        let mut r = Region::new(Box2D::new(10, 10, 13, 13));
        r.alpha_image = Some(GraphicRef {
            did: DataId(0x0600_0001),
            source: ImageSource::Interface,
            width: 4,
            height: 4,
            opaque: Some(mask),
            op: None,
        });
        assert!(
            !r.point_is_over(10, 10),
            "transparent corner must pass the click through"
        );
        assert!(r.point_is_over(11, 10), "opaque edge must be hit");
        assert!(
            !r.point_is_over(9, 10),
            "outside the box misses regardless of the mask"
        );
        // Without a mask the box test alone decides.
        r.alpha_image = None;
        assert!(r.point_is_over(10, 10));
    }

    /// Oracle: the z sort and bring-to-front list move.
    #[test]
    fn bring_to_front_is_a_list_move_not_a_z_change() {
        let z = |h: ElemHandle| (0, u32::try_from(h.index()).unwrap_or(0));
        let mut r = Region::default();
        let a = ElemHandle::for_test(1);
        let b = ElemHandle::for_test(2);
        let c = ElemHandle::for_test(3);
        r.add_child(a, z);
        r.add_child(b, z);
        r.add_child(c, z);
        assert_eq!(
            r.children,
            vec![a, b, c],
            "head = first added = bottom-most"
        );
        r.bring_child_to_front(a);
        assert_eq!(r.children, vec![b, c, a]);
    }

    /// An equal z level is broken by read order and not by creation order.
    #[test]
    fn an_equal_z_level_is_broken_by_read_order_and_not_by_creation_order() {
        let fill = ElemHandle::for_test(1);
        let trough = ElemHandle::for_test(2);
        let label = ElemHandle::for_test(3);
        let key = |h: ElemHandle| match h.index() {
            1 => (0, 2), // 0x00000002, read order 2
            2 => (0, 1), // 0x100000E7, read order 1
            _ => (0, 3), // 0x100000EB, read order 3
        };
        let mut r = Region::default();
        // Element-id order, which is what a `BTreeMap` walk produces.
        r.add_child(fill, key);
        r.add_child(trough, key);
        r.add_child(label, key);
        assert_eq!(
            r.children,
            vec![trough, fill, label],
            "the trough must be painted first or the fill is invisible"
        );
        // z still wins over read order: a background at z 10 stays at the head whatever its
        // authoring order.
        let back = ElemHandle::for_test(4);
        let key2 = |h: ElemHandle| if h.index() == 4 { (10, 99) } else { key(h) };
        let mut r = Region::default();
        r.add_child(fill, key2);
        r.add_child(back, key2);
        r.add_child(trough, key2);
        assert_eq!(r.children, vec![back, trough, fill]);
    }

    /// Oracle: the region's add-child and its z-level comparison,
    /// transcribed in [`Region::add_child`]: the walk is from the **tail**, the insert is *after*
    /// the first existing whose z is `>=` the new one's, and the draw iterates
    /// from the head. That orders the list **descending** by z — a higher `z_level` is further
    /// back — and the sort must be stable, or equal z-levels lose insertion order.
    ///
    /// The shipped `patch` layout (`0x21000000`) is the same statement in data: its full-screen
    /// black background carries z-level 10 and everything drawn over it carries 5 or 0.
    #[test]
    fn add_child_sorts_by_z_level_stably_with_the_highest_level_furthest_back() {
        let mut r = Region::default();
        let back = ElemHandle::for_test(1);
        let front = ElemHandle::for_test(2);
        let front2 = ElemHandle::for_test(3);
        let z = |h: ElemHandle| {
            (
                if h.index() == 1 { 100 } else { 0 },
                u32::try_from(h.index()).unwrap_or(0),
            )
        };
        r.add_child(back, z);
        r.add_child(front, z);
        r.add_child(front2, z);
        assert_eq!(
            r.children,
            vec![back, front, front2],
            "head = drawn first = furthest back"
        );

        // ...and the order the layout streams them in does not change the answer.
        let mut r = Region::default();
        r.add_child(front, z);
        r.add_child(front2, z);
        r.add_child(back, z);
        assert_eq!(r.children, vec![back, front, front2]);
    }
}
