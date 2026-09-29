//! `MockTransport` and the corpus replay driver.
//!
//! Every unit test builds a `MockTransport` from a corpus slice, runs `Session::tick` over the
//! scripted timeline, and asserts on the resulting state **and** on `sent`. This is the permanent
//! session test seam.
//!
//! # The corpus
//!
//! `fixtures/message-corpus/<scenario>/blobs.jsonl` is the reassembled blob stream in both
//! directions, one JSON object per line: `{idx, dir, t_rel, queue, blob_id, opcode,
//! payload_hex}`. It is generated *from* a capture, never written by hand — "a codec test whose
//! expected bytes were typed in from a document is a test of the document".
//!
//! `fixtures/packet-captures/<slug>.jsonl` holds raw *datagrams*, while this crate consumes
//! reassembled blobs. `dereth-corpus corpus` performs that derivation (with `dereth-transport`'s
//! reassembly), and its per-recording blob counts are asserted equal to the client's own before
//! it writes any.
//!
//! [`Corpus::load`] answers `Ok(None)` for a directory that is not there, because that is a real
//! state. It is the **test** that must not treat it as a passing one: with the corpus path
//! pointing at the wrong directory, the replay loop body never executes and the binary still
//! reports every test passed. So `replay_every_scenario` names its scenarios and `expect`s every
//! one of them.
//!
//! # Who originates a client-to-server blob
//!
//! `Session` is a protocol engine, not a player. It emits a blob **on its own** for exactly two
//! opcodes: `0xF657 Login_SendEnterWorld` when `0xF7DF` arrives, and `0xF7EA DDD_EndDDD` when
//! the patch exchange ends. Every other client-to-server blob in a real recording was
//! originated by the **caller** — the running client's UI, which calls [`Session::enter_world`],
//! [`Session::answer_ddd_interrogation`], [`Session::log_off`] or [`Session::send_action`].
//!
//! The corpus format never carried that, which is why a recording could not simply be pointed
//! at. [`replay`] stands in for the caller: it decodes the captured blob and calls the same
//! public method, then compares the bytes that come back — which exercises the encoder, the
//! queue map, and for `0xF653` the character id the session kept from an earlier action.
//!
//! So a captured client blob has **three** outcomes, not two: compared, driven-then-compared,
//! and [`ReplayReport::unmodelled`] — counted and named by opcode, never silently skipped.
//!
//! # The game-action envelope
//!
//! Of the corpus's 2,628 client blobs, 2,564 are the ordered game-action envelope `0xF7B1`. The
//! obstacle to comparing them is not the data: it is that
//! [`Session::send_action`] takes a **typed** `Message` while the corpus carries bytes, so
//! re-originating a captured action needs a decoder keyed on the sub-type dword. This is
//! that dispatch — [`DRIVABLE_ACTIONS`] and `send_captured_action`, generated from one list of
//! message types so the set cannot drift from the code that drives it. Each arm decodes with the
//! family's own [`Message::read`] and re-encodes through `send_action`; the `OrderedActionHeader`, the
//! stamp, the queue and the ordered flag all come from the session, which is what separates an
//! oracle from an echo.
//!
//! Two properties of the recording had to be modelled before any of it could be compared as a
//! value rather than modulo an offset, and both are already documented as real:
//!
//! * **Wire order is not send order.** A blob is reassembled when its last fragment lands, and
//!   the client coalesces, so a datagram carrying stamps 2, 4, 3 is written out in that order.
//!   [`send_order`] puts game actions back into `blob_id` order — the client's own sequence, out
//!   of the datagram header rather than out of the payload being tested.
//! * **The counter resets mid-session.** `early-inventory-and-casting` climbs to stamp 289, logs off, and starts
//!   again at 1. That is the event-counter reset from the exit-world disconnect, which the
//!   log-off execution ends in — the caller `docs/networking/messages/11-game-actions.md` records as
//!   unidentified.
//!
//! Both are handled in production code, not in the harness: the counter holds the last issued
//! value rather than the next, so the first game action carries **1** as every recorded session
//! does, and the `0xF653` arm resets it. Neither is visible to a synthetic test, because ACE
//! ignores `OrderedActionHeader.stamp_` entirely and a test that asserts increments rather than
//! values misses both.

/// The **raw** recordings, one layer below [`Corpus`]: `fixtures/packet-captures/<slug>.jsonl`, datagrams as the
/// proxy wrote them down.
pub mod capture;

/// Each time a recording's client entered the world, and as whom, read from its own blobs.
pub mod enter_world;

pub use capture::{captures_root, peer, shared_session, CaptureError, Datagram};

use crate::client_session::{Session, SessionState};
use dereth_primitives::{IncomingMessage, NetBlobId, NetQueue, RecipientId, Transport};
use dereth_protocol::admin::{AdminSendAdminRestoreCharacter, DddEndDdd, DddInterrogationResponse};
use dereth_protocol::login::{
    CharacterDeleteRequest, CharacterSendCharGenResult, LoginExecuteLogOffRequest,
    LoginSendEnterWorld, LoginSendEnterWorldRequest,
};
use dereth_protocol::objects::ObjectSendForceObjdesc;
use dereth_protocol::turbine::SendToRoomById;
use dereth_protocol::{Message, MessageError, OrderedActionHeader, Reader};
use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, PoisonError};

/// A `Transport` fed from a script, recording everything the session emits.
#[derive(Debug, Default)]
pub struct MockTransport {
    inbound: VecDeque<IncomingMessage>,
    /// Everything the session sent, in order.
    pub sent: Vec<SentBlob>,
    /// When set, the next `send` is recorded and then reported as failed, so a test can exercise
    /// the counter rollback.
    pub fail_next_send: bool,
    /// Set by `send` when `fail_next_send` was set.
    pub last_send_failed: bool,
}

/// One outbound blob as the mock saw it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentBlob {
    pub queue: NetQueue,
    pub ordered: bool,
    pub payload: Vec<u8>,
}

impl MockTransport {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Queue one server-to-client message.
    pub fn deliver(&mut self, m: IncomingMessage) {
        self.inbound.push_back(m);
    }

    /// Queue a whole blob — the opcode dword plus its body — on a queue.
    pub fn deliver_blob(&mut self, queue: NetQueue, blob: &[u8]) {
        self.deliver_blob_with_id(queue, blob, NetBlobId(0));
    }

    /// The same, with an explicit blob id so the ephemeral bit and the ordering type can be set.
    pub fn deliver_blob_with_id(&mut self, queue: NetQueue, blob: &[u8], blob_id: NetBlobId) {
        assert!(blob.len() >= 4, "a blob is at least its opcode dword");
        self.deliver(IncomingMessage {
            opcode: u32::from_le_bytes([blob[0], blob[1], blob[2], blob[3]]),
            queue,
            sender: RecipientId(0),
            blob_id,
            body: blob[4..].to_vec(),
        });
    }

    /// How many messages are still waiting to be polled.
    #[must_use]
    pub fn pending(&self) -> usize {
        self.inbound.len()
    }

    /// The payloads sent on one queue, in order.
    #[must_use]
    pub fn sent_on(&self, queue: NetQueue) -> Vec<&[u8]> {
        self.sent
            .iter()
            .filter(|s| s.queue == queue)
            .map(|s| s.payload.as_slice())
            .collect()
    }

    /// The opcodes sent, in order.
    #[must_use]
    pub fn sent_opcodes(&self) -> Vec<u32> {
        self.sent
            .iter()
            .filter(|s| s.payload.len() >= 4)
            .map(|s| u32::from_le_bytes([s.payload[0], s.payload[1], s.payload[2], s.payload[3]]))
            .collect()
    }

    /// Assert that the session sent exactly these `(queue, payload)` pairs, in order.
    ///
    /// # Errors
    /// Returns a description of the first difference.
    pub fn expect_sent(&self, expected: &[(NetQueue, Vec<u8>)]) -> Result<(), Mismatch> {
        if self.sent.len() != expected.len() {
            return Err(Mismatch {
                at: self.sent.len().min(expected.len()),
                what: format!(
                    "sent {} blobs, expected {}",
                    self.sent.len(),
                    expected.len()
                ),
            });
        }
        for (i, (s, (q, p))) in self.sent.iter().zip(expected).enumerate() {
            if s.queue != *q || &s.payload != p {
                return Err(Mismatch {
                    at: i,
                    what: format!(
                        "blob {i}: sent ({:?}, {:02X?}), expected ({q:?}, {p:02X?})",
                        s.queue, s.payload
                    ),
                });
            }
        }
        Ok(())
    }
}

/// What [`MockTransport::expect_sent`] found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mismatch {
    pub at: usize,
    pub what: String,
}

impl std::fmt::Display for Mismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.what)
    }
}

