//! The list an art pack source publishes, art.json: one JSON file at a stable HTTPS address
//! listing many art packs, each at its latest version, with where its packed file is and the file's
//! SHA-256; read, written, and a downloaded file checked against it.
//!
//! **Depends on** the asset formats (`dereth-assets`) for a pack's manifest and a packed file's
//! contents, and `serde_json` for the list. **Used by** the launcher's knowledge (`dereth-launch`),
//! which installs packs from sources, and the repository's tasks (`xtask`), which write Dereth's own
//! source.
//!
//! **Must never** read or write a file, or reach the network: its callers fetch and keep.
//!
//! `docs/art/sources.md` is the format's specification.
//!
//! ```json
//! {
//!   "format": 1,
//!   "name": "Dereth",
//!   "packs": [
//!     {
//!       "id": "network.dereth.remastered",
//!       "name": "Remastered",
//!       "version": "1.0.0",
//!       "description": "Trees, plants and rocks made again.",
//!       "authors": ["..."],
//!       "license": "CC-BY-4.0",
//!       "made_for": ["end-of-retail"],
//!       "file": "network.dereth.remastered/network.dereth.remastered-1.0.0-0123456789ab.dat",
//!       "sha256": "...",
//!       "size": 1176519680,
//!       "hash": "...",
//!       "released": "2026-11-08"
//!     }
//!   ]
//! }
//! ```
//!
//! The list is read leniently and checked strictly, as a plugin source's is. A key this reader
//! does not know is ignored, an entry that does not read is set aside with the reason and the
//! others are kept, and a list of a newer `format` is refused whole. A downloaded file is taken
//! only when it is the file the entry names (its size and SHA-256: [`Entry::check_download`]) and
//! the pack in it says what the entry says (its id, its version and the hash of what it was packed
//! from: [`Entry::check_packed`]).
//!
//! Where the file is, `file`, is an `https://` URL or a path relative to the list's own address.
//! Resolving it is the reader's.

use std::fmt::{self, Write as _};

use dereth_assets::art_pack::{is_pack_id, is_pack_version, About, ArtPackManifest, MADE_FOR};
use dereth_assets::packed_art::Packed;

/// The list's file name.
pub const FILE_NAME: &str = "art.json";

/// Dereth's own source: the art packs Dereth publishes, each packed file beside the list. The one
/// source a launcher starts with, and the only one ids under [`FIRST_PARTY_PREFIX`] install from.
pub const DERETH_SOURCE: &str = "https://art.dereth.network/art.json";

/// The ids that install only from [`DERETH_SOURCE`]: Dereth's own packs.
pub const FIRST_PARTY_PREFIX: &str = "network.dereth.";

/// The format this reader reads and writes. A list of a higher one is refused.
pub const FORMAT: i64 = 1;

/// The largest list a reader reads.
pub const MAX_BYTES: usize = 4 << 20;

/// The largest packed pack: a packed file's blocks lie below 2 GiB. A larger pack is split into
/// several, stacked.
pub const MAX_PACK_BYTES: u64 = 0x8000_0000;

/// One source's list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Catalog {
    /// The source's name, for the player; empty when it gives none.
    pub name: String,
    /// The packs it lists, one entry each, by id.
    pub packs: Vec<Entry>,
}

/// One pack's latest version, as its source lists it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Entry {
    pub id: String,
    pub name: String,
    /// `MAJOR.MINOR.PATCH`.
    pub version: String,
    pub description: String,
    pub authors: Vec<String>,
    pub license: String,
    pub homepage: String,
    /// The data its ids are chosen from ([`MADE_FOR`]).
    pub made_for: Vec<String>,
    /// Where the packed file is: an `https://` URL, or a path relative to the list's address.
    pub file: String,
    /// The file's SHA-256, lower-case hex.
    pub sha256: String,
    /// The file's size in bytes.
    pub size: u64,
    /// The hash of the folder the pack was packed from, as its packed file names it; empty when
    /// the list does not say.
    pub hash: String,
    /// What changed in this version, for the player.
    pub notes: String,
    /// The day it was published, `YYYY-MM-DD`; empty when the list does not say.
    pub released: String,
}

/// A list as read: the entries that read, and why each of the others did not.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Read {
    pub catalog: Catalog,
    /// One line per entry set aside.
    pub skipped: Vec<String>,
}

/// Why a list was refused whole.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogError {
    /// Larger than [`MAX_BYTES`].
    TooLarge,
    /// Not JSON, or not an object with a `format` and a list of `packs`.
    NotAList(String),
    /// A format this reader does not know.
    NewerFormat(i64),
}

