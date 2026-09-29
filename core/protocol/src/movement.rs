//! Family: movement — `docs/networking/messages/05-movement.md`.
//!
//! The client is authoritative for its own object and a pure receiver for everyone else's: it sends
//! **ordered game actions** on the Weenie queue describing what it wants to do, and the server sends
//! **unordered WorldObjects messages** describing where every object actually is.
//!
//! # The two traps in this family
//!
//! * **`PositionPack`'s quaternion bits are inverted.** A *set* bit means the component is
//!   **omitted** and defaulted; a *clear* bit means it is present. Bits 0–2 have the normal sense.
//!   Getting it backwards makes every object face north.
//! * **The motion command wire format is not uniform.** [`RawMotionState`] (client → server) packs
//!   full **32-bit ids** for style/forward/sidestep/turn but **16-bit indices** for queued actions;
//!   [`InterpretedMotionState`] (server → client) packs indices everywhere. The index is a position
//!   in the client's 412-entry `command_ids` table, and resolving it is the **animation crate's**
//!   job — this crate carries the index verbatim rather than assuming "the low 16 bits of the id
//!   are the index", which happens to be true of the shipped table and is not a property of the
//!   format.

use crate::archive::{Reader, Writer};
use crate::error::MessageError;
use crate::opcodes::Opcode;
use crate::types::physicsdesc::PhysicsEventStamp;
use crate::types::{Origin, PositionWire, Quat, Vec3};
use crate::Message;
use dereth_primitives::ObjectId;

/// The four sequence numbers every client-to-server movement pack echoes back, so the server can
/// tell whether the client had already seen a given correction when it produced the move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MoveTimestamps {
    pub instance: u16,
    pub server_control: u16,
    pub teleport: u16,
    pub force_position: u16,
}

impl MoveTimestamps {
    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            instance: r.u16()?,
            server_control: r.u16()?,
            teleport: r.u16()?,
            force_position: r.u16()?,
        })
    }

    fn write(&self, w: &mut Writer) {
        w.u16(self.instance);
        w.u16(self.server_control);
        w.u16(self.teleport);
        w.u16(self.force_position);
    }
}

/// The position pack.
///
/// The `flags` bits:
///
/// | bit | meaning |
/// |---:|---|
/// | `0x01` | a velocity vector follows the origin |
/// | `0x02` | a placement id follows |
/// | `0x04` | `has_contact` |
/// | `0x08` | the quaternion's **w is omitted** |
/// | `0x10` | **x omitted** |
/// | `0x20` | **y omitted** |
/// | `0x40` | **z omitted** |
///
/// Bits 3–6 are inverted relative to every other flag in the protocol. An omitted `w` defaults to
/// 1.0 and the omitted vector components to 0.0, which is why an inverted read yields north-facing
/// objects rather than an obvious failure.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PositionPack {
    pub flags: u32,
    pub origin: Origin,
    pub orientation: Quat,
    pub velocity: Option<Vec3>,
    pub placement_id: Option<u32>,
    pub instance_timestamp: u16,
    pub position_timestamp: u16,
    pub teleport_timestamp: u16,
    pub force_position_timestamp: u16,
}

/// The `PositionPack` flag bits.
pub mod position_flags {
    pub const HAS_VELOCITY: u32 = 0x0001;
    pub const HAS_PLACEMENT_ID: u32 = 0x0002;
    pub const IS_GROUNDED: u32 = 0x0004;
    /// Set = the component is **absent**.
    pub const ORIENTATION_HAS_NO_W: u32 = 0x0008;
    /// Set = the component is **absent**.
    pub const ORIENTATION_HAS_NO_X: u32 = 0x0010;
    /// Set = the component is **absent**.
    pub const ORIENTATION_HAS_NO_Y: u32 = 0x0020;
    /// Set = the component is **absent**.
    pub const ORIENTATION_HAS_NO_Z: u32 = 0x0040;
}

impl PositionPack {
    /// `has_contact`, i.e. `(flags >> 2) & 1`.
    #[must_use]
    pub fn has_contact(&self) -> bool {
        self.flags & position_flags::IS_GROUNDED != 0
    }

    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        use position_flags as f;
        let flags = r.u32()?;
        let origin = Origin::read(r)?;
        // Inverted: the bit is set when the component is ABSENT.
        let absent = |b: u32| flags & b != 0;
        let orientation = Quat {
            w: if absent(f::ORIENTATION_HAS_NO_W) {
                1.0
            } else {
                r.f32()?
            },
            x: if absent(f::ORIENTATION_HAS_NO_X) {
                0.0
            } else {
                r.f32()?
            },
            y: if absent(f::ORIENTATION_HAS_NO_Y) {
                0.0
            } else {
                r.f32()?
            },
            z: if absent(f::ORIENTATION_HAS_NO_Z) {
                0.0
            } else {
                r.f32()?
            },
        };
        let velocity = if flags & f::HAS_VELOCITY != 0 {
            Some(Vec3::read(r)?)
        } else {
            None
        };
        let placement_id = if flags & f::HAS_PLACEMENT_ID != 0 {
            Some(r.u32()?)
        } else {
            None
        };
        Ok(Self {
            flags,
            origin,
            orientation,
            velocity,
            placement_id,
            instance_timestamp: r.u16()?,
            position_timestamp: r.u16()?,
            teleport_timestamp: r.u16()?,
            force_position_timestamp: r.u16()?,
        })
    }

    pub fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        use position_flags as f;
        w.u32(self.flags);
        self.origin.write(w);
        let absent = |b: u32| self.flags & b != 0;
        if !absent(f::ORIENTATION_HAS_NO_W) {
            w.f32(self.orientation.w);
        }
        if !absent(f::ORIENTATION_HAS_NO_X) {
            w.f32(self.orientation.x);
        }
        if !absent(f::ORIENTATION_HAS_NO_Y) {
            w.f32(self.orientation.y);
        }
        if !absent(f::ORIENTATION_HAS_NO_Z) {
            w.f32(self.orientation.z);
        }
        if (self.flags & f::HAS_VELOCITY != 0) != self.velocity.is_some() {
            return Err(MessageError::Unencodable {
                field: "position velocity",
                reason: "presence must match flag 0x01",
            });
        }
        if let Some(v) = self.velocity {
            v.write(w);
        }
        if (self.flags & f::HAS_PLACEMENT_ID != 0) != self.placement_id.is_some() {
            return Err(MessageError::Unencodable {
                field: "position placement id",
                reason: "presence must match flag 0x02",
            });
        }
        if let Some(p) = self.placement_id {
            w.u32(p);
        }
        w.u16(self.instance_timestamp);
        w.u16(self.position_timestamp);
        w.u16(self.teleport_timestamp);
        w.u16(self.force_position_timestamp);
        Ok(())
    }
}

/// One queued action inside a motion state. 8 bytes.
///
/// `command_index` is the index into the client's 412-entry `command_ids` table, **not** the 32-bit
/// `MotionCommand` id. The animation crate owns the table.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MotionAction {
    pub command_index: u16,
    /// `(stamp & 0x7FFF) | (autonomous ? 0x8000 : 0)`.
    pub stamp_and_autonomy: u16,
    pub speed: f32,
}

