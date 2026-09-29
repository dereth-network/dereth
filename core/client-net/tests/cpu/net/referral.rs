//! A Referral queues one WorldLoginRequest (28 bytes, cookie) per server, resends every 1/3 s up to
//! the computed cap then raises a world-connection error, ends on the referred server
//! ConnectRequest, re-refers with the stored cookie on the 140 s timeout, and world/logon switches
//! move the current server.
//! Fixture: recorded messages and synthetic state or packets.

use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use dereth_client_net::net::{Net, NetConfig};
use dereth_client_net::RecipientId;
use dereth_primitives::LocalTime;
use dereth_transport::conn::{ConnectionState, Referral, ServerSwitch, ServerSwitchType};
use dereth_transport::flow::{world_login_cap, WORLD_LOGIN_RESEND};
use dereth_transport::wire::{OutPacket, PacketFlags, ParsedPacket, ProtoHeader};
use dereth_transport::CryptoSystem;

const S2C_SEED: u32 = 0xDEAD_BEEF;
const C2S_SEED: u32 = 0xF00D_F00D;

/// The client's own recipient id on the login connection. Non-zero on purpose: a recipient id of 0
/// is the client's "this receiver slot is free" sentinel (and the send uses the same test), so a connection parked on id 0 cannot receive
/// a sequenced packet at all.
const LOGON_REC: u16 = 0x0B;

/// ACE's `PacketOutboundReferral` hard-codes `(ushort)0x18` as the server id.
const WORLD_ID: u16 = 0x18;

const COOKIE: u64 = 0x0123_4567_89AB_CDEF;

fn logon() -> SocketAddr {
    "127.0.0.1:19000".parse().expect("addr")
}
fn world() -> SocketAddr {
    "127.0.0.1:19011".parse().expect("addr")
}

fn v4(addr: SocketAddr) -> std::net::SocketAddrV4 {
    match addr {
        SocketAddr::V4(a) => a,
        SocketAddr::V6(_) => panic!("v4 only"),
    }
}

/// ACE's `PacketOutboundReferral` body for `COOKIE`, `127.0.0.1:19011`, server `0x18`, written out
/// by hand from the server's outbound-referral writer:
///
/// ```csharp
/// DataWriter.Write(worldConnectionKey);   // ulong, little-endian
/// DataWriter.Write((ushort)2);            // sin_family = AF_INET
/// DataWriter.WriteUInt16BE(port);         // sin_port, network order
/// DataWriter.Write(host);                 // four address bytes
/// DataWriter.Write(0ul);                  // sin_zero[8]
/// DataWriter.Write((ushort)0x18);         // server id
/// DataWriter.Write((ushort)0);            // padding
/// DataWriter.Write(0u);                   // struct alignment tail
/// ```
///
/// 19011 is `0x4A43`.
const ACE_REFERRAL: [u8; 32] = [
    0xEF, 0xCD, 0xAB, 0x89, 0x67, 0x45, 0x23, 0x01, // cookie
    0x02, 0x00, // sin_family
    0x4A, 0x43, // sin_port, big-endian: 19011
    0x7F, 0x00, 0x00, 0x01, // sin_addr: 127.0.0.1
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // sin_zero
    0x18, 0x00, // server id
    0x00, 0x00, // padding
    0x00, 0x00, 0x00, 0x00, // alignment
];

/// A server -> client datagram carrying one **non-disposable** section, so it is sequenced and
/// ISAAC-encrypted, which is exactly what ACE's `EncryptedChecksum | Referral` produces.
///
/// `Referral`'s header flags are `0x40000062` — `EXCLUSIVE | PRIORITY | TOUCH_CONNECTION` and the
/// undocumented `0x40000000`, with **no** `DISPOSABLE` bit — so `OutPacket::needs_encryption` is
/// true and `serialize` demands a key. The incoming cipher starts from the outgoing seed and draws
/// one value per accepted encrypted sequence, so a
/// mirror seeded the same way and stepped in the same order produces the same keys.
fn s2c(seq: u32, mask: u32, body: Vec<u8>, key: &mut CryptoSystem) -> Vec<u8> {
    let mut p = OutPacket::new(ProtoHeader {
        seq_id: seq,
        rec_id: LOGON_REC,
        iteration: 1,
        ..Default::default()
    });
    p.add_optional_header(mask, body).expect("section");
    p.serialize(Some(key.next())).expect("encrypted")
}

