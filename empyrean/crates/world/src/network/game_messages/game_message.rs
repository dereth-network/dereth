// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/GameMessage.cs
//! Port of `Source/ACE.Server/Network/GameMessages/GameMessage.cs`.
//!
//! A [`GameMessage`] is ACE's `GameMessage`: an opcode, the queue it travels on, and the bytes its
//! constructor wrote. Every message class in `messages/` and every event in `game_event/events/` is
//! a builder function that returns one, having written exactly ACE's bytes in ACE's order.
//!
//! The writer is `Vec<u8>` with the [`BinaryWriter`] trait below: `System.IO.BinaryWriter`'s
//! primitive `Write` overloads (little-endian) plus ACE's `Network/Extensions.cs` extension methods,
//! which `empyrean-net` ports. Swapping to the shared `empyrean_common::dotnet` writer would be a
//! mechanical change here only.

use std::cell::RefCell;

use empyrean_entity::ObjectGuid;
use empyrean_net::extensions;
use empyrean_net::{GameMessageGroup, OutboundMessage, SessionId};

use crate::World;

use super::game_message_opcode::GameMessageOpcode;

// ACE: GameMessage
/// ACE `GameMessage`: `Opcode`, `Group` and `Data` (the opcode is its first dword, unless it is
/// `None`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameMessage {
    /// `GameMessage.Opcode`.
    pub opcode: GameMessageOpcode,
    /// `GameMessage.Group`.
    pub group: GameMessageGroup,
    /// `GameMessage.Data`: everything the constructor wrote.
    pub data: Vec<u8>,
}

impl GameMessage {
    // ACE: GameMessage.GameMessage
    /// `protected GameMessage(GameMessageOpcode opCode, GameMessageGroup group)`: writes the opcode
    /// as a `uint` unless it is `None`.
    #[must_use]
    pub fn new(op_code: GameMessageOpcode, group: GameMessageGroup) -> Self {
        Self::with_capacity(op_code, group, 0)
    }

    /// The `dataInitialCapacity` overload: the capacity is only an allocation hint in ACE, and
    /// here too. The bytes are identical.
    #[must_use]
    pub fn with_capacity(
        op_code: GameMessageOpcode,
        group: GameMessageGroup,
        data_initial_capacity: i32,
    ) -> Self {
        let mut data = Vec::with_capacity(usize::try_from(data_initial_capacity).unwrap_or(0));
        if op_code != GameMessageOpcode::None {
            data.write_u32(op_code.0);
        }
        GameMessage {
            opcode: op_code,
            group,
            data,
        }
    }

    /// A message whose body (everything after the opcode) is `body`, as dereth-protocol writes it:
    /// ACE's constructor writing the same fields in the same order. See [`GameMessage::write_proto`].
    #[must_use]
    pub fn from_proto<M: dereth_protocol::Message>(
        op_code: GameMessageOpcode,
        group: GameMessageGroup,
        body: &M,
    ) -> Self {
        debug_assert_eq!(op_code.0, M::OPCODE.0, "ACE's opcode is the body's own");
        let mut msg = Self::new(op_code, group);
        msg.write_proto(body);
        msg
    }

    /// Appends `body` as dereth-protocol writes it. The writer continues this message's bytes, so the
    /// 4-byte alignment inside the body is counted from the message's start, as ACE's
    /// `BinaryWriter` counts it. A known-type DataID takes the 2-byte form only below 0x4000, as
    /// the retail client packs it (V236; ACE's `WritePackedDwordOfKnownType` takes
    /// it up to 0x7FFF: the value read is the same). A string field must hold [`ace_str`]'s text;
    /// a string of 65,535 or more UTF-16 units must also be named to
    /// [`GameMessage::write_proto_strings`].
    ///
    /// # Panics
    /// When dereth-protocol cannot encode the body: a string that did not come through [`ace_str`], or
    /// a table of more than 65,536 entries.
    pub fn write_proto<M: dereth_protocol::Message>(&mut self, body: &M) {
        self.write_proto_strings(body, &[]);
    }

