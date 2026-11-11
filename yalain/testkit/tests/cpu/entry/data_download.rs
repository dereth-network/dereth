//! The data records a client asks for at the character list, served from data files held in
//! memory: the record from the file its kind lives in, compressed when that is smaller, a
//! landblock with its information, a refusal for what the files do not hold.

use std::collections::BTreeMap;
use std::io::Read as _;

use auberean_dat::DatKind;
use auberean_protocol::login::{DataRecord, DataRefused, DataRequest};
use auberean_protocol::{queue, ClientMessage, ServerMessage};
use yalain_testkit::Harness;
use yalain_world::download::{DataFiles, CELL_FILE, ENGLISH_FILE, PORTAL_FILE};
use yalain_world::reload::WorldTables;

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

const SCRIPT_STATE: u32 = 0x4000_002F;

/// A recipe the world defines is served from its record as the portal file would hold it, where
/// the server's files lack it (and with no files at all); a record the files hold of the same id is
/// served in its place; an id neither holds is refused.
/// Behaviour: server.data.world-recipes-served
#[test]
fn a_recipe_the_world_defines_is_served_from_its_record_where_the_files_lack_one() {
    const MADE: u32 = 0x6F7E_0001;
    const HELD: u32 = 0x4100_4B93;
    for data in [true, false] {
        let mut h = Harness::new();
        if data {
            h.server.data = Some(Box::new(files()));
        }
        let made = short_record(MADE);
        h.server.crafting.records.insert(MADE, made.clone());
        h.server.crafting.records.insert(HELD, short_record(0));
        let b = at_character_list(&mut h, "tester");
        let held = if data {
            short_record(HELD)
        } else {
            short_record(0)
        };
        for (id, want) in [(MADE, Some(made)), (HELD, Some(held)), (0x6F7E_0002, None)] {
            let before = h.bots[b].inbox.len();
            let m = ClientMessage::DataRequest(DataRequest {
                kind: SCRIPT_STATE,
                id,
            });
            h.bots[b].send(false, queue::DATABASE, &m);
            assert!(h.run_until(STEPS, |h| h.bots[b].inbox.len() > before));
            h.run(20);
            let got: Vec<ServerMessage> = h.bots[b].inbox[before..]
                .iter()
                .filter_map(|r| r.message())
                .collect();
            assert_eq!(got.len(), 1, "{id:08X}: one answer");
            match (&got[0], want) {
                (ServerMessage::DataRecord(r), Some(w)) => {
                    assert_eq!((r.dat_file, r.kind, r.id), (PORTAL_FILE, SCRIPT_STATE, id));
                    assert_eq!(inflated(r), w, "{id:08X}");
                }
                (ServerMessage::DataRefused(r), None) => assert_eq!(r.id, id),
                (other, w) => panic!("{id:08X} (files {data}): {other:?}, wanted {w:?}"),
            }
        }
    }
}

/// How a client logging in tells of its data in [`world_recipes_at_login`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Told {
    /// A retail client holding nothing past its files' own.
    Nothing,
    /// A retail client holding the world's recipes' iteration.
    TheirIteration,
    /// A client that keeps overlays.
    KeepsOverlays,
}

