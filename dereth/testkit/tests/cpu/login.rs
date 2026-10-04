//! Login: what a recorded login delivers, and what the client is before one.
//!
//! **The replay scenarios drive the client's own endpoint directly and book the claim at the end**,
//! as `net.rs` and `frame.rs` do: the subject is the handshake itself, the datagrams the client
//! answers with, and what the transport does with a recording's last datagram, none of which
//! `Given::EnteredWorld` can express. The character-set scenario uses `Given::EnteredWorld` on a
//! model-only client.
//!
//! **No socket is bound and no count is pinned.** Every byte comes out of a committed recording,
//! the recordings are the list `dereth_client_net::client_session::testing::session_index()` names,
//! and every denominator is read off them at run time.

use dereth_client::net::{ClientNetwork, LinkStatus};
use dereth_client::objects::ObjectStream;
use dereth_client_net::client_session::testing::capture::{self, Datagram};
use dereth_client_net::client_session::{DisconnectReason, SessionEvent, SessionState};
use dereth_primitives::{LocalTime, NetQueue};
use dereth_testkit::{Given, HeadlessClient};
use dereth_transport::wire::{PacketFlags, ParsedPacket};

/// The recording whose client half is compared against this client's, and the stand-in credential
/// it carries.
///
/// **It is a pseudonym and not a credential.** Every account, secret and character name in the
/// locked corpus is a same-length stand-in, so this names the recording's
/// own bytes; only the handshake scenario compares it against the wire, and it is this recording
/// it compares.
const FIRST_LOGIN: &str = "first-login-walk-jump";
const FIRST_LOGIN_ACCOUNT: &str = "ac01";

/// The connection sweep's own arm: the shard has this long to say something.
const CONNECTION_TIMEOUT: f64 = 140.0;

/// The recording, and the endpoint it replays into: both are `dereth_testkit::replay`'s, shared
/// with every scenario that replays a recording.
use dereth_testkit::replay::records as recording;

/// The endpoint, named with the account whose login request this file reads. The account is not
/// the harness's default here: three of these scenarios assert over the bytes the client's **own**
/// login request carries, and the account is one of them.
fn endpoint(records: &[Datagram], account: &str) -> ClientNetwork {
    dereth_testkit::replay::recorded_endpoint_as(records, account, account)
}

/// One recorded datagram this client emitted, and where it was sent.
type Emitted = (Vec<u8>, std::net::SocketAddr);

/// The whole recording through the real transport, the real session and the real object stream --
/// the application's own loop, with the renderer left out.
fn replay(session: &str, account: &str) -> (ClientNetwork, Vec<Emitted>, Vec<SessionEvent>) {
    let records = recording(session);
    let mut net = endpoint(&records, account);
    let mut objects = ObjectStream::new();
    let mut emitted: Vec<Emitted> = Vec::new();
    let mut events: Vec<SessionEvent> = Vec::new();
    let mut entered = false;
    for r in &records {
        let now = LocalTime(r.t);
        if !r.c2s {
            net.feed(&r.raw, r.peer(), now);
        }
        net.tick(now);
        emitted.extend(net.take_outgoing());
        for e in objects.pump(&mut net, now) {
            // The player picks a character; the state machine does not care when, only that the
            // shard's set came first.
            if let SessionEvent::CharacterSet(set) = &e {
                if !entered {
                    if let Some(c) = set.characters.first() {
                        let account = set.account.clone();
                        net.enter_world(c.gid, &account);
                        entered = true;
                    }
                }
            }
            // …or creates one in the session and enters with that, which is what three of the
            // recordings did: their account's first character set is empty.
            if let SessionEvent::CharGenResponse(r) = &e {
                if !entered
                    && r.response_type
                        == dereth_client_net::client_session::CG_VERIFICATION_RESPONSE_OK
                {
                    let account = net.characters().account.clone();
                    net.enter_world(r.identity.gid, &account);
                    entered = true;
                }
            }
            events.push(e);
        }
    }
    (net, emitted, events)
}

