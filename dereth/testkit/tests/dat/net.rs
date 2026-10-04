//! The link: the handshake and the link-status panel, what happens when the shard goes quiet, and
//! the one thing the client asks the shard for that is not a message at all -- a piece of the
//! world its own data files do not carry. Every scenario opens the retail dats under
//! `$DERETH_TEST_DAT_DIR`, and **this binary must run serially**: two headless clients in one process
//! share the UI request globals. The data-file scenarios drive the land source, the patcher and a
//! session directly (the subject is a cell dat with a record cut out of it, which `HeadlessClient`
//! has no gesture for) and book their claim on the model client.
//!
//! Safety: only a **copy** of the cell dat in the temp directory is opened for writing; every
//! retail dat is hashed before and after, and no socket is bound (the session is mock-transported).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use dereth_client::ddd::{drain_cache_misses, DddPatcher, OverlayTarget};
use dereth_client::land_source::DatLandSource;
use dereth_client_net::client_session::testing::MockTransport;
use dereth_client_net::client_session::Session;
use dereth_dat::write::DatWriter;
use dereth_dat::{DatFile, RetailDatStore};
use dereth_physics::source::LandSource;
use dereth_primitives::{DataId, LandblockId, NetQueue};
use dereth_testkit::HeadlessClient;

// -------------------------------------------------------------------------------------------
// Disposable copies, and the proof the originals were not touched.
// -------------------------------------------------------------------------------------------

fn digest(path: &Path) -> (u64, u64) {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in &bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    (bytes.len() as u64, h)
}

/// A scratch directory, the world's overlay folder in it, and the retail files it promises not
/// to touch.
struct Scratch {
    dir: PathBuf,
    overlay: dereth_dat::overlay::OverlayDir,
    _directory: dereth_dat::testing::ScratchDir,
    pristine: Vec<(PathBuf, (u64, u64))>,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let directory = dereth_dat::testing::ScratchDir::new(name).expect("a scratch directory");
        let dir = directory.path().to_path_buf();
        let overlay =
            dereth_dat::overlay::OverlayDir::new(&dir.join("overlay")).expect("an overlay folder");
        Self {
            dir,
            overlay,
            _directory: directory,
            pristine: Vec::new(),
        }
    }

    /// The patcher, writing the world's overlay over the store with this cell file.
    fn patcher(&self, cell: &Path) -> DddPatcher {
        DddPatcher::new(Some(OverlayTarget::new(
            self.overlay.clone(),
            &store_with_cell_copy(cell),
            "net scenarios",
        )))
    }

    /// The store with this cell file, as the world reads it through its overlay.
    fn world(&self, cell: &Path) -> RetailDatStore {
        store_with_cell_copy(cell)
            .with_overlay(&self.overlay, None)
            .expect("the overlay opens over its base")
    }

    fn watch(&mut self, name: &str) -> PathBuf {
        let src = dereth_dat::testing::dat_dir().join(name);
        assert!(
            src.is_file(),
            "{} is not a file; set DERETH_TEST_DAT_DIR",
            src.display()
        );
        let d = digest(&src);
        self.pristine.push((src.clone(), d));
        src
    }

    /// Copy the cell dat in. It is the only one these scenarios write to.
    fn cell_copy(&mut self) -> PathBuf {
        let src = self.watch("client_cell_1.dat");
        let dst = dereth_dat::RetailDat::Cell.in_dir(&self.dir);
        std::fs::copy(&src, &dst).unwrap_or_else(|e| panic!("copy {}: {e}", src.display()));
        dst
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        for (src, before) in &self.pristine {
            assert_eq!(
                &digest(src),
                before,
                "the retail dat {} changed",
                src.display()
            );
        }
    }
}

/// A store whose **cell** dat is the disposable copy and whose portal and language dats are the
/// retail install's, read-only. A landblock record lives in the cell dat, so only the cell dat has
/// to be writable, and copying the portal dat to demonstrate a landblock miss would be a scenario
/// nobody runs.
fn store_with_cell_copy(cell: &Path) -> RetailDatStore {
    let dir = dereth_dat::testing::dat_dir();
    let portal =
        DatFile::open(&dereth_dat::RetailDat::Portal.in_dir(&dir)).expect("the portal dat opens");
    let cell = DatFile::open(cell).expect("the cell copy opens");
    let local =
        DatFile::open(&dereth_dat::RetailDat::Local.in_dir(&dir)).expect("the language dat");
    let hi = dereth_dat::RetailDat::HighRes.in_dir(&dir);
    let highres = hi
        .is_file()
        .then(|| DatFile::open(&hi).expect("the high-res dat opens"));
    RetailDatStore::open_with(portal, cell, local, highres)
}

/// A landblock the shipped cell dat really carries, in a region the client loads.
const BLOCK: u16 = 0xA9B4;

fn landblock_id() -> LandblockId {
    LandblockId(BLOCK)
}

