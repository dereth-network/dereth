//! Application state access and direct event delivery.

use super::*;

impl<S: Shell> App<S> {
    /// How busy the client is, as any UI's cursor shows it: the world's one shared busy count,
    /// raised by a teleport, a cast, a use, a shop request, a swing, an examine and the allegiance
    /// request until each is answered. Nonzero is the hourglass.
    #[must_use]
    pub fn busy_count(&self) -> u32 {
        self.objects.world.magic.busy_count
    }

    /// The smart-box field-of-view distance with its override off: the game view distance.
    ///
    /// The aspect is built the same way the drawn world's view parameters build it, so the
    /// distance the animation ramps away from is the one the world is actually drawn at.
    pub(super) fn game_view_distance(&self) -> f32 {
        use dereth_client_contract::camera as cam;
        let (w, h) = self.present.size();
        #[allow(clippy::cast_precision_loss)]
        // LINT-OK: a back-buffer extent, at most a few thousand.
        let (fw, fh) = (w as f32, h as f32);
        // `Render.AspectRatio` and `Render.FieldOfView`, from the live scene when
        // there is one; the `Config` copy is the answer before a world is loaded, matching the
        // process render preferences at that point.
        let prefs = self
            .present
            .scene(self.world.as_ref())
            .map_or(self.cfg.render, |s| s.render_preferences());
        let display = prefs.aspect().display_aspect_ratio(fw, fh);
        let aspect = cam::compute_aspect_for_viewport(fw, fh, display, false);
        let fov = cam::fov_y_from_preference(prefs.game_fov_rad(), aspect);
        cam::view_distance_from_fov(fov)
    }

    /// The teleport state, for the report line and the tests.
    #[must_use]
    pub const fn teleport(&self) -> &crate::teleport::Teleport {
        &self.teleport
    }

    /// The teleport state, mutably, so a test can feed it the session events a live server would
    /// and then let the application's own frame do the rest.
    pub const fn teleport_mut(&mut self) -> &mut crate::teleport::Teleport {
        &mut self.teleport
    }

    /// The HUD state, for the report line and the tests.
    #[must_use]
    pub fn hud(&self) -> &S::Hud {
        &self.hud
    }

    /// The HUD state, mutably, so a test can feed it a recorded session's events.
    pub fn hud_mut(&mut self) -> &mut S::Hud {
        &mut self.hud
    }

    /// The character controls the input seam has latched, for the tests.
    ///
    /// The only writer is [`Self::apply_input_actions`], which runs on what
    /// `fire::walk_input_maps` produced -- so reading `forward` here is reading "did a control
    /// survive the walk", which is precisely what the typing barrier decides.
    #[must_use]
    pub const fn char_input(&self) -> crate::character::CharacterInput {
        self.char_input
    }

    /// The camera/flycam controls, for the tests.
    #[must_use]
    pub const fn camera_input(&self) -> CameraInput {
        self.input
    }

    /// The command interpreter's three command lists and the counters
    /// on them, for the tests.
    ///
    /// The one that matters is
    /// [`crate::character::MovementCommands::transient_motions_issued`], which is the denominator
    /// for `move_player`'s do-motion/stop-motion tail: *no emote key was pressed* and
    /// *the drain never ran* are the same reading of an empty queue without it.
    #[must_use]
    pub const fn movement(&self) -> &crate::character::MovementCommands {
        &self.movement
    }

    /// The movement command interpreter's three command lists, for the tests.
    ///
    /// "Hold `A`, press `D`, release `D`, still turning left" is a statement about the **turn
    /// list**, not about a bool, so the test has to be able to read the list.
    #[must_use]
    pub const fn movement_commands(&self) -> &crate::character::MovementCommands {
        &self.movement
    }

    /// The body's real camera controller, for the tests.
    ///
    /// The eight commands of [`Self::apply_world_camera_action`] are arithmetic and ordering —
    /// a viewer offset, a stiffness, two mode flags — so they are asserted against the client's
    /// own arms rather than by eye.
    #[must_use]
    pub fn camera_control(&self) -> Option<&crate::camera::CameraControl> {
        self.world
            .as_ref()
            .and_then(|w| w.character.as_ref())
            .map(|c| &c.camera)
    }

