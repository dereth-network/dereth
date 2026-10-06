//! The client's own files -- preferences, keymaps, screen layouts, and the caches of work it can
//! redo -- kept in browser storage.
//!
//! A worker can read and write browser storage synchronously only through a file it opened
//! beforehand, so all of them live in one: the page opens it before the client starts, hands its
//! bytes to [`install`], and gets the whole store back through the save hook after every write.
//! In between they are a map in memory, installed as the runtime's file store.
//!
//! The store's bytes are the files one after another, each as its path's length and UTF-8 bytes
//! and then its contents' length and bytes, lengths little-endian `u32`.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use dereth_client_runtime::platform::files::{self, FileHost};

/// Every file, by path.
pub type Files = BTreeMap<PathBuf, Vec<u8>>;

/// Where every change to the store is handed, as the whole store's new bytes.
type SaveHook = fn(&[u8]);

thread_local! {
    static FILES: RefCell<Files> = const { RefCell::new(BTreeMap::new()) };
    static SAVE: RefCell<Option<SaveHook>> = const { RefCell::new(None) };
}

/// The files in a store's bytes. A short or malformed tail is dropped: what came before it is kept.
#[must_use]
pub fn decode(mut bytes: &[u8]) -> Files {
    let mut files = Files::new();
    let take = |bytes: &mut &[u8]| -> Option<Vec<u8>> {
        let (len, rest) = bytes.split_first_chunk::<4>()?;
        let len = usize::try_from(u32::from_le_bytes(*len)).ok()?;
        if rest.len() < len {
            return None;
        }
        let (item, rest) = rest.split_at(len);
        *bytes = rest;
        Some(item.to_vec())
    };
    while !bytes.is_empty() {
        let (Some(path), Some(data)) = (take(&mut bytes), take(&mut bytes)) else {
            break;
        };
        let Ok(path) = String::from_utf8(path) else {
            break;
        };
        files.insert(PathBuf::from(path), data);
    }
    files
}

/// A store's bytes for `files`.
#[must_use]
pub fn encode(files: &Files) -> Vec<u8> {
    let mut out = Vec::new();
    let mut put = |item: &[u8]| {
        out.extend_from_slice(&u32::try_from(item.len()).unwrap_or(u32::MAX).to_le_bytes());
        out.extend_from_slice(item);
    };
    for (path, data) in files {
        put(path.to_string_lossy().as_bytes());
        put(data);
    }
    out
}

/// The file store the runtime is given: the map, saved whole after each write.
const STORE: FileHost = FileHost {
    read: |p| {
        FILES
            .with(|f| f.borrow().get(p).cloned())
            .ok_or_else(|| io::ErrorKind::NotFound.into())
    },
    write: |p, bytes| {
        let whole = FILES.with(|f| {
            let mut f = f.borrow_mut();
            f.insert(p.to_path_buf(), bytes.to_vec());
            encode(&f)
        });
        if let Some(save) = SAVE.with(|s| *s.borrow()) {
            save(&whole);
        }
        Ok(())
    },
    list: |dir| {
        Ok(FILES.with(|f| {
            f.borrow()
                .keys()
                .filter(|p| p.parent() == Some(dir))
                .cloned()
                .collect()
        }))
    },
    exists: |p| FILES.with(|f| f.borrow().contains_key(p)),
    is_file: |p| FILES.with(|f| f.borrow().contains_key(p)),
    remove_file: |p| {
        let whole = FILES.with(|f| {
            let mut f = f.borrow_mut();
            f.remove(p).ok_or(io::ErrorKind::NotFound)?;
            Ok::<_, io::Error>(encode(&f))
        })?;
        if let Some(save) = SAVE.with(|s| *s.borrow()) {
            save(&whole);
        }
        Ok(())
    },
    // Directories are virtual, but nested files still prevent retiring their parent.
    remove_empty_dir: |dir| Ok(FILES.with(|f| !f.borrow().keys().any(|p| p.starts_with(dir)))),
    read_only: |_| Ok(false),
    // Paths are only names here: there are no directories to make.
    make_dirs: |_| Ok(()),
    cache_dir: || Some(Path::new("/cache").to_path_buf()),
};

/// Keep this thread's client files in memory, starting from `stored` (a store's bytes, empty for
/// none), and hand every change to `save` as the whole store's new bytes.
pub fn install(stored: &[u8], save: Option<SaveHook>) {
    FILES.with(|f| *f.borrow_mut() = decode(stored));
    SAVE.with(|s| *s.borrow_mut() = save);
    files::install(STORE);
}

/// The paths the store holds, for the log.
#[must_use]
pub fn paths() -> Vec<PathBuf> {
    FILES.with(|f| f.borrow().keys().cloned().collect())
}

/// Where the preferences profile is kept.
#[must_use]
pub fn preferences_file() -> PathBuf {
    Path::new("/settings").join("UserPreferences.ini")
}

