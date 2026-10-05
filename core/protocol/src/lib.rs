//! The Asheron's Call wire protocol's message codecs: bytes to typed messages and back, with no
//! behaviour.
//!
//! **Depends on** `dereth-primitives`. **Used by** the client's session (`dereth-client-net`), the
//! object model and game rules (`dereth-client-model`, `dereth-rules`), the client runtime, the
//! SDK, the client and its test kit, the capture tools (`dereth-pcap`, `dereth-corpus`), and the
//! server (`empyrean-common`, `empyrean-entity`, `empyrean-net`, `empyrean-world`,
//! `empyrean-testkit`).
//!
//! **Must never** hold state or talk to a network: it consumes and produces bytes, and what reacts
//! to a message is the session or the game above it.
//!
//! Every message implements [`Message`], a **paired** `read`/`write` rather than one symmetric
//! serialise function, because a function that silently does the wrong thing in one direction is
//! the most expensive bug class there is. Every `read` ends with
//! [`archive::Reader::expect_exhausted`], so a message with a trailing byte fails rather than
//! decoding: that is what catches a byte-packed message decoded as if it were aligned.
//!
//! The community message catalogue is wrong in places, and each one is reproduced at its codec with
//! a comment. The load-bearing ones are [`items::ItemUpdateStackSize`] and
//! [`trade::HouseUpdateRestrictions`] (byte-packed), [`combat::AttackerNotification`] (`percent` is
//! an `f64`), [`social::SocialFriendsUpdate`] (a list), [`social::FellowshipUpdateFellow`] (a
//! leading object id), [`trade::TradeAddToTradeRecv`] (a third dword),
//! [`trade::TradeRegisterTrade`] (an `f64` stamp), the plugin-query family (non-empty payloads) and
//! the data-patch end message (`0xF7EA` both ways).
//!
//! **Specified in** the message catalogue, `docs/networking/messages/` (indexed by its
//! `README.md`), and the shared primitives in `docs/formats/03-serialisation-primitives.md`.

#![doc(html_no_source)]

#[macro_use]
mod macros;

pub mod actions;
pub mod admin;
pub mod archive;
pub mod combat;
pub mod comms;

pub mod error;
pub mod events;
pub mod items;
pub mod login;
pub mod movement;
pub mod objects;
pub mod opcodes;
pub mod order;
pub mod property;
pub(crate) mod property_types;
pub mod qualities;
mod registry;
pub mod social;
pub mod trade;
pub mod turbine;
pub mod types;
pub mod wrap;

pub use archive::{Reader, Writer};
pub use error::MessageError;
pub use opcodes::{Direction, Opcode, OpcodeInfo, OPCODES};
pub use order::{OrderedActionHeader, OrderedEventHeader};

/// One protocol message body.
///
/// `read` is handed the body **after** the opcode dword, in a [`Reader`] whose origin is set so the
/// alignment rule works out; `write` produces the same. The opcode itself is written by whoever
/// frames the blob — [`write_blob`] for a bare message, [`actions::pack_action`] for a game action.
pub trait Message: Sized {
    /// The dword that identifies this message on the wire.
    const OPCODE: Opcode;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError>;
    fn write(&self, w: &mut Writer) -> Result<(), MessageError>;
}

/// Decode a message body, asserting that it consumes every byte.
///
/// This is the entry point every dispatcher arm uses; the `expect_exhausted` here is the round-trip
/// gate's other half.
pub fn read_body<M: Message>(body: &[u8]) -> Result<M, MessageError> {
    let mut r = Reader::body(body);
    let m = M::read(&mut r)?;
    r.expect_exhausted()?;
    Ok(m)
}