    /// Return whether anything has asked the main loop to stop.
    ///
    /// A test reads it to assert that a plain `Escape` does not end the process. Ending the loop
    /// on `Escape` straight from the raw event drain would let one `Escape` with the chat entry
    /// focused terminate a live client.
    #[must_use]
    pub const fn device_done(&self) -> bool {
        self.pump.state.is_done
    }

    /// Everything this session's frames did, in order, with the totals every
    /// counter accessor above reads. See [`crate::frame_events`].
    #[must_use]
    pub const fn frame_events(&self) -> &FrameEvents {
        &self.events
    }

    /// The sound subsystem, for the tests and for the report line.
    pub fn audio_mut(&mut self) -> Option<&mut crate::audio::Audio> {
        self.audio.as_mut()
    }

    /// What the screens read out of globals this frame.
    #[must_use]
    pub fn host_state(&self) -> &HostState {
        &self.host_state
    }

    /// The same, mutably, for a test that has no server to hear it from.
    ///
    /// [`Self::build_host_state`] rewrites the five fields it reads off the link every frame and
    /// leaves the rest alone, so the character set, the world name and the char-gen response
    /// survive being written here -- which is what lets the character screens be driven offline.
    pub fn host_state_mut(&mut self) -> &mut HostState {
        &mut self.host_state
    }

    /// What the three host-side action arms did:
    /// `(visibility-toggle dispatches, of those that answered TRUE, stop-completely calls,
    /// screenshots saved, screenshots failed)`.
    #[must_use]
    pub const fn action_arm_host_stats(&self) -> (u64, u64, u64, u64, u64) {
        (
            self.events.total(FrameEventKind::EscapeOptionsToggle),
            self.events.amount(FrameEventKind::EscapeOptionsToggle),
            self.events.total(FrameEventKind::EscapeStopPerformed),
            self.events.total(FrameEventKind::ScreenshotSaved),
            self.events.total(FrameEventKind::ScreenshotFailed),
        )
    }

    /// The object tables and the render facts, for the tests and the log lines.
    #[must_use]
    pub fn objects(&self) -> &crate::objects::ObjectStream {
        &self.objects
    }

    /// The same tables, mutable. A test seeds a player and an inventory the
    /// way a `0x0013 Login_PlayerDescription` would, so that a combat-mode toggle has something to
    /// decide against without a server.
    pub fn objects_mut(&mut self) -> &mut crate::objects::ObjectStream {
        &mut self.objects
    }

    /// The interaction layer and the game model **at the same time**.
    ///
    /// Several of the production entry points a harness has to call take both --
    /// `Interaction::on_world_object_found(found, &mut world, now)` is the one that named it -- and
    /// [`Self::interaction_mut`] and [`Self::objects_mut`] are two separate borrows of `self`, so a
    /// caller with only those two has to build a model host of its own per scenario. They are
    /// disjoint fields; this hands out both.
    pub fn interaction_and_world_mut(
        &mut self,
    ) -> (
        &mut crate::interaction::Interaction,
        &mut dereth_client_model::World,
    ) {
        (&mut self.interaction, &mut self.objects.world)
    }

    /// Install a socket-free replay endpoint on a headless App that has no live link. The real
    /// frame consumes its raw transport queues; no decoded event/model replacement is injected.
    /// Returns the unconsumed endpoint on refusal, preserving an existing live/replay session.
    #[allow(clippy::result_large_err)] // the refused network is handed back whole, once
    pub fn attach_replay_network(
        &mut self,
        net: crate::net::ClientNetwork,
    ) -> Result<(), crate::net::ClientNetwork> {
        if !self.cfg.headless || self.link.is_some() {
            return Err(net);
        }
        self.link = Some(NetLink::replay(net));
        Ok(())
    }

