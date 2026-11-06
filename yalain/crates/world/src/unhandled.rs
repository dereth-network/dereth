//! What clients asked for that the server does not do yet: each kind logged the first time and
//! counted, so a live session lists exactly what players reached for.

use std::collections::BTreeMap;

/// The most kinds kept: a client can name as many as it likes (a function id, say), and each costs
/// memory and a line of the log. Past this they are counted under [`TOO_MANY`].
pub const MOST_KINDS: usize = 1024;

/// What every kind past [`MOST_KINDS`] counts as.
pub const TOO_MANY: &str = "more kinds than are kept";

/// The count of each unhandled request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Unhandled {
    counts: BTreeMap<String, u64>,
}

impl Unhandled {
    /// Note one request the server does not handle.
    pub fn note(&mut self, what: impl Into<String>) {
        let mut what = what.into();
        if self.counts.len() >= MOST_KINDS && !self.counts.contains_key(&what) {
            TOO_MANY.clone_into(&mut what);
        }
        let n = self.counts.entry(what.clone()).or_default();
        if *n == 0 {
            tracing::info!(%what, "not handled yet");
        }
        *n += 1;
    }

    /// How many times each kind was met.
    #[must_use]
    pub fn counts(&self) -> &BTreeMap<String, u64> {
        &self.counts
    }

    /// How many times one kind was met.
    #[must_use]
    pub fn count(&self, what: &str) -> u64 {
        self.counts.get(what).copied().unwrap_or(0)
    }
}
