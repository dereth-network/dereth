//! The plugin author's tool: makes a new plugin repository, checks a plugin as the hosts check it,
//! packs it into a zip, releases it and lists it in a plugin source's `plugins.json`.
//!
//! **Depends on** `dereth-plugin-package` for the package (the hosts' own reading), a compiled
//! plugin's source hash and the source's list, `dereth-plugin-api` for the API version and the
//! versions and ids, `dereth-plugin-lua` to compile a plugin's scripts as the hosts compile them,
//! `dereth-plugin-wasm` to check a compiled plugin's module as the hosts check it, `wit-component`
//! to make a module a component, `toml` to read a crate's name and `sha2` for the zip's own hash.
//! **Used by** no crate: authors run it, and the template's workflows run it.
//!
//! **Must never** write anywhere but the paths it is given, and the git repository a release is
//! made in.
//!
//! ```text
//! dereth-plugin new <folder> [--id <id>] [--name <name>] [--github <owner>/<repo>]
//!                                           a new plugin repository, from the template
//! dereth-plugin check <folder>              every check a host makes at load, and every script
//!                                           compiled; a folder of plugins checks each
//! dereth-plugin pack <folder> <out.zip>     the folder as a zip, and <out.zip>.sha256 beside it
//! dereth-plugin hash <zip>                  a package zip's id, version and SHA-256
//! dereth-plugin build <crate> <package>     a Rust plugin built as a component: <package>/plugin.wasm
//! dereth-plugin release <folder> <version> [--push]
//!                                           set the version, close the change log's section,
//!                                           commit and tag; push with --push
//! dereth-plugin notes <folder> <version>    that version's change-log section
//! dereth-plugin find <repository> <tag>     the plugin a release tag names: folder, id, version, name
//! dereth-plugin add <plugins.json> <zip> <url> [--notes <file>] [--name <source name>]
//!                                           list the zip's release in a source's plugins.json
//! dereth-plugin --version                   the tool's version (Dereth's), and the commit and
//!                                           target a release build was made from
//! ```
//!
//! `build` compiles the crate for `wasm32-unknown-unknown` in release (with `cargo`, into
//! `target/plugin-builds` under the current folder, or under `CARGO_TARGET_DIR`), makes the core
//! module a component with `wit-component`, adds the hash of the crate's sources
//! (`dereth-source-sha256`) and writes it to the package folder as `plugin.wasm`.
//!
//! `docs/plugins/sources.md` says what a source's list holds, and
//! `docs/plugins/guide/publishing.md` how a plugin is published with this tool and the template.

mod check;
mod list;
mod release;
mod template;
#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use dereth_plugin_package::source;
use dereth_plugin_package::{is_package_root, Package, MANIFEST_FILE};
use sha2::{Digest, Sha256};

const USAGE: &str = "\
dereth-plugin -- make, check, pack, release and list plugins

  dereth-plugin new <folder> [--id <id>] [--name <name>] [--github <owner>/<repo>]
  dereth-plugin check <folder>
  dereth-plugin pack <folder> <out.zip>
  dereth-plugin hash <zip>
  dereth-plugin build <crate> <package>
  dereth-plugin release <folder> <version> [--push]
  dereth-plugin notes <folder> <version>
  dereth-plugin find <repository> <tag>
  dereth-plugin add <plugins.json> <zip> <url> [--notes <file>] [--name <source name>]
  dereth-plugin --version";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(text) => {
            if !text.is_empty() {
                println!("{text}");
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("dereth-plugin: {e}");
            ExitCode::FAILURE
        }
    }
}

/// The words of a command line, and its `--name value` options and `--flag` switches.
struct Line<'a> {
    words: Vec<&'a str>,
    options: Vec<(&'a str, Option<&'a str>)>,
}

impl<'a> Line<'a> {
    /// `valued` are the options that take a value.
    fn parse(args: &'a [String], valued: &[&str]) -> Result<Self, String> {
        let mut words = Vec::new();
        let mut options = Vec::new();
        let mut it = args.iter().map(String::as_str);
        while let Some(a) = it.next() {
            if let Some(name) = a.strip_prefix("--") {
                if valued.contains(&name) {
                    let v = it.next().ok_or_else(|| format!("--{name} needs a value"))?;
                    options.push((name, Some(v)));
                } else {
                    options.push((name, None));
                }
            } else {
                words.push(a);
            }
        }
        Ok(Self { words, options })
    }

