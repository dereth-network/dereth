//! `empyrean-import ac1999 package`: a converted bundle as a release candidate that the same
//! inputs always produce byte for byte: a world ZIP, a source ZIP, `SHA256SUMS` and `release.json`.
//! Nothing is uploaded; publishing is a separate, deliberate step.
//!
//! Each ZIP holds its files in byte order of their paths, every entry stamped 2000-01-01 00:00:00
//! with mode 0644, deflated by the ZIP writer the lock file pins, with no directory entries and no
//! extra fields.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde_json::json;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

use super::source::hash;

pub const NAME: &str = "empyrean-world-launch";

fn options() -> Result<SimpleFileOptions, String> {
    let stamp = DateTime::from_date_and_time(2000, 1, 1, 0, 0, 0).map_err(|e| e.to_string())?;
    Ok(SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .compression_level(Some(6))
        .last_modified_time(stamp)
        .unix_permissions(0o644)
        .large_file(false))
}

/// A deterministic ZIP of `files` (archive path -> bytes).
pub fn zip(files: &BTreeMap<String, Vec<u8>>) -> Result<Vec<u8>, String> {
    let mut out = ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = options()?;
    for (name, bytes) in files {
        out.start_file(name.as_str(), options)
            .map_err(|e| e.to_string())?;
        out.write_all(bytes).map_err(|e| e.to_string())?;
    }
    Ok(out.finish().map_err(|e| e.to_string())?.into_inner())
}

/// Every file below `dir`, by its `/`-separated path relative to `dir` prefixed with `prefix`.
fn tree(dir: &Path, prefix: &str) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let mut out = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).map_err(|e| format!("{}: {e}", d.display()))? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
                stack.push(path);
                continue;
            }
            let rel = path
                .strip_prefix(dir)
                .map_err(|e| e.to_string())?
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            out.insert(
                format!("{prefix}{rel}"),
                std::fs::read(&path).map_err(|e| e.to_string())?,
            );
        }
    }
    Ok(out)
}

fn package(bundle: &Path, out: &Path) -> Result<(), String> {
    if out.exists() {
        return Err("output already exists; choose a new directory".into());
    }
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(bundle.join("manifest.json")).map_err(|e| format!("manifest.json: {e}"))?,
    )
    .map_err(|e| e.to_string())?;
    let source = &manifest["source"];
    let revision = source["revision"]
        .as_str()
        .ok_or("the bundle's manifest names no revision")?;
    let short = &revision[..revision.len().min(8)];

    // The world: every bundle file but the committed source, which ships beside it.
    let mut world = BTreeMap::new();
    for name in [
        "manifest.json",
        "changes.json",
        "empyrean-changes.json",
        "pack-report.json",
        "world.sql",
        "empyrean.sql",
        "world.pack",
    ] {
        world.insert(
            format!("{NAME}/{name}"),
            std::fs::read(bundle.join(name)).map_err(|e| format!("{name}: {e}"))?,
        );
    }
    // The committed source with the list it is checked against, as `--source` reads it back.
    let mut sources = tree(&bundle.join("source"), "source/")?;
    let mut described = serde_json::to_vec_pretty(&json!({"url":source["url"],
        "revision":revision,"files_sha256":source["files_sha256"]}))
    .map_err(|e| e.to_string())?;
    described.push(b'\n');
    sources.insert("source.json".into(), described);

    let names = [
        format!("{NAME}-{short}.zip"),
        format!("{NAME}-{short}-source.zip"),
    ];
    let archives = [zip(&world)?, zip(&sources)?];
    let parent = out
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let stage = parent.join(format!(
        "{}.partial-{}",
        out.file_name()
            .ok_or("output requires a directory name")?
            .to_string_lossy(),
        std::process::id()
    ));
    std::fs::create_dir(&stage).map_err(|e| format!("create staging directory: {e}"))?;
    let mut sums = String::new();
    let mut files = Vec::new();
    for (name, bytes) in names.iter().zip(&archives) {
        std::fs::write(stage.join(name), bytes).map_err(|e| e.to_string())?;
        sums.push_str(&format!("{}  {name}\n", hash(bytes)));
        files.push(json!({"name":name,"size":bytes.len(),"sha256":hash(bytes)}));
    }
    let release = json!({
        "name": NAME,
        "published": false,
        "source": {"url":source["url"],"revision":revision},
        "era": manifest["era"],
        "asset_target": manifest["asset_target"],
        "converter": manifest["converter"],
        "world_pack_sha256": manifest["world_pack_sha256"],
        "files": files,
        "zip": {"writer": env!("EMPYREAN_IMPORT_ZIP"), "method": "deflate", "level": 6,
            "timestamp": "2000-01-01T00:00:00", "mode": "0644", "order": "path bytes"},
        "rebuild": format!("empyrean-import ac1999 --revision {revision} --source {} --allow-incomplete --out <new-directory>", names[1]),
    });
    let mut text = serde_json::to_vec_pretty(&release).map_err(|e| e.to_string())?;
    text.push(b'\n');
    std::fs::write(stage.join("release.json"), &text).map_err(|e| e.to_string())?;
    sums.push_str(&format!("{}  release.json\n", hash(&text)));
    std::fs::write(stage.join("SHA256SUMS"), sums).map_err(|e| e.to_string())?;
    std::fs::rename(&stage, out).map_err(|e| e.to_string())
}

