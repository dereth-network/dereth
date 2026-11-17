//! Not ACE: the Deception retail gave each creature and summoned pet, as world data
//! (`deception.tsv` and `pet_deception.tsv`, beside this file).
//!
//! Assessing a creature rolls the assessor's skill against the creature's Deception. The
//! end-of-retail server sent back the difficulty it rolled against with the assessing skill after
//! a successful assessment, one value per weenie class in every reading. The creature table holds
//! the classes whose value differs from the one ACE's world database stores. ACE's world database
//! gives every skill of a summoned pet one number per tier (its essence's Summoning requirement);
//! the measured pets of a tier all had one Deception, and the pet table gives it to every pet of
//! that tier. Each row is one entry like the others of the corrections, applied when the weenie is
//! read and only while the weenie still stores the value the row names. A row that adds Deception
//! to a weenie with none adds it untrained, so nothing but the assessment's difficulty changes.

use std::sync::OnceLock;

use empyrean_entity::enums::Skill;

use super::{table_lines, Value, WeenieCorrection};

/// The DIVERGENCES row.
pub const DIVERGENCE: &str = "V559";

/// Where the evidence is.
pub const EVIDENCE: &str = "retail captures: assessment difficulty";

/// The creature table: comment lines (`#`), a header, then `wcid`, `stored` (`none`: no row),
/// `retail`, `readings` and `name`, tab-separated.
pub const TABLE: &str = include_str!("deception.tsv");

/// The summoned pet table: comment lines (`#`), a header, then `wcid`, `stored`, `retail`, `level`,
/// `readings` (0: the class was not measured, others of its tier were) and `name`, tab-separated.
pub const PET_TABLE: &str = include_str!("pet_deception.tsv");

/// One row of a table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub weenie_class_id: u32,
    /// The Deception the database stores (`None`: no row).
    pub stored: Option<u32>,
    /// The Deception retail used.
    pub retail: u32,
    /// A pet's level (the creature table has none).
    pub level: Option<u32>,
    /// How many times retail sent it.
    pub readings: u32,
    pub name: String,
}

fn number(s: &str) -> Option<u32> {
    s.parse().ok()
}

/// The rows of `text` in [`TABLE`]'s layout.
///
/// # Errors
/// A missing header, or a line without five fields or with a number that does not read, names the
/// line.
pub fn parse(text: &str) -> Result<Vec<Row>, String> {
    table_lines(text, "wcid\tstored\tretail\treadings\tname")?
        .into_iter()
        .map(|(bad, f)| {
            let [wcid, stored, retail, readings, name] = f[..] else {
                return Err(bad);
            };
            Ok(Row {
                weenie_class_id: number(wcid).ok_or_else(|| bad.clone())?,
                stored: if stored == "none" {
                    None
                } else {
                    Some(number(stored).ok_or_else(|| bad.clone())?)
                },
                retail: number(retail).ok_or_else(|| bad.clone())?,
                level: None,
                readings: number(readings).ok_or_else(|| bad.clone())?,
                name: name.to_owned(),
            })
        })
        .collect()
}

/// The rows of `text` in [`PET_TABLE`]'s layout.
///
/// # Errors
/// As [`parse`], with six fields.
pub fn parse_pets(text: &str) -> Result<Vec<Row>, String> {
    table_lines(text, "wcid\tstored\tretail\tlevel\treadings\tname")?
        .into_iter()
        .map(|(bad, f)| {
            let [wcid, stored, retail, level, readings, name] = f[..] else {
                return Err(bad);
            };
            Ok(Row {
                weenie_class_id: number(wcid).ok_or_else(|| bad.clone())?,
                stored: Some(number(stored).ok_or_else(|| bad.clone())?),
                retail: number(retail).ok_or_else(|| bad.clone())?,
                level: Some(number(level).ok_or_else(|| bad.clone())?),
                readings: number(readings).ok_or_else(|| bad.clone())?,
                name: name.to_owned(),
            })
        })
        .collect()
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

/// Every row of both tables as a correction, in class id order.
///
/// # Panics
/// When a table does not read (a test reads them).
#[must_use]
pub fn corrections() -> &'static [WeenieCorrection] {
    static ALL: OnceLock<Vec<WeenieCorrection>> = OnceLock::new();
    ALL.get_or_init(|| {
        let creatures = parse(TABLE).unwrap_or_else(|e| panic!("deception.tsv: {e}"));
        let pets = parse_pets(PET_TABLE).unwrap_or_else(|e| panic!("pet_deception.tsv: {e}"));
        let mut all: Vec<WeenieCorrection> =
            creatures.iter().chain(&pets).map(correction).collect();
        all.sort_by_key(|c| c.weenie_class_id);
        all
    })
}
