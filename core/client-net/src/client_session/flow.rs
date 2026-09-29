//! The connection / UI-mode state machine: connected → character select → DDD → in world.
//!
//! Source: `docs/networking/03-connection-state-machine.md` §4.
//!
//! # The two-step enter-world exchange
//!
//! Logging a character on is a two-phase machine and the client runs it twice:
//!
//! 1. **Phase 1** — the player picks a character. The awaiting-log-on flag is set and `0xF7C8
//!    Login_SendEnterWorldRequest` goes out on the Logon queue. Nothing else happens.
//! 2. `0xF7DF Login_EnterGame_ServerReady` arrives and sets the ready-to-enter-game flag.
//! 3. **Phase 2** — the player-system time step sees both flags set, clears the awaiting one, and
//!    runs the character log-on again. This time `0xF657 Login_SendEnterWorld` goes out with the
//!    character id and the account name, the ready flag is cleared and the log-on request time
//!    is stamped.
//!
//! The exchange exists so the server can refuse before any world state is built; keep both
//! messages. Sending `0xF657` without first getting `0xF7DF` is ignored by ACE.
//!
//! # The two timeouts
//!
//! The player system's per-frame step fires `ServerDied` if no `0x0013 Login_PlayerDescription`
//! arrives within **110 seconds** of the logon request. That, and a 40-second "haven't heard from
//! the server" check, are the only liveness guards on this path.
//!
//! **`0x0013` is what makes the client "in world"** — not the transport's connected state and not
//! `0xF7DF`. [`SessionState::Playable`] flips there and nowhere else.

use dereth_primitives::{LocalTime, ObjectId};

/// Where the session is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// The transport is up; nothing has been received yet.
    Connected,
    /// `0xF658 Login_LoginCharacterSet` has arrived.
    CharacterSelect,
    /// DDD is in progress.
    Patching,
    /// `0xF7C8` has been sent; awaiting `0xF7DF` and then `0x0013`.
    EnteringWorld,
    /// `0x0013 Login_PlayerDescription` has arrived.
    Playable,
    Disconnected(DisconnectReason),
}

/// Why the session ended.
///
/// The public state is `Disconnected` with a reason, but `NetErrorCode` is the *transport's* error
/// enum and lives in the transport half of `dereth-transport`, which the session layer must not name
/// (the isolation rule in the module root). These are the reasons
/// the **session layer** can decide on its own; a transport-level disconnect reaches the session as
/// [`DisconnectReason::Transport`] carrying the code as a plain integer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisconnectReason {
    /// No `0x0013` arrived within 110 s of the logon request.
    ServerDied,
    /// `0xF659 Character_CharacterError`, carrying the character-error value.
    CharacterError(u32),
    /// `0xF7DC Login_AccountBooted`.
    AccountBooted,
    /// `0xF7C1 Login_AccountBanned`.
    AccountBanned,
    /// `0xF653 Login_ExecuteLogOff` came back from the server.
    LoggedOff,
    /// A `NetErrorCode` from the transport, as a raw discriminant.
    Transport(u32),
}

/// The player system's world-entry timeout.
pub const LOGON_TIMEOUT_SECONDS: f64 = 110.0;

