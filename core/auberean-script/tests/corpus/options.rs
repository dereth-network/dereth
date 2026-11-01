//! The player's options and shortcuts on the wire: every options profile a server sent at a
//! session's start in 2013 and 2017, and every shortcut a client sent, reads as the engine holds
//! it and packs back to the same bytes (its aliases in their table's order by the wide strings'
//! hash); and every hash table a client's scripts made and sent has the 47 buckets a new table
//! has.

use std::collections::BTreeMap;

use auberean_script::args::{self, Arg};
use auberean_script::graph::Body;
use auberean_script::native_object::{native_type, NativeObject, NEW_TABLE_SIZE};
use auberean_testkit::captures::{self, carrier};

use crate::args::library;

/// Behaviour: script.options.every-captured-profile-round-trips
#[test]
fn every_captured_options_profile_and_shortcut_reads_as_the_engine_holds_it_and_packs_back() {
    let lib = library().expect("the September 2013 install opens");
    let index = auberean_testkit::data::require_capture_index();
    let mut counts = BTreeMap::<&str, usize>::new();
    let mut failures = BTreeMap::<String, usize>::new();
    let mut sizes = BTreeMap::<u8, usize>::new();
    captures::messages(&index, &carrier::ALL, &[2013, 2017], |m| {
        let Some(call) = m.script_call() else { return };
        let Ok(values) = args::decode(&call.args, lib) else {
            return;
        };
        for v in &values {
            let Arg::Object(g) = v else { continue };
            for e in &g.entries {
                let Body::Object(o) = &e.body else { continue };
                let Some(native) = &o.native else { continue };
                if !m.to_client {
                    if let NativeObject::Hash(h) = native {
                        *sizes.entry(h.size_index).or_default() += 1;
                    }
                }
                let NativeObject::Packed { ty, bytes } = native else {
                    continue;
                };
                let held = native.unpacked();
                let (what, back) = match (*ty, &held) {
                    (native_type::GAMEPLAY_OPTIONS_PROFILE, NativeObject::Options(o)) => {
                        if !o.aliases.is_empty() {
                            *counts.entry("profiles with aliases").or_default() += 1;
                            *counts.entry("aliases").or_default() += o.aliases.entries.len();
                        }
                        ("profiles", o.pack())
                    }
                    (native_type::SHORTCUT_INFO, NativeObject::Shortcut(s)) => {
                        ("shortcuts", s.pack())
                    }
                    (native_type::GAMEPLAY_OPTIONS_PROFILE | native_type::SHORTCUT_INFO, _) => {
                        *failures
                            .entry(format!("{ty:#x} does not read"))
                            .or_default() += 1;
                        continue;
                    }
                    _ => continue,
                };
                *counts.entry(what).or_default() += 1;
                if back != *bytes {
                    *failures
                        .entry(format!("{what}: packs back differently"))
                        .or_default() += 1;
                }
            }
        }
    })
    .expect("read the capture index");
    eprintln!("{counts:?}; client hash tables by bucket size place: {sizes:?}");
    assert!(failures.is_empty(), "{failures:#?}");
    assert_eq!(counts.get("profiles").copied(), Some(433));
    assert_eq!(counts.get("shortcuts").copied(), Some(1_003));
    assert_eq!(
        sizes.keys().copied().collect::<Vec<_>>(),
        vec![NEW_TABLE_SIZE]
    );
}
