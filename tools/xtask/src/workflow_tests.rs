//! What the CI workflows must keep doing, checked by reading them, because none of it can be tried
//! until a workflow reaches its forge.

use crate::util::workspace_root;

fn read(path: &str) -> String {
    std::fs::read_to_string(workspace_root().join(path)).expect(path)
}

/// The text of the job called `name` in a workflow, up to the next job.
fn job<'a>(workflow: &'a str, name: &str) -> &'a str {
    let jobs = workflow.split_once("\njobs:\n").expect("a jobs table").1;
    let start = jobs
        .find(&format!("  {name}:\n"))
        .unwrap_or_else(|| panic!("no job {name}"));
    let body = &jobs[start + name.len() + 4..];
    let end = body
        .match_indices("\n  ")
        .find(|(at, _)| {
            let rest = &body[at + 3..];
            let line = rest.lines().next().unwrap_or_default();
            !rest.starts_with(' ') && !line.starts_with('#') && line.ends_with(':')
        })
        .map_or(body.len(), |(at, _)| at);
    &body[..end]
}

/// The tier 0 parts a job's steps name with `--only`, in order.
fn parts(job: &str) -> Vec<&str> {
    job.lines()
        .filter_map(|line| line.split_once("cargo xtask ci tier0 --only ").map(|l| l.1))
        .flat_map(|list| list.split(','))
        .map(str::trim)
        .collect()
}

/// **Public CI runs every part of tier 0**: the build, the tests and clippy on every system, and the
/// rules and the docs once. Dropping a part from a job would pass silently otherwise.
#[test]
fn public_ci_runs_the_build_tests_and_clippy_everywhere_and_the_rules_and_docs_once() {
    let ci = read(".github/workflows/ci.yml");
    let tier0 = job(&ci, "tier0");
    assert_eq!(parts(tier0), ["build", "test", "clippy"]);
    assert!(tier0.contains("os: [windows-latest, ubuntu-latest, macos-latest]"));
    let rules = job(&ci, "rules");
    assert_eq!(parts(rules), ["rules", "docs"]);
    assert!(rules.contains("runs-on: ubuntu-latest"));
    assert!(!rules.contains("matrix"), "once, on one system");
}

/// **A step that fails does not hide the next one's failures**: every part after the first runs
/// whatever came before it, as tier 0's rows do.
#[test]
fn a_failed_part_of_public_ci_does_not_stop_the_parts_after_it() {
    let ci = read(".github/workflows/ci.yml");
    for name in ["tier0", "rules"] {
        let job = job(&ci, name);
        let lines: Vec<&str> = job.lines().collect();
        let mut seen = 0;
        for (at, line) in lines.iter().enumerate() {
            if !line.contains("cargo xtask ci tier0 --only ") {
                continue;
            }
            seen += 1;
            if seen > 1 {
                assert!(
                    lines[at.saturating_sub(4)..at]
                        .iter()
                        .any(|l| l.contains("if: ${{ !cancelled() }}")),
                    "{name}: {line}"
                );
            }
        }
        assert!(seen >= 2, "{name}");
    }
}

/// **sccache can only make a build faster, never fail one**: its install may fail, and the compiler
/// is only wrapped by the step that saw its server start, never in a job's `env`.
#[test]
fn a_failure_of_sccache_or_its_cache_service_leaves_the_build_running_without_it() {
    let ci = read(".github/workflows/ci.yml");
    for name in ["tier0", "rules"] {
        let job = job(&ci, name);
        let install = job.find("name: Install sccache").expect("the install");
        let use_it = job.find("name: Use sccache if it starts").expect("the use");
        let build = job.find("cargo xtask ci tier0 --only").expect("the build");
        assert!(install < use_it && use_it < build, "{name}");
        assert!(job[install..use_it].contains("continue-on-error: true"));
        let use_step = &job[use_it..build];
        assert!(use_step.contains("if: steps.sccache.outcome == 'success'"));
        assert!(use_step.contains("continue-on-error: true"));
        assert!(use_step.contains("sccache --start-server"));
        let started = use_step.find("$LASTEXITCODE -eq 0").expect("the check");
        let wrapped = use_step.find("RUSTC_WRAPPER=sccache").expect("the wrapper");
        assert!(started < wrapped, "{name}");
        assert_eq!(
            job.matches("RUSTC_WRAPPER=").count(),
            1,
            "{name}: only the step that saw it start"
        );
        assert!(
            job.contains("SCCACHE_IGNORE_SERVER_IO_ERROR: '1'"),
            "{name}"
        );
        assert!(job.contains("CARGO_INCREMENTAL: '0'"), "{name}");
    }
}

