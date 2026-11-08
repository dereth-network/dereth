//! A creature's treasure drawn from pools, where its loot profile draws from them: how many items
//! its counts give (the template's bounds hold), each drawn from the profile's draws by weight and
//! down through the pools to a template by the item level (the creature's loot base), the race's
//! own pools taking their share for a killer of their race; and once a corpse its rare, by its
//! chance. A creature's own trophies are drawn beside, as ever, and are not counted.
//! (2017: 1,672 of 1,702 corpses whose templates set bounds held their treasure and their profiles'
//! other items within them, against 1,606 counting the trophies too; item kinds climb by the
//! dropper's level, 4.3% of 5,443 items outside the windows fit to them; the killer's race takes its
//! own race's equipment, the killer's class no style of weapon: 0 of 3 races' tables beyond chance.)

use yalain_content::pools::Dice;

use crate::rng::Rng;

impl Dice for Rng {
    fn pick(&mut self, weights: &[f32]) -> Option<usize> {
        self.weighted(weights.iter().copied())
    }

    fn chance(&mut self, p: f32) -> bool {
        Rng::chance(self, p)
    }
}

/// The race of a killer who is a player in the world (its bit), 0 for any other.
#[must_use]
pub fn race_of<S: crate::Store>(s: &crate::Server<S>, killer: u64) -> u32 {
    s.players
        .values()
        .find(|p| p.character == killer)
        .map_or(0, |p| p.race)
}

/// Whether a profile draws its items from pools.
#[must_use]
pub fn draws_from_pools<S: crate::Store>(s: &crate::Server<S>, profile: &str) -> bool {
    s.loot
        .pools
        .profiles
        .get(profile)
        .is_some_and(|d| !d.draws.is_empty())
}

/// The templates a profile's pools give for `count` items at an item level, for a killer of a race.
pub fn drawn<S: crate::Store>(
    s: &mut crate::Server<S>,
    profile: &str,
    count: usize,
    level: i32,
    race: u32,
) -> Vec<u32> {
    let crate::Server { loot, rng, .. } = &mut *s;
    loot.pools.items(profile, count, level, race, rng)
}

/// A profile's rare, rolled once for a corpse: its template, when it comes up.
pub fn rare<S: crate::Store>(
    s: &mut crate::Server<S>,
    profile: &str,
    level: i32,
    race: u32,
) -> Option<u32> {
    let crate::Server { loot, rng, .. } = &mut *s;
    loot.pools.rare(profile, level, race, rng)
}