impl Transport for MockTransport {
    fn send(&mut self, queue: NetQueue, ordered: bool, payload: &[u8]) {
        self.sent.push(SentBlob {
            queue,
            ordered,
            payload: payload.to_vec(),
        });
        self.last_send_failed = self.fail_next_send;
        self.fail_next_send = false;
    }

    fn poll(&mut self) -> Option<IncomingMessage> {
        self.inbound.pop_front()
    }
}

// ---------------------------------------------------------------------------------------------
// The corpus replay driver
// ---------------------------------------------------------------------------------------------

/// One line of `blobs.jsonl`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorpusBlob {
    pub idx: usize,
    /// `"s2c"` or `"c2s"`.
    pub dir: Direction,
    /// Seconds since the capture started.
    pub t_rel_micros: u64,
    pub queue: NetQueue,
    pub blob_id: u64,
    pub opcode: u32,
    /// The whole blob: the opcode dword and its body.
    pub payload: Vec<u8>,
}

/// Which way a captured blob travelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    ServerToClient,
    ClientToServer,
}

/// One `{after_idx, state}` row of `checkpoints.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkpoint {
    pub after_idx: usize,
    pub state: SessionState,
    pub character_count: Option<usize>,
    pub world_name: Option<String>,
}

/// A loaded capture.
#[derive(Debug, Clone, Default)]
pub struct Corpus {
    pub name: String,
    pub blobs: Vec<CorpusBlob>,
    pub checkpoints: Vec<Checkpoint>,
}

/// Where the corpus lives, relative to the workspace root.
///
/// The corpus is generated from the recordings by `dereth-corpus corpus`. See the module docs.
///
/// # Pointing it somewhere else
///
/// Two environment variables override the default, **read-only** and in this order:
///
/// * `DERETH_TEST_CORPUS_ROOT` — a message-corpus directory outright, for a corpus that is not laid out
///   under a `fixtures/` root at all;
/// * `DERETH_TEST_FIXTURES` — a `fixtures/` root, whose `message-corpus` is then used.
///
/// They let the corpus gates run against a candidate corpus and against the locked one, and the
/// two result lines be compared, before the candidate is swapped in. The default path is the
/// locked, pseudonymised corpus. With neither set, the path is the default, and no test in the
/// tree sets them.
#[must_use]
pub fn corpus_root() -> PathBuf {
    if let Some(dir) = std::env::var_os("DERETH_TEST_CORPUS_ROOT") {
        return PathBuf::from(dir);
    }
    if let Some(dir) = std::env::var_os("DERETH_TEST_FIXTURES") {
        return PathBuf::from(dir).join("message-corpus");
    }
    // `CARGO_MANIFEST_DIR` is `core/client-net`.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/message-corpus")
        .to_path_buf()
}

impl Corpus {
    /// Load a scenario by its slug, or `Ok(None)` when the corpus has not been generated yet or
    /// does not carry that name.
    ///
    /// [`Corpus::name`] is the slug -- the on-disk directory name. A recording has no other name:
    /// numeric ids such as `session08` or `house2` do not resolve.
    ///
    /// # Errors
    /// Returns the parse error when a scenario directory exists but its contents are malformed —
    /// that is a real failure, not a missing fixture.
    pub fn load(scenario: &str) -> Result<Option<Self>, CorpusError> {
        let dir = corpus_root().join(scenario);
        let blobs_path = dir.join("blobs.jsonl");
        if !blobs_path.exists() {
            return Ok(None);
        }
        let text = std::fs::read_to_string(&blobs_path)
            .map_err(|e| CorpusError::Io(blobs_path.display().to_string(), e.to_string()))?;
        let mut blobs = Vec::new();
        for (n, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            blobs.push(parse_blob(line).map_err(|e| CorpusError::Parse {
                file: blobs_path.display().to_string(),
                line: n + 1,
                what: e,
            })?);
        }
        Ok(Some(Self {
            name: scenario.to_owned(),
            blobs,
            checkpoints: Vec::new(),
        }))
    }

    /// The scenario `slug`, parsed once per process and shared by every caller after the first.
    ///
    /// A test binary whose modules read the same recordings then parses each of them once, however
    /// many modules read it. The corpus is committed, so a slug it does not carry is a broken
    /// checkout or a misspelt name, and this panics rather than hand back nothing a test could read
    /// as a pass over an empty recording.
    ///
    /// # Panics
    /// When the corpus has no scenario `slug`, or it does not parse.
    #[must_use]
    pub fn shared(slug: &str) -> &'static Corpus {
        static CACHE: OnceLock<Mutex<BTreeMap<String, &'static Corpus>>> = OnceLock::new();
        let mut cache = CACHE
            .get_or_init(Mutex::default)
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(c) = cache.get(slug) {
            return c;
        }
        let c = Self::load(slug)
            .unwrap_or_else(|e| panic!("the corpus scenario {slug} does not parse: {e}"))
            .unwrap_or_else(|| {
                panic!(
                    "the netblob corpus has no scenario {slug}; the committed ones are {:?}",
                    session_names()
                )
            });
        let c: &'static Corpus = Box::leak(Box::new(c));
        cache.insert(slug.to_owned(), c);
        c
    }

    /// Every scenario the corpus index names, each through [`Corpus::shared`], in the index's own
    /// order.
    ///
    /// # Panics
    /// As [`Corpus::shared`], for any scenario the index names.
    #[must_use]
    pub fn shared_all() -> Vec<&'static Corpus> {
        session_names().iter().map(|s| Self::shared(s)).collect()
    }

    /// Every scenario the corpus index names, loaded, in the index's own order.
    ///
    /// # Panics
    /// Panics when a scenario the index names has no `blobs.jsonl`, or when one does not parse.
    /// Both are real failures: the index is generated from the recordings it lists, so a name
    /// without a directory means the fixture set is half-written, and a test that skipped it
    /// would read as a pass over a smaller corpus.
    #[must_use]
    pub fn load_all() -> Vec<Self> {
        session_names()
            .iter()
            .map(|name| {
                Self::load(name)
                    .unwrap_or_else(|e| panic!("the corpus scenario {name} does not parse: {e}"))
                    .unwrap_or_else(|| {
                        panic!(
                            "the netblob corpus is missing scenario {name}, which its own \
                             index.json names; it is generated by \
                             `cargo run -p dereth-corpus --release -- corpus` from                              `fixtures/packet-captures/<slug>.jsonl`."
                        )
                    })
            })
            .collect()
    }

    /// How many blobs this scenario carries in `dir` with `opcode`.
    ///
    /// This is the corpus's own count of itself. A test that has counted something the client
    /// produced compares it against this rather than against an integer typed into the source:
    /// the assertion keeps its strength -- the observed count must still equal the recorded one,
    /// blob for blob -- and stops reddening every time a recording is promoted into the set.
    #[must_use]
    pub fn count(&self, dir: Direction, opcode: u32) -> usize {
        self.blobs
            .iter()
            .filter(|b| b.dir == dir && b.opcode == opcode)
            .count()
    }

    /// Every client game action in this scenario, counted by its sub-type dword.
    ///
    /// The `0xF7B1` envelope is `[opcode][stamp][sub-type]`, so the sub-type is the dword at
    /// offset 8. Reading it at the *event* envelope's offset instead is an easy mistake: it reads
    /// the stamp, and turns eight `0x0062` into two.
    #[must_use]
    pub fn action_census(&self) -> BTreeMap<u32, usize> {
        let mut out = BTreeMap::new();
        for b in &self.blobs {
            if b.dir == Direction::ClientToServer && b.opcode == GAME_ACTION {
                if let Some(sub) = sub_type_at(&b.payload, ACTION_SUB_OFFSET) {
                    *out.entry(sub).or_default() += 1;
                }
            }
        }
        out
    }

    /// How many client game actions of sub-type `sub` this scenario carries.
    #[must_use]
    pub fn count_action(&self, sub: u32) -> usize {
        self.blobs
            .iter()
            .filter(|b| {
                b.dir == Direction::ClientToServer
                    && b.opcode == GAME_ACTION
                    && sub_type_at(&b.payload, ACTION_SUB_OFFSET) == Some(sub)
            })
            .count()
    }

    /// Every ordered server game event in this scenario, counted by its sub-type dword.
    ///
    /// The `0xF7B0` envelope is `[opcode][iid][stamp][sub-type]`: offset 12, not 8.
    #[must_use]
    pub fn event_census(&self) -> BTreeMap<u32, usize> {
        let mut out = BTreeMap::new();
        for b in &self.blobs {
            if b.dir == Direction::ServerToClient && b.opcode == GAME_EVENT {
                if let Some(sub) = sub_type_at(&b.payload, EVENT_SUB_OFFSET) {
                    *out.entry(sub).or_default() += 1;
                }
            }
        }
        out
    }

    /// How many ordered server game events of sub-type `sub` this scenario carries.
    #[must_use]
    pub fn count_event(&self, sub: u32) -> usize {
        self.blobs
            .iter()
            .filter(|b| {
                b.dir == Direction::ServerToClient
                    && b.opcode == GAME_EVENT
                    && sub_type_at(&b.payload, EVENT_SUB_OFFSET) == Some(sub)
            })
            .count()
    }
}

