// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Managers/InboundMessageManager.cs
//! Port of `Source/ACE.Server/Network/Managers/InboundMessageManager.cs`.
//!
//! # Shape
//!
//! ACE finds its handlers by reflection at start-up; here the two dictionaries are build-time
//! tables ([`dispatch_table`], generated from ACE's attributes).
//! Each entry points at a hand-written function in the module mirroring the ACE file.
//!
//! [`handle_client_message`] is what the world calls for each `empyrean_net::Event::Message`. As in
//! ACE, it checks the registered `SessionState` and **enqueues** the handler onto the inbound
//! message queue (ACE's `NetworkManager.InboundMessageQueue`), which the world loop drains with
//! [`run_inbound_message_queue`] (ACE: `WorldManager.UpdateWorld`). ACE checks the state
//! on the network thread when the message arrives; here the check runs on the world thread when
//! the event is handed over, against the game half of the session ([`crate::sessions`]).
//!
//! # Handlers and the payload
//!
//! A handler is `fn(&mut World, &mut Payload, SessionId) -> Result<(), MessageError>`. [`Payload`]
//! is ACE's `message.Payload` (a `BinaryReader` over the whole message, positioned after the
//! opcode): it keeps .NET's read semantics (a read past the end fails, a skip past the end does
//! not) and decodes typed messages through `dereth-protocol`. ACE's catch-log-continue around each
//! handler catches both an `Err` (ACE's `EndOfStreamException`) and a panic (`catch_unwind`).

use std::cell::RefCell;
use std::panic::{catch_unwind, AssertUnwindSafe};

use dereth_protocol::{Message, MessageError, Reader};
use empyrean_common::dotnet::binary_reader::{BinaryReader, ReadError};
use empyrean_net::{ClientMessage, SessionId, SessionState};

use crate::entity::actions::action_queue::{self, ActionQueue};
use crate::entity::actions::i_action::Action;
use crate::entity::actions::i_actor::Actor;
use crate::network::game_action::game_action_attribute::GameActionAttribute;
use crate::network::game_messages::game_message_attribute::GameMessageAttribute;
use crate::World;

#[path = "dispatch_table.rs"]
pub mod dispatch_table;

/// What a handler returns: `Err` stands for the exception a `BinaryReader` throws on a short
/// payload, which ACE's catch-log-continue swallows.
pub type HandlerResult = Result<(), MessageError>;

/// ACE `InboundMessageManager.MessageHandler` and `ActionHandler`: `(ClientMessage, Session)`.
pub type Handler = fn(&mut World, &mut Payload<'_>, SessionId) -> HandlerResult;

// ACE: InboundMessageManager.MessageHandlerInfo
/// One `[GameMessage]` registration.
#[derive(Debug, Clone, Copy)]
pub struct MessageHandlerInfo {
    /// The `GameMessageOpcode` member's name, for logs.
    pub name: &'static str,
    /// `Handler`.
    pub handler: Handler,
    /// `Attribute`.
    pub attribute: GameMessageAttribute,
}

// ACE: InboundMessageManager.ActionHandlerInfo
/// One `[GameAction]` registration.
#[derive(Debug, Clone, Copy)]
pub struct ActionHandlerInfo {
    /// The `GameActionType` member's name, for logs.
    pub name: &'static str,
    /// `Handler`.
    pub handler: Handler,
    /// `Attribute`.
    pub attribute: GameActionAttribute,
}

/// `messageHandlers.TryGetValue(opcode)`.
pub fn message_handler(opcode: u32) -> Option<&'static MessageHandlerInfo> {
    let t = dispatch_table::MESSAGE_HANDLERS;
    t.binary_search_by_key(&opcode, |h| h.attribute.opcode)
        .ok()
        .map(|i| &t[i])
}

