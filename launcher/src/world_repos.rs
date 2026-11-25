//! World repositories the launcher fetches for the player: a server world's rows (New Dereth's),
//! brought into Lyceum's folder for worlds so Lyceum opens them as it opens any world, and brought
//! up to date later.
//!
//! - **Fetching** reads the commit the repository's branch is at, from its public address's list of
//!   refs (`info/refs`), and unpacks that commit's archive into a folder of its own named for the
//!   world. Beside the files, [`MARKER`] records where they came from, the commit, and each file's
//!   SHA-256 as fetched. No git is needed on the player's computer.
//! - **Updating** is a fast-forward, read-only: when every file is still as fetched, the folder is
//!   replaced by the branch's newest commit; when any was changed, added or taken away, nothing is
//!   touched and the files are named, so a player's own edits are never thrown away by an update.
//!
//! The network is behind [`Remote`], so what is fetched and how is checked without it.

use std::collections::BTreeMap;
use std::io::Read as _;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// A world repository the launcher offers to fetch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Offered {
    /// Its name, as the page shows it, and the folder it is fetched into.
    pub name: &'static str,
    pub folder: &'static str,
    /// Its public address's owner and repository (the mirror of the world's own), and the branch
    /// fetched.
    pub owner: &'static str,
    pub repo: &'static str,
    pub branch: &'static str,
}

/// New Dereth's world repository, which the Library's New Dereth data set is built for.
pub const NEW_DERETH: Offered = Offered {
    name: "New Dereth",
    folder: "New Dereth",
    owner: "dereth-network",
    repo: "empyrean-world-newdereth",
    branch: "main",
};

/// The world repositories offered.
pub const OFFERED: [Offered; 1] = [NEW_DERETH];

/// The file beside a fetched repository's own that says what was fetched.
pub const MARKER: &str = ".dereth-fetched.json";

/// The largest archive fetched.
const MAX_ARCHIVE: u64 = 512 * 1024 * 1024;

/// What a fetched folder holds: where it came from, at which commit, and each file's SHA-256 as
/// fetched, by its path in the folder (`/`-separated).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fetched {
    pub source: String,
    pub branch: String,
    pub commit: String,
    pub files: BTreeMap<String, String>,
}

/// Where a repository is fetched from.
pub trait Remote {
    /// The commit `branch` is at.
    ///
    /// # Errors
    /// The address that won't answer, or a branch it lacks.
    fn head(&self, branch: &str) -> Result<String, String>;
    /// The archive (`.tar.gz`) of commit `commit`'s files, under one top folder.
    ///
    /// # Errors
    /// The address that won't answer.
    fn archive(&self, commit: &str) -> Result<Vec<u8>, String>;
    /// The address the repository is fetched from, as the marker records it.
    fn source(&self) -> String;
}

/// A repository on GitHub, read by its public addresses.
pub struct GitHub<'a> {
    pub owner: &'a str,
    pub repo: &'a str,
}

impl GitHub<'_> {
    /// Whether `source`, a marker's address, is this repository's.
    #[must_use]
    pub fn source_is(&self, source: &str) -> bool {
        source == Remote::source(self)
    }
}

impl Remote for GitHub<'_> {
    fn head(&self, branch: &str) -> Result<String, String> {
        let url = format!(
            "https://github.com/{}/{}.git/info/refs?service=git-upload-pack",
            self.owner, self.repo
        );
        let bytes = crate::components::download(&url, 4 * 1024 * 1024, &mut |_, _| {})?;
        let refs = String::from_utf8_lossy(&bytes);
        head_in(&refs, branch).ok_or_else(|| {
            if refs.contains("capabilities^{}") {
                format!("Nothing is published at {} yet.", self.source())
            } else {
                format!("{} has no branch {branch}.", self.source())
            }
        })
    }

    fn archive(&self, commit: &str) -> Result<Vec<u8>, String> {
        let url = format!(
            "https://codeload.github.com/{}/{}/tar.gz/{commit}",
            self.owner, self.repo
        );
        crate::components::download(&url, MAX_ARCHIVE, &mut |_, _| {})
    }

    fn source(&self) -> String {
        format!("https://github.com/{}/{}", self.owner, self.repo)
    }
}

/// The commit branch `branch` is at in a list of refs as git's smart protocol sends it (each line
/// a length, the commit and the ref's name).
#[must_use]
pub fn head_in(refs: &str, branch: &str) -> Option<String> {
    let want = format!("refs/heads/{branch}");
    refs.lines().find_map(|line| {
        let mut words = line.split(|c: char| c.is_whitespace() || c == '\0');
        let first = words.next()?;
        let name = words.next()?;
        // The first line of a list starts with its length; the commit is its last forty digits.
        let commit = first.get(first.len().checked_sub(40)?..)?;
        (name == want && commit.bytes().all(|b| b.is_ascii_hexdigit()))
            .then(|| commit.to_ascii_lowercase())
    })
}

