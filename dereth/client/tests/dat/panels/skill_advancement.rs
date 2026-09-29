//! Skill and attribute advancement on the character page: a raise or train sends nothing until a
//! row is picked, then names the row and the amount (`0x0044 TrainAttribute2nd`,
//! `0x0045 TrainAttribute`, `0x0046 TrainSkill`, `0x0047 TrainSkillAdvancementClass`); the
//! recorded answers land with their own sequences, a replayed or out-of-order answer is refused,
//! and an answer moves the number on screen and re-enables the button.
//! Fixture: the retail dats and every recording in `fixtures/packet-captures/`, replayed into a
//! headless gameplay `App` with no socket; requests are read from `ClientNetwork`'s outgoing
//! queue and compared by opcode, body and queue. Requests and answers are paired by timestamp
//! window. The spellbook mask and two negative quality-update controls are synthetic.

use crate::common::captures_dir;
use crate::common::client_dir;

use std::collections::BTreeMap;
use std::net::SocketAddr;

use dereth_client::app::App;
use dereth_client::net::ClientNetwork;
use dereth_client::objects::ObjectStream;
use dereth_client_model::qualities::update as qupdate;
use dereth_client_net::client_session::SessionEvent;
use dereth_primitives::{LocalTime, ObjectId};
use dereth_protocol::Message as _;
use dereth_transport::wire::ParsedPacket;
use dereth_ui::{ElemHandle, ElementId};
use dereth_ui_screens::panels::{skills, statmgmt};

// ---------------------------------------------------------------------------------------------
// Harness: missing fixture inputs fail; the headless presentation retains real UI state.
// ---------------------------------------------------------------------------------------------

fn dat_store() -> dereth_dat::RetailDatStore {
    dereth_dat::RetailDatStore::open_dir(&client_dir()).unwrap_or_else(|e| {
        panic!(
            "the retail dats at {} are this test's oracle: {e}",
            client_dir().display()
        )
    })
}

fn app_in_gameplay(frames: u32) -> App {
    crate::common::app::app_in_gameplay(frames, Some(CAPTURE_PLAYER))
}

use crate::common::app::gameplay_screen;

use dereth_client_net::client_session::testing::capture::{self, Datagram as Record};

fn load(session: &str) -> Vec<Record> {
    capture::shared_session(session).to_vec()
}

fn connection_sequence_number(records: &[Record]) -> u32 {
    dereth_client_net::recording::connection_sequence_number(records)
        .expect("the capture has no LoginRequest")
}

fn addr(pair: u16) -> SocketAddr {
    capture::peer(pair)
}

/// Replay a capture's server half.
///
/// `stop_at_player_description` leaves the session **in the world** rather than driving it through
/// the recorded log-off, because a disconnected session refuses `send_action` and the wire half of
/// these tests need one that does not.
fn replay(session: &str, stop_at_player_description: bool) -> (ClientNetwork, Vec<SessionEvent>) {
    let records = load(session);
    let mut net = ClientNetwork::new(
        "127.0.0.1:19000",
        7304,
        "ac01",
        "pass",
        connection_sequence_number(&records),
    )
    .expect("the loopback host binds");
    let mut objects = ObjectStream::new();
    let mut events = Vec::new();
    let mut entered = false;
    for r in &records {
        let now = LocalTime(r.t);
        if !r.c2s {
            net.feed(&r.raw, addr(r.pair), now);
        }
        net.tick(now);
        let _ = net.take_outgoing();
        let mut seen_desc = false;
        for e in objects.pump(&mut net, now) {
            if let SessionEvent::CharacterSet(set) = &e {
                if !entered {
                    if let Some(c) = set.characters.first() {
                        let account = set.account.clone();
                        net.enter_world(c.gid, &account);
                        entered = true;
                    }
                }
            }
            seen_desc |= matches!(e, SessionEvent::PlayerDescription(_));
            events.push(e);
        }
        if stop_at_player_description && seen_desc {
            break;
        }
    }
    assert!(
        events
            .iter()
            .any(|e| matches!(e, SessionEvent::PlayerDescription(_))),
        "{session} never reached 0x0013, so it cannot be this test's oracle"
    );
    let _ = net.take_outgoing();
    (net, events)
}

fn events_in_world(events: &[SessionEvent]) -> &[SessionEvent] {
    let seen = events
        .iter()
        .position(|e| matches!(e, SessionEvent::PlayerDescription(_)))
        .expect("the capture reached 0x0013");
    let end = events[seen..]
        .iter()
        .position(|e| {
            matches!(
                e,
                SessionEvent::LoggedOff
                    | SessionEvent::StateChanged(
                        dereth_client_net::client_session::SessionState::CharacterSelect
                            | dereth_client_net::client_session::SessionState::Disconnected(_)
                    )
            )
        })
        .map_or(events.len(), |i| seen + i);
    &events[..end]
}

fn player_description(events: &[SessionEvent]) -> &dereth_protocol::login::LoginPlayerDescription {
    events
        .iter()
        .find_map(|e| match e {
            SessionEvent::PlayerDescription(d) => Some(&**d),
            _ => None,
        })
        .expect("the capture's 0x0013")
}

/// Adopt the identity carried by the capture and install its parked 0x0013 on that player row.
/// app_in_gameplay initially seeds the synthetic CAPTURE_PLAYER. If the capture identifies a
/// different player, set_player allocates its store and installs the parked description.
fn embody(app: &mut App) {
    let player = app.hud().player.unwrap_or(CAPTURE_PLAYER);
    let w = &mut app.objects_mut().world;
    if w.player == Some(player) {
        return;
    }
    w.player = None;
    w.tables
        .weenies
        .insert(player, dereth_client_model::weenie::Weenie::new(player));
    assert!(w.set_player(player), "the identity is adopted once");
}

fn app_with_capture(session: &str) -> (App, Vec<SessionEvent>) {
    let (net, events) = replay(session, false);
    let mut app = app_in_gameplay(4);
    // The capture's own endpoint, socket-free. It used to be dropped here; it is kept so that a
    // request this file drives can be observed where retail puts it — on the wire. See
    // [`wire_actions`].
    app.attach_replay_network(net)
        .expect("a headless app with no link takes the replay endpoint");
    let _ = app.apply_hud_events(events_in_world(&events));
    embody(&mut app);
    for _ in 0..4 {
        app.frame();
    }
    app.ui_mut()
        .expect("the UI shell is up")
        .ui
        .requests
        .clear();
    let _ = wire_actions(&mut app);
    (app, events)
}

/// Every game action `app` has put on the wire since this was last called, in send order.
///
/// **This is the seam that replaced `dereth_ui_screens::requests::take()` for the click-driven
/// assertions in this file** — see the note on
/// [`a_raise_answer_moves_the_number_and_re_enables_the_button`]. Order and multiplicity are
/// preserved: it returns a `Vec`, not a set or a count.
fn wire_actions(app: &mut App) -> Vec<SentAction> {
    let out = app
        .replay_network_mut()
        .expect("app_with_capture attaches the capture's own replay endpoint")
        .take_outgoing();
    actions_in(&out)
}

/// Read the public skill table 0x0E000004 and experience table 0x0E000018 directly from the
/// archive. This bypasses panel state but shares asset decoders; it cannot exclude every shared
/// mistake.
fn tables() -> (
    dereth_assets::tables::SkillTable,
    dereth_assets::tables::XpTable,
) {
    use dereth_assets::Decode;
    use dereth_primitives::AssetSource as _;
    let store = dat_store();
    let id = dereth_primitives::DataId(0x0E00_0004);
    let s = store.read(id).expect("the SkillTable is in the dats");
    let skills = dereth_assets::tables::SkillTable::decode_payload(id, &s).expect("SkillTable");
    let id = dereth_primitives::DataId(0x0E00_0018);
    let x = store.read(id).expect("the experience table is in the dats");
    let xp = dereth_assets::tables::XpTable::decode_payload(id, &x)
        .expect("the experience table decodes");
    (skills, xp)
}

/// Reconstruct qualities from the capture's 0x0013 using the shared description decoder.
fn capture_qualities(events: &[SessionEvent]) -> dereth_client_model::Qualities {
    let mut q = dereth_client_model::Qualities::default();
    q.apply_ac_qualities(&player_description(events).qualities, LocalTime(0.0));
    q
}

// ---------------------------------------------------------------------------------------------
// Wire helpers.
// ---------------------------------------------------------------------------------------------

/// One game action pulled back out of a datagram this process emitted.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SentAction {
    queue: u16,
    stamp: u32,
    opcode: u32,
    body: Vec<u8>,
}

/// Extract ordered-action headers and bodies from outgoing fragment payloads, preserving order.
/// Short payloads and nonmatching header magic are skipped; this helper does not make them
/// decode failures. It retains queue IDs for caller assertions rather than accepting a match
/// found by scanning arbitrary offsets within a body.
fn actions_in(datagrams: &[(Vec<u8>, SocketAddr)]) -> Vec<SentAction> {
    let mut out = Vec::new();
    for (raw, _) in datagrams {
        let p = ParsedPacket::parse(raw).expect("this process's own datagram parses");
        for f in &p.fragments {
            if f.payload.len() < dereth_protocol::OrderedActionHeader::PACK_SIZE + 4 {
                continue;
            }
            let magic = u32::from_le_bytes(f.payload[0..4].try_into().expect("4"));
            if magic != dereth_protocol::OrderedActionHeader::MAGIC {
                continue;
            }
            let stamp = u32::from_le_bytes(f.payload[4..8].try_into().expect("4"));
            let opcode = u32::from_le_bytes(f.payload[8..12].try_into().expect("4"));
            out.push(SentAction {
                queue: f.header.queue_id,
                stamp,
                opcode,
                body: f.payload[12..].to_vec(),
            });
        }
    }
    out
}

/// Synthetic player ID for fixture worlds. Player adoption supplies the quality store absent
/// from a bare object row. Subject-free UI requests use this owner; public-update controls
/// separately test the subject ID. Define it once so those fixture roles remain consistent.
const CAPTURE_PLAYER: dereth_primitives::ObjectId = dereth_primitives::ObjectId(0x5000_0001);

/// Run one frame of `interaction::use_time` with a real, in-world [`ClientNetwork`] attached.
///
/// This is the whole of links 3 and 4: the requests go in as [`dereth_ui_screens::view::UiRequest`]s
/// and come out as datagrams.
fn route_and_send(
    inter: &mut dereth_client::interaction::Interaction,
    store: &dereth_dat::RetailDatStore,
    objects: &mut ObjectStream,
    net: &mut ClientNetwork,
    // Install the supplied description on the World-owned player store, or explicitly clear
    // it. The None arm tests the sender's missing-description refusal rather than defaulting
    // to a manufactured quality record.
    player_desc: Option<&dereth_client_model::Qualities>,
    requests: Vec<dereth_ui_screens::view::UiRequest>,
) -> (Vec<SentAction>, Vec<dereth_ui_screens::view::UiRequest>) {
    match player_desc {
        Some(q) => objects.world.seed_player_desc(CAPTURE_PLAYER, q.clone()),
        None => objects.world.clear_player_desc(),
    }
    inter.queue(Vec::new(), requests);
    let (unowned, _) = dereth_client::interaction::use_time(
        inter,
        store,
        None,
        objects,
        Some(net),
        Vec::new(),
        player_desc.is_some(),
        (800, 600),
        LocalTime(1.0),
    );
    // Tick the transport after routing, matching the required request-before-datagram order.
    // Session::send_action queues the action; net.tick emits the datagram observed below.
    net.tick(LocalTime(1.0));
    (actions_in(&net.take_outgoing()), unowned)
}

// ---------------------------------------------------------------------------------------------
// 0. The round-trip oracle: what the retail client asked for, and what the server answered.
//
// Round-trip helpers read recorded messages; later routing and negative tests also synthesize inputs.
// ---------------------------------------------------------------------------------------------