/// The ordered game-action envelope, client to server: `[opcode][stamp][sub-type]`.
pub const GAME_ACTION: u32 = 0xF7B1;

/// The ordered game-event envelope, server to client: `[opcode][iid][stamp][sub-type]`.
pub const GAME_EVENT: u32 = 0xF7B0;

/// Where the `0xF7B1` sub-type dword sits in the blob.
const ACTION_SUB_OFFSET: usize = 8;

/// Where the `0xF7B0` sub-type dword sits in the blob -- four bytes further in, past the `iid`.
const EVENT_SUB_OFFSET: usize = 12;

fn sub_type_at(payload: &[u8], off: usize) -> Option<u32> {
    let b = payload.get(off..off.checked_add(4)?)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// The locked corpus: every scenario `fixtures/message-corpus/index.json` names, in its own order.
///
/// The list is read from the index rather than written out here, so promoting a recording is one
/// regeneration of the fixture set and not an edit to every test that counts its sessions.
///
/// # Panics
/// Panics when the index is absent or names no scenario. The corpus is the oracle of every test
/// that asks for this list; an empty answer would let those tests pass over nothing.
#[must_use]
pub fn session_names() -> &'static [&'static str] {
    static NAMES: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    NAMES.get_or_init(|| session_index().iter().map(|(name, _)| *name).collect())
}

/// Every scenario as `(name, content slug)`, in the index's own order. **Both halves are the
/// slug.**
///
/// Each recording has a **content slug** -- `long-solo-play`, `house-purchase-refused`,
/// `requested-death-vitae-salvage` -- and the slug is the recording's only name (its file, its
/// `fixtures/message-corpus` directory, its index key), so the pair is the slug twice. The shape is
/// kept so existing callers compile; new code wants [`session_names`].
///
/// The names are read out of the corpus index rather than written here, for the same reason
/// [`session_names`] is: a promotion is one regeneration of the fixture set, not an edit to a list
/// in source.
///
/// # Panics
/// Panics when the index is absent, unreadable, or names no scenario -- the corpus is the oracle
/// of every test that asks for this, and an empty answer would let those tests pass over nothing.
#[must_use]
pub fn session_index() -> &'static [(&'static str, &'static str)] {
    static PAIRS: std::sync::OnceLock<Vec<(&'static str, &'static str)>> =
        std::sync::OnceLock::new();
    PAIRS.get_or_init(|| {
        let path = corpus_root().join("index.json");
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "the corpus index at {} is this list's source and cannot be read: {e}",
                path.display()
            )
        });
        // Every scenario object carries one `"name"`, and it is the slug. Scanning for it is what
        // keeps this a read of the index rather than a second copy of it.
        let mut out: Vec<(&'static str, &'static str)> = Vec::new();
        for (key, value) in string_fields(&text) {
            if key == "name" {
                let name = leak(value);
                out.push((name, name));
            }
        }
        out.dedup_by(|a, b| a.0 == b.0);
        assert!(
            !out.is_empty(),
            "{} names no scenario; the corpus index is malformed",
            path.display()
        );
        out
    })
}

/// The locked recording `name` refers to: `Some(name)` when it is the slug of a scenario the
/// corpus index names, `None` otherwise.
///
/// Slug-only since the captures reorganisation: an old id (`session08`, `house2`, ...) is a name
/// the corpus does not carry and answers `None`, the same as a name that was never recorded.
#[must_use]
pub fn resolve_session(name: &str) -> Option<&'static str> {
    session_names().iter().copied().find(|slug| *slug == name)
}

/// The content slug of a locked recording -- which is its name, so this is [`resolve_session`].
#[must_use]
pub fn session_slug(name: &str) -> Option<&'static str> {
    resolve_session(name)
}

fn leak(s: &str) -> &'static str {
    Box::leak(s.to_owned().into_boxed_str())
}

/// Every `"key": "value"` pair in a JSON text, in order, without pulling in a parser.
///
/// The corpus index is written by `dereth-corpus corpus` in its canonical JSON form, which never emits
/// an escape inside one of these names -- every slug is `[a-z0-9-]` -- so a scanner is
/// enough, and a scanner is what this file has always used. It stops at the first value that is
/// not a string, which is exactly the fields it is not looking for.
fn string_fields(text: &str) -> Vec<(&str, &str)> {
    let mut out = Vec::new();
    let b = text.as_bytes();
    let mut i = 0usize;
    while let Some(k0) = memfind(b, i, b'"') {
        let Some(k1) = memfind(b, k0 + 1, b'"') else {
            break;
        };
        let key = &text[k0 + 1..k1];
        let mut j = k1 + 1;
        while j < b.len() && (b[j] as char).is_whitespace() {
            j += 1;
        }
        if j >= b.len() || b[j] != b':' {
            i = k1 + 1;
            continue;
        }
        j += 1;
        while j < b.len() && (b[j] as char).is_whitespace() {
            j += 1;
        }
        if j < b.len() && b[j] == b'"' {
            let Some(v1) = memfind(b, j + 1, b'"') else {
                break;
            };
            out.push((key, &text[j + 1..v1]));
            i = v1 + 1;
        } else {
            i = j;
        }
    }
    out
}

fn memfind(b: &[u8], from: usize, needle: u8) -> Option<usize> {
    b.get(from..)
        .and_then(|s| s.iter().position(|c| *c == needle))
        .map(|p| p + from)
}

/// Why a corpus could not be loaded.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CorpusError {
    #[error("reading {0}: {1}")]
    Io(String, String),
    #[error("{file}:{line}: {what}")]
    Parse {
        file: String,
        line: usize,
        what: String,
    },
}

/// A deliberately small JSON-line reader.
///
/// The corpus format is fixed and flat, so a dependency-free reader avoids pulling `serde` into
/// this protocol-focused crate.
fn parse_blob(line: &str) -> Result<CorpusBlob, String> {
    let field = |name: &str| -> Option<&str> {
        let key = format!("\"{name}\"");
        let at = line.find(&key)? + key.len();
        let rest = line[at..].trim_start().strip_prefix(':')?.trim_start();
        let end = if let Some(stripped) = rest.strip_prefix('"') {
            return stripped.find('"').map(|e| &stripped[..e]);
        } else {
            rest.find([',', '}']).unwrap_or(rest.len())
        };
        Some(rest[..end].trim())
    };
    let need = |name: &str| field(name).ok_or_else(|| format!("missing field `{name}`"));
    // The corpus writes every numeric field as a plain integer; parsing through `u64`
    // keeps `blob_id` exact, which a `f64` round trip would not.
    let num = |name: &str| -> Result<u64, String> {
        need(name)?
            .parse::<u64>()
            .map_err(|_| format!("field `{name}` is not a whole number"))
    };

    let payload_hex = need("payload_hex")?;
    if payload_hex.len() % 2 != 0 {
        return Err("payload_hex has an odd length".into());
    }
    let mut payload = Vec::with_capacity(payload_hex.len() / 2);
    for i in (0..payload_hex.len()).step_by(2) {
        payload.push(
            u8::from_str_radix(&payload_hex[i..i + 2], 16)
                .map_err(|_| "payload_hex is not hexadecimal".to_string())?,
        );
    }
    let t_rel: f64 = need("t_rel")?
        .parse()
        .map_err(|_| "t_rel is not a number".to_string())?;
    if !t_rel.is_finite() || t_rel < 0.0 {
        return Err("t_rel must be a finite, non-negative number of seconds".into());
    }
    // Bounded above by the check, so the conversion cannot wrap.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let t_rel_micros = (t_rel * 1_000_000.0) as u64;

    let idx = usize::try_from(num("idx")?).map_err(|_| "idx does not fit".to_string())?;
    let queue_id =
        u8::try_from(num("queue")?).map_err(|_| "queue is not a queue id".to_string())?;
    let opcode = u32::try_from(num("opcode")?).map_err(|_| "opcode does not fit".to_string())?;
    Ok(CorpusBlob {
        idx,
        dir: match need("dir")? {
            "s2c" => Direction::ServerToClient,
            "c2s" => Direction::ClientToServer,
            other => return Err(format!("unknown direction `{other}`")),
        },
        t_rel_micros,
        queue: queue_from_id(queue_id),
        blob_id: num("blob_id")?,
        opcode,
        payload,
    })
}

