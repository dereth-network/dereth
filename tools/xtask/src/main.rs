//! The workspace task runner: the gate tiers, the clippy policy, the seams and the checks clippy
//! cannot express.
//!
//! **Depends on** `dereth-dat` alone among the workspace's crates, for the tests' lookup of the
//! retail dats (`dereth_dat::testing`); otherwise it runs `cargo` and reads the workspace's files.
//! **Used by**
//! nothing: it is the `cargo xtask` binary (the alias is in `.cargo/config.toml`).
//!
//! **Must never** pass a check it did not run: a skipped row is reported as not run, never as a
//! pass, and every checker is calibrated by tests against planted violations.
//!
//! One command per moment, client and server alike:
//!
//! | when | command | needs |
//! |---|---|---|
//! | while working, before each commit | `cargo xtask check` | nothing |
//! | before pushing or opening a PR | `cargo xtask ci tier0` | nothing: exactly what public CI runs |
//! | with the retail data | `cargo xtask ci tier1` | the dats and `world.pack` |
//! | milestones, on hardware | `cargo xtask ci tier2` | a graphics device and the dats |
//!
//! The expensive comparisons wait until the cheap ones hold: golden images against a renderer built
//! on unproven float precision produce an unbounded stream of false differences. The single checks
//! are listed, grouped, by `cargo xtask` with no arguments.
//!
//! Every command that builds or runs tests does so in the `test-release` profile: optimised, with
//! debug assertions and overflow checks on (see [`util::Profile`]). `--debug` on any of them
//! selects cargo's dev profile instead, for investigating a failure.

mod bundle;
mod check;
mod comment_lint;
mod crate_docs;
mod gates;
mod line_endings;
mod lint;
mod output_hygiene;
mod package;
mod release;
mod seams;
mod separation;
mod server;
mod skip_claims;
mod sweep;
mod test_lint;
mod testsrc;
mod util;

use util::{
    print_table, retail_data_available, run, workspace_root, NotRun, Outcome, Profile, Report,
};

const USAGE: &str = "\
usage: cargo xtask <task> [--debug]

When to run what (client and server alike):
  check                     while working, before each commit: clippy and the tests of the crates
                            changed since the base branch (their dependents linted too), and the
                            fast lints. Needs nothing. --base <ref> (default: research,
                            origin/HEAD, origin/main or main), -p <crate> to name the crates
  ci tier0                  before pushing or opening a PR: exactly what public CI runs. The build,
                            every no-data test, clippy, every rule below and the docs. Needs nothing
  ci tier1                  with the retail data: every crate's data tiers and the server's
                            real-content tier. Needs the dats (DERETH_TEST_DAT_DIR), world.pack
                            (EMPYREAN_TEST_WORLD_PACK) and ACE's world dump (EMPYREAN_WORLD_SQL, or
                            `empyrean-import fetch` once). --serial: the unsharded client dat tier
  ci tier2 [--milestone]    milestones, on hardware: the gpu tier and the retail comparisons. Needs a
                            graphics device and the dats. --milestone exits non-zero unless an
                            oracle row passed; it is the command a milestone signs off on. The
                            client's gpu binary runs sharded (DERETH_TEST_SHARDS=N); --serial: one process

Rules (each a row of `check` or `ci tier0`):
  fmt-check                 cargo fmt --all --check
  lint                      the bans clippy cannot express (`as i32`, unannotated HashMap) and the seams
  seams                     the seams alone: contract, ui-screens, client-runtime, client crates
  crate-docs                every crate's //! header and its Cargo description
  comment-lint              comments and messages describe behaviour: no work-item labels,
                            private-document pointers or study-method words
  test-lint [--list [p]]    tests named for what they prove and anchored to the claim they prove
  server-hygiene            the server's source rules (no todo!/unimplemented!, the port header)
  doc                       a check, not a docs build: rustdoc links and doc comments stay correct
                            (every rustdoc warning an error except links to private items)
  separation [--client-only] [ROOT]
                            the client/server dependency and licence rules
  output-hygiene [ROOT]     no test or tool writes into a tracked path or outside the workspace
  line-endings --tree [ROOT] | --normalise FILE... | FILE...
                            every tracked file has the ending .gitattributes gives it (ROOT: any
                            git checkout, default the workspace)
  skip-claims --tree [ROOT] no test skips when the retail dats are absent, and no comment claims
                            one does

Tests:
  build | test | clippy     cargo build / test / clippy -D warnings over the whole workspace
  gate <name>               one area's whole suite, data and device tiers included (--serial);
                            names: {GATES}
  gates                     every area's gate, one table
  sweep [SUITE...] [--shard-binary PKG:TIER] [--serial] [--shards N] ...
                            run test targets one by one, or one tier binary sharded across
                            processes, with a verdict that cannot read as silence (--help)

