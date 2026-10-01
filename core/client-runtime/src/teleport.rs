//! The teleport / portal animation, wired to the running client.
//!
//! The animation itself lives in
//! [`dereth_client_contract::teleport`]; it is a pure function of elapsed time and four observable
//! facts. This module owns the two things the model cannot know:
//!
//! 1. **The three `WorldObjects` flags** — `waiting_for_teleport`, `position_update_complete` and
//!    `has_been_teleported` — which between them produce the single edge that starts both the
//!    login tunnel and the portal tunnel.
//! 2. **Where the answers go**: the hide-world flag becomes "do not draw the world this frame", and
//!    the smart box's override FOV distance becomes
//!    `dereth_render::camera::view_distance_override`.
//!
//! # The flags, and the one place this deviates
//!
//! Player-teleport handling (`0xF751`) sets `waiting_for_teleport = 1` and clears
//! `position_update_complete`; smart-box simulation sets `position_update_complete = 1`
//! (and `has_been_teleported = 1`) on the first frame after `waiting_for_teleport` has cleared and
//! the cell manager is no longer blocking.
//!
//! What clears `waiting_for_teleport` in the client is the player's next **state** update arriving
//! with physics-state bit `0x4000` clear. This build's object stream does
//! not carry that physics state bit, so the equivalent used here is the player's next
//! `0xF748 Movement_PositionEvent` — the message that in practice carries the post-teleport
//! position and always accompanies the state update. **This is the one modelled substitution in
//! this module**, and the first thing to replace once the stream carries the state bit.
//!
//! # The portal-space line
//!
//! The teleport animation's rotation block raises one
//! chat notice every time the tunnel camera re-aims, **and on the tunnel's entry frame**, because
//! starting the animation (`TeleportAnim::begin`) zeroes the rotation leg's start time,
//! duration, start angle and end angle, and the block's test is `start + duration <= now`, so it
//! re-rolls on the entry frame as well as on every re-aim. In order, the block:
//!
//! * draws the leg's duration from `rand_double(0.6, 1.8)`, then
//!   its end angle from `rand_double(0.0, 360.0)` -- two draws, in that order;
//! * builds one `StringInfo` around the fixed wide literal
//!   `L"In Portal Space - Please Wait..."`;
//! * sends it as a display-string notice with chat type `0x1A`.
//!
//! So the "random portal-space message" is **one fixed wide literal**: the only constant
//! the block uses is that string, and nothing between the two `rand_double`s and the notice does
//! more than build the `StringInfo` and string and release a reference. There
//! is no third draw, no integer random draw, and no string-table lookup -- which is
//! why neither the dats nor ACE carry the text. The fixed literal contains
//! 32 UTF-16 units plus its terminating NUL.
//!
//! Both random draws come from the **CRT `rand()`** through `dereth_client_contract::teleport::rand_double` (the
//! `msvcr70` LCG, [`dereth_primitives::num::rng::CrtRand`]), in the order duration then angle; Turbine's
//! own `ran2` generator is untouched by this function. Ending the teleport animation does *not*
//! touch the four rotation doubles (it stores state 4 and the transition start time and drops the
//! FOV override), so the `TeleportAnimState::Tunnel -> TeleportAnimState::TunnelContinue` edge
//! raises no extra line.
//!
//! The communication system receives the display-string notice and adds the text to
//! the scroll with arguments `(text, 0x1A, true, 0)`, represented here by
//! [`dereth_client_model::scroll::Scroll::on_display_string_info`] on channel
//! `PORTAL_SPACE_CHAT_TYPE`. Type `0x1A` is the one the message-log panel accepts (the strip
//! across the top of the viewport) and the one the main chat window's default filter drops, so
//! in retail the line floats over the tunnel and lands in the log only with the Error group on.
//! `Teleport::take_notices` is the seam; `App::teleport_use_time` drains it onto the scroll.

/// Requesting logoff sets the logoff request time to `current_time + 3.0`.
pub const LOG_OFF_DELAY_SECONDS: f64 = 3.0;
/// The additional delay when the player-killer predicate answers true.
pub const PK_LOG_OFF_DELAY_SECONDS: f64 = 20.0;

/// `L"In Portal Space - Please Wait..."` -- the native wide literal sent on chat
/// channel `0x1A`.
/// One fixed string, not a table row and not a random pick; see the module header.
pub const PORTAL_SPACE_MESSAGE: &str = "In Portal Space - Please Wait...";