/// `actionHandlers.TryGetValue(opcode)`, then the game actions ACE has no handler for.
pub fn action_handler(opcode: u32) -> Option<&'static ActionHandlerInfo> {
    let t = dispatch_table::ACTION_HANDLERS;
    t.binary_search_by_key(&opcode, |h| h.attribute.opcode)
        .ok()
        .map(|i| &t[i])
        .or_else(|| {
            EMPYREAN_ACTION_HANDLERS
                .iter()
                .find(|h| h.attribute.opcode == opcode)
        })
}

/// Game actions ACE never dispatches, which Empyrean handles.
pub static EMPYREAN_ACTION_HANDLERS: &[ActionHandlerInfo] = &[
    // The early clients' spell research test (V432).
    ActionHandlerInfo {
        name: "TestSpellFormula",
        handler: crate::network::game_action::actions::game_action_magic_test_spell_formula::handle,
        attribute: crate::network::game_action::game_action_attribute::GameActionAttribute::new(
            0x004B,
        ),
    },
];

/// The inbound queue and counters. Lives in `World.sessions` (see [`crate::sessions::Sessions`]).
#[derive(Debug, Default)]
pub struct InboundMessageManagerState {
    /// ACE `NetworkManager.InboundMessageQueue`: an `ActionQueue` drained once per world update
    /// ([`run_inbound_message_queue`]); its actor is `Actor::InboundMessageQueue`.
    pub inbound_message_queue: ActionQueue,
    /// Not ACE: messages whose opcode has no handler (ACE only logs a warning).
    pub unhandled_messages: u64,
    /// Not ACE: game actions whose opcode has no handler (ACE only logs a warning).
    pub unhandled_actions: u64,
    /// Not ACE: handler invocations that failed and were caught and logged.
    pub handler_exceptions: u64,
    /// Not ACE: per-handler timing, off (`None`) unless a profiler turns it on (the soak does).
    /// When on, every handler invocation appends its kind, name and duration in seconds (on the
    /// performance monitor's clock); the reader drains it. Off, [`invoke`] only tests the option.
    pub handler_timing: Option<Vec<HandlerTime>>,
}

/// Not ACE: one timed handler invocation (`kind` is "GameMessage" or "GameAction"; a GameAction's
/// time is inside its `GameAction` message's).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HandlerTime {
    pub kind: &'static str,
    pub name: &'static str,
    pub seconds: f64,
}

/// `NetworkManager.InboundMessageQueue.RunActions()`: `ActionQueue.RunActions` on the inbound
/// queue. Work enqueued while it runs waits for the next call (ACE's `Queue.Count` snapshot); an
/// action's `NextAct` is routed through the shared `enqueue`.
pub fn run_inbound_message_queue(w: &mut World) {
    action_queue::run_actions(w, Actor::InboundMessageQueue);
}

/// `session.Player` inside a handler. The dispatch guards make it non-null; if it is null anyway,
/// this panics as ACE's `NullReferenceException` would, and the catch-log-continue catches it.
pub fn session_player(w: &World, session: SessionId) -> empyrean_entity::ObjectGuid {
    w.sessions.player(session).expect("session.Player is null")
}

// ACE: InboundMessageManager.Initialize
/// The tables are built at compile time; nothing to do. Kept so the start-up sequence reads as
/// ACE's.
pub fn initialize() {}

// ACE: InboundMessageManager.HandleClientMessage
pub fn handle_client_message(w: &mut World, message: ClientMessage, session: SessionId) {
    let opcode = message.opcode;

    if let Some(message_handler_info) = message_handler(opcode).copied() {
        let state = w.sessions.get(session).map(|s| s.state);
        if state == Some(message_handler_info.attribute.state) {
            w.sessions
                .inbound
                .inbound_message_queue
                .enqueue_action(Action::delegate(move |w: &mut World| {
                    // It's possible that before this work is executed by WorldManager, and after it
                    // was enqueued here, the session.Player was set to null.
                    if message_handler_info.attribute.state == SessionState::WorldConnected
                        && w.sessions.player(session).is_none()
                    {
                        return;
                    }

                    let mut payload = Payload::new(&message.data);
                    invoke(
                        w,
                        "GameMessage",
                        opcode,
                        message_handler_info.name,
                        session,
                        |w| (message_handler_info.handler)(w, &mut payload, session),
                    );
                }));
        }
    } else {
        log::warn!("Received unhandled fragment opcode: 0x{opcode:04X} - {opcode}");
        w.sessions.inbound.unhandled_messages += 1;
    }
}