/// The four train opcodes, in opcode order: `0x0044 TrainAttribute2nd`, `0x0045
/// TrainAttribute`, `0x0046 TrainSkill`, `0x0047 TrainSkillAdvancementClass`.
const TRAIN_OPCODES: [u32; 4] = [0x0044, 0x0045, 0x0046, 0x0047];

/// Every session on disk, discovered rather than named.
///
/// A fixed list of sessions could never see a **new** capture carrying a raise; a guard that
/// cannot see new data is not a guard.
fn corpus_sessions() -> Vec<String> {
    let mut sessions: Vec<String> = std::fs::read_dir(captures_dir())
        .expect("captures dir")
        .filter_map(|e| e.ok())
        .filter(|e| !crate::common::is_unclean_logout_recording(&e.path()))
        .filter_map(|e| {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some("jsonl") {
                return None;
            }
            p.file_stem().and_then(|x| x.to_str()).map(str::to_owned)
        })
        .collect();
    sessions.sort();
    assert!(
        !sessions.is_empty(),
        "no captures in {}",
        captures_dir().display()
    );
    sessions
}

/// One `[F7B1][stamp][opcode][body]` the **retail client** put on the wire, and when.
///
/// Only fragment 0 of a blob carries the `OrderedActionHeader`, so the index is tested rather than trusting
/// that no continuation fragment ever opens with `0xF7B1`.
fn captured_actions(session: &str) -> Vec<(f64, SentAction)> {
    let mut out = Vec::new();
    for r in load(session) {
        if !r.c2s {
            continue;
        }
        let Ok(p) = ParsedPacket::parse(&r.raw) else {
            continue;
        };
        for f in &p.fragments {
            if f.header.blob_num != 0
                || f.payload.len() < dereth_protocol::OrderedActionHeader::PACK_SIZE + 4
            {
                continue;
            }
            if u32::from_le_bytes(f.payload[0..4].try_into().expect("4"))
                != dereth_protocol::OrderedActionHeader::MAGIC
            {
                continue;
            }
            out.push((
                r.t,
                SentAction {
                    queue: f.header.queue_id,
                    stamp: u32::from_le_bytes(f.payload[4..8].try_into().expect("4")),
                    opcode: u32::from_le_bytes(f.payload[8..12].try_into().expect("4")),
                    body: f.payload[12..].to_vec(),
                },
            ));
        }
    }
    out
}

/// The retail client's own train and raise requests in one session, in the order it sent them.
fn captured_train_requests(session: &str) -> Vec<(f64, SentAction)> {
    captured_actions(session)
        .into_iter()
        .filter(|(_, a)| TRAIN_OPCODES.contains(&a.opcode))
        .collect()
}

/// `[id][amount]`, little-endian -- the body shape all four training messages share.
fn train_body(a: &SentAction) -> (u32, u32) {
    assert_eq!(
        a.body.len(),
        8,
        "a train request body is [id][amount]: {a:?}"
    );
    (
        u32::from_le_bytes(a.body[0..4].try_into().expect("4")),
        u32::from_le_bytes(a.body[4..8].try_into().expect("4")),
    )
}

/// Replay a capture's server half, keeping the capture time each `SessionEvent` emerged at.
///
/// [`replay`] is this with the times dropped; the round-trip oracle needs them, because pairing a
/// request with its answer is a question about *when*.
fn replay_timed(session: &str) -> Vec<(f64, SessionEvent)> {
    let records = load(session);
    let mut net = ClientNetwork::new(
        "127.0.0.1:19000",
        7304,
        "ac01",
        "pass",
        connection_sequence_number(&records),
    )
    .expect("the loopback host binds");
    let mut objects = ObjectStream::new();
    let mut events = Vec::new();
    let mut entered = false;
    for r in &records {
        let now = LocalTime(r.t);
        if !r.c2s {
            net.feed(&r.raw, addr(r.pair), now);
        }
        net.tick(now);
        let _ = net.take_outgoing();
        for e in objects.pump(&mut net, now) {
            if let SessionEvent::CharacterSet(set) = &e {
                if !entered {
                    if let Some(c) = set.characters.first() {
                        let account = set.account.clone();
                        net.enter_world(c.gid, &account);
                        entered = true;
                    }
                }
            }
            events.push((r.t, e));
        }
    }
    assert!(
        events
            .iter()
            .any(|(_, e)| matches!(e, SessionEvent::PlayerDescription(_))),
        "{session} never reached 0x0013, so it cannot be this test's oracle"
    );
    events
}

/// Maximum timestamp window used to pair replies, capped at the next request. The check requires
/// the worst latency to be under one fifth of this 0.5 s window (0.1 s). It does not correlate
/// request IDs.
const ANSWER_WINDOW_S: f64 = 0.5;

/// One captured round trip.
struct RoundTrip {
    /// Which capture, and which request within it, so a failure names the traffic.
    session: String,
    index: usize,
    when: f64,
    /// The retail client's own request.
    request: SentAction,
    /// Events inside this request's bounded timestamp window, in order.
    answers: Vec<SessionEvent>,
    /// The worst answer latency observed inside this trip, for the window's own report.
    latency: f64,
}

/// Split recorded traffic into request windows plus the prelude before the first request.
/// The prelude includes experience/credit updates needed before costing the first request.
/// Events outside each bounded answer window and before the next request are skipped here;
/// this is not a claim to replay every intervening world update into the arithmetic oracle.
fn round_trips(session: &str) -> (Vec<SessionEvent>, Vec<RoundTrip>) {
    let timed = replay_timed(session);
    let first_desc = timed
        .iter()
        .position(|(_, e)| matches!(e, SessionEvent::PlayerDescription(_)))
        .expect("the capture reached 0x0013");
    let last = timed[first_desc..]
        .iter()
        .position(|(_, e)| {
            matches!(
                e,
                SessionEvent::LoggedOff
                    | SessionEvent::StateChanged(
                        dereth_client_net::client_session::SessionState::CharacterSelect
                            | dereth_client_net::client_session::SessionState::Disconnected(_)
                    )
            )
        })
        .map_or(timed.len(), |i| first_desc + i);
    let timed = &timed[..last];

    let reqs = captured_train_requests(session);
    let mut trips = Vec::new();
    let mut prelude = Vec::new();
    let mut cursor = 0usize;
    for (i, (t, a)) in reqs.iter().enumerate() {
        while cursor < timed.len() && timed[cursor].0 < *t {
            if i == 0 {
                prelude.push(timed[cursor].1.clone());
            }
            cursor += 1;
        }
        let next = reqs.get(i + 1).map_or(f64::INFINITY, |(t, _)| *t);
        let close = next.min(t + ANSWER_WINDOW_S);
        let mut answers = Vec::new();
        let mut latency = 0.0f64;
        while cursor < timed.len() && timed[cursor].0 < close {
            // The latency reported is the *answer's*, not the window's: the world goes on around
            // the player inside it (heartbeat ints, regeneration ticks, other objects moving) and
            // those are carried along because the client sees them, but they say nothing about
            // how promptly the server answered.
            if let SessionEvent::UiEvent { opcode, blob } = &timed[cursor].1 {
                if qupdate::is_update_opcode(*opcode) {
                    if let Some(u) = qupdate::decode(*opcode, &blob[4..]) {
                        if u.subject.is_none() && qupdate::answers_a_raise_update(&u) {
                            latency = latency.max(timed[cursor].0 - t);
                        }
                    }
                }
            }
            answers.push(timed[cursor].1.clone());
            cursor += 1;
        }
        trips.push(RoundTrip {
            session: session.to_owned(),
            index: i,
            when: *t,
            request: a.clone(),
            answers,
            latency,
        });
        // Whatever is left before the next request is the world going on around the player --
        // heartbeat ints and regeneration ticks -- and belongs to no round trip.
        while cursor < timed.len() && timed[cursor].0 < next {
            cursor += 1;
        }
    }
    if reqs.is_empty() {
        prelude.extend(timed.iter().map(|(_, e)| e.clone()));
    }
    (prelude, trips)
}

/// Every session that carries at least one train or raise request, discovered.
fn sessions_with_train_traffic() -> Vec<String> {
    corpus_sessions()
        .into_iter()
        .filter(|s| !captured_train_requests(s).is_empty())
        .collect()
}

/// The private quality updates in one answer, decoded -- the server's own bodies.
fn decoded_answers(
    answers: &[SessionEvent],
) -> Vec<(dereth_protocol::Opcode, qupdate::QualityUpdate)> {
    answers
        .iter()
        .filter_map(|e| match e {
            SessionEvent::UiEvent { opcode, blob } if qupdate::is_update_opcode(*opcode) => {
                let u = qupdate::decode(*opcode, &blob[4..])
                    .unwrap_or_else(|| panic!("the capture's own {opcode:?} decodes"));
                (u.subject.is_none()).then_some((*opcode, u))
            }
            _ => None,
        })
        .collect()
}

/// The one answer on a given key, or `None`.
fn answer_on(
    answers: &[SessionEvent],
    t: dereth_client_model::StatType,
    property: u32,
) -> Option<(dereth_protocol::Opcode, qupdate::QualityUpdate)> {
    let key = dereth_client_model::StatKey::new(t, property);
    let mut hits = decoded_answers(answers)
        .into_iter()
        .filter(|(_, u)| u.key == key);
    let first = hits.next();
    assert!(
        hits.next().is_none(),
        "two answers on {t:?}/{property} in one round trip"
    );
    first
}

/// Apply captured descriptions and private quality updates to a separate qualities/gate pair.
/// This bypasses HUD and panel state, but uses the same qupdate::apply and sequence logic that
/// production ultimately uses. It is not an independent implementation of gate arithmetic.
fn apply_capture_events(
    q: &mut dereth_client_model::Qualities,
    stamper: &mut dereth_client_model::qualities::PropertySequenceGate,
    events: &[SessionEvent],
) {
    for e in events {
        match e {
            SessionEvent::PlayerDescription(d) => {
                q.apply_ac_qualities(&d.qualities, LocalTime(0.0));
            }
            SessionEvent::UiEvent { opcode, blob } if qupdate::is_update_opcode(*opcode) => {
                if let Some(u) = qupdate::decode(*opcode, &blob[4..]) {
                    if u.subject.is_none() {
                        let _ = qupdate::apply(q, stamper, &u);
                    }
                }
            }
            _ => {}
        }
    }
}

