//! Position reporting — the producer for `0xF753`, `0xF61C` and `0xF61B`.
//!
//! Without this module nothing in this workspace would *send* a position. The
//! recorded corpus is **1,187 `0xF61C` + 640 `0xF753` + 2 `0xF61B` = 1,829 of its 2,564 outbound
//! game actions, 71.33%** (counted over `fixtures/message-corpus/*/blobs.jsonl`), and
//! the session's own `client_session::testing` replay catalogue is otherwise their only
//! constructor — a test fixture standing in for a producer. ACE writes
//! `Player.Location` only from `GameActionMoveToState`, `GameActionAutonomousPosition` and
//! `GameActionJump`, so a client that sends none of the three leaves the server's copy of the
//! player frozen at the login position.
//!
//! # The chain, as retail behaves
//!
//! * The command interpreter's per-frame step opens by asking whether a position event is due
//!   (`should_send_position_event`) — and on a yes sends one (`send_position_event`).
//! * `send_position_event` ends in the autonomous-position sender, which writes the
//!   sub-type **`0xF753`**.
//! * `send_movement_event` ends in the move-to-state sender,
//!   which writes **`0xF61C`**. Its six call sites are all command-list changes:
//!   disable, select-left, three keyboard-command arms, mouse movement and stop-completely.
//!
//! # Layering
//!
//! `Session` is still a protocol engine and not a player: this reporter is a **separate** object
//! the caller drives, it is handed the body's state rather than reaching for it, and every blob it
//! produces goes out through [`crate::client_session::Session::send_action`] — the one outbound game-action path in
//! this workspace, so there is still one `OrderedActionHeader` counter. `dereth-client-net` deliberately does not
//! depend on `dereth-physics` or `dereth-animation`, so the two facts this module cannot compute — the
//! position-validity answer and the `RawMotionState` in the client's own wire form —
//! arrive on [`PlayerMotion`] from the caller. `dereth-client`'s `App::position_use_time` is that
//! caller and cites the transcriptions it uses.
//!
//! # The third writer
//!
//! [`PositionReporter::send_jump`] is the `0xF61B Movement_Jump` half. It is in
//! this module rather than beside it because `JumpPack` carries the *same* four sequences off the
//! *same* [`PlayerMotion`], and splitting them would let a slot drift; but it is **not** part of
//! the position-reporting state machine. Jump messages are sent by the combat jump path,
//! which is a separate operation and touches none of
//! the five position fields above.

use crate::client_session::Session;
use dereth_primitives::Transport;
use dereth_protocol::movement::{
    AutonomousPosition, JumpPack, MoveTimestamps, MoveToStatePack, MovementAutonomousPosition,
    MovementJump, MovementMoveToState, RawMotionState,
};
use dereth_protocol::types::{PositionWire, Vec3};

/// The position-event interval, in seconds.
///
/// The stored interval was verified as the `f64` value 1.0.
/// Its representation has low dword `0x00000000` and high dword `0x3FF00000`:
/// this is an exact floating-point value, not a rounded decimal approximation.
/// The interval is initialized before any position report is sent.
/// The observed initialization and the independent timing distribution below agree;
/// no alternate interval is inferred from the shorter gaps.
///
/// Corroborated independently by the recording itself: the 551 gaps between the 552 `0xF753`
/// blobs of `long-solo-play` have median **1.00048 s**, with 67 of them inside the 0.02 s-wide
/// window `[0.99, 1.01)` against 9 in the 0.09 s-wide `[0.90, 0.99)`. The sub-second gaps (157
/// below 0.5 s) are the two immediate triggers below and the long ones (125 at or above 2 s) are a
/// player standing still, whose position has not changed.
pub const TIME_BETWEEN_POSITION_EVENTS: f64 = 1.0;

/// The `0.0002f` constant (exactly `0.00019999999494757503`), which both comparisons below
/// use. Plane equality compares against it directly, while frame equality uses it for each origin
/// and quaternion component.
pub const POSITION_EPSILON: f32 = 0.0002;

