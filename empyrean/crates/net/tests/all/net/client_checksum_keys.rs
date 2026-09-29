//! Divergence: V267, V282
//! Encrypted client packets verify under the key the retail receive window assigns (next or
//! parked); login and play complete at 5% and 10% loss; a NAK retransmission and a damaged
//! packet's resend decrypt under the parked key.
//! Fixture: locally constructed packets and session state on a virtual clock.

// V267.

use std::time::Duration;

use dereth_primitives::NetQueue;
use dereth_transport::crc::recover_isaac_key;
use dereth_transport::session::SequenceWindow;
use dereth_transport::wire::{OutPacket, PacketFlags, ParsedPacket, ProtoHeader};
use empyrean_net::driver::memory::{Datagram, Link, LinkModel};
use empyrean_net::testing::{ClientStatus, Harness, LoginPolicy};
use empyrean_net::{
    AccountSelect, ClockSnapshot, ClockSnapshotExt, Event, GameMessageGroup, OutboundMessage,
    PortKind, SessionId, SessionTerminationReason,
};

use crate::common::{
    client_addr, connect_response_datagram, find_connect_request, login_request_datagram,
    lossy_harness, server, session_of, TICK,
};
use crate::fragments::message;

/// What happened to the encrypted client packets of one session.
#[derive(Debug, Default)]
struct Tally {
    /// Arrived while their sequence had a parked key (a retransmission, or a packet overtaken).
    parked: u64,
    /// Of those, the ones that carried the retransmission flag.
    parked_retransmissions: u64,
    /// Refused: a copy of a sequence already taken (a duplicate or a second resend).
    duplicates: u64,
    login_secs: f64,
    play_secs: f64,
    naks_to_client: i64,
    /// Keys still parked at the end, for sequences ACE had not yet asked for.
    left_waiting: usize,
    /// `LoginRequest`s the client sent, and the logins the server started from them.
    login_requests_sent: u32,
    logins_started: u32,
}

fn window(h: &Harness, id: SessionId) -> &SequenceWindow {
    &h.net
        .server
        .session(id)
        .expect("live")
        .network
        .connection_data
        .crypto_client
}

/// One harness frame, as `Harness::step`, except that client datagrams cross `up` here and reach
/// the server one at a time, each checked against the window as the server sees it.
fn step(h: &mut Harness, up: &mut Link, id: SessionId, tally: &mut Tally) {
    let now = h.net.now;
    let secs = h.now_seconds();
    for c in &mut h.clients {
        c.tick(secs);
    }
    for _ in 0..64 {
        let mut moved = 0usize;
        for c in &mut h.clients {
            for (to, bytes) in c.take_outgoing() {
                up.send(
                    Datagram {
                        from: c.addr,
                        to,
                        bytes,
                    },
                    now.monotonic,
                );
                moved += 1;
            }
        }
        for d in up.step(now.monotonic) {
            let kind = if d.to == h.net.server_addr(PortKind::C2S) {
                PortKind::C2S
            } else {
                PortKind::S2C
            };
            let p = ParsedPacket::parse(&d.bytes).expect("client packets parse");
            let seq = p.header.seq_id;
            let encrypted = p.header.header.is_encrypted();
            let before = window(h, id);
            let parked = before.seq_ids_we_naked.get(&seq).copied();
            let newer = seq > before.highest_id_received;
            let errors = h.net.server.statistics().c2s_crc_errors_aggregate;
            h.net.server.on_datagram(kind, d.from, &d.bytes, now);
            let refused = h.net.server.statistics().c2s_crc_errors_aggregate > errors;
            if encrypted {
                if let Some(key) = parked {
                    // The sender's key for this sequence is the one the window parked for it.
                    assert_eq!(
                        recover_isaac_key(p.header.checksum, p.header_hash, p.payload_hash),
                        key,
                        "seq {seq}"
                    );
                    assert!(
                        !refused,
                        "seq {seq}: a packet with its parked key was refused"
                    );
                    assert!(
                        !window(h, id).seq_ids_we_naked.contains_key(&seq),
                        "seq {seq}: its key is used up"
                    );
                    tally.parked += 1;
                    if p.header.header.contains(PacketFlags::RETRANSMISSION) {
                        tally.parked_retransmissions += 1;
                    }
                } else if newer {
                    assert!(!refused, "seq {seq}: a new sequence was refused");
                } else {
                    assert!(
                        refused,
                        "seq {seq}: a sequence already taken was accepted again"
                    );
                    tally.duplicates += 1;
                }
            }
            moved += 1;
        }
        h.net.pump();
        for c in &mut h.clients {
            while let Some(d) = h.net.client_recv(c.addr) {
                c.handle_datagram(d.from, &d.bytes, secs);
                moved += 1;
            }
        }
        let events: Vec<Event> = h.net.server.events().collect();
        moved += events.len();
        for e in events {
            match (&e, h.policy) {
                (
                    Event::LoginRequest {
                        session, request, ..
                    },
                    LoginPolicy::Immediate,
                ) => {
                    h.accounts.answer(&mut h.net.server, *session, request, now);
                }
                _ => h.events.push(e),
            }
        }
        if moved == 0 && h.net.links_idle() && up.is_empty() {
            break;
        }
    }
}

