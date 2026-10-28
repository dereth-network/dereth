//! Where a row of a pack came from: the data files, recorded sessions, a stated rule, or a person.
//!
//! A row says so with `prov`, an inline table whose `from` names the source; a field of the row
//! whose source differs from the row's says so in `prov.fields`:
//!
//! ```toml
//! [[static]]
//! model = 0x0200040B
//! at = [186.0, 30.0, "ground"]
//! prov = { from = "dat", record = 0xA9B4FFFE }
//!
//! [[camp]]
//! key = "c-4586"
//! radius = 5.0
//! regen = 192.0
//! prov = { from = "capture", evidence = "EVID-CAMPS", ref = "camp 4586", confidence = "low", sample = { creatures = 2, sessions = 1 }, fields = { regen = { from = "derived", rule = "camp-regen-median" }, radius = { from = "authored" } } }
//! ```
//!
//! | `from` | means | required | optional |
//! |---|---|---|---|
//! | `capture` | measured from recorded sessions | `evidence` (an opaque handle naming the measurement), `ref` (the row inside it, opaque), `confidence` (`high`, `medium`, `low`) | `sample`, the counts behind it by name |
//! | `dat` | the data files' own record | `record` (its id) | |
//! | `derived` | computed by a stated rule | `rule` | `record` (the record it reads) |
//! | `authored` | written by a person | | `by`, `note` |
//!
//! A row that says nothing was written by a person: [`Provenance::AUTHORED`]. Provenance never
//! changes what a row builds; it is carried so that a tool can show where each thing came from
//! and how sure that is.

use std::collections::BTreeMap;
use std::fmt;

use serde::Deserialize;

use crate::error::Loc;

/// What a row came from.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// Measured from recorded sessions.
    Capture,
    /// The data files' own record.
    Dat,
    /// Computed by a stated rule from other rows or the data files.
    Derived,
    /// Written by a person, in a pack.
    #[default]
    Authored,
}

impl Source {
    /// How a source writes it.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Capture => "capture",
            Self::Dat => "dat",
            Self::Derived => "derived",
            Self::Authored => "authored",
        }
    }

    /// One letter for it, as a badge shows it.
    #[must_use]
    pub const fn letter(self) -> char {
        match self {
            Self::Capture => 'C',
            Self::Dat => 'D',
            Self::Derived => 'R',
            Self::Authored => 'A',
        }
    }
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.word())
    }
}

/// How sure a measured row is.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    Low,
    Medium,
    High,
}

impl Confidence {
    /// How a source writes it.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

/// Where a row (or one of its fields) came from, and how sure that is.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Provenance {
    pub source: Source,
    /// A measurement's opaque handle (`evidence`).
    pub evidence: Option<String>,
    /// The row inside the measurement, opaque (`ref`).
    pub reference: Option<String>,
    /// How sure a measurement is.
    pub confidence: Option<Confidence>,
    /// The counts a measurement stands on, by what was counted (`sessions`, `corpses`, ...).
    pub sample: BTreeMap<String, u32>,
    /// The data files' record it is, or that a rule read.
    pub record: Option<u32>,
    /// The rule that computed it.
    pub rule: Option<String>,
    /// Who wrote it, and why.
    pub by: Option<String>,
    pub note: Option<String>,
    /// The row's fields whose source differs from the row's, by name.
    pub fields: BTreeMap<String, Provenance>,
}

impl Provenance {
    /// A row a person wrote: what a row that says nothing is.
    pub const AUTHORED: Self = Self {
        source: Source::Authored,
        evidence: None,
        reference: None,
        confidence: None,
        sample: BTreeMap::new(),
        record: None,
        rule: None,
        by: None,
        note: None,
        fields: BTreeMap::new(),
    };

    /// Whether the row was written by a person and says nothing more.
    #[must_use]
    pub fn is_plain_authored(&self) -> bool {
        *self == Self::AUTHORED
    }

    /// Where the row's field `name` came from: its own, when the row says so, else the row's.
    #[must_use]
    pub fn of_field(&self, name: &str) -> &Self {
        self.fields.get(name).unwrap_or(self)
    }
}

impl fmt::Display for Provenance {
    /// In words, as an editor shows it: `capture (low; creatures 2, sessions 1): camp 4586`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.source)?;
        let counts: Vec<String> = self
            .sample
            .iter()
            .map(|(k, n)| format!("{k} {n}"))
            .collect();
        match (self.confidence, counts.is_empty()) {
            (Some(c), true) => write!(f, " ({})", c.word())?,
            (Some(c), false) => write!(f, " ({}; {})", c.word(), counts.join(", "))?,
            (None, false) => write!(f, " ({})", counts.join(", "))?,
            (None, true) => {}
        }
        let what = self
            .reference
            .clone()
            .or_else(|| self.rule.clone())
            .or_else(|| self.record.map(|r| format!("record {r:#010X}")))
            .or_else(|| self.note.clone());
        if let Some(what) = what {
            write!(f, ": {what}")?;
        }
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Written {
    from: Source,
    evidence: Option<String>,
    #[serde(rename = "ref")]
    reference: Option<String>,
    confidence: Option<Confidence>,
    #[serde(default)]
    sample: BTreeMap<String, u32>,
    record: Option<u32>,
    rule: Option<String>,
    by: Option<String>,
    note: Option<String>,
    #[serde(default)]
    fields: BTreeMap<String, Provenance>,
}

impl<'de> Deserialize<'de> for Provenance {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let w = Written::deserialize(d)?;
        let missing = match w.from {
            Source::Capture if w.evidence.is_none() => Some("evidence"),
            Source::Capture if w.reference.is_none() => Some("ref"),
            Source::Capture if w.confidence.is_none() => Some("confidence"),
            Source::Dat if w.record.is_none() => Some("record"),
            Source::Derived if w.rule.is_none() => Some("rule"),
            _ => None,
        };
        if let Some(field) = missing {
            return Err(serde::de::Error::custom(format!(
                "provenance from {} names its {field}",
                w.from
            )));
        }
        Ok(Self {
            source: w.from,
            evidence: w.evidence,
            reference: w.reference,
            confidence: w.confidence,
            sample: w.sample,
            record: w.record,
            rule: w.rule,
            by: w.by,
            note: w.note,
            fields: w.fields,
        })
    }
}

/// [`Provenance::AUTHORED`], to lend.
static AUTHORED: Provenance = Provenance::AUTHORED;

/// The rows of some files that say where they came from, by where each is written. A row not
/// here was written by a person.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Rows(BTreeMap<Loc, Provenance>);

impl Rows {
    /// The row at `at` says it came from `provenance`, when it says anything.
    pub fn note(&mut self, at: &Loc, provenance: Option<&Provenance>) {
        if let Some(p) = provenance.filter(|p| !p.is_plain_authored()) {
            self.0.insert(at.clone(), p.clone());
        }
    }

    /// Where the row at `at` came from.
    #[must_use]
    pub fn of(&self, at: &Loc) -> &Provenance {
        self.0.get(at).unwrap_or(&AUTHORED)
    }

    /// Every row that says more than that a person wrote it, by where it is written.
    pub fn iter(&self) -> impl Iterator<Item = (&Loc, &Provenance)> + '_ {
        self.0.iter()
    }

    /// Whether no row says more than that a person wrote it.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The rows of `other` too.
    pub fn extend(&mut self, other: Self) {
        self.0.extend(other.0);
    }
}
