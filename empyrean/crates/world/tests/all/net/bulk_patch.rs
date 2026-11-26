//! Divergence: V519
//! A client patched to a world with a data overlay that asks for the patch as a stream
//! (`FLAG_BULK` in `0xF7E6`) is offered one (`0xF7ED`) in place of the records' `0xF7E2`s, and
//! reads every record from a TCP connection that presents the offer's token, once. What a client
//! that could not finish names in `0xF7EE` goes as `0xF7E2`s, once each, and so does a whole offer
//! it declines or nobody collects in its 120 seconds. A world with the stream off patches as
//! before, and the overlay a client takes holds the same records whichever way they came.
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and an overlay written in a scratch folder.

use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::sync::{Arc, Once};
use std::time::Duration;

use dereth_dat::overlay::{OverlayDir, OverlayWriter};
use dereth_dat::{ModernDat, RetailDatStore};
use dereth_primitives::{DataId, NetQueue};
use dereth_protocol::admin::{
    DddBeginDdd, DddBulkDecline, DddBulkOffer, DddData, DddInterrogation, DddInterrogationResponse,
    DddOverlayManifest, MostlyConsecutiveIntSet, OverlayBase, TaggedIterationList, BULK_MAGIC,
    BULK_VERSION,
};
use empyrean_common::master_configuration::MasterConfiguration;
use empyrean_dat::{DatManager, RealDats};
use empyrean_entity::enums::AccessLevel;
use empyrean_testkit::{ClientId, TestServer};

const WORLD: &str = "a streamed world";
const RECORDS: u32 = 300;
const FIRST: u32 = 0x0600_E000;
/// The most one wait on a stream may take: a guard against a hang, never the expected time.
const GUARD: Duration = Duration::from_secs(60);

/// The stream's listener, once for the whole test binary.
fn listen() -> u32 {
    static START: Once = Once::new();
    START.call_once(|| {
        // The test server's sessions play from addresses no socket here has, so a ticket opens
        // from any; the listener's own tests hold a ticket to its session's address.
        let limits = empyrean_world::managers::bulk_patch::Limits {
            same_address: false,
            ..empyrean_world::managers::bulk_patch::Limits::default()
        };
        empyrean_world::managers::bulk_patch::start(([127, 0, 0, 1], 0).into(), limits)
            .expect("the stream listens");
    });
    empyrean_world::managers::bulk_patch::port().expect("a port")
}

/// A world over the retail dats whose overlay adds [`RECORDS`] pictures, and the base files.
fn world(scratch: &Path) -> (Arc<DatManager>, RetailDatStore) {
    let dir = OverlayDir::new(&scratch.join("overlay")).expect("an overlay folder");
    let base_dir = dereth_dat::testing::dat_dir();
    let base = RetailDatStore::open_dir(&base_dir).expect("the retail dats");
    let portal = base.portal().base();
    let revision = dereth_dat::decompose::revision_over(&portal);
    let mut w = OverlayWriter::open_or_create(
        &dir.container(ModernDat::Portal),
        &portal,
        "client_portal.dat",
        WORLD,
        1,
    )
    .expect("the portal overlay");
    for n in 0..RECORDS {
        // Large enough that the server sends it compressed.
        w.save(
            &portal,
            DataId(FIRST + n),
            &vec![u8::try_from(n % 256).expect("a byte"); 4096],
            1,
            revision,
            1,
        )
        .expect("added");
    }
    w.add_iteration(revision, 1).expect("the revision");
    w.flush(1).expect("flushed");
    drop(w);
    let source = RealDats::open_era(&base_dir, dereth_primitives::ContainerEra::Modern)
        .expect("the base")
        .with_overlay(dir.path())
        .expect("the overlay over it");
    (
        DatManager::initialize(Arc::new(source)).expect("the world's dats"),
        base,
    )
}

