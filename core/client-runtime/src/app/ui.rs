//! UI frames, requests, and selection delivery.

use super::*;

impl<S: Shell> App<S> {
    /// Frame step 7: the UI update, the mode switch, and everything the front end does inside it.
    ///
    /// What is here runs whether or not there is a UI: the notices the UI would have taken are
    /// drained all the same, because a notice has no replay history and a UI that comes up later
    /// must not see an old one. The front end's own step is [`Shell::ui_frame`]; with no UI the
    /// input manager still gets its per-frame tick.
    pub(super) fn ui_use_time(&mut self, shell: &mut S, now: dereth_primitives::LocalTime) {
        // The previous frame's unclaimed actions expire here, before this frame's are produced.
        self.actions.begin_frame();
        // The world's systems, as the requests the interaction layer runs this frame see them.
        self.interaction.era_features = self.hud.era.features();
        self.duties.teleport_ticked = false;
        self.duties.hud_synced = false;
        self.duties.portal_driven = false;
        if std::mem::take(&mut self.duties.quit_owed) {
            self.pump.done();
        }
        self.tick_resolution(shell, now.0);
        self.objects.world.refresh_stack_split();
        shell.service_dialogs(&mut UiContext::new(self), now);
        for request in self.objects.world.take_book_requests() {
            let _ = self.run_request(request, now, &mut |_| {});
        }
        for request in self.hud.unknown_spell_favorites(&self.objects.world) {
            let _ = self.run_request(request, now, &mut |_| {});
        }
        // Communication notices affect command routing, so the existing subscriber receives
        // them before shell input, not in the late display-only power-bar batch below. If a mode
        // is queued, this is still the outgoing live subscriber; the new `post_init` reads globals.
        let chat_target = self.hud.auto_target_world(&self.objects.world);
        self.interaction
            .update_chat_target(&mut self.objects.world, now.0, &chat_target);
        let chat_focus_notices = self.objects.world.chat.take_talk_focus_notices();
        // The player's `transient_state & 1`, the one thing
        // the log-off confirmation checks before refusing an airborne log-off. Read here, before
        // `UiShell::frame_with_dispatch` runs the screen's per-frame step, so the frame that
        // answers *Yes* is judged on **this** frame's contact state. A frame with no body yet
        // reports "on the ground": that is the client's null-player arm reduced to the one answer a
        // player who is standing on a screen can actually be in.
        let player_airborne = self
            .world
            .as_ref()
            .and_then(|w| w.character.as_ref())
            .is_some_and(|c| !c.in_contact());
        shell.before_ui_input(player_airborne);
        crate::ui_context::offer_talk_focus_notices(
            &mut self.objects.world.chat,
            chat_focus_notices,
            &mut |focus, notice| shell.talk_focus_notice(focus, notice),
        );
        // Drain even with no UI/gameplay subscriber. Retail notices have no replay history;
        // keeping them while headless or on another screen would grow without bound and deliver
        // old combat into a later UI. The source ordering survives multiple edges in one frame.
        let power_bar_notices = self.objects.world.combat.take_power_bar_notices();
        // Notice subscribers have no replay history. Drain before every no-UI/other-screen
        // return, and do not deliver an old field's notices into a replacement generation.
        let external_container_notices = self.interaction.take_external_container_notices();
        self.hud.pending_external_container.clear();
        // Slumlord range exit is a UI notice, not a second proximity
        // poll. Drain it with the other one-generation panel notices below.
        let slumlord_range_exits = self.interaction.take_slumlord_range_exits();
        // Book range exit has the same one-generation UI-notice shape,
        // using the current book id as its stale-registration guard.
        let book_range_exits = self.interaction.take_book_range_exits();
        // The salvage window's notices, drained and delivered on exactly the same
        // terms for exactly the same reason: a panel generation that did not exist when the notice
        // was emitted keeps its `post_init` state, and an undelivered batch must not survive a
        // screen change and open a window for a tool used two screens ago.
        let salvage_notices = self.interaction.take_salvage_notices();
        self.hud.clear_salvage_notices();
        // Accepted secure-trade item ids, drained on the same terms and for the
        // same reason as the salvage batch
        // above: an undelivered batch must not survive a screen change and put an item on a trade
        // table that a different generation of the window owns.
        let trade_for_dummies = self.interaction.take_trade_for_dummies();
        self.build_host_state();
        shell.drive_pregame_screens(&mut UiContext::new(self));
        shell.drive_world_script(&mut UiContext::new(self), now);
        // Before the pointer events are dispatched: a click reaches smart-box object
        // search from inside `dispatch` below, and it must be measured against
        // the rectangle `<SBOX>` occupies *this* frame.
        self.note_game_viewport(shell);
        if !shell.has_ui() {
            // An absent local UI/module consumer has no replay history for placement calls, and
            // with no UI there is no request queue for any to be waiting in.
            // Destroying the external-container panel unregisters its range watches.
            // on_view_contents still registers on the model seam for non-App consumers; no
            // absent UI subscriber may retain those watches in a running application.
            self.objects
                .world
                .object_range_checks
                .unregister_all(dereth_client_model::range::RangeHandler::ExternalContainer);
            // No UI, but the device input still gets its per-frame tick: the input manager's
            // per-frame step is step 6 of the frame, and the repeat sweep is what makes a held key
            // repeat at all.
            shell.input_use_time(&mut UiContext::new(self), now);
            shell.hand_on_actions(&mut self.actions);
            self.run_owed_frame_duties(shell);
            return;
        }
        // The motion facts the UI's input callbacks read, and the selection notices raised since
        // the last delivery, before the UI takes its input: a click on a panel this frame is judged
        // against this frame's selection.
        {
            let viewport = self.present.size();
            let body = self.world.as_ref().and_then(|w| w.character.as_ref());
            self.interaction
                .prepare_ui_dispatch(body, &mut self.objects.world, viewport);
            let body = self.world.as_ref().and_then(|w| w.character.as_ref());
            self.interaction
                .dispatch_ui_selection_notices(body, &mut self.objects, now);
        }
        shell.ui_frame(
            &mut UiContext::new(self),
            now,
            UiNotices {
                power_bar: power_bar_notices,
                external_container: external_container_notices,
                slumlord_range_exits,
                book_range_exits,
                salvage: salvage_notices,
                trade_for_dummies,
            },
        );
        shell.hand_on_actions(&mut self.actions);
        self.run_owed_frame_duties(shell);
    }

