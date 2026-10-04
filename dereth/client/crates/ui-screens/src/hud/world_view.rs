//! `WorldView` and `WorldViewWrapper` — the 3D viewport's rectangle.
//!
//! The world is not drawn "behind" the UI: it is an element. This module owns the *rectangle*;
//! the world renderer supplies what is drawn inside it.

use dereth_primitives::DataId;
use dereth_ui::{Box2D, ElemHandle, ElementId, UiSystem};

/// The FPS / degrade read-out.
pub const FPS_DISPLAY: ElementId = ElementId(0x1000_0047);
/// The portal space, a `Viewport` holding the teleport object.
pub const PORTAL_SPACE: ElementId = ElementId(0x1000_0436);

/// The string the FPS meter builds, from table enum `0x10000001`, with two float
/// variables rendered to **2 decimals**.
pub const FPS_STRING: &str = "ID_SmartBox_FPS";
/// The two variable names.
pub const FPS_VARIABLES: [&str; 2] = ["fps_var", "deg_var"];
/// The DID-by-enum group holding the gameplay string tables.
pub const FPS_STRING_TABLE_GROUP: u32 = 4;
/// The table enum the FPS meter builds its string in.
pub const FPS_STRING_TABLE_ENUM: u32 = 0x1000_0001;
/// The shipped September 2013 answer for group 4 / enum `0x10000001`.
pub const FPS_STRING_TABLE: DataId = DataId(0x2300_0001);

/// The teleport object's lighting and camera.
pub mod teleport {
    /// The DID-by-enum argument that yields the teleport object.
    pub const OBJECT_ENUM: u32 = 0x1000_0001;
    /// One `DISTANT_LIGHT` of this intensity.
    pub const LIGHT_INTENSITY: f32 = 2.0;
    /// The light direction.
    pub const LIGHT_DIRECTION: (f32, f32, f32) = (0.3, -1.9, 0.65);
    /// The camera position.
    pub const CAMERA_POSITION: (f32, f32, f32) = (0.24, -2.7, 0.88);
}

/// `deg_var` is `auto_update ? deg_mul : user_bias`, the bias being the user-supplied degrade bias.
#[must_use]
pub fn deg_var(auto_update: bool, deg_mul: f32, user_bias: f32) -> f32 {
    if auto_update {
        deg_mul
    } else {
        user_bias
    }
}

/// The FPS meter's two variables, rendered to two decimals.
#[must_use]
pub fn fps_text(framerate: f32, deg: f32) -> String {
    format!("{framerate:.2} {deg:.2}")
}

/// Resolve the FPS meter's `StringInfo` through the installed localized table.
///
/// The float-variable insert receives `fps_var` and `deg_var`, in that order, with precision 2. The
/// string table owns the punctuation and surrounding words; the two formatted values are the
/// only literals supplied here. A missing DidMapper entry uses the measured shipped table; `None`
/// means its localized row was unavailable, in which case the existing element text is preserved
/// rather than replaced with an invented HUD.
#[must_use]
pub fn localized_fps_text(ui: &UiSystem, framerate: f32, deg: f32) -> Option<String> {
    let table = ui
        .env()
        .cloned()
        .and_then(|e| e.did_by_enum(FPS_STRING_TABLE_GROUP, FPS_STRING_TABLE_ENUM))
        .unwrap_or(FPS_STRING_TABLE);
    // The shipped row's own variable names are `[fps, deg]` [measured, 0x23000001] — and the names
    // are what the client matches on, not the order this call site writes them in.
    let (fps, deg) = (format!("{framerate:.2}"), format!("{deg:.2}"));
    ui.resolve_string_named(
        table,
        dereth_primitives::num::hash::str_hash(FPS_STRING.as_bytes()),
        &[("fps", fps.as_str()), ("deg", deg.as_str())],
    )
}

/// The FPS meter's final set-string effect.
///
/// Returns whether the localized row reached a real text element. Visibility is deliberately
/// owned by the receiver: native updates the text before showing the child on the enable edge,
/// then updates it every frame without another visibility write.
pub fn update_fps_meter(ui: &mut UiSystem, display: ElemHandle, framerate: f32, deg: f32) -> bool {
    let Some(text) = localized_fps_text(ui, framerate, deg) else {
        return false;
    };
    let Some(element) = ui.text_element_mut(display) else {
        return false;
    };
    element.set_text(&text);
    true
}

/// `WorldViewWrapper` -- the element whose screen box **is** the 3D viewport.
///
/// Re-exported from the gameplay screen so that the one caller who needs the rectangle asks this
/// module for it rather than reaching across into the screen's own id table.
pub const SMART_BOX: ElementId = ElementId(0x1000_049A);