/// Feed only the shard's half and never let the session poll, so what comes back out of the
/// transport is the whole blob stream the recording carried.
fn transport_blobs(session: &str) -> (ClientNetwork, Vec<dereth_primitives::IncomingMessage>) {
    let records = recording(session);
    let mut net = endpoint(&records, "dereth-testkit");
    for r in records.iter().filter(|r| !r.c2s) {
        net.feed(&r.raw, r.peer(), LocalTime(r.t));
    }
    let mut blobs = Vec::new();
    while let Some(m) = dereth_primitives::Transport::poll(&mut net.session.transport) {
        blobs.push(m);
    }
    (net, blobs)
}

/// Whole blobs out of the datagrams this client emitted, as `(queue, opcode)`.
fn sent_blobs(emitted: &[Emitted]) -> Vec<(u16, u32)> {
    let mut out = Vec::new();
    for (bytes, _) in emitted {
        let Ok(p) = ParsedPacket::parse(bytes) else {
            continue;
        };
        for f in &p.fragments {
            if f.header.blob_num == 0 && f.payload.len() >= 4 {
                out.push((
                    f.header.queue_id,
                    u32::from_le_bytes([f.payload[0], f.payload[1], f.payload[2], f.payload[3]]),
                ));
            }
        }
    }
    out
}

/// The blob id of every fragment in a set of datagrams, in order.
fn blob_ids(datagrams: &[Vec<u8>]) -> Vec<u64> {
    let mut out = Vec::new();
    for bytes in datagrams {
        let Ok(p) = ParsedPacket::parse(bytes) else {
            continue;
        };
        out.extend(p.fragments.iter().map(|f| f.header.blob_id()));
    }
    out
}

/// The recordings that carry a character into the world, discovered rather than named.
fn world_recordings() -> &'static [&'static str] {
    static CACHE: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        let mut out = Vec::new();
        for (id, _slug) in dereth_client_net::client_session::testing::session_index() {
            let (_, _, events) = replay(id, "dereth-testkit");
            if events
                .iter()
                .any(|e| matches!(e, SessionEvent::PlayerDescription(_)))
            {
                out.push(*id);
            }
        }
        assert!(
            out.is_empty().then_some(0).is_none(),
            "no recording enters the world at all"
        );
        out
    })
}

// -------------------------------------------------------------------------------------------
// login.handshake.is-the-retail-clients-own-bytes-on-the-shards-two-ports
// -------------------------------------------------------------------------------------------

/// Given the shard's half, this client answers with the retail client's own datagrams.
pub fn the_handshake_is_the_retail_clients_own() {
    let records = recording(FIRST_LOGIN);
    let theirs: Vec<&Datagram> = records.iter().filter(|r| r.c2s).collect();
    let (net, emitted, _) = replay(FIRST_LOGIN, FIRST_LOGIN_ACCOUNT);

    // The login request, byte for byte, to the port the recording was made against.
    let login_request = emitted[0].0 == theirs[0].raw
        && emitted[0].1 == net.logon_addr()
        && emitted[0].1 == capture::peer(0);
    // The connect response, byte for byte, to the **next port up** -- the asymmetry that is the
    // easiest thing in the whole handshake to get wrong.
    let connect_response = emitted[1].0 == theirs[1].raw
        && emitted[1].1 == capture::peer(1)
        && net.handshake_addr() == Some(emitted[1].1);
    let completed = net.status() == LinkStatus::Connected && net.error().is_none();

    // And the first message this client puts on the wire is numbered one, not zero, exactly as the
    // retail client's is: a shard parks a fragment numbered zero for ever and says nothing.
    let their_ids = blob_ids(&theirs.iter().map(|r| r.raw.clone()).collect::<Vec<_>>());
    let ours = blob_ids(&emitted.iter().map(|(b, _)| b.clone()).collect::<Vec<_>>());
    let numbered_from_one = their_ids.first() == ours.first()
        && their_ids.get(1) == ours.get(1)
        && their_ids.first().is_some_and(|id| id & 0xFFFF_FFFF == 1);

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "login.handshake.is-the-retail-clients-own-bytes-on-the-shards-two-ports",
        move |_| login_request && connect_response && completed && numbered_from_one,
    );
}

