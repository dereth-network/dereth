//! Physics scripts at runtime: the ported VC7 `qsort`, the intensity lookup, the
//! timeline and the **inline** hook execution.
//!
//! Three things here are contract:
//!
//! * **The timestamp comparator never returns 0.** It is
//!   `return (*b <= *a) ? 1 : -1`, so equal timestamps compare "greater". Handed to
//!   `msvcr70.dll`'s unstable quicksort, the resulting order of entries sharing a timestamp is
//!   decided by the partition choices — and 2,294 of the 4,248 shipped scripts contain duplicate
//!   timestamps. Neither `sort_by` nor `sort_unstable_by` reproduces it, so
//!   [`vc7_qsort`] is a port of the published VC7 algorithm.
//! * **`GetScript` resolves by intensity threshold in file order, with no randomisation.** ACE
//!   stubs this out entirely; see `docs/CORRECTIONS.md`.
//! * **Script hooks execute inline** in the script-manager update, not through the
//!   deferred animation-hook queue.

use dereth_primitives::{DataId, ServerTime};

use crate::data::{AnimAssets, PhysicsScriptData, PhysicsScriptTableData, ScriptStep};
use crate::hooks::AnimHook;

// ---------------------------------------------------------------------------------------------
// The VC7 qsort
// ---------------------------------------------------------------------------------------------

/// VC7's insertion cut-off. Below this size `qsort` drops to `shortsort`.
pub const CUTOFF: usize = 8;

/// `shortsort` from VC7's `qsort.c`: a selection sort that walks the range picking the maximum and
/// swapping it to the top.
///
/// The comparison is `comp(p, max) > 0`, so with a comparator that reports "greater" on ties the
/// running maximum advances past every equal element and the **last** maximum is the one swapped
/// out. That makes an **all-equal** range order-preserving — the shape most shipped scripts have —
/// but it does **not** make the sort stable in general: the swap is not adjacent, so a range with
/// two distinct timestamp groups can be permuted. See the correction in this module's tests.
fn shortsort<T>(v: &mut [T], lo: isize, hi: isize, comp: &dyn Fn(&T, &T) -> i32) {
    let at = |i: isize| usize::try_from(i).unwrap_or(0);
    let mut hi = hi;
    while hi > lo {
        let mut max = lo;
        let mut p = lo + 1;
        while p <= hi {
            if comp(&v[at(p)], &v[at(max)]) > 0 {
                max = p;
            }
            p += 1;
        }
        v.swap(at(max), at(hi));
        hi -= 1;
    }
}

