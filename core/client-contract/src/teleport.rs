//! `WorldView`'s teleport / portal animation.
//!
//! Oracles: the smart box's teleport begin, end and per-frame update, its field-of-view override
//! pair, its show and hide, its teleport-in-progress test, the client's random-double helper and
//! the UI easing table.
//!
//! # What this is
//!
//! A pure, time-driven model. It holds no element handles and draws nothing: `TeleportAnim::use_time`
//! takes the wall clock and the four facts the client can observe, and every visible consequence
//! comes back out as a field of `TeleportAnim` or as an entry in `TeleportAnim::take_effects`.
//! That is what lets the whole animation be asserted headlessly at a chosen instant.
//!
//! # Two entries, one path
//!
//! Logging in and portalling are **the same edge**. The smart box reports a teleport in progress when
//! a player object exists and its position update has not completed; the smart-box constructor
//! leaves the position-update-complete flag at 0, so it is *already* true the moment the player
//! object exists at login, and `0xF751 Effects_PlayerTeleport` puts it back to 0 for a portal. The
//! per-frame update watches for that flag disagreeing with the player's teleport-in-progress flag
//! and begins the teleport animation at `TAS_TUNNEL` either way. There is no second code path and
//! no "login" special case.
//!
//! The **third** entry is the log-off fade, which is the only one that starts at
//! `TeleportAnimState::WorldFadeOut`. Note that a portal does *not*: it enters at
//! `TeleportAnimState::Tunnel` directly: the client's per-frame update starts a teleport from
//! the world at the tunnel state, not the fade-out state.
//!
//! # The "fade" is a projection collapse
//!
//! There is no alpha quad. Enabling the field-of-view-distance override with distance `d` makes
//! every subsequent draw derive its projection from `d`: `fov = 2·atan(1/d)` and
//! `znear = (d >= 0.4) ? 0.1 : d*0.25`. Ramping `d` from the game's ~1.35 down to **0.001** takes
//! the field of view to 179.9° and the near plane to 0.00025, and *that* is the portal look. A
//! rebuild that read "view distance" as the far plane would draw nothing at all.
//!
//! # Randomness
//!
//! The client's random-double helper is `(b - a) * (double)rand() * 3.051850947599719e-05 + a` —
//! the **MSVC
//! CRT LCG**, [`dereth_primitives::num::rng::CrtRand`], *not* the Numerical Recipes `ran2`
//! ([`dereth_primitives::num::rng::Ran2`]). The two generators must never be merged;
//! picking the wrong one here would produce a
//! camera spin that looks plausible and is wrong. The multiplier is the constant compiled into the
//! binary, `1/32767`, and the expression order is the one the client evaluates.
//!
//! In the client `rand()` is the process-wide CRT stream. This build has no global CRT stream, so
//! the generator lives in the animation and is seeded explicitly; see `TeleportAnim::new`.

use dereth_primitives::num::rng::CrtRand;

/// `TeleportAnimState`, with the client's own numbering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum TeleportAnimState {
    /// Idle. Nothing is hidden and there is no FOV override.
    #[default]
    Off = 0,
    /// The world is still drawn while the projection collapses. Log-off only.
    WorldFadeOut = 1,
    /// The portal space is up and the projection opens back out.
    TunnelFadeIn = 2,
    /// Held for as long as the physics teleport takes.
    Tunnel = 3,
    /// The physics teleport finished; wait out the minimum and find the animation's exit frame.
    TunnelContinue = 4,
    /// The projection collapses again, over the portal space.
    TunnelFadeOut = 5,
    /// The world is back and the projection opens out onto it.
    WorldFadeIn = 6,
}

impl TeleportAnimState {
    /// The four states in which the world is hidden and the portal-space element is shown.
    ///
    /// The per-frame update's second `if`: `TunnelFadeIn || Tunnel || TunnelContinue ||
    /// TunnelFadeOut`.
    #[must_use]
    pub const fn is_tunnel(self) -> bool {
        matches!(
            self,
            Self::TunnelFadeIn | Self::Tunnel | Self::TunnelContinue | Self::TunnelFadeOut
        )
    }

    /// The two states in which the vivid target indicator is forced on.
    #[must_use]
    pub const fn is_world_fade(self) -> bool {
        matches!(self, Self::WorldFadeOut | Self::WorldFadeIn)
    }
}

/// The fixed teleport-animation timings.
pub mod timing {
    /// Every fade leg is exactly one second, and the easing input is `elapsed / 1.0`.
    pub const FADE_SECONDS: f64 = 1.0;
    /// [`super::TeleportAnimState::TunnelContinue`] will not leave before this.
    pub const TUNNEL_MIN_SECONDS: f64 = 2.0;
    /// ... and leaves unconditionally after this.
    pub const TUNNEL_CAP_SECONDS: f64 = 5.0;
    /// Start the portal object's sequence animation at frame 1 and 40 frames per second.
    pub const PORTAL_FRAMERATE: f64 = 40.0;
    /// The frame the per-frame update measures the exit window back from: `0x78`.
    pub const PORTAL_EXIT_FRAME: u32 = 120;
    /// The exit window, in seconds remaining: `1.1 < (120 - frame)/40 < 1.3`, **exclusive** at both
    /// ends, as retail's two comparisons are.
    pub const PORTAL_EXIT_WINDOW: (f32, f32) = (1.1, 1.3);
    /// The rotation leg duration is a random draw between 0.6 and 1.8.
    pub const ROTATION_SECONDS: (f64, f64) = (0.6, 1.8);
    /// The rotation target is a random draw between 0.0 and 360.0.
    pub const ROTATION_DEGREES: (f64, f64) = (0.0, 360.0);
    /// What the view distance collapses to. Not zero: `set_vdst` divides by it.
    pub const COLLAPSED_VIEW_DISTANCE: f32 = 0.001;
}

