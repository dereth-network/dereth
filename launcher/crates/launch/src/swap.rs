//! Putting a downloaded release in place of the running one, where no installer does it.
//!
//! On Windows the launcher ships as a zip, with no installer, and a running program's files can be
//! neither overwritten nor deleted there, but they can be renamed. So an update is unpacked into a
//! staging folder beside the launcher ([`STAGING_DIR`]), and each file and folder at the release's
//! top level is swapped in: what is there now is renamed aside (to `<name>`[`ASIDE_SUFFIX`]), the
//! new one is renamed into its place ([`swap_in`]), and the launcher restarts from the new files.
//! The next start removes what was set aside ([`clean_up`]), which the old process no longer
//! holds open.
//!
//! Every step is a rename inside one folder, so nothing is copied and a failure part-way is undone
//! by renaming back. macOS and Linux do not need this: the updater replaces the app bundle or the
//! AppImage itself.

use std::io;
use std::path::{Path, PathBuf};

/// The suffix of a file or folder set aside by an update, removed on the next start.
pub const ASIDE_SUFFIX: &str = ".dereth-old";

/// The folder beside the launcher that an update is unpacked into before it is swapped in.
pub const STAGING_DIR: &str = ".dereth-update";

/// The folder in an unpacked release that holds `exe_name`: `unpacked` itself, or the single
/// folder inside it (a release archived with a top-level folder of its own). `None` when neither
/// does, which means the download is not a release of this launcher.
pub fn release_root(unpacked: &Path, exe_name: &str) -> Option<PathBuf> {
    if unpacked.join(exe_name).is_file() {
        return Some(unpacked.to_path_buf());
    }
    let mut entries = std::fs::read_dir(unpacked).ok()?.flatten();
    let only = entries.next()?;
    if entries.next().is_some() {
        return None;
    }
    let dir = only.path();
    dir.join(exe_name).is_file().then_some(dir)
}

/// A free name to set `target` aside under: `<name>.dereth-old`, else `<name>.<n>.dereth-old`
/// when an earlier one is still there and cannot be removed (still held open).
fn aside_name(target: &Path) -> PathBuf {
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let first = target.with_file_name(format!("{name}{ASIDE_SUFFIX}"));
    if remove_any(&first) {
        return first;
    }
    (1u32..)
        .map(|n| target.with_file_name(format!("{name}.{n}{ASIDE_SUFFIX}")))
        .find(|p| remove_any(p))
        .expect("some aside name is free")
}

/// Remove a file or folder; true when nothing is at `path` afterwards.
fn remove_any(path: &Path) -> bool {
    match std::fs::symlink_metadata(path) {
        Err(_) => true,
        Ok(m) if m.is_dir() => std::fs::remove_dir_all(path).is_ok(),
        Ok(_) => std::fs::remove_file(path).is_ok(),
    }
}

/// One swap made, for undoing it.
struct Swapped {
    target: PathBuf,
    from: PathBuf,
    aside: Option<PathBuf>,
}

/// Swap every top-level entry of `release` into `dir`, setting aside what it replaces. Entries of
/// `dir` the release does not have are left as they are. Returns the names swapped in.
///
/// # Errors
/// A rename failed. Every swap made before it is undone, so `dir` is as it was.
pub fn swap_in(dir: &Path, release: &Path) -> io::Result<Vec<String>> {
    swap_in_with(dir, release, &mut |from, to| std::fs::rename(from, to))
}

/// [`swap_in`], renaming with `rename`, which the tests make fail on purpose.
fn swap_in_with(
    dir: &Path,
    release: &Path,
    rename: &mut dyn FnMut(&Path, &Path) -> io::Result<()>,
) -> io::Result<Vec<String>> {
    let mut names: Vec<_> = std::fs::read_dir(release)?
        .map(|e| e.map(|e| e.file_name()))
        .collect::<io::Result<_>>()?;
    names.sort();
    let mut done: Vec<Swapped> = Vec::new();
    for name in &names {
        let from = release.join(name);
        let target = dir.join(name);
        let mut step = || -> io::Result<Option<PathBuf>> {
            let aside = if std::fs::symlink_metadata(&target).is_ok() {
                let aside = aside_name(&target);
                rename(&target, &aside)?;
                Some(aside)
            } else {
                None
            };
            if let Err(e) = rename(&from, &target) {
                if let Some(a) = &aside {
                    let _ = std::fs::rename(a, &target);
                }
                return Err(e);
            }
            Ok(aside)
        };
        match step() {
            Ok(aside) => done.push(Swapped {
                target,
                from,
                aside,
            }),
            Err(e) => {
                for s in done.into_iter().rev() {
                    let _ = std::fs::rename(&s.target, &s.from);
                    if let Some(a) = s.aside {
                        let _ = std::fs::rename(a, &s.target);
                    }
                }
                return Err(e);
            }
        }
    }
    Ok(names
        .into_iter()
        .map(|n| n.to_string_lossy().into_owned())
        .collect())
}