/// A Dereth client's answer over the base files, asking for the stream when `bulk`.
fn answer(store: &RetailDatStore, bulk: bool) -> DddInterrogationResponse {
    let files = [
        (0, 1, store.portal()),
        (1, 2, store.cell()),
        (1, 3, store.local()),
    ];
    let run = |f: &dereth_dat::DatFile| {
        let n = i32::try_from(dereth_dat::overlay::base_iterations(f).len()).expect("a count");
        MostlyConsecutiveIntSet {
            iterations: n,
            ints: vec![-n, 1],
        }
    };
    DddInterrogationResponse {
        client_language: 1,
        iters_with_keys: files
            .iter()
            .map(|&(dat_file_type, dat_file_id, f)| TaggedIterationList {
                dat_file_type,
                dat_file_id,
                iterations: run(f),
            })
            .collect(),
        iters_without_keys: Vec::new(),
        flags: DddInterrogationResponse::FLAG_OVERLAY
            | if bulk {
                DddInterrogationResponse::FLAG_BULK
            } else {
                0
            },
        overlay_bases: files
            .iter()
            .map(|&(dat_file_type, dat_file_id, f)| OverlayBase {
                dat_file_type,
                dat_file_id,
                fingerprint: dereth_dat::overlay::fingerprint(f),
            })
            .collect(),
    }
}

/// A server on `dats`, one client interrogated and answered with `reply`.
fn exchange(dats: &Arc<DatManager>, reply: &DddInterrogationResponse) -> (TestServer, ClientId) {
    exchange_with(dats, reply, MasterConfiguration::default())
}

/// [`exchange`] on a server configured as `config`. Its patch state is built at start, as the
/// server builds it, with the files no overlay lies over walked once for every test.
fn exchange_with(
    dats: &Arc<DatManager>,
    reply: &DddInterrogationResponse,
    config: MasterConfiguration,
) -> (TestServer, ClientId) {
    let mut ts = TestServer::with_config(Arc::clone(dats), config, |w| {
        let d = Arc::clone(&w.dats);
        crate::support::ddd_start::initialize(&mut w.ddd_manager, &d);
    });
    ts.auth()
        .create_account(
            "acct",
            "pw",
            AccessLevel::Player,
            std::net::Ipv4Addr::LOCALHOST.into(),
        )
        .expect("created");
    let id = ts.connect("acct", "pw");
    assert!(
        ts.run_until(1.0, |ts| !ts.received::<DddInterrogation>(id).is_empty()),
        "interrogation"
    );
    ts.send_message(id, NetQueue::ClientCache, reply);
    ts.run_until(2.0, |_| false);
    (ts, id)
}

/// The stream `offer` opens: every record it carries, or `None` when the server closes it unread.
fn stream(port: u32, offer: &DddBulkOffer) -> Option<Vec<DddData>> {
    stream_with(port, &offer.token)
}

/// The stream `token` opens.
fn stream_with(port: u32, token: &[u8; 16]) -> Option<Vec<DddData>> {
    let at = SocketAddr::from(([127, 0, 0, 1], u16::try_from(port).expect("a port")));
    let mut s = TcpStream::connect_timeout(&at, GUARD).ok()?;
    s.set_read_timeout(Some(GUARD)).expect("a read deadline");
    s.set_write_timeout(Some(GUARD)).expect("a write deadline");
    s.write_all(&BULK_MAGIC).ok()?;
    s.write_all(&BULK_VERSION.to_le_bytes()).ok()?;
    s.write_all(token).ok()?;
    let mut out = Vec::new();
    loop {
        let mut len = [0u8; 4];
        fill(&mut s, &mut len)?;
        let len = u32::from_le_bytes(len) as usize;
        if len == 0 {
            return Some(out);
        }
        let mut body = vec![0u8; len];
        fill(&mut s, &mut body)?;
        out.push(dereth_protocol::read_body::<DddData>(&body).expect("a record"));
    }
}