/// A port of `msvcr70.dll`'s `qsort`: median-of-three quicksort, an explicit stack, and
/// `shortsort` below [`CUTOFF`].
///
/// this is the *published* VC7 `qsort.c`, not checked against
/// the shipped `msvcr70.dll`. The 166 shipped scripts with more than eight entries and a duplicate start
/// time, whose order depends on the partition choices, are the ones that would expose a difference;
/// a consumer excludes exactly those (`core/animation/tests/dat/scripts/qsort_parity.rs`) rather than distrust the corpus.
///
/// Index arithmetic replaces the original's `char*` pointer arithmetic one for one; the loop
/// structure and every comparison is the original's.
pub fn vc7_qsort<T>(v: &mut [T], comp: &dyn Fn(&T, &T) -> i32) {
    if v.len() < 2 {
        return;
    }
    // Indices are signed because the original's pointer arithmetic legitimately steps one element
    // below `lo` (`higuy -= width` before the `higuy > lo` test) and legitimately compares
    // `higuy - lo >= hi - loguy` with `loguy` one past `hi`. Unsigned indices would underflow at
    // both places; the *values* are identical.
    let at = |i: isize| usize::try_from(i).unwrap_or(0);
    // The original's `lostk`/`histk` are 30 deep, which bounds the recursion at 2^30 elements.
    let mut stack: Vec<(isize, isize)> = Vec::new();
    let mut lo: isize = 0;
    let mut hi: isize = isize::try_from(v.len()).unwrap_or(isize::MAX) - 1;

    loop {
        let size = usize::try_from(hi - lo + 1).unwrap_or(0);
        if size <= CUTOFF {
            shortsort(v, lo, hi, comp);
        } else {
            // Median of three: sort lo, mid and hi so that the pivot is the middle.
            let mut mid = lo + isize::try_from(size / 2).unwrap_or(0);
            if comp(&v[at(lo)], &v[at(mid)]) > 0 {
                v.swap(at(lo), at(mid));
            }
            if comp(&v[at(lo)], &v[at(hi)]) > 0 {
                v.swap(at(lo), at(hi));
            }
            if comp(&v[at(mid)], &v[at(hi)]) > 0 {
                v.swap(at(mid), at(hi));
            }

            let mut loguy = lo;
            let mut higuy = hi;
            loop {
                if mid > loguy {
                    loop {
                        loguy += 1;
                        if loguy >= mid || comp(&v[at(loguy)], &v[at(mid)]) > 0 {
                            break;
                        }
                    }
                }
                if mid <= loguy {
                    loop {
                        loguy += 1;
                        if loguy > hi || comp(&v[at(loguy)], &v[at(mid)]) > 0 {
                            break;
                        }
                    }
                }
                loop {
                    higuy -= 1;
                    if higuy <= mid || comp(&v[at(higuy)], &v[at(mid)]) <= 0 {
                        break;
                    }
                }
                if higuy < loguy {
                    break;
                }
                v.swap(at(loguy), at(higuy));
                if mid == higuy {
                    mid = loguy;
                }
            }

            higuy += 1;
            if mid < higuy {
                loop {
                    higuy -= 1;
                    if higuy <= mid || comp(&v[at(higuy)], &v[at(mid)]) != 0 {
                        break;
                    }
                }
            }
            if mid >= higuy {
                loop {
                    higuy -= 1;
                    if higuy <= lo || comp(&v[at(higuy)], &v[at(mid)]) != 0 {
                        break;
                    }
                }
            }

            // Recurse into the smaller half, loop on the larger one.
            if higuy - lo >= hi - loguy {
                if lo < higuy {
                    stack.push((lo, higuy));
                }
                if loguy < hi {
                    lo = loguy;
                    continue;
                }
            } else {
                if loguy < hi {
                    stack.push((loguy, hi));
                }
                if lo < higuy {
                    hi = higuy;
                    continue;
                }
            }
        }
        match stack.pop() {
            Some((l, h)) => {
                lo = l;
                hi = h;
            }
            None => return,
        }
    }
}

/// The physics script's own sort:
///
/// ```c
/// int compare(void *a, void *b) {
///     return (**(double**)b <= **(double**)a) ? 1 : -1;
/// }
/// ```
///
/// It **never returns 0**. That is not a bug to fix: it is the reason the tie order is an artefact
/// of the quicksort, and a comparator that returns 0 here produces a different, wrong order.
#[must_use]
pub fn script_data_sort(a: f64, b: f64) -> i32 {
    if b <= a {
        1
    } else {
        -1
    }
}

/// Sort one script's timeline the way does.
pub fn sort_script_data(steps: &mut [ScriptStep]) {
    vc7_qsort(steps, &|a: &ScriptStep, b: &ScriptStep| {
        script_data_sort(a.start_time, b.start_time)
    });
}

// ---------------------------------------------------------------------------------------------
// The script table
// ---------------------------------------------------------------------------------------------