impl MotionAction {
    #[must_use]
    pub fn stamp(&self) -> u16 {
        self.stamp_and_autonomy & 0x7FFF
    }

    #[must_use]
    pub fn autonomous(&self) -> bool {
        self.stamp_and_autonomy & 0x8000 != 0
    }

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            command_index: r.u16()?,
            stamp_and_autonomy: r.u16()?,
            speed: r.f32()?,
        })
    }

    fn write(&self, w: &mut Writer) {
        w.u16(self.command_index);
        w.u16(self.stamp_and_autonomy);
        w.f32(self.speed);
    }
}

/// The raw motion state, pack and unpack — **client → server**.
///
/// A leading 32-bit flags word, then only the non-default fields. Bits 11–15 are a 5-bit **count**
/// of queued actions.
///
/// The four command fields are full **32-bit `MotionCommand` ids**; only the queued actions are
/// index-compressed. That asymmetry is easy to get wrong.
///
/// # The corpus cannot show you a walking player
///
/// Every recorded session was run-locked, so over the **1,187** outbound `0xF61C` bodies the flags
/// word takes only nine values: `0x001` x228, `0x003` x69, `0x005` x318, `0x007` x43, `0x101` x179,
/// `0x103` x33, `0x105` x282, `0x107` x30 and `0x000` x5. Bit 0 (`current_holdkey`) is set in
/// **1,182 of 1,187**, and **flags `0x004` — a forward command with no hold key, which is exactly
/// what walking looks like — occurs 0 times in 1,187**. Bit 4 (`forward_speed`) is likewise absent
/// in all 1,187.
///
/// So the walking tests rest on the pack's presence rules (a field travels only
/// when it differs from its default: `forward_command != Ready`, `current_holdkey` not the no-key
/// value)
/// and on a driven body over Holtburg, **not** on the recording; only the running half is
/// corpus-checked. Nothing here is wrong — the rule is read out of the client's pack — but the
/// claim's support is narrower than the 1,187 suggests, and this says so rather than letting the
/// number stand in for coverage it does not give. One minute of a capture taken with the run key
/// off closes it. Counted over `fixtures/message-corpus/*/blobs.jsonl`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RawMotionState {
    pub current_holdkey: Option<u32>,
    pub current_style: Option<u32>,
    pub forward_command: Option<u32>,
    pub forward_holdkey: Option<u32>,
    pub forward_speed: Option<f32>,
    pub sidestep_command: Option<u32>,
    pub sidestep_holdkey: Option<u32>,
    pub sidestep_speed: Option<f32>,
    pub turn_command: Option<u32>,
    pub turn_holdkey: Option<u32>,
    pub turn_speed: Option<f32>,
    pub actions: Vec<MotionAction>,
}

impl RawMotionState {
    /// Rebuild the flags word from which fields are present. The action count occupies bits 11–15.
    pub fn flags(&self) -> Result<u32, MessageError> {
        let mut f = 0u32;
        for (i, present) in [
            self.current_holdkey.is_some(),
            self.current_style.is_some(),
            self.forward_command.is_some(),
            self.forward_holdkey.is_some(),
            self.forward_speed.is_some(),
            self.sidestep_command.is_some(),
            self.sidestep_holdkey.is_some(),
            self.sidestep_speed.is_some(),
            self.turn_command.is_some(),
            self.turn_holdkey.is_some(),
            self.turn_speed.is_some(),
        ]
        .into_iter()
        .enumerate()
        {
            if present {
                f |= 1 << i;
            }
        }
        if self.actions.len() > 31 {
            return Err(MessageError::Unencodable {
                field: "raw motion actions",
                reason: "the count is five bits, so at most 31 actions",
            });
        }
        // Guarded above.
        #[allow(clippy::cast_possible_truncation)]
        Ok(f | ((self.actions.len() as u32) << 11))
    }

    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let flags = r.u32()?;
        let has = |i: u32| flags & (1 << i) != 0;
        let opt_u32 = |r: &mut Reader<'_>, present: bool| -> Result<Option<u32>, MessageError> {
            if present {
                Ok(Some(r.u32()?))
            } else {
                Ok(None)
            }
        };
        let opt_f32 = |r: &mut Reader<'_>, present: bool| -> Result<Option<f32>, MessageError> {
            if present {
                Ok(Some(r.f32()?))
            } else {
                Ok(None)
            }
        };
        let mut s = Self {
            current_holdkey: opt_u32(r, has(0))?,
            current_style: opt_u32(r, has(1))?,
            forward_command: opt_u32(r, has(2))?,
            forward_holdkey: opt_u32(r, has(3))?,
            forward_speed: opt_f32(r, has(4))?,
            sidestep_command: opt_u32(r, has(5))?,
            sidestep_holdkey: opt_u32(r, has(6))?,
            sidestep_speed: opt_f32(r, has(7))?,
            turn_command: opt_u32(r, has(8))?,
            turn_holdkey: opt_u32(r, has(9))?,
            turn_speed: opt_f32(r, has(10))?,
            actions: Vec::new(),
        };
        let count = (flags >> 11) & 0x1F;
        for _ in 0..count {
            s.actions.push(MotionAction::read(r)?);
        }
        Ok(s)
    }

    pub fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.flags()?);
        for v in [
            self.current_holdkey,
            self.current_style,
            self.forward_command,
            self.forward_holdkey,
        ]
        .into_iter()
        .flatten()
        {
            w.u32(v);
        }
        if let Some(v) = self.forward_speed {
            w.f32(v);
        }
        for v in [self.sidestep_command, self.sidestep_holdkey]
            .into_iter()
            .flatten()
        {
            w.u32(v);
        }
        if let Some(v) = self.sidestep_speed {
            w.f32(v);
        }
        for v in [self.turn_command, self.turn_holdkey].into_iter().flatten() {
            w.u32(v);
        }
        if let Some(v) = self.turn_speed {
            w.f32(v);
        }
        for a in &self.actions {
            a.write(w);
        }
        Ok(())
    }
}

/// The interpreted motion state, pack and unpack — **server → client**.
///
/// Same idea as [`RawMotionState`] but every command is a **16-bit index**, there are no hold-key
/// fields, and the action count is bits 7–11. The wire order is: flags, the four command indices in
/// bit order, then the three speeds, then the actions, then **pad to 4**.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct InterpretedMotionState {
    pub current_style: Option<u16>,
    pub forward_command: Option<u16>,
    pub forward_speed: Option<f32>,
    pub sidestep_command: Option<u16>,
    pub sidestep_speed: Option<f32>,
    pub turn_command: Option<u16>,
    pub turn_speed: Option<f32>,
    pub actions: Vec<MotionAction>,
}