    /// [`GameMessage::write_proto`] for a body holding strings that may be 65,535 or more UTF-16
    /// units long. Such a string is written in the client's long form (0xFFFF then a dword), not
    /// ACE's wrapped 16-bit length (V241); `strings` is kept for the callers
    /// and no longer used.
    ///
    /// # Panics
    /// As [`GameMessage::write_proto`].
    pub fn write_proto_strings<M: dereth_protocol::Message>(&mut self, body: &M, strings: &[&str]) {
        write_record(&mut self.data, strings, |w| body.write(w));
    }

    /// The transport's view of this message (`Group` and `Data`), for `NetworkSession.EnqueueSend`.
    #[must_use]
    pub fn into_outbound(self) -> OutboundMessage {
        OutboundMessage {
            group: self.group,
            data: self.data,
        }
    }
}

/// The text ACE's `WriteString16L` writes for `data`, as a string dereth-protocol writes to the same
/// bytes: C#'s `null` is empty, and each UTF-16 unit Windows-1252 cannot represent is `?`.
#[must_use]
pub fn ace_str<'a>(data: impl Into<Option<&'a str>>) -> String {
    dereth_protocol::cp1252::decode(&dereth_protocol::cp1252::encode_lossy(
        data.into().unwrap_or(""),
    ))
}

/// What a dereth-protocol record's `write` returns: some cannot fail and return `()`.
pub trait RecordWritten {
    /// The outcome as a `Result`.
    ///
    /// # Errors
    /// The record's own error.
    fn into_result(self) -> Result<(), dereth_protocol::MessageError>;
}

impl RecordWritten for () {
    fn into_result(self) -> Result<(), dereth_protocol::MessageError> {
        Ok(())
    }
}

impl RecordWritten for Result<(), dereth_protocol::MessageError> {
    fn into_result(self) -> Result<(), dereth_protocol::MessageError> {
        self
    }
}

/// Appends what `write` writes with a dereth-protocol writer to `writer`, a message's bytes from its
/// start (so 4-byte alignment inside the record counts from there, as ACE's `BinaryWriter` counts
/// it). A known-type DataID is packed as the retail client packs it, the 2-byte form only below
/// 0x4000 (V236), where ACE's `WritePackedDwordOfKnownType` takes it up to 0x7FFF;
/// both read to the same value. Each of `strings` of 65,535 or more
/// UTF-16 units is rewritten to `WriteString16L`'s wrapped 16-bit length (see
/// [`GameMessage::write_proto_strings`]). This is how an ACE `Write(this BinaryWriter, ...)`
/// extension writes through dereth-protocol.
///
/// The writer **wraps** what a field cannot hold, as ACE's casts do: a hash table's `(ushort)`
/// count and size, an object description's `(byte)` counts, and `?` for a character windows-1252
/// lacks. A count it wraps is logged as a content error (V237: no content reaches one; the bytes
/// stay ACE's).
///
/// # Panics
/// When dereth-protocol still cannot encode the record (a record whose own fields disagree).
pub fn write_record<T: RecordWritten>(
    writer: &mut Vec<u8>,
    strings: &[&str],
    write: impl FnOnce(&mut dereth_protocol::Writer) -> T,
) {
    let start = writer.len();
    let mut w = dereth_protocol::Writer::from_vec(std::mem::take(writer)).with_wrapping_counts();
    // The closure's type names the code writing the record, for the log.
    let site = std::any::type_name_of_val(&write);
    write(&mut w)
        .into_result()
        .unwrap_or_else(|e| panic!("ACE's writer cannot fail here: {e}"));
    if w.counts_wrapped() > 0 {
        log::error!(
            "content error: {} count(s) too large for their field wrapped, writing {site}",
            w.counts_wrapped()
        );
    }
    *writer = w.into_inner();
    // V241: a string of 65,535+ units keeps dereth-protocol's long form (0xFFFF, then a dword), as the
    // client reads it; ACE's wrapped 16-bit length is no longer restored, so `strings` is unused.
    let _ = (start, strings);
}

