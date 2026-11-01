//! Every server-to-client script call recorded in 2013 and 2017, dispatched into its handler.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use auberean_script::bridge::{object_process, receive, Outcome};
use auberean_script::{Status, Stop, Vm};
use auberean_testkit::captures::{self, carrier};

use crate::world::{
    created, player_id, target, Made, Session, World, CREATE_OBJECT, CREATE_PLAYER,
};

/// Steps one call may take.
const BUDGET: u64 = 2_000_000;

/// One session's machine: its processes by engine object.
struct Running {
    vm: Vm,
    /// By object id (0 for a stand-in player).
    processes: HashMap<u64, usize>,
}

/// What a run of the corpus found.
#[derive(Default)]
pub struct Tally {
    pub calls: usize,
    pub outcomes: BTreeMap<String, usize>,
    /// Calls stopped at a native not ported yet, by native.
    pub stops: BTreeMap<u32, usize>,
    /// Other stops, by kind, with an example.
    pub failures: BTreeMap<String, (usize, String)>,
    pub receivers: BTreeMap<&'static str, usize>,
    /// Calls stopped using the null reference, by the function they stopped in.
    pub null_uses: BTreeMap<String, usize>,
}

fn kind(s: &Stop) -> String {
    let t = format!("{s:?}");
    t.split(['(', ' ', '{']).next().unwrap_or("").to_string()
}

