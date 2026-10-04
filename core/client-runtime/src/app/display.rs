//! Host events, display changes, and resolution transactions.

use super::*;

impl<S: Shell> App<S> {
    /// Drain the window queue completely, then process the Alt+Enter latch and return the done
    /// state.
    ///
    /// "the pump drains the queue completely each frame — it is not the classic `PeekMessage`-or-
    /// render alternation", which is what a zero timeout on `pump_events` gives.
    pub(super) fn do_event_loop(&mut self, shell: &mut S) -> bool {
        let time_ms = self.clock.tick_ms();
        // The full-screen-allowed flag. `finish_event_loop` reads it --
        // `s.full_screen = s.allow_full_screen_mode && !s.full_screen` is the event loop's own
        // epilogue -- and left at its `true` default it would make Alt+Enter live in every mode.
        //
        // It is the exact gate the full-screen rule needs: full screen belongs to
        // gameplay, so Alt+Enter is refused outside it rather than honoured and then quietly
        // undone by [`Self::follow_gameplay_full_screen`] at the next mode edge. Inside gameplay
        // the toggle is untouched and still overrides the preference until the player leaves.
        //
        // Set every frame rather than on the mode edge because it has to be right at the instant
        // the drain below consumes the latch, which is not an edge frame.
        self.pump.state.allow_full_screen_mode = shell.in_gameplay();
        let drained = self
            .window
            .pump_events((self.cfg.width, self.cfg.height), self.applied_full_screen);

        for event in drained.events {
            self.handle_window_event(shell, &event, time_ms);
        }
        shell.window_input(&mut UiContext::new(self), time_ms);
        if drained.exited {
            self.pump.done();
        }

        // The renderer exists whenever the graphics engine is up, which it is here.
        let done = self.pump.finish_event_drain(true);
        // The `Display.FullScreen` shadow comparison runs once a
        // frame: a value that differs from what the presentation is wearing triggers a change.
        // Both producers land here — Alt+Enter, which
        // `finish_event_drain` has just applied to the full-screen display preference, and
        // startup/live window state. Display requests use this same helper immediately after their
        // UI drain, matching retail's later-in-frame device preparation.
        //
        // The second half of the same flag compares the resolution display preference against
        // its own shadow *before* the full-screen one and raises the identical flag, so a
        // resolution change reaches `change_presentation` by exactly this route. The comparison
        // here is on the decoded `(width, height)` rather than on the packed mode descriptor,
        // because this build's presentation is the client rectangle and that is what the shadow has
        // to be able to answer for.
        self.apply_changed_display_presentation(shell);
        done
    }

    /// Apply the event loop's display-shadow transition.
    ///
    /// The shared check keeps the event-loop producer and the UI preference producer on the same
    /// transition: once [`Self::change_presentation`] moves the shadows, the next poll is a no-op.
    pub(super) fn apply_changed_display_presentation(&mut self, shell: &mut S) {
        if self.pump.state.full_screen != self.applied_full_screen
            || (self.cfg.width, self.cfg.height) != self.applied_resolution
            || self.cfg.display.sync_to_refresh != self.applied_sync_to_refresh
        {
            self.change_presentation(shell);
        }
    }

