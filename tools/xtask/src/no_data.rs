//! `cargo xtask ci no-data`: the workspace's tests as public CI runs them, on the machine you are
//! at, whatever data it holds.
//!
//! A machine with the game's data passes tests that a machine without it fails, so a test that
//! reaches for data it did not ask for is only found by public CI, late, and one failure at a time.
//! This runs tier 0's test commands with every variable that names data emptied or pointed at
//! nothing, the per-user folders (where the world database's download is kept) pointed at a new
//! empty one, and `--no-fail-fast`, then lists everything that failed.
//!
//! A cargo `[env]` entry above the workspace cannot be removed, only outvoted by a value in the
//! environment, so the variables are set rather than unset: each to what its readers take as not
//! set, or, where they would then look in the checkout, to a path that holds nothing.
//!
//! The tests are built first, with the per-user folders where they are, so a compiler wrapper such
//! as sccache keeps finding its own cache; then they run with the folders moved, and no wrapper,
//! since nothing is left to compile.

use std::collections::BTreeSet;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;

use crate::game::Game;
use crate::util::{print_table, target_dir, workspace_root, Outcome, Profile, Report};

/// The variables naming data whose readers take an empty value as not set.
const EMPTIED: &[&str] = &[
    "AUBEREAN_TEST_CAPTURE_INDEX",
    "AUBEREAN_TEST_DAT_DIR",
    "DERETH_CLASSIC_REFERENCE",
    "DERETH_CLASSIC_REFERENCE_PREVIEW",
    "DERETH_TEST_DAT_CAPTURES_DIR",
    "DERETH_TEST_DAT_DIR",
    "DERETH_TEST_DXC_DIR",
    "DERETH_TEST_PRETOD_DAT_DIR",
];

/// The variables naming data whose readers, with no value, look in the checkout or in the main
/// checkout of a linked one: these name a path that holds nothing.
const POINTED_AT_NOTHING: &[&str] = &[
    "DERETH_CLASSIC_PORTAL",
    "EMPYREAN_INFILTRATION_SQL",
    "EMPYREAN_TEST_INFILTRATION_PACK",
    "EMPYREAN_TEST_LAUNCH_FIXTURE",
    "EMPYREAN_TEST_WORLD_PACK",
    "EMPYREAN_WORLD_SQL",
    "YALAIN_TEST_WORLD_DB",
];

/// The prefixes of the variables only tests read.
const TEST_ONLY_PREFIXES: &[&str] = &[
    "ATHENAEUM_TEST_",
    "AUBEREAN_TEST_",
    "DERETH_TEST_",
    "EMPYREAN_TEST_",
    "LYCEUM_TEST_",
    "YALAIN_TEST_",
];

/// The per-user folders a program finds its caches and settings through.
const PER_USER: &[&str] = &[
    "APPDATA",
    "HOME",
    "LOCALAPPDATA",
    "USERPROFILE",
    "XDG_CACHE_HOME",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "XDG_STATE_HOME",
];

/// The names of the `[env]` entries in a cargo configuration file's text.
fn env_keys(config: &str) -> Vec<String> {
    let Ok(table) = config.parse::<toml::Table>() else {
        return Vec::new();
    };
    table
        .get("env")
        .and_then(toml::Value::as_table)
        .map(|env| env.keys().cloned().collect())
        .unwrap_or_default()
}

/// The `[env]` entries of every cargo configuration file above `ws` (the workspace's own sets
/// nothing that reaches data, and a parent's is where a checkout's data is named) and in the cargo
/// home.
fn inherited_env_keys(ws: &Path, cargo_home: &Path) -> BTreeSet<String> {
    let mut places: Vec<PathBuf> = ws
        .ancestors()
        .skip(1)
        .map(|dir| dir.join(".cargo"))
        .collect();
    places.push(cargo_home.to_path_buf());
    places
        .iter()
        .flat_map(|dir| [dir.join("config.toml"), dir.join("config")])
        .filter_map(|file| std::fs::read_to_string(file).ok())
        .flat_map(|text| env_keys(&text))
        .collect()
}

/// A variable's value, or the folder a tool defaults to under the user's own, before the per-user
/// folders are moved: cargo and rustup must keep finding what is installed.
fn tool_home(var: &str, default_name: &str) -> PathBuf {
    if let Some(v) = std::env::var_os(var).filter(|v| !v.is_empty()) {
        return PathBuf::from(v);
    }
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .unwrap_or_default();
    home.join(default_name)
}

/// What a variable is set to, or that it is taken away.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Setting {
    Value(String),
    Removed,
}

