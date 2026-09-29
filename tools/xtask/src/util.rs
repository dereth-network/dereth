//! Shared plumbing for the workspace tasks: finding the repo, running a command, and the
//! pass/fail/skip result every gate reports.

use std::path::{Path, PathBuf};
use std::process::Command;

use dereth_dat::RetailDat;

/// The outcome of one gate.
///
/// **There is deliberately no bare `Skip` variant, and its absence is the whole repair.** A gate
/// that did not run must say *why*, because the reason decides whether the run certified
/// anything, and one undifferentiated `Skip` cannot express that.
///
/// With a `Skip` returned when the retail dats are absent and an exit code of
/// `i32::from(fail > 0)`, a run that cannot find the dats prints *0 passed, 0 failed,
/// 1 skipped* and exits **0** having never run the dat gate's oracle harness, while its unit tests
/// (which may find the dats some other way) report as passed. That is a silently skipped oracle
/// in the instrument that certifies a *gate* rather than a test.
///
/// Having no variant, rather than a variant with a different meaning, is the point. `Outcome::Skip`
/// is `E0599`, so the silent shape is **not expressible** rather than discouraged -- the same bar as
/// test bodies, where `let Some(_s) = store() else { return };` is `E0308`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Pass,
    Fail,
    /// The gate did not run. [`NotRun`] says why, and the why decides the exit code.
    NotRun(NotRun),
}

/// Why a gate did not run. Choosing a variant is a claim about the absence, not a formality.
///
/// **Does the absence remove an input, or remove the machine's ability to ask the question?** A GPU gate
/// on a host with no D3D12 adapter is a different animal from a dat gate on a host that has the
/// dats sitting one directory away. A blanket "no more skipping" would be as wrong as the state
/// this replaced -- it would make every hosted CI runner red for a reason nobody can fix, and a
/// gate that is red from birth is a gate nobody reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotRun {
    /// **The machine cannot host the question.** No D3D12 adapter, no ACE server, no capture rig.
    /// The absence removes the ability to *ask*; there is no oracle sitting there unconsulted.
    /// Exits 0.
    ///
    /// A machine without a suitable graphics adapter cannot run a draw test. State at the site
    /// why the missing capability is environmental rather than a missing oracle. This variant is
    /// the only one of the three that may be chosen because of the **host**.
    Unhostable,
    /// **The oracle's input is absent.** The retail dats, a capture corpus, a recorded session.
    /// The question exists, the machine could ask it, and nobody did. Exits **non-zero**, because
    /// a run in this state has certified nothing about the gate it names.
    OracleAbsent,
    /// **The work does not exist yet.** An unstarted gate, unwritten comparison code, a
    /// deliberately deferred piece of work. Exits 0 and is printed on its own line, because a gate red from birth is a
    /// gate nobody reads -- but it is never counted as a pass, so the summary cannot claim
    /// progress that does not exist.
    NotBuilt,
}

impl Outcome {
    pub fn label(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
            Self::NotRun(NotRun::Unhostable) => "NO-HOST",
            Self::NotRun(NotRun::OracleAbsent) => "NO-ORACLE",
            Self::NotRun(NotRun::NotBuilt) => "NOT-BUILT",
        }
    }

    /// Whether this outcome must make the whole run exit non-zero.
    ///
    /// `Fail` obviously. `OracleAbsent` because an unasked question is not a passed one: that is
    /// the half `i32::from(fail > 0)` could not see. The other two exit 0 **and are still not
    /// passes** -- the difference between "this run says nothing about X" and "this run says X is
    /// fine" is carried by the printed table, not by the exit code alone.
    pub fn is_failure(self) -> bool {
        matches!(self, Self::Fail | Self::NotRun(NotRun::OracleAbsent))
    }
}

#[derive(Debug, Clone)]
pub struct Report {
    pub name: String,
    pub outcome: Outcome,
    pub detail: String,
}

impl Report {
    pub fn new(name: impl Into<String>, outcome: Outcome, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            outcome,
            detail: detail.into(),
        }
    }
}