fn landblock_did() -> DataId {
    DataId((u32::from(BLOCK) << 16) | 0xFFFF)
}

/// The shard's answer carrying a cell-dat record. The three small numbers are the protocol's own
/// (which container, which file, which kind of record), pinned as independent literals because
/// reading them back through the constant would not notice a wrong one.
fn cell_data_msg(id: DataId, payload: &[u8], version: u32, iteration: u32) -> Vec<u8> {
    let m = dereth_protocol::admin::DddData {
        dat_file_type: 1,
        dat_file_id: 2,
        resource_type: 1,
        resource_id: id.raw(),
        iteration,
        compressed: 0,
        version,
        data_size: payload.len() as u32 + 4,
        data: payload.to_vec(),
    };
    dereth_protocol::write_blob(&m).expect("the answer encodes")
}

/// Hand the shard's answer to the real decoder and then to the real patcher, exactly as the
/// client's own data-download arm does.
fn feed_answer(p: &mut DddPatcher, blob: &[u8]) {
    let mut state = dereth_client_net::client_session::DddState::RunTime;
    let incoming = dereth_primitives::IncomingMessage {
        opcode: 0xF7E2,
        queue: NetQueue::ClientCache,
        sender: dereth_primitives::RecipientId(0),
        blob_id: dereth_primitives::NetBlobId(0),
        // The body with the opcode already read off, which is what the client's own reader sees.
        body: blob[4..].to_vec(),
    };
    let d = dereth_client_net::client_session::dispatch::database::dispatch(&mut state, &incoming);
    let dereth_client_net::client_session::SessionEvent::Ddd(
        dereth_client_net::client_session::DddEvent::Data(decoded),
    ) = d.event
    else {
        panic!(
            "the data queue should decode the answer as a data event, got {:?}",
            d.event
        );
    };
    let (outcome, _) = p.on_data(&decoded);
    assert!(
        outcome.wrote(),
        "the answer must reach the container: {outcome:?}"
    );
}

/// A land source over a cell dat with one landblock record removed.
fn land_without_the_block(cell: &Path) -> DatLandSource {
    let store = Arc::new(store_with_cell_copy(cell));
    let region = dereth_client::world::load_region(&store).expect("the region decodes");
    DatLandSource::new(store, &region).expect("the land source comes up")
}

fn cut_the_record_out(cell: &Path, did: DataId) {
    let mut w = DatWriter::open(cell).expect("the copy opens for writing");
    assert!(
        w.remove(did).expect("remove"),
        "the record was there to remove"
    );
}

// -------------------------------------------------------------------------------------------
// data-request.missing-landblock.is-asked-for-once-and-the-answer-builds-the-ground
// -------------------------------------------------------------------------------------------

/// **The whole chain.** Ground the player's files do not carry is asked for once however many
/// times the client walks into it, the shard's answer is written into the world's overlay over the
/// player's files, and the next lookup builds the ground and can be stood on.
pub fn a_missing_landblock_is_asked_for_once_and_then_builds() {
    let mut s = Scratch::new("chain");
    let cell = s.cell_copy();
    let did = landblock_did();

    // What the shard will send back, taken from the file before it is cut out.
    let (record, version, iteration) = {
        let f = DatFile::open(&cell).expect("the copy opens");
        assert!(
            f.contains(did),
            "the shipped cell dat must carry this record"
        );
        let e = *f.entry(did).expect("an entry");
        (
            f.read(did).expect("the landblock record reads"),
            u32::from(e.version()),
            e.iteration,
        )
    };

    // Cut it out: this is "the player's data files are older than the shard's".
    cut_the_record_out(&cell, did);
    assert!(
        !DatFile::open(&cell).expect("reopen").contains(did),
        "the client now lacks it"
    );

    let land = land_without_the_block(&cell);
    let mut patcher = s.patcher(&cell);
    let mut session = Session::new(MockTransport::new());

    // 1. The streaming ring asks for the ground. This is the miss.
    let missing_before = land.landblock(landblock_id()).is_none();

    // 2. The client asks the shard. One request, on the data queue, unordered.
    let sent = drain_cache_misses(&land, &mut patcher, &mut session);
    let mut want = Vec::new();
    want.extend_from_slice(&0xF7E3u32.to_le_bytes());
    want.extend_from_slice(&1u32.to_le_bytes());
    want.extend_from_slice(&did.raw().to_le_bytes());
    let one_request = sent == 1
        && session.transport.sent.len() == 1
        && session.transport.sent[0].queue == NetQueue::ClientCache
        && !session.transport.sent[0].ordered
        && session.transport.sent[0].payload == want;

    // 3. Asking again does not ask again. The ground has to be *re-asked for* to reach that check
    //    at all, which is what happens when the streaming ring drops it and the player walks back
    //    in while the first request is still in flight.
    land.forget_blocks(&[did]);
    let still_missing = land.landblock(landblock_id()).is_none();
    let no_second_request = drain_cache_misses(&land, &mut patcher, &mut session) == 0
        && session.transport.sent.len() == 1
        && patcher.outstanding_gets() == 1;

    // 4. The shard answers, and the answer reaches the world's overlay byte for byte.
    feed_answer(
        &mut patcher,
        &cell_data_msg(did, &record, version, iteration),
    );
    let resupplied = patcher.take_resupplied();
    let answered = patcher.outstanding_gets() == 0
        && resupplied == vec![did]
        && s.world(&cell).read_cell(did).expect("reads") == record
        && !DatFile::open(&cell).expect("reopen").contains(did);

    // 5. The caller re-seeds the land source, and the next lookup builds the ground.
    land.resupply(Arc::new(s.world(&cell)), &resupplied);
    let built = land
        .landblock(landblock_id())
        .is_some_and(|b| b.id == landblock_id())
        && land.ground_height(landblock_id(), 12.0, 12.0).is_some()
        && land.take_missing().is_empty();

    println!(
        "ddd cache miss: missing={missing_before} one_request={one_request} \
         still_missing={still_missing} no_second={no_second_request} answered={answered} \
         built={built}"
    );

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "data-request.missing-landblock.is-asked-for-once-and-the-answer-builds-the-ground",
        move |_| {
            missing_before && one_request && still_missing && no_second_request && answered && built
        },
    );
}

