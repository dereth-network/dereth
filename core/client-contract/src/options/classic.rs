//! The classic interface's own settings: the thin block of options only that interface has.
//!
//! Every other setting either interface shows is one shared preference, edited by both
//! interfaces' pages, or a bit of the character's option words on the server. These three are
//! the classic interface's alone. The early clients kept them in the character's option word
//! (bits 4, 14 and 21); the later client gave those bits other meanings, so the word keeps the
//! later meanings and these live in the profile, under `[UI.Classic]`, beside the shared options.

use crate::view::PrefValue;

/// `[UI.Classic] InvertMouseLook`: mouse look turns the camera up when the mouse moves down.
/// Only the vertical motion is inverted, unlike `Input.InvertMouseLookYAxis`, which inverts both.
pub const INVERT_MOUSE_LOOK: &str = "UI.Classic.InvertMouseLook";

/// `[UI.Classic] RightClickMouseLook`: holding the right mouse button looks around.
pub const RIGHT_CLICK_MOUSE_LOOK: &str = "UI.Classic.RightClickMouseLook";

/// `[UI.Classic] StretchUI`: the interface is stretched to the window's height.
pub const STRETCH_UI: &str = "UI.Classic.StretchUI";

/// The block's names.
pub const NAMES: [&str; 3] = [INVERT_MOUSE_LOOK, RIGHT_CLICK_MOUSE_LOOK, STRETCH_UI];

/// Register the block in the option value store, every option off.
pub fn register() -> usize {
    NAMES
        .iter()
        .map(|n| {
            usize::from(super::store::register_preference(
                n,
                PrefValue::Bool(false),
                super::store::DataType::Bool,
            ))
        })
        .sum()
}

/// Whether the store holds `name` on.
#[must_use]
pub fn on(name: &str) -> bool {
    matches!(super::store::inq_value(name), Some(PrefValue::Bool(true)))
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (an option block's own registration; the interface that reads it is tested
    //! where it does).
    use super::*;

    #[test]
    fn the_block_registers_off_and_holds_what_is_set() {
        super::super::store::init();
        assert!(NAMES.iter().all(|n| !on(n)));
        assert!(super::super::store::set_value(
            STRETCH_UI,
            PrefValue::Bool(true)
        ));
        assert!(on(STRETCH_UI));
        assert!(!on(INVERT_MOUSE_LOOK));
    }
}