/// The client's queue ids.
#[must_use]
pub fn queue_from_id(id: u8) -> NetQueue {
    match id {
        2 => NetQueue::Control,
        3 => NetQueue::Weenie,
        4 => NetQueue::Logon,
        5 => NetQueue::ClientCache,
        9 => NetQueue::UiQueue,
        10 => NetQueue::WorldObjects,
        other => NetQueue::Other(other),
    }
}

/// The wire id of a decoded recording queue, including unregistered queues.
#[must_use]
pub fn queue_id(queue: NetQueue) -> u8 {
    match queue {
        NetQueue::Control => 2,
        NetQueue::Weenie => 3,
        NetQueue::Logon => 4,
        NetQueue::ClientCache => 5,
        NetQueue::UiQueue => 9,
        NetQueue::WorldObjects => 10,
        NetQueue::Other(id) => id,
    }
}

/// A captured client blob this driver cannot re-originate, named as precisely as the wire allows.
///
/// `0xF7B1` is an **envelope**, not a message type, so counting 2,564 of them says only "game
/// action" and gives the next reader nothing to work from. Keying a game action by its sub-type
/// dword is what turns the denominator into a work list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Unmodelled {
    /// A bare blob: the opcode dword is the whole identity.
    Blob(u32),
    /// A `0xF7B1` game action, keyed by the sub-type dword at blob offset 8.
    Action(u32),
}

impl std::fmt::Display for Unmodelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Blob(op) => write!(f, "{op:#06X}"),
            Self::Action(op) => write!(f, "0xF7B1/{op:#06X}"),
        }
    }
}

/// What a replay found.
///
/// The counts are deliberately separate. `compared` and `unmodelled` answer *yes* and *not asked*
/// about the same population, and a rate with no denominator is not a measurement: a replay that
/// compared 4 of 31 client blobs prints the same `ok` as one that compared all 31.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayReport {
    /// Server-to-client blobs fed into the session.
    pub delivered: usize,
    /// Client-to-server blobs whose bytes were compared against what the session sent.
    pub compared: usize,
    /// How many of `compared` the replay had to originate on the caller's behalf. The rest the
    /// session sent by itself, which is the stronger of the two oracles.
    pub driven: usize,
    /// How many of `compared` matched the captured blob **byte for byte including the stamp**.
    ///
    /// A game action can only be exact while every earlier action in the same stamp run was
    /// driven: the retail client consumed one counter value per action, so each captured
    /// action this driver cannot originate leaves our stamp one lower than the recorded one from
    /// there on. Those are still compared -- body exactly, stamp against the known offset -- but
    /// they are not byte-identical, and saying so is cheaper than pretending either way.
    pub exact: usize,
    /// Client-to-server blobs neither the session nor this driver can originate, by
    /// [`Unmodelled`]. **The third state**: not a pass, not a failure, and never silent.
    pub unmodelled: BTreeMap<Unmodelled, usize>,
    pub mismatches: Vec<Mismatch>,
}

/// Re-originate one captured game action through [`Session::send_action`].
///
/// Generated over the message list below so that the set of sub-opcodes this harness drives and
/// the code that drives them cannot drift apart: [`DRIVABLE_ACTIONS`] is derived from the same
/// list, and `replay_every_scenario` counts the corpus with it.
macro_rules! action_drivers {
    ($($m:ty),+ $(,)?) => {
        /// Every `0xF7B1` sub-opcode [`replay`] can re-originate.
        ///
        /// A captured action whose sub-type is in here **must** be compared; one that is not is
        /// counted as [`Unmodelled::Action`] and named in the report.
        pub const DRIVABLE_ACTIONS: &[u32] = &[$(<$m as Message>::OPCODE.0),+];

        fn send_captured_action(
            session: &mut Session<MockTransport>,
            sub_type: u32,
            body: &[u8],
        ) -> Option<Result<(), MessageError>> {
            $(
                if sub_type == <$m as Message>::OPCODE.0 {
                    return Some(
                        read_action_body::<$m>(body)
                            .and_then(|m| session.send_action(&m).map(|_| ())),
                    );
                }
            )+
            None
        }
    };
}

// **All 34** game actions the recorded corpus contains, in descending order of how often they
// appear across the seven sessions. There is no thirty-fifth, and that is asserted rather than
// assumed: `replay_every_scenario` now requires `unmodelled` to be **empty**, so a corpus blob
// this list does not name is a red test and not a smaller number.
//
// `0x01A1 Character_CharacterOptionsEvent` carries a whole `PlayerModule` as its body. Its codec
// lives in `dereth-protocol` beside the `PlayerModule` that `0x0013` carries, not in this test
// harness, because a codec belongs in `dereth-protocol`; so the arm below is one line.
//
// None of this is a second copy of a sender: each arm decodes with the family's own
// `Message::read` and re-encodes through `Session::send_action`, which is the one outbound
// game-action path in this workspace. The stamp, the queue, the `ordered` flag and the `OrderedActionHeader`
// framing all come from the session rather than from the capture, which is what makes it an
// oracle rather than an echo.
action_drivers!(
    dereth_protocol::movement::MovementMoveToState, // 0xF61C, 1187
    dereth_protocol::movement::MovementAutonomousPosition, // 0xF753,  640
    dereth_protocol::items::ItemQueryItemMana,      // 0x0263,  186
    dereth_protocol::combat::CombatQueryHealth,     // 0x01BF,  116
    dereth_protocol::items::InventoryUseEvent,      // 0x0036,   79
    dereth_protocol::items::InventoryPutItemInContainer, // 0x0019,   63
    dereth_protocol::combat::MagicCastTargetedSpell, // 0x004A,   45
    dereth_protocol::combat::CombatCancelAttack,    // 0x01B7,   34
    dereth_protocol::items::InventoryGetAndWieldItem, // 0x001A,   32
    dereth_protocol::combat::CombatChangeCombatMode, // 0x0053,   31
    dereth_protocol::objects::ItemAppraise,         // 0x00C8,   24
    dereth_protocol::combat::CharacterAddSpellFavorite, // 0x01E3,   16
    dereth_protocol::combat::CombatTargetedMissileAttack, // 0x000A,   13
    dereth_protocol::social::AllegianceUpdateRequest, // 0x001F,   12
    dereth_protocol::login::CharacterLoginCompleteNotification, // 0x00A1,   11
    dereth_protocol::admin::TrainSkill,             // 0x0046,   11
    dereth_protocol::items::InventoryGiveObjectRequest, // 0x00CD,   10
    dereth_protocol::combat::CombatTargetedMeleeAttack, // 0x0008,    8
    dereth_protocol::items::InventoryUseWithTargetEvent, // 0x0035,    7
    dereth_protocol::trade::HouseQueryHouse,        // 0x021E,    5
    dereth_protocol::trade::VendorBuy,              // 0x005F,    5
    dereth_protocol::admin::TrainAttribute,         // 0x0045,    4
    dereth_protocol::admin::TrainAttribute2nd,      // 0x0044,    4
    dereth_protocol::items::InventoryDropItem,      // 0x001B,    3
    dereth_protocol::movement::MovementJump,        // 0xF61B,    2
    dereth_protocol::comms::CommunicationTalk,      // 0x0015,    2
    dereth_protocol::admin::TrainSkillAdvancementClass, // 0x0047,    2
    dereth_protocol::items::InventoryStackableSplitToContainer, // 0x0055,    2
    dereth_protocol::combat::CharacterRemoveSpellFavorite, // 0x01E4,    1
    dereth_protocol::trade::VendorSell,             // 0x0060,    1
    dereth_protocol::items::InventoryStackableMerge, // 0x0054,    1
    dereth_protocol::objects::InventoryNoLongerViewingContents, // 0x0195,    1
    dereth_protocol::combat::CharacterTeleToLifestone, // 0x0063,    1
    dereth_protocol::login::CharacterCharacterOptionsEvent, // 0x01A1,    5
    // The three `fellowship*` recordings carry sixteen client actions no other session has; the
    // retail client's own bytes are their oracle.
    dereth_protocol::login::CharacterPlayerOptionChangedEvent, // 0x0005,   11
    dereth_protocol::social::SocialRemoveFriend,               // 0x0017,    1
    dereth_protocol::social::SocialAddFriend,                  // 0x0018,    3
    dereth_protocol::social::AllegianceSwearAllegiance,        // 0x001D,    7
    dereth_protocol::social::AllegianceBreakAllegiance,        // 0x001E,    5
    dereth_protocol::comms::CommunicationModifyCharacterSquelch, // 0x0058,   3
    dereth_protocol::comms::CommunicationModifyAccountSquelch, // 0x0059,    3
    dereth_protocol::comms::CommunicationTalkDirectByName,     // 0x005D,    1
    dereth_protocol::social::FellowshipCreate,                 // 0x00A2,    2
    dereth_protocol::social::FellowshipQuitRequest,            // 0x00A3,    5
    dereth_protocol::social::FellowshipDismiss,                // 0x00A4,    1
    dereth_protocol::social::FellowshipRecruit,                // 0x00A5,    9
    dereth_protocol::social::FellowshipUpdateRequest,          // 0x00A6,    8
    dereth_protocol::comms::CharacterConfirmationResponse,     // 0x0275,    9
    dereth_protocol::social::FellowshipAssignNewLeader,        // 0x0290,    2
    dereth_protocol::social::FellowshipChangeFellowOpenness,   // 0x0291,    1
    // The two housing recordings carry twenty-two more: the whole secure-trade client half and
    // nearly the whole client-facing house command surface.
    dereth_protocol::login::CharacterAddShortCut, // 0x019C,    2
    dereth_protocol::login::CharacterRemoveShortCut, // 0x019D,    2
    dereth_protocol::trade::TradeOpenTradeNegotiations, // 0x01F6,    1
    dereth_protocol::trade::TradeCloseTradeNegotiations, // 0x01F7,    1
    dereth_protocol::trade::TradeAddToTrade,      // 0x01F8,   10
    dereth_protocol::trade::TradeAcceptTradeRequest, // 0x01FA,    2
    dereth_protocol::trade::TradeResetTradeRequest, // 0x0204,    1
    dereth_protocol::trade::HouseBuyHouse,        // 0x021C,    2
    dereth_protocol::trade::HouseRentHouse,       // 0x0221,    1
    dereth_protocol::trade::HouseAddPermanentGuest, // 0x0245,    3
    dereth_protocol::trade::HouseRemovePermanentGuest, // 0x0246,    1
    dereth_protocol::trade::HouseSetOpenHouseStatus, // 0x0247,    5
    dereth_protocol::trade::HouseChangeStoragePermission, // 0x0249,    2
    dereth_protocol::trade::HouseBootSpecificHouseGuest, // 0x024A,    3
    dereth_protocol::trade::HouseRemoveAllStoragePermission, // 0x024C,    2
    dereth_protocol::trade::HouseRequestFullGuestList, // 0x024D,    2
    dereth_protocol::trade::HouseRemoveAllPermanentGuests, // 0x025E,    2
    dereth_protocol::trade::HouseTeleToHouse,     // 0x0262,    1
    dereth_protocol::trade::HouseSetHooksVisibility, // 0x0266,    2
    dereth_protocol::trade::HouseModifyAllegianceGuestPermission, // 0x0267,  5
    dereth_protocol::trade::HouseModifyAllegianceStoragePermission, // 0x0268, 2
    dereth_protocol::trade::HouseTeleToMansion,   // 0x0278,    2
    // The `requested-death-vitae-salvage` recording carries fourteen more, in five families.
    // Without them, 581 of that recording's 864 compared client blobs match from the sub-type
    // dword on but come out **one or more short in the `OrderedActionHeader` stamp**: the retail
    // client took a counter value for each and the harness would not, so `StampTrack::skipped`
    // carries the debt forward over every action that follows until the event counter is next
    // reset to 0.
    dereth_protocol::comms::CommunicationChannelBroadcast, // 0x0147,    8
    dereth_protocol::comms::CommunicationEmote,            // 0x01DF,    1
    dereth_protocol::comms::CommunicationSoulEmote,        // 0x01E1,    1
    dereth_protocol::trade::HouseAbandonHouse,             // 0x021F,    2
    dereth_protocol::items::InventoryCreateTinkeringTool,  // 0x027D,    2
);

