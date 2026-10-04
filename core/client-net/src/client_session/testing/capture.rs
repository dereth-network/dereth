//! Reading one **raw** recorded session out of `fixtures/packet-captures/<name>.jsonl`.
//!
//! `super::Corpus` owns the *decoded* corpus (`fixtures/message-corpus`): blobs, already reassembled,
//! already typed. This module is the layer below it -- the datagrams as they were recorded, before
//! reassembly -- and it lives here for the same reason `Corpus` does: a harness that replays a
//! login needs the bytes a socket would have delivered.
//!
//! **It is shared so there are not dozens of private copies.** A test target is one file and a
//! test file cannot lend a helper to another crate, so without this every capture-replaying module
//! would carry its own `fn load(session: &str)` over the same four fields and its own
//! `fn addr(pair: u16)`.
//!
//! A line is `{"t": <secs>, "dir": "c2s"|"s2c", "pair": <n>, "data": "<hex>"}` plus fields this
//! reader does not need. The scan is by field name rather than by a JSON parser for the same reason
//! a test would: the file is machine-written, one object per line, and the session
//! layer is not taking a JSON dependency to read four fields out of it.
//!
//! [`shared_session`] reads each recording once per process, so a test binary whose modules replay
//! the same recording parses it once.
//!
//! **What is deliberately not here.** The connection sequence number a replay endpoint is built
//! with is read out of the recording's own login request, and each enter-world out of the client's
//! own datagrams, which means parsing datagram headers; that is the transport's work, which the
//! session layer does not name (the isolation rule in the module root). Both live in
//! `dereth_client_net::recording` and take a `&[Datagram]` from here.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, PoisonError};

/// One recorded datagram, exactly as the proxy wrote it down.
#[derive(Debug, Clone, PartialEq)]
pub struct Datagram {
    /// Seconds from the start of the recording.
    ///
    /// **Not a clock to feed at.** A replay feeds at the client's own local time, because the login
    /// state machine's give-up timer is measured against the time a datagram arrives at, and a
    /// datagram fed at its recorded `t` arrives from the client's own future.
    pub t: f64,
    /// True when the client sent it, false when the shard did.
    pub c2s: bool,
    /// Which address pair it belongs to: the logon server is 0 and each world server that followed
    /// is the next number up.
    pub pair: u16,
    /// The datagram itself.
    pub raw: Vec<u8>,
}

impl Datagram {
    /// Where a datagram of this pair arrives from. See [`peer`].
    #[must_use]
    pub fn peer(&self) -> SocketAddr {
        peer(self.pair)
    }
}

/// The address a recorded pair's datagrams arrive from, and the address a replay endpoint sends to.
///
/// Pair 0 is the logon server and each world server the recording moved on to is the next port up.
/// Nothing binds this socket -- it is a name for a direction, so that every replaying harness gives
/// the same recording the same addresses and a session's per-peer state lines up.
#[must_use]
pub fn peer(pair: u16) -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], 19_000 + pair))
}

/// Where the raw recordings live, relative to the workspace root.
///
/// `fixtures/packet-captures`: the one place the datagrams live, beside their `index.json` and
/// `manifest.json`. There is deliberately no second serialisation of them.
#[must_use]
pub fn captures_root() -> PathBuf {
    // `CARGO_MANIFEST_DIR` is `core/client-net`.
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/packet-captures")
}

/// The recordings that end without a clean `Disconnect`, by slug, as the capture index lists
/// them (`without_disconnect` in `fixtures/packet-captures/index.json`).
///
/// They sit in the folder beside the others but the message corpus leaves them out, so a test
/// that scans the folder and wants only clean sessions skips these -- by the index's word, not by
/// a list of names of its own.
///
/// # Panics
/// When the capture index cannot be read or has no `without_disconnect` list: it is generated
/// with the recordings, so either is a broken checkout.
#[must_use]
pub fn without_disconnect() -> &'static [&'static str] {
    static NAMES: OnceLock<Vec<&'static str>> = OnceLock::new();
    NAMES.get_or_init(|| {
        let path = captures_root().join("index.json");
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("the capture index at {}: {e}", path.display()));
        let key = "\"without_disconnect\"";
        let at = text
            .find(key)
            .unwrap_or_else(|| panic!("{} has no {key} list", path.display()));
        let rest = &text[at + key.len()..];
        let open = rest.find('[').expect("the list opens");
        let close = rest[open..].find(']').expect("the list closes") + open;
        // Slugs are `[a-z0-9-]`, so every other `"` in the list delimits one.
        rest[open + 1..close]
            .split('"')
            .skip(1)
            .step_by(2)
            .map(|s| &*Box::leak(s.to_owned().into_boxed_str()))
            .collect()
    })
}

/// A capture file that is missing, unreadable, or not shaped like one.
#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("{path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{path} line {line}: {what}")]
    Malformed {
        path: String,
        line: usize,
        what: String,
    },
    #[error("{path} records no datagram at all")]
    Empty { path: String },
}