impl InterpretedMotionState {
    pub fn flags(&self) -> Result<u32, MessageError> {
        let mut f = 0u32;
        for (i, present) in [
            self.current_style.is_some(),
            self.forward_command.is_some(),
            self.forward_speed.is_some(),
            self.sidestep_command.is_some(),
            self.sidestep_speed.is_some(),
            self.turn_command.is_some(),
            self.turn_speed.is_some(),
        ]
        .into_iter()
        .enumerate()
        {
            if present {
                f |= 1 << i;
            }
        }
        if self.actions.len() > 31 {
            return Err(MessageError::Unencodable {
                field: "interpreted motion actions",
                reason: "the count is five bits, so at most 31 actions",
            });
        }
        #[allow(clippy::cast_possible_truncation)]
        Ok(f | ((self.actions.len() as u32) << 7))
    }

    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let flags = r.u32()?;
        let has = |i: u32| flags & (1 << i) != 0;
        // The four command indices come first, in bit order, then the three speeds.
        let current_style = if has(0) { Some(r.u16()?) } else { None };
        let forward_command = if has(1) { Some(r.u16()?) } else { None };
        let sidestep_command = if has(3) { Some(r.u16()?) } else { None };
        let turn_command = if has(5) { Some(r.u16()?) } else { None };
        let forward_speed = if has(2) { Some(r.f32()?) } else { None };
        let sidestep_speed = if has(4) { Some(r.f32()?) } else { None };
        let turn_speed = if has(6) { Some(r.f32()?) } else { None };
        let mut s = Self {
            current_style,
            forward_command,
            forward_speed,
            sidestep_command,
            sidestep_speed,
            turn_command,
            turn_speed,
            actions: Vec::new(),
        };
        let count = (flags >> 7) & 0x1F;
        for _ in 0..count {
            s.actions.push(MotionAction::read(r)?);
        }
        r.align4()?;
        Ok(s)
    }

    pub fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.flags()?);
        for v in [
            self.current_style,
            self.forward_command,
            self.sidestep_command,
            self.turn_command,
        ]
        .into_iter()
        .flatten()
        {
            w.u16(v);
        }
        for v in [self.forward_speed, self.sidestep_speed, self.turn_speed]
            .into_iter()
            .flatten()
        {
            w.f32(v);
        }
        for a in &self.actions {
            a.write(w);
        }
        w.align4();
        Ok(())
    }
}

// -------------------------------------------------------------------------------------------
// The movement buffer: what `0xF74C`, `0xF619` and a `PhysicsDesc` all carry.
// -------------------------------------------------------------------------------------------

/// `MovementType` — the discriminant switches on.
///
/// Used by `0xF74C Movement_SetObjectMovement`. Only `Invalid` carries an
/// [`InterpretedMotionState`]; the other four carry move-to and turn-to payloads.
pub mod movement_type {
    /// The default arm, and the only one that carries an `InterpretedMotionState`.
    pub const INVALID: u8 = 0;
    pub const MOVE_TO_OBJECT: u8 = 6;
    pub const MOVE_TO_POSITION: u8 = 7;
    pub const TURN_TO_OBJECT: u8 = 8;
    pub const TURN_TO_HEADING: u8 = 9;
}

/// `MotionFlags`, the high byte of the combined `u16` the movement unpack reads first.
///
/// The two names are external — ACE's `ACE.Entity/Enum/MotionFlags` — but the client reads exactly
/// these two bits of the combined word as `0x100` and `0x200`.
pub mod motion_flags {
    /// The body is followed by a `u32` object id to stick to.
    pub const STICK_TO_OBJECT: u8 = 0x01;
    pub const STANDING_LONG_JUMP: u8 = 0x02;
}

/// The decoded body of a movement buffer.
///
/// A `PhysicsDesc`'s embedded movement buffer starts here — the create handler takes the three
/// header fields of [`MovementBuffer`] from the descriptor's own
/// timestamps instead of from the buffer.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MovementBody {
    /// The low byte of the combined `u16`; see [`movement_type`].
    pub movement_type: u8,
    /// The high byte; see [`motion_flags`].
    pub motion_flags: u8,
    /// An **index** into the 412-entry `command_ids` table, like every other command here.
    pub current_style: u16,
    /// `movement_type == INVALID`: the state is given.
    pub interpreted: Option<InterpretedMotionState>,
    /// `motion_flags & STICK_TO_OBJECT`: the `u32` after the interpreted state.
    pub sticky_object: Option<ObjectId>,
    /// The four `MoveTo`/`TurnTo` arms, carried whole.
    ///
    /// The bytes are kept whole so the buffer re-encodes exactly; `MoveToArm` decodes them on
    /// demand.
    ///
    /// **How many buffers take this arm** — measured over the **whole-message** opcode space of
    /// all 11,245 blobs in the seven-session corpus (`fixtures/message-corpus/*/blobs.jsonl`).
    /// `0xF74C` is a
    /// plain `GameMessage`, so it lives in that space and can never appear as an `0xF7B0` or
    /// `0xF7B1` sub-opcode.
    ///
    /// | `movement_type` | buffers |
    /// |---|---:|
    /// | `INVALID` (0) — the interpreted/animating arm | 2,031 |
    /// | `MOVE_TO_OBJECT` (6) | 22 |
    /// | `MOVE_TO_POSITION` (7) | 12 |
    /// | `TURN_TO_OBJECT` (8) | 151 |
    /// | `TURN_TO_HEADING` (9) | 5 |
    /// | **total `0xF74C`** | **2,221** |
    ///
    /// So **190 of 2,221** take this field; all of them are a `MoveTo`/`TurnTo`, which is
    /// path-finding rather than animation. Asserted by the client-session movement-count test,
    /// so the next corpus growth reddens a named test instead of ageing this comment.
    pub unhandled: Vec<u8>,
}

/// The four `MoveTo`/`TurnTo` arms of a movement buffer, decoded.
///
/// The movement unpack reads each arm inline in its own
/// `case`, so there is no single struct in the client to name; this is those four `case` bodies,
/// in their wire order. Every field order and every size below is that function's.
///
/// The arm is decoded on demand from [`MovementBody::unhandled`] rather than replacing it, so a
/// buffer still re-encodes byte for byte from the bytes it arrived as.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MoveToArm {
    /// `case 6`. 52 bytes: `u32 target`, `Origin`, a 0x1C `MovementParameters`, `f32 run_rate`.
    ///
    /// **The client throws `origin` away when it knows the target.** `case 6` reads it into a
    /// local, then starts move-to-object for `target` with `params` -- which passes
    /// only the id, and takes the position from the object table instead. The origin is the
    /// fall-back: when the target object lookup returns null the case `break`s out of the switch
    /// and lands on move-to-position for `origin` and `params` at the bottom of the
    /// function, so an approach to an object the client has never heard of walks to the spot the
    /// server named.
    MoveToObject {
        target: ObjectId,
        origin: Origin,
        params: MovementParameters,
        run_rate: f32,
    },
    /// `case 7`. 48 bytes: `Origin`, a 0x1C `MovementParameters`, `f32 run_rate`.
    MoveToPosition {
        origin: Origin,
        params: MovementParameters,
        run_rate: f32,
    },
    /// `case 8`. 20 bytes: `u32 target`, `f32 desired_heading`, a 0x0C `MovementParameters`.
    ///
    /// The heading is read **before** the parameters and written **over** the
    /// `desired_heading` they carry, which is why it is hoisted out here.
    TurnToObject {
        target: ObjectId,
        desired_heading: f32,
        params: MovementParameters,
    },
    /// `case 9`. 12 bytes: a 0x0C `MovementParameters`.
    TurnToHeading { params: MovementParameters },
}

