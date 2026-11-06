//! Not ACE: which data files the server opened, and whether they are the ones its world's era
//! expects (V477).
//!
//! Each file is told apart by the iteration it carries: a file from before Throne of Destiny keeps
//! it in its header, a later file in its iteration list, and both are read without walking the
//! file. The small files of 1999 are also hashed, so the log can name the capture they are; a
//! later set is too large to hash at every start and is named by its iterations alone.
//!
//! An era names the files it expects ([`EraId::expected_dats`]): Launch the 1999 pair, February
//! 2005 the classic pair from after September 2004, the end of retail the later set. A world on
//! other files does not start unless its operator says so (`[era] any_dat_files`); the server
//! logs what it found either way. A world with a data overlay is checked against the base its
//! overlay was made on, not against its era.

use std::io::Read;
use std::path::Path;

use dereth_dat::{ClassicDat, ContainerEra, DatFile, ModernDat};
use empyrean_common::era::EraId;
use sha2::{Digest, Sha256};

/// A file is hashed only when it is no larger than this: the 1999 files are about 95 and 43 MB.
pub const HASHED_FILE_LIMIT: u64 = 128 * 1024 * 1024;

/// One 1999 file the log names by its hash: its file, capture date and SHA-256.
struct KnownFile {
    file: &'static str,
    capture: &'static str,
    sha256: &'static str,
}

const KNOWN_FILES: [KnownFile; 3] = [
    KnownFile {
        file: "portal.dat",
        capture: "the retail CD's of October 9, 1999",
        sha256: "195e6fbb6d1cb9c849b2f4732c29509b5f20f67038a1d0763041387287c1975a",
    },
    KnownFile {
        file: "portal.dat",
        capture: "October 31, 1999",
        sha256: "778f8d927d4fb82fc69bf03e06576a04fff3529d610a08debc0b863abddf80ff",
    },
    KnownFile {
        file: "cell.dat",
        capture: "the retail CD's of October 9, 1999",
        sha256: "111855655c7b1daec563a96476e29cf10f17a8252a50f6102f1245146e0870b8",
    },
];

/// One opened file: its name, the iteration it carries, and its SHA-256 when it was hashed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileFingerprint {
    pub file: &'static str,
    pub iteration: Option<u32>,
    pub sha256: Option<String>,
}

impl FileFingerprint {
    /// The capture a 1999 file is, by its hash.
    #[must_use]
    pub fn capture(&self) -> Option<&'static str> {
        let sha = self.sha256.as_deref()?;
        KNOWN_FILES
            .iter()
            .find(|k| k.file == self.file && k.sha256.eq_ignore_ascii_case(sha))
            .map(|k| k.capture)
    }

    /// `portal.dat iteration 105 (October 31, 1999)`, as the log names it.
    #[must_use]
    pub fn describe(&self) -> String {
        let iteration = self.iteration.map_or_else(
            || "no readable iteration".to_owned(),
            |n| format!("iteration {n}"),
        );
        match (self.capture(), &self.sha256) {
            (Some(capture), _) => format!("{} {iteration} ({capture})", self.file),
            (None, Some(sha)) => format!("{} {iteration} (SHA-256 {sha})", self.file),
            (None, None) => format!("{} {iteration}", self.file),
        }
    }
}

/// The set the server opened: which kind, and its portal and cell files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatFingerprint {
    pub container: ContainerEra,
    pub portal: FileFingerprint,
    pub cell: FileFingerprint,
}

impl DatFingerprint {
    /// The files, in words, for the log.
    #[must_use]
    pub fn describe(&self) -> String {
        format!("{}, {}", self.portal.describe(), self.cell.describe())
    }

    /// Whether these are the files `era` expects.
    #[must_use]
    pub fn suits(&self, era: EraId) -> bool {
        era.expected_dats()
            .admits(self.container, self.portal.iteration, self.cell.iteration)
    }
}

/// The portal and cell files of the `container` set in `dir`, fingerprinted.
#[must_use]
pub fn fingerprint(dir: &Path, container: ContainerEra) -> DatFingerprint {
    let (portal, cell) = match container {
        ContainerEra::Classic => (
            file(
                ClassicDat::Portal.file_name(),
                &ClassicDat::Portal.in_dir(dir),
            ),
            file(ClassicDat::Cell.file_name(), &ClassicDat::Cell.in_dir(dir)),
        ),
        ContainerEra::Modern => (
            file(
                ModernDat::Portal.file_name(),
                &ModernDat::Portal.in_dir(dir),
            ),
            file(ModernDat::Cell.file_name(), &ModernDat::Cell.in_dir(dir)),
        ),
    };
    DatFingerprint {
        container,
        portal,
        cell,
    }
}

fn file(name: &'static str, path: &Path) -> FileFingerprint {
    let iteration = DatFile::read_iterations(path).ok().map(|i| i.highest);
    let small = std::fs::metadata(path).is_ok_and(|m| m.len() <= HASHED_FILE_LIMIT);
    FileFingerprint {
        file: name,
        iteration,
        sha256: small.then(|| sha256_of(path)).flatten(),
    }
}

fn sha256_of(path: &Path) -> Option<String> {
    let mut input = std::fs::File::open(path).ok()?;
    let mut hash = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = input.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    Some(hash.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// What a world of `era` on `files` does at start-up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// The files are the era's.
    Suits,
    /// They are not, and the operator allowed it: the line to warn with.
    Allowed(String),
    /// They are not: the line to refuse with.
    Refused(String),
}

/// Whether a world of `era` starts on `files`; `any_dat_files` is the operator's `[era]` setting.
#[must_use]
pub fn verdict(era: EraId, files: &DatFingerprint, any_dat_files: bool) -> Verdict {
    if files.suits(era) {
        return Verdict::Suits;
    }
    let expected = expectation(era);
    if any_dat_files {
        Verdict::Allowed(format!(
            "The {era} era expects {expected}, and these are {}. Starting anyway: [era] any_dat_files is set.",
            files.describe()
        ))
    } else {
        Verdict::Refused(format!(
            "The {era} era expects {expected}, and these are {}. Point dat_files_directory at the era's files, or set any_dat_files = true in [era] to start anyway.",
            files.describe()
        ))
    }
}

/// The files an era expects, in words.
fn expectation(era: EraId) -> String {
    let expected = era.expected_dats();
    let range = |file: &str, r: Option<(u32, u32)>| match r {
        None => None,
        Some((lo, u32::MAX)) => Some(format!("{file} at iteration {lo} or later")),
        Some((lo, hi)) => Some(format!("{file} at iterations {lo} to {hi}")),
    };
    let set = match expected.container {
        ContainerEra::Classic => "the files from before Throne of Destiny",
        ContainerEra::Modern => "the files from Throne of Destiny on",
    };
    let ranges: Vec<String> = [
        range("portal.dat", expected.portal),
        range("cell.dat", expected.cell),
    ]
    .into_iter()
    .flatten()
    .collect();
    if ranges.is_empty() {
        set.to_owned()
    } else {
        format!("{set} ({})", ranges.join(", "))
    }
}