/// The known-type DataID to write for `value`, a `field` of object `guid` whose type's base is
/// `known_type`: `value` itself when the retail client's packer can express it (0, "none", or at
/// most 0x3FFFFFFF above the base); the base plus `value` when `value` is a bare offset below the
/// base sharing no bit with it; else 0, logged as a content error (V237).
///
/// ACE subtracts the base only when the value shares a bit with it. A value sharing none (palette
/// 0x42 against 0x04000000) is sent as the offset the client adds the base to, so it reads
/// 0x04000042: every clothing sub-palette is such an offset, since ACE keeps only the low 16 bits
/// of the id its palette set picks (0x040012D7 is held as 0x12D7). That id is sent as the client
/// read it from ACE. A value below the base that shares a bit (icon 0x02000102 against
/// 0x06000000) wraps and was sent as a long form the client masks to another id: that one is
/// refused.
#[must_use]
pub fn known_type_did(value: u32, known_type: u32, guid: ObjectGuid, field: &str) -> u32 {
    if value == 0 || value.wrapping_sub(known_type) < 0x4000_0000 {
        return value;
    }
    // V237: a bare offset is the id ACE's client read, so it is sent as the client read it from ACE.
    if value & known_type == 0 && value < known_type {
        return known_type + value;
    }
    log::error!(
        "content error: object {guid} {field} 0x{value:08X} is not a DataID of type 0x{known_type:08X}; sent as none"
    );
    0
}

/// The one-byte sequence a property update carries (`ByteSequence.NextBytes`).
///
/// # Panics
/// When `bytes` is not one byte: every property sequence is a `ByteSequence`.
#[must_use]
pub fn byte_sequence(bytes: &[u8]) -> u8 {
    match bytes {
        [b] => *b,
        other => panic!("a property sequence is one byte, not {other:?}"),
    }
}

/// The two-byte sequence an object message carries (`UShortSequence.NextBytes` or
/// `CurrentBytes`), little-endian.
///
/// # Panics
/// When `bytes` is not two bytes: every object sequence is a `UShortSequence`.
#[must_use]
pub fn ushort_sequence(bytes: &[u8]) -> u16 {
    match bytes {
        [lo, hi] => u16::from_le_bytes([*lo, *hi]),
        other => panic!("an object sequence is two bytes, not {other:?}"),
    }
}

/// The builders empyrean-net's transport calls for the messages it sends on its own (login rejects,
/// handshake failures, shutdown, a wrong client version): the ports of `GameMessageCharacterError`
/// and `GameMessageBootAccount`. Every `ServerNet` the world drives is built with these.
#[must_use]
pub fn transport_messages() -> empyrean_net::TransportMessages {
    use crate::network::game_messages::messages::game_message_boot_account::game_message_boot_account;
    use crate::network::game_messages::messages::game_message_character_error::game_message_character_error;
    empyrean_net::TransportMessages {
        character_error: |error| game_message_character_error(error).into_outbound(),
        boot_account: |reason| game_message_boot_account(reason).into_outbound(),
    }
}

// ---- ACE `session.Network.EnqueueSend(...)` -------------------------------------------------

/// `session.Network.EnqueueSend(params GameMessage[] messages)` with one message: hands the
/// message's group and bytes to the session's transport, in call order.
pub fn enqueue_send(w: &mut World, session: SessionId, msg: GameMessage) {
    send_bytes(w, session, msg.group, msg.data);
}

/// `session.Network.EnqueueSend(params GameMessage[])` and the `IEnumerable<GameMessage>`
/// overload: each message in order.
pub fn enqueue_send_many(
    w: &mut World,
    session: SessionId,
    messages: impl IntoIterator<Item = GameMessage>,
) {
    for msg in messages {
        enqueue_send(w, session, msg);
    }
}

/// One captured send: session, queue and bytes.
pub type SentMessage = (SessionId, GameMessageGroup, Vec<u8>);

thread_local! {
    /// Opt-in copy of every send on this thread, for tests: see [`start_capture`].
    static CAPTURE: RefCell<Option<Vec<SentMessage>>> = const { RefCell::new(None) };
}

/// ACE's `session.Network.EnqueueSend`: hands the message to the world's transport. A test
/// that called [`start_capture`] on this thread also receives a copy, in send order.
fn send_bytes(w: &mut World, session: SessionId, group: GameMessageGroup, bytes: Vec<u8>) {
    CAPTURE.with(|c| {
        if let Some(v) = c.borrow_mut().as_mut() {
            v.push((session, group, bytes.clone()));
        }
    });
    w.net.send(
        session,
        empyrean_net::OutboundMessage { group, data: bytes },
    );
}

