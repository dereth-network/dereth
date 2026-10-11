//! Stand-in ids: the object id a world gives each remote correspondent.
//!
//! A tell, a room line and a friends row each carry a sender id beside the name, and the retail
//! client makes a sender's name clickable only for ids in the player range it knows. A remote
//! character has no object on the receiving world, so the world gives it one id of its own, from a
//! block no local object uses ([`FIRST`] to [`LAST`]), and keeps it:
//!
//! - one id per remote identity (a character's address, or a channel's), the same for every local
//!   player, so a reply by id, a stored friend and a squelch all name the same correspondent;
//! - persisted, so the id survives a relog, a restart and a lost hub link;
//! - never reused, even after the correspondent is forgotten, so an old id in a client's squelch
//!   list or chat history can never come to mean someone else.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::address::Address;

/// The first stand-in id.
pub const FIRST: u32 = 0x6000_0001;
/// The last stand-in id.
pub const LAST: u32 = 0x6FFF_FFFF;

/// Whether `id` is in the stand-in block.
#[must_use]
pub fn is_stand_in(id: u32) -> bool {
    (FIRST..=LAST).contains(&id)
}

/// A world's stand-in table: every identity it has ever given an id, and the next id to give.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StandIns {
    /// The next id to give out.
    next: u32,
    /// Identity key (see [`Address::key`]) to id.
    by_key: BTreeMap<String, u32>,
    /// Id to the address as last written, for display and for replies.
    by_id: BTreeMap<u32, Address>,
}

impl Default for StandIns {
    fn default() -> Self {
        Self {
            next: FIRST,
            by_key: BTreeMap::new(),
            by_id: BTreeMap::new(),
        }
    }
}

/// The block is used up: 268 million identities later.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Exhausted;

impl StandIns {
    /// The id of `who`, given now if it has none. The address is remembered as written, so a
    /// later spelling with other capitals updates the display and keeps the id.
    ///
    /// # Errors
    /// [`Exhausted`] when every id in the block has been given.
    pub fn id_for(&mut self, who: &Address) -> Result<u32, Exhausted> {
        let key = who.key();
        if let Some(&id) = self.by_key.get(&key) {
            self.by_id.insert(id, who.clone());
            return Ok(id);
        }
        if self.next > LAST || self.next < FIRST {
            return Err(Exhausted);
        }
        let id = self.next;
        // LAST is below u32::MAX, so this never wraps; past LAST the block is exhausted.
        self.next = id + 1;
        self.by_key.insert(key, id);
        self.by_id.insert(id, who.clone());
        Ok(id)
    }

    /// The id `who` already has, if any.
    #[must_use]
    pub fn get(&self, who: &Address) -> Option<u32> {
        self.by_key.get(&who.key()).copied()
    }

    /// The identity an id stands for, if it is one this world gave.
    #[must_use]
    pub fn who(&self, id: u32) -> Option<&Address> {
        self.by_id.get(&id)
    }

    /// The table as it should be saved: the same identities and ids, with the next id moved on by
    /// `margin`. A world that saves this, gives ids, and stops before it saves again starts past
    /// every id it may have given, so even a crash between giving an id and saving never gives one
    /// twice. Each save after a restart skips at most `margin` ids, of the block's 268 million.
    #[must_use]
    pub fn reserved(&self, margin: u32) -> Self {
        let mut t = self.clone();
        t.next = t.next.saturating_add(margin).min(LAST + 1);
        t
    }

    /// Forgets the identity `id` stands for. The id is never given again: one a client still holds
    /// names no one, never someone else, and the identity gets a new id if it is seen again.
    pub fn forget(&mut self, id: u32) -> Option<Address> {
        let who = self.by_id.remove(&id)?;
        self.by_key.remove(&who.key());
        Some(who)
    }

    /// Every id the table holds, with its identity as last written.
    pub fn iter(&self) -> impl Iterator<Item = (u32, &Address)> {
        self.by_id.iter().map(|(id, who)| (*id, who))
    }

