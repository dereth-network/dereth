//! A plugin source's list, `plugins.json`: one JSON file at a stable HTTPS address listing many
//! plugins, each at its latest release only, with where its package's zip is and the zip's
//! SHA-256. `docs/plugins/sources.md` is the format's specification.
//!
//! The list is read leniently and checked strictly. A key this reader does not know is ignored
//! (fields are added without a new format), an entry that does not read is set aside with the
//! reason and the others are kept, and a list of a newer `format` is refused whole. A zip is
//! taken only when it is the file the entry names (its size and SHA-256) and its manifest says
//! what the entry says (the id, version, API version, permissions and web domains):
//! [`Entry::check`].

use std::fmt::{self, Write as _};

use dereth_plugin_api::manifest::{net_domain, PluginVersion};
use dereth_plugin_api::{ApiVersion, Manifest, PluginId, Value};
use sha2::{Digest, Sha256};

use crate::{source, Package};

/// The list's file name.
pub const FILE_NAME: &str = "plugins.json";

/// Dereth's own source: the plugins in Dereth's repository, on a host of their own with each zip
/// beside the list. The one source the launcher starts with, the one the web client installs from,
/// and the only one ids under `network.dereth.` install from.
pub const DERETH_SOURCE: &str = "https://plugins.dereth.network/plugins.json";

/// The format this reader reads and writes. A list of a higher one is refused: it changed in a
/// way this reader cannot follow.
pub const FORMAT: i64 = 1;

/// The largest list a host reads.
pub const MAX_BYTES: usize = 4 << 20;

/// The most JSON values a list may hold, counted as it is read.
const MAX_VALUES: usize = 500_000;

/// One source's list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Catalog {
    /// The source's name, for the player (`Virindi's plugins`); empty when it gives none.
    pub name: String,
    /// The plugins it lists, one entry each, by id.
    pub plugins: Vec<Entry>,
}

/// One plugin's latest release, as its source lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: PluginId,
    pub name: String,
    pub version: PluginVersion,
    /// The lowest API version the release needs.
    pub api: ApiVersion,
    pub description: String,
    pub authors: Vec<String>,
    pub homepage: String,
    /// The permissions its manifest asks for, by name, sorted.
    pub permissions: Vec<String>,
    /// The web domains its manifest names (`[net] domains`), sorted.
    pub domains: Vec<String>,
    /// Where the zip is: an `https://` URL, or a path relative to the list's own address.
    pub zip: String,
    /// The zip's SHA-256, lower-case hex.
    pub sha256: String,
    /// The zip's size in bytes, when the list says it.
    pub size: Option<u64>,
    /// What changed in this release, for the player.
    pub notes: String,
    /// The day it was released, `YYYY-MM-DD`; empty when the list does not say.
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
    /// Not JSON, or not an object with a `format` and a list of `plugins`.
    NotAList(String),
    /// A format this reader does not know.
    NewerFormat(i64),
}

impl fmt::Display for CatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge => write!(f, "the list is larger than {} MiB", MAX_BYTES >> 20),
            Self::NotAList(e) => write!(f, "not a plugin list: {e}"),
            Self::NewerFormat(n) => write!(
                f,
                "the list is in format {n}, newer than this Dereth reads ({FORMAT}); update Dereth"
            ),
        }
    }
}

impl std::error::Error for CatalogError {}

/// Why a downloaded zip is not the release its entry lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mismatch {
    /// Not the size the entry says.
    Size { expected: u64, got: u64 },
    /// Not the file the entry's SHA-256 names.
    Hash { expected: String, got: String },
    /// Not a package: not a zip, no manifest, or a manifest that does not read.
    NotAPackage(String),
    /// Its manifest says otherwise than the entry: which field, what the entry says and what the
    /// manifest says.
    Says {
        field: &'static str,
        listed: String,
        package: String,
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
            Self::NotAPackage(e) => write!(f, "the download is not a plugin package: {e}"),
            Self::Says {
                field,
                listed,
                package,
            } => write!(
                f,
                "the package's {field} is {package}, but its source lists {listed}"
            ),
        }
    }
}

impl std::error::Error for Mismatch {}

/// A list of names as one line, for messages.
fn names(list: &[String]) -> String {
    if list.is_empty() {
        "none".to_owned()
    } else {
        list.join(", ")
    }
}

/// The SHA-256 of `bytes`, lower-case hex.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    source::hex(&Sha256::digest(bytes))
}

/// The permissions a manifest asks for, by name, sorted.
#[must_use]
pub fn permission_names(m: &Manifest) -> Vec<String> {
    let mut out: Vec<String> = m.permissions.keys().map(ToString::to_string).collect();
    out.sort();
    out
}