/// Steps (with [`step`]) until `until` holds or `d` of virtual time has passed.
fn run(
    h: &mut Harness,
    up: &mut Link,
    id: SessionId,
    d: Duration,
    until: &mut dyn FnMut(&mut Harness) -> bool,
    tally: &mut Tally,
) -> bool {
    let end = h.net.now.monotonic + d;
    loop {
        step(h, up, id, tally);
        if until(h) {
            return true;
        }
        if h.net.now.monotonic >= end {
            return false;
        }
        h.net.advance(TICK);
    }
}

/// How long the stand-in world takes to answer a login: longer than the client's 2 s between
/// `LoginRequest`s, so a copy always reaches the server before the answer (V268/V282).
const SLOW_ANSWER: Duration = Duration::from_millis(2500);

/// Logs in over `login` links, the world answering each login [`SLOW_ANSWER`] late, then
/// exchanges 48 messages each way over `play` links. The copies of the `LoginRequest` (the
/// client's own re-sends and the link's duplicates) must start no second login and abort
/// nothing. Every message must arrive exactly once (client to server in order) within 60 s of
/// virtual time, and afterwards no key is left parked.
fn session(login: (LinkModel, LinkModel), play: (LinkModel, LinkModel)) -> Tally {
    let seed = play.0.seed;
    let mut h = lossy_harness(login.0, login.1);
    h.policy = LoginPolicy::Never;
    let c = h.add_client(client_addr(40, 50_000), "keys", "pw");
    let mut pending: Vec<(Duration, SessionId, empyrean_net::PacketInboundLoginRequest)> =
        Vec::new();
    let mut started: Vec<SessionId> = Vec::new();
    let connected = h.run_until(Duration::from_secs(60), TICK, |h| {
        let now = h.net.now;
        for e in std::mem::take(&mut h.events) {
            match e {
                Event::LoginRequest {
                    session, request, ..
                } => {
                    assert!(
                        !started.contains(&session),
                        "seed {seed}: a second login on one session"
                    );
                    started.push(session);
                    pending.push((now.monotonic + SLOW_ANSWER, session, request));
                }
                e => {
                    assert!(
                        !matches!(
                            e,
                            Event::Disconnected {
                                reason: SessionTerminationReason::BadHandshake,
                                ..
                            }
                        ),
                        "seed {seed}: a copy aborted the login: {e:?}"
                    );
                    h.events.push(e);
                }
            }
        }
        for (_, session, request) in pending.extract_if(.., |(due, ..)| *due <= now.monotonic) {
            h.accounts.answer(&mut h.net.server, session, &request, now);
        }
        h.clients[c].status() == ClientStatus::Connected
            && session_of(h, c).is_some_and(|id| {
                h.net
                    .server
                    .session(id)
                    .is_some_and(|s| s.network.send_resync)
            })
    });
    assert!(connected, "seed {seed}: login over the lossy link");
    let id = session_of(&h, c).expect("session");
    let mut tally = Tally {
        login_secs: h.now_seconds(),
        login_requests_sent: h.clients[c].login_requests_sent,
        logins_started: u32::try_from(started.len()).expect("few"),
        ..Tally::default()
    };
    assert!(
        tally.login_requests_sent >= 2,
        "seed {seed}: the client re-sent its LoginRequest before the answer"
    );
    // Let what the login left in flight land before the client-to-server link is taken over.
    h.net
        .set_link_models(LinkModel::perfect(), LinkModel::perfect());
    h.run_until(Duration::from_secs(2), TICK, |_| false);
    h.net.set_link_models(LinkModel::perfect(), play.1);
    let mut up = Link::new(play.0);
    h.events.clear();
    let start = h.now_seconds();
    let naks_before = h
        .net
        .server
        .statistics()
        .s2c_requests_for_retransmit_aggregate;

    let sizes = [12usize, 300, 1100, 40, 460, 2000, 8, 900];
    let mut c2s = Vec::new();
    let mut s2c = Vec::new();
    #[allow(clippy::cast_possible_truncation)]
    for i in 0..48u32 {
        let msg = message(0xF7B1 ^ (i << 16), sizes[i as usize % sizes.len()]);
        h.clients[c].send(NetQueue::Weenie, &msg);
        c2s.push(msg);
        let down = message(0xF000 + i, sizes[(i as usize + 3) % sizes.len()]);
        h.net.server.send(
            id,
            OutboundMessage {
                group: GameMessageGroup::UIQueue,
                data: down.clone(),
            },
        );
        s2c.push(down);
        run(
            &mut h,
            &mut up,
            id,
            Duration::from_millis(100),
            &mut |_| false,
            &mut tally,
        );
    }
    let mut got_s2c = Vec::new();
    let done = run(
        &mut h,
        &mut up,
        id,
        Duration::from_secs(60),
        &mut |h| {
            while let Some(m) = h.clients[c].poll() {
                let mut data = m.opcode.to_le_bytes().to_vec();
                data.extend_from_slice(&m.body);
                got_s2c.push(data);
            }
            let up = h
                .events
                .iter()
                .filter(|e| matches!(e, Event::Message { .. }))
                .count();
            up >= c2s.len() && got_s2c.len() >= s2c.len()
        },
        &mut tally,
    );
    let got_c2s: Vec<&Vec<u8>> = h
        .events
        .iter()
        .filter_map(|e| match e {
            Event::Message { message, .. } => Some(&message.data),
            _ => None,
        })
        .collect();
    assert!(
        done,
        "seed {seed}: stalled with {} of {} up, {} of {} down; up link {:?}",
        got_c2s.len(),
        c2s.len(),
        got_s2c.len(),
        s2c.len(),
        up.stats
    );
    assert_eq!(
        got_c2s,
        c2s.iter().collect::<Vec<_>>(),
        "seed {seed}: client to server, once each, in order"
    );
    got_s2c.sort();
    s2c.sort();
    assert_eq!(got_s2c, s2c, "seed {seed}: server to client, once each");
    assert!(
        up.stats.dropped > 0 && up.stats.reordered > 0 && up.stats.duplicated > 0,
        "seed {seed}: {:?}",
        up.stats
    );
    tally.play_secs = h.now_seconds() - start;
    tally.naks_to_client = h
        .net
        .server
        .statistics()
        .s2c_requests_for_retransmit_aggregate
        - naks_before;

    // Nothing stuck: once the links are quiet, a key is parked only for a sequence ACE's receive
    // order is itself still waiting for. (ACE asks for a gap only when a packet two or more past
    // it arrives, so the client's last lost packet can wait for later traffic; that is ACE's.)
    run(
        &mut h,
        &mut up,
        id,
        Duration::from_secs(5),
        &mut |_| false,
        &mut tally,
    );
    let w = window(&h, id);
    let last = h
        .net
        .server
        .session(id)
        .expect("live")
        .network
        .last_received_packet_sequence();
    assert!(w.highest_id_received >= last, "seed {seed}");
    assert!(
        w.seq_ids_we_naked.keys().all(|&s| s > last),
        "seed {seed}: keys parked for {:?}, below ACE's {last}",
        w.seq_ids_we_naked.keys()
    );
    tally.left_waiting = w.seq_ids_we_naked.len();
    tally
}

