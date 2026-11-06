//! The client's own files (its preferences, its key maps, what the game's scripts keep) in browser
//! storage.
//!
//! A worker can read and write browser storage synchronously only through a file it opened
//! beforehand, so all of them live in one: the worker opens it before the client starts, hands its
//! bytes to [`install`], and gets the whole store back through the save hook after every write.

use std::cell::RefCell;
use std::collections::BTreeMap;

use dereth_primitives::bundle;

/// Where every change to the store is handed, as the whole store's new bytes.
pub type SaveHook = fn(&[u8]);

thread_local! {
    static FILES: RefCell<BTreeMap<String, Vec<u8>>> = const { RefCell::new(BTreeMap::new()) };
    static SAVE: RefCell<Option<SaveHook>> = const { RefCell::new(None) };
}

fn saved() {
    let whole = FILES.with(|f| {
        bundle::encode(
            f.borrow()
                .iter()
                .map(|(name, data)| (name.as_str(), data.as_slice())),
        )
    });
    if let Some(save) = SAVE.with(|s| *s.borrow()) {
        save(&whole);
    }
}

/// Keep this thread's client files in memory, starting from `stored` (a store's bytes, empty for
/// none), and hand every change to `save`.
pub fn install(stored: &[u8], save: Option<SaveHook>) {
    FILES.with(|f| *f.borrow_mut() = bundle::decode(stored));
    SAVE.with(|s| *s.borrow_mut() = save);
}

/// The file `name`.
#[must_use]
pub fn read(name: &str) -> Option<Vec<u8>> {
    FILES.with(|f| f.borrow().get(name).cloned())
}

/// Keep `bytes` as the file `name`.
pub fn write(name: &str, bytes: &[u8]) {
    FILES.with(|f| f.borrow_mut().insert(name.to_owned(), bytes.to_vec()));
    saved();
}

/// Add `bytes` to the end of the file `name`, making it when there is none.
pub fn append(name: &str, bytes: &[u8]) {
    FILES.with(|f| {
        f.borrow_mut()
            .entry(name.to_owned())
            .or_default()
            .extend_from_slice(bytes);
    });
    saved();
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (a browser's way of keeping the client's files: nothing of the game's)
    use super::*;

    #[test]
    fn a_write_is_read_back_and_saved_whole() {
        thread_local! {
            static SAVED: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
        }
        install(
            &[],
            Some(|bytes| SAVED.with(|s| *s.borrow_mut() = bytes.to_vec())),
        );
        write("Preferences.ini", b"[Display]");
        append("chat.txt", b"one");
        append("chat.txt", b" two");
        assert_eq!(read("chat.txt").as_deref(), Some(&b"one two"[..]));
        let kept = bundle::decode(&SAVED.with(|s| s.borrow().clone()));
        assert_eq!(kept["Preferences.ini"], b"[Display]");
        assert_eq!(kept["chat.txt"], b"one two");
    }

    #[test]
    fn what_was_kept_before_is_there_after_the_store_is_opened_again() {
        let kept = bundle::encode([("Preferences.ini", &b"[Display]"[..])]);
        install(&kept, None);
        assert_eq!(read("Preferences.ini").as_deref(), Some(&b"[Display]"[..]));
        assert_eq!(read("nothing"), None);
    }
}