Release and packaging:
  package empyrean [--target <triple>]... [--out <dir>] [--allow-dirty] | --gather <dir>
                            Empyrean's release archives: builds both binaries per target, stages
                            the allowlist, runs the deny scan and the header checks, and writes
                            the archives, MANIFEST.txt, release.json and SHA256SUMS
                            (empyrean/RELEASING.md)
  package dereth [--target <triple>]... [--out <dir>] [--allow-dirty] [--moltenvk <dir>]
                 | --gather <dir>
                            Dereth's release files: builds the client and the launcher (Tauri) for
                            this host, and writes the launcher (Windows zip, macOS Dereth.app,
                            Linux AppImage), with the client inside it, checked and scanned, and
                            signed for the launcher's updater (latest.json) when
                            TAURI_SIGNING_PRIVATE_KEY is set (dereth/launcher/RELEASING.md)
  release empyrean|dereth <version> [--push] [--remote <name>]
                            cut a release on a clean main: set the version and commit it, run
                            tier 0 and a host package, tag empyrean-v<version> or
                            dereth-v<version>; pushes only with --push. A final release also
                            turns the Unreleased section of the product's CHANGES.md
                            (empyrean/ or dereth/) into the release's own, dated
  release-notes dereth|empyrean [--since <tag>] [--version <v>] [--out <file>]
                            print a release's notes without releasing: the highlights of the
                            product's CHANGES.md, then its commits since the previous final
                            dereth-v or empyrean-v tag, grouped (the release workflow writes
                            them the same way)
  version empyrean|dereth [--tag <tag>]
                            print the product's version; with --tag, fail unless the tag names it
  bundle-macos              the client as a macOS `Dereth.app` (--dat-dir <dir> links the retail
                            dats in; --debug bundles the dev profile)
  web [args]                the web client's developer runner (`dereth-web-dev`): builds the browser
                            client and serves it on 127.0.0.1; `cargo xtask web --help` lists its
                            options (--dat-dir, --server, --dev, --port, --no-build, --build-only)

Ledgers:
  test-inventory [--out F]  every #[test] fn in source, with its libtest name, assertion counts and
                            body hashes, to compare a tree before and after tests move
  deviations check          the deviation ledger (not built yet)
";

/// The usage text, with the gate names wrapped under their column.
fn usage() -> String {
    let indent = " ".repeat(28);
    let mut lines = vec![String::from("names:")];
    for name in gates::names().split(' ') {
        let last = lines.last_mut().expect("one line at least");
        if last.len() + 1 + name.len() > 72 {
            lines.push(name.to_owned());
        } else {
            last.push(' ');
            last.push_str(name);
        }
    }
    USAGE.replace("names: {GATES}", &lines.join(&format!("\n{indent}")))
}

/// `cargo xtask web`: runs the web client's developer runner, a binary of its own so that its
/// bindgen library is not built into this one, which every tier builds.
fn web(args: &[String]) -> i32 {
    let mut argv = vec!["run", "--release", "-q", "-p", "dereth-web-dev", "--"];
    argv.extend(args.iter().map(String::as_str));
    i32::from(!run(&workspace_root(), "cargo", &argv))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let profile = Profile::from_args(&args);
    let code = match args.first().map(String::as_str) {
        Some("check") => check::check(&args[1..]),
        Some("build") => i32::from(!run(&workspace_root(), "cargo", &["build", "--workspace"])),
        Some("test") => {
            let mut argv = vec!["test", "--workspace"];
            argv.extend_from_slice(profile.cargo_args());
            i32::from(!run(&workspace_root(), "cargo", &argv))
        }
        Some("clippy") => i32::from(!run(&workspace_root(), "cargo", CLIPPY_ARGS)),
        Some("lint") => lint::lint(),
        Some("seams") => seams::seams(),
        Some("crate-docs") => crate_docs::crate_docs(),
        Some("comment-lint") => comment_lint::comment_lint(),
        Some("server-hygiene") => server::server_hygiene(),
        Some("test-inventory") => testsrc::test_inventory(&args[1..]),
        Some("test-lint") => test_lint::test_lint(&args[1..]),
        Some("fmt-check") => i32::from(!run(&workspace_root(), "cargo", FMT_CHECK)),
        Some("doc") => i32::from(!doc_gate(&workspace_root())),
        Some("separation") => separation::separation(&args[1..]),
        Some("output-hygiene") => output_hygiene::output_hygiene(&args[1..]),
        Some("line-endings") => line_endings::line_endings(&args[1..]),
        Some("skip-claims") => skip_claims::skip_claims(&args[1..]),
        Some("sweep") => sweep::sweep(&args[1..]),
        Some("gate") => match gate_name(&args) {
            Some(g) => gates::gate(g, threads(&args), profile),
            None => {
                eprintln!(
                    "gate: name one, e.g. `cargo xtask gate physics`; known: {}",
                    gates::names()
                );
                2
            }
        },
        Some("gates") => print_table(
            "Gates -- every area's whole suite",
            &gates::gates(threads(&args), profile, gates::GpuTier::Include),
        ),
        Some("bundle-macos") => bundle::bundle_macos(&args[1..]),
        Some("package") => package::package(&args[1..]),
        Some("release") => release::release(&args[1..]),
        Some("release-notes") => release::notes::release_notes(&args[1..]),
        Some("version") => package::version_command(&args[1..]),
        Some("web") => web(&args[1..]),
        Some("deviations") => deviations(&args[1..]),
        Some("ci") => ci(&args[1..]),
        None | Some("help" | "--help" | "-h") => {
            print!("{}", usage());
            0
        }
        Some(other) => {
            eprint!("unknown task {other:?}\n\n{}", usage());
            2
        }
    };
    std::process::exit(code);
}