/// The chat type the portal-space line is sent with. It is
/// [`dereth_client_model::scroll::LOCAL_ERROR_TYPE`], the spew box's only accepted type.
pub const PORTAL_SPACE_CHAT_TYPE: u32 = dereth_client_model::scroll::LOCAL_ERROR_TYPE;

use dereth_client_contract::teleport::{TeleportAnim, TeleportEffect, TeleportInputs};
use dereth_client_net::client_session::{SessionEvent, SessionState};
use dereth_primitives::ObjectId;
use dereth_protocol::Opcode;

pub use dereth_client_contract::teleport::TeleportAnimState;

/// `WorldObjects`'s teleport bookkeeping plus the animation it drives.
#[derive(Debug)]
pub struct Teleport {
    /// The current player; teleport-in-progress is false while this is null.
    player: Option<ObjectId>,
    /// Whether the smart box is waiting for teleport.
    waiting_for_teleport: bool,
    /// Whether the smart box has completed its position update.
    position_update_complete: bool,
    /// The teleport latch, read and cleared by `teleport_occured`.
    has_been_teleported: bool,
    /// The logoff latch, armed by the logoff request and cleared when the logoff animation
    /// starts.
    log_off_requested: bool,
    /// The current time plus 3.0 at the request, and the world
    /// fade does not begin until it has passed. The logoff request adds a further **20 seconds** when
    /// the player object's PK-status query answers true.
    log_off_request_time: f64,
    /// When the animation's tunnel began, so the stand-in animation frame can be derived from
    /// elapsed time. Goes away with the real physics body.
    tunnel_started: Option<f64>,
    /// The animation.
    pub anim: TeleportAnim,
    /// How many times the tunnel has been entered. Asserted on rather than logged: a login that
    /// plays no tunnel is the defect this counter guards against.
    pub tunnels_played: u64,
    /// Number of requested login-complete notifications.
    pub login_completes: u64,
    /// How many login-complete notifications (`0x00A1`) the session actually sent.
    pub login_completes_sent: u64,
    /// Whether the login-complete notification is owed and has not
    /// gone out. The client sets this whenever sending the login-complete notification cannot
    /// complete and
    /// retries every frame, so it is a latch and not an edge.
    pub login_complete_pending: bool,
    /// The UI sound types the animation asked for, in order, for the host to play.
    pub pending_sounds: Vec<u32>,
    /// The teleport object's current animation frame as the **real**
    /// `CreatureMode` preview space reports it, pushed in by the host once a frame.
    ///
    /// `None` falls back to [`dereth_client_contract::teleport::modelled_anim_frame`], the
    /// modelled stand-in: a host with no preview space (a headless model test, or a
    /// frame before the space has built its object) still gets a plausible frame rather than a
    /// stuck 0, which would leave the tunnel unable to exit anywhere but its five-second cap.
    pub portal_anim_frame: Option<u32>,
    /// The queued `(channel, text)` notices the animation
    /// made since the host last took them: `(0x1A, "In Portal Space - Please Wait...")` once per
    /// rotation leg, the entry frame included. The host passes them to the world's scroll receiver.
    pending_notices: Vec<(u32, String)>,
    /// How many portal-space lines this `Teleport` has raised, ever. A counter and not a length,
    /// for the reason `Scroll::added` is one: the queue is drained.
    pub portal_messages: u64,
}

impl Default for Teleport {
    fn default() -> Self {
        Self::new()
    }
}

impl Teleport {
    #[must_use]
    pub fn new() -> Self {
        Self {
            player: None,
            waiting_for_teleport: false,
            position_update_complete: false,
            has_been_teleported: false,
            log_off_requested: false,
            log_off_request_time: 0.0,
            tunnel_started: None,
            // The client inherits whatever the process-wide CRT stream has reached; this build has
            // no global stream, so the seed is fixed and the camera spin is reproducible.
            anim: TeleportAnim::new(1),
            tunnels_played: 0,
            login_completes: 0,
            login_completes_sent: 0,
            login_complete_pending: false,
            pending_sounds: Vec::new(),
            portal_anim_frame: None,
            pending_notices: Vec::new(),
            portal_messages: 0,
        }
    }

    /// Whether the smart box has a teleport in progress.
    #[must_use]
    pub const fn teleport_in_progress(&self) -> bool {
        self.player.is_some() && !self.position_update_complete
    }