    /// Queue an action for the next frame, as a device would produce it. A script, a bot or a
    /// test acts through this; it reaches the same handlers a key does.
    pub fn inject_action(&mut self, action: crate::actions::Action) {
        self.actions.inject(action);
    }

    /// What the runtime owes one request a UI raised, done at once, in the client's order: the
    /// two requests that change the local player's look in place (the barber's particle script and
    /// motion table), the HUD's own placement owners, the game's handlers (chat-focus notices
    /// raised on the way are handed to `chat_focus` there and then, as the client delivers them to
    /// the chat entry synchronously), and the audio and device preferences. Returns what no runtime
    /// owner took.
    ///
    /// A UI calls it from inside its own input dispatch, at the point its listener raised the
    /// request, so the game sees the request before the UI's next listener runs. Follow it with
    /// [`Self::deliver_selection_notices`] before the next listener runs.
    pub fn run_request(
        &mut self,
        request: dereth_client_contract::UiRequest,
        now: dereth_primitives::LocalTime,
        chat_focus: &mut dyn FnMut(&mut dereth_client_model::chat::ChatState),
    ) -> Vec<dereth_client_contract::UiRequest> {
        use dereth_client_contract::UiRequest;
        match request {
            UiRequest::BarberLocalEffect(script) => {
                if let Some(mut scene) = self.present.scene_mut(self.world.as_mut()) {
                    scene.replace_player_particle_script(script);
                }
                Vec::new()
            }
            UiRequest::BarberLocalMotionTable(table) => {
                if let Some(mut scene) = self.present.scene_mut(self.world.as_mut()) {
                    scene.world_mut().replace_player_motion_table(table);
                }
                Vec::new()
            }
            request => {
                let mut request = vec![request];
                self.hud.consume_placement_requests(
                    &mut self.objects.world,
                    &mut request,
                    dereth_primitives::ServerTime(now.0),
                );
                if request.is_empty() {
                    return Vec::new();
                }
                // The player's position, for `@loc`: the one chat command that reads the body.
                // Stamped per request, the way the selection notices read the same origin, because
                // the handler runs against the model and the body lives on the scene.
                self.interaction.player_position = self
                    .world
                    .as_ref()
                    .and_then(|w| w.character.as_ref())
                    .map(crate::character::Character::position);
                if self.interaction.trade_note_values != self.hud.trade_note_values {
                    self.interaction
                        .trade_note_values
                        .clone_from(&self.hud.trade_note_values);
                }
                self.interaction.journal_coords = self.hud.coords;
                self.interaction.queue(Vec::new(), request);
                let remaining = self.interaction.run_ui_requests_with_chat_focus(
                    &mut self.objects.world,
                    self.hud.player_desc_received,
                    dereth_primitives::ServerTime(now.0),
                    chat_focus,
                );
                let remaining =
                    crate::audio::apply_preference_requests(self.audio.as_mut(), remaining);
                self.present.apply_device_preference_requests(remaining)
            }
        }
    }

