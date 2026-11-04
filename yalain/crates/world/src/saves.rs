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

/// The seconds a player in the world has played: those it had kept and those of this stay.
#[must_use]
pub fn played_seconds(p: &crate::world::Player, now: f64) -> u64 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let stay = (now - p.entered_at).max(0.0) as u64;
    p.play.played_seconds.saturating_add(stay)
}

/// Is a character younger than the operator asks of one sending to the Dereth Network
/// ([`ServerConfig::min_character_age_seconds`](crate::ServerConfig)), in the world or not? A
/// character with no kept play has played nothing.
#[must_use]
pub fn too_new<S: Store>(s: &mut Server<S>, character: u64) -> bool {
    let least = u64::from(s.cfg.min_character_age_seconds);
    if least == 0 {
        return false;
    }
    let now = s.saves.now.0;
    let played = match s.players.values().find(|p| p.character == character) {
        Some(p) => played_seconds(p, now),
        None => s
            .store
            .world_state(character)
            .ok()
            .flatten()
            .and_then(|w| crate::state::PlayState::from_bytes(&w.play).ok())
            .map_or(0, |play| play.played_seconds),
    };
    played < least
}

/// The names, in lower case, of the characters in the world too new to send to the network.
#[must_use]
pub fn too_new_in_world<S: Store>(s: &Server<S>) -> std::collections::HashSet<String> {
    let least = u64::from(s.cfg.min_character_age_seconds);
    if least == 0 {
        return std::collections::HashSet::new();
    }
    let now = s.saves.now.0;
    s.players
        .values()
        .filter(|p| played_seconds(p, now) < least)
        .map(|p| p.name.to_lowercase())
        .collect()
}

/// A record of the world's, read for its first use: `None` while the store cannot answer, for the
/// caller to try again and write nothing meanwhile; else what it holds, nothing when it has no such
/// record. A record kept that does not read (damaged, or a newer build's) is copied aside first, as
/// `<name>.unreadable`, `.unreadable.2` ..., since the world will write over it.
pub(crate) fn read_record<S: Store, T>(
    s: &mut Server<S>,
    name: &str,
    parse: fn(&[u8]) -> Option<T>,
) -> Option<Option<T>> {
    let bytes = match s.store.world_record(name) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => return Some(None),
        Err(e) => {
            tracing::error!("the store could not read the record {name}: {e}");
            return None;
        }
    };
    if let Some(read) = parse(&bytes) {
        return Some(Some(read));
    }
    let mut aside = format!("{name}.unreadable");
    let mut n = 2;
    loop {
        match s.store.world_record(&aside) {
            Ok(None) => break,
            Ok(Some(_)) => {
                aside = format!("{name}.unreadable.{n}");
                n += 1;
            }
            Err(e) => {
                tracing::error!("the store could not read the record {aside}: {e}");
                return None;
            }
        }
    }
    if let Err(e) = s.store.set_world_record(&aside, &bytes) {
        tracing::error!("the record {name} does not read, and could not be copied aside: {e}");
        return None;
    }
    tracing::error!(
        "the record {name} does not read, so the world starts it empty; its bytes are kept in the \
         database's world_record table as {aside}"
    );
    Some(None)
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