/// What a fetched folder says it holds, when it is one.
#[must_use]
pub fn fetched(dir: &Path) -> Option<Fetched> {
    serde_json::from_str(&std::fs::read_to_string(dir.join(MARKER)).ok()?).ok()
}

/// Unpack archive `tar_gz` into the new folder `into`, its top folder taken off: each file's
/// SHA-256 by its path there. A path that would leave the folder is refused.
fn unpack(tar_gz: &[u8], into: &Path) -> Result<BTreeMap<String, String>, String> {
    let mut files = BTreeMap::new();
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(tar_gz));
    let entries = archive.entries().map_err(|e| e.to_string())?;
    for entry in entries {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path().map_err(|e| e.to_string())?.into_owned();
        let rel: PathBuf = path.components().skip(1).collect();
        if rel.as_os_str().is_empty() {
            continue;
        }
        if rel.components().any(|c| !matches!(c, Component::Normal(_))) {
            return Err(format!(
                "the archive names {}, outside its folder",
                path.display()
            ));
        }
        let to = into.join(&rel);
        match entry.header().entry_type() {
            tar::EntryType::Directory => {
                std::fs::create_dir_all(&to).map_err(|e| format!("{}: {e}", to.display()))?;
            }
            tar::EntryType::Regular => {
                let mut bytes = Vec::new();
                entry
                    .read_to_end(&mut bytes)
                    .map_err(|e| format!("{}: {e}", rel.display()))?;
                if let Some(d) = to.parent() {
                    std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
                }
                std::fs::write(&to, &bytes).map_err(|e| format!("{}: {e}", to.display()))?;
                files.insert(slashed(&rel), hex(&bytes));
            }
            // GitHub's archives hold a global header and directories besides files; links and the
            // rest a world's rows never are.
            _ => {}
        }
    }
    Ok(files)
}

fn hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn slashed(rel: &Path) -> String {
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Every file under `dir` but the marker, by its path there, with its SHA-256.
fn hashes(dir: &Path) -> Result<BTreeMap<String, String>, String> {
    let mut out = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).map_err(|e| format!("{}: {e}", d.display()))? {
            let p = e.map_err(|e| format!("{}: {e}", d.display()))?.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            let rel = p.strip_prefix(dir).unwrap_or(&p);
            if rel == Path::new(MARKER) {
                continue;
            }
            let bytes = std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display()))?;
            out.insert(slashed(rel), hex(&bytes));
        }
    }
    Ok(out)
}

/// The files of fetched folder `dir` that are not as fetched: changed, added or taken away.
///
/// # Errors
/// A folder that won't read.
pub fn changes(dir: &Path, f: &Fetched) -> Result<Vec<String>, String> {
    let now = hashes(dir)?;
    let mut out: Vec<String> = now
        .iter()
        .filter(|(p, h)| f.files.get(*p) != Some(*h))
        .map(|(p, _)| p.clone())
        .collect();
    out.extend(f.files.keys().filter(|p| !now.contains_key(*p)).cloned());
    out.sort();
    Ok(out)
}

/// Commit `commit` of `remote` unpacked into `dir`, made whole before it takes `dir`'s place.
fn take(remote: &dyn Remote, branch: &str, commit: &str, dir: &Path) -> Result<(), String> {
    let archive = remote.archive(commit)?;
    let name = dir
        .file_name()
        .map_or_else(|| "world".into(), |n| n.to_string_lossy().into_owned());
    let staging = dir.with_file_name(format!(".{name}-fetching"));
    let old = dir.with_file_name(format!(".{name}-before"));
    for d in [&staging, &old] {
        if d.exists() {
            std::fs::remove_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
        }
    }
    let made = (|| -> Result<(), String> {
        std::fs::create_dir_all(&staging).map_err(|e| format!("{}: {e}", staging.display()))?;
        let files = unpack(&archive, &staging)?;
        if !files.contains_key("world.toml") {
            return Err(format!("{} holds no world.toml", remote.source()));
        }
        let marker = Fetched {
            source: remote.source(),
            branch: branch.to_owned(),
            commit: commit.to_owned(),
            files,
        };
        let text = serde_json::to_string_pretty(&marker).map_err(|e| e.to_string())?;
        std::fs::write(staging.join(MARKER), text).map_err(|e| e.to_string())?;
        if dir.exists() {
            std::fs::rename(dir, &old).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        std::fs::rename(&staging, dir).map_err(|e| format!("{}: {e}", dir.display()))
    })();
    if made.is_err() && old.exists() && !dir.exists() {
        let _ = std::fs::rename(&old, dir);
    }
    let _ = std::fs::remove_dir_all(&staging);
    let _ = std::fs::remove_dir_all(&old);
    made
}

/// Fetch `offered` from `remote` into the folder for worlds `home`: its branch's newest commit, in
/// a folder named for it. Answers its world file.
///
/// # Errors
/// A folder of that name already there, the remote's errors, and an archive that won't unpack or
/// holds no world.
pub fn fetch(remote: &dyn Remote, offered: &Offered, home: &Path) -> Result<PathBuf, String> {
    let dir = home.join(offered.folder);
    if dir.exists() {
        return Err(format!(
            "There is already a {} folder in {}.",
            offered.folder,
            home.display()
        ));
    }
    std::fs::create_dir_all(home).map_err(|e| format!("{}: {e}", home.display()))?;
    let commit = remote.head(offered.branch)?;
    take(remote, offered.branch, &commit, &dir)?;
    Ok(dir.join("world.toml"))
}

/// What [`update`] did.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Updated {
    /// The folder is at the branch's newest commit already.
    Current { commit: String },
    /// It was brought from one commit to the other.
    Moved { from: String, to: String },
}