/// `buf` read whole from `s`, or `None` when the server closed it first. A server that neither
/// sends nor closes within [`GUARD`] fails the test.
fn fill(s: &mut TcpStream, buf: &mut [u8]) -> Option<()> {
    match s.read_exact(buf) {
        Ok(()) => Some(()),
        Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
            panic!("the server neither sent nor closed the stream within {GUARD:?}")
        }
        Err(_) => None,
    }
}

/// The ids of the records the server sent `id` over UDP, in order.
fn udp_ids(ts: &TestServer, id: ClientId) -> Vec<u32> {
    let mut got: Vec<u32> = ts
        .received::<DddData>(id)
        .iter()
        .map(|d| d.resource_id)
        .collect();
    got.sort_unstable();
    got
}

#[test]
fn a_client_that_asks_is_offered_the_patch_as_a_stream_that_opens_once() {
    let port = listen();
    let scratch = dereth_dat::testing::ScratchDir::new("bulk-offer").expect("scratch");
    let (dats, base) = world(scratch.path());
    let (mut ts, id) = exchange(&dats, &answer(&base, true));
    ts.run_until(1.0, |_| false);
    let offers = ts.received::<DddBulkOffer>(id);
    assert_eq!(offers.len(), 1, "one offer");
    let offer = &offers[0];
    assert_eq!(offer.port, port);
    assert_eq!(offer.records, RECORDS);
    assert!(ts.received::<DddData>(id).is_empty(), "no record over UDP");

    let mut wrong = offer.token;
    wrong[0] ^= 1;
    assert!(
        stream_with(port, &wrong).is_none(),
        "a wrong token opens nothing"
    );
    let records = stream(port, offer).expect("the stream");
    let mut ids: Vec<u32> = records.iter().map(|d| d.resource_id).collect();
    ids.sort_unstable();
    assert_eq!(ids, (FIRST..FIRST + RECORDS).collect::<Vec<_>>());
    assert!(records
        .iter()
        .all(|d| d.compressed == 1 && d.dat_file_id == 1));
    assert!(stream(port, offer).is_none(), "the token opens one stream");

    // Collected: nothing goes over UDP when its 120 seconds are up.
    ts.run_until(125.0, |_| false);
    assert!(ts.received::<DddData>(id).is_empty());
}

/// An offer that reaches its client late, behind a manifest and a begin slow to arrive, still
/// opens its stream a minute and a half after it was made, and none of its records went over UDP
/// meanwhile.
#[test]
fn an_offer_collected_long_after_it_was_made_still_opens() {
    let port = listen();
    let scratch = dereth_dat::testing::ScratchDir::new("bulk-late").expect("scratch");
    let (dats, base) = world(scratch.path());
    let (mut ts, id) = exchange(&dats, &answer(&base, true));
    ts.run_until(1.0, |_| false);
    let offer = ts.received::<DddBulkOffer>(id)[0].clone();
    ts.run_until(90.0, |_| false);
    assert!(
        ts.received::<DddData>(id).is_empty(),
        "nothing over UDP while the offer waits"
    );
    let records = stream(port, &offer).expect("the late stream opens");
    assert_eq!(records.len(), RECORDS as usize);
}

#[test]
fn a_client_that_does_not_ask_is_patched_over_udp_as_before() {
    listen();
    let scratch = dereth_dat::testing::ScratchDir::new("bulk-none").expect("scratch");
    let (dats, base) = world(scratch.path());
    let (mut ts, id) = exchange(&dats, &answer(&base, false));
    ts.run_until(8.0, |ts| {
        ts.received::<DddData>(id).len() >= RECORDS as usize
    });
    assert!(ts.received::<DddBulkOffer>(id).is_empty());
    assert_eq!(ts.received::<DddData>(id).len(), RECORDS as usize);
}

