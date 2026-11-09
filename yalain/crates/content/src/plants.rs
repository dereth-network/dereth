//! What the world database says of dye plants: each species' plant, seed, pollen and plant item,
//! and the crosses whose pollinated plants give seeds of another species.

use std::collections::BTreeMap;

/// One species of dye plant: the planted plant's template, its seed, its pollen (a species may have
/// none), the plant item a harvest gives, and its pollen's name among the plants' texts (0 for a
/// species with no pollen).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Species {
    pub plant: u32,
    pub seed: u32,
    pub pollen: Option<u32>,
    pub harvest: u32,
    pub name: u32,
}

impl Species {
    #[must_use]
    pub const fn new(plant: u32, seed: u32, pollen: Option<u32>, harvest: u32, name: u32) -> Self {
        Self {
            plant,
            seed,
            pollen,
            harvest,
            name,
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

/// A world's dye plants: its species by their plants' templates, its crosses by the plant and the
/// pollen.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
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
