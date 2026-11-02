//! Every recorded data download through the client: the server's data messages of each recorded
//! session handed to a client over the end-of-retail files, each record stored and read back.

use std::net::SocketAddr;
use std::sync::Arc;

use auberean_client_sdk::protocol::{ClientMessage, ServerMessage};
use auberean_client_sdk::runtime::data::{DataFiles, ENGLISH};
use auberean_client_sdk::{Client, ClientData, ClientSettings};
use auberean_dat::disk::download::cache_pack;
use auberean_dat::locate::install_dats;
use auberean_dat::testing::require_target_install;
use auberean_dat::{DatFile, QualifiedId};
use auberean_testkit::captures;

/// A client over `data`, its session never started.
fn client(data: &ClientData) -> Client {
    let settings = ClientSettings {
        login_server: SocketAddr::from(([127, 0, 0, 1], 9000)),
        account: Vec::new(),
        ticket: Vec::new(),
        timestamp: 0,
        session_key: [0; 64],
        script_sends: false,
    };
    Client::new(settings, data.clone())
}

/// Every data file of the end-of-retail install, in the install's order.
fn all_files() -> ClientData {
    let dir = require_target_install();
    let files = install_dats(&dir)
        .expect("list the install")
        .into_iter()
        .map(|(_, p)| Arc::new(DatFile::open(&p).expect("a data file opens")))
        .collect();
    ClientData {
        files: DataFiles {
            files,
            language: ENGLISH,
        },
        scripts: None,
    }
}

const DATA_QUEUE: [u32; 6] = [0x10035, 0x10036, 0x10037, 0x10039, 0x1003A, 0x1003B];

#[derive(Debug, Default, PartialEq, Eq)]
struct Tally {
    sessions: u64,
    records: u64,
    stored: u64,
    read_back: u64,
    asked_again: u64,
    refused: u64,
    fatal: u64,
    downloads: u64,
    downloads_ended_by_the_client: u64,
}

/// Every recorded session's data messages (2005, 2013, 2017: 242 sessions), each session through a client of
/// its own: every record the server sent unpacks, stores in the file it names and reads back as
/// it came; no store fails; the one recorded download (2013) is followed to its end, every record
/// it waited for checked off and the end said.
///
/// Behaviour: data.download.records-stored-and-checked-off
#[test]
fn every_recorded_data_record_stores_and_reads_back() {
    let data = all_files();
    let index = auberean_testkit::data::require_capture_index();
    let mut t = Tally::default();
    let mut session = -1;
    let mut c: Option<Client> = None;
    let finish = |c: Option<Client>, t: &mut Tally| {
        if let Some(c) = c {
            t.fatal += u64::from(c.data_download().fatal.is_some());
            for s in c.sent() {
                match s.message {
                    ClientMessage::DataRequest(_) => t.asked_again += 1,
                    ClientMessage::DownloadEnds(_) => t.downloads_ended_by_the_client += 1,
                    _ => {}
                }
            }
        }
    };
    captures::messages(&index, &DATA_QUEUE, &[2005, 2013, 2017], |m| {
        if m.session != session {
            finish(c.take(), &mut t);
            session = m.session;
            c = Some(client(&data));
            t.sessions += 1;
        }
        let Some(client) = c.as_mut() else { return };
        let Ok(msg) = ServerMessage::decode(&m.raw) else {
            return;
        };
        let record = match &msg {
            ServerMessage::DataRecord(r) => Some(r.clone()),
            ServerMessage::DataRefused(_) => {
                t.refused += 1;
                None
            }
            ServerMessage::DownloadBegins(_) => {
                t.downloads += 1;
                None
            }
            _ => None,
        };
        client.take_message(m.blob_id, msg);
        if let Some(r) = record {
            t.records += 1;
            let Some(bytes) = cache_pack(r.size, &r.bytes) else {
                return;
            };
            t.stored += 1;
            let q = QualifiedId {
                id: r.id,
                kind: r.kind,
            };
            let back = client
                .data_download()
                .stores()
                .and_then(|s| s.get_disk_controller(q, r.dat_file))
                .and_then(|s| s.load_data(r.id));
            t.read_back += u64::from(back.as_deref() == Some(&bytes[..]));
        }
    })
    .expect("the capture index reads");
    finish(c.take(), &mut t);
    eprintln!("{t:?}");
    assert_eq!(t.fatal, 0, "no record failed to store");
    assert_eq!(
        t.read_back, t.stored,
        "every stored record reads back as it came"
    );
    assert_eq!(t.stored + t.asked_again, t.records);
    assert_eq!(t.downloads, 1);
    assert_eq!(t.downloads_ended_by_the_client, 1);
    // The 2009 captures are an emulator's, not retail's: not among them.
    assert_eq!(
        t,
        Tally {
            sessions: 242,
            records: 1167,
            stored: 1167,
            read_back: 1167,
            asked_again: 0,
            refused: 6050,
            fatal: 0,
            downloads: 1,
            downloads_ended_by_the_client: 1,
        },
    );
}