/// A `Net` with the login connection registered as registers
/// it, plus the mirror of its inbound ISAAC stream.
fn net() -> (Net, CryptoSystem) {
    let mut n = Net::new(NetConfig::default());
    n.add_connection(LOGON_REC, LOGON_REC, 1, S2C_SEED, C2S_SEED, Some(logon()));
    (n, CryptoSystem::new(S2C_SEED))
}

fn world_logins(n: &mut Net) -> Vec<(Vec<u8>, Option<SocketAddr>)> {
    n.take_outgoing()
        .into_iter()
        .filter(|(b, _)| {
            ParsedPacket::parse(b)
                .is_ok_and(|p| p.header.header.contains(PacketFlags::WORLD_LOGIN_REQUEST))
        })
        .collect()
}

/// Feed one `Referral` for `(cookie, addr, id)` on the login connection at sequence `seq`.
fn feed_referral(
    n: &mut Net,
    key: &mut CryptoSystem,
    seq: u32,
    cookie: u64,
    addr: SocketAddr,
    id: u16,
    now: f64,
) {
    let body = Referral {
        cookie,
        addr: v4(addr),
        id_server: id,
        family: 2,
    }
    .to_bytes();
    let dg = s2c(seq, PacketFlags::REFERRAL, body.to_vec(), key);
    n.feed(&dg, Some(logon()), LocalTime(now))
        .expect("a Referral is a well-formed packet");
}

// -------------------------------------------------------------------------------------------
// The bytes
// -------------------------------------------------------------------------------------------

/// Oracle: ACE's `PacketOutboundReferral` (above) and the client's own
/// field offsets — the cookie, the address and the server id in that order, with the
/// header's common base at `+0x18`. Two independent oracles, agreeing field for field.
#[test]
fn the_referral_bytes_are_ace_s_bytes() {
    let r = Referral::from_bytes(&ACE_REFERRAL).expect("32 bytes");
    assert_eq!(r.cookie, COOKIE, "the cookie at +0, little-endian");
    assert_eq!(r.family, 2, "sin_family at +8 is AF_INET");
    assert_eq!(
        r.addr,
        v4(world()),
        "sin_port is big-endian, sin_addr is the four host bytes"
    );
    assert_eq!(r.id_server, WORLD_ID, "the server id at +24");
    assert_eq!(r.to_bytes(), ACE_REFERRAL, "the round trip is byte-exact");

    // Calibration: the decoder must notice every field, or the equalities above prove nothing.
    for (at, what) in [
        (0usize, "cookie"),
        (10, "port"),
        (12, "address"),
        (24, "server id"),
    ] {
        let mut flipped = ACE_REFERRAL;
        flipped[at] ^= 0xFF;
        assert_ne!(
            Referral::from_bytes(&flipped).expect("32 bytes"),
            r,
            "flipping byte {at} ({what}) changed nothing"
        );
    }

    assert!(
        Referral::from_bytes(&ACE_REFERRAL[..31]).is_err(),
        "31 bytes is a truncated section, not a short read"
    );
}

/// The datagram puts on the wire, field for field.
///
/// The packet header starts zeroed and receives only its flags, checksum and
/// data length, so every other field is a hard zero. There is no recipient connection
/// for the referred server yet, so there is no id to take.
#[test]
fn the_world_login_request_is_28_bytes_with_a_zeroed_header_and_the_cookie() {
    let (mut n, mut key) = net();
    feed_referral(&mut n, &mut key, 2, COOKIE, world(), WORLD_ID, 1.0);

    let out = world_logins(&mut n);
    assert_eq!(out.len(), 1);
    let (bytes, dest) = &out[0];
    assert_eq!(bytes.len(), 28, "20-byte ProtoHeader + the 8-byte cookie");
    assert_eq!(
        dest,
        &Some(world()),
        "to the entry's server address verbatim -- the connect ack's port + 1 rule is not this path's"
    );

    let p = ParsedPacket::parse(bytes).expect("our own packet parses");
    assert_eq!(p.header.seq_id, 0, "unsequenced");
    assert_eq!(
        p.header.header.0,
        PacketFlags::WORLD_LOGIN_REQUEST,
        "the packet flags are the section mask"
    );
    assert_eq!(
        p.header.rec_id, 0,
        "the header is zeroed and the recipient id is never written"
    );
    assert_eq!(p.header.interval, 0);
    assert_eq!(p.header.datalen, 8);
    assert_eq!(p.header.iteration, 0);
    assert!(
        p.checksum_ok(None),
        "flags 7 has the disposable bit, so the packet takes no ISAAC key"
    );
    assert_eq!(
        p.optional
            .get(&PacketFlags::WORLD_LOGIN_REQUEST)
            .map(Vec::as_slice),
        Some(COOKIE.to_le_bytes().as_slice()),
        "the section body is the cookie, little-endian, and nothing else"
    );

    // The whole datagram, pinned. This is a change-detector, not a retail capture: no recorded
    // session contains a WorldLoginRequest. The checksum algorithm's own oracle is the retail
    // Disconnect in `disconnect.rs` and the vectors in `checksum_and_cipher_vectors.rs`.
    assert_eq!(
        bytes.as_slice(),
        [
            0x00, 0x00, 0x00, 0x00, // seq_id
            0x00, 0x00, 0x02, 0x00, // header = 0x00020000 WorldLoginRequest
            0x3B, 0x84, 0xCA, 0x45, // checksum
            0x00, 0x00, // rec_id
            0x00, 0x00, // interval
            0x08, 0x00, // datalen
            0x00, 0x00, // iteration
            0xEF, 0xCD, 0xAB, 0x89, 0x67, 0x45, 0x23, 0x01, // cookie
        ]
        .as_slice()
    );
}

