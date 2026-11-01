//! The data records a client asks for at the character list, served from data files held in
//! memory: the record from the file its kind lives in, compressed when that is smaller, a
//! landblock with its information, a refusal for what the files do not hold.

use std::collections::BTreeMap;
use std::io::Read as _;

use auberean_dat::DatKind;
use auberean_protocol::login::{DataRecord, DataRefused, DataRequest};
use auberean_protocol::{queue, ClientMessage, ServerMessage};
use yalain_testkit::Harness;
use yalain_world::download::{CELL_FILE, ENGLISH_FILE, PORTAL_FILE};

use super::character_select::{at_character_list, STEPS};

const STRING_TABLE: u32 = 0x4000_0026;
const PICTURE: u32 = 0x4000_003C;
const LANDBLOCK: u32 = 0x4000_0050;
const LANDBLOCK_INFO: u32 = 0x4000_0051;
const LIGHT_INFO: u32 = 0x4000_0059;

/// A record that compresses well, and one that does not (no byte repeats).
fn long_record(id: u32) -> Vec<u8> {
    let mut v = id.to_le_bytes().to_vec();
    v.extend(std::iter::repeat_n(7u8, 400));
    v
}

fn short_record(id: u32) -> Vec<u8> {
    let mut v = id.to_le_bytes().to_vec();
    v.extend([1u8, 2, 3, 5, 8, 13, 21, 34]);
    v
}

fn files() -> BTreeMap<(DatKind, u32), Vec<u8>> {
    let mut f = BTreeMap::new();
    f.insert((DatKind::Local, 0x2504_13CF), long_record(0x2504_13CF));
    f.insert((DatKind::Portal, 0x4100_4B93), short_record(0x4100_4B93));
    f.insert((DatKind::Cell, 0x3F9E_FFFF), long_record(0x3F9E_FFFF));
    f.insert((DatKind::Cell, 0x3F9E_FFFE), short_record(0x3F9E_FFFE));
    // Held in another file than its kind's: a request for it is still refused.
    f.insert((DatKind::Portal, 0x3F9E_FFFC), short_record(0x3F9E_FFFC));
    f
}

/// Ask for one record at the character list; the answers the server sent.
fn ask(kind: u32, id: u32, data: bool) -> Vec<(u16, ServerMessage)> {
    let mut h = Harness::new();
    if data {
        h.server.data = Some(Box::new(files()));
    }
    let b = at_character_list(&mut h, "tester");
    let before = h.bots[b].inbox.len();
    let m = ClientMessage::DataRequest(DataRequest { kind, id });
    h.bots[b].send(false, queue::DATABASE, &m);
    assert!(h.run_until(STEPS, |h| h.bots[b].inbox.len() > before));
    h.run(20);
    h.bots[b].inbox[before..]
        .iter()
        .filter_map(|r| Some((r.queue, r.message()?)))
        .collect()
}

fn inflated(r: &DataRecord) -> Vec<u8> {
    if r.size == 0 {
        return r.bytes.clone();
    }
    let mut out = Vec::new();
    flate2::read::ZlibDecoder::new(&r.bytes[..])
        .read_to_end(&mut out)
        .expect("inflates");
    assert_eq!(out.len(), r.size as usize);
    out
}

fn record(m: &ServerMessage) -> &DataRecord {
    match m {
        ServerMessage::DataRecord(r) => r,
        other => panic!("a data record, not {other:?}"),
    }
}

/// A record the server's files do not hold is refused with its kind, its id and error 1, on the
/// data queue; so is one held only in another file than its kind's, and any record when the
/// server has no data files.
/// Behaviour: server.data.requests-refused
#[test]
fn a_record_the_server_does_not_hold_is_refused_with_its_kind_id_and_error_one() {
    for (kind, id, data) in [
        (LIGHT_INFO, 0x2502_0001, true),
        (LIGHT_INFO, 0x3F9E_FFFC, true),
        (STRING_TABLE, 0x2504_13CF, false),
    ] {
        let got = ask(kind, id, data);
        assert_eq!(got.len(), 1, "{id:08X}: one answer");
        assert_eq!(got[0].0, queue::DATABASE);
        let want = DataRefused { kind, id, error: 1 };
        assert_eq!(got[0].1, ServerMessage::DataRefused(want), "{id:08X}");
    }
}

/// A record the server's files hold is sent on the data queue from the file its kind lives in,
/// named as the client names that file, with its kind and id: compressed when that is smaller
/// (its size then the record's length), as it is when not (size 0).
/// Behaviour: server.data.records-served
#[test]
fn a_held_record_is_sent_from_its_kinds_file_compressed_only_when_that_is_smaller() {
    let held = files();
    for (kind, id, file, dat_file, compressed) in [
        (
            STRING_TABLE,
            0x2504_13CF,
            DatKind::Local,
            ENGLISH_FILE,
            true,
        ),
        (PICTURE, 0x4100_4B93, DatKind::Portal, PORTAL_FILE, false),
    ] {
        let got = ask(kind, id, true);
        assert_eq!(got.len(), 1, "{id:08X}: one answer");
        assert_eq!(got[0].0, queue::DATABASE);
        let r = record(&got[0].1);
        assert_eq!((r.dat_file, r.kind, r.id), (dat_file, kind, id));
        assert_eq!(r.size != 0, compressed, "{id:08X} compressed");
        assert!(r.bytes.len() <= held[&(file, id)].len());
        assert_eq!(inflated(r), held[&(file, id)], "{id:08X}");
    }
}

/// A landblock sent brings its landblock information right after it, unasked, from the cell file.
/// Behaviour: server.data.landblock-brings-its-information
#[test]
fn a_landblock_sent_brings_its_landblock_information_right_after_it() {
    let held = files();
    let got = ask(LANDBLOCK, 0x3F9E_FFFF, true);
    let rs: Vec<&DataRecord> = got.iter().map(|(_, m)| record(m)).collect();
    assert_eq!(
        rs.iter().map(|r| (r.kind, r.id)).collect::<Vec<_>>(),
        [(LANDBLOCK, 0x3F9E_FFFF), (LANDBLOCK_INFO, 0x3F9E_FFFE)]
    );
    for r in rs {
        assert_eq!(r.dat_file, CELL_FILE);
        assert_eq!(inflated(r), held[&(DatKind::Cell, r.id)]);
    }
}