impl fmt::Display for CatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge => write!(f, "the list is larger than {} MiB", MAX_BYTES >> 20),
            Self::NotAList(e) => write!(f, "not an art pack list: {e}"),
            Self::NewerFormat(n) => write!(
                f,
                "the list is in format {n}, newer than this Dereth reads ({FORMAT}); update Dereth"
            ),
        }
    }
}

impl std::error::Error for CatalogError {}

/// Why a downloaded file is not the pack its entry lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mismatch {
    /// Not the size the entry says.
    Size { expected: u64, got: u64 },
    /// Not the file the entry's SHA-256 names.
    Hash { expected: String, got: String },
    /// Not a packed pack, or one whose manifest does not read.
    NotAPack(String),
    /// The pack says otherwise than the entry: which field, what the entry says and what the
    /// pack says.
    Says {
        field: &'static str,
        listed: String,
        pack: String,
    },
}

impl fmt::Display for Mismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Size { expected, got } => write!(
                f,
                "the download is {got} bytes, not the {expected} its source lists"
            ),
            Self::Hash { expected, got } => write!(
                f,
                "the download's SHA-256 is {got}, not the {expected} its source lists"
            ),
            Self::NotAPack(e) => write!(f, "the download is not an art pack: {e}"),
            Self::Says {
                field,
                listed,
                pack,
            } => write!(
                f,
                "the pack's {field} is {pack}, but its source lists {listed}"
            ),
        }
    }
}

impl std::error::Error for Mismatch {}

/// A version's three numbers, for ordering; `None` for anything but `MAJOR.MINOR.PATCH`.
#[must_use]
pub fn version_key(version: &str) -> Option<(u64, u64, u64)> {
    if !is_pack_version(version) {
        return None;
    }
    let mut n = version.split('.').map(|p| p.parse::<u64>().ok());
    Some((n.next()??, n.next()??, n.next()??))
}

/// Whether version `theirs` comes after version `ours`; false when either is not a version.
#[must_use]
pub fn is_newer(theirs: &str, ours: &str) -> bool {
    matches!((version_key(theirs), version_key(ours)), (Some(t), Some(o)) if t > o)
}

/// Whether pack `id` installs only from Dereth's own source.
#[must_use]
pub fn first_party_only(id: &str) -> bool {
    id.starts_with(FIRST_PARTY_PREFIX)
}

impl Entry {
    /// The entry listing the packed pack `packed`, whose file is `file` (as the list will name it),
    /// `size` bytes with SHA-256 `sha256`. What the pack's manifest leaves out of its `[pack]`
    /// listing keys is taken from `given` (a source's own word for a pack packed before it named
    /// them); where both name an id or a version, they must agree.
    ///
    /// # Errors
    /// The pack's manifest will not read, it and `given` disagree, or neither names an id and a
    /// version.
    pub fn for_packed(
        packed: &Packed,
        given: &About,
        file: &str,
        size: u64,
        sha256: &str,
        notes: &str,
        released: &str,
    ) -> Result<Self, String> {
        let mut m = ArtPackManifest::parse(&packed.manifest).map_err(|e| e.0)?;
        for (field, ours, theirs) in [
            ("id", &m.about.id, &given.id),
            ("version", &m.about.version, &given.version),
        ] {
            if !ours.is_empty() && !theirs.is_empty() && ours != theirs {
                return Err(format!(
                    "its manifest's {field} is {ours}, and the source gives {theirs}"
                ));
            }
        }
        let fill = |ours: &mut String, theirs: &String| {
            if ours.is_empty() {
                ours.clone_from(theirs);
            }
        };
        fill(&mut m.about.id, &given.id);
        fill(&mut m.about.version, &given.version);
        fill(&mut m.about.description, &given.description);
        fill(&mut m.about.license, &given.license);
        fill(&mut m.about.homepage, &given.homepage);
        if m.about.authors.is_empty() {
            m.about.authors.clone_from(&given.authors);
        }
        if m.about.made_for.is_empty() {
            m.about.made_for.clone_from(&given.made_for);
        }
        if !is_pack_id(&m.about.id) {
            return Err("neither its manifest's [pack] nor the source names an id".into());
        }
        if !is_pack_version(&m.about.version) {
            return Err("neither its manifest's [pack] nor the source names a version".into());
        }
        if size > MAX_PACK_BYTES {
            return Err(format!(
                "it is {size} bytes, larger than a packed pack can be ({MAX_PACK_BYTES})"
            ));
        }
        Ok(Self {
            name: if m.name.trim().is_empty() {
                m.about.id.clone()
            } else {
                m.name.trim().to_owned()
            },
            id: m.about.id,
            version: m.about.version,
            description: m.about.description,
            authors: m.about.authors,
            license: m.about.license,
            homepage: m.about.homepage,
            made_for: m.about.made_for,
            file: file.to_owned(),
            sha256: sha256.to_ascii_lowercase(),
            size,
            hash: packed.contents.hash.clone(),
            notes: notes.trim().to_owned(),
            released: released.to_owned(),
        })
    }

