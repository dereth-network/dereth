//! Multi-hashes in recorded script calls: the order they are packed in, and what reading one into
//! the reference table, as the engine reads it, does to that order.

use std::collections::BTreeMap;

use auberean_script::args::{self, Arg};
use auberean_script::graph::{Body, Graph};
use auberean_script::heap::Heap;
use auberean_script::multi_hash::{hash_of, START_BUCKETS};
use auberean_script::native_object::{Item, NativeObject};
use auberean_script::wire::Out;
use auberean_testkit::captures::{self, carrier};

use crate::args::library;

/// A packed multi-hash's entries and its packed bytes.
type Table = (Vec<(Item, Item)>, Vec<u8>);

/// The packed multi-hashes of a graph, in graph order.
fn multi_hashes(g: &Graph) -> Vec<Table> {
    g.entries
        .iter()
        .filter_map(|e| match &e.body {
            Body::Object(o) => match &o.native {
                Some(n @ NativeObject::PackedMultiHash { entries, .. }) => {
                    let mut w = Out::new();
                    n.write(&mut w).expect("a multi-hash packs");
                    Some((entries.clone(), w.buf))
                }
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// The graph read into a fresh reference table as the engine reads one, then packed again.
fn read_and_pack(g: &Graph) -> Graph {
    let mut heap = Heap::new();
    let root = heap
        .import(g, None)
        .expect("the graph reads into the table");
    heap.export(root).expect("the graph packs again")
}

/// What the engine's reading does to a table's packed order, worked out from the order alone:
/// keys in their packed order, except that keys sharing a bucket come in reverse order of their
/// first entries; within a key, its first entry, then the rest in reverse.
fn as_read(p: &[(Item, Item)]) -> Vec<(Item, Item)> {
    let mut runs: Vec<Vec<(Item, Item)>> = Vec::new();
    for e in p {
        match runs.iter_mut().find(|r| r[0].0 == e.0) {
            Some(r) => r.push(*e),
            None => runs.push(vec![*e]),
        }
    }
    let mut by_bucket: BTreeMap<usize, Vec<Vec<(Item, Item)>>> = BTreeMap::new();
    for r in runs {
        let b = hash_of(&r[0].0) as usize % START_BUCKETS;
        by_bucket.entry(b).or_default().insert(0, r);
    }
    by_bucket
        .into_values()
        .flatten()
        .flat_map(|r| {
            let mut out = vec![r[0]];
            out.extend(r[1..].iter().rev());
            out
        })
        .collect()
}

/// **Every multi-hash in a recorded script call is packed in the engine's bucket order, and
/// reading it as the engine reads one turns each key's entries after its first around**: packed
/// again, the table lists them so; read and packed once more, each table is the bytes the server
/// sent.
///
/// Behaviour: script.graph.multi-hash-reads-as-the-engine-reads
#[test]
fn every_recorded_multi_hash_reads_as_the_engine_reads_it_and_twice_comes_back_byte_for_byte() {
    let lib = library().expect("the September 2013 install opens");
    let index = auberean_testkit::data::require_capture_index();
    let mut tally = BTreeMap::<&str, usize>::new();
    let mut failures = BTreeMap::<&str, usize>::new();
    captures::messages(&index, &carrier::ALL, &[2013, 2017], |m| {
        let Some(call) = m.script_call() else { return };
        let Ok(values) = args::decode(&call.args, lib) else {
            return;
        };
        for v in &values {
            let Arg::Object(g) = v else { continue };
            let sent = multi_hashes(g);
            if sent.is_empty() {
                continue;
            }
            *tally.entry("calls").or_default() += 1;
            for (p, _) in &sent {
                *tally.entry("tables").or_default() += 1;
                *tally.entry("entries").or_default() += p.len();
                let ordered = p.windows(2).all(|w| {
                    hash_of(&w[0].0) as usize % START_BUCKETS
                        <= hash_of(&w[1].0) as usize % START_BUCKETS
                });
                if !ordered {
                    *failures.entry("not in bucket order").or_default() += 1;
                }
                if as_read(p) != *p {
                    *tally.entry("tables the reading reorders").or_default() += 1;
                }
            }
            let once = read_and_pack(g);
            let read: Vec<_> = multi_hashes(&once).into_iter().map(|(p, _)| p).collect();
            let worked_out: Vec<_> = sent.iter().map(|(p, _)| as_read(p)).collect();
            if read != worked_out {
                *failures
                    .entry("read once, not in the order worked out")
                    .or_default() += 1;
            }
            let twice: Vec<_> = multi_hashes(&read_and_pack(&once))
                .into_iter()
                .map(|(_, b)| b)
                .collect();
            let bytes: Vec<_> = sent.into_iter().map(|(_, b)| b).collect();
            if twice != bytes {
                *failures
                    .entry("read twice, not the bytes sent")
                    .or_default() += 1;
            }
        }
    })
    .expect("read the capture index");
    eprintln!("{tally:?}");
    assert!(failures.is_empty(), "{failures:#?}");
    assert_eq!(tally["calls"], 385);
    assert_eq!(tally["tables"], 385);
    assert_eq!(tally["entries"], 3_301);
    assert_eq!(tally["tables the reading reorders"], 335);
}
