//! The patch stream as the frame loop runs it (client divergence CD-bulk-patch): a server that
//! never offers one sees nothing but the flag, whichever server it is; an offer the client did not
//! ask for, or cannot open, or that arrives malformed, is declined or dropped; a stream that
//! breaks is declined with exactly what is still pending, and the patch completes the old way.
//!
//! The test speaks for the server: its messages go through the client's own queue-5 dispatcher,
//! and what the client sends is read back off its session.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use dereth_client_net::client_session::dispatch::database::{dispatch, DddDispatch};
use dereth_client_net::client_session::testing::{Corpus, Direction};
use dereth_client_net::client_session::{DddState, SessionEvent};
use dereth_dat::RetailDatStore;
use dereth_primitives::{DataId, IncomingMessage, NetBlobId, NetQueue, RecipientId};
use dereth_protocol::admin::{
    DddBeginDdd, DddBulkDecline, DddBulkOffer, DddData, DddEndDdd, DddInterrogation,
    DddInterrogationResponse, DddOverlayManifest, OverlayFileManifest, OverlayRecord,
    PatchRevision,
};
use dereth_protocol::{read_body, write_body, Message};

use super::*;

/// The most one wait may take: a guard against a hang, never the expected time.
const GUARD: Duration = Duration::from_secs(60);
/// The first of the pictures the tests' world adds; each test's records follow it.
const FIRST: u32 = 0x0600_E000;

fn retail_store() -> Arc<RetailDatStore> {
    Arc::new(dereth_dat::testing::open_store().unwrap_or_else(|| {
        panic!(
            "the retail dats are this test's oracle and they are not under {}",
            dereth_dat::testing::dat_dir().display()
        )
    }))
}

/// A scripted run with no interface, keeping the world's records in an overlay at `folder`,
/// asking for the stream unless `bulk` is off (`--no-bulk-patch`).
fn app(
    store: &Arc<RetailDatStore>,
    settings: &Path,
    folder: PathBuf,
    bulk: bool,
) -> App<NullShell> {
    let cfg = Config {
        headless: true,
        frames: None,
        connect: false,
        sound: false,
        host: "127.0.0.1".into(),
        port: 9000,
        account: "builder".into(),
        vg_password: "password".into(),
        enter_world: true,
        start_char: "Builder".into(),
        preferences_file: settings.join("UserPreferences.ini"),
        dat_dir: PathBuf::from("a directory that does not exist"),
        overlay_dat_dir: Some(folder),
        bulk_patch: bulk,
        ..Config::default()
    };
    let mut app = App::<NullShell>::bring_up_with_store(
        cfg,
        Some(Arc::clone(store)),
        |_| Ok(Platform::headless(64, 64)),
        |_, _, _, _| Ok(Box::new(crate::present::NullPresentation::new(64, 64))),
    )
    .expect("bring-up");
    let net = crate::net::ClientNetwork::new("127.0.0.1", 9000, "builder", "password", 1)
        .expect("an endpoint");
    assert!(app.attach_replay_network(net).is_ok());
    let link = app.link.as_mut().expect("the replay link");
    link.net
        .set_handshake_addr(SocketAddr::from(([127, 0, 0, 1], 9001)));
    link.net.session.keep_sent(true);
    app
}

/// A server's queue-5 blob, `[opcode][body]`, as the client's transport hands it on.
fn incoming(blob: &[u8]) -> IncomingMessage {
    IncomingMessage {
        opcode: u32::from_le_bytes(blob[..4].try_into().unwrap()),
        queue: NetQueue::ClientCache,
        sender: RecipientId(0),
        blob_id: NetBlobId::default(),
        body: blob[4..].to_vec(),
    }
}

fn blob<M: Message>(m: &M) -> Vec<u8> {
    dereth_protocol::write_blob(m).unwrap()
}

/// The client in one frame of `blobs` from the server: each goes through the queue-5 dispatcher,
/// whose own `0xF7EA` the session would send, and the events go to the frame loop.
fn frame(app: &mut App<NullShell>, state: &mut DddState, blobs: &[Vec<u8>]) {
    let mut events = Vec::new();
    for b in blobs {
        let DddDispatch { event, send_end } = dispatch(state, &incoming(b));
        if send_end {
            let link = app.link.as_mut().unwrap();
            link.net.session.send_ddd_end();
        }
        events.push(event);
    }
    app.process_logon_event_queue(&mut NullShell, events);
}

/// What the client sent, as `[opcode][body]` blobs.
fn sent(app: &mut App<NullShell>) -> Vec<Vec<u8>> {
    app.link.as_mut().unwrap().net.session.take_sent()
}