// -------------------------------------------------------------------------------------------
// Calibration
// -------------------------------------------------------------------------------------------

/// **The known-negative.** The same connection and the same clock, with a section that is not a
/// `Referral`, must produce nothing at all on the referral path.
#[test]
fn an_ordinary_datagram_produces_no_world_login_request() {
    let (mut n, mut key) = net();
    let dg = s2c(
        2,
        PacketFlags::NET_ERROR_DISCONNECT,
        dereth_transport::conn::NetErrorCode::None.pack().to_vec(),
        &mut key,
    );
    n.feed(&dg, Some(logon()), LocalTime(1.0))
        .expect("accepted");
    for t in 1..20 {
        n.process_connections(LocalTime(f64::from(t)));
        n.tick(LocalTime(f64::from(t)), Duration::from_millis(50));
    }
    assert!(n.referral_queue().is_empty());
    assert!(world_logins(&mut n).is_empty());
    assert!(!n.take_world_connection_error());
}

// -------------------------------------------------------------------------------------------
// The queue
// -------------------------------------------------------------------------------------------

/// The new entry: `n_auths_sent = 0`, `localtime_to_send_next_world_auth = 0.0`,
/// the address and the cookie copied. The zero time is why the first request goes out at once —
/// the referral queue's gate is `entry.t < current local time`.
#[test]
fn a_referral_queues_one_entry_and_fires_immediately() {
    let (mut n, mut key) = net();
    feed_referral(&mut n, &mut key, 2, COOKIE, world(), WORLD_ID, 1.0);

    assert_eq!(n.referral_queue().len(), 1);
    let e = n.referral_queue()[0];
    assert_eq!(e.id_server, WORLD_ID);
    assert_eq!(e.cookie, COOKIE);
    assert_eq!(e.server_addr, world());
    assert_eq!(
        e.n_auths_sent, 1,
        "the referral handler's tail walks the referral queue at once"
    );
    assert!(
        (e.localtime_to_send_next_world_auth.0 - (1.0 + WORLD_LOGIN_RESEND)).abs() < 1e-12,
        "t + 0.333333333"
    );
    assert_eq!(world_logins(&mut n).len(), 1);
}

/// A referral naming a server already in the queue returns without
/// touching anything — no second entry, no extra datagram, and the pending entry's cadence is not
/// restarted.
#[test]
fn a_second_referral_for_the_same_server_is_ignored() {
    let (mut n, mut key) = net();
    feed_referral(&mut n, &mut key, 2, COOKIE, world(), WORLD_ID, 1.0);
    let after_first = n.referral_queue()[0];
    let _ = world_logins(&mut n);

    // A different cookie and a different address, same server id: the duplicate scan keys on
    // `id_server` alone.
    feed_referral(
        &mut n,
        &mut key,
        3,
        0x9999,
        "127.0.0.1:19099".parse().unwrap(),
        WORLD_ID,
        1.1,
    );
    assert_eq!(n.referral_queue().len(), 1, "one entry per server id");
    assert_eq!(
        n.referral_queue()[0],
        after_first,
        "the standing entry is untouched"
    );
    assert!(
        world_logins(&mut n).is_empty(),
        "and nothing extra goes out"
    );
}