/// The first entry whose threshold is
/// **greater than or equal to** the intensity, in file order.
///
/// **The client's comparison is `<=`, not `<`.**
///
/// The loop compares the query intensity with each entry's modifier and returns that entry's
/// script id when the intensity is less than **or equal to** it; only a greater intensity falls
/// through to the next row. Equality therefore selects the row; `<` would skip it.
///
/// This is not cosmetic. The shipped tables are dominated by a single row with
/// `modifier == 1.0`, and the intensity the server sends with the commonest scripts is
/// exactly `1.0`: against the retail dats, the create play script at 1.0 resolves in **0** of
/// 164 tables under `<` and **102** under `<=`, and the level-up play script at 1.0 in **0**
/// under `<` and **97** under `<=`. Where both resolve they can still disagree — a
/// `(0.0, 0.5, 1.0)` table queried at `0.0` yields the `0.0` row here and the `0.5` row under `<`.
/// `docs/formats/18-physics-scripts.md` agrees.
///
/// The client does not sort the rows, so file order *is* evaluation order; and there is no
/// randomisation anywhere on this path (the only PRNG on the script path is in `CallPES`). ACE's
/// `PhysicsScriptTable.GetScript` is a stub returning 0 — do not port it.
#[must_use]
pub fn get_script(
    table: &PhysicsScriptTableData,
    script_type: u32,
    intensity: f32,
) -> Option<DataId> {
    // The outer table lookup is the hash lookup around this scan.
    let rows = table.scripts.get(&script_type)?;
    for r in rows {
        if intensity <= r.modifier {
            return Some(r.script_id);
        }
    }
    None
}

// ---------------------------------------------------------------------------------------------
// ScriptManager
// ---------------------------------------------------------------------------------------------

/// One queued script: its start time and shared script data. Queue order is stored by the manager.
#[derive(Debug, Clone)]
struct ScriptEntry {
    start_time: f64,
    script: std::sync::Arc<PhysicsScriptData>,
}

/// `ScriptManager` — the per-object queue of running scripts.
///
/// **Scripts queue, they do not overlap**: a second script starts exactly when the first one's
/// `length` elapses. That is why an object hit by several spells
/// plays their effects back to back.
#[derive(Debug, Default, Clone)]
pub struct ScriptManager {
    queue: std::collections::VecDeque<ScriptEntry>,
    /// `-1` before the first hook of the current script.
    hook_index: i64,
    /// `-1.0` is the "nothing pending" sentinel.
    next_hook_time: f64,
}