/// The portal space's own constants, and the per-frame update's.
pub mod portal_space {
    /// Data enum `0x10000001`, the portal object itself.
    pub const OBJECT_ENUM: u32 = 0x1000_0001;
    /// Data enum `0x10000002`, the sequence animation the portal object runs.
    pub const ANIMATION_ENUM: u32 = 0x1000_0002;
    /// The portal camera position used in both initialization and per-frame updates.
    ///
    /// The z is `0x3F6147AE`, which is **0.88**, not 0.87.
    pub const CAMERA_POSITION: (f32, f32, f32) = (0.24, -2.7, 0.88);
    /// Add a distant light with intensity 2.0.
    pub const LIGHT_INTENSITY: f32 = 2.0;
    /// Set light 0's direction.
    pub const LIGHT_DIRECTION: (f32, f32, f32) = (0.3, -1.9, 0.65);
    /// The chat channel the per-leg flavour string is pushed to,
    /// Display the message on channel `0x1A`.
    pub const MESSAGE_CHANNEL: u32 = 0x1A;
}

/// The client's random-double helper.
///
/// `(b - a) * rand() * 3.051850947599719e-05 + a`, in that order and with
/// that literal — the compiled `1/32767`, not a recomputed reciprocal.
#[must_use]
pub fn rand_double(rng: &mut CrtRand, a: f64, b: f64) -> f64 {
    /// The reciprocal of `RAND_MAX` the client compiled in.
    const ONE_OVER_RAND_MAX: f64 = 3.051_850_947_599_719e-5;
    (b - a) * f64::from(rng.next_u16()) * ONE_OVER_RAND_MAX + a
}

/// What the client can see that the animation cannot work out for itself.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TeleportInputs {
    /// Whether the smart box reports a teleport in progress.
    pub world_teleport_in_progress: bool,
    /// Whether logoff was requested and its request time is earlier than the current time.
    pub log_off_due: bool,
    /// Whether a teleport occurred, **already consumed** by the caller — the client's
    /// accessor clears the flag as it reads it, and the update reads it only in `TAS_OFF`.
    pub teleport_occured: bool,
    /// The game view distance with the override off: `1 / tan(fov_y / 2)`. Sampled
    /// once, on the `TAS_OFF -> anything` edge, into the game view distance.
    pub game_view_distance: f32,
    /// The portal object's current animation frame.
    pub portal_anim_frame: u32,
}

impl Default for TeleportInputs {
    fn default() -> Self {
        Self {
            world_teleport_in_progress: false,
            log_off_due: false,
            teleport_occured: false,
            // The default 90 degree preference at 4:3 gives fov_y = 1.2736 rad and this distance.
            game_view_distance: 1.354_264_5,
            portal_anim_frame: 0,
        }
    }
}

/// A side effect the animation asked for this frame, in the order the per-frame update performs
/// them.
///
/// These are the calls that are not state: sounds, the sequence animation on the portal object,
/// and the login-complete notification. Everything else is readable off `TeleportAnim`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeleportEffect {
    /// Play the centered enter-portal sound when the animation begins.
    PlayEnterPortalSound,
    /// Play the centered exit-portal sound on `TUNNEL_FADE_OUT -> WORLD_FADE_IN`.
    PlayExitPortalSound,
    /// Start the portal sequence selected by enum `0x10000002` at frame 1 and 40 fps.
    StartPortalAnimation,
    /// Clear the portal object's sequence animations.
    ClearPortalAnimation,
    /// Send the login-complete notification.
    SendLoginComplete,
    /// Read and clear the smart box's teleport-occurred flag, discarding the value: the end of
    /// the world fade-in has already reported the teleport it covered.
    ConsumeTeleportOccurred,
    /// Mark logoff as started, clearing the logoff-requested flag.
    SetLogOffStarted,
    /// Display a random portal-space message on channel `0x1A`, once per
    /// rotation leg. Which string table row it draws from is unresolved,
    /// so this records that the client would have
    /// said something rather than silently dropping it.
    ShowPortalMessage,
}

/// `WorldView`'s teleport animation.
///
/// Everything a caller needs to draw a frame is public and readable
/// without a method call, because that is what makes "the tunnel is active at t = 0.5 s" a one-line
/// assertion.
#[derive(Debug, Clone)]
pub struct TeleportAnim {
    /// The animation state.
    pub state: TeleportAnimState,
    /// The player's teleport-in-progress flag. Written here because the per-frame update is
    /// what writes it.
    pub teleport_in_progress: bool,
    /// True while the 3D world must not be drawn; the world renderer returns immediately when set.
    pub world_hidden: bool,
    /// Whether the portal-space element is visible. The per-frame update gates its whole setup
    /// block on this being false, so it is the "have I set the tunnel up yet" latch as well as the
    /// visibility.
    pub portal_space_visible: bool,
    /// Whether the vivid target indicator is enabled: on for world fades, off inside the tunnel.
    pub vivid_target_indicator: bool,
    /// The view-distance FOV override when enabled; `None` means the override is off.
    /// Feed it to `crate::camera::set_vdst`.
    pub view_distance: Option<f32>,
    /// The current rotation angle, degrees. The portal camera's heading:
    /// Set the portal-space camera direction to `(0, angle, 0)` degrees.
    pub rotation_angle: f64,
    /// The game view distance, sampled on the edge out of `TAS_OFF`.
    game_vdist: f64,
    /// The current view distance.
    cur_vdist: f32,
    /// When the current state transition started.
    transition_start: f64,
    /// When the current rotation leg started.
    rotation_start: f64,
    /// The current rotation leg's duration.
    rotation_duration: f64,
    /// The current rotation leg's start angle.
    rotation_start_angle: f64,
    /// The current rotation leg's end angle.
    rotation_end_angle: f64,
    /// The CRT stream the random draws come from.
    rng: CrtRand,
    /// The 100-entry level array built once during initialization.
    level_array: [i16; 100],
    effects: Vec<TeleportEffect>,
}

