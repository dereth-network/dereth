//! Checks of treasure pools and the `[[loot]]` draws that name them: every pool named is a pool of
//! the database, no pool leads back to itself, weights and shares are in range, and windows run
//! upward.

use std::collections::{BTreeMap, BTreeSet};

use crate::rows::{Database, PoolEntry, Row};
use crate::Problem;

/// The problems of the database's pools and draws.
#[must_use]
pub fn check(db: &Database) -> Vec<Problem> {
    let mut problems = Vec::new();
    let mut children: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for l in db.rows() {
        if let Row::TreasurePool(p) = &l.row {
            children.insert(
                &p.name,
                p.pools.iter().filter_map(|e| e.pool.as_deref()).collect(),
            );
        }
    }
    for l in db.rows() {
        let mut at = |m: String| problems.push(Problem::new(&l.file, l.line, m));
        let named = |field: &str, pool: &str, at: &mut dyn FnMut(String)| {
            if !children.contains_key(pool) {
                at(format!("{field}: no treasure pool {pool}"));
            }
        };
        match &l.row {
            Row::TreasurePool(p) => {
                for (field, list) in [("pools", &p.pools), ("items", &p.items)] {
                    entries(field, list, &mut at);
                }
                for e in &p.pools {
                    if let Some(n) = &e.pool {
                        named("pools", n, &mut at);
                    }
                }
                if let Some(own) = p.own {
                    if !(0.0..=1.0).contains(&own) {
                        at("own: a share between 0 and 1".into());
                    }
                    if p.race.is_none() {
                        at(
                            "own: a pool takes a share of its race's draws, and names no race"
                                .into(),
                        );
                    }
                }
                if leads_back(&p.name, &children) {
                    at(format!("pools: {} leads back to itself", p.name));
                }
            }
            Row::Loot(r) => {
                entries("draws", &r.draws, &mut at);
                for e in &r.draws {
                    if let Some(n) = &e.pool {
                        named("draws", n, &mut at);
                    }
                }
                if !r.draws.is_empty() && !r.items.is_empty() {
                    at("draws: a row draws its items from its draws or its items, not both".into());
                }
                if let Some((pool, chance)) = &r.rare {
                    named("rare", pool, &mut at);
                    if !(*chance > 0.0 && *chance <= 1.0) {
                        at("rare: a chance above 0 and at most 1".into());
                    }
                }
            }
            _ => {}
        }
    }
    problems
}

/// Weights from 0 and windows that run upward from level 0.
fn entries(field: &str, list: &[PoolEntry], at: &mut dyn FnMut(String)) {
    for e in list {
        if !(e.weight.is_finite() && e.weight >= 0.0) {
            at(format!("{field}: a weight of 0 or more"));
        }
        if e.least.is_some_and(|l| l < 0) {
            at(format!("{field}: a least level of 0 or more"));
        }
        if let (Some(l), Some(m)) = (e.least, e.most) {
            if l > m {
                at(format!("{field}: a least level above its most, {l} > {m}"));
            }
        }
    }
}

/// Whether a pool's entries lead back to it.
fn leads_back(start: &str, children: &BTreeMap<&str, Vec<&str>>) -> bool {
    let mut seen = BTreeSet::new();
    let mut stack: Vec<&str> = children.get(start).cloned().unwrap_or_default();
    while let Some(n) = stack.pop() {
        if n == start {
            return true;
        }
        if seen.insert(n) {
            stack.extend(children.get(n).into_iter().flatten().copied());
        }
    }
    false
}
