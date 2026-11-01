//! How reading a record fails: it does not decode ([`DecodeError`]). The container's own failures
//! are the shared container's (`dereth_dat::DatError`).

use core::fmt;

/// A record did not decode: what was wrong, and at which byte of the record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodeError {
    /// The offset in the record where the failing read started.
    pub at: usize,
    pub what: String,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "@{:#x}: {}", self.at, self.what)
    }
}

impl std::error::Error for DecodeError {}
