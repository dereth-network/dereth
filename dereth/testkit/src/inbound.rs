//! What the shard says: [`Inbound`], the `when` step that delivers a server-to-client message.
//!
//! Several sources, and the difference between them is what a scenario may assert about.
//!
//! * [`Inbound::from_corpus`] takes a **slice of a recording**. The bytes are the shard's own, so
//!   the scenario asserts about the model after the slice and never about the bytes -- the bytes
//!   are covered once and for all by the corpus property test in `dereth_client_net::client_session`, not a second time
//!   as a side effect of every scenario.
//! * [`Inbound::message`] takes a **typed message the scenario built**, for a claim no recording
//!   witnesses. It is encoded through the production writer, so the blob the client sees is the
//!   blob the codec makes.
//!
//! * [`Inbound::blobs`] takes **named blobs of a recording, in an order the scenario chooses**.
//!   `from_corpus` delivers a *range* in recorded order and [`Inbound::event`] delivers one
//!   hand-built event; what this is for is "these eight recorded blobs, in this order", which is
//!   what a scenario about two independently ordered queues needs.
//!
//! * [`Inbound::from_raw_capture`] takes a **slice of a recording's datagrams**, one layer further
//!   out than either of the two above: reassembly, ordering and the client's own connection sweep
//!   are what the scenario is driving, so the harness must not have done them for it. Scenarios
//!   whose subject is a lost fragment, a retry or an unclean logout are written in this.
//!
//! # A panel reacting to what arrives
//!
//! Every one of these is delivered through [`crate::HeadlessClient::deliver`], and on the `App`
//! backend that runs the frame's own panels callback -- so a claim that a shipped panel changed
//! when the shard said something is a claim this step can carry. With a no-op callback, every
//! panel that is a cached join of the notice stream would be blind to `Inbound`, and a scenario
//! asserting one had *not* changed would pass for the wrong reason.
//!
//! On the model backend there is no shell and the callback is honestly a no-op.
//!
//! # The blob-to-event conversion
//!
//! A recorded blob is bytes on a queue; the client's frame takes `SessionEvent`s. Converting one
//! means deciding which envelope a blob is in, and [`event_of`] is that decision, made once.

use std::ops::Range;

use dereth_client_net::client_session::testing::capture::Datagram;
use dereth_client_net::client_session::testing::{Corpus, CorpusBlob, Direction};
use dereth_client_net::client_session::SessionEvent;
use dereth_primitives::{LocalTime, ObjectId};
use dereth_protocol::Opcode;

use crate::client::{Backend, HeadlessClient, Step};

/// A server-to-client message a scenario delivers.
#[derive(Debug, Clone)]
pub enum Inbound {
    /// Every server-to-client blob of `session` whose index is in `range`, in recorded order.
    Corpus {
        session: &'static str,
        range: Range<usize>,
    },
    /// The named server-to-client blobs of `session`, in the order they are named.
    Blobs {
        session: &'static str,
        idx: Vec<usize>,
    },
    /// One event, built by the scenario.
    Event(Box<SessionEvent>),
    /// Every server-to-client **datagram** of a recording whose index is in `range`, fed through
    /// the client's real transport. See the module docs.
    RawCapture {
        records: std::sync::Arc<Vec<Datagram>>,
        range: Range<usize>,
    },
}

impl Inbound {
    /// A slice of a recording, by blob index.
    #[must_use]
    pub const fn from_corpus(session: &'static str, range: Range<usize>) -> Self {
        Self::Corpus { session, range }
    }

    /// Everything a recording carries up to `end`, which is how a scenario reaches the state a
    /// later slice depends on without replaying the logout at the recording's tail.
    #[must_use]
    pub const fn corpus_until(session: &'static str, end: usize) -> Self {
        Self::Corpus {
            session,
            range: 0..end,
        }
    }