    /// Whether a download of `size` bytes with SHA-256 `sha256` is the file the entry names.
    ///
    /// # Errors
    /// Its size, then its SHA-256, when either is not the entry's.
    pub fn check_download(&self, size: u64, sha256: &str) -> Result<(), Mismatch> {
        if size != self.size {
            return Err(Mismatch::Size {
                expected: self.size,
                got: size,
            });
        }
        if !sha256.eq_ignore_ascii_case(&self.sha256) {
            return Err(Mismatch::Hash {
                expected: self.sha256.clone(),
                got: sha256.to_ascii_lowercase(),
            });
        }
        Ok(())
    }

    /// Whether the packed pack `packed` is the pack the entry lists: its manifest's id and
    /// version where it names them, and the hash of what it was packed from where the entry names
    /// one. A pack whose manifest names neither is taken only by that hash.
    ///
    /// # Errors
    /// The manifest will not read, or says otherwise than the entry, or names no id and the entry
    /// no hash.
    pub fn check_packed(&self, packed: &Packed) -> Result<(), Mismatch> {
        let m = ArtPackManifest::parse(&packed.manifest)
            .map_err(|e| Mismatch::NotAPack(format!("its manifest will not read: {}", e.0)))?;
        let says = |field, listed: &str, pack: &str| {
            if listed == pack {
                Ok(())
            } else {
                Err(Mismatch::Says {
                    field,
                    listed: listed.to_owned(),
                    pack: if pack.is_empty() {
                        "not named".to_owned()
                    } else {
                        pack.to_owned()
                    },
                })
            }
        };
        if !m.about.id.is_empty() || self.hash.is_empty() {
            says("id", &self.id, &m.about.id)?;
        }
        if !m.about.version.is_empty() || self.hash.is_empty() {
            says("version", &self.version, &m.about.version)?;
        }
        if !self.hash.is_empty() {
            says("content hash", &self.hash, &packed.contents.hash)?;
        }
        Ok(())
    }

    fn write_lines(&self, out: &mut String) {
        let mut fields: Vec<(&str, String)> = vec![
            ("id", json_str(&self.id)),
            ("name", json_str(&self.name)),
            ("version", json_str(&self.version)),
        ];
        let mut text = |k, v: &str| {
            if !v.is_empty() {
                fields.push((k, json_str(v)));
            }
        };
        text("description", &self.description);
        if !self.authors.is_empty() {
            fields.push(("authors", json_list(&self.authors)));
        }
        let mut text = |k, v: &str| {
            if !v.is_empty() {
                fields.push((k, json_str(v)));
            }
        };
        text("license", &self.license);
        text("homepage", &self.homepage);
        if !self.made_for.is_empty() {
            fields.push(("made_for", json_list(&self.made_for)));
        }
        fields.push(("file", json_str(&self.file)));
        fields.push(("sha256", json_str(&self.sha256)));
        fields.push(("size", self.size.to_string()));
        let mut text = |k, v: &str| {
            if !v.is_empty() {
                fields.push((k, json_str(v)));
            }
        };
        text("hash", &self.hash);
        text("notes", &self.notes);
        text("released", &self.released);
        out.push_str("    {\n");
        for (i, (k, v)) in fields.iter().enumerate() {
            let comma = if i + 1 < fields.len() { "," } else { "" };
            let _ = writeln!(out, "      {}: {v}{comma}", json_str(k));
        }
        out.push_str("    }");
    }
}

fn json_str(s: &str) -> String {
    serde_json::Value::String(s.to_owned()).to_string()
}

fn json_list(list: &[String]) -> String {
    let items: Vec<String> = list.iter().map(|s| json_str(s)).collect();
    format!("[{}]", items.join(", "))
}

type Object = serde_json::Map<String, serde_json::Value>;

fn text(e: &Object, key: &str) -> Result<Option<String>, String> {
    match e.get(key) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(s)) => Ok(Some(s.trim().to_owned())),
        Some(_) => Err(format!("{key} is not text")),
    }
}

fn required(e: &Object, key: &str) -> Result<String, String> {
    text(e, key)?
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("no {key}"))
}

fn text_list(e: &Object, key: &str) -> Result<Vec<String>, String> {
    match e.get(key) {
        None | Some(serde_json::Value::Null) => Ok(Vec::new()),
        Some(serde_json::Value::Array(l)) => l
            .iter()
            .map(|v| {
                v.as_str()
                    .map(|s| s.trim().to_owned())
                    .ok_or_else(|| format!("{key} is not a list of text"))
            })
            .collect(),
        Some(_) => Err(format!("{key} is not a list of text")),
    }
}