    /// The App consumer for one host event, also used by socket-free synthetic window-event
    /// tests. DPI size negotiation belongs earlier, inside the host's live callback.
    pub fn handle_window_event(&mut self, shell: &mut S, event: &HostEvent, time_ms: u32) {
        let refresh_frame = matches!(event, HostEvent::ScaleFactorChanged)
            || matches!(event, HostEvent::Resized { width, height } if *width > 0 && *height > 0);
        if refresh_frame && !self.applied_full_screen {
            // The caption changes DPI even when our override keeps the client pixel extent
            // unchanged (which need not generate a Resized event). Refresh on either edge.
            if let Some(metrics) = self.window.frame_metrics() {
                self.frame_metrics = metrics;
            }
        }
        if let HostEvent::Resized { width, height } = event {
            if *width > 0 && *height > 0 {
                // A monitor/DPI change can resize the host without a preference change. Keep
                // the backbuffer and UI/hit-test extent in the host's same physical coordinates.
                // No reposition or preference write here; a minimized zero extent is ignored.
                self.restart_rendering_system(shell, *width, *height);
            }
        }
        self.note_flycam_input(event);
        // The window procedure runs its message table over the lifecycle messages the event maps
        // to. A front end whose input layer listens to the same messages (focus loss releases
        // every held control) hands them on itself.
        self.pump.on_window_event(event, time_ms);
        //  / `Deactivate` also reach the sound system:
        // `Focus` is what the "play only when the window is active" preference reads.
        if let HostEvent::Focused(active) = event {
            if let Some(audio) = self.audio.as_ref() {
                audio.set_focus(*active);
            }
        }
    }

    /// Build the two display-preference choice lists from the
    /// modes this machine reports.
    ///
    /// Returns how many resolution rows the drop-down will hold. The fall-back to
    /// [`STANDARD_DISPLAY_MODES`] is the declared divergence documented on that constant; the
    /// line it prints says which source was used, because "the list is short" and "the list came
    /// from the wrong place" are the same symptom otherwise.
    pub fn register_display_modes(&mut self) -> usize {
        use dereth_client_contract::options::store::{self, DisplayMode};

        let mut modes = self.window.display_modes();
        let enumerated = modes.len();
        if modes.is_empty() {
            modes = STANDARD_DISPLAY_MODES
                .iter()
                .map(|(width, height)| DisplayMode {
                    width: *width,
                    height: *height,
                    refresh_rate: 0,
                    bits_per_pixel: 32,
                })
                .collect();
        }
        store::initialize_display_preferences(&modes);
        let n = store::display_choices(store::DISPLAY_RESOLUTION).len();
        tracing::debug!(
            "-- {enumerated} adapter mode(s), \
             {n} resolution(s), {} refresh rate(s){}",
            store::display_choices(store::DISPLAY_REFRESH_RATE).len(),
            if enumerated == 0 {
                " (standard table: no adapter modes reported)"
            } else {
                ""
            }
        );
        n
    }