fn sweep(p: f64, seeds: std::ops::RangeInclusive<u64>) {
    let mut total = Tally::default();
    let mut worst = 0.0f64;
    let n = seeds.clone().count();
    for seed in seeds {
        let login = (
            LinkModel::lossy(p, seed ^ 0x55),
            LinkModel::lossy(p, seed ^ 0xAA),
        );
        let play = (
            LinkModel::lossy(p, seed),
            LinkModel::lossy(p, seed.wrapping_mul(0x9E37)),
        );
        let t = session(login, play);
        total.login_secs += t.login_secs;
        total.play_secs += t.play_secs;
        worst = worst.max(t.play_secs);
        total.parked += t.parked;
        total.parked_retransmissions += t.parked_retransmissions;
        total.duplicates += t.duplicates;
        total.naks_to_client += t.naks_to_client;
        total.left_waiting += t.left_waiting;
        total.login_requests_sent += t.login_requests_sent;
        total.logins_started += t.logins_started;
    }
    assert!(
        total.parked_retransmissions > 0,
        "p={p}: some retransmissions were exercised"
    );
    #[allow(clippy::cast_precision_loss)]
    let n = n as f64;
    eprintln!(
        "p={p}: {n} sessions; mean login {:.2} s; mean play {:.2} s (worst {worst:.2} s) for 4.8 s of sending; \
         {} LoginRequests sent for {} logins started; {} packets taken under parked keys ({} of them resends); {} copies refused; {} NAKs to the client; {} sequences still awaited at the end",
        total.login_secs / n,
        total.play_secs / n,
        total.login_requests_sent,
        total.logins_started,
        total.parked,
        total.parked_retransmissions,
        total.duplicates,
        total.naks_to_client,
        total.left_waiting,
    );
}

