//! Asheron's Call 2's tiers: its crates' tests that read its data, kept apart from the first
//! game's gates because they read other data.
//!
//! | tier | what it reads | the test binaries |
//! |---|---|---|
//! | `auberean-dat` | the installs | every crate's `dat` binary but the script machine's: the container, the decoders over every record, the world, bodies that move and animate, our client on our server, the desktop shell against a server double, the interface, a world builder's world over the world database |
//! | `auberean-script` | the installs | the script machine's `dat` binaries: the September 2013 library, its saved states, its code run with every native |
//! | `auberean-corpus` | the installs and the capture index | every `corpus` binary: every recorded message and script call, bodies and looks against the recordings, the recorded logins replayed through the client and against the server |
//! | `auberean-live` | the installs | the server's `live` binary: our client against the server's executable on loopback; the world builder's: its playtest with the server and our client as programs (it builds them first) |
//! | `auberean-network` | the installs and the first game's data files | the test kit's `network` binary: the Dereth Network end to end, the hub, a first-game world and two of this game's worlds as programs, with both games' clients; it builds those programs first |
//! | `auberean-gpu` | the end-of-retail data files and a graphics device | every `gpu` binary: the device's pipelines and frames with every kind of draw |
//!
//! `cargo xtask gate <tier>` runs one. `cargo xtask ci tier1` runs the crates' tests that read
//! nothing, then every tier but `auberean-gpu`, after the first game's rows (`--game auberean` runs
//! these alone, and `ci auberean` is the same); `ci tier2` runs `auberean-gpu`. A tier whose data
//! is not found reports `NO-ORACLE`, naming the variable to set, and the run exits non-zero: a
//! tier that read nothing certified nothing.

use std::path::PathBuf;

use crate::game::Game;
use crate::util::{print_table, run_captured, workspace_root, NotRun, Outcome, Profile, Report};

/// What a tier reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Data {
    /// The end-of-retail data files, which the September 2013 client reads: `AUBEREAN_TEST_DAT_DIR`.
    Installs,
    /// The capture index: `AUBEREAN_TEST_CAPTURE_INDEX`.
    Captures,
    /// The first game's data files, for its client and its world: `DERETH_TEST_DAT_DIR`.
    FirstGame,
}

/// One tier: its name, the area in words, what it reads, the programs its tests start (built
/// first, in the same profile), and its runs, each a crate's test binary with the features that
/// build it.
#[derive(Debug)]
pub struct Tier {
    pub name: &'static str,
    pub title: &'static str,
    pub reads: &'static [Data],
    pub builds: &'static [&'static str],
    pub runs: &'static [(&'static str, &'static str, &'static str)],
}

/// The tiers.
pub const TIERS: &[Tier] = &[
    Tier {
        name: "auberean-dat",
        title: "Asheron's Call 2 data files",
        reads: &[Data::Installs],
        builds: &[],
        runs: &[
            ("auberean-dat", "retail-dats", "dat"),
            ("auberean-assets", "retail-dats", "dat"),
            ("auberean-text", "retail-dats", "dat"),
            ("auberean-world", "retail-dats", "dat"),
            ("auberean-draw", "retail-dats", "dat"),
            ("auberean-physics", "retail-dats", "dat"),
            ("auberean-animation", "retail-dats", "dat"),
            ("yalain-testkit", "retail-dats", "dat"),
            ("auberean-shell", "retail-dats", "dat"),
            ("auberean-ui", "retail-dats", "dat"),
            ("auberean-ui-game", "retail-dats", "dat"),
            ("auberean-interface", "retail-dats", "dat"),
            ("athenaeum-foundry", "retail-dats", "dat"),
            ("athenaeum-editor", "retail-dats", "dat"),
            ("auberean-audio", "retail-dats", "dat"),
        ],
    },
    Tier {
        name: "auberean-script",
        title: "Asheron's Call 2 script machine",
        reads: &[Data::Installs],
        builds: &[],
        runs: &[
            ("auberean-script", "retail-dats", "dat"),
            ("auberean-glue", "retail-dats", "dat"),
        ],
    },
    Tier {
        name: "auberean-corpus",
        title: "Asheron's Call 2 recorded sessions",
        reads: &[Data::Installs, Data::Captures],
        builds: &[],
        runs: &[
            ("auberean-protocol", "retail-corpus", "corpus"),
            ("auberean-script", "retail-corpus", "corpus"),
            ("auberean-glue", "retail-corpus", "corpus"),
            ("auberean-animation", "retail-dats,retail-corpus", "corpus"),
            ("auberean-physics", "retail-dats,retail-corpus", "corpus"),
            ("auberean-client-runtime", "retail-corpus", "corpus"),
            ("auberean-scene", "retail-dats,retail-corpus", "corpus"),
            ("auberean-headless", "retail-corpus", "corpus"),
            ("yalain-testkit", "retail-corpus", "corpus"),
        ],
    },
    Tier {
        name: "auberean-live",
        title: "Asheron's Call 2 on loopback",
        reads: &[Data::Installs],
        builds: &["yalain", "auberean-client"],
        runs: &[
            ("yalain", "live", "live"),
            ("athenaeum-editor", "live", "live"),
        ],
    },
    Tier {
        name: "auberean-network",
        title: "the Dereth Network end to end",
        reads: &[Data::Installs, Data::FirstGame],
        builds: &[
            "dereth-network-hub",
            "empyrean-server",
            "empyrean-import",
            "yalain",
        ],
        runs: &[("yalain-testkit", "network", "network")],
    },
];