/// **The corpus carries the train and raise round trip, and this file is driven by all of it.**
///
/// Two things are asserted rather than described. **All four request forms are present**, so the
/// fraction this file proves against real traffic is four of four and can be checked rather than
/// claimed. And **every request the raw sweep finds is recovered as a round trip with an answer**,
/// for every session in the corpus -- which is what keeps the claim honest as the corpus grows: a
/// capture added later is swept, paired and asserted by the same discovered-sessions loop the
/// other tests run, so it cannot be silently uncovered.
///
/// **Falsified by** narrowing [`corpus_sessions`] to
/// `["first-login-walk-jump", "early-inventory-and-casting", "short-second-connection"]`:
/// `long-solo-play` and `short-play-with-training` vanish and the four-of-four assertion fails.
///
/// **One mutation is not detectable here:** dropping the
/// `blob_num == 0` test in [`captured_actions`] changes nothing, because no continuation fragment
/// anywhere in this corpus happens to open with `0xF7B1`. The test is a correctness guard the
/// present data cannot falsify, not a claim this file proves; it stays because a scan that reads
/// an `OrderedActionHeader` out of a fragment that has none is wrong whether or not today's captures catch
/// it.
#[test]
fn the_corpus_carries_the_train_and_raise_round_trip() {
    let mut by_op: BTreeMap<u32, u32> = BTreeMap::new();
    let mut by_session: BTreeMap<String, BTreeMap<u32, u32>> = BTreeMap::new();
    let mut other_c2s: BTreeMap<u32, u32> = BTreeMap::new();
    for session in corpus_sessions() {
        for (_, a) in captured_actions(&session) {
            if TRAIN_OPCODES.contains(&a.opcode) {
                *by_op.entry(a.opcode).or_default() += 1;
                *by_session
                    .entry(session.clone())
                    .or_default()
                    .entry(a.opcode)
                    .or_default() += 1;
            } else {
                *other_c2s.entry(a.opcode).or_default() += 1;
            }
        }
    }
    assert!(
        !other_c2s.is_empty(),
        "the sweep found no game actions at all, so it is not working"
    );
    eprintln!("corpus sweep: train/raise requests by session {by_session:#X?}");
    eprintln!("corpus sweep: other c2s game actions {other_c2s:#X?}");

    for op in TRAIN_OPCODES {
        assert!(
            by_op.get(&op).copied().unwrap_or(0) > 0,
            "no capture carries {op:#06X}; the oracle for that form would have to be synthetic \
             again. Census: {by_session:#X?}"
        );
    }
    let total: u32 = by_op.values().sum();
    assert!(
        total >= 21,
        "the corpus carries at least 21 train/raise requests and now carries \
         {total}: captures have been lost, not added. Census: {by_session:#X?}"
    );

    // Every swept request is recovered as a round trip, and every round trip has an answer the
    // stat-management panel redraws itself for. This is what makes the discovered-sessions loop in
    // the tests below a complete cover of the corpus rather than a sample of it.
    let sessions = sessions_with_train_traffic();
    assert_eq!(
        sessions,
        by_session.keys().cloned().collect::<Vec<_>>(),
        "the discovered set and the swept set are the same sessions"
    );
    let mut worst = 0.0f64;
    for session in &sessions {
        let swept = captured_train_requests(session).len();
        let (_, trips) = round_trips(session);
        assert_eq!(
            trips.len(),
            swept,
            "{session}: every swept request became a round trip"
        );
        for trip in &trips {
            let decoded = decoded_answers(&trip.answers);
            assert!(
                decoded
                    .iter()
                    .any(|(_, u)| qupdate::answers_a_raise_update(u)),
                "{}[{}] {:#06X} at t={:.3} got no answer the panel redraws for: {decoded:#?}",
                trip.session,
                trip.index,
                trip.request.opcode,
                trip.when
            );
            // Require the recorded queue for every train form to match the public Weenie queue.
            assert_eq!(
                u16::from(dereth_client_net::queues::Queue::Weenie as u8),
                trip.request.queue,
                "{}[{}] rode the Weenie queue",
                trip.session,
                trip.index
            );
            assert_eq!(
                dereth_protocol::Opcode(trip.request.opcode)
                    .info()
                    .and_then(|i| i.send_queue),
                Some(dereth_primitives::NetQueue::Weenie),
                "and that is the queue `dereth_protocol`'s own catalogue gives the opcode"
            );
            worst = worst.max(trip.latency);
        }
        // `OrderedActionHeader` stamps come off one global counter, so the retail client's own train requests
        // are strictly increasing in the order it sent them. That is the property
        // `Session::next_action_stamp` reproduces, asserted here against the recording.
        let stamps: Vec<u32> = trips.iter().map(|t| t.request.stamp).collect();
        for w in stamps.windows(2) {
            assert!(
                w[1] > w[0],
                "{session}: the retail client's stamps advance: {stamps:?}"
            );
        }
    }
    assert!(
        worst < ANSWER_WINDOW_S / 5.0,
        "the answer window is doing work rather than being slack: worst latency {worst:.3}s"
    );
    eprintln!(
        "{total} round trips over {} sessions, worst answer latency {worst:.3}s",
        sessions.len()
    );
}

/// Recompute costs and reproduce each captured train/raise request's opcode, body and queue.
/// State begins with the prelude and advances through each retained answer window. It does not
/// include events skipped between windows. Action stamps are tested separately; this station
/// does not require recomposed stamp bytes to equal the recording's global counter.
///
/// The five cost cases are skill raise-one/raise-ten, attribute raise-one/raise-ten and the
/// public skill record's trained-credit cost. The attribute increment is inferred from matching
/// skill behaviour; short-play-with-training contains raise-ten then raise-one requests for the same attribute, giving traffic
/// evidence for both increments.
///
/// This station sends the attribute pair through interaction::send_request directly. Attribute
/// UI emitters and routing exist, but this branch tests only the sender seam.
/// Skill cases traverse the UiRequest route. Recorded spending is compared with one or ten
/// levels, not used to infer which button gesture occurred.
///
/// **Falsified by** changing +1 to +2 in advancement::attribute_cost_to_raise, subtracting the
/// previous threshold instead of spent experience for skills, or removing R::TrainSkill from
/// send_request.
#[test]
fn every_captured_request_asks_for_the_amount_this_client_would_have_asked_for() {
    use dereth_client_model::{StatKey, StatType, StatValue};
    use dereth_ui_screens::view::UiRequest;

    let (skill_table, xp_table) = tables();
    let store = dat_store();
    let mut checked: BTreeMap<u32, u32> = BTreeMap::new();

    for session in sessions_with_train_traffic() {
        let (prelude, trips) = round_trips(&session);
        // A live, in-world link to recompose the requests through. It is this process's own
        // loopback session: nothing is sent to anything.
        let (mut net, _events) = replay(&session, true);
        let mut objects = ObjectStream::new();
        let mut inter = dereth_client::interaction::Interaction::new();

        let mut q = dereth_client_model::Qualities::default();
        let mut stamper = dereth_client_model::qualities::PropertySequenceGate::default();
        apply_capture_events(&mut q, &mut stamper, &prelude);

        for trip in &trips {
            let (id, amount) = train_body(&trip.request);
            let what = format!(
                "{}[{}] {:#06X} id={id} amount={amount} at t={:.3}",
                trip.session, trip.index, trip.request.opcode, trip.when
            );
            match trip.request.opcode {
                // ---- 0x0046 Train_TrainSkill: raise-1 or raise-10, the same message -----------
                0x0046 => {
                    let before = *q
                        .skill(id)
                        .unwrap_or_else(|| panic!("{what}: no such skill"));
                    assert!(
                        before.sac >= 2,
                        "{what}: the retail client raised a trained skill"
                    );
                    let one = dereth_client_model::advancement::skill_cost_to_raise(
                        &q,
                        &skill_table,
                        &xp_table,
                        id,
                    );
                    let ten =
                        dereth_client_model::advancement::skill_cost_to_raise_10(&q, &xp_table, id);
                    assert!(
                        amount == one || amount == ten,
                        "{what}: the retail client asked for {amount}; the single-level cost is {one} \
                         and the ten-level cost is {ten}. before={before:?}"
                    );
                    let (sent, unowned) = route_and_send(
                        &mut inter,
                        &store,
                        &mut objects,
                        &mut net,
                        Some(&q),
                        vec![UiRequest::TrainSkill {
                            skill: id,
                            xp: amount,
                        }],
                    );
                    assert!(
                        unowned.is_empty(),
                        "{what}: the raise is nobody's: {unowned:?}"
                    );
                    assert_eq!(sent.len(), 1, "{what}: one game action, got {sent:?}");
                    assert_eq!(sent[0].opcode, trip.request.opcode, "{what}: same opcode");
                    assert_eq!(
                        sent[0].body, trip.request.body,
                        "{what}: same body, byte for byte"
                    );
                    assert_eq!(sent[0].queue, trip.request.queue, "{what}: same net queue");
                }
                // ---- 0x0047 Train_TrainSkillAdvancementClass: credits, not experience ---------
                0x0047 => {
                    let before = *q
                        .skill(id)
                        .unwrap_or_else(|| panic!("{what}: no such skill"));
                    assert!(
                        before.sac <= 1,
                        "{what}: the retail client trained an untrained skill"
                    );
                    let cost = u32::try_from(
                        skill_table
                            .skills
                            .get(&id)
                            .unwrap_or_else(|| panic!("{what}: not in the SkillTable"))
                            .trained_cost,
                    )
                    .expect("a non-negative cost");
                    assert_eq!(
                        amount, cost,
                        "{what}: SkillBase::_trained_cost is the credit price. before={before:?}"
                    );
                    let (sent, unowned) = route_and_send(
                        &mut inter,
                        &store,
                        &mut objects,
                        &mut net,
                        Some(&q),
                        vec![UiRequest::TrainSkillAdvancementClass {
                            skill: id,
                            credits: amount,
                        }],
                    );
                    assert!(unowned.is_empty(), "{what}: {unowned:?}");
                    assert_eq!(sent.len(), 1, "{what}: one game action, got {sent:?}");
                    assert_eq!(sent[0].opcode, trip.request.opcode);
                    assert_eq!(
                        sent[0].body, trip.request.body,
                        "{what}: same body, byte for byte"
                    );
                    assert_eq!(sent[0].queue, trip.request.queue);
                }
                // ---- 0x0045 / 0x0044: the attribute pair -------------------------------------
                op @ (0x0045 | 0x0044) => {
                    let vital = op == 0x0044;
                    let (level_from_cp, cp_spent) = if vital {
                        let v = q
                            .attribute_2nd(id)
                            .unwrap_or_else(|| panic!("{what}: no such vital"));
                        (v.attribute.level_from_cp, v.attribute.cp_spent)
                    } else {
                        let a = q
                            .attribute(id)
                            .unwrap_or_else(|| panic!("{what}: no such attribute"));
                        (a.level_from_cp, a.cp_spent)
                    };
                    let one = dereth_client_model::advancement::attribute_cost_to_raise(
                        &xp_table,
                        level_from_cp,
                        cp_spent,
                        vital,
                    );
                    let ten = dereth_client_model::advancement::attribute_cost_to_raise_10(
                        &xp_table,
                        level_from_cp,
                        cp_spent,
                        vital,
                    );
                    assert!(
                        amount == one || amount == ten,
                        "{what}: the retail client asked for {amount}; the single-level cost is {one} \
                         and the ten-level cost is {ten}. level_from_cp={level_from_cp} \
                         cp_spent={cp_spent}. This checks the inferred attribute increment."
                    );
                    let req = if vital {
                        dereth_client_model::Request::TrainAttribute2nd(
                            dereth_protocol::admin::TrainAttribute2nd {
                                vital_id: id,
                                xp_spent: amount,
                            },
                        )
                    } else {
                        dereth_client_model::Request::TrainAttribute(
                            dereth_protocol::admin::TrainAttribute {
                                attribute_id: id,
                                xp_spent: amount,
                            },
                        )
                    };
                    assert!(
                        dereth_client::interaction::send_request(&mut net.session, &req),
                        "{what}: the sender arm accepted it"
                    );
                    net.tick(LocalTime(1.0));
                    let sent = actions_in(&net.take_outgoing());
                    assert_eq!(sent.len(), 1, "{what}: one game action, got {sent:?}");
                    assert_eq!(sent[0].opcode, op);
                    assert_eq!(
                        sent[0].body, trip.request.body,
                        "{what}: same body, byte for byte"
                    );
                    assert_eq!(sent[0].queue, trip.request.queue);
                }
                other => panic!("{what}: {other:#06X} is not a training opcode"),
            }
            *checked.entry(trip.request.opcode).or_default() += 1;

            // The server's own answer to *this* request is what advances the state the next one is
            // costed against, so it is applied here and nothing is guessed forward.
            let available_before = q.get(StatKey::new(StatType::Int64, 2));
            let credits_before = q.get(StatKey::new(StatType::Int, 0x18));
            apply_capture_events(&mut q, &mut stamper, &trip.answers);
            let available_after = q.get(StatKey::new(StatType::Int64, 2));
            let credits_after = q.get(StatKey::new(StatType::Int, 0x18));
            if trip.request.opcode == 0x0047 {
                assert_eq!(
                    available_before, available_after,
                    "{what}: training spends credits, not experience"
                );
                let (Some(StatValue::Int(before)), Some(StatValue::Int(after))) =
                    (credits_before, credits_after)
                else {
                    panic!("{what}: the capture has no AvailableSkillCredits to deduct from")
                };
                assert_eq!(
                    before - after,
                    i32::try_from(amount).expect("a small credit price"),
                    "{what}: AvailableSkillCredits fell by exactly the credits asked for"
                );
            } else {
                let (Some(StatValue::Int64(before)), Some(StatValue::Int64(after))) =
                    (available_before, available_after)
                else {
                    panic!("{what}: the capture has no AvailableExperience to deduct from")
                };
                assert_eq!(
                    before - after,
                    i64::from(amount),
                    "{what}: AvailableExperience fell by exactly what was asked for"
                );
                assert_eq!(
                    credits_before, credits_after,
                    "{what}: a raise spends experience, not credits"
                );
            }
        }
    }

    for op in TRAIN_OPCODES {
        assert!(
            checked.get(&op).copied().unwrap_or(0) > 0,
            "{op:#06X} was never exercised"
        );
    }
    eprintln!("requests recomposed and costed against the capture: {checked:#X?}");
}