    /// The selection notices a request raised (the defender and selection-change receivers),
    /// delivered at once, after the request and before the UI's next listener. A request the UI
    /// answered itself raises them too.
    pub fn deliver_selection_notices(&mut self, now: dereth_primitives::LocalTime) {
        let body = self.world.as_ref().and_then(|w| w.character.as_ref());
        self.interaction
            .dispatch_ui_selection_notices(body, &mut self.objects, now);
    }

    /// One pass of the dialog service ([`crate::dialogs::DialogService::service`]), with
    /// `presenter` showing the questions, or none to drop them.
    pub fn service_dialogs_with(
        &mut self,
        presenter: Option<&mut dyn crate::dialogs::DialogPresenter>,
        now: dereth_primitives::LocalTime,
    ) {
        self.dialogs.service(
            presenter,
            &mut self.interaction,
            &mut self.objects.world,
            now,
        );
    }

    /// Queue requests for the interaction step of the next frame: a chat line, a use of an
    /// object, anything the UI would have asked for.
    pub fn submit_requests(&mut self, requests: Vec<dereth_client_contract::UiRequest>) {
        self.interaction.queue(Vec::new(), requests);
    }

    /// Push the render device's viewport rectangle into [`crate::interaction`].
    ///
    /// The smart-box object-found receiver reads the render viewport and performs unsigned bounds
    /// comparisons against its width and height. `Interaction` has no renderer or UI tree, so the
    /// value crosses the same way [`Shell::examine_panel_open`] does: read here once a
    /// frame, immediately before the step that consumes it.
    ///
    /// It is deliberately [`Shell::game_viewport`] and not the renderer's stored viewport: the two
    /// are the same rectangle by construction — `App::frame` installs the first as the second —
    /// and asking the renderer instead would make the pick agree with a stored field rather than
    /// with the box the player is looking at.
    pub(super) fn note_game_viewport(&mut self, shell: &S) {
        let v = shell.game_viewport();
        self.interaction.note_game_viewport(v);
        let over = shell.pointer_over_game_view(self.interaction.cursor());
        self.interaction.note_pointer_over_game_view(over);
    }

    pub(super) fn current_selection_geometry(
        &self,
    ) -> (crate::selection_geometry::SceneSelectionPhysics, f32) {
        let origin = self
            .world
            .as_ref()
            .and_then(|w| w.character.as_ref())
            .map(crate::character::Character::position);
        let geometry =
            crate::selection_geometry::SceneSelectionPhysics::new(origin.as_ref(), &self.objects);
        let radius = dereth_client_contract::radar::radar_range(
            origin
                .as_ref()
                .is_some_and(|p| dereth_physics::landdefs::is_outdoors(p.cell)),
        );
        (geometry, radius)
    }

    pub(super) fn ui_queue_use_time(&mut self, shell: &mut S) {
        let now = dereth_primitives::LocalTime(self.timer.cur_time);
        self.deliver_session_events(shell, now);
        while self
            .link
            .as_mut()
            .is_some_and(|link| link.net.session.process_next_ui(now))
        {
            self.deliver_session_events(shell, now);
        }
        crate::interaction::registered_systems_use_time(
            &mut self.interaction,
            self.present.scene(self.world.as_ref()).as_deref(),
            &mut self.objects,
            self.link.as_mut().map(|link| &mut link.net),
            now,
        );
        shell.control_notice(crate::shell::ControlNotice::TargetMode(
            self.interaction.target_mode() != crate::interaction::TargetMode::None,
        ));
    }
}