dereth_testkit::scenarios! {
    scenario_the_handshake_is_the_retail_clients_own => the_handshake_is_the_retail_clients_own ["login.handshake.is-the-retail-clients-own-bytes-on-the-shards-two-ports"],
    scenario_every_recorded_blob_is_ephemeral_and_unordered => every_recorded_blob_is_ephemeral_and_unordered ["login.replay.every-blob-the-shard-sends-is-ephemeral-and-unordered"],
    scenario_the_data_download_interrogation_is_surfaced_and_unanswered => the_data_download_interrogation_is_surfaced_and_unanswered ["login.data-download.the-shards-interrogation-is-surfaced-and-left-unanswered"],
    scenario_the_description_is_what_puts_you_in_the_world => the_description_is_what_puts_you_in_the_world ["login.enter-world.the-description-and-not-the-ready-message-puts-you-in-the-world"],
    scenario_every_recorded_session_is_consumed_whole => every_recorded_session_is_consumed_whole ["login.replay.every-recorded-session-is-consumed-whole-and-stays-connected"],
    scenario_login_offers_the_recordings_own_characters => login_offers_the_recordings_own_characters ["login.replay.character-set-is-the-recordings-own"],
}

// -------------------------------------------------------------------------------------------
// login.replay.every-blob-the-shard-sends-is-ephemeral-and-unordered
// -------------------------------------------------------------------------------------------

/// Every blob the recorded shards sent, over every recording the corpus holds.
pub fn every_recorded_blob_is_ephemeral_and_unordered() {
    let mut blobs = 0usize;
    let mut all_ephemeral = true;
    let mut none_ordered = true;
    for (id, _slug) in dereth_client_net::client_session::testing::session_index() {
        let (_, stream) = transport_blobs(id);
        blobs += stream.len();
        for m in &stream {
            all_ephemeral &= m.blob_id.high32() & 0x8000_0000 == 0x8000_0000;
            none_ordered &= m.blob_id.high32() & 0x1F00_0000 == 0;
        }
    }
    println!("{blobs} recorded shard blobs, every one ephemeral and none ordered");

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "login.replay.every-blob-the-shard-sends-is-ephemeral-and-unordered",
        move |_| blobs > 0 && all_ephemeral && none_ordered,
    );
}

// -------------------------------------------------------------------------------------------
// login.data-download.the-shards-interrogation-is-surfaced-and-left-unanswered
// -------------------------------------------------------------------------------------------

/// The interrogation reaches the caller and nothing answers it, and the world is entered anyway.
pub fn the_data_download_interrogation_is_surfaced_and_unanswered() {
    let (_, emitted, events) = replay(FIRST_LOGIN, FIRST_LOGIN_ACCOUNT);
    let surfaced = events.iter().any(|e| {
        matches!(
            e,
            SessionEvent::Ddd(dereth_client_net::client_session::DddEvent::Interrogation(
                _
            ))
        )
    });
    // Building the answer needs an encoder over the data files' iteration lists, which is not the
    // application's work; what matters is that the shard lets the character in without it.
    let unanswered = !sent_blobs(&emitted).iter().any(|(_, op)| *op == 0xF7E6);
    let in_anyway = events
        .iter()
        .any(|e| matches!(e, SessionEvent::PlayerDescription(_)));

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "login.data-download.the-shards-interrogation-is-surfaced-and-left-unanswered",
        move |_| surfaced && unanswered && in_anyway,
    );
}