/// The client rect the smart box reports to the renderer -- the element's absolute, **inclusive**
/// screen box.
///
/// # What actually pushes it
///
/// The smart box's resize and move do **not** push the new rectangle to the renderer. The resize persists the chat window's width and
/// height properties (`0x10000088` / `0x10000089`) and touches no viewport. In retail the real
/// chain is:
///
/// ```text
/// a docked object moves or resizes (virtual screen position set)
/// -> the clamped position is recalculated
/// -> the game viewport is computed from the docked-bar rect
///        -> the render device's viewport is set (x, y, w, h, not raw)
/// -> the update-game-view notice is sent
/// -> the game view's update-game-view handler
/// ```
///
/// and that receiver is the one that decides the final rectangle. **It ignores all four notice
/// parameters** and re-reads its own region instead: it sets the viewport, not raw, from its own
/// screen x0, screen y0, width and height.
///
/// So the notice is a *"the layout changed, re-apply your rect"* ping, and because it runs after
/// the clamped-position recalculation's own viewport set, **the smart box element's box wins** -- which
/// is why this function, and not [`crate::hud::floaty::SMART_BOX_CHROME`] or the docked-bar
/// computation, is what the renderer is given.
///
/// The five triggers of the clamped-position recalculation are the complete list of resize
/// events: a device display-mode change, setting the virtual screen position, setting the clamped
/// game-view edge, a visibility change and setting the object, plus the smart box's own post-init
/// once at screen build.
#[must_use]
pub fn client_rect(ui: &UiSystem, h: ElemHandle) -> Box2D {
    ui.screen_box(h)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The world view binds the documented children and borders.
    #[test]
    fn the_world_view_binds_the_documented_children_and_borders() {
        assert_eq!(FPS_DISPLAY, ElementId(0x1000_0047));
        assert_eq!(
            SMART_BOX,
            crate::screens::gameplay::window::SMART_BOX,
            "the re-export must not drift from the screen's own id"
        );
        assert_eq!(PORTAL_SPACE, ElementId(0x1000_0436));
        let spec = crate::panels::catalogue::spec("WorldView").unwrap();
        assert!(spec.children.iter().any(|c| c.id == FPS_DISPLAY));
        assert!(spec.children.iter().any(|c| c.id == PORTAL_SPACE));
        assert_eq!(crate::hud::floaty::SMART_BOX_CHROME.len(), 8);
    }

    /// The fps read out renders two decimals and picks the documented degrade value.
    #[test]
    fn the_fps_read_out_renders_two_decimals_and_picks_the_documented_degrade_value() {
        assert_eq!(fps_text(59.994, 1.0), "59.99 1.00");
        assert_eq!(fps_text(30.0, -0.5), "30.00 -0.50");
        assert_eq!(deg_var(true, 1.0, 9.0), 1.0);
        assert_eq!(deg_var(false, 1.0, 9.0), 9.0);
        assert_eq!(FPS_STRING, "ID_SmartBox_FPS");
        assert_eq!(FPS_VARIABLES, ["fps_var", "deg_var"]);
        assert_eq!(FPS_STRING_TABLE_GROUP, 4);
        assert_eq!(FPS_STRING_TABLE_ENUM, 0x1000_0001);
        assert_eq!(FPS_STRING_TABLE, DataId(0x2300_0001));
    }

    /// the enum, the light and the camera.
    #[test]
    fn the_teleport_object_uses_the_documented_light_and_camera() {
        assert_eq!(teleport::OBJECT_ENUM, 0x1000_0001);
        assert_eq!(teleport::LIGHT_INTENSITY, 2.0);
        assert_eq!(teleport::LIGHT_DIRECTION, (0.3, -1.9, 0.65));
        assert_eq!(teleport::CAMERA_POSITION, (0.24, -2.7, 0.88));
    }

    /// Oracle: the smart box's resize and move — the rectangle the world camera is
    /// given is the element's absolute screen box, and boxes are **inclusive**.
    #[test]
    fn the_client_rect_is_the_elements_absolute_inclusive_screen_box() {
        let mut ui = UiSystem::new((800, 600));
        let h = ui.create_hollow(Some(ui.root()));
        ui.resize_to(h, 640, 480);
        ui.move_to(h, 80, 60);
        let r = client_rect(&ui, h);
        assert_eq!((r.x0, r.y0), (80, 60));
        assert_eq!((r.width(), r.height()), (640, 480));
        // Inclusive: x1 is the last pixel, not one past it.
        assert_eq!(r.x1, 80 + 640 - 1);
        assert_eq!(r.y1, 60 + 480 - 1);
    }
}
