//! Building the New Dereth data set ([`dereth_launch::newdereth`]): the art tool beside the client
//! run on the player's own Asheron's Call and Asheron's Call 2 files, its lines read as it goes,
//! and the folder it wrote put in the data set's place once it is whole.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::Read as _;
use std::path::{Path, PathBuf};

use dereth_launch::ac2_look::Line;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::ac2_look::RunTool;

/// How far a build has got, for the page.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Progress {
    /// What it is doing, in plain words.
    pub doing: String,
    /// The steps done.
    pub steps: usize,
    /// The steps it is expected to take: the last build's, else the steps a build is known to say.
    pub of: Option<usize>,
}

/// The data set on this computer, as the page shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BuiltView {
    pub generation: u32,
    /// The folder of its data files.
    pub overlay: String,
    /// The converter's version that built it.
    pub converter: String,
}

/// The Library's New Dereth data set: what it is built from, whether it can be built, the set on
/// this computer and how a build is going.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct View {
    /// The art tool is beside the client.
    pub tool: bool,
    /// The Asheron's Call 2 files it is built from, when there are any, and whether they check.
    pub ac2: Option<String>,
    pub ac2_ok: bool,
    /// Asheron's Call's own files are here.
    pub ac1: bool,
    /// The New Dereth world's file, once it has been fetched.
    pub world: Option<String>,
    pub built: Option<BuiltView>,
    /// The set on this computer is not the land the New Dereth world pins, and is to be built
    /// again.
    pub stale: bool,
    pub progress: Option<Progress>,
    /// Why the last build failed, until the next one.
    pub failed: Option<String>,
}

/// The art tool run for a build of the data set: [`crate::ac2_look::run_tool`], its failures said
/// as the data set's.
///
/// # Errors
/// It will not start, or it fails.
pub fn run_tool(
    program: &Path,
    args: &[OsString],
    line: &mut dyn FnMut(&str),
    log: &Path,
) -> Result<(), String> {
    crate::ac2_look::run_tool_logged(program, args, line, "The New Dereth data set", Some(log))
}

/// What a build said: the folder of data files it wrote, the steps it took and the converter's
/// version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Said {
    pub overlay: PathBuf,
    pub steps: usize,
    pub converter: String,
}

/// Run a build, telling `progress` each step in plain words against `of` (the steps a build is
/// expected to take).
///
/// # Errors
/// It will not run, fails, or never says it wrote the set.
pub fn build(
    run: &RunTool<'_>,
    tool: &Path,
    args: &[OsString],
    of: Option<usize>,
    progress: &mut dyn FnMut(Progress),
) -> Result<Said, String> {
    let mut steps = 0;
    let mut built = None;
    let mut converter = String::new();
    let mut refused = None;
    let ran = run(tool, args, &mut |l| match Line::read(l) {
        Line::Other => {
            if let Some(v) = dereth_launch::newdereth::converter_of(l) {
                converter = v;
            }
            if let Some(r) = dereth_launch::newdereth::LandRefusal::of(l) {
                refused = Some(r);
            }
        }
        Line::Step(what) => {
            steps += 1;
            progress(Progress {
                doing: dereth_launch::newdereth::step_words(&what).to_owned(),
                steps,
                of,
            });
        }
        Line::Built { file, .. } => built = Some(file),
    });
    // The world pins another land: said as what to do, not as the converter says it.
    if let Some(r) = refused.filter(|_| ran.is_err()) {
        return Err(r.words());
    }
    ran?;
    built
        .map(|overlay| Said {
            overlay,
            steps,
            converter,
        })
        .ok_or_else(|| "The converter never said it wrote the New Dereth data set.".to_owned())
}

/// The SHA-256 of each of the land's files in the folder of data files `overlay`, by the `[land]`
/// key that pins it ([`dereth_launch::newdereth::LAND_FILES`]); a file not there is left out.
///
/// # Errors
/// A file there that will not read.
pub fn land_files(overlay: &Path) -> Result<BTreeMap<String, String>, String> {
    let mut out = BTreeMap::new();
    for (file, key) in dereth_launch::newdereth::LAND_FILES {
        let path = overlay.join(file);
        if path.is_file() {
            out.insert(key.to_owned(), digest_of(&path)?);
        }
    }
    Ok(out)
}

/// The SHA-256 of the file `path`, read whole.
///
/// # Errors
/// It will not read.
fn digest_of(path: &Path) -> Result<String, String> {
    let mut input = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut all = Sha256::new();
    let mut buf = vec![0; 1 << 16];
    loop {
        let n = input.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        all.update(&buf[..n]);
    }
    Ok(format!("{:x}", all.finalize()))
}

/// Put the folder a build wrote, `staging`, in the data set's place, `set`: the set there before
/// is moved to `replaced` first, and taken away once the new one is in place, or put back when it
/// cannot be.
///
/// # Errors
/// A folder that will not move, as when a program has its files open.
pub fn place(staging: &Path, set: &Path, replaced: &Path) -> Result<(), String> {
    let busy = |e: std::io::Error| {
        format!("The New Dereth data set could not be put in place ({e}). Close Lyceum and any world playing New Dereth, then build it again.")
    };
    if replaced.exists() {
        std::fs::remove_dir_all(replaced).map_err(busy)?;
    }
    let had = set.exists();
    if had {
        std::fs::rename(set, replaced).map_err(busy)?;
    }
    if let Err(e) = std::fs::rename(staging, set) {
        if had {
            let _ = std::fs::rename(replaced, set);
        }
        return Err(busy(e));
    }
    if had {
        let _ = std::fs::remove_dir_all(replaced);
    }
    Ok(())
}