/// Remove what an earlier update set aside in `dir`, and any staging folder it left. Anything
/// still held open stays for the next start. Returns how many entries were removed.
pub fn clean_up(dir: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            name.ends_with(ASIDE_SUFFIX) || name == STAGING_DIR
        })
        .filter(|e| remove_any(&e.path()))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datset::tests::tmp;

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn read(path: &Path) -> String {
        std::fs::read_to_string(path).unwrap()
    }

    #[test]
    fn an_update_sets_the_old_files_aside_puts_the_new_in_place_and_the_next_start_clears_them() {
        let dir = tmp("swap");
        write(&dir.join("dereth.exe"), "launcher 1");
        write(&dir.join("dereth-client.exe"), "client 1");
        write(&dir.join("licenses/NOTICE.txt"), "notice 1");
        write(&dir.join("mine.txt"), "the player's");
        let staged = dir.join(STAGING_DIR);
        write(&staged.join("dereth-0.2.0/dereth.exe"), "launcher 2");
        write(&staged.join("dereth-0.2.0/dereth-client.exe"), "client 2");
        write(&staged.join("dereth-0.2.0/licenses/NOTICE.txt"), "notice 2");

        let release =
            release_root(&staged, "dereth.exe").expect("the single top folder holds the launcher");
        let swapped = swap_in(&dir, &release).unwrap();
        assert_eq!(swapped, ["dereth-client.exe", "dereth.exe", "licenses"]);
        assert_eq!(read(&dir.join("dereth.exe")), "launcher 2");
        assert_eq!(read(&dir.join("dereth-client.exe")), "client 2");
        assert_eq!(read(&dir.join("licenses/NOTICE.txt")), "notice 2");
        assert_eq!(
            read(&dir.join("dereth.exe.dereth-old")),
            "launcher 1",
            "the running one is only renamed"
        );
        assert_eq!(
            read(&dir.join("licenses.dereth-old/NOTICE.txt")),
            "notice 1"
        );
        assert_eq!(read(&dir.join("mine.txt")), "the player's");

        assert_eq!(clean_up(&dir), 4, "three set aside, and the staging folder");
        let mut left: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        left.sort();
        assert_eq!(
            left,
            ["dereth-client.exe", "dereth.exe", "licenses", "mine.txt"]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_second_update_before_the_clean_up_finds_another_aside_name() {
        let dir = tmp("swap-twice");
        write(&dir.join("dereth.exe"), "2");
        write(&dir.join("dereth.exe.dereth-old"), "1");
        write(&dir.join("new/dereth.exe"), "3");
        swap_in(&dir, &dir.join("new")).unwrap();
        assert_eq!(read(&dir.join("dereth.exe")), "3");
        // The stale one could be removed here, so its name is reused; on Windows, where the file
        // may still be open, the numbered name is taken instead.
        assert_eq!(read(&dir.join("dereth.exe.dereth-old")), "2");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failed_swap_puts_everything_back() {
        let dir = tmp("swap-fail");
        write(&dir.join("a.txt"), "old a");
        write(&dir.join("b.txt"), "old b");
        write(&dir.join("new/a.txt"), "new a");
        write(&dir.join("new/b.txt"), "new b");
        // The fourth rename (the new `b.txt` into place) fails, as a file held open would.
        let mut n = 0;
        let result = swap_in_with(&dir, &dir.join("new"), &mut |from, to| {
            n += 1;
            if n == 4 {
                return Err(io::Error::new(io::ErrorKind::PermissionDenied, "held open"));
            }
            std::fs::rename(from, to)
        });
        assert!(result.is_err());
        assert_eq!(
            read(&dir.join("a.txt")),
            "old a",
            "the first swap was undone"
        );
        assert_eq!(read(&dir.join("b.txt")), "old b", "the second was put back");
        assert_eq!(read(&dir.join("new/a.txt")), "new a");
        assert_eq!(read(&dir.join("new/b.txt")), "new b");
        assert_eq!(clean_up(&dir), 0, "nothing was left aside");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_download_without_the_launcher_is_not_a_release() {
        let dir = tmp("swap-root");
        write(&dir.join("x/readme.txt"), "");
        assert_eq!(release_root(&dir, "dereth.exe"), None);
        write(&dir.join("dereth.exe"), "");
        assert_eq!(
            release_root(&dir, "dereth.exe"),
            Some(dir.clone()),
            "the launcher at the top level"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
