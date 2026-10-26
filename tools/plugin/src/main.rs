//! The plugin author's tool: packs a plugin folder into a zip, prints a zip's SHA-256, and builds a
//! Rust plugin as a component.
//!
//! **Depends on** `dereth-plugin-package` for the package (the hosts' own reading) and a compiled
//! plugin's source hash, `wit-component` to make a module a component, `toml` to read a crate's
//! name and `sha2` for the zip's own hash. **Used by** no crate: authors run it.
//!
//! **Must never** write anywhere but the paths it is given.
//!
//! ```text
//! dereth-plugin pack <folder> <out.zip>     the folder as a zip; prints the zip's SHA-256
//! dereth-plugin hash <zip>                  a package zip's id, version and SHA-256
//! dereth-plugin build <crate> <package>     a Rust plugin built as a component: <package>/plugin.wasm
//! ```
//!
//! `build` compiles the crate for `wasm32-unknown-unknown` in release (with `cargo`, into
//! `target/plugin-builds` under the current folder, or under `CARGO_TARGET_DIR`), makes the core
//! module a component with `wit-component`, adds the hash of the crate's sources
//! (`dereth-source-sha256`) and writes it to the package folder as `plugin.wasm`.
//!
//! The zip's SHA-256 is what the launcher checks an installed release against (`<url>.sha256`);
//! `docs/plugins/packages.md` says how.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use dereth_plugin_package::source;
use dereth_plugin_package::{is_package_root, Package, MANIFEST_FILE};
use sha2::{Digest, Sha256};

const USAGE: &str = "\
dereth-plugin -- pack and hash plugin packages, and build Rust plugins

  dereth-plugin pack <folder> <out.zip>
  dereth-plugin hash <zip>
  dereth-plugin build <crate> <package>";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match words.as_slice() {
        ["pack", folder, out] => pack(Path::new(folder), Path::new(out)),
        ["hash", p] => hash(Path::new(p)),
        ["build", krate, package] => build(Path::new(krate), Path::new(package)),
        _ => Err(USAGE.to_owned()),
    };
    match result {
        Ok(text) => {
            println!("{text}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("dereth-plugin: {e}");
            ExitCode::FAILURE
        }
    }
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

/// A package zip's id and version, and the zip's SHA-256.
fn describe(p: &Package, zip: &[u8]) -> Result<String, String> {
    let m = p.manifest().map_err(|e| e.to_string())?;
    Ok(format!(
        "{} {}\nzip sha256 {}",
        m.id,
        m.version,
        source::hex(&Sha256::digest(zip))
    ))
}

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
