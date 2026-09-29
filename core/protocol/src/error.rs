//! The one error type for this crate.
//!
//! The observed archive reader has a *sticky silent* error latch: raising an error sets flag bit
//! 2 and subsequent reads return no data, so the remaining decode steps do nothing and a
//! half-populated object reaches the engine. See
//! `docs/formats/03-serialisation-primitives.md` §1.1 and its rebuild note 7: we deliberately do
//! **not** reproduce that. We stop at the first failure and name the field.

/// Why a message could not be decoded or encoded.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MessageError {
    /// The cursor ran off the end of the buffer.
    #[error(
        "unexpected end of buffer: needed {needed} bytes at offset {at}, {available} available"
    )]
    UnexpectedEof {
        at: usize,
        needed: usize,
        available: usize,
    },

    /// `Message::read` completed but did not consume the whole body.
    ///
    /// This is the assertion that finds a byte-packed message decoded as if it were aligned.
    #[error("{left} trailing bytes after decoding the message body")]
    TrailingBytes { left: usize },

    /// A dword that must equal a fixed constant did not.
    #[error("bad magic: expected 0x{expected:04X}, found 0x{found:08X}")]
    BadMagic { expected: u32, found: u32 },

    /// The double opcode check failed: the body's own leading dword disagrees with the switch arm
    /// it was routed to. The client drops such a message silently
    /// (`docs/networking/messages/00-dispatch-and-queues.md` "Rebuild notes").
    #[error("opcode mismatch: dispatched as 0x{expected:04X}, body says 0x{found:08X}")]
    OpcodeMismatch { expected: u32, found: u32 },

    /// A field held a value the client's own unpacker rejects.
    #[error("field `{field}` holds invalid value {value}")]
    InvalidValue { field: &'static str, value: u64 },

    /// A length prefix asked for more than the buffer can hold.
    #[error(
        "field `{field}` declares length {len}, which exceeds the {available} bytes available"
    )]
    LengthOverrun {
        field: &'static str,
        len: usize,
        available: usize,
    },

    /// A field whose layout this crate does not implement.
    ///
    /// Used only where the client's own unpacker exists but the layout of a sub-record could not be
    /// established from the sources available, and where no shipped server sends it. Failing loudly
    /// is the point: a guessed layout would desynchronise everything after it.
    #[error("`{field}` is not decoded: {note}")]
    Unsupported {
        field: &'static str,
        note: &'static str,
    },

    /// A value cannot be represented on the wire (only reachable when encoding).
    #[error("cannot encode `{field}`: {reason}")]
    Unencodable {
        field: &'static str,
        reason: &'static str,
    },
}