// -------------------------------------------------------------------------------------------
// login.enter-world.the-description-and-not-the-ready-message-puts-you-in-the-world
// -------------------------------------------------------------------------------------------

/// The journey, not the terminal state: every recording ends with a clean logout.
pub fn the_description_is_what_puts_you_in_the_world() {
    let mut journeys = 0usize;
    let mut order_holds = true;
    let mut queue_holds = true;
    let mut description_is_real = true;
    for name in world_recordings() {
        let (net, emitted, events) = replay(name, "dereth-testkit");
        let ready = events
            .iter()
            .position(|e| matches!(e, SessionEvent::EnterWorldReady));
        let desc = events
            .iter()
            .position(|e| matches!(e, SessionEvent::PlayerDescription(_)));
        let playable = events
            .iter()
            .position(|e| e == &SessionEvent::StateChanged(SessionState::Playable));
        let (Some(ready), Some(desc), Some(playable)) = (ready, desc, playable) else {
            order_holds = false;
            continue;
        };
        journeys += 1;
        // The shard says the world is ready first, and it is the description that flips the state.
        order_holds &= ready < desc && desc + 1 == playable;
        if let SessionEvent::PlayerDescription(m) = &events[desc] {
            description_is_real &=
                !m.content_profiles.is_empty() || !m.inventory_placements.is_empty();
        }
        // The enter-world request and the character pick both go out on the login queue, in that
        // order. Against this shard both recipients are the same, which is exactly why this is
        // asserted on the queue and not on the address.
        let enter: Vec<(u16, u32)> = sent_blobs(&emitted)
            .into_iter()
            .filter(|(_, op)| *op == 0xF7C8 || *op == 0xF657)
            .collect();
        queue_holds &= enter == vec![(4, 0xF7C8), (4, 0xF657)];
        // A clean logout leaves the session back at character select, which is correct behaviour
        // and not a failure to enter.
        queue_holds &= matches!(
            net.session_state(),
            SessionState::Playable | SessionState::CharacterSelect
        );
    }
    // The transport handed the session the messages the journey is made of, on the queues the
    // client routes them by.
    let (net, blobs) = transport_blobs(FIRST_LOGIN);
    let on = |q: NetQueue, op: u32| blobs.iter().any(|m| m.queue == q && m.opcode == op);
    let the_whole_login_arrived = net.status() == LinkStatus::Connected
        && on(NetQueue::ClientCache, 0xF7E5)
        && on(NetQueue::UiQueue, 0xF658)
        && on(NetQueue::UiQueue, 0xF7E1)
        && on(NetQueue::UiQueue, 0xF7DF)
        && on(NetQueue::WorldObjects, 0xF746);

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "login.enter-world.the-description-and-not-the-ready-message-puts-you-in-the-world",
        move |_| {
            journeys > 0
                && order_holds
                && queue_holds
                && description_is_real
                && the_whole_login_arrived
        },
    );
}

// -------------------------------------------------------------------------------------------
// login.replay.every-recorded-session-is-consumed-whole-and-stays-connected
// -------------------------------------------------------------------------------------------

/// What the recording itself says about the blobs on its wire.
#[derive(Default)]
struct WireBlobs {
    complete: usize,
    incomplete: usize,
    duplicate_datagrams: usize,
}