/// Where the folder the tool said it wrote, `said`, is inside the folder it was given, `staging`:
/// as said, or as both are on disk.
#[must_use]
pub fn inside(said: &Path, staging: &Path) -> Option<PathBuf> {
    if said.starts_with(staging) {
        return Some(said.to_path_buf());
    }
    let (said, base) = (said.canonicalize().ok()?, staging.canonicalize().ok()?);
    said.strip_prefix(&base).ok().map(|rel| staging.join(rel))
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the launcher building the New Dereth data set, with a stand-in for the art tool)
    use super::*;

    /// A build the converter refuses because the world pins another land fails with what to do,
    /// not with the converter's own words.
    #[test]
    fn a_build_of_another_land_than_the_world_pins_says_what_to_do() {
        let run: &RunTool<'_> = &|_, _, line| {
            line("converter 0123456789abcdef");
            line("step inputs");
            line("refused land 6 47ced4bf5 -");
            Err("The New Dereth data set could not be built: this converter makes another land than the world pins".into())
        };
        assert_eq!(
            build(run, Path::new("tool"), &[], None, &mut |_| {}),
            Err("The New Dereth world on this computer is at land generation 6, made with a different converter (dereth 47ced4bf5) than this launcher's. Update the New Dereth world to a newer land generation, or build with the converter the world names.".into())
        );
    }

    #[test]
    fn a_build_says_each_step_against_the_last_and_ends_with_the_folder_it_wrote() {
        let run: &RunTool<'_> = &|_tool, args, line| {
            assert_eq!(args[0], "newdereth-world");
            for l in [
                "converter 0123456789abcdef",
                "step inputs",
                "step lighting",
                "built C:/a b/overlay 4",
            ] {
                line(l);
            }
            Ok(())
        };
        let mut seen = Vec::new();
        let done = build(
            run,
            Path::new("tool"),
            &["newdereth-world".into()],
            Some(5),
            &mut |p| {
                seen.push(p);
            },
        );
        assert_eq!(
            done,
            Ok(Said {
                overlay: PathBuf::from("C:/a b/overlay"),
                steps: 2,
                converter: "0123456789abcdef".into()
            })
        );
        assert_eq!(
            seen,
            [
                Progress {
                    doing: "Reading the files".into(),
                    steps: 1,
                    of: Some(5)
                },
                Progress {
                    doing: "Building".into(),
                    steps: 2,
                    of: Some(5)
                }
            ]
        );
        let silent: &RunTool<'_> = &|_, _, _| Ok(());
        assert!(build(silent, Path::new("tool"), &[], None, &mut |_| {}).is_err());
        let failing: &RunTool<'_> =
            &|_, _, _| Err("The New Dereth data set could not be built.".into());
        assert_eq!(
            build(failing, Path::new("tool"), &[], None, &mut |_| {}),
            Err("The New Dereth data set could not be built.".into())
        );
    }

    #[test]
    fn a_new_set_takes_the_old_ones_place_only_whole() {
        let temp = tempfile::tempdir().unwrap();
        let (staging, set, replaced) = (
            temp.path().join(".building"),
            temp.path().join("set"),
            temp.path().join(".replaced"),
        );
        std::fs::create_dir_all(&staging).unwrap();
        std::fs::write(staging.join("a.dat"), b"one").unwrap();
        place(&staging, &set, &replaced).unwrap();
        assert_eq!(std::fs::read(set.join("a.dat")).unwrap(), b"one");
        std::fs::create_dir_all(&staging).unwrap();
        std::fs::write(staging.join("a.dat"), b"two").unwrap();
        place(&staging, &set, &replaced).unwrap();
        assert_eq!(std::fs::read(set.join("a.dat")).unwrap(), b"two");
        assert!(!staging.exists() && !replaced.exists());
        // Nothing built: the set stays as it was.
        assert!(place(&staging, &set, &replaced).is_err());
        assert_eq!(std::fs::read(set.join("a.dat")).unwrap(), b"two");
    }

    #[test]
    fn the_lands_files_are_hashed_by_the_key_that_pins_each() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("overlay_cell.dat"), b"cell").unwrap();
        std::fs::write(temp.path().join("world.toml"), b"rules").unwrap();
        let files = land_files(temp.path()).unwrap();
        assert_eq!(
            files.keys().map(String::as_str).collect::<Vec<_>>(),
            ["cell_overlay_sha256", "rules_sha256"]
        );
        assert_eq!(
            files["cell_overlay_sha256"],
            "39d02b42938b2b75d027ebeb25c9d1fd0b71b6e0453e217a2e85cedc7e4637b4"
        );
    }

    #[test]
    fn the_folder_a_build_names_is_found_inside_the_one_it_was_given() {
        let temp = tempfile::tempdir().unwrap();
        let staging = temp.path().join(".building");
        std::fs::create_dir_all(staging.join("overlay")).unwrap();
        assert_eq!(
            inside(&staging.join("overlay"), &staging),
            Some(staging.join("overlay"))
        );
        let canonical = staging.join("overlay").canonicalize().unwrap();
        assert_eq!(inside(&canonical, &staging), Some(staging.join("overlay")));
        assert_eq!(inside(temp.path(), &staging), None);
    }
}
