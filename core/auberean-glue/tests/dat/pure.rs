//! The library's pure script functions run to their end.
//!
//! A function is pure here when its code, and every function it calls by name, calls no native,
//! makes no call through a receiver (which could run anything) and makes no new object (whose
//! initializer could); and its parameters are all numbers. Each runs on an instance of its own
//! package (a copy of the package's default state when there is one), with every parameter zero
//! and every out parameter (a parameter passed by reference) a reference to a slot of the caller.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use auberean_script::args::tag_for;
use auberean_script::context::Context;
use auberean_script::library::NO_CODE;
use auberean_script::opcode::{self, op};
use auberean_script::value::{tag, FunctionId};
use auberean_script::{Status, Stop, Vm};

use crate::world::{target, World};

/// The frame member flag of a parameter passed by reference.
const BY_REFERENCE: u32 = 0x1;

/// Whether the code from `offset` (bytes) to its function's end is pure, following direct calls.
fn pure(
    code: &[u32],
    start: usize,
    ends: &HashMap<usize, usize>,
    seen: &mut HashMap<usize, bool>,
) -> bool {
    if let Some(v) = seen.get(&start) {
        return *v;
    }
    seen.insert(start, true);
    let Some(&end) = ends.get(&start) else {
        seen.insert(start, false);
        return false;
    };
    let mut at = start;
    let mut ok = true;
    while at < end && ok {
        let w = code[at];
        match opcode::low_byte(w) {
            op::CALL_NATIVE | op::CALL | op::NEW | op::NEW_NATIVE => ok = false,
            0x4F..=0x54 => ok = false,
            op::CALL_AT => {
                let target = code[at + 1];
                if target != NO_CODE {
                    ok = pure(code, (target / 4) as usize, ends, seen);
                }
            }
            _ => {}
        }
        at += opcode::length(code, at).unwrap_or(1);
    }
    seen.insert(start, ok);
    ok
}

/// Behaviour: script.interp.pure-functions-complete
#[test]
fn every_pure_script_function_runs_to_its_end() {
    let (dat, prog) = target().expect("the September 2013 install opens");
    let lib = &prog.library;
    // Each script function's code range.
    let mut ends = HashMap::new();
    let mut functions = Vec::new();
    for p in lib.packages.iter().filter(|p| !p.is_native()) {
        for f in &p.functions {
            if let Some(o) = f.offset {
                let s = (o / 4) as usize;
                ends.insert(s, s + (f.size / 4) as usize);
                functions.push((p.id, f.id, s, format!("{}::{}", p.name, f.name)));
            }
        }
    }
    let mut seen = HashMap::new();
    let mut world = World::new(dat, prog);
    let mut tally = BTreeMap::<String, usize>::new();
    let mut examples = BTreeMap::<String, String>::new();
    let mut ran = 0;
    for (package, id, start, name) in &functions {
        if !pure(&prog.code, *start, &ends, &mut seen) {
            continue;
        }
        // Its parameters, deepest slot first (the last parameter), all numbers.
        let Some(frame) = lib.frame(name) else {
            continue;
        };
        let mut params: Vec<_> = frame.members.iter().filter(|m| m.slot < -1).collect();
        params.sort_by_key(|m| m.slot);
        let Some(tags): Option<Vec<u32>> =
            params.iter().map(|m| tag_for(m.kind, m.flags)).collect()
        else {
            continue;
        };
        let by_reference: Vec<bool> = params.iter().map(|m| m.flags & BY_REFERENCE != 0).collect();
        if tags
            .iter()
            .zip(&by_reference)
            .any(|(t, r)| *t == auberean_script::args::pack_tag::OBJECT && !r)
        {
            continue;
        }
        let mut vm = Vm::new(Arc::clone(prog));
        vm.budget = 1_000_000;
        let this = match vm.new_object(&mut world, *package, prog) {
            Ok(t) => t,
            Err(_) => {
                *tally.entry("no instance could be made".into()).or_default() += 1;
                continue;
            }
        };
        let mut cx = Context::new(usize::MAX, 0);
        // The caller's slots the out parameters refer to.
        let mut outs = Vec::new();
        for r in &by_reference {
            if *r {
                outs.push(cx.sp);
                cx.push(0, tag::INT).unwrap();
            }
        }
        let mut outs = outs.into_iter();
        // The result slot: two slots when the function stores a 64-bit value, one is enough
        // for the rest (a function that returns nothing leaves it).
        cx.push(0, tag::INT).unwrap();
        cx.push(0, tag::INT).unwrap();
        for (t, r) in tags.iter().zip(&by_reference) {
            if *r {
                #[allow(clippy::cast_possible_truncation)]
                let slot = outs.next().unwrap() as u32;
                let id = vm.heap.add_relative(auberean_script::value::NULL, slot);
                cx.push(id, tag::REFERENCE).unwrap();
                continue;
            }
            match *t {
                2 => cx.push_long(0, tag::LONG_INT).unwrap(),
                4 => cx.push_long(0, tag::DOUBLE).unwrap(),
                3 => cx.push(0, tag::FLOAT).unwrap(),
                _ => cx.push(0, tag::INT).unwrap(),
            }
        }
        cx.push(this, tag::REFERENCE).unwrap();
        ran += 1;
        let key = match vm.call(&mut world, &mut cx, *package, FunctionId(id.0).direct()) {
            Ok(Status::Terminated) => "completed".to_string(),
            Ok(s) => format!("ended {s:?}"),
            Err(Stop::NotPorted { .. }) => "stopped at a native".to_string(),
            Err(e) => {
                let k = format!("{e:?}")
                    .split(['(', ' ', '{'])
                    .next()
                    .unwrap_or("")
                    .to_string();
                examples
                    .entry(k.clone())
                    .or_insert_with(|| format!("{name}: {e}"));
                k
            }
        };
        *tally.entry(key).or_default() += 1;
    }
    eprintln!("{ran} pure functions run: {tally:#?}\n{examples:#?}");
    assert!(ran > 1_000, "{ran}");
    assert_eq!(
        tally.get("completed").copied().unwrap_or(0),
        ran,
        "{tally:?} {examples:?}"
    );
}