/// What takes the data away, while the tests are built and while they run: each variable of the
/// process that only tests read is taken away; then the named ones and whatever a parent
/// configuration sets (`inherited`) are emptied, or pointed at `nothing`.
fn no_data_settings(
    process_vars: &[String],
    inherited: &BTreeSet<String>,
    nothing: &str,
) -> Vec<(String, Setting)> {
    let mut settings: Vec<(String, Setting)> = Vec::new();
    let mut set = |name: &str, setting: Setting| {
        settings.retain(|(n, _)| n != name);
        settings.push((name.to_owned(), setting));
    };
    for var in process_vars {
        if TEST_ONLY_PREFIXES.iter().any(|p| var.starts_with(p)) {
            set(var, Setting::Removed);
        }
    }
    for var in inherited {
        set(var, Setting::Value(String::new()));
    }
    for var in EMPTIED {
        set(var, Setting::Value(String::new()));
    }
    for var in POINTED_AT_NOTHING {
        set(var, Setting::Value(nothing.to_owned()));
    }
    settings
}

/// What else holds while the tests run: the per-user folders go to `home`, with cargo's and
/// rustup's own homes kept where they are, and cargo uses no compiler wrapper, which would look for
/// its cache in those folders.
fn running_settings(home: &str, cargo_home: &str, rustup_home: &str) -> Vec<(String, Setting)> {
    let mut settings: Vec<(String, Setting)> = PER_USER
        .iter()
        .map(|var| ((*var).to_owned(), Setting::Value(home.to_owned())))
        .collect();
    settings.push((
        "CARGO_HOME".to_owned(),
        Setting::Value(cargo_home.to_owned()),
    ));
    settings.push((
        "RUSTUP_HOME".to_owned(),
        Setting::Value(rustup_home.to_owned()),
    ));
    settings.push(("RUSTC_WRAPPER".to_owned(), Setting::Value(String::new())));
    settings
}

/// What a run of `cargo test` said failed.
#[derive(Debug, Default, PartialEq, Eq)]
struct Failures {
    /// `test name ... FAILED`, from every binary.
    tests: BTreeSet<String>,
    /// The targets cargo names as failed, a crash with no failed test among them.
    targets: BTreeSet<String>,
    /// A crate that did not compile.
    builds: BTreeSet<String>,
}

impl Failures {
    fn read(&mut self, line: &str) {
        let line = line.trim_end();
        if let Some(name) = line
            .strip_prefix("test ")
            .and_then(|rest| rest.strip_suffix(" ... FAILED"))
        {
            self.tests.insert(name.to_owned());
        } else if let Some(rest) = line.strip_prefix("error: test failed, to rerun pass `") {
            if let Some(target) = rest.split('`').next() {
                self.targets.insert(target.to_owned());
            }
        } else if let Some(target) = line
            .trim_start()
            .strip_prefix('`')
            .and_then(|rest| rest.strip_suffix('`'))
        {
            if line.starts_with("    `") {
                self.targets.insert(target.to_owned());
            }
        } else if line.starts_with("error: could not compile `") {
            if let Some(name) = line.split('`').nth(1) {
                self.builds.insert(name.to_owned());
            }
        }
    }

    fn is_empty(&self) -> bool {
        self.tests.is_empty() && self.targets.is_empty() && self.builds.is_empty()
    }

    /// One line for a table's row.
    fn summary(&self) -> String {
        if !self.builds.is_empty() {
            return format!("did not build: {}", join(&self.builds));
        }
        format!(
            "{} test(s) in {} target(s)",
            self.tests.len(),
            self.targets.len()
        )
    }
}

fn join(names: &BTreeSet<String>) -> String {
    names.iter().cloned().collect::<Vec<_>>().join(", ")
}

/// Lines of `from`, sent to `lines` as they come.
fn forward(from: impl Read + Send + 'static, lines: mpsc::Sender<(bool, String)>, is_err: bool) {
    std::thread::spawn(move || {
        for line in BufReader::new(from).lines() {
            let Ok(line) = line else { break };
            if lines.send((is_err, line)).is_err() {
                break;
            }
        }
    });
}