/// Run every recorded server-to-client call of the given years.
pub fn run_corpus(years: &[u16]) -> Option<Tally> {
    let (dat, prog) = target()?;
    let index = auberean_testkit::data::require_capture_index();
    let lib = &prog.library;
    let mut world = World::new(dat, prog);
    let mut t = Tally::default();
    // One machine per capture (one run of the client, its sessions in order): the player and
    // the objects carry over a hand-off to another server.
    let mut capture_id = -1;
    let mut session = Session::default();
    let mut running: Option<Running> = None;
    // The package of the most common player, for calls whose session never created its player.
    let mut players: BTreeMap<Made, usize> = BTreeMap::new();
    let types = [
        CREATE_PLAYER,
        CREATE_OBJECT,
        carrier::TO_PLAYER,
        carrier::TO_CELL_OBJECT,
        carrier::TO_VISUAL_OBJECT,
    ];
    let mut pending = Vec::new();
    captures::messages(&index, &types, years, |m| pending.push(m)).expect("read the capture index");
    // First pass: the players' packages, for the sessions that lack one.
    {
        let mut player = None;
        let mut cid = -1;
        for m in &pending {
            if m.capture != cid {
                cid = m.capture;
                player = None;
            }
            match m.message_type {
                CREATE_PLAYER => player = player_id(&m.raw),
                CREATE_OBJECT => {
                    if let Some((id, made)) = created(&m.raw) {
                        if Some(id) == player {
                            *players.entry(made).or_default() += 1;
                        }
                    }
                }
                _ => {}
            }
        }
    }
    let default_player = players.iter().max_by_key(|(_, n)| **n).map(|(m, _)| *m)?;
    for m in pending {
        if m.capture != capture_id {
            capture_id = m.capture;
            session = Session::default();
            running = None;
        }
        match m.message_type {
            CREATE_PLAYER => {
                session.player = player_id(&m.raw);
                continue;
            }
            CREATE_OBJECT => {
                if let Some((id, made)) = created(&m.raw) {
                    session.objects.insert(id, made);
                }
                continue;
            }
            _ => {}
        }
        let Some(call) = m.script_call() else {
            continue;
        };
        t.calls += 1;
        let r = running.get_or_insert_with(|| Running {
            vm: Vm::new(Arc::clone(prog)),
            processes: HashMap::new(),
        });
        let player = session
            .player
            .and_then(|p| session.objects.get(&p).map(|m| (p, *m)));
        let (key, made) = match call
            .object
            .and_then(|id| session.objects.get(&id).map(|m| (id, *m)))
        {
            Some(x) => {
                *t.receivers.entry("the object the call names").or_default() += 1;
                x
            }
            None => {
                let who = if call.object.is_some() {
                    "the player (the object was never created)"
                } else {
                    "the session's player"
                };
                match player {
                    Some(x) => {
                        *t.receivers.entry(who).or_default() += 1;
                        x
                    }
                    None => {
                        *t.receivers.entry("a stand-in player").or_default() += 1;
                        (0, default_player)
                    }
                }
            }
        };
        r.vm.budget = BUDGET;
        let process = match r.processes.get(&key) {
            Some(p) => Ok(*p),
            None => object_process(&mut r.vm, &mut world, made.package, made.entity),
        };
        let outcome = process.and_then(|p| {
            r.processes.insert(key, p);
            receive(&mut r.vm, &mut world, p, call.function, &call.args)
        });
        match outcome {
            Ok(Outcome::Ran(Status::Terminated)) => {
                *t.outcomes.entry("completed".into()).or_default() += 1
            }
            Ok(Outcome::Ran(s)) => *t.outcomes.entry(format!("ended {s:?}")).or_default() += 1,
            Ok(Outcome::Refused(why)) => {
                let e = t.failures.entry("arguments refused".into()).or_default();
                e.0 += 1;
                e.1 = why;
            }
            Err(Stop::NotPorted { native, .. }) => {
                *t.outcomes
                    .entry("stopped at a native not ported yet".into())
                    .or_default() += 1;
                *t.stops.entry(native).or_default() += 1;
            }
            Err(Stop::NullReference) => {
                // The script reached for an object an earlier call would have made, had it not
                // stopped at a native: a stop of the script, not of the machine.
                *t.outcomes
                    .entry("stopped using the null reference".into())
                    .or_default() += 1;
                #[allow(clippy::cast_possible_truncation)]
                let at = r.vm.stopped_at.map_or(String::new(), |i| {
                    lib.function_at((i * 4) as u32).to_string()
                });
                *t.null_uses.entry(at).or_default() += 1;
            }
            Err(e) => {
                let f = t.failures.entry(kind(&e)).or_default();
                f.0 += 1;
                if f.1.is_empty() {
                    #[allow(clippy::cast_possible_truncation)]
                    let at = r.vm.stopped_at.map_or(String::new(), |i| {
                        lib.function_at((i * 4) as u32).to_string()
                    });
                    f.1 = format!(
                        "{:08X} {}: {e} in {at}",
                        call.function,
                        lib.function_name(auberean_script::value::FunctionId(call.function))
                            .unwrap_or_default()
                    );
                }
            }
        }
        r.vm.heap.abandon_builds();
        r.vm.stopped_at = None;
        if r.vm.heap.added_since_collection > 500_000 {
            r.vm.collect();
        }
    }
    Some(t)
}

/// Behaviour: script.bridge.every-captured-call-dispatches
#[test]
fn every_relaunch_server_call_runs_to_its_end_or_stops_at_a_named_native() {
    let _ = target().expect("the September 2013 install opens");
    let _index = auberean_testkit::data::require_capture_index();
    let t = run_corpus(&[2013, 2017]).expect("the corpus");
    let (_, prog) = target().unwrap();
    eprintln!(
        "{} calls; outcomes {:?}; receivers {:?}; {} natives stopped at",
        t.calls,
        t.outcomes,
        t.receivers,
        t.stops.len()
    );
    let mut stops: Vec<_> = t.stops.iter().collect();
    stops.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (native, n) in stops.iter().take(60) {
        eprintln!(
            "  {n:>7}  {}  {}",
            auberean_script::value::NativeId(**native),
            prog.library.native_name(**native)
        );
    }
    for (f, n) in &t.null_uses {
        eprintln!("  null reference used in {f}: {n}");
    }
    for (k, (n, ex)) in &t.failures {
        eprintln!("  failure {k}: {n} (e.g. {ex})");
    }
    assert_eq!(t.calls, 757_726);
    assert!(
        t.failures.is_empty(),
        "the machine itself failed: {:?}",
        t.failures
    );
}
