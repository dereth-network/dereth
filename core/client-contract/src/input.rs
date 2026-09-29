//! `InputPump` — the seam between whoever owns the input manager and whoever pumps it.
//!
//! `dereth-ui` *calls* this trait and `InputShell` *implements* it. With the trait in `dereth-ui`
//! and `InputShell` in `dereth-client-runtime`, neither the trait nor the type would be local to
//! the crate holding the impl, and the orphan rule refuses that. Both sides depend on this crate,
//! so the trait is here and `dereth_ui::{InputPump, NullInputPump}` are re-exports.

use dereth_primitives::LocalTime;

/// Where input events come from. The input owner implements it; the UI only pumps it.
pub trait InputPump: std::fmt::Debug {
    /// Advance the input manager to `now` — step 6 of the frame.
    fn use_time(&mut self, now: LocalTime);

    /// Whether shift is held. The client answers it by testing the left-shift bit of the input
    /// manager's meta-key mask — current state, not a record of a keystroke.
    ///
    /// **Two element behaviours read this and nothing else does**:
    /// `dereth_ui::UiSystem::mouse_move_element` and `dereth_ui::UiSystem::mouse_resize_element`,
    /// both to snap a dragged window
    /// on to a ten-pixel grid. It is a *poll*, not an event — pressing shift alone raises no
    /// action, so it cannot arrive through `dereth_ui::focus::InputEvent`, which is why it is a
    /// method here and why `dereth_ui::UiSystem::use_time` latches it once a frame rather than the
    /// geometry reaching for it.
    ///
    /// **The default is `false` and the override is not written yet.** the input manager has the
    /// state — `InputManager::meta_key_mode` and `MasterInputMap::meta_mode_from_key` are both
    /// public, and `dereth-input`'s own `right_shift_sets_the_shift_bit` test pins the bit — but
    /// `dereth-client`'s `InputShell` (the one non-test implementor) does not forward it.
    /// The client input shell supplies the one production override.
    fn shift_key_down(&self) -> bool {
        false
    }
}

/// An input pump that does nothing, for tests and for the pre-input bring-up.
#[derive(Debug, Default)]
pub struct NullInputPump;
impl InputPump for NullInputPump {
    fn use_time(&mut self, _now: LocalTime) {}
}