/// Not ACE (V566): a client that asks what its items make (`FLAG_RECIPES`) is told the server
/// answers it, once, beside its patch; one that does not ask is told nothing new.
#[test]
fn a_client_that_asks_for_recipe_answers_is_offered_them_and_one_that_does_not_is_not() {
    use dereth_protocol::admin::ExtensionOffer;
    listen();
    let scratch = dereth_dat::testing::ScratchDir::new("recipes-offer").expect("scratch");
    let (dats, base) = world(scratch.path());
    let mut asks = answer(&base, false);
    asks.flags |= DddInterrogationResponse::FLAG_RECIPES;
    let (ts, id) = exchange(&dats, &asks);
    assert_eq!(
        ts.received::<ExtensionOffer>(id),
        vec![ExtensionOffer {
            flags: ExtensionOffer::RECIPES
        }]
    );
    let (ts, id) = exchange(&dats, &answer(&base, false));
    assert!(ts.received::<ExtensionOffer>(id).is_empty());
}

#[test]
fn a_world_with_the_stream_off_patches_a_client_that_asks_as_before() {
    listen();
    let scratch = dereth_dat::testing::ScratchDir::new("bulk-off").expect("scratch");
    let (dats, base) = world(scratch.path());
    let mut config = MasterConfiguration::default();
    config.dat_overlay.bulk = false;
    let (mut ts, id) = exchange_with(&dats, &answer(&base, true), config);
    ts.run_until(8.0, |ts| {
        ts.received::<DddData>(id).len() >= RECORDS as usize
    });
    assert!(ts.received::<DddBulkOffer>(id).is_empty());
    assert_eq!(
        udp_ids(&ts, id),
        (FIRST..FIRST + RECORDS).collect::<Vec<_>>()
    );
}

#[test]
fn what_a_client_declines_goes_over_udp_once_however_it_is_split() {
    let port = listen();
    let scratch = dereth_dat::testing::ScratchDir::new("bulk-decline").expect("scratch");
    let (dats, base) = world(scratch.path());
    let (mut ts, id) = exchange(&dats, &answer(&base, true));
    ts.run_until(1.0, |_| false);
    let offer = ts.received::<DddBulkOffer>(id).remove(0);
    let named = |ns: std::ops::Range<u32>| -> Vec<(u32, u32, u32)> {
        ns.map(|n| (0, 1, FIRST + n * 3)).collect()
    };
    // Two declines that overlap, and one naming nothing the offer held.
    for pending in [
        named(0..10),
        named(5..15),
        vec![(0, 1, 0x0600_0001), (7, 7, FIRST)],
    ] {
        ts.send_message(id, NetQueue::ClientCache, &DddBulkDecline { pending });
    }
    ts.run_until(8.0, |ts| ts.received::<DddData>(id).len() >= 15);
    ts.run_until(1.0, |_| false);
    let want: Vec<u32> = (0..15).map(|n| FIRST + n * 3).collect();
    assert_eq!(udp_ids(&ts, id), want, "each named record once");
    assert!(
        stream(port, &offer).is_none(),
        "a declined offer opens no stream"
    );

    // A decline naming nothing is the whole offer: the rest, and nothing twice.
    ts.send_message(id, NetQueue::ClientCache, &DddBulkDecline::default());
    ts.run_until(8.0, |ts| {
        ts.received::<DddData>(id).len() >= RECORDS as usize
    });
    ts.run_until(1.0, |_| false);
    assert_eq!(
        udp_ids(&ts, id),
        (FIRST..FIRST + RECORDS).collect::<Vec<_>>()
    );
}

#[test]
fn an_offer_nobody_collects_goes_over_udp() {
    listen();
    let scratch = dereth_dat::testing::ScratchDir::new("bulk-expiry").expect("scratch");
    let (dats, base) = world(scratch.path());
    let (mut ts, id) = exchange(&dats, &answer(&base, true));
    let offer = ts.received::<DddBulkOffer>(id)[0].clone();
    ts.run_until(135.0, |ts| {
        ts.received::<DddData>(id).len() >= RECORDS as usize
    });
    assert_eq!(ts.received::<DddData>(id).len(), RECORDS as usize);
    assert!(
        stream(listen(), &offer).is_none(),
        "a withdrawn offer opens no stream"
    );
}