fn opcode(b: &[u8]) -> u32 {
    u32::from_le_bytes(b[..4].try_into().unwrap())
}

/// The declines among `sent`.
fn declines(sent: &[Vec<u8>]) -> Vec<DddBulkDecline> {
    sent.iter()
        .filter(|b| opcode(b) == DddBulkDecline::OPCODE.0)
        .map(|b| read_body::<DddBulkDecline>(&b[4..]).unwrap())
        .collect()
}

/// The answer among `sent`.
fn answer(sent: &[Vec<u8>]) -> DddInterrogationResponse {
    let b = sent
        .iter()
        .find(|b| opcode(b) == DddInterrogationResponse::OPCODE.0)
        .expect("an answer");
    read_body(&b[4..]).unwrap()
}

/// One server's session as the client sees it: what it sent, how the patch stood after.
#[derive(Debug, PartialEq)]
struct Seen {
    sent: Vec<Vec<u8>>,
    finished: bool,
    streaming: bool,
}

fn play(
    store: &Arc<RetailDatStore>,
    dir: &Path,
    name: &str,
    bulk: bool,
    frames: &[Vec<Vec<u8>>],
) -> Seen {
    let side = if bulk { "asks" } else { "never-asks" };
    let settings = dir.join(format!("{name}-{side}-settings"));
    std::fs::create_dir_all(&settings).unwrap();
    let mut app = app(store, &settings, dir.join(format!("{name}-{side}")), bulk);
    let mut state = DddState::default();
    for blobs in frames {
        frame(&mut app, &mut state, blobs);
    }
    Seen {
        sent: sent(&mut app),
        finished: app.host_state().patch_finished,
        streaming: app.bulk.is_some(),
    }
}

/// `seen` with the stream's flag taken out of its answer.
fn without_the_flag(mut seen: Seen) -> Seen {
    for b in &mut seen.sent {
        if opcode(b) == DddInterrogationResponse::OPCODE.0 {
            let mut a: DddInterrogationResponse = read_body(&b[4..]).unwrap();
            a.flags &= !DddInterrogationResponse::FLAG_BULK;
            *b = blob(&a);
        }
    }
    seen
}

/// The world the tests' servers patch: `n` pictures after [`FIRST`], over the player's portal.
fn world(store: &RetailDatStore, n: u32) -> (DddOverlayManifest, DddBeginDdd, Vec<DddData>) {
    let records: Vec<DddData> = (0..n)
        .map(|i| {
            let data = vec![u8::try_from(i % 251).unwrap(); 64 + i as usize];
            DddData {
                dat_file_type: 0,
                dat_file_id: 1,
                resource_type: 6,
                resource_id: FIRST + i,
                iteration: 1,
                compressed: 0,
                version: 1,
                data_size: u32::try_from(data.len() + 4).unwrap(),
                data,
            }
        })
        .collect();
    let manifest = DddOverlayManifest {
        world_key: "a streamed world".into(),
        total_bytes: 0,
        files: vec![OverlayFileManifest {
            dat_file_type: 0,
            dat_file_id: 1,
            base_name: "client_portal.dat".into(),
            base_fingerprint: dereth_dat::overlay::fingerprint(store.portal()),
            base_records: None,
            base_iterations: 0,
            exact_iterations: false,
            revisions: Vec::new(),
            records: records
                .iter()
                .map(|d| OverlayRecord {
                    id: d.resource_id,
                    iteration: 1,
                    size: u32::try_from(d.data.len()).unwrap(),
                    sha256: dereth_dat::overlay::record_hash(&d.data),
                })
                .collect(),
            tombstones: Vec::new(),
        }],
    };
    let next = dereth_dat::overlay::base_iterations(store.portal())
        .last()
        .copied()
        .unwrap_or(0)
        + 1;
    let begin = DddBeginDdd {
        data_expected: 0,
        revisions: vec![PatchRevision {
            dat_file_type: 0,
            dat_file_id: 1,
            iteration: next,
            ids_to_download: records.iter().map(|d| d.resource_id).collect(),
            ids_to_purge: Vec::new(),
        }],
    };
    (manifest, begin, records)
}