    /// Change presentation, minus the D3D9 device reset this build does not have.
    ///
    /// The client changes the window style, applies a frame-changed `SetWindowPos`, resets the
    /// presentation, moves the window to its final rectangle and Z-order, then notifies the UI of
    /// the display change.
    ///
    /// The order is the client's, including the hazard it accepts: the style and the Z-order
    /// change **before** the device is reset and the final move happens **after**. The reset here
    /// is the presentation's resize, the swap-chain half, and the UI half is
    /// [`Shell::set_display`], and this is the caller that makes it run after start-up.
    ///
    /// The presentation stays **windowed** in the D3D sense at all times: this build uses
    /// borderless windowed rather than exclusive mode, so there is no mode switch to make and
    /// [`dereth_client_contract::window_proc::placement`] hands back the monitor's rectangle instead of the
    /// requested resolution. See that function for the divergence in full.
    ///
    /// **The placement function is the second declared divergence in this path.** The final
    /// `SetWindowPos`'s rectangle is
    /// [`dereth_client_contract::window_proc::change_presentation_placement`], whose arithmetic
    /// keeps the window at its existing top-left rather than returning it to the creation-time
    /// centre: a window size change does not snap the window to the centre of the screen, and
    /// leaving full screen puts the window back at the top-left it had before it went full screen.
    /// Read that function for retail's numbers and the off-screen rule.
    fn change_presentation(&mut self, shell: &mut S) {
        let full = self.pump.state.full_screen;
        // The client reads the *previous* presentation's `FullScreen` byte — the copy taken
        // before the display-preference load overwrote it — and branches on it,
        // so the old value has to be captured before the shadow moves.
        let was_full = self.applied_full_screen;
        self.applied_full_screen = full;
        self.applied_resolution = (self.cfg.width, self.cfg.height);
        self.applied_sync_to_refresh = self.cfg.display.sync_to_refresh;
        self.present
            .set_presentation_sync(full, self.cfg.display.sync_to_refresh);
        // **This function is split in two, and the split is retail's own.**
        // `change_presentation`'s window calls (`SetWindowLongA`, the two `SetWindowPos`) act on
        // the window handle; resetting the device and
        // broadcasting global message `(0x0E, 0)` do not touch a window at all. A
        // headless run has no window handle and every other step still applies to it — the
        // offscreen target is resized by the same device reset and the element manager is re-laid
        // out by the same broadcast. Returning at the `host` test would leave a headless `App`
        // unable to apply a presentation change and nothing able to measure one.
        let requested = (self.cfg.width, self.cfg.height);
        // The placement is computed for both paths, and *before* the `host` test, for the same
        // reason the rendering restart is in front of it: the arithmetic touches no window.
        // `GetWindowRect` is the live window when there is one and [`Self::window_rect`]'s
        // maintained rectangle when there is not, so the "do not re-centre" rule is observable
        // headlessly.
        let screen = self.window.screen_metrics(self.frame_metrics);
        let current = if self.window.has_window() {
            self.window.window_rect()
        } else {
            self.window_rect
        };
        // The rectangle whose top-left a windowed result keeps (client divergence CD-001): the
        // window's own for a windowed change; on leaving full screen, the one it had before it
        // went full screen, since its current one is the monitor's. Going full screen remembers
        // it.
        let keep = match (was_full, full) {
            (false, true) => {
                self.windowed_rect_before_full_screen = current;
                None
            }
            (true, false) => self.windowed_rect_before_full_screen.take(),
            (true, true) => None,
            (false, false) => current,
        };
        // **`change_presentation_placement`, NOT `placement`.** Creation placement centres on
        // the screen because it is creating a window; a presentation change has a window already,
        // and this build keeps its **top-left**. Calling `placement` here would snap the window
        // to the middle of the monitor on every resolution pick and on both edges of the
        // forced-resolution path.
        let place = dereth_client_contract::window_proc::change_presentation_placement(
            full,
            true,
            i32::try_from(self.cfg.width).unwrap_or(i32::MAX),
            i32::try_from(self.cfg.height).unwrap_or(i32::MAX),
            keep,
            &screen,
        );
        self.window_rect = Some(dereth_client_contract::window_proc::Rect {
            left: place.x,
            top: place.y,
            right: place.x + place.cx,
            bottom: place.y + place.cy,
        });
        if !self.window.has_window() {
            self.restart_rendering_system(shell, requested.0, requested.1);
            return;
        }
        // **Full screen is the window system's own request; windowed is retail's arithmetic.**
        //
        // The windowed branch is `SetWindowLongA` + `SetWindowPos` as the client makes them:
        // `place.style`'s `WS_POPUP` bit, the Z-order, then the outer rectangle
        // [`dereth_client_contract::window_proc::change_presentation_placement`] computed.
        //
        // The full-screen branch does not imitate a mode switch with those same three calls.
        // Restyle-plus-move-plus-resize is only a fullscreen *request* on Windows; see
        // [`crate::platform::window::WindowHost::set_borderless_fullscreen`] for why it produces
        // a chromeless non-fullscreen window on Wayland and on macOS. The rectangle is still
        // computed above, because the offscreen path below resizes to it and the placement
        // assertions read it; a window, when there is one, is asked instead of arranged.
        let applied = if full {
            self.window.set_borderless_fullscreen(true);
            self.window.set_topmost(place.topmost);
            // The window system answers with the extent it granted, which on every backend is
            // the monitor's and need not be the rectangle computed above -- a scaled display or
            // a compositor with a reserved edge will differ, and the back buffer must follow the
            // window rather than the arithmetic.
            //
            // On Wayland the request is asynchronous and this reads the *old* extent, because the
            // compositor has not answered yet. That is correct and not a race: the answer arrives
            // as a `Resized`, and [`Self::handle_window_event`] puts it straight through
            // `restart_rendering_system`, which is the same call this line ends in. The first
            // frame after the switch is drawn at the old size; every one after it is not.
            self.window.client_size()
        } else {
            self.window.set_borderless_fullscreen(false);
            // The style, exactly as the client's `SetWindowLongA` is.
            self.window
                .set_decorations(place.style & dereth_client_contract::window_proc::WS_POPUP == 0);
            self.window.set_topmost(place.topmost);
            // `cx, cy` are the **outer** rectangle; framed, the host adds the frame back, so the
            // client area asked for is the requested resolution -- which is what
            // the original `cx = Width + 2 * dialog-frame width` calculation specifies.
            let applied = self
                .window
                .request_inner_size(self.cfg.width, self.cfg.height);
            self.window.set_outer_position(place.x, place.y);
            applied
        };
        self.restart_rendering_system(shell, applied.0, applied.1);
    }

