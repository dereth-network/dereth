//! Where the target indicator goes: the on-screen brackets' box around the target's rectangle,
//! and the off-screen arrow's edge position and image, from the target's heading.

use dereth_primitives::num::math;

/// The inset every clamp uses.
pub const INSET: i32 = 8;

/// The draw's on-screen placement:
///
/// First correct an inverted endpoint, then apply the near and far viewport clamps.
/// Retail applies them in this order, and the order matters.
#[must_use]
pub fn on_screen_position(
    rect: (i32, i32, i32, i32),
    corner: (i32, i32),
    viewport: (i32, i32),
) -> (i32, i32) {
    let (left, top, right, bottom) = rect;
    let x = left - corner.0;
    let y = top - corner.1;
    let x = if x > right { right - 1 } else { x };
    let y = if y > bottom { bottom - 1 } else { y };
    let x = x.max(INSET).min(viewport.0 - 2 * corner.0 - INSET);
    let y = y.max(INSET).min(viewport.1 - 2 * corner.1 - INSET);
    (x, y)
}

/// The draw also clamps the right/bottom endpoints, then
/// resizes to (right - x + cornerW, bottom - y + cornerH). DAT anchors place
/// the other three corners; manually positioning them would bypass retail layout.
#[must_use]
pub fn on_screen_box(
    rect: (i32, i32, i32, i32),
    corner: (i32, i32),
    viewport: (i32, i32),
) -> (i32, i32, i32, i32) {
    let (x, y) = on_screen_position(rect, corner, viewport);
    let right = rect
        .2
        .max(corner.0 + INSET)
        .min(viewport.0 - corner.0 - INSET);
    let bottom = rect
        .3
        .max(corner.1 + INSET)
        .min(viewport.1 - corner.1 - INSET);
    (x, y, right - x + corner.0, bottom - y + corner.1)
}

/// The off-screen arrow: octant-specific ray/edge intersection followed
/// by the 8px clamp. The edge switches are at 45 degrees, not viewport aspect.
#[must_use]
pub fn off_screen_position(heading: f32, image: (i32, i32), viewport: (i32, i32)) -> (i32, i32) {
    let a = f64::from(heading);
    let (w, h) = (f64::from(viewport.0), f64::from(viewport.1));
    let (iw, ih) = (f64::from(image.0), f64::from(image.1));
    // A shipped `f32` constant, promoted to double for the calculation.
    let tan = |degrees: f64| math::tan(degrees * f64::from(0.017_453_292_f32));
    let (x, y) = if a <= 45.0 {
        (w * 0.5 + h * 0.5 * tan(a) - iw * 0.5, 8.0)
    } else if a <= 90.0 {
        (w - iw - 8.0, h * 0.5 - w * 0.5 * tan(90.0 - a) - ih * 0.5)
    } else if a <= 135.0 {
        (w - iw - 8.0, h * 0.5 + w * 0.5 * tan(a - 90.0) - ih * 0.5)
    } else if a <= 180.0 {
        (w * 0.5 + h * 0.5 * tan(180.0 - a) - iw * 0.5, h - ih - 8.0)
    } else if a <= 225.0 {
        (w * 0.5 - h * 0.5 * tan(a - 180.0) - iw * 0.5, h - ih - 8.0)
    } else if a <= 270.0 {
        (8.0, h * 0.5 + w * 0.5 * tan(270.0 - a) - ih * 0.5)
    } else if a <= 315.0 {
        (8.0, h * 0.5 - w * 0.5 * tan(a - 270.0) - ih * 0.5)
    } else {
        (w * 0.5 - h * 0.5 * tan(360.0 - a) - iw * 0.5, 8.0)
    };
    // Both coordinates are stored as float before the clamp,
    // then truncated to integer. Retaining f64 through truncation loses pixels near an integer.
    #[allow(clippy::cast_possible_truncation)]
    let pixel = |v: f64, max: f64| {
        let v = v as f32;
        let max = max as f32;
        // Source checks the maximum first, returning immediately.
        dereth_primitives::num::to_i32(if v > max {
            max
        } else if v < 8.0 {
            8.0
        } else {
            v
        })
    };
    (pixel(x, w - iw - 8.0), pixel(y, h - ih - 8.0))
}

