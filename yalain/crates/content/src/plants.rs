//! What the world database says of plants a seed plants: each species' plant, seed, pollen and plant
//! item, how many one seed plants, when they are ripe, whether they need care and where their lines
//! are; and the crosses whose pollinated plants give seeds of another species.

use std::collections::BTreeMap;

/// How long a plant takes to ripen when its row says nothing, in seconds since it was planted (the
/// dye plants').
pub const RIPE_SECONDS: f64 = 180.0;

/// One species of plant: the planted plant's template, its seed, its pollen (a species may have
/// none), the plant item a harvest gives, and its pollen's name among the plants' texts (0 for a
/// species with no pollen); how many plants one seed plants, how long after planting each is ripe,
/// whether it needs water and fertilizer, and its own lines (none: the dye plants').
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Species {
    pub plant: u32,
    pub seed: u32,
    pub pollen: Option<u32>,
    pub harvest: u32,
    pub name: u32,
    pub count: u32,
    pub ripe: f64,
    pub tended: bool,
    pub texts: Option<Texts>,
}

/// Where a species' lines are: their string table, which holds the dye plants' lines for a plant not
/// yet ripe and one its user did not plant under the same ids, and the ids of its examination's
/// countdown label, of its examination's line once ripe, and of its harvest's line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Texts {
    pub table: u32,
    pub maturity_in: u32,
    pub mature: u32,
    pub harvest: u32,
}

impl Species {
    /// A dye plant: one a seed, ripe in [`RIPE_SECONDS`], needing care, with the dye plants' lines.
    #[must_use]
    pub const fn new(plant: u32, seed: u32, pollen: Option<u32>, harvest: u32, name: u32) -> Self {
        Self {
            plant,
            seed,
            pollen,
            harvest,
            name,
            count: 1,
            ripe: RIPE_SECONDS,
            tended: true,
            texts: None,
        }
    }
}

/// A cross: a plant of one species pollinated with another's pollen gives so many seeds of a
/// third.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cross {
    /// The plant's template, and the pollen's.
    pub plant: u32,
    pub pollen: u32,
    /// The seeds' template, and how many.
    pub seeds: u32,
    pub count: u32,
}

/// A world's plants: its species by their plants' templates, its crosses by the plant and the
/// pollen.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DyePlants {
    pub species: BTreeMap<u32, Species>,
    pub crosses: BTreeMap<(u32, u32), Cross>,
}

impl DyePlants {
    /// The species a seed plants.
    #[must_use]
    pub fn sown_by(&self, seed: u32) -> Option<Species> {
        self.species.values().copied().find(|s| s.seed == seed)
    }

    /// The species whose pollen this is.
    #[must_use]
    pub fn pollen_of(&self, pollen: u32) -> Option<Species> {
        self.species
            .values()
            .copied()
            .find(|s| s.pollen == Some(pollen))
    }

    /// The cross of a plant and a pollen, when known.
    #[must_use]
    pub fn cross(&self, plant: u32, pollen: u32) -> Option<Cross> {
        self.crosses.get(&(plant, pollen)).copied()
    }

    /// Every template the dye plants make: each species' plant, seed, plant item and pollen, and
    /// each cross's seeds.
    pub fn templates(&self) -> impl Iterator<Item = u32> + '_ {
        self.species
            .values()
            .flat_map(|s| [s.plant, s.seed, s.harvest].into_iter().chain(s.pollen))
            .chain(self.crosses.values().map(|c| c.seeds))
    }
}
