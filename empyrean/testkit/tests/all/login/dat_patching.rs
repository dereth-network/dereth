//! ACE: Source/ACE.Server/Network/Handlers/DDDHandler.cs::DDD_InterrogationResponse
//! DDD at login: matching dats told over; missing iterations booted when patching off, patched
//! when on; newer dats booted; latest-only client patched/booted; landblock request sends info
//! and itself.
//! Fixture: a virtual-time TestServer, isolated stores and synthetic dats.

use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;

use dereth_primitives::NetQueue;
use dereth_protocol::admin::{
    DddBeginDdd, DddData, DddEndDdd, DddInterrogation, DddInterrogationResponse,
    MostlyConsecutiveIntSet, TaggedIterationList,
};
use dereth_protocol::login::{LoginAccountBooted, LoginCharacterSet};
use dereth_protocol::{self as proto, Message};
use empyrean_dat::{DatDatabaseType, DatManager, FakeDats};
use empyrean_entity::enums::AccessLevel;
use empyrean_net::SessionId;
use empyrean_testkit::{dats, ClientId, TestServer};
use empyrean_world::managers::{ddd_manager, world_manager};
use empyrean_world::network::handlers::ddd_handler;
use empyrean_world::network::managers::inbound_message_manager::Payload;

const TEXTURE: u32 = 0x0600_0001;
const SETUP: u32 = 0x0200_0002;
const LANDBLOCK: u32 = 0xA9B4_FFFF;
const LANDBLOCK_INFO: u32 = 0xA9B4_FFFE;

/// The server's dats: portal at iteration 3 (a compressible texture from iteration 2, a tiny setup
/// from iteration 3), cell and language at 1, and one landblock with its info in the cell dat.
fn server_dats() -> Arc<DatManager> {
    dats::with_stat_tables(FakeDats::new())
        .with_iteration(DatDatabaseType::Portal, 3)
        .with_iteration(DatDatabaseType::Cell, 1)
        .with_iteration(DatDatabaseType::Language, 1)
        .with_raw(DatDatabaseType::Portal, TEXTURE, vec![7u8; 600])
        .with_file_iteration(DatDatabaseType::Portal, TEXTURE, 2)
        .with_raw(DatDatabaseType::Portal, SETUP, vec![1, 2, 3])
        .with_file_iteration(DatDatabaseType::Portal, SETUP, 3)
        .with_raw(DatDatabaseType::Cell, LANDBLOCK, (0..40).collect())
        .with_raw(DatDatabaseType::Cell, LANDBLOCK_INFO, vec![8u8; 12])
        .build()
        .expect("fake dats")
}

/// A server whose `DDDManager` has been initialised over its dats, with an account.
fn server() -> TestServer {
    let mut ts = TestServer::with_setup(server_dats(), |w| {
        let d = Arc::clone(&w.dats);
        ddd_manager::initialize(&mut w.ddd_manager, &d);
    });
    world_manager::open(&mut ts.world, None);
    ts.auth()
        .create_account(
            "acct",
            "secret",
            AccessLevel::Player,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        )
        .expect("created");
    ts
}

/// Logs in and runs until the DDD interrogation has arrived.
fn login(ts: &mut TestServer) -> (ClientId, SessionId) {
    let id = ts.connect("acct", "secret");
    assert!(
        ts.run_until(1.0, |ts| !ts.received::<DddInterrogation>(id).is_empty()
            && !ts.received::<LoginCharacterSet>(id).is_empty()),
        "the character list and the interrogation arrive"
    );
    (id, ts.world.net.find_by_account("acct").expect("a session"))
}

/// A run of `n` iterations from 1 (`-n, 1`), or none.
fn run(n: i32) -> MostlyConsecutiveIntSet {
    MostlyConsecutiveIntSet {
        iterations: n,
        ints: if n == 0 { Vec::new() } else { vec![-n, 1] },
    }
}

