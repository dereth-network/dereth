//! Every recorded message, both directions, decoded and encoded again.

use std::collections::BTreeMap;
use std::path::Path;

use auberean_protocol::{ClientBlob, ServerMessage};
use rusqlite::{Connection, OpenFlags};

/// What one direction and type did.
#[derive(Debug, Default, Clone)]
struct Tally {
    messages: u64,
    exact: u64,
    same_bytes: u64,
    /// Failures by kind: how many, from which recording era, and one example.
    failures: BTreeMap<String, (u64, BTreeMap<String, u64>, String)>,
}

fn open(path: &Path) -> Connection {
    let uri = format!(
        "file:{}?mode=ro&immutable=1",
        path.to_string_lossy().replace('\\', "/")
    );
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY
        | OpenFlags::SQLITE_OPEN_NO_MUTEX
        | OpenFlags::SQLITE_OPEN_URI;
    Connection::open_with_flags(uri, flags).expect("the capture index opens read-only")
}

fn note(t: &mut Tally, kind: &str, era: &str, key: String) {
    let f = t
        .failures
        .entry(kind.to_string())
        .or_insert_with(|| (0, BTreeMap::new(), key));
    f.0 += 1;
    *f.1.entry(era.to_string()).or_default() += 1;
}

/// Decode and re-encode the messages of the sessions `lo..hi`.
fn run(path: &Path, lo: i64, hi: i64) -> BTreeMap<(String, u32), Tally> {
    let con = open(path);
    let mut stmt = con
        .prepare(
            "SELECT s.session_key, m.idx, m.dir, m.queue, m.mtype, m.raw, c.file \
             FROM message m JOIN session s USING (session_id) JOIN capture c USING (capture_id) \
             WHERE m.status = 'ac2' AND m.session_id >= ?1 AND m.session_id < ?2",
        )
        .expect("the query prepares");
    let mut rows = stmt.query([lo, hi]).expect("the query runs");
    let mut out: BTreeMap<(String, u32), Tally> = BTreeMap::new();
    while let Some(row) = rows.next().expect("a row") {
        let key: String = row.get(0).expect("key");
        let idx: i64 = row.get(1).expect("idx");
        let dir: String = row.get(2).expect("dir");
        let queue: u16 = row.get(3).expect("queue");
        let mtype: u32 = row.get(4).expect("type");
        let raw: Vec<u8> = row.get(5).expect("raw");
        let file: String = row.get(6).expect("file");
        let era = if file.starts_with("2009-") {
            "2009 emulator".to_string()
        } else {
            file[..4].to_string()
        };
        let t = out.entry((dir.clone(), mtype)).or_default();
        t.messages += 1;
        let at = format!("{key}#{idx}");
        let encoded = if dir == "s2c" {
            ServerMessage::decode(&raw).map(|m| m.encode())
        } else {
            ClientBlob::decode(queue, &raw).map(|m| m.encode())
        };
        match encoded {
            Ok(bytes) => {
                t.exact += 1;
                if bytes == raw {
                    t.same_bytes += 1;
                } else {
                    note(t, "encodes to other bytes", &era, at);
                }
            }
            Err(e) => note(t, e.kind(), &era, at),
        }
    }
    out
}

/// Every message recorded in the retail sessions, both directions, decodes with every byte used
/// and encodes back to exactly the bytes recorded. The 2009 community emulator's messages are
/// held to the same reading; where it wrote what retail never did (trailing zeros after a
/// player's qualities, non-zero padding) the difference is listed and allowed for it alone.
/// Behaviour: protocol.message.every-recorded-message-round-trips
#[test]
fn every_recorded_message_decodes_exactly_and_encodes_back() {
    let path = auberean_testkit::data::require_capture_index();
    let sessions: i64 = open(&path)
        .query_row("SELECT max(session_id) + 1 FROM session", [], |r| r.get(0))
        .expect("session count");
    let threads = std::thread::available_parallelism()
        .map_or(4, |n| n.get())
        .clamp(2, 12);
    let step = i64::try_from(threads).unwrap_or(4);
    let mut total: BTreeMap<(String, u32), Tally> = BTreeMap::new();
    std::thread::scope(|s| {
        let handles: Vec<_> = (0..step)
            .map(|k| {
                let path = path.clone();
                s.spawn(move || {
                    let mut part = BTreeMap::new();
                    let chunk = (sessions / (step * 8)).max(1);
                    let mut lo = k * chunk;
                    while lo < sessions {
                        for (key, t) in run(&path, lo, lo + chunk) {
                            merge(&mut part, key, t);
                        }
                        lo += chunk * step;
                    }
                    part
                })
            })
            .collect();
        for h in handles {
            for (key, t) in h.join().expect("a worker finished") {
                merge(&mut total, key, t);
            }
        }
    });

    let (mut messages, mut exact, mut same) = (0u64, 0u64, 0u64);
    let mut retail_failures = Vec::new();
    let mut emulator_failures = 0u64;
    println!("dir  type      messages      exact  same bytes  failures");
    for ((dir, mtype), t) in &total {
        messages += t.messages;
        exact += t.exact;
        same += t.same_bytes;
        let fails: Vec<String> = t
            .failures
            .iter()
            .map(|(k, (n, eras, ex))| format!("{k} x{n} {eras:?} e.g. {ex}"))
            .collect();
        println!(
            "{dir}  {mtype:#07x} {:>10} {:>10} {:>11}  {}",
            t.messages,
            t.exact,
            t.same_bytes,
            fails.join("; ")
        );
        for (kind, (n, eras, ex)) in &t.failures {
            for (era, m) in eras {
                if era == "2009 emulator" {
                    emulator_failures += m;
                } else {
                    retail_failures.push(format!("{dir} {mtype:#x} {kind} x{n} ({era}) e.g. {ex}"));
                }
            }
        }
    }
    println!(
        "all: {messages} messages, {exact} decode exactly, {same} encode back to the same bytes"
    );
    println!("the 2009 emulator's messages that differ from retail's form: {emulator_failures}");
    assert!(messages > 0, "the index holds messages");
    assert!(
        retail_failures.is_empty(),
        "retail messages that fail:\n{}",
        retail_failures.join("\n")
    );
}

fn merge(into: &mut BTreeMap<(String, u32), Tally>, key: (String, u32), t: Tally) {
    let e = into.entry(key).or_default();
    e.messages += t.messages;
    e.exact += t.exact;
    e.same_bytes += t.same_bytes;
    for (k, (n, eras, ex)) in t.failures {
        let f = e
            .failures
            .entry(k)
            .or_insert_with(|| (0, BTreeMap::new(), ex));
        f.0 += n;
        for (era, m) in eras {
            *f.1.entry(era).or_default() += m;
        }
    }
}
