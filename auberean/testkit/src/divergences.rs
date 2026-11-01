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
pub static DIVERGENCES: &[Divergence] = &[];

/// The divergence with an id.
#[must_use]
pub fn lookup(id: &str) -> Option<&'static Divergence> {
    DIVERGENCES.iter().find(|d| d.id == id)
}