/// A client logging in with the operator's retail patching on or off, told of its data: what came
/// before its character list, and whether the list waited for it to store them.
fn world_recipes_at_login(
    records: &[(u32, Vec<u8>)],
    patching: bool,
    told: Told,
) -> (Vec<ServerMessage>, bool) {
    use auberean_protocol::login::{DataFileIterations, DataHeld, Empty, OverlayHeld};
    use auberean_protocol::types::{IterationRun, IterationSet};
    let mut h = Harness::new();
    h.server.cfg.dat_overlay.enable_dat_patching = patching;
    h.server.crafting.records.extend(records.iter().cloned());
    let iteration = yalain_world::recipe_download::iteration(&h.server.crafting.records);
    let b = h.logging_in("tester");
    if told == Told::KeepsOverlays {
        let keeps = OverlayHeld {
            flags: 0,
            bases: Vec::new(),
        };
        h.bots[b].send(false, queue::DATABASE, &ClientMessage::OverlayHeld(keeps));
    }
    let files = if told == Told::TheirIteration {
        vec![DataFileIterations {
            dat_file: PORTAL_FILE,
            iterations: IterationSet {
                total: 138,
                runs: vec![
                    IterationRun::Run {
                        count: 137,
                        first: 1,
                    },
                    IterationRun::One(iteration.cast_signed()),
                ],
            },
        }]
    } else {
        Vec::new()
    };
    let before = h.bots[b].inbox.len();
    let held = ClientMessage::DataHeld(DataHeld {
        language: 0x8000_0001,
        files,
    });
    h.bots[b].send(false, queue::DATABASE, &held);
    h.run(20);
    let got: Vec<ServerMessage> = h.bots[b].inbox[before..]
        .iter()
        .filter_map(|r| r.message())
        .take_while(|m| !matches!(m, ServerMessage::CharacterList(_)))
        .collect();
    let listed = |h: &Harness| {
        h.bots[b]
            .messages(false)
            .iter()
            .any(|m| matches!(m, ServerMessage::CharacterList(_)))
    };
    let waited = !listed(&h);
    if waited {
        h.bots[b].send(false, queue::DATABASE, &ClientMessage::DownloadEnds(Empty));
        assert!(h.run_until(STEPS, listed), "listed once it has stored them");
    }
    (got, waited)
}

/// A retail client never asks for a recipe's record, so the world's recipes go to it as it logs
/// in, when the operator patches retail clients: the download begins with their ids under the
/// portal file's iteration for them, each record follows as the file would hold it, and its
/// characters wait until it has stored them. A client listing that iteration, one that keeps
/// overlays (it asks), a world with no recipes of its own, and any client with retail patching off
/// are sent none, and listed at once.
/// Behaviour: server.data.world-recipes-sent-to-retail
#[test]
fn a_retail_client_is_sent_the_worlds_recipes_as_it_logs_in_when_the_operator_patches_it() {
    use auberean_protocol::login::{DownloadBegins, MissingIterations};
    use yalain_world::recipe_download::{FIRST_ITERATION, ITERATIONS};
    let records = vec![
        (0x6F7E_1606, short_record(0x6F7E_1606)),
        (0x6F7E_1601, long_record(0x6F7E_1601)),
    ];
    let iteration = yalain_world::recipe_download::iteration(&records.iter().cloned().collect());
    assert!((FIRST_ITERATION..FIRST_ITERATION + ITERATIONS).contains(&iteration));

    let (got, waited) = world_recipes_at_login(&records, true, Told::Nothing);
    assert!(waited, "its characters wait for the recipes to be stored");
    assert_eq!(got.len(), 3, "{got:?}");
    let ServerMessage::DownloadBegins(DownloadBegins { bytes, files }) = &got[0] else {
        panic!("the download begins first, not {:?}", got[0]);
    };
    assert_eq!(
        files,
        &vec![MissingIterations {
            dat_file: PORTAL_FILE,
            iteration,
            download: vec![0x6F7E_1601, 0x6F7E_1606],
            purge: vec![],
        }]
    );
    let sent: usize = got[1..].iter().map(|m| record(m).bytes.len()).sum();
    assert_eq!(*bytes as usize, sent);
    for (m, (id, want)) in got[1..].iter().zip([&records[1], &records[0]]) {
        let r = record(m);
        assert_eq!((r.dat_file, r.kind, r.id), (PORTAL_FILE, SCRIPT_STATE, *id));
        assert_eq!(&inflated(r), want, "{id:08X}");
    }

    for (records, patching, told, case) in [
        (
            &records[..],
            true,
            Told::TheirIteration,
            "it lists their iteration",
        ),
        (&records[..], true, Told::KeepsOverlays, "it keeps overlays"),
        (&records[..], false, Told::Nothing, "retail patching off"),
        (
            &[][..],
            true,
            Told::Nothing,
            "no recipes of the world's own",
        ),
    ] {
        let (got, waited) = world_recipes_at_login(records, patching, told);
        assert!(!waited, "{case}: listed at once");
        assert!(
            !got.iter().any(|m| matches!(
                m,
                ServerMessage::DownloadBegins(_) | ServerMessage::DataRecord(_)
            )),
            "{case}: sent none: {got:?}"
        );
    }
}

