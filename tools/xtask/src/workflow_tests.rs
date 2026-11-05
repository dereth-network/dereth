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
