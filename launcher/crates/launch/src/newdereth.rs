//! The New Dereth data set: the data files a New Dereth world is played on, converted from the
//! player's own Asheron's Call 2 files by the art tool beside the client ([`PROGRAM`]), on the
//! player's own computer. Nothing of Asheron's Call 2 ships with Dereth.
//!
//! **What it is built from**: the New Dereth world's `world.toml` (its world repository, which
//! pins the land it is built on: its content digest and its files' hashes, under `[land]`), the
//! player's Asheron's Call files
//! (the Library's default Modern set) and Asheron's Call 2 files (Auberean's library's default
//! set, the one place either Library's Add Folder or Download fills).
//!
//! **Where it is kept**: [`FOLDER`] in the launcher's library folder. A build writes into a
//! folder of its own beside it and takes its place only once it is whole, so a build that fails
//! leaves the set there as it was.
//!
//! **The tool's lines.** A build prints `converter <version>` first ([`converter_of`]), then
//! `step <what>` a line as it goes and `built <dir> <generation>` at the end
//! ([`crate::ac2_look::Line::read`] reads both); its `stats.txt` names the content it built
//! ([`content_of_stats`]).
//!
//! **When it is built again** ([`Built::is_land_of`]): when it is not the land the world pins,
//! its content digest and its files' hashes compared with the world's `[land]`, as Lyceum compares
//! a land it is given.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The art tool, beside the client.
pub const PROGRAM: &str = crate::ac2_look::PROGRAM;

/// The tool's command that builds the data set.
pub const COMMAND: &str = "newdereth-world";

/// The data set's folder in the launcher's library folder.
pub const FOLDER: &str = "newdereth";

/// The folder a build writes into, beside [`FOLDER`], until it is whole.
pub const BUILDING: &str = ".newdereth-building";

/// The folder the data set it replaces is moved to while the new one takes its place.
pub const REPLACED: &str = ".newdereth-replaced";

/// The file in the launcher's data folder that holds everything the last build said, made anew by
/// each build.
pub const BUILD_LOG: &str = "newdereth-build.log";

/// The folder the build is told to write the data files into, in the folder it builds in.
pub const OVERLAY: &str = "overlay";

/// The file in the data set's folder that says it is the launcher's New Dereth data set, which a
/// hosted world may play without being held to it ([`is_data_set`]): what it was built from.
pub const MARKER: &str = "newdereth.json";

/// Whether the data overlay folder `overlay` is the data files of a New Dereth data set the
/// launcher built: its folder holds [`MARKER`].
#[must_use]
pub fn is_data_set(overlay: &Path) -> bool {
    overlay
        .parent()
        .is_some_and(|set| set.join(MARKER).is_file())
}

/// The build's report that names the content it built, beside the folder of data files.
pub const STATS: &str = "stats.txt";

/// The steps a build says, in the order it takes them, each with the page's words for it. A build
/// may say others; each still counts as a step.
pub const STEPS: [(&str, &str); 13] = [
    ("inputs", "Reading the files"),
    ("ac2", "Reading Asheron's Call 2"),
    ("land", "Land"),
    ("statics", "Buildings and objects"),
    ("dungeons", "Dungeons"),
    ("ledges", "Ledges"),
    ("cover", "Ground cover"),
    ("blocks", "Blocks"),
    ("pictures", "Pictures"),
    ("start", "Starting area"),
    ("writing", "Writing"),
    ("map", "Map"),
    ("verifying", "Checking"),
];

/// What a build step says on the page: the step's plain words, or "Building" for one not known.
#[must_use]
pub fn step_words(step: &str) -> &'static str {
    let first = step.split_whitespace().next().unwrap_or("");
    STEPS
        .iter()
        .find(|(s, _)| *s == first)
        .map_or("Building", |(_, w)| w)
}

/// The converter's version, from its first line `converter <version>`; `None` for any other line.
#[must_use]
pub fn converter_of(line: &str) -> Option<String> {
    let version = line.trim().strip_prefix("converter ")?.trim();
    (!version.is_empty() && !version.contains(char::is_whitespace)).then(|| version.to_owned())
}

/// A build the converter refused because the world pins another land, as it says on its line
/// `refused land <the world's generation> <the converter the world names, or -> <this land's
/// generation in the world's ledger, or ->`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LandRefusal {
    /// The land generation the world is at.
    pub world: u32,
    /// The converter the world names (its commit's first digits).
    pub converter: Option<String>,
    /// The generation this converter's land is in the world's ledger, when it is there.
    pub this: Option<u32>,
}