/// **A duplicate is a datagram the recording really carries twice, not one that arrives late.**
///
/// "A sequence number at or below the highest seen so far" is the same thing only while the shard
/// delivers in order. One recording in the corpus carries a burst of several hundred out-of-order
/// server datagrams in four seconds, none of them repeated and every one of them accepted; the
/// high-water reading calls all of them duplicates, and a hand-written inventory of recordings that
/// left that one out would never see it. What the client's own transport refuses is a datagram it
/// has already had, so that is what is counted here: a sequence number seen before, from the same
/// shard, on the same connection.
fn wire_blobs(records: &[Datagram]) -> WireBlobs {
    use std::collections::{BTreeMap, BTreeSet};

    let mut seen: BTreeMap<u64, (u16, BTreeSet<u16>)> = BTreeMap::new();
    let mut arrived: BTreeSet<(u16, u16, u32)> = BTreeSet::new();
    let mut out = WireBlobs::default();
    for r in records.iter().filter(|r| !r.c2s) {
        let Ok(p) = ParsedPacket::parse(&r.raw) else {
            continue;
        };
        if p.header.header.is_encrypted()
            && !arrived.insert((p.header.rec_id, r.pair, p.header.seq_id))
        {
            out.duplicate_datagrams += 1;
        }
        for f in &p.fragments {
            let e = seen
                .entry(f.header.blob_id())
                .or_insert((f.header.num_frags, BTreeSet::new()));
            e.1.insert(f.header.blob_num);
        }
    }
    for (num_frags, indices) in seen.into_values() {
        if indices.len() == usize::from(num_frags) {
            out.complete += 1;
        } else {
            out.incomplete += 1;
        }
    }
    out
}

/// The longest stretch during which the shard said nothing, measured the way the client's own
/// connection sweep measures it.
fn longest_server_silence(records: &[Datagram]) -> f64 {
    let mut last: Option<f64> = None;
    let mut worst = 0.0_f64;
    for r in records {
        if let Some(l) = last {
            worst = worst.max(r.t - l);
        }
        if !r.c2s {
            last = Some(r.t);
        }
    }
    worst
}

/// One segment per handshake the shard answered, starting at the login request that earned it.
///
/// **A recording is not always one login.** A player can log out and back in inside one capture,
/// and the transport's own sequence numbers restart when that happens -- so a recording replayed
/// as one stream reads every restart as a duplicate datagram. The split is on the *answered*
/// login request, because an unanswered one is a login that never happened.
fn is_login_request(r: &Datagram) -> bool {
    r.c2s
        && ParsedPacket::parse(&r.raw)
            .is_ok_and(|p| p.optional.contains_key(&PacketFlags::LOGIN_REQUEST))
}

fn is_connect_request(r: &Datagram) -> bool {
    !r.c2s
        && ParsedPacket::parse(&r.raw)
            .is_ok_and(|p| p.optional.contains_key(&PacketFlags::CONNECT_REQUEST))
}

fn logins(records: &[Datagram]) -> Vec<std::ops::Range<usize>> {
    let mut starts: Vec<usize> = Vec::new();
    for (i, _) in records
        .iter()
        .enumerate()
        .filter(|(_, r)| is_connect_request(r))
    {
        let start = (0..i)
            .rev()
            .find(|&j| is_login_request(&records[j]))
            .unwrap_or_else(|| {
                panic!("a handshake answer at datagram {i} with no request before it")
            });
        if starts.last() != Some(&start) {
            starts.push(start);
        }
    }
    assert!(!starts.is_empty(), "a recording with no handshake in it");
    let mut out = Vec::new();
    for (n, s) in starts.iter().enumerate() {
        let end = starts.get(n + 1).copied().unwrap_or(records.len());
        out.push(*s..end);
    }
    out
}