/// The world's recipes go out under one iteration for one set of records, and another when any
/// record changes, is added or is taken away.
/// Behaviour: server.data.world-recipes-sent-to-retail
#[test]
fn the_worlds_recipes_go_out_under_another_iteration_when_they_change() {
    use std::collections::HashMap;
    use yalain_world::recipe_download::iteration;
    let one: HashMap<u32, Vec<u8>> = [
        (0x6F7E_1601, short_record(1)),
        (0x6F7E_1604, short_record(4)),
    ]
    .into();
    let mut changed = one.clone();
    changed.insert(0x6F7E_1604, short_record(5));
    let mut more = one.clone();
    more.insert(0x6F7E_1606, short_record(6));
    let mut fewer = one.clone();
    fewer.remove(&0x6F7E_1604);
    assert_eq!(iteration(&one), iteration(&one.clone()));
    for other in [&changed, &more, &fewer] {
        assert_ne!(iteration(&one), iteration(other));
    }
}

const ROOM: u32 = 0x4000_0026;

/// A record of this many bytes that compresses well.
fn plain(id: u32, n: usize) -> Vec<u8> {
    let mut v = id.to_le_bytes().to_vec();
    v.resize(n, 9);
    v
}

/// A client at its list asking for these, all in one pass of the world.
fn asked(h: &mut Harness, b: usize, ids: impl IntoIterator<Item = u32>) {
    for id in ids {
        let r = ClientMessage::DataRequest(DataRequest { kind: ROOM, id });
        h.bots[b].send(false, queue::DATABASE, &r);
    }
}

fn refusals(h: &Harness, b: usize, from: usize) -> usize {
    h.bots[b].inbox[from..]
        .iter()
        .filter(|r| matches!(r.message(), Some(ServerMessage::DataRefused(_))))
        .count()
}

const MIB: usize = 1024 * 1024;

/// The most a budget lets compress over three steps, one record of [`MIB`] begun past its end.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn most_compressed(at_once: f64, per_second: f64) -> u64 {
    (at_once + 3.0 * yalain_testkit::STEP * per_second) as u64 + MIB as u64
}

fn long_records(n: u32, each: usize) -> BTreeMap<(DatKind, u32), Vec<u8>> {
    (0..n)
        .map(|i| ((DatKind::Local, 0x2500_0000 + i), plain(i, each)))
        .collect()
}

/// A very long record is compressed before the world runs: however often it is asked for, a pass
/// of the world compresses nothing for it, and every answer inflates to the record.
/// Behaviour: server.data.answers-never-hold-up-the-world
#[test]
fn a_very_long_record_is_compressed_once_before_the_world_runs() {
    let id = 0x2500_0000;
    let record = plain(id, yalain_world::download::PREPARED_FROM);
    let mut h = Harness::new();
    let mut f = BTreeMap::new();
    f.insert((DatKind::Local, id), record.clone());
    h.server.data = Some(Box::new(f));
    yalain_world::download::prepare(&mut h.server);
    let b = at_character_list(&mut h, "one");
    let from = h.bots[b].inbox.len();
    asked(&mut h, b, [id; 5]);
    h.run(10);
    assert_eq!(
        h.server.downloads.compressed, 0,
        "nothing compressed in a pass"
    );
    let answers: Vec<DataRecord> = h.bots[b].inbox[from..]
        .iter()
        .filter_map(|r| match r.message() {
            Some(ServerMessage::DataRecord(d)) => Some(d),
            _ => None,
        })
        .collect();
    assert_eq!(answers.len(), 5, "each is answered");
    for d in answers {
        let mut back = Vec::new();
        flate2::read::ZlibDecoder::new(&d.bytes[..])
            .read_to_end(&mut back)
            .expect("inflates");
        assert_eq!(back, record);
    }
}