/// Two different worlds are two independent entries with two independent cadences.
#[test]
fn two_referrals_for_different_worlds_both_run() {
    let (mut n, mut key) = net();
    let other: SocketAddr = "127.0.0.2:19022".parse().unwrap();
    feed_referral(&mut n, &mut key, 2, COOKIE, world(), WORLD_ID, 1.0);
    feed_referral(&mut n, &mut key, 3, 0xAAAA_BBBB, other, 0x19, 1.0);
    assert_eq!(n.referral_queue().len(), 2);

    let out = world_logins(&mut n);
    assert_eq!(out.len(), 2);
    let mut dests: Vec<SocketAddr> = out.iter().filter_map(|(_, d)| *d).collect();
    dests.sort_unstable();
    assert_eq!(dests, vec![world(), other]);
}

/// The receiver table is 256 entries, so
/// a referral naming a wider id is dropped before anything else happens.
#[test]
fn an_id_server_outside_the_receiver_table_is_ignored() {
    let (mut n, mut key) = net();
    feed_referral(&mut n, &mut key, 2, COOKIE, world(), 0x100, 1.0);
    assert!(n.referral_queue().is_empty());
    assert!(world_logins(&mut n).is_empty());
}

/// When the named slot is **live**, the referral is not a new login —
/// it refreshes that connection's referral cookie and returns. Nothing is queued and nothing
/// is sent.
#[test]
fn a_referral_for_a_live_connection_only_refreshes_its_cookie() {
    let (mut n, mut key) = net();
    assert_eq!(n.referral_cookie(RecipientId(LOGON_REC)), 0);

    feed_referral(&mut n, &mut key, 2, COOKIE, world(), LOGON_REC, 1.0);

    assert_eq!(
        n.referral_cookie(RecipientId(LOGON_REC)),
        COOKIE,
        "the live arm stores the cookie at ReceiverData+0x70"
    );
    assert!(n.referral_queue().is_empty(), "and queues nothing");
    assert!(world_logins(&mut n).is_empty(), "and sends nothing");
    assert_eq!(
        n.connection_state(RecipientId(LOGON_REC)),
        ConnectionState::Connected,
        "the connection is left standing"
    );
}

/// the connection-state test — a referral for a connection that
/// is already past `Connected` removes it **first**, which frees the slot, and the same call
/// then queues the new login. That is the disposal half of the re-connect path.
#[test]
fn a_referral_for_a_tearing_down_connection_removes_it_then_queues() {
    let (mut n, mut key) = net();
    // A second connection so that removing it does not take connection removal's server log-off
    // branch (it is neither the logon nor the current-server recipient).
    n.add_connection(WORLD_ID, WORLD_ID, 1, 1, 2, Some(world()));
    // Drive it into `DisconnectReceived` through the 140-second silence, the ordinary way.
    n.process_connections(LocalTime(141.0));
    assert_eq!(
        n.connection_state(RecipientId(WORLD_ID)),
        ConnectionState::DisconnectReceived
    );

    feed_referral(&mut n, &mut key, 2, COOKIE, world(), WORLD_ID, 142.0);

    assert_eq!(
        n.connection_state(RecipientId(WORLD_ID)),
        ConnectionState::Disconnected,
        "the connection was removed"
    );
    assert_eq!(
        n.referral_queue().len(),
        1,
        "and the slot being free, the referral queued"
    );
    assert_eq!(world_logins(&mut n).len(), 1);
}

// -------------------------------------------------------------------------------------------
// The cadence
// -------------------------------------------------------------------------------------------

/// The gate is `entry.localtime_to_send_next_world_auth < t`, and
/// `t` is sampled once. Nothing else paces this — not `tick`,
/// not the flow queue's 0.5 s interval.
#[test]
fn the_resend_cadence_is_one_third_of_a_second_on_the_clock_alone() {
    let (mut n, mut key) = net();
    feed_referral(&mut n, &mut key, 2, COOKIE, world(), WORLD_ID, 1.0);
    assert_eq!(world_logins(&mut n).len(), 1, "the first one is immediate");

    // Not yet due: 1.0 + 0.333333333 is still ahead.
    n.process_connections(LocalTime(1.3));
    assert!(
        world_logins(&mut n).is_empty(),
        "0.3 s is short of 0.333333333"
    );
    assert_eq!(n.referral_queue()[0].n_auths_sent, 1);

    // Due.
    n.process_connections(LocalTime(1.4));
    assert_eq!(world_logins(&mut n).len(), 1);
    assert_eq!(n.referral_queue()[0].n_auths_sent, 2);
    assert!(
        (n.referral_queue()[0].localtime_to_send_next_world_auth.0 - 1.733_333_333).abs() < 1e-9
    );

    // A whole second of clock is three requests, not four: the next-send time is computed from
    // the *pass*'s clock, not from the previous due time, so the cadence does not catch up.
    let mut sent = 0;
    let mut t = 1.4;
    for _ in 0..30 {
        t += 0.1;
        n.process_connections(LocalTime(t));
        sent += world_logins(&mut n).len();
    }
    assert_eq!(
        sent, 7,
        "3.0 s of clock at 0.4 s effective spacing (0.333.. rounded up to a tick)"
    );
}