/// Compare recorded answers with World-owned player qualities and the same object's stamps.
/// For every paired trip, require exact stored records, experience/credit arithmetic and no
/// new stale or unstorable outcomes. A raise adds spent experience and advances level; training
/// moves advancement class from untrained to trained with zero spent experience and rank.
///
/// The captured counter walks include AvailableExperience (Int64/2) and repeated skill raises in
/// long-solo-play. The HUD dispatches updates and counts outcomes; it owns no separate
/// qualities or stamper copy.
///
/// **Falsified by** deleting the `stamper.update(...)` guard from
/// `dereth_client_model::qualities::update::apply` (the stamps stop being consumed and the last-writer
/// check on the walked properties fails) and by removing `StatType::Skill` from `Qualities::set`.
#[test]
fn the_captured_answers_land_with_the_captures_own_sequences() {
    use dereth_client_model::{StatKey, StatType, StatValue};

    let mut forms: BTreeMap<u32, u32> = BTreeMap::new();
    for session in sessions_with_train_traffic() {
        let (prelude, trips) = round_trips(&session);
        let mut app = app_in_gameplay(4);
        let _ = app.apply_hud_events(&prelude);
        app.frame();
        assert!(
            app.objects().world.player_qualities().is_some(),
            "{session}: the prelude carries the 0x0013 this test reads through"
        );

        for trip in &trips {
            let (id, amount) = train_body(&trip.request);
            let what = format!(
                "{}[{}] {:#06X} id={id} amount={amount} at t={:.3}",
                trip.session, trip.index, trip.request.opcode, trip.when
            );
            let q0 = app
                .objects()
                .world
                .player_qualities()
                .expect("the player description")
                .clone();
            let stale_before = app.hud().stats.quality_updates_stale;
            let unstorable_before = app.hud().stats.quality_updates_unstorable;

            let _ = app.apply_hud_events(&trip.answers);
            app.frame();
            let q1 = app
                .objects()
                .world
                .player_qualities()
                .expect("the player description")
                .clone();
            assert_eq!(
                app.hud().stats.quality_updates_stale,
                stale_before,
                "{what}: a live server's own stream is in order"
            );
            assert_eq!(
                app.hud().stats.quality_updates_unstorable,
                unstorable_before,
                "{what}: every answer had somewhere to land"
            );

            match trip.request.opcode {
                0x0046 | 0x0047 => {
                    let (op, u) = answer_on(&trip.answers, StatType::Skill, id)
                        .unwrap_or_else(|| panic!("{what}: no skill answer"));
                    let StatValue::Skill(server) = u.value else {
                        panic!("{what}: {op:?} is not the whole-record form")
                    };
                    let before = *q0.skill(id).expect("the skill before");
                    let got = *q1.skill(id).expect("the skill after");
                    assert_eq!(
                        got, server,
                        "{what}: the stored record is the server's, exactly"
                    );
                    if trip.request.opcode == 0x0046 {
                        assert_eq!(
                            got.pp,
                            before.pp + amount,
                            "{what}: exactly the experience asked for went into _pp"
                        );
                        assert!(
                            got.level_from_pp > before.level_from_pp,
                            "{what}: {} -> {}",
                            before.level_from_pp,
                            got.level_from_pp
                        );
                        assert_eq!(got.sac, before.sac, "{what}: a raise does not change _sac");
                    } else {
                        assert_eq!(before.sac, 1, "{what}: untrained before");
                        assert_eq!(got.sac, 2, "{what}: trained after");
                        assert_eq!(got.pp, 0, "{what}: training buys the grade, not a level");
                        assert_eq!(got.level_from_pp, 0);
                    }
                    assert_eq!(
                        app.objects()
                            .world
                            .weenie(app.objects().world.player.expect("a player"))
                            .expect("the player row")
                            .stamper
                            .as_ref()
                            .expect("the stamper was set up")
                            .stamp(StatKey::new(StatType::Skill, id).0),
                        Some(u.sequence),
                        "{what}: the shared skill counter holds the capture's own sequence"
                    );
                }
                0x0045 => {
                    let (_, u) = answer_on(&trip.answers, StatType::Attribute, id)
                        .unwrap_or_else(|| panic!("{what}: no attribute answer"));
                    let StatValue::Attribute(server) = u.value else {
                        panic!("{what}: not the whole-record form")
                    };
                    let before = q0.attribute(id).expect("the attribute before");
                    let got = q1.attribute(id).expect("the attribute after");
                    assert_eq!(
                        got, server,
                        "{what}: the stored record is the server's, exactly"
                    );
                    assert_eq!(
                        got.cp_spent,
                        before.cp_spent + amount,
                        "{what}: exactly the experience asked for went into _cp_spent"
                    );
                    assert!(
                        got.level_from_cp > before.level_from_cp,
                        "{what}: the level moved"
                    );
                    assert_eq!(
                        got.init_level, before.init_level,
                        "{what}: the innate does not"
                    );
                    assert_eq!(
                        app.objects()
                            .world
                            .weenie(app.objects().world.player.expect("a player"))
                            .expect("the player row")
                            .stamper
                            .as_ref()
                            .expect("the stamper was set up")
                            .stamp(StatKey::new(StatType::Attribute, id).0),
                        Some(u.sequence)
                    );
                }
                0x0044 => {
                    let (op, u) = answer_on(&trip.answers, StatType::Attribute2nd, id)
                        .unwrap_or_else(|| panic!("{what}: no vital answer"));
                    let StatValue::Attribute2nd(server) = u.value else {
                        panic!("{what}: {op:?} is not the whole-record form")
                    };
                    let before = q0.attribute_2nd(id).expect("the vital before");
                    let got = q1.attribute_2nd(id).expect("the vital after");
                    // A vital's maximum id and its current id (1/2, 3/4, 5/6) name one record, so
                    // the record the answer window leaves behind is the last writer on *either*
                    // id, not the raise's own answer: a regeneration tick (`0x02E9`, current level
                    // only) on the paired id can land after the `0x02E7` inside the same window
                    // (post-relog-attribute-training does, 83 ms later). Walk the window's own
                    // updates in arrival order over the pre-window record.
                    let pair = if id % 2 == 1 { id + 1 } else { id - 1 };
                    let mut want = before;
                    let mut ticks_after_record = 0_u32;
                    let mut seen_record = false;
                    for (_, w) in decoded_answers(&trip.answers) {
                        if w.key != StatKey::new(StatType::Attribute2nd, id)
                            && w.key != StatKey::new(StatType::Attribute2nd, pair)
                        {
                            continue;
                        }
                        match w.value {
                            StatValue::Attribute2nd(r) => {
                                want = r;
                                seen_record = true;
                            }
                            StatValue::Attribute2ndLevel(n) => {
                                want.current_level = n;
                                ticks_after_record += u32::from(seen_record);
                            }
                            ref other => panic!("{what}: {other:?} on a vital key"),
                        }
                    }
                    assert!(
                        seen_record,
                        "{what}: the window carries the whole-record answer"
                    );
                    assert_eq!(
                        got, want,
                        "{what}: the stored record is the last writer's on either vital id \
                         ({ticks_after_record} current-level tick(s) after the server's record)"
                    );
                    assert_eq!(
                        got.attribute, server.attribute,
                        "{what}: everything but the current level is the server's record, exactly"
                    );
                    assert_eq!(
                        got.attribute.cp_spent,
                        before.attribute.cp_spent + amount,
                        "{what}: exactly the experience asked for went into _cp_spent"
                    );
                    assert!(
                        got.attribute.level_from_cp > before.attribute.level_from_cp,
                        "{what}: the level moved"
                    );
                    assert_eq!(
                        app.objects()
                            .world
                            .weenie(app.objects().world.player.expect("a player"))
                            .expect("the player row")
                            .stamper
                            .as_ref()
                            .expect("the stamper was set up")
                            .stamp(StatKey::new(StatType::Attribute2nd, id).0),
                        Some(u.sequence)
                    );
                }
                other => panic!("{what}: {other:#06X}"),
            }

            // The currency half, and its own counter.
            if trip.request.opcode == 0x0047 {
                let (_, u) = answer_on(&trip.answers, StatType::Int, 0x18)
                    .unwrap_or_else(|| panic!("{what}: no AvailableSkillCredits answer"));
                assert_eq!(
                    q1.get(StatKey::new(StatType::Int, 0x18)),
                    Some(u.value.clone()),
                    "{what}: the credit count on screen is the server's"
                );
                assert_eq!(
                    app.objects()
                        .world
                        .weenie(app.objects().world.player.expect("a player"))
                        .expect("the player row")
                        .stamper
                        .as_ref()
                        .expect("the stamper was set up")
                        .stamp(StatKey::new(StatType::Int, 0x18).0),
                    Some(u.sequence)
                );
                assert!(
                    answer_on(&trip.answers, StatType::Int64, 2).is_none(),
                    "{what}: training does not touch AvailableExperience"
                );
            } else {
                let (_, u) = answer_on(&trip.answers, StatType::Int64, 2)
                    .unwrap_or_else(|| panic!("{what}: no AvailableExperience answer"));
                let (StatValue::Int64(after), Some(StatValue::Int64(before))) =
                    (u.value.clone(), q0.get(StatKey::new(StatType::Int64, 2)))
                else {
                    panic!("{what}: AvailableExperience is not an Int64")
                };
                assert_eq!(
                    before - after,
                    i64::from(amount),
                    "{what}: the deduction the server made is the amount that was asked for"
                );
                assert_eq!(
                    q1.get(StatKey::new(StatType::Int64, 2)),
                    Some(StatValue::Int64(after)),
                    "{what}: and it is what the footer's line two will read"
                );
                assert_eq!(
                    app.objects()
                        .world
                        .weenie(app.objects().world.player.expect("a player"))
                        .expect("the player row")
                        .stamper
                        .as_ref()
                        .expect("the stamper was set up")
                        .stamp(StatKey::new(StatType::Int64, 2).0),
                    Some(u.sequence),
                    "{what}: AvailableExperience's counter is the capture's own"
                );
                assert!(
                    answer_on(&trip.answers, StatType::Int64, 1).is_none(),
                    "{what}: a raise moves AvailableExperience and leaves TotalExperience alone"
                );
            }
            *forms.entry(trip.request.opcode).or_default() += 1;
        }
    }
    for op in TRAIN_OPCODES {
        assert!(
            forms.get(&op).copied().unwrap_or(0) > 0,
            "{op:#06X} has no captured answer"
        );
    }
    eprintln!("captured answers applied and checked: {forms:#X?}");
}

