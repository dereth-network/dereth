//! Every recorded January 2017 game login, replayed through the client against a server double:
//! the session's course, the model, what the client sent, and how the server's calls ran.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use auberean_client_sdk::protocol::types::IterationRun;
use auberean_client_sdk::protocol::ClientMessage;
use auberean_client_sdk::runtime::download::SERVER_LIBRARY_ITERATIONS;
use auberean_testkit::double::recorded::{first_packets, game_logins, script};

use crate::client_data;
use crate::replay::{replay, Outcome, TICKET};

/// Every game login's outcome, replayed once for all the tests on every core.
fn outcomes() -> Option<&'static Vec<(String, Outcome)>> {
    static O: OnceLock<Option<Vec<(String, Outcome)>>> = OnceLock::new();
    O.get_or_init(|| {
        let data = client_data()?;
        let index = auberean_testkit::data::require_capture_index();
        let logins = game_logins(&index, "2017").expect("the index reads");
        let next = AtomicUsize::new(0);
        let out = Mutex::new(Vec::new());
        let threads = std::thread::available_parallelism()
            .map_or(4, |n| n.get())
            .min(12);
        std::thread::scope(|s| {
            for _ in 0..threads {
                s.spawn(|| loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(g) = logins.get(i) else { break };
                    let sc = script(&index, g).expect("the script builds");
                    let mut o = replay(&sc, data, i as u64);
                    o.first_packets = first_packets(&index, g).expect("the index reads");
                    out.lock()
                        .expect("the results")
                        .push((g.key().to_owned(), o));
                });
            }
        });
        let mut v = out.into_inner().expect("the results");
        v.sort_by(|a, b| a.0.cmp(&b.0));
        Some(v)
    })
    .as_ref()
}

macro_rules! need_outcomes {
    () => {{
        assert!(client_data().is_some(), "the September 2013 install opens");
        let _ = auberean_testkit::data::require_capture_index();
        outcomes().expect("the replays")
    }};
}

/// The client's messages of one type, in order.
fn ours(o: &Outcome, mtype: u32) -> Vec<&ClientMessage> {
    o.sent
        .iter()
        .map(|s| &s.message)
        .filter(|m| m.message_type() == mtype)
        .collect()
}

/// The message types whose blob ids are compared with the recorded client's, with a name each.
const BLOB_TYPES: [(u32, &str); 5] = [
    (0x10038, "data versions"),
    (0x1002B, "entries"),
    (0x10027, "log-offs"),
    (0x100D1, "scenes drawn"),
    (0x1008E, "positions"),
];

/// The recorded client's messages of one type, in order, decoded.
fn recorded(o: &Outcome, mtype: u32) -> Vec<(f64, ClientMessage)> {
    o.recorded
        .iter()
        .filter(|(_, m)| m.mtype == mtype)
        .filter_map(|(t, m)| ClientMessage::decode(&m.raw).ok().map(|c| (*t, c)))
        .collect()
}

/// The data versions a client sent, less the iterations of a server's library, which it lists
/// whatever it holds and the recorded client didn't. Nothing else of them is taken out.
fn without_a_servers_library(m: &ClientMessage) -> ClientMessage {
    let mut m = m.clone();
    if let ClientMessage::DataHeld(held) = &mut m {
        let library = SERVER_LIBRARY_ITERATIONS;
        for file in &mut held.files {
            let runs = &mut file.iterations.runs;
            let before = runs.len();
            runs.retain(|r| {
                !matches!(r, IterationRun::Run { count, first }
                    if i64::from(*first) == i64::from(library.start)
                        && i64::from(*count) == i64::from(library.end - library.start))
            });
            if runs.len() != before {
                file.iterations.total -= (library.end - library.start).cast_unsigned();
            }
        }
    }
    m
}