/// Starts (or restarts) capturing this thread's sends. Test support; not an ACE member.
pub fn start_capture() {
    CAPTURE.with(|c| *c.borrow_mut() = Some(Vec::new()));
}

/// Drains the capture started by [`start_capture`] (empty if none was started).
pub fn take_sent() -> Vec<SentMessage> {
    CAPTURE.with(|c| {
        c.borrow_mut()
            .as_mut()
            .map(std::mem::take)
            .unwrap_or_default()
    })
}

/// `System.IO.BinaryWriter`'s `Write` overloads and ACE's `BinaryWriter` extension methods
/// (`Source/ACE.Server/Network/Extensions.cs`, ported in `empyrean_net::extensions`), over a byte
/// vector. `BinaryWriter` writes every primitive little-endian; `Write(bool)` is one byte.
pub trait BinaryWriter {
    /// `Write(byte)`.
    fn write_u8(&mut self, v: u8);
    /// `Write(bool)`: one byte, 1 or 0.
    fn write_bool(&mut self, v: bool);
    /// `Write(ushort)`.
    fn write_u16(&mut self, v: u16);
    /// `Write(uint)`.
    fn write_u32(&mut self, v: u32);
    /// `Write(int)`.
    fn write_i32(&mut self, v: i32);
    /// `Write(ulong)`.
    fn write_u64(&mut self, v: u64);
    /// `Write(long)`.
    fn write_i64(&mut self, v: i64);
    /// `Write(float)`.
    fn write_f32(&mut self, v: f32);
    /// `Write(double)`.
    fn write_f64(&mut self, v: f64);
    /// `Write(byte[])`.
    fn write_bytes(&mut self, v: &[u8]);
    /// `WriteGuid(ObjectGuid)`: the full guid as a `uint`.
    fn write_guid(&mut self, guid: ObjectGuid);
    /// `WriteString16L(string)`: takes a `&str` or an `Option<&str>` (C#'s `null` is written as
    /// empty).
    fn write_string16l<'a>(&mut self, data: impl Into<Option<&'a str>>);
    /// `Align()`: pad the stream to a multiple of four.
    fn align(&mut self);
    /// `WritePosition(uint value, long position)`: overwrite a `uint` in place.
    fn write_position(&mut self, value: u32, position: usize);
    /// `BaseStream.Position` (always the end: nothing here seeks).
    fn stream_position(&self) -> usize;
}

impl BinaryWriter for Vec<u8> {
    fn write_u8(&mut self, v: u8) {
        self.push(v);
    }
    fn write_bool(&mut self, v: bool) {
        self.push(u8::from(v));
    }
    fn write_u16(&mut self, v: u16) {
        self.extend_from_slice(&v.to_le_bytes());
    }
    fn write_u32(&mut self, v: u32) {
        self.extend_from_slice(&v.to_le_bytes());
    }
    fn write_i32(&mut self, v: i32) {
        self.extend_from_slice(&v.to_le_bytes());
    }
    fn write_u64(&mut self, v: u64) {
        self.extend_from_slice(&v.to_le_bytes());
    }
    fn write_i64(&mut self, v: i64) {
        self.extend_from_slice(&v.to_le_bytes());
    }
    fn write_f32(&mut self, v: f32) {
        self.extend_from_slice(&v.to_le_bytes());
    }
    fn write_f64(&mut self, v: f64) {
        self.extend_from_slice(&v.to_le_bytes());
    }
    fn write_bytes(&mut self, v: &[u8]) {
        self.extend_from_slice(v);
    }
    fn write_guid(&mut self, guid: ObjectGuid) {
        extensions::write_guid(self, guid.full());
    }
    fn write_string16l<'a>(&mut self, data: impl Into<Option<&'a str>>) {
        extensions::write_string16l(self, data.into());
    }
    fn align(&mut self) {
        extensions::align(self);
    }
    fn write_position(&mut self, value: u32, position: usize) {
        extensions::write_position(self, value, position);
    }
    fn stream_position(&self) -> usize {
        self.len()
    }
}