/// `280.0 / 0.333333333` is **840.00000084**, the comparison is
/// `cap <= n_auths_sent`, and it runs *before* the increment — so the entry is still eligible at
/// `n_auths_sent == 840` and the client sends **841** requests before giving up.
///
/// The give-up arm removes the entry (an unordered remove by index) and notifies the plugins of
/// the world-connection-error status (5, with two zero arguments). It raises no `NetError`,
/// sends no `Disconnect` and touches no other connection.
#[test]
fn the_cadence_gives_up_after_the_computed_cap_and_raises_a_world_connection_error() {
    assert!(
        (world_login_cap() - 840.000_000_84).abs() < 1e-6,
        "the cap is a run-time division of the two double constants, not a written-out 840"
    );

    let (mut n, mut key) = net();
    feed_referral(&mut n, &mut key, 2, COOKIE, world(), WORLD_ID, 1.0);
    let mut sent = world_logins(&mut n).len();

    let mut t = 1.0;
    let mut last_send_at = 1.0;
    while !n.referral_queue().is_empty() {
        t += WORLD_LOGIN_RESEND + 1e-6;
        n.process_connections(LocalTime(t));
        let n_now = world_logins(&mut n).len();
        if n_now > 0 {
            last_send_at = t;
        }
        sent += n_now;
        assert!(
            t < 400.0,
            "the window is 280 s; this loop should not run past it"
        );
    }

    assert_eq!(sent, 841, "n_auths_sent 0..=840 are all eligible");
    assert!(
        (last_send_at - 1.0 - 840.0 * WORLD_LOGIN_RESEND).abs() < 0.01,
        "first send to last send is 840 intervals of 0.333333333 s -- the 280.0 s window, \
         measured at {last_send_at}"
    );
    assert!(
        n.take_world_connection_error(),
        "the plugins are notified of the world-connection error"
    );
    assert!(
        !n.take_world_connection_error(),
        "the edge is consumed once"
    );
    assert_eq!(
        n.connection_state(RecipientId(LOGON_REC)),
        ConnectionState::DisconnectReceived,
        "the login connection timed out on its own 140 s silence, not on the referral"
    );
    assert!(!n.log_off_sent(), "the referral give-up is not a goodbye");
}

/// The referral-queue scan: the referred server answering ends
/// the cadence. Without it the client would hammer a world server it is already connected to for
/// 280 seconds.
#[test]
fn the_referred_server_s_connect_request_ends_the_cadence() {
    let (mut n, mut key) = net();
    feed_referral(&mut n, &mut key, 2, COOKIE, world(), WORLD_ID, 1.0);
    assert_eq!(world_logins(&mut n).len(), 1);

    // The connection-request handler's step 3: register the peer under the id the referral named.
    n.add_connection(WORLD_ID, WORLD_ID, 1, 0x11, 0x22, Some(world()));
    assert!(n.referral_queue().is_empty(), "the spent entry is dropped");

    for i in 1..10 {
        n.process_connections(LocalTime(1.0 + f64::from(i) * 0.5));
    }
    assert!(world_logins(&mut n).is_empty(), "and nothing more is sent");
}

/// The send returns 0 once log-off has been sent. The referral-queue walk still
/// advances `n_auths_sent` and the timer — the attempt is made and the datagram is dropped on the
/// way to the socket, which is why the goodbye is still the last thing on the wire.
#[test]
fn after_the_goodbye_the_cadence_counts_but_transmits_nothing() {
    let (mut n, mut key) = net();
    feed_referral(&mut n, &mut key, 2, COOKIE, world(), WORLD_ID, 1.0);
    let _ = n.take_outgoing();

    n.log_off_server();
    let _ = n.take_outgoing();

    n.process_connections(LocalTime(2.0));
    assert!(
        world_logins(&mut n).is_empty(),
        "the send refuses once log-off has been sent"
    );
    assert_eq!(
        n.referral_queue()[0].n_auths_sent,
        2,
        "the attempt was still made and still counted"
    );
}

// -------------------------------------------------------------------------------------------
// The 140-second self-referral
// -------------------------------------------------------------------------------------------