/// The draw's off-screen arrow choice, from the heading in 45° sectors.
///
/// | heading (deg) | image |
/// |---|---|
/// | `< 23` or `≥ 338` | 6 |
/// | `23 … 68` | 7 |
/// | `68 … 113` | 9 |
/// | `113 … 158` | 12 |
/// | `158 … 203` | 11 |
/// | `203 … 248` | 10 |
/// | `248 … 293` | 8 |
/// | `293 … 338` | 5 |
#[must_use]
pub fn off_screen_image(heading_degrees: f32) -> usize {
    let h = heading_degrees.rem_euclid(360.0);
    if h < 23.0 || h >= 338.0 {
        6
    } else if h < 68.0 {
        7
    } else if h < 113.0 {
        9
    } else if h < 158.0 {
        12
    } else if h < 203.0 {
        11
    } else if h < 248.0 {
        10
    } else if h < 293.0 {
        8
    } else {
        5
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the recovered HUD behavior's heading table — every sector boundary, in both
    /// directions, plus the wrap at 0/360.
    #[test]
    fn the_eight_off_screen_arrows_cover_the_documented_sectors() {
        assert_eq!(off_screen_image(0.0), 6);
        assert_eq!(off_screen_image(22.9), 6);
        assert_eq!(off_screen_image(23.0), 7);
        assert_eq!(off_screen_image(67.9), 7);
        assert_eq!(off_screen_image(68.0), 9);
        assert_eq!(off_screen_image(112.9), 9);
        assert_eq!(off_screen_image(113.0), 12);
        assert_eq!(off_screen_image(157.9), 12);
        assert_eq!(off_screen_image(158.0), 11);
        assert_eq!(off_screen_image(202.9), 11);
        assert_eq!(off_screen_image(203.0), 10);
        assert_eq!(off_screen_image(247.9), 10);
        assert_eq!(off_screen_image(248.0), 8);
        assert_eq!(off_screen_image(292.9), 8);
        assert_eq!(off_screen_image(293.0), 5);
        assert_eq!(off_screen_image(337.9), 5);
        assert_eq!(off_screen_image(338.0), 6, "the sector wraps through north");
        assert_eq!(off_screen_image(359.9), 6);
        assert_eq!(off_screen_image(360.0), 6);
        assert_eq!(off_screen_image(-1.0), 6, "a negative heading normalises");
        // All eight images are in 5..=12, and all eight are used.
        let mut used: Vec<usize> = (0..360).map(|d| off_screen_image(d as f32)).collect();
        used.sort_unstable();
        used.dedup();
        assert_eq!(used, vec![5, 6, 7, 8, 9, 10, 11, 12]);
    }

    /// Oracle: the draw's actual comparisons. Retail applies it only for an inverted
    /// endpoint, before both viewport clamps.
    #[test]
    fn the_on_screen_brackets_clamp_to_the_viewport_and_to_the_targets_own_rectangle() {
        let vp = (800, 600);
        let corner = (16, 16);
        // Comfortably inside: the bracket sits one corner up and left of the rectangle.
        assert_eq!(
            on_screen_position((100, 200, 140, 240), corner, vp),
            (84, 184)
        );
        // Against the top-left: the 8-pixel inset takes over.
        assert_eq!(on_screen_position((0, 0, 40, 40), corner, vp), (8, 8));
        // Against the bottom-right: the far clamp is `viewport − 2·corner − 8`.
        let (x, y) = on_screen_position((790, 590, 900, 900), corner, vp);
        assert_eq!((x, y), (800 - 32 - 8, 600 - 32 - 8));
        // A tiny rectangle still takes the near clamp, not an extra final right-1 clamp.
        let (x, _) = on_screen_position((0, 0, 3, 3), corner, vp);
        assert_eq!(x, 8, "near clamp comes after inverted-endpoint correction");
    }

    #[test]
    fn retail_bracket_extent_and_arrow_edges_keep_the_source_clamp_order() {
        assert_eq!(
            on_screen_box((100, 200, 140, 240), (16, 16), (800, 600)),
            (84, 184, 72, 72)
        );
        assert_eq!(
            on_screen_box((0, 0, 3, 3), (16, 16), (800, 600)),
            (8, 8, 32, 32)
        );
        assert_eq!(
            on_screen_box((790, 590, 900, 900), (16, 16), (800, 600)),
            (760, 560, 32, 32)
        );
        // Sequential max/min also defines a tiny viewport; Rust clamp() would panic.
        assert_eq!(
            on_screen_position((0, 0, 3, 3), (16, 16), (20, 20)),
            (-20, -20)
        );
        assert_eq!(off_screen_position(0.0, (16, 16), (800, 600)), (392, 8));
        assert_eq!(off_screen_position(90.0, (16, 16), (800, 600)), (776, 292));
        assert_eq!(off_screen_position(180.0, (16, 16), (800, 600)), (392, 576));
        assert_eq!(off_screen_position(270.0, (16, 16), (800, 600)), (8, 292));
        // Storing to float rounds 414.999998... to 415 before the integer truncation.
        assert_eq!(off_screen_position(4.3841, (16, 16), (800, 600)), (415, 8));
    }
}