impl MovementBody {
    /// # Errors
    /// [`MessageError`] when the buffer is short or the interpreted state does not decode.
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let combined = r.u16()?;
        #[allow(clippy::cast_possible_truncation)] // both halves of a u16, not a float
        let (movement_type, motion_flags) = ((combined & 0xFF) as u8, (combined >> 8) as u8);
        let current_style = r.u16()?;
        let mut b = Self {
            movement_type,
            motion_flags,
            current_style,
            ..Self::default()
        };
        if movement_type == movement_type::INVALID {
            b.interpreted = Some(InterpretedMotionState::read(r)?);
            if motion_flags & motion_flags::STICK_TO_OBJECT != 0 {
                b.sticky_object = Some(ObjectId(r.u32()?));
            }
        } else {
            b.unhandled = r.rest().to_vec();
        }
        Ok(b)
    }

    /// Decode the `MoveTo`/`TurnTo` arm this buffer carries, if it carries one.
    ///
    /// `Ok(None)` for `movement_type == INVALID`, which carries an
    /// [`InterpretedMotionState`] instead.
    ///
    /// # Errors
    /// [`MessageError`] when the arm is short, over-long, or its type is not one of the four.
    pub fn decode_move_to(&self) -> Result<Option<MoveToArm>, MessageError> {
        if self.movement_type == movement_type::INVALID {
            return Ok(None);
        }
        let mut r = Reader::new(&self.unhandled);
        let arm = match self.movement_type {
            movement_type::MOVE_TO_OBJECT => MoveToArm::MoveToObject {
                target: ObjectId(r.u32()?),
                origin: Origin::read(&mut r)?,
                params: MovementParameters::unpack_net(&mut r, 0x1C)?,
                run_rate: r.f32()?,
            },
            movement_type::MOVE_TO_POSITION => MoveToArm::MoveToPosition {
                origin: Origin::read(&mut r)?,
                params: MovementParameters::unpack_net(&mut r, 0x1C)?,
                run_rate: r.f32()?,
            },
            movement_type::TURN_TO_OBJECT => {
                let target = ObjectId(r.u32()?);
                let desired_heading = r.f32()?;
                MoveToArm::TurnToObject {
                    target,
                    desired_heading,
                    params: MovementParameters::unpack_net(&mut r, 0x0C)?,
                }
            }
            movement_type::TURN_TO_HEADING => MoveToArm::TurnToHeading {
                params: MovementParameters::unpack_net(&mut r, 0x0C)?,
            },
            other => {
                return Err(MessageError::InvalidValue {
                    field: "MovementBody::movement_type",
                    value: u64::from(other),
                })
            }
        };
        r.expect_exhausted()?;
        Ok(Some(arm))
    }

    /// The exact inverse of [`Self::decode_move_to`]: the bytes that belong in
    /// [`Self::unhandled`]. The server needs to produce these arms, and the
    /// field orders above are the detail that drifts when the layout is written out by hand.
    #[must_use]
    pub fn encode_move_to(arm: &MoveToArm) -> Vec<u8> {
        let mut w = Writer::new();
        match *arm {
            MoveToArm::MoveToObject {
                target,
                origin,
                params,
                run_rate,
            } => {
                w.u32(target.0);
                origin.write(&mut w);
                params.pack_net(&mut w);
                w.f32(run_rate);
            }
            MoveToArm::MoveToPosition {
                origin,
                params,
                run_rate,
            } => {
                origin.write(&mut w);
                params.pack_net(&mut w);
                w.f32(run_rate);
            }
            MoveToArm::TurnToObject {
                target,
                desired_heading,
                params,
            } => {
                w.u32(target.0);
                w.f32(desired_heading);
                params.pack_net(&mut w);
            }
            MoveToArm::TurnToHeading { params } => params.pack_net(&mut w),
        }
        w.into_inner()
    }

    /// The inverse of [`Self::read`].
    ///
    /// Added for the server rebuild, which can produce the interpreted state but not the wrapper
    /// around it. Reassembling this layout on the other side would duplicate what this crate owns,
    /// and the alignment rule is exactly the detail that drifts when it is duplicated.
    ///
    /// # Errors
    /// [`MessageError::Unencodable`] when the arm and its payload disagree — `INVALID` without an
    /// interpreted state, a non-`INVALID` type carrying one, or `STICK_TO_OBJECT` disagreeing with
    /// `sticky_object`. None of those is representable, so this refuses rather than guessing.
    pub fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        let invalid = self.movement_type == movement_type::INVALID;
        if invalid != self.interpreted.is_some() {
            return Err(MessageError::Unencodable {
                field: "MovementBody::interpreted",
                reason: "an interpreted state is present exactly when movement_type is INVALID",
            });
        }
        let sticky = self.motion_flags & motion_flags::STICK_TO_OBJECT != 0;
        if invalid && sticky != self.sticky_object.is_some() {
            return Err(MessageError::Unencodable {
                field: "MovementBody::sticky_object",
                reason: "sticky_object is present exactly when the STICK_TO_OBJECT flag is set",
            });
        }
        w.u16(u16::from(self.movement_type) | (u16::from(self.motion_flags) << 8));
        w.u16(self.current_style);
        if let Some(state) = &self.interpreted {
            state.write(w)?;
            if let Some(o) = self.sticky_object {
                w.u32(o.0);
            }
        } else {
            w.bytes(&self.unhandled);
        }
        Ok(())
    }
}

/// The whole movement buffer of `0xF74C Movement_SetObjectMovement`, header included.
///
/// The physics layer peels three fields and aligns before handing the rest
/// to the movement unpack:
///
/// ```text
/// movement_ts = u16;  server_ts = u16;  autonomous = u8;  align to 4
/// ```
///
/// **The alignment origin is the blob, not the buffer.** In a `0xF74C` the buffer begins at blob
/// offset 10, so the pad after the five header bytes is one byte and not three; read it with
/// [`Reader::with_origin`] at [`MovementBuffer::BLOB_ORIGIN`], which is what
/// [`MovementSetObjectMovement::decoded_movement`] does. All **2,031** type-0 buffers of the
/// **seven**-session corpus (`fixtures/message-corpus/*/blobs.jsonl`, the whole-message opcode
/// space of 11,245 blobs) consume exactly under that rule and none does under the other. The
/// count is asserted by the client-session movement-count test.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MovementBuffer {
    pub movement_timestamp: u16,
    pub server_control_timestamp: u16,
    /// `last_move_was_autonomous`. The player **ignores** an autonomous movement event about
    /// himself: it is the server echoing what he just sent.
    pub autonomous: bool,
    pub body: MovementBody,
}

impl MovementBuffer {
    /// Where a `0xF74C`'s movement buffer starts within its blob: `[opcode][id][instance]`.
    pub const BLOB_ORIGIN: usize = 10;