// ACE: InboundMessageManager.HandleGameAction
/// Called by `GameActionPacket.HandleGameAction`, inside the queued message handler.
pub fn handle_game_action(
    w: &mut World,
    opcode: u32,
    message: &mut Payload<'_>,
    session: SessionId,
) {
    if let Some(action_handler_info) = action_handler(opcode).copied() {
        // It's possible that before this work is executed by WorldManager, and after it was
        // enqueued here, the session.Player was set to null.
        if w.sessions.player(session).is_none() {
            return;
        }

        invoke(
            w,
            "GameAction",
            opcode,
            action_handler_info.name,
            session,
            |w| (action_handler_info.handler)(w, message, session),
        );
    } else {
        log::warn!("Received unhandled GameActionType: 0x{opcode:04X} - {opcode}");
        w.sessions.inbound.unhandled_actions += 1;
    }
}

/// ACE's `try { handler.Invoke(message, session); } catch (Exception ex) { log.Error(...) }`: runs
/// `f`, and logs and counts an `Err` or a panic instead of propagating it.
pub fn invoke(
    w: &mut World,
    kind: &'static str,
    opcode: u32,
    name: &'static str,
    session: SessionId,
    f: impl FnOnce(&mut World) -> HandlerResult,
) {
    // Not ACE: the flag-gated handler timing
    let start = w
        .sessions
        .inbound
        .handler_timing
        .is_some()
        .then(|| crate::managers::server_performance_monitor::stopwatch_start(w));
    let outcome = catch_unwind(AssertUnwindSafe(|| f(&mut *w)));
    if let Some(start) = start {
        let seconds =
            crate::managers::server_performance_monitor::stopwatch_elapsed_seconds(w, start);
        if let Some(times) = w.sessions.inbound.handler_timing.as_mut() {
            times.push(HandlerTime {
                kind,
                name,
                seconds,
            });
        }
    }
    let error = match outcome {
        Ok(Ok(())) => return,
        Ok(Err(e)) => e.to_string(),
        Err(panic) => panic
            .downcast_ref::<&str>()
            .map(|s| (*s).to_owned())
            .or_else(|| panic.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "panic".to_owned()),
    };
    let (account_id, account, player) = match w.sessions.get(session) {
        Some(s) => (
            s.account_id,
            s.account.clone().unwrap_or_default(),
            s.player,
        ),
        None => (0, String::new(), None),
    };
    log::error!(
        "Received {kind} packet that threw an exception from account: {account_id}:{account}, player: {player:?}, opcode: 0x{opcode:04X}:{name}"
    );
    log::error!("{error}");
    w.sessions.inbound.handler_exceptions += 1;
}

// ---- the payload reader ------------------------------------------------------------------------

/// ACE `ClientMessage.Payload`: a `BinaryReader` over the whole message (opcode included),
/// positioned after the opcode.
///
/// The reads are the shared [`BinaryReader`] (`empyrean_common::dotnet`), the one implementation of
/// .NET's semantics: a fixed-size read past the end fails (`EndOfStreamException`, here
/// `MessageError::UnexpectedEof`); `ReadBytes` and `ReadChars` return what is there; `Skip` and
/// `Align` move the position and may leave it past the end, which only the next read notices.
/// This wrapper adds the error mapping, the test trace and typed decoding through `dereth-protocol`
/// ([`Payload::decode`]) with the alignment origin at the blob's start, as ACE's `Align` uses the
/// stream position.
#[derive(Debug)]
pub struct Payload<'a> {
    reader: BinaryReader<'a>,
}

