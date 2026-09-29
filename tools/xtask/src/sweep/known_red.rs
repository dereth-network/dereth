//! The standing red list, and the decision it makes about one finished suite.
//!
//! Each entry names a suite known to fail, who owns its status and why it is red, specific enough
//! that a reader can tell a stale entry from a live one without running anything. The verdict is
//! phrased "green except N, which are ..." rather than "green", and an unexpected red and a
//! *fixed* known red both fail the sweep: a known red that starts passing means its entry is stale,
//! and a stale red list is how a real regression gets waved through.
//!
//! **Measured, more than once:** a list rewritten after a suite-by-suite isolated rerun found
//! that seven of its nine entries passed. None was a contention artefact and none was fixed by
//! accident: each had a specific repair, and in every case the client had been right while the
//! test's claim about the reference behaviour was wrong. For a day that list advertised seven
//! fictional defects while listing none of the six real ones, which were invisible because
//! `cargo test -p <crate>` stops at the first failing test binary unless `--no-fail-fast` is
//! passed: every broad per-package run reported one red and silently hid the rest. The list is
//! measured suite by suite, never package by package.
//!
//! The lesson of the rewrites, measured a third time: of six suites red on one day, four were
//! instruments that could no longer fail, one was a false positive in a gate's own pattern, and
//! exactly one was a live client-defect candidate.
//!
//! ## Retired entries, and why they left
//!
//! Kept because a red list that forgets why an entry left is how the same wrong fix gets proposed
//! twice.
//!
//! * `rendering::part_degrade_levels`: green 6 of 6 on two separate runs.
//! * `world::landblock_interior_release`: the fixture could not parent anything (an assetless
//!   object stream, objects with no setup), so the two "extra" ids were the parented objects; the
//!   fixture now opens the dats. It came back a day later on a different assertion (the body
//!   count gained the landscape's own objects when a second producer began registering them) and
//!   was split into interior and outdoor counts; and once more on a walk that teleported into a
//!   departed room, a station retail cannot stand at. A suite retired yesterday is not a suite
//!   that stays green: the only thing that catches that inside a day is running the sweep.
//! * `quickbar`: green 10/0 at the default thread count.
//! * `line_endings`: repaired by a byte-level normalise mode, the first entry the stale-entry
//!   rule would have failed on.
//! * `o820_track_instruments`: private instrumentation rather than a crate test; the suite is gone.
//! * `rendering::texture_cache_key_spaces`: test rot; its original station used scenery on coarse
//!   detail rings that the client no longer builds. Moved to a station whose full-detail core
//!   reaches the same conflicts.
//! * `astra_door_control_audit`: the census moved by exactly the three new captures; two quoted
//!   figures were converted to derived ones, each watched red first, and its denominator guard
//!   was kept because it is what caught the change.
//! * `serv-conform`: the server workspace, out of this sweep's scope (the server has its own
//!   gate). Its capture-family classification is a separate open question.
//! * `streaming` and `world::viewer_cell::load_time`: one cause, bisected with the good endpoint
//!   measured: the camera-only viewpoint gate read "no viewer object" as "keep the block you have",
//!   so the body-less window never moved. A dictionary that once held this key twice silently kept
//!   the second, which is why the table is a list with a duplicate-key test.
//! * `rendering::building_boundary_draw`: inverted rather than fixed, because the claim was wrong
//!   about retail: the reference destroys an object placed on an outdoor id inside a building's
//!   solid volume rather than drawing it. A deliberate red is only as good as its premise.
//! * `astra_position_entry`, `objects::parked_attachments`: one real defect -- a destruction
//!   deadline stamped a second time at a later clock; the leave-visibility path now latches `lost`
//!   without pushing a verdict.
//! * `o76_pickup`: a frozen corpus literal moved by two new house captures; the failure output now
//!   names the file that moved it.
//! * `movement::remote_root_motion`, `astra_text_emote`, `interaction`, `o349_combat_maps`: no
//!   production code changed: three froze a literal before a change toward reference behaviour,
//!   one used an instrument that could no longer fail, one used a station the reference cannot
//!   stand at. Cumulative counters were replaced by deltas or by the named message.
//! * `o70_wizard` (a real state-setting defect: a missing "already in this state" early return),
//!   `particles` and `teleport` (test corrections, each watched failing at the new predicate).
//! * `o140_name_field`, `o191_text_mode_focus`: assertions about a seam retail clears or does not
//!   have; the press bit is asserted as an edge.
//! * `shutdown`, `o85_unreachable_eight`: assertions about a seam retail does not have; the
//!   entry's own count was stale.
//! * `world::viewer_cell`, `selection::object_range_checks::range_watch`,
//!   `selection::selection_cycle_actions`: stations retail cannot stand at, rebuilt at points the
//!   room's own geometry says are inside.
//! * `o410_combat`: bisected over 258 revisions in eight steps; a panel's legitimate update request
//!   was left behind, and the baseline is now drained and named.
//! * `panel_contents` (with `o422_character_panels`, `panel_input`, `o182_skill_rows`): one cause,
//!   a parked player description never rebuilt the panel tables, which also silently refused
//!   input. No assertion was weakened; one was added and watched red.
//! * `retail_dat_skip_claims`: a missing word boundary scored "No datagram" as an absence claim.
//! * `inventory::corpse_loot_window`, `astra_split_journey`, `astra_targeted_delivery`,
//!   `o83_paper_doll_drop`, `panels`: fixed by one change whose diagnosis differed from the one
//!   the entries carried for a day. Only a suite-by-suite re-measurement after a fix lands can
//!   retire an entry.
//! * `skill_advancement`, `attributes_panel`, `o422_character_panels`, `o123_one_selection_edge`,
//!   `o117_stack_gate`, `o48_slot_decoration`, `o740_vendor_stack_and_containers`: the client was
//!   right and the test's claim about retail was wrong. Two assertions had become unable to fail
//!   and were moved to the wire.
//! * `o373_split_commit`, `o124_selection_queries`: fixture assumptions repaired; a vendor panel
//!   selected the first row on every rebuild.
//! * `rendering::adaptive_degrade`: the governor was right. Its two differentials counted particle
//!   terminator selections, and once only full-detail blocks carried objects no particle around
//!   the stations sat in a band the bias can move, so both arms read identical numbers while the
//!   captures differed by millions of pixels. They now measure the static world's selected levels,
//!   terminator selections and triangles, which the bias moves at every station; cutting the
//!   bias's route to the level lookup makes both arms identical again, pixels included.
//!
//! ## Not in this list, on purpose: things that read red and are not
//!
//! Three silences are separable by evidence, not by judgement, and the value of the distinction is
//! that it is falsifiable:
//!
//! | what the log shows | what it is |
//! |---|---|
//! | `LNK1318` | a full disk (see `--min-free-gb`) |
//! | `0xc0000005` with no partial output | a crash; re-run alone before believing it |
//! | exit -1 with partial libtest output | contention: some tests printed `ok`, then the process was killed |
//!
//! Three suites that died with an access violation during a run with three other worktrees active
//! all passed alone (3/0, 12/0, 4/0): contention, and listing them would have been nine tests of
//! fiction.

