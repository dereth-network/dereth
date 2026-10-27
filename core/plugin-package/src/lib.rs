//! Plugin packages as files: read from a folder's bytes or a zip, and their manifest, with no
//! game, no engine and no I/O in it; a compiled plugin's source hash; and the list a plugin source
//! publishes.
//!
//! **Depends on** the interface (`dereth-plugin-api`) for the manifest and JSON, `toml` for
//! `plugin.toml`, `zip` for zip packages, and `sha2` for a compiled plugin's source hash and a
//! zip's. **Used by** the plugin host (`dereth-plugin-host`), the launcher's knowledge
//! (`dereth-launch`), which installs packages from sources, and the author's tool
//! (`dereth-plugin`), which makes them and their sources' lists.
//!
//! **Must never** depend on another crate of this workspace, the game, an engine or a platform
//! (`cargo xtask seams`, `seam: plugin-package deps`): the launcher builds it.
//!
//! [`Package`] is a package's files, [`source`] a compiled plugin's source hash, and [`catalog`] a
//! source's `plugins.json`: its entries, and the check that a downloaded zip is the release its
//! entry lists.

pub mod catalog;
pub mod source;

use std::collections::BTreeMap;

use dereth_plugin_api::manifest::{is_package_path, SCRIPT_ENTRY};
use dereth_plugin_api::{Manifest, ManifestError, Value};

/// The manifest's file name at a package's root.
pub const MANIFEST_FILE: &str = "plugin.toml";

/// The entry of a script written for UtilityBelt's scripting: a folder holding it and no manifest
/// is a package too, read as such a script ([`Manifest::for_script_folder`]).
pub const SCRIPT_FILE: &str = SCRIPT_ENTRY;

/// Whether a folder's file names make it a package: it holds a manifest, or it is a script
/// written for UtilityBelt's scripting (an `index.lua` and no manifest).
#[must_use]
pub fn is_package_root(has_file: impl Fn(&str) -> bool) -> bool {
    has_file(MANIFEST_FILE) || has_file(SCRIPT_FILE)
}

/// The most files a package may hold, and the most bytes in all.
pub const MAX_FILES: usize = 2_000;
pub const MAX_BYTES: usize = 64 << 20;

/// The file extensions a package may hold: text, Lua, TOML, JSON, XML, images and fonts, a
/// compiled plugin's WebAssembly (sandboxed, not native code), and `.sig` (a file some older
/// packages carry, read as any other).
pub const ALLOWED_EXTENSIONS: &[&str] = &[
    "lua", "luau", "toml", "json", "xml", "txt", "md", "png", "ttf", "otf", "utl", "nav", "met",
    "sig", "wasm",
];

/// One plugin package: its files by `/`-separated path relative to the package root.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Package {
    /// Where the package came from, for messages (a folder path, a file name).
    pub origin: String,
    pub files: BTreeMap<String, Vec<u8>>,
}

/// Why a package was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageError {
    NoManifest,
    /// The manifest is not UTF-8 TOML.
    ManifestSyntax(String),
    Manifest(ManifestError),
    /// A path that leaves the package, or a file of a kind a package may not hold.
    BadFile(String),
    TooLarge,
    /// The entry file named by the manifest is not in the package.
    NoEntry(String),
}

impl std::fmt::Display for PackageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoManifest => write!(f, "the package has no {MANIFEST_FILE} at its root"),
            Self::ManifestSyntax(e) => write!(f, "{MANIFEST_FILE} is not valid TOML: {e}"),
            Self::Manifest(e) => write!(f, "{MANIFEST_FILE}: {e}"),
            Self::BadFile(p) => write!(f, "the package may not hold {p:?}"),
            Self::TooLarge => write!(
                f,
                "the package holds more than {MAX_FILES} files or {MAX_BYTES} bytes"
            ),
            Self::NoEntry(e) => write!(f, "the entry file {e:?} is not in the package"),
        }
    }
}

impl std::error::Error for PackageError {}

/// A TOML document as a plain value. A date or time is kept as its text.
#[must_use]
pub fn toml_to_value(v: &toml::Value) -> Value {
    match v {
        toml::Value::String(s) => Value::Str(s.clone()),
        toml::Value::Integer(i) => Value::Int(*i),
        toml::Value::Float(f) => Value::Num(*f),
        toml::Value::Boolean(b) => Value::Bool(*b),
        toml::Value::Datetime(d) => Value::Str(d.to_string()),
        toml::Value::Array(a) => Value::List(a.iter().map(toml_to_value).collect()),
        toml::Value::Table(t) => Value::Map(
            t.iter()
                .map(|(k, v)| (k.clone(), toml_to_value(v)))
                .collect(),
        ),
    }
}