/// What one connection's answers may compress at once has a limit, and what is past it is refused;
/// time fills it again.
/// Behaviour: server.data.answers-never-hold-up-the-world
#[test]
fn what_a_connections_answers_compress_at_once_has_a_limit() {
    let mut h = Harness::new();
    h.server.data = Some(Box::new(long_records(40, MIB)));
    let b = at_character_list(&mut h, "one");
    let from = h.bots[b].inbox.len();
    asked(&mut h, b, (0..40).map(|i| 0x2500_0000 + i));
    h.run(3);
    let most = most_compressed(
        yalain_world::download::CONNECTION_AT_ONCE,
        yalain_world::download::CONNECTION_PER_SECOND,
    );
    let done = h.server.downloads.compressed;
    assert!(done >= 8 * MIB as u64, "up to the limit: {done}");
    assert!(done <= most, "no further: {done} of {most}");
    h.run(10);
    assert!(refusals(&h, b, from) > 0, "the rest are refused");
    h.run_to(yalain_net::LocalTime(h.now.0 + 5.0));
    let from = h.bots[b].inbox.len();
    asked(&mut h, b, [0x2500_0000]);
    h.run(10);
    assert_eq!(refusals(&h, b, from), 0, "answered again in a while");
}

/// So has what all the world's answers compress at once, however many connections ask.
/// Behaviour: server.data.answers-never-hold-up-the-world
#[test]
fn what_all_the_worlds_answers_compress_at_once_has_a_limit() {
    let mut h = Harness::new();
    h.server.data = Some(Box::new(long_records(8, MIB)));
    let clients: Vec<usize> = ["a1", "a2", "a3", "a4", "a5", "a6"]
        .map(|account| at_character_list(&mut h, account))
        .to_vec();
    for b in &clients {
        asked(&mut h, *b, (0..8).map(|i| 0x2500_0000 + i));
    }
    h.run(3);
    let most = most_compressed(
        yalain_world::download::WORLD_AT_ONCE,
        yalain_world::download::WORLD_PER_SECOND,
    );
    let done = h.server.downloads.compressed;
    assert!(done >= 16 * MIB as u64, "up to the limit: {done}");
    assert!(done <= most, "no further: {done} of {most}");
}

/// What a connection has waiting to be sent has a limit too: past it, requests are let go and
/// nothing more is queued.
/// Behaviour: server.data.answers-never-hold-up-the-world
#[test]
fn what_a_connection_has_waiting_to_be_sent_has_a_limit() {
    // A record that does not compress, of the length prepared at the start.
    let id = 0x2500_0000u32;
    let mut x = 0x2545_F491_4F6C_DD1Du64;
    let mut record = Vec::with_capacity(yalain_world::download::PREPARED_FROM + 8);
    while record.len() < yalain_world::download::PREPARED_FROM {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        record.extend_from_slice(&x.to_le_bytes());
    }
    let mut h = Harness::new();
    let mut f = BTreeMap::new();
    f.insert((DatKind::Local, id), record);
    h.server.data = Some(Box::new(f));
    yalain_world::download::prepare(&mut h.server);
    let b = at_character_list(&mut h, "one");
    let conn = *h.server.sessions.keys().next().expect("logged in");
    asked(&mut h, b, [id; 40]);
    h.run(4);
    let one = yalain_world::download::PREPARED_FROM / 400;
    let waiting = h.net.large_waiting(conn);
    assert!(
        waiting >= yalain_world::download::MOST_WAITING / 2,
        "a good part of the limit: {waiting}"
    );
    assert!(
        waiting <= yalain_world::download::MOST_WAITING + one,
        "no further: {waiting}"
    );
    asked(&mut h, b, [id; 40]);
    h.run(4);
    assert!(
        h.net.large_waiting(conn) <= waiting,
        "nothing more is queued"
    );
}

