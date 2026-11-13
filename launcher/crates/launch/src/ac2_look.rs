//! The AC2 look: an art pack of Asheron's Call 2's ground, trees, rocks and sky, which the launcher
//! builds on the player's own computer from the player's own Asheron's Call 2 files. Nothing of
//! that art ships with Dereth: the art tool beside the client ([`PROGRAM`]) reads the files and the
//! tables of which AC2 thing stands for which, and writes the pack.
//!
//! **The parts** ([`PARTS`]) are the player's to take or leave, each a tick box: the ground, the
//! red desert (the desert's ground in AC2's own red, on by default), the trees and ground cover, the
//! rocks and the sky. The build is one pack, [`PACK_ID`], with the parts ticked; the rocks also read
//! Asheron's Call's own files, for the collision each rock is fitted inside.
//!
//! **Where it is kept**: the art folder's `ac2-look/` ([`FOLDER`]): the pack ([`FILE`]), which
//! joins the art packs on this computer; what it was built from ([`RECORD`]): the parts, the
//! install and the version of each part's table; and the player's choices for the next build
//! ([`CHOICES`]): the parts ticked, and the install's folder when the player chose one other than
//! the Library's. The tables come with the art tool, each with its version, so
//! a newer tool may bring a newer table: a look built from an older one is built again
//! ([`Built::stale`]).
//!
//! **The tool's lines.** `--versions` prints `look <version>` and then a part and its table's
//! version a line ([`Versions::read`]); a build prints `step <what>` a line as it goes and
//! `built <file> <version>` at the end ([`Line::read`]).

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The art tool, beside the client.
pub const PROGRAM: &str = "dereth-art";

/// The tool's command that builds the look.
pub const COMMAND: &str = "ac2-look";

/// The look's pack id.
pub const PACK_ID: &str = "local.ac2-look";

/// The look's folder in the art folder.
pub const FOLDER: &str = "ac2-look";

/// The packed look in its folder.
pub const FILE: &str = "ac2-look.dat";

/// What the look was built from, in its folder.
pub const RECORD: &str = "built.json";

/// The player's choices for the next build, in its folder.
pub const CHOICES: &str = "choices.json";

/// The parts, by the tool's name for each and the page's, in the order the page lists them.
pub const PARTS: [(&str, &str); 5] = [
    ("terrain", "Terrain"),
    ("red-desert", "Red Desert"),
    ("trees", "Trees and Ground Cover"),
    ("rocks", "Rocks"),
    ("sky", "Sky"),
];

/// The part that reads Asheron's Call's own files.
pub const NEEDS_AC1: &str = "rocks";

/// Every part, the default.
#[must_use]
pub fn every_part() -> Vec<String> {
    PARTS.iter().map(|(p, _)| (*p).to_owned()).collect()
}

/// The parts of `asked` the look knows, in its order, each once.
#[must_use]
pub fn known_parts(asked: &[String]) -> Vec<String> {
    PARTS
        .iter()
        .filter(|(p, _)| asked.iter().any(|a| a == p))
        .map(|(p, _)| (*p).to_owned())
        .collect()
}

/// The tables' versions, as the art tool reports them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Versions {
    /// The look's own version, which the pack is given.
    pub look: String,
    /// Each part's table's version.
    pub parts: BTreeMap<String, u32>,
}

impl Versions {
    /// The tool's `--versions` lines; `None` when they name no look version.
    #[must_use]
    pub fn read(text: &str) -> Option<Self> {
        let mut v = Self::default();
        for line in text.lines() {
            let mut words = line.split_whitespace();
            match (words.next(), words.next()) {
                (Some("look"), Some(version)) => version.clone_into(&mut v.look),
                (Some(part), Some(n)) => {
                    if let Ok(n) = n.parse() {
                        v.parts.insert(part.to_owned(), n);
                    }
                }
                _ => {}
            }
        }
        (!v.look.is_empty()).then_some(v)
    }
}

