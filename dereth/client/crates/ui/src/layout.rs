//! Four-edge anchoring, `move_to`/`resize_to` and `BorderLocation`.
//!
//! The rules were checked against the state-description size-and-position update
//! and the element's parent-size-change handler.
//!
//! **There is no docking and no flow layout.** Every rectangle comes out of the layout dat in
//! *design* coordinates, valid for the `LayoutDesc`'s own display width and height, and
//! four independent edge modes say how each edge follows a change of reference box.

use crate::region::Box2D;

/// The edge attachment type. The enum's names are not recoverable — its mapper (`EnumMapper 0x22000017`)
/// is server-only and does not ship — so the names here describe the retail
/// *arithmetic*, which is what matters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EdgeMode {
    /// 0 — the design value, except that a live element that already has a size (or is already
    /// initialised) keeps its **current** coordinate instead. That is how a user-moved window
    /// survives a resolution change; see [`apply_live_mode_zero`].
    #[default]
    Fixed = 0,
    /// 1 — left/top unchanged; right/bottom follow the delta.
    AnchorStart = 1,
    /// 2 — left/top follow the delta; right/bottom unchanged.
    AnchorEnd = 2,
    /// 3 — centred, with the integer halving `(x1 - x0 + 1) / 2`.
    Centre = 3,
    /// 4 — proportional. **UNVERIFIED**: only the float-to-int truncation is
    /// established, not the multiply that feeds it.
    Proportional = 4,
}

impl EdgeMode {
    #[must_use]
    pub const fn from_u32(v: u32) -> Self {
        match v {
            1 => Self::AnchorStart,
            2 => Self::AnchorEnd,
            3 => Self::Centre,
            4 => Self::Proportional,
            _ => Self::Fixed,
        }
    }
}

/// The four edge modes of one element, in the order `ElementDesc` stores them
/// (left, top, right, bottom).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Edges {
    pub left: EdgeMode,
    pub top: EdgeMode,
    pub right: EdgeMode,
    pub bottom: EdgeMode,
}

/// Scale one edge coordinate proportionally.
///
/// **UNVERIFIED.** For mode 4 the state description's size-and-position
/// update is known only to end in a float-to-int truncation; its operands are not established.
/// This implements `trunc(old * newExtent / oldExtent)` and is marked as unverified until they
/// are. `dereth_primitives::num::to_i32_f64` is the truncation the original
/// performs, so at least that half is right.
fn proportional(edge: i32, near: i32, old_extent: i32, new_extent: i32) -> i32 {
    if old_extent == 0 {
        return edge;
    }
    let rel = f64::from(edge - near);
    let scaled = rel * f64::from(new_extent) / f64::from(old_extent);
    near + dereth_primitives::num::to_i32_f64(scaled)
}

/// The state description's size-and-position update, as the retail client does it.
///
/// Takes the *old* reference box and the *new* reference box and rewrites the rectangle. `Δw` and
/// `Δh` are computed from the inclusive boxes:
///
/// ```text
/// dw = (new.x1 - new.x0) - (old.x1 - old.x0)
/// dh = (new.y1 - new.y0) - (old.y1 - old.y0)
/// ```
///
/// Note which quantities the centring uses: **the old width**, `(x1 - x0 + 1) / 2`, computed from
/// the rectangle as it was on entry — the left-edge branch does not disturb the value the
/// right-edge branch reads.
#[must_use]
pub fn update_size_and_position(rect: Box2D, old_ref: Box2D, new_ref: Box2D, e: Edges) -> Box2D {
    let (x0, y0, x1, y1) = (rect.x0, rect.y0, rect.x1, rect.y1);
    let dw = (new_ref.x1 - new_ref.x0) - (old_ref.x1 - old_ref.x0);
    let dh = (new_ref.y1 - new_ref.y0) - (old_ref.y1 - old_ref.y0);
    let new_w = new_ref.x1 - new_ref.x0 + 1;
    let new_h = new_ref.y1 - new_ref.y0 + 1;
    let own_w = x1 - x0 + 1;
    let own_h = y1 - y0 + 1;

    let nx0 = match e.left {
        EdgeMode::AnchorEnd => x0 + dw,
        EdgeMode::Centre => new_w / 2 - own_w / 2,
        // UNVERIFIED: see `proportional`.
        EdgeMode::Proportional => proportional(x0, old_ref.x0, old_ref.x1 - old_ref.x0 + 1, new_w),
        EdgeMode::Fixed | EdgeMode::AnchorStart => x0,
    };
    let nx1 = match e.right {
        EdgeMode::AnchorStart => x1 + dw,
        EdgeMode::Centre => new_w / 2 - 1 + own_w / 2,
        // UNVERIFIED: see `proportional`.
        EdgeMode::Proportional => proportional(x1, old_ref.x0, old_ref.x1 - old_ref.x0 + 1, new_w),
        EdgeMode::Fixed | EdgeMode::AnchorEnd => x1,
    };
    let ny0 = match e.top {
        EdgeMode::AnchorEnd => y0 + dh,
        EdgeMode::Centre => new_h / 2 - own_h / 2,
        // UNVERIFIED: see `proportional`.
        EdgeMode::Proportional => proportional(y0, old_ref.y0, old_ref.y1 - old_ref.y0 + 1, new_h),
        EdgeMode::Fixed | EdgeMode::AnchorStart => y0,
    };
    let ny1 = match e.bottom {
        EdgeMode::AnchorStart => y1 + dh,
        EdgeMode::Centre => new_h / 2 - 1 + own_h / 2,
        // UNVERIFIED: see `proportional`.
        EdgeMode::Proportional => proportional(y1, old_ref.y0, old_ref.y1 - old_ref.y0 + 1, new_h),
        EdgeMode::Fixed | EdgeMode::AnchorEnd => y1,
    };
    Box2D::new(nx0, ny0, nx1, ny1)
}