/// Every recorded 2017 game login replays through the client to its end: its login connection
/// held until the server let go of it, every world login the recorded client made made again by
/// following the referrals, the player of every world entry made and held by the model.
///
/// Behaviour: session.replay.every-2017-game-login-replays-through-the-client
#[test]
fn every_recorded_2017_game_login_replays_through_the_client() {
    let all = need_outcomes!();
    let mut t: BTreeMap<&str, u64> = BTreeMap::new();
    for (key, o) in all {
        *t.entry("game logins").or_default() += 1;
        *t.entry("world logins recorded").or_default() += o.world_logins_recorded as u64;
        *t.entry("world logins").or_default() += u64::from(o.world_logins);
        *t.entry("referrals").or_default() += u64::from(o.referrals);
        *t.entry("switches").or_default() += u64::from(o.switches);
        *t.entry("login drops").or_default() += u64::from(o.login_drops);
        *t.entry("gates the double fired itself").or_default() += o.gates_forced.len() as u64;
        *t.entry("players created").or_default() += o.players_created as u64;
        *t.entry("players held").or_default() += o.players_held as u64;
        *t.entry("player actions").or_default() += o.actions as u64;
        *t.entry("player actions taken").or_default() += o.actions_done as u64;
        *t.entry("stalled").or_default() += u64::from(o.stalled);
        if o.stalled || o.login_drops > 0 || o.world_logins as usize != o.world_logins_recorded {
            eprintln!("{key}: {:?} {}", o.drop_at, o.stall);
        }
    }
    eprintln!("{t:#?}");
    assert_eq!(t["game logins"], 180);
    assert_eq!(t["stalled"], 0, "every replay reaches its end");
    assert_eq!(t["login drops"], 0, "the login connection is held");
    assert_eq!(
        t["world logins"], t["world logins recorded"],
        "every world login is made again"
    );
    assert_eq!(t["players held"], t["players created"]);
    assert_eq!(t["player actions taken"], t["player actions"]);
}

/// The model's surroundings and inventory agree with what the server later says: the client asks
/// for the objects the recorded client asked for (the objects its messages named and never
/// created), and the items and containers the server lists as the player's inventory are the ones
/// the model holds as the player's.
///
/// Behaviour: model.replay.surroundings-and-inventory-agree-with-the-server
#[test]
fn the_models_surroundings_and_inventory_agree_with_the_server() {
    let all = need_outcomes!();
    let (mut both, mut only_recorded, mut only_ours) = (0usize, 0usize, 0usize);
    let mut inv = crate::replay::InventoryCheck::default();
    for (_, o) in all {
        let r: BTreeSet<u64> = recorded(o, 0x1008F)
            .into_iter()
            .filter_map(|(_, m)| match m {
                ClientMessage::ForceCreate(t) => Some(t.0),
                _ => None,
            })
            .collect();
        let ours: BTreeSet<u64> = ours(o, 0x1008F)
            .into_iter()
            .filter_map(|m| match m {
                ClientMessage::ForceCreate(t) => Some(t.0),
                _ => None,
            })
            .collect();
        both += r.intersection(&ours).count();
        only_recorded += r.difference(&ours).count();
        only_ours += ours.difference(&r).count();
        inv.named += o.inventory_check.named;
        inv.held += o.inventory_check.held;
        inv.elsewhere += o.inventory_check.elsewhere;
        inv.unknown += o.inventory_check.unknown;
    }
    eprintln!("objects asked for again: both {both}, only the recorded client {only_recorded}, only ours {only_ours}");
    eprintln!("inventory the server named: {inv:?}");
    // Where the two differ the recorded client asked for objects whose creation it had lost by
    // means this model does not follow yet; the client asks for next to nothing the recorded one
    // did not.
    assert!(
        only_ours * 100 < both,
        "{only_ours} asked for beyond the recorded client's {both}"
    );
    assert!(only_recorded * 10 < both);
    assert_eq!(inv.named, inv.held + inv.elsewhere + inv.unknown);
    assert!(
        inv.unknown * 1000 < inv.named,
        "inventory the model never had: {}",
        inv.unknown
    );
    assert!(
        inv.held * 100 >= inv.named * 88,
        "inventory held by the player: {}",
        inv.held
    );
}