/// Behaviour: advancement.answer.an-out-of-order-or-replayed-answer-is-refused
/// **A sequence the capture has already used is refused, using the capture's own bytes.**
///
/// `long-solo-play` raises one skill five
/// times in a row, so `Qualities_PrivateUpdateSkill` on that property walks sequences 0..5 with a
/// different `Skill` body at each step. Replaying an earlier one after a later one is therefore a
/// **real** stale update, and the gate must drop it and leave the number where the server put it.
///
/// **Falsified by** deleting the `stamper.update(...)` guard from
/// `dereth_client_model::qualities::update::apply`: the replayed body lands and the level walks backwards.
#[test]
fn a_replayed_captured_answer_is_refused_as_stale() {
    use dereth_client_model::{StatKey, StatType, StatValue};

    let mut proved = 0u32;
    for session in sessions_with_train_traffic() {
        let (prelude, trips) = round_trips(&session);
        // A property the capture updates more than once, and its answers in order.
        let mut history: BTreeMap<u32, Vec<(usize, qupdate::QualityUpdate)>> = BTreeMap::new();
        for trip in &trips {
            for (_, u) in decoded_answers(&trip.answers) {
                if matches!(u.value, StatValue::Skill(_)) {
                    history
                        .entry(u.key.property())
                        .or_default()
                        .push((trip.index, u));
                }
            }
        }
        let Some((property, walk)) = history.into_iter().find(|(_, v)| v.len() >= 2) else {
            continue;
        };
        // The gate is a wrap comparison, so "earlier" must be earlier by sequence and not only by
        // arrival: assert that before using it as a stale message.
        let (first_i, first) = walk.first().expect("two or more").clone();
        let (last_i, last) = walk.last().expect("two or more").clone();
        assert!(
            last.sequence > first.sequence,
            "{session}: skill {property}'s sequences must walk for this to be a stale replay: \
             {} then {}",
            first.sequence,
            last.sequence
        );
        assert_ne!(
            first.value, last.value,
            "{session}: and the two bodies must differ"
        );

        let mut app = app_in_gameplay(4);
        let _ = app.apply_hud_events(&prelude);
        for trip in &trips {
            let _ = app.apply_hud_events(&trip.answers);
        }
        app.frame();

        let settled = *app
            .objects()
            .world
            .player_qualities()
            .expect("the player description")
            .skill(property)
            .expect("the skill");
        let StatValue::Skill(want) = last.value.clone() else {
            unreachable!()
        };
        assert_eq!(
            settled, want,
            "{session}: the last answer the server sent is the one on screen"
        );
        assert_eq!(
            app.objects()
                .world
                .weenie(app.objects().world.player.expect("a player"))
                .expect("the player row")
                .stamper
                .as_ref()
                .expect("the stamper was set up")
                .stamp(StatKey::new(StatType::Skill, property).0),
            Some(last.sequence),
            "{session}: the counter is at the last sequence"
        );
        let stale_before = app.hud().stats.quality_updates_stale;

        // Now replay round trip `first_i`'s own answer -- the capture's bytes, unmodified.
        let earlier = trips[first_i]
            .answers
            .iter()
            .filter(|e| match e {
                SessionEvent::UiEvent { opcode, blob } if qupdate::is_update_opcode(*opcode) => {
                    qupdate::decode(*opcode, &blob[4..]).is_some_and(|u| {
                        u.key == first.key && matches!(u.value, StatValue::Skill(_))
                    })
                }
                _ => false,
            })
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            earlier.len(),
            1,
            "{session}: one earlier answer on skill {property}"
        );
        let _ = app.apply_hud_events(&earlier);
        app.frame();

        assert_eq!(
            *app.objects()
                .world
                .player_qualities()
                .expect("the player description")
                .skill(property)
                .expect("the skill"),
            want,
            "{session}: round trip {first_i}'s answer replayed after round trip {last_i}'s is \
             stale under the half-range window and must not move the number"
        );
        assert_eq!(
            app.hud().stats.quality_updates_stale,
            stale_before + 1,
            "{session}: and the rejection is counted rather than being silent"
        );
        proved += 1;
    }
    assert!(
        proved > 0,
        "no capture repeats a skill property, so the gate is unproven by traffic"
    );
}

// ---------------------------------------------------------------------------------------------
// 1. Links 3 and 4 — the request is routed and becomes bytes.
// ---------------------------------------------------------------------------------------------

/// Behaviour: advancement.raise.a-raise-sends-nothing-until-a-row-is-picked-and-then-names-it-and-the-cost
/// Route a constructed skill raise using a cost computed from the capture's initial qualities.
/// This tests links 3/4, not an actual footer click or a matching recorded request in this station.
/// Require both ownership and one emitted action, then verify opcode,
/// body, queue and counter advancement. Raise-ten uses the same opcode with a different amount.
/// The separate round-trip and footer tests supply recorded-spend and displayed-cost checks.
///
/// **Falsified by** removing the `UiRequest::TrainSkill` arm from `run_ui_requests` (the request
/// goes back to `unowned` and no datagram appears) or the `R::TrainSkill` arm from `send_request`
/// (the request is routed and then refused, and the datagram list is empty). Both were run.
#[test]
fn a_raise_leaves_the_panel_and_lands_on_the_wire_as_0x0046() {
    use dereth_ui_screens::view::UiRequest;

    let (mut net, events) = replay("first-login-walk-jump", true);
    let (skill_table, xp_table) = tables();
    let q = capture_qualities(&events);
    let store = dat_store();
    let mut objects = ObjectStream::new();
    let mut inter = dereth_client::interaction::Interaction::new();

    // The capture's own first trained-or-specialised skill, and its own cost to raise.
    let skills = player_description(&events)
        .qualities
        .skills
        .as_ref()
        .expect("a skill table");
    let skill = skills
        .entries
        .iter()
        .find(|(_, s)| s.sac >= 2)
        .map(|(id, _)| *id)
        .expect("the capture has a trained skill");
    let want_xp =
        dereth_client_model::advancement::skill_cost_to_raise(&q, &skill_table, &xp_table, skill);
    let want_xp_10 = dereth_client_model::advancement::skill_cost_to_raise_10(&q, &xp_table, skill);
    assert!(
        want_xp > 0,
        "a trained skill below its cap costs something to raise"
    );
    assert!(
        want_xp_10 >= want_xp,
        "ten levels cost at least as much as one"
    );

    let stamp_before = net.session.next_action_stamp();
    let (sent, unowned) = route_and_send(
        &mut inter,
        &store,
        &mut objects,
        &mut net,
        Some(&q),
        vec![UiRequest::TrainSkill { skill, xp: want_xp }],
    );

    assert!(
        unowned.is_empty(),
        "link 3: the raise is no longer a request nobody owns: {unowned:?}"
    );
    assert_eq!(
        inter.stats.requests_sent, 1,
        "one request reached the session"
    );
    assert_eq!(inter.stats.requests_undeliverable, 0);
    assert_eq!(inter.stats.requests_refused, 0);

    assert_eq!(
        sent.len(),
        1,
        "link 4: exactly one game action on the wire, got {sent:?}"
    );
    let a = &sent[0];
    assert_eq!(a.opcode, 0x0046, "Train_TrainSkill");
    assert_eq!(
        a.opcode,
        <dereth_protocol::admin::TrainSkill as dereth_protocol::Message>::OPCODE.0,
        "and it is the opcode `dereth_protocol` gives the message, not a literal typed twice"
    );
    // `[skillId][xpSpent]`, little-endian, and the xp is the footer's own number.
    let mut want = Vec::new();
    want.extend_from_slice(&skill.to_le_bytes());
    want.extend_from_slice(&want_xp.to_le_bytes());
    assert_eq!(
        a.body, want,
        "the body carries skill {skill} and {want_xp} experience"
    );
    assert_eq!(
        a.stamp, stamp_before,
        "the action-order header stamp came from the one global game-action counter"
    );
    assert_eq!(
        net.session.next_action_stamp(),
        stamp_before + 1,
        "and the counter advanced exactly once"
    );

    // Raise-10 is the **same message** with the other amount — there is no "raise ten" opcode.
    let (sent, _) = route_and_send(
        &mut inter,
        &store,
        &mut objects,
        &mut net,
        Some(&q),
        vec![UiRequest::TrainSkill {
            skill,
            xp: want_xp_10,
        }],
    );
    assert_eq!(sent.len(), 1);
    assert_eq!(
        sent[0].opcode, 0x0046,
        "raising ten levels also sends 0x0046"
    );
    let mut want10 = Vec::new();
    want10.extend_from_slice(&skill.to_le_bytes());
    want10.extend_from_slice(&want_xp_10.to_le_bytes());
    assert_eq!(sent[0].body, want10);
    // Both actions use queue 3, the public Weenie queue, and agree with the opcode catalogue.
    assert_eq!(
        dereth_protocol::Opcode(0x0046)
            .info()
            .and_then(|i| i.send_queue),
        Some(dereth_primitives::NetQueue::Weenie),
        "Train_TrainSkill is catalogued as a Weenie-queue send"
    );
    assert_eq!(
        u16::from(dereth_client_net::queues::Queue::Weenie as u8),
        sent[0].queue,
        "and that is the queue the fragment header carries"
    );
    assert_eq!(
        a.queue, sent[0].queue,
        "both raises are on the same net queue"
    );
}

/// Behaviour: advancement.train.the-train-request-reaches-the-wire
/// Training emits 0x0047; routed skill senders re-read advancement class before sending.
/// A class above untrained can be raised, while untrained or lower can be trained. Require each
/// sender to refuse the other's category, and require missing player qualities to refuse a
/// raise. The refusal counters distinguish rejected requests from silently unowned requests.
///
/// **Falsified by** deleting the `sac` test from `dereth_client_model::advancement::send_train_skill`: the
/// untrained skill then puts a `0x0046` on the wire and the `refused` count drops to zero.
#[test]
fn the_train_request_is_0x0047_and_the_two_gates_refuse_each_others_work() {
    use dereth_ui_screens::view::UiRequest;

    let (mut net, events) = replay("first-login-walk-jump", true);
    let (skill_table, _xp) = tables();
    let q = capture_qualities(&events);
    let store = dat_store();
    let mut objects = ObjectStream::new();
    let mut inter = dereth_client::interaction::Interaction::new();

    let untrained = skill_table
        .skills
        .keys()
        .copied()
        .find(|id| q.skill(*id).is_none_or(|s| s.sac <= 1))
        .expect("the capture has an untrained skill");
    let credits =
        u32::try_from(skill_table.skills[&untrained].trained_cost).expect("a non-negative cost");
    let trained = q
        .skills
        .as_ref()
        .expect("skills")
        .iter()
        .find(|(_, s)| s.sac >= 2)
        .map(|(id, _)| *id)
        .expect("the capture has a trained skill");

    // The train request goes out, in **credits**.
    let (sent, unowned) = route_and_send(
        &mut inter,
        &store,
        &mut objects,
        &mut net,
        Some(&q),
        vec![UiRequest::TrainSkillAdvancementClass {
            skill: untrained,
            credits,
        }],
    );
    assert!(unowned.is_empty());
    assert_eq!(sent.len(), 1, "one action, got {sent:?}");
    assert_eq!(sent[0].opcode, 0x0047, "Train_TrainSkillAdvancementClass");
    let mut want = Vec::new();
    want.extend_from_slice(&untrained.to_le_bytes());
    want.extend_from_slice(&credits.to_le_bytes());
    assert_eq!(sent[0].body, want, "[skillId][creditsSpent], little-endian");

    // Now each gate, on the routed path. A raise on an untrained skill sends nothing...
    let refused_before = inter.stats.requests_refused;
    let (sent, _) = route_and_send(
        &mut inter,
        &store,
        &mut objects,
        &mut net,
        Some(&q),
        vec![UiRequest::TrainSkill {
            skill: untrained,
            xp: 1,
        }],
    );
    assert!(
        sent.is_empty(),
        "an untrained skill cannot be raised: {sent:?}"
    );
    assert_eq!(
        inter.stats.requests_refused,
        refused_before + 1,
        "and the refusal is counted"
    );

    // ...and a train on an already-trained skill sends nothing either.
    let (sent, _) = route_and_send(
        &mut inter,
        &store,
        &mut objects,
        &mut net,
        Some(&q),
        vec![UiRequest::TrainSkillAdvancementClass {
            skill: trained,
            credits: 1,
        }],
    );
    assert!(
        sent.is_empty(),
        "a trained skill cannot be trained again: {sent:?}"
    );
    assert_eq!(inter.stats.requests_refused, refused_before + 2);

    // With no supplied player qualities there is no gate to read, so nothing is sent and nothing is
    // guessed — the failure mode a `unwrap_or_default()` here would turn into a wrong send.
    let (sent, _) = route_and_send(
        &mut inter,
        &store,
        &mut objects,
        &mut net,
        None,
        vec![UiRequest::TrainSkill {
            skill: trained,
            xp: 1,
        }],
    );
    assert!(sent.is_empty(), "no player description means no raise");
    assert_eq!(inter.stats.requests_refused, refused_before + 3);
}

