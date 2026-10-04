//! Dat sets: a folder of the game's data files, Modern or Classic ([`SetKind`]).
//!
//! A Modern set is the four later files, identified by what a world compares. A Classic set is the
//! pair from before Throne of Destiny, `portal.dat` and `cell.dat`, known by being the pair. One
//! folder may hold both: the names never collide, and the launcher lists them as two sets.
//!
//! **Identity is iterations, not file hashes.** Two correct end-of-retail sets can hash differently:
//! the 6096 and 4186 installs have the same `client_local_English.dat` contents in a different
//! block order. So a set is known by its four iteration counts, and a whole-file hash is used only to
//! verify a download against the hash its world published.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use crate::dat::{read_dat, ContainerEra, DatKind};

/// One count per file. Any of them may be unknown: a world that has not said, or a file that is
/// not there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Iterations {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub portal: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cell: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub highres: Option<u32>,
}

/// Which of the four files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DatRole {
    Portal,
    Cell,
    Local,
    Highres,
}

impl DatRole {
    pub const ALL: [DatRole; 4] = [
        DatRole::Portal,
        DatRole::Cell,
        DatRole::Local,
        DatRole::Highres,
    ];

    pub fn label(self) -> &'static str {
        match self {
            DatRole::Portal => "portal",
            DatRole::Cell => "cell",
            DatRole::Local => "local",
            DatRole::Highres => "highres",
        }
    }

    /// The file name the client opens. The local file is per language; English is the only one
    /// that shipped.
    pub fn file_name(self) -> &'static str {
        match self {
            DatRole::Portal => "client_portal.dat",
            DatRole::Cell => "client_cell_1.dat",
            DatRole::Local => "client_local_English.dat",
            DatRole::Highres => "client_highres.dat",
        }
    }
}

impl Iterations {
    /// What the retail servers ran with when they closed, and what every emulator ships expecting.
    pub const END_OF_RETAIL: Iterations = Iterations {
        portal: Some(2072),
        cell: Some(982),
        local: Some(994),
        highres: Some(497),
    };

    pub fn is_empty(&self) -> bool {
        self.portal.is_none()
            && self.cell.is_none()
            && self.local.is_none()
            && self.highres.is_none()
    }

    pub fn get(&self, role: DatRole) -> Option<u32> {
        match role {
            DatRole::Portal => self.portal,
            DatRole::Cell => self.cell,
            DatRole::Local => self.local,
            DatRole::Highres => self.highres,
        }
    }

    pub fn set(&mut self, role: DatRole, v: Option<u32>) {
        match role {
            DatRole::Portal => self.portal = v,
            DatRole::Cell => self.cell = v,
            DatRole::Local => self.local = v,
            DatRole::Highres => self.highres = v,
        }
    }

    /// `2072/982/994/497`, with a dash for an unknown.
    pub fn label(&self) -> String {
        DatRole::ALL
            .iter()
            .map(|r| {
                self.get(*r)
                    .map_or_else(|| "-".to_owned(), |n| n.to_string())
            })
            .collect::<Vec<_>>()
            .join("/")
    }

    pub fn is_end_of_retail(&self) -> bool {
        // highres is optional on the player's side: without it the set is still end of retail.
        self.portal == Self::END_OF_RETAIL.portal
            && self.cell == Self::END_OF_RETAIL.cell
            && self.local == Self::END_OF_RETAIL.local
            && self
                .highres
                .is_none_or(|h| Some(h) == Self::END_OF_RETAIL.highres)
    }

    /// Compare what the player has (`self`) with what a world expects.
    ///
    /// `roles` is which files the world actually compares: GDLE's check covers portal and cell
    /// only, and the launcher mirrors the server rather than being stricter than it.
    pub fn compare(&self, expected: &Iterations, roles: &[DatRole]) -> Comparison {
        let mut c = Comparison::default();
        for &role in roles {
            let Some(want) = expected.get(role) else {
                continue;
            };
            match self.get(role) {
                None => c.missing.push(role),
                Some(have) if have > want => c.newer.push(Mismatch { role, have, want }),
                Some(have) if have < want => c.older.push(Mismatch { role, have, want }),
                Some(_) => {}
            }
        }
        c
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mismatch {
    pub role: DatRole,
    pub have: u32,
    pub want: u32,
}

/// The result of [`Iterations::compare`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Comparison {
    pub newer: Vec<Mismatch>,
    pub older: Vec<Mismatch>,
    pub missing: Vec<DatRole>,
}

impl Comparison {
    pub fn matches(&self) -> bool {
        self.newer.is_empty() && self.older.is_empty() && self.missing.is_empty()
    }
}

/// One file of a set, as last read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatFileState {
    pub role: DatRole,
    pub file_name: String,
    pub size: u64,
    /// Seconds since the epoch. With the size, what decides whether a re-read is needed.
    pub modified: u64,
    pub read_only: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iterations: Option<u32>,
    /// Why the file could not be read, when it could not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Where a set came from, which decides which worlds may use it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DatOrigin {
    /// The default set of its kind: for a Modern set, the player's own install, used read-only by
    /// every world that neither patches nor ships its own.
    Shared,
    /// A private copy for one world, which may write to it.
    World { slug: String },
    /// A downloaded custom set, by the hash its world published.
    Custom { sha256: String },
    /// A folder the player added and has not assigned.
    Unassigned,
}

