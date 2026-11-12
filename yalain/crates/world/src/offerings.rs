//! A world's offerings ([`yalain_content::offerings`]) as the world keeps them: what each holds, the
//! same for every player, and until when it stands open.
//!
//! **Taking.** A give to the object of an item of its set that it does not hold yet, while it is
//! not open, is taken: the object says its lines for a give of that item, the item is kept (it
//! leaves the giver's pack and the world, destroyed in the object, as everyone who sees the object
//! is told), and the object that stands for it, if any, is made for everyone who sees its place.
//! Any other give of an item of its set (one it holds, or any while it is open) is refused as a
//! give it wants nothing of is, and the item handed back. A give the item's quest steps take runs
//! them instead; a give of an item not in the set is the object's as any other.
//!
//! **Open.** As the last item of the set comes, the objects standing for the others are forgotten
//! and the offering's own objects are made, all in that moment (2017: both passes recorded at the
//! Chaos Portal, its two flares gone and the portal there in the instant of the third part's
//! keeping), standing for its time; then they are forgotten and it holds nothing again.
//!
//! **Kept.** What each offering holds, and until when one stands open, is the world record
//! [`RECORD`], written in the pass that changes it and read at the first step, so a restart keeps
//! it. An offering a world no longer has, or an item no longer of its set, is dropped.
//!
//! A world with no offerings does none of this.

use std::collections::{BTreeMap, BTreeSet};

use yalain_net::{ConnId, ServerNet};
use yalain_store::Store;

use crate::{presence, Clock, Server};

/// The world's record of its offerings.
pub const RECORD: &str = "offerings";

/// The record's first line.
const RECORD_HEAD: &str = "offerings 1";

/// What each offering holds and until when (seconds since 1970) one stands open, by its key, read
/// from the record once.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Offerings {
    pub loaded: bool,
    pub held: BTreeMap<String, BTreeSet<u32>>,
    pub open_until: BTreeMap<String, i64>,
}

impl Offerings {
    /// The record in the server's encoding: a head line, then a line an item held (`held`, the
    /// offering's key, the item's template in hexadecimal) and a line an offering open (`open`,
    /// its key, until when), tab-separated.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = format!("{RECORD_HEAD}\n");
        for (key, items) in &self.held {
            for item in items {
                out.push_str(&format!("held\t{key}\t{item:08X}\n"));
            }
        }
        for (key, until) in &self.open_until {
            out.push_str(&format!("open\t{key}\t{until}\n"));
        }
        out.into_bytes()
    }

    /// Read the record back; none when it does not read.
    #[must_use]
    pub fn from_bytes(b: &[u8]) -> Option<Self> {
        let text = std::str::from_utf8(b).ok()?;
        let mut lines = text.lines();
        if lines.next()? != RECORD_HEAD {
            return None;
        }
        let mut o = Self::default();
        for line in lines.filter(|l| !l.is_empty()) {
            let c: Vec<&str> = line.split('\t').collect();
            match c[..] {
                ["held", key, item] => {
                    let item = u32::from_str_radix(item, 16).ok()?;
                    o.held.entry(key.to_string()).or_default().insert(item);
                }
                ["open", key, until] => {
                    o.open_until.insert(key.to_string(), until.parse().ok()?);
                }
                _ => return None,
            }
        }
        Some(o)
    }

    /// Drop what the world's offerings no longer name: an offering it no longer has, an item no
    /// longer of its set.
    pub fn keep_only(&mut self, table: &yalain_content::offerings::OfferingTable) {
        self.held.retain(|key, items| {
            let Some(o) = table.offering(key) else {
                return false;
            };
            items.retain(|i| o.takes.iter().any(|p| p.item == *i));
            !items.is_empty()
        });
        self.open_until
            .retain(|key, _| table.offering(key).is_some());
    }
}

/// Read the offerings once. Whether they are read: while the store cannot answer, nothing about
/// them is kept.
pub fn ensure<S: Store>(s: &mut Server<S>) -> bool {
    if s.offerings.loaded {
        return true;
    }
    let Some(read) = crate::saves::read_record(s, RECORD, Offerings::from_bytes) else {
        return false;
    };
    let mut read = read.unwrap_or_default();
    read.keep_only(&s.content.offerings);
    s.offerings = Offerings {
        loaded: true,
        ..read
    };
    true
}

/// Keep the offerings as they now stand, at the end of this pass.
fn keep<S: Store>(s: &mut Server<S>) {
    if s.offerings.loaded {
        let b = s.offerings.to_bytes();
        s.saves.hold_record(RECORD, b);
    }
}

/// Is an offering open now?
fn is_open<S: Store>(s: &Server<S>, key: &str) -> bool {
    s.offerings.open_until.contains_key(key)
}

