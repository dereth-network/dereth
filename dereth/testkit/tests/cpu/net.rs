//! The link: the handshake, and what happens when the shard stops answering.
//!
//! **The handshake scenarios drive a `ClientNetwork` directly and book the claim at the end**, as
//! `frame.rs` and `combat.rs` do: `HeadlessClient` takes a whole recorded login or nothing, and the
//! subject here is the datagrams *before* a login, at wall-clock times a scenario has to choose.
//! The silence scenario replays the two **unclean-logout** recordings, `unclean-logout-short` and
//! `unclean-logout-long`, named rather than globbed; a name that does not load is a failure, not a
//! shortfall.
//!
//! No socket is bound. `ClientNetwork::new` is the socket-free half of the client's endpoint; every
//! datagram below is either built in this process and handed to it, or read out of a committed
//! recording.

use std::net::SocketAddr;

use dereth_client_net::client_session::testing::capture;
use dereth_client_net::client_session::{DisconnectReason, SessionState};
use dereth_client_net::RecipientId;
use dereth_primitives::LocalTime;
use dereth_testkit::{HeadlessClient, Inbound};
use dereth_transport::conn::{ConnectRequest, ConnectionState, NetErrorCode};
use dereth_transport::wire::{OutPacket, PacketFlags, ParsedPacket, ProtoHeader};
use dereth_transport::CryptoSystem;
use {dereth_client_runtime::net::ClientNetwork, dereth_client_runtime::net::LinkStatus};

/// The id the shard gives this client. Non-zero: zero is the client's own "this slot is free"
/// sentinel, so a scenario that used it would be asserting against the empty case.
const RECIPIENT: u16 = 0x0B;
const PEER: &str = "127.0.0.1:19000";
/// The same address one port up, which is where the shard says to answer.
const NEXT_PORT: &str = "127.0.0.1:19001";
const OUTGOING_SEED: u32 = 0x6A98_D361;
const INCOMING_SEED: u32 = 0x1C08_CCF5;

/// The cookie the shard hands out during the handshake.
const COOKIE: u64 = 0xCB85_1E1F_0244_2EED;

/// A stand-in account, not a real one. Nothing here compares it against anything -- it is the
/// credential the login request carries -- so it is written as a stand-in for the same reason the
/// corpus now carries stand-ins.
const ACCOUNT: &str = "acct01";

/// The give-up interval is measured in wall clock, so a scenario asks for it by moving the clock
/// well past it rather than by naming it.
const LONG_SILENCE: LocalTime = LocalTime(200.0);

fn peer() -> SocketAddr {
    PEER.parse().expect("peer address")
}

fn next_port() -> SocketAddr {
    NEXT_PORT.parse().expect("the shard's real port")
}

/// The shard's answer to a login: "talk to me on the next port, and here is a cookie".
fn connect_request(cookie: u64) -> Vec<u8> {
    let mut p = OutPacket::new(ProtoHeader {
        seq_id: 0,
        rec_id: RECIPIENT,
        interval: 0x0440,
        iteration: 1,
        ..ProtoHeader::default()
    });
    p.add_optional_header(
        PacketFlags::CONNECT_REQUEST,
        ConnectRequest {
            server_time: 1234.5,
            cookie,
            net_id: 0,
            outgoing_seed: OUTGOING_SEED,
            incoming_seed: INCOMING_SEED,
        }
        .to_bytes()
        .to_vec(),
    )
    .expect("ConnectRequest section");
    p.serialize(None).expect("a connect request is disposable")
}

/// The first datagram that is **not** part of the handshake, which is what moves the link from
/// connecting to connected. Sequenced and encrypted, because only the handshake is disposable.
fn first_ordinary_packet(key: &mut CryptoSystem) -> Vec<u8> {
    let mut p = OutPacket::new(ProtoHeader {
        seq_id: 2,
        rec_id: RECIPIENT,
        interval: 0x0441,
        iteration: 1,
        ..ProtoHeader::default()
    });
    p.add_optional_header(PacketFlags::ECHO_REQUEST, 0f32.to_le_bytes().to_vec())
        .expect("EchoRequest section");
    p.serialize(Some(key.next())).expect("encrypted")
}

/// A link driven all the way to connected, over a handshake carrying `cookie`.
fn connected(cookie: u64) -> ClientNetwork {
    let mut c =
        ClientNetwork::new(PEER, 7304, ACCOUNT, ACCOUNT, 0).expect("a socket-free endpoint");
    c.tick(LocalTime(0.0));
    let _ = c.take_outgoing(); // the login request

    c.feed(&connect_request(cookie), peer(), LocalTime(0.1));
    let _ = c.take_outgoing(); // the answer, on the next port

    let mut key = CryptoSystem::new(OUTGOING_SEED);
    c.feed(&first_ordinary_packet(&mut key), peer(), LocalTime(0.2));
    let _ = c.take_outgoing();
    assert_eq!(
        c.status(),
        LinkStatus::Connected,
        "the link reached connected"
    );
    c
}

/// The logins this client has tried to put on the wire since the last drain.
fn world_logins(c: &mut ClientNetwork) -> Vec<(Vec<u8>, SocketAddr)> {
    c.take_outgoing()
        .into_iter()
        .filter(|(b, _)| {
            ParsedPacket::parse(b)
                .is_ok_and(|p| p.header.header.contains(PacketFlags::WORLD_LOGIN_REQUEST))
        })
        .collect()
}

// -------------------------------------------------------------------------------------------
// link.handshake.answers-the-shard-on-the-next-port-and-echoes-its-cookie
// -------------------------------------------------------------------------------------------

/// The shard names a port and a cookie; the client answers there, with that.
pub fn the_handshake_answers_on_the_next_port() {
    let mut c =
        ClientNetwork::new(PEER, 7304, ACCOUNT, ACCOUNT, 0).expect("a socket-free endpoint");
    c.tick(LocalTime(0.0));
    let _ = c.take_outgoing();

    c.feed(&connect_request(COOKIE), peer(), LocalTime(0.1));
    let connecting =
        c.status() == LinkStatus::LoginConnecting && c.handshake_addr() == Some(next_port());

    let out = c.take_outgoing();
    let one_answer = out.len() == 1 && out[0].1 == next_port();
    let echoed = ParsedPacket::parse(&out[0].0).is_ok_and(|ack| {
        ack.header.header.contains(PacketFlags::CONNECT_RESPONSE)
            && ack
                .optional
                .get(&PacketFlags::CONNECT_RESPONSE)
                .map(Vec::as_slice)
                == Some(&COOKIE.to_le_bytes()[..])
    });

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "link.handshake.answers-the-shard-on-the-next-port-and-echoes-its-cookie",
        move |_| connecting && one_answer && echoed,
    );
}