/// Which of the two sets the game shipped a folder holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SetKind {
    /// The files from Throne of Destiny on (`client_portal.dat`, `client_cell_1.dat`,
    /// `client_local_English.dat`, `client_highres.dat`): every world's set at the end of retail.
    /// A set saved before there were two kinds is one of these.
    #[default]
    Modern,
    /// The files from before Throne of Destiny, `portal.dat` and `cell.dat`, which always come as
    /// a pair: the world of an era before it, and the classic interface and looks for any other.
    Classic,
}

/// The two files of a Classic set, by the role each plays: the portal file and the cell file.
pub const CLASSIC_FILES: [(DatRole, &str); 2] =
    [(DatRole::Portal, "portal.dat"), (DatRole::Cell, "cell.dat")];

/// A dat set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatSet {
    pub id: String,
    pub path: PathBuf,
    /// Modern or Classic. Each kind has its own default ([`DatOrigin::Shared`]).
    #[serde(default)]
    pub kind: SetKind,
    pub origin: DatOrigin,
    pub files: Vec<DatFileState>,
    /// When a patching world last raised this set's iterations, detected after a session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_patched_by_server: Option<u64>,
    /// Whether the launcher made this set (and so may delete it) or the player pointed at it (and
    /// so it is only ever forgotten, never deleted).
    #[serde(default)]
    pub created_by_launcher: bool,
}

/// The id reserved for the shared end-of-retail set.
pub const SHARED_SET_ID: &str = "eor";

impl DatSet {
    pub fn iterations(&self) -> Iterations {
        let mut it = Iterations::default();
        for f in &self.files {
            it.set(f.role, f.iterations);
        }
        it
    }

    pub fn file(&self, role: DatRole) -> Option<&DatFileState> {
        self.files.iter().find(|f| f.role == role)
    }

    pub fn total_size(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }

    /// The world this set belongs to, if it is a private copy.
    pub fn owner_world(&self) -> Option<&str> {
        match &self.origin {
            DatOrigin::World { slug } => Some(slug),
            _ => None,
        }
    }

    /// Re-read the folder, reusing the stored reading of any file whose size and modification time
    /// are unchanged. That is what keeps the pre-launch check under a millisecond or two when
    /// nothing has moved.
    ///
    /// Returns whether any iteration rose, which after a session on a patching world means the
    /// world wrote to the set; `now` is recorded as the time it did.
    pub fn refresh(&mut self, now: u64) -> bool {
        if self.kind == SetKind::Classic {
            self.files = scan_classic_dir(&self.path);
            return false;
        }
        let before = self.iterations();
        self.files = scan_files(&self.path, &self.files);
        let after = self.iterations();
        let rose = DatRole::ALL
            .iter()
            .any(|r| matches!((before.get(*r), after.get(*r)), (Some(b), Some(a)) if a > b));
        if rose {
            self.last_patched_by_server = Some(now);
        }
        rose
    }
}

/// Read the four files of a folder. Files that are absent are simply not listed; files that are
/// present but unreadable are listed with their error, so the interface can say which one is wrong.
pub fn scan_dir(path: &Path) -> Vec<DatFileState> {
    scan_files(path, &[])
}

/// Read a folder's Classic pair, `portal.dat` and `cell.dat`: each file that is there, by its size,
/// time and the iteration its header keeps; a file that is not the one its name says is listed with
/// why. Empty when neither is there.
pub fn scan_classic_dir(dir: &Path) -> Vec<DatFileState> {
    CLASSIC_FILES
        .iter()
        .filter_map(|&(role, name)| {
            let path = dir.join(name);
            let meta = std::fs::metadata(&path).ok().filter(|m| m.is_file())?;
            let (iterations, error) = match read_dat(&path) {
                Ok(info) if info.era == ContainerEra::PreTod && kind_matches(role, info.kind) => {
                    (Some(info.iterations.count), None)
                }
                Ok(_) => (
                    None,
                    Some(format!(
                        "this is not the {} file from before Throne of Destiny",
                        role.label()
                    )),
                ),
                Err(e) => (None, Some(e.to_string())),
            };
            Some(DatFileState {
                role,
                file_name: name.to_owned(),
                size: meta.len(),
                modified: meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_secs()),
                read_only: meta.permissions().readonly(),
                iterations,
                error,
            })
        })
        .collect()
}