dereth_testkit::scenarios! {
    scenario_a_missing_landblock_is_asked_for_once_and_then_builds => a_missing_landblock_is_asked_for_once_and_then_builds ["data-request.missing-landblock.is-asked-for-once-and-the-answer-builds-the-ground"],
    scenario_a_miss_with_no_link_is_kept => a_miss_with_no_link_is_kept ["data-request.missing-landblock.with-no-link-the-miss-is-kept-rather-than-asked-for"],
    scenario_a_refusal_lets_the_client_ask_again => a_refusal_lets_the_client_ask_again ["data-request.missing-landblock.a-refusal-lets-the-client-ask-for-it-again"],
    scenario_a_record_that_is_present_but_unreadable_is_not_asked_for => a_record_that_is_present_but_unreadable_is_not_asked_for ["data-request.missing-landblock.a-record-that-is-there-but-unreadable-is-not-asked-for"],
    scenario_the_link_lamp_lights_on_the_finished_handshake => the_link_lamp_lights_on_the_finished_handshake ["link.connected.the-lamp-lights-when-the-handshake-finishes-and-not-when-the-shard-first-answers"],
    scenario_the_packet_loss_line_always_carries_a_number => the_packet_loss_line_always_carries_a_number ["link.packet-loss.the-line-always-carries-a-number-and-it-is-the-ping-that-can-be-unknown"],
    scenario_a_client_that_has_heard_nothing_reads_as_total_loss => a_client_that_has_heard_nothing_reads_as_total_loss ["link.packet-loss.a-client-that-has-heard-nothing-reads-as-total-loss-until-the-first-reading"],
    scenario_the_panel_shows_the_links_own_packet_loss_figure => the_panel_shows_the_links_own_packet_loss_figure ["link.packet-loss.what-the-panel-shows-is-the-links-own-figure-on-a-lossy-link-and-zero-on-a-clean-one"],
    scenario_a_wrong_version_refusal_is_an_error_box_and_then_the_client_exits => a_wrong_version_refusal_is_an_error_box_and_then_the_client_exits ["link.first-connection.a-refusal-before-the-link-is-up-is-an-error-box-and-then-the-client-exits"],
    scenario_every_refusal_reads_its_own_sentence => every_refusal_reads_its_own_sentence ["link.first-connection.every-refusal-reads-its-own-sentence-from-the-connection-error-strings"],
}

// -------------------------------------------------------------------------------------------
// data-request.missing-landblock.with-no-link-the-miss-is-kept-rather-than-asked-for
// -------------------------------------------------------------------------------------------

/// **No shard, no request -- but the miss is remembered.** Dropping it would mean ground that
/// went missing while the link was down never being asked for at all.
pub fn a_miss_with_no_link_is_kept() {
    let mut s = Scratch::new("no_link");
    let cell = s.cell_copy();
    let did = landblock_did();
    cut_the_record_out(&cell, did);

    let land = land_without_the_block(&cell);
    let missing = land.landblock(landblock_id()).is_none();
    // The client's own request pass returns before draining when it has no link, so the queue
    // still holds the miss -- and taking it is what a drain does, so a second take is empty.
    let kept = land.take_missing() == vec![(1, did)];
    let taken_once = land.take_missing().is_empty();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "data-request.missing-landblock.with-no-link-the-miss-is-kept-rather-than-asked-for",
        move |_| missing && kept && taken_once,
    );
}

// -------------------------------------------------------------------------------------------
// data-request.missing-landblock.a-refusal-lets-the-client-ask-for-it-again
// -------------------------------------------------------------------------------------------