/// Does an offering hold an item now?
fn holds<S: Store>(s: &Server<S>, key: &str, item: u32) -> bool {
    s.offerings.held.get(key).is_some_and(|h| h.contains(&item))
}

/// Is the placed object at this place in the world's list kept from the world now by an offering:
/// one standing for an item the offering does not hold, or holds while it is open; one the offering
/// opens, while it is not open.
#[must_use]
pub fn hides<S: Store>(s: &Server<S>, object: usize) -> bool {
    let t = &s.content.offerings;
    if t.is_empty() {
        return false;
    }
    t.offerings.iter().any(|o| {
        let open = is_open(s, &o.key);
        if o.opens.contains(&object) {
            return !open;
        }
        o.takes
            .iter()
            .any(|p| p.shows == Some(object) && (open || !holds(s, &o.key, p.item)))
    })
}

/// What an offering makes of an item given its object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Offered {
    /// The object takes no offering, or the item is not of its set.
    Not,
    /// An item of its set it does not hold, while it is not open: taken.
    Takes,
    /// An item of its set it holds already, or any while it is open: refused.
    Refuses,
}

/// What the offering of the placed object at this place in the world's list makes of an item of
/// this template given to it. While what the offerings hold cannot be read, an item of the set is
/// refused.
pub fn given<S: Store>(s: &mut Server<S>, object: usize, item: u32) -> Offered {
    let Some(o) = s.content.offerings.of_object(object) else {
        return Offered::Not;
    };
    if !o.takes.iter().any(|p| p.item == item) {
        return Offered::Not;
    }
    let key = o.key.clone();
    if !ensure(s) || is_open(s, &key) || holds(s, &key, item) {
        Offered::Refuses
    } else {
        Offered::Takes
    }
}

/// The offering of the placed object at this place in the world's list has taken an item of this
/// template (kept by the caller): it holds it, and what stands for it is made; or it holds the
/// whole set now, and opens.
pub fn took<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    object: usize,
    item: u32,
    clock: Clock,
) {
    if !ensure(s) {
        return;
    }
    let Some(o) = s.content.offerings.of_object(object).cloned() else {
        return;
    };
    let held = s.offerings.held.entry(o.key.clone()).or_default();
    held.insert(item);
    let whole = o.takes.iter().all(|p| held.contains(&p.item));
    if whole {
        // What stood for the others goes, as the offering's own objects come.
        s.offerings
            .open_until
            .insert(o.key.clone(), clock.unix + i64::from(o.lasts));
        for p in &o.takes {
            if let Some(i) = p.shows.filter(|_| p.item != item) {
                presence::content_gone(s, net, i);
            }
        }
        for &i in &o.opens {
            placed(s, net, i);
        }
    } else if let Some(i) = o
        .takes
        .iter()
        .find(|p| p.item == item)
        .and_then(|p| p.shows)
    {
        placed(s, net, i);
    }
    keep(s);
}

/// An object an offering now stands is made for those who see it, unless something else (an event
/// that is off) still keeps it from the world.
fn placed<S: Store>(s: &mut Server<S>, net: &mut ServerNet, object: usize) {
    if !crate::events::hides(s, object) {
        presence::content_placed(s, net, object, None);
    }
}

/// At each world step: an offering whose time open is over closes, its objects forgotten, and
/// holds nothing again.
pub fn step<S: Store>(s: &mut Server<S>, net: &mut ServerNet, clock: Clock) {
    if s.content.offerings.is_empty() || !ensure(s) {
        return;
    }
    let over: Vec<String> = s
        .offerings
        .open_until
        .iter()
        .filter(|(_, until)| **until <= clock.unix)
        .map(|(k, _)| k.clone())
        .collect();
    if over.is_empty() {
        return;
    }
    for key in &over {
        let opens = s
            .content
            .offerings
            .offering(key)
            .map(|o| o.opens.clone())
            .unwrap_or_default();
        for i in opens {
            presence::content_gone(s, net, i);
        }
        s.offerings.open_until.remove(key);
        s.offerings.held.remove(key);
    }
    keep(s);
}

/// A give the object's offering takes: its lines for that item said, and the item kept.
pub(crate) fn takes<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    object: usize,
    given: crate::gives::Given,
    clock: Clock,
) {
    let who = s.content.objects[object].template;
    let speaker = crate::talk::content_speaker(s, object);
    for line in crate::talk::with_give(s, who, Some(object), given.template) {
        crate::talk::say(s, net, world, &speaker, &line.words, line.how);
    }
    crate::gives::keep(s, net, world, given);
    took(s, net, object, given.template, clock);
}