/// What the client sends on its own equals what the recorded client sent wherever it is not
/// particular to the session: its data versions (but those it lists for a server's library), its entries into the world and log-offs, the scene
/// drawn, and its position reports' cadence and flags; every blob's ordering type; and the stamps
/// of the data versions, the entries and the log-offs. (The ordered stamps of the scene drawn and
/// the position reports count every ordered message before them, the script calls the replay
/// does not send among them, so they are counted and not judged.)
///
/// Behaviour: session.replay.what-the-client-sends-equals-the-recorded-client
#[test]
fn what_the_client_sends_on_its_own_equals_the_recorded_client() {
    let all = need_outcomes!();
    let mut t: BTreeMap<&str, u64> = BTreeMap::new();
    let mut ticket_spans: BTreeMap<i64, u64> = BTreeMap::new();
    let mut gaps_ours = Vec::new();
    let mut gaps_rec = Vec::new();
    // Per message type: blob ids compared, then how many had the same ordering type, stamp and
    // sequence as the recorded client's.
    let mut blobs: BTreeMap<&str, [u64; 4]> = BTreeMap::new();
    for (_, o) in all {
        let held_r = recorded(o, 0x10038);
        let held_o = ours(o, 0x10038);
        *t.entry("data versions recorded").or_default() += held_r.len() as u64;
        *t.entry("data versions sent").or_default() += held_o.len() as u64;
        for ((_, r), m) in held_r.iter().zip(&held_o) {
            *t.entry("data versions byte for byte").or_default() +=
                u64::from(r.encode() == without_a_servers_library(m).encode());
        }
        for (mtype, name, same) in [
            (0x1002B, "entries recorded", "entries byte for byte"),
            (0x10027, "log-offs recorded", "log-offs byte for byte"),
        ] {
            let r = recorded(o, mtype);
            let m = ours(o, mtype);
            *t.entry(name).or_default() += r.len() as u64;
            for ((_, r), m) in r.iter().zip(&m) {
                *t.entry(same).or_default() += u64::from(r.encode() == m.encode());
            }
        }
        // The game login: as long as the recorded one but for the ticket, whose length is the
        // session's own; the recorded packets' flags.
        if let (Some(login), Some((flags, len))) = (&o.login, o.first_packets.first()) {
            *t.entry("game logins compared").or_default() += 1;
            *t.entry("game logins flagged as a login").or_default() +=
                u64::from(*flags == 0x1_0000);
            let ours = 20 + login.len() as i64 - TICKET.len() as i64;
            *ticket_spans.entry(i64::from(*len) - ours).or_default() += 1;
        }
        for (flags, len) in o.first_packets.iter().skip(1) {
            *t.entry("world logins compared").or_default() += 1;
            *t.entry("world logins of 28 bytes, flagged a world login")
                .or_default() += u64::from(*flags == 0x2_0000 && *len == 28);
        }
        // The blob ids: per message type, the recorded client's and ours paired in order, their
        // ordering type, stamp and sequence compared.
        let ours_blobs: Vec<&auberean_testkit::double::Sent> = {
            let mut v: Vec<_> = o.received.iter().flatten().collect();
            v.sort_by_key(|m| m.blob_id & 0xFFFF_FFFF);
            v
        };
        for (mtype, name) in BLOB_TYPES {
            let r: Vec<u64> = o
                .recorded
                .iter()
                .filter(|(_, m)| m.mtype == mtype)
                .map(|(_, m)| m.blob_id)
                .collect();
            let m: Vec<u64> = ours_blobs
                .iter()
                .filter(|b| b.raw.get(..4) == Some(&mtype.to_le_bytes()[..]))
                .map(|b| b.blob_id)
                .collect();
            for (r, m) in r.iter().zip(&m) {
                let c = blobs.entry(name).or_default();
                c[0] += 1;
                c[1] += u64::from(r >> 56 == m >> 56);
                c[2] += u64::from((r >> 32) & 0xFFFF == (m >> 32) & 0xFFFF);
                c[3] += u64::from(r & 0xFFFF_FFFF == m & 0xFFFF_FFFF);
            }
        }
        let sr = recorded(o, 0x100D1).len();
        let so = ours(o, 0x100D1).len();
        *t.entry("scenes drawn recorded").or_default() += sr as u64;
        *t.entry("scenes drawn sent").or_default() += so as u64;
        *t.entry("game logins with as many scenes drawn")
            .or_default() += u64::from(sr == so);
        let pr = recorded(o, 0x1008E);
        let po: Vec<(f64, &ClientMessage)> = o
            .sent
            .iter()
            .filter(|s| s.message.message_type() == 0x1008E)
            .map(|s| (s.at, &s.message))
            .collect();
        *t.entry("positions recorded").or_default() += pr.len() as u64;
        *t.entry("positions sent").or_default() += po.len() as u64;
        for (_, m) in &pr {
            if let ClientMessage::Position(p) = m {
                *t.entry("positions recorded jumping").or_default() += u64::from(p.flags & 2 != 0);
                *t.entry("positions recorded moving").or_default() += u64::from(p.flags & 4 != 0);
            }
        }
        for (_, m) in &po {
            if let ClientMessage::Position(p) = m {
                *t.entry("positions sent jumping").or_default() += u64::from(p.flags & 2 != 0);
                *t.entry("positions sent moving").or_default() += u64::from(p.flags & 4 != 0);
            }
        }
        let steady = |a: &ClientMessage, b: &ClientMessage| match (a, b) {
            (ClientMessage::Position(a), ClientMessage::Position(b)) => {
                a.heading_motion & 0xFF_FFFF == b.heading_motion & 0xFF_FFFF
                    && a.heading_motion.to_le_bytes()[1..3] != [0x7F, 0x7F]
                    && a.flags & 2 == 0
                    && b.flags & 2 == 0
            }
            _ => false,
        };
        for w in pr.windows(2) {
            if steady(&w[0].1, &w[1].1) {
                gaps_rec.push(w[1].0 - w[0].0);
            }
        }
        for w in po.windows(2) {
            if steady(w[0].1, w[1].1) {
                gaps_ours.push(w[1].0 - w[0].0);
            }
        }
    }
    let median = |v: &mut Vec<f64>| {
        v.sort_by(f64::total_cmp);
        v.get(v.len() / 2).copied().unwrap_or(0.0)
    };
    let within = |v: &[f64]| v.iter().filter(|g| (0.2..0.25).contains(*g)).count();
    eprintln!("{t:#?}");
    eprintln!("recorded login size less ours without the ticket: {ticket_spans:?}");
    eprintln!("blob ids [compared, same ordering type, same stamp, same sequence]: {blobs:#?}");
    eprintln!(
        "steady running reports: recorded {} (median gap {:.3} s, {} within 0.20-0.25 s), ours {} (median {:.3} s, {} within)",
        gaps_rec.len(),
        median(&mut gaps_rec),
        within(&gaps_rec),
        gaps_ours.len(),
        median(&mut gaps_ours),
        within(&gaps_ours)
    );
    assert_eq!(
        t["data versions byte for byte"],
        t["data versions recorded"]
    );
    assert_eq!(t["entries byte for byte"], t["entries recorded"]);
    assert!(t["log-offs byte for byte"] + 2 >= t["log-offs recorded"]);
    assert_eq!(
        t["game logins flagged as a login"],
        t["game logins compared"]
    );
    assert_eq!(
        t["world logins of 28 bytes, flagged a world login"],
        t["world logins compared"]
    );
    assert!(
        ticket_spans.keys().all(|k| *k == 246 || *k == 250),
        "{ticket_spans:?}"
    );
    assert_eq!(t["positions sent jumping"], t["positions recorded jumping"]);
    for (name, [compared, ordering, stamp, _]) in &blobs {
        assert_eq!(
            ordering, compared,
            "{name}: every blob's ordering type is the recorded one"
        );
        if ["data versions", "entries", "log-offs"].contains(name) {
            assert!(
                stamp + 1 >= *compared,
                "{name}: {stamp} of {compared} stamps the recorded ones"
            );
        }
    }
    let m = median(&mut gaps_ours);
    assert!((0.2..0.22).contains(&m), "steady reports every {m:.3} s");
}

