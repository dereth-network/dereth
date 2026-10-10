//! The application run loop and ordered frame.

use super::*;

impl<S: Shell> App<S> {
    /// Run the main loop: run the per-frame step until it returns false.
    ///
    /// Returns the number of frames drawn.
    pub fn run(&mut self, shell: &mut S) -> u64 {
        self.state = AppState::Running;
        while self.frame(shell) {}
        if let Some(effect) = self.resolution.cancel() {
            self.apply_resolution_effect(effect);
        }
        self.state = AppState::ShuttingDown;
        self.frames_drawn()
    }

    /// One iteration of the main loop, in the documented order. Returns false when
    /// the loop should end.
    #[allow(clippy::too_many_lines)]
    pub fn frame(&mut self, shell: &mut S) -> bool {
        // The frame's spans: the frame and, as each step starts, that step. See
        // `crate::frame::FrameSpans`.
        let mut spans = crate::frame::FrameSpans::begin(self.frames_drawn() + 1);
        self.perf
            .frame_started(web_time::Instant::now(), self.perf_steps);
        self.events.drain_frame();
        // `--capture-at` and `--set-at`: a picture of the frame just drawn, then the settings for
        // the one about to be.
        self.scripted_use_time();

        // Explicit component adapter: ObjectStream::apply_event may have already accepted
        // local calls before App began. Complete that old journal before simulation or a
        // snapshot sync can consume it. Real network entries remain raw until their owning
        // WorldObjects phase and therefore cannot enter this path.
        if self.objects.has_player_motion_dispatches() {
            self.player_teleport_use_time(shell);
        }

        // Client use time prepends the UI queue-manager drain before chaining the base step.
        spans.step(FrameStep::UiQueueStep);
        self.events.push(FrameEvent::Step(FrameStep::UiQueueStep));
        self.ui_queue_use_time(shell);

        spans.step(FrameStep::ClockSample);
        self.events.push(FrameEvent::Step(FrameStep::ClockSample));
        self.timer.update_time(&*self.clock);

        spans.step(FrameStep::ProcessWindowEvents);
        self.events
            .push(FrameEvent::Step(FrameStep::ProcessWindowEvents));
        if self.do_event_loop(shell) {
            // "If true, the per-frame step returns false immediately -- the network is not pumped
            // and no frame is drawn."
            if let Some(effect) = self.resolution.cancel() {
                self.apply_resolution_effect(effect);
            }
            self.state = AppState::ShuttingDown;
            return false;
        }

        // Receive first, run queue-4 callbacks next, then process outbound packets.
        // Reception retains raw UI/WorldObjects entries; it does not admit them against old objects.
        spans.step(FrameStep::NetworkStep);
        self.events.push(FrameEvent::Step(FrameStep::NetworkStep));
        let net_time = dereth_primitives::LocalTime(self.timer.local_time);
        // Link-status timestamps use server-adjusted `cur_time`, while connection and snapshot
        // cadences use local elapsed time. The network layer therefore publishes edges and this frame loop
        // performs the stamp: first on the login-connected edge, then on each two-second
        // connection heartbeat.
        let heartbeat_time = self.timer.cur_time;
        let time_sync = if let Some(link) = self.link.as_mut() {
            // The time-sync speed-check tail reads the
            // global, not `local_time`, and reads it before receive handling below. Publishing
            // the pre-sync value here matches retail's order: it occurs at the top of the network
            // step, before this frame's datagrams are walked.
            link.set_cur_time(heartbeat_time);
            link.receive_use_time(net_time);
            if link.net.take_connected() {
                crate::net::link_status_holder::on_connected(heartbeat_time);
            }
            // A link heartbeat updates the holder's *second* store,
            // the average packet loss. The same callback also refreshes
            // the last-heard-from-server time, so both stores happen on this one edge.
            if let Some(loss) = link.net.take_link_heartbeat() {
                crate::net::link_status_holder::on_heartbeat(heartbeat_time);
                crate::net::link_status_holder::on_packet_loss(loss);
            }
            // Time synchronization is accepted during packet handling, exactly here: after this
            // frame's datagrams have been parsed and before subsequent frame consumers read the
            // clock. Draining it
            // a step later would put the first `cur_time` reader of the frame on the stale clock.
            link.net.session.transport.take_time_sync()
        } else {
            None
        };
        // **Retail has only one such clock-setting call.** The time-sync handler reads
        // the `double` payload at offset `0x18` in the time-sync header, copies it to a
        // local, and passes that value by const reference
        // to the clock's time setter.
        //
        // This is what makes the world clock the shard's rather than this process's: with no sync
        // the external time offset stays 0 and `cur_time` is just local elapsed seconds, which is a
        // process-local epoch -- the wrong date on the map panel and the wrong day/night cycle.
        if let Some(server_time) = time_sync {
            self.timer.set_time(&*self.clock, server_time);
        }
        // The first connection is made before any screen exists, so a login that ends before the
        // link was ever up is not a screen: it is one modal error box, and when the player closes
        // it the process exits. Nothing else runs this frame and no frame is drawn.
        if self.connect_failure_use_time() {
            self.pump.done();
            if let Some(effect) = self.resolution.cancel() {
                self.apply_resolution_effect(effect);
            }
            self.state = AppState::ShuttingDown;
            return false;
        }
        self.deliver_session_events(shell, dereth_primitives::LocalTime(self.timer.cur_time));
        spans.step(FrameStep::LoginEvents);
        self.events.push(FrameEvent::Step(FrameStep::LoginEvents));
        while self
            .link
            .as_mut()
            .is_some_and(|link| link.net.session.process_next_logon())
        {
            self.deliver_session_events(shell, dereth_primitives::LocalTime(self.timer.cur_time));
        }
        // Per-event delivery carries lines from scroll-text calls to the
        // HUD's spew box and chat interface. Every frame also refreshes the three once-a-frame
        // `display_time_stamps`/clock inputs at the head of `Hud::apply_events` and drains pending
        // notices even without new events; that is a frame phase in its own right.
        // The per-event `apply_hud_events` call lives in `deliver_session_events`; without this
        // empty application, a frame with no link or new
        // packet would not drain a notice that a UI request raised in step 7 of the previous frame.
        let _ = self.apply_hud_events(shell, &[]);
        // The login-event queue's per-frame half. Everything before
        // its event loop -- the link-status and rejected-datagram lines, the net-error report
        // and its no-UI exit, and the `--linger` deadline whose own comment says it is
        // "checked before the batch so a run with no traffic still ends" -- is once-a-frame
        // work, not per-event work. Running the whole call per session event would silently make
        // all four conditional on this frame having had traffic.
        self.process_logon_event_queue(shell, Vec::new());
        spans.step(FrameStep::PacketStep);
        self.events.push(FrameEvent::Step(FrameStep::PacketStep));
        if let Some(link) = self.link.as_mut() {
            link.packet_controller_use_time(net_time);
        }
        spans.step(FrameStep::AssetCacheStep);
        self.events
            .push(FrameEvent::Step(FrameStep::AssetCacheStep)); // Shared asset-cache loads.
        while self
            .link
            .as_mut()
            .is_some_and(|link| link.net.session.process_next_cache())
        {
            self.deliver_session_events(shell, dereth_primitives::LocalTime(self.timer.cur_time));
        }
        // The other half of the shared asset cache: a get whose object is
        // neither in memory nor on disk sends the object-missing request `0xF7E3`.
        // This runs in the same frame phase as the queue-5 drain above, because in retail
        // both operations belong to the same cache.
        self.request_missing_cell_records();
        // Step 7 is **where the phase state machine
        // advances**, after global message 3 and with the queued mode switch running last.
        // The input-manager update is step 6 inside it.
        spans.step(FrameStep::UiStep);
        self.events.push(FrameEvent::Step(FrameStep::UiStep));
        self.ui_use_time(shell, dereth_primitives::LocalTime(self.timer.cur_time));
        // Input action dispatch's
        // input-handler leg for the movement interpreter and camera controller: the actions the UI declined
        // become motion commands and camera rotations *here*, downstream of `walk_input_maps` and
        // therefore of the typing barrier. It is before `WorldScene::update` reads `char_input`,
        // and after step 7 because the UI gets first refusal.
        self.apply_input_actions(shell);
        self.apply_orbit_camera();
        spans.step(FrameStep::WorldViewStep);
        self.events.push(FrameEvent::Step(FrameStep::WorldViewStep));
        // The landscape cannot be built until the server says where the player is, and the objects
        // cannot be drawn until it exists. Both happen here, outside the frame bracket.
        self.load_pending_scene();
        // Object maintenance runs only while the cell manager is not blocking,
        // before physics. No connection/no player body is not itself a cell-load blocker.
        if self.pending_scene.is_none() {
            let (geometry, radius) = self.current_selection_geometry();
            let session = self.link.as_mut().map(|link| &mut link.net.session);
            let inter = &mut self.interaction;
            let hud = &mut self.hud;
            let now = dereth_primitives::LocalTime(self.timer.cur_time);
            inter.last_use_time = now;
            self.objects.use_time_with_dispatch(
                dereth_primitives::ServerTime(self.timer.cur_time),
                session,
                &mut |world, notice| {
                    inter.dispatch_object_notice(world, notice.clone(), &geometry, radius, now);
                    object_panel_notice(shell, hud, inter, world, &notice);
                },
            );
            self.deliver_object_notices();
        }
        self.sync_objects();
        // The teleport's second half. Run the smart-box teleport arm at its own frame step and
        // **after** the UI animation ran inside step 7; see
        // `crate::teleport::Teleport::step_world_view` for why that order is load-bearing. It
        // is after `load_pending_scene` because "the cell manager is no longer blocking" is what
        // this build reads as "the body is standing in a loaded scene".
        let blocking = self.pending_scene.is_some()
            || self
                .world
                .as_ref()
                .and_then(|w| w.character.as_ref())
                .is_none()
            || self
                .present
                .scene(self.world.as_ref())
                .is_some_and(|s| s.loading_near_viewer());
        self.teleport.step_world_view(blocking);
        // The sound world step: sets the listener, fires the animation sound hooks and runs
        // the ambient sounds.
        if self.pending_scene.is_none() {
            crate::audio::world_use_time(
                self.audio.as_mut(),
                self.present.scene_mut(self.world.as_mut()).as_deref_mut(),
                &mut self.objects,
                self.anim_assets.as_ref(),
                &self.store,
                dereth_primitives::LocalTime(self.timer.cur_time),
            );
        }
        // The world simulation's slot. With no world the free camera is here, and this is
        // where a viewpoint moves -- once per frame, off the one sample.
        #[allow(clippy::cast_possible_truncation)]
        // LINT-OK: a frame delta in seconds, narrowed for the camera's own f32 arithmetic. It is
        // not engine arithmetic: the free camera is a debug flycam, not the camera controller.
        let dt = (self.timer.cur_time - self.last_time).clamp(0.0, 0.25) as f32;
        self.last_time = self.timer.cur_time;
        let input = self.input;
        let char_input = self.char_input;
        // The rising-edge controls are consumed here, once, so a key press cannot be issued twice.
        self.char_input.jump = false;
        let now = dereth_primitives::LocalTime(self.timer.cur_time);
        // Motion interpolation asks the weenie for the run rate *live*, on every
        // `apply_run_to_command`, `get_max_speed`, `get_adjusted_max_speed` and
        // `get_state_velocity`; here the answer is a copied fact in `MotionEnv`, so it is refreshed
        // once per frame, before `world.update` -> `Character::apply_input` issues the motion whose
        // speed it scales. Without this every body runs at the `my_run_rate` default of 1.0 and
        // the Run skill does not reach the ground at all.
        if let Some(character) = self
            .world
            .as_ref()
            .and_then(|w| w.character.as_ref())
            .filter(|_| self.pending_scene.is_none())
        {
            crate::character::refresh_run_rate(
                character,
                self.hud.player_desc(&self.objects.world),
                self.hud.skill_table.as_ref(),
                self.hud.quality_filter.as_ref(),
                &self.objects.world.world_rules,
            );
        }
        if let Some(mut world) = self
            .present
            .scene_mut(self.world.as_mut())
            .filter(|_| self.pending_scene.is_none())
        {
            world.world_mut().smooth_animation = self.smooth_animation;
            world.update(input, char_input, now, dt);
            if let Some(c) = world.character() {
                self.objects.publish_physics_cells(&c.world);
            }
            // The real camera, whose swept
            // sphere replaces the debug chase camera `world.update` just placed. It runs here
            // rather than inside `world.update` because the sweep starts from the pivot the body
            // has just moved to and must be expressed in the block the re-centre has just chosen.
            //
            // **What that costs is measured, and the line stays where it is.** The client
            // sweeps and re-centres inside one (`update_viewer`
            // then the normal world render); here the re-centre inside `world.update` reads the
            // viewpoint this line left **last** frame. Measured over five walking stations, that
            // lag is one frame of camera travel: 0.099 m walking,
            // 0.152 m running, 0.192 m off-axis, and a 0.971 m worst case that is a camera being
            // released from a wall rather than one following a body -- 0.41% to 4.05% of a 24 m
            // cell. Across 5,400 frames, 33 take a different cell decision and 3 a different block
            // decision, and no single frame ever moves the cell index by more than 1: each is a
            // boundary reached one frame late, never one skipped.
            //
            // The block the sweep is expressed in is what binds, and it binds on the half of this
            // call that is not the sweep. `CameraControl::update_viewer` is block-independent --
            // it works in `Position` and physics space -- but this function's tail is
            // `scene.camera = FreeCamera::from_frame(&c.camera_render_frame()?)`, and
            // `Character::render_frame_of` subtracts the **current** viewer block. Above the
            // re-centre that leaves the render camera in the block the frame is leaving: a 192 m
            // error once per crossing, bought with a 0.97 m worst-case lag. A faithful move is a
            // split -- sweep above, projection below -- and the measurement does not pay for it.
            // See `WorldScene::viewpoint` for the table and the rest of the reasoning.
            crate::camera::update_viewer(&mut *world, input, now, f64::from(dt));
        }
        // Camera rotation turns the *body*
        // in two of its three arms — in first person, and whenever
        // `Input.UseMouseTurning` is on — and the calls it makes to do it are the command
        // interpreter's, not the camera's. It runs here, immediately after `update_viewer`,
        // because `update_viewer` is where held-key camera repeat reaches
        // `Rotate` at all; see [`Self::apply_camera_turn`].
        self.mouse_look_idle(now);
        self.apply_camera_turn(now);
        // Target tracking: the camera update routine at the three edges retail raises it on — see
        // [`Self::last_target_tracking`] for why an edge detector is the faithful shape here and a
        // per-frame re-run is not.
        //
        // It runs **after** `update_viewer` rather than before for the same reason
        // `update_viewer` is where it is: it ends in `set_target_for_offset`, which
        // only writes, and `update_camera` reads that mask at the top
        // of the *next* frame's sweep. Putting it before would have it decide on a selection the
        // frame's own physics had not yet moved the body away from.
        self.update_target_tracking();
        // Apply the three player-option environment arms (persistent day,
        // weather, and fog) plus their initialization copies. This sits beside
        // `update_target_tracking` because it
        // is the fourth arm of the same option-change switch and the same kind of edge; after it
        // for no reason except that retail's player-module initialization runs them in that order.
        self.apply_player_option_effects();
        // The callback queue is outside the cell-blocked simulation branch. Released
        // arrivals and control-loss hooks finish before the next message and command tail.
        self.step_incoming_world_objects(shell);
        self.command_interpreter_control_transfer(shell);
        self.position_use_time();
        // The landblock window was re-centred inside `world.update`;
        // this is the fetch it asked for, and it is here rather than a line later because
        // `Gpu::upload_texture` runs a command list of its own and the frame begin is next.
        self.stream_world();

        // Smart-box drawing builds the selection ray from
        // *this* frame's camera and raises the smart-box object-found notice before the UI overlay
        // draws, so the pick runs after the camera update and before the frame bracket.
        self.interaction_use_time(shell);
        // This frame's requests have gone out; a headless client with no server has its stand-in
        // answer them, and the answers are read from the next frame on.
        if let Some(stub) = self.server_stub.as_mut() {
            stub.answer(&self.interaction.last_sent, &mut self.objects.world);
        }

        // Apply the object-found notice's tooltip flag
        // and the UI manager's `clear_tooltip` on the wrapper element, one statement after the
        // world draw that raised the notice, for the same reason `update_cursor_state` below is:
        // this is where `click_object_id` becomes known, and both are calls the notice makes
        // through a singleton `interaction.rs` cannot see. Nothing observes the flag until the
        // next UI frame, whose `check_tooltip` runs *before*
        // it broadcasts global message 3 to the global-loop listeners — so the client's own
        // ordering is that the pick answer of frame N is read by the tooltip check of frame N+1.
        if shell.has_ui() {
            if let Some(tooltip) = self.interaction.take_world_tooltip() {
                shell.world_tooltip(tooltip);
            }
        }

        shell.draw_world_target(&mut UiContext::new(self));

        // Cursor-state update chooses one of the dat's 41 cursors
        // and pushes it as the manager's **default**. Retail calls it from eight notice
        // handlers rather than from the frame loop — busy-count up and down, target-mode change,
        // use, examine object, examine spell, use-done and the object-found notice — and the
        // union of those is "whenever any of its five inputs moved". Calling it once a frame is
        // the same answer and cannot miss an edge, because the current and last cursor ids
        // between them make a redundant call free.
        //
        // It sits **here**, immediately after `interaction_use_time`, because that is where
        // smart-box drawing completes the pick that writes `click_object_id` — the `h` of
        // the state machine — and before the frame bracket, where a `SetCursor` would be
        // competing with the swap chain.
        shell.update_cursor(&mut UiContext::new(self));

        // Copy the completed frame into the UI image, and the
        // mirror `Paste` reads back. Here, beside the cursor drain, because both are
        // the same shape: `dereth-ui` records what only a host with a window can perform. Both
        // directions are cheap -- the send happens only on a Copy, the read only when
        // `GetClipboardSequenceNumber` moved.
        shell.sync_clipboard();

        shell.compose_ui(&mut UiContext::new(self));

        // The performance panel's font goes up outside the frame bracket, once.
        let perf_panel = dereth_client_contract::options::performance::shown();
        if perf_panel && !self.perf_font {
            match self
                .present
                .overlay_upload(crate::perf::FONT_TEXTURE, &crate::perf::font_sheet())
            {
                Ok(()) => self.perf_font = true,
                Err(e) => tracing::warn!("the performance panel's font: {e}"),
            }
        }

        spans.step(FrameStep::PrepareDevice);
        self.events.push(FrameEvent::Step(FrameStep::PrepareDevice));
        self.present.prepare_graphics_device();

        spans.step(FrameStep::BeginFrame);
        self.events.push(FrameEvent::Step(FrameStep::BeginFrame));
        if let Err(e) = self.present.start_frame() {
            tracing::error!("starting a frame failed: {e}");
            if let Some(effect) = self.resolution.cancel() {
                self.apply_resolution_effect(effect);
            }
            self.state = AppState::ShuttingDown;
            return false;
        }

        // **The 3D viewport follows the smart box.**
        //
        // Moving or resizing a docked object updates the smart-box region
        // and broadcasts the resulting change. The
        // handler that decides the final rectangle **ignores the notice's four parameters** and
        // re-reads its own region.
        //
        // So the rectangle is `<SBOX>`'s screen box, and reading it here -- once per frame, from
        // the live tree -- is the same answer the notice would carry with none of the machinery,
        // because a notice this build has no subscriber list for would be transcribed and unwired
        // rather than a fix. What is *not* optional is that something consume it: without this,
        // `hud::world_view::client_rect` and `dereth_render::camera::compute_game_viewport` are
        // called by nobody and the viewport never moves.
        spans.step(FrameStep::DrawWorld);
        self.events.push(FrameEvent::Step(FrameStep::DrawWorld));
        self.present.set_game_viewport(shell.game_viewport());
        if let Err(e) = self.present.draw_scene(self.world.as_ref()) {
            tracing::warn!("draw failed: {e}");
        }

        // Ending the frame with `true` is three things in one call: **the 2D UI
        // overlay**, `EndScene` and `Present`. The overlay is composited over the finished 3D
        // frame with the depth test off, which is why it is inside this step and not a step of its
        // own -- the documented frame order has fourteen calls and this is one of them.
        spans.step(FrameStep::PresentFrame);
        self.events.push(FrameEvent::Step(FrameStep::PresentFrame));
        if let Err(e) = shell.draw_ui(&mut self.present) {
            tracing::warn!("the UI overlay failed: {e}");
        }
        // The performance panel, over whatever the interface drew.
        if perf_panel && self.perf_font {
            let items = crate::perf::panel_items(&self.perf.summary().lines(), self.present.size());
            if let Err(e) = self.present.draw_overlay(&items) {
                tracing::warn!("the performance panel failed: {e}");
            }
        }
        if let Err(e) = self.present.end_frame() {
            tracing::error!("finishing a frame failed: {e}");
            if let Some(effect) = self.resolution.cancel() {
                self.apply_resolution_effect(effect);
            }
            self.state = AppState::ShuttingDown;
            return false;
        }
        self.events.push(FrameEvent::FrameDrawn);

        spans.step(FrameStep::PaceFrame);
        self.events.push(FrameEvent::Step(FrameStep::PaceFrame));
        self.pacer.frame_sleep(self.pump.state.is_active_app);
        self.perf_steps = spans.times();

        // `--enter-world` ends the loop when its script has run: `0x0013` made the session
        // playable, the log-off went out and the server answered. The retail equivalent is
        // Shift+Escape -> EXIT -> Yes, so the account is not left logged in until the server's own timeout.
        if self.script == EnterWorldScript::Done {
            self.pump.done();
            if let Some(effect) = self.resolution.cancel() {
                self.apply_resolution_effect(effect);
            }
            self.state = AppState::ShuttingDown;
            return false;
        }

        // `--frames n` is this rebuild's test limit; the ordinary client runs until shutdown.
        if self.cfg.frames.is_some_and(|n| self.frames_drawn() >= n) {
            self.pump.done();
            if let Some(effect) = self.resolution.cancel() {
                self.apply_resolution_effect(effect);
            }
            self.state = AppState::ShuttingDown;
            return false;
        }
        true
    }
}
