//! What clients asked for that the server does not do yet: each kind logged the first time and
//! counted, so a live session lists exactly what players reached for.

use std::collections::BTreeMap;

/// The count of each unhandled request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Unhandled {
    counts: BTreeMap<String, u64>,
}

impl Unhandled {
    /// Note one request the server does not handle.
    pub fn note(&mut self, what: impl Into<String>) {
        let what = what.into();
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
