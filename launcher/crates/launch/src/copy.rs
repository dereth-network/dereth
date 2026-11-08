//! Private dat sets: copying the shared set into one a world may write to.
//!
//! A world that patches dats over the network gets a set of its own, so that it can never change
//! the files every other world uses. The Dereth client is pointed at that set. A retail client
//! always plays with the dats beside it, so a private set is the Dereth client's alone.

use std::io;
use std::path::{Path, PathBuf};

use crate::datset::DatRole;

/// Copy a set's four dats into `to`, reporting `(done, total)` bytes as it goes.
///
/// This is the "Create a private copy (1.4 GB)" of the world page. The copies are made writable,
/// whatever the source's permissions were: a private set exists to be patched. A partial copy is
/// removed on failure, so a set is either complete or absent.
pub fn copy_dat_set(from: &Path, to: &Path, progress: &mut dyn FnMut(u64, u64)) -> io::Result<u64> {
    use std::io::{Read, Write};
    let files: Vec<(PathBuf, u64)> = DatRole::ALL
        .iter()
        .map(|r| from.join(r.file_name()))
        .filter_map(|p| {
            std::fs::metadata(&p)
                .ok()
                .filter(|m| m.is_file())
                .map(|m| (p, m.len()))
        })
        .collect();
    let total: u64 = files.iter().map(|(_, n)| n).sum();
    std::fs::create_dir_all(to)?;
    let mut done = 0u64;
    let result = (|| -> io::Result<()> {
        for (src, _) in &files {
            let name = src.file_name().expect("a dat path has a file name");
            let mut r = std::fs::File::open(src)?;
            let mut w = std::fs::File::create(to.join(name))?;
            let mut buf = vec![0u8; 1 << 20];
            loop {
                let n = r.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                w.write_all(&buf[..n])?;
                done += n as u64;
                progress(done, total);
            }
            w.sync_all()?;
        }
        Ok(())
    })();
    if let Err(e) = result {
        for (src, _) in &files {
            if let Some(name) = src.file_name() {
                let _ = std::fs::remove_file(to.join(name));
            }
        }
        return Err(e);
    }
    Ok(total)
}

/// Copy a whole install folder `from`, every file in every folder under it, into `to`, reporting
/// `(done, total)` bytes as it goes: an Asheron's Call 2 world's private copy of the retail
/// client's install, which it plays from and may write to. The copies are made writable. A link
/// in the folder refuses the copy, and a partial copy is removed on failure, so a copy is either
/// complete or absent.
///
/// # Errors
/// A file that cannot be read or written, or a link.
pub fn copy_install(from: &Path, to: &Path, progress: &mut dyn FnMut(u64, u64)) -> io::Result<u64> {
    use std::io::{Read, Write};
    fn walk(dir: &Path, rel: &Path, out: &mut Vec<(PathBuf, u64)>) -> io::Result<()> {
        for e in std::fs::read_dir(dir)? {
            let e = e?;
            let kind = e.file_type()?;
            let rel = rel.join(e.file_name());
            if kind.is_symlink() {
                return Err(io::Error::other("the install holds a link"));
            } else if kind.is_dir() {
                walk(&e.path(), &rel, out)?;
            } else if kind.is_file() {
                out.push((rel, e.metadata()?.len()));
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    walk(from, Path::new(""), &mut files)?;
    let total: u64 = files.iter().map(|(_, n)| n).sum();
    if to.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "the copy's folder is there already",
        ));
    }
    let mut done = 0u64;
    let result = (|| -> io::Result<()> {
        let mut buf = vec![0u8; 1 << 20];
        for (rel, _) in &files {
            let dest = to.join(rel);
            if let Some(dir) = dest.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let mut r = std::fs::File::open(from.join(rel))?;
            let mut w = std::fs::File::create(&dest)?;
            loop {
                let n = r.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                w.write_all(&buf[..n])?;
                done += n as u64;
                progress(done, total);
            }
            w.sync_all()?;
        }
        Ok(())
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_dir_all(to);
        return Err(e);
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datset::tests::{fake_set, tmp};
    use crate::datset::Iterations;

    #[test]
    fn a_private_copy_is_complete_writable_and_reported() {
        let root = tmp("copyset");
        fake_set(&root.join("from"), Iterations::END_OF_RETAIL);
        let mut last = (0, 0);
        let total = copy_dat_set(&root.join("from"), &root.join("to"), &mut |d, t| {
            last = (d, t)
        })
        .unwrap();
        assert_eq!(last, (total, total));
        for r in DatRole::ALL {
            let m = std::fs::metadata(root.join("to").join(r.file_name())).unwrap();
            assert!(!m.permissions().readonly());
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A private copy of an install holds every file of every folder in it, writable, with its
    /// bytes reported as they go; a copy whose folder is there already is refused and leaves it.
    #[test]
    fn a_private_copy_of_an_install_is_whole_writable_and_reported() {
        let root = tmp("copy-install");
        let from = root.join("install");
        std::fs::create_dir_all(from.join("data/sub")).unwrap();
        std::fs::write(from.join("AC2client.exe"), b"a client").unwrap();
        std::fs::write(from.join("data/portal.dat"), vec![7u8; 3 << 20]).unwrap();
        std::fs::write(from.join("data/sub/local.dat"), b"local").unwrap();
        let mut ro = std::fs::metadata(from.join("data/sub/local.dat"))
            .unwrap()
            .permissions();
        ro.set_readonly(true);
        std::fs::set_permissions(from.join("data/sub/local.dat"), ro).unwrap();
        let to = root.join("copies/dawnsong");
        let mut last = (0, 0);
        let total = copy_install(&from, &to, &mut |d, t| last = (d, t)).unwrap();
        assert_eq!(total, 8 + (3 << 20) + 5);
        assert_eq!(last, (total, total));
        assert_eq!(
            std::fs::read(to.join("AC2client.exe")).unwrap(),
            b"a client"
        );
        assert_eq!(
            std::fs::read(to.join("data/sub/local.dat")).unwrap(),
            b"local"
        );
        for f in ["AC2client.exe", "data/portal.dat", "data/sub/local.dat"] {
            assert!(
                !std::fs::metadata(to.join(f))
                    .unwrap()
                    .permissions()
                    .readonly(),
                "{f}"
            );
        }
        std::fs::write(to.join("data/portal.dat"), b"patched").unwrap();
        assert!(copy_install(&from, &to, &mut |_, _| {}).is_err());
        assert_eq!(
            std::fs::read(to.join("data/portal.dat")).unwrap(),
            b"patched",
            "a copy there already is left as it is"
        );
        let mut rw = std::fs::metadata(from.join("data/sub/local.dat"))
            .unwrap()
            .permissions();
        #[allow(clippy::permissions_set_readonly_false)] // so the folder can be removed
        rw.set_readonly(false);
        std::fs::set_permissions(from.join("data/sub/local.dat"), rw).unwrap();
        let _ = std::fs::remove_dir_all(&root);
    }
}
