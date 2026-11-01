//! The periodic save: every character in the world is kept again once its last save is
//! [`ServerConfig::save_seconds`](crate::ServerConfig) old (five minutes unless the operator gives
//! another), as Empyrean saves each player on its heartbeat once its last save is that old.
//!
//! Any save of a character (its leaving, a play event's) counts: the next periodic save comes that
//! long after it. A character's first is that long after it entered the world.
//!
//! Each save is one character's own transaction, and a tick of the world keeps at most
//! [`PER_TICK`] characters, the longest unsaved first: when many come due together (a crowd that
//! entered at once), the rest wait for the next ticks, so no tick of the loop stalls on the store.

use std::collections::HashMap;

use yalain_net::{ConnId, LocalTime};
use yalain_store::Store;

use crate::{Clock, Server};

/// Seconds between a character's saves (Empyrean's `player_save_interval`).
pub const SAVE_SECONDS: u32 = 300;

/// The most characters a tick keeps.
pub const PER_TICK: usize = 1;

/// When each character in the world was last kept.
#[derive(Debug, Default)]
pub struct Saves {
    /// The time as the world last read it.
    pub(crate) now: LocalTime,
    /// By character id: when it was last kept (or entered the world).
    last: HashMap<u64, LocalTime>,
}

impl Saves {
    /// A character was kept just now.
    pub(crate) fn kept(&mut self, character: u64) {
        self.last.insert(character, self.now);
    }

    /// When a character in the world was last kept (or entered), if the world has seen it.
    #[must_use]
    pub fn last(&self, character: u64) -> Option<LocalTime> {
        self.last.get(&character).copied()
    }
}

/// Keep the characters whose last save is old enough, at most [`PER_TICK`] of them, the longest
/// unsaved first. A character the world has not seen before starts its wait now.
pub(crate) fn tick<S: Store>(s: &mut Server<S>, clock: Clock) {
    s.saves.now = clock.local;
    let interval = f64::from(s.cfg.save_seconds);
    let mut due: Vec<(f64, u64, ConnId)> = Vec::new();
    for (world, p) in &s.players {
        let last = *s.saves.last.entry(p.character).or_insert(clock.local);
        if clock.local.0 - last.0 >= interval {
            due.push((last.0, p.character, *world));
        }
    }
    // Characters no longer in the world are forgotten.
    if s.saves.last.len() > s.players.len() {
        let here: std::collections::HashSet<u64> =
            s.players.values().map(|p| p.character).collect();
        s.saves.last.retain(|id, _| here.contains(id));
    }
    due.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    for (_, _, world) in due.into_iter().take(PER_TICK) {
        crate::world::keep_player(s, world);
    }
}

/// Keep every character in the world now (the console's `save-world`); the world goes on.
/// Returns how many were kept.
pub fn save_world<S: Store>(s: &mut Server<S>, clock: Clock) -> usize {
    s.saves.now = clock.local;
    let mut worlds: Vec<_> = s.players.keys().copied().collect();
    worlds.sort();
    for world in &worlds {
        crate::world::keep_player(s, *world);
    }
    worlds.len()
}