/// 5% loss, reordering and duplication each way, the login included.
#[test]
fn login_and_play_complete_at_5_percent_loss() {
    sweep(0.05, 1..=16);
}

/// 10% loss, reordering and duplication each way, the login included.
#[test]
fn login_and_play_complete_at_10_percent_loss() {
    sweep(0.10, 101..=116);
}

/// A session driven by hand: the server, the session, the client's address, its key stream, and
/// the time.
pub(crate) fn by_hand() -> (
    empyrean_net::ServerNet,
    SessionId,
    std::net::SocketAddr,
    dereth_transport::CryptoSystem,
    ClockSnapshot,
) {
    let mut net = server(Default::default());
    let from = client_addr(41, 40_000);
    let now = ClockSnapshot::at_seconds(1.0);
    net.on_datagram(
        PortKind::C2S,
        from,
        &login_request_datagram("nak", "pw", "1802"),
        now,
    );
    let Some(Event::LoginRequest { session, .. }) = net.events().next() else {
        panic!("login request")
    };
    assert_eq!(
        net.account_select_callback(session, now),
        AccountSelect::Continue
    );
    net.accept_login(session, 1, "nak".into(), 1);
    let out: Vec<_> = net.poll(now).collect();
    let (_, cr, _) = find_connect_request(&out).expect("ConnectRequest");
    net.on_datagram(
        PortKind::S2C,
        from,
        &connect_response_datagram(cr.cookie, session.client_id),
        now,
    );
    let _ = net.events().count();
    let _ = net.poll(now).count();
    (
        net,
        session,
        from,
        dereth_transport::CryptoSystem::new(cr.seeds().client_to_server),
        now,
    )
}

/// A client packet: sequence `seq`, one single-fragment message whose fragment sequence is `frag`,
/// under `key`.
fn packet(session: SessionId, seq: u32, frag: u32, key: u32, retransmission: bool) -> Vec<u8> {
    let mut p = OutPacket::new(ProtoHeader {
        seq_id: seq,
        rec_id: session.client_id,
        iteration: 1,
        ..ProtoHeader::default()
    });
    if retransmission {
        p.header.header = PacketFlags(p.header.header.0 | PacketFlags::RETRANSMISSION);
    }
    p.add_fragment(dereth_transport::Fragment::new(
        dereth_transport::FragmentHeader {
            blob_id_low: frag,
            blob_id_high: 0x8000_0000,
            num_frags: 1,
            blob_num: 0,
            queue_id: 5,
            blob_frag_size: 0,
        },
        message(0xF7B1 ^ (frag << 16), 16),
    ))
    .expect("fragment");
    p.serialize(Some(key)).expect("encrypted")
}