impl LandRefusal {
    /// The converter's line, or `None` for any other.
    #[must_use]
    pub fn of(line: &str) -> Option<Self> {
        let mut w = line
            .trim()
            .strip_prefix("refused land ")?
            .split_whitespace();
        let world = w.next()?.parse().ok()?;
        let converter = w.next().filter(|c| *c != "-").map(str::to_owned);
        let this = match w.next()? {
            "-" => None,
            n => Some(n.parse().ok()?),
        };
        w.next().is_none().then_some(Self {
            world,
            converter,
            this,
        })
    }

    /// What the player is told: what is wrong, and what to do.
    #[must_use]
    pub fn words(&self) -> String {
        let named = self
            .converter
            .as_deref()
            .map(|c| format!(" (dereth {c})"))
            .unwrap_or_default();
        let world = self.world;
        match self.this {
            Some(n) if n > world => format!(
                "This launcher builds New Dereth's land generation {n}, newer than the New Dereth world on this computer, which is at generation {world}. Update the New Dereth world to generation {n}, or build with the converter the world names{named}."
            ),
            Some(n) => format!(
                "This launcher builds New Dereth's land generation {n}, older than the New Dereth world on this computer, which is at generation {world}. Update the launcher, or build with the converter the world names{named}."
            ),
            None => format!(
                "The New Dereth world on this computer is at land generation {world}, made with a different converter{named} than this launcher's. Update the New Dereth world to a newer land generation, or build with the converter the world names."
            ),
        }
    }
}

/// The land's files in its folder of data files, each by the `[land]` key that pins its SHA-256.
pub const LAND_FILES: [(&str, &str); 4] = [
    ("overlay_cell.dat", "cell_overlay_sha256"),
    ("overlay_portal.dat", "portal_overlay_sha256"),
    ("places.tsv", "places_sha256"),
    ("world.toml", "rules_sha256"),
];

/// The tool's arguments that build the data set for the world `world` (its `world.toml`) from
/// Asheron's Call's files `dats` and Asheron's Call 2's `ac2` into the folder `out`.
#[must_use]
pub fn build_args(world: &Path, dats: &Path, ac2: &Path, out: &Path) -> Vec<OsString> {
    vec![
        COMMAND.into(),
        "--world".into(),
        world.into(),
        "--dats".into(),
        dats.into(),
        "--ac2-dats".into(),
        ac2.into(),
        "--out".into(),
        out.into(),
    ]
}

/// The content digest and generation a build's `stats.txt` names, from its line `overlay content
/// <digest> is generation <n>`; `None` when it names none.
#[must_use]
pub fn content_of_stats(text: &str) -> Option<(String, u32)> {
    text.lines().find_map(|line| {
        let rest = line.trim().strip_prefix("overlay content ")?;
        let mut words = rest.split_whitespace();
        let digest = words.next()?;
        let (Some("is"), Some("generation"), Some(n)) = (words.next(), words.next(), words.next())
        else {
            return None;
        };
        let n = n
            .trim_end_matches(|c: char| !c.is_ascii_digit())
            .parse()
            .ok()?;
        Some((digest.to_ascii_lowercase(), n))
    })
}

/// What a world file pins its land to under `[land]`, by key: its content digest
/// (`content_sha256`) and each of [`LAND_FILES`]' hashes it names, lower case. Empty when it pins
/// none or does not read.
#[must_use]
pub fn pins(world_toml: &str) -> std::collections::BTreeMap<String, String> {
    let table = world_toml.parse::<toml::Table>().unwrap_or_default();
    let Some(land) = table.get("land").and_then(toml::Value::as_table) else {
        return std::collections::BTreeMap::new();
    };
    std::iter::once("content_sha256")
        .chain(LAND_FILES.iter().map(|(_, key)| *key))
        .filter_map(|key| {
            let pin = land.get(key)?.as_str()?;
            Some((key.to_owned(), pin.to_ascii_lowercase()))
        })
        .collect()
}

/// What the data set on this computer was built from, kept in the launcher's state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Built {
    /// The folder holding its data files, which Lyceum, Empyrean and Dereth are given.
    pub overlay: PathBuf,
    /// The content it holds, by its digest, and that content's generation.
    pub content: String,
    pub generation: u32,
    /// The SHA-256 of each of [`LAND_FILES`] it holds, by the `[land]` key that pins it, taken as
    /// it was built.
    #[serde(default)]
    pub files: std::collections::BTreeMap<String, String>,
    /// The converter's version, as it said it, for the page.
    #[serde(default)]
    pub converter: String,
    /// The steps its build took, which the next build's progress is measured against.
    #[serde(default)]
    pub steps: usize,
}