/// Route the synthetic spellbook filter mask as 0x0286. The original filter handler established
/// that the mask changed before sending; this routed request reports it without a skill-class
/// gate. This test observes encoding/routing, not the upstream changed-mask predicate.
#[test]
fn the_spellbook_filter_write_back_reaches_the_wire() {
    use dereth_ui_screens::view::UiRequest;

    let (mut net, events) = replay("first-login-walk-jump", true);
    let q = capture_qualities(&events);
    let store = dat_store();
    let mut objects = ObjectStream::new();
    let mut inter = dereth_client::interaction::Interaction::new();

    const MASK: u32 = 0x0000_2AAA;
    let (sent, unowned) = route_and_send(
        &mut inter,
        &store,
        &mut objects,
        &mut net,
        Some(&q),
        vec![UiRequest::SetSpellbookFilter { mask: MASK }],
    );
    assert!(unowned.is_empty());
    assert_eq!(sent.len(), 1, "got {sent:?}");
    assert_eq!(sent[0].opcode, 0x0286, "Character_SpellbookFilterEvent");
    assert_eq!(sent[0].body, MASK.to_le_bytes().to_vec());
}

// ---------------------------------------------------------------------------------------------
// 2. Link 5 — the answer is applied. Corpus oracle.
// ---------------------------------------------------------------------------------------------

/// Apply early-inventory-and-casting's five recorded private skill updates on four properties and rebuild rows.
/// Property 0x10 advances sequence 0 to 1 and spent experience 551 to 554. The World's player
/// store owns the qualities, with the HUD dispatching and rebuilding joins. Decode expected fields from recorded bodies, compare final values/stamps and require
/// enchanted row values. The last-writer oracle here relies on this recorded in-order stream.
///
/// **Falsified by** removing `StatType::Skill` from `Qualities::set` (the skills stop landing and
/// `skill_updates` goes to zero) or by removing the `apply_quality_update` arm from
/// `Hud::ui_event` (the same, plus every `0x02CF` stops landing).
#[test]
fn the_captures_own_skill_updates_land_and_move_the_panel() {
    let (_net, events) = replay("early-inventory-and-casting", false);
    let in_world = events_in_world(&events);

    // The oracle: every 0x02DD in the capture, decoded here, last-writer-wins per property.
    let mut want_skill: BTreeMap<u32, dereth_protocol::types::qualities::Skill> = BTreeMap::new();
    let mut want_int64: BTreeMap<u32, i64> = BTreeMap::new();
    let mut seen_0x02dd = 0u32;
    for e in in_world {
        let SessionEvent::UiEvent { opcode, blob } = e else {
            continue;
        };
        let body = &blob[4..];
        match *opcode {
            dereth_protocol::Opcode::QUALITIES_PRIVATE_UPDATE_SKILL => {
                let m = dereth_protocol::qualities::QualitiesPrivateUpdateSkill::read(
                    &mut dereth_protocol::archive::Reader::new(body),
                )
                .expect("the capture's own 0x02DD decodes");
                want_skill.insert(m.0.property_id, m.0.value);
                seen_0x02dd += 1;
            }
            dereth_protocol::Opcode::QUALITIES_PRIVATE_UPDATE_INT64 => {
                let m = dereth_protocol::qualities::QualitiesPrivateUpdateInt64::read(
                    &mut dereth_protocol::archive::Reader::new(body),
                )
                .expect("the capture's own 0x02CF decodes");
                want_int64.insert(m.0.property_id, m.0.value);
            }
            _ => {}
        }
    }
    assert_eq!(
        seen_0x02dd, 5,
        "early-inventory-and-casting carries five Qualities_PrivateUpdateSkill"
    );
    assert_eq!(
        want_skill.len(),
        4,
        "on four distinct skills, one of them twice"
    );
    assert!(
        want_int64.contains_key(&2),
        "and AvailableExperience moves too — the other half of the raise answer"
    );

    let mut app = app_in_gameplay(4);
    let _ = app.apply_hud_events(in_world);
    for _ in 0..4 {
        app.frame();
    }

    let s = app.hud().stats;
    assert_eq!(
        s.skill_updates, 5,
        "all five reached the player description"
    );
    assert_eq!(
        s.quality_updates_stale, 0,
        "a live server's own stream is in order"
    );
    assert_eq!(s.quality_updates_unstorable, 0);
    assert!(
        s.quality_updates > 5,
        "the int and int64 updates landed as well: {}",
        s.quality_updates
    );

    let q = app
        .objects()
        .world
        .player_qualities()
        .expect("the player description");
    for (id, want) in &want_skill {
        let got = q
            .skill(*id)
            .unwrap_or_else(|| panic!("skill {id} is missing"));
        assert_eq!(got.level_from_pp, want.level_from_pp, "skill {id} level");
        assert_eq!(got.sac, want.sac, "skill {id} sac");
        assert_eq!(got.pp, want.pp, "skill {id} experience spent");
        assert_eq!(
            got.resistance_of_last_check, want.resistance_of_last_check,
            "skill {id} resistance"
        );
    }
    // The one property the capture updates twice must hold the **second** value, which is the
    // whole point of the sequence gate being a gate and not a filter.
    let twice = 0x10u32;
    assert_eq!(
        q.skill(twice).expect("skill 0x10").pp,
        want_skill[&twice].pp,
        "the later of the two 0x02DD on property 0x10 is the one on screen"
    );
    assert_eq!(
        app.objects()
            .world
            .weenie(app.objects().world.player.expect("a player"))
            .expect("the player row")
            .stamper
            .as_ref()
            .expect("the stamper was set up")
            .stamp(
                dereth_client_model::StatKey::new(dereth_client_model::StatType::Skill, twice).0
            ),
        Some(1),
        "and the counter walked 0 -> 1 on the shared skill tag"
    );

    // The Int64s land too — this is `AvailableExperience`, the footer's line two.
    for (p, want) in &want_int64 {
        assert_eq!(
            q.get(dereth_client_model::StatKey::new(
                dereth_client_model::StatType::Int64,
                *p
            )),
            Some(dereth_client_model::StatValue::Int64(*want)),
            "PropertyInt64 {p}"
        );
    }

    // Check panel rows against the enchanted skill total. Original row formatting uses the
    // enchanted query result (raw=false); the raw query is only a comparand. Keep that
    // alternative as a discriminator: every checked captured skill must give different raw
    // and enchanted readings, so reverting the row cannot satisfy both comparisons.
    let (skill_table, _xp) = tables();
    let rows: BTreeMap<u32, i32> = app
        .hud()
        .panels
        .skills
        .rows
        .iter()
        .map(|r| (r.skill, r.value))
        .collect();
    let mut checked = 0;
    let mut differed = 0;
    for id in want_skill.keys() {
        if !skill_table.skills.contains_key(id) {
            continue;
        }
        let inq = |raw: bool| {
            i32::try_from(
                dereth_client_model::skills::inq_skill(q, &skill_table, *id, raw)
                    .expect("the skill query returns a level"),
            )
            .expect("a level fits")
        };
        let want = inq(false);
        assert_eq!(
            rows.get(id),
            Some(&want),
            "the row for skill {id} shows the updated level"
        );
        if inq(true) != want {
            differed += 1;
        }
        checked += 1;
    }
    assert!(
        checked > 0,
        "none of the capture's changed skills has a row, so nothing was checked"
    );
    assert_eq!(
        differed, checked,
        "the raw and enchanted skill readings must differ here, or this assertion cannot tell them apart"
    );
}

// ---------------------------------------------------------------------------------------------
// 3. Link 6 — the answer moves the number on screen. Corpus oracle.
// ---------------------------------------------------------------------------------------------

/// One `Qualities_PrivateUpdateSkill` as a `SessionEvent`, built from the decoded definition.
fn skill_answer(
    sequence: u8,
    skill: u32,
    value: dereth_protocol::types::qualities::Skill,
) -> SessionEvent {
    let m = dereth_protocol::qualities::QualitiesPrivateUpdateSkill(
        dereth_protocol::qualities::PrivateUpdate {
            sequence,
            property_id: skill,
            value,
        },
    );
    let mut w = dereth_protocol::archive::Writer::new();
    w.u32(dereth_protocol::Opcode::QUALITIES_PRIVATE_UPDATE_SKILL.0);
    m.write(&mut w).expect("encode");
    SessionEvent::UiEvent {
        opcode: dereth_protocol::Opcode::QUALITIES_PRIVATE_UPDATE_SKILL,
        blob: w.into_inner(),
    }
}

// The positive raise reply comes from recorded traffic. skill_answer serves the controlled
// stale-update test;
// the wrong-subject test constructs its public form separately. These controls do not depend
// on the corpus containing the desired exceptional ordering or subject pair.

/// Expose the character page and select the row's sub-panel before pointer hit testing.
/// Original list-box row lookup refused queries when the pointer was not over the list.
/// This helper uses the screen's panel-visibility handler, then dispatches MOUSE_CLICK directly
/// to the tab found through the current Panel's tab_to_page map. It does not synthesize a full
/// pointer gesture for the toolbar or tab; press_row below supplies the actual row mouse input.
fn show_page_for(app: &mut App, h: ElemHandle) {
    let page = dereth_ui_screens::panels::remaining::CHARACTER_PAGE;
    {
        let (ui, screen) = gameplay_screen(app);
        if let Some(panel_id) = screen
            .panels
            .pages
            .iter()
            .find(|p| p.element == page)
            .map(|p| p.panel_id)
        {
            screen.recv_set_panel_visibility(ui, panel_id, true);
        }
    }
    app.frame();
    // Which sub-panel is this row in? Walk up to whichever of the two the row sits under.
    let sub = {
        let (ui, _) = gameplay_screen(app);
        let mut cur = Some(h);
        let mut found = None;
        while let Some(c) = cur {
            let id = ui.node(c).map(dereth_ui::ElementNode::element_id);
            if id == Some(dereth_ui_screens::panels::skills::PANEL)
                || id == Some(dereth_ui_screens::panels::attributes::PANEL)
            {
                found = id;
                break;
            }
            cur = ui.parent(c);
        }
        found
    };
    if let Some(sub) = sub {
        let tab = {
            let (ui, screen) = gameplay_screen(app);
            let root = screen.root().expect("the gameplay root");
            ui.get_child_recursive(root, page)
                .and_then(|ph| {
                    let n = ui.node(ph)?;
                    let b = n.behaviour.as_ref()?;
                    let p = (**b)
                        .as_any()?
                        .downcast_ref::<dereth_ui::widgets::panel::Panel>()?;
                    p.tab_to_page
                        .iter()
                        .find(|(_, pg)| **pg == sub)
                        .map(|(t, _)| *t)
                })
                .and_then(|t| ui.get_child_recursive(root, t))
        };
        if let Some(th) = tab {
            let (ui, _) = gameplay_screen(app);
            ui.broadcast_element_message(th, dereth_ui::msg::element::id::MOUSE_CLICK, 0, 0);
        }
    }
    for _ in 0..3 {
        app.frame();
    }
}
/// Press the row centre through UiSystem::mouse_down and the actual hit-test producer.
/// A field without a tooltip is not mouse-visible, so the real press hits its list box; a
/// row-targeted message injection would bypass that producer. The footer buttons, by contrast,
/// are dispatched directly later.
fn press_row(app: &mut App, h: ElemHandle) {
    show_page_for(app, h);
    // The list hit test rejects y at or beyond its height, so scroll the row into view first.
    // scroll_to_view drives the scrollable element used by the scrollbar, preserving the real
    // layout/hit-test relationship.
    {
        let mut panels = std::mem::take(&mut app.hud_mut().panels);
        {
            let (ui, _) = gameplay_screen(app);
            for w in [panels.skills.list.as_mut(), panels.attributes.list.as_mut()]
                .into_iter()
                .flatten()
            {
                if let Some(i) = w.index_of(h) {
                    w.scroll_to_view(ui, i);
                }
            }
        }
        app.hud_mut().panels = panels;
    }
    {
        let (ui, _) = gameplay_screen(app);
        let b = ui.screen_box(h);
        ui.mouse_down(
            dereth_ui::focus::action::PRIMARY_CLICK,
            (b.x0 + b.x1) / 2,
            (b.y0 + b.y1) / 2,
        );
    }
    app.frame();
}

