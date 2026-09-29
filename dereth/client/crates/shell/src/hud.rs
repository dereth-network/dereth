//! The HUD.
//!
//! The model — the object rows, the chat composition, the property caches and the read-only
//! [`HudView`] projection that is the game's `GameView` — is [`dereth_client_runtime::hud`], and every
//! item of it is re-exported here at its historical path. What this module adds is the panel set
//! the model sits beside: the gameplay screen's [`RemainingPanels`], which the application owns
//! across screen rebuilds, wrapped in [`RemainingPanels`] so the model can feed the two receivers it
//! feeds as events land (the speech-bubble strip and the abuse panel). [`Hud`] and [`HudView`]
//! here are the model over that panel set; the per-frame drive that hands the model to the live
//! screen is [`crate::hud_drive`].

use dereth_ui_screens::panels::remaining::RemainingPanels;

pub use dereth_client_runtime::hud::*;

pub use crate::hud_drive::{deliver_chat_focus_notices, deliver_power_bar_notices};

/// The HUD model over the gameplay screen's panels, plus the per-frame drive that hands it to the
/// live gameplay screen ([`crate::hud_drive`]). Everything else is the model's, through `Deref`.
#[derive(Debug)]
pub struct Hud(pub dereth_client_runtime::hud::Hud<RemainingPanels>);

impl Default for Hud {
    /// A fresh HUD. Constructing one is also where this build hands the model crate the answers
    /// every host gives alike (see [`install_shared`]); the host's own, the local time among them,
    /// are installed by the application that knows its host ([`install_platform`]).
    fn default() -> Self {
        install_shared();
        Self(dereth_client_runtime::hud::Hud::default())
    }
}

impl Hud {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl std::ops::Deref for Hud {
    type Target = dereth_client_runtime::hud::Hud<RemainingPanels>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for Hud {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// The read-only view over [`Hud`] and the world: the production `GameView`.
pub type HudView<'a> = dereth_client_runtime::hud::HudView<'a, RemainingPanels>;

/// Hand the model crate the platform answers it cannot compute itself: the local-time shift the
/// C runtime's `localtime` would apply and the URL launch, which are host `H`'s, and the caret
/// blink interval. Idempotent: the first host installed stands.
pub fn install_platform<H: crate::platform::host::Host>() {
    dereth_client_runtime::platform::clock::install_local_utc_offset(H::local_utc_offset_secs);
    dereth_client_runtime::platform::shell::install_uri_launcher(H::launch_uri);
    install_shared();
}

/// The answers every host gives alike: the caret blink interval. Idempotent.
pub fn install_shared() {
    dereth_client_runtime::platform::caret::install_caret_blink(
        dereth_render::window_proc::caret_blink_time_seconds,
    );
}