impl Package {
    /// Check the package's files and read its manifest.
    ///
    /// # Errors
    /// [`PackageError`]: a missing or invalid manifest, a file a package may not hold, too many
    /// files or bytes, or a missing entry file.
    pub fn manifest(&self) -> Result<Manifest, PackageError> {
        if self.files.len() > MAX_FILES
            || self.files.values().map(Vec::len).sum::<usize>() > MAX_BYTES
        {
            return Err(PackageError::TooLarge);
        }
        for path in self.files.keys() {
            let ext = path
                .rsplit_once('.')
                .map(|(_, e)| e.to_ascii_lowercase())
                .unwrap_or_default();
            if !is_package_path(path) || !ALLOWED_EXTENSIONS.contains(&ext.as_str()) {
                return Err(PackageError::BadFile(path.clone()));
            }
        }
        let Some(bytes) = self.files.get(MANIFEST_FILE) else {
            // A script written for UtilityBelt's scripting comes as its folder alone.
            if self.files.contains_key(SCRIPT_FILE) {
                return Ok(Manifest::for_script_folder(&self.folder_name()));
            }
            return Err(PackageError::NoManifest);
        };
        let text =
            std::str::from_utf8(bytes).map_err(|e| PackageError::ManifestSyntax(e.to_string()))?;
        let table: toml::Table =
            toml::from_str(text).map_err(|e| PackageError::ManifestSyntax(e.to_string()))?;
        let m = Manifest::from_value(&toml_to_value(&toml::Value::Table(table)))
            .map_err(PackageError::Manifest)?;
        if !self.files.contains_key(&m.entry) {
            return Err(PackageError::NoEntry(m.entry));
        }
        Ok(m)
    }

    /// A package from a zip file's bytes. The files are its entries with `plugin.toml` at the
    /// root, or inside one folder that holds everything (as an archiver makes of a folder), which
    /// is then the root. Folders are skipped; more than [`MAX_FILES`] entries or [`MAX_BYTES`]
    /// bytes unpacked is refused before it is unpacked.
    ///
    /// # Errors
    /// Not a zip, an entry that does not unpack, or too large.
    pub fn from_zip(origin: &str, bytes: &[u8]) -> Result<Self, PackageError> {
        use std::io::Read as _;
        let bad = |e: &dyn std::fmt::Display| PackageError::BadFile(format!("{origin}: {e}"));
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| bad(&e))?;
        if zip.len() > MAX_FILES {
            return Err(PackageError::TooLarge);
        }
        let mut files = BTreeMap::new();
        let mut total = 0usize;
        for i in 0..zip.len() {
            let entry = zip.by_index(i).map_err(|e| bad(&e))?;
            if entry.is_dir() {
                continue;
            }
            let name = entry.name().replace('\\', "/");
            // The size the entry says, then no more than the budget left, whatever it says.
            let left = MAX_BYTES.saturating_sub(total);
            if usize::try_from(entry.size()).unwrap_or(usize::MAX) > left {
                return Err(PackageError::TooLarge);
            }
            let mut data = Vec::new();
            entry
                .take(left as u64 + 1)
                .read_to_end(&mut data)
                .map_err(|e| bad(&e))?;
            total += data.len();
            if total > MAX_BYTES {
                return Err(PackageError::TooLarge);
            }
            files.insert(name, data);
        }
        // One folder holding everything, with the manifest (or a script's `index.lua`) in it:
        // that folder is the root.
        if !is_package_root(|f| files.contains_key(f)) {
            let top = files
                .keys()
                .next()
                .and_then(|k| k.split_once('/'))
                .map(|(t, _)| format!("{t}/"));
            if let Some(top) = top.filter(|t| {
                files.keys().all(|k| k.starts_with(t.as_str()))
                    && is_package_root(|f| files.contains_key(&format!("{t}{f}")))
            }) {
                files = files
                    .into_iter()
                    .map(|(k, v)| (k[top.len()..].to_owned(), v))
                    .collect();
            }
        }
        Ok(Self {
            origin: origin.to_owned(),
            files,
        })
    }

    /// The package as a zip file, its files at the root in path order, Deflate-compressed: what
    /// [`Package::from_zip`] reads back to the same files.
    #[must_use]
    pub fn to_zip(&self) -> Vec<u8> {
        use std::io::Write as _;
        let mut buf = std::io::Cursor::new(Vec::new());
        let mut z = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, bytes) in &self.files {
            // Writing to memory fails only on a name or size zip cannot hold, which a package's
            // caps rule out.
            if z.start_file(name.as_str(), opts).is_ok() {
                let _ = z.write_all(bytes);
            }
        }
        let _ = z.finish();
        buf.into_inner()
    }

    /// The name of the folder (or zip file) the package came from: the last part of its origin,
    /// without a `.zip`.
    #[must_use]
    pub fn folder_name(&self) -> String {
        let last = self
            .origin
            .trim_end_matches(['/', '\\'])
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or_default();
        let lower = last.to_ascii_lowercase();
        if lower.ends_with(".zip") {
            last[..last.len() - 4].to_owned()
        } else {
            last.to_owned()
        }
    }

    /// A text file of the package.
    #[must_use]
    pub fn text(&self, path: &str) -> Option<String> {
        self.files
            .get(path)
            .map(|b| String::from_utf8_lossy(b).into_owned())
    }

    /// The source of module `name` (`"lib.util"` is `lib/util.lua`, or `lib/util.luau`).
    #[must_use]
    pub fn module(&self, name: &str) -> Option<String> {
        if name.is_empty() || name.split('.').any(str::is_empty) {
            return None;
        }
        let base = name.replace('.', "/");
        self.text(&format!("{base}.lua"))
            .or_else(|| self.text(&format!("{base}.luau")))
    }
}