/// Behaviour: net.dat-patch.a-server-that-never-offers-a-stream-sees-only-the-flag
/// Every kind of server that offers no stream is patched exactly as before when the client asks
/// for one: ACE (its recorded sessions), GDLE (its exchange, which answers the client's answer
/// with `0xF7EA` alone), and an Empyrean that offers none (an older one, or one with the stream
/// turned off), which patches the old way. What the client sends differs only by the flag in its
/// answer, the patch ends the same, and no stream is opened.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn a_server_that_never_offers_a_stream_sees_only_the_flag_and_patches_as_before() {
    let store = retail_store();
    let scratch = dereth_dat::testing::ScratchDir::new("bulk-never-offered").expect("scratch");
    let mut servers: Vec<(String, Vec<Vec<Vec<u8>>>)> = Vec::new();
    // ACE, as recorded: each queue-5 message the server sent, a frame each.
    for name in [
        "ddd-interrogation-only",
        "first-login-walk-jump",
        "short-second-connection",
    ] {
        let frames: Vec<Vec<Vec<u8>>> = Corpus::shared(name)
            .blobs
            .iter()
            .filter(|b| b.dir == Direction::ServerToClient && b.queue == NetQueue::ClientCache)
            .map(|b| vec![b.payload.clone()])
            .collect();
        assert!(!frames.is_empty(), "{name}: the recording has its exchange");
        servers.push((format!("ACE {name}"), frames));
    }
    // GDLE: its interrogation, then `0xF7EA` once the answer is in.
    servers.push((
        "GDLE".into(),
        vec![
            vec![blob(&DddInterrogation {
                servers_region: 1,
                name_rule_language: 1,
                product_id: 1,
                supported_languages: vec![0, 1],
            })],
            vec![blob(&DddEndDdd)],
        ],
    ));
    // An Empyrean that offers no stream: the manifest, the begin, every record, the end.
    let (manifest, begin, records) = world(&store, 5);
    servers.push((
        "Empyrean without the stream".into(),
        vec![
            vec![blob(&DddInterrogation::default())],
            vec![blob(&manifest), blob(&begin)],
            records.iter().map(blob).collect(),
            vec![blob(&DddEndDdd)],
        ],
    ));
    for (name, frames) in &servers {
        let slug: String = name.chars().filter(char::is_ascii_alphanumeric).collect();
        let asks = play(&store, scratch.path(), &slug, true, frames);
        let never = play(&store, scratch.path(), &slug, false, frames);
        let a = answer(&asks.sent);
        assert_ne!(
            a.flags & DddInterrogationResponse::FLAG_OVERLAY,
            0,
            "{name}: the client keeps an overlay"
        );
        assert_ne!(
            a.flags & DddInterrogationResponse::FLAG_BULK,
            0,
            "{name}: it asks"
        );
        assert_eq!(
            answer(&never.sent).flags & DddInterrogationResponse::FLAG_BULK,
            0,
            "{name}: --no-bulk-patch never asks"
        );
        assert!(!asks.streaming && declines(&asks.sent).is_empty(), "{name}");
        assert_eq!(without_the_flag(asks), never, "{name}");
    }
}

/// Behaviour: net.dat-patch.an-offer-not-asked-for-or-unusable-is-declined-whole
/// An offer the client did not ask for (`--no-bulk-patch`) or that names no port it can reach is
/// declined whole, with an empty list, at once; a second offer while a stream is read is left
/// alone; the browser's client, which cannot open one, declines every offer. An offer whose body
/// does not read is dropped like any message that does not read, and the patch goes on.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn an_offer_not_asked_for_or_unusable_is_declined_whole_and_a_malformed_one_is_dropped() {
    use crate::bulk_patch::{offer_choice, OfferChoice};
    let store = retail_store();
    let scratch = dereth_dat::testing::ScratchDir::new("bulk-declined").expect("scratch");
    let (manifest, begin, _) = world(&store, 3);
    let offer = |port: u32| DddBulkOffer {
        port,
        token: [5; 16],
        records: 3,
        bytes: 200,
    };
    let start = vec![
        vec![blob(&DddInterrogation::default())],
        vec![blob(&manifest), blob(&begin)],
    ];
    for (name, bulk, port) in [
        ("not asked for", false, 9001),
        ("port 0", true, 0),
        ("a port past 65535", true, 70_000),
    ] {
        let mut frames = start.clone();
        frames.push(vec![blob(&offer(port))]);
        let seen = play(
            &store,
            scratch.path(),
            &name.replace(' ', "-"),
            bulk,
            &frames,
        );
        assert_eq!(
            declines(&seen.sent),
            vec![DddBulkDecline::default()],
            "{name}: the whole offer is declined"
        );
        assert!(!seen.streaming, "{name}");
    }

    // Malformed: a body that ends short is dropped, and nothing is declined or opened.
    let mut short = blob(&offer(9001));
    short.truncate(4 + 10);
    let mut state = DddState::default();
    assert!(matches!(
        dispatch(&mut state, &incoming(&short)).event,
        SessionEvent::Dropped { .. }
    ));
    let mut frames = start.clone();
    frames.push(vec![short]);
    let seen = play(&store, scratch.path(), "malformed", true, &frames);
    assert!(declines(&seen.sent).is_empty() && !seen.streaming);

    // Unknown to this client: a later number on the queue is dropped as any unknown message is.
    let mut state = DddState::default();
    assert!(matches!(
        dispatch(&mut state, &incoming(&[0xFF, 0xF7, 0, 0, 1, 2, 3, 4])).event,
        SessionEvent::Dropped { .. }
    ));

    assert_eq!(
        offer_choice(true, true, true),
        OfferChoice::Ignore,
        "a second offer"
    );
    assert_eq!(offer_choice(true, true, false), OfferChoice::Take);
    assert_eq!(
        offer_choice(false, true, false),
        OfferChoice::Decline,
        "the browser"
    );
    assert_eq!(offer_choice(false, false, false), OfferChoice::Decline);
}

