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
}
