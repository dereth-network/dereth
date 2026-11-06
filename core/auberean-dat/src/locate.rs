//! Where an install's data files are (the `fs` feature). The one place that names the files: every
//! other crate and tool finds an install and its files through here.

use std::path::{Path, PathBuf};

use dereth_dat::{FileSet, ModernDat};

use crate::container::LAYOUT;

/// The data files of an install.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DatName {
    /// The portal file: everything shared by the whole world.
    Portal,
    /// The high-resolution pictures.
    HighRes,
    /// A cell file: the landscape and interiors of one region (1 or 2).
    Cell(u8),
    /// A language's file (`English`, `Deutsch`, `Francais`, `Japanese`, `Korean`, `Chinese`).
    Local(String),
}

impl DatName {
    /// The file's name in an install folder.
    #[must_use]
    pub fn file_name(&self) -> String {
        match self {
            Self::Portal => "portal.dat".into(),
            Self::HighRes => "highres.dat".into(),
            Self::Cell(n) => format!("cell_{n}.dat"),
            Self::Local(lang) => format!("local_{lang}.dat"),
        }
    }

    /// The file's path in an install folder.
    #[must_use]
    pub fn in_dir(&self, dir: &Path) -> PathBuf {
        dir.join(self.file_name())
    }

    /// The name of a file name, if it is one of an install's data files.
    #[must_use]
    pub fn parse(file_name: &str) -> Option<Self> {
        let lower = file_name.to_ascii_lowercase();
        let stem = lower.strip_suffix(".dat")?;
        match stem {
            "portal" => Some(Self::Portal),
            "highres" => Some(Self::HighRes),
            _ => {
                if let Some(n) = stem.strip_prefix("cell_") {
                    return n.parse().ok().map(Self::Cell);
                }
                stem.strip_prefix("local_")?;
                let lang = &file_name["local_".len()..file_name.len() - ".dat".len()];
                Some(Self::Local(lang.to_string()))
            }
        }
    }
}

/// The files a world's overlay lies over, by the role each plays: the first region's cell file and
/// the English file are the only ones of their kind it knows.
pub const FILE_SET: FileSet = FileSet::new(
    [
        Some("portal.dat"),
        Some("cell_1.dat"),
        Some("local_English.dat"),
        Some("highres.dat"),
    ],
    LAYOUT,
);

/// The name of the file that plays `role` in [`FILE_SET`].
#[must_use]
pub const fn role_file(role: ModernDat) -> &'static str {
    FILE_SET.holder_name(role)
}

/// Whether a folder holds an install (its portal file).
#[must_use]
pub fn holds_install(dir: &Path) -> bool {
    DatName::Portal.in_dir(dir).is_file()
}

/// The first of the folders offered that holds an install.
pub fn locate_install(candidates: impl IntoIterator<Item = PathBuf>) -> Option<PathBuf> {
    candidates.into_iter().find(|d| holds_install(d))
}

/// Every data file of an install folder, by name order: the containers named by [`DatName`]
/// (other `.dat` files an install carries, which are not containers, are left out).
pub fn install_dats(dir: &Path) -> std::io::Result<Vec<(DatName, PathBuf)>> {
    let mut v: Vec<(DatName, PathBuf)> = std::fs::read_dir(dir)?
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            Some((DatName::parse(&name)?, e.path()))
        })
        .collect();
    v.sort_by_key(|a| a.0.file_name());
    Ok(v)
}

/// Every file of an install folder named `*.dat`, by name, containers or not (an install also
/// carries `.dat` files that are not containers).
pub fn all_dat_files(dir: &Path) -> std::io::Result<Vec<(String, PathBuf)>> {
    let mut v: Vec<(String, PathBuf)> = std::fs::read_dir(dir)?
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            name.to_ascii_lowercase()
                .ends_with(".dat")
                .then(|| (name, e.path()))
        })
        .collect();
    v.sort();
    Ok(v)
}

/// The installs directly under a folder (each a folder holding a portal file), by name.
#[must_use]
pub fn installs_under(root: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| holds_install(p))
        .collect();
    v.sort();
    v
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (file naming)
    use super::*;

    #[test]
    fn every_name_parses_back_from_its_file_name() {
        for n in [
            DatName::Portal,
            DatName::HighRes,
            DatName::Cell(2),
            DatName::Local("English".into()),
        ] {
            assert_eq!(DatName::parse(&n.file_name()), Some(n));
        }
        assert_eq!(DatName::parse("Country.dat"), None);
        assert_eq!(DatName::parse("PORTAL.DAT"), Some(DatName::Portal));
    }
}