/// Every file of the overlay folder `dir`, by name, and its bytes.
fn files(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out: Vec<(String, Vec<u8>)> = std::fs::read_dir(dir)
        .expect("the overlay folder")
        .map(|e| e.expect("an entry").path())
        .filter(|p| p.is_file())
        .map(|p| {
            (
                p.file_name()
                    .expect("a name")
                    .to_string_lossy()
                    .into_owned(),
                std::fs::read(&p).expect("a file"),
            )
        })
        .collect();
    out.sort();
    out
}

/// The client's own patcher, applying what the server sent `id` and then `records`, into `dir`.
fn apply(ts: &TestServer, id: ClientId, base: &RetailDatStore, dir: &Path, records: &[DddData]) {
    use dereth_client_runtime::ddd::{DddPatcher, OverlayTarget};
    let dir = OverlayDir::new(dir).expect("the client's overlay folder");
    let mut patcher = DddPatcher::new(Some(OverlayTarget::new(dir, base, "127.0.0.1:9000")));
    patcher.on_interrogation();
    for m in ts.received::<DddOverlayManifest>(id) {
        patcher.on_manifest(&m);
    }
    let begin = ts.received::<DddBeginDdd>(id).remove(0);
    patcher.on_begin(&begin);
    for d in records {
        patcher.on_data(d);
    }
    let summary = patcher.on_end();
    assert_eq!(patcher.refused(), None, "{:?}", patcher.notices());
    assert_eq!(
        (summary.refused, summary.still_pending),
        (0, 0),
        "{summary:?}"
    );
}

#[test]
fn the_overlay_a_client_takes_holds_the_same_bytes_whichever_way_its_records_came() {
    let port = listen();
    let scratch = dereth_dat::testing::ScratchDir::new("bulk-identical").expect("scratch");
    let (dats, base) = world(scratch.path());

    let (mut ts, id) = exchange(&dats, &answer(&base, false));
    ts.run_until(8.0, |ts| {
        ts.received::<DddData>(id).len() >= RECORDS as usize
    });
    let by_udp = scratch.path().join("by-udp");
    apply(&ts, id, &base, &by_udp, &ts.received::<DddData>(id));

    let (mut ts, id) = exchange(&dats, &answer(&base, true));
    ts.run_until(1.0, |_| false);
    let offer = ts.received::<DddBulkOffer>(id).remove(0);
    let at = std::net::SocketAddr::from(([127, 0, 0, 1], u16::try_from(port).expect("a port")));
    let mut download =
        dereth_client_runtime::bulk_patch::BulkDownload::start(at, offer.token).expect("a reader");
    let mut streamed = Vec::new();
    while download.ended().is_none() {
        assert!(download.wait(GUARD), "the stream ends");
        streamed.extend(download.drain(usize::MAX).into_iter().map(|d| *d.record));
    }
    assert_eq!(download.ended(), Some(&Ok(())));
    assert_eq!(streamed.len(), RECORDS as usize);
    assert!(ts.received::<DddData>(id).is_empty(), "nothing over UDP");
    let by_stream = scratch.path().join("by-stream");
    apply(&ts, id, &base, &by_stream, &streamed);

    // Each overlay is read through by the client's own files: every record the world added is
    // the world's own bytes either way, and so is every revision. (The container files are not
    // compared whole: two patches the same way into two folders differ in their headers.)
    let read = |dir: &Path| {
        base.clone()
            .with_overlay(&OverlayDir::new(dir).expect("an overlay"), None)
            .expect("the overlay lies over the base")
    };
    let (udp, tcp) = (read(&by_udp), read(&by_stream));
    for n in 0..RECORDS {
        let (id, want) = (
            FIRST + n,
            vec![u8::try_from(n % 256).expect("a byte"); 4096],
        );
        assert_eq!(
            udp.read_portal(DataId(id)).expect("over UDP"),
            want,
            "{id:#010X}"
        );
        assert_eq!(
            tcp.read_portal(DataId(id)).expect("over TCP"),
            want,
            "{id:#010X}"
        );
    }
    assert_eq!(
        dereth_dat::overlay::base_iterations(udp.portal()),
        dereth_dat::overlay::base_iterations(tcp.portal())
    );
    assert!(!files(&by_udp).is_empty() && files(&by_udp).len() == files(&by_stream).len());
}