    fn value(&self, name: &str) -> Option<&'a str> {
        self.options
            .iter()
            .find(|(n, _)| *n == name)
            .and_then(|(_, v)| *v)
    }

    fn flag(&self, name: &str) -> bool {
        self.options.iter().any(|(n, _)| *n == name)
    }

    /// Refuse an option the command does not take.
    fn only(&self, allowed: &[&str]) -> Result<(), String> {
        match self.options.iter().find(|(n, _)| !allowed.contains(n)) {
            Some((n, _)) => Err(format!("unknown option --{n}\n\n{USAGE}")),
            None => Ok(()),
        }
    }
}

/// What `dereth-plugin --version` prints: the version, which is Dereth's, and the commit and
/// target a release build was made from (`unknown` in a build that did not say).
fn version_text() -> String {
    format!(
        "dereth-plugin {}
commit: {}
target: {}",
        env!("CARGO_PKG_VERSION"),
        option_env!("DERETH_BUILD_COMMIT").unwrap_or("unknown"),
        option_env!("DERETH_BUILD_TARGET").unwrap_or("unknown"),
    )
}

fn run(args: &[String]) -> Result<String, String> {
    if matches!(args, [a] if a == "--version" || a == "version") {
        return Ok(version_text());
    }
    let line = Line::parse(args, &["id", "name", "github", "notes"])?;
    match line.words.as_slice() {
        ["new", folder] => {
            line.only(&["id", "name", "github"])?;
            template::new(
                Path::new(folder),
                &template::Options {
                    id: line.value("id"),
                    name: line.value("name"),
                    github: line.value("github"),
                },
            )
        }
        ["check", folder] => {
            line.only(&[])?;
            check::check(Path::new(folder))
        }
        ["pack", folder, out] => {
            line.only(&[])?;
            pack(Path::new(folder), Path::new(out))
        }
        ["hash", p] => {
            line.only(&[])?;
            hash(Path::new(p))
        }
        ["build", krate, package] => {
            line.only(&[])?;
            build(Path::new(krate), Path::new(package))
        }
        ["release", folder, version] => {
            line.only(&["push"])?;
            release::release(Path::new(folder), version, &today(), line.flag("push"))
        }
        ["notes", folder, version] => {
            line.only(&[])?;
            release::notes(Path::new(folder), version)
        }
        ["find", repo, tag] => {
            line.only(&[])?;
            release::find(Path::new(repo), tag)
        }
        ["add", list_file, zip, url] => {
            line.only(&["notes", "name"])?;
            let notes = match line.value("notes") {
                Some(f) => std::fs::read_to_string(f).map_err(|e| format!("{f}: {e}"))?,
                None => String::new(),
            };
            list::add(
                Path::new(list_file),
                Path::new(zip),
                url,
                &notes,
                line.value("name"),
                &today(),
            )
        }
        _ => Err(USAGE.to_owned()),
    }
}

/// Today's date, `YYYY-MM-DD`, in UTC.
fn today() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    date_of_day(i64::try_from(secs / 86_400).unwrap_or(0))
}

/// The date `days` days after 1970-01-01, `YYYY-MM-DD` (the proleptic Gregorian calendar).
fn date_of_day(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

/// A folder's files by `/`-separated relative path; hidden files and folders are left out.
fn read_folder(dir: &Path) -> Result<Package, String> {
    if !is_package_root(|f| dir.join(f).is_file()) {
        return Err(format!("{} has no {MANIFEST_FILE}", dir.display()));
    }
    let mut p = Package {
        origin: dir.display().to_string(),
        ..Package::default()
    };
    let mut stack = vec![PathBuf::new()];
    while let Some(rel) = stack.pop() {
        let entries =
            std::fs::read_dir(dir.join(&rel)).map_err(|e| format!("{}: {e}", rel.display()))?;
        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let path = rel.join(&name);
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            if kind.is_dir() {
                stack.push(path);
            } else if kind.is_file() {
                let bytes = std::fs::read(dir.join(&path)).map_err(|e| e.to_string())?;
                let key = path
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/");
                p.files.insert(key, bytes);
            }
        }
    }
    Ok(p)
}

fn sha256_hex(bytes: &[u8]) -> String {
    source::hex(&Sha256::digest(bytes))
}

/// A package zip's id and version, and the zip's SHA-256.
fn describe(p: &Package, zip: &[u8]) -> Result<String, String> {
    let m = p.manifest().map_err(|e| e.to_string())?;
    Ok(format!(
        "{} {}\nzip sha256 {}",
        m.id,
        m.version,
        sha256_hex(zip)
    ))
}