    /// The named server-to-client blobs of a recording, **in the order they are named** rather
    /// than in recorded order.
    ///
    /// [`Self::from_corpus`] delivers a contiguous range in the order the recording has it, which
    /// is the right step when the recorded order is what the scenario is replaying. This one is
    /// for the scenario whose subject **is** the order: a message the shard puts on the UI queue
    /// and one it puts on the object queue are in two independent sequence spaces, so a client
    /// may see either first, and a claim about what the player ends up looking at has to be
    /// measured over every order the two queues permit rather than over the one the recording
    /// happened to capture.
    ///
    /// The bytes are still the shard's own and the envelope is still [`event_of`]'s, so a blob
    /// delivered here is the blob [`Self::from_corpus`] would have delivered; only the order is
    /// the scenario's.
    ///
    /// # Panics
    /// Panics when the recording is absent or does not parse, and when an index names no
    /// server-to-client blob of it -- a permutation over an index that delivers nothing would
    /// read as a pass over nothing.
    #[must_use]
    pub fn blobs(session: &'static str, idx: &[usize]) -> Self {
        Self::Blobs {
            session,
            idx: idx.to_vec(),
        }
    }

    /// One typed message, encoded through the production writer and delivered on the UI queue --
    /// which is where the ordered event envelope puts the messages a panel consumes.
    ///
    /// # Panics
    /// Panics when the message does not encode, which is a defect in the codec and not a scenario
    /// the client could be in.
    #[must_use]
    pub fn message<M: dereth_protocol::Message>(m: &M) -> Self {
        let mut w = dereth_protocol::archive::Writer::new();
        w.u32(M::OPCODE.0);
        m.write(&mut w).expect("the message encodes");
        Self::Event(Box::new(SessionEvent::UiEvent {
            opcode: M::OPCODE,
            blob: w.into_inner(),
        }))
    }

    /// One message on the object stream's own queue -- the envelope the physics and object
    /// messages arrive in, where the UI queue's ordering does not apply.
    ///
    /// # Panics
    /// Panics when the message does not encode.
    #[must_use]
    pub fn world_view<M: dereth_protocol::Message>(m: &M) -> Self {
        let body = dereth_protocol::write_body(m).expect("the message encodes");
        Self::Event(Box::new(SessionEvent::WorldObject {
            opcode: M::OPCODE,
            body,
        }))
    }

    /// Any event, for the handful a scenario must build by hand.
    #[must_use]
    pub fn event(e: SessionEvent) -> Self {
        Self::Event(Box::new(e))
    }

    /// A slice of a recording's **datagrams**, by datagram index, fed through the client's real
    /// transport rather than as decoded events.
    ///
    /// The whole recording is `0..usize::MAX`. The client must already have an endpoint -- a
    /// recorded `Given::EnteredWorld`, or one attached with [`crate::replay`] -- because the
    /// datagrams are addressed to it.
    ///
    /// # Panics
    /// Panics when the recording does not load.
    #[must_use]
    pub fn from_raw_capture(session: &str, range: Range<usize>) -> Self {
        Self::RawCapture {
            records: std::sync::Arc::new(crate::replay::records(session)),
            range,
        }
    }

    /// The same, over datagrams the scenario already has -- a recording outside the locked corpus,
    /// or one segment of one. The two unclean-logout recordings under `fixtures/packet-captures/` are read this
    /// way, because their scenario feeds each login segment on its own.
    #[must_use]
    pub fn datagrams(records: std::sync::Arc<Vec<Datagram>>, range: Range<usize>) -> Self {
        Self::RawCapture { records, range }
    }
}