/// Behaviour: net.dat-patch.a-broken-bulk-stream-is-declined-with-what-is-pending-and-completes-the-old-way
/// A stream that breaks part way is declined, once its records are applied, with exactly the
/// records still pending, on queue 5; the server sends those as `0xF7E2`s, and the patch completes
/// with every record in the overlay, the streamed ones and the rest alike. A long list goes as
/// several declines.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn a_stream_that_breaks_is_declined_with_what_is_pending_and_the_patch_completes_the_old_way() {
    let store = retail_store();
    let scratch = dereth_dat::testing::ScratchDir::new("bulk-broken").expect("scratch");
    let n = u32::try_from(DddBulkDecline::MOST).unwrap() + 40;
    let (manifest, begin, records) = world(&store, n);
    let streamed = 30usize;

    // The stand-in sends the first records, then half of the next, and is gone.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let to_send: Vec<Vec<u8>> = records[..=streamed]
        .iter()
        .map(|d| {
            let body = write_body(d).unwrap();
            let mut f = u32::try_from(body.len()).unwrap().to_le_bytes().to_vec();
            f.extend_from_slice(&body);
            f
        })
        .collect();
    let server = std::thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let mut hello = [0u8; 24];
        s.read_exact(&mut hello).unwrap();
        for f in &to_send[..streamed] {
            s.write_all(f).unwrap();
        }
        s.write_all(&to_send[streamed][..10]).unwrap();
        hello
    });

    let settings = scratch.path().join("settings");
    std::fs::create_dir_all(&settings).unwrap();
    let folder = scratch.path().join("overlay");
    let mut app = app(&store, &settings, folder.clone(), true);
    let mut state = DddState::default();
    frame(&mut app, &mut state, &[blob(&DddInterrogation::default())]);
    frame(&mut app, &mut state, &[blob(&manifest), blob(&begin)]);
    frame(
        &mut app,
        &mut state,
        &[blob(&DddBulkOffer {
            port: u32::from(port),
            token: [5; 16],
            records: n,
            bytes: 0,
        })],
    );
    assert!(app.bulk.is_some(), "the stream is open");
    let hello = server.join().unwrap();
    assert_eq!(&hello[8..], &[5; 16], "the offer's token");

    // Frames, each applying what the stream brought, until it is done with.
    while app.bulk.is_some() {
        assert!(app.bulk.as_mut().unwrap().wait(GUARD), "the stream ends");
        let events = app.take_bulk_records();
        app.process_logon_event_queue(&mut NullShell, events);
    }
    let out = sent(&mut app);
    let named: Vec<(u32, u32, u32)> = declines(&out).into_iter().flat_map(|d| d.pending).collect();
    let mut want: Vec<(u32, u32, u32)> = records[streamed..]
        .iter()
        .map(|d| (0, 1, d.resource_id))
        .collect();
    let mut got = named.clone();
    got.sort_unstable();
    want.sort_unstable();
    assert_eq!(got, want, "exactly what is pending, once each");
    assert_eq!(
        declines(&out).len(),
        2,
        "split at {} a message",
        DddBulkDecline::MOST
    );
    assert!(!app.host_state().patch_finished);

    // The server sends what was declined the old way, and the patch completes.
    let rest: Vec<Vec<u8>> = records[streamed..].iter().map(blob).collect();
    frame(&mut app, &mut state, &rest);
    let out = sent(&mut app);
    assert!(
        out.iter().any(|b| opcode(b) == DddEndDdd::OPCODE.0),
        "the client ends the patch"
    );
    frame(&mut app, &mut state, &[blob(&DddEndDdd)]);
    assert!(app.host_state().patch_finished);
    assert_eq!(app.world_refused(), None, "{:?}", app.ddd().notices());
    for d in &records {
        assert_eq!(
            app.store.read_portal(DataId(d.resource_id)).unwrap(),
            d.data,
            "{:#010X}",
            d.resource_id
        );
    }
}