/// Decode a game action's payload -- the bytes after `[0xF7B1][stamp][sub-type]`.
///
/// The alignment origin is **12**, because the client's align rule is computed from the *blob's*
/// start and the body begins twelve bytes into the blob.
/// [`dereth_protocol::actions::action_body_writer`] is the same constant on the encode side, and using
/// a different one here would decode against a padding the encoder did not write.
///
/// **What actually matters is the origin modulo four**: the align rule pads by
/// `(-blob_offset) & 3`, so 12 and [`dereth_protocol::read_body`]'s origin of 4 are
/// indistinguishable and swapping them changes nothing at all. Origin 13 reddens
/// `replay_every_scenario` at once. The constant stays 12 because that is the true blob offset and
/// the writer's.
fn read_action_body<M: Message>(body: &[u8]) -> Result<M, MessageError> {
    let mut r = Reader::with_origin(body, OrderedActionHeader::PACK_SIZE + 4);
    let m = M::read(&mut r)?;
    r.expect_exhausted()?;
    Ok(m)
}

/// The corpus in the order the client **sent** it.
///
/// `blobs.jsonl` is in *arrival* order: a blob is written out when its last fragment lands, and
/// the client coalesces several blobs into one datagram, so two actions sent as stamp 3 then
/// stamp 4 can be reassembled 4 then 3. That is not a defect in the corpus and it is not loss --
/// `docs/networking/messages/11-game-actions.md` records it as anomaly 2, *"actions are written out of
/// stamp order within a single datagram"*, seen in four of the seven recorded sessions, no stamp
/// ever repeated. A receiver that treats wire order as send order rejects the third action, and a
/// **replay** that does so compares each of the three against the wrong one of its neighbours.
///
/// `blob_id` is the client's own per-queue blob sequence, taken from the *datagram* header rather
/// than from the action payload, so it states the send order without consulting the stamp this
/// harness exists to prove. Game actions are put back into that order **in the slots they already
/// occupy**, so the interleaving with the server's traffic, and every other blob's position, is
/// untouched.
fn send_order(corpus: &Corpus) -> Vec<&CorpusBlob> {
    let mut order: Vec<&CorpusBlob> = corpus.blobs.iter().collect();
    let slots: Vec<usize> = order
        .iter()
        .enumerate()
        .filter(|(_, b)| {
            b.dir == Direction::ClientToServer && b.opcode == OrderedActionHeader::MAGIC
        })
        .map(|(i, _)| i)
        .collect();
    let mut actions: Vec<&CorpusBlob> = slots.iter().map(|i| order[*i]).collect();
    actions.sort_by_key(|b| b.blob_id);
    for (slot, blob) in slots.into_iter().zip(actions) {
        order[slot] = blob;
    }
    order
}