/// The client's answer: portal, cell and language iteration lists, keyed as the retail client keys
/// them.
fn response(portal: MostlyConsecutiveIntSet, cell: i32, language: i32) -> DddInterrogationResponse {
    DddInterrogationResponse {
        client_language: 1,
        iters_with_keys: vec![
            TaggedIterationList {
                dat_file_type: 0,
                dat_file_id: 1,
                iterations: portal,
            },
            TaggedIterationList {
                dat_file_type: 1,
                dat_file_id: 2,
                iterations: run(cell),
            },
            TaggedIterationList {
                dat_file_type: 1,
                dat_file_id: 3,
                iterations: run(language),
            },
        ],
        iters_without_keys: Vec::new(),
        flags: 0,
        overlay_bases: Vec::new(),
    }
}

/// The DDD messages (`0xF7E2`..`0xF7EB`) the client received, in order.
fn ddd_opcodes(ts: &TestServer, id: ClientId) -> Vec<u32> {
    ts.received_raw(id)
        .iter()
        .map(|m| m.opcode)
        .filter(|o| (0xF7E2..=0xF7EB).contains(o))
        .collect()
}

/// A client whose dats match the server's gets `EndDDD` at once and nothing else; the session
/// stays up.
#[test]
fn a_client_with_matching_dats_is_told_ddd_is_over() {
    let mut ts = server();
    let (id, session) = login(&mut ts);
    ts.send_message(id, NetQueue::ClientCache, &response(run(3), 1, 1));
    assert!(
        ts.run_until(1.0, |ts| !ts.received::<DddEndDdd>(id).is_empty()),
        "EndDDD arrives"
    );
    ts.run_until(2.0, |_| false);
    assert_eq!(
        ddd_opcodes(&ts, id),
        vec![DddInterrogation::OPCODE.0, DddEndDdd::OPCODE.0]
    );
    assert!(ts.world.net.session(session).is_some(), "still connected");
    let s = ts.world.sessions.get(session).expect("game half");
    assert!(!s.begin_ddd_sent && s.ddd_data_queue.is_none());
}

/// Patching is off by default: a client missing an iteration is booted with
/// `dat_older_warning_msg` (its final full stop trimmed).
#[test]
fn a_client_missing_iterations_is_booted_when_patching_is_off() {
    let mut ts = server();
    let (id, session) = login(&mut ts);
    ts.send_message(id, NetQueue::ClientCache, &response(run(2), 1, 1));
    assert!(
        ts.run_until(1.0, |ts| !ts.received::<LoginAccountBooted>(id).is_empty()),
        "booted"
    );
    assert_eq!(
        ts.received::<LoginAccountBooted>(id),
        vec![LoginAccountBooted {
            reason: Some(" because Your DAT files are incomplete.\nThis server does not support dynamic DAT updating at this time.\nPlease visit https://dereth.network to download the complete DAT files".to_owned())
        }]
    );
    assert!(ts.received::<DddEndDdd>(id).is_empty() && ts.received::<DddBeginDdd>(id).is_empty());
    assert!(
        ts.run_until(5.0, |ts| ts.world.net.session(session).is_none()),
        "the session ends"
    );
}

/// A client with more iterations than the server is booted with `dat_newer_warning_msg` minus its
/// last character.
#[test]
fn a_client_with_newer_dats_is_booted() {
    let mut ts = server();
    let (id, _) = login(&mut ts);
    ts.send_message(id, NetQueue::ClientCache, &response(run(3), 2, 1));
    assert!(
        ts.run_until(1.0, |ts| !ts.received::<LoginAccountBooted>(id).is_empty()),
        "booted"
    );
    assert_eq!(
        ts.received::<LoginAccountBooted>(id)[0].reason.as_deref(),
        Some(" because Your DAT files are newer than expected.\nPlease visit https://dereth.network to download the correct DAT files")
    );
}

/// A `BeginDDD` revision: dat file type, dat file id, iteration, downloads, purges.
type Revision = (u32, u32, u32, Vec<u32>, Vec<u32>);

/// `payload` as the handler sees it: the opcode, then the body.
fn blob<M: Message>(m: &M) -> Vec<u8> {
    proto::write_blob(m).expect("encodable")
}

