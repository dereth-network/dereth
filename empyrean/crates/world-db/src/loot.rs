//! What a world's loot is at a layer, and the swap from the source's loot to ACE's.
//!
//! A world drops loot by one of two algorithms. Under the source's, its `loot-policy` is the
//! source's and a creature's death treasure type is a `grouped-treasure`; under ACE's, it has no
//! `loot-policy` and the type is a `death-treasure`. A world may hold both tables while no type is
//! in both.
//!
//! **The swap** is the layer where the world's loot becomes ACE's. It does three things at once:
//!
//! - it takes the source's `loot-policy` away (`[[remove]] items = ["loot-policy"]`), leaving ACE's;
//! - it removes every `grouped-treasure <type>`;
//! - it adds `[[treasure_death]]` rows, one for each type a creature's `DeathTreasureType` names
//!   (that a creature names a type the world has is a reference [`crate::refs`] checks).
//!
//! What the check holds every layer to:
//!
//! | What | Severity |
//! |---|---|
//! | a treasure type both a `death-treasure` and a `grouped-treasure` | error |
//! | `grouped-treasure` with no source `loot-policy` (the policy taken away alone) | error |
//! | a swap that adds no `death-treasure` | error; a snapshot's own, a warning |
//! | a `grouped-treasure` removed while the loot stays the source's (the removals alone, or with the new rows) | error; a snapshot's own, a warning |
//! | the swap itself | reported |
//!
//! A snapshot (the base or a checkpoint) is a whole world from its source: where its source's
//! loot is ACE's and the world below it was the source's, the checkpoint is the swap, its three
//! parts the difference of the two snapshots.

use std::collections::BTreeSet;

use empyrean_content::WorldContent;

/// A world's loot: the source's policy, when it has one (its algorithm), and the treasure types
/// of each table.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Loot {
    pub policy: Option<String>,
    pub grouped: BTreeSet<u32>,
    pub death: BTreeSet<u32>,
}

impl Loot {
    /// The loot of `c`.
    #[must_use]
    pub fn of(c: &WorldContent) -> Self {
        Self {
            policy: c.loot_policies.first().map(|p| p.algorithm.clone()),
            grouped: c.grouped_treasure.iter().map(|g| g.treasure_type).collect(),
            death: c.treasure_death.iter().map(|d| d.treasure_type).collect(),
        }
    }
}

/// What the loot check says of one layer: its errors, what is its source's own when the layer is
/// a snapshot, and the swap when the layer is it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LootCheck {
    pub errors: Vec<String>,
    pub inherited: Vec<String>,
    pub reports: Vec<String>,
}

/// The loot of a layer, `now`, checked on its own and against the world below it, `below` (none
/// for the base); `snapshot` when the layer is the base or a checkpoint.
#[must_use]
pub fn check(below: Option<&Loot>, now: &Loot, snapshot: bool) -> LootCheck {
    let mut out = LootCheck::default();
    for t in now.grouped.intersection(&now.death) {
        out.errors.push(format!(
            "death-treasure {t} and grouped-treasure {t}: a treasure type is ACE's death treasure or the source's grouped treasure, not both"
        ));
    }
    if now.policy.is_none() && !now.grouped.is_empty() {
        out.errors.push(format!(
            "loot-policy: the world has none, so its loot is ACE's, and still has {} grouped-treasure (first {}): the swap to ACE's removes every one",
            now.grouped.len(),
            first(&now.grouped)
        ));
    }
    let Some(below) = below else {
        return out;
    };
    let mut part = Vec::new();
    let removed: BTreeSet<u32> = below.grouped.difference(&now.grouped).copied().collect();
    if below.policy.is_some() && now.policy.is_none() {
        let added = now.death.difference(&below.death).count();
        if added == 0 {
            part.push(
                "loot-policy: the swap to ACE's loot adds no death-treasure: it removes the source's loot policy, every grouped-treasure, and adds [[treasure_death]] rows together"
                    .to_owned(),
            );
        }
        out.reports.push(format!(
            "the loot becomes ACE's: the source's loot-policy goes, {} grouped-treasure go and {added} death-treasure come",
            removed.len()
        ));
    } else if now.policy.is_some() {
        for t in &removed {
            part.push(format!(
                "grouped-treasure {t}: removed while the loot is the source's; the swap to ACE's removes the source's loot-policy and every grouped-treasure, and adds [[treasure_death]] rows, together"
            ));
        }
    }
    if snapshot {
        out.inherited.extend(part);
    } else {
        out.errors.extend(part);
    }
    out
}

/// The first of `types`, for a line that counts them.
fn first(types: &BTreeSet<u32>) -> String {
    types
        .first()
        .map_or_else(String::new, |t| format!("grouped-treasure {t}"))
}