/// The mode-0 special case of edge adjustment.
///
/// > if the element already has a non-zero width or height, or is already initialised, an edge with
/// > mode 0 keeps the element's *current* box coordinate.
///
/// Retail guards the whole block with "width != 0, or height != 0, or already initialised" and then
/// overrides each of the four coordinates independently.
#[must_use]
pub fn apply_live_mode_zero(
    computed: Box2D,
    live: Box2D,
    e: Edges,
    has_size_or_init: bool,
) -> Box2D {
    if !has_size_or_init {
        return computed;
    }
    Box2D {
        x0: if e.left == EdgeMode::Fixed {
            live.x0
        } else {
            computed.x0
        },
        y0: if e.top == EdgeMode::Fixed {
            live.y0
        } else {
            computed.y0
        },
        x1: if e.right == EdgeMode::Fixed {
            live.x1
        } else {
            computed.x1
        },
        y1: if e.bottom == EdgeMode::Fixed {
            live.y1
        } else {
            computed.y1
        },
    }
}

/// The nine border hot zones used during mouse resizing to decide which edges follow the pointer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BorderLocation {
    #[default]
    None,
    UpperLeft,
    Top,
    UpperRight,
    Right,
    LowerRight,
    Bottom,
    LowerLeft,
    Left,
}

impl BorderLocation {
    /// Which zone a point is in, given the box and the grab margin in pixels.
    ///
    /// The original's zone widths come from the resize-bar child elements rather than from a
    /// constant, so this helper takes the margin: it is the geometry, not a magic number.
    #[must_use]
    pub fn classify(b: Box2D, x: i32, y: i32, margin: i32) -> Self {
        if !b.contains(x, y) {
            return Self::None;
        }
        let left = x - b.x0 < margin;
        let right = b.x1 - x < margin;
        let top = y - b.y0 < margin;
        let bottom = b.y1 - y < margin;
        match (left, top, right, bottom) {
            (true, true, _, _) => Self::UpperLeft,
            (_, true, true, _) => Self::UpperRight,
            (true, _, _, true) => Self::LowerLeft,
            (_, _, true, true) => Self::LowerRight,
            (true, ..) => Self::Left,
            (_, true, ..) => Self::Top,
            (_, _, true, _) => Self::Right,
            (_, _, _, true) => Self::Bottom,
            _ => Self::None,
        }
    }

    /// Which of the four edges this zone drags.
    #[must_use]
    pub const fn edges(self) -> (bool, bool, bool, bool) {
        match self {
            Self::None => (false, false, false, false),
            Self::UpperLeft => (true, true, false, false),
            Self::Top => (false, true, false, false),
            Self::UpperRight => (false, true, true, false),
            Self::Right => (false, false, true, false),
            Self::LowerRight => (false, false, true, true),
            Self::Bottom => (false, false, false, true),
            Self::LowerLeft => (true, false, false, true),
            Self::Left => (true, false, false, false),
        }
    }
}

