//! The periodic save: every character in the world is kept again once its last save is
//! [`ServerConfig::save_seconds`](crate::ServerConfig) old (five minutes unless the operator gives
//! another), as Empyrean saves each player on its heartbeat once its last save is that old.
//!
//! Any save of a character (its leaving, a play event's) counts: the next periodic save comes that
//! long after it. A character's first is that long after it entered the world.
//!
//! A tick of the world makes at most [`PER_TICK`] periodic saves, the longest unsaved first: when
//! many come due together (a crowd that entered at once), the rest wait for the next ticks, so no
//! tick of the loop stalls on the store.
//!
//! **An option a client sends** (its window places, its chat's colors, an alias) is kept with the
//! next save of the character, at most [`OPTIONS_SECONDS`] on: clients send them often. Leaving and
//! a stop keep it as they keep all else, and a crash loses that long of them at most.
//!
//! **What a save is.** Keeping a character takes its state as it stands now and holds it here; what
//! is held goes to the store in one transaction at the end of the world's pass ([`Server::pump`]),
//! before anything that pass said is sent. So all a pass did is kept whole: a trade's two sides, a
//! craft's product and its ingredients, a sale and its record. A crash loses the passes since, never
//! one side of one.
//!
//! **A save that fails.** What it held stays held, and is tried again every [`RETRY_SECONDS`]; a
//! character that leaves meanwhile, or comes back, is held too (its entry reads what is held before
//! the store). Nothing is lost unless the server itself dies before saves work again. So everyone
//! says so: the operator reads an error line at once and every [`REMIND_SECONDS`], `/status` shows it,
//! and each player in the world, or entering it, is told [`CANNOT_SAVE`], then [`SAVING_AGAIN`].
//!
//! **A piece of work that panics.** What it held is put back as it was ([`Saves::take_back`]), so
//! only whole pieces reach the store ([`crate::guard`]).

use std::collections::{BTreeMap, HashMap};

use yalain_net::{ConnId, LocalTime, ServerNet};
use yalain_store::{Store, StoreError, StoreResult, WorldState};

use crate::{Clock, Server};

/// Seconds between a character's saves (Empyrean's `player_save_interval`).
pub const SAVE_SECONDS: u32 = 300;

/// How long an option a client sends waits for a save of its character, in seconds.
pub const OPTIONS_SECONDS: f64 = 10.0;

/// The most characters a tick keeps.
pub const PER_TICK: usize = 1;

/// How often saves that fail are tried again, in seconds.
pub const RETRY_SECONDS: f64 = 5.0;

/// How often the operator is told again that saves are failing, in seconds.
pub const REMIND_SECONDS: f64 = 60.0;

/// What each player in the world is told as saves start failing, and as they work again.
pub const CANNOT_SAVE: &str =
    "This world can't save right now. What you do may be lost if the server goes down.";
pub const SAVING_AGAIN: &str = "This world is saving again.";

/// Saves that are failing.
#[derive(Debug, Clone, Copy)]
struct Failing {
    since: LocalTime,
    next_try: LocalTime,
    next_reminder: LocalTime,
}

/// When each character in the world was last kept, and what is on its way to the store.
#[derive(Debug, Default)]
pub struct Saves {
    /// The time as the world last read it.
    pub(crate) now: LocalTime,
    /// The same, in seconds since 1970.
    pub(crate) unix: i64,
    /// By character id: when it was last kept (or entered the world).
    last: HashMap<u64, LocalTime>,
    /// By character id: when an option not yet kept first came.
    options: HashMap<u64, LocalTime>,
    /// Characters' states not yet in the store.
    held: BTreeMap<u64, WorldState>,
    /// World records not yet in the store.
    held_records: BTreeMap<String, Vec<u8>>,
    /// What the piece of work under way replaced in what is held, oldest first.
    replaced: Vec<Replaced>,
    failing: Option<Failing>,
}

/// What a piece of work replaced in what is held.
#[derive(Debug)]
enum Replaced {
    State(u64, Option<WorldState>),
    Record(String, Option<Vec<u8>>),
}

impl Saves {
    /// A character was kept just now.
    pub(crate) fn kept(&mut self, character: u64) {
        self.last.insert(character, self.now);
        self.options.remove(&character);
    }

    /// A character's option changed: it goes with its next save, soon enough.
    pub(crate) fn option_changed(&mut self, character: u64) {
        self.options.entry(character).or_insert(self.now);
    }

    /// A character's state as it now stands, to go to the store at the end of this pass.
    pub(crate) fn hold(&mut self, character: u64, state: WorldState) {
        let was = self.held.insert(character, state);
        self.replaced.push(Replaced::State(character, was));
    }

    /// A world record as it now stands, to go to the store at the end of this pass.
    pub(crate) fn hold_record(&mut self, name: &str, value: Vec<u8>) {
        let was = self.held_records.insert(name.to_owned(), value);
        self.replaced.push(Replaced::Record(name.to_owned(), was));
    }

    /// A character is gone from the store: nothing more of it is to be kept.
    pub(crate) fn forget(&mut self, character: u64) {
        let was = self.held.remove(&character);
        self.replaced.push(Replaced::State(character, was));
    }

    /// A piece of the world's work begins: everything before it is whole.
    pub fn piece_begins(&mut self) {
        self.replaced.clear();
    }

    /// The piece of work under way panicked: what it held, half done, is put back as it was.
    pub(crate) fn take_back(&mut self) {
        for r in self.replaced.drain(..).rev() {
            match r {
                Replaced::State(id, Some(st)) => {
                    self.held.insert(id, st);
                }
                Replaced::State(id, None) => {
                    self.held.remove(&id);
                }
                Replaced::Record(name, Some(value)) => {
                    self.held_records.insert(name, value);
                }
                Replaced::Record(name, None) => {
                    self.held_records.remove(&name);
                }
            }
        }
    }