/// **A refusal is not the end of it.** The failed request stops being outstanding, so the next
/// time the player walks into that ground the client asks again -- with the same bytes.
pub fn a_refusal_lets_the_client_ask_again() {
    let mut s = Scratch::new("retry");
    let cell = s.cell_copy();
    let did = landblock_did();
    cut_the_record_out(&cell, did);

    let land = land_without_the_block(&cell);
    let mut patcher = s.patcher(&cell);
    let mut session = Session::new(MockTransport::new());

    let missing = land.landblock(landblock_id()).is_none();
    let asked = drain_cache_misses(&land, &mut patcher, &mut session) == 1
        && patcher.outstanding_gets() == 1;

    patcher.on_error(&dereth_protocol::admin::DddError {
        resource_type: 1,
        resource_id: did.raw(),
        error: 1,
    });
    let failed = patcher.take_failed_gets();
    let retired = patcher.outstanding_gets() == 0 && failed == vec![did];
    // The request is no longer outstanding, but something still has to ask, and the asker is a
    // cache that remembers the miss.
    land.forget_blocks(&failed);

    let still_missing = land.landblock(landblock_id()).is_none();
    let asked_again = drain_cache_misses(&land, &mut patcher, &mut session) == 1
        && session.transport.sent.len() == 2
        && session.transport.sent[1].payload == session.transport.sent[0].payload;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "data-request.missing-landblock.a-refusal-lets-the-client-ask-for-it-again",
        move |_| missing && asked && retired && still_missing && asked_again,
    );
}

// -------------------------------------------------------------------------------------------
// data-request.missing-landblock.a-record-that-is-there-but-unreadable-is-not-asked-for
// -------------------------------------------------------------------------------------------

/// **Present but unreadable is a different failure.** The shard cannot fix it by sending the same
/// bytes again, and the client does not ask it to.
pub fn a_record_that_is_present_but_unreadable_is_not_asked_for() {
    let mut s = Scratch::new("unreadable");
    let cell = s.cell_copy();
    let did = landblock_did();
    {
        let mut w = DatWriter::open(&cell).expect("the copy opens for writing");
        // A landblock record the decoder will refuse, but which the container *has*.
        w.save(did, &[0u8; 16], 1, 0, 1_700_000_000)
            .expect("save a short record");
    }

    let land = land_without_the_block(&cell);
    let does_not_build = land.landblock(landblock_id()).is_none();
    let nothing_owed = land.take_missing().is_empty();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "data-request.missing-landblock.a-record-that-is-there-but-unreadable-is-not-asked-for",
        move |_| does_not_build && nothing_owed,
    );
}

// =============================================================================================
// The link lamp and the link-status panel
//
// The claims about the link alone open no data file; they are in the `cpu` tier's `net.rs`.
// These four are the ones that need the shipped data: the panel's own words, and a running client
// carrying the figure to it.
//
// **The holder they read is a process-wide value**, which is why these four are in the serial
// binary and the link-only ones are not: a parallel binary would have two scenarios reading one
// another's.
//
// No socket is bound. Every datagram is built here and handed to the client's own socket-free
// endpoint through `feed`, which is the call its socket loop makes.
// =============================================================================================

use std::net::SocketAddr;

use dereth_client::net::{link_status_holder, ClientNetwork, LinkStatus};
use dereth_client_net::linkstatus::{HEARTBEAT_INTERVAL, INITIAL_PACKET_LOSS};
use dereth_primitives::LocalTime;
use dereth_testkit::ClientSpec;
use dereth_transport::wire::{OutPacket, PacketFlags, ProtoHeader};
use dereth_transport::CryptoSystem;

const LINK_RECIPIENT: u16 = 0x000B;
const LINK_PEER: &str = "127.0.0.1:19000";
const LINK_OUTGOING_SEED: u32 = 0xDEAD_BEEF;
const LINK_INCOMING_SEED: u32 = 0x1234_5678;

fn link_peer() -> SocketAddr {
    LINK_PEER.parse().expect("the peer address")
}

/// The shard's opening answer, in the clear.
fn shard_hello() -> Vec<u8> {
    let mut packet = OutPacket::new(ProtoHeader {
        seq_id: 0,
        rec_id: LINK_RECIPIENT,
        interval: 0x100,
        iteration: 1,
        ..ProtoHeader::default()
    });
    packet
        .add_optional_header(
            PacketFlags::CONNECT_REQUEST,
            dereth_transport::conn::ConnectRequest {
                server_time: 0.0,
                cookie: 0x0BAD_F00D_0BAD_F00D,
                net_id: 0,
                outgoing_seed: LINK_OUTGOING_SEED,
                incoming_seed: LINK_INCOMING_SEED,
            }
            .to_bytes()
            .to_vec(),
        )
        .expect("the opening answer carries the handshake header");
    packet.serialize(None).expect("a serialisable packet")
}