    /// World drawing returns immediately when this hidden flag is set.
    #[must_use]
    pub const fn world_hidden(&self) -> bool {
        self.anim.world_hidden
    }

    /// View-distance field of view while its override is enabled.
    #[must_use]
    pub const fn view_distance(&self) -> Option<f32> {
        self.anim.view_distance
    }

    /// A logoff request arms the world fade three seconds out, plus
    /// twenty seconds when the local predicate is true.
    pub fn request_log_off(&mut self, now: f64, is_player_killer: bool) {
        self.log_off_requested = true;
        self.log_off_request_time = now
            + LOG_OFF_DELAY_SECONDS
            + if is_player_killer {
                PK_LOG_OFF_DELAY_SECONDS
            } else {
                0.0
            };
    }

    /// Expose the logoff-request latch so a test can see that the client asked.
    ///
    /// True from the logoff request until the smart-box animation update marks the logoff
    /// started — i.e. for the three seconds between the request and the
    /// world beginning to fold away, or twenty-three seconds for a player killer. It is the one
    /// client-side fact that says the departure is under way and the screen has *not* been queued.
    #[must_use]
    pub const fn log_off_pending(&self) -> bool {
        self.log_off_requested
    }

    /// The client's `WorldObjects` constructor state: every flag zero, no player.
    ///
    /// Called at character-session end, which is what `ObjectStream::reset` does with the object
    /// tables.
    pub fn reset(&mut self) {
        let seed = self.tunnels_played;
        *self = Self::new();
        // Do not restart the camera spin from the same seed every session.
        #[allow(clippy::cast_possible_truncation)]
        // LINT-OK: a session counter, wrapped on purpose.
        {
            self.anim = TeleportAnim::new(1 + seed as u32);
        }
    }

    /// The events `WorldObjects` acts on. Order within the batch is the arrival order.
    pub fn apply_events(&mut self, events: &[SessionEvent]) {
        for e in events {
            match e {
                // Player creation starts the login tunnel. `position_update_complete` is 0 from
                // the constructor, so `teleport_in_progress()` is true the instant the player
                // exists — which is the login tunnel, and is not a special case.
                SessionEvent::PlayerCreated(id) => {
                    self.player = Some(*id);
                }
                SessionEvent::WorldObject { opcode, body } => self.world_view(*opcode, body),
                // **`WorldReset`** clears the player pointer, all three
                // teleport booleans and the animation scalars, so the
                // animation's state is reset on the **enter-world** edge, whatever the last
                // session's ending was. The three below are endings and are kept; this one is what
                // makes the login tunnel start clean after an ending that raised none of them.
                SessionEvent::WorldReset
                | SessionEvent::StateChanged(
                    SessionState::CharacterSelect | SessionState::Disconnected(_),
                )
                | SessionEvent::LoggedOff => self.reset(),
                _ => {}
            }
        }
    }

    fn world_view(&mut self, op: Opcode, body: &[u8]) {
        match op {
            // The client also gates on the message's
            // own sequence being no older than the player's `update_times[4]`; the session's
            // dispatcher has already applied that gate for every other smart-box message and
            // `0xF751` carries only a `u16`, so what reaches here is the accepted one.
            Opcode::EFFECTS_PLAYER_TELEPORT => {
                if self.player.is_some() {
                    self.position_update_complete = false;
                    self.has_been_teleported = false;
                    self.waiting_for_teleport = true;
                }
            }
            // The modelled stand-in for `state & 0x4000` clearing — see the module header.
            Opcode::MOVEMENT_POSITION_EVENT
                if self.player.is_some() && leading_object_id(body) == self.player =>
            {
                self.waiting_for_teleport = false;
            }
            _ => {}
        }
    }

    /// Advance the teleport state after [`Self::anim_use_time`].
    ///
    /// The order is load-bearing: teleport animation runs inside the UI-element update,
    /// step 7 of the frame, before smart-box simulation at step 8.
    /// So on the frame the player object appears, the animation sees
    /// `position_update_complete == 0` and starts the tunnel, and only *then* does the smart box
    /// mark the position settled. Run the other way round, a login whose landblocks were already
    /// resident would play no tunnel at all — which is the shape of the reported defect.
    ///
    /// `blocking_for_cells` means the world is frozen while the
    /// landblocks page in. Here it is "the player body is not standing in a loaded scene yet",
    /// which is the same condition and is what makes the login tunnel last as long as the load
    /// does.
    pub fn step_world_view(&mut self, blocking_for_cells: bool) {
        if !blocking_for_cells
            && self.player.is_some()
            && !self.waiting_for_teleport
            && !self.position_update_complete
        {
            self.position_update_complete = true;
            self.has_been_teleported = true;
        }
    }