dereth_testkit::scenarios! {
    scenario_the_handshake_answers_on_the_next_port => the_handshake_answers_on_the_next_port ["link.handshake.answers-the-shard-on-the-next-port-and-echoes-its-cookie"],
    scenario_the_cookie_outlives_the_handshake => the_cookie_outlives_the_handshake ["link.referral-cookie.outlives-the-handshake-that-carried-it"],
    scenario_a_silent_shard_is_logged_into_again => a_silent_shard_is_logged_into_again ["link.silent-shard.logs-in-again-once-with-the-cookie-it-was-given"],
    scenario_a_silent_shard_with_no_cookie_just_drops => a_silent_shard_with_no_cookie_just_drops ["link.silent-shard.with-no-cookie-is-an-ordinary-timeout"],
    scenario_a_recorded_silence_ends_the_link => a_recorded_silence_ends_the_link ["link.silent-shard.a-recorded-silence-ends-the-link-and-says-the-shard-died"],
    scenario_a_link_reading_is_taken_every_two_seconds => a_link_reading_is_taken_every_two_seconds ["link.reading.is-taken-every-two-seconds-and-the-counters-start-again-after-it"],
    scenario_a_refusal_after_the_login_is_over_is_a_lost_shard => a_refusal_after_the_login_is_over_is_a_lost_shard ["link.dropped.a-refusal-after-the-login-window-closes-is-a-lost-shard-and-not-a-login-failure"],
    scenario_a_named_reason_survives_the_goodbye_that_follows_it => a_named_reason_survives_the_goodbye_that_follows_it ["link.dropped.a-shard-that-says-why-keeps-that-reason-while-the-goodbye-completes"],
    scenario_a_goodbye_with_no_complaint_is_answered_once_and_then_the_link_goes_down => a_goodbye_with_no_complaint_is_answered_once_and_then_the_link_goes_down ["link.dropped.a-goodbye-is-answered-once-and-the-link-is-taken-down-on-the-next-pass"],
    scenario_a_tampered_datagram_changes_nothing_and_another_party_leaving_is_not_the_shard => a_tampered_datagram_changes_nothing_and_another_party_leaving_is_not_the_shard ["link.dropped.a-tampered-datagram-changes-nothing-and-another-recipient-leaving-is-not-the-shard"],
    scenario_only_real_silence_takes_the_link_down => only_real_silence_takes_the_link_down ["link.dropped.only-real-silence-takes-the-link-down-and-a-stalled-client-is-not-silence"],
    scenario_the_packet_loss_figure_counts_both_directions => the_packet_loss_figure_counts_both_directions ["link.packet-loss.counts-what-the-client-sent-as-well-as-what-it-received"],
    scenario_a_burst_of_loss_ages_out_of_the_window => a_burst_of_loss_ages_out_of_the_window ["link.packet-loss.a-burst-ages-out-of-a-window-far-longer-than-the-line-says"],
}

// -------------------------------------------------------------------------------------------
// link.referral-cookie.outlives-the-handshake-that-carried-it
// -------------------------------------------------------------------------------------------

/// The cookie is kept with the connection, not with the handshake that carried it.
///
/// **This is the order that matters**, and it is why the two halves are separate booleans: the
/// client's own handshake copy is cleared the moment the link reaches connected, so a build that
/// kept the cookie only there has forgotten it exactly when it would be needed.
pub fn the_cookie_outlives_the_handshake() {
    let mut c =
        ClientNetwork::new(PEER, 7304, ACCOUNT, ACCOUNT, 0).expect("a socket-free endpoint");
    let nothing_registered_yet = c.session.transport.referral_cookie(RecipientId(RECIPIENT)) == 0;

    c.tick(LocalTime(0.0));
    let _ = c.take_outgoing();
    c.feed(&connect_request(COOKIE), peer(), LocalTime(0.1));
    let written = c.session.transport.referral_cookie(RecipientId(RECIPIENT)) == COOKIE;

    let _ = c.take_outgoing();
    let mut key = CryptoSystem::new(OUTGOING_SEED);
    c.feed(&first_ordinary_packet(&mut key), peer(), LocalTime(0.2));
    let survives = c.status() == LinkStatus::Connected
        && c.session.transport.referral_cookie(RecipientId(RECIPIENT)) == COOKIE;

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "link.referral-cookie.outlives-the-handshake-that-carried-it",
        move |_| nothing_registered_yet && written && survives,
    );
}

// -------------------------------------------------------------------------------------------
// link.silent-shard.logs-in-again-once-with-the-cookie-it-was-given
// -------------------------------------------------------------------------------------------

/// A shard that stops answering is dropped, the player is told, and one fresh login is built.
///
/// **And that login is not transmitted, which is the client's own fold and not an omission
/// here.** Removing a connection that is the client's only shard logs the whole endpoint off, and
/// nothing is sent afterwards. The datagram reaches the wire only for a third connection -- a
/// shard this client was referred to and has not been switched to -- which a single-shard scenario
/// cannot have. Asserted rather than smoothed over.
pub fn a_silent_shard_is_logged_into_again() {
    let mut c = connected(COOKIE);
    c.tick(LONG_SILENCE);

    let queue = c.session.transport.referral_queue();
    let one_login = queue.len() == 1
        && queue[0].cookie == COOKIE
        && queue[0].id_server == RECIPIENT
        && queue[0].server_addr == peer()
        && queue[0].n_auths_sent == 1;

    let dropped = c.session.transport.connection_state(RecipientId(RECIPIENT))
        == ConnectionState::Disconnected
        && c.session.transport.log_off_sent();
    let nothing_sent = world_logins(&mut c).is_empty();

    // A removal that logs the endpoint off silently would leave a live screen in front of a dead
    // link, so the player has to hear about it.
    let told = c.status() == LinkStatus::Disconnected
        && c.session_state()
            == dereth_client_net::client_session::SessionState::Disconnected(
                dereth_client_net::client_session::DisconnectReason::ServerDied,
            );

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "link.silent-shard.logs-in-again-once-with-the-cookie-it-was-given",
        move |_| one_login && dropped && nothing_sent && told,
    );
}