/// The web domains a manifest names, sorted.
#[must_use]
pub fn domain_names(m: &Manifest) -> Vec<String> {
    let mut out = m.net_domains.clone();
    out.sort();
    out.dedup();
    out
}

impl Entry {
    /// The entry for a package's zip: what its manifest says, the zip's SHA-256 and size, and
    /// `zip`, where it will be downloaded from (an `https://` URL, or a path relative to the
    /// list). `notes` and `released` are the release's notes and day.
    ///
    /// # Errors
    /// The zip is not a package whose manifest reads.
    pub fn for_zip(
        bytes: &[u8],
        zip: &str,
        notes: &str,
        released: &str,
    ) -> Result<Self, crate::PackageError> {
        let package = Package::from_zip(zip, bytes)?;
        let m = package.manifest()?;
        if m.ubscript && !package.files.contains_key(crate::MANIFEST_FILE) {
            // A script with no manifest has no version to update by.
            return Err(crate::PackageError::NoManifest);
        }
        Ok(Self {
            id: m.id.clone(),
            name: m.name.clone(),
            version: m.version.clone(),
            api: m.api,
            description: m.description.clone(),
            authors: m.authors.clone(),
            homepage: m.homepage.clone(),
            permissions: permission_names(&m),
            domains: domain_names(&m),
            zip: zip.to_owned(),
            sha256: sha256_hex(bytes),
            size: Some(bytes.len() as u64),
            notes: notes.trim().to_owned(),
            released: released.to_owned(),
        })
    }

    /// Check a downloaded zip against the entry: its size (when listed) and SHA-256, that it is a
    /// package, and that its manifest's id, version, API version, permissions and web domains are
    /// the entry's. Answers the package and its manifest.
    ///
    /// # Errors
    /// The first way the zip differs from what the entry lists.
    pub fn check(&self, bytes: &[u8]) -> Result<(Package, Manifest), Mismatch> {
        let got = bytes.len() as u64;
        if let Some(expected) = self.size.filter(|s| *s != got) {
            return Err(Mismatch::Size { expected, got });
        }
        let got = sha256_hex(bytes);
        if got != self.sha256 {
            return Err(Mismatch::Hash {
                expected: self.sha256.clone(),
                got,
            });
        }
        let package = Package::from_zip(self.id.as_str(), bytes)
            .map_err(|e| Mismatch::NotAPackage(e.to_string()))?;
        let m = package
            .manifest()
            .map_err(|e| Mismatch::NotAPackage(e.to_string()))?;
        let says = |field, listed: String, package: String| {
            if listed == package {
                Ok(())
            } else {
                Err(Mismatch::Says {
                    field,
                    listed,
                    package,
                })
            }
        };
        says("id", self.id.to_string(), m.id.to_string())?;
        says("version", self.version.to_string(), m.version.to_string())?;
        says("API version", self.api.to_string(), m.api.to_string())?;
        says(
            "permissions",
            names(&self.permissions),
            names(&permission_names(&m)),
        )?;
        says(
            "web domains",
            names(&self.domains),
            names(&domain_names(&m)),
        )?;
        Ok((package, m))
    }

    /// Whether a host implementing `host` runs this release.
    #[must_use]
    pub fn runs_on(&self, host: ApiVersion) -> bool {
        host.runs(self.api)
    }

    fn write_lines(&self, out: &mut String) {
        let mut fields: Vec<(&str, String)> = vec![
            ("id", json_str(self.id.as_str())),
            ("name", json_str(&self.name)),
            ("version", json_str(&self.version.to_string())),
            ("api", json_str(&self.api.to_string())),
        ];
        if !self.description.is_empty() {
            fields.push(("description", json_str(&self.description)));
        }
        if !self.authors.is_empty() {
            fields.push(("authors", json_list(&self.authors)));
        }
        if !self.homepage.is_empty() {
            fields.push(("homepage", json_str(&self.homepage)));
        }
        fields.push(("permissions", json_list(&self.permissions)));
        fields.push(("domains", json_list(&self.domains)));
        fields.push(("zip", json_str(&self.zip)));
        fields.push(("sha256", json_str(&self.sha256)));
        if let Some(size) = self.size {
            fields.push(("size", size.to_string()));
        }
        if !self.notes.is_empty() {
            fields.push(("notes", json_str(&self.notes)));
        }
        if !self.released.is_empty() {
            fields.push(("released", json_str(&self.released)));
        }
        out.push_str("    {\n");
        for (i, (k, v)) in fields.iter().enumerate() {
            let comma = if i + 1 < fields.len() { "," } else { "" };
            let _ = writeln!(out, "      {}: {v}{comma}", json_str(k));
        }
        out.push_str("    }");
    }
}

