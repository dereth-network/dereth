//! Not ACE: how far a monster's attack chase stops from its target, by the weapon it holds, as
//! world data (`chase_reach.tsv`, beside this file).
//!
//! The end-of-retail server sent each attack chase a distance to the target drawn afresh between
//! the creature's scale and twice it; a monster holding a weapon drew between `factor` times its
//! scale and twice that. The table holds the factor of every weapon class measured; any other
//! weapon takes [`FALLBACK`].

use std::collections::HashMap;
use std::sync::OnceLock;

use super::table_lines;

/// The DIVERGENCES row.
pub const DIVERGENCE: &str = "V541";

/// Where the evidence is.
pub const EVIDENCE: &str = "retail captures: monster chase distances";

/// The table: comment lines (`#`), a header, then `wcid`, `factor`, `chases` and `name`,
/// tab-separated.
pub const TABLE: &str = include_str!("chase_reach.tsv");

/// The factor of a weapon the table does not hold: the median of the measured weapons'.
pub const FALLBACK: f32 = 1.36;

/// One row of the table.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub weenie_class_id: u32,
    pub factor: f32,
    /// How many chases measured it.
    pub chases: u32,
    pub name: String,
}

/// The rows of `text` in [`TABLE`]'s layout.
///
/// # Errors
/// A missing header, or a line without four fields or with a number that does not read, names the
/// line.
pub fn parse(text: &str) -> Result<Vec<Row>, String> {
    table_lines(text, "wcid\tfactor\tchases\tname")?
        .into_iter()
        .map(|(bad, f)| {
            let [wcid, factor, chases, name] = f[..] else {
                return Err(bad);
            };
            Ok(Row {
                weenie_class_id: wcid.parse().map_err(|_| bad.clone())?,
                factor: factor.parse().map_err(|_| bad.clone())?,
                chases: chases.parse().map_err(|_| bad.clone())?,
                name: name.to_owned(),
            })
        })
        .collect()
}

/// The chase factor of a weapon of class `weenie_class_id`: the table's, else [`FALLBACK`].
///
/// # Panics
/// When the checked-in table does not read (a test reads it).
#[must_use]
pub fn weapon_factor(weenie_class_id: u32) -> f32 {
    static BY_CLASS: OnceLock<HashMap<u32, f32>> = OnceLock::new();
    BY_CLASS
        .get_or_init(|| {
            parse(TABLE)
                .expect("the chase reach table reads")
                .into_iter()
                .map(|r| (r.weenie_class_id, r.factor))
                .collect()
        })
        .get(&weenie_class_id)
        .copied()
        .unwrap_or(FALLBACK)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_chase_reach_table_reads_every_weapon_once() {
        let rows = parse(TABLE).expect("reads");
        assert!(rows.len() > 100, "{}", rows.len());
        let mut seen = std::collections::HashSet::new();
        for r in &rows {
            assert!(
                seen.insert(r.weenie_class_id),
                "{} twice",
                r.weenie_class_id
            );
            assert!((0.9..2.5).contains(&r.factor), "{}: {}", r.name, r.factor);
        }
    }

    #[test]
    fn a_weapon_outside_the_table_takes_the_measured_median() {
        assert!((weapon_factor(u32::MAX) - FALLBACK).abs() < f32::EPSILON);
        let first = parse(TABLE).expect("reads")[0].clone();
        assert!((weapon_factor(first.weenie_class_id) - first.factor).abs() < f32::EPSILON);
    }
}