/// Print one table and return the process exit code.
///
/// The exit code is derived from [`Outcome::is_failure`] per row, **not** from a count of `Fail`.
/// `i32::from(fail > 0)` would make every non-pass invisible to CI, so a retail-dat skip could
/// never be noticed from an exit status. Adding a new kind of non-answer does not silently widen
/// the hole, because the exit code asks each row a question rather than counting one variant.
pub fn print_table(title: &str, reports: &[Report]) -> i32 {
    let width = reports
        .iter()
        .map(|r| r.name.len())
        .max()
        .unwrap_or(4)
        .max(4);
    println!("\n{title}");
    println!("{}", "-".repeat(title.len()));
    for r in reports {
        println!("  {:<width$}  {}  {}", r.name, r.outcome.label(), r.detail);
    }
    let tally = |o: Outcome| reports.iter().filter(|r| r.outcome == o).count();
    let pass = tally(Outcome::Pass);
    let fail = tally(Outcome::Fail);
    let no_oracle = tally(Outcome::NotRun(NotRun::OracleAbsent));
    let no_host = tally(Outcome::NotRun(NotRun::Unhostable));
    let not_built = tally(Outcome::NotRun(NotRun::NotBuilt));
    println!(
        "\n  {pass} passed, {fail} failed, {no_oracle} NOT RUN (oracle absent), \
         {no_host} not run (machine cannot host), {not_built} not run (not built yet)"
    );
    if no_oracle > 0 {
        println!(
            "  {no_oracle} gate(s) had an oracle to consult and did not consult it. This run \
             certifies NOTHING about them, so it exits non-zero. See the detail column."
        );
    }
    if pass == 0 {
        println!("  NOTHING IN THIS TABLE PASSED. This run certifies nothing.");
    }
    i32::from(reports.iter().any(|r| r.outcome.is_failure()))
}

/// The Cargo workspace root, two levels above this crate. Every input and every script the tasks
/// run is inside it.
pub fn workspace_root() -> PathBuf {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    dir.pop(); // tools/xtask -> tools
    dir.pop(); // tools -> the workspace root
    dir
}

/// Cargo's target directory for the workspace, found the way cargo finds it: `cargo metadata`
/// honours `CARGO_TARGET_DIR` and the `build.target-dir` setting. `target/` in the workspace when
/// cargo cannot say.
pub fn target_dir() -> PathBuf {
    let ws = workspace_root();
    Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(&ws)
        .output()
        .ok()
        .filter(|out| out.status.success())
        .and_then(|out| serde_json::from_slice::<serde_json::Value>(&out.stdout).ok())
        .and_then(|meta| meta["target_directory"].as_str().map(PathBuf::from))
        .unwrap_or_else(|| ws.join("target"))
}

/// The workspace's crate directories, each `<group>/<short>`: `core/` holds the shared crates,
/// `dereth/` the products (the client, whose own crates are under `dereth/client/crates/`, the
/// launcher, whose logic crate is under `dereth/launcher/crates/`, the headless client and the
/// test kit), `tools/` the research and build tools, and `empyrean/` the
/// server (its ported crates under `empyrean/crates/`). A crate's directory is its package name
/// without the `dereth-` or `empyrean-` prefix. Nested groups come first, so the longest group
/// that prefixes a path is the one it is read against.
pub const CRATE_GROUPS: &[&str] = &[
    "dereth/client/crates",
    "dereth/launcher/crates",
    "empyrean/crates",
    "core",
    "dereth",
    "tools",
    "empyrean",
];

/// The crates whose directory is not their name without `dereth-`, as `(directory, package)`.
pub(crate) const NAMED_OTHERWISE: &[(&str, &str)] =
    &[("dereth/client/crates/shell", "dereth-client-shell")];

/// The directory of the workspace crate `krate` (a package name). An `empyrean-*` crate is looked
/// for under `empyrean/crates/` and `empyrean/`, any other under `core/`, `dereth/`,
/// `dereth/client/crates/`, `dereth/launcher/crates/` and `tools/`; a name found in none gives its would-be `core/` path,
/// which the caller's read then reports as missing.
pub fn crate_dir(ws: &Path, krate: &str) -> PathBuf {
    if let Some((dir, _)) = NAMED_OTHERWISE.iter().find(|(_, name)| *name == krate) {
        return ws.join(dir);
    }
    let (short, groups): (&str, &[&str]) = match krate.strip_prefix("empyrean-") {
        Some(short) => (short, &["empyrean/crates", "empyrean"]),
        None => (
            krate.strip_prefix("dereth-").unwrap_or(krate),
            &[
                "core",
                "dereth",
                "dereth/client/crates",
                "dereth/launcher/crates",
                "tools",
            ],
        ),
    };
    groups
        .iter()
        .map(|g| ws.join(g).join(short))
        .find(|d| d.join("Cargo.toml").is_file())
        .unwrap_or_else(|| ws.join("core").join(short))
}