fn json_str(s: &str) -> String {
    Value::Str(s.to_owned()).to_json()
}

fn json_list(list: &[String]) -> String {
    let items: Vec<String> = list.iter().map(|s| json_str(s)).collect();
    format!("[{}]", items.join(", "))
}

/// A field of an entry as text: `Ok(None)` when absent.
fn text(e: &Value, key: &str) -> Result<Option<String>, String> {
    match e.get(key) {
        Value::Nil => Ok(None),
        Value::Str(s) => Ok(Some(s.clone())),
        _ => Err(format!("{key} is not text")),
    }
}

fn required(e: &Value, key: &str) -> Result<String, String> {
    text(e, key)?
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| format!("no {key}"))
}

fn text_list(e: &Value, key: &str) -> Result<Vec<String>, String> {
    match e.get(key) {
        Value::Nil => Ok(Vec::new()),
        Value::List(l) => l
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| format!("{key} is not a list of text"))
            })
            .collect(),
        _ => Err(format!("{key} is not a list of text")),
    }
}

fn entry(e: &Value) -> Result<Entry, String> {
    if e.as_map().is_none() {
        return Err("not an object".into());
    }
    let id_text = required(e, "id")?;
    let id = PluginId::parse(&id_text).ok_or_else(|| format!("{id_text:?} is not a plugin id"))?;
    if id.is_reserved() {
        return Err(format!("{id} is in a namespace kept for the client itself"));
    }
    let version_text = required(e, "version")?;
    let version = PluginVersion::parse(&version_text)
        .ok_or_else(|| format!("{version_text:?} is not a SemVer version"))?;
    let api_text = required(e, "api")?;
    let api = ApiVersion::parse(&api_text)
        .ok_or_else(|| format!("{api_text:?} is not an API version (MAJOR.MINOR)"))?;
    let sha = required(e, "sha256")?.to_ascii_lowercase();
    if sha.len() != 64 || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("sha256 is not 64 hexadecimal digits".into());
    }
    let size = match e.get("size") {
        Value::Nil => None,
        v => Some(
            v.as_i64()
                .and_then(|n| u64::try_from(n).ok())
                .ok_or("size is not a whole number of bytes")?,
        ),
    };
    let mut permissions = text_list(e, "permissions")?;
    permissions.sort();
    permissions.dedup();
    let mut domains: Vec<String> = text_list(e, "domains")?
        .into_iter()
        .map(|d| d.to_ascii_lowercase())
        .collect();
    if let Some(bad) = domains.iter().find(|d| net_domain(d).is_none()) {
        return Err(format!("{bad:?} is not a web domain"));
    }
    domains.sort();
    domains.dedup();
    Ok(Entry {
        name: text(e, "name")?.unwrap_or_else(|| id.to_string()),
        id,
        version,
        api,
        description: text(e, "description")?.unwrap_or_default(),
        authors: text_list(e, "authors")?,
        homepage: text(e, "homepage")?.unwrap_or_default(),
        permissions,
        domains,
        zip: required(e, "zip")?,
        sha256: sha,
        size,
        notes: text(e, "notes")?.unwrap_or_default(),
        released: text(e, "released")?.unwrap_or_default(),
    })
}

impl Catalog {
    /// Read a list.
    ///
    /// # Errors
    /// [`CatalogError`]: too large, not a list, or of a newer format. An entry that does not read
    /// is not an error: it is set aside in [`Read::skipped`].
    pub fn parse(bytes: &[u8]) -> Result<Read, CatalogError> {
        if bytes.len() > MAX_BYTES {
            return Err(CatalogError::TooLarge);
        }
        let text = std::str::from_utf8(bytes)
            .map_err(|_| CatalogError::NotAList("the text is not UTF-8".into()))?;
        // A byte-order mark is not JSON, but an editor may write one.
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let doc = Value::from_json_within(text, MAX_VALUES)
            .map_err(|e| CatalogError::NotAList(e.to_string()))?;
        if doc.as_map().is_none() {
            return Err(CatalogError::NotAList("it is not a JSON object".into()));
        }
        let format = doc
            .get("format")
            .as_i64()
            .ok_or_else(|| CatalogError::NotAList("it names no format".into()))?;
        if format > FORMAT {
            return Err(CatalogError::NewerFormat(format));
        }
        if format < 1 {
            return Err(CatalogError::NotAList(format!("format {format}")));
        }
        let list = doc
            .get("plugins")
            .as_list()
            .ok_or_else(|| CatalogError::NotAList("it has no list of plugins".into()))?;
        let mut read = Read {
            catalog: Catalog {
                name: doc
                    .get("name")
                    .as_str()
                    .unwrap_or_default()
                    .trim()
                    .to_owned(),
                plugins: Vec::new(),
            },
            skipped: Vec::new(),
        };
        for (i, e) in list.iter().enumerate() {
            let label = e
                .get("id")
                .as_str()
                .map_or_else(|| format!("plugin {}", i + 1), str::to_owned);
            match entry(e) {
                Ok(entry) if read.catalog.plugins.iter().any(|p| p.id == entry.id) => {
                    read.skipped
                        .push(format!("{label}: listed more than once; the first is kept"));
                }
                Ok(entry) => read.catalog.plugins.push(entry),
                Err(why) => read.skipped.push(format!("{label}: {why}")),
            }
        }
        Ok(read)
    }