    /// Install a socket-free endpoint whose datagrams the host carries (a web page's relay), on
    /// an App of any kind that has no link yet. It is the replay endpoint's arrangement for a
    /// windowed client: the frame consumes the transport queues the host fills and drains, and
    /// nothing else about the run changes.
    ///
    /// # Errors
    /// The endpoint back, unconsumed, when the App already has a link.
    #[allow(clippy::result_large_err)] // the refused network is handed back whole, once
    pub fn attach_relay_network(
        &mut self,
        net: crate::net::ClientNetwork,
    ) -> Result<(), crate::net::ClientNetwork> {
        if self.link.is_some() {
            return Err(net);
        }
        self.link = Some(NetLink::replay(net));
        // A real server is behind the relay, and it answers for itself.
        self.server_stub = None;
        Ok(())
    }

    /// Feed/inspect only an explicitly socket-free endpoint, never an owner's live connection.
    /// The dat store this client reads, shared with its world. A probe for the tests: whether
    /// the DDD interrogation's product id granted `client_highres.dat`.
    #[must_use]
    pub fn dat_store(&self) -> &std::sync::Arc<dereth_dat::RetailDatStore> {
        &self.store
    }

    pub fn replay_network_mut(&mut self) -> Option<&mut crate::net::ClientNetwork> {
        self.link
            .as_mut()
            .filter(|link| link.local_addr().is_none())
            .map(|link| &mut link.net)
    }

    /// The interaction state: `WorldObjects`'s pick, the `SearchReason` machine and the request
    /// counters.
    #[must_use]
    pub fn interaction(&self) -> &crate::interaction::Interaction {
        &self.interaction
    }

    /// The same state, writable — the sibling of [`Self::objects_mut`] and
    /// `Self::renderer_mut`, and for the same reason: a harness that drives a real `App` must be
    /// able to hand it the pointer events would have
    /// dispatched, which is [`crate::interaction::Interaction::queue`]. Without it
    /// the only way to exercise a click against a *resized* smart box is to build an `Interaction`
    /// by hand, and an `Interaction` built by hand has never had `App::frame` push
    /// `<SBOX>`'s rectangle into it, which is precisely the step under test.
    pub fn interaction_mut(&mut self) -> &mut crate::interaction::Interaction {
        &mut self.interaction
    }

    /// The production UI-queue interaction boundary, also usable without a socket by replay
    /// harnesses. `App::frame` delivers the already ordered session events here.
    ///
    /// **It runs with no panels callback and no selection geometry**, which is not what
    /// [`Self::frame`] does — see [`Self::apply_interaction_events_to_panels`] for the boundary
    /// the frame really uses. This one is kept as it is because eighty-odd stations under
    /// `tests/` are written against it.
    pub fn apply_interaction_events(
        &mut self,
        events: &[dereth_client_net::client_session::SessionEvent],
    ) {
        self.interaction.last_use_time = dereth_primitives::LocalTime(self.timer.cur_time);
        crate::interaction::apply_events(&mut self.interaction, events, &mut self.objects.world);
    }

    /// The same boundary **as `App::frame` runs it**: the current selection geometry, and the
    /// frame's own panels callback, so that a panel which is a cached join of the notice stream
    /// sees the message.
    ///
    /// [`Self::apply_interaction_events`] passes `None` geometry and a no-op
    /// `panels` argument, so a harness that delivers through it has every panel blind to the
    /// message it delivers -- silently, which makes a wrong assertion look right rather than
    /// red. The two lines below are `frame`'s own, lifted so a harness can reach them; `frame`
    /// itself is unchanged and still calls `apply_events_at_boundary` inline.
    pub fn apply_interaction_events_to_panels(
        &mut self,
        shell: &mut S,
        events: &[dereth_client_net::client_session::SessionEvent],
    ) {
        self.interaction.last_use_time = dereth_primitives::LocalTime(self.timer.cur_time);
        let (geometry, radius) = self.current_selection_geometry();
        let hud = &mut self.hud;
        crate::interaction::apply_events_at_boundary(
            &mut self.interaction,
            events,
            &mut self.objects.world,
            Some((&geometry, radius)),
            &mut |inter, world, notice| object_panel_notice(shell, hud, inter, world, notice),
        );
    }