    /// # Errors
    /// [`MessageError`] when the buffer is short or its body does not decode.
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let movement_timestamp = r.u16()?;
        let server_control_timestamp = r.u16()?;
        let autonomous = r.u8()? != 0;
        r.align4()?;
        Ok(Self {
            movement_timestamp,
            server_control_timestamp,
            autonomous,
            body: MovementBody::read(r)?,
        })
    }

    /// The inverse of [`Self::read`], **including the blob-relative pad**.
    ///
    /// [`Writer::align4`] pads to the next boundary of the *blob*, not of the writer's own buffer,
    /// so this is correct as long as the caller has already written the `0xF74C` header
    /// (`[opcode][id][instance]`, [`Self::BLOB_ORIGIN`] bytes) into the same writer — which puts the
    /// pad after the five header bytes at one byte, not three. That single byte is the whole reason
    /// this method exists here rather than in a caller.
    ///
    /// # Errors
    /// [`MessageError`] from the body.
    pub fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u16(self.movement_timestamp);
        w.u16(self.server_control_timestamp);
        w.u8(u8::from(self.autonomous));
        w.align4();
        self.body.write(w)
    }
}

/// The jump pack — the body of `0xF61B Movement_Jump`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct JumpPack {
    /// The power-bar level, `MIN_JUMP_EXTENT ..= 1`.
    pub extent: f32,
    pub velocity: Vec3,
    pub position: PositionWire,
    pub timestamps: MoveTimestamps,
}

impl JumpPack {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let p = Self {
            extent: r.f32()?,
            velocity: Vec3::read(r)?,
            position: PositionWire::read(r)?,
            timestamps: MoveTimestamps::read(r)?,
        };
        r.align4()?;
        Ok(p)
    }

    pub fn write(&self, w: &mut Writer) {
        w.f32(self.extent);
        self.velocity.write(w);
        self.position.write(w);
        self.timestamps.write(w);
        w.align4();
    }
}

/// The move-to-state pack — the body of `0xF61C Movement_MoveToState`.
///
/// The trailing flag byte is built as `(contact ? 1 : 0) | (longjump_mode ? 2 : 0)`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MoveToStatePack {
    pub raw_motion_state: RawMotionState,
    pub position: PositionWire,
    pub timestamps: MoveTimestamps,
    pub contact: bool,
    pub longjump_mode: bool,
}

impl MoveToStatePack {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let raw_motion_state = RawMotionState::read(r)?;
        let position = PositionWire::read(r)?;
        let timestamps = MoveTimestamps::read(r)?;
        let flags = r.u8()?;
        r.align4()?;
        Ok(Self {
            raw_motion_state,
            position,
            timestamps,
            contact: flags & 1 != 0,
            longjump_mode: flags & 2 != 0,
        })
    }

    pub fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        self.raw_motion_state.write(w)?;
        self.position.write(w);
        self.timestamps.write(w);
        w.u8(u8::from(self.contact) | (u8::from(self.longjump_mode) << 1));
        w.align4();
        Ok(())
    }
}

/// The autonomous-position pack — the body of `0xF753`. 44 bytes with a full
/// position.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AutonomousPosition {
    pub position: PositionWire,
    pub timestamps: MoveTimestamps,
    pub contact: u8,
}

impl AutonomousPosition {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let p = Self {
            position: PositionWire::read(r)?,
            timestamps: MoveTimestamps::read(r)?,
            contact: r.u8()?,
        };
        r.align4()?;
        Ok(p)
    }

    pub fn write(&self, w: &mut Writer) {
        self.position.write(w);
        self.timestamps.write(w);
        w.u8(self.contact);
        w.align4();
    }
}

/// `TurnToEventPack` — the body of `0xF649 Movement_TurnToEvent`: the heading in degrees as a
/// `float`, then the run flag as an `int32`, eight bytes, which is the client's own record for it.
/// No recorded session carries one and no send of it is known, so the round trip is the only
/// check it has.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TurnToEventPack {
    pub absolute_degrees: f32,
    pub run: i32,
}

impl TurnToEventPack {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            absolute_degrees: r.f32()?,
            run: r.i32()?,
        })
    }

    pub fn write(&self, w: &mut Writer) {
        w.f32(self.absolute_degrees);
        w.i32(self.run);
    }
}

// ---------------------------------------------------------------------------------------------
// Server → client
// ---------------------------------------------------------------------------------------------

/// `0xF748 Movement_PositionEvent`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MovementPositionEvent {
    pub id: ObjectId,
    pub position: PositionPack,
}

impl Message for MovementPositionEvent {
    const OPCODE: Opcode = Opcode::MOVEMENT_POSITION_EVENT;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            id: ObjectId(r.u32()?),
            position: PositionPack::read(r)?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.id.0);
        self.position.write(w)
    }
}

/// `0xF619 Movement_PositionAndMovementEvent`: the object id, a `PositionPack`, then the rest of
/// the blob as the movement buffer, which is carried opaquely and handed to the movement decoder,
/// as the client does. The buffer opens with the two `u16` sequence stamps and the autonomous byte
/// and then pads to a dword; a `PositionPack` is always whole dwords, so that pad is three bytes
/// here, where after `0xF74C`'s shorter header it is one. No recorded session carries one.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MovementPositionAndMovementEvent {
    pub id: ObjectId,
    pub position: PositionPack,
    /// The movement buffer, unparsed. Parse it with [`InterpretedMotionState`] if you need it.
    pub movement: Vec<u8>,
}

impl Message for MovementPositionAndMovementEvent {
    const OPCODE: Opcode = Opcode::MOVEMENT_POSITION_AND_MOVEMENT_EVENT;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            id: ObjectId(r.u32()?),
            position: PositionPack::read(r)?,
            movement: r.rest().to_vec(),
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.id.0);
        self.position.write(w)?;
        w.bytes(&self.movement);
        Ok(())
    }
}

/// `0xF74C Movement_SetObjectMovement`.
///
/// **This one has no `PositionPack`** — do not try to read one. It carries just the instance
/// sequence as a `u16` at offset 8, then the movement buffer from offset 10.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MovementSetObjectMovement {
    pub id: ObjectId,
    pub instance_sequence: u16,
    pub movement: Vec<u8>,
}

impl Message for MovementSetObjectMovement {
    const OPCODE: Opcode = Opcode::MOVEMENT_SET_OBJECT_MOVEMENT;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            id: ObjectId(r.u32()?),
            instance_sequence: r.u16()?,
            movement: r.rest().to_vec(),
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.id.0);
        w.u16(self.instance_sequence);
        w.bytes(&self.movement);
        Ok(())
    }
}

impl MovementSetObjectMovement {
    /// Decode [`Self::movement`] with the blob-relative origin its alignment needs.
    ///
    /// # Errors
    /// [`MessageError`] when the buffer does not decode or does not consume exactly.
    pub fn decoded_movement(&self) -> Result<MovementBuffer, MessageError> {
        let mut r = Reader::with_origin(&self.movement, MovementBuffer::BLOB_ORIGIN);
        let b = MovementBuffer::read(&mut r)?;
        r.expect_exhausted()?;
        Ok(b)
    }