/// Behaviour: link.referral-cookie.outlives-the-handshake-that-carried-it
/// Behaviour: link.silent-shard.logs-in-again-once-with-the-cookie-it-was-given
/// A connection that was `Connected`, has a
/// referral cookie and times out hands a `Referral` it built itself to its own referral handler.
/// Nothing goes on the wire as a `Referral`; what comes out is the
/// `WorldLoginRequest` of the re-login.
#[test]
fn the_140_second_timeout_re_refers_with_the_stored_cookie() {
    let (mut n, mut key) = net();
    // A second connection, so that removing it does not take connection removal's server log-off
    // branch. It is a world we were referred to and have not been switched to.
    n.add_connection(WORLD_ID, WORLD_ID, 1, 1, 2, Some(world()));
    // The live arm of the referral handler stores the cookie on the connection the referral names.
    feed_referral(&mut n, &mut key, 2, COOKIE, world(), WORLD_ID, 1.0);
    assert_eq!(n.referral_cookie(RecipientId(WORLD_ID)), COOKIE);
    {
        // The connection must actually be `Connected` for the arm to fire; drive it there with
        // an encrypted packet of its own. Its incoming cipher starts from seed 1.
        let mut k2 = CryptoSystem::new(1);
        let mut p = OutPacket::new(ProtoHeader {
            seq_id: 2,
            rec_id: WORLD_ID,
            iteration: 1,
            ..Default::default()
        });
        p.add_optional_header(PacketFlags::ECHO_RESPONSE, vec![0u8; 8])
            .expect("section");
        let dg = p.serialize(Some(k2.next())).expect("encrypted");
        n.feed(&dg, Some(world()), LocalTime(1.0))
            .expect("accepted");
    }
    assert_eq!(
        n.connection_state(RecipientId(WORLD_ID)),
        ConnectionState::Connected
    );
    let _ = n.take_outgoing();

    n.process_connections(LocalTime(142.0));

    assert_eq!(
        n.connection_state(RecipientId(WORLD_ID)),
        ConnectionState::Disconnected,
        "the referral handler's `> Connected` arm removed it"
    );
    assert_eq!(n.referral_queue().len(), 1, "and queued the re-login");
    assert_eq!(n.referral_queue()[0].cookie, COOKIE);
    assert_eq!(n.referral_queue()[0].server_addr, world());
    let out = world_logins(&mut n);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].1, Some(world()));
}

/// Behaviour: link.silent-shard.with-no-cookie-is-an-ordinary-timeout
/// A timeout without a referral cookie is the ordinary disconnect.
#[test]
fn a_timeout_without_a_referral_cookie_is_the_ordinary_disconnect() {
    let (mut n, _key) = net();
    n.add_connection(WORLD_ID, WORLD_ID, 1, 1, 2, Some(world()));
    assert_eq!(n.referral_cookie(RecipientId(WORLD_ID)), 0);

    n.process_connections(LocalTime(142.0));

    assert_eq!(
        n.connection_state(RecipientId(WORLD_ID)),
        ConnectionState::DisconnectReceived,
        "the lifecycle, untouched"
    );
    assert!(n.referral_queue().is_empty());
}

// -------------------------------------------------------------------------------------------
// ServerSwitch — the same native family, and small
// -------------------------------------------------------------------------------------------