/// With patching on: `BeginDDD` lists the missing portal iterations and their files (the texture
/// counted at its compressed size plus 4, the setup at its 3 bytes); five seconds later the queue
/// sends each file in order, the texture compressed; the client's `EndDDD` is answered with one.
#[test]
fn a_client_missing_iterations_is_patched_when_patching_is_on() {
    let mut ts = server();
    let (id, session) = login(&mut ts);
    let bytes = blob(&response(
        MostlyConsecutiveIntSet {
            iterations: 1,
            ints: vec![1],
        },
        1,
        1,
    ));
    ddd_handler::ddd_interrogation_response_with(
        &mut ts.world,
        &mut Payload::new(&bytes),
        session,
        true,
    )
    .expect("handled");
    assert!(
        ts.run_until(1.0, |ts| !ts.received::<DddBeginDdd>(id).is_empty()),
        "BeginDDD arrives"
    );

    let compressed = ddd_manager::compress(&[7u8; 600]);
    let begin = &ts.received::<DddBeginDdd>(id)[0];
    assert_eq!(begin.data_expected as usize, compressed.len() + 4 + 3);
    let revisions: Vec<Revision> = begin
        .revisions
        .iter()
        .map(|r| {
            (
                r.dat_file_type,
                r.dat_file_id,
                r.iteration,
                r.ids_to_download.clone(),
                r.ids_to_purge.clone(),
            )
        })
        .collect();
    assert_eq!(
        revisions,
        vec![
            (0, 1, 2, vec![TEXTURE], vec![]),
            (0, 1, 3, vec![SETUP], vec![])
        ]
    );
    let s = ts.world.sessions.get(session).expect("game half");
    assert!(s.begin_ddd_sent);
    assert_eq!(
        s.ddd_data_queue
            .as_ref()
            .map(|q| q.iter().copied().collect::<Vec<_>>()),
        Some(vec![
            (TEXTURE, DatDatabaseType::Portal),
            (SETUP, DatDatabaseType::Portal)
        ])
    );

    // ProcessDDDQueue waits five seconds after BeginDDD.
    ts.run_until(4.5, |_| false);
    assert!(
        ts.received::<DddData>(id).is_empty(),
        "nothing before the breathing room"
    );
    assert!(
        ts.run_until(2.0, |ts| ts.received::<DddData>(id).len() == 2),
        "both files arrive"
    );
    let data = ts.received::<DddData>(id);
    let mut want_texture = 600u32.to_le_bytes().to_vec();
    want_texture.extend(&compressed);
    assert_eq!(
        (
            data[0].resource_id,
            data[0].iteration,
            data[0].compressed,
            data[0].data.clone()
        ),
        (TEXTURE, 2, 1, want_texture)
    );
    assert_eq!(
        (
            data[1].resource_id,
            data[1].iteration,
            data[1].compressed,
            data[1].data.clone()
        ),
        (SETUP, 3, 0, vec![1, 2, 3])
    );

    ts.send_message(id, NetQueue::ClientCache, &DddEndDdd);
    assert!(
        ts.run_until(1.0, |ts| !ts.received::<DddEndDdd>(id).is_empty()),
        "EndDDD answered"
    );
    assert!(
        !ts.world
            .sessions
            .get(session)
            .expect("game half")
            .begin_ddd_sent
    );
    // A second EndDDD, with no BeginDDD outstanding, is not answered.
    ts.send_message(id, NetQueue::ClientCache, &DddEndDdd);
    ts.run_until(1.0, |_| false);
    assert_eq!(ts.received::<DddEndDdd>(id).len(), 1);
}

const ITERATION_FILE: u32 = 0xFFFF_0001;

