//! The Horizon interface in the modern interface's place, when the player chooses it: brought up on
//! its own art, which the host hands over, and driven at each step of the frame as the classic
//! interface is.

use dereth_horizon::runtime::HorizonFrontEnd;

use super::{Cx, FrontEnd, FrontEndServices};
use crate::classic_face::Refusal;
use crate::platform::host::{HorizonArt, Host};
use crate::present::ClientPresentation;

/// The Horizon interface, when it has been brought up, and whether it is the one shown.
#[derive(Debug, Default)]
pub(crate) struct HorizonFace {
    pub ui: Option<HorizonFrontEnd>,
    pub active: bool,
    /// Its key bindings page, carried out on the input manager.
    pub keys: super::horizon_keys::HorizonKeys,
    /// The player has been told the choice of it waits for its art, which is still loading.
    pub told_loading: bool,
}

impl HorizonFace {
    /// The Horizon interface, while it is the one shown.
    pub fn active_mut(&mut self) -> Option<&mut HorizonFrontEnd> {
        if self.active {
            self.ui.as_mut()
        } else {
            None
        }
    }

    /// The Horizon interface, while it is the one shown.
    pub fn active(&self) -> Option<&HorizonFrontEnd> {
        if self.active {
            self.ui.as_ref()
        } else {
            None
        }
    }
}

/// Bring the Horizon interface up: its own art, as the host hands it over, and its settings from
/// its settings file. Not yet while the art is loading, and refused, with the host's reason, when
/// the host cannot get it.
pub(super) fn build_horizon<H: Host>(cx: &mut Cx<'_, H>) -> Result<HorizonFrontEnd, Refusal> {
    let pieces = match H::horizon_art() {
        HorizonArt::Ready(pieces) => pieces,
        HorizonArt::Loading => return Err(Refusal::HorizonArtLoading),
        HorizonArt::Unavailable(why) => return Err(Refusal::HorizonArt(why)),
    };
    let preferences = cx.config().preferences_file.clone();
    let settings = preferences
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .map(|d| d.join(dereth_horizon::options::FILE_NAME));
    let options = settings
        .as_deref()
        .map(dereth_horizon::options::HorizonOptions::load)
        .unwrap_or_default();
    let art = std::sync::Arc::new(dereth_horizon::art::Art::new(pieces));
    let mut ui = HorizonFrontEnd::new(art, options, settings);
    ui.start(cx);
    Ok(ui)
}

impl<H: Host> FrontEnd<H> for HorizonFrontEnd {
    fn reread_files(&mut self, cx: &mut Cx<'_, H>, _services: &mut FrontEndServices<H>) {
        HorizonFrontEnd::reread_files(self, cx);
    }
    fn game_viewport(&self) -> Option<dereth_primitives::Viewport> {
        None
    }
    fn pointer_over_game_view(&self, _cursor: (i32, i32)) -> bool {
        !self.ui.pointer_over_ui
    }
    fn examine_panel_open(&mut self) -> bool {
        HorizonFrontEnd::examine_panel_open(self)
    }
    fn close_examine_panel(&mut self) {
        HorizonFrontEnd::close_examine_panel(self);
    }
    fn service_dialogs(&mut self, cx: &mut Cx<'_, H>, now: dereth_primitives::LocalTime) {
        HorizonFrontEnd::service_dialogs(self, cx, now);
    }
    fn before_ui_input(&mut self, player_airborne: bool) {
        HorizonFrontEnd::before_ui_input(self, player_airborne);
    }
    fn talk_focus_notice(
        &mut self,
        talk_focus: dereth_client_model::chat::TalkFocus,
        notice: dereth_client_model::chat::TalkFocusNotice,
    ) -> bool {
        HorizonFrontEnd::talk_focus_notice(self, talk_focus, notice)
    }
    fn deliver_power_bar_notices(
        &mut self,
        _hud: &mut crate::hud::Hud,
        notices: Vec<dereth_client_model::combat::PowerBarNotice>,
    ) {
        HorizonFrontEnd::deliver_power_bar_notices(self, notices);
    }
    fn object_panel_notice(
        &mut self,
        _hud: &mut crate::hud::Hud,
        _world: &dereth_client_model::World,
        _notice: &dereth_client_model::Notice,
    ) -> Vec<dereth_client_contract::UiRequest> {
        Vec::new()
    }
    fn emit_magic_notices(&mut self, notices: Vec<dereth_client_contract::view::MagicNotice>) {
        HorizonFrontEnd::emit_magic_notices(self, notices);
    }
    fn open_vendor_buying(&mut self, _hud: &mut crate::hud::Hud) {
        HorizonFrontEnd::open_vendor_buying(self);
    }
    fn run_ui_layout_commands(
        &mut self,
        _prefs: &std::path::Path,
        _character: &str,
        _world: &str,
        _layout_commands: Vec<dereth_client_runtime::interaction::UiLayoutCommand>,
    ) {
    }
    fn split_stack(
        &mut self,
        _view: &dereth_client_runtime::hud::HudView<'_>,
        _selected: dereth_primitives::ObjectId,
    ) {
    }
    fn dispatch_input_action(&mut self, _action: u32) -> Option<bool> {
        None
    }
    fn world_tooltip(&mut self, _tooltip: dereth_client_runtime::interaction::WorldTooltip) {}
    fn chat_generation(&self) -> Option<u64> {
        self.in_gameplay().then_some(1)
    }
    fn in_gameplay(&self) -> bool {
        HorizonFrontEnd::in_gameplay(self)
    }
    fn hides_world(&self) -> bool {
        false
    }
    fn requests(&mut self) -> Option<&mut dereth_client_contract::requests::Outbox> {
        Some(&mut self.outbox)
    }
    fn frame(
        &mut self,
        cx: &mut Cx<'_, H>,
        _services: &mut FrontEndServices<H>,
        now: dereth_primitives::LocalTime,
        notices: dereth_client_runtime::shell::UiNotices,
    ) -> bool {
        HorizonFrontEnd::frame(self, cx, now, notices)
    }
    fn before_portal(&mut self, _cx: &mut Cx<'_, H>, _services: &mut FrontEndServices<H>) {}
    fn after_portal(&mut self, _cx: &mut Cx<'_, H>, _services: &mut FrontEndServices<H>) {}
    fn compose(&mut self, cx: &mut Cx<'_, H>, _services: &mut FrontEndServices<H>) {
        HorizonFrontEnd::compose(self, cx);
        for error in self.errors.drain(..) {
            tracing::warn!("Horizon interface: {error}");
        }
    }
    fn draw(
        &mut self,
        present: &mut dyn ClientPresentation,
        _services: &FrontEndServices<H>,
    ) -> Result<(), dereth_client_runtime::present::PresentError> {
        HorizonFrontEnd::draw(self, present)
    }
    fn resize(&mut self, _display: (i32, i32)) {}
    fn suspend(&mut self, cx: &mut Cx<'_, H>) {
        // The other interfaces draw their own screens before the world, and lay nothing on the
        // ground under the selection.
        cx.hide_backdrop();
        cx.present_mut().set_ground_markers(&[]);
        HorizonFrontEnd::suspend(self);
    }
}
