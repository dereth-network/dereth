//! Install the desktop's platform answers for the client shell's HUD.

/// Hand the model crate the desktop's platform answers: the local-time shift the C runtime's
/// `localtime` would apply, the desktop URL launch and the caret blink interval. Idempotent.
pub fn install_platform() {
    dereth_desktop::hud::install_platform::<crate::Dereth>();
}