/// The four size clamps an element declares — attributes `0x3C`..=`0x3F`, read fresh on every
/// motion event by the element's mouse resize and by
/// its `resize_to`.
///
/// `None` means the attribute is **absent**, which is not the same as zero: the client tests
/// whether the integer attribute lookup succeeded before applying each clamp, so an absent maximum does not pin the
/// element to nothing. See [`crate::props::attr::MIN_WIDTH`] for the id-to-name mapping and for
/// the three places that had it backwards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SizeClamps {
    /// `0x3F`.
    pub min_w: Option<i32>,
    /// `0x3D`.
    pub max_w: Option<i32>,
    /// `0x3E`.
    pub min_h: Option<i32>,
    /// `0x3C`.
    pub max_h: Option<i32>,
}

impl SizeClamps {
    /// Apply the clamps to a requested size, each axis minimum first and then maximum, so a
    /// contradictory pair ends at the maximum. An absent bound does not clamp.
    #[must_use]
    pub fn apply(&self, w: i32, h: i32) -> (i32, i32) {
        let mut w = w;
        let mut h = h;
        if let Some(v) = self.min_w {
            w = w.max(v);
        }
        if let Some(v) = self.max_w {
            w = w.min(v);
        }
        if let Some(v) = self.min_h {
            h = h.max(v);
        }
        if let Some(v) = self.max_h {
            h = h.min(v);
        }
        (w, h)
    }
}

/// The element's mouse resize, the geometry half — everything up to but not
/// including the parent clamp and the `resize_to`/`move_to` pair, which need the tree.
///
/// **This is not "move the edges the zone selects".** Three things that shape cannot express,
/// all of them load-bearing and all of them established against retail:
///
/// 1. **Every arm is computed from the drag *origin*, not incrementally from the live box.**
///    The drag-start origin and size and the initial mouse position are latched by
///    the element's start-resizing latch and the whole rectangle is re-derived from them on
///    each motion event, so a drag that runs into a clamp and comes back out again lands exactly
///    where the pointer says. An incremental form loses the excess against the clamp for ever.
/// 2. **There are four clamps, not two**, and they are attributes rather than parameters.
/// 3. **Shift snaps all four coordinates** down to a multiple of 10, which
///    [`crate::UiSystem::mouse_move_element`]'s equivalent does only for two — and there only
///    conditionally. The input manager's shift-key state is the source in both.
///
/// `start` is the drag-start `(x, y, width, height)`; `dx`/`dy` are
/// `mouse - initial mouse`. The result is `(x, y, w, h)` in the parent's coordinates.
///
/// The client works in `(left, top, right, bottom)` with an **exclusive** far edge here
/// (`right = start x + start width`), which is why this returns a size rather than a
/// [`Box2D`]: our `Box2D` is inclusive, and converting twice inside the arms is exactly where an
/// off-by-one would hide.
#[must_use]
#[allow(clippy::similar_names)]
pub fn mouse_resize(
    start: (i32, i32, i32, i32),
    border: BorderLocation,
    dx: i32,
    dy: i32,
    c: SizeClamps,
    shift: bool,
) -> (i32, i32, i32, i32) {
    let (sx, sy, sw, sh) = start;
    let mut left = sx;
    let mut top = sy;
    let mut right = sx + sw;
    let mut bottom = sy + sh;

    // The near-edge forms: the clamp moves the edge that is following the mouse.
    let clamp_left = |left: &mut i32, right: i32| {
        if let Some(m) = c.min_w {
            if right - *left < m {
                *left = right - m;
            }
        }
        if let Some(m) = c.max_w {
            if m < right - *left {
                *left = right - m;
            }
        }
    };
    let clamp_right = |left: i32, right: &mut i32| {
        if let Some(m) = c.min_w {
            if *right - left < m {
                *right = m + left;
            }
        }
        if let Some(m) = c.max_w {
            if m < *right - left {
                *right = m + left;
            }
        }
    };
    let clamp_top = |top: &mut i32, bottom: i32| {
        if let Some(m) = c.min_h {
            if bottom - *top < m {
                *top = bottom - m;
            }
        }
        if let Some(m) = c.max_h {
            if m < bottom - *top {
                *top = bottom - m;
            }
        }
    };
    let clamp_bottom = |top: i32, bottom: &mut i32| {
        if let Some(m) = c.min_h {
            if *bottom - top < m {
                *bottom = m + top;
            }
        }
        if let Some(m) = c.max_h {
            if m < *bottom - top {
                *bottom = m + top;
            }
        }
    };

    match border {
        BorderLocation::None => return (sx, sy, sw, sh),
        BorderLocation::UpperLeft => {
            top += dy;
            left += dx;
            clamp_left(&mut left, right);
            clamp_top(&mut top, bottom);
        }
        BorderLocation::Top => {
            top += dy;
            clamp_top(&mut top, bottom);
        }
        BorderLocation::UpperRight => {
            right += dx;
            top += dy;
            clamp_right(left, &mut right);
            clamp_top(&mut top, bottom);
        }
        BorderLocation::Right => {
            right += dx;
            clamp_right(left, &mut right);
        }
        BorderLocation::LowerRight => {
            right += dx;
            bottom = sy + dy + sh;
            clamp_right(left, &mut right);
            clamp_bottom(top, &mut bottom);
        }
        BorderLocation::Bottom => {
            bottom = sy + dy + sh;
            clamp_bottom(top, &mut bottom);
        }
        BorderLocation::LowerLeft => {
            bottom = sy + dy + sh;
            left += dx;
            clamp_left(&mut left, right);
            clamp_bottom(top, &mut bottom);
        }
        BorderLocation::Left => {
            left += dx;
            clamp_left(&mut left, right);
        }
    }

    if shift {
        for v in [&mut left, &mut top, &mut right, &mut bottom] {
            *v -= *v % SHIFT_SNAP;
        }
    }
    if left < 0 {
        left = 0;
    }
    if top < 0 {
        top = 0;
    }
    (left, top, right - left, bottom - top)
}