/// `MIN_JUMP_EXTENT`, the `float` `0x3A83126F`, i.e.
/// `0.0010000000474974513f`.
///
/// The jump path reads it twice, once as the threshold `0.001 <= level` and once as the floor,
/// and both reads are the **same constant**. Together they implement
/// `extent = max(level, MIN_JUMP_EXTENT)`, so an instantaneous tap jumps at 0.001,
/// never at 0. Corroborated independently by `dereth_animation::motion::power_bar_level`, which is
/// derived the same way.
pub const MIN_JUMP_EXTENT: f32 = 0.001;

/// The player's contact plane, in the frame of `contact_plane_cell_id`.
///
/// Only [`ContactPlane::approx_eq`] is ever asked of it here, which is the whole of the client's
/// use in `PositionReporter::should_send_position_event`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ContactPlane {
    pub normal: Vec3,
    pub d: f32,
}

impl ContactPlane {
    /// Plane equality requires all four components within [`POSITION_EPSILON`],
    /// **strictly** less than, never equal.
    #[must_use]
    pub fn approx_eq(&self, o: &Self) -> bool {
        (self.normal.x - o.normal.x).abs() < POSITION_EPSILON
            && (self.normal.y - o.normal.y).abs() < POSITION_EPSILON
            && (self.normal.z - o.normal.z).abs() < POSITION_EPSILON
            && (self.d - o.d).abs() < POSITION_EPSILON
    }
}

/// The frame equality test compares all three origin coordinates and all four quaternion
/// components with the same `0.0002` epsilon.
///
/// The client's own field order is `w x y z`, which is [`dereth_protocol::types::Quat`]'s.
#[must_use]
pub fn frames_equal(a: &dereth_protocol::types::Frame, b: &dereth_protocol::types::Frame) -> bool {
    (a.origin.x - b.origin.x).abs() < POSITION_EPSILON
        && (a.origin.y - b.origin.y).abs() < POSITION_EPSILON
        && (a.origin.z - b.origin.z).abs() < POSITION_EPSILON
        && (a.orientation.w - b.orientation.w).abs() < POSITION_EPSILON
        && (a.orientation.x - b.orientation.x).abs() < POSITION_EPSILON
        && (a.orientation.y - b.orientation.y).abs() < POSITION_EPSILON
        && (a.orientation.z - b.orientation.z).abs() < POSITION_EPSILON
}

/// Everything the two senders read from the player in one frame's worth of state.
///
/// The caller fills this from the body it owns and hands it in; nothing here reaches for it. Field
/// by field, against the player state and its validity checks:
///
/// | field | client |
/// |---|---|
/// | `position` | `player->position` |
/// | `position_valid` | a valid inbound cell id and seven `_isnan` checks on the frame |
/// | `timestamps` | `update_times[8]`, `[5]`, `[4]`, `[6]` — the instance, server-controlled-move, teleport and force-position sequences, in that argument order in both senders |
/// | `contact` | `(transient_state & 1) && (transient_state & 2)` — `CONTACT_TS` and `ON_WALKABLE_TS` |
/// | `longjump_mode` | the motion interpreter's standing-longjump flag |
/// | `raw_motion_state` | the player's queried raw motion state |
/// | `contact_plane` | `player->contact_plane` |
///
/// `contact` is the only input to `AutonomousPosition`'s trailing byte, and
/// `PositionReporter::send_position_event` refuses to send at all unless it is true — so
/// **every** `0xF753` the client can emit carries `contact = 1`. That is a falsifiable prediction
/// and the corpus confirms it at **640 of 640**.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlayerMotion {
    pub position: PositionWire,
    pub position_valid: bool,
    pub timestamps: MoveTimestamps,
    pub contact: bool,
    pub longjump_mode: bool,
    pub raw_motion_state: RawMotionState,
    pub contact_plane: ContactPlane,
}