/// Bring fetched folder `dir` up to its branch's newest commit on `remote`: refused, naming them,
/// while any of its files is not as fetched.
///
/// # Errors
/// A folder that wasn't fetched, local changes, and the remote's errors.
pub fn update(remote: &dyn Remote, dir: &Path) -> Result<Updated, String> {
    let f =
        fetched(dir).ok_or_else(|| format!("{} was not fetched by the launcher", dir.display()))?;
    let changed = changes(dir, &f)?;
    if !changed.is_empty() {
        let shown: Vec<&str> = changed.iter().take(5).map(String::as_str).collect();
        return Err(format!(
            "This world's files were changed here ({}{}), so it isn't updated: an update would lose those changes. Move the folder aside to fetch it again.",
            shown.join(", "),
            if changed.len() > shown.len() {
                format!(" and {} more", changed.len() - shown.len())
            } else {
                String::new()
            }
        ));
    }
    let newest = remote.head(&f.branch)?;
    if newest == f.commit {
        return Ok(Updated::Current { commit: newest });
    }
    take(remote, &f.branch, &newest, dir)?;
    Ok(Updated::Moved {
        from: f.commit,
        to: newest,
    })
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the launcher fetching a world repository for the player)
    use super::*;
    use std::cell::RefCell;

    /// A commit as a test has it: its id, and each file's path and text.
    type Commit = (String, Vec<(&'static str, &'static str)>);

    /// A repository as a test has it: its branch's commits, newest last.
    struct Fake {
        commits: RefCell<Vec<Commit>>,
    }

    impl Fake {
        fn new(files: Vec<(&'static str, &'static str)>) -> Self {
            Self {
                commits: RefCell::new(vec![(format!("{:040x}", 1), files)]),
            }
        }
        fn commit(&self, files: Vec<(&'static str, &'static str)>) {
            let mut c = self.commits.borrow_mut();
            let n = c.len() + 1;
            c.push((format!("{n:040x}"), files));
        }
    }

    impl Remote for Fake {
        fn head(&self, branch: &str) -> Result<String, String> {
            (branch == "main")
                .then(|| self.commits.borrow().last().unwrap().0.clone())
                .ok_or_else(|| format!("no branch {branch}"))
        }
        fn archive(&self, commit: &str) -> Result<Vec<u8>, String> {
            let commits = self.commits.borrow();
            let (_, files) = commits
                .iter()
                .find(|(c, _)| c == commit)
                .ok_or("no such commit")?;
            let mut out = Vec::new();
            {
                let gz = flate2::write::GzEncoder::new(&mut out, flate2::Compression::fast());
                let mut tar = tar::Builder::new(gz);
                for (path, text) in files {
                    let mut h = tar::Header::new_gnu();
                    h.set_size(text.len() as u64);
                    h.set_mode(0o644);
                    h.set_cksum();
                    tar.append_data(&mut h, format!("repo-{commit}/{path}"), text.as_bytes())
                        .unwrap();
                }
                tar.into_inner().unwrap().finish().unwrap();
            }
            Ok(out)
        }
        fn source(&self) -> String {
            "https://example.invalid/a-world".into()
        }
    }

    const WORLD: &str = "format_version = 1\nkey = \"a-world\"\nera = \"newdereth\"\npacks = [\"base/emergence\"]\nlineage = []\n";

    fn home(tag: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("launcher-world-repos-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    /// From a launcher with no repository, fetching one gives a folder of the branch's newest
    /// commit in the folder for worlds, which the Lyceum page lists and opens as a world
    /// repository, and which says what it was fetched from and at which commit.
    #[test]
    fn fetching_a_world_repository_gives_its_branchs_newest_files_in_the_worlds_folder() {
        let home = home("fetch");
        let remote = Fake::new(vec![
            ("world.toml", WORLD),
            ("base/emergence/pack.toml", "name = \"emergence\"\n"),
        ]);
        remote.commit(vec![
            ("world.toml", WORLD),
            (
                "base/emergence/pack.toml",
                "name = \"emergence\"\nversion = \"2\"\n",
            ),
        ]);
        let world = fetch(&remote, &OFFERED[0], &home).unwrap();
        assert_eq!(world, home.join("New Dereth").join("world.toml"));
        assert!(crate::lyceum_world::is_repository(&world));
        assert_eq!(
            std::fs::read_to_string(home.join("New Dereth/base/emergence/pack.toml")).unwrap(),
            "name = \"emergence\"\nversion = \"2\"\n",
            "the newest commit's"
        );
        let f = fetched(&home.join("New Dereth")).unwrap();
        assert_eq!(f.commit, format!("{:040x}", 2));
        assert_eq!(f.files.len(), 2);
        assert!(changes(&home.join("New Dereth"), &f).unwrap().is_empty());
        let listed = crate::components::list_worlds(Some(&home), &[]);
        assert!(
            listed.iter().any(|(w, mine)| *w == world && *mine),
            "{listed:?}"
        );
        // A second fetch finds the folder there.
        assert!(fetch(&remote, &OFFERED[0], &home)
            .unwrap_err()
            .contains("already"));
        let _ = std::fs::remove_dir_all(&home);
    }

    /// An update takes the branch's newest commit when every file is as fetched (a file the new
    /// commit drops goes), says so when there is nothing newer, and refuses, naming the files and
    /// touching nothing, when any was changed or added.
    #[test]
    fn an_update_fast_forwards_an_unchanged_folder_and_refuses_one_with_local_changes() {
        let home = home("update");
        let remote = Fake::new(vec![
            ("world.toml", WORLD),
            ("base/emergence/old.toml", "[[weenie]]\nclass = 1\n"),
        ]);
        let world = fetch(&remote, &OFFERED[0], &home).unwrap();
        let dir = world.parent().unwrap();
        assert_eq!(
            update(&remote, dir).unwrap(),
            Updated::Current {
                commit: format!("{:040x}", 1)
            }
        );
        remote.commit(vec![
            ("world.toml", WORLD),
            ("base/emergence/new.toml", "[[weenie]]\nclass = 2\n"),
        ]);
        // A change of the player's own stops it, and nothing is touched.
        std::fs::write(
            dir.join("base/emergence/mine.toml"),
            "[[weenie]]\nclass = 3\n",
        )
        .unwrap();
        let refused = update(&remote, dir).unwrap_err();
        assert!(refused.contains("base/emergence/mine.toml"), "{refused}");
        assert!(dir.join("base/emergence/old.toml").exists());
        assert_eq!(fetched(dir).unwrap().commit, format!("{:040x}", 1));
        std::fs::remove_file(dir.join("base/emergence/mine.toml")).unwrap();

        assert_eq!(
            update(&remote, dir).unwrap(),
            Updated::Moved {
                from: format!("{:040x}", 1),
                to: format!("{:040x}", 2)
            }
        );
        assert!(dir.join("base/emergence/new.toml").exists());
        assert!(
            !dir.join("base/emergence/old.toml").exists(),
            "dropped upstream"
        );
        assert_eq!(fetched(dir).unwrap().commit, format!("{:040x}", 2));
        let _ = std::fs::remove_dir_all(&home);
    }

    /// The commit a branch is at is read from git's list of refs, the first line's length and
    /// capabilities aside.
    #[test]
    fn a_branchs_commit_is_read_from_the_list_of_refs() {
        let refs = "001e# service=git-upload-pack\n0000015534e6d0f1b6d2cbf6a0b1f9c2d0e0a1b2c3d4e5f6 HEAD\0multi_ack side-band-64k\n003f6283f6d0a1b2c3d4e5f60718293a4b5c6d7e8f90 refs/heads/main\n003fabcdefabcdefabcdefabcdefabcdefabcdefab refs/heads/other\n0000";
        assert_eq!(
            head_in(refs, "main").as_deref(),
            Some("6283f6d0a1b2c3d4e5f60718293a4b5c6d7e8f90")
        );
        assert_eq!(head_in(refs, "missing"), None);
        // An empty repository lists no refs, only its capabilities.
        let empty = "001e# service=git-upload-pack\n000001480000000000000000000000000000000000000000 capabilities^{}\0multi_ack\n0000";
        assert_eq!(head_in(empty, "main"), None);
    }
}