    /// Both halves in the client's own order, for a caller that has no frame to hang them on.
    pub fn use_time(&mut self, now: f64, blocking_for_cells: bool, game_view_distance: f32) {
        self.anim_use_time(now, game_view_distance);
        self.step_world_view(blocking_for_cells);
    }

    /// Gameplay smart-box simulation — frame step 7.
    ///
    /// `game_view_distance` is the FOV-derived distance with the override off; see
    /// `dereth_render::camera::view_distance_from_fov`.
    pub fn anim_use_time(&mut self, now: f64, game_view_distance: f32) {
        let was_tunnel = self.anim.state.is_tunnel();
        let inputs = TeleportInputs {
            world_teleport_in_progress: self.teleport_in_progress(),
            log_off_due: self.log_off_requested && self.log_off_request_time < now,
            // Reading the teleport-occurrence latch also clears it; the frame tick does this only in
            // `TAS_OFF`, which is exactly when this matters.
            teleport_occured: self.anim.state == TeleportAnimState::Off
                && std::mem::take(&mut self.has_been_teleported),
            game_view_distance,
            // The real sequence counter when the preview space has one, and only
            // otherwise the stand-in.
            portal_anim_frame: self.portal_anim_frame.unwrap_or_else(|| {
                self.tunnel_started.map_or(0, |t0| {
                    dereth_client_contract::teleport::modelled_anim_frame(now - t0)
                })
            }),
        };
        self.anim.use_time(now, &inputs);

        for fx in self.anim.take_effects() {
            match fx {
                TeleportEffect::PlayEnterPortalSound => {
                    self.pending_sounds
                        .push(dereth_audio::trigger::SOUND_UI_ENTER_PORTAL);
                }
                TeleportEffect::PlayExitPortalSound => {
                    self.pending_sounds
                        .push(dereth_audio::trigger::SOUND_UI_EXIT_PORTAL);
                }
                TeleportEffect::StartPortalAnimation => self.tunnel_started = Some(now),
                TeleportEffect::ClearPortalAnimation => self.tunnel_started = None,
                TeleportEffect::SendLoginComplete => {
                    self.login_completes += 1;
                    self.login_complete_pending = true;
                }
                TeleportEffect::ConsumeTeleportOccurred => self.has_been_teleported = false,
                TeleportEffect::SetLogOffStarted => self.log_off_requested = false,
                // One fixed literal, sent as a display-string notice on
                // chat type `0x1A`, raised on
                // every rotation leg. It is a literal built into the client, not a table row.
                TeleportEffect::ShowPortalMessage => {
                    self.portal_messages += 1;
                    self.pending_notices
                        .push((PORTAL_SPACE_CHAT_TYPE, PORTAL_SPACE_MESSAGE.to_owned()));
                }
            }
        }
        if !was_tunnel && self.anim.state.is_tunnel() {
            self.tunnels_played += 1;
        }
    }

    /// Whatever sounds the animation asked for since the last call.
    pub fn take_sounds(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.pending_sounds)
    }

    /// The queued `(channel, text)` notices the
    /// animation made since the last call, in order. The host puts each on
    /// the world's scroll-notice receiver, where the communication system lands them.
    pub fn take_notices(&mut self) -> Vec<(u32, String)> {
        std::mem::take(&mut self.pending_notices)
    }

    /// A login-complete notification is owed. Clear the flag once the
    /// `0x00A1` has actually gone out; leave it set and it is retried next frame, which is the
    /// client's own pending-login-complete retry loop.
    #[must_use]
    pub const fn login_complete_owed(&self) -> bool {
        self.login_complete_pending
    }

    /// The notification went out.
    pub fn login_complete_sent(&mut self) {
        self.login_complete_pending = false;
    }
}

