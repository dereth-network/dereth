//! Saved script states: every packed graph reads to its last byte and writes back to the same
//! bytes, and reads into the reference table.

// Hand-written programs and tables a few hundred entries long: every length and offset fits in
// 32 bits.
#![allow(clippy::cast_possible_truncation)]

use std::collections::BTreeMap;

use auberean_assets::script::ScriptState;
use auberean_script::graph::{Body, Graph};
use auberean_script::heap::Heap;
use auberean_script::native_object::NativeObject;

use crate::common::need_target;

/// Every saved state's record: its id and decoded framing.
fn each_state(mut f: impl FnMut(u32, &ScriptState)) -> usize {
    let (dat, _) = crate::common::target().unwrap();
    let ids: Vec<u32> = dat
        .entries()
        .map(|e| e.id)
        .filter(|id| (0x6F00_0000..=0x7FFF_FFFF).contains(id))
        .collect();
    for id in &ids {
        let bytes = dat.read(*id).unwrap();
        let s: ScriptState =
            auberean_assets::decode(*id, &bytes, &auberean_assets::Context::empty()).unwrap();
        f(*id, &s);
    }
    ids.len()
}

/// Behaviour: script.graph.every-saved-state-round-trips
#[test]
fn every_saved_state_reads_exactly_and_writes_back_to_the_same_bytes() {
    let (_, lib) = need_target!();
    let mut stats = BTreeMap::<&str, usize>::new();
    let mut natives = BTreeMap::<u32, usize>::new();
    let mut failures = Vec::new();
    let n = each_state(|id, s| {
        let g = match Graph::decode(&s.graph, lib) {
            Ok(g) => g,
            Err(e) => {
                failures.push(format!("{id:08X}: {e}"));
                return;
            }
        };
        let back = g.encode().unwrap();
        if back != s.graph {
            let at = back
                .iter()
                .zip(&s.graph)
                .position(|(a, b)| a != b)
                .unwrap_or(back.len().min(s.graph.len()));
            failures.push(format!("{id:08X}: writes back differently from byte {at}"));
        }
        *stats.entry("references").or_default() += g.ids.len();
        for e in &g.entries {
            match &e.body {
                Body::Singleton(_) => *stats.entry("singletons").or_default() += 1,
                Body::Object(o) if o.native_type == 0 => {
                    *stats.entry("script objects").or_default() += 1
                }
                Body::Object(o) => {
                    *stats.entry("native objects").or_default() += 1;
                    *natives.entry(u32::from(o.native_type)).or_default() += 1;
                    if matches!(o.native, Some(NativeObject::Packed { .. })) {
                        *stats.entry("natives kept as bytes").or_default() += 1;
                    }
                    if let Some(NativeObject::PackedMultiHash { entries, .. }) = &o.native {
                        *stats.entry("multi-hashes").or_default() += 1;
                        *stats.entry("multi-hash entries").or_default() += entries.len();
                    }
                }
                Body::Elsewhere => *stats.entry("packed elsewhere").or_default() += 1,
                Body::Relative(_) => *stats.entry("relative").or_default() += 1,
            }
        }
    });
    eprintln!("{n} states; {stats:?}");
    let top: Vec<String> = natives
        .iter()
        .filter(|(_, c)| **c >= 500)
        .map(|(t, c)| format!("{}={c}", lib.package_name(*t)))
        .collect();
    eprintln!("native objects: {}", top.join(" "));
    assert!(
        failures.is_empty(),
        "{} of {n} failed: {:?}",
        failures.len(),
        &failures[..failures.len().min(20)]
    );
    assert_eq!(n, 41_098);
    assert_eq!(stats["references"], 474_113);
    assert_eq!(stats["script objects"], 147_316);
    assert_eq!(stats["native objects"], 278_624);
    assert_eq!(stats["singletons"], 48_173);
    // Every saved multi-hash is empty: reading one as the engine does cannot reorder it.
    assert_eq!(stats["multi-hashes"], 31);
    assert_eq!(stats["multi-hash entries"], 0);
}

/// Behaviour: script.graph.hash-tables-pack-in-bucket-order
#[test]
fn every_packed_hash_table_lists_its_entries_in_bucket_order() {
    let (_, lib) = need_target!();
    let (mut tables, mut entries) = (0usize, 0usize);
    let mut unordered = Vec::new();
    each_state(|id, s| {
        let Ok(g) = Graph::decode(&s.graph, lib) else {
            return;
        };
        for e in &g.entries {
            let Body::Object(o) = &e.body else { continue };
            if let Some(NativeObject::Hash(h)) = &o.native {
                tables += 1;
                entries += h.len();
            }
        }
        // Writing back walks the buckets in order; equal bytes mean the stored order was
        // bucket order already.
        if g.encode().unwrap() != s.graph {
            unordered.push(id);
        }
    });
    eprintln!("{tables} hash tables, {entries} entries");
    assert!(tables > 30_000);
    assert!(unordered.is_empty(), "{unordered:08X?}");
}