fn entry(e: &serde_json::Value) -> Result<Entry, String> {
    let e = e.as_object().ok_or("not an object")?;
    let id = required(e, "id")?;
    if !is_pack_id(&id) {
        return Err(format!("{id:?} is not an art pack id"));
    }
    let version = required(e, "version")?;
    if !is_pack_version(&version) {
        return Err(format!("{version:?} is not a version (MAJOR.MINOR.PATCH)"));
    }
    let sha = required(e, "sha256")?.to_ascii_lowercase();
    if sha.len() != 64 || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("sha256 is not 64 hexadecimal digits".into());
    }
    let size = e
        .get("size")
        .and_then(serde_json::Value::as_u64)
        .ok_or("no size, or it is not a whole number of bytes")?;
    if size == 0 || size > MAX_PACK_BYTES {
        return Err(format!(
            "size {size} is not a packed pack's (1 byte to {MAX_PACK_BYTES})"
        ));
    }
    let made_for: Vec<String> = text_list(e, "made_for")?
        .into_iter()
        .filter(|m| MADE_FOR.contains(&m.as_str()))
        .collect();
    Ok(Entry {
        name: text(e, "name")?
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| id.clone()),
        id,
        version,
        description: text(e, "description")?.unwrap_or_default(),
        authors: text_list(e, "authors")?,
        license: text(e, "license")?.unwrap_or_default(),
        homepage: text(e, "homepage")?.unwrap_or_default(),
        made_for,
        file: required(e, "file")?,
        sha256: sha,
        size,
        hash: text(e, "hash")?.unwrap_or_default(),
        notes: text(e, "notes")?.unwrap_or_default(),
        released: text(e, "released")?.unwrap_or_default(),
    })
}

impl Catalog {
    /// The list in `bytes`.
    ///
    /// # Errors
    /// [`CatalogError`] when the list is refused whole; entries that do not read are in
    /// [`Read::skipped`].
    pub fn parse(bytes: &[u8]) -> Result<Read, CatalogError> {
        if bytes.len() > MAX_BYTES {
            return Err(CatalogError::TooLarge);
        }
        let text = std::str::from_utf8(bytes)
            .map_err(|_| CatalogError::NotAList("the text is not UTF-8".into()))?;
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let doc: serde_json::Value =
            serde_json::from_str(text).map_err(|e| CatalogError::NotAList(e.to_string()))?;
        let Some(doc) = doc.as_object() else {
            return Err(CatalogError::NotAList("it is not a JSON object".into()));
        };
        let format = doc
            .get("format")
            .and_then(serde_json::Value::as_i64)
            .ok_or_else(|| CatalogError::NotAList("it names no format".into()))?;
        if format > FORMAT {
            return Err(CatalogError::NewerFormat(format));
        }
        if format < 1 {
            return Err(CatalogError::NotAList(format!("format {format}")));
        }
        let list = doc
            .get("packs")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| CatalogError::NotAList("it has no list of packs".into()))?;
        let mut read = Read {
            catalog: Catalog {
                name: doc
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .trim()
                    .to_owned(),
                packs: Vec::new(),
            },
            skipped: Vec::new(),
        };
        for (i, e) in list.iter().enumerate() {
            let label = e
                .get("id")
                .and_then(serde_json::Value::as_str)
                .map_or_else(|| format!("pack {}", i + 1), str::to_owned);
            match entry(e) {
                Ok(entry) if read.catalog.packs.iter().any(|p| p.id == entry.id) => {
                    read.skipped
                        .push(format!("{label}: listed more than once; the first is kept"));
                }
                Ok(entry) => read.catalog.packs.push(entry),
                Err(why) => read.skipped.push(format!("{label}: {why}")),
            }
        }
        Ok(read)
    }

    /// The entry for pack `id`.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&Entry> {
        self.packs.iter().find(|p| p.id == id)
    }

    /// The list as `art.json`: its entries in id order, two spaces a level, a field a line.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut packs: Vec<&Entry> = self.packs.iter().collect();
        packs.sort_by(|a, b| a.id.cmp(&b.id));
        let mut out = format!("{{\n  \"format\": {FORMAT},\n");
        if !self.name.is_empty() {
            let _ = writeln!(out, "  \"name\": {},", json_str(&self.name));
        }
        if packs.is_empty() {
            out.push_str("  \"packs\": []\n}\n");
            return out;
        }
        out.push_str("  \"packs\": [\n");
        for (i, p) in packs.iter().enumerate() {
            p.write_lines(&mut out);
            out.push_str(if i + 1 < packs.len() { ",\n" } else { "\n" });
        }
        out.push_str("  ]\n}\n");
        out
    }
}

#[cfg(test)]
mod tests;