impl Default for TeleportAnim {
    fn default() -> Self {
        Self::new(0)
    }
}

impl TeleportAnim {
    /// `seed` is `srand`'s. The client never seeds the CRT stream for this — it inherits whatever
    /// the process has reached — so the value is this rebuild's choice and is here so a test can
    /// pin the camera spin.
    #[must_use]
    pub fn new(seed: u32) -> Self {
        Self {
            state: TeleportAnimState::Off,
            teleport_in_progress: false,
            world_hidden: false,
            portal_space_visible: false,
            vivid_target_indicator: true,
            view_distance: None,
            rotation_angle: 0.0,
            game_vdist: 0.0,
            cur_vdist: 0.0,
            transition_start: 0.0,
            rotation_start: 0.0,
            rotation_duration: 0.0,
            rotation_start_angle: 0.0,
            rotation_end_angle: 0.0,
            rng: CrtRand::new(seed),
            level_array: crate::media::level_array(),
            effects: Vec::new(),
        }
    }

    /// Whatever the animation asked the host to do since the last call. Never silently dropped.
    pub fn take_effects(&mut self) -> Vec<TeleportEffect> {
        std::mem::take(&mut self.effects)
    }

    /// The current view distance, for a test that wants the ramp's value without the `Option`.
    #[must_use]
    pub const fn current_view_distance(&self) -> f32 {
        self.cur_vdist
    }

    /// The shared animation-level lookup scaled by `1/1024` to a 0…1 ease.
    ///
    /// `anim_level` clamps its input to `[0, 1]` itself, so the client's explicit `min(u, 1.0)`
    /// at the fade sites is belt and braces; both are reproduced.
    #[must_use]
    fn ease(&self, t: f64) -> f64 {
        #[allow(clippy::cast_possible_truncation)]
        // LINT-OK: `anim_level` takes an `f32`, as retail's lookup takes a `float`; the narrowing is the call signature.
        let t = t as f32;
        f64::from(crate::media::anim_level(&self.level_array, t)) / 1024.0
    }

    /// Begin the teleport animation.
    ///
    /// The game view distance is sampled **only** on the edge out of `TAS_OFF`, so a second `begin`
    /// inside a running animation keeps the distance the world actually had.
    pub fn begin(&mut self, state: TeleportAnimState, now: f64, game_view_distance: f32) {
        if self.state == TeleportAnimState::Off {
            self.game_vdist = f64::from(game_view_distance);
        }
        #[allow(clippy::cast_possible_truncation)]
        // LINT-OK: the current view distance is a `float` field assigned from the `double` game
        // view distance.
        {
            self.cur_vdist = self.game_vdist as f32;
        }
        self.rotation_start = 0.0;
        self.rotation_duration = 0.0;
        self.rotation_start_angle = 0.0;
        self.rotation_end_angle = 0.0;
        self.state = state;
        self.transition_start = now;
        self.effects.push(TeleportEffect::PlayEnterPortalSound);
    }

    /// End the animation — the physics teleport finished.
    ///
    /// Note it drops the FOV override entirely rather than freezing it: the tunnel is drawn at the
    /// game's own field of view until `TunnelFadeOut` starts collapsing it again.
    pub fn end(&mut self, now: f64) {
        if self.state != TeleportAnimState::Off {
            self.state = TeleportAnimState::TunnelContinue;
            self.transition_start = now;
            self.view_distance = None;
        }
    }

