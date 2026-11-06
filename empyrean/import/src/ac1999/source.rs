//! Immutable tree/blob snapshots, unaffected by checkout attributes or replacement objects.

use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub const DATABASE: &str = "Server/Database/AC1999-World";
pub const TREASURE: &str = "Server/Bin/Data/Treasure";
pub const LICENSE: &str = "Server/LICENSE";
pub const URL: &str = "https://gitlab.com/adamplumb/ac1999.git";

fn git_path(path: &Path) -> Result<String, String> {
    let text = path.to_str().ok_or("Git paths must be valid Unicode")?;
    if let Some(unc) = text.strip_prefix("\\\\?\\UNC\\") {
        Ok(format!("\\\\{unc}"))
    } else {
        Ok(text.strip_prefix("\\\\?\\").unwrap_or(text).to_owned())
    }
}

pub fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn command(repo: &Path) -> Result<Command, String> {
    let mut cmd = Command::new("git");
    cmd.args([
        "--no-replace-objects",
        "-C",
        &git_path(repo)?,
        "-c",
        "credential.helper=",
        "-c",
        "core.hooksPath=",
    ])
    .env("GIT_TERMINAL_PROMPT", "0");
    Ok(cmd)
}

fn git(repo: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let output = command(repo)?
        .args(args)
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if !output.status.success() {
        return Err(format!("git: {}", String::from_utf8_lossy(&output.stderr)));
    }
    Ok(output.stdout)
}

fn portable_path(name: &str, seen: &mut BTreeMap<String, String>) -> Result<(), String> {
    if !(name.starts_with(&format!("{DATABASE}/"))
        || name.starts_with(&format!("{TREASURE}/"))
        || name == LICENSE)
    {
        return Err(format!("unexpected source path {name:?}"));
    }
    let mut prefix = String::new();
    for part in name.split('/') {
        if part.is_empty()
            || part == "."
            || part == ".."
            || part.ends_with(['.', ' '])
            || part
                .chars()
                .any(|c| c.is_control() || "\\:*?\"<>|".contains(c))
        {
            return Err(format!("nonportable source path {name:?}"));
        }
        let stem = part.split('.').next().unwrap_or(part).to_ascii_uppercase();
        if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CLOCK$")
            || ["COM", "LPT"].iter().any(|p| {
                stem.strip_prefix(p).is_some_and(|s| {
                    matches!(
                        s,
                        "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                    )
                })
            })
        {
            return Err(format!("reserved source path {name:?}"));
        }
        if !prefix.is_empty() {
            prefix.push('/');
        }
        prefix.push_str(part);
        let key = prefix.to_lowercase();
        if let Some(old) = seen.insert(key, prefix.clone()) {
            if old != prefix {
                return Err(format!(
                    "case-colliding source paths {old:?} and {prefix:?}"
                ));
            }
        }
    }
    Ok(())
}

/// Whether `path` is a source bundle (a directory holding `source.json` or a converter bundle's
/// `manifest.json` beside `source/`, or a ZIP of one) rather than a Git repository.
pub fn is_bundle(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("zip"))
        || (path.join("source").is_dir()
            && (path.join("source.json").is_file() || path.join("manifest.json").is_file()))
}

/// The committed inputs from a source bundle, checked file for file against its own list of
/// SHA-256s: the same files, no more and no fewer, at the requested revision.
pub fn from_bundle(
    bundle: &Path,
    revision: &str,
    scratch: &Path,
) -> Result<(PathBuf, BTreeMap<String, String>), String> {
    let dir = if bundle.is_file() {
        let out = scratch.join("bundle");
        unzip(bundle, &out)?;
        out
    } else {
        bundle.to_path_buf()
    };
    let read_json = |p: PathBuf| -> Result<serde_json::Value, String> {
        serde_json::from_slice(&std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display()))?)
            .map_err(|e| format!("{}: {e}", p.display()))
    };
    let described = if dir.join("source.json").is_file() {
        read_json(dir.join("source.json"))?
    } else {
        read_json(dir.join("manifest.json"))?["source"].clone()
    };
    if described["url"] != URL || described["revision"] != revision {
        return Err("the source bundle is not this repository at this revision".into());
    }
    let expected: BTreeMap<String, String> = described["files_sha256"]
        .as_object()
        .ok_or("the source bundle lists no files")?
        .iter()
        .map(|(k, v)| Ok((k.clone(), v.as_str().ok_or("bad file hash")?.to_owned())))
        .collect::<Result<_, String>>()?;
    let mut seen = BTreeMap::new();
    for name in expected.keys() {
        portable_path(name, &mut seen)?;
    }
    if !expected.contains_key(LICENSE) {
        return Err("source license is absent".into());
    }
    let mut found = BTreeMap::new();
    let mut stack = vec![dir.join("source")];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            if kind.is_dir() {
                stack.push(entry.path());
            } else if kind.is_file() {
                let rel = entry
                    .path()
                    .strip_prefix(dir.join("source"))
                    .map_err(|e| e.to_string())?
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/");
                let bytes = std::fs::read(entry.path()).map_err(|e| e.to_string())?;
                found.insert(rel, (hash(&bytes), bytes));
            } else {
                return Err("the source bundle holds something other than files".into());
            }
        }
    }
    if found.len() != expected.len()
        || found
            .iter()
            .any(|(name, (sha, _))| expected.get(name) != Some(sha))
    {
        return Err("the source bundle's files do not match its hashes".into());
    }
    let root = scratch.join("source");
    for (name, (_, bytes)) in &found {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().ok_or("source file has no parent")?)
            .map_err(|e| e.to_string())?;
        std::fs::write(path, bytes).map_err(|e| e.to_string())?;
    }
    Ok((root, expected))
}