pub fn run(args: &[String]) -> ExitCode {
    let (mut bundle, mut out) = (None, None);
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let value = it.next().map(PathBuf::from);
        match arg.as_str() {
            "--bundle" if bundle.is_none() => bundle = value,
            "--out" if out.is_none() => out = value,
            _ => {
                eprintln!("empyrean-import ac1999 package: unknown or repeated option {arg}");
                return ExitCode::from(2);
            }
        }
    }
    let (Some(bundle), Some(out)) = (bundle, out) else {
        eprintln!("usage: empyrean-import ac1999 package --bundle <bundle> --out <new-directory>");
        return ExitCode::from(2);
    };
    match package(&bundle, &out) {
        Ok(()) => {
            println!(
                "wrote the unpublished candidate to {}; nothing was uploaded",
                out.display()
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("empyrean-import ac1999 package: {e}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    //! Divergence: V462
    //! The candidate's archives are the same bytes whatever order the files are given in and
    //! whenever they are written. Fixture: two synthetic files.

    use std::collections::BTreeMap;

    #[test]
    fn a_zip_of_the_same_files_is_the_same_bytes_with_fixed_stamps_and_modes() {
        let files: BTreeMap<String, Vec<u8>> = [
            ("b/world.sql".to_owned(), b"second".to_vec()),
            ("a/manifest.json".to_owned(), b"first".to_vec()),
        ]
        .into();
        let one = super::zip(&files).unwrap();
        let two = super::zip(&files.clone().into_iter().rev().collect()).unwrap();
        assert_eq!(one, two);
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(one)).unwrap();
        let names: Vec<_> = archive.file_names().map(str::to_owned).collect();
        assert_eq!(names, ["a/manifest.json", "b/world.sql"]);
        for i in 0..archive.len() {
            let entry = archive.by_index(i).unwrap();
            let stamp = entry.last_modified().unwrap();
            assert_eq!((stamp.year(), stamp.month(), stamp.day()), (2000, 1, 1));
            assert_eq!(entry.unix_mode().map(|m| m & 0o777), Some(0o644));
            assert!(entry.extra_data().is_none_or(<[u8]>::is_empty));
        }
    }
    /// What `fetch` takes from a release is what this writes: the packer's own layout, read back
    /// by the fetcher.
    #[test]
    fn the_fetcher_finds_the_pack_in_the_world_zip_a_candidate_writes() {
        let dir = std::env::temp_dir().join(format!("empyrean-package-{}", std::process::id()));
        let bundle = dir.join("bundle");
        std::fs::create_dir_all(bundle.join("source")).unwrap();
        let revision = "a".repeat(40);
        let manifest = serde_json::json!({
            "source": {"url": "https://example.invalid/x.git", "revision": revision, "files_sha256": {}},
            "asset_target": {}, "converter": {}, "era": "launch", "world_pack_sha256": "0",
        });
        std::fs::write(bundle.join("manifest.json"), manifest.to_string()).unwrap();
        for name in [
            "changes.json",
            "empyrean-changes.json",
            "pack-report.json",
            "world.sql",
            "empyrean.sql",
        ] {
            std::fs::write(bundle.join(name), b"{}").unwrap();
        }
        std::fs::write(bundle.join("world.pack"), b"the pack").unwrap();
        std::fs::write(bundle.join("source/LICENSE"), b"x").unwrap();
        let out = dir.join("out");
        super::package(&bundle, &out).unwrap();
        let zip = std::fs::read(out.join(format!("{}-aaaaaaaa.zip", super::NAME))).unwrap();
        let pack = crate::fetch::unzip_pack(std::io::Cursor::new(zip), "w.pack", &dir).unwrap();
        assert_eq!(std::fs::read(pack).unwrap(), b"the pack");
        std::fs::remove_dir_all(&dir).ok();
    }
}