/// A shared-reader error as the handlers' `MessageError`.
fn message_error(e: ReadError) -> MessageError {
    match e {
        ReadError::EndOfStream {
            at,
            needed,
            available,
        } => MessageError::UnexpectedEof {
            at,
            needed,
            available,
        },
        ReadError::OutputBufferTooSmall { at } => MessageError::InvalidValue {
            field: "ReadChars (ArgumentException: the output char buffer is too small)",
            value: at as u64,
        },
        ReadError::NegativeCount { at } => MessageError::InvalidValue {
            field: "ReadChars (ArgumentOutOfRangeException: negative count)",
            value: at as u64,
        },
    }
}

impl<'a> Payload<'a> {
    /// A payload over a complete message, positioned after its opcode (ACE's constructor reads it).
    pub fn new(data: &'a [u8]) -> Self {
        let mut reader = BinaryReader::new(data);
        reader.set_position(4.min(data.len()));
        Self { reader }
    }

    /// `BaseStream.Position`.
    pub fn position(&self) -> usize {
        self.reader.position()
    }

    /// `BaseStream.Length - BaseStream.Position` (0 when past the end).
    pub fn remaining(&self) -> usize {
        self.reader.remaining()
    }

    /// `ReadByte`.
    pub fn read_byte(&mut self) -> Result<u8, MessageError> {
        let v = self.reader.read_byte().map_err(message_error)?;
        trace(Read::U8(v));
        Ok(v)
    }

    /// `ReadUInt16`.
    pub fn read_u16(&mut self) -> Result<u16, MessageError> {
        let v = self.reader.read_u16().map_err(message_error)?;
        trace(Read::U16(v));
        Ok(v)
    }

    /// `ReadUInt32`.
    pub fn read_u32(&mut self) -> Result<u32, MessageError> {
        let v = self.reader.read_u32().map_err(message_error)?;
        trace(Read::U32(v));
        Ok(v)
    }

    /// `ReadInt32`.
    pub fn read_i32(&mut self) -> Result<i32, MessageError> {
        let v = self.reader.read_i32().map_err(message_error)?;
        trace(Read::I32(v));
        Ok(v)
    }

    /// `ReadSingle`.
    pub fn read_single(&mut self) -> Result<f32, MessageError> {
        let v = self.reader.read_f32().map_err(message_error)?;
        trace(Read::F32(v));
        Ok(v)
    }

    /// `ReadDouble`.
    pub fn read_double(&mut self) -> Result<f64, MessageError> {
        let v = self.reader.read_f64().map_err(message_error)?;
        trace(Read::F64(v));
        Ok(v)
    }

    /// `ReadBytes(count)`: .NET returns what is left when fewer than `count` remain.
    pub fn read_bytes(&mut self, count: usize) -> Vec<u8> {
        let v = self.reader.read_bytes(count).to_vec();
        trace(Read::Bytes(v.clone()));
        v
    }

    /// ACE.Common `BinaryReaderExtensions.Skip`: `BaseStream.Position += length`, never failing.
    pub fn skip(&mut self, length: usize) {
        self.reader.skip(length);
    }

    /// ACE `Extensions.Align(BinaryReader)`: pad the position to a multiple of 4.
    pub fn align(&mut self) {
        self.skip(self.position().wrapping_neg() & 3);
    }

    /// ACE.Common `BinaryReaderExtensions.ReadString16L`, the shared port
    /// ([`BinaryReader::read_string16l`]): the client's packed string as the client writes it
    /// (V232), a byte count of Windows-1252 bytes, then a skip to the next multiple of 4 of the
    /// position.
    pub fn read_string16l(&mut self) -> Result<String, MessageError> {
        let s = self.reader.read_string16l().map_err(message_error)?;
        trace(Read::Str(s.clone()));
        Ok(s)
    }

    /// ACE `PackableList.ReadListUInt32`: a `u32` count, then that many `u32`s.
    pub fn read_list_u32(&mut self) -> Result<Vec<u32>, MessageError> {
        let count = self.read_u32()?;
        let mut list = Vec::new();
        for _ in 0..count {
            list.push(self.read_u32()?);
        }
        Ok(list)
    }

