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
//! | on hardware | `cargo xtask ci tier2` | a graphics device and the dats |
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
mod publish;
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
  ci tier2                  on hardware: the gpu tier and the retail comparisons. Needs a graphics
                            device and the dats. The client's gpu binary runs sharded
                            (DERETH_TEST_SHARDS=N); --serial: one process

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
  package web [--out <dir>] [--tag <tag>] [--no-build] [--allow-dirty]
                            the web client's release files: builds the module, stages the page's
                            allowlist, runs the deny scan, and writes dereth-web-<version>.zip,
                            web.json (the update manifest) and SHA256SUMS (dereth/web/DEPLOY.md)
  publish dereth|empyrean|web <version> --upload <dir> | --finish | --publish
          [--repo <owner/name>] [--yes]
                            the GitHub release without the release workflows: each machine
                            uploads its `package` folder to the tag's draft (same name and bytes
                            skipped, other bytes refused); --finish lists what is missing or
                            writes the merged index and the notes; --publish publishes the draft.
                            A dry run unless --yes; the token is DERETH_PUBLISH_TOKEN or
                            GITHUB_TOKEN (CONTRIBUTING.md, Releasing without GitHub Actions)
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
        Some("publish") => publish::publish(&args[1..]),
        Some("release") => release::release(&args[1..]),
        Some("release-notes") => release::notes::release_notes(&args[1..]),
        Some("version") => package::version_command(&args[1..]),
        Some("web") => web(&args[1..]),
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
    // A workspace build turns the device's high-fidelity seam on, because the presentation that
    // asks for it is a member; the client is shipped without it. So the device is also built and
    // tested on its own, as the shipped client has it.
    let mut device_alone = vec!["test", "-p", "dereth-render"];
    device_alone.extend_from_slice(profile.cargo_args());
    let mut reports = vec![
        step("cargo build --workspace", run(&ws, "cargo", &build)),
        step(
            "cargo test (client and shared crates)",
            run(&ws, "cargo", &test),
        ),
        step(
            "cargo test -p dereth-render (the device as shipped)",
            run(&ws, "cargo", &device_alone),
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
    match server::infiltration_pack() {
        Some(p) => println!("world.pack.infiltration found at {}", p.display()),
        None => println!(
            "world.pack.infiltration NOT found; the real-content rows that read the February 2005 \
             era report NO-ORACLE. Build it with `cargo run -p empyrean-import -- fetch --world 16py \
             --pack --out world.pack.infiltration`, or set EMPYREAN_TEST_INFILTRATION_PACK to it."
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

/// Tier 2: the `gpu` binaries, then the comparisons against retail.
fn tier2(args: &[String], profile: Profile) -> i32 {
    let ws = workspace_root();
    let dats = retail_data_available();
    let mut reports: Vec<Report> = gates::gpu_invocations(profile, threads(args))
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
    // The corpus check compares the derived corpus against the recordings it is derived from,
    // never against retail, and the gpu rows pin this client's own drawing; neither is a
    // comparison with retail.
    reports.push(corpus_check_step(
        &ws,
        "message corpus consistency (informational)",
        profile,
    ));
    // NOT-BUILT rather than NO-ORACLE: the comparison code does not exist, so there is nothing a
    // host could have declined to run.
    reports.push(Report::new(
        "retail comparisons",
        Outcome::NotRun(NotRun::NotBuilt),
        "golden images and long physics traces against retail are not built yet",
    ));
    print_table("CI tier 2 -- hardware and retail comparisons", &reports)
}

fn ci(args: &[String]) -> i32 {
    let profile = Profile::from_args(args);
    match args.first().map(String::as_str) {
        Some("tier0") => tier0(profile),
        Some("tier1") => tier1(args, profile),
        Some("tier2") => tier2(args, profile),
        _ => {
            eprintln!("usage: cargo xtask ci tier0|tier1|tier2 [--serial] [--debug]");
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
}
