//! The data sets on offer and where each is kept: a JSON document, so the links can be changed
//! without a release of anything that reads it. The one built in is `sources.json` at the crate's
//! root.

use serde::{Deserialize, Serialize};

use crate::archive::Format;
use crate::mega::{FileRef, LinkError};

/// The document's version this reader understands.
pub const VERSION: u32 = 1;

/// The built-in list.
pub const BUILT_IN: &str = include_str!("../sources.json");

/// Every data set on offer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Catalogue {
    pub version: u32,
    /// Where a player can read how to get the files by hand, for when a download cannot be had.
    pub guide: String,
    pub sets: Vec<SetSource>,
}

/// Which files a set is: Asheron's Call's four later ones, or its pair from before Throne of
/// Destiny; or Asheron's Call 2's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SetKind {
    Modern,
    Classic,
    Auberean,
}

/// The iteration counts the set's files report; any may be left out.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
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

/// One data set on offer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetSource {
    /// Its id, also the name of the folder it is kept in.
    pub id: String,
    /// What the player is shown.
    pub label: String,
    pub kind: SetKind,
    /// The page that links to its files, for the player who wants to see where they come from.
    pub page: String,
    pub iterations: Iterations,
    pub archives: Vec<ArchiveSource>,
}

/// One archive of a set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveSource {
    pub format: Format,
    /// Its size in bytes: a link that answers another size has changed, and is not used.
    pub size: u64,
    /// Where it is kept: tried in order until one answers.
    pub mirrors: Vec<Mirror>,
    /// The files taken out of it; anything else in it is left.
    pub files: Vec<FileSource>,
    /// Whether the set is whole without it: when it cannot be had, the set ends without its files.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub optional: bool,
}

/// A link to an archive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mirror {
    pub link: String,
    /// The file's own key, for a file in a shared folder (whose link carries only the folder's).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

impl Mirror {
    pub fn file(&self) -> Result<FileRef, LinkError> {
        FileRef::parse(&self.link, self.key.as_deref())
    }
}

/// A file the set ends with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileSource {
    /// Its name, as the client opens it; the archive member of that name (in any folder of it).
    pub name: String,
    pub size: u64,
    /// Its SHA-256, lowercase hex. Without one, the archive's own checksum is the only check.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

/// Why a list could not be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogueError(pub String);

impl std::fmt::Display for CatalogueError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "the list of data sets could not be read: {}", self.0)
    }
}

impl std::error::Error for CatalogueError {}

impl Catalogue {
    /// The list built in.
    #[must_use]
    pub fn built_in() -> Catalogue {
        // Checked by this crate's tests; a broken one is a build that never shipped.
        Catalogue::parse(BUILT_IN).unwrap_or_else(|_| Catalogue {
            version: VERSION,
            guide: String::new(),
            sets: Vec::new(),
        })
    }

    /// Read a list, and check everything in it can be used: a version this understands, links
    /// that read, set ids that can be folder names, and well-formed hashes.
    pub fn parse(text: &str) -> Result<Catalogue, CatalogueError> {
        let c: Catalogue = serde_json::from_str(text).map_err(|e| CatalogueError(e.to_string()))?;
        if c.version != VERSION {
            return Err(CatalogueError(format!(
                "version {} (this reads {VERSION})",
                c.version
            )));
        }
        for set in &c.sets {
            let id_ok = !set.id.is_empty()
                && set.id.len() <= 64
                && set
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
            if !id_ok {
                return Err(CatalogueError(format!("set id {:?}", set.id)));
            }
            if set.archives.is_empty() {
                return Err(CatalogueError(format!("{}: no archives", set.id)));
            }
            for a in &set.archives {
                if a.mirrors.is_empty() || a.files.is_empty() {
                    return Err(CatalogueError(format!(
                        "{}: an archive with no links or no files",
                        set.id
                    )));
                }
                for m in &a.mirrors {
                    m.file()
                        .map_err(|e| CatalogueError(format!("{}: {}: {e}", set.id, m.link)))?;
                }
                for f in &a.files {
                    let name_ok = !f.name.is_empty()
                        && f.name
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.')
                        && f.name.to_ascii_lowercase().ends_with(".dat");
                    if !name_ok {
                        return Err(CatalogueError(format!(
                            "{}: file name {:?}",
                            set.id, f.name
                        )));
                    }
                    if let Some(h) = &f.sha256 {
                        if h.len() != 64
                            || !h
                                .bytes()
                                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                        {
                            return Err(CatalogueError(format!(
                                "{}: {}: a SHA-256 of 64 lowercase hex digits",
                                set.id, f.name
                            )));
                        }
                    }
                }
            }
        }
        Ok(c)
    }

    /// The set with id `id`.
    #[must_use]
    pub fn set(&self, id: &str) -> Option<&SetSource> {
        self.sets.iter().find(|s| s.id == id)
    }
}

impl SetSource {
    /// The bytes downloaded: every archive.
    #[must_use]
    pub fn download_bytes(&self) -> u64 {
        self.archives.iter().map(|a| a.size).sum()
    }

    /// The bytes the set's files take once unpacked.
    #[must_use]
    pub fn set_bytes(&self) -> u64 {
        self.archives
            .iter()
            .flat_map(|a| &a.files)
            .map(|f| f.size)
            .sum()
    }