/// What the look on this computer was built from.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Built {
    /// The parts ticked.
    pub parts: Vec<String>,
    /// The tables it was built from.
    pub versions: Versions,
    /// The Asheron's Call 2 install it was built from.
    pub install: PathBuf,
}

impl Built {
    /// Whether a table any of its parts was built from has a newer version in `now`.
    #[must_use]
    pub fn stale(&self, now: &Versions) -> bool {
        self.parts.iter().any(|p| {
            let had = self.versions.parts.get(p);
            now.parts.get(p).is_some_and(|n| had != Some(n))
        })
    }

    /// The record in the look's folder `dir`; `None` when there is none or it will not read.
    #[must_use]
    pub fn load(dir: &Path) -> Option<Self> {
        let bytes = std::fs::read(dir.join(RECORD)).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    /// Write the record into the look's folder `dir`.
    ///
    /// # Errors
    /// The folder or the file will not write.
    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(dir.join(RECORD), text)
    }
}

/// The player's choices for the next build.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Choices {
    /// The parts ticked.
    pub parts: Vec<String>,
    /// The install's folder, when the player chose one other than the Library's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install: Option<PathBuf>,
}

impl Default for Choices {
    fn default() -> Self {
        Self {
            parts: every_part(),
            install: None,
        }
    }
}

impl Choices {
    /// The choices in the look's folder `dir`; the defaults (every part) when there are none.
    #[must_use]
    pub fn load(dir: &Path) -> Self {
        std::fs::read(dir.join(CHOICES))
            .ok()
            .and_then(|b| serde_json::from_slice::<Self>(&b).ok())
            .map(|c| Self {
                parts: known_parts(&c.parts),
                ..c
            })
            .unwrap_or_default()
    }

    /// Write the choices into the look's folder `dir`.
    ///
    /// # Errors
    /// The folder or the file will not write.
    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(dir.join(CHOICES), text)
    }
}

/// The art tool's arguments that build `parts` from the install `ac2` (and Asheron's Call's files
/// `ac1`, for the rocks) into the file `out`.
#[must_use]
pub fn build_args(ac2: &Path, ac1: Option<&Path>, out: &Path, parts: &[String]) -> Vec<OsString> {
    let mut args: Vec<OsString> = vec![
        COMMAND.into(),
        "--ac2".into(),
        ac2.into(),
        "--out".into(),
        out.into(),
        "--parts".into(),
        parts.join(",").into(),
    ];
    if let Some(ac1) = ac1 {
        args.extend(["--ac1".into(), ac1.into()]);
    }
    args
}

/// The art tool's arguments that ask for its tables' versions.
#[must_use]
pub fn versions_args() -> Vec<OsString> {
    vec![COMMAND.into(), "--versions".into()]
}

/// One line the tool says as it builds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    /// What it is doing now.
    Step(String),
    /// It wrote the pack: its file and version.
    Built { file: PathBuf, version: String },
    /// Anything else.
    Other,
}

impl Line {
    /// A line of the tool's standard output.
    #[must_use]
    pub fn read(line: &str) -> Self {
        let line = line.trim_end();
        if let Some(what) = line.strip_prefix("step ") {
            return Self::Step(what.to_owned());
        }
        if let Some(rest) = line.strip_prefix("built ") {
            if let Some((file, version)) = rest.rsplit_once(' ') {
                return Self::Built {
                    file: PathBuf::from(file),
                    version: version.to_owned(),
                };
            }
        }
        Self::Other
    }
}