/// What the flow machine wants done this tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowAction {
    /// Send `0xF7C8 Login_SendEnterWorldRequest` on the Logon queue.
    SendEnterWorldRequest,
    /// Send `0xF657 Login_SendEnterWorld` with this character and the account name.
    SendEnterWorld(ObjectId),
    /// Send `0xF653 Login_ExecuteLogOff`.
    SendLogOff(ObjectId),
    /// The exit-world disconnect.
    ///
    /// This is the client's own teardown rather than a message. Its session-visible half resets
    /// the event counter to 0; its transport half is
    /// `dereth_client_net::Net::exit_world_disconnect`, which the host performs because [`crate::client_session::Session`]
    /// reaches its transport only through the two-method `Transport` seam.
    ExitWorldDisconnect,
    /// Entering the world — the client marks itself in game, phase 2's last act but
    /// one. Also the host's, and for the same reason.
    EnterWorld,
    /// Reset the smart box with argument 1 — the world teardown.
    ///
    /// The reset takes one integer argument, and a test on that argument selects the wipe: with it
    /// clear the function stops after the cell-manager reset,
    /// with it set it also runs the cell wipe and the destruction of the queued net blobs. Phase 2
    /// of the character log-on pushes **1**, and
    /// it does so **before** the enter-world send and before the enter-world call — so the old
    /// world is gone before the client asks for the new one, which is why no object-lifetime
    /// guard is needed on the far side.
    ///
    /// It is raised here rather than on [`FlowAction::ExitWorldDisconnect`] because those are two
    /// different teardowns: `ExitWorldDisconnect` is the *transport's*, and
    /// the log-off execution raises it without touching the world at all.
    WorldReset,
    /// The state changed.
    StateChanged(SessionState),
}

/// The client-side login flow.
#[derive(Debug, Clone)]
pub struct Flow {
    state: SessionState,
    /// Whether a character log-on is awaiting the ready signal.
    awaiting_log_on: bool,
    /// Whether the client is ready to enter the game.
    ready_to_enter_game: bool,
    /// The selected character's id, as the UI flow's persistent data holds it.
    selected_character: Option<ObjectId>,
    /// `None` is the client's zero.
    log_on_request_time: Option<LocalTime>,
    /// Whether the player description has arrived.
    player_desc_received: bool,
}

impl Default for Flow {
    fn default() -> Self {
        Self::new()
    }
}