    /// Decodes a `dereth-protocol` message body at the current position, without requiring it to
    /// consume the rest of the payload (a `BinaryReader` never checks for trailing bytes).
    pub fn decode<M: Message + std::fmt::Debug>(&mut self) -> Result<M, MessageError> {
        self.decode_with(M::read)
    }

    /// [`Payload::decode`] for a record that ends with a packed string.
    ///
    /// [`Payload::read_string16l`] skips a string's padding even past the end of the message, as
    /// every skip here may; `dereth-protocol` requires the padding bytes. So when the message stops
    /// inside the last string's padding, the record is read over the message completed to its
    /// next multiple of 4, and taken only if it reads the same whether those bytes are all zero or
    /// all `0xFF`: then they were padding, which nothing reads, and the result is the reader's.
    /// Any other short message fails as it does through the reader.
    pub fn decode_padded<M: Message + PartialEq + std::fmt::Debug>(
        &mut self,
    ) -> Result<M, MessageError> {
        let at = self.position();
        let rest = self.reader.rest();
        let mut r = Reader::with_origin(rest, at);
        let (v, used) = match M::read(&mut r) {
            Ok(v) => (v, r.position()),
            Err(e) => {
                let pad = (at + rest.len()).wrapping_neg() & 3;
                if pad == 0 {
                    return Err(e);
                }
                let completed = |fill: u8| {
                    let mut buf = rest.to_vec();
                    buf.resize(rest.len() + pad, fill);
                    let mut r = Reader::with_origin(&buf, at);
                    M::read(&mut r).map(|v| (v, r.position()))
                };
                match (completed(0), completed(0xFF)) {
                    (Ok((a, used)), Ok((b, _))) if a == b => (a, used),
                    _ => return Err(e),
                }
            }
        };
        self.reader.set_position(at + used);
        trace(Read::Decoded(format!("{v:?}")));
        Ok(v)
    }

    /// Decodes a `dereth-protocol` value with a reader function (for the structures `dereth-protocol`
    /// exposes as `fn read(&mut Reader)` rather than as a `Message`).
    pub fn decode_with<T: std::fmt::Debug>(
        &mut self,
        read: impl FnOnce(&mut Reader<'_>) -> Result<T, MessageError>,
    ) -> Result<T, MessageError> {
        let at = self.position();
        let mut r = Reader::with_origin(self.reader.rest(), at);
        let v = read(&mut r)?;
        self.reader.set_position(at + r.position());
        trace(Read::Decoded(format!("{v:?}")));
        Ok(v)
    }
}

/// ACE's `Convert.ToBoolean(uint)` / `Convert.ToBoolean(int)`: any non-zero value is `true`.
pub fn to_boolean(v: impl Into<i64>) -> bool {
    v.into() != 0
}

// ---- test hook ---------------------------------------------------------------------------------

/// One value a handler read from its payload, recorded while a [`start_trace`] is active.
#[derive(Debug, Clone, PartialEq)]
pub enum Read {
    U8(u8),
    U16(u16),
    U32(u32),
    I32(i32),
    F32(f32),
    F64(f64),
    Str(String),
    Bytes(Vec<u8>),
    /// A `dereth-protocol` decode, as its `Debug` form.
    Decoded(String),
}

thread_local! {
    static TRACE: RefCell<Option<Vec<Read>>> = const { RefCell::new(None) };
}

fn trace(r: Read) {
    TRACE.with(|t| {
        if let Some(v) = t.borrow_mut().as_mut() {
            v.push(r);
        }
    });
}

/// Test hook: start recording the values handlers read on this thread.
pub fn start_trace() {
    TRACE.with(|t| *t.borrow_mut() = Some(Vec::new()));
}

/// Test hook: the values read on this thread since [`start_trace`] (or the last take); recording
/// continues.
pub fn take_trace() -> Vec<Read> {
    TRACE.with(|t| {
        t.borrow_mut()
            .as_mut()
            .map(std::mem::take)
            .unwrap_or_default()
    })
}
