//! The HUD.
//!
//! The model — the object rows, the chat composition, the property caches and the read-only
//! [`HudView`] projection that is the game's `GameView` — is [`dereth_client_runtime::hud`], and every
//! item of it is re-exported here at its historical path. What this module adds is the retail UI's
//! panel set, kept beside the model in the slot every front end has (`HudSlot`): the gameplay
//! screen's [`RemainingPanels`], which outlive screen rebuilds, and which the model lends the two
//! receivers it feeds as events land (the speech-bubble strip and the abuse panel). The per-frame
//! drive that hands the model and the panels to the live screen is [`crate::hud_drive`].

use dereth_ui_screens::panels::remaining::RemainingPanels;

pub use dereth_client_runtime::hud::*;

pub use crate::hud_drive::{deliver_power_bar_notices, talk_focus_notice};

/// The HUD model and the retail UI's panel set beside it, plus the per-frame drive that hands both
/// to the live gameplay screen ([`crate::hud_drive`]). Everything else is the model's, through
/// `Deref`.
#[derive(Debug)]
pub struct Hud {
    model: dereth_client_runtime::hud::Hud,
    /// The gameplay screen's panels, kept across screen rebuilds.
    pub panels: RemainingPanels,
    /// The classic interface's receivers, which take the model's offers while it is the
    /// interface shown.
    pub classic: dereth_classic_ui::runtime::ClassicHudPanels,
    /// Whether the classic interface is the one shown.
    pub classic_active: bool,
}

impl Default for Hud {
    /// A fresh HUD. Platform readers are installed by the application that knows its host
    /// ([`install_platform`]); constructing a receiver does not choose a host.
    fn default() -> Self {
        Self {
            model: dereth_client_runtime::hud::Hud::default(),
            panels: RemainingPanels::default(),
            classic: Default::default(),
            classic_active: false,
        }
    }
}

impl Hud {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply a batch of session events to the model, with this panel set as its receivers. See
    /// [`dereth_client_runtime::hud::Hud::apply_events`].
    pub fn apply_events(
        &mut self,
        events: &[dereth_client_net::client_session::SessionEvent],
        world: &mut dereth_client_model::World,
    ) -> Vec<dereth_client_contract::chat::interface::ChatMessage> {
        self.model.apply_events_with_combat_mode_handler(
            events,
            world,
            &mut self.panels,
            None,
            &mut |_, _| {},
        )
    }
}

impl Hud {
    /// [`Self::apply_events`] with the combat system's callback and the UI's request queue. See
    /// [`dereth_client_runtime::hud::Hud::apply_events_with_combat_mode_handler`].
    pub fn apply_events_with_combat_mode_handler(
        &mut self,
        events: &[dereth_client_net::client_session::SessionEvent],
        world: &mut dereth_client_model::World,
        ui_requests: Option<&mut dereth_client_contract::requests::Outbox>,
        on_combat_mode: &mut dyn FnMut(
            &mut dereth_client_model::World,
            dereth_client_model::combat::CombatMode,
        ),
    ) -> Vec<dereth_client_contract::chat::interface::ChatMessage> {
        self.model.apply_events_with_combat_mode_handler(
            events,
            world,
            &mut self.panels,
            ui_requests,
            on_combat_mode,
        )
    }
}

impl HudSlot for Hud {
    fn split(&mut self) -> (&mut dereth_client_runtime::hud::Hud, &mut dyn HudPanels) {
        if self.classic_active {
            (&mut self.model, &mut self.classic)
        } else {
            (&mut self.model, &mut self.panels)
        }
    }
}

impl dereth_classic_ui::runtime::ClassicHudSlot for Hud {
    fn classic_panels(&mut self) -> &mut dereth_classic_ui::runtime::ClassicHudPanels {
        &mut self.classic
    }
}

impl std::ops::Deref for Hud {
    type Target = dereth_client_runtime::hud::Hud;
    fn deref(&self) -> &Self::Target {
        &self.model
    }
}

impl std::ops::DerefMut for Hud {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.model
    }
}

/// Hand the model crate the platform answers it cannot compute itself: the local-time shift the
/// C runtime's `localtime` would apply and the URL launch, which are host `H`'s, and the caret
/// blink interval. Idempotent: the first host installed stands.
pub fn install_platform<H: crate::platform::host::Host>() {
    dereth_client_runtime::platform::clock::install_local_utc_offset(H::local_utc_offset_secs);
    dereth_client_runtime::platform::shell::install_uri_launcher(H::launch_uri);
    dereth_client_runtime::platform::caret::install_caret_blink(H::caret_blink_secs);
}
