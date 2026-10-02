//! `VividTargetIndicator` — the on-screen corner brackets and the off-screen direction arrows.
//!
//! The brackets take their colour from **the target's radar blip colour** — the target brackets use
//! the radar palette, which is why [`crate::mapradar::radar`] owns the colour and this module only
//! places things.

use dereth_primitives::num::math;
use dereth_primitives::ObjectId;

/// The twelve source images, looked up by enum `i` in group `0x10000009`, DAT type `0x0C`, for
/// `i` = 1…12, into a 13-slot image array (**index 0 unused**).
///
/// "Images 1–4 are the four on-screen corner brackets, 5–12 the eight off-screen direction arrows."
pub const SOURCE_IMAGE_COUNT: usize = 12;
/// The enum group and type the images come from.
pub const IMAGE_ENUM_GROUP: u32 = 0x1000_0009;
/// See [`IMAGE_ENUM_GROUP`].
pub const IMAGE_DAT_TYPE: u32 = 0x0C;

/// The off-screen element, child `0x10000045` of the smart-box element.
pub const OFF_SCREEN_ELEMENT: dereth_ui::ElementId = dereth_ui::ElementId(0x1000_0045);

/// The two displayable results of the smart box's object bounding-box query; any UI's target
/// indicator reads them, so they are the contract's.
pub use dereth_client_contract::target::Projection;

/// The inset every clamp uses.
pub const INSET: i32 = 8;

/// The object-select status values this module branches on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectStatus {
    /// On screen — draw the four corner brackets around the object's rectangle.
    OnScreen,
    /// Off screen — draw one of the eight edge arrows. Invalid/not-found
    /// are distinct return values and do not enter either display arm.
    OffScreen,
}

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

/// The actual shipped SBOX consumer. `None` projection is retail's NotFound/Invalid
/// return (leave existing presentation alone); a disabled/empty selection hides both.
pub fn draw(
    ui: &mut dereth_ui::UiSystem,
    root: dereth_ui::ElemHandle,
    state: VividTargetIndicator,
    projection: Option<Projection>,
    color: u32,
    viewport: (i32, i32),
) -> bool {
    use crate::screens::gameplay::window;
    use dereth_ui::{
        region::{GraphicRef, SurfaceOp},
        ElementId,
    };
    let Some(sbox) = ui.get_child_recursive(root, window::SMART_BOX) else {
        return false;
    };
    let Some(on) = ui.get_child(sbox, window::TARGET_ON_SCREEN) else {
        return false;
    };
    let Some(off) = ui.get_child(sbox, OFF_SCREEN_ELEMENT) else {
        return false;
    };
    if !state.should_draw() {
        let changed = ui.node(on).is_some_and(|n| n.region.flags.visible)
            || ui.node(off).is_some_and(|n| n.region.flags.visible);
        ui.set_visible(on, false);
        ui.set_visible(off, false);
        return changed;
    }
    let Some(projection) = projection else {
        return false;
    };
    let image = |ui: &mut dereth_ui::UiSystem, h, index| {
        if let (Some(did), Some(n)) = (
            ui.env()
                .cloned()
                .and_then(|e| e.did_by_enum(IMAGE_ENUM_GROUP, index)),
            ui.node_mut(h),
        ) {
            let b = n.region.box_;
            let mut g = GraphicRef::opaque_surface(did, b.width(), b.height());
            g.op = Some(SurfaceOp::Colorize(color));
            n.region.image = Some(g);
        }
    };
    match projection {
        Projection::OnScreen(rect) => {
            let Some(first) = ui.get_child(on, ElementId(0x1000_0039)) else {
                return false;
            };
            let b = ui.node(first).expect("live child").region.box_;
            let (x, y, w, h) = on_screen_box(rect, (b.width(), b.height()), viewport);
            for i in 1..=4 {
                if let Some(c) = ui.get_child(on, ElementId(0x1000_0038 + i)) {
                    image(ui, c, i);
                }
            }
            ui.move_to(on, x, y);
            ui.resize_to(on, w, h);
            ui.set_visible(on, true);
            ui.set_visible(off, false);
        }
        Projection::OffScreen(heading) => {
            image(
                ui,
                off,
                u32::try_from(off_screen_image(heading)).expect("source image 5..12"),
            );
            let b = ui.node(off).expect("live child").region.box_;
            let (x, y) = off_screen_position(heading, (b.width(), b.height()), viewport);
            ui.move_to(off, x, y);
            ui.set_visible(on, false);
            ui.set_visible(off, true);
        }
    }
    true
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

/// The indicator's live state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VividTargetIndicator {
    /// Enabled — cleared while the UI is hidden.
    pub enabled: bool,
    /// Display on, mirroring the `VividTargetingIndicator` character option
    /// (the display-state update, on the player-option-changed notice).
    pub display_on: bool,
    /// The current target, or `None`.
    pub target: Option<ObjectId>,
}

impl Default for VividTargetIndicator {
    fn default() -> Self {
        // Constructor: enabled, but awaiting the player option and a target.
        Self {
            enabled: true,
            display_on: false,
            target: None,
        }
    }
}

impl VividTargetIndicator {
    /// The draw's first line: "if disabled, turned off or no target, both elements are
    /// hidden".
    #[must_use]
    pub fn should_draw(&self) -> bool {
        self.enabled && self.display_on && self.target.is_some()
    }

    /// Selecting an object's first rule: "selecting yourself, an object you own, or an
    /// object in a container clears the selection instead".
    pub fn set_selected(
        &mut self,
        iid: ObjectId,
        is_self: bool,
        is_owned: bool,
        is_in_container: bool,
    ) {
        self.target = if is_self || is_owned || is_in_container {
            None
        } else {
            Some(iid)
        };
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

    /// Oracle: the draw's actual comparisons. The old §8.1
    /// summary incorrectly applied right-1 last; retail applies it only for an inverted
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

    /// Oracle: the draw's first line and the selection rule.
    #[test]
    fn the_indicator_hides_unless_all_three_conditions_hold_and_refuses_three_targets() {
        let mut v = VividTargetIndicator::default();
        assert!(!v.should_draw());
        v.enabled = true;
        v.display_on = true;
        assert!(!v.should_draw(), "no target");
        v.set_selected(ObjectId(7), false, false, false);
        assert_eq!(v.target, Some(ObjectId(7)));
        assert!(v.should_draw());
        v.display_on = false;
        assert!(!v.should_draw(), "the character option turns it off");

        for (is_self, owned, contained) in [
            (true, false, false),
            (false, true, false),
            (false, false, true),
        ] {
            let mut v = VividTargetIndicator {
                enabled: true,
                display_on: true,
                target: None,
            };
            v.set_selected(ObjectId(7), is_self, owned, contained);
            assert_eq!(v.target, None, "selecting it clears the selection instead");
        }
    }

    /// Oracle: §8.1's constructor paragraph — twelve images in thirteen slots, index 0 unused.
    #[test]
    fn the_twelve_source_images_come_from_the_documented_enum_group() {
        assert_eq!(SOURCE_IMAGE_COUNT, 12);
        assert_eq!(IMAGE_ENUM_GROUP, 0x1000_0009);
        assert_eq!(IMAGE_DAT_TYPE, 0x0C);
        assert_eq!(OFF_SCREEN_ELEMENT, dereth_ui::ElementId(0x1000_0045));
        // Images 1..4 are the corners; the eight arrows are exactly the ones off_screen_image uses.
        for i in 1..=4 {
            assert!(!(5..=12).contains(&i));
        }
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
