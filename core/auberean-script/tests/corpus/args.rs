//! Script-call arguments on the wire: every captured call of 2013 and 2017 reads to its last
//! byte, writes back to the same bytes, and carries exactly its function's declared parameters.

use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;

use auberean_dat::locate::{install_dats, DatName};
use auberean_dat::testing::{require_install, TARGET};
use auberean_dat::DatFile;
use auberean_script::args::{self, expected_tags, Arg};
use auberean_script::library::{Library, RECORD};
use auberean_script::opcode::{self, op};
use auberean_script::value::FunctionId;
use auberean_testkit::captures::{self, carrier};

pub(crate) fn library() -> Option<&'static Library> {
    static L: OnceLock<Option<Library>> = OnceLock::new();
    L.get_or_init(|| {
        let dir = require_install(TARGET);
        let (_, path) = install_dats(&dir)
            .ok()?
            .into_iter()
            .find(|(n, _)| *n == DatName::Portal)?;
        let dat = DatFile::open(&path).ok()?;
        dat.read_checked(RECORD, Library::from_record)
            .ok()
            .map(|c| c.value)
    })
    .as_ref()
}

/// The functions that send each function of the other side: a sending function pushes the other
/// side's function id (named outright, a function this library does not define) before it sends.
fn senders(lib: &Library) -> HashMap<u32, String> {
    let mut out = HashMap::new();
    let mut at = 0;
    while at < lib.code.len() {
        let w = lib.code[at];
        if opcode::low_byte(w) == op::PUSH {
            let v = lib.code[at + 1];
            let f = FunctionId(v);
            if f.is_direct() && lib.function_name(f).is_none() {
                #[allow(clippy::cast_possible_truncation)]
                let name = lib.function_at((at * 4) as u32).to_string();
                out.entry(v).or_insert(name);
            }
        }
        at += opcode::length(&lib.code, at).unwrap();
    }
    out
}

/// Behaviour: script.args.every-captured-call-round-trips
#[test]
fn every_relaunch_script_call_reads_exactly_writes_back_and_matches_its_parameters() {
    let lib = library().expect("the September 2013 install opens");
    let index = auberean_testkit::data::require_capture_index();
    let senders = senders(lib);
    let mut calls = BTreeMap::<&str, usize>::new();
    let mut failures = BTreeMap::<String, usize>::new();
    let mut natives = BTreeMap::<u32, usize>::new();
    let mut expected: HashMap<(bool, u32), Option<Vec<u32>>> = HashMap::new();
    let n = captures::messages(&index, &carrier::ALL, &[2013, 2017], |m| {
        let dir = if m.to_client {
            "server to client"
        } else {
            "client to server"
        };
        *calls.entry(dir).or_default() += 1;
        let Some(call) = m.script_call() else {
            *failures
                .entry(format!("{dir}: the carrier does not read"))
                .or_default() += 1;
            return;
        };
        let values = match args::decode(&call.args, lib) {
            Ok(v) => v,
            Err(e) => {
                *failures.entry(format!("{dir}: {}", e.what)).or_default() += 1;
                return;
            }
        };
        match args::encode(&values) {
            Ok(b) if b == call.args => {}
            _ => {
                *failures
                    .entry(format!("{dir}: writes back differently"))
                    .or_default() += 1
            }
        }
        for v in &values {
            if let Arg::Object(g) = v {
                for e in &g.entries {
                    if let auberean_script::graph::Body::Object(o) = &e.body {
                        if o.native_type != 0 {
                            *natives.entry(u32::from(o.native_type)).or_default() += 1;
                        }
                    }
                }
            }
        }
        let want = expected
            .entry((m.to_client, call.function))
            .or_insert_with(|| {
                if m.to_client {
                    let name = lib.function_name(FunctionId(call.function))?;
                    expected_tags(lib, &name, false)
                } else {
                    let name = senders.get(&call.function)?;
                    expected_tags(lib, name, true)
                }
            });
        let got: Vec<u32> = values.iter().map(Arg::tag).collect();
        match want {
            Some(w) if *w == got => *calls.entry("matching their parameters").or_default() += 1,
            Some(_) => {
                *failures
                    .entry(format!(
                        "{dir}: {:#x} carries other parameters",
                        call.function
                    ))
                    .or_default() += 1;
            }
            None => {
                *failures
                    .entry(format!(
                        "{dir}: {:#x} has no declared parameters",
                        call.function
                    ))
                    .or_default() += 1;
            }
        }
    })
    .expect("read the capture index");
    let top: Vec<String> = natives
        .iter()
        .map(|(t, c)| format!("{}={c}", lib.package_name(*t)))
        .collect();
    eprintln!("{n} calls: {calls:?}\nnative objects: {}", top.join(" "));
    assert!(failures.is_empty(), "{failures:#?}");
    assert_eq!(calls["client to server"], 300_121);
    assert_eq!(calls["server to client"], 757_726);
    assert_eq!(calls["matching their parameters"], 300_121 + 757_726);
}