    /// Reset the device, then broadcast global message `0x0E` — `change_presentation`'s tail,
    /// with no window in it, lifted out of [`Self::change_presentation`].
    ///
    /// The element manager's `display` must agree with the back buffer or every screen box is
    /// wrong, which is why the broadcast reads the size back off the device rather than trusting
    /// the requested one: `ResizeBuffers` is allowed to answer with a different extent, and a
    /// failed resize must leave the UI on the size the device still has.
    fn restart_rendering_system(&mut self, shell: &mut S, width: u32, height: u32) {
        if (width, height) != self.present.size() && width > 0 && height > 0 {
            if let Err(e) = self.present.resize(width, height) {
                tracing::warn!("presentation change: {e}");
                self.resolution_host_result(true, (width, height));
                return;
            }
        }
        self.resolution_host_result(false, (width, height));
        let (w, h) = self.present.size();
        // Broadcast the global refresh message, which re-lays the UI out at the new extent and
        // pushes UI global message `0x0E` at every registered listener.
        shell.set_display((
            i32::try_from(w).unwrap_or(i32::MAX),
            i32::try_from(h).unwrap_or(i32::MAX),
        ));
    }

    pub(super) fn apply_resolution_effect(&mut self, effect: crate::resolution::ResolutionEffect) {
        let packed = ((effect.size.0 << 16) | effect.size.1) as i32;
        if effect.store {
            dereth_client_contract::options::store::set_value(
                "Display.Resolution",
                dereth_client_contract::PrefValue::Int(packed),
            );
        }
        self.set_display_resolution(packed);
        if !effect.resize {
            // A host failure reconciles the requested shadow without another native resize.
            self.applied_resolution = self.present.size();
        }
    }

    pub(super) fn tick_resolution(&mut self, shell: &mut S, now: f64) {
        use dereth_client_contract::options::interface::Interface;
        self.resolution
            .interface(Interface::chosen() == Interface::Classic);
        if let Some(size) = self.resolution.tick(now, self.present.size()) {
            self.apply_resolution_effect(size);
        }
        let prompt = self.resolution.prompt();
        shell.resolution_prompt(&mut UiContext::new(self), prompt);
    }

    pub(super) fn resolution_host_result(&mut self, failed: bool, attempted: (u32, u32)) {
        if let Some((token, expected)) = self.resolution.awaited() {
            if failed && attempted != expected {
                return;
            }
            if let Some(size) =
                self.resolution
                    .host_result(token, self.present.size(), failed, self.timer.cur_time)
            {
                self.apply_resolution_effect(size);
            }
        }
    }