    /// How many input actions [`Self::apply_input_actions`] routed into the body or
    /// the camera this session.
    ///
    /// A denominator, not decoration: "typing did not walk the character" is the same observation
    /// as "no action arrived at all", and a test that cannot tell them apart passes on a build that
    /// blocks movement for ever.
    #[must_use]
    pub const fn actions_routed(&self) -> u64 {
        self.events.total(FrameEventKind::ActionRouted)
    }

    /// Device completion is the single quit switch. Every documented exit path routes
    /// through it: `WM_CLOSE`/`WM_DESTROY`, `SC_CLOSE`, the `Exit`/`Quit` console commands, and the
    /// epilogue UI's confirmation.
    pub fn done(&mut self) {
        self.pump.done();
    }

    /// The context a front end's step is handed, for a host that feeds its front end outside the
    /// frame (a window event delivered directly, say).
    pub fn ui_context(&mut self) -> UiContext<'_, S> {
        UiContext::new(self)
    }

    /// What the last frame did, in call order. The frame-order test reads this.
    #[must_use]
    pub fn last_frame_steps(&self) -> &[FrameStep] {
        FrameRecorder::new(&self.events).steps()
    }

    #[must_use]
    pub fn state(&self) -> AppState {
        self.state
    }

    #[must_use]
    pub const fn frames_drawn(&self) -> u64 {
        self.events.total(FrameEventKind::FrameDrawn)
    }

    #[must_use]
    pub fn config(&self) -> &Config {
        &self.cfg
    }

    /// The device state, read-only — including the full-screen display preference, which is
    /// what Alt+Enter and the `Display.FullScreen` option both write and what
    /// [`Self::change_presentation`] applies.
    #[must_use]
    pub fn device_state(&self) -> &dereth_client_contract::window_proc::DeviceState {
        &self.pump.state
    }

    /// The clock's local/server times and the server offset behind them, read-only.
    ///
    /// Exists so a test can tell a clock that *rejected* a stale `TimeSync` from a map panel that
    /// merely did not redraw inside its five-second throttle. Those are two different builds and
    /// the rendered date alone cannot separate them.
    #[must_use]
    pub fn clock(&self) -> &Clock {
        &self.timer
    }

    /// The presentation this `App` was built with.
    ///
    /// A headless harness reads its [`crate::present::NullPresentation`] counters back through
    /// this.
    #[must_use]
    pub fn presentation(&self) -> &S::Present {
        &self.present
    }

    /// The world state this `App` owns: the body, the server objects' simulation,
    /// the residency window, the camera and the clock. `None` before a world is loaded, and with
    /// a presentation that does not build a world.
    #[must_use]
    pub fn world_state(&self) -> Option<&crate::world_state::WorldState> {
        self.world.as_ref()
    }

    /// …and writable.
    pub fn world_state_mut(&mut self) -> Option<&mut crate::world_state::WorldState> {
        self.world.as_mut()
    }

    /// Have the presentation load a world, straight away and outside the frame:
    /// what the device tests did through the renderer, with the world state landing here.
    ///
    /// # Errors
    /// As [`crate::present::Presentation::load_world`].
    pub fn load_world(
        &mut self,
        store: &std::sync::Arc<dereth_dat::RetailDatStore>,
        cfg: crate::scene::SceneConfig,
    ) -> Result<(), crate::landblock::WorldError> {
        self.present.load_world(store, cfg, &mut self.world)
    }

    /// The presentation's world draw on its own, from this `App`'s world state:
    /// what the device tests did through the renderer.
    ///
    /// # Errors
    /// As [`crate::present::Presentation::draw_scene`].
    pub fn draw_world_scene(&mut self) -> Result<(), crate::present::PresentError> {
        self.present.draw_scene(self.world.as_ref())
    }

    /// The window title bar, for the hand check of the windowed path.
    pub fn set_title(&self, title: &str) {
        self.window.set_title(title);
    }
}