/// The tier on a graphics device: `ci tier2`'s, never `ci tier1`'s. A crate that adds a `gpu` test
/// binary lists it here (`tests::every_data_binary_is_in_one_tier_with_the_features_it_requires`
/// fails until it is).
pub const GPU_TIER: Tier = Tier {
    name: "auberean-gpu",
    title: "Asheron's Call 2 on a graphics device",
    reads: &[Data::Installs],
    builds: &[],
    runs: &[("auberean-gpu", "retail-dats", "gpu")],
};

/// Every tier: the data tiers, then the one on a graphics device.
pub fn tiers() -> impl Iterator<Item = &'static Tier> {
    TIERS.iter().chain(std::iter::once(&GPU_TIER))
}

/// The tier called `name`, if one is.
#[must_use]
pub fn find(name: &str) -> Option<&'static Tier> {
    tiers().find(|t| t.name.eq_ignore_ascii_case(name))
}

fn env_path(var: &str) -> Option<PathBuf> {
    std::env::var_os(var)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// Why a set of data files cannot be read, naming its variable, or `None` when it is found.
fn data_files(set: &str, var: &str) -> Option<String> {
    match env_path(var) {
        Some(d) if d.join("portal.dat").is_file() => None,
        Some(d) => Some(format!(
            "the {set} data files were not found: no portal.dat in {} ({var})",
            d.display()
        )),
        None => Some(format!(
            "the {set} data files were not found: {var} is unset"
        )),
    }
}

/// Why a kind of data cannot be read, naming the variable to set, or `None` when it is found.
#[must_use]
pub fn shortfall(data: Data) -> Option<String> {
    match data {
        Data::Installs => data_files("end-of-retail", "AUBEREAN_TEST_DAT_DIR"),
        Data::Captures => match env_path("AUBEREAN_TEST_CAPTURE_INDEX") {
            Some(p) if p.is_file() => None,
            Some(p) => Some(format!(
                "the capture index was not found: no file at {} (AUBEREAN_TEST_CAPTURE_INDEX)",
                p.display()
            )),
            None => Some(
                "the capture index was not found: AUBEREAN_TEST_CAPTURE_INDEX is unset".to_owned(),
            ),
        },
        Data::FirstGame => dereth_dat::testing::shortfall()
            .map(|why| format!("the first game's data files were not found: {why}")),
    }
}

/// The cargo arguments of one run. A `gpu` binary runs its tests one at a time: they share one
/// device.
#[must_use]
pub fn invocation(run: (&str, &str, &str), profile: Profile) -> Vec<String> {
    let (krate, features, binary) = run;
    let mut argv: Vec<String> = vec!["test".into()];
    argv.extend(profile.cargo_args().iter().map(|s| (*s).to_owned()));
    argv.extend(["-p", krate, "--features", features, "--test", binary].map(str::to_owned));
    if binary == "gpu" {
        argv.extend(["--", "--test-threads=1"].map(str::to_owned));
    }
    argv
}

/// One tier's rows: one per run, one `NO-ORACLE` row naming what is missing, or one `NOT-BUILT`
/// row for a tier with no binary yet.
pub fn run_tier(tier: &Tier, profile: Profile) -> Vec<Report> {
    if tier.runs.is_empty() {
        return vec![Report::new(
            format!("{} ({})", tier.name, tier.title),
            Outcome::NotRun(NotRun::NotBuilt),
            "no crate has a test binary for this tier yet",
        )];
    }
    let missing: Vec<String> = tier.reads.iter().filter_map(|d| shortfall(*d)).collect();
    if !missing.is_empty() {
        return vec![Report::new(
            format!("{} ({})", tier.name, tier.title),
            Outcome::NotRun(NotRun::OracleAbsent),
            missing.join("; "),
        )];
    }
    let ws = workspace_root();
    if !tier.builds.is_empty() {
        let mut argv: Vec<&str> = vec!["build"];
        argv.extend_from_slice(profile.cargo_args());
        for p in tier.builds {
            argv.extend(["-p", p]);
        }
        println!("$ cargo {}", argv.join(" "));
        let (ok, out) = run_captured(&ws, "cargo", &argv);
        print!("{out}");
        if !ok {
            return vec![Report::new(
                format!("{} ({})", tier.name, tier.title),
                Outcome::Fail,
                format!(
                    "the programs it starts did not build: {}",
                    tier.builds.join(", ")
                ),
            )];
        }
    }
    tier.runs
        .iter()
        .map(|run| {
            let argv = invocation(*run, profile);
            let args: Vec<&str> = argv.iter().map(String::as_str).collect();
            println!("$ cargo {}", argv.join(" "));
            let (ok, out) = run_captured(&ws, "cargo", &args);
            print!("{out}");
            let n = super::tests_run(&out);
            let label = format!("{}: {} {}", tier.name, run.0, run.2);
            match (ok, n) {
                (true, 0) => Report::new(
                    label,
                    Outcome::Fail,
                    "ran no test: a binary that ran nothing certified nothing",
                ),
                (true, n) => Report::new(label, Outcome::Pass, format!("{n} test(s)")),
                (false, n) => Report::new(
                    label,
                    Outcome::Fail,
                    format!("{n} test(s) passed; see above"),
                ),
            }
        })
        .collect()
}

/// `cargo xtask gate <tier>`.
pub fn gate(tier: &Tier, profile: Profile) -> i32 {
    print_table(
        &format!("Gate {} ({})", tier.name, tier.title),
        &run_tier(tier, profile),
    )
}

/// The Asheron's Call 2 packages: `auberean-*`, `yalain`, `yalain-*`, `athenaeum-*`.
fn packages() -> Vec<String> {
    let ws = workspace_root();
    let Ok(out) = std::process::Command::new("cargo")
        .args([
            "metadata",
            "--no-deps",
            "--format-version",
            "1",
            "--offline",
        ])
        .current_dir(&ws)
        .output()
    else {
        return Vec::new();
    };
    let meta: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_default();
    let mut names: Vec<String> = meta["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| p["name"].as_str())
        .filter(|n| crate::test_lint::is_ac2_package(n))
        .map(str::to_owned)
        .collect();
    names.sort();
    names
}

/// Says, before the tiers run, which of their data was found; with both games running, a shortfall
/// also says how to leave this game out.
pub fn announce(game: Game) {
    for (data, what) in [
        (Data::Installs, "end-of-retail data files"),
        (Data::Captures, "capture index"),
    ] {
        match shortfall(data) {
            None => println!("Asheron's Call 2's {what} found"),
            Some(why) => println!(
                "Asheron's Call 2's {what} NOT found; every tier that reads this reports \
                 NO-ORACLE, so this run exits non-zero: {why}.{}",
                if game == Game::All {
                    " `--game dereth` runs Asheron's Call's tiers alone."
                } else {
                    ""
                }
            ),
        }
    }
}

/// `ci tier1`'s rows for Asheron's Call 2: its crates' tests that read nothing, then every data
/// tier (not the one on a graphics device).
pub fn tier1_rows(profile: Profile) -> Vec<Report> {
    let ws = workspace_root();
    let pkgs = packages();
    let mut argv: Vec<&str> = vec!["test"];
    argv.extend_from_slice(profile.cargo_args());
    for p in &pkgs {
        argv.extend(["-p", p.as_str()]);
    }
    println!("$ cargo {}", argv.join(" "));
    let (ok, out) = run_captured(&ws, "cargo", &argv);
    print!("{out}");
    let n = super::tests_run(&out);
    let mut reports = vec![Report::new(
        "auberean: the crates' tests that read nothing",
        if ok && n > 0 {
            Outcome::Pass
        } else {
            Outcome::Fail
        },
        format!("{} package(s), {n} test(s)", pkgs.len()),
    )];
    for t in TIERS {
        reports.extend(run_tier(t, profile));
    }
    reports
}

/// `ci tier2`'s rows for Asheron's Call 2: the tier on a graphics device.
pub fn gpu_rows(profile: Profile) -> Vec<Report> {
    run_tier(&GPU_TIER, profile)
}

/// `cargo xtask gates`'s rows for Asheron's Call 2: its whole suite, the device's tier included.
pub fn gate_rows(profile: Profile) -> Vec<Report> {
    let mut reports = tier1_rows(profile);
    reports.extend(gpu_rows(profile));
    reports
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **Every data test binary of Asheron's Call 2 is in exactly one tier, with the features its
    /// manifest requires**: read from the manifests of every one of its crates in the workspace,
    /// so a binary added, renamed or given another feature without this table following is a
    /// failure, not a tier that silently stopped running it.
    #[test]
    fn every_data_binary_is_in_one_tier_with_the_features_it_requires() {
        let ws = workspace_root();
        let root = std::fs::read_to_string(ws.join("Cargo.toml")).expect("the root manifest");
        let members = root
            .split_once("members = [")
            .and_then(|(_, rest)| rest.split_once(']'))
            .expect("[workspace] members")
            .0;
        let mut declared = Vec::new();
        let mut crates = 0;
        for dir in members
            .lines()
            .map(str::trim)
            .filter(|l| l.starts_with('"'))
            .map(|l| l.trim_end_matches(',').trim_matches('"'))
        {
            let text = std::fs::read_to_string(ws.join(dir).join("Cargo.toml")).expect("manifest");
            let manifest: toml::Value = toml::from_str(&text).expect("a manifest");
            let krate = manifest["package"]["name"].as_str().expect("a name");
            if !crate::test_lint::is_ac2_package(krate) {
                continue;
            }
            crates += 1;
            for t in manifest
                .get("test")
                .and_then(|t| t.as_array())
                .into_iter()
                .flatten()
            {
                let name = t["name"].as_str().unwrap_or_default();
                if name == "cpu" {
                    continue;
                }
                let features: Vec<&str> = t
                    .get("required-features")
                    .and_then(|f| f.as_array())
                    .into_iter()
                    .flatten()
                    .filter_map(|f| f.as_str())
                    .collect();
                declared.push((krate.to_owned(), name.to_owned(), features.join(",")));
            }
        }
        assert!(
            crates > 10,
            "the second game's manifests were read: {crates}"
        );
        let mut tiered: Vec<(String, String, String)> = tiers()
            .flat_map(|t| t.runs.iter())
            .map(|(k, f, b)| ((*k).to_owned(), (*b).to_owned(), (*f).to_owned()))
            .collect();
        declared.sort();
        tiered.sort();
        assert_eq!(tiered, declared);
    }

    /// **A missing kind of data names its variable**, and a run is the one cargo command.
    #[test]
    fn a_shortfall_names_its_variable_and_a_run_is_one_command() {
        // The process's own variables decide the answer here, so only the wording is checked.
        if let Some(s) = shortfall(Data::Captures) {
            assert!(s.contains("AUBEREAN_TEST_CAPTURE_INDEX"), "{s}");
        }
        if let Some(s) = shortfall(Data::Installs) {
            assert!(s.contains("AUBEREAN_TEST_DAT_DIR"), "{s}");
        }
        if let Some(s) = shortfall(Data::FirstGame) {
            assert!(s.contains("DERETH_TEST_DAT_DIR"), "{s}");
        }
        assert_eq!(
            invocation(("auberean-dat", "retail-dats", "dat"), Profile::TestRelease).join(" "),
            "test --profile test-release -p auberean-dat --features retail-dats --test dat"
        );
        assert_eq!(
            find("AUBEREAN-CORPUS").map(|t| t.name),
            Some("auberean-corpus")
        );
        let network = find("auberean-network").expect("the network's tier");
        assert_eq!(
            network.builds,
            [
                "dereth-network-hub",
                "empyrean-server",
                "empyrean-import",
                "yalain"
            ],
            "it builds every program its test starts"
        );
        assert!(
            find("dat").is_none(),
            "the first game's gate names are not these"
        );
    }

    /// **The device's tier is tier 2's alone, runs its binaries one test at a time, and reports
    /// NOT-BUILT while it has none**: tier 1 (the data tiers) never reaches a device, and a tier
    /// with no binary is never counted as a pass or as an unconsulted oracle.
    #[test]
    fn the_device_tier_is_tier2s_alone_and_not_built_while_it_has_no_binary() {
        assert!(
            !TIERS.iter().any(|t| t.name == GPU_TIER.name),
            "tier 1 runs TIERS, which must not hold the device's tier"
        );
        assert!(TIERS
            .iter()
            .flat_map(|t| t.runs.iter())
            .all(|(_, _, b)| *b != "gpu"));
        assert!(GPU_TIER.runs.iter().all(|(_, _, b)| *b == "gpu"));
        assert_eq!(find("auberean-gpu").map(|t| t.name), Some("auberean-gpu"));
        assert!(
            tiers().any(|t| t.name == "auberean-gpu"),
            "`cargo xtask gate auberean-gpu` is listed"
        );
        assert_eq!(
            invocation(("auberean-gpu", "retail-dats", "gpu"), Profile::TestRelease).join(" "),
            "test --profile test-release -p auberean-gpu --features retail-dats --test gpu \
             -- --test-threads=1"
        );
        let empty = Tier {
            name: "auberean-planted",
            title: "a tier with no binary",
            reads: &[Data::Installs, Data::Captures],
            builds: &[],
            runs: &[],
        };
        let rows = run_tier(&empty, Profile::TestRelease);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].outcome, Outcome::NotRun(NotRun::NotBuilt));
        assert!(!rows[0].outcome.is_failure());
    }
}