    /// Apply display-preference requests to the window state.
    ///
    /// `Display.FullScreen` is a registered preference with an options-page row
    /// (`dereth_ui_screens::options::config`'s `GRAPHICS` group).
    /// [`crate::config::Config::apply_preferences`] parses it into
    /// [`crate::config::DisplayPrefs::full_screen`], and this is its reader. The write lands on
    /// the full-screen preference, which is [`dereth_client_contract::window_proc::DeviceState`]'s
    /// `full_screen` — the same field Alt+Enter flips — and the change is applied at the end of
    /// the producing frame after the UI request drain, which is where retail's per-frame poll
    /// applies it too.
    pub(super) fn apply_display_preference_requests(
        &mut self,
        shell: &S,
        requests: Vec<dereth_client_contract::UiRequest>,
    ) -> Vec<dereth_client_contract::UiRequest> {
        use dereth_client_contract::{PrefValue, UiRequest};
        let in_gameplay = shell.in_gameplay();
        let mut resolution: Option<i32> = None;
        let rest: Vec<UiRequest> = requests
            .into_iter()
            .filter(|r| {
                match r {
                    UiRequest::Resolution(action) => {
                        if let dereth_client_contract::resolution::ResolutionAction::Begin {
                            size,
                            ..
                        } = action
                        {
                            if !self.resolution.pending()
                                && (self.use_forced_resolution
                                    || self.pump.state.full_screen
                                    || size.0 < 800
                                    || size.1 < 600)
                            {
                                self.apply_resolution_effect(crate::resolution::ResolutionEffect {
                                    size: *size,
                                    resize: true,
                                    store: true,
                                });
                                return false;
                            }
                        }
                        if let Some(size) = self.resolution.action(
                            *action,
                            self.present.size(),
                            self.timer.cur_time,
                        ) {
                            self.apply_resolution_effect(size);
                        }
                        return false;
                    }
                    UiRequest::SetPreference(name, PrefValue::Bool(v)) => {
                        if name.eq_ignore_ascii_case("Display.FullScreen") {
                            self.cfg.display.full_screen = *v;
                            // **The one rule, in the one place that is not a mode edge.** The
                            // preference is stored whatever mode the player is in -- the options
                            // page is reachable from character select, and a box they tick there
                            // has to survive to the next save. The *shadow* only ever carries it
                            // in gameplay, because that is where full screen lives. Ticking the box
                            // outside gameplay therefore changes nothing on screen until they enter
                            // the world, which is what [`Self::follow_gameplay_full_screen`] then
                            // does.
                            self.pump.state.full_screen = *v && in_gameplay;
                            return false;
                        }
                        if name.eq_ignore_ascii_case("Display.SyncToRefresh") {
                            self.cfg.display.sync_to_refresh = *v;
                            return false;
                        }
                    }
                    // `Display.Resolution` — the resolution drop-down. The drop-down's selection
                    // callback stores the packed mode descriptor straight into the resolution
                    // preference, and the rendering preference poll picks the change up later in
                    // the producing frame. Without this arm the value falls past every consumer and
                    // is printed as *"UI request with no owner yet"*.
                    UiRequest::SetPreference(name, PrefValue::Int(v))
                        if name.eq_ignore_ascii_case(
                            dereth_client_contract::options::store::DISPLAY_RESOLUTION,
                        ) =>
                    {
                        self.resolution.cancel();
                        resolution = Some(*v);
                        return false;
                    }
                    _ => {}
                }
                true
            })
            .collect();
        if let Some(desc) = resolution {
            self.set_display_resolution(desc);
        }
        rest
    }

