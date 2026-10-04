//! Turning the requests a client produced back into the opcodes they leave as, and reading the
//! client-to-server half of a recording.
//!
//! **There is no table of opcodes here.** A request is handed to the production sender over a mock
//! transport and the opcode dword is read back off the blob it wrote, so what this reports is what
//! the client would have sent and not a second transcription of the same mapping. A request whose
//! sender refuses it contributes nothing, which is the honest answer: nothing left.
//!
//! The session each request is driven through is fresh, so the ordered stamp it allocates is
//! always the first one. That is deliberate -- this module answers "which message", never "in
//! which order on the wire", and a stamp read out of here would be a fiction.
//!
//! # The ordered envelope reports its sub-type
//!
//! Every ordered game action leaves as the *same* envelope opcode, so a caller that has to tell a
//! swing from a cancel would get the same number for both. [`opcodes`] therefore reports the **sub-type** for an
//! ordered envelope and the opcode dword for everything else, which is what a caller asking
//! "which message" meant in both cases. [`ORDERED_ACTION`] is the envelope itself, for the one
//! caller that wants to know an envelope was used at all.
//!
//! # The other direction of a recording
//!
//! [`crate::Inbound::from_corpus`] delivers a recording's **server-to-client** blobs. The other
//! half is what the recording's own client sent. A claim such as *"all ten recorded gives are
//! answered by this and by nothing else"* is a count over the c2s side, and [`Outbound::recorded`]
//! is that read, once.
//!
//! **A [`Sent`] carries its `payload`**, so a scenario can read a field out of what the
//! recording's client sent -- the give's own item id, say -- through [`Sent::field`], and not only
//! count the messages.

use std::ops::Range;

use dereth_client_model::Request;
use dereth_client_net::client_session::testing::{Corpus, Direction, MockTransport};
use dereth_client_net::client_session::Session;
use dereth_primitives::NetQueue;

/// The envelope every ordered game **action** is inside -- what the client sends. The ordered
/// **event** envelope, which is what the shard sends, is a different number; see [`crate::wire`].
pub const ORDERED_ACTION: u32 = 0xF7B1;

/// Where an ordered action's sub-type sits inside its blob: `[0xF7B1][stamp][sub-type][body]`.
const ACTION_SUB_TYPE: usize = 8;