    /// How many identities the table holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_key.len()
    }

    /// Whether none has.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_key.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(s: &str) -> Address {
        Address::parse(s).expect("an address")
    }

    /// Spec: stand-in ids: one id per identity, from the stand-in block.
    #[test]
    fn each_identity_gets_one_id_from_the_stand_in_block() {
        let mut t = StandIns::default();
        let bob = t.id_for(&a("Bob@coldeve")).expect("an id");
        assert!(is_stand_in(bob));
        assert_eq!(bob, FIRST);
        assert_eq!(
            t.id_for(&a("BOB@coldeve")).expect("an id"),
            bob,
            "case is one identity"
        );
        let ann = t.id_for(&a("Ann@frost")).expect("an id");
        assert_ne!(ann, bob);
        assert_eq!(
            t.who(ann).map(Address::to_string).as_deref(),
            Some("Ann@frost")
        );
        assert_eq!(
            t.who(bob).map(Address::to_string).as_deref(),
            Some("BOB@coldeve"),
            "the last spelling is the display"
        );
        assert!(!is_stand_in(0x5000_0001) && !is_stand_in(0x7000_0000));
    }

    /// Spec: stand-in ids: the table survives a save and a load unchanged, ids and next id.
    #[test]
    fn the_table_round_trips_through_its_saved_form() {
        let mut t = StandIns::default();
        let bob = t.id_for(&a("Bob@coldeve")).expect("an id");
        let json = serde_json::to_string(&t).expect("saved");
        let mut back: StandIns = serde_json::from_str(&json).expect("loaded");
        assert_eq!(back, t);
        assert_eq!(back.id_for(&a("Bob@coldeve")), Ok(bob));
        assert_ne!(
            back.id_for(&a("Eve@coldeve")),
            Ok(bob),
            "no id is given twice"
        );
    }

    /// Spec: stand-in ids: a saved table starts past every id given since it was saved.
    #[test]
    fn a_saved_table_starts_past_every_id_given_after_the_save() {
        let mut t = StandIns::default();
        t.id_for(&a("Bob@coldeve")).expect("an id");
        let saved = t.reserved(1024);
        // Ids given after the save, then a crash before the next one.
        let name = |i: u8| {
            format!(
                "Eve{}{}@coldeve",
                char::from(b'a' + i % 26),
                char::from(b'a' + i / 26)
            )
        };
        let given: Vec<u32> = (0..100u8)
            .map(|i| t.id_for(&a(&name(i))).expect("an id"))
            .collect();
        let mut restarted = saved;
        let next = restarted.id_for(&a("New@coldeve")).expect("an id");
        assert!(
            given.iter().all(|&g| g < next),
            "no id given before the crash is given again"
        );
        assert_eq!(
            restarted.get(&a("Bob@coldeve")),
            t.get(&a("Bob@coldeve")),
            "saved ids keep their identities"
        );
        let full = StandIns {
            next: LAST,
            ..StandIns::default()
        };
        assert_eq!(
            full.reserved(1024).next,
            LAST + 1,
            "the margin never runs past the block"
        );
    }

    /// Spec: stand-in ids: a forgotten identity's id is never given again; seen again, the
    /// identity gets a new one.
    #[test]
    fn a_forgotten_identitys_id_is_never_given_again() {
        let mut t = StandIns::default();
        let bob = t.id_for(&a("Bob@coldeve")).expect("an id");
        let ann = t.id_for(&a("Ann@coldeve")).expect("an id");
        assert_eq!(
            t.forget(bob).map(|w| w.to_string()).as_deref(),
            Some("Bob@coldeve")
        );
        assert_eq!(
            (t.len(), t.who(bob), t.get(&a("Bob@coldeve"))),
            (1, None, None)
        );
        assert_eq!(t.iter().map(|(id, _)| id).collect::<Vec<_>>(), [ann]);
        let again = t.id_for(&a("Bob@coldeve")).expect("an id");
        assert!(again != bob && again != ann, "{again:#x}");
        assert!(t.forget(bob).is_none());
    }

    /// Spec: stand-in ids: the block ends; no id outside it is ever given.
    #[test]
    fn the_last_id_is_given_once_and_then_the_block_is_exhausted() {
        let mut t = StandIns {
            next: LAST,
            ..StandIns::default()
        };
        assert_eq!(t.id_for(&a("Last@coldeve")), Ok(LAST));
        assert_eq!(t.id_for(&a("After@coldeve")), Err(Exhausted));
        assert_eq!(
            t.id_for(&a("Last@coldeve")),
            Ok(LAST),
            "a known identity still answers"
        );
    }
}