/// Behaviour: script.heap.saved-state-reads-into-the-table
#[test]
fn a_saved_state_read_into_the_table_packs_again_to_the_same_bytes_under_new_ids() {
    let (_, lib) = need_target!();
    let mut failures = Vec::new();
    let n = each_state(|id, s| {
        let g = Graph::decode(&s.graph, lib).unwrap();
        // A fresh table gives the graph's entries the ids 0, 1, 2... in packed order, so packing
        // the root again must give the same bytes with every id renamed to its place.
        let mut heap = Heap::new();
        let root = heap.import(&g, None).unwrap();
        let again = heap.export(root).unwrap().encode().unwrap();
        let renamed = renamed(&g).encode().unwrap();
        if again != renamed {
            failures.push(format!("{id:08X}"));
        }
    });
    assert!(
        failures.is_empty(),
        "{} of {n}: {:?}",
        failures.len(),
        &failures[..failures.len().min(20)]
    );
}

/// A graph with every id renamed to its place in the graph.
fn renamed(g: &Graph) -> Graph {
    use std::collections::HashMap;
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
        *id = i as u32;
    }
    for e in &mut out.entries {
        match &mut e.body {
            Body::Object(o) => {
                if let Some(n) = &mut o.native {
                    n.remap(&mut f);
                } else {
                    for (s, t) in o.slots.iter_mut().zip(&o.tags) {
                        if auberean_script::value::tag::is_reference(*t)
                            && *s != auberean_script::value::NULL
                        {
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

/// The portal file, with the next so many reads failing once it is armed.
#[derive(Debug)]
struct FlakyPortal {
    file: std::fs::File,
    fail_next: std::sync::atomic::AtomicU32,
}

impl auberean_dat::DatStorage for FlakyPortal {
    fn read_exact_at(&self, offset: u64, buf: &mut [u8]) -> std::io::Result<()> {
        use std::sync::atomic::Ordering;
        let left = self.fail_next.load(Ordering::Relaxed);
        if left > 0 {
            self.fail_next.store(left - 1, Ordering::Relaxed);
            return Err(std::io::Error::other("a read the machine got wrong"));
        }
        auberean_dat::DatStorage::read_exact_at(&self.file, offset, buf)
    }
}

/// A saved state whose first read fails is read again and kept, not remembered as absent; a
/// state that fails every read is remembered as one that would not read, with why.
///
/// Behaviour: script.graph.a-saved-state-that-fails-to-read-once-is-read-again
#[test]
fn a_saved_state_whose_first_read_fails_is_read_again_not_remembered_absent() {
    use std::sync::atomic::Ordering;
    let (dat, lib) = need_target!();
    let id = dat
        .entries()
        .map(|e| e.id)
        .find(|id| (0x6F00_0000..=0x7FFF_FFFF).contains(id))
        .expect("a saved state");
    let path = auberean_dat::locate::install_dats(&auberean_dat::testing::require_install(
        auberean_dat::testing::TARGET,
    ))
    .expect("the install's files")
    .into_iter()
    .find(|(n, _)| *n == auberean_dat::locate::DatName::Portal)
    .expect("the portal file")
    .1;
    let flaky = std::sync::Arc::new(FlakyPortal {
        file: std::fs::File::open(&path).expect("the portal file opens"),
        fail_next: std::sync::atomic::AtomicU32::new(0),
    });
    // The storage is shared with the test so it can be armed after the directory is read.
    #[derive(Debug)]
    struct Shared(std::sync::Arc<FlakyPortal>);
    impl auberean_dat::DatStorage for Shared {
        fn read_exact_at(&self, offset: u64, buf: &mut [u8]) -> std::io::Result<()> {
            auberean_dat::DatStorage::read_exact_at(&*self.0, offset, buf)
        }
    }
    let portal = auberean_dat::DatFile::from_storage(path, Box::new(Shared(flaky.clone())))
        .expect("the portal file reads");
    let mut once = auberean_script::states::StateCache::new(std::sync::Arc::new(portal.clone()));
    flaky.fail_next.store(1, Ordering::Relaxed);
    assert!(once.get(id, lib).is_some(), "read again after one failure");
    assert_eq!(once.reread, 1);
    assert!(once.failures.is_empty(), "{:?}", once.failures);

    let mut always = auberean_script::states::StateCache::new(std::sync::Arc::new(portal));
    flaky.fail_next.store(u32::MAX, Ordering::Relaxed);
    assert!(always.get(id, lib).is_none());
    assert_eq!(always.failures.len(), 1);
    flaky.fail_next.store(0, Ordering::Relaxed);
    assert!(
        always.get(id, lib).is_none(),
        "a repeated failure is the data's own"
    );
}
