//! The world-view screen's teleport / portal animation.
//!
//! The model lives in [`dereth_client_contract::teleport`]: it is a pure function of elapsed time
//! and four observable facts, and `dereth_client_runtime::teleport` drives it from a core module, so it
//! sits in the crate both sides may name. Everything it holds is re-exported here, so
//! `dereth_ui_screens::screens::teleport::TeleportAnim` and every sibling path resolve.
//!
//! The model's own tests live with it. The one below is the single assertion that cannot: it
//! reads `crate::hud::world_view::teleport`, this crate's second transcription of the same retail
//! call, and a contract crate may not name a presentation crate.

pub use dereth_client_contract::teleport::*;

#[cfg(test)]
mod tests {
    /// Oracle: the portal camera's Z offset — the float the world-view screen hands the camera
    /// when it is built, bit pattern `0x3F6147AE`, i.e. 0.88 — asserted against the float in
    /// `dereth_client_contract::teleport`'s `the_portal_camera_z_is_the_float_in_the_binary`. What is left
    /// here is the half of that test which is this crate's: its two transcriptions agree.
    #[test]
    fn the_portal_camera_position_agrees_with_the_world_view_transcription() {
        assert_eq!(
            crate::hud::world_view::teleport::CAMERA_POSITION,
            super::portal_space::CAMERA_POSITION
        );
    }
}