/// **The forge's own machines run all of it**: every part of tier 0 but the tests, which run as
/// `ci no-data`; the docs on every system, since public CI's cover only Linux; on `landed` alone.
#[test]
fn the_own_machines_run_every_part_on_landed_with_docs_on_every_system() {
    let landed = read(".forgejo/workflows/landed.yml");
    assert!(landed.contains("branches: [landed]"));
    assert!(landed.contains("os: [windows, macos, linux]"));
    assert!(landed.contains("runs-on: ${{ matrix.os }}"));
    assert_eq!(parts(&landed), ["build", "clippy", "docs", "rules"]);
    assert!(landed.contains("cargo xtask ci no-data"));
    assert!(
        !landed.contains("uses:"),
        "bash, git and cargo alone: no node on these machines"
    );
    let docs = landed.split_once("- name: Docs").expect("the docs").1;
    assert_eq!(
        docs.lines().nth(1).map(str::trim),
        Some("if: ${{ !cancelled() }}"),
        "no condition on the system"
    );
}

/// **No label a hosted runner has**: this forge also reads the GitHub workflows of a branch with no
/// `.forgejo` folder, and a runner answering to `ubuntu-latest` would run them, the release
/// workflows among them.
#[test]
fn the_own_machines_labels_are_none_of_a_hosted_runners() {
    for file in [
        ".forgejo/workflows/landed.yml",
        ".forgejo/workflows/landed-tier1.yml",
    ] {
        let text = read(file);
        assert!(!text.contains("-latest"), "{file}");
    }
}

/// **The tier that reads the game's data sends nothing of it to the log**: its output goes to a
/// file on the machine, and only its closing table and the names of failed tests are printed.
#[test]
fn tier_one_on_the_forge_keeps_its_output_on_the_machine() {
    let tier1 = read(".forgejo/workflows/landed-tier1.yml");
    assert_eq!(
        tier1.matches("cargo xtask ci tier1").count(),
        2,
        "the run and the description"
    );
    assert!(tier1.contains("cargo xtask ci tier1 > \"$log\" 2>&1 || status=$?"));
    let printed: Vec<&str> = tier1
        .lines()
        .map(str::trim)
        .filter(|l| l.contains("\"$log\""))
        .collect();
    assert_eq!(
        printed,
        [
            "cargo xtask ci tier1 > \"$log\" 2>&1 || status=$?",
            "sed -n '/^CI tier 1/,$p' \"$log\"",
            r#"grep -E '^test .+ \.\.\. FAILED$' "$log" || true"#,
        ]
    );
    assert!(tier1.contains("runs-on: retail-data"));
    assert!(tier1.contains("refs/heads/landed"));
    assert!(tier1.contains("schedule:") && tier1.contains("workflow_dispatch:"));
    assert!(!tier1.contains("push:"), "a batch, not a run per push");
}

/// **The table that goes to the log is the one tier 1 prints**, so the filter above finds it.
#[test]
fn the_filter_finds_tier_ones_table_by_its_title() {
    let main = read("tools/xtask/src/main.rs");
    assert!(main.contains("\"CI tier 1 -- the retail data{}\""));
}

/// **The clean between runs keeps the build, on Windows too**: Git Bash rewrites an argument that
/// starts with a slash into a Windows path, so `-e /target` would keep nothing and every run would
/// start cold.
#[test]
fn the_clean_between_runs_names_the_build_in_a_form_git_bash_leaves_alone() {
    for file in [
        ".forgejo/workflows/landed.yml",
        ".forgejo/workflows/landed-tier1.yml",
    ] {
        assert!(
            read(file).contains("git clean -q -ffdx -e target/\n"),
            "{file}"
        );
    }
}

/// **Dereth's release runs the relay it carries**, on every system, and holds it to Empyrean's
/// version, with its license and notice beside it.
#[test]
fn dereth_s_release_runs_the_relay_at_empyrean_s_version() {
    let workflow = read(".github/workflows/release-dereth.yml");
    let version = job(&workflow, "version");
    assert!(version.contains("relay=$(cargo xtask version empyrean)"));
    assert!(version.contains("relay: ${{ steps.version.outputs.relay }}"));
    let package = job(&workflow, "package");
    assert!(package.contains("RELAY: ${{ needs.version.outputs.relay }}"));
    for at in [
        "relay=\"smoke/dereth-$VERSION/empyrean-relay.exe\"",
        "relay=\"smoke/squashfs-root/usr/bin/empyrean-relay\"",
        "relay=\"smoke/Dereth.app/Contents/MacOS/empyrean-relay\"",
    ] {
        assert!(package.contains(at), "{at}");
    }
    assert!(package.contains("run \"$relay\" --version"));
    assert!(package.contains("grep -qx \"empyrean-relay $RELAY\" version-relay.txt"));
    assert!(package.contains("$relay_notices/LICENSE-empyrean-relay"));
    assert!(package.contains("$relay_notices/NOTICE-empyrean-relay.txt"));
}
