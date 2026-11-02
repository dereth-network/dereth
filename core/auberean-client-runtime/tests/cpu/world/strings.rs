//! The interface's text elements' string downloads: a reference whose string tables the files
//! lack asks the data cache for them, and is written once they have come or been refused.

use std::sync::Arc;

use auberean_assets::strings::{StringEntry, StringTable};
use auberean_client_runtime::data::{DataFiles, ENGLISH};
use auberean_client_runtime::{Client, ClientData, ClientSettings};
use auberean_dat::{DatFile, QualifiedId, RecordType};
use auberean_protocol::login::{DataRecord, DataRefused};
use auberean_protocol::{ClientMessage, ServerMessage};
use auberean_text::StringInfo;

use super::download::{file, file_with};

const PORTAL: u64 = 0x1_0000_0000;
const CELL: u64 = 0x2_0000_0001;
const LOCAL: u64 = 0x3_8000_0001;
/// A blob on the login connection, taken as it comes.
const LOGIN: u64 = 0x2300_0000_0000_0000;

/// The table the files hold: token 7 sends its text on to [`PRIVATE`], token 8 is its own.
const HELD: u32 = 0x2500_0001;
const PRIVATE: u32 = 0x2502_0001;
/// A table the files lack: token 7 sends its text on to [`MISSING_PRIVATE`].
const MISSING: u32 = 0x2500_0002;
const MISSING_PRIVATE: u32 = 0x2502_0002;

fn table(entries: &[(u32, u32, &str)]) -> StringTable {
    let mut t = StringTable::default();
    for &(token, private, text) in entries {
        t.entries.push(StringEntry {
            token,
            table: private,
            strings: if text.is_empty() {
                Vec::new()
            } else {
                vec![Some(text.to_owned())]
            },
            ..StringEntry::default()
        });
    }
    t
}

fn local() -> Arc<DatFile> {
    let held = table(&[(7, PRIVATE, ""), (8, 0, "Own text")]);
    file_with(LOCAL, &[(HELD, auberean_assets::encode(HELD, &held))])
}

fn client() -> Client {
    Client::new(
        ClientSettings {
            login_server: "127.0.0.1:9000".parse().expect("an address"),
            account: b"a".to_vec(),
            ticket: Vec::new(),
            timestamp: 0x5000_0000,
            session_key: [0; 64],
            script_sends: false,
        },
        ClientData {
            files: DataFiles {
                files: vec![file(PORTAL, &[]), file(CELL, &[]), local()],
                language: ENGLISH,
            },
            scripts: None,
        },
    )
}

const fn q(id: u32) -> QualifiedId {
    QualifiedId {
        id,
        kind: RecordType::STRING_TABLE.0,
    }
}

/// The string tables the client asked the server for, in order.
fn asked(c: &Client) -> Vec<u32> {
    c.sent()
        .iter()
        .filter_map(|s| match &s.message {
            ClientMessage::DataRequest(r) if r.kind == RecordType::STRING_TABLE.0 => Some(r.id),
            _ => None,
        })
        .collect()
}

fn send(c: &mut Client, n: u64, id: u32, t: &StringTable) {
    c.take_message(
        LOGIN | n,
        ServerMessage::DataRecord(DataRecord {
            dat_file: LOCAL,
            kind: RecordType::STRING_TABLE.0,
            id,
            size: 0,
            bytes: auberean_assets::encode(id, t),
        }),
    );
}

fn refuse(c: &mut Client, n: u64, id: u32) {
    c.take_message(
        LOGIN | n,
        ServerMessage::DataRefused(DataRefused {
            kind: RecordType::STRING_TABLE.0,
            id,
            error: 1,
        }),
    );
}

fn written(c: &Client, info: &StringInfo) -> String {
    let units = c
        .data_download()
        .strings()
        .write(info)
        .expect("a cache with files");
    String::from_utf16_lossy(&units)
}

