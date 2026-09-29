//! Inside the 60 s window nothing is sent and nothing latches; past the latch the client sends one
//! lone net error and repeats until a sync lands in the window; a caller that never publishes
//! cur_time never accuses.
//! Fixture: recorded messages and synthetic state or packets.

use dereth_client_net::net::{Net, NetConfig};
use dereth_primitives::LocalTime;
use dereth_transport::conn::NetErrorCode;
use dereth_transport::wire::{PacketFlags, ParsedPacket};
use dereth_transport::{CryptoSystem, OutPacket, ProtoHeader};

const REC_ID: u16 = 0x0B;
const ITERATION: u16 = 1;
/// The incoming stream is seeded from `add_connection`'s third argument.
const INCOMING_SEED: u32 = 0xDEAD_BEEF;
const OUTGOING_SEED: u32 = 0x1234_5678;

fn server_addr() -> std::net::SocketAddr {
    "127.0.0.1:19000".parse().expect("addr")
}

/// A `Net` with one registered peer, plus the server-side ISAAC stream that seals its datagrams.
fn linked() -> (Net, CryptoSystem) {
    let mut net = Net::new(NetConfig::default());
    net.add_connection(
        REC_ID,
        REC_ID,
        ITERATION,
        INCOMING_SEED,
        OUTGOING_SEED,
        Some(server_addr()),
    );
    (net, CryptoSystem::new(INCOMING_SEED))
}

/// One sequenced, encrypted server datagram carrying nothing but a `TimeSync`.
fn time_sync(seq_id: u32, server_time: f64, crypto: &mut CryptoSystem) -> Vec<u8> {
    let mut p = OutPacket::new(ProtoHeader {
        seq_id,
        rec_id: REC_ID,
        iteration: ITERATION,
        ..Default::default()
    });
    p.add_optional_header(PacketFlags::TIME_SYNC, server_time.to_le_bytes().to_vec())
        .expect("TimeSync section");
    p.serialize(Some(crypto.next())).expect("serialize")
}

/// Feed one TimeSync at a given published current game time, then drain the send side.
///
/// Everything runs at `LocalTime(0.0)` so the 2.0 s cumulative-ACK cadence and the interval
/// counter contribute nothing: whatever comes out is the tail's doing and only the tail's.
fn sync_at(
    net: &mut Net,
    crypto: &mut CryptoSystem,
    seq: u32,
    cur: f64,
    server: f64,
) -> Vec<Vec<u8>> {
    net.set_cur_time(cur);
    net.feed(
        &time_sync(seq, server, crypto),
        Some(server_addr()),
        LocalTime(0.0),
    )
    .expect("the datagram is well formed and in sequence");
    net.tick(LocalTime(0.0), std::time::Duration::from_millis(50));
    net.take_outgoing().into_iter().map(|(b, _)| b).collect()
}

/// The datagram retail builds: a lone, unsequenced, plaintext `NetError` section carrying
/// `ID_ConnectionError_RunningSpeedhack`.
///
/// Built here from the wire spec rather than copied out of a run, so the production path and the
/// expectation reach the same bytes by two different routes.
fn expected_net_error() -> Vec<u8> {
    let mut p = OutPacket::new(ProtoHeader {
        // A packet with no fragments and only disposable headers
        // **reuses** `highest_id_sent`, which is 1 on a fresh `FlowQueue`, and is sent in the
        // clear.
        seq_id: 1,
        rec_id: REC_ID,
        interval: 0,
        iteration: ITERATION,
        ..Default::default()
    });
    p.add_optional_header(
        PacketFlags::NET_ERROR,
        NetErrorCode::RunningSpeedhack.pack().to_vec(),
    )
    .expect("NetError section");
    p.serialize(None).expect("serialize")
}

// -------------------------------------------------------------------------------------------
// 1. Below the threshold: silence, for ever.
// -------------------------------------------------------------------------------------------

/// the reset arm. A client whose clock is inside the 60 s window says nothing, and
/// the latch stays at 0.0 no matter how many syncs go by.
#[test]
fn a_client_inside_the_window_sends_nothing_and_never_latches() {
    let (mut net, mut crypto) = linked();
    for (i, cur) in [1000.0f64, 1000.5, 1059.999, 1060.0, 1000.0]
        .into_iter()
        .enumerate()
    {
        let out = sync_at(
            &mut net,
            &mut crypto,
            2 + u32::try_from(i).expect("small"),
            cur,
            1000.0,
        );
        assert!(
            out.is_empty(),
            "cur_time {cur} is within 60 s of the server's 1000.0"
        );
        assert_eq!(
            net.speed_hack_detection_time(),
            0.0,
            "the latch stays clear at cur_time {cur}"
        );
    }
}

// -------------------------------------------------------------------------------------------
// 2. Past the threshold: latch first, accuse 60 s later.
// -------------------------------------------------------------------------------------------

