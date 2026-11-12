//! Private dat sets: copying the shared set into one a world may write to.
//!
//! A world that patches dats over the network gets a set of its own, so that it can never change
//! the files every other world uses. The Dereth client is pointed at that set. A retail client
//! always plays with the dats beside it, so a private set is the Dereth client's alone.

use std::io;
use std::path::{Path, PathBuf};

/// A world's private copy of a whole install, which the world builder makes too.
pub use dereth_dat::install_copy::{copy_install, install_size};

use crate::datset::DatRole;

/// The folder of the launcher's library that holds Asheron's Call 2 worlds' copies of the retail
/// client's install, each `copy-<n>`.
pub const RETAIL_COPIES: &str = "auberean-copies";

/// The lock a copy of the install holds while it is made, beside it: `copy-<n>.lock`.
fn lock_file(copy: &Path) -> PathBuf {
    copy.with_extension("lock")
}

/// Hold the lock of `copy`, a folder in [`RETAIL_COPIES`], while it is made: a launcher started
/// meanwhile leaves it be ([`sweep_unfinished_copies`]). It is let go when the file is dropped.
///
/// # Errors
/// The lock file cannot be made, or another launcher holds it.
pub fn lock_copy(copy: &Path) -> io::Result<std::fs::File> {
    if let Some(dir) = copy.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let f = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(lock_file(copy))?;
    f.try_lock().map_err(|e| match e {
        std::fs::TryLockError::WouldBlock => {
            io::Error::other("another launcher is making this copy")
        }
        std::fs::TryLockError::Error(e) => e,
    })?;
    Ok(f)
}

/// Let go of the lock of `copy` held while it was made ([`lock_copy`]), and remove its file.
pub fn unlock_copy(copy: &Path, lock: std::fs::File) {
    // Closed before it is removed; a browser's file has no handle to close.
    #[cfg_attr(target_arch = "wasm32", allow(clippy::drop_non_drop))]
    drop(lock);
    let _ = std::fs::remove_file(lock_file(copy));
}

/// Remove every copy of the install in `copies` (the library's [`RETAIL_COPIES`]) that was never
/// finished: a `copy-<n>` folder that isn't one of the copies recorded as made (`kept`) and that
/// no launcher is making now, with its lock. A launcher that stopped part way through a copy
/// leaves one. What was removed.
pub fn sweep_unfinished_copies<'a>(
    copies: &Path,
    kept: impl IntoIterator<Item = &'a PathBuf>,
) -> Vec<PathBuf> {
    let same = |p: &Path| std::fs::canonicalize(p).ok();
    let kept: Vec<PathBuf> = kept.into_iter().filter_map(|p| same(p)).collect();
    let Ok(entries) = std::fs::read_dir(copies) else {
        return Vec::new();
    };
    let mut removed = Vec::new();
    for e in entries.flatten() {
        let path = e.path();
        let name = e.file_name();
        let Some(n) = name.to_str().and_then(|n| n.strip_prefix("copy-")) else {
            continue;
        };
        if n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit()) || !path.is_dir() {
            continue;
        }
        if same(&path).is_some_and(|p| kept.contains(&p)) {
            continue;
        }
        // One being made holds its lock; taking it means no launcher is.
        let Ok(lock) = lock_copy(&path) else {
            continue;
        };
        if std::fs::remove_dir_all(&path).is_ok() {
            removed.push(path.clone());
        }
        unlock_copy(&path, lock);
    }
    removed
}

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

#[cfg(test)]
mod tests {
    use super::*;

    /// At a launcher's start, a copy of the install that was never recorded as made (one a
    /// launcher stopped part way through) is removed; a recorded copy, one another launcher is
    /// making, and anything else in the folder are left.
    #[test]
    fn copies_never_finished_are_swept_and_recorded_ones_kept() {
        let root = tmp("copy-sweep");
        let copies = root.join(RETAIL_COPIES);
        for n in 1..=3 {
            std::fs::create_dir_all(copies.join(format!("copy-{n}")).join("data")).unwrap();
            std::fs::write(
                copies.join(format!("copy-{n}")).join("data/portal.dat"),
                b"p",
            )
            .unwrap();
        }
        std::fs::create_dir_all(copies.join("other")).unwrap();
        std::fs::write(copies.join("notes.txt"), b"n").unwrap();
        let making = lock_copy(&copies.join("copy-3")).unwrap();
        let kept = [copies.join("copy-1")];
        let removed = sweep_unfinished_copies(&copies, kept.iter());
        assert_eq!(removed, vec![copies.join("copy-2")]);
        assert!(copies.join("copy-1/data/portal.dat").is_file(), "recorded");
        assert!(!copies.join("copy-2").exists(), "never finished");
        assert!(
            copies.join("copy-3/data/portal.dat").is_file(),
            "being made"
        );
        assert!(copies.join("other").is_dir() && copies.join("notes.txt").is_file());
        drop(making);
        let removed = sweep_unfinished_copies(&copies, kept.iter());
        assert_eq!(removed, vec![copies.join("copy-3")], "no longer being made");
        assert!(sweep_unfinished_copies(&root.join("none"), kept.iter()).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }
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
}
