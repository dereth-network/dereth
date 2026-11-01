//! The behaviour registry: what this client claims, and which test asserts each claim.
//!
//! A [`Behaviour`] is a claim split into the part anyone may read (an id, a sentence, the build it
//! was read off) and an [`Evidence`] handle that resolves privately to what proves it. Tests name
//! the row they assert with a `/// Behaviour: <id>` line; the row names that test as its
//! `station`. `cargo xtask test-lint` checks both directions: every `Behaviour:` line names a row,
//! and every row's station is a live test.
//!
//! # Rows
//!
//! - `id`: dotted lower case, `<subject>.<thing>.<claim-in-hyphens>`. Renaming one renames a claim.
//! - `says`: the claim in words a player or a reader of the data would recognize: no address, no
//!   internal symbol.
//! - `since`: [`RETAIL`] (what the September 2013 client does), [`THIS_CLIENT`] (this client's own
//!   rule, naming its divergence) or [`TOOLING`] (a developer affordance).
//! - `evidence`: an opaque handle, `AC2-EVID-` and capital letters, digits and hyphens. It never
//!   spells an address or a source; the private index resolves it.
//! - `station`: the test that asserts the row, as `<package>::<binary>::<module path>::<fn>`.
//!
//! One file per subject, each exporting a `ROWS` slice in id order; [`SUBJECTS`] lists them in
//! name order.

/// Where a behaviour's proof lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Evidence {
    /// A handle that resolves only in the private evidence index.
    Private(&'static str),
    /// A published reference anyone can follow.
    Public(&'static str),
}

impl Evidence {
    /// The handle itself.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Private(s) | Self::Public(s) => s,
        }
    }
}

/// What the asserting test needs to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    /// Nothing private: no retail file, no capture.
    Cpu,
    /// The retail data files.
    Dat,
    /// The capture index.
    Corpus,
    /// A graphics device.
    Gpu,
    /// A server on loopback.
    Live,
}

impl Tier {
    /// The test binary this tier's tests live in.
    #[must_use]
    pub const fn binary(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Dat => "dat",
            Self::Corpus => "corpus",
            Self::Gpu => "gpu",
            Self::Live => "live",
        }
    }
}

/// One claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Behaviour {
    pub id: &'static str,
    pub says: &'static str,
    pub since: &'static str,
    /// The divergence this row is part of: `Some` exactly when `since` is [`THIS_CLIENT`].
    pub divergence: Option<&'static str>,
    pub evidence: Evidence,
    pub station: &'static str,
    pub tier: Tier,
}

/// Build one [`Behaviour`] row; a missing field is a compile error. `divergence` is written
/// straight after `since` on a [`THIS_CLIENT`] row.
#[macro_export]
macro_rules! behaviour {
    (
        id: $id:literal,
        says: $says:literal,
        since: $since:expr,
        $(divergence: $divergence:literal,)?
        evidence: $evidence:expr,
        station: $station:literal,
        tier: $tier:expr $(,)?
    ) => {
        $crate::behaviours::Behaviour {
            id: $id,
            says: $says,
            since: $since,
            divergence: $crate::behaviour!(@divergence $($divergence)?),
            evidence: $evidence,
            station: $station,
            tier: $tier,
        }
    };
    (@divergence) => {
        None
    };
    (@divergence $divergence:literal) => {
        Some($divergence)
    };
}

/// Retail as the September 2013 client (00.00.00.0930) does it: the build this workspace rebuilds.
/// A claim about the data files is checked against the end-of-retail set.
pub const RETAIL: &str = "retail 2013-09";

/// This client's own rule, where it deliberately differs from retail.
pub const THIS_CLIENT: &str = "this client";

/// A developer affordance a player never meets.
pub const TOOLING: &str = "tooling";

/// One subject's rows.
#[derive(Debug, Clone, Copy)]
pub struct Subject {
    pub name: &'static str,
    pub rows: &'static [Behaviour],
}

pub mod collision;
pub mod data;
pub mod detection;
pub mod missile;
pub mod motion;
pub mod parts;
pub mod pathfind;
pub mod protocol;
pub mod script;
pub mod shape;
pub mod text;
pub mod transition;
pub mod world;

/// Every subject, in name order. A new subject adds its `mod` line above and its entry here.
pub static SUBJECTS: &[Subject] = &[
    Subject {
        name: "collision",
        rows: collision::ROWS,
    },
    Subject {
        name: "data",
        rows: data::ROWS,
    },
    Subject {
        name: "detection",
        rows: detection::ROWS,
    },
    Subject {
        name: "missile",
        rows: missile::ROWS,
    },
    Subject {
        name: "motion",
        rows: motion::ROWS,
    },
    Subject {
        name: "parts",
        rows: parts::ROWS,
    },
    Subject {
        name: "pathfind",
        rows: pathfind::ROWS,
    },
    Subject {
        name: "protocol",
        rows: protocol::ROWS,
    },
    Subject {
        name: "script",
        rows: script::ROWS,
    },
    Subject {
        name: "shape",
        rows: shape::ROWS,
    },
    Subject {
        name: "text",
        rows: text::ROWS,
    },
    Subject {
        name: "transition",
        rows: transition::ROWS,
    },
    Subject {
        name: "world",
        rows: world::ROWS,
    },
];

/// Every row of every subject.
pub fn all() -> impl Iterator<Item = &'static Behaviour> {
    SUBJECTS.iter().flat_map(|s| s.rows.iter())
}

/// The row with an id.
#[must_use]
pub fn lookup(id: &str) -> Option<&'static Behaviour> {
    all().find(|b| b.id == id)
}
