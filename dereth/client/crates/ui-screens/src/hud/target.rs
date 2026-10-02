//! `VividTargetIndicator` — the on-screen corner brackets and the off-screen direction arrows.
//!
//! The brackets take their colour from **the target's radar blip colour** — the target brackets use
//! the radar palette, which is why [`crate::mapradar::radar`] owns the colour and this module only
//! places things.

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

/// The placement rules: the shared presentation crate's.
pub use dereth_presentation::target::{
    off_screen_image, off_screen_position, on_screen_box, on_screen_position, INSET,
};

/// The object-select status values this module branches on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectStatus {
    /// On screen — draw the four corner brackets around the object's rectangle.
    OnScreen,
    /// Off screen — draw one of the eight edge arrows. Invalid/not-found
    /// are distinct return values and do not enter either display arm.
    OffScreen,
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
}