/// The grid a held shift snaps a dragged or resized window on to — `x - x % 10` in both the
/// element's mouse move and its mouse resize.
///
/// It rounds **toward zero**, not down, because C's `%` truncates; the two agree for the
/// non-negative coordinates the clamps below leave behind, and Rust's `%` truncates the same way.
pub const SHIFT_SNAP: i32 = 10;

#[cfg(test)]
mod tests {
    use super::*;

    const REF800: Box2D = Box2D {
        x0: 0,
        y0: 0,
        x1: 799,
        y1: 599,
    };
    const REF1920: Box2D = Box2D {
        x0: 0,
        y0: 0,
        x1: 1919,
        y1: 1079,
    };

    fn e(l: u32, t: u32, r: u32, b: u32) -> Edges {
        Edges {
            left: EdgeMode::from_u32(l),
            top: EdgeMode::from_u32(t),
            right: EdgeMode::from_u32(r),
            bottom: EdgeMode::from_u32(b),
        }
    }

    /// Oracle: the state description's size-and-position update. Mode 1 on
    /// the right edge is `x1 += dw`; the left edge is untouched by modes 0 and 1 alike.
    #[test]
    fn mode_one_stretches_the_far_edge_by_the_delta() {
        // The hollow root: all four edges mode 1, full screen at the design size.
        let r =
            update_size_and_position(Box2D::new(0, 0, 799, 599), REF800, REF1920, e(1, 1, 1, 1));
        assert_eq!(r, Box2D::new(0, 0, 1919, 1079));
        assert_eq!(r.width(), 1920);
        assert_eq!(r.height(), 1080);
    }

    /// Oracle: same function, the mode-2 branch — the near edge follows the delta and the
    /// far edge does not, which pins a widget to the right/bottom of its parent.
    #[test]
    fn mode_two_pins_to_the_far_side() {
        let r = update_size_and_position(
            Box2D::new(700, 500, 799, 599),
            REF800,
            REF1920,
            e(2, 2, 0, 0),
        );
        assert_eq!(r, Box2D::new(700 + 1120, 500 + 480, 799, 599));
    }

    /// Oracle: the `== 3` branches, `x0 = new_w/2 - w/2` and `x1 = new_w/2 - 1 + w/2`, with the
    /// integer halving `(x1 - x0 + 1) / 2` — contract 11.4 and trap 6 of the track spec.
    ///
    /// The odd-width case is the one that catches an off-by-one: a 101-wide box centred in 1920
    /// lands at 960-50=910 .. 960-1+50=1009, i.e. it stays 100 wide, one *less* than it was. That
    /// is the client's arithmetic, not a mistake here.
    #[test]
    fn mode_three_centres_with_integer_halving() {
        let even =
            update_size_and_position(Box2D::new(0, 0, 99, 49), REF800, REF1920, e(3, 3, 3, 3));
        assert_eq!(
            even,
            Box2D::new(960 - 50, 540 - 25, 960 - 1 + 50, 540 - 1 + 25)
        );
        assert_eq!(even.width(), 100);

        let odd =
            update_size_and_position(Box2D::new(0, 0, 100, 0), REF800, REF1920, e(3, 0, 3, 0));
        assert_eq!(odd.x0, 960 - 50);
        assert_eq!(odd.x1, 960 - 1 + 50);
        assert_eq!(
            odd.width(),
            100,
            "101 truncates to 100 through two integer halvings"
        );
    }