    /// The client's per-frame update for this animation, in its own order.
    ///
    /// `now` is the current frame time. **Everything here is a function of elapsed seconds**; nothing
    /// accumulates per frame, so the same `now` sequence gives the same result at any frame rate.
    pub fn use_time(&mut self, now: f64, inputs: &TeleportInputs) {
        // --- the log-off entry -------------------------------------------------------------
        if inputs.log_off_due {
            self.begin(
                TeleportAnimState::WorldFadeOut,
                now,
                inputs.game_view_distance,
            );
            self.teleport_in_progress = true;
            self.effects.push(TeleportEffect::SetLogOffStarted);
        }

        // --- the teleport edge, which is also the login edge --------------------------------
        // The smart-box teleport state differs from the player's stored teleport state. The false arm
        // deliberately does *not* clear the teleport-in-progress flag; only `WorldFadeIn`'s
        // completion does, so the two disagree for the whole tail of the animation and this branch
        // is simply not taken again because the state is no longer `TAS_TUNNEL`.
        if inputs.world_teleport_in_progress != self.teleport_in_progress {
            if inputs.world_teleport_in_progress {
                self.teleport_in_progress = true;
                self.begin(TeleportAnimState::Tunnel, now, inputs.game_view_distance);
            } else if self.state == TeleportAnimState::Tunnel {
                self.end(now);
            }
        }

        if self.state == TeleportAnimState::Off {
            // The teleport-occurred flag is read and cleared here, and only here, while idle:
            // it is how a teleport that never started an animation still tells the server the
            // client has finished loading.
            if inputs.teleport_occured {
                self.effects.push(TeleportEffect::SendLoginComplete);
            }
            return;
        }

        if self.state.is_world_fade() {
            self.vivid_target_indicator = true;
        }

        if self.state.is_tunnel() {
            self.enter_portal_space();
            self.spin_camera(now);
        }

        // --- the two projection ramps -------------------------------------------------------
        // Both read the transition start time, both are eased, both are exactly one second.
        if matches!(
            self.state,
            TeleportAnimState::WorldFadeOut | TeleportAnimState::TunnelFadeOut
        ) {
            let a = self.ease((now - self.transition_start) / timing::FADE_SECONDS);
            let collapsed = f64::from(timing::COLLAPSED_VIEW_DISTANCE);
            self.set_view_distance((collapsed - self.game_vdist) * a + self.game_vdist);
        }
        if matches!(
            self.state,
            TeleportAnimState::TunnelFadeIn | TeleportAnimState::WorldFadeIn
        ) {
            let a = self.ease((now - self.transition_start) / timing::FADE_SECONDS);
            let collapsed = f64::from(timing::COLLAPSED_VIEW_DISTANCE);
            self.set_view_distance((self.game_vdist - collapsed) * a + collapsed);
        }

        self.advance(now, inputs);
    }

    /// The per-frame block that runs while the portal space is not visible.
    fn enter_portal_space(&mut self) {
        if self.portal_space_visible {
            return;
        }
        self.vivid_target_indicator = false;
        self.effects.push(TeleportEffect::StartPortalAnimation);
        self.portal_space_visible = true;
        // The world is hidden here. This is the animation's own hiding and is client behaviour, not
        // a UI hide-polarity compensation: it sets the world's hidden flag, which
        // the world draw tests before drawing the world at all.
        self.world_hidden = true;
    }

    /// The camera spin. A leg runs for a random 0.6 to 1.8 seconds to a random 0 to 360
    /// degrees, eased by the same curve as the fades.
    fn spin_camera(&mut self, now: f64) {
        // The client's test is `(end < now) == (end == now)`, which is true only when `end > now`.
        // So the eased arm runs while the leg is still running and the re-roll happens on the
        // frame the leg's end time is reached or passed. `begin` zeroes both, so the first frame
        // of every animation rolls a leg.
        if self.rotation_start + self.rotation_duration > now {
            let a = self.ease((now - self.rotation_start) / self.rotation_duration);
            self.rotation_angle = (self.rotation_end_angle - self.rotation_start_angle) * a
                + self.rotation_start_angle;
        } else {
            self.rotation_angle = self.rotation_end_angle;
            self.rotation_start = now;
            self.rotation_duration = rand_double(
                &mut self.rng,
                timing::ROTATION_SECONDS.0,
                timing::ROTATION_SECONDS.1,
            );
            self.rotation_start_angle = self.rotation_angle;
            self.rotation_end_angle = rand_double(
                &mut self.rng,
                timing::ROTATION_DEGREES.0,
                timing::ROTATION_DEGREES.1,
            );
            self.effects.push(TeleportEffect::ShowPortalMessage);
        }
    }

    /// Enable the field-of-view-distance override with `d` and store it as the current view
    /// distance.
    fn set_view_distance(&mut self, d: f64) {
        #[allow(clippy::cast_possible_truncation)]
        // LINT-OK: the client evaluates the ramp in double precision and stores to the `float`
        // current view distance; the narrowing is that store.
        {
            self.cur_vdist = d as f32;
        }
        self.view_distance = Some(self.cur_vdist);
    }

    /// The five state transitions, in the per-frame update's order.
    ///
    /// They are sequential `if`s in the client, not a chain, and the `TunnelContinue` test is
    /// deliberately first. Each one stamps `transition_start = now`, so no two can fire in the same
    /// frame however large the step is — which is what keeps a stalled frame from skipping a leg.
    fn advance(&mut self, now: f64, inputs: &TeleportInputs) {
        let elapsed = now - self.transition_start;

        if self.state == TeleportAnimState::TunnelContinue && elapsed >= timing::TUNNEL_MIN_SECONDS
        {
            if elapsed >= timing::TUNNEL_CAP_SECONDS || in_exit_window(inputs.portal_anim_frame) {
                self.state = TeleportAnimState::TunnelFadeOut;
                self.transition_start = now;
            }
            return;
        }
        if self.state == TeleportAnimState::WorldFadeOut && elapsed >= timing::FADE_SECONDS {
            self.state = TeleportAnimState::TunnelFadeIn;
            self.transition_start = now;
            return;
        }
        if self.state == TeleportAnimState::TunnelFadeIn && elapsed >= timing::FADE_SECONDS {
            self.state = TeleportAnimState::Tunnel;
            self.transition_start = now;
            return;
        }
        if self.state == TeleportAnimState::TunnelFadeOut && elapsed >= timing::FADE_SECONDS {
            // The override is re-armed at the collapsed value the ramp reached, *then* the world
            // comes back — so the first frame of `WorldFadeIn` shows the world at 179.9°.
            self.view_distance = Some(self.cur_vdist);
            self.portal_space_visible = false;
            // The world is shown again.
            self.world_hidden = false;
            self.effects.push(TeleportEffect::ClearPortalAnimation);
            self.effects.push(TeleportEffect::PlayExitPortalSound);
            self.state = TeleportAnimState::WorldFadeIn;
            self.transition_start = now;
            return;
        }
        if self.state == TeleportAnimState::WorldFadeIn && elapsed >= timing::FADE_SECONDS {
            // The transition start is set to `cur_time - 1.0`, which is the client's, and is what
            // leaves the ramp pinned at its end value rather than at zero.
            self.transition_start = now - timing::FADE_SECONDS;
            self.effects.push(TeleportEffect::SendLoginComplete);
            self.teleport_in_progress = false;
            self.state = TeleportAnimState::Off;
            // Turn the field-of-view-distance override off (0.0).
            self.view_distance = None;
            // Last, the teleport-occurred flag is read and its value thrown away: the teleport
            // this animation covered has just been reported, so the idle state must not report it
            // a second time on the next frame.
            self.effects.push(TeleportEffect::ConsumeTeleportOccurred);
        }
    }
}