    /// Store the resolution display preference and decode its width and height.
    ///
    /// The width is the high 16 bits and the height the low 16; below 800x600 the load fails.
    ///
    /// The refusal below 800x600 is the client's and is kept: the display-preference load
    /// answering `false` makes `change_presentation` return `false` without touching anything,
    /// which is what leaving the fields alone reproduces. The packed value is still stored,
    /// because the drop-down's store happens before `change_presentation` runs and it is what the
    /// next preference save writes.
    ///
    /// Returns whether the presentation size moved.
    fn set_display_resolution(&mut self, desc: i32) -> bool {
        let packed = u32::from_ne_bytes(desc.to_ne_bytes());
        self.cfg.display.resolution = packed;
        // The mode decode owns the floor as well; `self.forced_pair()` is
        // the forced-resolution override, which sits *after* the decode and before the caller sees the
        // presentation. So a size chosen while the force is up is stored and not applied — which
        // is retail, and is why the drop-down is only reachable from the gameplay screen.
        let Some(p) = self.cfg.load_display_preferences(None, true) else {
            tracing::warn!(
                "Display.Resolution {packed:#010x} is below \
                 the display-preference load's 800x600 floor; the presentation is unchanged"
            );
            return false;
        };
        // The preference was authored, so this is the size to come back to when the force lifts.
        self.unforced_resolution = (p.width, p.height);
        let p = self
            .cfg
            .load_display_preferences(self.forced_pair(), true)
            .unwrap_or(p);
        if (p.width, p.height) == (self.cfg.width, self.cfg.height) {
            return false;
        }
        self.cfg.width = p.width;
        self.cfg.height = p.height;
        true
    }

    /// The forced width and height when the override is enabled.
    ///
    /// This is the argument [`crate::config::Config::load_display_preferences`] carries for the
    /// display-size override — the mechanism by which retail runs the login and character-select
    /// screens at a fixed size.
    fn forced_pair(&self) -> Option<(u32, u32)> {
        self.use_forced_resolution.then_some(self.forced_resolution)
    }

    /// Whether the display-size override is enabled.
    #[must_use]
    pub fn uses_forced_resolution(&self) -> bool {
        self.use_forced_resolution
    }

    /// The outer window rectangle, used to position the resized window.
    ///
    /// `pub` for the same reason [`Self::force_display_resolution`] is: it is the observable the
    /// "do not re-centre on a size change" rule is asserted on, and the window-position tests
    /// have to be able to read it.
    #[must_use]
    pub fn window_rect(&self) -> Option<dereth_client_contract::window_proc::Rect> {
        self.window_rect
    }

    /// Force display resolution with `(bool force, ulong width, ulong height)`.
    ///
    /// Width, height, and the force flag are always stored first. With no active device, or before
    /// presentation readiness, nothing else happens. Enabling the force changes presentation only
    /// when the forced size differs from the current size. Disabling it changes presentation only
    /// when the old flag was set and the saved preference differs from the current size; an invalid
    /// saved preference still falls through to the forced fallback dimensions.
    ///
    /// The client calls this while entering the login screen (`true`), during construction
    /// (`false`), and when gameplay-screen lifetime changes (`true` unless gameplay is active). The
    /// console command is a fourth entry point.
    /// [`Self::follow_screen_forced_resolution`] is the pair of screen-lifetime calls.
    ///
    /// **`pub` because it is `static` in retail** and because the resolution tests have to lift
    /// the force to reach the drop-down, exactly as opening
    /// Client Options from the gameplay screen does.
    pub fn force_display_resolution(
        &mut self,
        shell: &mut S,
        force: bool,
        width: u32,
        height: u32,
    ) {
        let was_forced = self.use_forced_resolution;
        self.forced_resolution = (width, height);
        self.use_forced_resolution = force;
        // With no renderer or an uninitialized device, the flag is recorded and
        // nothing else happens, which is exactly the state of the startup call.
        if !self.device_is_initialized() {
            return;
        }
        let live = self.present.size();
        let want = if force {
            // /`Height` against the forced pair.
            if live == (width, height) {
                return;
            }
            (width, height)
        } else {
            // un-forcing something that was never forced does nothing at all.
            if !was_forced {
                return;
            }
            // the restore arm, which is the display-preference load with the override
            // now off. See [`Self::unforced_resolution`] for why the target is latched here.
            let target = self.unforced_resolution;
            if live == target {
                return;
            }
            target
        };
        // Below the 800x600 floor the display-preference load answers false, `change_presentation`
        // returns false and nothing moves — the corresponding below-range branch.
        if want.0 < 800 || want.1 < 600 {
            return;
        }
        self.cfg.width = want.0;
        self.cfg.height = want.1;
        // Apply the new dimensions immediately through a tail call, rather than waiting for a poll.
        self.change_presentation(shell);
    }