    /// Oracle: the initialized-element exception in the size-and-position update, guarded by
    /// "width != 0, or height != 0, or already initialised".
    #[test]
    fn mode_zero_keeps_the_live_value_once_the_element_has_a_size() {
        let design = Box2D::new(10, 10, 109, 59);
        let live = Box2D::new(500, 300, 599, 349); // the user dragged the window here
        let computed = update_size_and_position(design, REF800, REF1920, e(0, 0, 0, 0));
        assert_eq!(computed, design, "mode 0 alone is the design rectangle");
        assert_eq!(
            apply_live_mode_zero(computed, live, e(0, 0, 0, 0), true),
            live,
            "an initialised element keeps where the user put it"
        );
        assert_eq!(
            apply_live_mode_zero(computed, live, e(0, 0, 0, 0), false),
            design,
            "before it has a size, the design rectangle wins"
        );
        // Only the mode-0 edges are overridden.
        let mixed = e(0, 1, 1, 1);
        let c2 = update_size_and_position(design, REF800, REF1920, mixed);
        let r = apply_live_mode_zero(c2, live, mixed, true);
        assert_eq!(r.x0, live.x0);
        assert_eq!(r.y0, c2.y0);
    }

    /// Mode four is the documented approximation.
    #[test]
    fn mode_four_is_the_documented_approximation() {
        // 400 of 800 -> half of 1920.
        let r =
            update_size_and_position(Box2D::new(400, 0, 400, 0), REF800, REF1920, e(4, 0, 4, 0));
        assert_eq!(r.x0, 960);
        // 1/3 of the way across truncates toward zero, as the original's float-to-int does.
        let r =
            update_size_and_position(Box2D::new(267, 0, 267, 0), REF800, REF1920, e(4, 0, 4, 0));
        assert_eq!(r.x0, 640); // 267 * 1920 / 800 = 640.8 -> 640
    }

    /// Oracle: the nine `BorderLocation` values and the client border-position calculations.
    #[test]
    fn border_zones_select_the_edges_that_follow_the_mouse() {
        let b = Box2D::new(0, 0, 99, 99);
        assert_eq!(
            BorderLocation::classify(b, 1, 1, 4),
            BorderLocation::UpperLeft
        );
        assert_eq!(
            BorderLocation::classify(b, 98, 98, 4),
            BorderLocation::LowerRight
        );
        assert_eq!(BorderLocation::classify(b, 50, 1, 4), BorderLocation::Top);
        assert_eq!(BorderLocation::classify(b, 50, 50, 4), BorderLocation::None);
        assert_eq!(
            BorderLocation::classify(b, 200, 50, 4),
            BorderLocation::None
        );
    }