/// [`read_body`], tolerating the **sender's** alignment padding.
///
/// A message body is not necessarily a multiple of four bytes long, and both ends pad it before it
/// goes on the wire: ACE's `GameMessage` constructors end with `Writer.Align()` (see
/// `GameMessageDeleteObject`, which reserves 12 bytes for a 10-byte message), and the client's own
/// senders write "the body with 4-byte alignment after each field"
/// (`docs/networking/messages/05-movement.md`). The retail receiver is an `Archive` over the blob and
/// simply stops reading, so the extra bytes are invisible to it.
///
/// [`read_body`] would reject such a blob, because "the byte cursor lands on the end of the record"
/// is how this crate proves a decoder. This keeps that proof everywhere it applies and gives up
/// exactly three bytes of it: at most three trailing bytes, and **only if they are zero**. A layout
/// error that leaves four or more bytes over, or leaves anything non-zero, still fails.
///
/// `0xF747 Item_DeleteObject` is the case that found this: 4 + 2 bytes of content in an 8-byte
/// body, in `fixtures/packet-captures/early-inventory-and-casting.jsonl`.
///
/// # Errors
/// [`MessageError`] from the decoder, or when more than three bytes are left over.
pub fn read_body_padded<M: Message>(body: &[u8]) -> Result<M, MessageError> {
    let mut r = Reader::body(body);
    let m = M::read(&mut r)?;
    let left = r.remaining();
    if left >= 4 || r.rest().iter().any(|b| *b != 0) {
        r.expect_exhausted()?;
    }
    Ok(m)
}

/// The **double opcode check**: decode a body that has been routed to a given switch arm, verifying
/// that the body's own leading dword agrees.
///
/// Each of the client's arms runs only when its system exists and the buffer's first dword is the
/// arm's opcode, and a message whose first dword does not match the arm is dropped silently. The check is not redundant with
/// the switch: replayed blobs re-enter the blob dispatcher from
/// object-blob processing and crucial ordered-event reception.
pub fn read_checked<M: Message>(blob: &[u8]) -> Result<M, MessageError> {
    let mut r = Reader::new(blob);
    let found = r.u32()?;
    if found != M::OPCODE.0 {
        return Err(MessageError::OpcodeMismatch {
            expected: M::OPCODE.0,
            found,
        });
    }
    let m = M::read(&mut r)?;
    r.expect_exhausted()?;
    Ok(m)
}

/// Encode a message as a complete blob: `[opcode][body]`.
pub fn write_blob<M: Message>(m: &M) -> Result<Vec<u8>, MessageError> {
    let mut w = Writer::new();
    w.u32(M::OPCODE.0);
    m.write(&mut w)?;
    Ok(w.into_inner())
}

/// Encode a message body alone, with the alignment origin set as if the opcode preceded it.
pub fn write_body<M: Message>(m: &M) -> Result<Vec<u8>, MessageError> {
    let mut w = Writer::body();
    m.write(&mut w)?;
    Ok(w.into_inner())
}

/// Round-trip a message body and assert the bytes are reproduced exactly.
///
/// The helper every family's tests use, and the crate's central fidelity assertion: decoding
/// correctly is easy to fake with a lenient parser, re-encoding to the same bytes is not. It
/// lives in the crate root behind `cfg(test)` so that every family module can reach it.
#[cfg(test)]
pub(crate) fn round_trip<M: Message + std::fmt::Debug + PartialEq>(bytes: &[u8]) -> M {
    let m: M = read_body(bytes).unwrap_or_else(|e| panic!("decoding {bytes:02X?}: {e}"));
    let again = write_body(&m).unwrap_or_else(|e| panic!("re-encoding {m:?}: {e}"));
    assert_eq!(
        again, bytes,
        "re-encoding {m:?} did not reproduce the input bytes"
    );
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_checked_enforces_the_double_opcode_check() {
        // Login_ExecuteLogOff has an empty body, which makes it the cheapest probe.
        let good = write_blob(&login::LoginExecuteLogOff).unwrap();
        assert!(read_checked::<login::LoginExecuteLogOff>(&good).is_ok());

        let mut bad = good.clone();
        bad[0] = 0x00;
        assert!(matches!(
            read_checked::<login::LoginExecuteLogOff>(&bad),
            Err(MessageError::OpcodeMismatch { .. })
        ));
    }
}