    /// Presentation readiness and renderer availability are the two gates guarding every arm of
    /// [`Self::force_display_resolution`].
    ///
    /// The renderer in this build is constructed with the `App`, so the observable that tells a
    /// live device from a not-yet-initialised one is its extent.
    fn device_is_initialized(&self) -> bool {
        let (w, h) = self.present.size();
        w > 0 && h > 0
    }

    /// Follow gameplay-screen construction and destruction, the only two screen-lifetime callers of
    /// [`Self::force_display_resolution`]. Construction disables the forced
    /// 800x600 login size. Destruction restores it unless the application is already quitting.
    ///
    /// The gameplay object exists exactly while the current mode is gameplay, so observing the
    /// mode-switch edge reproduces its constructor and destructor. `Pump::is_done` supplies the
    /// quitting predicate; a closing window is never resized.
    /// **`Display.FullScreen` applies on entering the game, and only there.** This is a
    /// declared divergence (client divergence CD-005) with no retail counterpart, because retail's full screen is a D3D9
    /// device mode that exists for the life of the process.
    ///
    /// Two separate things are wrong with applying it at start-up. Creating the window
    /// borderless and moving it to the monitor rectangle **is not a fullscreen request** and is
    /// not portable: a window cannot position itself on Wayland, and on macOS the rectangle does
    /// not cover the menu bar or the Dock, so both platforms would draw a chromeless window that
    /// does not fill the screen. And on Windows, where the rectangle does work, coming into
    /// gameplay *from* a full-screen character select goes through
    /// [`Self::follow_screen_forced_resolution`]'s forced display resolution on the same edge,
    /// which fights it.
    ///
    /// So: the pre-game flow is windowed, the world is whatever the preference says, and the
    /// transition is the mode edge. This runs on the same `screen_changed` edge as the forced
    /// resolution and for the same reason -- the flow has just swapped the screen, so
    /// this *is* the gameplay-screen construction/destruction edge. It writes only the shadow;
    /// [`Self::apply_changed_display_presentation`] notices and
    /// [`Self::change_presentation`] does the work, exactly as it does for Alt+Enter and for the
    /// options page.
    ///
    /// Alt+Enter outside gameplay is refused rather than fought: this function would undo it on
    /// the next mode edge, and a toggle that silently reverts is worse than one that does
    /// nothing. The event loop (`do_event_loop`) is where that refusal lives.
    pub fn follow_gameplay_full_screen(&mut self, shell: &mut S) {
        self.duties.gameplay_followed = Some(shell.in_gameplay());
        if self.pump.state.is_done {
            return;
        }
        self.pump.state.full_screen = shell.in_gameplay() && self.cfg.display.full_screen;
    }

    /// The forced pre-game display size on the game screen's construction and destruction edges.
    /// See [`Self::follow_gameplay_full_screen`]; a front end that does not call the pair on its own
    /// screen edges has them called at the foot of its UI step whenever
    /// [`Shell::in_gameplay`] changes.
    pub fn follow_screen_forced_resolution(&mut self, shell: &mut S) {
        self.duties.gameplay_followed = Some(shell.in_gameplay());
        if self.pump.state.is_done {
            return;
        }
        let gameplay = shell.in_gameplay();
        // `self.forced_resolution` rather than the literal `800, 600` retail pushes: see
        // [`Self::unforced_resolution`]. For every client whose presentation came from the
        // preference the two are the same number, which is the only case the force acts in.
        let (w, h) = self.forced_resolution;
        self.force_display_resolution(shell, !gameplay, w, h);
    }
}