    /// Oracle: the element's mouse resize, arm by arm.
    ///
    /// `start` is the drag-start `(x, y, width, height)`.
    ///
    #[test]
    fn a_resize_drag_is_re_derived_from_the_drag_origin_every_time() {
        const START: (i32, i32, i32, i32) = (100, 100, 200, 150);
        let none = SizeClamps::default();

        // Lower-right: both far edges follow, the origin does not.
        assert_eq!(
            mouse_resize(START, BorderLocation::LowerRight, 10, 20, none, false),
            (100, 100, 210, 170)
        );
        // Upper-left: both near edges follow, the far edges do not, so the size shrinks by the
        // same delta the origin gained.
        assert_eq!(
            mouse_resize(START, BorderLocation::UpperLeft, 10, 20, none, false),
            (110, 120, 190, 130)
        );
        // One edge at a time.
        assert_eq!(
            mouse_resize(START, BorderLocation::Right, 10, 20, none, false),
            (100, 100, 210, 150)
        );
        assert_eq!(
            mouse_resize(START, BorderLocation::Bottom, 10, 20, none, false),
            (100, 100, 200, 170)
        );
        assert_eq!(
            mouse_resize(START, BorderLocation::Left, 10, 20, none, false),
            (110, 100, 190, 150)
        );
        assert_eq!(
            mouse_resize(START, BorderLocation::Top, 10, 20, none, false),
            (100, 120, 200, 130)
        );
        // No border is the function's first check: with no current border it returns at once.
        assert_eq!(
            mouse_resize(START, BorderLocation::None, 10, 20, none, false),
            START
        );

        // **The property that an incremental form cannot have.** A drag far past the minimum and
        // back to +10 lands on exactly the same rectangle as a single +10, because every arm is
        // computed from the origin rather than from the live box.
        let c = SizeClamps {
            min_w: Some(50),
            max_w: None,
            min_h: Some(40),
            max_h: None,
        };
        let squashed = mouse_resize(START, BorderLocation::LowerRight, -900, -900, c, false);
        assert_eq!(
            squashed,
            (100, 100, 50, 40),
            "the two minima clamp rather than inverting the box"
        );
        assert_eq!(
            mouse_resize(START, BorderLocation::LowerRight, 10, 20, c, false),
            (100, 100, 210, 170),
            "and the excess against the clamp is not lost"
        );

        // All four clamps.
        let c = SizeClamps {
            min_w: Some(50),
            max_w: Some(120),
            min_h: Some(40),
            max_h: Some(90),
        };
        assert_eq!(
            mouse_resize(START, BorderLocation::LowerRight, 500, 500, c, false),
            (100, 100, 120, 90)
        );
        assert_eq!(
            mouse_resize(START, BorderLocation::UpperLeft, -500, -500, c, false),
            (180, 160, 120, 90)
        );

        // **The order of the two width clamps, and it was written down backwards.** Every arm of
        // the mouse resize reads `0x3F` (minimum) **first** and `0x3D` (maximum)
        // **second** — the right border's pair is read in that order — so when a layout
        // declares a minimum larger than its maximum the **maximum** is what survives, because it
        // is applied last. The heights are the same order: `0x3E` then `0x3C`.
        //
        // This is asserted rather than described because it is unreachable from the shipped data
        // (no layout declares a contradictory pair) and therefore invisible to every other test in
        // this file. Kept and pinned explicitly
        // because it is the client's.
        let contradictory = SizeClamps {
            min_w: Some(200),
            max_w: Some(120),
            min_h: Some(200),
            max_h: Some(90),
        };
        assert_eq!(
            mouse_resize(
                START,
                BorderLocation::LowerRight,
                0,
                0,
                contradictory,
                false
            ),
            (100, 100, 120, 90),
            "min is applied first and max second, so max wins a contradiction"
        );

        assert_eq!(
            mouse_resize(START, BorderLocation::UpperLeft, -500, -500, none, false),
            (0, 0, 300, 250),
            "the near edges floor at zero and the far edges stay where they were"
        );

        // The shift snap rounds all four coordinates toward zero on a multiple of ten.
        // left 100+3=103 -> 100; top 100+7=107 -> 100; right/bottom untouched by an upper-left drag
        // (300 and 250, both already multiples of ten).
        assert_eq!(
            mouse_resize(START, BorderLocation::UpperLeft, 3, 7, none, true),
            (100, 100, 200, 150)
        );
        // A far-edge drag snaps the far edges instead: right 300+7=307 -> 300, bottom 250+3 -> 250.
        assert_eq!(
            mouse_resize(START, BorderLocation::LowerRight, 7, 3, none, true),
            (100, 100, 200, 150)
        );
        assert_eq!(
            mouse_resize(START, BorderLocation::LowerRight, 17, 13, none, true),
            (100, 100, 210, 160)
        );
    }

    /// [`BorderLocation::edges`] is a table of which edges each zone drags; the arms of
    /// [`mouse_resize`] are written independently of it. Asserting that the two agree is what
    /// stops one of them drifting: a wrong arm shows up as an edge that moved and should not have.
    #[test]
    fn every_arm_moves_exactly_the_edges_the_zone_table_names() {
        const START: (i32, i32, i32, i32) = (100, 100, 200, 150);
        let all = [
            BorderLocation::None,
            BorderLocation::UpperLeft,
            BorderLocation::Top,
            BorderLocation::UpperRight,
            BorderLocation::Right,
            BorderLocation::LowerRight,
            BorderLocation::Bottom,
            BorderLocation::LowerLeft,
            BorderLocation::Left,
        ];
        for b in all {
            let (x, y, w, h) = mouse_resize(START, b, 11, 13, SizeClamps::default(), false);
            let (l, t, r, bo) = b.edges();
            assert_eq!(x != START.0, l, "{b:?}: left");
            assert_eq!(y != START.1, t, "{b:?}: top");
            assert_eq!(x + w != START.0 + START.2, r, "{b:?}: right");
            assert_eq!(y + h != START.1 + START.3, bo, "{b:?}: bottom");
        }
    }
}
