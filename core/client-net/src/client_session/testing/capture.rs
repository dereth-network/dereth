//! Raw recording fixtures, with corpus lookup and shared per-process loading.
//!
//! Parsing and recording values are shared with production hosts; only fixture lookup,
//! filesystem loading and the shared cache live here.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, PoisonError};

use super::super::recording::parse;
pub use super::super::recording::{peer, CaptureError, Datagram};

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
    parse(&name, &text)
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