#[cfg(test)]
mod tests {
    use super::{Package, PackageError};

    fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
        use std::io::Write as _;
        let mut buf = std::io::Cursor::new(Vec::new());
        let mut z = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, bytes) in files {
            z.start_file(*name, opts).expect("entry");
            z.write_all(bytes).expect("bytes");
        }
        z.finish().expect("zip");
        buf.into_inner()
    }

    #[test]
    fn a_zip_package_reads_from_its_root_or_from_the_one_folder_that_holds_it() {
        let manifest: &[u8] = b"id = \"a.b\"\nname = \"A\"\nversion = \"1.0.0\"\napi = \"0.4\"\n";
        let flat = zip_of(&[("plugin.toml", manifest), ("main.lua", b"print(1)")]);
        let p = Package::from_zip("flat.zip", &flat).expect("reads");
        assert!(p.manifest().is_ok(), "{:?}", p.manifest());
        let nested = zip_of(&[("ab/plugin.toml", manifest), ("ab/main.lua", b"print(1)")]);
        let q = Package::from_zip("nested.zip", &nested).expect("reads");
        assert_eq!(p.files, q.files);
        assert!(Package::from_zip("junk.zip", b"not a zip").is_err());
    }

    #[test]
    fn a_zip_that_unpacks_past_the_limit_is_refused() {
        let big = vec![0u8; super::MAX_BYTES + 1];
        let z = zip_of(&[("plugin.toml", b""), ("data.txt", &big)]);
        assert_eq!(
            Package::from_zip("big.zip", &z),
            Err(PackageError::TooLarge)
        );
    }

    fn package(files: &[(&str, &str)]) -> Package {
        Package {
            origin: "test".into(),
            files: files
                .iter()
                .map(|(p, t)| ((*p).to_owned(), t.as_bytes().to_vec()))
                .collect(),
        }
    }

    const MANIFEST: &str =
        "id = \"example.xp\"\nname = \"XP\"\nversion = \"1.0.0\"\napi = \"0.1\"\n";

    #[test]
    fn a_package_needs_its_manifest_and_its_entry_file() {
        assert_eq!(
            package(&[("main.lua", "")]).manifest(),
            Err(PackageError::NoManifest)
        );
        assert_eq!(
            package(&[("plugin.toml", MANIFEST)]).manifest(),
            Err(PackageError::NoEntry("main.lua".into()))
        );
        let m = package(&[("plugin.toml", MANIFEST), ("main.lua", "")])
            .manifest()
            .expect("valid");
        assert_eq!(m.id.as_str(), "example.xp");
    }

    #[test]
    fn a_repeated_manifest_key_and_native_code_are_refused() {
        let twice = format!("{MANIFEST}name = \"again\"\n");
        assert!(matches!(
            package(&[("plugin.toml", &twice), ("main.lua", "")]).manifest(),
            Err(PackageError::ManifestSyntax(_))
        ));
        assert_eq!(
            package(&[
                ("plugin.toml", MANIFEST),
                ("main.lua", ""),
                ("bin/x.dll", "")
            ])
            .manifest(),
            Err(PackageError::BadFile("bin/x.dll".into()))
        );
    }

    #[test]
    fn a_folder_with_index_lua_and_no_manifest_is_a_utilitybelt_script() {
        let mut p = package(&[("index.lua", "print(1)"), ("quests.lua", "")]);
        p.origin = "C:/Users/a/plugins/RarePointer".into();
        let m = p.manifest().expect("a script folder");
        assert!(m.ubscript);
        assert_eq!(
            (m.id.as_str(), m.entry.as_str()),
            ("ubscript.rarepointer", "index.lua")
        );
        p.origin = "D:\\scripts\\Flag Tracker.zip".into();
        assert_eq!(
            p.manifest().expect("zip").id.as_str(),
            "ubscript.flag-tracker"
        );
        assert_eq!(
            package(&[("main.lua", "")]).manifest(),
            Err(PackageError::NoManifest)
        );
        assert!(super::is_package_root(|f| f == "index.lua"));
        assert!(!super::is_package_root(|f| f == "main.lua"));
    }

    #[test]
    fn modules_resolve_to_lua_files_inside_the_package() {
        let p = package(&[("lib/util.lua", "return 1"), ("other.luau", "return 2")]);
        assert_eq!(p.module("lib.util").as_deref(), Some("return 1"));
        assert_eq!(p.module("other").as_deref(), Some("return 2"));
        assert_eq!(p.module("lib..util"), None);
        assert_eq!(p.module("missing"), None);
    }
}
