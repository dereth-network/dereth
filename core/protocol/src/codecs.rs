//! Every message codec of this crate, by opcode and direction: what reads an `[opcode][body]`
//! whose type is known only at run time.
//!
//! Each codec is a [`Message`] type's `read`, keyed by its opcode and by the direction the master
//! opcode table gives it; a handful of opcodes carry a different body each way and name their
//! direction here. A body read through a codec must be consumed exactly, allowing only the
//! sender's alignment padding (at most three trailing zero bytes), the rule the client's own
//! dispatch uses ([`crate::read_body_padded`]). A codec answers the message's derived `Debug`
//! text: the crate gives every message a derived `Debug` and no other serialisation, so a reader
//! that wants fields reads that text back (its shape is the derived one: `Name { field: value }`).
//!
//! The table is built from the crate's message declarations ([`crate::for_each_message`]). The
//! capture tools and the client's plugin message stream both decode through it.

use crate::{Message, MessageError, Opcode, Reader};

/// Which way a message travels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Dir {
    /// Client to server.
    C2s,
    /// Server to client.
    S2c,
}

/// Why a codec refused a body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    /// The error's variant, and the field for the variants that name one: `UnexpectedEof`,
    /// `TrailingBytes`, `InvalidValue:<field>`, ...
    pub kind: String,
    /// The offset in the blob where the reader stood when it stopped.
    pub offset: usize,
    /// The error's full text.
    pub text: String,
}

type DecodeFn = fn(&[u8], usize) -> Result<(String, u8), Failure>;

/// One codec.
#[derive(Debug, Clone, Copy)]
pub struct Codec {
    /// `module::Type` in `dereth_protocol`.
    pub name: &'static str,
    pub opcode: u32,
    /// The direction, when the opcode's two directions carry different bodies.
    pub only: Option<Dir>,
    decode: DecodeFn,
}

impl Codec {
    /// Read `body` (the bytes after the type dword, at `origin` in its blob for the alignment
    /// rule): its derived `Debug` text and the count of trailing zero bytes taken as padding.
    ///
    /// # Errors
    /// Why the codec refused the body.
    pub fn decode(&self, body: &[u8], origin: usize) -> Result<(String, u8), Failure> {
        (self.decode)(body, origin)
    }

    /// Whether this codec reads messages travelling `dir`.
    #[must_use]
    pub fn reads(&self, dir: Dir) -> bool {
        if let Some(only) = self.only {
            return only == dir;
        }
        match Opcode(self.opcode).info().map(|i| i.direction) {
            Some(crate::Direction::S2C) => dir == Dir::S2c,
            Some(crate::Direction::C2S) => dir == Dir::C2s,
            _ => true,
        }
    }
}

fn failure(e: &MessageError, offset: usize) -> Failure {
    let kind = match e {
        MessageError::UnexpectedEof { .. } => "UnexpectedEof".to_string(),
        MessageError::TrailingBytes { .. } => "TrailingBytes".to_string(),
        MessageError::BadMagic { .. } => "BadMagic".to_string(),
        MessageError::OpcodeMismatch { .. } => "OpcodeMismatch".to_string(),
        MessageError::InvalidValue { field, .. } => format!("InvalidValue:{field}"),
        MessageError::LengthOverrun { field, .. } => format!("LengthOverrun:{field}"),
        MessageError::Unsupported { field, .. } => format!("Unsupported:{field}"),
        MessageError::Unencodable { field, .. } => format!("Unencodable:{field}"),
    };
    Failure {
        kind,
        offset,
        text: e.to_string(),
    }
}

/// Read one body at `origin` with message type `M`.
fn dec<M: Message + std::fmt::Debug>(body: &[u8], origin: usize) -> Result<(String, u8), Failure> {
    let mut r = Reader::with_origin(body, origin);
    match M::read(&mut r) {
        Ok(m) => {
            let left = r.remaining();
            let at = r.blob_offset();
            if left == 0 {
                return Ok((format!("{m:?}"), 0));
            }
            if left < 4 && r.rest().iter().all(|b| *b == 0) {
                return Ok((format!("{m:?}"), u8::try_from(left).unwrap_or(0)));
            }
            Err(failure(&MessageError::TrailingBytes { left }, at))
        }
        Err(e) => Err(failure(&e, r.blob_offset())),
    }
}