/// Sequence 3 is lost: 4 and 5 arrive, the server parks 3's key and NAKs 3, and the resend of 3
/// under the parked key is accepted and fills the gap in order. A duplicate of the resend is
/// refused without drawing a key, and 6 still decrypts under the next key.
///
/// Where ACE and retail part: a packet carrying the right stream's key for another sequence. ACE
/// found any key up to 256 draws ahead; the retail window binds each key to its sequence, so
/// sequence 3 carrying 5's key is refused, and 3's parked key survives the attempt.
#[test]
fn a_nak_and_its_retransmission_decrypt_under_the_parked_key() {
    let (mut net, session, from, mut stream, mut now) = by_hand();
    let keys: Vec<u32> = (0..6).map(|_| stream.next()).collect(); // sequences 2..=7
    let key = |seq: u32| keys[seq as usize - 2];
    let window = |net: &empyrean_net::ServerNet| {
        net.session(session)
            .expect("live")
            .network
            .connection_data
            .crypto_client
            .clone()
    };
    let messages = |net: &mut empyrean_net::ServerNet| {
        net.events()
            .filter(|e| matches!(e, Event::Message { .. }))
            .count()
    };
    let errors = |net: &empyrean_net::ServerNet| net.statistics().c2s_crc_errors_aggregate;

    net.on_datagram(
        PortKind::C2S,
        from,
        &packet(session, 2, 1, key(2), false),
        now,
    );
    assert_eq!(messages(&mut net), 1);
    // 3 lost; 4 and 5 arrive.
    now = now.advanced(Duration::from_secs(2));
    net.on_datagram(
        PortKind::C2S,
        from,
        &packet(session, 4, 3, key(4), false),
        now,
    );
    net.on_datagram(
        PortKind::C2S,
        from,
        &packet(session, 5, 4, key(5), false),
        now,
    );
    assert_eq!(messages(&mut net), 0, "held until the gap is filled");
    assert_eq!(
        window(&net).seq_ids_we_naked.get(&3),
        Some(&key(3)),
        "3's key was drawn and parked"
    );
    let naks: Vec<Vec<u32>> = net
        .poll(now)
        .filter_map(|o| {
            let p = ParsedPacket::parse(&o.bytes).ok()?;
            p.optional
                .get(&PacketFlags::REQUEST_RETRANSMIT)
                .map(|b| dereth_transport::wire::optional::seq_ids(b))
        })
        .collect();
    assert_eq!(naks, vec![vec![3]], "the server asks for 3");

    // Sequence 3 with another sequence's key (5's, from the right stream) is refused.
    let e = errors(&net);
    net.on_datagram(
        PortKind::C2S,
        from,
        &packet(session, 3, 2, key(5), true),
        now,
    );
    assert_eq!(errors(&net), e + 1);
    assert_eq!(messages(&mut net), 0);
    assert_eq!(
        window(&net).seq_ids_we_naked.get(&3),
        Some(&key(3)),
        "still parked"
    );

    // The resend, under 3's own key.
    net.on_datagram(
        PortKind::C2S,
        from,
        &packet(session, 3, 2, key(3), true),
        now,
    );
    assert_eq!(errors(&net), e + 1, "accepted");
    assert_eq!(messages(&mut net), 3, "3, then the held 4 and 5");
    assert!(window(&net).seq_ids_we_naked.is_empty());

    // A duplicate of the resend: refused, and no key drawn.
    net.on_datagram(
        PortKind::C2S,
        from,
        &packet(session, 3, 2, key(3), true),
        now,
    );
    assert_eq!(errors(&net), e + 2);
    net.on_datagram(
        PortKind::C2S,
        from,
        &packet(session, 6, 5, key(6), false),
        now,
    );
    assert_eq!(
        errors(&net),
        e + 2,
        "6 decrypts under the next key: the stream kept its place"
    );
    assert_eq!(messages(&mut net), 1);
    assert_eq!(
        net.session(session)
            .expect("live")
            .network
            .last_received_packet_sequence(),
        6
    );
}

/// A packet that arrives damaged is treated as lost: its key is parked again, and the good copy
/// the client resends is accepted under it.
#[test]
fn a_damaged_packet_keeps_its_key_for_the_resend() {
    let (mut net, session, from, mut stream, now) = by_hand();
    let k2 = stream.next();
    let k3 = stream.next();
    let mut bad = packet(session, 2, 1, k2, false);
    let last = bad.len() - 1;
    bad[last] ^= 0x5A;
    net.on_datagram(PortKind::C2S, from, &bad, now);
    assert_eq!(net.statistics().c2s_crc_errors_aggregate, 1);
    let w = net
        .session(session)
        .expect("live")
        .network
        .connection_data
        .crypto_client
        .clone();
    assert_eq!(
        w.seq_ids_we_naked.get(&2),
        Some(&k2),
        "2's key parked again"
    );
    net.on_datagram(PortKind::C2S, from, &packet(session, 3, 2, k3, false), now);
    net.on_datagram(PortKind::C2S, from, &packet(session, 2, 1, k2, true), now);
    assert_eq!(net.statistics().c2s_crc_errors_aggregate, 1);
    assert_eq!(
        net.events()
            .filter(|e| matches!(e, Event::Message { .. }))
            .count(),
        2
    );
}
