//! The client's own files -- preferences, keymaps, screen layouts -- kept in browser storage.
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
    read_only: |_| Ok(false),
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
}