/// The server's `0xF7DE` chat-server packet: the chat library's own framing, read by the typed
/// incoming decoder rather than the pass-through envelope.
fn turbine_incoming(body: &[u8], origin: usize) -> Result<(String, u8), Failure> {
    crate::turbine::decode_incoming(body)
        .map(|p| (format!("{p:?}"), 0))
        .map_err(|e| failure(&e, origin))
}

macro_rules! c {
    ($m:ident :: $t:ident, $ty:ty, auto) => {
        Codec {
            name: concat!(stringify!($m), "::", stringify!($t)),
            opcode: <$ty as Message>::OPCODE.0,
            only: None,
            decode: dec::<$ty>,
        }
    };
    ($m:ident :: $t:ident, $ty:ty, $d:ident) => {
        Codec {
            name: concat!(stringify!($m), "::", stringify!($t)),
            opcode: <$ty as Message>::OPCODE.0,
            only: Some(Dir::$d),
            decode: dec::<$ty>,
        }
    };
}

/// Types of `dereth_protocol` deliberately not in [`REGISTRY`], with the reason. The registry test
/// requires every `Message` in the crate's source to be in one list or the other.
pub const NOT_REGISTERED: &[(&str, &str)] = &[(
    "comms::CommunicationTurbineChat",
    "the pass-through envelope accepts any body; 0xF7DE is read by turbine::SendToRoomById \
     (client to server) and turbine::decode_incoming (server to client)",
)];

/// The name of the one codec that is not a `Message` type.
pub const TURBINE_INCOMING: &str = "turbine::decode_incoming";

macro_rules! codecs {
    (messages { $( $m:ident :: $t:ident, $ty:ty, $capture:ident, $rank:tt, $fields:ident; )* }
     opaque { $( $om:ident :: $ot:ident, $oty:ty, $orank:tt, $ofields:ident; )* }) => {
        /// Every codec, in protocol declaration order, after the structured incoming envelope.
        pub static REGISTRY: &[Codec] = &[
            Codec {
                name: TURBINE_INCOMING,
                opcode: 0xF7DE,
                only: Some(Dir::S2c),
                decode: turbine_incoming,
            },
            $( c!($m::$t, $ty, $capture), )*
        ];
    };
}

crate::for_each_message!(codecs);

/// The codecs for one type in one direction.
pub fn codecs(dir: Dir, mtype: u32) -> impl Iterator<Item = &'static Codec> {
    REGISTRY
        .iter()
        .filter(move |c| c.opcode == mtype && c.reads(dir))
}

/// One whole blob read: its type and, when a codec read it, the codec and the fields' `Debug`
/// text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Read {
    /// The type the message is filed under: the wrapped type of an ordered game event or action,
    /// else the blob's first dword.
    pub mtype: u32,
    /// The ordered-event header's object (the event's recipient), for a wrapped game event.
    pub recipient: Option<u32>,
    /// The codec that read it (`module::Type`) and its `Debug` text; `None` when no codec reads
    /// this type this way, or the one that does refused the body.
    pub fields: Option<(&'static str, String)>,
}

fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
}

/// Read a whole blob travelling `dir`: `[opcode][body]`, where a server's game event may be
/// wrapped in the ordered-event header (`0xF7B0`: the recipient, a stamp, then the event's type)
/// and a client's game action in the ordered-action header (`0xF7B1`: a stamp, then the action's
/// type). `None` for a blob shorter than its type dword.
#[must_use]
pub fn read_blob(dir: Dir, blob: &[u8]) -> Option<Read> {
    let opcode = u32_at(blob, 0)?;
    let (sub, recipient, origin) = match (dir, opcode) {
        (Dir::S2c, 0xF7B0) if blob.len() >= 16 => (u32_at(blob, 12), u32_at(blob, 4), 16),
        (Dir::C2s, 0xF7B1) if blob.len() >= 12 => (u32_at(blob, 8), None, 12),
        _ => (None, None, 4),
    };
    let mtype = sub.unwrap_or(opcode);
    let body = &blob[origin..];
    let fields = codecs(dir, mtype).find_map(|c| {
        c.decode(body, origin)
            .ok()
            .map(|(debug, _padding)| (c.name, debug))
    });
    Some(Read {
        mtype,
        recipient,
        fields,
    })
}
