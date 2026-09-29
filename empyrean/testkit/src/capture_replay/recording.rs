//! A recorded session, reassembled from its datagrams.
//!
//! The recordings are `fixtures/packet-captures/<slug>.jsonl`, one flat corpus named by slug:
//! one datagram per line, `{t, dir, data}`. Each datagram is parsed with the shared wire parser,
//! retransmits are dropped by `(direction, packet sequence)`, and the fragments are reassembled into
//! messages. Nothing here keeps or prints a payload; callers see opcodes, guids and counts.
//!
//! **Send order.** The server's messages are put back into the order the server sent them: by the
//! blob sequence (the low dword of the blob id), which ACE numbers per connection across every
//! queue. ACE packs small messages into earlier datagrams, so arrival order is not send order (a
//! spawn's `PlayScript` can arrive before its `CreateObject`). The client's messages keep their
//! arrival order and are interleaved by time against the running maximum of the server messages'
//! arrival times.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use dereth_transport::wire::ParsedPacket;

/// One reassembled message.
#[derive(Debug, Clone)]
pub struct Msg {
    /// Seconds since the recording's first datagram (for the server's messages: when the datagram
    /// that completed it arrived).
    pub t: f64,
    /// Sent by the client.
    pub c2s: bool,
    /// The fragment's queue id (1-11).
    pub queue: u16,
    /// The blob sequence (the low dword of the blob id).
    pub seq: u32,
    /// The message, opcode first.
    pub payload: Vec<u8>,
}

impl Msg {
    #[must_use]
    pub fn opcode(&self) -> u32 {
        crate::capture_replay::wire::u32_at(&self.payload, 0).unwrap_or(0)
    }
}

/// A recording: its name (its slug, the file stem) and its messages
/// in send order (see the module docs).
#[derive(Debug, Clone)]
pub struct Recording {
    pub name: String,
    pub msgs: Vec<Msg>,
    /// Datagrams that did not parse (reported, never fatal).
    pub unparsed: usize,
}

/// `fixtures/packet-captures` of this checkout, or `DERETH_TEST_CAPTURES`.
#[must_use]
pub fn captures_root() -> PathBuf {
    empyrean_common::test_paths::captures()
}

/// Every recording slug in `root`: the `*.jsonl` files there, sorted. Globbed, never listed, so a
/// recording added or removed is picked up. The two unclean-logout recordings
/// (`unclean-logout-short`, `unclean-logout-long`) are ordinary members: they lack a clean
/// `Disconnect` by design, which the comparison explains without knowing their names.
///
/// # Panics
/// When `root` cannot be read: the tier needs its input.
#[must_use]
pub fn session_names(root: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(root)
        .unwrap_or_else(|e| panic!("the capture corpus at {}: {e}", root.display()))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|x| x == "jsonl"))
        .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .collect();
    names.sort();
    names
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

/// A blob being reassembled: its fragment count, queue, and the fragments seen by index.
type Partial = (u16, u16, BTreeMap<u16, Vec<u8>>);

/// Loads and reassembles one recording.
///
/// # Panics
/// When the file cannot be read or a line is not JSON.
#[must_use]
pub fn load(root: &Path, name: &str) -> Recording {
    let path = root.join(format!("{name}.jsonl"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut pending: BTreeMap<(bool, u64), Partial> = BTreeMap::new();
    let mut seen = BTreeSet::new();
    let mut c2s = Vec::new();
    let mut s2c = Vec::new();
    let mut unparsed = 0;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let d: serde_json::Value = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("{}: not JSON: {e}", path.display()));
        let is_c2s = d["dir"] == "c2s";
        let t = d["t"].as_f64().unwrap_or(0.0);
        let Some(bytes) = d["data"].as_str().and_then(unhex) else {
            unparsed += 1;
            continue;
        };
        let Ok(packet) = ParsedPacket::parse(&bytes) else {
            unparsed += 1;
            continue;
        };
        if packet.fragments.is_empty() || !seen.insert((is_c2s, packet.header.seq_id)) {
            continue;
        }
        for f in packet.fragments {
            let key = (
                is_c2s,
                u64::from(f.header.blob_id_high) << 32 | u64::from(f.header.blob_id_low),
            );
            let slot = pending
                .entry(key)
                .or_insert_with(|| (f.header.num_frags, f.header.queue_id, BTreeMap::new()));
            slot.2.insert(f.header.blob_num, f.payload);
            if slot.2.len() == usize::from(slot.0) {
                let Some((_, queue, parts)) = pending.remove(&key) else {
                    continue;
                };
                let msg = Msg {
                    t,
                    c2s: is_c2s,
                    queue,
                    seq: f.header.blob_id_low,
                    payload: parts.into_values().flatten().collect(),
                };
                if is_c2s {
                    c2s.push(msg);
                } else {
                    s2c.push(msg);
                }
            }
        }
    }
    s2c.sort_by_key(|m| m.seq);
    Recording {
        name: name.to_owned(),
        msgs: interleave(s2c, c2s),
        unparsed,
    }
}

/// The server's messages (already in send order) interleaved with the client's (in arrival order):
/// a client message goes before the first server message whose running-maximum arrival time is
/// later than its own.
#[must_use]
pub fn interleave(s2c: Vec<Msg>, c2s: Vec<Msg>) -> Vec<Msg> {
    let mut out = Vec::with_capacity(s2c.len() + c2s.len());
    let mut c2s = c2s.into_iter().peekable();
    let mut high = f64::NEG_INFINITY;
    for m in s2c {
        high = high.max(m.t);
        while c2s.peek().is_some_and(|c| c.t < high) {
            out.extend(c2s.next());
        }
        out.push(m);
    }
    out.extend(c2s);
    out
}
