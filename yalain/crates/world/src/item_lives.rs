//! Items that last only a while: an item of a template a world's row gives a lifetime is gone that
//! many seconds after it was made, by the calendar, its holder in the world or not.
//!
//! **Its end** is set as it is made ([`crate::inventory::make_item`]) and kept with it, so a piece
//! split from a stack, an item a broker kept and an item kept with its player while it was away
//! keep the end they had.
//!
//! **Going.** At the world's step after its end, an item in a pack (worn or not) is taken out of it
//! and the client told it is gone, as an item used up; one lying in the world is gone from it for
//! everyone who sees it. One in a corpse or a chest, or kept by a broker, waits until it comes out
//! into a pack or down. A player entering the world with one whose end passed while it was away
//! does not bring it in: its pack never shows it.

use yalain_net::ServerNet;
use yalain_store::Store;

use crate::holding::{self, Place};
use crate::state::PlayState;
use crate::Server;

/// The items in the world that have an end.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Lives {
    pub watched: Vec<u64>,
}

/// An item just made: if its template has a lifetime, it gets its end from now and is watched.
pub fn made<S: Store>(s: &mut Server<S>, item: u64) {
    let now = s.game_now;
    let Some(t) = s.things.get_mut(item) else {
        return;
    };
    let Some(lasts) = s.item_rows.lives.get(&t.template) else {
        return;
    };
    t.own.ends = Some(now + f64::from(*lasts));
    s.item_lives.watched.push(item);
}

/// An item come back into the world with what was kept of it: watched if it has an end.
pub fn came_back<S: Store>(s: &mut Server<S>, item: u64) {
    if s.things.get(item).is_some_and(|t| t.own.ends.is_some())
        && !s.item_lives.watched.contains(&item)
    {
        s.item_lives.watched.push(item);
    }
}

/// A character entering the world leaves behind every item whose end has passed: out of its pack
/// and out of what is kept of its items.
pub fn at_entry(play: &mut PlayState, now: f64) {
    let gone: Vec<u64> = play
        .item_states
        .iter()
        .filter(|k| k.own.ends.is_some_and(|e| e <= now))
        .map(|k| k.id)
        .collect();
    if gone.is_empty() {
        return;
    }
    play.items.retain(|h| !gone.contains(&h.id));
    play.item_states.retain(|k| !gone.contains(&k.id));
    crate::inventory::close_up(&mut play.items);
}

/// The world's step: every watched item whose end has come goes from where it is.
pub fn step<S: Store>(s: &mut Server<S>, net: &mut ServerNet) {
    let now = s.game_now;
    let due: Vec<u64> = s
        .item_lives
        .watched
        .iter()
        .copied()
        .filter(|id| {
            s.things
                .get(*id)
                .is_none_or(|t| t.own.ends.is_none_or(|e| e <= now))
        })
        .collect();
    for item in due {
        if goes(s, net, item) {
            s.item_lives.watched.retain(|x| *x != item);
        }
    }
}

/// An item whose end has come goes from where it is. Returns whether it is done with: gone, with
/// no end, or no longer in the world (kept with a player away, or gone already).
fn goes<S: Store>(s: &mut Server<S>, net: &mut ServerNet, item: u64) -> bool {
    let Some(template) = s
        .things
        .get(item)
        .filter(|t| t.own.ends.is_some())
        .map(|t| t.template)
    else {
        return true;
    };
    match holding::place_of(s, item) {
        Some(Place::Pack(character)) => {
            let world = s
                .players
                .iter()
                .find(|(_, p)| p.character == character)
                .map(|(w, _)| *w);
            if let Some(world) = world {
                crate::inventory::take_item(s, net, world, item, template);
            }
            true
        }
        Some(Place::Ground) => {
            crate::inventory::gone_from_where_it_lay(s, net, item);
            true
        }
        Some(Place::Contents(_) | Place::Kept(_) | Place::Nowhere) => false,
        None => true,
    }
}
