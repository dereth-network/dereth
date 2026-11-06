//! The workspace task runner: the gate tiers, the clippy policy, the seams and the checks clippy
//! cannot express.
//!
//! **Depends on** two of the workspace's crates: `dereth-dat`, for the tests' lookup of the retail
//! dats (`dereth_dat::testing`), and `dereth-plugin-package`, to pack and list Dereth's own plugin
//! source (`package plugins`); otherwise it runs `cargo` and reads the workspace's files.
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
//! | when a test might lean on data you have | `cargo xtask ci no-data` | nothing: the tests as public CI runs them |
//! | with the retail data | `cargo xtask ci tier1` | the dats and `world.pack`; Asheron's Call 2's data files and capture index |
//! | on hardware | `cargo xtask ci tier2` | a graphics device and the dats |
//!
//! Each covers both games, Asheron's Call and Asheron's Call 2; `--game dereth` or `--game auberean`
//! limits one to a single game's crates (see [`game`]).
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
mod deploy;
mod game;
mod gates;
mod line_endings;
mod lint;
mod no_data;
mod output_hygiene;
mod package;
mod preflight;
mod publish;
mod release;
mod seams;
mod separation;
mod server;
mod skip_claims;
mod sweep;
mod test_lint;
mod testsrc;
mod tier_targets;
mod util;
#[cfg(test)]
mod workflow_tests;

use game::Game;
use util::{
    print_table, retail_data_available, run, workspace_root, NotRun, Outcome, Profile, Report,
};

const USAGE: &str = "\
usage: cargo xtask <task> [--debug]

When to run what (client and server alike, both games):
  check                     while working, before each commit: clippy and the tests of the crates
                            changed since the base branch (their dependents linted too), and the
                            fast lints. Needs nothing. --base <ref> (default: research,
                            origin/HEAD, origin/main or main), -p <crate> to name the crates
  ci tier0                  before pushing or opening a PR: exactly what public CI runs. The build,
                            every no-data test, clippy, every rule below and the docs. Needs nothing.
                            --only build,test,clippy,docs,rules: those parts alone (public CI runs
                            the first three on every system, and the rules and docs once)
  ci no-data                the workspace's tests as public CI runs them, on any machine: no data
                            (every variable naming it emptied), an empty per-user cache and
                            --no-fail-fast, then a list of everything that failed
  ci tier1                  with the retail data: every crate's data tiers and the server's
                            real-content tier. Needs the dats (DERETH_TEST_DAT_DIR), world.pack
                            (EMPYREAN_TEST_WORLD_PACK) and ACE's world dump (EMPYREAN_WORLD_SQL, or
                            `empyrean-import fetch` once). --serial: the unsharded client dat tier.
                            Then Asheron's Call 2's: its crates' tests that read nothing and its
                            data tiers (gates auberean-dat, auberean-script, auberean-corpus,
                            auberean-live), which need its data files and capture index
                            (AUBEREAN_TEST_* variables)
  ci tier2                  on hardware: the gpu tier and the retail comparisons. Needs a graphics
                            device and the dats. The client's gpu binary runs sharded
                            (DERETH_TEST_SHARDS=N); --serial: one process. Then Asheron's Call
                            2's (gate auberean-gpu), which has no gpu binary yet
  ci auberean              the same as `ci tier1 --game auberean`
  --game dereth|auberean|all
                            on build, check, ci tier0|tier1|tier2 and gates: one game's crates only.
                            auberean (Asheron's Call 2): auberean-*, yalain, yalain-* and athenaeum-*;
                            dereth (Asheron's Call): every other crate. Default all. The rules
                            below run whatever the filter

Rules (each a row of `check` or `ci tier0`):
  fmt-check                 cargo fmt --all --check
  lint                      the bans clippy cannot express (`as i32`, unannotated HashMap) and the seams
  seams                     the seams alone: contract, ui-screens, client-runtime, client crates
  crate-docs                every crate's //! header and its Cargo description
  comment-lint              comments and messages describe behaviour: no work-item labels,
                            private-document pointers or study-method words
  test-lint [--list [p]]    tests named for what they prove and anchored to the claim they prove
  tier-targets              every dat, corpus, gpu or live test target declares required features
  server-hygiene            the server's source rules (no todo!/unimplemented!, the port header)
  doc                       a check, not a docs build: rustdoc links and doc comments stay correct
                            (every rustdoc warning an error except links to private items). It writes
                            no pages; plain `cargo doc` does
  separation [--client-only] [ROOT]
                            the client/server dependency and licence rules
  output-hygiene [ROOT]     no test or tool writes into a tracked path or outside the workspace
  line-endings --tree [ROOT] | --normalise FILE... | FILE...
                            every tracked file has the ending .gitattributes gives it (ROOT: any
                            git checkout, default the workspace)
  skip-claims --tree [ROOT] no test skips when the retail dats are absent, and no comment claims
                            one does

Tests:
  build [--game G] [ARGS]   cargo build of one game's products and tools, or both (ARGS go to cargo:
                            `cargo xtask build --game auberean --release`); on Windows, Asheron's
                            Call 2's also builds the retail client's friends library
                            (auberean_friends.dll) beside the programs
  test | clippy             cargo test / clippy -D warnings over the whole workspace (clippy with
                            `retail-dats` on, so the data and device tests are linted too)
  gate <name>               one area's whole suite, data and device tiers included (--serial);
                            names: {GATES}
  gates [--game G]          every area's gate, one table (both games')
  sweep [SUITE...] [--shard-binary PKG:TIER] [--serial] [--shards N] ...
                            run test targets one by one, or one tier binary sharded across
                            processes, with a verdict that cannot read as silence (--help)