/// The dword at `off`, when the blob is that long.
fn dword(blob: &[u8], off: usize) -> Option<u32> {
    let s = blob.get(off..off + 4)?;
    Some(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

/// Which message a blob is: the ordered sub-type when it is an ordered action, and the opcode
/// dword otherwise.
#[must_use]
pub fn message_of(blob: &[u8]) -> Option<u32> {
    match dword(blob, 0)? {
        ORDERED_ACTION => dword(blob, ACTION_SUB_TYPE),
        op => Some(op),
    }
}

/// Which message every request is, in order. See [`message_of`].
#[must_use]
pub fn opcodes(requests: &[Request]) -> Vec<u32> {
    let mut out = Vec::new();
    for r in requests {
        let mut s: Session<MockTransport> = Session::new(MockTransport::new());
        if dereth_client_runtime::requests::send_request(&mut s, r) {
            out.extend(
                s.transport
                    .sent
                    .iter()
                    .filter_map(|b| message_of(&b.payload)),
            );
        }
    }
    out
}

/// Whether `requests` carries one of `opcode`, which is the question most scenarios ask.
#[must_use]
pub fn carries(requests: &[Request], opcode: u32) -> bool {
    opcodes(requests).contains(&opcode)
}

/// One blob the recording's own client sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sent {
    /// The blob's index in the recording, which is what a [`Outbound::recorded`] range is over.
    pub idx: usize,
    /// Microseconds from the start of the recording.
    pub t_rel_micros: u64,
    /// The queue it went out on.
    pub queue: NetQueue,
    /// The opcode dword, envelope and all.
    pub opcode: u32,
    /// Which message it is: the ordered sub-type when it is an ordered action, and [`Self::opcode`]
    /// otherwise. See [`message_of`].
    pub message: u32,
    /// The blob itself, envelope and all.
    ///
    /// Without it a scenario could *count* what the recording's client sent and never read a
    /// field out of one, so an oracle of the shape "all ten recorded gives are answered by a reply
    /// naming **that item**" could not be written at all. [`Sent::field`] is the read; the raw bytes are here because
    /// this struct must not decide which messages have which fields.
    pub payload: Vec<u8>,
}

impl Sent {
    /// The dword `n` fields into the blob's **body**, counting from the end of the envelope.
    ///
    /// An ordered action's body starts after `[opcode][stamp][sub-type]`; anything else's starts
    /// after its opcode dword. So `field(0)` is the first argument of the message either way,
    /// which is what a caller asking "which item was this give about" means.
    #[must_use]
    pub fn field(&self, n: usize) -> Option<u32> {
        let base = if dword(&self.payload, 0) == Some(ORDERED_ACTION) {
            ACTION_SUB_TYPE + 4
        } else {
            4
        };
        dword(&self.payload, base + n * 4)
    }
}

/// The client-to-server half of a recording.
///
/// It is a reader and not a step: a scenario does not *deliver* what a recorded
/// client sent -- that client is not this one -- it asks what that client sent so that it can
/// assert this one sends the same set and nothing else.
#[derive(Debug, Clone, Copy)]
pub struct Outbound;

impl Outbound {
    /// Every client-to-server blob of `session` whose index is in `range`, in recorded order.
    ///
    /// # Panics
    /// Panics when the recording is absent or does not parse, and when the slice carries no
    /// client-to-server blob at all -- a reader that answered an empty list would let a scenario
    /// pass over nothing, which is the whole failure mode this crate exists to remove.
    #[must_use]
    pub fn recorded(session: &str, range: Range<usize>) -> Vec<Sent> {
        let rows = Self::all(session);
        let out: Vec<Sent> = rows
            .into_iter()
            .filter(|s| range.contains(&s.idx))
            .collect();
        assert!(
            !out.is_empty(),
            "{session} carries no client-to-server blob in {range:?}; a slice that reads \
             nothing would pass over nothing"
        );
        out
    }

    /// Every client-to-server blob of `session`, in recorded order.
    ///
    /// # Panics
    /// Panics when the recording is absent, does not parse, or carries no client-to-server blob.
    #[must_use]
    pub fn all(session: &str) -> Vec<Sent> {
        let corpus = Corpus::load(session)
            .unwrap_or_else(|e| panic!("the recording {session} does not parse: {e}"))
            .unwrap_or_else(|| {
                panic!(
                    "the decoded corpus has no scenario {session}; it is generated from the \
                     committed recordings and a missing one is a broken checkout"
                )
            });
        let out: Vec<Sent> = corpus
            .blobs
            .iter()
            .filter(|b| b.dir == Direction::ClientToServer)
            .map(|b| Sent {
                idx: b.idx,
                t_rel_micros: b.t_rel_micros,
                queue: b.queue,
                opcode: b.opcode,
                message: message_of(&b.payload).unwrap_or(b.opcode),
                payload: b.payload.clone(),
            })
            .collect();
        assert!(
            !out.is_empty(),
            "{session} carries no client-to-server blob at all; the corpus is the oracle here \
             and an empty one is a broken checkout"
        );
        out
    }

    /// Whether `session` decodes to any client-to-server blob at all.
    ///
    /// [`Self::all`] panics on a recording that does not, and the panic is right -- a reader that
    /// answered an empty list would let a scenario pass over nothing. This is the way to **ask**
    /// without risking it. One recording of the locked corpus is such a one: it decodes to a single blob, so a census walking every recording has to skip
    /// it and say so rather than die on it.
    ///
    /// # Panics
    /// Panics when the recording is absent or does not parse -- which is a broken checkout and
    /// not something a scenario may be asked about.
    #[must_use]
    pub fn has_client_half(session: &str) -> bool {
        let corpus = Corpus::load(session)
            .unwrap_or_else(|e| panic!("the recording {session} does not parse: {e}"))
            .unwrap_or_else(|| {
                panic!(
                    "the decoded corpus has no scenario {session}; it is generated from the \
                     committed recordings and a missing one is a broken checkout"
                )
            });
        corpus
            .blobs
            .iter()
            .any(|b| b.dir == Direction::ClientToServer)
    }

    /// How many of `message` the recording's own client sent, over the whole recording.
    ///
    /// # Panics
    /// As [`Self::all`].
    #[must_use]
    pub fn count(session: &str, message: u32) -> usize {
        Self::all(session)
            .iter()
            .filter(|s| s.message == message)
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_primitives::ObjectId;

    /// The sender is the oracle, not a table here: a request whose opcode this module reports is a
    /// request the production sender actually wrote out.
    #[test]
    fn an_opcode_is_read_off_the_blob_the_production_sender_wrote() {
        let r = Request::ForceObjdesc(dereth_protocol::objects::ObjectSendForceObjdesc {
            id: ObjectId(0x8000_09DE),
        });
        let mut s: Session<MockTransport> = Session::new(MockTransport::new());
        assert!(dereth_client_runtime::requests::send_request(&mut s, &r));
        let direct = s.transport.sent_opcodes();
        assert_eq!(
            opcodes(std::slice::from_ref(&r)),
            direct,
            "and it is the same answer"
        );
        assert_eq!(direct.len(), 1, "one request, one blob");
        assert!(carries(std::slice::from_ref(&r), direct[0]));
    }

    #[test]
    fn no_requests_is_no_opcodes_and_not_a_panic() {
        assert!(opcodes(&[]).is_empty());
        assert!(!carries(&[], 0xF6EA));
    }

    /// Two ordered actions share one envelope opcode and differ by their sub-type, so the envelope
    /// alone cannot tell them apart: a scenario that matched on the envelope would count every
    /// ordered action as the same message. Asserted over blobs this module builds through the
    /// production writer rather than over a table.
    #[test]
    fn an_ordered_action_reports_its_sub_type_and_not_the_envelope_every_one_of_them_shares() {
        let drop = dereth_protocol::actions::pack_action(
            1,
            &dereth_protocol::items::InventoryDropItem {
                item: ObjectId(0x5000_0001),
            },
        )
        .expect("the action frames");
        let sub = dword(&drop, ACTION_SUB_TYPE).expect("the blob carries its sub-type");
        assert_eq!(
            dword(&drop, 0),
            Some(ORDERED_ACTION),
            "it is the shared envelope"
        );
        assert_eq!(
            message_of(&drop),
            Some(sub),
            "and the message is the sub-type inside it"
        );
        assert_ne!(
            sub, ORDERED_ACTION,
            "or the distinction this test is about does not exist"
        );

        // A blob that is not an ordered envelope keeps answering its own opcode dword.
        let bare = 0xF6EA_u32.to_le_bytes().to_vec();
        assert_eq!(message_of(&bare), Some(0xF6EA));
        assert_eq!(
            message_of(&[0x01, 0x02]),
            None,
            "and a blob too short to have one says so"
        );
    }
}