// -------------------------------------------------------------------------------------------
// link.silent-shard.with-no-cookie-is-an-ordinary-timeout
// -------------------------------------------------------------------------------------------

/// **The known-negative.** The identical run with no cookie must queue nothing: a scenario that
/// could not report "no second login" would prove nothing about the one above.
pub fn a_silent_shard_with_no_cookie_just_drops() {
    let mut c = connected(0);
    let no_cookie = c.session.transport.referral_cookie(RecipientId(RECIPIENT)) == 0;

    c.tick(LONG_SILENCE);

    let nothing_queued = c.session.transport.referral_queue().is_empty();
    let ordinary = c.session.transport.connection_state(RecipientId(RECIPIENT))
        == ConnectionState::DisconnectReceived
        && !c.session.transport.log_off_sent();
    let nothing_sent = world_logins(&mut c).is_empty();

    let mut client = HeadlessClient::model();
    client.assert_behaviour(
        "link.silent-shard.with-no-cookie-is-an-ordinary-timeout",
        move |_| no_cookie && nothing_queued && ordinary && nothing_sent,
    );
}

// -------------------------------------------------------------------------------------------
// link.silent-shard.a-recorded-silence-ends-the-link-and-says-the-shard-died
// -------------------------------------------------------------------------------------------

/// The shard has this long to say something before the client gives up on it. It is the client's
/// own interval, named here only so the scenario can tell a silence that should end a link from
/// one that should not; nothing below asserts the number.
const CONNECTION_TIMEOUT: f64 = 140.0;

/// Every unclean-logout recording, by slug, in name order.
///
/// These are the recordings in the corpus that deliberately end without a clean `Disconnect`:
/// the shard stopped talking and the client was left to notice. The capture index lists them
/// (`without_disconnect`); every other recording under `fixtures/packet-captures/` is a session
/// that was still being talked to when it stopped.
fn unclean_logout_recordings() -> Vec<(String, Vec<capture::Datagram>)> {
    let slugs = capture::without_disconnect();
    assert!(
        !slugs.is_empty(),
        "the capture index lists no recording without a Disconnect; this scenario's oracle is gone"
    );
    slugs
        .iter()
        .map(|slug| {
            let recs = capture::load_session(slug).unwrap_or_else(|e| {
                panic!(
                    "the recording {slug} holds this scenario's oracle and is committed to the \
                     repository; it did not load: {e}"
                )
            });
            ((*slug).to_owned(), recs)
        })
        .collect()
}

/// One segment per handshake the shard answered, starting at the login request that earned it.
///
/// A recording that holds no login request holds no session to replay, and this panics rather
/// than skip it.
fn logins(records: &[capture::Datagram]) -> Vec<std::ops::Range<usize>> {
    let is_login = |r: &capture::Datagram| {
        r.c2s
            && ParsedPacket::parse(&r.raw)
                .is_ok_and(|p| p.optional.contains_key(&PacketFlags::LOGIN_REQUEST))
    };
    let is_answer = |r: &capture::Datagram| {
        !r.c2s
            && ParsedPacket::parse(&r.raw)
                .is_ok_and(|p| p.optional.contains_key(&PacketFlags::CONNECT_REQUEST))
    };
    let mut starts: Vec<usize> = Vec::new();
    for (i, _) in records.iter().enumerate().filter(|(_, r)| is_answer(r)) {
        let start = (0..i)
            .rev()
            .find(|&j| is_login(&records[j]))
            .unwrap_or_else(|| {
                panic!("a handshake answer at datagram {i} with no login request before it")
            });
        if starts.last() != Some(&start) {
            starts.push(start);
        }
    }
    assert!(!starts.is_empty(), "a recording with no handshake in it");
    (0..starts.len())
        .map(|n| starts[n]..starts.get(n + 1).copied().unwrap_or(records.len()))
        .collect()
}

/// The longest stretch during which the shard said nothing, measured the way the client's own
/// connection sweep measures it: from the last datagram the shard sent to every later point in the
/// recording, the tail included.
fn longest_server_silence(segment: &[capture::Datagram]) -> f64 {
    let mut last: Option<f64> = None;
    let mut worst = 0.0_f64;
    for r in segment {
        if let Some(l) = last {
            worst = worst.max(r.t - l);
        }
        if !r.c2s {
            last = Some(r.t);
        }
    }
    worst
}