Release and packaging:
  package empyrean [--target <triple>]... [--out <dir>] [--allow-dirty] | --gather <dir>
                            Empyrean's release archives: builds both binaries per target, stages
                            the allowlist, runs the deny scan and the header checks, and writes
                            the archives, MANIFEST.txt, release.json and SHA256SUMS
                            (empyrean/RELEASING.md)
  package dereth --launcher <dir> [--target <triple>]... [--out <dir>] [--allow-dirty]
                 [--moltenvk <dir>] | --gather <dir>
                            Dereth's release files: builds the client for this host, and puts it in
                            the launcher release the tree names (Windows zip, macOS Dereth.app,
                            Linux AppImage), checked and scanned, and signed for the launcher's
                            updater (latest.json) when TAURI_SIGNING_PRIVATE_KEY is set
                            (launcher/RELEASING.md)
  package lyceum [--target <triple>]... [--out <dir>] [--allow-dirty] [--moltenvk <dir>]
                 | --gather <dir>
                            Lyceum's release files: builds the editor and the lyceum command for
                            this host, and writes them with the notices, README and CHANGES
                            (Windows zip, macOS Lyceum.app, Linux tarball with a desktop entry),
                            checked and scanned, with MANIFEST.txt, release.json and SHA256SUMS
                            (lyceum/RELEASING.md)
  package launcher [--target <triple>]... [--out <dir>] [--allow-dirty] [--moltenvk <dir>]
          [--friends-dll <file>] | --gather <dir> | --fetch <dir> [--wait] [--repo <owner/name>]
                            the launcher's own release files, with no game client beside it;
                            --fetch gets the release of the version in the tree
  package auberean --launcher <dir> [--target <triple>]... [--out <dir>] [--allow-dirty]
                 | --gather <dir>
                            Auberean's release files: the launcher release the tree names, with the
                            Auberean client and launcher-defaults.json beside it (Windows zip,
                            macOS Dereth.app, Linux AppImage), checked and scanned, and signed for the launcher's
                            updater (latest.json, by the launcher's version) when
                            TAURI_SIGNING_PRIVATE_KEY is set (auberean/RELEASING.md)
  package friends-dll [--out <dir>] [--allow-dirty]
                            the retail AC2 client's friends library (auberean_friends.dll), which
                            the launcher's package carries: built on Windows for
                            i686-pc-windows-msvc with the C runtime linked in, and checked; a
                            launcher package on another host takes it with --friends-dll
  package athenaeum [--target <triple>]... [--out <dir>] [--allow-dirty] | --gather <dir>
                            Asheron's Call 2's world builder: the editor and the athenaeum command,
                            with yalain and auberean beside them, built for this host and written
                            with the notices, README and CHANGES (Windows zip, macOS .app, Linux
                            AppImage packed by appimagetool), checked and scanned, with
                            MANIFEST.txt, release.json and SHA256SUMS (auberean/RELEASING.md)
  package yalain [--target <triple>]... [--out <dir>] [--allow-dirty] | --gather <dir>
                            Asheron's Call 2's server: builds yalain per target as Empyrean's are
                            built, stages the allowlist, runs the deny scan and the header check,
                            and writes the archives, MANIFEST.txt, release.json and SHA256SUMS
  package web [--out <dir>] [--tag <tag>] [--no-build] [--allow-dirty]
                            the web client's release files: builds the modules, stages the page's
                            allowlist, runs the deny scan, and writes dereth-web-<version>.zip,
                            web.json (the update manifest) and SHA256SUMS (dereth/web/DEPLOY.md)
  package plugins [--out <dir>]
                            Dereth's plugin source: each sample of dereth/plugins packed into a zip
                            and listed in plugins.json, as the web release publishes it beside
                            the page (docs/plugins/sources.md)
  publish drafts [<tag>...] [--wait] [--repo <owner/name>] [--yes]
                            publish the whole drafts the release workflows left, Dereth's final
                            release as Latest; --wait stays until the tags named (else those on
                            main's head) are published, and names any workflow that failed. A dry
                            run unless --yes.
  publish dereth|empyrean|web|lyceum <version> --upload <dir> | --finish | --publish
          [--repo <owner/name>] [--yes]
                            the GitHub release without the release workflows: each machine
                            uploads its `package` folder to the tag's draft (same name and bytes
                            skipped, other bytes refused); --finish lists what is missing or
                            writes the merged index and the notes; --publish publishes the draft.
                            A dry run unless --yes; the token is DERETH_PUBLISH_TOKEN or
                            GITHUB_TOKEN (CONTRIBUTING.md, Releasing without GitHub Actions)
  deploy web <version>|latest [--project <name>] [--branch <name>] [--zip <file>] [--dry-run]
                            put a published web client release on Cloudflare Pages: downloads it,
                            checks it against its web.json, unpacks it and runs `wrangler pages
                            deploy` on it; the project is --project or DERETH_PAGES_PROJECT, the
                            login wrangler's own (dereth/web/DEPLOY.md)
  deploy plugins [--project <name>] [--force] [--dry-run]
                            put Dereth's plugin source on its Cloudflare Pages project, the host
                            of https://plugins.dereth.network/plugins.json: built from a commit
                            main has (a branch's with --force), with its _headers; the project is
                            --project or DERETH_PLUGINS_PAGES_PROJECT (docs/plugins/sources.md)
  release empyrean|dereth|lyceum|auberean|yalain|athenaeum <version> [--push] [--remote <name>]
                            cut a release on a clean main: set the version and commit it, run
                            tier 0 (and, for Asheron's Call 2's products, tier 1 --game auberean
                            when its data is found) and a host package, tag <product>-v<version>;
                            pushes only with --push. A final release also turns the Unreleased
                            section of the product's CHANGES.md into the release's own, dated;
                            Lyceum's and Asheron's Call 2's refuse a release without highlights
  release network-hub|network-ace <version> [--push] [--remote <name>]
                            the same for the Dereth Network's hub or ACE mod: set its version
                            and commit it, run its checks as CI does (the mod's need dotnet,
                            and those that need ACE run when DERETH_ACE_DIR names an ACE server
                            folder), build the hub's release binary for this host (the hub),
                            tag network-hub-v<version> or network-ace-v<version>. No
                            CHANGES.md: the release workflow writes the notes from the commits
                            (network/RELEASING.md)
  preflight [--with <world>]... [--dry-run] | play [--web] | rollback | finish | status | setup
                            on a branch, before anything is released: push the commit to the
                            remote's `preflight` branch (mirrored to GitHub, where CI runs on it),
                            build what changed since main as release candidates, run the dev hub
                            on a defanged copy of the real registry, and install the candidates in
                            the dev worlds, linked to it; the setup file is the owner's
                            (network/RELEASING.md, Preflight)
  ship [--dry-run] [--skip-checks | --allow-failing] [--local-tier0]
                            after a preflight: once CI is green on exactly the preflighted commit,
                            main fast-forwarded to it (refused when its branch has moved), each
                            changed product released and pushed as `release` does it, in this
                            process, and the web client tagged with Dereth; --skip-checks does not
                            ask CI, --allow-failing ships despite failed checks, --local-tier0 runs
                            tier 0 here first
  release-notes dereth|empyrean|lyceum|auberean|yalain|athenaeum|launcher [--since <tag>] [--version <v>]
                [--out <file>]
                            print a release's notes without releasing: the highlights of the
                            product's CHANGES.md, then its commits since the previous final
                            <product>-v tag, grouped (the release workflow writes them the same
                            way); a first release has its highlights alone
  version empyrean|dereth|lyceum|auberean|yalain|athenaeum|launcher [--tag <tag>]
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
    if let Some(rustdoc) = std::env::var_os(RUSTDOC_STAND_IN) {
        std::process::exit(rustdoc_stand_in(&rustdoc));
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    let profile = Profile::from_args(&args);
    let code = match args.first().map(String::as_str) {
        Some("check") => check::check(&args[1..]),
        Some("build") => match Game::from_args(&args[1..]) {
            Ok(game) => {
                let rest = Game::without_flag(&args[1..]);
                let mut argv = vec!["build"];
                argv.extend(game.selection());
                argv.extend(rest.iter().map(String::as_str));
                if !run(&workspace_root(), "cargo", &argv) {
                    1
                } else if cfg!(windows) && game.auberean() {
                    // The retail client's friends library, beside the launcher built here.
                    match package::friends_dll::build_beside(&rest) {
                        Ok(_) => 0,
                        Err(e) => {
                            eprintln!("the retail AC2 client's friends library: {e}");
                            1
                        }
                    }
                } else {
                    0
                }
            }
            Err(e) => {
                eprintln!("{e}");
                2
            }
        },
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
        Some("tier-targets") => tier_targets::tier_targets(),
        Some("fmt-check") => i32::from(!run(&workspace_root(), "cargo", FMT_CHECK)),
        Some("doc") => i32::from(!doc_gate(&workspace_root(), DOC_ARGS)),
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
        Some("gates") => match Game::from_args(&args) {
            Ok(game) => all_gates(&args, profile, game),
            Err(e) => {
                eprintln!("gates: {e}");
                2
            }
        },
        Some("bundle-macos") => bundle::bundle_macos(&args[1..]),
        Some("package") => package::package(&args[1..]),
        Some("publish") => publish::publish(&args[1..]),
        Some("deploy") => deploy::deploy(&args[1..]),
        Some("release") => release::release(&args[1..]),
        Some("release-notes") => release::notes::release_notes(&args[1..]),
        Some("preflight") => preflight::preflight(&args[1..]),
        Some("ship") => preflight::ship_command(&args[1..]),
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
        step("xtask tier-targets", tier_targets::tier_targets() == 0),
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

/// One row of tier 0 that runs cargo.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Tier0Step {
    /// `cargo <argv>`, its row named by the label.
    Cargo(String, Vec<&'static str>),
    /// The server's unit tier and its time budget ([`server::unit_tier`]).
    ServerUnits,
    /// Clippy, naming the crates responsible when it fails: the row's label and `cargo <argv>`.
    Clippy(String, Vec<&'static str>),
    /// The documentation check: `cargo <argv>` with [`DOC_FLAGS`], its row named by the label.
    Doc(String, Vec<&'static str>),
}

impl Tier0Step {
    /// The part of tier 0 this row belongs to.
    fn part(&self) -> Tier0Part {
        match self {
            Tier0Step::Cargo(_, argv) if argv.first() == Some(&"build") => Tier0Part::Build,
            Tier0Step::Cargo(..) | Tier0Step::ServerUnits => Tier0Part::Test,
            Tier0Step::Clippy(..) => Tier0Part::Clippy,
            Tier0Step::Doc(..) => Tier0Part::Docs,
        }
    }
}

/// A part of tier 0, for `--only`: public CI runs the build, the tests and clippy on every system,
/// and the rules and the docs, which read the same source everywhere, once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tier0Part {
    Build,
    Test,
    Clippy,
    Docs,
    Rules,
}

impl Tier0Part {
    const ALL: [Tier0Part; 5] = [
        Tier0Part::Build,
        Tier0Part::Test,
        Tier0Part::Clippy,
        Tier0Part::Docs,
        Tier0Part::Rules,
    ];

    fn parse(name: &str) -> Result<Self, String> {
        match name {
            "build" => Ok(Tier0Part::Build),
            "test" => Ok(Tier0Part::Test),
            "clippy" => Ok(Tier0Part::Clippy),
            "docs" => Ok(Tier0Part::Docs),
            "rules" => Ok(Tier0Part::Rules),
            other => Err(format!(
                "--only takes build, test, clippy, docs and rules, not {other:?}"
            )),
        }
    }

    /// The parts `--only a,b` names; every part when it is not given.
    fn from_args(args: &[String]) -> Result<Vec<Self>, String> {
        let mut named = None;
        let mut it = args.iter();
        while let Some(arg) = it.next() {
            if arg == "--only" {
                named = Some(it.next().ok_or("--only names at least one part")?.as_str());
            } else if let Some(value) = arg.strip_prefix("--only=") {
                named = Some(value);
            }
        }
        match named {
            None => Ok(Self::ALL.to_vec()),
            Some(list) => list.split(',').map(Self::parse).collect(),
        }
    }
}

/// Tier 0's cargo rows for `game`, in order. Both games' are the rows tier 0 has always run; one
/// game's select its crates in each command and leave out the rows that are the other's alone (the
/// device and the runtime as shipped, and the server, are the first game's).
fn tier0_steps(game: Game, profile: Profile) -> Vec<Tier0Step> {
    let with = |verb: &'static str, extra: &[&'static str]| -> Vec<&'static str> {
        let mut argv = vec![verb];
        argv.extend(game.selection());
        argv.extend_from_slice(extra);
        argv.extend_from_slice(profile.cargo_args());
        argv
    };
    let mut steps = vec![Tier0Step::Cargo(
        format!("cargo build {}", game.scope()),
        with("build", &[]),
    )];
    if game.dereth() {
        // The server's tests are a row of their own, timed against its host budget.
        steps.push(Tier0Step::Cargo(
            "cargo test (client and shared crates)".to_owned(),
            with("test", &["--exclude", "empyrean-*", "--no-fail-fast"]),
        ));
        // A workspace build turns the device's high-fidelity seam on, because the presentation
        // that asks for it is a member; the client is shipped without it. So the device is also
        // built and tested on its own, as the shipped client has it.
        let mut device_alone = vec!["test", "-p", "dereth-render", "--no-fail-fast"];
        device_alone.extend_from_slice(profile.cargo_args());
        steps.push(Tier0Step::Cargo(
            "cargo test -p dereth-render (the device as shipped)".to_owned(),
            device_alone,
        ));
        steps.push(Tier0Step::ServerUnits);
    } else {
        steps.push(Tier0Step::Cargo(
            "cargo test (Asheron's Call 2's crates)".to_owned(),
            with("test", &["--no-fail-fast"]),
        ));
    }
    steps.push(Tier0Step::Clippy(
        format!(
            "cargo clippy {} --all-targets --features {}",
            game.scope(),
            clippy_features(game)
        ),
        clippy_args(game),
    ));
    let mut doc = vec!["doc"];
    doc.extend(game.selection());
    doc.extend(["--no-deps", "--document-private-items"]);
    steps.push(Tier0Step::Doc(
        "xtask doc (rustdoc warnings denied)".to_owned(),
        doc,
    ));
    if game.dereth() {
        // One workspace doc build merges the features every member asks of the runtime, the test
        // kit's test support among them, which the client is shipped without. So the runtime is
        // also documented on its own, with its own features, as the client has it.
        steps.push(Tier0Step::Doc(
            "xtask doc -p dereth-client-runtime (the runtime as shipped)".to_owned(),
            RUNTIME_DOC_ARGS.to_vec(),
        ));
    }
    steps
}

/// Tier 0's test rows as commands: the label and `cargo <argv>` of each, the server's included.
fn tier0_test_commands(game: Game, profile: Profile) -> Vec<(String, Vec<&'static str>)> {
    tier0_steps(game, profile)
        .into_iter()
        .filter(|planned| planned.part() == Tier0Part::Test)
        .map(|planned| match planned {
            Tier0Step::ServerUnits => {
                let mut argv = vec!["test"];
                argv.extend_from_slice(profile.cargo_args());
                argv.extend_from_slice(server::SERVER_CRATES);
                argv.push("--no-fail-fast");
                ("cargo test (the server's crates)".to_owned(), argv)
            }
            Tier0Step::Cargo(label, argv)
            | Tier0Step::Clippy(label, argv)
            | Tier0Step::Doc(label, argv) => (label, argv),
        })
        .collect()
}

/// Tier 0: everything that needs no data, client and server alike, each thing once.
///
/// The client's and the server's tests are two rows (the server's run is timed against its
/// host budget); the build, clippy and the docs cover the whole workspace in one invocation each.
/// The divergence lists and the message corpus are checked by tests in the `cargo test` rows, so
/// they are not run again as rows of their own. `game` limits the cargo rows to one game's crates
/// ([`tier0_steps`]); the rules read the whole repository whatever it is.
fn tier0(profile: Profile, game: Game) -> i32 {
    tier0_parts(profile, game, &Tier0Part::ALL)
}

/// The `parts` of tier 0, in tier 0's order. Each cargo row says how long it took: the numbers to
/// read when tier 0 gets slow.
fn tier0_parts(profile: Profile, game: Game, parts: &[Tier0Part]) -> i32 {
    let ws = workspace_root();
    let mut reports = Vec::new();
    for planned in tier0_steps(game, profile) {
        if !parts.contains(&planned.part()) {
            continue;
        }
        let started = std::time::Instant::now();
        let mut rows = match planned {
            Tier0Step::Cargo(label, argv) => vec![step(&label, run(&ws, "cargo", &argv))],
            Tier0Step::ServerUnits => server::unit_tier(profile),
            Tier0Step::Clippy(label, argv) => vec![clippy_step(&ws, &label, &argv)],
            Tier0Step::Doc(label, argv) => vec![step(&label, doc_gate(&ws, &argv))],
        };
        say_time_taken(&mut rows, started.elapsed());
        reports.extend(rows);
    }
    if parts.contains(&Tier0Part::Rules) {
        reports.extend(cheap_lints());
        reports.push(source_rule("separation", || {
            separation::report(&separation::check(&ws, false))
        }));
        reports.push(source_rule("output-hygiene", || output_hygiene::run(&ws)));
    }
    print_table(
        &format!(
            "CI tier 0 -- no data (before pushing; public CI){}",
            game.title_suffix()
        ),
        &reports,
    )
}

/// Adds how long a tier 0 row took to its first report's detail.
fn say_time_taken(rows: &mut [Report], taken: std::time::Duration) {
    let Some(row) = rows.first_mut() else {
        return;
    };
    let secs = taken.as_secs();
    let taken = format!("took {}m {:02}s", secs / 60, secs % 60);
    row.detail = if row.detail.is_empty() {
        taken
    } else {
        format!("{}, {taken}", row.detail)
    };
}

/// Tier 1: every area's data tier (the gates without their `gpu` binaries) and the server's
/// real-content tier, then Asheron's Call 2's crates' tests and data tiers; `game` limits it to one
/// game. A game whose data is not found reports its rows NO-ORACLE and the run exits non-zero; the
/// other game's rows run as they would alone.
fn tier1(args: &[String], profile: Profile, game: Game) -> i32 {
    let mut reports = Vec::new();
    if game.dereth() {
        reports.extend(tier1_first_game(args, profile, game));
    }
    if game.auberean() {
        gates::auberean::announce(game);
        reports.extend(gates::auberean::tier1_rows(profile));
    }
    print_table(
        &format!("CI tier 1 -- the retail data{}", game.title_suffix()),
        &reports,
    )
}

/// Tier 1's rows for Asheron's Call: what is found, then every gate without its `gpu` binaries,
/// the server's real-content rows and the importer's real dump.
fn tier1_first_game(args: &[String], profile: Profile, game: Game) -> Vec<Report> {
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
                 non-zero. Set DERETH_TEST_DAT_DIR to the directory holding them.{}",
                owed.len(),
                owed.join(", "),
                if game == Game::All {
                    " `--game auberean` runs Asheron's Call 2's tiers alone."
                } else {
                    ""
                }
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
    match server::launch_fixture() {
        Some(p) => println!("the Launch world fixture found at {}", p.display()),
        None => println!(
            "the Launch world fixture NOT found; the Launch world row reports NO-ORACLE. Set              EMPYREAN_TEST_LAUNCH_FIXTURE to its manifest (the 1999 dats and an ac1999 pack)."
        ),
    }
    let mut reports = gates::gates(threads(args), profile, gates::GpuTier::Exclude);
    reports.extend(server::real_content(profile, dats.as_deref()));
    reports.push(server::real_dump(profile));
    reports.push(server::launch_content(profile));
    reports
}

/// Tier 2: the `gpu` binaries, then the comparisons against retail; `game` limits it to one game.
fn tier2(args: &[String], profile: Profile, game: Game) -> i32 {
    let mut reports = Vec::new();
    if game.dereth() {
        reports.extend(tier2_first_game(args, profile));
    }
    if game.auberean() {
        reports.extend(gates::auberean::gpu_rows(profile));
    }
    print_table(
        &format!(
            "CI tier 2 -- hardware and retail comparisons{}",
            game.title_suffix()
        ),
        &reports,
    )
}

/// Tier 2's rows for Asheron's Call: its `gpu` binaries, the corpus check and the comparisons.
fn tier2_first_game(args: &[String], profile: Profile) -> Vec<Report> {
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
    reports
}

/// `cargo xtask gates`: every area's gate, one table; `game` limits it to one game's.
fn all_gates(args: &[String], profile: Profile, game: Game) -> i32 {
    let mut reports = Vec::new();
    if game.dereth() {
        reports.extend(gates::gates(
            threads(args),
            profile,
            gates::GpuTier::Include,
        ));
    }
    if game.auberean() {
        reports.extend(gates::auberean::gate_rows(profile));
    }
    print_table(
        &format!("Gates -- every area's whole suite{}", game.title_suffix()),
        &reports,
    )
}

/// A CI tier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CiTier {
    Zero,
    NoData,
    One,
    Two,
}

/// The usage line of `cargo xtask ci`.
const CI_USAGE: &str =
    "usage: cargo xtask ci tier0|no-data|tier1|tier2|auberean [--game dereth|auberean|all] [--only part,...] [--serial] [--debug]";

/// The tier and the game `cargo xtask ci <args>` asks for. `auberean` is tier 1 for Asheron's
/// Call 2, so a `--game` beside it may only say the same.
fn ci_request(args: &[String]) -> Result<(CiTier, Game), String> {
    let game = Game::given(args)?;
    let tier = match args.first().map(String::as_str) {
        Some("tier0") => CiTier::Zero,
        Some("no-data") => CiTier::NoData,
        Some("tier1") => CiTier::One,
        Some("tier2") => CiTier::Two,
        Some("auberean") => {
            return match game {
                None | Some(Game::Auberean) => Ok((CiTier::One, Game::Auberean)),
                Some(_) => {
                    Err("`ci auberean` is `ci tier1 --game auberean`; name no other game".into())
                }
            }
        }
        _ => return Err("name a tier".into()),
    };
    Ok((tier, game.unwrap_or(Game::All)))
}

fn ci(args: &[String]) -> i32 {
    let profile = Profile::from_args(args);
    let parts = match Tier0Part::from_args(args) {
        Ok(parts) => parts,
        Err(e) => {
            eprintln!("ci: {e}\n{CI_USAGE}");
            return 2;
        }
    };
    if parts != Tier0Part::ALL && args.first().map(String::as_str) != Some("tier0") {
        eprintln!("ci: --only is for tier0\n{CI_USAGE}");
        return 2;
    }
    match ci_request(args) {
        Ok((CiTier::Zero, game)) => tier0_parts(profile, game, &parts),
        Ok((CiTier::NoData, game)) => no_data::no_data(no_data::commands(game, profile)),
        Ok((CiTier::One, game)) => tier1(args, profile, game),
        Ok((CiTier::Two, game)) => tier2(args, profile, game),
        Err(e) => {
            eprintln!("ci: {e}\n{CI_USAGE}");
            2
        }
    }
}

/// Clippy over `game`'s crates ([`clippy_args`]), but naming the crates responsible when it fails;
/// its row is called `label`.
///
/// A workspace-wide gate that just says "clippy failed" makes everyone read the whole log to
/// find out whether the problem is theirs. This names the crates; whoever owns them decides.
fn clippy_step(ws: &std::path::Path, label: &str, argv: &[&str]) -> Report {
    let (ok, out) = util::run_captured(ws, "cargo", argv);
    if ok {
        return Report::new(label, Outcome::Pass, String::new());
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
    Report::new(label, Outcome::Fail, detail)
}

/// The features clippy turns on over `game`'s crates: `retail-dats` and, where Asheron's Call 2's
/// crates are among them, `retail-corpus`, which only they have (cargo refuses a feature no
/// selected crate has).
fn clippy_features(game: Game) -> &'static str {
    if game.auberean() {
        "retail-dats,retail-corpus"
    } else {
        "retail-dats"
    }
}

/// [`CLIPPY_ARGS`] over `game`'s crates: the same command for both games, the same lints and
/// targets over one game's crates.
fn clippy_args(game: Game) -> Vec<&'static str> {
    if game == Game::All {
        return CLIPPY_ARGS.to_vec();
    }
    let mut argv = vec!["clippy"];
    argv.extend(game.selection());
    argv.extend([
        "--all-targets",
        "--features",
        clippy_features(game),
        "--",
        "-D",
        "warnings",
    ]);
    argv
}

/// Clippy over every crate and target, every warning an error. `retail-dats` and
/// `retail-corpus` are on, or `--all-targets` would leave out every test binary that reads the
/// retail dats (each crate's `dat` and `gpu` tiers) or Asheron's Call 2's recorded sessions (its
/// `corpus` tiers) without a word; they only select targets, so nothing needs the data.
const CLIPPY_ARGS: &[&str] = &[
    "clippy",
    "--workspace",
    "--all-targets",
    "--features",
    "retail-dats,retail-corpus",
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

/// `cargo doc` of the client runtime alone, with its own features: the workspace's docs turn on
/// its test support, which the client is shipped without.
const RUNTIME_DOC_ARGS: &[&str] = &[
    "doc",
    "-p",
    "dereth-client-runtime",
    "--no-deps",
    "--document-private-items",
];

/// What the documentation gate adds to [`DOC_FLAGS`]: rustdoc reports everything it would and
/// writes no pages. The pages are the cost: a whole-workspace build writes about 80,000 files, and
/// each crate rewrites the shared search index under a lock, which took six times as long as the
/// check itself.
const DOC_CHECK_ONLY: &str = "--check -Zunstable-options";

/// While set, this program stands in for rustdoc, whose path is the value ([`doc_gate`]).
const RUSTDOC_STAND_IN: &str = "XTASK_RUSTDOC";

/// Run the documentation gate: `args` ([`DOC_ARGS`], or one crate's) with [`DOC_FLAGS`] as
/// `RUSTDOCFLAGS`, checking without writing pages.
///
/// That flag is unstable, which a stable rustdoc allows only under `RUSTC_BOOTSTRAP`. Setting that
/// for the whole of `cargo doc` would reach the build scripts of the dependencies, some of which
/// read it to turn on nightly features, so this program stands in for rustdoc and sets it for
/// rustdoc alone.
fn doc_gate(ws: &std::path::Path, args: &[&str]) -> bool {
    println!(
        "$ RUSTDOCFLAGS=\"{DOC_CHECK_ONLY} {DOC_FLAGS}\" cargo {}",
        args.join(" ")
    );
    let Ok(this_program) = std::env::current_exe() else {
        eprintln!("the documentation gate could not find its own program");
        return false;
    };
    let rustdoc = std::env::var_os("RUSTDOC").unwrap_or_else(|| "rustdoc".into());
    std::process::Command::new("cargo")
        .args(args)
        .env("RUSTDOCFLAGS", format!("{DOC_CHECK_ONLY} {DOC_FLAGS}"))
        .env("RUSTDOC", this_program)
        .env(RUSTDOC_STAND_IN, rustdoc)
        .current_dir(ws)
        .status()
        .is_ok_and(|s| s.success())
}

/// Stands in for `rustdoc`: runs it with this program's arguments and allows its unstable flags.
fn rustdoc_stand_in(rustdoc: &std::ffi::OsStr) -> i32 {
    std::process::Command::new(rustdoc)
        .args(std::env::args_os().skip(1))
        .env("RUSTC_BOOTSTRAP", "1")
        .status()
        .ok()
        .and_then(|s| s.code())
        .unwrap_or(1)
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

    /// **Clippy lints the tests that read the retail dats too**: the workspace's clippy, which
    /// `xtask clippy` and tier 0 run, turns `retail-dats` on, so `--all-targets` takes in every
    /// crate's `dat` and `gpu` binaries.
    #[test]
    fn clippy_turns_retail_dats_on() {
        let features = CLIPPY_ARGS.iter().position(|a| *a == "--features");
        assert!(features
            .and_then(|i| CLIPPY_ARGS.get(i + 1))
            .is_some_and(|f| f.split(',').any(|f| f == "retail-dats")));
        assert!(CLIPPY_ARGS.contains(&"--all-targets"));
    }

    /// Tier 0's cargo rows, each as its label and its command.
    fn tier0_rows(game: Game) -> Vec<(String, String)> {
        tier0_steps(game, Profile::TestRelease)
            .into_iter()
            .map(|s| match s {
                Tier0Step::Cargo(label, argv) => (label, argv.join(" ")),
                Tier0Step::ServerUnits => ("server".to_owned(), "the unit tier".to_owned()),
                Tier0Step::Clippy(label, argv) => (format!("clippy: {label}"), argv.join(" ")),
                Tier0Step::Doc(label, argv) => (format!("doc: {label}"), argv.join(" ")),
            })
            .collect()
    }

    fn parts_of(args: &[&str]) -> Result<Vec<Tier0Part>, String> {
        Tier0Part::from_args(&args.iter().map(|a| (*a).to_owned()).collect::<Vec<_>>())
    }

    /// **`--only` selects tier 0's parts**, and with none named every part runs, so the command
    /// a contributor types still runs what public CI runs across all its jobs.
    #[test]
    fn only_selects_tier0_parts_and_no_flag_means_all_of_them() {
        assert_eq!(parts_of(&["tier0"]).unwrap(), Tier0Part::ALL);
        assert_eq!(
            parts_of(&["tier0", "--only", "build,clippy"]).unwrap(),
            [Tier0Part::Build, Tier0Part::Clippy]
        );
        assert_eq!(parts_of(&["--only=docs"]).unwrap(), [Tier0Part::Docs]);
        assert!(parts_of(&["tier0", "--only", "bild"]).is_err());
        assert!(parts_of(&["tier0", "--only"]).is_err());
    }

    /// **Every cargo row of tier 0 belongs to the part that names it**: the steps of public CI's
    /// jobs together run every row, whichever game.
    #[test]
    fn every_tier0_row_has_the_part_its_command_names() {
        for game in [Game::All, Game::Dereth, Game::Auberean] {
            for planned in tier0_steps(game, Profile::TestRelease) {
                let expected = match &planned {
                    Tier0Step::Cargo(_, argv) => match argv[0] {
                        "build" => Tier0Part::Build,
                        "test" => Tier0Part::Test,
                        other => panic!("a cargo row runs {other}"),
                    },
                    Tier0Step::ServerUnits => Tier0Part::Test,
                    Tier0Step::Clippy(..) => Tier0Part::Clippy,
                    Tier0Step::Doc(..) => Tier0Part::Docs,
                };
                assert_eq!(planned.part(), expected);
            }
        }
    }

    /// **A row says how long it took**, beside what it already says.
    #[test]
    fn a_tier0_rows_time_is_added_to_its_detail() {
        let mut rows = [
            Report::new("a", Outcome::Pass, ""),
            Report::new("b", Outcome::Pass, ""),
        ];
        say_time_taken(&mut rows, std::time::Duration::from_secs(125));
        assert_eq!(rows[0].detail, "took 2m 05s");
        assert_eq!(rows[1].detail, "");
        let mut rows = [Report::new("a", Outcome::Pass, "17.4s")];
        say_time_taken(&mut rows, std::time::Duration::from_secs(60));
        assert_eq!(rows[0].detail, "17.4s, took 1m 00s");
        say_time_taken(&mut [], std::time::Duration::ZERO);
    }

    /// **Tier 0 over both games runs exactly these commands**, under these labels, and its tests run
    /// on past a failure, so one run reports them all: the filter changes nothing unless given.
    #[test]
    fn tier0_over_both_games_runs_the_commands_it_always_ran() {
        let rows = tier0_rows(Game::All);
        let expected = [
            (
                "cargo build --workspace",
                "build --workspace --profile test-release",
            ),
            (
                "cargo test (client and shared crates)",
                "test --workspace --exclude empyrean-* --no-fail-fast --profile test-release",
            ),
            (
                "cargo test -p dereth-render (the device as shipped)",
                "test -p dereth-render --no-fail-fast --profile test-release",
            ),
            ("server", "the unit tier"),
            (
                "clippy: cargo clippy --workspace --all-targets --features retail-dats,retail-corpus",
                "clippy --workspace --all-targets --features retail-dats,retail-corpus -- -D warnings",
            ),
            (
                "doc: xtask doc (rustdoc warnings denied)",
                "doc --workspace --no-deps --document-private-items",
            ),
            (
                "doc: xtask doc -p dereth-client-runtime (the runtime as shipped)",
                "doc -p dereth-client-runtime --no-deps --document-private-items",
            ),
        ]
        .map(|(l, c)| (l.to_owned(), c.to_owned()));
        assert_eq!(rows, expected);
        assert_eq!(clippy_args(Game::All), CLIPPY_ARGS);
        let doc = tier0_steps(Game::All, Profile::TestRelease)
            .into_iter()
            .find_map(|s| match s {
                Tier0Step::Doc(_, argv) => Some(argv),
                _ => None,
            });
        assert_eq!(
            doc.as_deref(),
            Some(DOC_ARGS),
            "the `doc` rule's own command"
        );
    }

    /// **Each game's tier 0 selects that game's crates in every cargo row and keeps only its own
    /// rows**: the first game's leaves out the second's crates and keeps the server, the device
    /// and the runtime as shipped; the second's names its crates alone and turns on the feature
    /// only its crates have.
    #[test]
    fn each_games_tier0_selects_its_crates_in_every_row() {
        let ac2 = tier0_rows(Game::Auberean);
        let ac2_crates = "-p auberean-* -p yalain -p yalain-* -p athenaeum-*";
        assert_eq!(
            ac2.iter().map(|(_, c)| c.as_str()).collect::<Vec<_>>(),
            [
                format!("build {ac2_crates} --profile test-release"),
                format!("test {ac2_crates} --no-fail-fast --profile test-release"),
                format!(
                    "clippy {ac2_crates} --all-targets --features retail-dats,retail-corpus -- -D warnings"
                ),
                format!("doc {ac2_crates} --no-deps --document-private-items"),
            ]
        );
        for (label, command) in &ac2 {
            assert!(
                !command.contains("dereth-") && !command.contains("empyrean-"),
                "{label}: {command}"
            );
        }

        let ac1 = tier0_rows(Game::Dereth);
        let without =
            "--exclude auberean-* --exclude yalain --exclude yalain-* --exclude athenaeum-*";
        assert_eq!(
            ac1.len(),
            tier0_rows(Game::All).len(),
            "the same rows as both games'"
        );
        for (label, command) in &ac1 {
            let whole = command.contains("--workspace");
            assert_eq!(
                whole,
                command.contains(without),
                "{label}: a workspace command leaves the second game out: {command}"
            );
            assert!(!command.contains("auberean-* -p"), "{label}: {command}");
        }
        assert!(ac1.iter().any(|(l, _)| l == "server"));
        assert!(ac1
            .iter()
            .any(|(_, c)| c.ends_with("--features retail-dats -- -D warnings")));
        assert!(
            !ac1.iter().any(|(_, c)| c.contains("retail-corpus")),
            "no crate of the first game has `retail-corpus`, and cargo refuses a feature none has"
        );
    }

    /// **`ci` takes a tier and a game; `ci auberean` is tier 1 for the second game**, and a game
    /// beside it that says otherwise is refused rather than half-obeyed.
    #[test]
    fn ci_takes_a_tier_and_a_game_and_auberean_is_tier1_for_the_second_game() {
        for (words, want) in [
            (&["tier0"][..], (CiTier::Zero, Game::All)),
            (&["no-data"][..], (CiTier::NoData, Game::All)),
            (&["tier1"], (CiTier::One, Game::All)),
            (&["tier2", "--serial"], (CiTier::Two, Game::All)),
            (
                &["tier0", "--game", "auberean"],
                (CiTier::Zero, Game::Auberean),
            ),
            (
                &["tier1", "--serial", "--game", "dereth"],
                (CiTier::One, Game::Dereth),
            ),
            (
                &["tier2", "--game=auberean", "--debug"],
                (CiTier::Two, Game::Auberean),
            ),
            (&["auberean"], (CiTier::One, Game::Auberean)),
            (
                &["auberean", "--game", "auberean"],
                (CiTier::One, Game::Auberean),
            ),
        ] {
            assert_eq!(ci_request(&argv(words)), Ok(want), "{words:?}");
        }
        for bad in [
            &[][..],
            &["tier3"],
            &["--game", "dereth"],
            &["auberean", "--game", "dereth"],
            &["auberean", "--game", "all"],
            &["tier1", "--game", "both"],
        ] {
            assert!(ci_request(&argv(bad)).is_err(), "{bad:?}");
        }
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