/// What the reporter has done, so a test can assert emission rather than infer it.
///
/// A recorded session shows how a position report is **encoded**, not when one is **emitted**, so
/// a producer needs counters of its own.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PositionReporterStats {
    /// `0xF753` blobs handed to [`Session::send_action`].
    pub position_events: u64,
    /// `0xF61C` blobs handed to [`Session::send_action`].
    pub movement_events: u64,
    /// Sends refused by `send_position_event`'s own gate (no contact, or an invalid position).
    pub position_events_gated: u64,
    /// `0xF61B` blobs handed to [`Session::send_action`] — the third writer of the
    /// server's copy of the player, beside `0xF753` and `0xF61C`.
    pub jump_events: u64,
    /// Sends `send_action` would not encode. Zero is the only correct value.
    pub encode_failures: u64,
}

/// Five position-reporting fields and the two senders that use them.
///
/// The state retains the last sent time, position and contact plane,
/// the interval between position events, and the autonomy level.
/// The two send paths share this state.
#[derive(Debug, Clone)]
pub struct PositionReporter {
    /// The interpreter's active test — `enabled != 0 && player != NULL`, the first of
    /// `should_send_position_event`'s four gates. The caller sets it when there is a body and a
    /// link.
    pub active: bool,
    /// The command interpreter's autonomy level, initialized to **2 by the constructor**; the setter
    /// refuses anything above 2. `should_send_position_event` requires exactly 2, so the
    /// client reports its position autonomously by default and stops while the server is driving.
    pub autonomy_level: u32,
    time_between_position_events: f64,
    last_sent_position_time: f64,
    last_sent_position: PositionWire,
    last_sent_contact_plane: ContactPlane,
    /// **The edge detector for `0xF61C`.**
    ///
    /// The client sends a `MoveToState` from six handlers after changing a movement command list.
    /// This build does not store those lists; their common observable is that the
    /// body's `RawMotionState` differs from the one last sent, so that is the edge taken here.
    ///
    /// It **under**-sends rather than over-sends, and the size is measured rather than asserted:
    /// of the 1,187 recorded `0xF61C` blobs, **48** carry a `RawMotionState` byte-identical to the
    /// one immediately before it in send order and **1,139 of the 1,186 consecutive pairs
    /// (96.0%) differ** — those 48 are the call sites that re-send an unchanged state
    /// (the select-left handler and the stop-completely path). `None` until the first send, so the
    /// first change away from the default state is itself an edge.
    last_sent_motion: Option<RawMotionState>,
    pub stats: PositionReporterStats,
}

impl PositionReporter {
    /// The interpreter constructor's five initialisations.
    ///
    /// `last_sent_position_time` starts at the current time, not at zero — a reporter that
    /// starts at zero fires on its very first frame because `0 + 1.0 < now` for any real clock.
    #[must_use]
    pub fn new(now: f64) -> Self {
        Self {
            active: false,
            autonomy_level: 2,
            time_between_position_events: TIME_BETWEEN_POSITION_EVENTS,
            last_sent_position_time: now,
            // `objcell_id = 0`, identity quaternion, zero origin — the constructor's cached
            // default, which is never a valid cell, so the first send is never suppressed by it.
            last_sent_position: PositionWire::default(),
            last_sent_contact_plane: ContactPlane::default(),
            last_sent_motion: None,
            stats: PositionReporterStats::default(),
        }
    }

    /// The interval, so a test can state it as a literal against the client's own constant rather
    /// than reading it back through the symbol that wrote it.
    #[must_use]
    pub const fn interval(&self) -> f64 {
        self.time_between_position_events
    }

    /// The last position actually sent in a `0xF753`, for the tests.
    #[must_use]
    pub const fn last_sent_position(&self) -> &PositionWire {
        &self.last_sent_position
    }

