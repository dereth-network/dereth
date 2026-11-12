//! A world's private copy of a retail client's whole install: every file of every folder in it,
//! made writable, which a world that patches the client's data files is played from, so the
//! player's own install is only ever read. The launcher makes one as a world is played, and the
//! world builder as it plays a world with the retail client.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// Every file of every folder under the install folder `from`, by its path within it, with its size.
fn install_files(from: &Path) -> io::Result<Vec<(PathBuf, u64)>> {
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
    Ok(files)
}

/// The bytes a copy of the install folder `from` takes ([`copy_install`]).
///
/// # Errors
/// A folder that cannot be read, or a link.
pub fn install_size(from: &Path) -> io::Result<u64> {
    Ok(install_files(from)?.iter().map(|(_, n)| n).sum())
}

/// Copy a whole install folder `from`, every file in every folder under it, into `to`, reporting
/// `(done, total)` bytes as it goes: a world's private copy of a retail client's install,
/// which it plays from and may write to. The copies are made writable. A link
/// in the folder refuses the copy, and so does `cancel` set while it goes. A partial copy is
/// removed on failure, so a copy is either complete or absent.
///
/// # Errors
/// A file that cannot be read or written, a link, or the copy cancelled.
pub fn copy_install(
    from: &Path,
    to: &Path,
    progress: &mut dyn FnMut(u64, u64),
    cancel: &AtomicBool,
) -> io::Result<u64> {
    use std::io::{Read, Write};
    let files = install_files(from)?;
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
                if cancel.load(Ordering::Relaxed) {
                    return Err(io::Error::other("the copy was cancelled"));
                }
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
    //! Behaviour: none (tooling: copying an install folder for a world to play from)
    use super::*;

    /// A fresh scratch folder for one test.
    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("dereth-dat-copy-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A private copy of an install holds every file of every folder in it, writable, with its
    /// bytes reported as they go; a copy whose folder is there already is refused and leaves it.
    #[test]
    fn a_private_copy_of_an_install_is_whole_writable_and_reported() {
        let root = tmp("install");
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
        let go = AtomicBool::new(false);
        assert_eq!(install_size(&from).unwrap(), 8 + (3 << 20) + 5);
        let total = copy_install(&from, &to, &mut |d, t| last = (d, t), &go).unwrap();
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
        assert!(copy_install(&from, &to, &mut |_, _| {}, &go).is_err());
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

    /// A copy of an install cancelled while it goes stops, and leaves no folder behind.
    #[test]
    fn a_cancelled_copy_of_an_install_leaves_nothing() {
        let root = tmp("install-cancel");
        let from = root.join("install");
        std::fs::create_dir_all(&from).unwrap();
        std::fs::write(from.join("AC2client.exe"), b"a client").unwrap();
        std::fs::write(from.join("portal.dat"), vec![7u8; 3 << 20]).unwrap();
        let to = root.join("copies/dawnsong");
        let cancel = AtomicBool::new(false);
        let mut seen = 0;
        let copied = copy_install(
            &from,
            &to,
            &mut |_, _| {
                seen += 1;
                cancel.store(true, Ordering::Relaxed);
            },
            &cancel,
        );
        assert!(copied.is_err());
        assert_eq!(seen, 1, "it stops at the next block");
        assert!(!to.exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}