/// The little-endian dword at `at`, when the buffer is long enough.
fn dword_at(b: &[u8], at: usize) -> Option<u32> {
    b.get(at..at + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

/// How far this session's game-action counter has fallen behind the recorded one, and why.
///
/// The retail client took one counter value per action it sent. This driver takes one per
/// action it can *re-originate*, so every captured action with no arm in [`action_drivers`] leaves
/// our stamp one lower than the recorded one — permanently, until the counter is reset. Tracking
/// that explicitly is what lets the stamp be asserted as a **value** rather than modulo a floating
/// offset, and a floating offset is what hid the off-by-one in `ActionCounter` for as long as this
/// gate has existed.
///
/// The session resets the event counter to 0 when the server's
/// `0xF653` comes back. It is observed rather than predicted: `Session::next_action_stamp` going
/// *down* can only be that, and using the session's own public reading keeps this harness from
/// having to model when it happens.
#[derive(Debug, Clone, Copy)]
struct StampTrack {
    skipped: u32,
    last_peek: u32,
}

impl StampTrack {
    fn new(peek: u32) -> Self {
        Self {
            skipped: 0,
            last_peek: peek,
        }
    }

    /// Note the session's counter after anything that could have moved it.
    fn observe(&mut self, peek: u32) {
        if peek < self.last_peek {
            // The action counter was reset: both counters start again, so the debt is cleared.
            self.skipped = 0;
        }
        self.last_peek = peek;
    }

    /// A captured action this driver could not re-originate: the recording consumed a stamp and
    /// this session did not.
    fn not_driven(&mut self) {
        self.skipped = self.skipped.saturating_add(1);
    }
}

/// Replay a capture through a session and compare each reproducible client response.
///
/// ```text
/// for each blob, in send order:
///     if dir == s2c:  deliver; tick(t_rel)
///     if dir == c2s:  originate it the way the caller would, then compare the bytes back
/// ```
///
/// A client-to-server blob has three outcomes and never two. `0xF657` and `0xF7EA` the session
/// emits by itself. `0xF7C8`, `0xF7E6`, `0xF653` and every `0xF7B1` game action whose sub-type is
/// in [`DRIVABLE_ACTIONS`] are **driven**: the captured blob is decoded and handed to the same
/// public method a UI caller would use, and what comes back is compared. Everything else is
/// counted by [`Unmodelled`] and named.
///
/// **What is compared, and what is not.** A game action's body — everything from blob offset 8 —
/// is compared exactly; so is the queue. The stamp is compared as a value against
/// [`StampTrack`]'s bookkeeping, which is an equality whenever the driver has originated every
/// action before it. Nothing is compared "structurally": the movement messages round-trip their
/// own timestamps through their own codecs, so a field this harness could not reproduce shows up
/// as a mismatch rather than as a rule.
pub fn replay(session: &mut Session<MockTransport>, corpus: &Corpus) -> ReplayReport {
    let mut report = ReplayReport {
        delivered: 0,
        compared: 0,
        driven: 0,
        exact: 0,
        unmodelled: BTreeMap::new(),
        mismatches: Vec::new(),
    };
    let mut next_sent = 0usize;
    let mut stamps = StampTrack::new(session.next_action_stamp());
    // The one caller decision the corpus records but does not carry at the moment it is made:
    // `0xF7C8` has an empty body and the character it was pressed for is named by the `0xF657`
    // that follows it. Read up front, per cycle -- see [`enter_world_decisions`].
    let decisions = enter_world_decisions(corpus);

    for blob in send_order(corpus) {
        #[allow(clippy::cast_precision_loss)]
        let now = dereth_primitives::LocalTime(blob.t_rel_micros as f64 / 1_000_000.0);
        match blob.dir {
            Direction::ServerToClient => {
                session.transport.deliver_blob_with_id(
                    blob.queue,
                    &blob.payload,
                    NetBlobId(blob.blob_id),
                );
                session.tick(now);
                report.delivered += 1;
                stamps.observe(session.next_action_stamp());
            }
            Direction::ClientToServer => {
                // Stand in for the caller. See the module docs: the session originates two
                // opcodes by itself and the client's UI originates the rest, so a recording
                // cannot be replayed without something playing the UI's part.
                let body = blob.payload.get(4..).unwrap_or_default();
                let driven = if blob.opcode == OrderedActionHeader::MAGIC {
                    // A game action. `Session::send_action` takes a typed `Message`, so the
                    // captured body is decoded with its family's own codec and handed straight
                    // back through the one outbound game-action path this workspace has. The
                    // `OrderedActionHeader`, the stamp, the queue and the ordered flag are the session's,
                    // which is what makes the comparison worth making.
                    let Some(sub) = dword_at(&blob.payload, 8) else {
                        report.mismatches.push(Mismatch {
                            at: blob.idx,
                            what: format!(
                                "blob {}: a 0xF7B1 envelope of {} bytes has no sub-type dword",
                                blob.idx,
                                blob.payload.len()
                            ),
                        });
                        continue;
                    };
                    let action_body = blob.payload.get(12..).unwrap_or_default();
                    match send_captured_action(session, sub, action_body) {
                        None => {
                            *report
                                .unmodelled
                                .entry(Unmodelled::Action(sub))
                                .or_default() += 1;
                            stamps.not_driven();
                            continue;
                        }
                        Some(Err(e)) => {
                            report.mismatches.push(Mismatch {
                                at: blob.idx,
                                what: format!(
                                    "blob {}: the captured 0xF7B1/{sub:#06X} did not round-trip \
                                     through send_action: {e}",
                                    blob.idx
                                ),
                            });
                            stamps.not_driven();
                            continue;
                        }
                        Some(Ok(())) => true,
                    }
                } else if blob.opcode == LoginSendEnterWorldRequest::OPCODE.0 {
                    // `0xF7C8` carries **no body** -- it is the bare "the player pressed
                    // ENTER". Which character was pressed for is only stated in the `0xF657`
                    // that follows, so the choice is read out of the capture ahead of time
                    // (`decisions`, above the loop, one per cycle). That is the caller's
                    // decision being stood in for; what the session then does with it --
                    // remembering the id and the account, and emitting `0xF657` when `0xF7DF`
                    // arrives -- is compared byte for byte a few blobs later, and is not fed to
                    // it.
                    let Some((character, account)) = decisions.get(&blob.idx).cloned() else {
                        report.mismatches.push(Mismatch {
                            at: blob.idx,
                            what: format!(
                                "blob {}: the capture enters the world but carries no 0xF657 \
                                 to say which character",
                                blob.idx
                            ),
                        });
                        continue;
                    };
                    session.enter_world(character, &account);
                    true
                } else if blob.opcode == DddInterrogationResponse::OPCODE.0 {
                    // Decoding the captured response and re-encoding it through the session is
                    // a round trip of the codec, not a comparison of the bytes with themselves:
                    // `answer_ddd_interrogation` takes the typed message, not a buffer.
                    match dereth_protocol::read_body::<DddInterrogationResponse>(body) {
                        Ok(m) => {
                            session.answer_ddd_interrogation(&m);
                            true
                        }
                        Err(e) => {
                            report.mismatches.push(Mismatch {
                                at: blob.idx,
                                what: format!(
                                    "blob {}: the captured 0xF7E6 does not decode: {e}",
                                    blob.idx
                                ),
                            });
                            continue;
                        }
                    }
                } else if blob.opcode == LoginExecuteLogOffRequest::OPCODE.0 {
                    // No argument is decoded: the character id has to come from the state the
                    // session kept when it entered the world, which is what makes this an
                    // oracle rather than an echo.
                    session.log_off();
                    true
                } else if blob.opcode == ObjectSendForceObjdesc::OPCODE.0 {
                    // Not a game action -- `0xF6EA` is a bare eight-byte blob on the Control
                    // queue. `Session::send_force_objdesc` exists because of these 35 recorded
                    // blobs; nothing else in the workspace emits one.
                    match dereth_protocol::read_body::<ObjectSendForceObjdesc>(body) {
                        Ok(m) => {
                            session.send_force_objdesc(m.id);
                            true
                        }
                        Err(e) => {
                            report.mismatches.push(Mismatch {
                                at: blob.idx,
                                what: format!(
                                    "blob {}: the captured 0xF6EA does not decode: {e}",
                                    blob.idx
                                ),
                            });
                            continue;
                        }
                    }
                } else if blob.opcode == CharacterSendCharGenResult::OPCODE.0 {
                    // The char-gen result is the caller's, but the **account** beside it is not:
                    // `create_character` takes only the result and reads the account out of the
                    // `0xF658` the session was handed earlier. Feeding the decoded result back
                    // therefore tests the kept state as well as the codec -- the same shape as
                    // `0xF653`.
                    match dereth_protocol::read_body::<CharacterSendCharGenResult>(body) {
                        Ok(m) => {
                            session.create_character(m.result);
                            true
                        }
                        Err(e) => {
                            report.mismatches.push(Mismatch {
                                at: blob.idx,
                                what: format!(
                                    "blob {}: the captured 0xF656 does not decode: {e}",
                                    blob.idx
                                ),
                            });
                            continue;
                        }
                    }
                } else if blob.opcode == CharacterDeleteRequest::OPCODE.0 {
                    // `0xF655` names the character by **slot**, while the public delete operation
                    // takes an id and looks the slot up by the selected character id. So the caller's gesture is
                    // "delete the character in this slot", and what the comparison then tests is
                    // the session's own `0xF658`-derived character set: the slot is turned into
                    // an id here, and `Session::delete_character` has to turn that id back into
                    // the same slot **and** name the account the character set carried. Neither
                    // the account nor the slot in the sent blob comes from the captured bytes.
                    match dereth_protocol::read_body::<CharacterDeleteRequest>(body) {
                        Ok(m) => {
                            let gid = usize::try_from(m.slot_index)
                                .ok()
                                .and_then(|i| session.characters().characters.get(i))
                                .map(|c| c.gid);
                            let Some(gid) = gid else {
                                report.mismatches.push(Mismatch {
                                    at: blob.idx,
                                    what: format!(
                                        "blob {}: the captured 0xF655 names slot {}, which the session's character set does not hold",
                                        blob.idx, m.slot_index
                                    ),
                                });
                                continue;
                            };
                            session.delete_character(gid);
                            true
                        }
                        Err(e) => {
                            report.mismatches.push(Mismatch {
                                at: blob.idx,
                                what: format!(
                                    "blob {}: the captured 0xF655 does not decode: {e}",
                                    blob.idx
                                ),
                            });
                            continue;
                        }
                    }
                } else if blob.opcode == AdminSendAdminRestoreCharacter::OPCODE.0 {
                    // `0xF7D9` is a bare Control-queue send whose two narrow-string fields
                    // the client never fills, so only the id is the
                    // caller's; the two empty strings are the session's own and are compared
                    // rather than fed in.
                    match dereth_protocol::read_body::<AdminSendAdminRestoreCharacter>(body) {
                        Ok(m) => {
                            session.restore_character(m.iid);
                            true
                        }
                        Err(e) => {
                            report.mismatches.push(Mismatch {
                                at: blob.idx,
                                what: format!(
                                    "blob {}: the captured 0xF7D9 does not decode: {e}",
                                    blob.idx
                                ),
                            });
                            continue;
                        }
                    }
                } else if blob.opcode == SendToRoomById::OPCODE.0 {
                    // `0xF7DE` is chatclient.dll's own packet inside a Logon-queue blob with no
                    // `OrderedActionHeader`; `Session::send_turbine_chat` re-frames it through
                    // `SendToRoomById::network_packet`, which is the codec under test.
                    match dereth_protocol::read_body::<SendToRoomById>(body) {
                        Ok(m) => {
                            if let Err(e) = session.send_turbine_chat(&m) {
                                report.mismatches.push(Mismatch {
                                    at: blob.idx,
                                    what: format!(
                                        "blob {}: the captured 0xF7DE did not round-trip: {e}",
                                        blob.idx
                                    ),
                                });
                                continue;
                            }
                            true
                        }
                        Err(e) => {
                            report.mismatches.push(Mismatch {
                                at: blob.idx,
                                what: format!(
                                    "blob {}: the captured 0xF7DE does not decode: {e}",
                                    blob.idx
                                ),
                            });
                            continue;
                        }
                    }
                } else if blob.opcode == LoginSendEnterWorld::OPCODE.0
                    || blob.opcode == DddEndDdd::OPCODE.0
                {
                    // Originated by the session with no caller involvement at all.
                    false
                } else {
                    *report
                        .unmodelled
                        .entry(Unmodelled::Blob(blob.opcode))
                        .or_default() += 1;
                    continue;
                };
                report.compared += 1;
                if driven {
                    report.driven += 1;
                }
                let Some(sent) = session.transport.sent.get(next_sent) else {
                    report.mismatches.push(Mismatch {
                        at: blob.idx,
                        what: format!("blob {}: the session sent nothing", blob.idx),
                    });
                    continue;
                };
                next_sent += 1;
                match compare_c2s(blob, sent, stamps.skipped) {
                    Ok(true) => report.exact += 1,
                    Ok(false) => {}
                    Err(m) => report.mismatches.push(m),
                }
                stamps.observe(session.next_action_stamp());
            }
        }
    }
    report
}

/// Which character each captured `0xF7C8 Login_SendEnterWorldRequest` was pressed for.
///
/// Not a single value read from the corpus's **first** `0xF657`: most recordings enter the world
/// as one character and stay, but `requested-death-vitae-salvage` does not: six `enter world -> log
/// off` cycles on **three** characters over one `ConnectRequest`, because `Session.LogOffPlayer` ->
/// `SendFinalLogOffMessages` (ACE's `Source/ACE.Server/Network/Session.cs`) sets
/// `Player = null`, sends `0xF653` and a fresh `0xF658`, and drops the session back to
/// `SessionState.AuthConnected` **without touching the transport**. A single value would enter
/// every later cycle as the first character and re-originate four blobs naming the wrong one.
///
/// The decision is the **caller's**, which is why it is read from the capture and not from the
/// session: the character screen's enter-game is a click and `0xF7C8` carries no
/// body. What the session does with it -- keeping the selected-avatar id, emitting `0xF657` when
/// `0xF7DF` arrives, and still knowing the character when `0xF653` goes out a cycle later -- is
/// compared byte for byte and is not fed in.
fn enter_world_decisions(
    corpus: &Corpus,
) -> BTreeMap<usize, (dereth_primitives::ObjectId, String)> {
    let mut asks: Vec<usize> = corpus
        .blobs
        .iter()
        .filter(|b| {
            b.dir == Direction::ClientToServer && b.opcode == LoginSendEnterWorldRequest::OPCODE.0
        })
        .map(|b| b.idx)
        .collect();
    asks.sort_unstable();
    let mut out = BTreeMap::new();
    for b in &corpus.blobs {
        if b.dir != Direction::ClientToServer || b.opcode != LoginSendEnterWorld::OPCODE.0 {
            continue;
        }
        let Some(m) = b
            .payload
            .get(4..)
            .and_then(|p| dereth_protocol::read_body::<LoginSendEnterWorld>(p).ok())
        else {
            continue;
        };
        // The ask this answer belongs to is the last `0xF7C8` before it. Retail cannot have two
        // outstanding: the character log-on's first step sends only while no log-on is awaiting
        // an answer.
        if let Some(ask) = asks.iter().rev().find(|i| **i < b.idx) {
            out.insert(*ask, (m.character, m.account));
        }
    }
    out
}

/// Compare one captured client-to-server blob against what the session sent.
///
/// `Ok(true)` means the two are identical byte for byte. `Ok(false)` means everything compared
/// agreed but the stamp is offset by `skipped`, which is the documented consequence of a captured
/// action this driver cannot re-originate; see [`StampTrack`].
fn compare_c2s(captured: &CorpusBlob, sent: &SentBlob, skipped: u32) -> Result<bool, Mismatch> {
    if sent.queue != captured.queue {
        return Err(Mismatch {
            at: captured.idx,
            what: format!(
                "blob {}: sent on {:?}, captured on {:?}",
                captured.idx, sent.queue, captured.queue
            ),
        });
    }

    if dword_at(&captured.payload, 0) == Some(OrderedActionHeader::MAGIC) {
        let (Some(cap_stamp), Some(our_stamp)) =
            (dword_at(&captured.payload, 4), dword_at(&sent.payload, 4))
        else {
            return Err(Mismatch {
                at: captured.idx,
                what: format!(
                    "blob {}: the captured blob is a game action but the sent one is {} bytes",
                    captured.idx,
                    sent.payload.len()
                ),
            });
        };

        // Everything from the sub-type dword on compares exactly, including the length.
        if sent.payload.get(8..) != captured.payload.get(8..) {
            return Err(Mismatch {
                at: captured.idx,
                what: format!(
                    "blob {}: action body differs\n  sent     {:02X?}\n  captured {:02X?}",
                    captured.idx,
                    &sent.payload[8.min(sent.payload.len())..],
                    &captured.payload[8.min(captured.payload.len())..]
                ),
            });
        }

        // And so does the stamp, as a value. `skipped` is zero unless a captured action earlier
        // in this stamp run had no driver here, in which case it is exactly how many.
        let want = our_stamp.wrapping_add(skipped);
        if cap_stamp != want {
            return Err(Mismatch {
                at: captured.idx,
                what: format!(
                    "blob {}: the captured action carries stamp {cap_stamp}; this session's \
                     counter issued {our_stamp} and {skipped} captured action(s) since the last \
                     action-counter reset had no driver here, so it should have been {want}",
                    captured.idx
                ),
            });
        }
        return Ok(skipped == 0);
    }

    if sent.payload != captured.payload {
        return Err(Mismatch {
            at: captured.idx,
            what: format!(
                "blob {}: payload differs\n  sent     {:02X?}\n  captured {:02X?}",
                captured.idx, sent.payload, captured.payload
            ),
        });
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_protocol::write_blob;

    #[test]
    fn a_shared_scenario_is_parsed_once_and_equals_a_fresh_load() {
        let first = Corpus::shared("first-login-walk-jump");
        assert!(
            std::ptr::eq(first, Corpus::shared("first-login-walk-jump")),
            "the second caller gets the same corpus, not a second parse"
        );
        let fresh = Corpus::load("first-login-walk-jump")
            .expect("parses")
            .expect("committed");
        assert_eq!(first.blobs, fresh.blobs);
        let all = Corpus::shared_all();
        assert_eq!(all.len(), session_names().len());
        assert!(
            all.iter().any(|c| std::ptr::eq(*c, first)),
            "every scenario comes out of the same cache"
        );
    }

    #[test]
    fn the_mock_records_what_the_session_sends() {
        let mut t = MockTransport::new();
        t.send(NetQueue::Logon, false, &[1, 2, 3, 4]);
        t.send(NetQueue::Weenie, true, &[5, 6, 7, 8]);
        assert_eq!(t.sent.len(), 2);
        assert_eq!(t.sent_on(NetQueue::Logon), vec![&[1u8, 2, 3, 4][..]]);
        assert!(t
            .expect_sent(&[
                (NetQueue::Logon, vec![1, 2, 3, 4]),
                (NetQueue::Weenie, vec![5, 6, 7, 8])
            ])
            .is_ok());
        assert!(t
            .expect_sent(&[(NetQueue::Logon, vec![1, 2, 3, 4])])
            .is_err());
    }

    #[test]
    fn the_mock_delivers_blobs_in_order() {
        let mut t = MockTransport::new();
        let a = write_blob(&dereth_protocol::login::LoginSendEnterWorldRequest).unwrap();
        let b = write_blob(&dereth_protocol::login::LoginEnterGameServerReady).unwrap();
        t.deliver_blob(NetQueue::UiQueue, &a);
        t.deliver_blob(NetQueue::UiQueue, &b);
        assert_eq!(t.pending(), 2);
        assert_eq!(t.poll().unwrap().opcode, 0xF7C8);
        assert_eq!(t.poll().unwrap().opcode, 0xF7DF);
        assert!(t.poll().is_none());
    }

    /// The JSON-line reader, on a line in the documented shape.
    #[test]
    fn a_corpus_line_parses() {
        let line = r#"{"idx": 12, "dir": "s2c", "t_rel": 1.25, "queue": 9, "blob_id": 3, "opcode": 63457, "payload_hex": "E1F70000"}"#;
        let b = parse_blob(line).unwrap();
        assert_eq!(b.idx, 12);
        assert_eq!(b.dir, Direction::ServerToClient);
        assert_eq!(b.t_rel_micros, 1_250_000);
        assert_eq!(b.queue, NetQueue::UiQueue);
        assert_eq!(b.opcode, 0xF7E1);
        assert_eq!(b.payload, vec![0xE1, 0xF7, 0x00, 0x00]);
    }

    #[test]
    fn a_malformed_corpus_line_is_an_error_not_a_panic() {
        assert!(parse_blob("{}").is_err());
        assert!(parse_blob(r#"{"idx":1,"dir":"sideways","t_rel":0,"queue":9,"blob_id":0,"opcode":0,"payload_hex":""}"#).is_err());
    }

    /// The corpus can be absent before fixture generation; loading must say so rather than fail.
    #[test]
    fn a_missing_corpus_is_not_an_error() {
        assert!(Corpus::load("no-such-scenario").unwrap().is_none());
    }

    /// A blob with the given envelope, a stamp that is **not** the sub-type, and a sub-type.
    fn envelope(dir: Direction, opcode: u32, stamp: u32, sub: u32) -> CorpusBlob {
        let mut payload = opcode.to_le_bytes().to_vec();
        if opcode == GAME_EVENT {
            payload.extend_from_slice(&0x5000_0001_u32.to_le_bytes());
        }
        payload.extend_from_slice(&stamp.to_le_bytes());
        payload.extend_from_slice(&sub.to_le_bytes());
        CorpusBlob {
            idx: 0,
            dir,
            t_rel_micros: 0,
            queue: NetQueue::UiQueue,
            blob_id: 0,
            opcode,
            payload,
        }
    }

    /// A sub type is read at its own envelopes offset.
    #[test]
    fn a_sub_type_is_read_at_its_own_envelopes_offset() {
        let c = Corpus {
            name: "synthetic".to_string(),
            blobs: vec![
                envelope(Direction::ClientToServer, GAME_ACTION, 0x0062, 0xF61C),
                envelope(Direction::ClientToServer, GAME_ACTION, 0x0062, 0xF61C),
                envelope(Direction::ServerToClient, GAME_EVENT, 0xF61C, 0x0062),
            ],
            checkpoints: Vec::new(),
        };
        assert_eq!(
            c.count_action(0xF61C),
            2,
            "the action's sub-type is the dword at offset 8"
        );
        assert_eq!(
            c.count_action(0x0062),
            0,
            "and offset 4 is the stamp, not a sub-type"
        );
        assert_eq!(
            c.count_event(0x0062),
            1,
            "the event's sub-type is the dword at offset 12"
        );
        assert_eq!(
            c.count_event(0xF61C),
            0,
            "and offset 8 is the event's stamp"
        );
        assert_eq!(c.count(Direction::ClientToServer, GAME_ACTION), 2);
        assert_eq!(
            c.count(Direction::ServerToClient, GAME_ACTION),
            0,
            "direction is part of it"
        );
        assert_eq!(c.action_census(), BTreeMap::from([(0xF61C, 2)]));
        assert_eq!(c.event_census(), BTreeMap::from([(0x0062, 1)]));
    }

    /// A blob too short to carry a sub-type is not counted as sub-type zero.
    #[test]
    fn a_truncated_envelope_is_not_counted_as_sub_type_zero() {
        let mut short = envelope(Direction::ClientToServer, GAME_ACTION, 1, 0);
        short.payload.truncate(10);
        let c = Corpus {
            name: "synthetic".to_string(),
            blobs: vec![short],
            checkpoints: Vec::new(),
        };
        assert_eq!(
            c.count(Direction::ClientToServer, GAME_ACTION),
            1,
            "the blob is there"
        );
        assert_eq!(c.count_action(0), 0, "but it carries no sub-type to count");
        assert!(c.action_census().is_empty());
    }

    /// The list of scenarios is the index's, and the index is the directories on disk.
    ///
    /// No count is written here: a promotion changes the number and must not redden a test.
    #[test]
    fn the_index_names_exactly_the_scenarios_on_disk() {
        let root = corpus_root();
        if !root.join("index.json").exists() {
            // The corpus is a generated fixture set; `Corpus::load` already documents that a
            // missing one is a real state rather than an error.
            return;
        }
        let mut on_disk: Vec<String> = std::fs::read_dir(&root)
            .expect("the corpus root is readable")
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.join("blobs.jsonl").exists())
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(str::to_owned))
            .collect();
        on_disk.sort();
        let mut named: Vec<String> = session_names().iter().map(|s| (*s).to_owned()).collect();
        named.sort();
        assert_eq!(
            named, on_disk,
            "index.json and the corpus directories disagree"
        );
        assert!(!named.is_empty(), "the corpus names at least one scenario");
    }

    /// Every recording answers to its slug and to nothing else, and no two share one.
    ///
    /// The slug is the recording's only name since the captures reorganisation. An old id must not
    /// resolve: if it did, half the tree could go on naming recordings by a name the corpus no
    /// longer documents.
    #[test]
    fn a_recording_answers_to_its_slug_and_only_its_slug() {
        if !corpus_root().join("index.json").exists() {
            return;
        }
        let pairs = session_index();
        assert_eq!(
            pairs.len(),
            session_names().len(),
            "one pair per named scenario"
        );
        let mut slugs: Vec<&str> = pairs.iter().map(|(_, slug)| *slug).collect();
        slugs.sort_unstable();
        let before = slugs.len();
        slugs.dedup();
        assert_eq!(slugs.len(), before, "two recordings share a slug");
        for (name, slug) in pairs {
            assert_eq!(name, slug, "the index's name is the slug");
            assert_eq!(
                resolve_session(slug),
                Some(*slug),
                "{slug} does not resolve to itself"
            );
            assert_eq!(session_slug(slug), Some(*slug));
            let c = Corpus::load(slug)
                .expect("the slug loads")
                .expect("the slug names a scenario");
            assert_eq!(c.name, *slug, "the loaded name is the slug");
        }
        for retired in [
            "session01",
            "session08",
            "house2",
            "fellowship",
            "desync-two-1",
        ] {
            assert_eq!(resolve_session(retired), None, "{retired} is a retired id");
            assert!(
                Corpus::load(retired)
                    .expect("no directory is not an error")
                    .is_none(),
                "{retired} still loads"
            );
        }
        assert_eq!(resolve_session("no-such-recording"), None);
    }

    /// The census agrees with a hand count over the same blobs, and finds the corpus's traffic.
    #[test]
    fn the_census_is_the_blob_list_counted() {
        if !corpus_root().join("index.json").exists() {
            return;
        }
        let mut movement = 0usize;
        for c in Corpus::load_all() {
            let actions = c
                .blobs
                .iter()
                .filter(|b| b.dir == Direction::ClientToServer && b.opcode == GAME_ACTION)
                .count();
            assert_eq!(
                c.count(Direction::ClientToServer, GAME_ACTION),
                actions,
                "{}",
                c.name
            );
            assert_eq!(
                c.action_census().values().sum::<usize>(),
                actions,
                "{}: every recorded action envelope is long enough to carry its sub-type",
                c.name
            );
            let events = c
                .blobs
                .iter()
                .filter(|b| b.dir == Direction::ServerToClient && b.opcode == GAME_EVENT)
                .count();
            assert_eq!(
                c.count(Direction::ServerToClient, GAME_EVENT),
                events,
                "{}",
                c.name
            );
            assert_eq!(
                c.event_census().values().sum::<usize>(),
                events,
                "{}",
                c.name
            );
            for (sub, n) in c.action_census() {
                assert_eq!(c.count_action(sub), n, "{}: action {sub:#06X}", c.name);
            }
            movement += c.count_action(0xF61C);
        }
        // The calibration, as a floor rather than a value: an instrument that counted nothing
        // would agree with every hand count above and still measure nothing.
        assert!(movement > 0, "the corpus carries recorded movement actions");
    }

    /// The client's queue ids.
    #[test]
    fn the_queue_ids_are_the_clients() {
        assert_eq!(queue_from_id(4), NetQueue::Logon);
        assert_eq!(queue_from_id(5), NetQueue::ClientCache);
        assert_eq!(queue_from_id(9), NetQueue::UiQueue);
        assert_eq!(queue_from_id(10), NetQueue::WorldObjects);
        assert_eq!(queue_from_id(1), NetQueue::Other(1));
    }
}