impl Flow {
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: SessionState::Connected,
            awaiting_log_on: false,
            ready_to_enter_game: false,
            selected_character: None,
            log_on_request_time: None,
            player_desc_received: false,
        }
    }

    #[must_use]
    pub fn state(&self) -> SessionState {
        self.state
    }

    #[must_use]
    pub fn selected_character(&self) -> Option<ObjectId> {
        self.selected_character
    }

    fn set_state(&mut self, s: SessionState, out: &mut Vec<FlowAction>) {
        if self.state != s {
            self.state = s;
            out.push(FlowAction::StateChanged(s));
        }
    }

    /// `0xF658 Login_LoginCharacterSet` arrived: the character-select screen can be drawn.
    ///
    /// It can arrive again mid-session — after a delete, ACE re-sends the whole list — so it does
    /// **not** unwind an enter-world in progress.
    pub fn character_set_received(&mut self, out: &mut Vec<FlowAction>) {
        if matches!(self.state, SessionState::Connected | SessionState::Patching) {
            self.set_state(SessionState::CharacterSelect, out);
        }
    }

    /// DDD started.
    pub fn patching_started(&mut self, out: &mut Vec<FlowAction>) {
        if self.state == SessionState::Connected {
            self.set_state(SessionState::Patching, out);
        }
    }

    /// DDD finished.
    pub fn patching_finished(&mut self, out: &mut Vec<FlowAction>) {
        if self.state == SessionState::Patching {
            self.set_state(SessionState::Connected, out);
        }
    }

    /// The character screen's enter-game → the character log-on, **phase 1**.
    ///
    /// Records the choice and, when no request is already outstanding, tears the world down and
    /// sends `0xF7C8`.
    ///
    /// **The teardown is inside the guard and comes first.**
    ///
    /// ```c
    /// if (!awaiting_log_on) { disconnect_from_world(); send_enter_world_request(); }
    /// awaiting_log_on = true;
    /// ```
    ///
    /// This is one of the two enter-world teardown call sites, and it is what makes a
    /// **second** entry from the same client session start where the first did: without it the
    /// action counter carries the last session's stamp into the new one.
    pub fn enter_world(&mut self, character: ObjectId, out: &mut Vec<FlowAction>) {
        self.selected_character = Some(character);
        if self.ready_to_enter_game {
            // The server was already ready — go straight to phase 2.
            self.phase_two(out);
            return;
        }
        if !self.awaiting_log_on {
            out.push(FlowAction::ExitWorldDisconnect);
            out.push(FlowAction::SendEnterWorldRequest);
        }
        self.awaiting_log_on = true;
        self.set_state(SessionState::EnteringWorld, out);
    }

    /// `0xF7DF Login_EnterGame_ServerReady` →
    /// The server-ready handler, whose whole body is
    ///
    /// ```c
    /// if (awaiting_log_on) { disconnect_from_world(); ready_to_enter_game = true; }
    /// ```
    ///
    /// so the teardown runs a **second** time on every world entry.
    pub fn server_ready(&mut self, out: &mut Vec<FlowAction>) {
        if self.awaiting_log_on {
            out.push(FlowAction::ExitWorldDisconnect);
            self.ready_to_enter_game = true;
        }
        // The player-system time step runs phase 2 on the next tick; doing it here is the same thing
        // one frame earlier and keeps the ordering assertion in the replay harness simple.
        self.tick_log_on(out);
    }

    /// The half of the player-system time step that advances the login.
    fn tick_log_on(&mut self, out: &mut Vec<FlowAction>) {
        if self.awaiting_log_on && self.ready_to_enter_game {
            self.awaiting_log_on = false;
            self.phase_two(out);
        }
    }

    /// The character log-on, **phase 2**.
    ///
    /// ```c
    /// clear the squelch database;
    /// if there is no packet controller, jump to the notice;
    /// gid = the UI flow's persistent selected-avatar id;
    /// if (!gid) return false;
    /// if a smart box exists, reset it with argument 1;
    /// flush the queued UI event deliverer;
    /// send enter-world for `gid` and the account id;
    /// set talk focus to 1;
    /// set combat mode to 1;
    /// enter the network world;
    /// ```
    ///
    /// **The world teardown is here, and it is the only place it is.** It runs
    /// *before* the enter-world send, so it is strictly ahead of the new session's `0xF746`,
    /// its objects and its `0x0013`; nothing that arrives afterwards can be caught by it.
    fn phase_two(&mut self, out: &mut Vec<FlowAction>) {
        let Some(gid) = self.selected_character else {
            return;
        };
        out.push(FlowAction::WorldReset);
        out.push(FlowAction::SendEnterWorld(gid));
        self.ready_to_enter_game = false;
        // Network enter-world occurs three statements after `SendEnterWorld` and
        // before the log-on request time is stamped.
        out.push(FlowAction::EnterWorld);
        self.set_state(SessionState::EnteringWorld, out);
    }

    /// Called when `0xF657` has actually gone out, so the 110-second clock starts from the send.
    pub fn enter_world_sent(&mut self, now: LocalTime) {
        self.log_on_request_time = Some(now);
    }

    /// `0x0013 Login_PlayerDescription` arrived — **this is what makes the client "in world"**.
    pub fn player_description_received(&mut self, out: &mut Vec<FlowAction>) {
        self.player_desc_received = true;
        self.log_on_request_time = None;
        self.set_state(SessionState::Playable, out);
    }

    /// `0xF659 Character_CharacterError`: clears both flags and drops back to the disconnected
    /// screen.
    pub fn character_error(&mut self, code: u32, out: &mut Vec<FlowAction>) {
        self.ready_to_enter_game = false;
        self.awaiting_log_on = false;
        self.log_on_request_time = None;
        self.set_state(
            SessionState::Disconnected(DisconnectReason::CharacterError(code)),
            out,
        );
    }

    /// `0xF653 Login_ExecuteLogOff` received:
    ///
    /// ```c
    /// clear the initial-login-complete and log-off-requested flags;  stamp the log-off time;
    /// disconnect from the world;
    /// clear the player-initialized and player-description flags;  clear the player id;
    /// ```
    ///
    /// The teardown is the middle statement, and it is the same action the two enter-world edges
    /// raise, so the event-counter reset fires from one place rather than being open-coded in
    /// [`crate::client_session::Session`]'s `0xF653` arm.
    pub fn log_off_received(&mut self, out: &mut Vec<FlowAction>) {
        self.player_desc_received = false;
        self.log_on_request_time = None;
        out.push(FlowAction::ExitWorldDisconnect);
        self.selected_character = None;
        self.set_state(SessionState::CharacterSelect, out);
    }

    /// The player asked to log off: send `0xF653` with the character id.
    pub fn log_off(&mut self, out: &mut Vec<FlowAction>) {
        if let Some(gid) = self.selected_character {
            out.push(FlowAction::SendLogOff(gid));
        }
    }

    /// A disconnect the session decided on.
    pub fn disconnect(&mut self, reason: DisconnectReason, out: &mut Vec<FlowAction>) {
        self.set_state(SessionState::Disconnected(reason), out);
    }

    /// The player system's per-frame step — the 110-second world-entry timeout.
    ///
    /// The clock starts when `0xF657` goes out, not when the player clicks; a world entry that
    /// produces no `Login_PlayerDescription` within 110 s falls through to the disconnected screen.
    pub fn tick(&mut self, now: LocalTime, out: &mut Vec<FlowAction>) {
        self.tick_log_on(out);
        if let Some(t) = self.log_on_request_time {
            if !self.player_desc_received && now.seconds_since(t) >= LOGON_TIMEOUT_SECONDS {
                self.log_on_request_time = None;
                self.set_state(
                    SessionState::Disconnected(DisconnectReason::ServerDied),
                    out,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: f64) -> LocalTime {
        LocalTime(s)
    }

    /// The H3 acceptance test: `Connected → CharacterSelect → EnteringWorld → Playable`, with the
    /// two-step enter-world emitting `0xF7C8` **then** `0xF657`, in that order.
    ///
    /// Oracle: the recovered character-selection flow §"Enter world" and its
    /// sequence diagram, transcribing the character log-on.
    #[test]
    fn the_two_step_enter_world_emits_f7c8_then_f657() {
        let mut f = Flow::new();
        let mut out = Vec::new();
        assert_eq!(f.state(), SessionState::Connected);

        f.character_set_received(&mut out);
        assert_eq!(f.state(), SessionState::CharacterSelect);

        out.clear();
        f.enter_world(ObjectId(0x5000_0001), &mut out);
        assert_eq!(
            out[..2],
            [
                FlowAction::ExitWorldDisconnect,
                FlowAction::SendEnterWorldRequest
            ],
            "the teardown, then 0xF7C8"
        );
        assert_eq!(f.state(), SessionState::EnteringWorld);

        // Nothing else goes out until the server says it is ready.
        out.clear();
        f.tick(t(1.0), &mut out);
        assert!(out.is_empty(), "0xF657 must wait for 0xF7DF");

        f.server_ready(&mut out);
        assert_eq!(
            out[..3],
            [
                FlowAction::ExitWorldDisconnect,
                FlowAction::WorldReset,
                FlowAction::SendEnterWorld(ObjectId(0x5000_0001)),
            ],
            "the transport teardown, the world teardown, then 0xF657"
        );
        assert!(
            out.contains(&FlowAction::EnterWorld),
            "entering the world follows the send"
        );
        f.enter_world_sent(t(1.0));

        out.clear();
        f.player_description_received(&mut out);
        assert_eq!(f.state(), SessionState::Playable);
        assert_eq!(out, vec![FlowAction::StateChanged(SessionState::Playable)]);
    }

    /// The **110-second** no-`0x0013` timeout fires `ServerDied`. Oracle:
    /// The world-entry timeout: `log_on_request_time != 0 && cur_time - log_on_request_time
    /// >= 110.0`.
    #[test]
    fn the_hundred_and_ten_second_timeout_fires_server_died() {
        let mut f = Flow::new();
        let mut out = Vec::new();
        f.character_set_received(&mut out);
        f.enter_world(ObjectId(1), &mut out);
        f.server_ready(&mut out);
        f.enter_world_sent(t(10.0));

        out.clear();
        f.tick(t(119.0), &mut out);
        assert!(out.is_empty(), "109 seconds is not enough");
        assert_eq!(f.state(), SessionState::EnteringWorld);

        f.tick(t(120.0), &mut out);
        assert_eq!(
            f.state(),
            SessionState::Disconnected(DisconnectReason::ServerDied),
            "the test is >=, so exactly 110 s fires"
        );
    }

    /// The timeout is cancelled by `0x0013`, not by `0xF7DF`.
    #[test]
    fn the_player_description_cancels_the_timeout() {
        let mut f = Flow::new();
        let mut out = Vec::new();
        f.enter_world(ObjectId(1), &mut out);
        f.server_ready(&mut out);
        f.enter_world_sent(t(0.0));
        f.player_description_received(&mut out);

        out.clear();
        f.tick(t(1000.0), &mut out);
        assert_eq!(f.state(), SessionState::Playable);
    }

    /// A second `enter_world` while a request is outstanding must not send a second `0xF7C8`;
    /// the character log-on guards it with `if (!awaiting_log_on)`.
    #[test]
    fn a_second_click_does_not_resend_the_request() {
        let mut f = Flow::new();
        let mut out = Vec::new();
        f.enter_world(ObjectId(1), &mut out);
        let first = out
            .iter()
            .filter(|a| **a == FlowAction::SendEnterWorldRequest)
            .count();
        f.enter_world(ObjectId(2), &mut out);
        let second = out
            .iter()
            .filter(|a| **a == FlowAction::SendEnterWorldRequest)
            .count();
        assert_eq!(first, 1);
        assert_eq!(second, 1, "still only one request");
        // But the newer choice wins, exactly as the client's selected-avatar id does.
        assert_eq!(f.selected_character(), Some(ObjectId(2)));
    }

    /// `log_off` emits `0xF653` with the character id.
    #[test]
    fn log_off_emits_f653() {
        let mut f = Flow::new();
        let mut out = Vec::new();
        f.enter_world(ObjectId(0x5000_0009), &mut out);
        out.clear();
        f.log_off(&mut out);
        assert_eq!(out, vec![FlowAction::SendLogOff(ObjectId(0x5000_0009))]);
    }

    /// A character error clears both flags so a retry starts from phase 1 again.
    #[test]
    fn a_character_error_unwinds_the_enter_world() {
        let mut f = Flow::new();
        let mut out = Vec::new();
        f.enter_world(ObjectId(1), &mut out);
        f.server_ready(&mut out);
        out.clear();
        f.character_error(13, &mut out);
        assert_eq!(
            f.state(),
            SessionState::Disconnected(DisconnectReason::CharacterError(13))
        );

        out.clear();
        f.enter_world(ObjectId(1), &mut out);
        assert_eq!(
            out[..2],
            [
                FlowAction::ExitWorldDisconnect,
                FlowAction::SendEnterWorldRequest
            ]
        );
    }

    /// The character set arriving again — ACE re-sends the whole list after a delete — must not
    /// knock the session out of the world.
    #[test]
    fn a_second_character_set_does_not_unwind_a_live_session() {
        let mut f = Flow::new();
        let mut out = Vec::new();
        f.character_set_received(&mut out);
        f.enter_world(ObjectId(1), &mut out);
        f.server_ready(&mut out);
        f.player_description_received(&mut out);
        assert_eq!(f.state(), SessionState::Playable);

        out.clear();
        f.character_set_received(&mut out);
        assert_eq!(f.state(), SessionState::Playable);
        assert!(out.is_empty());
    }

    /// Patching sits between `Connected` and `CharacterSelect` and does not block the character set.
    #[test]
    fn patching_is_a_state_of_its_own() {
        let mut f = Flow::new();
        let mut out = Vec::new();
        f.patching_started(&mut out);
        assert_eq!(f.state(), SessionState::Patching);
        // Against ACE the character set arrives before DDD ends; it still wins.
        f.character_set_received(&mut out);
        assert_eq!(f.state(), SessionState::CharacterSelect);
    }
}
