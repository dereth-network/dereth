//! What the 1999 data files force on ac1999's world, and nothing else: what ac1999 chose to have is
//! its author's to choose, so this drops only what the 1999 `portal.dat` cannot draw and what the 1999
//! `cell.dat` has no room for. Content whose art is absent keeps its definition but loses whatever
//! places or creates it; placements in rooms the 1999 cells lack are dropped.

use std::collections::{BTreeMap, BTreeSet};

use empyrean_content::error::ImportError;

use super::cells::Target;
use super::convert::fail;

pub const POLICY: &str = "launch-world-v2";

/// Content whose definitions stay but which nothing in the world may place or create: the 1999
/// `portal.dat` has no art for it.
pub const CUT: &[(i128, &str)] = &[
    (5765, NO_ART),
    (8232, NO_ART),
    (8271, NO_ART),
    (8272, NO_ART),
    (8273, NO_ART),
    (8274, NO_ART),
    (8275, NO_ART),
    (8395, NO_ART),
];
const NO_ART: &str = "its assets are absent from the 1999 portal.dat";

/// Placement roots in interior cells the retail CD's `cell.dat` lacks: guid, class and cell. No
/// 1999 room of their landblocks matches the room they name, so they and their linked children go.
pub const MISSING_CELLS: &[(i128, i128, i128)] = &[
    (0x701D_1088, 2179, 0x01D1_0359),
    (0x72D5_A006, 3955, 0x2D5A_013F),
    (0x764D_500E, 4219, 0x64D5_012C),
    (0x7808_8001, 3955, 0x8088_0100),
    (0x7E53_D007, 4219, 0xE53D_0113),
];
const MISSING_CELL: &str = "placed in an interior cell the 1999 cell.dat lacks";

/// Stored destinations in interior cells the 1999 `cell.dat` lacks: object, position type, cell.
pub const MISSING_DESTINATIONS: &[(i128, i128, i128)] = &[(3657, 2, 0x018A_017F)];

/// What the target removes from the world, settled against the source before any row is written.
#[derive(Debug, Default)]
pub struct Policy {
    active: bool,
    cut: BTreeMap<i128, &'static str>,
    /// Placed instances the world drops, with the reason.
    pub guids: BTreeMap<i128, String>,
    destinations: BTreeMap<(i128, i128, i128), bool>,
    /// What the policy reviewed that the source no longer has, in words: the source removed it
    /// since, which can only leave less to cut. The manifest lists them for the next review.
    pub gone: BTreeSet<String>,
}

/// The rows of the source the policy is settled against.
#[derive(Debug, Default)]
pub struct Placements {
    /// guid -> (class, cell).
    pub instances: BTreeMap<i128, (i128, i128)>,
    /// (parent, child) links.
    pub links: Vec<(i128, i128)>,
    pub weenies: BTreeSet<i128>,
}

impl Policy {
    pub fn new(target: Target) -> Self {
        if !target.is_1999() {
            return Self::default();
        }
        Self {
            active: true,
            cut: CUT.iter().copied().collect(),
            guids: BTreeMap::new(),
            destinations: MISSING_DESTINATIONS.iter().map(|d| (*d, false)).collect(),
            gone: BTreeSet::new(),
        }
    }

    pub fn active(&self) -> bool {
        self.active
    }

    /// Work out every placement the world drops: the roots in missing cells and placements of
    /// cut classes, the children linked to either, and a generator whose every child is dropped.
    /// A reviewed row the source no longer has is noted in [`Policy::gone`]; one it has changed
    /// refuses the conversion.
    pub fn settle(&mut self, p: &Placements) -> Result<(), ImportError> {
        if !self.active {
            return Ok(());
        }
        for class in self.cut.keys() {
            if !p.weenies.contains(class) {
                self.gone.insert(format!("cut class {class}"));
            }
        }
        let mut children: BTreeMap<i128, Vec<i128>> = BTreeMap::new();
        for &(parent, child) in &p.links {
            children.entry(parent).or_default().push(child);
        }
        let mut roots = Vec::new();
        for &(guid, class, cell) in MISSING_CELLS {
            match p.instances.get(&guid) {
                None => {
                    self.gone.insert(format!("placement {guid:#X}"));
                    continue;
                }
                Some(&row) if row != (class, cell) => {
                    return Err(fail(format!(
                        "{POLICY}: placement {guid:#X} does not match the reviewed row; review the source"
                    )));
                }
                Some(_) => {}
            }
            self.guids.insert(guid, MISSING_CELL.into());
            roots.push(guid);
        }
        for (&guid, &(class, _)) in &p.instances {
            if let Some(reason) = self.cut.get(&class) {
                self.guids.insert(guid, (*reason).into());
                roots.push(guid);
            }
        }
        for &(object, _, _) in MISSING_DESTINATIONS {
            if !p.weenies.contains(&object) {
                self.gone.insert(format!("destination of {object}"));
                self.destinations.retain(|d, _| d.0 != object);
            }
        }
        for root in roots {
            for child in children.get(&root).into_iter().flatten() {
                self.guids
                    .entry(*child)
                    .or_insert_with(|| format!("linked child of a dropped placement {root:#X}"));
            }
        }
        for (parent, kids) in &children {
            if !self.guids.contains_key(parent) && kids.iter().all(|k| self.guids.contains_key(k)) {
                self.guids.insert(
                    *parent,
                    "generator whose every linked child is dropped".into(),
                );
            }
        }
        Ok(())
    }

    /// The reason a row naming `class` from `owner` is cut, unless the owner is itself cut (its
    /// own definition is kept whole).
    pub fn cut_reference(&self, owner: Option<i128>, class: i128) -> Option<&'static str> {
        if owner.is_some_and(|o| self.cut.contains_key(&o)) {
            return None;
        }
        self.cut.get(&class).copied()
    }

    pub fn destination(&mut self, object: i128, kind: i128, cell: i128) -> bool {
        match self.destinations.get_mut(&(object, kind, cell)) {
            Some(seen) => {
                *seen = true;
                true
            }
            None => false,
        }
    }

    pub fn complete(&self) -> Result<(), ImportError> {
        if let Some((d, _)) = self.destinations.iter().find(|(_, seen)| !**seen) {
            return Err(fail(format!(
                "{POLICY}: missing reviewed destination of {}; review the source",
                d.0
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