impl Built {
    /// Whether the set is the land `world_toml` pins: the world pins one, and each pin it names,
    /// the content digest and the files' hashes, is the set's. Then it is that world's land, and
    /// current; a world that pins a land the set is not wants it built again.
    #[must_use]
    pub fn is_land_of(&self, world_toml: &str) -> bool {
        let pins = pins(world_toml);
        !pins.is_empty()
            && pins.iter().all(|(key, pin)| {
                let held = if key == "content_sha256" {
                    Some(&self.content)
                } else {
                    self.files.get(key)
                };
                held.is_some_and(|h| h.eq_ignore_ascii_case(pin))
            })
    }
}

/// Where the data files a build said it wrote in `said` (the folder it built in, `staging`) are
/// once that folder has become the data set's folder `set`; `None` when it named a folder outside
/// the one it was given.
#[must_use]
pub fn placed(said: &Path, staging: &Path, set: &Path) -> Option<PathBuf> {
    let rel = said.strip_prefix(staging).ok()?;
    Some(if rel.as_os_str().is_empty() {
        set.to_path_buf()
    } else {
        set.join(rel)
    })
}

/// The build's `stats.txt`: in the folder of data files it wrote, `overlay`, or in a folder above
/// it no higher than the folder it built in, `staging`.
#[must_use]
pub fn stats_file(overlay: &Path, staging: &Path) -> Option<PathBuf> {
    overlay
        .ancestors()
        .take_while(|a| a.starts_with(staging))
        .map(|a| a.join(STATS))
        .find(|f| f.is_file())
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the launcher's half of building the New Dereth data set)
    use super::*;

    #[test]
    fn a_build_is_asked_for_the_world_both_games_files_and_its_folder() {
        let args = build_args(
            Path::new("w/world.toml"),
            Path::new("ac1"),
            Path::new("ac2"),
            Path::new("out"),
        );
        assert_eq!(
            args,
            [
                "newdereth-world",
                "--world",
                "w/world.toml",
                "--dats",
                "ac1",
                "--ac2-dats",
                "ac2",
                "--out",
                "out"
            ]
            .map(OsString::from)
        );
    }

    #[test]
    fn each_known_step_has_plain_words_and_an_unknown_one_still_reads() {
        assert_eq!(step_words("inputs"), "Reading the files");
        assert_eq!(step_words("verifying"), "Checking");
        assert_eq!(step_words("dungeons 3 of 472"), "Dungeons");
        assert_eq!(step_words("lighting"), "Building");
        assert_eq!(STEPS.len(), 13);
        assert_eq!(step_words(""), "Building");
    }

    #[test]
    fn the_stats_name_the_content_and_its_generation() {
        let stats = "sky: AC1's (--sky ac1)\n\
            overlay content 5A4620CB00 is generation 4 (generations.tsv): iterations 1..=4 over the cell file\n\
            overlay containers:\n";
        assert_eq!(content_of_stats(stats), Some(("5a4620cb00".into(), 4)));
        assert_eq!(content_of_stats("overlay content abc is new\n"), None);
        assert_eq!(content_of_stats("nothing here\n"), None);
    }

    /// A build refused for the world's pinning another land is said plainly: the world's land
    /// generation, what to do, and no digest or flag.
    #[test]
    fn a_refused_land_is_said_as_what_to_do() {
        let r = LandRefusal::of("refused land 6 47ced4bf5 -").expect("a refusal");
        assert_eq!(
            r,
            LandRefusal {
                world: 6,
                converter: Some("47ced4bf5".into()),
                this: None
            }
        );
        assert_eq!(
            r.words(),
            "The New Dereth world on this computer is at land generation 6, made with a different converter (dereth 47ced4bf5) than this launcher's. Update the New Dereth world to a newer land generation, or build with the converter the world names."
        );
        let newer = LandRefusal::of("refused land 6 - 10").unwrap();
        assert_eq!(
            newer.words(),
            "This launcher builds New Dereth's land generation 10, newer than the New Dereth world on this computer, which is at generation 6. Update the New Dereth world to generation 10, or build with the converter the world names."
        );
        assert!(LandRefusal::of("refused land 10 - 6")
            .unwrap()
            .words()
            .contains("Update the launcher"));
        for other in [
            "converter abc",
            "refused land x - -",
            "refused land 6 - - more",
            "step land",
        ] {
            assert_eq!(LandRefusal::of(other), None, "{other}");
        }
    }

    #[test]
    fn the_converter_says_its_version_first() {
        assert_eq!(
            converter_of("converter 0123456789abcdef").as_deref(),
            Some("0123456789abcdef")
        );
        assert_eq!(converter_of("step inputs"), None);
        assert_eq!(converter_of("converter "), None);
    }

    #[test]
    fn a_world_pins_its_land_by_its_content_and_its_files() {
        let world = "lineage = []\n[land]\ncontent_sha256 = \"5A4620CB00\"\n\
            cell_overlay_sha256 = \"CE11AB\"\nledger = \"land/g.tsv\"\n";
        let p = pins(world);
        assert_eq!(
            p.get("content_sha256").map(String::as_str),
            Some("5a4620cb00")
        );
        assert_eq!(
            p.get("cell_overlay_sha256").map(String::as_str),
            Some("ce11ab")
        );
        assert_eq!(p.len(), 2, "the ledger is no pin");
        assert!(pins("lineage = []\n").is_empty());
        assert!(pins("not toml [").is_empty());
    }

    #[test]
    fn a_set_is_the_worlds_land_only_while_every_pin_is_its() {
        let files = LAND_FILES
            .iter()
            .map(|(_, key)| ((*key).to_owned(), format!("{key}-hash")))
            .collect();
        let built = Built {
            overlay: PathBuf::from("set/overlay"),
            content: "aa".into(),
            generation: 4,
            files,
            converter: "0123456789abcdef".into(),
            steps: 13,
        };
        let world = |content: &str, cell: &str| {
            format!(
                "[land]\ncontent_sha256 = '{content}'\ncell_overlay_sha256 = '{cell}'\n\
                 portal_overlay_sha256 = 'portal_overlay_sha256-hash'\n\
                 places_sha256 = 'places_sha256-hash'\nrules_sha256 = 'rules_sha256-hash'\n"
            )
        };
        assert!(built.is_land_of(&world("AA", "cell_overlay_sha256-hash")));
        assert!(
            !built.is_land_of(&world("bb", "cell_overlay_sha256-hash")),
            "other content"
        );
        assert!(
            !built.is_land_of(&world("aa", "other")),
            "another cell file"
        );
        assert!(
            built.is_land_of("[land]\ncontent_sha256 = 'aa'\n"),
            "the pins it names"
        );
        assert!(!built.is_land_of("lineage = []\n"), "no land pinned");
        let unhashed = Built {
            files: std::collections::BTreeMap::new(),
            ..built
        };
        assert!(!unhashed.is_land_of(&world("aa", "cell_overlay_sha256-hash")));
    }

    #[test]
    fn the_folder_a_build_wrote_is_found_again_in_the_sets_place() {
        let staging = Path::new("lib/.newdereth-building");
        let set = Path::new("lib/newdereth");
        assert_eq!(
            placed(&staging.join("overlay"), staging, set),
            Some(set.join("overlay"))
        );
        assert_eq!(placed(staging, staging, set), Some(set.to_path_buf()));
        assert_eq!(placed(Path::new("elsewhere/overlay"), staging, set), None);
    }

    #[test]
    fn a_data_set_is_known_by_its_marker_beside_its_data_files() {
        let dir = std::env::temp_dir().join(format!("dereth-newdereth-set-{}", std::process::id()));
        let overlay = dir.join("overlay");
        std::fs::create_dir_all(&overlay).expect("a folder");
        let before = is_data_set(&overlay);
        std::fs::write(dir.join(MARKER), "{}").expect("writes");
        let after = is_data_set(&overlay);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(!before && after);
    }

    #[test]
    fn the_stats_are_found_beside_the_data_files_or_in_them() {
        let dir = std::env::temp_dir().join(format!("dereth-newdereth-{}", std::process::id()));
        let overlay = dir.join("overlay");
        std::fs::create_dir_all(&overlay).expect("a folder");
        let none = stats_file(&overlay, &dir);
        std::fs::write(dir.join(STATS), "beside").expect("writes");
        let beside = stats_file(&overlay, &dir);
        std::fs::write(overlay.join(STATS), "in").expect("writes");
        let inside = stats_file(&overlay, &dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(none, None);
        assert_eq!(beside, Some(dir.join(STATS)));
        assert_eq!(inside, Some(overlay.join(STATS)));
    }
}