/// **The recorded silence really does end the link, and the player is told why.**
///
/// The locked corpus cannot assert this: every recording in it is a session that was still being
/// talked to when the capture stopped, so the arm that fires on a silence is never taken there.
/// These recordings were made on purpose to demonstrate it.
///
/// What drives the client is `Inbound::from_raw_capture`'s sibling `Inbound::datagrams` --
/// **datagrams and not blobs**, which is the whole point here: the subject is a silence the
/// connection sweep measures, so reassembly, ordering and the sweep have to be the client's own.
/// The step takes the datagrams this scenario already loaded rather than a session name, so each
/// login segment of a recording is fed on its own.
#[allow(clippy::too_many_lines)]
pub fn a_recorded_silence_ends_the_link() {
    let mut with_silence = 0usize;
    let mut without_silence = 0usize;
    let mut every_silence_ended_the_link = true;
    let mut every_silence_said_the_shard_died = true;
    let mut every_quiet_link_lived = true;
    let mut nothing_refused = true;

    for (name, records) in unclean_logout_recordings() {
        let segments = logins(&records);
        let carries_a_disconnect = records.iter().any(|r| {
            ParsedPacket::parse(&r.raw)
                .is_ok_and(|p| p.header.header.contains(PacketFlags::DISCONNECT))
        });
        // The connection count and the presence of a disconnect are *recorded*, never
        // assumed. They are derived here from the datagrams and printed; nothing pins them.
        println!(
            "unclean logout: {name} -- {} connection(s), carries a disconnect: {}",
            segments.len(),
            carries_a_disconnect
        );

        let shared = std::sync::Arc::new(records.clone());
        for (n, range) in segments.into_iter().enumerate() {
            let segment = &records[range.clone()];
            let silence = longest_server_silence(segment);
            let sequence =
                dereth_headless::capture::connection_sequence_number(segment).unwrap_or(0);

            // A model client with the recording's own endpoint under it, fed the segment's
            // datagrams at their recorded times -- which is what makes a 140-second silence a
            // 140-second silence rather than 140 frames.
            let mut c = HeadlessClient::model();
            c.attach_replay(dereth_testkit::replay::endpoint(
                &capture::peer(0).to_string(),
                sequence,
            ));
            c.when(Inbound::datagrams(std::sync::Arc::clone(&shared), range));

            let net = c
                .replay_net_mut()
                .expect("the endpoint this segment was fed through");
            let status = net.status();
            let state = net.session_state();
            let rejected = net.rejected();
            let server_died = state == SessionState::Disconnected(DisconnectReason::ServerDied);
            // The link reached `Connected` at some point: a segment starts at its own login
            // request, so a session that never connected would have nothing to go quiet.
            let was_connected = status == LinkStatus::Connected || server_died;
            let events = c.view().objects().world.tables.weenies.len();

            println!(
                "unclean logout: {name} #{n} -- {events} object(s) in the world, silence \
                 {silence:.1} s, refused {rejected}, ended {status:?} / {state:?}"
            );
            // Whatever the link's fate, a recording is not a source of bad packets: the transport
            // refuses nothing here, and a refusal would mean the replay had lost the stream.
            nothing_refused &= rejected == 0;
            assert!(was_connected, "{name} #{n} never reached a connected link");
            if silence > CONNECTION_TIMEOUT {
                with_silence += 1;
                every_silence_ended_the_link &= status == LinkStatus::Disconnected;
                every_silence_said_the_shard_died &= server_died;
            } else {
                without_silence += 1;
                every_quiet_link_lived &= status == LinkStatus::Connected && !server_died;
            }
        }
    }

    println!(
        "{with_silence} recorded login(s) ended on a silence the recording itself carries, \
         {without_silence} did not"
    );

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "link.silent-shard.a-recorded-silence-ends-the-link-and-says-the-shard-died",
        move |_| {
            with_silence > 0
                && every_silence_ended_the_link
                && every_silence_said_the_shard_died
                && every_quiet_link_lived
                && nothing_refused
        },
    );
}

// =============================================================================================
// The link itself: the goodbye, the refusals, the silence and the packet-loss figure
//
// The link lamp is a thing on the screen, but these claims are about the link, so they are filed
// under `net`. **They are `cpu` scenarios**: neither of the two drives below opens a retail data
// file -- one is the client's own socket-free endpoint, the other is the transport underneath it.
// The claims that really do need the shipped data -- the panel's own text, and the running client
// that carries the figure to it -- are in the `dat` tier's `tests/dat/net.rs`.
//
// No socket is bound. Every datagram is built in this process and handed to `feed`, which is the
// same call the client's socket loop makes.
// =============================================================================================

/// A shard that can build the three datagrams the handshake and the goodbye are made of, for a
/// socket-free [`ClientNetwork`]. The crypto is the client's own, keyed by the seed the opening
/// answer named, so an encrypted datagram this builds is one the client will accept.
struct LinkShard {
    crypto: CryptoSystem,
    sequence: u32,
    from: SocketAddr,
}

/// The seed the opening answer hands the client for the traffic coming back to it.
const LINK_OUTGOING_SEED: u32 = 0xDEAD_BEEF;
/// The seed for the other direction. Nothing below reads it; it is part of the answer.
const LINK_INCOMING_SEED: u32 = 0x1234_5678;

impl LinkShard {
    fn new() -> (ClientNetwork, Self) {
        let net =
            ClientNetwork::new(PEER, 7304, "link", "unused", 0).expect("a socket-free endpoint");
        (
            net,
            Self {
                crypto: CryptoSystem::new(LINK_OUTGOING_SEED),
                sequence: 1,
                from: peer(),
            },
        )
    }

