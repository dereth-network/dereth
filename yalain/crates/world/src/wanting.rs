//! Wanting objects ([`yalain_content::wanting`]) as the world keeps them: the item each object has
//! drawn and wants now.
//!
//! **Drawn.** The first item of a row's set used on one of its objects, or given to it, draws the
//! object's want by the row's weights with the world's dice; the object keeps it, the same for every
//! player, apart from the row's other objects.
//!
//! **Refused.** Another item of the set, once the player has come to the object, is refused: the
//! object says the row's line to the player, the item stays in the player's pack (handed back, for a
//! give), and no quest step runs.
//!
//! **Met.** The item wanted, used so that a quest step runs, or given so that a step runs or a give
//! row takes it, is the want met: forgotten, so the next item of the set to come draws again. A use
//! or give of it that runs nothing leaves the want as it is. Nothing of this is kept across a
//! restart; a reload that changes the rows forgets every want.
//!
//! Nothing here is recorded: no 2017 session went where an object wants so. The set, its weights and
//! what an object says are the world's rows'.

use std::collections::BTreeMap;

use yalain_net::{ConnId, ServerNet};
use yalain_store::Store;

use crate::Server;

/// The item each object wants now (its template), by the object's place in the world's list of
/// placed objects.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Wants {
    pub drawn: BTreeMap<usize, u32>,
}

/// What an object makes of an item of a template come to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Judged {
    /// The object wants by no row, or the item is not of its row's set.
    Not,
    /// The item it wants: the use or give goes on.
    Wanted,
    /// Another item of its set: refused.
    Refused,
}

/// What the placed object at this place in the world's list makes of an item of this template used
/// on it or given to it, drawing its want first if it has none.
pub fn judge<S: Store>(s: &mut Server<S>, object: usize, item: u32) -> Judged {
    let Some(r) = s.content.wanting.of_object(object) else {
        return Judged::Not;
    };
    if !r.in_set(item) {
        return Judged::Not;
    }
    let wanted = match s.wants.drawn.get(&object) {
        Some(w) => *w,
        None => {
            let items = r.items.clone();
            let Some(w) = crate::makes::pick_one(s, &items) else {
                return Judged::Not;
            };
            s.wants.drawn.insert(object, w);
            w
        }
    };
    if wanted == item {
        Judged::Wanted
    } else {
        Judged::Refused
    }
}

/// The item the placed object at this place in the world's list wants now, if it has drawn one.
#[must_use]
pub fn wanted<S: Store>(s: &Server<S>, object: usize) -> Option<u32> {
    s.wants.drawn.get(&object).copied()
}

/// An item the object does not want has come to it: it says its row's line to the player. The
/// caller answers the use, or hands the item back.
pub fn refused<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, object: usize) {
    let Some((string, table)) = s.content.wanting.of_object(object).and_then(|r| r.refused) else {
        return;
    };
    let speaker = crate::talk::content_speaker(s, object);
    let words = yalain_content::talk::Words::Ref(string, table);
    crate::talk::say(
        s,
        net,
        world,
        &speaker,
        &words,
        yalain_content::talk::How::Tell,
    );
}

/// An item of this template, used on the placed object at this place in the world's list or given
/// to it, ran what it moves: if it is the one the object wants, the want is met and forgotten.
pub fn met<S: Store>(s: &mut Server<S>, object: usize, item: u32) {
    if s.wants.drawn.get(&object) == Some(&item) {
        s.wants.drawn.remove(&object);
    }
}

/// The rows given anew: every want is forgotten, drawn again as an item next comes.
pub fn forget_all<S: Store>(s: &mut Server<S>) {
    s.wants = Wants::default();
}
