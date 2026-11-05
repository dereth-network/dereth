//! The data download through the runtime, start to finish: the server's question answered from
//! the files, a download's drops made and records stored, the end said, the files' new versions
//! answered next time, and the connecting screen told each step.

use std::sync::Arc;

use auberean_client_runtime::data::{DataFiles, ENGLISH};
use auberean_client_runtime::download::SERVER_LIBRARY_ITERATIONS;
use auberean_client_runtime::{Client, ClientData, ClientSettings};
use auberean_dat::disk::controller::OPEN_CREATE;
use auberean_dat::disk::download::event;
use auberean_dat::disk::{DatStorageMut, Files, MemoryFile, OpenMode, OpenSpec, Store, VersionSet};
use auberean_dat::DatFile;
use auberean_protocol::login::{DataQuery, DataRecord, DownloadBegins, Empty, MissingIterations};
use auberean_protocol::types::IterationRun;
use auberean_protocol::{ClientMessage, ServerMessage};

const PORTAL: u64 = 0x1_0000_0000;
const CELL: u64 = 0x2_0000_0001;
/// A blob on the login connection, taken as it comes.
const LOGIN: u64 = 0x2300_0000_0000_0000;

/// One new file in memory.
struct One(MemoryFile);

impl Files for One {
    fn open(&mut self, _: &str, _: OpenMode) -> Option<Box<dyn DatStorageMut>> {
        Some(Box::new(self.0.clone()))
    }
}

/// A data file of this id holding versions 1 and 2, and the given records.
pub(super) fn file(id: u64, records: &[u32]) -> Arc<DatFile> {
    let records: Vec<(u32, Vec<u8>)> = records.iter().map(|&r| (r, b"old".to_vec())).collect();
    file_with(id, &records)
}

/// A data file of this id holding versions 1 and 2, and these records with these bytes.
pub(super) fn file_with(id: u64, records: &[(u32, Vec<u8>)]) -> Arc<DatFile> {
    let mut one = One(MemoryFile::default());
    let mut s = Store::new(Box::new(|| 0x5000_0000));
    let mut spec = OpenSpec::new(
        "f",
        "",
        u32::try_from(id >> 32).unwrap(),
        u32::try_from(id & 0xFFFF_FFFF).unwrap(),
        OPEN_CREATE,
        0x400 + 128 * 0x100,
        0x100,
        -1,
        -1,
        false,
    );
    assert!(s.init_file(&mut one, &mut spec));
    let mut v = VersionSet::new();
    v.add(1);
    v.add(2);
    assert!(s.save_iteration_list(&mut v));
    for (r, bytes) in records {
        assert!(s.save_data(*r, bytes));
    }
    s.close();
    Arc::new(DatFile::from_bytes(one.0.bytes()).unwrap())
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
                files: vec![
                    file(PORTAL, &[]),
                    file(CELL, &[0x0101_FFFF, 0x0101_FFFE, 0x0102_FFFF]),
                ],
                language: ENGLISH,
            },
            scripts: None,
        },
    )
}

/// The versions each file was answered with, the last time the client was asked, less those it
/// lists for a server's library whatever it holds.
fn last_held(c: &Client) -> Vec<(u64, Vec<i32>)> {
    answered(c)
        .into_iter()
        .map(|(f, mut v)| {
            v.retain(|x| !SERVER_LIBRARY_ITERATIONS.contains(x));
            (f, v)
        })
        .collect()
}

/// The versions each file was answered with, the last time the client was asked.
fn answered(c: &Client) -> Vec<(u64, Vec<i32>)> {
    let held = c
        .sent()
        .iter()
        .rev()
        .find_map(|s| match &s.message {
            ClientMessage::DataHeld(h) => Some(h.clone()),
            _ => None,
        })
        .expect("an answer");
    assert_eq!(held.language, ENGLISH);
    held.files
        .iter()
        .map(|f| {
            let mut v = Vec::new();
            for r in &f.iterations.runs {
                match *r {
                    IterationRun::One(x) => v.push(x),
                    IterationRun::Run { count, first } => {
                        v.extend((0..count).map(|k| first.cast_signed() + k.cast_signed()));
                    }
                }
            }
            (f.dat_file, v)
        })
        .collect()
}

fn ends_sent(c: &Client) -> usize {
    c.sent()
        .iter()
        .filter(|s| matches!(s.message, ClientMessage::DownloadEnds(_)))
        .count()
}

/// Asked which data it holds, the client lists the iterations of a server's library for its portal
/// file, beside those the file has had; no other file's list changes, and the file's own list stays
/// as it was, so a later question is answered the same.
/// Behaviour: data.download.lists-a-servers-library-as-held
#[test]
fn the_portal_file_is_listed_as_holding_a_servers_library() {
    let mut c = client();
    let mut portal: Vec<i32> = vec![1, 2];
    portal.extend(SERVER_LIBRARY_ITERATIONS);
    for blob in 1..=2 {
        c.take_message(LOGIN | blob, ServerMessage::DataQuery(DataQuery::default()));
        assert_eq!(
            answered(&c),
            vec![(PORTAL, portal.clone()), (CELL, vec![1, 2])]
        );
    }
}

