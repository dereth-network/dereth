//! Viewports: where the game shows a scene inside the interface. A viewport names its scene
//! (`uiscene`) and the camera to look through (`camera`).

use crate::system::{Handle, Ui, Value};
use crate::token::{attr, Token};

/// The camera a viewport looks through, by name.
pub const CAMERA: Token = Token::of("camera");
/// Marks an element as showing a scene.
pub const VIEWPORT: Token = Token::of("viewport");

impl Ui {
    /// A new viewport is marked as one.
    // D2-11687, D2-11693
    pub(crate) fn viewport_created(&mut self, h: Handle) {
        self.set_attr(h, VIEWPORT, Value::Bool(true));
    }

    /// The scene a viewport shows (0 for none).
    // D2-11686, D2-11690, D2-11694
    #[must_use]
    pub fn viewport_scene(&self, h: Handle) -> u32 {
        self.node(h)
            .and_then(|n| n.id_attr(attr::UISCENE))
            .unwrap_or(0)
    }

    /// The name of the camera a viewport looks through.
    // D2-11689, D2-11695, D2-11688
    #[must_use]
    pub fn viewport_camera(&self, h: Handle) -> Option<String> {
        match self.node(h)?.attr(CAMERA)? {
            Value::Text(s) => Some(s.clone()),
            _ => None,
        }
    }
}