/// Every login in every recording, run to its end through the client's own endpoint.
#[allow(clippy::too_many_lines)]
pub fn every_recorded_session_is_consumed_whole() {
    let mut rows = 0usize;
    let mut refusals_are_the_recordings_own = true;
    let mut nothing_incomplete = true;
    let mut nothing_left_waiting = true;
    let mut every_blob_accounted_for = true;
    let mut the_link_matches_the_recording = true;
    let mut lived = 0usize;
    let mut died_of_recorded_silence = 0usize;

    for (id, slug) in dereth_client_net::client_session::testing::session_index() {
        let records = recording(id);
        for (n, range) in logins(&records).into_iter().enumerate() {
            let segment = &records[range];
            let wire = wire_blobs(segment);
            let silence = longest_server_silence(segment);
            let mut net = endpoint(segment, "dereth-testkit");
            let mut objects = ObjectStream::new();
            let mut events = 0usize;
            let mut entered = false;
            let mut left_connected = false;
            let mut was_connected = false;
            let mut server_died = false;
            for r in segment {
                let now = LocalTime(r.t);
                if !r.c2s {
                    net.feed(&r.raw, r.peer(), now);
                }
                net.tick(now);
                let _ = net.take_outgoing();
                for e in objects.pump(&mut net, now) {
                    if let SessionEvent::CharacterSet(set) = &e {
                        if !entered {
                            if let Some(ch) = set.characters.first() {
                                let account = set.account.clone();
                                net.enter_world(ch.gid, &account);
                                entered = true;
                            }
                        }
                    }
                    events += 1;
                }
                if net.status() == LinkStatus::Connected {
                    was_connected = true;
                } else if was_connected {
                    left_connected = true;
                }
                if net.session_state() == SessionState::Disconnected(DisconnectReason::ServerDied) {
                    server_died = true;
                }
            }
            let mut residual = 0usize;
            while dereth_primitives::Transport::poll(&mut net.session.transport).is_some() {
                residual += 1;
            }
            rows += 1;
            println!(
                "conformance: {slug} #{n} -- {} blobs on the wire, {events} session events, \
                 refused {} of {} recorded duplicates, silence {silence:.1} s, ended {:?}",
                wire.complete,
                net.rejected(),
                wire.duplicate_datagrams,
                net.status()
            );
            // The transport refuses exactly what the recording itself delivered twice, and
            // nothing else: a silent drop is only ever visible as this count.
            refusals_are_the_recordings_own &= net.rejected() as usize == wire.duplicate_datagrams;
            nothing_incomplete &= wire.incomplete == 0;
            nothing_left_waiting &= residual == 0;
            // Every message the recording carries whole reached the session layer as at least one
            // event. The surplus is the session's own non-blob events, which is why this is not an
            // equality.
            every_blob_accounted_for &= events >= wire.complete;
            // And the link's fate is the recording's own: it lives unless the recording goes
            // silent for longer than the shard is given, in which case it must end and say why.
            if silence > CONNECTION_TIMEOUT {
                died_of_recorded_silence += 1;
                the_link_matches_the_recording &=
                    net.status() == LinkStatus::Disconnected && server_died;
            } else {
                lived += 1;
                the_link_matches_the_recording &=
                    net.status() == LinkStatus::Connected && !left_connected && !server_died;
            }
        }
    }
    println!(
        "{rows} recorded logins -- {lived} lived to the last datagram, \
         {died_of_recorded_silence} ended on a silence the recording itself carries"
    );

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "login.replay.every-recorded-session-is-consumed-whole-and-stays-connected",
        move |_| {
            rows > 0
                && lived > 0
                && refusals_are_the_recordings_own
                && nothing_incomplete
                && nothing_left_waiting
                && every_blob_accounted_for
                && the_link_matches_the_recording
        },
    );
}

// -------------------------------------------------------------------------------------------
// 10. login.replay.character-set-is-the-recordings-own
// -------------------------------------------------------------------------------------------

/// Given what a shard said to a retail client, this client reaches character select with the
/// recording's own account and characters.
pub fn login_offers_the_recordings_own_characters() {
    let mut c = HeadlessClient::model();
    c.given(Given::EnteredWorld {
        session: "first-login-walk-jump",
        character: None,
    });
    let account = c.account().to_owned();
    let characters = c.characters().to_vec();

    c.assert_behaviour(
        "login.replay.character-set-is-the-recordings-own",
        move |_v| {
            !account.is_empty() && characters.len() == 2 && characters.iter().all(|n| !n.is_empty())
        },
    );
}
