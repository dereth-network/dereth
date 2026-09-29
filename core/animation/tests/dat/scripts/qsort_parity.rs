//! Behaviour: none (checks formats, geometry, numeric contracts or repository tools)
//! Every script sorts its entries; equal-time entries retain the ordering of the selected sorting path.
//! Fixture: the shipped retail DAT records and recorded inputs.

use super::common;

use dereth_animation::data::ScriptStep;
use dereth_animation::hooks::{AnimHook, HookKind};
use dereth_animation::script::{script_data_sort, sort_script_data, vc7_qsort, CUTOFF};
use dereth_assets::Decode;
use dereth_dat::DbType;

/// Sort `(index, time)` pairs with the client's comparator and return the resulting index order.
fn vc7_order(times: &[f64]) -> Vec<usize> {
    let mut v: Vec<(usize, f64)> = times.iter().copied().enumerate().collect();
    vc7_qsort(&mut v, &|a: &(usize, f64), b: &(usize, f64)| {
        script_data_sort(a.1, b.1)
    });
    v.into_iter().map(|(i, _)| i).collect()
}

/// Every shipped script sorts and small ties stay stable.
#[test]
fn every_shipped_script_sorts_and_small_ties_stay_stable() {
    let assets = common::open();
    let ids = assets.store().ids_of(DbType::PhysicsScript);
    let records = ids.len();
    assert!(records > 0, "the input contains physics scripts");

    let mut total_entries = 0usize;
    let mut with_dupes = 0usize;
    let mut small_dupes = 0usize;
    let mut large_dupes = 0usize;
    let mut small_unstable = 0usize;
    let mut per_record = std::collections::BTreeMap::<usize, usize>::new();

    for id in ids {
        let bytes = assets
            .store()
            .read_typed(DbType::PhysicsScript, id)
            .expect("read");
        let s = dereth_assets::PhysicsScript::decode_payload(id, &bytes).expect("decode");
        let n = s.script_data.len();
        total_entries += n;
        *per_record.entry(n).or_default() += 1;

        // File order, before the sort.
        let file: Vec<f64> = s.script_data.iter().map(|e| e.start_time).collect();
        let mut sorted = file.clone();
        sorted.sort_by(f64::total_cmp);
        let has_dupes = sorted.windows(2).any(|w| w[0] == w[1]);
        if has_dupes {
            with_dupes += 1;
            if n <= CUTOFF {
                small_dupes += 1;
            } else {
                large_dupes += 1;
            }
        }

        let order = vc7_order(&file);
        assert_eq!(order.len(), n, "{id}");
        let mut seen = vec![false; n];
        for &i in &order {
            assert!(!seen[i], "{id}: index {i} twice");
            seen[i] = true;
        }
        for w in order.windows(2) {
            assert!(file[w[0]] <= file[w[1]], "{id}: not sorted");
        }
        if has_dupes && n <= CUTOFF {
            let mut stable: Vec<usize> = (0..n).collect();
            stable.sort_by(|a, b| file[*a].total_cmp(&file[*b]));
            if order != stable {
                small_unstable += 1;
            }
        }

        // The runtime path agrees with the raw one, and `length` comes from the *last* entry.
        let script = dereth_world_data::anim_convert::physics_script(&s);
        assert_eq!(script.steps.len(), n, "{id}: a hook was dropped");
        for w in script.steps.windows(2) {
            assert!(w[0].start_time <= w[1].start_time, "{id}");
        }
        assert_eq!(
            script.length,
            script.steps.last().map_or(0.0, |e| e.start_time),
            "{id}"
        );
    }

    assert!(total_entries > 0, "the input contains script entries");
    assert!(
        small_dupes > 0 && large_dupes > 0,
        "both sorting paths receive equal-time entries"
    );
    assert!(
        records > with_dupes,
        "distinct-time entries are also exercised"
    );
    assert_eq!(small_dupes + large_dupes, with_dupes);
    assert_eq!(per_record.values().sum::<usize>(), records);
    // The 166 records with more than eight entries and a tie go through the quicksort partition,
    // which is where `msvcr70.dll` could differ from the published algorithm; they are excluded
    // from the parity claim (sorted, but their tie order is not asserted), not distrusted wholesale.
    eprintln!(
        "corpus: {small_unstable} of the {small_dupes} short records sort differently from a \
         stable sort"
    );
    assert_eq!(
        small_unstable, 0,
        "small partitions preserve equal-time entry order"
    );
}

/// A `qsort` that is only ever exercised on real data is a `qsort` whose partition arm is never
/// tested. This drives the quicksort path directly.
#[test]
fn the_quicksort_path_handles_the_shapes_that_break_naive_partitions() {
    let hook = AnimHook::new(0, HookKind::NoOp);
    let mk = |times: &[f64]| -> Vec<ScriptStep> {
        times
            .iter()
            .map(|t| ScriptStep {
                start_time: *t,
                hook,
            })
            .collect()
    };
    for shape in [
        (0..64).map(f64::from).collect::<Vec<_>>(), // already sorted
        (0..64).rev().map(f64::from).collect(),     // reversed
        vec![1.0; 64],                              // all equal
        (0..64).map(|i| f64::from(i % 4)).collect(), // four big groups
        (0..64).map(|i| f64::from(i % 2) * 1e9).collect(), // two far-apart groups
    ] {
        let mut v = mk(&shape);
        sort_script_data(&mut v);
        for w in v.windows(2) {
            assert!(w[0].start_time <= w[1].start_time, "{shape:?}");
        }
        assert_eq!(v.len(), shape.len());
    }
}