/// `--serial` on any command that runs a gate.
///
/// The default is the sharded form: `dereth-client`'s `dat` and `gpu` binaries run as N single-threaded
/// processes over disjoint module slices rather than as one. `--serial` is the unsharded form, the
/// one a sharded run must be equivalent to -- so it stays reachable as the reference.
fn threads(args: &[String]) -> gates::Threads {
    if args.iter().any(|a| a == "--serial") {
        gates::Threads::Serial
    } else {
        gates::Threads::Sharded
    }
}

/// The gate `cargo xtask gate <name>` names, skipping any flag.
///
/// A free function because the alternative -- `args[1]` -- made `cargo xtask gate --serial client`
/// look up a gate called `--serial` and print "unknown gate", which is a flag silently eating
/// the argument after it. The two orders have to mean the same thing.
fn gate_name(args: &[String]) -> Option<&String> {
    args[1..].iter().find(|a| !a.starts_with("--"))
}

/// The deviation ledger. `deviations.toml` does not exist yet, so this reports NOT-BUILT rather
/// than a pass. An unexplained difference and an
/// explained one must not look the same in the output, and neither must an absent ledger and an
/// empty one.
fn deviations(args: &[String]) -> i32 {
    if args.first().map(String::as_str) != Some("check") {
        eprintln!("usage: cargo xtask deviations check");
        return 2;
    }
    let ledger = workspace_root().join("deviations.toml");
    if ledger.is_file() {
        eprintln!(
            "{} exists but `deviations check` is not implemented yet",
            ledger.display()
        );
        return 2;
    }
    // NOT-BUILT, not an unconsulted oracle: there is no ledger to read because nobody has written
    // one, so there is no question this host declined to ask. It exits 0 and prints "NOTHING IN
    // THIS TABLE PASSED", which is the honest reading of `xtask deviations check` today.
    //
    // The exit code is the one the table computes. A hard-coded `0` returned after `print_table`
    // would be an instrument whose verdict is thrown away by its caller, and it would go on
    // discarding a real failure the day the ledger exists and this function has something to fail
    // about.
    print_table(
        "xtask deviations check",
        &[Report::new(
            "deviation ledger",
            Outcome::NotRun(NotRun::NotBuilt),
            "deviations.toml does not exist; the ledger is not built yet",
        )],
    )
}