    /// The shard's opening answer, in the clear, as it always is.
    fn hello(&self) -> Vec<u8> {
        let mut packet = OutPacket::new(ProtoHeader {
            seq_id: 0,
            rec_id: RECIPIENT,
            interval: 0x100,
            iteration: 1,
            ..ProtoHeader::default()
        });
        packet
            .add_optional_header(
                PacketFlags::CONNECT_REQUEST,
                ConnectRequest {
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

    /// An ordinary encrypted datagram -- the first of which is what finishes the handshake.
    fn ordinary(&mut self) -> Vec<u8> {
        self.sequence = self.sequence.wrapping_add(1);
        let mut packet = OutPacket::new(ProtoHeader {
            seq_id: self.sequence,
            rec_id: RECIPIENT,
            interval: 0x100,
            iteration: 1,
            ..ProtoHeader::default()
        });
        packet
            .add_optional_header(PacketFlags::ECHO_REQUEST, 0f32.to_le_bytes().to_vec())
            .expect("an ordinary header");
        packet
            .serialize(Some(self.crypto.next()))
            .expect("an encrypted packet")
    }

    /// The shard's goodbye, carrying either a reason or the no-complaint sentinel.
    fn goodbye(&mut self, code: NetErrorCode) -> Vec<u8> {
        self.sequence = self.sequence.wrapping_add(1);
        let mut packet = OutPacket::new(ProtoHeader {
            seq_id: self.sequence,
            rec_id: RECIPIENT,
            interval: 0x100,
            iteration: 1,
            ..ProtoHeader::default()
        });
        packet
            .add_optional_header(PacketFlags::NET_ERROR_DISCONNECT, code.pack().to_vec())
            .expect("the goodbye header");
        packet
            .serialize(Some(self.crypto.next()))
            .expect("an encrypted packet")
    }
}

/// The handshake, run: the opening answer and then one ordinary datagram.
fn a_connected_link() -> (ClientNetwork, LinkShard) {
    let (mut net, mut shard) = LinkShard::new();
    net.feed(&shard.hello(), shard.from, LocalTime(10.0));
    net.take_outgoing();
    let ordinary = shard.ordinary();
    net.feed(&ordinary, shard.from, LocalTime(10.1));
    assert_eq!(
        net.status(),
        LinkStatus::Connected,
        "the fixture's own handshake finished"
    );
    assert!(net.take_connected(), "and it reported the edge once");
    (net, shard)
}

/// The no-complaint sentinel a graceful goodbye carries, and the body the client sends back.
const NO_COMPLAINT: [u8; 8] = [0x95, 0x67, 0xD0, 0x02, 0x08, 0, 0, 0];

// ---------------------------------------------------------------------------------------------
// link.reading.is-taken-every-two-seconds-and-the-counters-start-again-after-it
// ---------------------------------------------------------------------------------------------

/// A reading every two seconds, counted once, carrying the silence -- and the counters start again.
///
/// Two halves meet here: a reading being taken with nothing arriving, and the counters the same
/// reading carries. They are one claim and this is it.
pub fn a_link_reading_is_taken_every_two_seconds() {
    use dereth_client_net::linkstatus::HEARTBEAT_INTERVAL;

    // Part one: the reading is taken on a link where nothing is arriving at all.
    let (mut net, mut shard) = LinkShard::new();
    net.feed(&shard.hello(), shard.from, LocalTime(10.0));
    assert_eq!(net.status(), LinkStatus::LoginConnecting);
    let hello_is_not_connected = !net.take_connected();

    let ordinary = shard.ordinary();
    let arrived_bytes = u32::try_from(ordinary.len()).expect("a small datagram");
    net.feed(&ordinary, shard.from, LocalTime(10.1));
    let connected_once = net.take_connected() && !net.take_connected();

    net.receive_use_time(LocalTime(11.999));
    let before_the_boundary = net.take_link_heartbeat().is_none();
    net.receive_use_time(LocalTime(12.0));
    let first = net.take_link_heartbeat() == Some(0.0);
    let s = net.session.transport.link_status().snapshot;
    let carries_the_bytes = s.bytes_received == arrived_bytes;
    let carries_the_silence = (s.time_since_last_got_data - 1.9).abs() < 1e-5;

    net.receive_use_time(LocalTime(13.999));
    let next_is_independent = net.take_link_heartbeat().is_none();
    net.receive_use_time(LocalTime(14.0));
    let second = net.take_link_heartbeat() == Some(0.0);
    let q = net.session.transport.link_status().snapshot;
    let quiet_is_a_reading_of_nothing =
        q.bytes_received == 0 && (q.time_since_last_got_data - 3.9).abs() < 1e-5;

    // Part two: the counters the reading carries, and the fact that they start again.
    let (mut raw, addr) = a_bare_transport();
    for i in 0..10 {
        #[allow(clippy::cast_precision_loss)]
        let t = LocalTime(0.1 * f64::from(i));
        raw.feed(&plain_datagram(), Some(addr), t)
            .expect("a legal datagram");
    }
    let counted = raw.current_link_status().pkts_received == 10;
    let no_reading_yet =
        raw.link_status().pkts_received.count() == 0 && raw.take_heartbeat().is_none();

    raw.feed(&plain_datagram(), Some(addr), LocalTime(HEARTBEAT_INTERVAL))
        .expect("legal");
    raw.process_connections(LocalTime(HEARTBEAT_INTERVAL));
    let one_sample = raw.link_status().pkts_received.count() == 1
        && (raw.link_status().pkts_received.total() - 11.0).abs() < f64::EPSILON;
    let started_again = raw.current_link_status().pkts_received == 0;
    let notified_once = raw.take_heartbeat().is_some() && raw.take_heartbeat().is_none();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "link.reading.is-taken-every-two-seconds-and-the-counters-start-again-after-it",
        move |_| {
            hello_is_not_connected
                && connected_once
                && before_the_boundary
                && first
                && carries_the_bytes
                && carries_the_silence
                && next_is_independent
                && second
                && quiet_is_a_reading_of_nothing
                && counted
                && no_reading_yet
                && one_sample
                && started_again
                && notified_once
        },
    );
}

// ---------------------------------------------------------------------------------------------
// link.dropped.a-refusal-after-the-login-window-closes-is-a-lost-shard-and-not-a-login-failure
// ---------------------------------------------------------------------------------------------

/// A refusal after the login is over logs the client off and reads as the shard having gone.
pub fn a_refusal_after_the_login_is_over_is_a_lost_shard() {
    let (mut net, shard) = a_connected_link();
    net.packet_controller_use_time(LocalTime(20.1));
    net.take_outgoing();

    let mut refusal = OutPacket::new(ProtoHeader {
        seq_id: 0,
        rec_id: RECIPIENT,
        interval: 0x100,
        iteration: 1,
        ..ProtoHeader::default()
    });
    refusal
        .add_optional_header(
            PacketFlags::NET_ERROR,
            NetErrorCode::ServerFull.pack().to_vec(),
        )
        .expect("the refusal header");
    net.feed(
        &refusal.serialize(None).expect("a serialisable packet"),
        shard.from,
        LocalTime(20.2),
    );

    let down = net.status() == LinkStatus::Disconnected;
    // No login is left to fail, so the specific reason is not published as a login error...
    let not_a_login_failure = net.error().is_none();
    let edge_withdrawn = !net.take_connected();
    // ...and the client logs itself off and says the shard has gone.
    let logged_off = net.session.transport.log_off_sent();
    let shard_died =
        net.session_state() == SessionState::Disconnected(DisconnectReason::ServerDied);
    let out = net.take_outgoing();
    let one_goodbye = out.len() == 1
        && ParsedPacket::parse(&out[0].0)
            .expect("a parsable goodbye")
            .header
            .header
            .0
            == PacketFlags::DISCONNECT;

    // The other end of the same rule: a refusal that arrives before the link is even up is still
    // read, and is a login failure, because the login is what it is refusing.
    let before_the_link = {
        let (mut early, early_shard) = LinkShard::new();
        let mut packet = OutPacket::new(ProtoHeader {
            rec_id: RECIPIENT,
            ..ProtoHeader::default()
        });
        packet
            .add_optional_header(
                PacketFlags::NET_ERROR,
                NetErrorCode::ServerFull.pack().to_vec(),
            )
            .expect("the refusal header");
        early.feed(
            &packet.serialize(None).expect("a serialisable packet"),
            early_shard.from,
            LocalTime(2.0),
        );
        early.status() == LinkStatus::Failed(NetErrorCode::ServerFull)
    };

    let mut c = HeadlessClient::model();
    c.assert_behaviour("link.dropped.a-refusal-after-the-login-window-closes-is-a-lost-shard-and-not-a-login-failure", move |_| {
        down && not_a_login_failure && edge_withdrawn && logged_off && shard_died && one_goodbye
            && before_the_link
    });
}

// ---------------------------------------------------------------------------------------------
// link.dropped.a-shard-that-says-why-keeps-that-reason-while-the-goodbye-completes
// ---------------------------------------------------------------------------------------------

/// The named reason is published at once and survives the goodbye that follows it.
pub fn a_named_reason_survives_the_goodbye_that_follows_it() {
    let (mut net, mut shard) = a_connected_link();

    let goodbye = shard.goodbye(NetErrorCode::ServerFull);
    net.feed(&goodbye, shard.from, LocalTime(30.2));
    let told_at_once = net.status() == LinkStatus::Failed(NetErrorCode::ServerFull)
        && net.error() == Some(NetErrorCode::ServerFull);
    // ...and the ordinary goodbye lifecycle is not short-circuited by it.
    let still_saying_goodbye = net
        .session
        .transport
        .connection_state(RecipientId(RECIPIENT))
        == ConnectionState::DisconnectReceived;

    net.packet_controller_use_time(LocalTime(30.2));
    net.take_outgoing(); // an ordinary acknowledgement may be due first
    net.packet_controller_use_time(LocalTime(30.3));
    let out = net.take_outgoing();
    let answered_once = out.len() == 1
        && ParsedPacket::parse(&out[0].0)
            .expect("a parsable response")
            .optional
            .get(&PacketFlags::NET_ERROR_DISCONNECT)
            == Some(&NO_COMPLAINT.to_vec());

    net.receive_use_time(LocalTime(30.4));
    let taken_down = net.status() == LinkStatus::Disconnected;
    let reason_kept = net.error() == Some(NetErrorCode::ServerFull);
    let shard_died =
        net.session_state() == SessionState::Disconnected(DisconnectReason::ServerDied);

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "link.dropped.a-shard-that-says-why-keeps-that-reason-while-the-goodbye-completes",
        move |_| {
            told_at_once
                && still_saying_goodbye
                && answered_once
                && taken_down
                && reason_kept
                && shard_died
        },
    );
}

// ---------------------------------------------------------------------------------------------
// link.dropped.a-goodbye-is-answered-once-and-the-link-is-taken-down-on-the-next-pass
// ---------------------------------------------------------------------------------------------

/// A goodbye with no complaint: answered once, even when repeated, and then the link goes down.
pub fn a_goodbye_with_no_complaint_is_answered_once_and_then_the_link_goes_down() {
    let (mut net, mut shard) = a_connected_link();

    let goodbye = shard.goodbye(NetErrorCode::None);
    net.feed(&goodbye, shard.from, LocalTime(10.2));
    // The header owns the change of state, not the no-complaint body inside it...
    let saying_goodbye = net
        .session
        .transport
        .connection_state(RecipientId(RECIPIENT))
        == ConnectionState::DisconnectReceived;
    // ...and no complaint is not an error, so the link is still up and nothing is published.
    let still_up = net.status() == LinkStatus::Connected && net.error().is_none();

    net.packet_controller_use_time(LocalTime(10.2));
    let first_pass = net.take_outgoing();
    let nothing_yet = first_pass.iter().all(|(bytes, _)| {
        !ParsedPacket::parse(bytes)
            .is_ok_and(|p| p.header.header.contains(PacketFlags::NET_ERROR_DISCONNECT))
    });

    // A second, identical goodbye must not throw away the answer already queued behind the first
    // -- which would hand that answer off unacknowledged and queue a duplicate behind it.
    let repeated = shard.goodbye(NetErrorCode::None);
    net.feed(&repeated, shard.from, LocalTime(10.25));
    let still_saying_goodbye = net
        .session
        .transport
        .connection_state(RecipientId(RECIPIENT))
        == ConnectionState::DisconnectReceived;

    net.packet_controller_use_time(LocalTime(10.3));
    let out = net.take_outgoing();
    let answered_once = out.len() == 1 && {
        let response = ParsedPacket::parse(&out[0].0).expect("a parsable response");
        response.header.header.is_encrypted()
            && response.header.seq_id != 0
            && response.optional.get(&PacketFlags::NET_ERROR_DISCONNECT)
                == Some(&NO_COMPLAINT.to_vec())
    };

    net.receive_use_time(LocalTime(10.4));
    let gone = net
        .session
        .transport
        .connection_state(RecipientId(RECIPIENT))
        == ConnectionState::Disconnected
        && net.status() == LinkStatus::Disconnected
        && net.session_state() == SessionState::Disconnected(DisconnectReason::ServerDied);

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "link.dropped.a-goodbye-is-answered-once-and-the-link-is-taken-down-on-the-next-pass",
        move |_| {
            saying_goodbye
                && still_up
                && nothing_yet
                && still_saying_goodbye
                && answered_once
                && gone
        },
    );
}

