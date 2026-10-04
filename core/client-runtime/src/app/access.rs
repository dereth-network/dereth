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

    /// The HUD state, for the report line and the tests.
    #[must_use]
    pub fn hud(&self) -> &S::Hud {
        &self.hud
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

    /// The object tables and the render facts, for the tests and the log lines.
    #[must_use]
    pub fn objects(&self) -> &crate::objects::ObjectStream {
        &self.objects
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

    /// The window title bar, for the hand check of the windowed path.
    pub fn set_title(&self, title: &str) {
        self.window.set_title(title);
    }
}