/// `(suite, status owner, why it is red)`. A list, not a map, so a duplicate key is a test failure
/// rather than an entry silently lost.
pub const KNOWN_RED: &[(&str, &str, &str)] = &[];

/// Binaries whose every test is `#[ignore]`d on purpose, so "0 passed 0 failed" is their designed
/// resting state and not a gate behind a feature. A NO-TESTS result for any binary not listed here
/// is still a fault.
pub const KNOWN_NO_TESTS: &[(&str, &str)] = &[(
    "dereth-clipboard::roundtrip",
    "writes the REAL system clipboard; run by hand with --ignored --test-threads=1",
)];

/// The red list's entry for `suite`, if it has one.
pub fn known_red(
    table: &[(&'static str, &'static str, &'static str)],
    suite: &str,
) -> Option<(&'static str, &'static str)> {
    table
        .iter()
        .find(|(s, _, _)| *s == suite)
        .map(|(_, owner, why)| (*owner, *why))
}

/// The red list's decision about one finished suite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Score {
    /// Red, and not in the list. Fails the sweep.
    RedUnowned,
    /// Red, and registered with a cause and an owner. Does not fail.
    RedKnown,
    /// Green, and still in the list. **Fails the sweep.**
    StaleEntry,
    /// Green, and not registered.
    Ok,
}

impl Score {
    /// Whether this decision makes the whole sweep exit 1.
    ///
    /// The stale entry is the arm that was once missing: an entry whose suite started passing
    /// printed a notice and left the exit code alone, and from then on it *excused* that suite, so
    /// a genuine regression there read as a known red and the sweep exited 0.
    pub fn fails_the_sweep(self) -> bool {
        matches!(self, Score::RedUnowned | Score::StaleEntry)
    }
}

/// The decision, as a pure function of the table, the suite and its failure count.
///
/// A function rather than four lines inside the sweep's loop, so that every arm can be driven by
/// a test: three suites once went red for three days with a working unexpected-red guard sitting
/// right there, because the guard could not be run without a sweep and the sweep was never run.
pub fn score(
    table: &[(&'static str, &'static str, &'static str)],
    suite: &str,
    failed: usize,
) -> Score {
    let known = known_red(table, suite).is_some();
    match (failed > 0, known) {
        (true, true) => Score::RedKnown,
        (true, false) => Score::RedUnowned,
        (false, true) => Score::StaleEntry,
        (false, false) => Score::Ok,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic table, so that a live entry being fixed never breaks these arms.
    const SYNTH: &[(&str, &str, &str)] = &[("owned_suite", "synthetic fixture", "not a real red")];

    /// A red suite not in the list fails the sweep; the same red, registered, does not; a
    /// registered suite that now passes fails the sweep; an ordinary green suite is quiet.
    #[test]
    fn each_arm_of_the_red_list_decision() {
        assert_eq!(score(SYNTH, "unowned_suite", 3), Score::RedUnowned);
        assert_eq!(score(SYNTH, "owned_suite", 3), Score::RedKnown);
        assert_eq!(score(SYNTH, "owned_suite", 0), Score::StaleEntry);
        assert_eq!(score(SYNTH, "unowned_suite", 0), Score::Ok);
    }

    /// The exit code, driven through the same decision the sweep counts with: one planted unowned
    /// red among twenty green exits 1, one registered red exits 0, a registered suite that passes
    /// exits 1, and all green exits 0.
    #[test]
    fn one_unowned_red_or_one_stale_entry_fails_a_whole_sweep() {
        let exit = |planted: Option<(&str, usize)>| -> i32 {
            let mut results: Vec<(String, usize)> = (0..20).map(|i| (format!("g{i}"), 0)).collect();
            if let Some((s, f)) = planted {
                results.push((s.to_owned(), f));
            }
            i32::from(
                results
                    .iter()
                    .any(|(s, f)| score(SYNTH, s, *f).fails_the_sweep()),
            )
        };
        assert_eq!(exit(Some(("unowned_suite", 2))), 1);
        assert_eq!(exit(Some(("owned_suite", 2))), 0);
        assert_eq!(exit(Some(("owned_suite", 0))), 1);
        assert_eq!(exit(None), 0);
    }

    /// Every entry carries an owner and a cause a reader can act on, and no suite is listed twice.
    #[test]
    fn every_known_red_entry_is_unique_and_carries_an_owner_and_a_cause() {
        for (suite, owner, why) in KNOWN_RED {
            assert!(!owner.is_empty(), "{suite}: no owner");
            assert!(why.len() > 20, "{suite}: the cause is too thin to act on");
        }
        let mut names: Vec<&str> = KNOWN_RED.iter().map(|(s, _, _)| *s).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(names.len(), before, "a suite is listed twice");
        let mut designed: Vec<&str> = KNOWN_NO_TESTS.iter().map(|(s, _)| *s).collect();
        designed.sort_unstable();
        designed.dedup();
        assert_eq!(designed.len(), KNOWN_NO_TESTS.len());
    }
}
