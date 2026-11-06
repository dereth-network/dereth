//! Script calls: the envelope, and the arguments inside it read as the script machine's values.

use std::path::Path;
use std::sync::OnceLock;

use auberean_dat::locate::{install_dats, DatName};
use auberean_dat::testing::{require_install, TARGET};
use auberean_dat::DatFile;
use auberean_protocol::script::{ClientCall, ScriptArgs};
use auberean_protocol::{ClientMessage, ServerMessage};
use auberean_script::library::{Library, RECORD};
use rusqlite::{Connection, OpenFlags};

fn library() -> Option<&'static Library> {
    static L: OnceLock<Option<Library>> = OnceLock::new();
    L.get_or_init(|| {
        let dir = require_install(TARGET);
        let (_, path) = install_dats(&dir)
            .ok()?
            .into_iter()
            .find(|(n, _)| *n == DatName::Portal)?;
        let dat = DatFile::open(&path).ok()?;
        dat.read_checked(RECORD, Library::from_record)
            .ok()
            .map(|c| c.value)
    })
    .as_ref()
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

/// Rebuild a call's arguments from its values: `None` when they did not read.
fn repack(args: &ScriptArgs, lib: &Library) -> Option<ScriptArgs> {
    let values = args.values(lib).ok()?;
    ScriptArgs::from_values(&values).ok()
}

fn repack_call(c: &ClientCall, lib: &Library) -> Option<ClientCall> {
    let args = match &c.args {
        Some(a) => Some(repack(a, lib)?),
        None => None,
    };
    Some(ClientCall {
        function: c.function,
        args,
    })
}

/// Every script call recorded with the relaunched game (2013 and 2017), both ways: the message
/// reads, its arguments read as the script machine's values against the September 2013 library,
/// and the message rebuilt from those values is the message recorded.
/// Behaviour: protocol.script.every-relaunch-call-reads-to-values-and-back
#[test]
fn every_relaunch_script_call_reads_to_values_and_rebuilds_the_same_message() {
    let lib = library().expect("the September 2013 install opens");
    let path = auberean_testkit::data::require_capture_index();
    let con = open(&path);
    let mut stmt = con
        .prepare(
            "SELECT s.session_key, m.idx, m.dir, m.raw FROM message m JOIN session s USING (session_id) \
             JOIN capture c USING (capture_id) WHERE m.status = 'ac2' \
             AND m.mtype IN (65619, 65638, 65639, 65732, 65738) \
             AND (c.file LIKE '2013-%' OR c.file LIKE '2017-%')",
        )
        .expect("the query prepares");
    let mut rows = stmt.query([]).expect("the query runs");
    let (mut calls, mut same) = (0u64, 0u64);
    let mut failures: Vec<String> = Vec::new();
    while let Some(row) = rows.next().expect("a row") {
        let key: String = row.get(0).expect("key");
        let idx: i64 = row.get(1).expect("idx");
        let dir: String = row.get(2).expect("dir");
        let raw: Vec<u8> = row.get(3).expect("raw");
        calls += 1;
        let rebuilt = if dir == "c2s" {
            match ClientMessage::decode(&raw) {
                Ok(ClientMessage::ServerCall(mut c)) => repack(&c.args, lib).map(|a| {
                    c.args = a;
                    ClientMessage::ServerCall(c).encode()
                }),
                _ => None,
            }
        } else {
            match ServerMessage::decode(&raw) {
                Ok(ServerMessage::PlayerCall(c)) => {
                    repack_call(&c, lib).map(|c| ServerMessage::PlayerCall(c).encode())
                }
                Ok(ServerMessage::PlayerCallAlternate(c)) => {
                    repack_call(&c, lib).map(|c| ServerMessage::PlayerCallAlternate(c).encode())
                }
                Ok(ServerMessage::CellObjectCall(mut o)) => repack_call(&o.call, lib).map(|c| {
                    o.call = c;
                    ServerMessage::CellObjectCall(o).encode()
                }),
                Ok(ServerMessage::VisualObjectCall(mut o)) => repack_call(&o.call, lib).map(|c| {
                    o.call = c;
                    ServerMessage::VisualObjectCall(o).encode()
                }),
                _ => None,
            }
        };
        if rebuilt.as_deref() == Some(raw.as_slice()) {
            same += 1;
        } else if failures.len() < 20 {
            failures.push(format!("{key}#{idx} ({dir})"));
        }
    }
    println!(
        "{calls} relaunch script calls; {same} read to values and rebuild the recorded message"
    );
    assert!(calls > 0, "the index holds script calls");
    assert_eq!(same, calls, "calls that do not rebuild: {failures:?}");
}