/// Pack `folder` into `out`, and write `<out>.sha256` beside it: the zip's SHA-256 and its file
/// name, as `sha256sum` writes them.
fn pack(folder: &Path, out: &Path) -> Result<String, String> {
    let p = read_folder(folder)?;
    p.manifest().map_err(|e| e.to_string())?;
    let bytes = p.to_zip();
    // What was written reads back as the same package.
    let back = Package::from_zip("check", &bytes).map_err(|e| e.to_string())?;
    if back.files != p.files {
        return Err("the zip does not read back as the folder".into());
    }
    std::fs::write(out, &bytes).map_err(|e| format!("{}: {e}", out.display()))?;
    let file_name = out
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut sum = out.as_os_str().to_owned();
    sum.push(".sha256");
    let sum = PathBuf::from(sum);
    std::fs::write(&sum, format!("{}  {file_name}\n", sha256_hex(&bytes)))
        .map_err(|e| format!("{}: {e}", sum.display()))?;
    describe(&p, &bytes)
}

/// A crate's source files for its source hash: every file under it but `target` and hidden ones.
fn crate_sources(krate: &Path) -> Result<std::collections::BTreeMap<String, Vec<u8>>, String> {
    let mut files = std::collections::BTreeMap::new();
    let mut stack = vec![PathBuf::new()];
    while let Some(rel) = stack.pop() {
        let entries =
            std::fs::read_dir(krate.join(&rel)).map_err(|e| format!("{}: {e}", rel.display()))?;
        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || (rel.as_os_str().is_empty() && name == "target") {
                continue;
            }
            let path = rel.join(&name);
            if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
                stack.push(path);
            } else {
                let key = path
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/");
                let bytes = std::fs::read(krate.join(&path)).map_err(|e| e.to_string())?;
                files.insert(key, bytes);
            }
        }
    }
    Ok(files)
}

fn leb128(mut n: usize, out: &mut Vec<u8>) {
    loop {
        // Seven bits at a time, low first.
        #[allow(clippy::cast_possible_truncation)]
        let byte = (n & 0x7f) as u8;
        n >>= 7;
        if n == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

/// Build the Rust plugin crate `krate` as a component in `package` (see the module
/// documentation).
fn build(krate: &Path, package: &Path) -> Result<String, String> {
    let manifest = krate.join("Cargo.toml");
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map_or_else(|| PathBuf::from("target"), PathBuf::from)
        .join("plugin-builds");
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let status = std::process::Command::new(cargo)
        .args([
            "build",
            "--release",
            "--target",
            "wasm32-unknown-unknown",
            "--manifest-path",
        ])
        .arg(&manifest)
        .arg("--target-dir")
        .arg(&target)
        .status()
        .map_err(|e| format!("cargo: {e}"))?;
    if !status.success() {
        return Err("the crate did not build".into());
    }
    let text =
        std::fs::read_to_string(&manifest).map_err(|e| format!("{}: {e}", manifest.display()))?;
    let doc: toml::Table = toml::from_str(&text).map_err(|e| e.to_string())?;
    let name = doc
        .get("package")
        .and_then(|p| p.get("name"))
        .and_then(toml::Value::as_str)
        .ok_or("the crate's Cargo.toml names no package")?
        .replace('-', "_");
    let core_path = target
        .join("wasm32-unknown-unknown/release")
        .join(format!("{name}.wasm"));
    let core = std::fs::read(&core_path).map_err(|e| format!("{}: {e}", core_path.display()))?;
    let mut component = wit_component::ComponentEncoder::default()
        .module(&core)
        .and_then(|e| e.validate(true).encode())
        .map_err(|e| format!("not a component: {e:#}"))?;
    let hash = source::hex(&source::source_hash(&crate_sources(krate)?));
    let mut payload = Vec::new();
    leb128(source::SOURCE_SECTION.len(), &mut payload);
    payload.extend_from_slice(source::SOURCE_SECTION.as_bytes());
    payload.extend_from_slice(hash.as_bytes());
    component.push(0);
    leb128(payload.len(), &mut component);
    component.extend_from_slice(&payload);
    let out = package.join("plugin.wasm");
    std::fs::write(&out, &component).map_err(|e| format!("{}: {e}", out.display()))?;
    Ok(format!(
        "{}: {} bytes (module {} bytes), sources {hash}",
        out.display(),
        component.len(),
        core.len()
    ))
}

fn hash(p: &Path) -> Result<String, String> {
    if p.is_dir() {
        return Err(format!(
            "{} is a folder: pack it first, then hash the zip",
            p.display()
        ));
    }
    let bytes = std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()))?;
    let pkg = Package::from_zip(&p.display().to_string(), &bytes).map_err(|e| e.to_string())?;
    describe(&pkg, &bytes)
}