// ---------------------------------------------------------------------------------------------
// link.dropped.a-tampered-datagram-changes-nothing-and-another-recipient-leaving-is-not-the-shard
// ---------------------------------------------------------------------------------------------

/// The two guards that stand beside the graceful goodbye's own claim.
pub fn a_tampered_datagram_changes_nothing_and_another_party_leaving_is_not_the_shard() {
    // One: a datagram whose contents do not match what it claims is thrown away whole. Its body
    // is a perfectly readable goodbye-with-a-reason and none of it is acted on.
    let tampered = {
        let (mut net, mut shard) = a_connected_link();
        let mut corrupt = shard.goodbye(NetErrorCode::ServerFull);
        corrupt[8] ^= 1;
        net.feed(&corrupt, shard.from, LocalTime(1.2));
        net.status() == LinkStatus::Connected
            && net.error().is_none()
            && net.rejected() == 1
            && net
                .session
                .transport
                .connection_state(RecipientId(RECIPIENT))
                == ConnectionState::Connected
    };

    // Two: a second party on the same link saying goodbye takes only that party away.
    const OTHER: u16 = 0x0C;
    const OTHER_INCOMING: u32 = 0xCAFE_BABE;
    let (mut net, shard) = a_connected_link();
    net.session.transport.add_connection_at(
        OTHER,
        OTHER,
        1,
        OTHER_INCOMING,
        0x0D15_EA5E,
        Some(shard.from),
        LocalTime(3.1),
    );
    let mut other_crypto = CryptoSystem::new(OTHER_INCOMING);
    let mut other_sequence = 2_u32;
    let mut other_packet = |mask: u32, body: Vec<u8>| {
        let mut packet = OutPacket::new(ProtoHeader {
            seq_id: other_sequence,
            rec_id: OTHER,
            interval: 0x100,
            iteration: 1,
            ..ProtoHeader::default()
        });
        other_sequence = other_sequence.wrapping_add(1);
        packet
            .add_optional_header(mask, body)
            .expect("the other party's header");
        packet
            .serialize(Some(other_crypto.next()))
            .expect("an encrypted packet")
    };
    net.session
        .transport
        .feed(
            &other_packet(PacketFlags::ECHO_REQUEST, 0f32.to_le_bytes().to_vec()),
            Some(shard.from),
            LocalTime(3.2),
        )
        .expect("the other party is established");
    net.session
        .transport
        .feed(
            &other_packet(PacketFlags::NET_ERROR_DISCONNECT, NO_COMPLAINT.to_vec()),
            Some(shard.from),
            LocalTime(3.3),
        )
        .expect("the other party says goodbye");
    net.session
        .transport
        .tick(LocalTime(3.3), std::time::Duration::ZERO);
    net.session.transport.take_outgoing();
    net.session
        .transport
        .tick(LocalTime(3.4), std::time::Duration::ZERO);
    net.session.transport.take_outgoing();
    net.session.transport.process_connections(LocalTime(3.5));

    let other_gone =
        net.session.transport.connection_state(RecipientId(OTHER)) == ConnectionState::Disconnected;
    let shard_untouched = net
        .session
        .transport
        .connection_state(RecipientId(RECIPIENT))
        == ConnectionState::Connected
        && !net.session.transport.take_server_died()
        && net.status() == LinkStatus::Connected;

    let mut c = HeadlessClient::model();
    c.assert_behaviour("link.dropped.a-tampered-datagram-changes-nothing-and-another-recipient-leaving-is-not-the-shard", move |_| {
        tampered && other_gone && shard_untouched
    });
}