/// **The rejecting station.** Past the window and past the latch, `Net` puts exactly the retail
/// `NetError` datagram in the outgoing queue and nothing else.
#[test]
fn past_the_window_and_the_latch_the_client_accuses_itself_with_one_lone_net_error() {
    let (mut net, mut crypto) = linked();

    // The first out-of-tolerance sync only records the time.
    let out = sync_at(&mut net, &mut crypto, 2, 1100.0, 1000.0);
    assert!(
        out.is_empty(),
        "the latch arm stores the time and returns; it sends nothing"
    );
    assert_eq!(
        net.speed_hack_detection_time(),
        1100.0,
        "the speed-check time = cur_time"
    );

    // Still out of tolerance, but not yet 60 s past the latch.
    let out = sync_at(&mut net, &mut crypto, 3, 1159.0, 1000.0);
    assert!(out.is_empty(), "cur <= latch + 60 sends nothing");
    let out = sync_at(&mut net, &mut crypto, 4, 1160.0, 1000.0);
    assert!(out.is_empty(), "the equality boundary emits nothing");

    // Past both boundaries.
    let out = sync_at(&mut net, &mut crypto, 5, 1160.001, 1000.0);
    assert_eq!(out.len(), 1, "one datagram, and only one");

    let parsed = ParsedPacket::parse(&out[0]).expect("the accusation parses");
    assert_eq!(
        parsed.header.header.0,
        PacketFlags::NET_ERROR,
        "the header carries section 0x00100000 and nothing else -- flag bit 0x02 is exclusive"
    );
    assert!(
        parsed.fragments.is_empty(),
        "no blob rides an exclusive header"
    );
    assert_eq!(parsed.optional.len(), 1, "one section on the packet");
    assert!(
        parsed.checksum_ok(None),
        "flag bit 0x01 is disposable, so the packet is unencrypted and CRCed in the clear"
    );
    assert_eq!(
        parsed.header.rec_id, REC_ID,
        "the transmit path stamps the recipient id with the net id"
    );
    assert_eq!(parsed.header.iteration, ITERATION);
    assert_eq!(
        parsed.header.seq_id, 1,
        "disposable-only: the highest sent id is reused, not advanced"
    );

    let body = parsed
        .optional
        .get(&PacketFlags::NET_ERROR)
        .expect("the NetError section");
    assert_eq!(
        body.as_slice(),
        &[0x1B, 0x61, 0x61, 0x03, 0x08, 0x00, 0x00, 0x00],
        "the error header packs string id = str_hash(\"ID_ConnectionError_RunningSpeedhack\") \
         = 0x0361611B, table id = 8"
    );
    assert_eq!(
        body.as_slice(),
        NetErrorCode::RunningSpeedhack.pack().as_slice()
    );

    // And the whole datagram, byte for byte, against an independently built one.
    assert_eq!(
        out[0],
        expected_net_error(),
        "the exact bytes retail would put on the wire"
    );
}

/// Behaviour: link.time-sync.a-client-running-fast-accuses-itself-once-per-sync-until-it-is-back-in-the-window
/// The sending arm writes nothing back to the latch (it has no store to
/// the latch), so a client that stays fast re-accuses itself on every subsequent TimeSync --
/// and one sync back inside the window silences it again.
#[test]
fn the_accusation_repeats_until_a_sync_lands_inside_the_window() {
    let (mut net, mut crypto) = linked();
    assert!(
        sync_at(&mut net, &mut crypto, 2, 1100.0, 1000.0).is_empty(),
        "latch"
    );
    assert_eq!(
        sync_at(&mut net, &mut crypto, 3, 1200.0, 1000.0).len(),
        1,
        "first accusation"
    );
    assert_eq!(
        net.speed_hack_detection_time(),
        1100.0,
        "the latch is not cleared by sending"
    );
    assert_eq!(
        sync_at(&mut net, &mut crypto, 4, 1300.0, 1000.0).len(),
        1,
        "and again"
    );

    // One good sync zeroes the double.
    assert!(sync_at(&mut net, &mut crypto, 5, 1300.0, 1300.0).is_empty());
    assert_eq!(
        net.speed_hack_detection_time(),
        0.0,
        "reset to 0.0, both dwords"
    );
    // ...so the next out-of-tolerance sync is a first offence again: latch, no packet.
    assert!(
        sync_at(&mut net, &mut crypto, 6, 1400.0, 1300.0).is_empty(),
        "latch, not accuse"
    );
    assert_eq!(net.speed_hack_detection_time(), 1400.0);
}

/// A client that never publishes current game time -- every replay, and every in-crate fixture
/// that predates this card -- reads 0.0, which is always inside the window of a real server time
/// and therefore can only ever *clear* the latch.
///
/// This is why the replay corpus is unaffected: it is not that the recorded sessions happen to be
/// in tolerance (they are), it is that the default reading cannot reach the sending arm at all.
#[test]
fn a_caller_that_never_publishes_cur_time_can_never_accuse() {
    let mut net = Net::new(NetConfig::default());
    net.add_connection(
        REC_ID,
        REC_ID,
        ITERATION,
        INCOMING_SEED,
        OUTGOING_SEED,
        Some(server_addr()),
    );
    let mut crypto = CryptoSystem::new(INCOMING_SEED);
    for seq in 2..8u32 {
        // No `set_cur_time` anywhere.
        net.feed(
            &time_sync(seq, 9.0e8, &mut crypto),
            Some(server_addr()),
            LocalTime(0.0),
        )
        .expect("accepted");
        net.tick(LocalTime(0.0), std::time::Duration::from_millis(50));
        assert!(net.take_outgoing().is_empty(), "seq {seq}");
    }
    assert_eq!(net.speed_hack_detection_time(), 0.0);
}