/// Is a tier-2 run entitled to let anybody sign off a milestone?
///
/// **The decision, and the argument for it.**
///
/// Tier 2 exists to answer one question: *has this build been compared against retail?* It runs
/// nightly and before every milestone, and it is the only tier that ever consults an oracle. Today
/// it holds one informational step and two `NotBuilt` rows, so it exits 0 having compared nothing
/// against anything.
///
/// There are two obvious repairs and both are wrong on their own:
///
/// * **Make the whole run exit non-zero until an oracle passes.** That is red from birth, and a
///   gate that is always red is a gate nobody reads (the sweep's `KNOWN_RED` exists for
///   exactly that kind of gate). Worse,
///   and this is the half the "red from birth" objection usually misses: an exit code that is 1
///   today, 1 tomorrow and 1 on the day a real golden-image comparison *fails* is not an exit
///   code that distinguishes a new failure. It has stopped measuring that change.
/// * **Leave it green and stop citing it in the milestone checklist.** There is no milestone
///   checklist. A repository-wide search for `tier2` and `tier 2` returns this
///   file's own module doc, the validation-tier table, and
///   two unrelated prose mentions of cost. **There is nothing to stop citing**, so that horn is
///   vacuous as stated, and taking it would leave the exit code saying exactly what it says now.
///
/// **The rule, and it is the third option: tier 2 asks two questions and had one number for
/// them.** *Did anything I ran fail?* and *has an oracle certified this build?* are different
/// questions with different right answers today -- no and no respectively -- and one exit code
/// cannot carry both. So they get two commands:
///
/// * `cargo xtask ci tier2` keeps the per-row exit code, so it still measures the first question
///   and will still go red the day a comparison genuinely fails. It **always** prints the
///   milestone verdict in words, so the number is never the only thing on screen.
/// * `cargo xtask ci tier2 --milestone` asks the second question and **cannot answer yes while
///   every oracle row is `NotBuilt`**. It exits non-zero and names the missing oracle rows. That is the command a milestone sign-off runs, and it is red today.
///
/// This makes tier 2 incapable of reporting a milestone-ready state while every oracle row is
/// `NotBuilt`, without spending the exit code of the run people actually type.
///
/// **The objection to state rather than hide: `--milestone` could be the gate nobody reads,
/// wearing a flag.** Two things bound that. It is red for exactly one nameable reason (the
/// capture rig does not exist) rather than for a diffuse tree-wide backlog, so it is a to-do with
/// an exit code and it clears the day the rig does. And the default run prints its verdict and the
/// command unconditionally, so the flag cannot be forgotten by anyone who ran tier 2 at all.
///
/// `Err` carries the whole message; `Ok(n)` is the number of oracle rows that passed.
fn milestone_verdict(oracle_rows: &[Report]) -> Result<usize, String> {
    // A tier 2 that declared no oracle rows would pass vacuously: an empty space would
    // return a confident yes. Deleting the two `NotBuilt` rows must not be
    // a way to certify a milestone.
    if oracle_rows.is_empty() {
        return Err(
            "MILESTONE NOT CERTIFIED: tier 2 declared NO oracle rows at all. An empty oracle \
             tier certifies nothing, and it must not be possible to make this check pass by \
             deleting the rows that fail it."
                .to_owned(),
        );
    }
    let passed = oracle_rows
        .iter()
        .filter(|r| r.outcome == Outcome::Pass)
        .count();
    if passed > 0 {
        return Ok(passed);
    }
    let owing: Vec<String> = oracle_rows
        .iter()
        .map(|r| format!("{} [{}] {}", r.name, r.outcome.label(), r.detail))
        .collect();
    Err(format!(
        "MILESTONE NOT CERTIFIED: 0 of {} oracle row(s) passed, so nothing in this build has been \
         compared against retail. Owing:\n    {}",
        oracle_rows.len(),
        owing.join("\n    ")
    ))
}

/// The exit code of a tier-2 run, from the two questions it asks.
///
/// `ran` is [`print_table`]'s verdict on what actually executed. `certified` is whether an oracle
/// row passed. `milestone` is whether the caller asked the milestone question at all.
///
/// Extracted from [`ci`] because `ci` cannot be called from a test without building the workspace,
/// and an exit code nobody can test is exactly the failure this function exists to prevent. Note what it does
/// *not* do: `ran` is never suppressed. A tier-2 run whose informational corpus check genuinely failed
/// exits non-zero in both modes, so the milestone flag can only ever make a run redder, never
/// greener.
fn tier2_exit(ran: i32, certified: bool, milestone: bool) -> i32 {
    if milestone && !certified {
        ran.max(1)
    } else {
        ran
    }
}

/// `dereth-corpus corpus --check`: the committed message corpus and packet-capture index are exactly
/// what the recordings generate. The same comparison is the `dereth-corpus` consistency test, so
/// tier 0's `cargo test` runs it; tier 2 names it as an informational row.
fn corpus_check_step(ws: &std::path::Path, label: &str, profile: Profile) -> Report {
    let mut argv = vec!["run", "-q"];
    argv.extend_from_slice(profile.cargo_args());
    argv.extend_from_slice(&["-p", "dereth-corpus", "--", "corpus", "--check"]);
    step(label, run(ws, "cargo", &argv))
}

/// The rules that take seconds and read the whole repository: `check` and `ci tier0` both run
/// them, as one row each.
fn cheap_lints() -> Vec<Report> {
    let ws = workspace_root();
    let hygiene = server::hygiene_findings(&ws);
    for b in hygiene.iter().take(50) {
        println!("  {b}");
    }
    vec![
        step("cargo fmt --all --check", run(&ws, "cargo", FMT_CHECK)),
        step("xtask lint (bans and seams)", lint::lint() == 0),
        step("xtask crate-docs", crate_docs::crate_docs() == 0),
        step("xtask comment-lint", comment_lint::comment_lint() == 0),
        step("xtask test-lint", test_lint::test_lint(&[]) == 0),
        Report::new(
            "xtask server-hygiene",
            if hygiene.is_empty() {
                Outcome::Pass
            } else {
                Outcome::Fail
            },
            format!("{} finding(s)", hygiene.len()),
        ),
    ]
}