/// V290, end to end with patching on: a client holding only the portal dat's latest iteration
/// (`1` entry, `3`) is sent iterations 1 and 2, the first carrying the iteration file, which
/// streams with the type the client keys it by (6) and no panic; the session lives, and the
/// client's `EndDDD` is answered.
#[test]
fn a_client_with_only_the_latest_iteration_is_patched_to_the_end() {
    let mut ts = server();
    let (id, session) = login(&mut ts);
    let bytes = blob(&response(
        MostlyConsecutiveIntSet {
            iterations: 1,
            ints: vec![3],
        },
        1,
        1,
    ));
    ddd_handler::ddd_interrogation_response_with(
        &mut ts.world,
        &mut Payload::new(&bytes),
        session,
        true,
    )
    .expect("handled");
    assert!(
        ts.run_until(1.0, |ts| !ts.received::<DddBeginDdd>(id).is_empty()),
        "BeginDDD arrives"
    );
    let begin = &ts.received::<DddBeginDdd>(id)[0];
    let revisions: Vec<Revision> = begin
        .revisions
        .iter()
        .map(|r| {
            (
                r.dat_file_type,
                r.dat_file_id,
                r.iteration,
                r.ids_to_download.clone(),
                r.ids_to_purge.clone(),
            )
        })
        .collect();
    // iteration 1 also brought the three stat tables
    let first = vec![0x0E00_0003, 0x0E00_0004, 0x0E00_000E, ITERATION_FILE];
    assert_eq!(
        revisions,
        vec![(0, 1, 1, first, vec![]), (0, 1, 2, vec![TEXTURE], vec![])]
    );

    assert!(
        ts.run_until(7.0, |ts| ts.received::<DddData>(id).len() == 5),
        "every file arrives"
    );
    let data = ts.received::<DddData>(id);
    let got: Vec<(u32, u32, u32, u32, u32)> = data
        .iter()
        .map(|d| {
            (
                d.dat_file_type,
                d.dat_file_id,
                d.resource_type,
                d.resource_id,
                d.iteration,
            )
        })
        .collect();
    // each type is the one the client keys the id by: its id ranges' (the stat tables 0x1000000x,
    // the texture 12), and 6 for the iteration file, which they do not cover
    assert_eq!(
        got,
        vec![
            (0, 1, 0x1000_0003, 0x0E00_0003, 1),
            (0, 1, 0x1000_0004, 0x0E00_0004, 1),
            (0, 1, 0x1000_0005, 0x0E00_000E, 1),
            (0, 1, 6, ITERATION_FILE, 1),
            (0, 1, 12, TEXTURE, 2),
        ]
    );
    assert!(ts.world.net.session(session).is_some(), "the session lives");

    ts.send_message(id, NetQueue::ClientCache, &DddEndDdd);
    assert!(
        ts.run_until(1.0, |ts| !ts.received::<DddEndDdd>(id).is_empty()),
        "EndDDD answered"
    );
    assert!(ts.world.net.session(session).is_some(), "still connected");
}

/// V290 with patching off (ACE's default): the same client is booted as before.
#[test]
fn a_client_with_only_the_latest_iteration_is_booted_when_patching_is_off() {
    let mut ts = server();
    let (id, _) = login(&mut ts);
    ts.send_message(
        id,
        NetQueue::ClientCache,
        &response(
            MostlyConsecutiveIntSet {
                iterations: 1,
                ints: vec![3],
            },
            1,
            1,
        ),
    );
    assert!(
        ts.run_until(1.0, |ts| !ts.received::<LoginAccountBooted>(id).is_empty()),
        "booted"
    );
    assert!(ts.received::<DddBeginDdd>(id).is_empty() && ts.received::<DddData>(id).is_empty());
}

/// `DDD_RequestDataMessage` with patching on: a landblock request queues its `LandBlockInfo`
/// first, then the landblock itself, and both go out as cell files.
#[test]
fn a_landblock_request_sends_its_info_and_itself() {
    let mut ts = server();
    let (id, session) = login(&mut ts);
    let bytes = blob(&dereth_protocol::admin::DddRequestData {
        resource_type: 1,
        resource_id: LANDBLOCK,
    });
    ddd_handler::ddd_request_data_message_with(
        &mut ts.world,
        &mut Payload::new(&bytes),
        session,
        true,
    )
    .expect("handled");
    assert!(
        ts.run_until(2.0, |ts| ts.received::<DddData>(id).len() == 2),
        "both files arrive"
    );
    let data = ts.received::<DddData>(id);
    let got: Vec<(u32, u32, u32, u32, usize)> = data
        .iter()
        .map(|d| {
            (
                d.dat_file_type,
                d.dat_file_id,
                d.resource_type,
                d.resource_id,
                d.data.len(),
            )
        })
        .collect();
    assert_eq!(
        got,
        vec![(1, 2, 2, LANDBLOCK_INFO, 12), (1, 2, 1, LANDBLOCK, 40)]
    );

    // Patching off and no warning: the request is only logged.
    ddd_handler::ddd_request_data_message_with(
        &mut ts.world,
        &mut Payload::new(&bytes),
        session,
        false,
    )
    .expect("handled");
    ts.run_until(2.0, |_| false);
    assert_eq!(ts.received::<DddData>(id).len(), 2);
}