    /// The most storage getting it takes at once: every archive is kept until its files are
    /// unpacked, then removed, so the peak is the last archive beside all the files.
    #[must_use]
    pub fn peak_bytes(&self) -> u64 {
        let mut files = 0u64;
        let mut peak = 0u64;
        for a in &self.archives {
            peak = peak.max(files + a.size + a.files.iter().map(|f| f.size).sum::<u64>());
            files += a.files.iter().map(|f| f.size).sum::<u64>();
        }
        peak
    }

    /// The name each archive is kept under while it is downloaded and unpacked.
    #[must_use]
    pub fn archive_names(&self) -> Vec<String> {
        self.archives
            .iter()
            .enumerate()
            .map(|(i, a)| format!("{}-{}.{}", self.id, i + 1, a.format.extension()))
            .collect()
    }

    /// The names of the files the set ends with.
    #[must_use]
    pub fn file_names(&self) -> Vec<String> {
        self.archives
            .iter()
            .flat_map(|a| &a.files)
            .map(|f| f.name.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the list of data sets the community's archive holds, and its sizes).
    use super::*;

    /// The built-in list reads, and offers the end-of-retail set, the February 2005 pair, the
    /// Launch pair and Asheron's Call 2's end-of-retail set, with the iterations the launcher
    /// matches them by.
    #[test]
    fn the_built_in_list_reads() {
        let c = Catalogue::parse(BUILT_IN).unwrap();
        assert_eq!(c.guide, "https://www.accpp.net/manual-installation");
        let eor = c.set("end-of-retail").unwrap();
        assert_eq!(eor.kind, SetKind::Modern);
        assert_eq!(
            (
                eor.iterations.portal,
                eor.iterations.cell,
                eor.iterations.local
            ),
            (Some(2072), Some(982), Some(994))
        );
        assert_eq!(
            eor.file_names(),
            [
                "client_portal.dat",
                "client_cell_1.dat",
                "client_local_English.dat",
                "client_highres.dat"
            ]
        );
        // The high-resolution textures come in an archive of their own, which the set is whole
        // without.
        assert_eq!(
            eor.archives.iter().map(|a| a.optional).collect::<Vec<_>>(),
            [false, true]
        );
        let feb = c.set("infiltration").unwrap();
        assert_eq!(feb.kind, SetKind::Classic);
        assert_eq!(
            (feb.iterations.portal, feb.iterations.cell),
            (Some(2112), Some(1593))
        );
        assert_eq!(feb.file_names(), ["portal.dat", "cell.dat"]);
        assert_eq!(
            feb.archive_names(),
            ["infiltration-1.7z", "infiltration-2.7z"]
        );
        let launch = c.set("launch").unwrap();
        assert_eq!(launch.kind, SetKind::Classic);
        assert_eq!(
            (launch.iterations.portal, launch.iterations.cell),
            (Some(102), Some(67))
        );
        assert_eq!(launch.file_names(), ["portal.dat", "cell.dat"]);
        let ac2 = c.set("auberean-end-of-retail").unwrap();
        assert_eq!(ac2.kind, SetKind::Auberean);
        assert_eq!(
            ac2.file_names(),
            [
                "portal.dat",
                "cell_1.dat",
                "local_English.dat",
                "highres.dat"
            ]
        );
        assert!(
            c.sets
                .iter()
                .flat_map(|s| s.archives.iter().flat_map(|a| &a.files))
                .all(|f| f.sha256.is_some()),
            "every file is checked by its hash"
        );
        assert_eq!(Catalogue::built_in(), c);
    }

    /// The storage a set needs at its peak is the most any archive takes beside the files unpacked
    /// up to it: the last archive beside every file, or an earlier, larger one beside its own.
    #[test]
    fn peak_storage_is_the_last_archive_beside_the_files() {
        let c = Catalogue::built_in();
        let eor = c.set("end-of-retail").unwrap();
        assert_eq!(
            eor.peak_bytes(),
            507_898_112 + 926_941_184 + 348_127_232 + 1_048_576
        );
        let feb = c.set("infiltration").unwrap();
        assert_eq!(feb.peak_bytes(), feb.set_bytes() + 33_566_695);
    }

    /// A list with a link that does not read, a bad id, a bad hash or another version is refused.
    #[test]
    fn refuses_a_list_it_cannot_use() {
        let good = BUILT_IN;
        for (from, to) in [
            ("\"version\": 1", "\"version\": 2"),
            ("\"id\": \"end-of-retail\"", "\"id\": \"../up\""),
            (
                "https://mega.nz/file/Q98n0BiR#",
                "https://example.com/file/Q98n0BiR#",
            ),
            (
                "dc6e500ba22e6b186db7171e3f3345238b6444c85d798adc85e550973b8d12e4",
                "DC6E",
            ),
            ("\"name\": \"cell.dat\"", "\"name\": \"../cell.dat\""),
        ] {
            assert!(good.contains(from), "{from}");
            assert!(
                Catalogue::parse(&good.replacen(from, to, 1)).is_err(),
                "{to}"
            );
        }
    }
}
