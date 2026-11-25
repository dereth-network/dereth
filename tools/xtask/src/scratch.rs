//! A test's folder in the temporary directory, made anew and removed with everything in it when
//! dropped, so a run leaves nothing behind even when a test fails.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// How many scratch folders this process has made: each one's number.
static MADE: AtomicU64 = AtomicU64::new(0);

/// The folder `name`, with the process id and the folder's own number after it, so runs side by
/// side and tests of one run on parallel threads keep apart.
pub(crate) struct Scratch(PathBuf);

impl Scratch {
    pub(crate) fn new(name: &str) -> Self {
        let n = MADE.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("{name}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a temp directory");
        Self(dir)
    }
}

impl std::ops::Deref for Scratch {
    type Target = PathBuf;
    fn deref(&self) -> &PathBuf {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