/// Behaviour: data.download.records-stored-and-checked-off
#[test]
fn a_download_runs_through_the_client_from_the_question_to_the_end() {
    let mut c = client();
    c.take_message(LOGIN | 1, ServerMessage::DataQuery(DataQuery::default()));
    assert_eq!(
        last_held(&c),
        vec![(PORTAL, vec![1, 2]), (CELL, vec![1, 2])]
    );
    // The portal waits for one record; the cell file drops a landblock and takes its version.
    c.take_message(
        LOGIN | 2,
        ServerMessage::DownloadBegins(DownloadBegins {
            bytes: 9,
            files: vec![
                MissingIterations {
                    dat_file: PORTAL,
                    iteration: 3,
                    download: vec![0x0600_0001],
                    purge: vec![],
                },
                MissingIterations {
                    dat_file: CELL,
                    iteration: 4,
                    download: vec![],
                    purge: vec![0x0101_FFFF],
                },
            ],
        }),
    );
    assert_eq!(ends_sent(&c), 0, "a record is waited for");
    let kind = auberean_dat::type_of_id(0x0600_0001).unwrap().0;
    c.take_message(
        LOGIN | 3,
        ServerMessage::DataRecord(DataRecord {
            dat_file: PORTAL,
            kind,
            id: 0x0600_0001,
            size: 0,
            bytes: b"new".to_vec(),
        }),
    );
    assert_eq!(
        ends_sent(&c),
        1,
        "the last record ends the download from the client's side"
    );
    c.take_message(LOGIN | 4, ServerMessage::DownloadEnds(Empty));
    let d = c.data_download();
    assert_eq!(d.fatal, None);
    assert_eq!(
        d.events,
        vec![(event::PATCHING, 9), (event::PROGRESS, 7), (event::DONE, 0)]
    );
    // Asked again: the new versions, as stored.
    c.take_message(LOGIN | 5, ServerMessage::DataQuery(DataQuery::default()));
    assert_eq!(
        last_held(&c),
        vec![(PORTAL, vec![1, 2, 3]), (CELL, vec![1, 2, 4])]
    );
}

/// Behaviour: data.download.a-bad-record-is-asked-again
#[test]
fn a_record_that_does_not_inflate_is_asked_for_again() {
    let mut c = client();
    c.take_message(
        LOGIN | 1,
        ServerMessage::DataRecord(DataRecord {
            dat_file: PORTAL,
            kind: 0x4000_0006,
            id: 0x0600_0009,
            size: 40,
            bytes: vec![0x78, 0x9C, 1, 2, 3, 4],
        }),
    );
    let asked: Vec<_> = c
        .sent()
        .iter()
        .filter_map(|s| match &s.message {
            ClientMessage::DataRequest(r) => Some((r.kind, r.id)),
            _ => None,
        })
        .collect();
    assert_eq!(asked, vec![(0x4000_0006, 0x0600_0009)]);
}

/// The records the client asked the server for, in order.
fn asked(c: &Client) -> Vec<(u32, u32)> {
    c.sent()
        .iter()
        .filter_map(|s| match &s.message {
            ClientMessage::DataRequest(r) => Some((r.kind, r.id)),
            _ => None,
        })
        .collect()
}

/// Behaviour: data.requests.missing-records-are-asked-of-the-server
#[test]
fn a_record_asked_for_that_no_file_holds_is_asked_of_the_server_and_its_record_or_refusal_finishes_it(
) {
    use auberean_assets::requests::{Event, Listener, Outcome, GATHER_NONE};
    use auberean_assets::tables::IdMap;
    use auberean_protocol::login::DataRefused;

    let mut c = client();
    let map = |n: u32| auberean_dat::QualifiedId {
        id: 0x2800_0000 + n,
        kind: auberean_dat::RecordType::ID_MAP.0,
    };
    let to = Listener { to: 1, user: 5 };
    let got = c.data_download_mut().get(GATHER_NONE, map(1), Some(to));
    let refused = c.data_download_mut().get(GATHER_NONE, map(2), Some(to));
    c.upkeep();
    let kind = auberean_dat::RecordType::ID_MAP.0;
    assert_eq!(asked(&c), vec![(kind, map(1).id), (kind, map(2).id)]);
    // The record comes, is stored, and finishes the first; the second is refused.
    c.take_message(
        LOGIN | 1,
        ServerMessage::DataRecord(DataRecord {
            dat_file: PORTAL,
            kind,
            id: map(1).id,
            size: 0,
            bytes: auberean_assets::encode(map(1).id, &IdMap::default()),
        }),
    );
    c.take_message(
        LOGIN | 2,
        ServerMessage::DataRefused(DataRefused {
            kind,
            id: map(2).id,
            error: 1,
        }),
    );
    c.upkeep();
    let told = |record, outcome| Event::Finished {
        to: 1,
        record,
        outcome,
        user: 5,
        times: 1,
    };
    let done = |batch, outcome| Event::BatchDone {
        to: 1,
        batch,
        outcome,
        user: 5,
    };
    assert_eq!(
        c.data_download().told,
        vec![
            told(map(1), Outcome::Success),
            told(map(2), Outcome::Failure),
            done(got, Outcome::Success),
            done(refused, Outcome::Failure),
        ]
    );
    let record = c
        .data_download_mut()
        .get_now(GATHER_NONE, map(1))
        .expect("the record the server sent");
    assert!(record.downcast::<IdMap>().is_ok());
}