fn unzip(zip_path: &Path, out: &Path) -> Result<(), String> {
    let file = std::fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut total = 0u64;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = entry
            .enclosed_name()
            .ok_or("unsafe path in the source bundle")?;
        total = total.saturating_add(entry.size());
        if total > 512 * 1024 * 1024 {
            return Err("source exceeds converter size limits".into());
        }
        let path = out.join(name);
        if entry.is_dir() {
            std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
            continue;
        }
        std::fs::create_dir_all(path.parent().ok_or("bad entry")?).map_err(|e| e.to_string())?;
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut entry, &mut bytes).map_err(|e| e.to_string())?;
        std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn snapshot(
    source: Option<&Path>,
    revision: &str,
    scratch: &Path,
) -> Result<(PathBuf, BTreeMap<String, String>), String> {
    if let Some(bundle) = source.filter(|s| is_bundle(s)) {
        return from_bundle(bundle, revision, scratch);
    }
    let repo = if let Some(source) = source {
        source.canonicalize().map_err(|e| e.to_string())?
    } else {
        let repo = scratch.join("repository");
        std::fs::create_dir(&repo).map_err(|e| e.to_string())?;
        git(&repo, &["init", "--bare"])?;
        git(&repo, &["fetch", "--depth=1", "--no-tags", URL, revision])?;
        repo
    };
    let actual = git(
        &repo,
        &["rev-parse", "--verify", &format!("{revision}^{{commit}}")],
    )?;
    if String::from_utf8_lossy(&actual).trim() != revision {
        return Err("revision must identify the exact commit".into());
    }
    let listing = git(
        &repo,
        &[
            "ls-tree",
            "-r",
            "-l",
            "-z",
            "--full-tree",
            revision,
            "--",
            DATABASE,
            TREASURE,
            LICENSE,
        ],
    )?;
    let mut entries = Vec::new();
    let mut seen = BTreeMap::new();
    let mut total = 0usize;
    for entry in listing.split(|b| *b == 0).filter(|b| !b.is_empty()) {
        let entry = std::str::from_utf8(entry).map_err(|_| "non-Unicode source path")?;
        let (fields, name) = entry.split_once('\t').ok_or("invalid Git tree entry")?;
        portable_path(name, &mut seen)?;
        let fields: Vec<_> = fields.split_whitespace().collect();
        if fields.len() != 4 || !matches!(fields[0], "100644" | "100755") || fields[1] != "blob" {
            return Err(format!("unsupported tree entry {name:?}"));
        }
        let size = fields[3]
            .parse::<usize>()
            .map_err(|_| "invalid blob size")?;
        total = total.checked_add(size).ok_or("source size overflow")?;
        if size > 128 * 1024 * 1024 || total > 512 * 1024 * 1024 {
            return Err("source exceeds converter size limits".into());
        }
        entries.push((name.to_owned(), fields[2].to_owned(), size));
    }
    if !entries.iter().any(|e| e.0 == LICENSE) {
        return Err("source license is absent".into());
    }
    let requests = entries
        .iter()
        .map(|e| format!("{}\n", e.1))
        .collect::<String>();
    let mut child = command(&repo)?
        .args(["cat-file", "--batch"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut input = child.stdin.take().ok_or("missing Git input pipe")?;
    let writer = std::thread::spawn(move || input.write_all(requests.as_bytes()));
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    writer
        .join()
        .map_err(|_| "Git input writer panicked")?
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "git cat-file: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let root = scratch.join("source");
    let mut remaining = output.stdout.as_slice();
    let mut hashes = BTreeMap::new();
    for (name, oid, size) in entries {
        let end = remaining
            .iter()
            .position(|b| *b == b'\n')
            .ok_or("short Git blob header")?;
        let expected = format!("{oid} blob {size}");
        if remaining.get(..end) != Some(expected.as_bytes()) {
            return Err("unexpected Git blob header".into());
        }
        remaining = &remaining[end + 1..];
        let bytes = remaining.get(..size).ok_or("short Git blob")?;
        let path = root.join(&name);
        std::fs::create_dir_all(path.parent().ok_or("source file has no parent")?)
            .map_err(|e| e.to_string())?;
        if hashes.insert(name, hash(bytes)).is_some() {
            return Err("duplicate source path".into());
        }
        std::fs::write(path, bytes).map_err(|e| e.to_string())?;
        if remaining.get(size) != Some(&b'\n') {
            return Err("missing Git blob terminator".into());
        }
        remaining = &remaining[size + 1..];
    }
    if !remaining.is_empty() {
        return Err("extra Git blob data".into());
    }
    Ok((root, hashes))
}