/// An ordinary encrypted datagram, which is what finishes the handshake.
fn shard_ordinary(crypto: &mut CryptoSystem, sequence: &mut u32) -> Vec<u8> {
    *sequence = sequence.wrapping_add(1);
    let mut packet = OutPacket::new(ProtoHeader {
        seq_id: *sequence,
        rec_id: LINK_RECIPIENT,
        interval: 0x100,
        iteration: 1,
        ..ProtoHeader::default()
    });
    packet
        .add_optional_header(PacketFlags::ECHO_REQUEST, 0f32.to_le_bytes().to_vec())
        .expect("an ordinary header");
    packet
        .serialize(Some(crypto.next()))
        .expect("an encrypted packet")
}

/// One unsequenced, unencrypted arrival.
fn plain_arrival() -> Vec<u8> {
    let mut p = OutPacket::new(ProtoHeader {
        seq_id: 0,
        rec_id: LINK_RECIPIENT,
        interval: 0x100,
        iteration: 1,
        ..ProtoHeader::default()
    });
    p.add_optional_header(PacketFlags::ACK_SEQUENCE, 1_u32.to_le_bytes().to_vec())
        .expect("one optional header");
    p.serialize(None).expect("a serialisable packet")
}

/// The peer asking for `ids.len()` datagrams to be sent again -- one lost datagram each.
fn ask_again(ids: &[u32]) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(&u32::try_from(ids.len()).expect("a small ask").to_le_bytes());
    for id in ids {
        body.extend_from_slice(&id.to_le_bytes());
    }
    let mut p = OutPacket::new(ProtoHeader {
        seq_id: 0,
        rec_id: LINK_RECIPIENT,
        interval: 0x100,
        iteration: 1,
        ..ProtoHeader::default()
    });
    p.add_optional_header(PacketFlags::REQUEST_RETRANSMIT, body)
        .expect("one optional header");
    p.serialize(None).expect("a serialisable packet")
}

// ---------------------------------------------------------------------------------------------
// link.connected.the-lamp-lights-when-the-handshake-finishes-and-not-when-the-shard-first-answers
// ---------------------------------------------------------------------------------------------

/// The lamp lights on the finished handshake and not on the shard's opening answer.
///
/// It is a whole client and not a bare endpoint because the claim is that the value a *frame*
/// publishes moves at that edge and not earlier: the endpoint's own state is half of it and the
/// process-wide value the panel reads is the other half.
pub fn the_link_lamp_lights_on_the_finished_handshake() {
    link_status_holder::disconnected();
    let mut c = HeadlessClient::new(ClientSpec::retail());
    c.attach_replay(
        ClientNetwork::new(LINK_PEER, 7304, "link", "unused", 0).expect("a socket-free endpoint"),
    );
    let mut crypto = CryptoSystem::new(LINK_OUTGOING_SEED);
    let mut sequence = 1_u32;

    let hello = shard_hello();
    c.replay_net_mut()
        .expect("the endpoint")
        .feed(&hello, link_peer(), LocalTime(10.0));
    c.tick(1);
    let half_way =
        c.replay_net_mut().expect("the endpoint").status() == LinkStatus::LoginConnecting;
    let lamp_is_out = {
        let now = c.view().expect_app().clock().cur_time;
        link_status_holder::connection_status(now).is_none()
    };

    let ordinary = shard_ordinary(&mut crypto, &mut sequence);
    c.replay_net_mut()
        .expect("the endpoint")
        .feed(&ordinary, link_peer(), LocalTime(10.1));
    c.tick(1);
    let connected = c.replay_net_mut().expect("the endpoint").status() == LinkStatus::Connected;
    let lamp_is_lit = {
        let now = c.view().expect_app().clock().cur_time;
        link_status_holder::connection_status(now).is_some()
    };

    c.assert_behaviour("link.connected.the-lamp-lights-when-the-handshake-finishes-and-not-when-the-shard-first-answers", move |_| {
        half_way && lamp_is_out && connected && lamp_is_lit
    });
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// The link-status panel's own words
// ---------------------------------------------------------------------------------------------

/// A view that reports one packet-loss figure and no answer for the round trip.
#[derive(Debug)]
struct OneReading {
    loss: f32,
}

impl dereth_ui_screens::view::GameView for OneReading {
    fn packet_loss_percent(&self) -> f32 {
        self.loss
    }
}

/// The shipped gameplay tree with the real string table installed, and the link-status panel
/// written against a view reporting `loss`. The panel's own text comes back.
fn link_panel_text(loss: f32) -> String {
    use dereth_primitives::AssetSource as _;
    use dereth_ui::framework::{DidMapperResolver, LayoutEnum};
    use dereth_ui::{ElementId, UiSystem};
    use dereth_ui_screens::panels::linkstatus::LinkStatusPanel;

    let store = std::sync::Arc::new(dereth_dat::testing::open_store().expect(
        "the shipped string table lives in the retail data files: set DERETH_TEST_DAT_DIR",
    ));
    let master_id = DataId(0x3900_0001);
    let bytes = store.read(master_id).expect("the master property record");
    let master =
        <dereth_assets::MasterProperty as dereth_assets::Decode>::decode_payload(master_id, &bytes)
            .expect("it decodes");
    let mut ui = UiSystem::new((800, 600));
    ui.property_types = master.property_types();
    let mut flow = dereth_ui::UiFlow::new();
    dereth_ui_screens::register_all(&mut ui, &mut flow);
    let source = std::rc::Rc::new(std::sync::Arc::clone(&store));
    let resolver = std::rc::Rc::new(
        DidMapperResolver::load_via_master(source.as_ref()).expect("the id mapper"),
    );
    dereth_ui_screens::env::install(&mut ui, source, resolver);
    let root = dereth_ui_screens::env::create_and_add_root_element(
        &mut ui,
        LayoutEnum(0x1000_0006),
        ElementId(0x1000_0495),
    )
    .expect("the shipped gameplay root");

    let mut panel = LinkStatusPanel::default();
    panel.post_init(&mut ui, root);
    assert!(
        panel.fully_bound(),
        "the shipped layout carries the panel and its text element"
    );
    panel.write(&mut ui, &OneReading { loss });
    panel.text.clone()
}

/// The loss line always carries a number; it is the round trip that can be unknown.
pub fn the_packet_loss_line_always_carries_a_number() {
    use dereth_ui_screens::panels::linkstatus::{string, UNKNOWN};

    let mut always_a_number = true;
    let mut never_the_markers = true;
    for loss in [0.0_f32, INITIAL_PACKET_LOSS, 0.4, 0.0381] {
        let t = link_panel_text(loss);
        always_a_number &= t.contains(&format!("{}[{loss:.2}]", string::PACKET_LOSS));
        never_the_markers &= t.contains(string::PACKET_LOSS)
            && !t.contains(&format!("{}[{UNKNOWN}]", string::PACKET_LOSS));
    }

    // And the line that genuinely can be unknown still says so, which is what makes the reading
    // above a measurement rather than a panel with no unknown marker in it at all.
    let ping_is_unknown = link_panel_text(0.0).contains(&format!("{}[{UNKNOWN}]", string::PING));

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "link.packet-loss.the-line-always-carries-a-number-and-it-is-the-ping-that-can-be-unknown",
        move |_| always_a_number && never_the_markers && ping_is_unknown,
    );
}

