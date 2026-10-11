//! The Dereth Network's conformance suite: the vectors every implementation agrees on, the checks
//! any hub must pass over its WebSocket, and the checks any world must pass with the suite playing
//! the hub.
//!
//! **Depends on** `dereth-network-protocol`. **Used by** the hub's tests (`dereth-network-hub`),
//! Empyrean's network scenarios (`empyrean-testkit`) and the `dereth-network-conformance` binary.
//!
//! **Must never** need anything of an implementation but its public face: a hub's address and
//! keys, or a world's [`world::WorldUnderTest`]. The vectors are plain JSON (`vectors/`), so an
//! implementation in any language runs the same data.
//!
//! **Specified in** `docs/network/06-conformance.md`. [`vectors`] runs the data against this
//! repository's protocol crate; [`hub`] holds the hub checks; [`world`] the world checks.

pub mod hub;
pub mod vectors;
pub mod world;

use std::fmt;

/// One check's outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    /// What is being checked: `addresses`, `text rules`, `link messages`, `hub` or `world`.
    pub suite: &'static str,
    /// The check's name.
    pub name: &'static str,
    pub result: Result<(), String>,
}

impl Check {
    #[must_use]
    pub fn new(suite: &'static str, name: &'static str, result: Result<(), String>) -> Self {
        Self {
            suite,
            name,
            result,
        }
    }
}

impl fmt::Display for Check {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.result {
            Ok(()) => write!(f, "pass  {} :: {}", self.suite, self.name),
            Err(e) => write!(f, "FAIL  {} :: {}: {e}", self.suite, self.name),
        }
    }
}

/// The failures among `checks`.
#[must_use]
pub fn failures(checks: &[Check]) -> Vec<&Check> {
    checks.iter().filter(|c| c.result.is_err()).collect()
}