// ---------------------------------------------------------------------------------------------
// link.dropped.only-real-silence-takes-the-link-down-and-a-stalled-client-is-not-silence
// ---------------------------------------------------------------------------------------------

/// Two clocks: the shard's silence, and the client's own stall.
///
/// The silence the client allows is measured against its own network clock, and the comparison is
/// strict; the second clock is the client's own frame time, and a gap in *that* suppresses the
/// pass rather than being read as the shard having gone -- so a client that was frozen does not
/// hang up on a shard that was talking the whole time.
pub fn only_real_silence_takes_the_link_down() {
    let (mut net, _shard) = a_connected_link();

    net.session.transport.set_cur_time(10.0);
    net.receive_use_time(LocalTime(10.1));
    net.session.transport.set_cur_time(10.1);
    net.receive_use_time(LocalTime(150.1));
    let strict = net
        .session
        .transport
        .connection_state(RecipientId(RECIPIENT))
        == ConnectionState::Connected;

    net.session.transport.set_cur_time(200.1);
    net.receive_use_time(LocalTime(200.2));
    let a_stall_is_not_silence = net
        .session
        .transport
        .connection_state(RecipientId(RECIPIENT))
        == ConnectionState::Connected;

    net.session.transport.set_cur_time(200.2);
    net.receive_use_time(LocalTime(200.3));
    let the_next_frame_sees_it = net
        .session
        .transport
        .connection_state(RecipientId(RECIPIENT))
        == ConnectionState::Disconnected;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "link.dropped.only-real-silence-takes-the-link-down-and-a-stalled-client-is-not-silence",
        move |_| strict && a_stall_is_not_silence && the_next_frame_sees_it,
    );
}

// =============================================================================================
// The packet-loss figure
//
// Two things are deliberately not rows: the calibration that an untouched link divides nothing and
// answers none (which is the premise of the two scenarios below and is asserted in them), and a
// check that the starting figure is spelled the same in the two crates that carry it, which would
// be a literal beside a literal.
// =============================================================================================

/// One connection on a socket-free transport, with the first one naming the shard's own recipient
/// -- which is what the two-second reading keys on.
fn a_bare_transport() -> (dereth_client_net::Net, SocketAddr) {
    let mut net = dereth_client_net::Net::new(dereth_client_net::NetConfig::default());
    let addr = peer();
    net.add_connection(
        RECIPIENT,
        0,
        1,
        LINK_OUTGOING_SEED,
        LINK_INCOMING_SEED,
        Some(addr),
    );
    assert_eq!(
        net.world_recipient().0,
        RECIPIENT,
        "the reading keys on the shard's recipient"
    );
    (net, addr)
}

/// The plainest thing a live shard sends: one unsequenced, unencrypted datagram, and one arrival.
fn plain_datagram() -> Vec<u8> {
    let mut p = OutPacket::new(ProtoHeader {
        seq_id: 0,
        rec_id: RECIPIENT,
        interval: 0x100,
        iteration: 1,
        ..ProtoHeader::default()
    });
    p.add_optional_header(PacketFlags::ACK_SEQUENCE, 1_u32.to_le_bytes().to_vec())
        .expect("one optional header");
    p.serialize(None).expect("a serialisable packet")
}

