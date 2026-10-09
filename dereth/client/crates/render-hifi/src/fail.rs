//! The failure latch: the first failure is logged once and the presentation stays off until its
//! settings change.

use crate::HifiError;

/// Whether the presentation has failed, and how.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FailLatch {
    failed: Option<String>,
}

impl FailLatch {
    /// Record `error`. Returns whether this is the first failure, the one to log.
    pub fn trip(&mut self, error: &HifiError) -> bool {
        if self.failed.is_some() {
            return false;
        }
        self.failed = Some(error.to_string());
        true
    }

    /// The first failure, once one has happened.
    #[must_use]
    pub fn failed(&self) -> Option<&str> {
        self.failed.as_deref()
    }

    /// Forget the failure, as a change of settings does.
    pub fn reset(&mut self) {
        self.failed = None;
    }
}