/// Before the first reading the line reads as complete loss, and an untouched link is not that.
pub fn a_client_that_has_heard_nothing_reads_as_total_loss() {
    link_status_holder::reset_packet_loss();
    let starts_there =
        (link_status_holder::packet_loss() - INITIAL_PACKET_LOSS).abs() < f32::EPSILON;
    let shown = link_panel_text(INITIAL_PACKET_LOSS).contains("[1.00]");

    // A link nothing has ever touched divides nothing and contributes none -- a different state
    // from the one above, which is the whole point of the starting figure being what it is.
    let mut untouched = dereth_client_net::Net::new(dereth_client_net::NetConfig::default());
    untouched.add_connection(
        LINK_RECIPIENT,
        0,
        1,
        LINK_OUTGOING_SEED,
        LINK_INCOMING_SEED,
        Some(link_peer()),
    );
    let untouched_is_none = untouched.average_packet_loss().abs() < f64::EPSILON
        && untouched.link_status().pkts_received.count() == 0;

    let mut c = HeadlessClient::model();
    c.assert_behaviour("link.packet-loss.a-client-that-has-heard-nothing-reads-as-total-loss-until-the-first-reading", move |_| {
        starts_there && shown && untouched_is_none
    });
}

// ---------------------------------------------------------------------------------------------
// link.packet-loss.what-the-panel-shows-is-the-links-own-figure-on-a-lossy-link-and-zero-on-a-clean-one
// ---------------------------------------------------------------------------------------------

/// A client, a shard, and forty datagrams: the figure a player would read.
///
/// **The shard carries the network clock rather than reading it off the client**, deliberately.
/// A headless client steps its own frame clock by a fixed quantum and leaves the wall clock
/// alone, so a forty-frame loop spans a few milliseconds and a reading two seconds apart could
/// never be taken inside one. The time handed to `feed` here is the same parameter the socket
/// loop passes in production.
fn a_client_told_forty_datagrams(lossy: bool) -> (f32, f64, f64) {
    link_status_holder::reset_packet_loss();
    let mut c = HeadlessClient::new(ClientSpec::retail());
    c.attach_replay(
        ClientNetwork::new(LINK_PEER, 7304, "link", "unused", 0).expect("a socket-free endpoint"),
    );
    // The whole handshake is not needed: the counters live on the transport, and naming the
    // recipient is what the reading keys on.
    c.replay_net_mut()
        .expect("the endpoint")
        .session
        .transport
        .add_connection(
            LINK_RECIPIENT,
            0,
            1,
            LINK_OUTGOING_SEED,
            LINK_INCOMING_SEED,
            Some(link_peer()),
        );

    let step = HEARTBEAT_INTERVAL / 10.0;
    let mut t = 0.0;
    for i in 0_u32..40 {
        t += step;
        let raw = if lossy && i % 10 == 3 {
            ask_again(&[i + 1, i + 2])
        } else {
            plain_arrival()
        };
        let now = LocalTime(t);
        let net = c.replay_net_mut().expect("the endpoint");
        net.feed(&raw, link_peer(), now);
        net.receive_use_time(now);
        c.tick(1);
    }

    let holder = link_status_holder::packet_loss();
    let net = c.replay_net_mut().expect("the endpoint");
    let averages = net.average_packet_loss();
    let s = net.session.transport.link_status();
    let by_hand = 2.0 * (s.pkts_naked.total() / (s.pkts_received.total() + s.pkts_sent.total()));
    c.shutdown();
    (holder, averages, by_hand)
}