/// Whether a Classic set's files are the whole pair.
pub fn is_classic_pair(files: &[DatFileState]) -> bool {
    CLASSIC_FILES
        .iter()
        .all(|(role, _)| files.iter().any(|f| f.role == *role))
}

fn local_file_name(dir: &Path) -> String {
    // English first, as the client does; otherwise any other language's file.
    let english = DatRole::Local.file_name();
    if dir.join(english).exists() {
        return english.to_owned();
    }
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .find(|n| n.starts_with("client_local_") && n.ends_with(".dat"))
        .unwrap_or_else(|| english.to_owned())
}

fn scan_files(dir: &Path, previous: &[DatFileState]) -> Vec<DatFileState> {
    let mut out = Vec::new();
    for role in DatRole::ALL {
        let name = if role == DatRole::Local {
            local_file_name(dir)
        } else {
            role.file_name().to_owned()
        };
        let path = dir.join(&name);
        let Ok(meta) = std::fs::metadata(&path) else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        let size = meta.len();
        let modified = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs());
        let read_only = meta.permissions().readonly();
        if let Some(p) = previous
            .iter()
            .find(|p| p.role == role && p.file_name == name)
        {
            if p.size == size && p.modified == modified && p.error.is_none() {
                out.push(DatFileState {
                    read_only,
                    ..p.clone()
                });
                continue;
            }
        }
        let (iterations, error) = match read_dat(&path) {
            Ok(info) if info.era == ContainerEra::Tod && kind_matches(role, info.kind) => {
                (Some(info.iterations.count), None)
            }
            Ok(info) => (
                None,
                Some(format!(
                    "this is a {:?} file, not {}",
                    info.kind,
                    role.label()
                )),
            ),
            Err(e) => (None, Some(e.to_string())),
        };
        out.push(DatFileState {
            role,
            file_name: name,
            size,
            modified,
            read_only,
            iterations,
            error,
        });
    }
    out
}