/// A world switch: the current-server recipient becomes the
/// recipient id the packet arrived on. The first switch is accepted whatever its stamp; after
/// that the stamp must be strictly newer.
#[test]
fn a_world_switch_moves_the_current_server_and_a_stale_stamp_does_not() {
    let (mut n, mut key) = net();
    n.add_connection(WORLD_ID, WORLD_ID, 1, 0x33, 0x44, Some(world()));
    assert_eq!(n.world_recipient(), RecipientId(LOGON_REC));

    // ACE's PacketOutboundServerSwitch: `(uint)0x18` then `(uint)0`, i.e. its server id lands in
    // the stamp field and the type is the world switch.
    let body = ServerSwitch {
        seq_no: 0x18,
        switch_type: ServerSwitchType::WorldSwitch,
    }
    .to_bytes();
    assert_eq!(body, [0x18, 0, 0, 0, 0, 0, 0, 0], "ACE's eight bytes");

    let dg = s2c(2, PacketFlags::SERVER_SWITCH, body.to_vec(), &mut key);
    n.feed(&dg, Some(logon()), LocalTime(1.0))
        .expect("accepted");
    assert_eq!(
        n.world_recipient(),
        RecipientId(LOGON_REC),
        "the switch names the connection it arrived on, which here is the login one"
    );
    assert!(n.world_switch_history().been_switched_before);
    assert_eq!(n.world_switch_history().last_switch_stamp, 0x18);

    // A stale stamp on the other connection changes nothing.
    let mut k2 = CryptoSystem::new(0x33);
    let stale = {
        let mut p = OutPacket::new(ProtoHeader {
            seq_id: 2,
            rec_id: WORLD_ID,
            iteration: 1,
            ..Default::default()
        });
        p.add_optional_header(
            PacketFlags::SERVER_SWITCH,
            ServerSwitch {
                seq_no: 0x17,
                switch_type: ServerSwitchType::WorldSwitch,
            }
            .to_bytes()
            .to_vec(),
        )
        .expect("section");
        p.serialize(Some(k2.next())).expect("encrypted")
    };
    n.feed(&stale, Some(world()), LocalTime(1.1))
        .expect("accepted");
    assert_eq!(
        n.world_recipient(),
        RecipientId(LOGON_REC),
        "0x17 is older than 0x18, so the switch is refused"
    );
    assert_eq!(n.world_switch_history().last_switch_stamp, 0x18);

    // A newer one moves it.
    let fresh = {
        let mut p = OutPacket::new(ProtoHeader {
            seq_id: 3,
            rec_id: WORLD_ID,
            iteration: 1,
            ..Default::default()
        });
        p.add_optional_header(
            PacketFlags::SERVER_SWITCH,
            ServerSwitch {
                seq_no: 0x19,
                switch_type: ServerSwitchType::WorldSwitch,
            }
            .to_bytes()
            .to_vec(),
        )
        .expect("section");
        p.serialize(Some(k2.next())).expect("encrypted")
    };
    n.feed(&fresh, Some(world()), LocalTime(1.2))
        .expect("accepted");
    assert_eq!(n.world_recipient(), RecipientId(WORLD_ID));
    assert_eq!(
        n.logon_recipient(),
        RecipientId(LOGON_REC),
        "a world switch leaves the logon recipient alone"
    );
}

/// A logon switch moves the current server only outside the world.
#[test]
fn a_logon_switch_moves_the_current_server_only_outside_the_world() {
    for in_game in [false, true] {
        let (mut n, mut key) = net();
        n.add_connection(WORLD_ID, WORLD_ID, 1, 0x33, 0x44, Some(world()));
        if in_game {
            n.enter_world();
        }
        let mut k2 = CryptoSystem::new(0x33);
        let dg = {
            let mut p = OutPacket::new(ProtoHeader {
                seq_id: 2,
                rec_id: WORLD_ID,
                iteration: 1,
                ..Default::default()
            });
            p.add_optional_header(
                PacketFlags::SERVER_SWITCH,
                ServerSwitch {
                    seq_no: 7,
                    switch_type: ServerSwitchType::LogonSwitch,
                }
                .to_bytes()
                .to_vec(),
            )
            .expect("section");
            p.serialize(Some(k2.next())).expect("encrypted")
        };
        let _ = key.next();
        n.feed(&dg, Some(world()), LocalTime(1.0))
            .expect("accepted");

        assert_eq!(n.logon_recipient(), RecipientId(WORLD_ID), "always");
        assert_eq!(
            n.world_recipient(),
            if in_game {
                RecipientId(LOGON_REC)
            } else {
                RecipientId(WORLD_ID)
            },
            "in_game = {in_game}"
        );
        assert!(
            !n.logon_switch_history().last_switch_stamp.eq(&0)
                && n.logon_switch_history().been_switched_before
        );
        assert!(
            !n.world_switch_history().been_switched_before,
            "a logon switch does not touch the world history"
        );
    }
}

/// Exit world disconnect clears the world switch history only.
#[test]
fn exit_world_disconnect_clears_the_world_switch_history_only() {
    let (mut n, mut key) = net();
    let sw = |n: &mut Net, key: &mut CryptoSystem, seq, ty| {
        let dg = s2c(
            seq,
            PacketFlags::SERVER_SWITCH,
            ServerSwitch {
                seq_no: 0x50,
                switch_type: ty,
            }
            .to_bytes()
            .to_vec(),
            key,
        );
        n.feed(&dg, Some(logon()), LocalTime(1.0))
            .expect("accepted");
    };
    sw(&mut n, &mut key, 2, ServerSwitchType::WorldSwitch);
    sw(&mut n, &mut key, 3, ServerSwitchType::LogonSwitch);
    assert!(n.world_switch_history().been_switched_before);
    assert!(n.logon_switch_history().been_switched_before);

    n.exit_world_disconnect();

    assert!(
        !n.world_switch_history().been_switched_before,
        "the world half is reset"
    );
    assert_eq!(
        n.world_switch_history().last_switch_stamp,
        0x50,
        "the stamp itself is not cleared; only the flag is"
    );
    assert!(
        n.logon_switch_history().been_switched_before,
        "the logon half is deliberately left alone"
    );
}