/// Every datagram of the recording at `path`, in recorded order.
///
/// # Errors
/// [`CaptureError`] when the file is absent, unreadable, empty, or a line is missing one of the
/// four fields. **An absent recording is an error and never an empty replay**: the captures are
/// committed, so a missing one is a broken checkout, and a harness that returned zero datagrams
/// would read as a pass over nothing.
pub fn load(path: &Path) -> Result<Vec<Datagram>, CaptureError> {
    let name = path.display().to_string();
    let text = std::fs::read_to_string(path).map_err(|source| CaptureError::Io {
        path: name.clone(),
        source,
    })?;
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let bad = |what: &str| CaptureError::Malformed {
            path: name.clone(),
            line: i + 1,
            what: what.to_owned(),
        };
        let t: f64 = field(line, "\"t\"")
            .ok_or_else(|| bad("no t"))?
            .parse()
            .map_err(|_| bad("t is not a number"))?;
        let dir = field(line, "\"dir\"").ok_or_else(|| bad("no dir"))?;
        let pair: u16 = field(line, "\"pair\"")
            .ok_or_else(|| bad("no pair"))?
            .parse()
            .map_err(|_| bad("pair is not a number"))?;
        let data = field(line, "\"data\"").ok_or_else(|| bad("no data"))?;
        let raw = hex(data).ok_or_else(|| bad("data is not hex"))?;
        out.push(Datagram {
            t,
            c2s: dir == "c2s",
            pair,
            raw,
        });
    }
    if out.is_empty() {
        return Err(CaptureError::Empty { path: name });
    }
    Ok(out)
}

/// [`load`] by recording name -- its content slug, the file stem -- under
/// [`captures_root`]. A name the index does not carry is used as given, so a recording outside
/// the locked corpus still loads by file name.
///
/// # Errors
/// As [`load`].
pub fn load_session(name: &str) -> Result<Vec<Datagram>, CaptureError> {
    let name = super::resolve_session(name).unwrap_or(name);
    load(&captures_root().join(format!("{name}.jsonl")))
}

/// [`load_session`], read once per process and shared by every caller after the first.
///
/// The recordings are committed, so one that is absent or malformed is a broken checkout, and this
/// panics rather than hand back an empty replay.
///
/// # Panics
/// As [`load_session`] errors.
#[must_use]
pub fn shared_session(name: &str) -> &'static [Datagram] {
    static CACHE: OnceLock<Mutex<BTreeMap<String, &'static [Datagram]>>> = OnceLock::new();
    let mut cache = CACHE
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    if let Some(d) = cache.get(name) {
        return d;
    }
    let d = load_session(name)
        .unwrap_or_else(|e| panic!("the recording {name} does not load: {e}"))
        .leak();
    cache.insert(name.to_owned(), d);
    d
}

/// The value of one `"key"` on a one-object JSON line, as text.
fn field<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let at = line.find(key)?;
    let rest = line[at + key.len()..].trim_start_matches([' ', ':', '"']);
    let end = rest.find(['"', ',', '}'])?;
    Some(&rest[..end])
}

fn hex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    s.as_bytes()
        .chunks(2)
        .map(|c| u8::from_str_radix(std::str::from_utf8(c).ok()?, 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_yields_its_four_fields_and_a_blank_line_yields_nothing() {
        let dir = std::env::temp_dir().join("dereth-net-client-session-capture-test");
        std::fs::create_dir_all(&dir).expect("a temp dir");
        let path = dir.join("tiny.jsonl");
        std::fs::write(
            &path,
            "{\"idx\":0,\"t\":0.5,\"dir\":\"s2c\",\"pair\":2,\"data\":\"0aff10\"}\n\n\
             {\"idx\":1,\"t\":1.25,\"dir\":\"c2s\",\"pair\":0,\"data\":\"\"}\n",
        )
        .expect("write");

        let recs = load(&path).expect("two records");
        assert_eq!(recs.len(), 2, "the blank line is not a record");
        assert!((recs[0].t - 0.5).abs() < f64::EPSILON);
        assert!(!recs[0].c2s);
        assert_eq!(recs[0].pair, 2);
        assert_eq!(recs[0].raw, vec![0x0A, 0xFF, 0x10]);
        assert!(recs[1].c2s);
        assert_eq!(
            recs[1].raw,
            Vec::<u8>::new(),
            "an empty payload is still a datagram"
        );

        std::fs::remove_file(&path).expect("clean up");
    }

    #[test]
    fn an_absent_capture_is_an_error_and_not_an_empty_replay() {
        let e = load(Path::new("no-such-session.jsonl")).expect_err("no such file");
        assert!(matches!(e, CaptureError::Io { .. }), "{e}");
    }

    #[test]
    fn a_pairs_peer_is_the_loopback_port_that_pair_up_from_the_logon_server() {
        assert_eq!(peer(0).to_string(), "127.0.0.1:19000");
        assert_eq!(peer(1).to_string(), "127.0.0.1:19001");
        assert_eq!(
            Datagram {
                t: 0.0,
                c2s: false,
                pair: 3,
                raw: Vec::new()
            }
            .peer(),
            peer(3),
            "a datagram names its own peer, so a caller cannot pair it with another's"
        );
    }

    #[test]
    fn the_committed_corpus_is_readable_through_this_reader() {
        // The recordings are committed, so this is not a fixture that may be absent.
        let root = captures_root();
        assert!(
            root.is_dir(),
            "the capture corpus is absent at {}",
            root.display()
        );
        let recs = load_session("first-login-walk-jump")
            .expect("first-login-walk-jump is in the committed corpus");
        assert!(
            recs.iter().any(|r| r.c2s),
            "a recording carries both directions"
        );
        assert!(
            recs.iter().any(|r| !r.c2s),
            "a recording carries both directions"
        );
    }

    #[test]
    fn a_shared_recording_is_read_once_and_equals_a_fresh_read() {
        let first = shared_session("first-login-walk-jump");
        let again = shared_session("first-login-walk-jump");
        assert!(
            std::ptr::eq(first, again),
            "the second caller gets the same datagrams, not a second read"
        );
        let fresh = load_session("first-login-walk-jump").expect("committed");
        assert_eq!(first, fresh.as_slice());
    }
}
