//! Where an item is, and the one way it moves.
//!
//! An item's place is written twice: in its own record (what holds it, or where it lies) and in the
//! list of whatever holds it (a player's pack, a corpse's or a chest's contents). [`move_item`]
//! changes both at once, and only from the place its caller expects the item to be: from anywhere
//! else it refuses and nothing changes, so a move that was asked for some time ago (a take whose
//! behavior is through, a pickup, a trade's items) can't take an item that has gone elsewhere since.

use yalain_net::ConnId;
use yalain_store::{Store, StoredPosition};

use crate::inventory::{insert_at, room_for_one};
use crate::state::Held;
use crate::Server;

/// Where an item is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// In a player's pack, worn or not (the player's character).
    Pack(u64),
    /// In a corpse or a chest, which lists it.
    Contents(u64),
    /// Held by what keeps no list of its own: a creature's weapon, a broker's goods.
    Kept(u64),
    /// Lying in the world.
    Ground,
    /// Nowhere: just made, on its way down from a player's hand, or about to be gone.
    Nowhere,
}

/// Where an item is to go.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum To {
    /// Into the pack of the player on this world connection.
    Pack(ConnId, Slot),
    /// Into a corpse or a chest, at the end of its list.
    Contents(u64),
    /// To what keeps no list of it.
    Kept(u64),
    /// Down in the world, here.
    Ground(StoredPosition),
    /// Out of every place.
    Nowhere,
}

/// Where in a pack an item goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    /// The front of the first segment with room (the pack's front when every one is full).
    Room,
    /// This slot, the items from there on moving up one.
    At(u32),
}

/// The item is not where its mover expected it to be (or is no longer in the world).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stale {
    pub item: u64,
    pub expected: Place,
    pub found: Option<Place>,
}

/// The list that holds an item in a corpse or a chest, when `holder` is one.
fn list<S: Store>(s: &Server<S>, holder: u64) -> Option<&Vec<u64>> {
    if let Some(c) = s.things.get(holder).and_then(|t| t.corpse.as_ref()) {
        return Some(&c.contents);
    }
    s.chests.by_id.get(&holder).map(|c| &c.contents)
}

fn list_mut<S: Store>(s: &mut Server<S>, holder: u64) -> Option<&mut Vec<u64>> {
    if s.things.get(holder).is_some_and(|t| t.corpse.is_some()) {
        return s
            .things
            .get_mut(holder)
            .and_then(|t| t.corpse.as_mut())
            .map(|c| &mut c.contents);
    }
    s.chests.by_id.get_mut(&holder).map(|c| &mut c.contents)
}

/// The world connection of the player whose character this is.
fn world_of<S: Store>(s: &Server<S>, character: u64) -> Option<ConnId> {
    crate::social::world_of(s, character)
}

/// Where an item is, as its own record and the list that holds it both say; none when it is not
/// in the world, or when the two disagree.
#[must_use]
pub fn place_of<S: Store>(s: &Server<S>, item: u64) -> Option<Place> {
    let t = s.things.get(item)?;
    match t.container {
        Some(holder) => {
            if let Some(p) = world_of(s, holder).and_then(|w| s.players.get(&w)) {
                return p
                    .play
                    .items
                    .iter()
                    .any(|h| h.id == item)
                    .then_some(Place::Pack(holder));
            }
            match list(s, holder) {
                Some(l) => l.contains(&item).then_some(Place::Contents(holder)),
                None => Some(Place::Kept(holder)),
            }
        }
        None if t.at.is_some() => Some(Place::Ground),
        None => Some(Place::Nowhere),
    }
}