/// Every smart-box message except `0xF746` and `0xF751` opens with the object id.
fn leading_object_id(body: &[u8]) -> Option<ObjectId> {
    let b: [u8; 4] = body.get(..4)?.try_into().ok()?;
    Some(ObjectId(u32::from_le_bytes(b)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player_created(id: u32) -> SessionEvent {
        SessionEvent::PlayerCreated(ObjectId(id))
    }

    fn teleport_msg() -> SessionEvent {
        SessionEvent::WorldObject {
            opcode: Opcode::EFFECTS_PLAYER_TELEPORT,
            body: vec![0, 0],
        }
    }

    fn position_msg(id: u32) -> SessionEvent {
        SessionEvent::WorldObject {
            opcode: Opcode::MOVEMENT_POSITION_EVENT,
            body: id.to_le_bytes().to_vec(),
        }
    }

    /// Oracle: the teleport-in-progress predicate and the constructor's zeroed
    /// `position_update_complete` in the smart-box update path.
    ///
    /// **Logging in plays the tunnel.** The player object arriving is the whole trigger; there is
    /// no login-specific call in the original world-view path observed by this unit.
    #[test]
    fn logging_in_plays_the_tunnel_and_ends_on_the_world() {
        let mut t = Teleport::new();
        assert!(!t.teleport_in_progress(), "no player yet");
        t.apply_events(&[player_created(0x5000_0001)]);
        assert!(
            t.teleport_in_progress(),
            "the player exists and its position has not settled"
        );

        // The landblocks are still loading, so the tunnel holds.
        t.use_time(0.0, true, 1.354_264_5);
        assert_eq!(t.anim.state, TeleportAnimState::Tunnel);
        assert!(t.world_hidden(), "the world is hidden behind the tunnel");
        assert_eq!(t.tunnels_played, 1);
        t.use_time(0.5, true, 1.354_264_5);
        assert_eq!(
            t.anim.state,
            TeleportAnimState::Tunnel,
            "still loading at t = 0.5 s"
        );
        assert!(t.world_hidden());

        // The scene is up. The animation ran first and still saw the tunnel, so the flag settles
        // this frame and ending the teleport animation fires on the next -- which is the frame
        // ordering that guarantees a login always plays at least one frame of tunnel.
        t.use_time(1.0, false, 1.354_264_5);
        assert!(!t.teleport_in_progress());
        assert_eq!(t.anim.state, TeleportAnimState::Tunnel);
        t.use_time(1.02, false, 1.354_264_5);
        assert_eq!(t.anim.state, TeleportAnimState::TunnelContinue);

        // Two seconds of tunnel, then the 5 s cap, then two one-second fades.
        for i in 1..=200 {
            t.use_time(1.0 + f64::from(i) * 0.05, false, 1.354_264_5);
        }
        assert_eq!(t.anim.state, TeleportAnimState::Off, "ends on the world");
        assert!(!t.world_hidden());
        assert_eq!(t.view_distance(), None, "no override left behind");
        assert!(
            t.login_completes >= 1,
            "the login-complete notification fired"
        );
        assert_eq!(t.tunnels_played, 1, "one tunnel, not one per frame");
    }

    /// Oracle: the end of the world fade-in sends the login-complete notification, then reads the
    /// smart box's teleport-occurred flag and throws the value away.
    ///
    /// **A login reports itself once.** The player's position settles inside the tunnel, which sets
    /// the teleport-occurred flag; the fade-in's end reports that teleport, so the idle state that
    /// follows has nothing left to report.
    #[test]
    fn a_login_sends_one_login_complete_and_not_a_second_from_the_idle_state() {
        let mut t = Teleport::new();
        t.apply_events(&[player_created(0x5000_0001)]);
        t.use_time(0.0, true, 1.354_264_5);
        t.use_time(1.0, false, 1.354_264_5);
        for i in 1..=400 {
            t.use_time(1.0 + f64::from(i) * 0.05, false, 1.354_264_5);
        }
        assert_eq!(t.anim.state, TeleportAnimState::Off, "ends on the world");
        assert_eq!(t.login_completes, 1, "one notification for one login");
    }

    /// Oracle: player-teleport handling and smart-box simulation's
    /// `waiting_for_teleport == 0 && position_update_complete == 0` arm.
    ///
    /// **Portalling plays the tunnel** — the second entry, through the identical edge.
    #[test]
    fn portalling_plays_the_tunnel_through_the_same_edge_as_login() {
        let mut t = Teleport::new();
        t.apply_events(&[player_created(0x5000_0001)]);
        // Settle the login first.
        for i in 0..400 {
            t.use_time(f64::from(i) * 0.05, false, 1.354_264_5);
        }
        assert_eq!(t.anim.state, TeleportAnimState::Off);
        let after_login = t.tunnels_played;

        // `0xF751` arrives at t = 20.
        t.apply_events(&[teleport_msg()]);
        assert!(
            t.teleport_in_progress(),
            "position_update_complete was cleared"
        );
        t.use_time(20.0, false, 1.354_264_5);
        assert_eq!(t.anim.state, TeleportAnimState::Tunnel);
        assert!(t.world_hidden());
        assert_eq!(t.tunnels_played, after_login + 1);

        // Still hidden half a second in, and the sound went out on entry.
        t.use_time(20.5, false, 1.354_264_5);
        assert!(
            t.world_hidden(),
            "the world is hidden at t = 0.5 s into the tunnel"
        );
        assert!(t
            .take_sounds()
            .contains(&dereth_audio::trigger::SOUND_UI_ENTER_PORTAL));

        // The server's post-teleport position arrives; the tunnel begins to unwind.
        t.apply_events(&[position_msg(0x5000_0001)]);
        t.use_time(21.0, false, 1.354_264_5);
        assert_eq!(
            t.anim.state,
            TeleportAnimState::Tunnel,
            "step 7 ran before step 8"
        );
        t.use_time(21.02, false, 1.354_264_5);
        assert_eq!(t.anim.state, TeleportAnimState::TunnelContinue);
        for i in 1..=200 {
            t.use_time(21.0 + f64::from(i) * 0.05, false, 1.354_264_5);
        }
        assert_eq!(t.anim.state, TeleportAnimState::Off, "ends on the world");
        assert!(!t.world_hidden());
        assert!(t
            .take_sounds()
            .contains(&dereth_audio::trigger::SOUND_UI_EXIT_PORTAL));
    }

    /// Oracle: a position event for *another* object must not clear
    /// `waiting_for_teleport`, or a passing NPC would end the player's tunnel.
    #[test]
    fn another_objects_position_does_not_end_the_players_teleport() {
        let mut t = Teleport::new();
        t.apply_events(&[player_created(0x5000_0001), teleport_msg()]);
        assert!(t.waiting_for_teleport);
        t.apply_events(&[position_msg(0x5000_00FF)]);
        assert!(t.waiting_for_teleport, "someone else moved");
        t.apply_events(&[position_msg(0x5000_0001)]);
        assert!(!t.waiting_for_teleport);
    }

    /// Oracle: the whole point of this unit. With the animation never started the world is never
    /// hidden and the projection is never overridden — which is what the live client did, and is
    /// the defect. This is the control for the two tests above.
    #[test]
    fn with_no_player_nothing_is_hidden_and_no_projection_is_overridden() {
        let mut t = Teleport::new();
        for i in 0..200 {
            t.use_time(f64::from(i) * 0.05, false, 1.354_264_5);
        }
        assert_eq!(t.anim.state, TeleportAnimState::Off);
        assert!(!t.world_hidden());
        assert_eq!(t.view_distance(), None);
        assert_eq!(t.tunnels_played, 0);
        assert!(t.take_sounds().is_empty());
    }

    /// Oracle: marking logoff as started clears `log_off_requested`, so the
    /// world-fade-out start (`TeleportAnimState::WorldFadeOut`) in `use_time` fires exactly once.
    #[test]
    fn a_log_off_fades_the_world_out_once_and_not_once_per_frame() {
        let mut t = Teleport::new();
        t.apply_events(&[player_created(0x5000_0001)]);
        for i in 0..400 {
            t.use_time(f64::from(i) * 0.05, false, 1.354_264_5);
        }
        let before = t.tunnels_played;
        t.request_log_off(30.0, false);
        // `logOffRequestTime = now + 3.0`: nothing happens for three seconds.
        t.use_time(30.0, false, 1.354_264_5);
        assert_eq!(
            t.anim.state,
            TeleportAnimState::Off,
            "the request is three seconds out"
        );
        t.use_time(32.9, false, 1.354_264_5);
        assert_eq!(t.anim.state, TeleportAnimState::Off);
        let t0 = 33.1;
        t.use_time(t0, false, 1.354_264_5);
        assert_eq!(t.anim.state, TeleportAnimState::WorldFadeOut);
        assert!(
            !t.world_hidden(),
            "the world is still drawn while its projection collapses"
        );
        assert!(
            t.view_distance().is_some(),
            "the FOV override is armed on the first frame"
        );
        t.use_time(t0 + 0.5, false, 1.354_264_5);
        assert_eq!(
            t.anim.state,
            TeleportAnimState::WorldFadeOut,
            "not restarted"
        );
        t.use_time(t0 + 1.0, false, 1.354_264_5);
        assert_eq!(t.anim.state, TeleportAnimState::TunnelFadeIn);
        assert_eq!(t.tunnels_played, before + 1);
    }

    /// Oracle: character-session end tearing `WorldObjects` down. A stale `player` would make the
    /// next character's login skip its tunnel.
    #[test]
    fn ending_the_session_puts_every_flag_back_to_the_constructors() {
        let mut t = Teleport::new();
        t.apply_events(&[player_created(0x5000_0001), teleport_msg()]);
        t.use_time(0.0, false, 1.354_264_5);
        assert!(t.world_hidden());
        t.apply_events(&[SessionEvent::LoggedOff]);
        assert!(!t.teleport_in_progress());
        assert!(!t.world_hidden());
        assert_eq!(t.anim.state, TeleportAnimState::Off);
        assert_eq!(t.view_distance(), None);
    }

    /// The tunnels entry frame says in portal space and every re aim says it again.
    #[test]
    fn the_tunnels_entry_frame_says_in_portal_space_and_every_re_aim_says_it_again() {
        let mut t = Teleport::new();
        t.apply_events(&[player_created(0x5000_0001)]);
        assert!(
            t.take_notices().is_empty(),
            "nothing is said before the tunnel"
        );
        // The entry frame: the tunnel start (`TAS_TUNNEL`) and the rotation block in the
        // same `UseTime`.
        t.use_time(0.0, true, 1.354_264_5);
        assert_eq!(t.anim.state, TeleportAnimState::Tunnel);
        assert_eq!(
            t.take_notices(),
            vec![(0x1A, "In Portal Space - Please Wait...".to_owned())],
            "the entry frame raises exactly one line, on channel 0x1A"
        );
        assert_eq!(t.portal_messages, 1);
        // Inside a leg nothing is said; the leg is at least 0.6 s.
        t.use_time(0.5, true, 1.354_264_5);
        assert!(
            t.take_notices().is_empty(),
            "a leg in progress says nothing"
        );
        // Ten seconds of tunnel at 30 Hz: every leg is 0.6..=1.8 s, so between 6 and 17 more.
        for i in 1..=300 {
            t.use_time(f64::from(i) / 30.0, true, 1.354_264_5);
        }
        let n = t.take_notices();
        assert!(
            (5..=17).contains(&n.len()),
            "{} re-aims in ten seconds",
            n.len()
        );
        assert!(n
            .iter()
            .all(|l| l == &(0x1A, PORTAL_SPACE_MESSAGE.to_owned())));
        assert_eq!(t.portal_messages, 1 + n.len() as u64);
        // Draining is a take: nothing is said twice.
        assert!(t.take_notices().is_empty());
    }

    /// Oracle: the project's no-frame-counting rule. The same eight seconds at 15 fps and at
    /// 144 fps must leave the client in the same state with the same projection.
    #[test]
    fn the_wiring_is_frame_rate_independent() {
        let run = |step: f64| {
            let mut t = Teleport::new();
            t.apply_events(&[player_created(0x5000_0001)]);
            let mut now = 0.0;
            while now <= 8.0 + 1e-9 {
                t.use_time(now, now < 1.0, 1.354_264_5);
                now += step;
            }
            (
                t.anim.state,
                t.world_hidden(),
                t.view_distance(),
                t.tunnels_played,
            )
        };
        let slow = run(1.0 / 15.0);
        let fast = run(1.0 / 144.0);
        assert_eq!(slow.0, fast.0);
        assert_eq!(slow.1, fast.1);
        assert_eq!(slow.3, fast.3, "one tunnel at either rate");
        match (slow.2, fast.2) {
            (None, None) => {}
            (Some(a), Some(b)) => assert!((a - b).abs() < 1e-3, "{a} vs {b}"),
            (x, y) => panic!("override disagrees: {x:?} vs {y:?}"),
        }
    }
}