/// Where the overlay blocklist is kept: beside the preferences profile, as the desktop client
/// keeps it.
fn blocklist_file() -> PathBuf {
    dereth_client_runtime::world_overlay::blocklist_file(&dereth_client_runtime::config::Config {
        preferences_file: preferences_file(),
        ..dereth_client_runtime::config::Config::default()
    })
}

/// The worlds whose overlays the player refuses, as the client reads its blocklist.
#[must_use]
pub fn blocklist() -> std::collections::BTreeSet<String> {
    dereth_client_runtime::world_overlay::parse_blocklist(
        &files::read_to_string(&blocklist_file()).unwrap_or_default(),
    )
}

/// Put the world `key` on the blocklist, or take it off; the file's other lines (comments
/// included) are kept as they were.
///
/// # Errors
/// The store will not take the file.
pub fn set_blocked(key: &str, blocked: bool) -> io::Result<()> {
    let key = key.trim();
    let path = blocklist_file();
    let text = files::read_to_string(&path).unwrap_or_default();
    let mut lines: Vec<&str> = text.lines().filter(|l| l.trim() != key).collect();
    if blocked && !key.is_empty() {
        lines.push(key);
    }
    let mut out = lines.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    files::write(&path, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A store's bytes decode to the files they were encoded from, and a torn tail loses only
    /// itself.
    #[test]
    fn a_store_round_trips_and_a_torn_tail_loses_only_itself() {
        let mut files = Files::new();
        files.insert(preferences_file(), b"[Display]\r\nWidth=1024\r\n".to_vec());
        files.insert(PathBuf::from("/settings/acclient.keymap"), vec![0xFF; 3]);
        let bytes = encode(&files);
        assert_eq!(decode(&bytes), files);
        let torn = decode(&bytes[..bytes.len() - 1]);
        assert_eq!(torn.len(), 1);
        assert!(torn.contains_key(&preferences_file()));
    }

    /// Once installed, a write through the runtime's file store is read back, listed in its
    /// directory, and handed to the save hook as the whole store.
    #[test]
    fn a_write_is_read_back_and_saved_whole() {
        thread_local! {
            static SAVED: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
        }
        install(
            &[],
            Some(|bytes| SAVED.with(|s| *s.borrow_mut() = bytes.to_vec())),
        );
        let path = preferences_file();
        files::write(&path, "[Input]\r\nKeymapFile=acclient.keymap\r\n").expect("written");
        assert_eq!(
            files::read_to_string(&path).expect("read"),
            "[Input]\r\nKeymapFile=acclient.keymap\r\n"
        );
        assert_eq!(
            files::list(Path::new("/settings")).expect("listed"),
            vec![path.clone()]
        );
        assert_eq!(decode(&SAVED.with(|s| s.borrow().clone()))[&path].len(), 37);
    }
    /// The overlay blocklist is the client's own file beside the preferences: a world is put on
    /// it and taken off it, and its other lines stay.
    #[test]
    fn a_world_is_put_on_the_overlay_blocklist_and_taken_off_it() {
        install(&[], None);
        files::write(&blocklist_file(), "# refused worlds\nOld World\n").unwrap();
        set_blocked("Infiltration", true).unwrap();
        set_blocked("Infiltration", true).unwrap();
        assert_eq!(
            blocklist().into_iter().collect::<Vec<_>>(),
            ["Infiltration", "Old World"]
        );
        set_blocked("Old World", false).unwrap();
        assert_eq!(
            files::read_to_string(&blocklist_file()).unwrap(),
            "# refused worlds\nInfiltration\n"
        );
        assert_eq!(
            blocklist_file(),
            Path::new("/settings").join("overlay-blocklist.txt")
        );
        files::install(files::DISK);
    }

    #[test]
    fn deletion_saves_once_and_nested_files_prevent_directory_retirement() {
        thread_local! {
            static SAVES: RefCell<Vec<Vec<u8>>> = const { RefCell::new(Vec::new()) };
        }
        let dir = Path::new("/settings/old");
        let path = dir.join("nested/keys");
        let initial = Files::from([(path.clone(), b"bindings".to_vec())]);
        install(
            &encode(&initial),
            Some(|bytes| SAVES.with(|s| s.borrow_mut().push(bytes.to_vec()))),
        );
        assert!(files::is_file(&path));
        assert!(!files::is_file(dir));
        assert!(files::list(dir).unwrap().is_empty());
        assert!(!files::remove_empty_dir(dir).unwrap());
        files::remove_file(&path).unwrap();
        assert_eq!(
            files::remove_file(&path).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        assert!(files::remove_empty_dir(dir).unwrap());
        SAVES.with(|s| {
            assert_eq!(s.borrow().len(), 1);
            assert!(decode(&s.borrow()[0]).is_empty());
        });
        files::install(files::DISK);
    }
}