    /// The exact inverse of [`Self::decoded_movement`]: the bytes that belong in
    /// [`Self::movement`], written at the blob-relative origin their alignment needs.
    ///
    /// # Errors
    /// [`MessageError`] when the buffer's arm and payload disagree.
    pub fn encode_movement(b: &MovementBuffer) -> Result<Vec<u8>, MessageError> {
        let mut w = Writer::with_origin(MovementBuffer::BLOB_ORIGIN);
        b.write(&mut w)?;
        Ok(w.into_inner())
    }
}

/// `0xF74E Movement_VectorUpdate`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MovementVectorUpdate {
    pub id: ObjectId,
    pub velocity: Vec3,
    pub omega: Vec3,
    pub timestamps: PhysicsEventStamp,
}

impl Message for MovementVectorUpdate {
    const OPCODE: Opcode = Opcode::MOVEMENT_VECTOR_UPDATE;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            id: ObjectId(r.u32()?),
            velocity: Vec3::read(r)?,
            omega: Vec3::read(r)?,
            timestamps: PhysicsEventStamp::read(r)?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.id.0);
        self.velocity.write(w);
        self.omega.write(w);
        self.timestamps.write(w);
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------
// Client → server (ordered game actions on the Weenie queue)
// ---------------------------------------------------------------------------------------------

/// `0xF61B Movement_Jump`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MovementJump(pub JumpPack);

impl Message for MovementJump {
    const OPCODE: Opcode = Opcode::MOVEMENT_JUMP;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self(JumpPack::read(r)?))
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        self.0.write(w);
        Ok(())
    }
}

/// `0xF7C9 Movement_Jump_NonAutonomous` — `[float extent]`, aligned to 4.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MovementJumpNonAutonomous {
    pub extent: f32,
}

impl Message for MovementJumpNonAutonomous {
    const OPCODE: Opcode = Opcode::MOVEMENT_JUMP_NON_AUTONOMOUS;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let m = Self { extent: r.f32()? };
        r.align4()?;
        Ok(m)
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.f32(self.extent);
        w.align4();
        Ok(())
    }
}

/// `0xF61C Movement_MoveToState`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MovementMoveToState(pub MoveToStatePack);

impl Message for MovementMoveToState {
    const OPCODE: Opcode = Opcode::MOVEMENT_MOVE_TO_STATE;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self(MoveToStatePack::read(r)?))
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        self.0.write(w)
    }
}

/// `0xF61E Movement_DoMovementCommand` — `[u32 motion][float speed][u32 HoldKey]`.
///
/// The community catalogue does list this opcode; it is easy to miss because the sender
/// writes it as the float bit pattern `8.82902e-41`, which *is* `0xF61E`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MovementDoMovementCommand {
    /// A full 32-bit `MotionCommand` id.
    pub motion: u32,
    pub speed: f32,
    pub hold_key: u32,
}

impl Message for MovementDoMovementCommand {
    const OPCODE: Opcode = Opcode::MOVEMENT_DO_MOVEMENT_COMMAND;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            motion: r.u32()?,
            speed: r.f32()?,
            hold_key: r.u32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.motion);
        w.f32(self.speed);
        w.u32(self.hold_key);
        Ok(())
    }
}

/// `0xF661 Movement_StopMovementCommand` — `[u32 motion][u32 HoldKey]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MovementStopMovementCommand {
    pub motion: u32,
    pub hold_key: u32,
}

impl Message for MovementStopMovementCommand {
    const OPCODE: Opcode = Opcode::MOVEMENT_STOP_MOVEMENT_COMMAND;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            motion: r.u32()?,
            hold_key: r.u32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.motion);
        w.u32(self.hold_key);
        Ok(())
    }
}

/// `0xF649 Movement_TurnToEvent`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MovementTurnToEvent(pub TurnToEventPack);

impl Message for MovementTurnToEvent {
    const OPCODE: Opcode = Opcode::MOVEMENT_TURN_TO_EVENT;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self(TurnToEventPack::read(r)?))
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        self.0.write(w);
        Ok(())
    }
}

/// `0xF752 Movement_AutonomyLevel` — `[u32 level]`, aligned to 4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MovementAutonomyLevel {
    pub level: u32,
}

impl Message for MovementAutonomyLevel {
    const OPCODE: Opcode = Opcode::MOVEMENT_AUTONOMY_LEVEL;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let m = Self { level: r.u32()? };
        r.align4()?;
        Ok(m)
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.level);
        w.align4();
        Ok(())
    }
}

/// `0xF753 Movement_AutonomousPosition` — the unprompted 1 Hz position update.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MovementAutonomousPosition(pub AutonomousPosition);

impl Message for MovementAutonomousPosition {
    const OPCODE: Opcode = Opcode::MOVEMENT_AUTONOMOUS_POSITION;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self(AutonomousPosition::read(r)?))
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        self.0.write(w);
        Ok(())
    }
}

/// The three movement-parameter size variants.
///
/// A 0x1C-byte form, a 0x0C-byte form, and anything else fails.
/// The structure is a `MoveToMovementParameters`/`TurnToMovementParameters` pair carried inside the
/// `MoveTo`/`TurnTo` movement data, not a top-level message; the discriminator is the declared size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MovementParameters {
    /// The 0x1C-byte `MoveTo` form.
    MoveTo {
        bitfield: u32,
        distance_to_object: f32,
        min_distance: f32,
        fail_distance: f32,
        speed: f32,
        walk_run_threshold: f32,
        desired_heading: f32,
    },
    /// The 0x0C-byte `TurnTo` form.
    TurnTo {
        bitfield: u32,
        speed: f32,
        desired_heading: f32,
    },
}

impl MovementParameters {
    /// The default bitfield, `0x1EE0F`, and the walk/run threshold, 15.0.
    ///
    /// ACE's `MovementParameters` defaults set `CanCharge` and use a threshold of 1.0; both are
    /// wrong. See `docs/CORRECTIONS.md`.
    pub const DEFAULT_BITFIELD: u32 = 0x0001_EE0F;
    /// The client's walk/run threshold.
    pub const DEFAULT_WALK_RUN_THRESHOLD: f32 = 15.0;

    /// Decode by declared size: 0x1C, 0x0C, or a failure. Nothing else is accepted.
    pub fn unpack_net(r: &mut Reader<'_>, size: usize) -> Result<Self, MessageError> {
        match size {
            0x1C => Ok(Self::MoveTo {
                bitfield: r.u32()?,
                distance_to_object: r.f32()?,
                min_distance: r.f32()?,
                fail_distance: r.f32()?,
                speed: r.f32()?,
                walk_run_threshold: r.f32()?,
                desired_heading: r.f32()?,
            }),
            0x0C => Ok(Self::TurnTo {
                bitfield: r.u32()?,
                speed: r.f32()?,
                desired_heading: r.f32()?,
            }),
            other => Err(MessageError::InvalidValue {
                field: "movement-parameter wire size",
                value: other as u64,
            }),
        }
    }