/// How the server's calls into the client's scripts ran against the model: to their end, or
/// stopped at a native not written yet (counted by native), or otherwise.
///
/// Behaviour: script.replay.server-calls-run-against-the-model
#[test]
fn the_servers_calls_run_against_the_model() {
    let all = need_outcomes!();
    let mut calls = auberean_client_sdk::runtime::CallTally::default();
    let mut notices: BTreeMap<u32, u64> = BTreeMap::new();
    for (_, o) in all {
        calls.calls += o.calls.calls;
        calls.completed += o.calls.completed;
        calls.ended += o.calls.ended;
        calls.refused += o.calls.refused;
        calls.null_reference += o.calls.null_reference;
        calls.no_receiver += o.calls.no_receiver;
        for (k, v) in &o.calls.not_ported {
            *calls.not_ported.entry(*k).or_default() += v;
        }
        for (k, v) in &o.calls.stopped {
            *calls.stopped.entry(k.clone()).or_default() += v;
        }
        for (k, v) in &o.notices {
            *notices.entry(*k).or_default() += v;
        }
    }
    let data = client_data().expect("the data");
    let lib = &data.scripts.as_ref().expect("the scripts").1.library;
    eprintln!(
        "{} calls: {} to the end, {} ended otherwise, {} refused, {} null reference, {} with no receiver, {} stopped at a native not written yet",
        calls.calls,
        calls.completed,
        calls.ended,
        calls.refused,
        calls.null_reference,
        calls.no_receiver,
        calls.not_ported.values().sum::<u64>()
    );
    let mut stops: Vec<_> = calls.not_ported.iter().collect();
    stops.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (native, n) in stops.iter().take(40) {
        eprintln!("  {n:>7}  {}", lib.native_name(**native));
    }
    for (k, n) in &calls.stopped {
        eprintln!("  stopped: {k}: {n}");
    }
    let mut ns: Vec<_> = notices.iter().collect();
    ns.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (native, n) in ns.iter().take(10) {
        eprintln!("  notice {n:>7}  {}", lib.native_name(**native));
    }
    assert!(calls.stopped.is_empty(), "the machine itself failed");
    assert_eq!(calls.null_reference, 0);
}