/// Tier 0: everything that needs no data, client and server alike, each thing once.
///
/// The client's and the server's tests are two rows (the server's run is timed against its
/// host budget); the build, clippy and the docs cover the whole workspace in one invocation each.
/// The divergence lists and the message corpus are checked by tests in the `cargo test` rows, so
/// they are not run again as rows of their own.
fn tier0(profile: Profile) -> i32 {
    let ws = workspace_root();
    let mut build = vec!["build", "--workspace"];
    build.extend_from_slice(profile.cargo_args());
    let mut test = vec!["test", "--workspace", "--exclude", "empyrean-*"];
    test.extend_from_slice(profile.cargo_args());
    let mut reports = vec![
        step("cargo build --workspace", run(&ws, "cargo", &build)),
        step(
            "cargo test (client and shared crates)",
            run(&ws, "cargo", &test),
        ),
    ];
    reports.extend(server::unit_tier(profile));
    reports.push(clippy_step(&ws));
    reports.push(step("xtask doc (rustdoc warnings denied)", doc_gate(&ws)));
    reports.extend(cheap_lints());
    reports.push(source_rule("separation", || {
        separation::report(&separation::check(&ws, false))
    }));
    reports.push(source_rule("output-hygiene", || output_hygiene::run(&ws)));
    reports.push(Report::new(
        "xtask deviations check",
        Outcome::NotRun(NotRun::NotBuilt),
        "the deviation ledger is not built yet",
    ));
    print_table("CI tier 0 -- no data (before pushing; public CI)", &reports)
}

/// Tier 1: every area's data tier (the gates without their `gpu` binaries) and the server's
/// real-content tier.
fn tier1(args: &[String], profile: Profile) -> i32 {
    let dats = retail_data_available();
    match &dats {
        Some(dir) => println!("retail dats found at {}", dir.display()),
        None => {
            // Name the gates whose harnesses read them: derived from the catalogue, so the
            // message cannot go stale when it changes.
            let owed: Vec<&str> = gates::GATES
                .iter()
                .filter(|g| g.needs_retail())
                .map(|g| g.name)
                .collect();
            println!(
                "retail dats NOT found; the dat tiers will fail, {} gate harness(es) report \
                 NO-ORACLE ({}) and so do the server's real-content rows, so this run exits \
                 non-zero. Set DERETH_TEST_DAT_DIR to the directory holding them.",
                owed.len(),
                owed.join(", ")
            );
        }
    }
    match server::world_pack() {
        Some(p) => println!("world.pack found at {}", p.display()),
        None => println!(
            "world.pack NOT found; the server's real-content rows report NO-ORACLE. Set \
             EMPYREAN_TEST_WORLD_PACK to it."
        ),
    }
    match server::world_sql() {
        Some(p) => println!("ACE's world dump found at {}", p.display()),
        None => println!(
            "ACE's world dump NOT found; the importer's real-dump row reports NO-ORACLE. Run \
             `cargo run -p empyrean-import -- fetch` once, or set EMPYREAN_WORLD_SQL."
        ),
    }
    let mut reports = gates::gates(threads(args), profile, gates::GpuTier::Exclude);
    reports.extend(server::real_content(profile, dats.as_deref()));
    reports.push(server::real_dump(profile));
    print_table("CI tier 1 -- the retail data", &reports)
}

/// Tier 2: the `gpu` binaries, then the comparisons against retail and the milestone verdict.
fn tier2(args: &[String], profile: Profile) -> i32 {
    let ws = workspace_root();
    let milestone = args.iter().any(|a| a == "--milestone");
    let dats = retail_data_available();
    let mut informational: Vec<Report> = gates::gpu_invocations(profile, threads(args))
        .into_iter()
        .map(|(krate, call)| {
            let label = format!("gpu tier: {krate}");
            if dats.is_none() {
                return Report::new(
                    label,
                    Outcome::NotRun(NotRun::OracleAbsent),
                    "the retail dats are not found; set DERETH_TEST_DAT_DIR",
                );
            }
            step(&label, call.run())
        })
        .collect();
    // The corpus check is NOT an oracle row -- it compares the derived corpus against the
    // recordings it is derived from, never against retail -- and keeping it out of `oracle_rows`
    // is what stops it standing in for the comparison that is missing. The gpu rows are not
    // oracle rows either: they pin this client's own drawing.
    informational.push(corpus_check_step(
        &ws,
        "message corpus consistency (informational)",
        profile,
    ));
    // Both NOT-BUILT rather than NO-ORACLE: the deciding fact is that the comparison code does
    // not exist, not that a host declined to run it. A capture rig would be
    // `NotRun::Unhostable`; there is nothing yet for a rig to drive.
    //
    // **These two rows are the whole reason tier 2 exists**, and the list is separate from the
    // table precisely so that the milestone question can be asked of them alone. See
    // `milestone_verdict` for the decision and the argument.
    let oracle_rows = vec![
        Report::new(
            "golden images",
            Outcome::NotRun(NotRun::NotBuilt),
            "no capture rig yet",
        ),
        Report::new(
            "long physics traces / full-session replay",
            Outcome::NotRun(NotRun::NotBuilt),
            "deferred deliberately: running these against code that is about to change wastes the effort",
        ),
    ];
    let mut reports = informational;
    reports.extend(oracle_rows.iter().cloned());
    let ran = print_table("CI tier 2 -- hardware and retail comparisons", &reports);
    let verdict = milestone_verdict(&oracle_rows);
    match &verdict {
        Ok(n) => println!("\n  MILESTONE: {n} oracle row(s) passed against retail."),
        Err(msg) => {
            println!("\n  {msg}");
            if milestone {
                println!(
                    "  `--milestone` asks whether an oracle certified this build. It did \
                     not, so this exits non-zero."
                );
            } else {
                println!(
                    "  This run exits {ran} because nothing it RAN failed. That is NOT a \
                     milestone verdict and must not be read as one -- run `cargo xtask ci \
                     tier2 --milestone` for that, which is red today."
                );
            }
        }
    }
    tier2_exit(ran, verdict.is_ok(), milestone)
}

