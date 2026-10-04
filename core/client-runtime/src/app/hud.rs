//! HUD event application and deferred frame duties.

use super::*;

impl<S: Shell> App<S> {
    /// Rebuild the HUD model's per-frame values -- the radar list, the coordinates and the
    /// heading -- from where the player's body stands this frame.
    ///
    /// A front end whose panels read the model during its own UI step runs this at the point they
    /// need it; otherwise the runtime runs it at the foot of the UI step. See [`FrameDuties`].
    pub fn sync_hud(&mut self) {
        self.duties.hud_synced = true;
        let viewer = self
            .world
            .as_ref()
            .and_then(|w| w.character.as_ref())
            .map(|c| {
                let p = c.position();
                crate::hud::ViewerFrame {
                    position: p,
                    heading_degrees: dereth_animation::frame::get_heading(&p.frame),
                }
            });
        self.hud.sync(&self.objects, viewer);
    }

    /// The game duties the front end did not run during its UI step, run at its foot so that
    /// every front end has them. See [`FrameDuties`].
    pub(super) fn run_owed_frame_duties(&mut self, shell: &mut S) {
        if !self.duties.hud_synced {
            self.sync_hud();
        }
        // The game screen's size follows its construction and destruction. With no UI there is
        // no screen to follow.
        if shell.has_ui() && self.duties.gameplay_followed != Some(shell.in_gameplay()) {
            self.follow_screen_forced_resolution(shell);
            self.follow_gameplay_full_screen(shell);
        }
        if !self.duties.teleport_ticked {
            self.teleport_use_time(shell);
        }
        // Straight after the tick, whose tunnel state it reads.
        if !self.duties.portal_driven {
            self.portal_space_use_time(shell);
        }
    }

    /// [`crate::hud::Hud::apply_events`] against this application's own object tables.
    ///
    /// `apply_events` takes the tables because `0x0013`'s two inventory lists
    /// go into them (`dereth_client_model`'s `update_object_inventory`), and the two
    /// fields cannot be borrowed through `App` from outside. This is the same call the frame makes.
    pub fn apply_hud_events(
        &mut self,
        shell: &mut S,
        events: &[dereth_client_net::client_session::SessionEvent],
    ) -> Vec<dereth_client_contract::chat::interface::ChatMessage> {
        // Text-scroll composition reads this filter bit for every line.
        // Keep the decoded table on the world-side scroll so both the actual Scroll producer and
        // HUD's still-direct incoming handlers use one matcher. This runs before Hud drains or
        // composes anything, and after any WorldObjects reset has installed a fresh world.
        self.objects.world.scroll.filter_language = self
            .objects
            .world
            .player_system
            .options
            .get(dereth_client_model::player::options::option::FILTER_LANGUAGE);
        self.objects
            .world
            .scroll
            .taboo_table
            .clone_from(&self.taboo_table);
        if events.iter().any(|event| {
            use dereth_client_net::client_session::{SessionEvent, SessionState};
            matches!(
                event,
                SessionEvent::LoggedOff
                    | SessionEvent::StateChanged(
                        SessionState::CharacterSelect | SessionState::Disconnected(_)
                    )
            )
        }) {
            shell.clear_chat_history();
        }
        // A Turbine callback is a synchronous notice to the CURRENT chat subscribers.
        // Preserve that generation through the deferred Hud delivery, not across a rebuild.
        let chat_generation = shell.chat_generation();
        self.hud.set_turbine_chat_generation(chat_generation);
        // Most frames have no combat-quality update. Avoid copying every object's selection
        // geometry unless an integer update can reach the callback below.
        if !events.iter().any(|e| {
            matches!(e, dereth_client_net::client_session::SessionEvent::UiEvent { opcode, .. }
                if *opcode == dereth_protocol::Opcode::QUALITIES_PRIVATE_UPDATE_INT
                    || *opcode == dereth_protocol::Opcode::QUALITIES_UPDATE_INT)
        }) {
            let (hud, panels) = crate::hud::HudSlot::split(&mut self.hud);
            return hud.apply_events_with_combat_mode_handler(
                events,
                &mut self.objects.world,
                panels,
                shell.ui_requests(),
                &mut |_, _| {},
            );
        }
        let origin = self
            .world
            .as_ref()
            .and_then(|w| w.character.as_ref())
            .map(crate::character::Character::position);
        let phys =
            crate::selection_geometry::SceneSelectionPhysics::new(origin.as_ref(), &self.objects);
        let radius = dereth_client_contract::radar::radar_range(
            origin
                .as_ref()
                .is_some_and(|p| dereth_physics::landdefs::is_outdoors(p.cell)),
        );
        let now = dereth_primitives::LocalTime(self.timer.cur_time);
        let interaction = &mut self.interaction;
        let mut combat_modes = Vec::new();
        let (hud, panels) = crate::hud::HudSlot::split(&mut self.hud);
        let applied = hud.apply_events_with_combat_mode_handler(
            events,
            &mut self.objects.world,
            panels,
            shell.ui_requests(),
            &mut |world, mode| {
                interaction.on_combat_mode_quality_changed(world, mode, &phys, radius, now);
                combat_modes.push(world.combat.combat_mode.raw());
            },
        );
        // The device input follows each combat-mode change, in the order they landed. Nothing
        // reads a control between the change and this line, so telling it after the batch is
        // telling it at the change.
        for mode in combat_modes {
            shell.control_notice(crate::shell::ControlNotice::CombatMode(mode));
        }
        applied
    }
}