/// Dispatch BUTTON_CLICKED to the footer container selected by the panel's live state.
fn click_footer_button(app: &mut App, child: u32) {
    {
        let (ui, screen) = gameplay_screen(app);
        let root = screen.root().expect("the gameplay root");
        let page = ui
            .get_child_recursive(root, dereth_ui_screens::panels::remaining::CHARACTER_PAGE)
            .expect("the character page");
        let panel = ui
            .get_child_recursive(page, skills::PANEL)
            .expect("the skills panel");
        let state = ui.node(panel).map_or(0, |n| n.state.0);
        let c = ui
            .get_child_recursive(panel, ElementId(statmgmt::Footer::container_for(state)))
            .expect("the footer container the panel's state names");
        let h = ui
            .get_child_recursive(c, ElementId(child))
            .expect("the footer button");
        ui.broadcast_element_message(h, dereth_ui::msg::element::id::BUTTON_CLICKED, 7, 0);
    }
    // Two frames are required by the current ordering: App::frame ticks the packet controller
    // before the interaction slot. The action routed later becomes a datagram on the next
    // controller pass; reading the queue after one frame would be too early.
    app.frame();
    app.frame();
}

/// Read a footer child's live text through the state-selected container.
fn footer_text(app: &mut App, child: u32) -> String {
    let (ui, screen) = gameplay_screen(app);
    let root = screen.root().expect("the gameplay root");
    let page = ui
        .get_child_recursive(root, dereth_ui_screens::panels::remaining::CHARACTER_PAGE)
        .expect("the character page");
    let panel = ui
        .get_child_recursive(page, skills::PANEL)
        .expect("the skills panel");
    let state = ui.node(panel).map_or(0, |n| n.state.0);
    let c = ui
        .get_child_recursive(panel, ElementId(statmgmt::Footer::container_for(state)))
        .expect("the footer container");
    let h = ui
        .get_child_recursive(c, ElementId(child))
        .expect("the footer child");
    ui.text_element_mut(h)
        .map_or_else(String::new, |t| t.glyphs.inq_text(false))
}

/// Read a footer child's current element-state word through the same container.
fn footer_state(app: &mut App, child: u32) -> u32 {
    let (ui, screen) = gameplay_screen(app);
    let root = screen.root().expect("the gameplay root");
    let page = ui
        .get_child_recursive(root, dereth_ui_screens::panels::remaining::CHARACTER_PAGE)
        .expect("the character page");
    let panel = ui
        .get_child_recursive(page, skills::PANEL)
        .expect("the skills panel");
    let state = ui.node(panel).map_or(0, |n| n.state.0);
    let c = ui
        .get_child_recursive(panel, ElementId(statmgmt::Footer::container_for(state)))
        .expect("the footer container");
    let h = ui
        .get_child_recursive(c, ElementId(child))
        .expect("the footer child");
    ui.node(h)
        .map(|n| n.state.0)
        .expect("the button has a state")
}

/// Drive short-play-with-training skill-raise replies through panel values, footer text and button state.
/// The recorded sequence includes skill, attribute and vital raises plus training; this station
/// selects 0x0046 requests whose skills have visible rows and applies other answers to retain
/// state. It uses NullPresentation, so these are UI-state observations rather than pixels.
///
/// 1. Select the row through mouse_down and compare the footer's cost with recorded spending
///    and its available experience with the pre-reply player store.
/// 2. Dispatch the footer button's message, require one encoded request, the awaiting-raise
///    latch and disabled state 0x0D. This is not a physical mouse gesture on that button.
/// 3. Apply recorded skill and available-experience replies with their captured sequences.
/// 4. Compare the enchanted row value, experience deduction, next cost and re-enabled button.
///    Require raw/enchanted readings and old/new costs to differ, preserving the discriminators.
///
/// A further button dispatch is tested only after the last captured raise, because it re-latches
/// the panel and must not alter the preconditions of intervening recorded round trips.
///
/// **Falsified by** removing the response handling that clears awaiting_raise and refreshes the
/// selection (the button stays disabled and the footer stale), or removing the
/// rebuild_panel_tables call from Hud::apply_quality_update (the row stays stale).
#[test]
fn a_raise_answer_moves_the_number_and_re_enables_the_button() {
    use dereth_client_model::{StatKey, StatType, StatValue};

    const SESSION: &str = "short-play-with-training";
    let (prelude, trips) = round_trips(SESSION);
    assert!(
        trips.iter().any(|t| t.request.opcode == 0x0046),
        "{SESSION} is this test's oracle and must carry a raise"
    );
    let (skill_table, xp_table) = tables();

    let mut app = app_in_gameplay(4);
    // The capture's own socket-free endpoint, in world, so the clicks below can be observed at
    // the wire. `replay(.., true)` stops at `0x0013`, which is all that is needed to send: the
    // HUD state this test reads comes from `prelude`, not from this link.
    let (net, _) = replay(SESSION, true);
    app.attach_replay_network(net)
        .expect("a headless app with no link takes the replay endpoint");
    let _ = app.apply_hud_events(&prelude);
    for _ in 0..4 {
        app.frame();
    }
    app.ui_mut()
        .expect("the UI shell is up")
        .ui
        .requests
        .clear();
    let _ = wire_actions(&mut app);

    let last_raise = trips
        .iter()
        .rev()
        .find(|t| t.request.opcode == 0x0046)
        .map(|t| t.index)
        .expect("a raise");
    let mut proved = 0u32;
    for trip in &trips {
        let (skill, cost) = train_body(&trip.request);
        let row = app
            .hud()
            .panels
            .skills
            .rows
            .iter()
            .find(|r| r.skill == skill)
            .map(|r| r.element);
        let (Some(h), 0x0046) = (row, trip.request.opcode) else {
            // Not a raise, or a skill the page does not show: apply the answer and move on, so the
            // state the next round trip is read against is still the capture's.
            let _ = app.apply_hud_events(&trip.answers);
            app.frame();
            continue;
        };
        let what = format!(
            "{SESSION}[{}] raise of skill {skill} for {cost}",
            trip.index
        );
        let q0 = app
            .objects()
            .world
            .player_qualities()
            .expect("the player description")
            .clone();
        let Some(StatValue::Int64(available_before)) = q0.get(StatKey::new(StatType::Int64, 2))
        else {
            panic!("{what}: the capture has an AvailableExperience")
        };

        // ---- 1. the footer, against the retail client's own request ---------------------------
        press_row(&mut app, h);
        assert_eq!(
            footer_text(&mut app, statmgmt::child::LINE_ONE_VALUE),
            statmgmt::num(cost),
            "{what}: the footer's cost is the amount the retail client put on the wire"
        );
        assert_eq!(
            dereth_client_model::advancement::skill_cost_to_raise(
                &q0,
                &skill_table,
                &xp_table,
                skill
            ),
            cost,
            "{what}: and it is the advancement helper's computed cost"
        );
        assert_eq!(
            footer_text(&mut app, statmgmt::child::LINE_TWO_VALUE),
            statmgmt::num(available_before),
            "{what}: and the server's own unassigned experience"
        );
        let level_before = app
            .hud()
            .panels
            .skills
            .rows
            .iter()
            .find(|r| r.skill == skill)
            .expect("the row")
            .value;

        // ---- 2. the click ---------------------------------------------------------------------
        //
        // F7 moved this assertion from the UI request outbox to emitted datagrams. Original
        // raise handling called its sender synchronously from the element callback; keeping
        // the request until a later inspection was a fixture/queue artifact. Current synchronous
        // ownership removes that retention. Preserve exactly one message plus opcode/body
        // checks rather than weakening the old exactly-one-request requirement.
        let _ = wire_actions(&mut app);
        click_footer_button(&mut app, statmgmt::child::BUTTON);
        let sent = wire_actions(&mut app);
        assert_eq!(sent.len(), 1, "{what}: one click, one message: {sent:?}");
        assert_eq!(sent[0].opcode, 0x0046, "{what}: Train_TrainSkill");
        let mut want_body = Vec::new();
        want_body.extend_from_slice(&skill.to_le_bytes());
        want_body.extend_from_slice(&cost.to_le_bytes());
        assert_eq!(
            sent[0].body, want_body,
            "{what}: the message the panel sends carries the footer's own number"
        );
        assert!(
            app.hud().panels.skills.awaiting_raise,
            "{what}: the double-spend latch is set"
        );
        assert_eq!(
            footer_state(&mut app, statmgmt::child::BUTTON),
            statmgmt::button_state::DISABLED,
            "{what}: raising the selected skill disables the clicked button"
        );
        let answered_before = app.hud().stats.raises_answered;

        // ---- 3. the server's own answer, out of the capture ------------------------------------
        let (_, skill_answer) = answer_on(&trip.answers, StatType::Skill, skill)
            .unwrap_or_else(|| panic!("{what}: the capture's own 0x02DD"));
        let (_, xp_answer) = answer_on(&trip.answers, StatType::Int64, 2)
            .unwrap_or_else(|| panic!("{what}: the capture's own 0x02CF"));
        let StatValue::Skill(after) = skill_answer.value else {
            panic!("{what}: not a Skill")
        };
        let StatValue::Int64(available_after) = xp_answer.value else {
            panic!("{what}: not Int64")
        };
        let _ = app.apply_hud_events(&trip.answers);
        for _ in 0..2 {
            app.frame();
        }

        // ---- 4. the numbers -------------------------------------------------------------------
        let q1 = app
            .objects()
            .world
            .player_qualities()
            .expect("the player description")
            .clone();
        let got = *q1.skill(skill).expect("the skill");
        assert_eq!(
            got, after,
            "{what}: the stored record is the server's, exactly"
        );
        assert_eq!(
            available_before - available_after,
            i64::from(cost),
            "{what}: the server deducted exactly the cost the footer showed"
        );

        // The oracle is the enchanted total (raw=false), the original row's displayed query
        // result; raw=true stays alongside it as the unequal negative control.
        let want_level = i32::try_from(
            dereth_client_model::skills::inq_skill(&q1, &skill_table, skill, false)
                .expect("the skill query returns a level"),
        )
        .expect("a level fits");
        assert_ne!(
            want_level,
            i32::try_from(
                dereth_client_model::skills::inq_skill(&q1, &skill_table, skill, true)
                    .expect("the skill query returns a level")
            )
            .expect("a level fits"),
            "{what}: the raw and enchanted readings must differ, or this cannot tell them apart"
        );
        let row_after = app
            .hud()
            .panels
            .skills
            .rows
            .iter()
            .find(|r| r.skill == skill)
            .expect("the row survived the rebuild")
            .value;
        assert_eq!(
            row_after, want_level,
            "{what}: the row shows the updated enchanted skill level"
        );
        assert_eq!(
            row_after,
            level_before + 1,
            "{what}: one bought rank is one displayed level: {level_before} -> {row_after}"
        );

        let want_cost_after = dereth_client_model::advancement::skill_cost_to_raise(
            &q1,
            &skill_table,
            &xp_table,
            skill,
        );
        assert_eq!(
            footer_text(&mut app, statmgmt::child::LINE_TWO_VALUE),
            statmgmt::num(available_after),
            "{what}: unassigned experience on screen is the server's own new number"
        );
        assert_eq!(
            footer_text(&mut app, statmgmt::child::LINE_ONE_VALUE),
            statmgmt::num(want_cost_after),
            "{what}: and the next raise's cost is recomputed from the new _pp"
        );
        assert_ne!(
            want_cost_after, cost,
            "{what}: the next level costs a different amount"
        );

        assert!(
            !app.hud().panels.skills.awaiting_raise,
            "{what}: the latch cleared on the answer"
        );
        assert_eq!(
            app.hud().stats.raises_answered,
            answered_before + 1,
            "{what}: the stat-management panel's 0x10000004 arm ran exactly once"
        );
        assert_eq!(
            footer_state(&mut app, statmgmt::child::BUTTON),
            statmgmt::button_state::ENABLED,
            "{what}: and the raise button is enabled again"
        );

        // Only the last captured raise gets this extra request: it re-latches awaiting_raise.
        // Resetting the latch between recorded trips would bypass the behavior being checked.
        if trip.index == last_raise {
            // At the wire, for the same reason as the click above: the outbox does not survive
            // the frame that produced the request, and the client has no such outbox anyway.
            // Still exactly one message, and its body is still the recomputed cost.
            let _ = wire_actions(&mut app);
            click_footer_button(&mut app, statmgmt::child::BUTTON);
            let sent = wire_actions(&mut app);
            assert_eq!(
                sent.len(),
                1,
                "{what}: a further raise sends one message: {sent:?}"
            );
            assert_eq!(sent[0].opcode, 0x0046, "{what}: Train_TrainSkill");
            let mut want_body = Vec::new();
            want_body.extend_from_slice(&skill.to_le_bytes());
            want_body.extend_from_slice(&want_cost_after.to_le_bytes());
            assert_eq!(
                sent[0].body, want_body,
                "{what}: a further raise carries the recomputed cost"
            );
        }
        proved += 1;
    }
    assert!(
        proved > 0,
        "{SESSION}'s raises all landed on skills the page does not show"
    );
    eprintln!("{proved} captured raises driven through the page");
}

