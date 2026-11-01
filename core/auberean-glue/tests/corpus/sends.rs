//! Every client-to-server script call recorded in 2013 and 2017, made again by its sending
//! function from the recorded values.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use auberean_script::args::{self, Arg};
use auberean_script::bridge::{params_of, send_call};
use auberean_script::graph::{Body, Graph};
use auberean_script::library::Library;
use auberean_script::opcode::{self, op};
use auberean_script::value::{tag, FunctionId, NULL};
use auberean_script::{Status, Stop, Vm};
use auberean_testkit::captures::{self, carrier};

use crate::world::{target, World};

/// The interface native type (what sending functions send through).
const INTERFACE_TYPE: u32 = 127;

/// For each function of the other side, the function of this library that sends it: the one that
/// pushes its id.
fn senders(lib: &Library) -> HashMap<u32, FunctionId> {
    let by_name: HashMap<String, FunctionId> = lib
        .packages
        .iter()
        .flat_map(|p| {
            p.functions
                .iter()
                .map(move |f| (format!("{}::{}", p.name, f.name), f.id))
        })
        .collect();
    let mut out = HashMap::new();
    let mut at = 0;
    while at < lib.code.len() {
        if opcode::low_byte(lib.code[at]) == op::PUSH {
            let f = FunctionId(lib.code[at + 1]);
            if f.is_direct() && lib.function_name(f).is_none() {
                #[allow(clippy::cast_possible_truncation)]
                let name = lib.function_at((at * 4) as u32);
                if let Some(id) = by_name.get(name) {
                    out.entry(f.0).or_insert(*id);
                }
            }
        }
        at += opcode::length(&lib.code, at).unwrap();
    }
    out
}

/// A graph with every id renamed to its place, so two packings of one object compare equal.
fn renamed(g: &Graph) -> Graph {
    #[allow(clippy::cast_possible_truncation)]
    let place: HashMap<u32, u32> = g
        .ids
        .iter()
        .enumerate()
        .map(|(i, id)| (*id, i as u32))
        .collect();
    let mut f = |r: u32| place.get(&r).copied().unwrap_or(r);
    let mut out = g.clone();
    for (i, id) in out.ids.iter_mut().enumerate() {
        #[allow(clippy::cast_possible_truncation)]
        let i = i as u32;
        *id = i;
    }
    for e in &mut out.entries {
        match &mut e.body {
            Body::Object(o) => {
                if let Some(n) = &mut o.native {
                    n.remap(&mut f);
                } else {
                    for (s, t) in o.slots.iter_mut().zip(&o.tags) {
                        if tag::is_reference(*t) && *s != NULL {
                            *s = f(*s);
                        }
                    }
                }
            }
            Body::Relative(t) => *t = f(*t),
            _ => {}
        }
    }
    out
}

fn same_args(a: &[Arg], b: &[Arg]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| match (x, y) {
            (Arg::Object(g), Arg::Object(h)) => renamed(g) == renamed(h),
            _ => x == y,
        })
}

/// Behaviour: script.bridge.every-captured-send-repacks
#[test]
fn every_relaunch_client_call_is_packed_again_by_its_sending_function() {
    let (dat, prog) = target().expect("the September 2013 install opens");
    let index = auberean_testkit::data::require_capture_index();
    let lib = &prog.library;
    let senders = senders(lib);
    let mut world = World::new(dat, prog);
    let mut vm = Vm::new(Arc::clone(prog));
    let interface = vm.new_native(INTERFACE_TYPE, prog);
    vm.held.push(interface);
    // A sending function only packs and sends: the process it runs in needs no instance.
    let process = vm.create_process(interface);
    let mut outcome = BTreeMap::<String, usize>::new();
    let mut stops = BTreeMap::<String, usize>::new();
    let n = captures::messages(&index, &[carrier::TO_SERVER], &[2013, 2017], |m| {
        let call = m.script_call().expect("a script call");
        let Some(sender) = senders.get(&call.function) else {
            *outcome.entry("no sending function".into()).or_default() += 1;
            return;
        };
        let values = args::decode(&call.args, lib).expect("the arguments read");
        vm.budget = 1_000_000;
        world.sent.clear();
        let r = params_of(&mut vm, &values)
            .and_then(|p| send_call(&mut vm, &mut world, process, *sender, interface, &p));
        let key = match r {
            Ok(Status::Terminated) => match world.sent.as_slice() {
                [(f, bytes)] if *f == call.function => {
                    if *bytes == call.args {
                        "sent again byte for byte"
                    } else if args::decode(bytes, lib).is_ok_and(|v| same_args(&v, &values)) {
                        "sent again, objects under other ids"
                    } else {
                        "sent different arguments"
                    }
                }
                [] => "sent nothing",
                _ => "sent something else",
            },
            Ok(_) => "ended otherwise",
            Err(Stop::NotPorted { native, .. }) => {
                *stops.entry(lib.native_name(native)).or_default() += 1;
                "stopped at a native not ported yet"
            }
            Err(e) => {
                *stops.entry(format!("{e}")).or_default() += 1;
                "the machine failed"
            }
        };
        *outcome.entry(key.into()).or_default() += 1;
        vm.heap.abandon_builds();
        if vm.heap.added_since_collection > 200_000 {
            vm.collect();
        }
    })
    .expect("read the capture index");
    eprintln!(
        "{n} calls ({} through the encrypted carrier): {outcome:#?}\nstops: {stops:#?}",
        world.encrypted
    );
    assert_eq!(n, 300_121);
    let good = outcome
        .get("sent again byte for byte")
        .copied()
        .unwrap_or(0)
        + outcome
            .get("sent again, objects under other ids")
            .copied()
            .unwrap_or(0);
    assert_eq!(good, n, "{outcome:?}");
}