/// A reference whose table the files hold, but whose entry sends its text on to a private table
/// they lack, asks the server for the private table only; it waits until the table comes, and is
/// then written from it. A reference whose tables are all at hand asks for nothing.
///
/// Behaviour: data.string-download.a-private-table-the-files-lack-is-asked-for
#[test]
fn a_reference_whose_private_table_the_files_lack_asks_for_it_and_is_written_when_it_comes() {
    let mut c = client();
    let strings = c.data_download().strings().clone();
    assert_eq!(strings.start(&StringInfo::entry(HELD, 8)), None);
    assert_eq!(written(&c, &StringInfo::entry(HELD, 8)), "Own text");
    let info = StringInfo::entry(HELD, 7);
    let n = strings.start(&info).expect("it waits");
    c.upkeep();
    assert_eq!(asked(&c), vec![PRIVATE]);
    c.upkeep();
    assert!(strings.waiting(n), "it waits for the private table");
    send(&mut c, 1, PRIVATE, &table(&[(7, 0, "From the server")]));
    c.upkeep();
    assert!(!strings.waiting(n), "the table came");
    assert_eq!(written(&c, &info), "From the server");
    assert_eq!(asked(&c), vec![PRIVATE], "asked once");
}

/// A refused private table ends the wait; the reference then writes as the client's error message
/// naming its token and table, and the refusal is kept for the session: the same reference given
/// again asks for nothing more.
///
/// Behaviour: data.string-download.a-refused-table-shows-the-error-message
#[test]
fn a_refused_table_ends_the_wait_and_the_reference_shows_the_clients_error_message() {
    let mut c = client();
    let strings = c.data_download().strings().clone();
    let info = StringInfo::entry(HELD, 7);
    let n = strings.start(&info).expect("it waits");
    c.upkeep();
    refuse(&mut c, 1, PRIVATE);
    c.upkeep();
    assert!(!strings.waiting(n), "the refusal ends the wait");
    assert_eq!(
        written(&c, &info),
        "<string table error; token: 0x00000007 tableDID: 0x25000001>"
    );
    let again = strings.start(&info).expect("its table is still missing");
    c.upkeep();
    c.upkeep();
    assert!(
        !strings.waiting(again),
        "the refused table is not waited for"
    );
    assert_eq!(asked(&c), vec![PRIVATE], "nor asked for again");
}

/// A reference whose own table the files lack asks for it; once it comes, the private table the
/// reference's entry in it names is asked for too, and the reference waits for both.
///
/// Behaviour: data.string-download.a-table-the-files-lack-is-asked-for-then-its-private-table
#[test]
fn a_table_the_files_lack_is_asked_for_and_then_the_private_table_its_entry_names() {
    let mut c = client();
    let strings = c.data_download().strings().clone();
    let info = StringInfo::entry(MISSING, 7);
    let n = strings.start(&info).expect("it waits");
    c.upkeep();
    assert_eq!(asked(&c), vec![MISSING]);
    send(&mut c, 1, MISSING, &table(&[(7, MISSING_PRIVATE, "")]));
    c.upkeep();
    assert_eq!(asked(&c), vec![MISSING, MISSING_PRIVATE]);
    c.upkeep();
    assert!(strings.waiting(n), "it waits for the private table");
    send(&mut c, 2, MISSING_PRIVATE, &table(&[(7, 0, "At last")]));
    c.upkeep();
    assert!(!strings.waiting(n));
    assert_eq!(written(&c, &info), "At last");
}

/// A download dropped while it waits lets its batch go: the download is forgotten and nobody is
/// told of the table when it comes, which is stored as any other.
///
/// Behaviour: data.string-download.a-dropped-download-lets-its-batch-go
#[test]
fn a_download_dropped_while_it_waits_lets_its_batch_go() {
    let mut c = client();
    let strings = c.data_download().strings().clone();
    let info = StringInfo::entry(HELD, 7);
    let n = strings.start(&info).expect("it waits");
    c.upkeep();
    assert_eq!(c.data_download().requests.pending(), vec![q(PRIVATE)]);
    assert_eq!(c.data_download().requests.live_batches(), 1);
    strings.release(n);
    c.upkeep();
    assert_eq!(c.data_download().requests.live_batches(), 0);
    assert!(strings.is_empty());
    send(&mut c, 1, PRIVATE, &table(&[(7, 0, "Stored")]));
    c.upkeep();
    assert!(strings.is_empty());
    assert_eq!(written(&c, &info), "Stored");
}
