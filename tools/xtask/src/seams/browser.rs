//! Rule 15: **the shared crates build for a browser.** Every crate under `core/` builds for
//! `wasm32-unknown-unknown` with its default features, both games' core included, but for the
//! crates of [`NOT_IN_BROWSER`], each named with its reason. A new crate under `core/` is built
//! for the browser from the moment it is a workspace member, unless it is added to that list here,
//! in review. An entry that names no crate under `core/` is a failure too, so the list cannot
//! outlive what it excuses.
//!
//! A host without the browser target cannot ask, and says so.

use std::path::{Path, PathBuf};
use std::process::Command;

use super::WorkspacePackage;
use crate::util::{NotRun, Outcome, Report};

/// The folder of the shared crates.
const CORE: &str = "core/";

/// The crates under `core/` that do not build for the browser, each with why.
pub const NOT_IN_BROWSER: &[(&str, &str)] = &[
    (
        "dereth-server-updater",
        "a game server's updater: it fetches releases over HTTPS (rustls on ring, whose C needs a \
         compiler for the target, and getrandom, which has no browser source), unpacks archives \
         and restarts its process",
    ),
    (
        "dereth-plugin-lua",
        "the Luau VM is C++, built for the browser only with zig's clang and the web client's \
         build environment; the web client's own build (CI's web job) builds it there",
    ),
];

/// The crates under `core/` the row builds, sorted: every workspace package whose folder is
/// under `core/`, but for [`NOT_IN_BROWSER`]. The `Err` names each entry of the list that is not a
/// crate under `core/`.
pub fn browser_crates(
    packages: &[WorkspacePackage],
    not_in_browser: &[(&str, &str)],
) -> Result<Vec<String>, Vec<String>> {
    let core: Vec<&WorkspacePackage> = packages
        .iter()
        .filter(|p| p.dir.starts_with(CORE))
        .collect();
    let stale: Vec<String> = not_in_browser
        .iter()
        .filter(|(name, _)| !core.iter().any(|p| p.name == *name))
        .map(|(name, _)| {
            format!("{name} is excused from the browser build but is no crate under core/")
        })
        .collect();
    if !stale.is_empty() {
        return Err(stale);
    }
    let mut out: Vec<String> = core
        .iter()
        .filter(|p| !not_in_browser.iter().any(|(name, _)| *name == p.name))
        .map(|p| p.name.clone())
        .collect();
    out.sort();
    Ok(out)
}

/// Whether this host's toolchain has the browser target's standard library.
fn has_browser_target() -> bool {
    Command::new("rustc")
        .args(["--print", "sysroot"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            PathBuf::from(String::from_utf8_lossy(&o.stdout).trim())
                .join("lib/rustlib/wasm32-unknown-unknown")
        })
        .is_some_and(|p| p.is_dir())
}

/// The row: every crate of [`browser_crates`], built for the browser.
pub fn report(ws: &Path, packages: &Result<Vec<WorkspacePackage>, String>) -> Report {
    const NAME: &str = "seam: core in a browser";
    let crates = match packages {
        Ok(ps) => browser_crates(ps, NOT_IN_BROWSER),
        Err(e) => Err(vec![e.clone()]),
    };
    let crates = match crates {
        Ok(c) if c.len() >= 40 => c,
        Ok(c) => {
            return Report::new(
                NAME,
                Outcome::Fail,
                format!(
                    "found {} crate(s) under core/; expected at least 40",
                    c.len()
                ),
            )
        }
        Err(v) => {
            for line in &v {
                println!("seam: {line}");
            }
            return Report::new(NAME, Outcome::Fail, "see above");
        }
    };
    if !has_browser_target() {
        return Report::new(
            NAME,
            Outcome::NotRun(NotRun::Unhostable),
            "the toolchain has no wasm32-unknown-unknown target (rustup target add it)",
        );
    }
    let mut cmd = Command::new("cargo");
    cmd.current_dir(ws).args([
        "build",
        "--profile",
        "test-release",
        "--target",
        "wasm32-unknown-unknown",
    ]);
    for p in &crates {
        cmd.args(["-p", p]);
    }
    let excused: Vec<&str> = NOT_IN_BROWSER.iter().map(|(n, _)| *n).collect();
    match cmd.output() {
        Ok(o) if o.status.success() => Report::new(
            NAME,
            Outcome::Pass,
            format!(
                "{} core crates build for wasm32-unknown-unknown; not {} (named with their reasons)",
                crates.len(),
                excused.join(", ")
            ),
        ),
        Ok(o) => {
            println!("seam: {}", String::from_utf8_lossy(&o.stderr));
            Report::new(
                NAME,
                Outcome::Fail,
                "a core crate does not build for the browser; see above (a crate that cannot \
                 is named in seams/browser.rs with its reason)",
            )
        }
        Err(e) => Report::new(NAME, Outcome::Fail, format!("cargo build: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pkg(name: &str, dir: &str) -> WorkspacePackage {
        WorkspacePackage {
            name: name.to_owned(),
            dir: dir.to_owned(),
            deps: Vec::new(),
        }
    }

    fn workspace() -> Vec<WorkspacePackage> {
        vec![
            pkg("dereth-dat", "core/dat"),
            pkg("auberean-dat", "core/auberean-dat"),
            pkg("dereth-server-updater", "core/server-updater"),
            pkg("dereth-client", "dereth/client"),
            pkg("empyrean-world", "empyrean/crates/world"),
        ]
    }

    /// **Every crate under `core/` is built for the browser but the excused**, a new one
    /// included without naming it anywhere, and nothing outside `core/`.
    #[test]
    fn every_core_crate_is_built_for_the_browser_but_the_named_exceptions() {
        let excused = [("dereth-server-updater", "a server's own")];
        assert_eq!(
            browser_crates(&workspace(), &excused).expect("the list names core crates"),
            ["auberean-dat", "dereth-dat"]
        );
        let mut grown = workspace();
        grown.push(pkg("dereth-new", "core/new"));
        assert!(browser_crates(&grown, &excused)
            .expect("the list names core crates")
            .contains(&"dereth-new".to_owned()));
        assert_eq!(
            browser_crates(&workspace(), &[]).expect("no list").len(),
            3,
            "without the list, the updater is built too"
        );
    }

    /// **An exception that names no crate under `core/` fails**: a crate that left `core/`, was
    /// renamed or never was there.
    #[test]
    fn an_exception_for_no_core_crate_is_refused() {
        for name in ["dereth-gone", "dereth-client"] {
            let excused = [(name, "a stale reason")];
            let err = browser_crates(&workspace(), &excused).expect_err("stale entry");
            assert!(err[0].contains(name), "{err:?}");
        }
    }

    /// Each exception carries its reason, and none is Asheron's Call 2's core, which builds for
    /// the browser whole (its own seam's rule 3).
    #[test]
    fn each_exception_has_its_reason_and_none_is_the_second_games_core() {
        for (name, why) in NOT_IN_BROWSER {
            assert!(!name.starts_with("auberean-"), "{name}");
            assert!(why.len() > 20, "{name} needs its reason");
        }
    }
}