fn kind_matches(role: DatRole, kind: DatKind) -> bool {
    matches!(
        (role, kind),
        (DatRole::Portal, DatKind::Portal)
            | (DatRole::Cell, DatKind::Cell { .. })
            | (DatRole::Local, DatKind::Local { .. })
            | (DatRole::Highres, DatKind::HighRes)
    )
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::dat::testdat::eor;

    /// A folder holding a synthetic set with the given counts.
    pub fn fake_set(dir: &Path, it: Iterations) {
        std::fs::create_dir_all(dir).unwrap();
        for (role, set, sub) in [
            (DatRole::Portal, 1, 0),
            (DatRole::Cell, 2, 1),
            (DatRole::Local, 3, 1),
            (DatRole::Highres, 1, 0x6946_6948),
        ] {
            if let Some(n) = it.get(role) {
                std::fs::write(dir.join(role.file_name()), eor(set, sub, n)).unwrap();
            }
        }
    }

    pub fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("dereth-launch-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn a_folder_reads_as_its_four_counts() {
        let d = tmp("scan");
        fake_set(&d, Iterations::END_OF_RETAIL);
        let set = DatSet {
            id: "x".into(),
            path: d.clone(),
            kind: SetKind::Modern,
            origin: DatOrigin::Unassigned,
            files: scan_dir(&d),
            last_patched_by_server: None,
            created_by_launcher: false,
        };
        assert_eq!(set.iterations(), Iterations::END_OF_RETAIL);
        assert!(set.iterations().is_end_of_retail());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_file_under_the_wrong_name_is_named_as_wrong() {
        let d = tmp("wrongname");
        std::fs::create_dir_all(&d).unwrap();
        // A cell file saved as the portal file.
        std::fs::write(d.join(DatRole::Portal.file_name()), eor(2, 1, 5)).unwrap();
        let files = scan_dir(&d);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].iterations, None);
        assert!(files[0].error.as_deref().unwrap().contains("not portal"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_rise_after_a_session_is_recorded_as_a_patch() {
        let d = tmp("patched");
        fake_set(&d, Iterations::END_OF_RETAIL);
        let mut set = DatSet {
            id: "w".into(),
            path: d.clone(),
            kind: SetKind::Modern,
            origin: DatOrigin::World {
                slug: "coldeve".into(),
            },
            files: scan_dir(&d),
            last_patched_by_server: None,
            created_by_launcher: true,
        };
        assert!(!set.refresh(100), "nothing changed");
        let mut newer = Iterations::END_OF_RETAIL;
        newer.portal = Some(2080);
        fake_set(&d, newer);
        // The rewrite can land in the same second at the same size; age the cached readings so
        // this tests the rise, not the file system's clock.
        set.files.iter_mut().for_each(|f| f.modified = 0);
        assert!(set.refresh(200));
        assert_eq!(set.last_patched_by_server, Some(200));
        assert_eq!(set.iterations().portal, Some(2080));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn comparison_sorts_each_file_into_newer_older_or_missing() {
        let have = Iterations {
            portal: Some(2080),
            cell: Some(900),
            local: Some(994),
            highres: None,
        };
        let c = have.compare(&Iterations::END_OF_RETAIL, &DatRole::ALL);
        assert_eq!(
            c.newer,
            [Mismatch {
                role: DatRole::Portal,
                have: 2080,
                want: 2072
            }]
        );
        assert_eq!(
            c.older,
            [Mismatch {
                role: DatRole::Cell,
                have: 900,
                want: 982
            }]
        );
        assert_eq!(c.missing, [DatRole::Highres]);
    }

    #[test]
    fn a_world_that_does_not_say_about_a_file_is_not_compared_on_it() {
        let expected = Iterations {
            portal: Some(2072),
            ..Default::default()
        };
        let have = Iterations {
            portal: Some(2072),
            cell: Some(1),
            ..Default::default()
        };
        assert!(have.compare(&expected, &DatRole::ALL).matches());
    }

    #[test]
    fn a_classic_pair_is_read_beside_a_modern_set_and_one_file_alone_is_not_a_pair() {
        let d = tmp("classic");
        fake_set(&d, Iterations::END_OF_RETAIL);
        std::fs::write(
            d.join("portal.dat"),
            crate::dat::testdat::pre_tod(false, 2112),
        )
        .unwrap();
        let one = scan_classic_dir(&d);
        assert_eq!(one.len(), 1);
        assert!(!is_classic_pair(&one));
        std::fs::write(d.join("cell.dat"), crate::dat::testdat::pre_tod(true, 1593)).unwrap();
        let mut set = DatSet {
            id: "c".into(),
            path: d.clone(),
            kind: SetKind::Classic,
            origin: DatOrigin::Unassigned,
            files: scan_classic_dir(&d),
            last_patched_by_server: None,
            created_by_launcher: false,
        };
        assert!(is_classic_pair(&set.files));
        assert_eq!(set.total_size(), 0x800 + 0x500);
        let it = set.iterations();
        assert_eq!(
            (it.portal, it.cell),
            (Some(2112), Some(1593)),
            "each file's header iteration"
        );
        assert!(set.files.iter().all(|f| f.error.is_none()));
        // A later file under an older one's name is named as wrong.
        std::fs::write(d.join("cell.dat"), eor(2, 1, 982)).unwrap();
        let wrong = scan_classic_dir(&d);
        let cell = wrong.iter().find(|f| f.role == DatRole::Cell).unwrap();
        assert_eq!(cell.iterations, None);
        assert!(cell
            .error
            .as_deref()
            .unwrap()
            .contains("before Throne of Destiny"));
        // And an older file under a later one's name.
        std::fs::write(
            d.join(DatRole::Portal.file_name()),
            crate::dat::testdat::pre_tod(false, 2112),
        )
        .unwrap();
        let later = scan_dir(&d);
        assert!(later
            .iter()
            .find(|f| f.role == DatRole::Portal)
            .unwrap()
            .error
            .is_some());
        fake_set(&d, Iterations::END_OF_RETAIL);
        assert_eq!(
            scan_dir(&d).len(),
            4,
            "the Modern files beside it are read as before"
        );
        std::fs::remove_file(d.join("cell.dat")).unwrap();
        assert!(!set.refresh(5), "a Classic set is never patched");
        assert!(!is_classic_pair(&set.files));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_set_saved_before_there_were_two_kinds_is_modern() {
        let s: DatSet = serde_json::from_str(
            r#"{"id":"eor","path":"C:/ac","origin":{"kind":"shared"},"files":[]}"#,
        )
        .unwrap();
        assert_eq!(s.kind, SetKind::Modern);
    }

    #[test]
    fn labels_show_unknowns_as_dashes() {
        assert_eq!(Iterations::END_OF_RETAIL.label(), "2072/982/994/497");
        assert_eq!(
            Iterations {
                portal: Some(1),
                ..Default::default()
            }
            .label(),
            "1/-/-/-"
        );
    }
}