    pub fn pack_net(&self, w: &mut Writer) {
        match *self {
            Self::MoveTo {
                bitfield,
                distance_to_object,
                min_distance,
                fail_distance,
                speed,
                walk_run_threshold,
                desired_heading,
            } => {
                w.u32(bitfield);
                w.f32(distance_to_object);
                w.f32(min_distance);
                w.f32(fail_distance);
                w.f32(speed);
                w.f32(walk_run_threshold);
                w.f32(desired_heading);
            }
            Self::TurnTo {
                bitfield,
                speed,
                desired_heading,
            } => {
                w.u32(bitfield);
                w.f32(speed);
                w.f32(desired_heading);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    /// A `0xF74C` movement buffer round-trips byte for byte, **including the one-byte pad**.
    ///
    /// Oracle: the physics layer peels `u16 movement_ts`, `u16 server_ts`,
    /// `u8 autonomous`, then aligns — and the alignment origin is the **blob**, not the buffer. In a
    /// `0xF74C` the buffer starts at blob offset 10, so five header bytes land at 10..15 and the pad
    /// to 16 is **one** byte. Align to the buffer instead and you pad three, which is the single
    /// detail that drifts when this layout is reassembled by hand — the reason
    /// [`MovementBuffer::write`] exists here rather than in a caller.
    #[test]
    fn a_movement_buffer_round_trips_with_the_one_byte_blob_relative_pad() {
        let buf = MovementBuffer {
            movement_timestamp: 1,
            server_control_timestamp: 0,
            autonomous: true,
            body: MovementBody {
                movement_type: movement_type::INVALID,
                motion_flags: 0,
                current_style: 0x3D,
                interpreted: Some(InterpretedMotionState::default()),
                sticky_object: None,
                unhandled: Vec::new(),
            },
        };

        let bytes = MovementSetObjectMovement::encode_movement(&buf).expect("encodes");
        // 5 header bytes + 1 pad, then the body.
        assert_eq!(bytes[5], 0, "the pad byte is a zero");
        assert_eq!(&bytes[..5], &[1, 0, 0, 0, 1], "u16, u16, u8");
        assert_eq!(
            bytes.len() % 4,
            2,
            "blob origin 10 means the body starts at blob offset 16"
        );

        let mut r = Reader::with_origin(&bytes, MovementBuffer::BLOB_ORIGIN);
        let back = MovementBuffer::read(&mut r).expect("decodes");
        r.expect_exhausted().expect("consumes exactly");
        assert_eq!(back, buf);
    }

    /// The arm and its payload must agree, or the buffer is not representable.
    #[test]
    fn a_movement_body_refuses_an_arm_that_disagrees_with_its_payload() {
        let mut b = MovementBody {
            movement_type: movement_type::INVALID,
            interpreted: None,
            ..MovementBody::default()
        };
        let mut w = Writer::with_origin(MovementBuffer::BLOB_ORIGIN);
        assert!(matches!(
            b.write(&mut w),
            Err(MessageError::Unencodable { .. })
        ));

        b.interpreted = Some(InterpretedMotionState::default());
        b.motion_flags = motion_flags::STICK_TO_OBJECT;
        let mut w = Writer::with_origin(MovementBuffer::BLOB_ORIGIN);
        assert!(
            matches!(b.write(&mut w), Err(MessageError::Unencodable { .. })),
            "STICK_TO_OBJECT without a sticky_object"
        );
    }

    use super::*;
    use crate::{round_trip, write_body};

    /// Oracle: `docs/networking/messages/05-movement.md` §1.1 — a *set* bit means the quaternion
    /// component is **omitted**.
    #[test]
    fn position_pack_omission_bits_are_inverted() {
        use position_flags as f;
        // All four set: no quaternion floats on the wire at all, and the defaults come back.
        let p = PositionPack {
            flags: f::ORIENTATION_HAS_NO_W
                | f::ORIENTATION_HAS_NO_X
                | f::ORIENTATION_HAS_NO_Y
                | f::ORIENTATION_HAS_NO_Z,
            origin: Origin {
                objcell_id: 0x00A9_0125,
                ..Origin::default()
            },
            orientation: Quat {
                w: 1.0,
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            ..PositionPack::default()
        };
        let mut w = Writer::new();
        p.write(&mut w).unwrap();
        // 4 flags + 16 origin + 8 timestamps = 28, with no quaternion.
        assert_eq!(w.len(), 28);
        let bytes = w.into_inner();
        let mut r = Reader::new(&bytes);
        assert_eq!(PositionPack::read(&mut r).unwrap(), p);
        r.expect_exhausted().unwrap();

        // No bits set: all four floats present.
        let p = PositionPack {
            flags: 0,
            orientation: Quat {
                w: 0.7,
                x: 0.0,
                y: 0.0,
                z: 0.7,
            },
            ..PositionPack::default()
        };
        let mut w = Writer::new();
        p.write(&mut w).unwrap();
        assert_eq!(w.len(), 44, "four floats present");
        let bytes = w.into_inner();
        let mut r = Reader::new(&bytes);
        assert_eq!(PositionPack::read(&mut r).unwrap(), p);
    }

    /// The failure mode the inversion causes: reading the bits the natural way round would take the
    /// 28-byte body above and try to read four floats that are not there.
    #[test]
    fn reading_the_omission_bits_the_wrong_way_round_would_overrun() {
        use position_flags as f;
        let p = PositionPack {
            flags: f::ORIENTATION_HAS_NO_W
                | f::ORIENTATION_HAS_NO_X
                | f::ORIENTATION_HAS_NO_Y
                | f::ORIENTATION_HAS_NO_Z,
            ..PositionPack::default()
        };
        let mut w = Writer::new();
        p.write(&mut w).unwrap();
        // 28 bytes. A reader that treats "set" as "present" would want 44.
        assert_eq!(w.len(), 28);
    }

    /// Oracle: the raw-motion layout (`docs/networking/messages/05-movement.md` §1.2) — the four
    /// command fields are full 32-bit ids, the queued actions carry 16-bit indices.
    #[test]
    fn raw_motion_state_mixes_ids_and_indices() {
        let s = RawMotionState {
            current_style: Some(0x4000_0001),
            forward_command: Some(0x4500_0005),
            forward_speed: Some(1.5),
            actions: vec![MotionAction {
                command_index: 12,
                stamp_and_autonomy: 0x8003,
                speed: 1.0,
            }],
            ..RawMotionState::default()
        };
        let mut w = Writer::new();
        s.write(&mut w).unwrap();
        // flags(4) + style(4) + forward(4) + speed(4) + one action(8) = 24
        assert_eq!(w.len(), 24);
        let flags = u32::from_le_bytes(w.as_slice()[0..4].try_into().unwrap());
        assert_eq!(flags >> 11, 1, "the action count lives in bits 11..15");
        let bytes = w.into_inner();
        let mut r = Reader::new(&bytes);
        let back = RawMotionState::read(&mut r).unwrap();
        assert_eq!(back, s);
        r.expect_exhausted().unwrap();
        assert_eq!(back.actions[0].stamp(), 3);
        assert!(back.actions[0].autonomous());
    }

    /// Oracle: the interpreted-motion layout (`docs/networking/messages/05-movement.md` §1.2) —
    /// indices everywhere, the action count at bits 7–11, and the four indices before the three
    /// speeds.
    #[test]
    fn interpreted_motion_state_uses_indices_everywhere() {
        let s = InterpretedMotionState {
            current_style: Some(1),
            forward_command: Some(2),
            forward_speed: Some(1.5),
            actions: vec![MotionAction::default()],
            ..InterpretedMotionState::default()
        };
        let mut w = Writer::new();
        s.write(&mut w).unwrap();
        // flags(4) + two indices(4) + one speed(4) + one action(8) = 20, already 4-aligned.
        assert_eq!(w.len(), 20);
        let flags = u32::from_le_bytes(w.as_slice()[0..4].try_into().unwrap());
        assert_eq!(
            (flags >> 7) & 0x1F,
            1,
            "the action count lives in bits 7..11"
        );
        let bytes = w.into_inner();
        let mut r = Reader::new(&bytes);
        assert_eq!(InterpretedMotionState::read(&mut r).unwrap(), s);
        r.expect_exhausted().unwrap();
    }

    /// An odd number of `u16` indices makes the trailing align to 4 fire, which is the easiest part
    /// of this pack to lose.
    #[test]
    fn interpreted_motion_state_pads_to_four() {
        let s = InterpretedMotionState {
            current_style: Some(1),
            ..InterpretedMotionState::default()
        };
        let mut w = Writer::new();
        s.write(&mut w).unwrap();
        assert_eq!(w.len(), 8, "4 flags + 2 index + 2 pad");
        let bytes = w.into_inner();
        let mut r = Reader::new(&bytes);
        assert_eq!(InterpretedMotionState::read(&mut r).unwrap(), s);
        r.expect_exhausted().unwrap();
    }

    /// Oracle: `docs/networking/messages/05-movement.md` §2 — `AutonomousPosition` is 44 bytes with
    /// a full position.
    #[test]
    fn autonomous_position_pack_is_forty_four_bytes() {
        let p = AutonomousPosition::default();
        let mut w = Writer::new();
        p.write(&mut w);
        assert_eq!(
            w.len(),
            44,
            "32 position + 8 timestamps + 1 contact + 3 pad"
        );
        let bytes = w.into_inner();
        let mut r = Reader::new(&bytes);
        assert_eq!(AutonomousPosition::read(&mut r).unwrap(), p);
        r.expect_exhausted().unwrap();
    }

    /// Oracle: `docs/networking/messages/05-movement.md` §3 — `0xF74C` has **no `PositionPack`**,
    /// just the instance sequence and then the movement buffer.
    #[test]
    fn set_object_movement_has_no_position_pack() {
        let m = MovementSetObjectMovement {
            id: ObjectId(0x5000_0001),
            instance_sequence: 3,
            movement: vec![1, 2, 3, 4],
        };
        let bytes = write_body(&m).unwrap();
        assert_eq!(bytes.len(), 4 + 2 + 4);
        let _: MovementSetObjectMovement = round_trip(&bytes);
    }

    /// Oracle: `docs/CORRECTIONS.md` — the default bitfield is `0x1EE0F` and the walk/run
    /// threshold 15.0, not ACE's `CanCharge` default and 1.0.
    #[test]
    fn movement_parameters_has_three_size_variants() {
        let move_to = MovementParameters::MoveTo {
            bitfield: MovementParameters::DEFAULT_BITFIELD,
            distance_to_object: 1.0,
            min_distance: 0.0,
            fail_distance: 10.0,
            speed: 1.0,
            walk_run_threshold: MovementParameters::DEFAULT_WALK_RUN_THRESHOLD,
            desired_heading: 0.0,
        };
        let mut w = Writer::new();
        move_to.pack_net(&mut w);
        assert_eq!(w.len(), 0x1C);
        let bytes = w.into_inner();
        let mut r = Reader::new(&bytes);
        assert_eq!(
            MovementParameters::unpack_net(&mut r, 0x1C).unwrap(),
            move_to
        );

        let turn_to = MovementParameters::TurnTo {
            bitfield: 0,
            speed: 1.0,
            desired_heading: 90.0,
        };
        let mut w = Writer::new();
        turn_to.pack_net(&mut w);
        assert_eq!(w.len(), 0x0C);
        let bytes = w.into_inner();
        let mut r = Reader::new(&bytes);
        assert_eq!(
            MovementParameters::unpack_net(&mut r, 0x0C).unwrap(),
            turn_to
        );

        // Any other size fails rather than guessing.
        let mut r = Reader::new(&[0u8; 32]);
        assert!(MovementParameters::unpack_net(&mut r, 0x10).is_err());
    }

    #[test]
    fn every_movement_message_round_trips() {
        let pos = PositionPack {
            flags: position_flags::IS_GROUNDED,
            origin: Origin {
                objcell_id: 0x00A9_0125,
                ..Origin::default()
            },
            ..PositionPack::default()
        };
        let _: MovementPositionEvent = round_trip(
            &write_body(&MovementPositionEvent {
                id: ObjectId(1),
                position: pos,
            })
            .unwrap(),
        );
        let _: MovementPositionAndMovementEvent = round_trip(
            &write_body(&MovementPositionAndMovementEvent {
                id: ObjectId(1),
                position: pos,
                movement: vec![0, 0, 0, 0],
            })
            .unwrap(),
        );
        let _: MovementVectorUpdate = round_trip(
            &write_body(&MovementVectorUpdate {
                id: ObjectId(1),
                velocity: Vec3::default(),
                omega: Vec3::default(),
                timestamps: PhysicsEventStamp {
                    instance: 1,
                    event: 2,
                },
            })
            .unwrap(),
        );
        let _: MovementJump = round_trip(&write_body(&MovementJump(JumpPack::default())).unwrap());
        let _: MovementJumpNonAutonomous =
            round_trip(&write_body(&MovementJumpNonAutonomous { extent: 0.5 }).unwrap());
        let _: MovementMoveToState = round_trip(
            &write_body(&MovementMoveToState(MoveToStatePack {
                contact: true,
                longjump_mode: true,
                ..MoveToStatePack::default()
            }))
            .unwrap(),
        );
        let _: MovementDoMovementCommand = round_trip(
            &write_body(&MovementDoMovementCommand {
                motion: 1,
                speed: 1.0,
                hold_key: 0,
            })
            .unwrap(),
        );
        let _: MovementStopMovementCommand = round_trip(
            &write_body(&MovementStopMovementCommand {
                motion: 1,
                hold_key: 0,
            })
            .unwrap(),
        );
        let _: MovementTurnToEvent = round_trip(
            &write_body(&MovementTurnToEvent(TurnToEventPack {
                absolute_degrees: 90.0,
                run: 1,
            }))
            .unwrap(),
        );
        let _: MovementAutonomyLevel =
            round_trip(&write_body(&MovementAutonomyLevel { level: 2 }).unwrap());
        let _: MovementAutonomousPosition = round_trip(
            &write_body(&MovementAutonomousPosition(AutonomousPosition::default())).unwrap(),
        );
    }
}