/// The package name of the crate in directory `short` of `group`: the inverse of [`crate_dir`].
pub fn crate_name(group: &str, short: &str) -> String {
    let dir = format!("{group}/{short}");
    if let Some((_, name)) = NAMED_OTHERWISE.iter().find(|(d, _)| *d == dir) {
        return (*name).to_owned();
    }
    match (group, short) {
        ("empyrean" | "empyrean/crates", s) => format!("empyrean-{s}"),
        (_, "xtask") => "xtask".to_owned(),
        (_, s) => format!("dereth-{s}"),
    }
}

/// The cargo profile the test and CI tiers build and run in.
///
/// `test-release` (`Cargo.toml`) by default: optimised, so the tiers run at release speed, with
/// debug assertions and overflow checks kept on, so a `debug_assert!`, a `#[cfg(debug_assertions)]`
/// test and an arithmetic overflow still fail a run. `--debug` selects cargo's dev profile, for
/// investigating one failure; it is not what a gate a merge relies on runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    TestRelease,
    Dev,
}

impl Profile {
    /// `--debug` anywhere on the command line selects [`Profile::Dev`].
    pub fn from_args(args: &[String]) -> Self {
        if args.iter().any(|a| a == "--debug") {
            Profile::Dev
        } else {
            Profile::TestRelease
        }
    }

    /// The cargo arguments that select this profile. The dev profile is cargo's default: none.
    pub fn cargo_args(self) -> &'static [&'static str] {
        match self {
            Profile::TestRelease => &["--profile", "test-release"],
            Profile::Dev => &[],
        }
    }
}

/// Run a command in a directory, streaming its output. Returns whether it succeeded.
pub fn run(dir: &Path, program: &str, args: &[&str]) -> bool {
    println!("$ {program} {}", args.join(" "));
    Command::new(program)
        .args(args)
        .current_dir(dir)
        .status()
        .map(|s| s.success())
        .unwrap_or_else(|e| {
            eprintln!("failed to launch {program}: {e}");
            false
        })
}

/// Run a command capturing its output. Returns (success, combined output).
pub fn run_captured(dir: &Path, program: &str, args: &[&str]) -> (bool, String) {
    match Command::new(program).args(args).current_dir(dir).output() {
        Ok(out) => {
            let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
            text.push_str(&String::from_utf8_lossy(&out.stderr));
            (out.status.success(), text)
        }
        Err(e) => (false, format!("failed to launch {program}: {e}")),
    }
}

/// The four files [`retail_data_available`] requires. Named here so a failure message can list
/// exactly which of them was missing rather than saying "the dats".
pub const RETAIL_DATS: [RetailDat; 4] = RetailDat::ALL;

/// The directory [`retail_data_available`] looks in: the tests' own lookup,
/// [`dereth_dat::testing::dat_dir`] (`DERETH_TEST_DAT_DIR`), so the
/// gates and the tests they run cannot disagree about where the dats are. When none is found it
/// is the first directory that lookup tried, which the failure line names.
pub fn retail_dir_searched() -> PathBuf {
    dereth_dat::testing::dat_dir()
}

/// True when the four retail dats are present in [`retail_dir_searched`].
pub fn retail_data_available() -> Option<PathBuf> {
    let dir = retail_dir_searched();
    let all = RETAIL_DATS.iter().all(|d| d.in_dir(&dir).is_file());
    all.then_some(dir)
}