/// Move an item from where its caller expects it to be to somewhere else: out of the list that
/// held it, into the one that is to hold it, and its own record changed to match. Refused, nothing
/// changed, when the item is not where expected. Returns the item's slot in its new place (its
/// pack slot; its place in a corpse's or a chest's list; 0 elsewhere).
///
/// # Errors
/// The item is elsewhere, or not in the world.
pub fn move_item<S: Store>(
    s: &mut Server<S>,
    item: u64,
    from: Place,
    to: To,
) -> Result<u32, Stale> {
    let found = place_of(s, item);
    let target_ok = match to {
        To::Pack(world, _) => s.players.contains_key(&world),
        To::Contents(holder) => list(s, holder).is_some(),
        _ => true,
    };
    let thing = s.things.get(item).map(|t| (t.template, t.quantity));
    let Some((template, quantity)) = thing.filter(|_| found == Some(from) && target_ok) else {
        return Err(Stale {
            item,
            expected: from,
            found,
        });
    };
    match from {
        Place::Pack(holder) => {
            if let Some(p) = world_of(s, holder).and_then(|w| s.players.get_mut(&w)) {
                p.play.items.retain(|h| h.id != item);
            }
        }
        Place::Contents(holder) => {
            if let Some(l) = list_mut(s, holder) {
                l.retain(|x| *x != item);
            }
        }
        Place::Kept(_) | Place::Ground | Place::Nowhere => {}
    }
    if let Some(t) = s.things.get_mut(item) {
        t.at = None;
        t.container = None;
    }
    let slot = match to {
        To::Pack(world, slot) => {
            let Some(p) = s.players.get_mut(&world) else {
                return Ok(0);
            };
            let slot = match slot {
                Slot::Room => room_for_one(&mut p.play.items),
                Slot::At(n) => {
                    insert_at(&mut p.play.items, n);
                    n
                }
            };
            p.play.items.push(Held {
                id: item,
                template,
                slot,
                location: 0,
                quantity,
            });
            let owner = p.character;
            if let Some(t) = s.things.get_mut(item) {
                t.container = Some(owner);
            }
            slot
        }
        To::Contents(holder) => {
            let n = list_mut(s, holder).map_or(0, |l| {
                l.push(item);
                l.len() - 1
            });
            if let Some(t) = s.things.get_mut(item) {
                t.container = Some(holder);
            }
            u32::try_from(n).unwrap_or(0)
        }
        To::Kept(holder) => {
            if let Some(t) = s.things.get_mut(item) {
                t.container = Some(holder);
            }
            0
        }
        To::Ground(at) => {
            if let Some(t) = s.things.get_mut(item) {
                t.at = Some(at);
            }
            0
        }
        To::Nowhere => 0,
    };
    Ok(slot)
}

/// Take an item out of the place its caller expects it in and out of the world. Returns what it
/// was.
///
/// # Errors
/// The item is elsewhere, or not in the world.
pub fn destroy_item<S: Store>(
    s: &mut Server<S>,
    item: u64,
    from: Place,
) -> Result<crate::things::Thing, Stale> {
    move_item(s, item, from, To::Nowhere)?;
    s.things.remove(item).ok_or(Stale {
        item,
        expected: from,
        found: None,
    })
}

/// An item's place in a corpse's or a chest's list.
#[must_use]
pub fn slot_in<S: Store>(s: &Server<S>, holder: u64, item: u64) -> Option<u32> {
    list(s, holder)?
        .iter()
        .position(|x| *x == item)
        .and_then(|i| u32::try_from(i).ok())
}

/// Take an item out of the pack of the player on this world connection and out of the world: one
/// whose template the world lacks is held in the pack's list alone, and leaves just that. Returns
/// what it was in the world, if anything.
///
/// # Errors
/// The pack does not hold it.
pub fn destroy_held<S: Store>(
    s: &mut Server<S>,
    world: ConnId,
    item: u64,
) -> Result<Option<crate::things::Thing>, Stale> {
    let Some(character) = s.players.get(&world).map(|p| p.character) else {
        return Err(Stale {
            item,
            expected: Place::Nowhere,
            found: place_of(s, item),
        });
    };
    if s.things.get(item).is_some() {
        return destroy_item(s, item, Place::Pack(character)).map(Some);
    }
    let Some(i) = s
        .players
        .get(&world)
        .and_then(|p| p.play.items.iter().position(|h| h.id == item))
    else {
        return Err(Stale {
            item,
            expected: Place::Pack(character),
            found: None,
        });
    };
    if let Some(p) = s.players.get_mut(&world) {
        p.play.items.remove(i);
    }
    Ok(None)
}