fn ci(args: &[String]) -> i32 {
    let profile = Profile::from_args(args);
    match args.first().map(String::as_str) {
        Some("tier0") => tier0(profile),
        Some("tier1") => tier1(args, profile),
        Some("tier2") => tier2(args, profile),
        _ => {
            eprintln!("usage: cargo xtask ci tier0|tier1|tier2 [--milestone] [--serial] [--debug]");
            2
        }
    }
}

/// `cargo clippy --workspace --all-targets -- -D warnings`, but naming the crates responsible when
/// it fails.
///
/// A workspace-wide gate that just says "clippy failed" makes everyone read the whole log to
/// find out whether the problem is theirs. This names the crates; whoever owns them decides.
fn clippy_step(ws: &std::path::Path) -> Report {
    let (ok, out) = util::run_captured(ws, "cargo", CLIPPY_ARGS);
    if ok {
        return Report::new(
            "cargo clippy --workspace --all-targets",
            Outcome::Pass,
            String::new(),
        );
    }
    print!("{out}");
    // With `-D warnings` a lint becomes an error, so the summary line reads
    //   error: could not compile `dereth-client-net` (lib) due to 4 previous errors
    // Without it, the same crate reports
    //   warning: `dereth-client-net` (lib) generated 4 warnings
    let mut crates: Vec<&str> = out
        .lines()
        .filter(|l| l.contains("could not compile ") || l.contains(") generated "))
        .filter_map(|l| l.split('`').nth(1))
        .collect();
    crates.sort_unstable();
    crates.dedup();
    let detail = if crates.is_empty() {
        "see the log above".to_owned()
    } else {
        format!("warnings in: {}", crates.join(", "))
    };
    Report::new(
        "cargo clippy --workspace --all-targets",
        Outcome::Fail,
        detail,
    )
}

/// Clippy over every crate and target, every warning an error.
const CLIPPY_ARGS: &[&str] = &[
    "clippy",
    "--workspace",
    "--all-targets",
    "--",
    "-D",
    "warnings",
];

/// The workspace is rustfmt-clean; this keeps it so.
const FMT_CHECK: &[&str] = &["fmt", "--all", "--check"];

/// `cargo doc` over the whole workspace, private items included.
const DOC_ARGS: &[&str] = &[
    "doc",
    "--workspace",
    "--no-deps",
    "--document-private-items",
];

/// Every rustdoc lint is an error except `private_intra_doc_links`.
///
/// Most of this workspace's readers are working on it, not calling it, and most of its items are
/// crate-private, so the docs are built with `--document-private-items` and a link from a public
/// item to a private one resolves and renders. The lint that flags such a link exists for crates
/// whose published docs omit private items; here it would only push the docs to stop naming the
/// code they describe. Everything else rustdoc reports (a link that resolves to nothing, an
/// ambiguous link, a bare URL, a stray HTML tag) is a real defect and fails the gate.
const DOC_FLAGS: &str = "-D warnings -A rustdoc::private_intra_doc_links";

/// Run the documentation gate: [`DOC_ARGS`] with [`DOC_FLAGS`] as `RUSTDOCFLAGS`.
fn doc_gate(ws: &std::path::Path) -> bool {
    println!(
        "$ RUSTDOCFLAGS=\"{DOC_FLAGS}\" cargo {}",
        DOC_ARGS.join(" ")
    );
    std::process::Command::new("cargo")
        .args(DOC_ARGS)
        .env("RUSTDOCFLAGS", DOC_FLAGS)
        .current_dir(ws)
        .status()
        .is_ok_and(|s| s.success())
}

fn step(name: &str, ok: bool) -> Report {
    Report::new(
        name,
        if ok { Outcome::Pass } else { Outcome::Fail },
        String::new(),
    )
}