    /// The should-send-position test, whole.
    ///
    /// ```text
    /// if (!active || autonomy_level != 2 || !world_objects || !player) return 0;
    /// if (time_between_position_events + last_sent_position_time < current_time) {
    ///     if (last_sent_position.objcell_id == player->position.objcell_id
    ///         && frames_equal(&last_sent_position.frame, &player->position.frame))
    ///         return 0;
    ///     return 1;
    /// }
    /// if (last_sent_position.objcell_id != player->position.objcell_id) return 1;
    /// return planes_equal(&last_sent_contact_plane, &player->contact_plane) == 0;
    /// ```
    ///
    /// Three triggers, and they are separable because the client separates them:
    ///
    /// 1. the **1.0 s** schedule, and then only if the position actually moved;
    /// 2. a **cell-id change**, immediately, without waiting for the interval;
    /// 3. a **contact-plane change**, immediately, but only while the cell is unchanged — the
    ///    `Plane` compare is in the `else` of the cell test, so a cell change short-circuits it.
    ///
    /// Note what trigger 1 does *not* do: it does not consult the contact plane. A player standing
    /// perfectly still on an unchanging plane sends nothing at all, for ever, which is why the
    /// recorded gaps run to 59.8 s.
    #[must_use]
    pub fn should_send_position_event(&self, now: f64, m: &PlayerMotion) -> bool {
        if !self.active || self.autonomy_level != 2 {
            return false;
        }
        if self.time_between_position_events + self.last_sent_position_time < now {
            if self.last_sent_position.objcell_id == m.position.objcell_id
                && frames_equal(&self.last_sent_position.frame, &m.position.frame)
            {
                return false;
            }
            return true;
        }
        if self.last_sent_position.objcell_id != m.position.objcell_id {
            return true;
        }
        !self.last_sent_contact_plane.approx_eq(&m.contact_plane)
    }

    /// Build the autonomous position pack from position, contact and timestamps 8, 5, 4 and 6,
    /// in the same argument order as the position sender.
    ///
    /// `contact` is `(transient_state & 1) && (transient_state & 2)` recomputed *after* the gate
    /// that already required both bits, so it is always `1` on the wire.
    #[must_use]
    pub fn autonomous_position_pack(m: &PlayerMotion) -> AutonomousPosition {
        AutonomousPosition {
            position: m.position,
            timestamps: m.timestamps,
            contact: u8::from(m.contact),
        }
    }

    /// Build the move-to-state pack from raw motion, position, contact, long-jump state and
    /// timestamps 8, 5, 4 and 6, in the same argument order as the movement sender.
    #[must_use]
    pub fn move_to_state_pack(m: &PlayerMotion) -> MoveToStatePack {
        MoveToStatePack {
            raw_motion_state: m.raw_motion_state.clone(),
            position: m.position,
            timestamps: m.timestamps,
            contact: m.contact,
            longjump_mode: m.longjump_mode,
        }
    }

    /// The position sender — the `0xF753` sender.
    ///
    /// Its gate is `world_objects && player && (transient_state & 1) && (transient_state & 2) &&
    /// a valid player position; on the far side it updates **all three** of
    /// `last_sent_position_time`, `last_sent_position` and `last_sent_contact_plane`.
    ///
    /// Returns the `OrderedActionHeader` stamp the blob carried, or `None` if the gate refused it.
    pub fn send_position_event<T: Transport>(
        &mut self,
        now: f64,
        m: &PlayerMotion,
        session: &mut Session<T>,
    ) -> Option<u32> {
        if !m.contact || !m.position_valid {
            self.stats.position_events_gated += 1;
            return None;
        }
        let pack = Self::autonomous_position_pack(m);
        match session.send_action(&MovementAutonomousPosition(pack)) {
            Ok(stamp) => {
                self.stats.position_events += 1;
                self.last_sent_position_time = now;
                self.last_sent_position = m.position;
                self.last_sent_contact_plane = m.contact_plane;
                Some(stamp)
            }
            Err(_) => {
                self.stats.encode_failures += 1;
                None
            }
        }
    }