/// A lossy link moves the figure off its starting value; a clean one reads exactly none.
pub fn the_panel_shows_the_links_own_packet_loss_figure() {
    let (loss, averages, by_hand) = a_client_told_forty_datagrams(true);
    let moved = (loss - INITIAL_PACKET_LOSS).abs() > f32::EPSILON;
    let between = loss > 0.0 && loss < 1.0;
    #[allow(clippy::cast_possible_truncation)]
    let is_the_links_own =
        (loss - averages as f32).abs() < 1e-6 && (loss - by_hand as f32).abs() < 1e-6;

    let (clean, _, _) = a_client_told_forty_datagrams(false);
    let clean_reads_none = clean.abs() < f32::EPSILON;

    let mut c = HeadlessClient::model();
    c.assert_behaviour("link.packet-loss.what-the-panel-shows-is-the-links-own-figure-on-a-lossy-link-and-zero-on-a-clean-one", move |_| {
        moved && between && is_the_links_own && clean_reads_none
    });
}

// ---------------------------------------------------------------------------------------------
// The first connection refused: one error box, then the process ends
// ---------------------------------------------------------------------------------------------

/// The shard's refusal as it sends one before any connection exists: sequence 0, in the clear,
/// and nothing in it but the error.
fn refusal(code: dereth_transport::conn::NetErrorCode) -> Vec<u8> {
    let mut packet = OutPacket::new(ProtoHeader::default());
    packet
        .add_optional_header(PacketFlags::NET_ERROR, code.pack().to_vec())
        .expect("the refusal header");
    packet.serialize(None).expect("a serialisable packet")
}

/// The connection-error table's id in the shipped dats: what table 8 resolves to.
fn connection_error_table(store: &RetailDatStore) -> DataId {
    dereth_assets::did_by_enum(store, dereth_client::connect_failure::STRING_TABLE_GROUP, 8)
        .expect("table 8 resolves in the shipped dats")
}

/// A wrong client version, refused before the link is up, is one Game Error box quoting table
/// 8's sentence, and the frame that sees it is the last one.
pub fn a_wrong_version_refusal_is_an_error_box_and_then_the_client_exits() {
    use dereth_transport::conn::NetErrorCode;

    let mut c = HeadlessClient::new(ClientSpec::retail());
    c.attach_replay(
        ClientNetwork::new(LINK_PEER, 7304, "link", "unused", 0).expect("a socket-free endpoint"),
    );
    let table = connection_error_table(c.dat_store().expect("a retail client has dats"));
    // The login is under way: the first request goes out and the link is not up.
    c.tick(1);
    let logging_in =
        c.replay_net_mut().expect("the endpoint").status() == LinkStatus::LoginAuthenticating;
    let no_box_yet = c.app_mut().connect_failure().is_none();

    c.replay_net_mut().expect("the endpoint").feed(
        &refusal(NetErrorCode::NetVersionMismatch),
        link_peer(),
        LocalTime(2.0),
    );
    let refused = c.replay_net_mut().expect("the endpoint").login_refusal()
        == Some(NetErrorCode::NetVersionMismatch);
    // The frame that observes the refusal shows the box and ends the loop, drawing nothing.
    let frames_before = c.view().expect_app().frames_drawn();
    let kept_running = c.app_mut().frame();
    let nothing_drawn = c.view().expect_app().frames_drawn() == frames_before;
    let expected = format!(
        "Failed to establish connection to the server: (You do not have the current version of \
         the client installed.) ({:8X}:00A7E948)",
        table.0
    );
    let failure = c
        .app_mut()
        .connect_failure()
        .expect("the box was shown")
        .clone();
    eprintln!("{}: {}", failure.popup.caption, failure.popup.text);
    let the_right_box = failure.popup.caption == "Game Error"
        && failure.popup.text == expected
        && failure.popup.style == dereth_client::connect_failure::ERROR_BOX_STYLE
        && failure.code == NetErrorCode::NetVersionMismatch;
    // And it stays ended: the next frame does not come back to life.
    let still_ended = !c.app_mut().frame();

    // The same refusal after the link has been up is a lost shard, not this box.
    let after_the_link = {
        let mut late = HeadlessClient::new(ClientSpec::retail());
        late.attach_replay(
            ClientNetwork::new(LINK_PEER, 7304, "link", "unused", 0)
                .expect("a socket-free endpoint"),
        );
        let mut crypto = CryptoSystem::new(LINK_OUTGOING_SEED);
        let mut sequence = 1_u32;
        let hello = shard_hello();
        late.replay_net_mut()
            .expect("the endpoint")
            .feed(&hello, link_peer(), LocalTime(10.0));
        let ordinary = shard_ordinary(&mut crypto, &mut sequence);
        late.replay_net_mut()
            .expect("the endpoint")
            .feed(&ordinary, link_peer(), LocalTime(10.1));
        let up = late.replay_net_mut().expect("the endpoint").status() == LinkStatus::Connected;
        late.replay_net_mut().expect("the endpoint").feed(
            &refusal(NetErrorCode::NetVersionMismatch),
            link_peer(),
            LocalTime(10.2),
        );
        let no_refusal = late
            .replay_net_mut()
            .expect("the endpoint")
            .login_refusal()
            .is_none();
        let running = late.app_mut().frame();
        let no_box = late.app_mut().connect_failure().is_none();
        late.shutdown();
        up && no_refusal && running && no_box
    };

    c.assert_behaviour("link.first-connection.a-refusal-before-the-link-is-up-is-an-error-box-and-then-the-client-exits", move |_| {
        logging_in && no_box_yet && refused && !kept_running && nothing_drawn && the_right_box
            && still_ended && after_the_link
    });
    c.shutdown();
}