/// The dats missing from `dir`, in [`RETAIL_DATS`] order. Empty when all four are there.
///
/// A failure that says *"the retail dats are missing"* cannot be acted on; one that names
/// `client_highres.dat` and the directory it looked in can. `dereth-dat` requires only three of the
/// four (`REQUIRED_DATS`) while this requires all four, so an install missing only the high-res
/// dat opens happily in the store and answers *no retail data* here -- precisely the partial state
/// that could otherwise produce a silent half-skip.
pub fn retail_shortfall(dir: &Path) -> Vec<&'static str> {
    RETAIL_DATS
        .iter()
        .filter(|d| !d.in_dir(dir).is_file())
        .map(|d| d.file_name())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(outcome: Outcome) -> Report {
        Report::new("probe", outcome, String::new())
    }

    /// A gate that did not consult an available oracle makes the run exit non zero.
    #[test]
    fn a_gate_that_did_not_consult_an_available_oracle_makes_the_run_exit_non_zero() {
        let table = [
            report(Outcome::Pass),
            report(Outcome::NotRun(NotRun::OracleAbsent)),
        ];
        assert_eq!(
            print_table("calibration: one unconsulted oracle", &table),
            1,
            "a run that skipped an oracle it could have consulted must not exit 0"
        );
    }

    /// **Negative control**, and it is the half that decides whether this repair is right rather
    /// than merely loud. A machine with no D3D12 adapter cannot ask a draw-test question at all,
    /// and an unlanded unit has no question to ask yet. Neither is an unconsulted oracle, so
    /// neither may redden CI -- a blanket "no more skipping" would make every hosted runner red
    /// for a reason nobody can fix.
    #[test]
    fn an_unhostable_question_and_unbuilt_work_stay_quiet() {
        let table = [
            report(Outcome::Pass),
            report(Outcome::NotRun(NotRun::Unhostable)),
            report(Outcome::NotRun(NotRun::NotBuilt)),
        ];
        assert_eq!(
            print_table(
                "negative control: nothing was skipped that could have run",
                &table
            ),
            0,
            "an environmental absence and unbuilt work are not unconsulted oracles"
        );
    }

    /// The two ends, so the instrument is shown to discriminate rather than merely to fire.
    #[test]
    fn the_two_ends_still_read_the_way_they_always_did() {
        assert_eq!(print_table("all pass", &[report(Outcome::Pass)]), 0);
        assert_eq!(
            print_table("one fail", &[report(Outcome::Pass), report(Outcome::Fail)]),
            1
        );
    }

    /// The exit code must be a property of the ROWS, not of a count of one variant. This is the
    /// half `i32::from(fail > 0)` got wrong, and the test is written so that reverting to any
    /// `fail`-counting form reddens it.
    #[test]
    fn the_exit_code_asks_every_row_rather_than_counting_fails() {
        for (outcome, expected) in [
            (Outcome::Pass, false),
            (Outcome::Fail, true),
            (Outcome::NotRun(NotRun::OracleAbsent), true),
            (Outcome::NotRun(NotRun::Unhostable), false),
            (Outcome::NotRun(NotRun::NotBuilt), false),
        ] {
            assert_eq!(
                outcome.is_failure(),
                expected,
                "{outcome:?} classified wrongly"
            );
            assert_eq!(
                print_table("single row", &[report(outcome)]),
                i32::from(expected)
            );
        }
    }

    /// Every outcome prints a distinct word. A table where two different non-answers share a
    /// label is the state this unit replaced, one level down in the presentation.
    #[test]
    fn every_outcome_has_its_own_label_and_none_of_them_is_the_old_word() {
        let all = [
            Outcome::Pass,
            Outcome::Fail,
            Outcome::NotRun(NotRun::Unhostable),
            Outcome::NotRun(NotRun::OracleAbsent),
            Outcome::NotRun(NotRun::NotBuilt),
        ];
        let mut labels: Vec<&str> = all.iter().map(|o| o.label()).collect();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(
            labels.len(),
            all.len(),
            "two outcomes share a label: {labels:?}"
        );
        assert!(
            !labels.contains(&"SKIP"),
            "`SKIP` is the word that meant five different things; it does not come back"
        );
    }

    /// `retail_shortfall` names what is missing. Calibration on a directory that certainly holds
    /// no dats; negative control on one that holds all four, built here rather than borrowed from
    /// the environment so the test says the same thing on a machine with no retail install.
    #[test]
    fn the_shortfall_names_the_missing_files_and_is_empty_when_all_four_are_there() {
        let base = std::env::temp_dir().join("dere-o840-shortfall");
        let empty = base.join("empty");
        let full = base.join("full");
        std::fs::create_dir_all(&empty).expect("a temp directory");
        std::fs::create_dir_all(&full).expect("a temp directory");
        assert_eq!(
            retail_shortfall(&empty),
            RETAIL_DATS.map(RetailDat::file_name).to_vec(),
            "a directory with no dats must name all four"
        );
        for d in RETAIL_DATS {
            std::fs::write(d.in_dir(&full), b"not a dat, but it is a file").expect("write probe");
        }
        assert!(
            retail_shortfall(&full).is_empty(),
            "four files present must produce no shortfall"
        );
        // And the discriminating middle case: the partial install that used to half-skip.
        std::fs::remove_file(RetailDat::HighRes.in_dir(&full)).expect("remove one");
        assert_eq!(retail_shortfall(&full), vec!["client_highres.dat"]);
        std::fs::remove_dir_all(&base).ok();
    }
}