/// A datagram asking for `ids.len()` sequence numbers to be sent again -- the peer saying it lost
/// them, which is what the loss figure's numerator counts.
fn ask_again_for(ids: &[u32]) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(&u32::try_from(ids.len()).expect("a small ask").to_le_bytes());
    for id in ids {
        body.extend_from_slice(&id.to_le_bytes());
    }
    let mut p = OutPacket::new(ProtoHeader {
        seq_id: 0,
        rec_id: RECIPIENT,
        interval: 0x100,
        iteration: 1,
        ..ProtoHeader::default()
    });
    p.add_optional_header(PacketFlags::REQUEST_RETRANSMIT, body)
        .expect("one optional header");
    p.serialize(None).expect("a serialisable packet")
}

// ---------------------------------------------------------------------------------------------
// link.packet-loss.counts-what-the-client-sent-as-well-as-what-it-received
// ---------------------------------------------------------------------------------------------

/// The proportion, over both directions.
pub fn the_packet_loss_figure_counts_both_directions() {
    use dereth_client_net::linkstatus::{HEARTBEAT_INTERVAL, INITIAL_PACKET_LOSS};

    // The premise, and the calibration: an untouched link
    // divides nothing and answers none -- and none is not the starting figure a client shows.
    let (untouched, _) = a_bare_transport();
    let untouched_is_none = untouched.average_packet_loss().abs() < f64::EPSILON
        && untouched.link_status().pkts_received.count() == 0
        && (f64::from(INITIAL_PACKET_LOSS) - untouched.average_packet_loss()).abs() > f64::EPSILON;

    // Nothing sent: the proportion is twice the asks over what arrived.
    let (mut net, addr) = a_bare_transport();
    for i in 0..19 {
        #[allow(clippy::cast_precision_loss)]
        net.feed(
            &plain_datagram(),
            Some(addr),
            LocalTime(0.05 * f64::from(i)),
        )
        .expect("a legal datagram");
    }
    net.feed(&ask_again_for(&[7, 8, 9, 10]), Some(addr), LocalTime(1.0))
        .expect("a legal ask");
    let counted =
        net.current_link_status().pkts_received == 20 && net.current_link_status().pkts_naked == 4;
    net.feed(&plain_datagram(), Some(addr), LocalTime(HEARTBEAT_INTERVAL))
        .expect("legal");
    net.process_connections(LocalTime(HEARTBEAT_INTERVAL));
    let a = net.link_status();
    let nothing_sent = a.pkts_sent.total().abs() < f64::EPSILON;
    let received_only = (net.average_packet_loss() - 2.0 * (4.0 / 21.0)).abs() < 1e-12;

    // And with the client sending too, the same asks read as less loss, because the denominator
    // is everything that crossed the link.
    let (mut net, addr) = a_bare_transport();
    for _ in 0..10 {
        net.feed(&plain_datagram(), Some(addr), LocalTime(0.0))
            .expect("legal");
    }
    net.feed(&ask_again_for(&[1, 2]), Some(addr), LocalTime(0.5))
        .expect("legal");
    net.tick(LocalTime(1.0), std::time::Duration::from_millis(50));
    let sent = net.take_outgoing().len();
    assert!(
        sent > 0,
        "the client must really have sent something for this to measure anything"
    );
    let sent_counted = net.current_link_status().pkts_sent == u16::try_from(sent).expect("small");
    net.feed(
        &plain_datagram(),
        Some(addr),
        LocalTime(HEARTBEAT_INTERVAL + 1.0),
    )
    .expect("legal");
    net.process_connections(LocalTime(HEARTBEAT_INTERVAL + 1.0));
    #[allow(clippy::cast_precision_loss)]
    let want = 2.0 * (2.0 / (12.0 + sent as f64));
    let both_ways = (net.average_packet_loss() - want).abs() < 1e-12;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "link.packet-loss.counts-what-the-client-sent-as-well-as-what-it-received",
        move |_| {
            untouched_is_none
                && counted
                && nothing_sent
                && received_only
                && sent_counted
                && both_ways
        },
    );
}

// ---------------------------------------------------------------------------------------------
// link.packet-loss.a-burst-ages-out-of-a-window-far-longer-than-the-line-says
// ---------------------------------------------------------------------------------------------

/// A burst arrives, is diluted, and finally leaves the figure altogether.
pub fn a_burst_of_loss_ages_out_of_the_window() {
    use dereth_client_net::linkstatus::{HEARTBEAT_INTERVAL, PACKET_SAMPLES};

    let (mut net, addr) = a_bare_transport();
    let mut t = 0.0;
    net.feed(
        &ask_again_for(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]),
        Some(addr),
        LocalTime(t),
    )
    .expect("legal");
    t += HEARTBEAT_INTERVAL;
    net.feed(&plain_datagram(), Some(addr), LocalTime(t))
        .expect("legal");
    net.process_connections(LocalTime(t));
    let lossy = net.average_packet_loss();
    let in_the_window = lossy > 0.0;

    for _ in 1..PACKET_SAMPLES {
        t += HEARTBEAT_INTERVAL;
        net.feed(&plain_datagram(), Some(addr), LocalTime(t))
            .expect("legal");
        net.process_connections(LocalTime(t));
    }
    let diluted = net.average_packet_loss();
    let still_there = diluted > 0.0 && diluted < lossy;
    let whole_window = net.link_status().pkts_naked.count()
        == u16::try_from(PACKET_SAMPLES).expect("the window is a small number");

    t += HEARTBEAT_INTERVAL;
    net.feed(&plain_datagram(), Some(addr), LocalTime(t))
        .expect("legal");
    net.process_connections(LocalTime(t));
    let gone = net.average_packet_loss().abs() < 1e-12;

    // And the window really is longer than the line beside it claims: forty readings two seconds
    // apart is eighty seconds, not ten.
    #[allow(clippy::cast_precision_loss)]
    let longer_than_the_line_says = PACKET_SAMPLES as f64 * HEARTBEAT_INTERVAL > 10.0;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "link.packet-loss.a-burst-ages-out-of-a-window-far-longer-than-the-line-says",
        move |_| in_the_window && still_there && whole_window && gone && longer_than_the_line_says,
    );
}
