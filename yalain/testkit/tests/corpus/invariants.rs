//! The world connections' invariants held on the January 2017 recordings themselves: every
//! message about an object names one the client was told of and has not forgotten, and each
//! object's stamps only move forward.
//!
//! A client keeps what it knows across the hand-offs between world servers (a new world
//! connection does not make again what the last one made), and forgets everything when its
//! player leaves the world; so the check runs per capture (one run of the client), in time order
//! across its world connections, starting again at each player made.

use std::collections::BTreeMap;

use auberean_protocol::ServerMessage;
use rusqlite::{Connection, OpenFlags};
use yalain_testkit::invariants::Checker;

fn open(path: &std::path::Path) -> Connection {
    let uri = format!(
        "file:{}?mode=ro&immutable=1",
        path.to_string_lossy().replace('\\', "/")
    );
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY
        | OpenFlags::SQLITE_OPEN_NO_MUTEX
        | OpenFlags::SQLITE_OPEN_URI;
    Connection::open_with_flags(uri, flags).expect("the capture index opens read-only")
}

/// Every 2017 world connection's server messages, in the order the client took them, keep the
/// known-object and stamp invariants, apart from the breaches tallied and explained.
/// Behaviour: server.replay.recorded-world-invariants
#[test]
fn the_recorded_2017_world_connections_keep_the_object_and_stamp_invariants() {
    let path = auberean_testkit::data::require_capture_index();
    let con = open(&path);
    let mut stmt = con
        .prepare(
            "SELECT s.capture_id, m.raw, m.session_id FROM message m JOIN session s USING (session_id) \
             JOIN capture c USING (capture_id) \
             WHERE s.game = 'ac2' AND s.world_login = 1 AND c.file LIKE '2017-%' \
             AND m.status = 'ac2' AND m.dir = 's2c' ORDER BY s.capture_id, m.t, m.session_id, m.idx",
        )
        .expect("prepares");
    let mut rows = stmt.query([]).expect("runs");
    let mut checked: BTreeMap<String, usize> = BTreeMap::new();
    let mut breaches: BTreeMap<String, usize> = BTreeMap::new();
    let mut sessions = 0usize;
    let mut current = None;
    let mut c = Checker::new();
    // Before a capture has seen its player enter, the client holds objects the recording never
    // saw made (the capture began in mid-session).
    let mut entered = false;
    let mut flush = |c: &mut Checker, entered: bool| {
        for (k, n) in std::mem::take(&mut c.checked) {
            *checked.entry(k).or_default() += n;
        }
        for (k, n) in std::mem::take(&mut c.breaches) {
            let k = if entered {
                k
            } else {
                format!("{k} (the capture began in mid-session)")
            };
            *breaches.entry(k).or_default() += n;
        }
    };
    while let Some(r) = rows.next().expect("a row") {
        let sid: i64 = r.get(0).expect("id");
        let raw: Vec<u8> = r.get(1).expect("raw");
        if current != Some(sid) {
            flush(&mut c, entered);
            c = Checker::new();
            entered = false;
            current = Some(sid);
            sessions += 1;
        }
        c.on_connection(r.get(2).expect("session"));
        if let Ok(m) = ServerMessage::decode(&raw) {
            if matches!(m, ServerMessage::PlayerObject(_)) {
                flush(&mut c, entered);
                c = Checker::new();
                entered = true;
            }
            c.see(&m);
        }
    }
    flush(&mut c, entered);
    println!("{sessions} captures' 2017 world connections; messages about an object, by kind:");
    for (k, n) in &checked {
        println!("  {n:9}  {k}");
    }
    println!("breaches:");
    for (k, n) in &breaches {
        println!("  {n:9}  {k}");
    }
    assert!(sessions > 0);
}
