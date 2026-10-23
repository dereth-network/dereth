//! Bounded extraction into a new component staging folder.

use dereth_launch::components::{safe_member, MAX_UNPACKED_BYTES};
use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::path::Path;

struct Extractor<'a> {
    root: &'a Path,
    names: BTreeSet<String>,
    remaining: u64,
}
impl Extractor<'_> {
    fn member(
        &mut self,
        name: &str,
        directory: bool,
        size: u64,
        executable: bool,
        input: &mut dyn Read,
    ) -> Result<(), String> {
        let name = name.trim_end_matches('/');
        if !safe_member(name) || !self.names.insert(name.to_ascii_lowercase()) {
            return Err("The archive contains an unsafe or duplicate path.".into());
        }
        if self.names.len() > 100_000 || size > self.remaining {
            return Err("The archive exceeds the installation limit.".into());
        }
        self.remaining -= size;
        let path = self.root.join(name);
        if directory {
            std::fs::create_dir_all(path).map_err(|e| e.to_string())?;
        } else {
            std::fs::create_dir_all(path.parent().ok_or("Invalid archive path")?)
                .map_err(|e| e.to_string())?;
            let mut output = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .map_err(|e| e.to_string())?;
            let copied =
                std::io::copy(&mut input.take(size + 1), &mut output).map_err(|e| e.to_string())?;
            if copied != size {
                return Err("The archive contains a truncated or oversized file.".into());
            }
            output.flush().map_err(|e| e.to_string())?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(
                    &path,
                    std::fs::Permissions::from_mode(if executable { 0o755 } else { 0o644 }),
                )
                .map_err(|e| e.to_string())?;
            }
            #[cfg(not(unix))]
            let _ = executable;
        }
        Ok(())
    }
}

/// The caller owns a fresh, empty staging folder. Links and special files are refused.
pub fn extract(bytes: &[u8], filename: &str, root: &Path) -> Result<(), String> {
    if std::fs::read_dir(root)
        .map_err(|e| e.to_string())?
        .next()
        .is_some()
    {
        return Err("The installation staging folder is not empty.".into());
    }
    let mut out = Extractor {
        root,
        names: BTreeSet::new(),
        remaining: MAX_UNPACKED_BYTES,
    };
    if filename.ends_with(".zip") {
        let mut archive =
            zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
        for i in 0..archive.len() {
            let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
            let mode = file.unix_mode().unwrap_or(0);
            let kind = mode & 0o170000;
            if !matches!(kind, 0 | 0o100000 | 0o040000) {
                return Err(
                    "Links and special files are not allowed in component archives.".into(),
                );
            }
            let name = file.name().to_owned();
            out.member(
                &name,
                file.is_dir(),
                file.size(),
                mode & 0o111 != 0,
                &mut file,
            )?;
        }
    } else if filename.ends_with(".tar.gz") {
        let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
        for entry in archive.entries().map_err(|e| e.to_string())? {
            let mut file = entry.map_err(|e| e.to_string())?;
            let kind = file.header().entry_type();
            if !kind.is_file() && !kind.is_dir() {
                return Err(
                    "Links and special files are not allowed in component archives.".into(),
                );
            }
            let name =
                String::from_utf8(file.path_bytes().into_owned()).map_err(|e| e.to_string())?;
            let size = file.size();
            let mode = file.header().mode().map_err(|e| e.to_string())?;
            out.member(&name, kind.is_dir(), size, mode & 0o111 != 0, &mut file)?;
        }
    } else {
        return Err("Unsupported component archive.".into());
    }
    if out.names.is_empty() {
        return Err("The component archive is empty.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, bytes) in entries {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }
    fn scratch() -> std::path::PathBuf {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "component-archive-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        path
    }
    #[test]
    fn extraction_writes_nested_files_and_refuses_overwriting_an_installation() {
        let root = scratch();
        let bytes = zip(&[("package/bin/program", b"program")]);
        extract(&bytes, "component.zip", &root).unwrap();
        assert_eq!(
            std::fs::read(root.join("package/bin/program")).unwrap(),
            b"program"
        );
        assert!(extract(&bytes, "component.zip", &root).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn extraction_refuses_parent_paths_and_case_collisions() {
        for entries in [
            vec![("../outside", b"bad".as_slice())],
            vec![
                ("BIN/app", b"one".as_slice()),
                ("bin/APP", b"two".as_slice()),
            ],
        ] {
            let root = scratch();
            assert!(extract(&zip(&entries), "component.zip", &root).is_err());
            std::fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn tar_archives_extract_files_and_refuse_links() {
        for link in [false, true] {
            let root = scratch();
            let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            let mut archive = tar::Builder::new(encoder);
            let mut header = tar::Header::new_gnu();
            header.set_path("package/program").unwrap();
            header.set_mode(0o755);
            if link {
                header.set_entry_type(tar::EntryType::Symlink);
                header.set_link_name("../../outside").unwrap();
                header.set_size(0);
            } else {
                header.set_size(3);
            }
            header.set_cksum();
            archive
                .append(
                    &header,
                    if link {
                        b"".as_slice()
                    } else {
                        b"app".as_slice()
                    },
                )
                .unwrap();
            let bytes = archive.into_inner().unwrap().finish().unwrap();
            let result = extract(&bytes, "component.tar.gz", &root);
            if link {
                assert!(result.is_err());
            } else {
                result.unwrap();
                assert_eq!(std::fs::read(root.join("package/program")).unwrap(), b"app");
            }
            std::fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn oversized_members_are_refused_before_creating_a_file() {
        let root = scratch();
        let mut out = Extractor {
            root: &root,
            names: BTreeSet::new(),
            remaining: 10,
        };
        assert!(out
            .member("program", false, 11, false, &mut b"".as_slice())
            .is_err());
        assert!(!root.join("program").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}
