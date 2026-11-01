//! The places this client deliberately differs from retail. Each is named, never numbered:
//! `CD2-` and a short lowercase name joined by hyphens (`CD2-wider-view-distance`). A behaviour row
//! stamped `THIS_CLIENT` names the divergence it is part of.

/// One deliberate difference from retail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Divergence {
    pub id: &'static str,
    /// What this client does differently, in words a player would recognize.
    pub says: &'static str,
}

/// Every divergence, in id order.
pub static DIVERGENCES: &[Divergence] = &[
    Divergence {
        id: "CD2-checked-script-machine",
        says: "Where the retail script machine would run past the end of a stack, a table or the \
               code (a native nothing is bound to, a stack overflow, a reference to nothing, an \
               integer division by zero), this one stops the script cleanly and says why instead \
               of crashing the game or going on with garbage.",
    },
    Divergence {
        id: "CD2-data-read-again",
        says: "A record the data files hold that fails to read is read again before it counts as missing, so one bad read on a busy machine does not lose it for the rest of the session.",
    },
    Divergence {
        id: "CD2-script-step-budget",
        says: "Every script run has a step budget: one that runs too long is stopped instead of \
               freezing the game, where the retail client would hang.",
    },
];

/// The divergence with an id.
#[must_use]
pub fn lookup(id: &str) -> Option<&'static Divergence> {
    DIVERGENCES.iter().find(|d| d.id == id)
}
