//! Not ACE: the body armor retail gave each creature, as world data (`body_armor.tsv`, beside
//! this file).
//!
//! A successful assessment of a creature shows its armor at nine places (head, chest, abdomen,
//! upper arm, lower arm, hand, upper leg, lower leg, foot): its body's base armor there plus the
//! armor it wears. At the end of retail that readout was the same for every creature of a class
//! and moved only with what it wore. The table holds the classes whose body, the readout less what
//! the class always wears, differs from the one ACE's world database stores, at the body parts
//! keyed by those places (0 to 8). Each row is one entry like the others of the corrections,
//! applied when the weenie is read and only while the weenie still stores the nine values the row
//! names. A part's other values, its armor against each damage type among them, are left alone,
//! as are the parts of a body that are none of the nine places.

use std::sync::OnceLock;

use super::{table_lines, Value, WeenieCorrection, PLACES};

/// The DIVERGENCES row.
pub const DIVERGENCE: &str = "V559";

/// Where the evidence is.
pub const EVIDENCE: &str = "retail captures: assessment armor levels";

/// The table: comment lines (`#`), a header, then `wcid`, `stored`, `corrected`, `readout`,
/// `worn` (nine comma-separated values each; `-` for a place the body has no part at), `readings`
/// and `name`, tab-separated.
pub const TABLE: &str = include_str!("body_armor.tsv");

/// One row of [`TABLE`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub weenie_class_id: u32,
    /// The base armor the database stores at each place (`None`: no part there).
    pub stored: [Option<i32>; PLACES],
    /// The base armor retail showed: the readout less what is worn.
    pub corrected: [Option<i32>; PLACES],
    /// Retail's armor levels.
    pub readout: [i32; PLACES],
    /// The armor of what the class always wears, at each place.
    pub worn: [i32; PLACES],
    /// How many replies showed the readout.
    pub readings: u32,
    pub name: String,
}

fn places(s: &str) -> Option<[Option<i32>; PLACES]> {
    let values: Vec<Option<i32>> = s
        .split(',')
        .map(|v| {
            if v == "-" {
                Some(None)
            } else {
                v.parse().ok().map(Some)
            }
        })
        .collect::<Option<_>>()?;
    values.try_into().ok()
}

fn numbers(s: &str) -> Option<[i32; PLACES]> {
    let values: Vec<i32> = s
        .split(',')
        .map(|v| v.parse().ok())
        .collect::<Option<_>>()?;
    values.try_into().ok()
}

/// The rows of `text` in [`TABLE`]'s layout.
///
/// # Errors
/// A missing header, or a line without seven fields or with a value that does not read, names the
/// line.
pub fn parse(text: &str) -> Result<Vec<Row>, String> {
    table_lines(
        text,
        "wcid\tstored\tcorrected\treadout\tworn\treadings\tname",
    )?
    .into_iter()
    .map(|(bad, f)| {
        let [wcid, stored, corrected, readout, worn, readings, name] = f[..] else {
            return Err(bad);
        };
        Ok(Row {
            weenie_class_id: wcid.parse().map_err(|_| bad.clone())?,
            stored: places(stored).ok_or_else(|| bad.clone())?,
            corrected: places(corrected).ok_or_else(|| bad.clone())?,
            readout: numbers(readout).ok_or_else(|| bad.clone())?,
            worn: numbers(worn).ok_or_else(|| bad.clone())?,
            readings: readings.parse().map_err(|_| bad.clone())?,
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
        stored: Value::BodyArmor(row.stored),
        corrected: Value::BodyArmor(row.corrected),
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
            .unwrap_or_else(|e| panic!("body_armor.tsv: {e}"))
            .iter()
            .map(correction)
            .collect();
        all.sort_by_key(|c| c.weenie_class_id);
        all
    })
}