/// Files with a world's overlay over them, which holds a word on every record they have.
#[derive(Debug)]
struct Overlaid(BTreeMap<(DatKind, u32), Vec<u8>>);

impl DataFiles for Overlaid {
    fn read(&self, file: DatKind, id: u32) -> Option<Vec<u8>> {
        self.0.read(file, id)
    }

    fn very_long(&self) -> Vec<(DatKind, u32)> {
        self.0.very_long()
    }

    fn overlaid(&self, _file: DatKind, _id: u32) -> bool {
        true
    }
}

/// The record a client gets when it asks for `id` of a kind that lives in the language's file.
fn answer(h: &mut Harness, id: u32) -> Vec<u8> {
    let b = at_character_list(h, "one");
    let from = h.bots[b].inbox.len();
    asked(h, b, [id]);
    h.run(10);
    let d = h.bots[b].inbox[from..]
        .iter()
        .find_map(|r| match r.message() {
            Some(ServerMessage::DataRecord(d)) => Some(d),
            _ => None,
        })
        .expect("it is answered");
    let mut back = Vec::new();
    if d.size == 0 {
        back = d.bytes;
    } else {
        flate2::read::ZlibDecoder::new(&d.bytes[..])
            .read_to_end(&mut back)
            .expect("inflates");
    }
    back
}

/// A reload that opened the files again answers from them from then on, and a very long record a
/// world's overlay holds is read for each answer: compressed before the world ran, it would still
/// be answered as it was.
/// Behaviour: server.data.a-reload-answers-from-the-overlay-as-it-now-is
#[test]
fn a_reload_answers_from_the_overlay_as_it_now_is() {
    let (id, n) = (0x2500_0000, yalain_world::download::PREPARED_FROM);
    let of = |fill: u8| {
        let mut v = plain(id, n);
        v[n - 1] = fill;
        let mut f = BTreeMap::new();
        f.insert((DatKind::Local, id), v);
        f
    };
    let mut h = Harness::new();
    h.server.data = Some(Box::new(Overlaid(of(1))));
    yalain_world::download::prepare(&mut h.server);
    assert_eq!(answer(&mut h, id)[n - 1], 1, "the overlay as it was");
    assert!(
        h.server.downloads.compressed > 0,
        "nothing was kept for it before the world ran"
    );

    let t = WorldTables {
        files: Some(Box::new(Overlaid(of(2)))),
        ..WorldTables::default()
    };
    let clock = h.clock();
    h.server.reload(&mut h.net, clock, t);
    assert_eq!(answer(&mut h, id)[n - 1], 2, "the overlay as it now is");
}

/// What a world's overlay holds no word on stays compressed from before the world ran, whatever a
/// reload opens: the install's files don't change.
/// Behaviour: server.data.a-reload-answers-from-the-overlay-as-it-now-is
#[test]
fn a_very_long_record_no_overlay_holds_stays_compressed_over_a_reload() {
    let (id, n) = (0x2500_0000, yalain_world::download::PREPARED_FROM);
    let mut f = BTreeMap::new();
    f.insert((DatKind::Local, id), plain(id, n));
    let mut h = Harness::new();
    h.server.data = Some(Box::new(f.clone()));
    yalain_world::download::prepare(&mut h.server);
    let t = WorldTables {
        files: Some(Box::new(f)),
        ..WorldTables::default()
    };
    let clock = h.clock();
    h.server.reload(&mut h.net, clock, t);
    assert_eq!(answer(&mut h, id).len(), n);
    assert_eq!(
        h.server.downloads.compressed, 0,
        "nothing compressed in a pass"
    );
}