/// Runs `cargo <argv>` with `settings`, passing its output on as it comes; whether it succeeded and
/// what it said failed.
fn run_tests(ws: &Path, argv: &[&str], settings: &[(String, Setting)]) -> (bool, Failures) {
    println!("$ cargo {}", argv.join(" "));
    let mut command = Command::new("cargo");
    command
        .args(argv)
        .current_dir(ws)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (name, setting) in settings {
        match setting {
            Setting::Value(v) => command.env(name, v),
            Setting::Removed => command.env_remove(name),
        };
    }
    let mut failures = Failures::default();
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) => {
            eprintln!("failed to launch cargo: {e}");
            return (false, failures);
        }
    };
    let (sender, lines) = mpsc::channel();
    if let Some(out) = child.stdout.take() {
        forward(out, sender.clone(), false);
    }
    if let Some(err) = child.stderr.take() {
        forward(err, sender.clone(), true);
    }
    drop(sender);
    for (is_err, line) in lines {
        if is_err {
            eprintln!("{line}");
        } else {
            println!("{line}");
        }
        failures.read(&line);
    }
    let ok = child.wait().is_ok_and(|s| s.success());
    (ok, failures)
}

/// `cargo xtask ci no-data`: `commands` run in the no-data environment, every failure listed at the
/// end. Exits non-zero when any failed.
pub fn no_data(commands: Vec<(String, Vec<&'static str>)>) -> i32 {
    let ws = workspace_root();
    let scratch = target_dir().join("no-data");
    if scratch.exists() {
        if let Err(e) = std::fs::remove_dir_all(&scratch) {
            eprintln!("no-data: could not empty {}: {e}", scratch.display());
            return 1;
        }
    }
    let home = scratch.join("home");
    if let Err(e) = std::fs::create_dir_all(&home) {
        eprintln!("no-data: could not make {}: {e}", home.display());
        return 1;
    }
    let cargo_home = tool_home("CARGO_HOME", ".cargo");
    let rustup_home = tool_home("RUSTUP_HOME", ".rustup");
    let process_vars: Vec<String> = std::env::vars_os()
        .filter_map(|(name, _)| name.into_string().ok())
        .collect();
    let building = no_data_settings(
        &process_vars,
        &inherited_env_keys(&ws, &cargo_home),
        &scratch.join("nothing").to_string_lossy(),
    );
    let mut running = building.clone();
    running.extend(running_settings(
        &home.to_string_lossy(),
        &cargo_home.to_string_lossy(),
        &rustup_home.to_string_lossy(),
    ));

    let mut reports = Vec::new();
    let mut all = Failures::default();
    for (label, argv) in &commands {
        let build = building_args(argv);
        let (built, mut failures) = run_tests(&ws, &build, &building);
        let ok = built && {
            let (ok, ran) = run_tests(&ws, argv, &running);
            failures = ran;
            ok
        };
        let (outcome, detail) = if ok {
            (Outcome::Pass, String::new())
        } else if failures.is_empty() {
            (Outcome::Fail, "see the log above".to_owned())
        } else {
            (Outcome::Fail, failures.summary())
        };
        reports.push(Report::new(label.clone(), outcome, detail));
        all.tests.extend(failures.tests);
        all.targets.extend(failures.targets);
        all.builds.extend(failures.builds);
    }
    let code = print_table("No data: the tests as public CI runs them", &reports);
    if !all.is_empty() {
        println!("\nEverything that failed:");
        for name in &all.builds {
            println!("  did not build: {name}");
        }
        for target in &all.targets {
            println!("  target: {target}");
        }
        for test in &all.tests {
            println!("  test: {test}");
        }
    }
    code
}

/// The command that only builds what `argv` tests: cargo refuses `--no-fail-fast` beside `--no-run`.
fn building_args<'a>(argv: &[&'a str]) -> Vec<&'a str> {
    let mut build: Vec<&str> = argv
        .iter()
        .copied()
        .filter(|a| *a != "--no-fail-fast")
        .collect();
    build.push("--no-run");
    build
}