    /// Are saves failing?
    #[must_use]
    pub const fn failing(&self) -> bool {
        self.failing.is_some()
    }

    /// The characters whose states are not yet in the store.
    #[must_use]
    pub fn unsaved(&self) -> Vec<u64> {
        self.held.keys().copied().collect()
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
        None => state_of(s, character)
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
    s.saves.unix = clock.unix;
    let interval = f64::from(s.cfg.save_seconds);
    let mut due: Vec<(f64, u64, ConnId)> = Vec::new();
    for (world, p) in &s.players {
        let last = *s.saves.last.entry(p.character).or_insert(clock.local);
        let options = s.saves.options.get(&p.character);
        if clock.local.0 - last.0 >= interval
            || options.is_some_and(|o| clock.local.0 - o.0 >= OPTIONS_SECONDS)
        {
            due.push((last.0, p.character, *world));
        }
    }
    // Characters no longer in the world are forgotten.
    if s.saves.last.len() > s.players.len() {
        let here: std::collections::HashSet<u64> =
            s.players.values().map(|p| p.character).collect();
        s.saves.last.retain(|id, _| here.contains(id));
        s.saves.options.retain(|id, _| here.contains(id));
    }
    due.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    for (_, _, world) in due.into_iter().take(PER_TICK) {
        crate::world::keep_player(s, world);
    }
}

/// Keep every character in the world now (the console's `save-world`); the world goes on.
/// Returns how many were kept.
///
/// # Errors
/// The store failed; what it was to keep stays held, and is tried again.
pub fn save_world<S: Store>(s: &mut Server<S>, clock: Clock) -> Result<usize, StoreError> {
    s.saves.now = clock.local;
    let mut worlds: Vec<_> = s.players.keys().copied().collect();
    worlds.sort();
    for world in &worlds {
        crate::world::keep_player(s, *world);
    }
    write_out(s)?;
    Ok(worlds.len())
}

/// A character's state as it now stands: what is held for the store, else what the store has.
///
/// # Errors
/// The store failed.
pub fn state_of<S: Store>(s: &mut Server<S>, character: u64) -> StoreResult<Option<WorldState>> {
    if let Some(st) = s.saves.held.get(&character) {
        return Ok(Some(st.clone()));
    }
    s.store.world_state(character)
}

/// Keep the state of a character out of the world, at once.
///
/// # Errors
/// The store failed; the state stays held, and is tried again.
pub fn put<S: Store>(s: &mut Server<S>, character: u64, state: WorldState) -> StoreResult<()> {
    s.saves.hold(character, state);
    write_out(s)
}

/// Set more ids aside for the world's things when its block is running low, however far past its
/// end they were given while the store would not answer.
fn reserve_ids<S: Store>(s: &mut Server<S>) -> StoreResult<()> {
    while s.things.low() {
        let first = s.store.reserve_thing_numbers(crate::things::ID_BLOCK)?;
        s.things.add_block(first, crate::things::ID_BLOCK);
    }
    Ok(())
}

/// Send what is held to the store, as one transaction. Held still if it failed.
///
/// # Errors
/// The store failed.
pub fn write_out<S: Store>(s: &mut Server<S>) -> StoreResult<()> {
    if s.saves.held.is_empty() && s.saves.held_records.is_empty() {
        return Ok(());
    }
    let states: Vec<(u64, &WorldState)> = s.saves.held.iter().map(|(id, st)| (*id, st)).collect();
    let records: Vec<(&str, &[u8])> = s
        .saves
        .held_records
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_slice()))
        .collect();
    s.store.save_all(&states, &records)?;
    s.saves.held.clear();
    s.saves.held_records.clear();
    s.saves.replaced.clear();
    Ok(())
}

/// The end of a pass of the world: what it held goes to the store, or stays held and is tried again
/// in a while; a change in whether saves work is told.
pub(crate) fn end_of_pass<S: Store>(s: &mut Server<S>, net: &mut ServerNet, clock: Clock) {
    let now = clock.local;
    if s.saves.failing.is_some_and(|f| now.0 < f.next_try.0) {
        return;
    }
    match reserve_ids(s).and_then(|()| write_out(s)) {
        Ok(()) => {
            if let Some(f) = s.saves.failing.take() {
                tracing::warn!(
                    "saving works again, after {:.0} s; everything held is in the database",
                    now.0 - f.since.0
                );
                tell_everyone(s, net, SAVING_AGAIN);
            }
        }
        Err(e) => {
            let waiting = s.saves.held.len();
            let f = s.saves.failing.get_or_insert(Failing {
                since: now,
                next_try: now,
                next_reminder: now,
            });
            f.next_try = LocalTime(now.0 + RETRY_SECONDS);
            let (first, remind) = (f.since.0 == now.0, now.0 >= f.next_reminder.0);
            let since = now.0 - f.since.0;
            if remind {
                f.next_reminder = LocalTime(now.0 + REMIND_SECONDS);
                tracing::error!(
                    "SAVES ARE FAILING, for {since:.0} s: {e}. {waiting} characters are held in \
                     memory and tried again every {RETRY_SECONDS:.0} s; they are lost if the server \
                     dies before this is fixed. Look at the database's disk, its permissions and \
                     what else has it open."
                );
            }
            if first {
                tell_everyone(s, net, CANNOT_SAVE);
            }
        }
    }
}

/// Tell each player in the world.
fn tell_everyone<S: Store>(s: &Server<S>, net: &mut ServerNet, text: &str) {
    for world in s.players.keys() {
        crate::login::notice(net, *world, text);
    }
}