impl ScriptManager {
    #[must_use]
    pub fn new() -> Self {
        Self {
            queue: std::collections::VecDeque::new(),
            hook_index: -1,
            next_hook_time: -1.0,
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Resolve the id and queue it. A script the assets do
    /// not have is dropped, matching a failed asset lookup.
    pub fn add_script(&mut self, id: DataId, assets: &dyn AnimAssets, now: ServerTime) -> bool {
        match assets.script(id) {
            Some(s) => {
                self.add_script_internal(s, now);
                true
            }
            None => false,
        }
    }

    /// Add a script, internally.
    pub fn add_script_internal(
        &mut self,
        script: std::sync::Arc<PhysicsScriptData>,
        now: ServerTime,
    ) {
        let start_time = match self.queue.back() {
            Some(last) => last.script.length + last.start_time,
            None => now.0,
        };
        let first = script.steps.first().map_or(0.0, |s| s.start_time);
        let empty = self.queue.is_empty();
        self.queue.push_back(ScriptEntry { start_time, script });
        if empty {
            self.hook_index = -1;
            self.next_hook_time = first + start_time;
        }
    }

    /// Take the next hook.
    fn next_hook(&mut self) -> Option<AnimHook> {
        let curr = self.queue.front()?;
        self.hook_index += 1;
        let i = usize::try_from(self.hook_index).ok()?;
        let n = curr.script.steps.len();
        if i >= n {
            return None;
        }
        if i + 1 < n {
            self.next_hook_time = curr.script.steps[i + 1].start_time + curr.start_time;
        } else if let Some(next) = self.queue.get(1) {
            self.next_hook_time =
                next.script.steps.first().map_or(0.0, |s| s.start_time) + next.start_time;
        } else {
            self.next_hook_time = -1.0;
        }
        Some(curr.script.steps[i].hook)
    }

    /// Update the running scripts.
    ///
    /// Hooks whose times have already passed **all fire in the same step** — a script never
    /// catches up gradually — and they are appended to `out` as they are executed, **inline**,
    /// rather than queued on the object's deferred animation-hook array.
    pub fn update_scripts(&mut self, now: ServerTime, out: &mut Vec<AnimHook>) {
        while !self.queue.is_empty() && self.next_hook_time <= now.0 {
            match self.next_hook() {
                Some(h) => out.push(h),
                None => {
                    self.queue.pop_front();
                    self.hook_index = -1;
                    match self.queue.front() {
                        None => self.next_hook_time = -1.0,
                        Some(c) => {
                            self.next_hook_time =
                                c.script.steps.first().map_or(0.0, |s| s.start_time) + c.start_time;
                        }
                    }
                }
            }
        }
    }

    /// `~` — drain the chain. Note it does **not** undo anything the hooks
    /// already did: a script interrupted halfway leaves the object at whatever translucency, scale
    /// or omega the last hook set.
    pub fn clear(&mut self) {
        self.queue.clear();
        self.hook_index = -1;
        self.next_hook_time = -1.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{PhysicsScriptTableData, ScriptAndMod};
    use crate::hooks::HookKind;

    fn step(t: f64, tag: u32) -> ScriptStep {
        ScriptStep {
            start_time: t,
            // The emitter id is a convenient tag: it survives the sort and identifies the entry.
            hook: AnimHook::new(0, HookKind::DestroyParticle { emitter_id: tag }),
        }
    }

    fn tags(steps: &[ScriptStep]) -> Vec<u32> {
        steps
            .iter()
            .map(|s| match s.hook.kind {
                HookKind::DestroyParticle { emitter_id } => emitter_id,
                _ => u32::MAX,
            })
            .collect()
    }

    /// ORACLE: the comparator in the recovered runtime-library behavior
    /// section 2.3, transcribed from the physics script's own sort.
    #[test]
    fn the_comparator_never_reports_equality() {
        assert_eq!(script_data_sort(1.0, 2.0), -1);
        assert_eq!(script_data_sort(2.0, 1.0), 1);
        assert_eq!(
            script_data_sort(1.0, 1.0),
            1,
            "equal times compare greater, never 0"
        );
    }

    /// The ported `qsort` sorts, for every size across the cut-off.
    #[test]
    fn the_ported_qsort_actually_sorts() {
        for n in 0..40usize {
            let mut v: Vec<ScriptStep> = (0..n)
                .map(|i| {
                    step(
                        f64::from(u32::try_from((i * 37) % 11).unwrap_or(0)),
                        u32::try_from(i).unwrap_or(0),
                    )
                })
                .collect();
            sort_script_data(&mut v);
            for w in v.windows(2) {
                assert!(w[0].start_time <= w[1].start_time, "n = {n}");
            }
        }
    }

    /// The ≤ 8 case goes through `shortsort`. It preserves the file order of equal elements when
    /// **every** element is equal — which is the common shape in the shipped scripts — but it is a
    /// selection sort, and a selection sort is not stable in general.
    ///
    /// **A correction.** The recovered runtime-library behavior section 2.3 says that with a comparator reporting "greater" on ties,
    /// `shortsort` "preserves the input order of equal elements", and conclude that the 2,128
    /// records with ≤ 8 entries must sort exactly like a stable sort. The first half is true only
    /// for an all-equal range: the swap that moves the running maximum to the top is not adjacent,
    /// so a range with two distinct timestamp groups can be permuted. The worked example below is
    /// the counterexample, and `core/animation/tests/dat/scripts/qsort_parity.rs` measures how many of the 2,128 are actually
    /// affected.
    #[test]
    fn shortsort_preserves_the_file_order_only_when_every_timestamp_is_equal() {
        for n in 1..=CUTOFF {
            let mut v: Vec<ScriptStep> = (0..n)
                .map(|i| step(0.0, u32::try_from(i).unwrap_or(0)))
                .collect();
            sort_script_data(&mut v);
            let expect: Vec<u32> = (0..u32::try_from(n).unwrap_or(0)).collect();
            assert_eq!(tags(&v), expect, "n = {n}");
        }
        // Two runs of equal times: a stable sort would give [1, 3, 0, 2, 4]; `shortsort` does not.
        let mut v = vec![
            step(1.0, 0),
            step(0.0, 1),
            step(1.0, 2),
            step(0.0, 3),
            step(1.0, 4),
        ];
        sort_script_data(&mut v);
        assert_eq!(
            tags(&v),
            vec![3, 1, 0, 2, 4],
            "the two zero-time entries were swapped"
        );
    }

    /// Get script takes the first threshold at or above the intensity.
    #[test]
    fn get_script_takes_the_first_threshold_at_or_above_the_intensity() {
        let mut t = PhysicsScriptTableData::default();
        t.scripts.insert(
            32,
            vec![
                ScriptAndMod {
                    modifier: 0.34,
                    script_id: DataId(0x3300_0001),
                },
                ScriptAndMod {
                    modifier: 0.67,
                    script_id: DataId(0x3300_0002),
                },
                ScriptAndMod {
                    modifier: 1.01,
                    script_id: DataId(0x3300_0003),
                },
            ],
        );
        assert_eq!(get_script(&t, 32, 0.0), Some(DataId(0x3300_0001)));
        assert_eq!(get_script(&t, 32, 0.5), Some(DataId(0x3300_0002)));
        assert_eq!(get_script(&t, 32, 1.0), Some(DataId(0x3300_0003)));
        assert_eq!(get_script(&t, 32, 0.34), Some(DataId(0x3300_0001)));
        // Past every threshold: INVALID_DID.
        assert_eq!(get_script(&t, 32, 2.0), None);
        // An unknown type is INVALID_DID too, not a panic.
        assert_eq!(get_script(&t, 99, 0.5), None);

        // File order, not sorted order.
        let mut t = PhysicsScriptTableData::default();
        t.scripts.insert(
            1,
            vec![
                ScriptAndMod {
                    modifier: 1.01,
                    script_id: DataId(0x3300_00AA),
                },
                ScriptAndMod {
                    modifier: 0.34,
                    script_id: DataId(0x3300_00BB),
                },
            ],
        );
        assert_eq!(
            get_script(&t, 1, 0.5),
            Some(DataId(0x3300_00AA)),
            "the first row wins even though its threshold is the larger"
        );
    }

    /// The timeline: hooks fire in order, everything already due fires in the same step, and the
    /// second script starts exactly `length` after the first.
    #[test]
    fn scripts_queue_rather_than_overlap_and_fire_inline() {
        let a = std::sync::Arc::new(PhysicsScriptData {
            steps: vec![step(0.0, 1), step(0.5, 2), step(1.0, 3)],
            length: 1.0,
        });
        let b = std::sync::Arc::new(PhysicsScriptData {
            steps: vec![step(0.0, 4), step(0.25, 5)],
            length: 0.25,
        });
        let mut m = ScriptManager::new();
        m.add_script_internal(a, ServerTime(100.0));
        m.add_script_internal(b, ServerTime(100.0));

        let mut out = Vec::new();
        m.update_scripts(ServerTime(100.0), &mut out);
        assert_eq!(tags_of(&out), vec![1]);

        out.clear();
        m.update_scripts(ServerTime(100.6), &mut out);
        assert_eq!(tags_of(&out), vec![2]);

        // Everything already due fires at once: the rest of A and the first of B.
        out.clear();
        m.update_scripts(ServerTime(101.0), &mut out);
        assert_eq!(tags_of(&out), vec![3, 4]);

        out.clear();
        m.update_scripts(ServerTime(101.25), &mut out);
        assert_eq!(tags_of(&out), vec![5]);
        // The `while` loop keeps going after the last hook: the next-hook step finds none, the
        // finished script is popped and `next_hook_time` becomes the -1.0 sentinel, all in the
        // same call.
        assert!(m.is_empty());

        out.clear();
        m.update_scripts(ServerTime(102.0), &mut out);
        assert!(out.is_empty());
    }

    fn tags_of(hooks: &[AnimHook]) -> Vec<u32> {
        hooks
            .iter()
            .map(|h| match h.kind {
                HookKind::DestroyParticle { emitter_id } => emitter_id,
                _ => u32::MAX,
            })
            .collect()
    }
}