// -------------------------------------------------------------------------------------------
// The neighbours this must not disturb
// -------------------------------------------------------------------------------------------

/// A referral in flight must not change one byte of ordinary traffic: the two-second cumulative
/// ACK still goes out, on the connection's own address, with its own sequencing.
///
/// Both arms feed the same number of encrypted datagrams at the same sequence numbers, so the ACK
/// value and the ISAAC stream are identical by construction and the only difference under test is
/// which section the second one carries.
#[test]
fn a_pending_referral_does_not_disturb_ordinary_traffic() {
    let run = |with_referral: bool| -> Vec<(Vec<u8>, Option<SocketAddr>)> {
        let (mut n, mut key) = net();
        let dg = s2c(
            2,
            PacketFlags::ECHO_RESPONSE,
            vec![0, 0, 0, 0, 0, 0, 0, 0],
            &mut key,
        );
        n.feed(&dg, Some(logon()), LocalTime(0.0))
            .expect("accepted");
        if with_referral {
            feed_referral(&mut n, &mut key, 3, COOKIE, world(), WORLD_ID, 0.0);
        } else {
            let dg = s2c(
                3,
                PacketFlags::ECHO_RESPONSE,
                vec![0, 0, 0, 0, 0, 0, 0, 0],
                &mut key,
            );
            n.feed(&dg, Some(logon()), LocalTime(0.0))
                .expect("accepted");
        }
        let mut out = Vec::new();
        for i in 1..=8 {
            let t = f64::from(i) * 0.5;
            n.process_connections(LocalTime(t));
            n.tick(LocalTime(t), Duration::from_millis(50));
            out.extend(n.take_outgoing().into_iter().filter(|(b, _)| {
                ParsedPacket::parse(b)
                    .is_ok_and(|p| !p.header.header.contains(PacketFlags::WORLD_LOGIN_REQUEST))
            }));
        }
        out
    };
    assert_eq!(
        run(true),
        run(false),
        "the non-referral datagrams must be identical, byte for byte and address for address"
    );
    assert!(
        !run(false).is_empty(),
        "and there must be some, or this proves nothing"
    );
}

/// The disconnect lifecycle is unchanged with a referral in flight.
#[test]
fn the_disconnect_lifecycle_is_unchanged_with_a_referral_in_flight() {
    let (mut n, mut key) = net();
    feed_referral(&mut n, &mut key, 2, COOKIE, world(), WORLD_ID, 0.0);
    let _ = n.take_outgoing();

    let dg = s2c(
        3,
        PacketFlags::NET_ERROR_DISCONNECT,
        dereth_transport::conn::NetErrorCode::ServerClosedConnection
            .pack()
            .to_vec(),
        &mut key,
    );
    n.feed(&dg, Some(logon()), LocalTime(1.0))
        .expect("accepted");
    assert_eq!(
        n.connection_state(RecipientId(LOGON_REC)),
        ConnectionState::DisconnectReceived
    );

    let mut answered = false;
    for i in 1..=6 {
        let t = 1.0 + f64::from(i) * 0.5;
        n.process_connections(LocalTime(t));
        n.tick(LocalTime(t), Duration::from_millis(50));
        for (b, dest) in n.take_outgoing() {
            let p = ParsedPacket::parse(&b).expect("parses");
            if p.header.header.contains(PacketFlags::NET_ERROR_DISCONNECT) {
                answered = true;
                assert_eq!(dest, Some(logon()));
            }
        }
    }
    assert!(answered, "the NetErrorDisconnect response still goes out");
    assert_eq!(
        n.connection_state(RecipientId(LOGON_REC)),
        ConnectionState::Disconnected,
        "and the connection was removed, as has it"
    );
}

/// The addresses the referral path uses are the referred ones and nothing is ever sent to
/// `port + 1`: that rule belongs to the connect ack and the local-interval bump, not here.
#[test]
fn the_referral_never_uses_the_handshake_port() {
    let (mut n, mut key) = net();
    feed_referral(&mut n, &mut key, 2, COOKIE, world(), WORLD_ID, 1.0);
    for (_, dest) in world_logins(&mut n) {
        let d = dest.expect("addressed");
        assert_eq!(d.port(), world().port(), "not port + 1");
        assert_eq!(d.ip(), IpAddr::from([127, 0, 0, 1]));
    }
}
