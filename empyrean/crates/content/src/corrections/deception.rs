//! Not ACE: the Deception retail gave each creature, as world data (`deception.tsv`, beside this
//! file).
//!
//! Assessing a creature rolls the assessor's skill against the creature's Deception. The
//! end-of-retail server sent back the difficulty it rolled against with the assessing skill after
//! a successful assessment, one value per weenie class in every reading. The table holds the
//! classes whose value differs from the one ACE's world database stores: each row is one entry
//! like the others of the corrections, applied when the weenie is read and only while the weenie
//! still stores the value the row names. A row that adds Deception to a weenie with none adds it
//! untrained, so nothing but the assessment's difficulty changes.

use std::sync::OnceLock;

use empyrean_entity::enums::Skill;

use super::{Value, WeenieCorrection};

/// The DIVERGENCES row.
pub const DIVERGENCE: &str = "V559";

/// Where the evidence is.
pub const EVIDENCE: &str = "retail captures: assessment difficulty";

/// The table: comment lines (`#`), a header, then `wcid`, `stored` (`none`: no row), `retail`,
/// `readings` and `name`, tab-separated.
pub const TABLE: &str = include_str!("deception.tsv");

/// One row of [`TABLE`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub weenie_class_id: u32,
    /// The Deception the database stores (`None`: no row).
    pub stored: Option<u32>,
    /// The Deception retail used.
    pub retail: u32,
    /// How many times retail sent it.
    pub readings: u32,
    pub name: String,
}

/// The rows of `text` in a [`TABLE`]'s layout.
///
/// # Errors
/// A line without five fields, or with a number that does not read, names the line.
pub fn parse(text: &str) -> Result<Vec<Row>, String> {
    let mut rows = Vec::new();
    let mut header = false;
    for (n, line) in text.lines().enumerate() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if !header {
            if line != "wcid\tstored\tretail\treadings\tname" {
                return Err(format!("line {}: not the table's header", n + 1));
            }
            header = true;
            continue;
        }
        let bad = || format!("line {}: {line:?}", n + 1);
        let f: Vec<&str> = line.split('\t').collect();
        let [wcid, stored, retail, readings, name] = f[..] else {
            return Err(bad());
        };
        let number = |s: &str| s.parse::<u32>().map_err(|_| bad());
        rows.push(Row {
            weenie_class_id: number(wcid)?,
            stored: if stored == "none" {
                None
            } else {
                Some(number(stored)?)
            },
            retail: number(retail)?,
            readings: number(readings)?,
            name: name.to_owned(),
        });
    }
    Ok(rows)
}

/// The correction a row makes.
#[must_use]
pub fn correction(row: &Row) -> WeenieCorrection {
    WeenieCorrection {
        weenie_class_id: row.weenie_class_id,
        stored: Value::Skill(Skill::Deception, row.stored),
        corrected: Value::Skill(Skill::Deception, Some(row.retail)),
        divergence: DIVERGENCE,
        evidence: EVIDENCE,
    }
}

/// Every row of [`TABLE`] as a correction, in class id order.
///
/// # Panics
/// When the table does not read (a test reads it).
#[must_use]
pub fn corrections() -> &'static [WeenieCorrection] {
    static ALL: OnceLock<Vec<WeenieCorrection>> = OnceLock::new();
    ALL.get_or_init(|| {
        let mut all: Vec<WeenieCorrection> = parse(TABLE)
            .unwrap_or_else(|e| panic!("deception.tsv: {e}"))
            .iter()
            .map(correction)
            .collect();
        all.sort_by_key(|c| c.weenie_class_id);
        all
    })
}
