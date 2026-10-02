//! The desktop's platform answers for the HUD model.

/// Hand the model crate the desktop's platform answers: the local-time shift the C runtime's
/// `localtime` would apply, the desktop URL launch and the caret blink interval. Idempotent.
pub fn install_platform<P: crate::Product>() {
    dereth_client_shell::hud::install_platform::<crate::Desktop<P>>();
}