    /// The movement sender — the `0xF61C` sender.
    ///
    /// Its gate is `player && world_objects && player has a raw motion state && autonomy_level !=
    /// 0`, and on the far side it updates **`last_sent_position_time` only**. That asymmetry is
    /// load-bearing: a `MoveToState` postpones the next `0xF753` by a full second without
    /// refreshing the position the schedule compares against, so the *next* autonomous position
    /// still reports movement that happened in between.
    pub fn send_movement_event<T: Transport>(
        &mut self,
        now: f64,
        m: &PlayerMotion,
        session: &mut Session<T>,
    ) -> Option<u32> {
        if self.autonomy_level == 0 {
            return None;
        }
        let pack = Self::move_to_state_pack(m);
        match session.send_action(&MovementMoveToState(pack)) {
            Ok(stamp) => {
                self.stats.movement_events += 1;
                self.last_sent_motion = Some(m.raw_motion_state.clone());
                self.last_sent_position_time = now;
                Some(stamp)
            }
            Err(_) => {
                self.stats.encode_failures += 1;
                None
            }
        }
    }

    /// The jump pack as the jump path
    /// builds it — the body of `0xF61B Movement_Jump`.
    ///
    /// ```text
    /// pack = JumpPack(extent, velocity, player position,
    ///                 update_times[8], [5], [4], [6])
    /// dispatch the jump event with this pack
    /// ```
    ///
    /// **The four sequences are the same four, in the same argument order** the movement and
    /// position senders pass — the instance, server-controlled-move, teleport and force-position
    /// sequences — so this reads them off the
    /// same [`PlayerMotion`] and a slot wired to a neighbour is wrong here for the same reason.
    /// Left to right the four are `update_times[8], [5], [4], [6]`.
    ///
    /// `velocity` is **not** the body's world velocity: the jump path reads the local physics
    /// velocity, which is transformed
    /// by `position.frame`'s cached local-to-global matrix — the velocity expressed in the **body's own
    /// frame**. The caller owes that transform for the same reason it owes the position-validity
    /// result.
    #[must_use]
    pub fn jump_pack(extent: f32, velocity: Vec3, m: &PlayerMotion) -> JumpPack {
        JumpPack {
            extent: extent.max(MIN_JUMP_EXTENT),
            velocity,
            position: m.position,
            timestamps: m.timestamps,
        }
    }

    /// The jump path's **autonomous success arm** — the `0xF61B` sender.
    ///
    /// The whole of the arm:
    ///
    /// ```text
    /// if no jump is pending: return
    /// extent = max(power-bar level, MIN_JUMP_EXTENT)      // one constant, read twice
    /// finish the jump                                     // clears the power bar
    /// status = apply the jump to the player's motion interpreter with extent
    /// read the player's local physics velocity             // AFTER the impulse, unconditionally
    /// status 0:    build the JumpPack and send it         // 0xF61B
    /// status 0x24: "you can't jump while in the air"
    /// status 0x48: "you can't jump from this position"
    /// status 0x49: "you can't jump loaded down"
    /// ```
    ///
    /// So the message goes out **only** when the jump operation returned 0, and the velocity in
    /// it is the one the impulse just produced — `jump()` is called before the read. The caller
    /// owns both facts, because both live behind `dereth-animation` and `dereth-physics`, which this crate
    /// deliberately does not depend on; `accepted` is that answer.
    ///
    /// **It does not touch any of the five periodic-position fields.** The jump request is
    /// owned by combat input, a separate subsystem: a jump neither postpones the next `0xF753` nor
    /// refreshes `last_sent_position`. That asymmetry is asserted rather than described, because a
    /// build that folded the jump into the reporter would look identical until the schedule ran.
    ///
    /// Returns the `OrderedActionHeader` stamp the blob carried, or `None` if the jump was refused or would
    /// not encode.
    pub fn send_jump<T: Transport>(
        &mut self,
        accepted: bool,
        extent: f32,
        velocity: Vec3,
        m: &PlayerMotion,
        session: &mut Session<T>,
    ) -> Option<u32> {
        if !accepted {
            return None;
        }
        let pack = Self::jump_pack(extent, velocity, m);
        self.send_jump_pack(pack, session)
    }