    /// The entry for plugin `id`.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&Entry> {
        self.plugins.iter().find(|p| p.id.as_str() == id)
    }

    /// Put `entry` in the list, in place of the entry with its id if there is one (a plugin is
    /// listed at its latest release only), keeping the list in id order. Answers the entry it
    /// replaced.
    pub fn put(&mut self, entry: Entry) -> Option<Entry> {
        if let Some(old) = self.plugins.iter_mut().find(|p| p.id == entry.id) {
            return Some(std::mem::replace(old, entry));
        }
        let at = self.plugins.partition_point(|p| p.id < entry.id);
        self.plugins.insert(at, entry);
        None
    }

    /// The list as `plugins.json`'s text: the format, the name when there is one, and each entry
    /// with its fields in a fixed order, one to a line.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut out = String::from("{\n");
        let _ = writeln!(out, "  \"format\": {FORMAT},");
        if !self.name.is_empty() {
            let _ = writeln!(out, "  \"name\": {},", json_str(&self.name));
        }
        if self.plugins.is_empty() {
            out.push_str("  \"plugins\": []\n}\n");
            return out;
        }
        out.push_str("  \"plugins\": [\n");
        for (i, p) in self.plugins.iter().enumerate() {
            p.write_lines(&mut out);
            out.push_str(if i + 1 < self.plugins.len() {
                ",\n"
            } else {
                "\n"
            });
        }
        out.push_str("  ]\n}\n");
        out
    }
}

/// Where an entry's zip is: `reference` as written in the list, read against `base`, the list's
/// own `https://` address. An `https://` URL is itself; a path is the list's folder's (or, with a
/// leading `/`, its site's); `//host/...` is `https://host/...`. `None` for any other scheme
/// (`http:` included) and for a base that is not `https://`.
#[must_use]
pub fn resolve(base: &str, reference: &str) -> Option<String> {
    let reference = reference.trim();
    let is_https = |s: &str| s.len() > 8 && s[..8].eq_ignore_ascii_case("https://");
    if is_https(reference) {
        return Some(reference.to_owned());
    }
    if reference.is_empty()
        || reference
            .split(['/', '?', '#'])
            .next()
            .is_some_and(|first| first.contains(':'))
    {
        return None;
    }
    if !is_https(base) {
        return None;
    }
    if let Some(rest) = reference.strip_prefix("//") {
        return Some(format!("https://{rest}"));
    }
    let base = base.split(['?', '#']).next().unwrap_or(base);
    let after = &base[8..];
    let (host, path) = after.find('/').map_or((after, "/"), |i| after.split_at(i));
    let joined = if reference.starts_with('/') {
        reference.to_owned()
    } else {
        let folder = &path[..=path.rfind('/').unwrap_or(0)];
        format!("{folder}{reference}")
    };
    // The dot segments of the path (not of a query) are walked out.
    let (path_part, tail) = joined
        .find(['?', '#'])
        .map_or((joined.as_str(), ""), |i| joined.split_at(i));
    let mut out: Vec<&str> = Vec::new();
    let segments: Vec<&str> = path_part.split('/').skip(1).collect();
    for (i, s) in segments.iter().enumerate() {
        let last = i + 1 == segments.len();
        match *s {
            "." => {
                if last {
                    out.push("");
                }
            }
            ".." => {
                out.pop();
                if last {
                    out.push("");
                }
            }
            s => out.push(s),
        }
    }
    Some(format!("https://{host}/{}{tail}", out.join("/")))
}

#[cfg(test)]
mod tests;