/// What a build step says on the page: the kind of thing, in plain words.
#[must_use]
pub fn step_words(step: &str) -> &'static str {
    match step.split_whitespace().next() {
        Some("ground") => "Ground",
        Some("model") => "Trees and rocks",
        Some("cover") => "Ground cover",
        Some("sky") => "Sky",
        Some("packing") => "Packing",
        _ => "Building",
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the launcher's half of the AC2 look builder)
    use super::*;

    #[test]
    fn the_tools_versions_read_back_as_the_look_and_each_parts_table() {
        let v = Versions::read("look 0.1.4\nterrain 1\nred-desert 1\ntrees 2\nrocks 1\nsky 1\n")
            .expect("versions");
        assert_eq!(v.look, "0.1.4");
        assert_eq!(v.parts.get("trees"), Some(&2));
        assert_eq!(v.parts.len(), 5);
        assert_eq!(Versions::read("nothing here\n"), None);
    }

    #[test]
    fn a_look_is_stale_only_when_a_table_of_a_part_it_holds_is_newer() {
        let then = Versions::read("look 0.1.4\nterrain 1\ntrees 1\nrocks 1\nsky 1\n").expect("v");
        let built = Built {
            parts: vec!["terrain".into(), "sky".into()],
            versions: then.clone(),
            install: PathBuf::from("ac2"),
        };
        assert!(!built.stale(&then));
        let trees = Versions::read("look 0.1.5\nterrain 1\ntrees 2\nrocks 1\nsky 1\n").expect("v");
        assert!(!built.stale(&trees), "the trees are not in it");
        let sky = Versions::read("look 0.1.5\nterrain 1\ntrees 1\nrocks 1\nsky 2\n").expect("v");
        assert!(built.stale(&sky));
    }

    #[test]
    fn a_record_writes_and_reads_back() {
        let dir = std::env::temp_dir().join(format!("dereth-ac2-look-{}", std::process::id()));
        let built = Built {
            parts: every_part(),
            versions: Versions::read("look 0.1.4\nsky 1\n").expect("v"),
            install: PathBuf::from("C:/AC2"),
        };
        let saved = built.save(&dir);
        let read = Built::load(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        saved.expect("saves");
        assert_eq!(read, Some(built));
    }

    #[test]
    fn choices_start_with_every_part_and_keep_only_known_ones() {
        let dir = std::env::temp_dir().join(format!("dereth-ac2-choices-{}", std::process::id()));
        let fresh = Choices::load(&dir);
        let c = Choices {
            parts: vec!["sky".into(), "dungeons".into()],
            install: Some(PathBuf::from("D:/AC2")),
        };
        let saved = c.save(&dir);
        let read = Choices::load(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        saved.expect("saves");
        assert_eq!(fresh.parts, every_part());
        assert_eq!(read.parts, ["sky"]);
        assert_eq!(read.install, Some(PathBuf::from("D:/AC2")));
    }

    #[test]
    fn the_build_lines_read_as_steps_and_the_built_file() {
        assert_eq!(
            Line::read("step model 4700003E-24\n"),
            Line::Step("model 4700003E-24".into())
        );
        assert_eq!(
            Line::read("built C:/My Art/ac2-look/ac2-look.dat 0.1.4"),
            Line::Built {
                file: PathBuf::from("C:/My Art/ac2-look/ac2-look.dat"),
                version: "0.1.4".into()
            }
        );
        assert_eq!(Line::read("left out: nothing"), Line::Other);
        assert_eq!(step_words("cover 4700000B"), "Ground cover");
    }

    #[test]
    fn the_build_names_the_parts_and_only_reads_ac1_when_given_it() {
        let parts = known_parts(&["sky".into(), "dungeons".into(), "terrain".into()]);
        assert_eq!(parts, ["terrain", "sky"]);
        let args = build_args(Path::new("ac2"), None, Path::new("out.dat"), &parts);
        let words: Vec<String> = args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            words,
            [
                "ac2-look",
                "--ac2",
                "ac2",
                "--out",
                "out.dat",
                "--parts",
                "terrain,sky"
            ]
        );
        let args = build_args(
            Path::new("ac2"),
            Some(Path::new("ac1")),
            Path::new("o"),
            &parts,
        );
        assert_eq!(args.len(), 9);
    }
}