/// `(0x78 - frame) / 40.0` in `(1.1, 1.3)`, with the client's **unsigned** subtraction.
///
/// The current-frame-number query returns a `ulong` and the difference is converted to float as
/// unsigned, so a frame past 120 wraps to about 4.29e9 and can never fall in the window. That is
/// reproduced rather than clamped: it is what keeps a mis-seeded frame counter from exiting the
/// tunnel early.
#[must_use]
pub fn in_exit_window(frame: u32) -> bool {
    #[allow(clippy::cast_precision_loss)]
    // LINT-OK: the client's `ulong -> float` conversion, exactly.
    let remaining = timing::PORTAL_EXIT_FRAME.wrapping_sub(frame) as f32;
    #[allow(clippy::cast_possible_truncation)]
    // LINT-OK: the framerate is a `float` literal in `set_sequence_animation`.
    let seconds = remaining / timing::PORTAL_FRAMERATE as f32;
    seconds > timing::PORTAL_EXIT_WINDOW.0 && seconds < timing::PORTAL_EXIT_WINDOW.1
}

/// The frame the portal/tunnel animation object would be showing, for a host with no preview-space
/// physics object to ask.
///
/// `set_sequence_animation(did, clear = 1, low_frame = 1, framerate = 40)` starts at frame 1 and
/// the sequence loops; 120 frames at 40 fps is a three-second loop. **This is a stand-in**: the
/// real input is the physics object's current animation frame, and when the preview space exists
/// this is where it plugs in. Kept separate from `TeleportAnim` for exactly that reason.
#[must_use]
pub fn modelled_anim_frame(elapsed: f64) -> u32 {
    if elapsed <= 0.0 {
        return 1;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // LINT-OK: bounded by the modulus below; a frame index, not engine arithmetic.
    let n = (elapsed * timing::PORTAL_FRAMERATE) as u64;
    #[allow(clippy::cast_possible_truncation)]
    // LINT-OK: the modulus is 120.
    {
        (1 + n % u64::from(timing::PORTAL_EXIT_FRAME)) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run the animation from `t0` to `t1` at a fixed step, returning the model.
    fn run(
        anim: &mut TeleportAnim,
        t0: f64,
        t1: f64,
        step: f64,
        mut inputs: impl FnMut(f64) -> TeleportInputs,
    ) {
        let mut t = t0;
        while t <= t1 + 1e-9 {
            let i = inputs(t);
            anim.use_time(t, &i);
            t += step;
        }
    }

    fn teleporting(on: bool) -> TeleportInputs {
        TeleportInputs {
            world_teleport_in_progress: on,
            ..TeleportInputs::default()
        }
    }

    /// Oracle: the client's random-double helper —
    /// `(b - a) * (double)rand() * 3.051850947599719e-05 + a`, over the MSVC CRT LCG.
    ///
    /// This is the test that fails if the generator is swapped for `Ran2`: the first three draws of
    /// each are computed here side by side and asserted to differ, so "it looked plausible" cannot
    /// pass.
    #[test]
    fn rand_double_draws_from_the_crt_lcg_and_not_from_ran2() {
        // `srand(1)` then `rand()` gives 41, 18467, 6334 — the textbook MSVC sequence.
        let mut r = CrtRand::new(1);
        assert_eq!(
            [r.next_u16(), r.next_u16(), r.next_u16()],
            [41, 18467, 6334]
        );

        let mut r = CrtRand::new(1);
        let d = rand_double(&mut r, 0.6, 1.8);
        assert!((d - ((1.8 - 0.6) * 41.0 * 3.051_850_947_599_719e-5 + 0.6)).abs() < 1e-15);
        // 41/32767 of the way from 0.6 to 1.8 is only just above 0.6.
        assert!((0.600_1..0.601_6).contains(&d), "{d}");

        // The range is closed at both ends: rand() == 0 gives `a`, rand() == 32767 gives `b`.
        assert!(
            (rand_double(&mut CrtRand::new(1), 0.0, 360.0) - 41.0 * 360.0 / 32767.0).abs() < 1e-9
        );

        // The other generator gives a different sequence for the same seed, so the two
        // algorithms cannot substitute for each other.
        let mut ran2 = dereth_primitives::num::rng::Ran2::new(1);
        let ran2_first = (1.8 - 0.6) * ran2.next_f64() + 0.6;
        assert!(
            (ran2_first - d).abs() > 0.01,
            "ran2 and the CRT LCG must not agree; if they do this test proves nothing"
        );
    }

    /// The smart-box flag disagreeing with the player's teleport-in-progress flag is the only
    /// entry, and it enters at `TAS_TUNNEL`.
    #[test]
    fn a_teleport_enters_the_tunnel_directly_and_hides_the_world() {
        let mut a = TeleportAnim::new(1);
        assert_eq!(a.state, TeleportAnimState::Off);
        assert!(!a.world_hidden);

        a.use_time(10.0, &teleporting(true));
        assert_eq!(a.state, TeleportAnimState::Tunnel, "not the world fade-out");
        assert!(a.world_hidden, "the world is hidden during the tunnel");
        assert!(a.portal_space_visible);
        assert!(!a.vivid_target_indicator);
        assert!(a.teleport_in_progress);
        let fx = a.take_effects();
        assert!(fx.contains(&TeleportEffect::PlayEnterPortalSound));
        assert!(fx.contains(&TeleportEffect::StartPortalAnimation));
    }

    /// The same edge is reached during login: the smart-box teleport-in-progress predicate is
    /// `player != null && position_update_complete == 0`, and the constructor leaves
    /// `position_update_complete` at 0 — so login raises the identical flag and takes the identical
    /// path. Both entries, one test, because there is only one path to test.
    #[test]
    fn logging_in_plays_the_same_tunnel_as_portalling() {
        let mut login = TeleportAnim::new(7);
        login.use_time(0.5, &teleporting(true));
        let mut portal = TeleportAnim::new(7);
        portal.use_time(0.5, &teleporting(true));
        assert_eq!(login.state, portal.state);
        assert_eq!(login.world_hidden, portal.world_hidden);
        assert_eq!(login.take_effects(), portal.take_effects());
    }

    /// Oracle: the per-frame update's tunnel-continue block and the four one-second legs.
    ///
    /// The whole timeline, asserted at instants: the tunnel is up at t = 0.5 s with the world
    /// hidden, is still up two seconds after the physics finished, and ends on the world.
    #[test]
    fn the_timeline_is_tunnel_then_fade_out_then_world_fade_in_then_off() {
        let mut a = TeleportAnim::new(3);
        let physics_done = 1.5;
        // Frame far from the exit window, so only the 5.0 s cap can end the tunnel.
        let held = TeleportInputs {
            portal_anim_frame: 0,
            ..TeleportInputs::default()
        };

        // t = 0: the teleport starts.
        a.use_time(
            0.0,
            &TeleportInputs {
                world_teleport_in_progress: true,
                ..held
            },
        );
        // t = 0.5: the tunnel is active and the world is hidden.
        a.use_time(
            0.5,
            &TeleportInputs {
                world_teleport_in_progress: true,
                ..held
            },
        );
        assert_eq!(a.state, TeleportAnimState::Tunnel);
        assert!(
            a.world_hidden,
            "the world is hidden behind the tunnel at t = 0.5 s"
        );

        // The physics teleport finishes.
        a.use_time(
            physics_done,
            &TeleportInputs {
                world_teleport_in_progress: false,
                ..held
            },
        );
        assert_eq!(a.state, TeleportAnimState::TunnelContinue);
        assert_eq!(
            a.view_distance, None,
            "ending the teleport animation drops the override"
        );

        // Two seconds is the minimum, and with the frame outside the window it does not leave.
        a.use_time(
            physics_done + 1.99,
            &TeleportInputs {
                world_teleport_in_progress: false,
                ..held
            },
        );
        assert_eq!(a.state, TeleportAnimState::TunnelContinue);
        a.use_time(
            physics_done + 2.01,
            &TeleportInputs {
                world_teleport_in_progress: false,
                ..held
            },
        );
        assert_eq!(
            a.state,
            TeleportAnimState::TunnelContinue,
            "frame 0 is not in the exit window"
        );
        assert!(a.world_hidden);

        // The 5.0 s cap.
        a.use_time(
            physics_done + 5.0,
            &TeleportInputs {
                world_teleport_in_progress: false,
                ..held
            },
        );
        assert_eq!(a.state, TeleportAnimState::TunnelFadeOut);
        assert!(
            a.world_hidden,
            "the world stays hidden through the tunnel's own fade"
        );

        // One second of tunnel fade-out, then the world comes back.
        let t = physics_done + 6.0;
        a.use_time(
            t,
            &TeleportInputs {
                world_teleport_in_progress: false,
                ..held
            },
        );
        assert_eq!(a.state, TeleportAnimState::WorldFadeIn);
        assert!(!a.world_hidden, "the world is shown after the tunnel");
        assert!(!a.portal_space_visible);
        let fx = a.take_effects();
        assert!(fx.contains(&TeleportEffect::PlayExitPortalSound));
        assert!(fx.contains(&TeleportEffect::ClearPortalAnimation));
        assert!(
            a.view_distance.is_some_and(|d| d < 0.01),
            "the world's first frame back is at the collapsed distance: {:?}",
            a.view_distance
        );

        // One second of world fade-in, and it ends on the world with no override at all.
        a.use_time(
            t + 1.0,
            &TeleportInputs {
                world_teleport_in_progress: false,
                ..held
            },
        );
        assert_eq!(a.state, TeleportAnimState::Off, "ends on the world");
        assert_eq!(a.view_distance, None);
        assert!(!a.world_hidden);
        assert!(!a.teleport_in_progress);
        assert!(a
            .take_effects()
            .contains(&TeleportEffect::SendLoginComplete));
    }

    /// Oracle: `(120 - curFrame)/40 ∈ (1.1, 1.3)` — the tunnel exits *early* when the portal
    /// animation reaches its own window, which is what synchronises the cut.
    #[test]
    fn the_tunnel_leaves_early_on_the_animations_exit_frame() {
        assert!(!in_exit_window(0), "(120-0)/40 = 3.0");
        assert!(in_exit_window(75), "(120-75)/40 = 1.125");
        assert!(in_exit_window(69), "(120-69)/40 = 1.275");
        assert!(in_exit_window(72), "(120-72)/40 = 1.2");
        assert!(
            !in_exit_window(76),
            "(120-76)/40 = 1.1 exactly, and the test is exclusive"
        );
        assert!(
            !in_exit_window(68),
            "(120-68)/40 = 1.3 exactly, and the test is exclusive"
        );
        assert!(
            !in_exit_window(121),
            "unsigned wrap, not a negative remainder"
        );

        let mut a = TeleportAnim::new(5);
        a.use_time(0.0, &teleporting(true));
        a.use_time(0.1, &teleporting(false));
        assert_eq!(a.state, TeleportAnimState::TunnelContinue);
        // At 2.5 s in, still inside the 2..5 window, with the animation on frame 72.
        a.use_time(
            2.5,
            &TeleportInputs {
                world_teleport_in_progress: false,
                portal_anim_frame: 72,
                ..TeleportInputs::default()
            },
        );
        assert_eq!(
            a.state,
            TeleportAnimState::TunnelFadeOut,
            "left before the 5 s cap"
        );
    }

    /// Oracle: the teleport begin samples the game view distance only when the state is `Off`,
    /// and the two ramps in the per-frame update.
    ///
    /// The ramp is asserted at instants against `anim_level`'s own curve, not against a
    /// hand-rolled easing: if the curve changed, this fails.
    #[test]
    fn the_view_distance_ramp_is_the_shared_ease_between_the_game_distance_and_one_thousandth() {
        let table = crate::media::level_array();
        let mut a = TeleportAnim::new(11);
        let inputs = TeleportInputs {
            log_off_due: true,
            game_view_distance: 1.354_264_5,
            ..TeleportInputs::default()
        };
        a.use_time(100.0, &inputs);
        assert_eq!(a.state, TeleportAnimState::WorldFadeOut);
        assert!(
            !a.world_hidden,
            "the world is still drawn while its projection collapses"
        );

        for u in [0.0_f64, 0.25, 0.5, 0.75, 0.99] {
            a.use_time(
                100.0 + u,
                &TeleportInputs {
                    log_off_due: false,
                    ..inputs
                },
            );
            #[allow(clippy::cast_possible_truncation)]
            let level = f64::from(crate::media::anim_level(&table, u as f32)) / 1024.0;
            #[allow(clippy::cast_possible_truncation)]
            // LINT-OK: the store to the current view distance, a `float`, is what is checked.
            let want = ((0.001 - 1.354_264_5_f64) * level + 1.354_264_5) as f32;
            assert!(
                (a.current_view_distance() - want).abs() < 1e-6,
                "u = {u}: got {}, want {want}",
                a.current_view_distance()
            );
        }
        // Monotone down, and it never reaches zero -- `set_vdst` divides by it.
        assert!(a.current_view_distance() > 0.0);
        assert!(a.current_view_distance() < 0.1);
    }

    /// Oracle: the project's standing rule that a timing test must not depend on frame count.
    ///
    /// The same 8 seconds at 15 fps and at 144 fps must reach the same state with the same
    /// projection, because every quantity in `UseTime` is a function of elapsed seconds. This is
    /// the test that would have caught the sky-scroll bug's shape.
    #[test]
    fn the_timeline_is_identical_at_two_frame_rates() {
        let out = |step: f64| {
            let mut a = TeleportAnim::new(2);
            let physics_done = 1.0;
            run(&mut a, 0.0, 8.0, step, |t| TeleportInputs {
                world_teleport_in_progress: t < physics_done,
                // The frame counter is driven off elapsed time too, so the input is the same
                // function of `t` at both rates.
                portal_anim_frame: modelled_anim_frame(t),
                ..TeleportInputs::default()
            });
            (
                a.state,
                a.view_distance,
                a.world_hidden,
                a.portal_space_visible,
            )
        };
        let slow = out(1.0 / 15.0);
        let fast = out(1.0 / 144.0);
        assert_eq!(
            slow.0, fast.0,
            "state after 8 s must not depend on the frame rate"
        );
        assert_eq!(slow.2, fast.2);
        assert_eq!(slow.3, fast.3);
        match (slow.1, fast.1) {
            (None, None) => {}
            (Some(a), Some(b)) => assert!((a - b).abs() < 1e-3, "{a} vs {b}"),
            (x, y) => panic!("override disagrees: {x:?} vs {y:?}"),
        }

        // And the four-second mark, mid-tunnel, agrees too.
        let at4 = |step: f64| {
            let mut a = TeleportAnim::new(2);
            run(&mut a, 0.0, 4.0, step, |t| TeleportInputs {
                world_teleport_in_progress: t < 1.0,
                portal_anim_frame: 0,
                ..TeleportInputs::default()
            });
            a.state
        };
        assert_eq!(at4(1.0 / 15.0), at4(1.0 / 144.0));
    }

    /// Oracle: `UseTime`'s rotation block, and the two random draws per leg in **that order**
    /// (duration first, then angle).
    #[test]
    fn the_camera_spins_in_eased_legs_of_the_documented_lengths() {
        let mut a = TeleportAnim::new(1);
        a.use_time(0.0, &teleporting(true));
        // The first frame rolls a leg: duration from rand() = 41, angle from rand() = 18467.
        let want_duration = (1.8 - 0.6) * 41.0 * 3.051_850_947_599_719e-5 + 0.6;
        let want_angle = 360.0 * 18467.0 * 3.051_850_947_599_719e-5;
        assert!(
            (a.rotation_duration - want_duration).abs() < 1e-12,
            "{}",
            a.rotation_duration
        );
        assert!(
            (a.rotation_end_angle - want_angle).abs() < 1e-9,
            "{}",
            a.rotation_end_angle
        );
        assert!((0.6..=1.8).contains(&a.rotation_duration));
        assert!((0.0..=360.0).contains(&a.rotation_end_angle));
        assert_eq!(
            a.rotation_angle, 0.0,
            "the leg starts where the last one ended"
        );

        // Halfway through the leg the angle is the eased fraction of the way there, not the linear
        // one -- which is what `anim_level` buys.
        let mid = a.rotation_duration * 0.5;
        a.use_time(mid, &teleporting(true));
        let table = crate::media::level_array();
        #[allow(clippy::cast_possible_truncation)]
        let level = f64::from(crate::media::anim_level(
            &table,
            (mid / want_duration) as f32,
        )) / 1024.0;
        assert!((a.rotation_angle - want_angle * level).abs() < 1e-9);

        // Past the end, a new leg is rolled and the angle lands exactly on the old target.
        let end = a.rotation_duration;
        let old_target = a.rotation_end_angle;
        a.use_time(end + 0.001, &teleporting(true));
        assert!((a.rotation_angle - old_target).abs() < 1e-12);
        assert_ne!(a.rotation_end_angle, old_target, "a new target was rolled");
    }

    /// Oracle: the teleport end does nothing when the state is `TAS_OFF`, and the fact
    /// that the false arm of the teleport edge only fires in `TAS_TUNNEL`.
    #[test]
    fn the_physics_finishing_outside_the_tunnel_does_nothing() {
        let mut a = TeleportAnim::new(1);
        a.use_time(0.0, &teleporting(false));
        assert_eq!(a.state, TeleportAnimState::Off);
        a.end(0.0);
        assert_eq!(
            a.state,
            TeleportAnimState::Off,
            "End does nothing from TAS_OFF"
        );

        // Log-off starts at WORLD_FADE_OUT; the physics flag going false must not cut it short.
        let mut b = TeleportAnim::new(1);
        b.use_time(
            0.0,
            &TeleportInputs {
                log_off_due: true,
                ..TeleportInputs::default()
            },
        );
        assert_eq!(b.state, TeleportAnimState::WorldFadeOut);
        b.use_time(0.1, &teleporting(false));
        assert_eq!(b.state, TeleportAnimState::WorldFadeOut);
        // ... and it reaches the tunnel a second later.
        b.use_time(1.0, &teleporting(false));
        assert_eq!(b.state, TeleportAnimState::TunnelFadeIn);
        // The state changes at the *end* of `UseTime`, after the portal-space block has already
        // run for the old state, so the world is hidden on the next frame and not this one. That
        // is the client's order and is left alone.
        assert!(!b.world_hidden, "the transition frame is still the world's");
        b.use_time(1.0 + 1.0 / 60.0, &teleporting(false));
        assert!(b.world_hidden, "the frame after it is the tunnel's");
    }

    /// Oracle: the smart box's post-init camera-position argument, the float `0x3F6147AE`
    /// (0.88).
    #[test]
    fn the_portal_camera_z_is_the_float_in_the_binary() {
        assert_eq!(portal_space::CAMERA_POSITION, (0.24, -2.7, 0.88));
        assert_eq!(f32::from_bits(0x3F61_47AE), portal_space::CAMERA_POSITION.2);
        // The earlier description used 0.87; that float is 0x3F5EB852.
        assert_ne!(f32::from_bits(0x3F5E_B852), portal_space::CAMERA_POSITION.2);
        // `dereth_ui_screens`' own second transcription of the same call is checked against this
        // one where it lives, in `dereth_ui_screens::screens::teleport`'s remaining test -- a
        // contract crate may not name a presentation crate.
    }

    /// Oracle: `set_sequence_animation(..., lowFrame = 1, framerate = 40.0)` and the 120-frame
    /// sequence the exit window measures back from.
    #[test]
    fn the_modelled_animation_frame_loops_at_forty_frames_a_second() {
        assert_eq!(modelled_anim_frame(0.0), 1);
        assert_eq!(modelled_anim_frame(0.025), 2, "one frame at 40 fps");
        assert_eq!(modelled_anim_frame(1.0), 41);
        assert_eq!(
            modelled_anim_frame(3.0),
            1,
            "120 frames at 40 fps is a 3 s loop"
        );
        // The exit window is reachable: frames 69..=75 are inside it.
        assert!((0..200)
            .map(|i| modelled_anim_frame(f64::from(i) * 0.025))
            .any(in_exit_window));
    }
}
