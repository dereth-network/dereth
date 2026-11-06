//! Divergence: V509
//! A client patched to a world with a data overlay that asks for the patch as a stream
//! (`FLAG_BULK` in `0xF7E6`) is offered one (`0xF7ED`) in place of the records' `0xF7E2`s, and
//! reads every record from a TCP connection that presents the offer's token, once. What a client
//! that could not finish names in `0xF7EE` goes as `0xF7E2`s, and so does a whole offer nobody
//! collects in its 30 seconds.
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and an overlay written in a scratch folder.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::sync::{Arc, Once};

use dereth_dat::overlay::{OverlayDir, OverlayWriter};
use dereth_dat::{ModernDat, RetailDatStore};
use dereth_primitives::{DataId, NetQueue};
use dereth_protocol::admin::{
    DddBulkDecline, DddBulkOffer, DddData, DddInterrogation, DddInterrogationResponse,
    MostlyConsecutiveIntSet, OverlayBase, TaggedIterationList, BULK_MAGIC, BULK_VERSION,
};
use empyrean_dat::{DatManager, RealDats};
use empyrean_entity::enums::AccessLevel;
use empyrean_testkit::{ClientId, TestServer};

const WORLD: &str = "a streamed world";
const RECORDS: u32 = 300;
const FIRST: u32 = 0x0600_E000;

/// The stream's listener, once for the whole test binary.
fn listen() -> u32 {
    static START: Once = Once::new();
    START.call_once(|| {
        empyrean_world::managers::bulk_patch::start(([127, 0, 0, 1], 0).into())
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
    let mut ts = TestServer::with_setup(Arc::clone(dats), |w| {
        let d = Arc::clone(&w.dats);
        empyrean_world::managers::ddd_manager::initialize(&mut w.ddd_manager, &d);
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
    let mut s = TcpStream::connect(("127.0.0.1", u16::try_from(port).expect("a port"))).ok()?;
    s.write_all(&BULK_MAGIC).ok()?;
    s.write_all(&BULK_VERSION.to_le_bytes()).ok()?;
    s.write_all(&offer.token).ok()?;
    let mut out = Vec::new();
    loop {
        let mut len = [0u8; 4];
        s.read_exact(&mut len).ok()?;
        let len = u32::from_le_bytes(len) as usize;
        if len == 0 {
            return Some(out);
        }
        let mut body = vec![0u8; len];
        s.read_exact(&mut body).ok()?;
        out.push(dereth_protocol::read_body::<DddData>(&body).expect("a record"));
    }
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

    let records = stream(port, offer).expect("the stream");
    let mut ids: Vec<u32> = records.iter().map(|d| d.resource_id).collect();
    ids.sort_unstable();
    assert_eq!(ids, (FIRST..FIRST + RECORDS).collect::<Vec<_>>());
    assert!(records
        .iter()
        .all(|d| d.compressed == 1 && d.dat_file_id == 1));
    assert!(stream(port, offer).is_none(), "the token opens one stream");

    // Collected: nothing goes over UDP when its 30 seconds are up.
    ts.run_until(35.0, |_| false);
    assert!(ts.received::<DddData>(id).is_empty());
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

#[test]
fn what_a_client_declines_goes_over_udp() {
    listen();
    let scratch = dereth_dat::testing::ScratchDir::new("bulk-decline").expect("scratch");
    let (dats, base) = world(scratch.path());
    let (mut ts, id) = exchange(&dats, &answer(&base, true));
    ts.run_until(1.0, |_| false);
    assert_eq!(ts.received::<DddBulkOffer>(id).len(), 1);
    let pending: Vec<(u32, u32, u32)> = (0..10).map(|n| (0, 1, FIRST + n * 3)).collect();
    ts.send_message(id, NetQueue::ClientCache, &DddBulkDecline { pending });
    ts.run_until(8.0, |ts| ts.received::<DddData>(id).len() >= 10);
    ts.run_until(1.0, |_| false);
    let mut got: Vec<u32> = ts
        .received::<DddData>(id)
        .iter()
        .map(|d| d.resource_id)
        .collect();
    got.sort_unstable();
    assert_eq!(got, (0..10).map(|n| FIRST + n * 3).collect::<Vec<_>>());
}

#[test]
fn an_offer_nobody_collects_goes_over_udp() {
    listen();
    let scratch = dereth_dat::testing::ScratchDir::new("bulk-expiry").expect("scratch");
    let (dats, base) = world(scratch.path());
    let (mut ts, id) = exchange(&dats, &answer(&base, true));
    let offer = ts.received::<DddBulkOffer>(id)[0].clone();
    ts.run_until(45.0, |ts| {
        ts.received::<DddData>(id).len() >= RECORDS as usize
    });
    assert_eq!(ts.received::<DddData>(id).len(), RECORDS as usize);
    assert!(
        stream(listen(), &offer).is_none(),
        "a withdrawn offer opens no stream"
    );
}