    /// Send an already produced post-impulse/pre-physics JumpPack. The App and isolated adapter
    /// share this encoder/statistics owner; no caller needs to reconstruct the body later.
    pub fn send_jump_pack<T: Transport>(
        &mut self,
        pack: JumpPack,
        session: &mut Session<T>,
    ) -> Option<u32> {
        match session.send_action(&MovementJump(pack)) {
            Ok(stamp) => {
                self.stats.jump_events += 1;
                Some(stamp)
            }
            Err(_) => {
                self.stats.encode_failures += 1;
                None
            }
        }
    }

    /// True when the body's raw motion state differs from the one the last `0xF61C` carried.
    ///
    /// See [`PositionReporter::last_sent_motion`]'s note for why this stands in for the client's
    /// six command handlers, and for the measured size of the difference.
    #[must_use]
    pub fn movement_state_changed(&self, m: &PlayerMotion) -> bool {
        self.last_sent_motion.as_ref() != Some(&m.raw_motion_state)
    }

    /// The opening of the interpreter's per-frame step, plus this build's
    /// stand-in for the six places retail sends a movement event.
    ///
    /// Order matters and is the client's: the command handlers run in the input step, *before*
    /// the per-frame step asks whether a position event is due. So a frame in which the movement state changed
    /// sends the `0xF61C` first, which stamps `last_sent_position_time`, which then suppresses the
    /// `0xF753` for the next second — exactly as retail behaves when a key is pressed.
    pub fn use_time<T: Transport>(&mut self, now: f64, m: &PlayerMotion, session: &mut Session<T>) {
        if !self.active {
            return;
        }
        if self.movement_state_changed(m) {
            self.send_movement_event(now, m, session);
        }
        if self.should_send_position_event(now, m) {
            self.send_position_event(now, m, session);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The constant, spelled as a literal once, against the value retail uses.
    ///
    /// A test that reads a constant through the same symbol it writes through cannot detect a
    /// wrong constant. `0x3FF00000_00000000` is the double the constructor stores.
    #[test]
    fn the_interval_is_the_double_the_constructor_stores() {
        assert_eq!(TIME_BETWEEN_POSITION_EVENTS, 1.0);
        assert_eq!(
            TIME_BETWEEN_POSITION_EVENTS.to_bits(),
            0x3FF0_0000_0000_0000
        );
        assert_eq!(PositionReporter::new(0.0).interval(), 1.0);
    }

    /// Retail's epsilon is the float `0x3951B717`, which is `0.00019999999494757503f`.
    #[test]
    fn the_epsilon_is_the_float_at_the_address_both_comparisons_read() {
        assert_eq!(
            POSITION_EPSILON.to_bits(),
            u32::from_le_bytes([0x17, 0xB7, 0x51, 0x39])
        );
    }

    /// Strictly less than, on every component, including `d`.
    #[test]
    fn planes_compare_component_wise_and_strictly() {
        let a = ContactPlane {
            normal: Vec3 {
                x: 0.0,
                y: 0.0,
                z: 1.0,
            },
            d: 0.0,
        };
        assert!(a.approx_eq(&a));
        let mut b = a;
        b.d = POSITION_EPSILON;
        assert!(!a.approx_eq(&b), "equal to the epsilon is not within it");
        b.d = POSITION_EPSILON / 2.0;
        assert!(a.approx_eq(&b));
        let mut c = a;
        c.normal.y = 0.001;
        assert!(!a.approx_eq(&c));
    }
}