/// Isolate a captured whole-record vital reply from accompanying experience updates.
/// The original key-only predicate rejected all tag-9 updates to avoid regeneration ticks,
/// thereby also rejecting whole SecondaryAttribute replies (0x02E7) to 0x0044 raises. The
/// value-aware predicate distinguishes those from current-level-only 0x02E9 ticks.
///
/// The test discovers train sessions and pins 64 vital cases.
/// Remove the accompanying 0x02CF so it cannot trigger the response arm for the wrong reason.
/// Compare the applied record and raises_answered increment. Unlike the button station above,
/// this test does not first set and then directly inspect an awaiting-raise latch.
///
/// **Falsified by** putting `update::answers_a_raise(u.key)` back in `Hud::apply_quality_update`
/// in place of `answers_a_raise_update(&u)`: `raises_answered` stops moving. That is the mutation
/// this test was written from.
#[test]
fn a_captured_vital_raise_answer_on_its_own_clears_the_latch() {
    use dereth_client_model::{StatType, StatValue};

    let mut proved = 0u32;
    for session in sessions_with_train_traffic() {
        let (prelude, trips) = round_trips(&session);
        let mut app = app_in_gameplay(4);
        let _ = app.apply_hud_events(&prelude);
        app.frame();

        for trip in &trips {
            if trip.request.opcode != 0x0044 {
                let _ = app.apply_hud_events(&trip.answers);
                app.frame();
                continue;
            }
            let (vital, amount) = train_body(&trip.request);
            let what = format!(
                "{session}[{}] vital {vital} raised for {amount}",
                trip.index
            );
            // The vital record alone: no `0x02CF`, which `answers_a_raise` already accepted and
            // which would otherwise set the flag for the wrong reason.
            let only_the_vital: Vec<SessionEvent> = trip
                .answers
                .iter()
                .filter(|e| match e {
                    SessionEvent::UiEvent { opcode, blob }
                        if qupdate::is_update_opcode(*opcode) =>
                    {
                        qupdate::decode(*opcode, &blob[4..]).is_some_and(|u| {
                            u.subject.is_none() && matches!(u.value, StatValue::Attribute2nd(_))
                        })
                    }
                    _ => false,
                })
                .cloned()
                .collect();
            assert_eq!(
                only_the_vital.len(),
                1,
                "{what}: one 0x02E7 answers one 0x0044"
            );
            let (_, u) = answer_on(&only_the_vital, StatType::Attribute2nd, vital)
                .unwrap_or_else(|| panic!("{what}: it names the vital that was raised"));
            let StatValue::Attribute2nd(server) = u.value.clone() else {
                unreachable!()
            };

            let before = app
                .objects()
                .world
                .player_qualities()
                .expect("the player description")
                .attribute_2nd(vital)
                .expect("the vital");
            let answered_before = app.hud().stats.raises_answered;
            let _ = app.apply_hud_events(&only_the_vital);
            app.frame();

            assert_eq!(
                server.attribute.cp_spent,
                before.attribute.cp_spent + amount,
                "{what}: the server's record carries exactly the experience asked for"
            );
            assert_eq!(
                app.objects()
                    .world
                    .player_qualities()
                    .expect("the player description")
                    .attribute_2nd(vital)
                    .expect("the vital"),
                server,
                "{what}: and it landed"
            );
            assert_eq!(
                app.hud().stats.raises_answered,
                answered_before + 1,
                "{what}: the 0x10000004 arm ran for a vital raise's own answer"
            );
            // The rest of the round trip, so the next one reads the capture's state.
            let _ = app.apply_hud_events(&trip.answers);
            app.frame();
            proved += 1;
        }
    }
    assert_eq!(
        proved, 64,
        "the selected corpus must yield 64 vital-raise cases"
    );
}

/// Behaviour: advancement.answer.an-out-of-order-or-replayed-answer-is-refused
/// A stale synthetic skill reply must not change the stored level. The sequence gate precedes
/// writing and uses a half-range wrap comparison, not ordinary greater-than. This pair exercises
/// sequence 0 after 1, not wraparound itself. The older reply deliberately carries a different,
/// larger level so accepting it would be visible.
///
/// **Falsified by** deleting the `stamper.update(...)` guard from
/// `dereth_client_model::qualities::update::apply`, which makes the second assertion fail.
#[test]
fn an_out_of_order_answer_is_rejected_and_leaves_the_number_alone() {
    let (mut app, events) = app_with_capture("first-login-walk-jump");
    let q0 = capture_qualities(&events);
    let skill = q0
        .skills
        .as_ref()
        .expect("skills")
        .iter()
        .find(|(_, s)| s.sac >= 2)
        .map(|(id, _)| *id)
        .expect("a trained skill");
    let before = *q0.skill(skill).expect("the skill");

    let newer = dereth_protocol::types::qualities::Skill {
        level_from_pp: before.level_from_pp + 2,
        ..before
    };
    let older = dereth_protocol::types::qualities::Skill {
        level_from_pp: before.level_from_pp + 9,
        ..before
    };

    let _ = app.apply_hud_events(&[skill_answer(1, skill, newer)]);
    app.frame();
    assert_eq!(
        app.objects()
            .world
            .player_qualities()
            .expect("q")
            .skill(skill)
            .expect("s")
            .level_from_pp,
        before.level_from_pp + 2,
        "sequence 1 landed"
    );
    let stale_before = app.hud().stats.quality_updates_stale;

    let _ = app.apply_hud_events(&[skill_answer(0, skill, older)]);
    app.frame();
    assert_eq!(
        app.objects()
            .world
            .player_qualities()
            .expect("q")
            .skill(skill)
            .expect("s")
            .level_from_pp,
        before.level_from_pp + 2,
        "sequence 0 after 1 is one step backwards under the half-range window and must not apply"
    );
    assert_eq!(
        app.hud().stats.quality_updates_stale,
        stale_before + 1,
        "and the rejection is counted rather than being silent"
    );
}

/// A public update naming another object must not modify the player's World-owned qualities.
/// Private forms are player-scoped; public forms carry a subject that the World's player-update
/// path checks. The same payload addressed to the player must then apply, excluding a blanket
/// rejection of all public updates. The HUD counts outcomes rather than owning a second store.
#[test]
fn a_public_update_about_another_object_is_not_the_players() {
    let (mut app, events) = app_with_capture("first-login-walk-jump");
    let q0 = capture_qualities(&events);
    let skill = q0
        .skills
        .as_ref()
        .expect("skills")
        .iter()
        .find(|(_, s)| s.sac >= 2)
        .map(|(id, _)| *id)
        .expect("a trained skill");
    let before = *q0.skill(skill).expect("the skill");

    let mine = app.hud().player.expect("the capture created the player");
    let other = ObjectId(mine.0 ^ 0x0000_00FF);
    let value = dereth_protocol::types::qualities::Skill {
        level_from_pp: before.level_from_pp + 5,
        ..before
    };
    let m = dereth_protocol::qualities::QualitiesUpdateSkill(
        dereth_protocol::qualities::PublicUpdate {
            sequence: 0,
            object: other,
            property_id: skill,
            value,
        },
    );
    let mut w = dereth_protocol::archive::Writer::new();
    w.u32(dereth_protocol::Opcode::QUALITIES_UPDATE_SKILL.0);
    m.write(&mut w).expect("encode");
    let _ = app.apply_hud_events(&[SessionEvent::UiEvent {
        opcode: dereth_protocol::Opcode::QUALITIES_UPDATE_SKILL,
        blob: w.into_inner(),
    }]);
    app.frame();

    assert_eq!(
        app.objects()
            .world
            .player_qualities()
            .expect("q")
            .skill(skill)
            .expect("s")
            .level_from_pp,
        before.level_from_pp,
        "another object's skill is not the player's"
    );
    assert_eq!(app.hud().stats.skill_updates, 0);

    // The same message addressed to the player does land, so the guard is a guard and not a
    // blanket refusal of the public form.
    let m = dereth_protocol::qualities::QualitiesUpdateSkill(
        dereth_protocol::qualities::PublicUpdate {
            sequence: 0,
            object: mine,
            property_id: skill,
            value,
        },
    );
    let mut w = dereth_protocol::archive::Writer::new();
    w.u32(dereth_protocol::Opcode::QUALITIES_UPDATE_SKILL.0);
    m.write(&mut w).expect("encode");
    let _ = app.apply_hud_events(&[SessionEvent::UiEvent {
        opcode: dereth_protocol::Opcode::QUALITIES_UPDATE_SKILL,
        blob: w.into_inner(),
    }]);
    app.frame();
    assert_eq!(
        app.objects()
            .world
            .player_qualities()
            .expect("q")
            .skill(skill)
            .expect("s")
            .level_from_pp,
        before.level_from_pp + 5,
    );
    assert_eq!(app.hud().stats.skill_updates, 1);
}
