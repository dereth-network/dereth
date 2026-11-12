//! Levers: the placed objects a world's lever rows name ([`yalain_content::levers`]).
//!
//! **Pulling.** A player who uses a lever is walked to within three meters of it (the lever's own
//! least use distance). There it pulls it: the player plays the pulling its kind names (as a key
//! turning in a lock), the lever plays its template's use animation, and the use is answered as an
//! object's.
//!
//! **What a pull opens.** The pull counts for every row the lever is in. Once every lever of a row
//! is pulled (each within the row's `within` seconds of the last, when it names a time), the row's
//! doors open for everyone, as a use opens them and whatever their locks, and stay open the row's
//! time ([`crate::usable::doors::held_open`]); the row's pulls start over.
//!
//! Nothing here is recorded: no lever was seen in use in 2013 or 2017. What a lever is and how near
//! it is used are its kind's script's; what it opens, and for how long, is the world's rows'.

use std::collections::HashMap;

use auberean_protocol::types::BehaviorParams;
use auberean_protocol::ServerMessage;
use yalain_net::{ConnId, ServerNet};
use yalain_store::Store;

use crate::places::{self, Use};
use crate::usable::doors::{self, Held};
use crate::{presence, progress, Clock, Server};

/// How near the lever the player is walked, in meters (the lever's script's least use distance).
pub const REACH: f32 = 3.0;

/// What the player plays as it pulls a lever (the lever's script's user animation).
pub const PULLING: u32 = 0x4100_00FA;

/// The pulls of each row still waiting on its other levers: when each lever was pulled (local
/// seconds), by the lever's place in the world's list of placed objects, by the row's key.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Levers {
    pub pulled: HashMap<String, HashMap<usize, f64>>,
}

/// Is the object of this id a lever of some row, standing in the world now?
#[must_use]
pub fn is_lever<S: Store>(s: &Server<S>, id: u64) -> bool {
    !s.content.levers.is_empty()
        && crate::events::content_standing(s, id).is_some_and(|i| s.content.levers.is_lever(i))
}

/// A player asks to use a lever: walked to it.
pub fn used<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, u: Use, clock: Clock) {
    places::walk_to(s, net, world, u, REACH);
    places::arrived(s, net, world, clock);
}

/// A player has come to a lever: it pulls it, the use is answered, and the rows it completes open
/// their doors.
pub fn at_lever<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    u: &Use,
    clock: Clock,
) {
    let Some(i) = crate::events::content_standing(s, u.item) else {
        return;
    };
    let (template, cell, origin) = {
        let o = &s.content.objects[i];
        (o.template, o.cell, o.origin)
    };
    if let Some(animation) = s
        .catalog
        .get(template)
        .and_then(|t| t.props.word("Usage_Animation"))
    {
        let lever = ServerMessage::PlayBehavior(auberean_protocol::physics::ObjectEvent {
            sender: places::object_stamp(s, u.item),
            action: BehaviorParams::behavior(animation),
        });
        places::to_knowers(s, net, u.item, &lever);
    }
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let pulls =
        ServerMessage::PlayBehavior(places::player_event(p, BehaviorParams::behavior(PULLING)));
    let me = p.character;
    places::to_all_knowing(s, net, me, &pulls);
    let distance = places::reach(&u.from, cell, origin).unwrap_or(0.0);
    let done = places::use_done(u, me, places::USE_FLAGS_PERSON, false, distance);
    progress::private(net, world, &done);
    pulled(s, net, i, clock);
}

/// A lever is pulled: each row it is in counts it, and a row whose levers are all pulled opens its
/// doors and starts over.
pub fn pulled<S: Store>(s: &mut Server<S>, net: &mut ServerNet, lever: usize, clock: Clock) {
    let now = clock.local.0;
    let rows: Vec<_> = s.content.levers.pulled_by(lever).cloned().collect();
    for row in rows {
        let pulls = s.levers.pulled.entry(row.key.clone()).or_default();
        if let Some(within) = row.within {
            pulls.retain(|_, at| now - *at <= within);
        }
        pulls.insert(lever, now);
        if !row.levers.iter().all(|l| pulls.contains_key(l)) {
            continue;
        }
        pulls.clear();
        let held = row.open_for.map_or(Held::UntilClosed, Held::For);
        for &door in &row.opens {
            let id = presence::content_id(&s.content.objects, door);
            doors::held_open(s, net, id, held, clock);
        }
    }
}