/// Each record of the overlay that goes compressed is compressed once, when the world's patch
/// state is built, and that copy is what every client is sent: two clients' streams and two
/// clients' `0xF7E2`s compress none of them again, every client is sent the same bytes for each
/// record, and they are the bytes the patch's size was counted from.
#[test]
fn an_overlay_record_is_compressed_once_and_every_client_is_sent_that_copy() {
    use empyrean_common::config_manager::ConfigManager;
    use empyrean_dat::DatDatabaseType::Portal;
    use empyrean_world::managers::bulk_patch::{BulkItem, Limits, Service};
    use empyrean_world::managers::ddd_manager::{self, DddManagerState};
    use empyrean_world::network::game_messages::messages::game_message_ddd_data_message::ddd_data_body;
    // The walk reads the default configuration, on this thread alone.
    let _config = ConfigManager::override_for_thread(MasterConfiguration::default());
    let scratch = dereth_dat::testing::ScratchDir::new("bulk-compressed-once").expect("scratch");
    let (dats, _) = world(scratch.path());
    let mut state = DddManagerState::new();
    crate::support::ddd_start::initialize(&mut state, &dats);
    assert_eq!(
        state.overlay_records.len(),
        RECORDS as usize,
        "every overlay record, compressed at start"
    );

    let service = Service::listen(SocketAddr::from(([127, 0, 0, 1], 0)), Limits::default())
        .expect("the stream listens");
    let items: Vec<BulkItem> = (FIRST..FIRST + RECORDS)
        .map(|id| BulkItem {
            id,
            db: Portal,
            compress: true,
        })
        .collect();
    let streams: Vec<Vec<DddData>> = (0..2)
        .map(|n| {
            let token = service
                .offer(
                    items.clone(),
                    Arc::clone(&dats),
                    Arc::clone(&state.overlay_records),
                    &format!("client {n}"),
                    std::net::Ipv4Addr::LOCALHOST.into(),
                )
                .expect("a ticket");
            stream_with(u32::from(service.port()), &token).expect("the stream")
        })
        .collect();
    assert_eq!(streams[0].len(), RECORDS as usize);
    assert_eq!(service.deflated(), 0, "no record compressed for a stream");
    assert!(streams[0] == streams[1], "both streams are the same bytes");

    // Over UDP, for two clients: the same bytes again, and nothing compressed into the per-send
    // cache.
    for _ in 0..2 {
        for d in &streams[0] {
            let contents = ddd_manager::try_get_dat_file_contents_for_transmission(
                &mut state,
                &dats,
                d.resource_id,
                Portal,
            )
            .expect("the record");
            assert!(
                ddd_data_body(contents, Portal) == *d,
                "{:#010X}",
                d.resource_id
            );
        }
    }
    assert!(
        state.compressed_dat_files_cache[&Portal]
            .keys()
            .all(|id| !(FIRST..FIRST + RECORDS).contains(id)),
        "no overlay record compressed again over UDP"
    );

    let sent: usize = streams[0].iter().map(|d| d.data.len()).sum();
    let counted: usize = (FIRST..FIRST + RECORDS)
        .map(|id| {
            4 + usize::try_from(state.dat_file_sizes[&Portal][&id].compressed_file_size)
                .expect("a size")
        })
        .sum();
    assert_eq!(sent, counted, "the bytes the patch's size counted");
}
