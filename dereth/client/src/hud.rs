//! The HUD: the client shell's ([`dereth_client_shell::hud`], re-exported here), with the
//! desktop's platform answers installed by [`install_platform`].

pub use dereth_client_shell::hud::*;

/// Hand the model crate the desktop's platform answers: the local-time shift the C runtime's
/// `localtime` would apply, the desktop URL launch and the caret blink interval. Idempotent.
pub fn install_platform() {
    dereth_desktop::hud::install_platform::<crate::Dereth>();
}