/// A whole-workspace source rule as a tier row: it passes when the check exits 0, and it runs
/// whatever came before it failed.
fn source_rule(name: &str, check: impl FnOnce() -> i32) -> Report {
    println!("--- {name}");
    let outcome = if check() == 0 {
        Outcome::Pass
    } else {
        Outcome::Fail
    };
    Report::new(name, outcome, String::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(words: &[&str]) -> Vec<String> {
        words.iter().map(|s| (*s).to_owned()).collect()
    }

    /// **The test tiers run in `test-release` unless `--debug` is given**, from either side of the
    /// gate name, and no near-miss selects the dev profile.
    #[test]
    fn the_tiers_run_in_test_release_unless_debug_is_given() {
        for cmd in [
            &["test"][..],
            &["gates"],
            &["gate", "client"],
            &["ci", "tier0"],
            &["ci", "tier1"],
        ] {
            assert_eq!(
                Profile::from_args(&argv(cmd)),
                Profile::TestRelease,
                "{cmd:?}"
            );
        }
        assert_eq!(
            Profile::TestRelease.cargo_args(),
            &["--profile", "test-release"]
        );
        for cmd in [
            &["test", "--debug"][..],
            &["gate", "--debug", "client"],
            &["gate", "client", "--debug"],
            &["ci", "tier1", "--serial", "--debug"],
        ] {
            assert_eq!(Profile::from_args(&argv(cmd)), Profile::Dev, "{cmd:?}");
        }
        assert!(
            Profile::Dev.cargo_args().is_empty(),
            "dev is cargo's default profile"
        );
        for near in [
            &["gates", "debug"][..],
            &["gates", "--debug=1"],
            &["gates", "--serial"],
        ] {
            assert_eq!(
                Profile::from_args(&argv(near)),
                Profile::TestRelease,
                "{near:?} is not `--debug`"
            );
        }
        assert_eq!(
            gate_name(&argv(&["gate", "--debug", "client"])),
            Some(&"client".to_owned()),
            "the flag is never mistaken for the gate"
        );
    }

    /// **`--serial` is opt-in and order-independent** (unit R-shard).
    ///
    /// The default has to be the sharded form -- a gate nobody types a flag for is the gate that
    /// actually runs -- and the flag has to be reachable from either side of the gate name,
    /// because `gate --serial client` and `gate client --serial` are both things a person types. The
    /// negative control is the one that matters: nothing else on the command line may turn
    /// sharding off by accident.
    #[test]
    fn serial_is_opt_in_from_either_side_of_the_gate_name() {
        assert_eq!(threads(&argv(&["gates"])), gates::Threads::Sharded);
        assert_eq!(threads(&argv(&["gate", "client"])), gates::Threads::Sharded);
        assert_eq!(
            threads(&argv(&["gates", "--serial"])),
            gates::Threads::Serial
        );
        assert_eq!(
            threads(&argv(&["gate", "--serial", "client"])),
            gates::Threads::Serial
        );
        assert_eq!(
            threads(&argv(&["gate", "client", "--serial"])),
            gates::Threads::Serial
        );
        assert_eq!(
            threads(&argv(&["ci", "tier1", "--serial"])),
            gates::Threads::Serial
        );
        // Negative control: a near-miss must not disarm the default.
        for near in [
            &["gates", "--milestone"][..],
            &["gates", "serial"],
            &["gates", "--serial=1"],
        ] {
            assert_eq!(
                threads(&argv(near)),
                gates::Threads::Sharded,
                "{near:?} is not `--serial` and must not turn sharding off"
            );
        }
        // And the flag must never be mistaken for the gate.
        assert_eq!(
            gate_name(&argv(&["gate", "--serial", "client"])),
            Some(&"client".to_owned())
        );
        assert_eq!(
            gate_name(&argv(&["gate", "client", "--serial"])),
            Some(&"client".to_owned())
        );
        assert_eq!(gate_name(&argv(&["gate"])), None);
        assert_eq!(
            gate_name(&argv(&["gate", "--serial"])),
            None,
            "a flag is not a gate"
        );
    }

    fn row(name: &str, outcome: Outcome) -> Report {
        Report::new(name, outcome, "detail")
    }

    /// The two rows `ci tier2` builds today, verbatim in shape.
    fn tier2_oracle_rows_today() -> Vec<Report> {
        vec![
            row("golden images", Outcome::NotRun(NotRun::NotBuilt)),
            row(
                "long physics traces / full-session replay",
                Outcome::NotRun(NotRun::NotBuilt),
            ),
        ]
    }

    /// **Calibration on the real instance.** This is what `cargo xtask ci tier2` holds on
    /// 2026-09-08 -- two `NotBuilt` oracle rows -- and it is the state the row calls
    /// milestone-unready. The message must name the rows, because a refusal that does not say who
    /// owes the work is a refusal nobody can act on.
    #[test]
    fn two_unbuilt_oracle_rows_cannot_certify_a_milestone() {
        let err = milestone_verdict(&tier2_oracle_rows_today())
            .expect_err("0 of 2 oracle rows passed; this must not certify a milestone");
        assert!(err.contains("MILESTONE NOT CERTIFIED"), "{err}");
        assert!(err.contains("0 of 2"), "{err}");
        assert!(
            err.contains("golden images"),
            "the failure must name the row that owes it: {err}"
        );
        assert!(err.contains("NOT-BUILT"), "and the state it is in: {err}");
    }

    /// Nothing short of a passing oracle row certifies a milestone.
    #[test]
    fn nothing_short_of_a_passing_oracle_row_certifies_a_milestone() {
        let non_pass = [
            Outcome::Fail,
            Outcome::NotRun(NotRun::Unhostable),
            Outcome::NotRun(NotRun::OracleAbsent),
            Outcome::NotRun(NotRun::NotBuilt),
        ];
        for a in non_pass {
            assert!(
                milestone_verdict(&[row("golden images", a)]).is_err(),
                "{a:?} alone certified a milestone"
            );
            for b in non_pass {
                assert!(
                    milestone_verdict(&[row("golden images", a), row("traces", b)]).is_err(),
                    "{a:?} + {b:?} certified a milestone"
                );
            }
        }
    }

    /// One passing oracle row is enough and the gate can go green.
    #[test]
    fn one_passing_oracle_row_is_enough_and_the_gate_can_go_green() {
        assert_eq!(
            milestone_verdict(&[row("golden images", Outcome::Pass)]),
            Ok(1)
        );
        assert_eq!(
            milestone_verdict(&[
                row("golden images", Outcome::Pass),
                row("traces", Outcome::NotRun(NotRun::NotBuilt)),
            ]),
            Ok(1),
            "landing alone must clear the milestone gate; Wave 3 is deliberately deferred"
        );
        assert_eq!(
            milestone_verdict(&[row("a", Outcome::Pass), row("b", Outcome::Pass)]),
            Ok(2)
        );
    }

    /// **The empty-space guard.** `0 of 0` is a wrong-space census with a confident yes, and it is
    /// reachable by the cheapest possible edit: deleting the two rows that fail.
    /// An empty oracle list must be distinguishable from a complete passing list.
    /// Removing every row must not be a way to certify a milestone.
    #[test]
    fn an_oracle_tier_with_no_oracle_rows_at_all_certifies_nothing() {
        let err = milestone_verdict(&[]).expect_err("an empty oracle tier must not certify");
        assert!(err.contains("NO oracle rows at all"), "{err}");
    }

    /// **The exit code the two questions produce.** `milestone_verdict` decides the verdict;
    /// this decides what the process returns, and until it was extracted no test covered the
    /// plumbing between them -- which would have made "exits non-zero" a claim resting on one
    /// manual run.
    ///
    /// The four rows are: the calibration (the milestone question, red today); the decision (the
    /// default run stays green, which is the whole argument of this unit); the proof the gate can
    /// clear (green the day an oracle passes, without editing this file); and the guarantee that
    /// the flag can only ever redden -- a genuine failure in what RAN is never masked by either
    /// mode.
    #[test]
    fn the_milestone_flag_can_only_ever_make_a_run_redder() {
        assert_eq!(
            tier2_exit(0, false, true),
            1,
            "the milestone question is red today"
        );
        assert_eq!(
            tier2_exit(0, false, false),
            0,
            "the default run keeps measuring what it ran"
        );
        assert_eq!(
            tier2_exit(0, true, true),
            0,
            "and it goes green when an oracle passes"
        );
        assert_eq!(tier2_exit(0, true, false), 0);
        for milestone in [false, true] {
            for certified in [false, true] {
                assert_eq!(
                    tier2_exit(1, certified, milestone),
                    1,
                    "a run that FAILED must stay failed (certified={certified}, \
                     milestone={milestone})"
                );
            }
        }
    }

    /// The informational step is not an oracle row.
    #[test]
    fn the_informational_step_is_not_an_oracle_row() {
        let with_stale_counted = vec![
            row("message corpus consistency (informational)", Outcome::Pass),
            row("golden images", Outcome::NotRun(NotRun::NotBuilt)),
            row("traces", Outcome::NotRun(NotRun::NotBuilt)),
        ];
        assert_eq!(
            milestone_verdict(&with_stale_counted),
            Ok(1),
            "counting the informational step WOULD certify a milestone -- which is why `ci tier2` \
             keeps it out of `oracle_rows`, and why moving it in is the mutation to fear"
        );
        assert!(milestone_verdict(&tier2_oracle_rows_today()).is_err());
    }
}