/// Every code the first connection can end with, and the sentence the shipped table gives it.
///
/// Oracle: the retail `client_local_English.dat`, table 8, read by string hash; the sentences are
/// the shipped rows verbatim, spelling included. The crypto-failure row substitutes an error
/// number a network error does not carry, so it cannot be rendered and says why instead.
pub fn every_refusal_reads_its_own_sentence() {
    use dereth_transport::conn::NetErrorCode as E;

    let c = HeadlessClient::new(ClientSpec::retail());
    let store = Arc::clone(c.dat_store().expect("a retail client has dats"));
    let table = connection_error_table(&store);
    let rows: &[(E, &str)] = &[
        (E::BadServerAddress, "Invalid server address"),
        (
            E::CantBind,
            "Can't bind local socket.  Is another client running?",
        ),
        (
            E::CantSocket,
            "Couldn't create local socket.  Make sure you have a working network.",
        ),
        (
            E::AbortedHandshake,
            "Connection attempt was aborted by the user",
        ),
        (
            E::RunningSpeedhack,
            "Connection closed because use of a speed-altering program was detected",
        ),
        (
            E::NoLogonServer,
            "There is no logon server available for your account.",
        ),
        (
            E::NetVersionMismatch,
            "You do not have the current version of the client installed.",
        ),
        (E::ServerFull, "Server Full"),
        (E::BadCryptoKey, "Invalid or corrupted authentication"),
        (
            E::InsufficientPrivilege,
            "Your account has insufficient priveleges to continue",
        ),
        (
            E::SecondLogon,
            "Connection prempted by another connection attempt",
        ),
        (
            E::ServerClosedConnection,
            "Server has closed this connection",
        ),
        (E::ServerTimedOutClient, "Server timed out the connection"),
        (E::ClientTimedOutServer, "Client timed out the connection"),
        (E::PlayerAlreadyLoggedOn, "Player already logged on"),
        (E::ClientLogOnFailed, "Client logon failed"),
        (E::AccountAuthenticationFailed, "Access Denied."),
        // Stored with an escaped line break, which the lookup turns into a real one.
        (
            E::LogonServerMigrated,
            "Logon was aborted by the server.\nPlease try again.",
        ),
        (E::Generic, "There was an error with this connection"),
    ];
    let mut wrong = Vec::new();
    for (code, sentence) in rows {
        let got = dereth_client::connect_failure::connect_failure(*code, &*store)
            .popup
            .text;
        let want = format!(
            "Failed to establish connection to the server: ({sentence}) ({:8X}:{:08X})",
            table.0,
            code.string_id()
        );
        if got != want {
            wrong.push(format!("{code:?}: {got:?}"));
        }
    }
    let crypto = dereth_client::connect_failure::connect_failure(E::CantCrypto, &*store)
        .popup
        .text;
    let crypto_says_why =
        crypto.contains("<could not render string:") && crypto.contains("Reason = 6>");
    if !crypto_says_why {
        wrong.push(format!("CantCrypto: {crypto:?}"));
    }
    assert!(
        wrong.is_empty(),
        "rows that did not read their own sentence: {wrong:#?}"
    );

    let mut c = c;
    c.assert_behaviour("link.first-connection.every-refusal-reads-its-own-sentence-from-the-connection-error-strings", move |_| {
        wrong.is_empty() && crypto_says_why
    });
    c.shutdown();
}
