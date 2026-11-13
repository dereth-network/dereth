//! Guarded objects ([`yalain_content::guards`]) as the world keeps them: whether each row's camp
//! stood dead at the last step, and the objects a use has taken from the world for a while.
//!
//! **Dead.** A camp's creatures are all dead once it has filled and keeps none alive now: its last
//! creature died and none has come back yet. Until a camp first fills, it counts as guarded.
//!
//! **Refused.** A use of a guarded object while its camp keeps a creature alive is refused once the
//! player has come to it: the object says the row's line to the player, and the use is answered as
//! an object's with nothing more done (no quest step runs).
//!
//! **Hidden.** A row that hides its objects keeps them from the world while its camp keeps a
//! creature alive: at the step its last creature is found dead they are made for everyone who sees
//! their place, and at the step one is found back they are forgotten.
//!
//! **Gone a while.** A row that names a time takes an object from the world at each use of it that
//! is carried out (its quest steps run), for that long: it is forgotten by everyone at once, and
//! made again at the first step after its time unless something else keeps it from the world then.
//! Nothing of this is kept across a restart.
//!
//! Nothing here is recorded: no guarded object was seen in use in 2013 or 2017. Which objects a
//! camp guards, what they say and how long they go are the world's rows'.

use std::collections::{BTreeMap, BTreeSet};

use yalain_net::{ConnId, ServerNet};
use yalain_store::Store;

use crate::places::{self, Use};
use crate::{presence, progress, Clock, Server};

/// What the world keeps of its guarded rows: whether each row's camp stood dead at the last step,
/// by its key; and the objects gone a while, by their place in the world's list of placed objects,
/// with when (local seconds) each comes back.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Guards {
    pub dead: BTreeMap<String, bool>,
    pub gone: BTreeMap<usize, f64>,
}

/// Are every one of a camp's creatures dead now: it has filled, and keeps none alive?
#[must_use]
pub fn camp_dead<S: Store>(s: &Server<S>, camp: usize) -> bool {
    s.camps
        .get(camp)
        .is_some_and(|c| c.filled && c.alive.is_empty())
}

/// Does a creature of a camp guarding the placed object at this place in the world's list live
/// now?
#[must_use]
pub fn guarded<S: Store>(s: &Server<S>, object: usize) -> bool {
    let t = &s.content.guards;
    !t.is_empty() && t.of_object(object).any(|r| !camp_dead(s, r.camp))
}

/// Is the placed object at this place in the world's list kept from the world now by a guarded
/// row: one that hides it while its camp keeps a creature alive, or a use that took it a while?
#[must_use]
pub fn hides<S: Store>(s: &Server<S>, object: usize) -> bool {
    let t = &s.content.guards;
    if t.is_empty() {
        return false;
    }
    s.guards.gone.contains_key(&object)
        || t.of_object(object)
            .any(|r| r.hidden && !s.guards.dead.get(&r.key).copied().unwrap_or(false))
}

/// A player has come to a guarded object while a creature of its camp lives: the object says the
/// row's line, and the use is answered with nothing more done.
pub fn refused<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    u: &Use,
    object: usize,
) {
    let line = s
        .content
        .guards
        .of_object(object)
        .find(|r| !camp_dead(s, r.camp))
        .and_then(|r| r.refused);
    if let Some((string, table)) = line {
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
    let (cell, origin) = {
        let o = &s.content.objects[object];
        (o.cell, o.origin)
    };
    let me = s.players.get(&world).map_or(0, |p| p.character);
    let distance = places::reach(&u.from, cell, origin).unwrap_or(0.0);
    let done = places::use_done(u, me, places::USE_FLAGS_PERSON, false, distance);
    progress::private(net, world, &done);
}

/// A use of the placed object at this place in the world's list has been carried out: a row that
/// names a time takes it from the world for that long.
pub fn used<S: Store>(s: &mut Server<S>, net: &mut ServerNet, object: usize, clock: Clock) {
    let Some(gone_for) = s
        .content
        .guards
        .of_object(object)
        .filter_map(|r| r.gone_for)
        .reduce(f64::max)
    else {
        return;
    };
    if s.guards.gone.contains_key(&object) {
        return;
    }
    presence::content_gone(s, net, object);
    s.guards.gone.insert(object, clock.local.0 + gone_for);
}

/// At each world step: a row whose camp has died out or come back since the last step shows or
/// hides its objects; an object whose time gone is over comes back.
pub fn step<S: Store>(s: &mut Server<S>, net: &mut ServerNet, clock: Clock) {
    if s.content.guards.is_empty() {
        return;
    }
    let mut show = BTreeSet::new();
    let mut hide = BTreeSet::new();
    let rows = s.content.guards.rows.clone();
    for r in &rows {
        let dead = camp_dead(s, r.camp);
        let was = s.guards.dead.insert(r.key.clone(), dead).unwrap_or(false);
        if r.hidden && dead != was {
            for &o in &r.objects {
                if dead {
                    show.insert(o);
                } else {
                    hide.insert(o);
                }
            }
        }
    }
    let now = clock.local.0;
    let back: Vec<usize> = s
        .guards
        .gone
        .iter()
        .filter(|(_, at)| **at <= now)
        .map(|(o, _)| *o)
        .collect();
    for o in back {
        s.guards.gone.remove(&o);
        show.insert(o);
    }
    for o in hide {
        if !s.guards.gone.contains_key(&o) {
            presence::content_gone(s, net, o);
        }
    }
    for o in show {
        if !crate::events::hides(s, o) {
            presence::content_placed(s, net, o, None);
        }
    }
}