impl Step for Inbound {
    fn apply(self, client: &mut HeadlessClient) {
        match self {
            Self::RawCapture { records, range } => {
                crate::replay::feed(client, &records, range);
            }
            Self::Event(e) => {
                let now = now_of(client);
                client.deliver(std::slice::from_ref(&e), now);
            }
            Self::Blobs { session, idx } => {
                let corpus = corpus_of(session);
                for i in idx {
                    let b = corpus
                        .blobs
                        .iter()
                        .find(|b| b.dir == Direction::ServerToClient && b.idx == i)
                        .unwrap_or_else(|| {
                            panic!(
                                "{session} has no server-to-client blob {i}; an order over an \
                                 index that delivers nothing would read as a pass over nothing"
                            )
                        });
                    deliver_blob(client, b);
                }
            }
            Self::Corpus { session, range } => {
                let corpus = corpus_of(session);
                let rows: Vec<&CorpusBlob> = corpus
                    .blobs
                    .iter()
                    .filter(|b| b.dir == Direction::ServerToClient && range.contains(&b.idx))
                    .collect();
                assert!(
                    !rows.is_empty(),
                    "{session} carries no server-to-client blob in {range:?}; a slice that \
                     delivers nothing would read as a pass over nothing"
                );
                for b in rows {
                    deliver_blob(client, b);
                }
            }
        }
    }
}

/// The decoded recording, or the panic that says which of the two things went wrong.
fn corpus_of(session: &str) -> Corpus {
    Corpus::load(session)
        .unwrap_or_else(|e| panic!("the recording {session} does not parse: {e}"))
        .unwrap_or_else(|| {
            panic!(
                "the decoded corpus has no scenario {session}; it is generated from the \
                 committed recordings and a missing one is a broken checkout"
            )
        })
}

/// One recorded blob, in its own envelope and at its own recorded moment.
fn deliver_blob(client: &mut HeadlessClient, b: &CorpusBlob) {
    let e = event_of(b);
    let now = LocalTime(std::time::Duration::from_micros(b.t_rel_micros).as_secs_f64());
    client.deliver(std::slice::from_ref(&e), now);
}

/// The clock a hand-built event arrives at: the client's own.
///
/// `pub(crate)` because [`crate::replay::Peer`] stamps its datagrams here too, so
/// that the connection sweep measures a silence against the same clock the scenario is running on.
pub(crate) fn now_of(client: &HeadlessClient) -> LocalTime {
    match client.backend() {
        Backend::App(app) => LocalTime(app.clock().local_time),
        Backend::Model(m) => LocalTime(m.now),
    }
}

/// Which envelope a recorded blob is in.
///
/// The ordered **event** envelope carries the object id before the sub-type, so a message inside
/// it starts four bytes further in than one inside the ordered **action** envelope. Reading the
/// action's offset on an event is a recorded mistake: it reads the stamp.
#[must_use]
pub fn event_of(b: &CorpusBlob) -> SessionEvent {
    const PLAYER_DESCRIPTION: u32 = 0x0013;
    const ORDERED_EVENT: u32 = 0xF7B0;
    const PLAYER_CREATED: u32 = 0xF746;

    let sub = |off: usize| -> Option<u32> {
        let s = b.payload.get(off..off + 4)?;
        Some(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    };
    match b.opcode {
        ORDERED_EVENT if sub(12) == Some(PLAYER_DESCRIPTION) => {
            SessionEvent::PlayerDescription(Box::new(
                dereth_protocol::read_body(&b.payload[16..])
                    .expect("the recorded description decodes"),
            ))
        }
        ORDERED_EVENT => SessionEvent::UiEvent {
            opcode: Opcode(sub(12).unwrap_or_default()),
            blob: b.payload[12..].to_vec(),
        },
        _ if b.queue == dereth_primitives::NetQueue::UiQueue => SessionEvent::UiEvent {
            opcode: Opcode(b.opcode),
            blob: b.payload.clone(),
        },
        PLAYER_CREATED => SessionEvent::PlayerCreated(ObjectId(
            sub(4).expect("a player-created blob carries its id"),
        )),
        _ => SessionEvent::WorldObject {
            opcode: Opcode(b.opcode),
            body: b.payload[4..].to_vec(),
        },
    }
}