/// Tier 0's test commands for `game`.
pub fn commands(game: Game, profile: Profile) -> Vec<(String, Vec<&'static str>)> {
    crate::tier0_test_commands(game, profile)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setting<'a>(settings: &'a [(String, Setting)], name: &str) -> Option<&'a Setting> {
        settings.iter().find(|(n, _)| n == name).map(|(_, s)| s)
    }

    fn value(text: &str) -> Setting {
        Setting::Value(text.to_owned())
    }

    /// **No variable naming data survives**: the named ones go empty or point at nothing, a
    /// parent configuration's entries go empty whatever they are called, and a test-only variable
    /// the machine has set is taken away.
    #[test]
    fn no_variable_that_names_data_reaches_the_tests() {
        let inherited = BTreeSet::from(["A_PARENT_ENTRY".to_owned()]);
        let process = ["DERETH_TEST_CAPTURES".to_owned(), "PATH".to_owned()];
        let s = no_data_settings(&process, &inherited, "N");
        assert_eq!(setting(&s, "DERETH_TEST_DAT_DIR"), Some(&value("")));
        assert_eq!(setting(&s, "AUBEREAN_TEST_DAT_DIR"), Some(&value("")));
        assert_eq!(setting(&s, "A_PARENT_ENTRY"), Some(&value("")));
        assert_eq!(setting(&s, "EMPYREAN_TEST_WORLD_PACK"), Some(&value("N")));
        assert_eq!(setting(&s, "EMPYREAN_WORLD_SQL"), Some(&value("N")));
        assert_eq!(setting(&s, "DERETH_TEST_CAPTURES"), Some(&Setting::Removed));
        assert_eq!(setting(&s, "PATH"), None, "the rest of the machine stays");
    }

    /// **A parent's entry for a variable that must point at nothing still points at nothing**:
    /// empty, its reader would look in the checkout.
    #[test]
    fn a_parents_entry_does_not_empty_a_variable_that_must_point_at_nothing() {
        let inherited = BTreeSet::from(["EMPYREAN_TEST_WORLD_PACK".to_owned()]);
        let s = no_data_settings(&[], &inherited, "N");
        assert_eq!(setting(&s, "EMPYREAN_TEST_WORLD_PACK"), Some(&value("N")));
    }

    /// **While the tests run the per-user folders move and the tools stay findable**, so the cache
    /// is empty and cargo still runs, with no wrapper looking for its cache in those folders.
    #[test]
    fn the_per_user_folders_move_and_cargo_and_rustup_stay() {
        let s = running_settings("H", "C", "R");
        for var in PER_USER {
            assert_eq!(setting(&s, var), Some(&value("H")), "{var}");
        }
        assert_eq!(setting(&s, "CARGO_HOME"), Some(&value("C")));
        assert_eq!(setting(&s, "RUSTUP_HOME"), Some(&value("R")));
        assert_eq!(setting(&s, "RUSTC_WRAPPER"), Some(&value("")));
    }

    /// **The entries of a configuration file are read by name**, and a file that does not parse
    /// gives none.
    #[test]
    fn a_configurations_env_entries_are_read_by_name() {
        let text =
            "[alias]\nx = \"y\"\n[env]\nONE = \"a\"\nTWO = { value = \"b\", relative = true }\n";
        assert_eq!(env_keys(text), ["ONE", "TWO"]);
        assert_eq!(env_keys("[alias]\nx = \"y\"\n"), Vec::<String>::new());
        assert_eq!(env_keys("not toml ["), Vec::<String>::new());
    }

    /// **What cargo test says failed is all read**: failed tests from any binary, a target that
    /// crashed with no test to name, cargo's closing list, and a crate that did not compile.
    #[test]
    fn every_kind_of_failure_is_read_from_cargos_output() {
        let mut f = Failures::default();
        for line in [
            "test a::passes ... ok",
            "test a::fails ... FAILED",
            "test b::also_fails ... FAILED\r",
            "error: test failed, to rerun pass `-p some-crate --test dat`",
            "error: 2 targets failed:",
            "    `-p some-crate --test dat`",
            "    `-p other-crate --lib`",
            "Caused by:",
            "   Compiling something `odd`",
        ] {
            f.read(line);
        }
        assert_eq!(
            f.tests.iter().map(String::as_str).collect::<Vec<_>>(),
            ["a::fails", "b::also_fails"]
        );
        assert_eq!(
            f.targets.iter().map(String::as_str).collect::<Vec<_>>(),
            ["-p other-crate --lib", "-p some-crate --test dat"]
        );
        assert_eq!(f.summary(), "2 test(s) in 2 target(s)");

        let mut f = Failures::default();
        assert!(f.is_empty());
        f.read("error: could not compile `some-crate` (lib test) due to 1 previous error");
        assert_eq!(f.summary(), "did not build: some-crate");
    }

    /// **Tier 0's test commands are what this runs**, so it cannot drift from what public CI does.
    #[test]
    fn it_runs_tier0s_test_commands() {
        let rows = commands(Game::All, Profile::TestRelease);
        let rendered: Vec<String> = rows.iter().map(|(_, argv)| argv.join(" ")).collect();
        assert_eq!(
            rendered,
            [
                "test --workspace --exclude empyrean-* --no-fail-fast --profile test-release",
                "test -p dereth-render --no-fail-fast --profile test-release",
                "test --profile test-release -p empyrean-* --no-fail-fast",
            ]
        );
        assert_eq!(
            building_args(&rows[1].1).join(" "),
            "test -p dereth-render --profile test-release --no-run"
        );
    }
}
