//! Allegiance halls: a monarch who uses a hall's binding stone binds its whole allegiance to the hall
//! for Allegiance Recall.
//!
//! Anyone but a monarch is refused at once, wherever it stands (the stone is for monarchs alone). A
//! monarch is walked to the stone. Come to it:
//!
//! - one the stone's own requirements keep out (the quest its template needs, given by reading the
//!   hall's sign) is refused, as such a use of any object is;
//! - one short of the hall's cost is told it has not enough money, and nothing is paid or bound;
//! - otherwise the cost is paid, the allegiance is bound to the hall in place of any hall before,
//!   the monarch is told so, and the use goes on as the stone's own: the quest effects its template
//!   names for a use (completing the quest that gave the permission), then its quest steps.
//!
//! From then on Allegiance Recall takes every member of the allegiance to the hall's place.

use yalain_net::{ConnId, ServerNet};
use yalain_store::Store;

use crate::allegiance::BoundHall;
use crate::places::{self, Use};
use crate::{progress, quals, Clock, Server};

/// How near the stone a monarch is walked, in meters.
pub const REACH: f32 = places::PERSON_REACH;

/// "You do not have enough money to bind to this allegiance stone."
pub const TOO_POOR: (u32, u32) = (0x7B54_42C8, 0x2500_0014);
/// "You succesfully bind your allegiance to the Allegiance Recall Stone."
pub const BOUND: (u32, u32) = (0xD3B8_00CC, 0x2500_0014);
/// The kind of text both are shown as.
pub const SHOWN_AS: u32 = 8;

/// The hall whose binding stone is this placed object, if it is one.
#[must_use]
pub fn stone<S: Store>(s: &Server<S>, id: u64) -> Option<&yalain_content::halls::Hall> {
    let i = crate::presence::content_by_id(&s.content.objects, id)?;
    s.content.halls.of_stone(i)
}

/// A monarch asks to use a hall's binding stone: walked to it.
pub fn used<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, u: Use, clock: Clock) {
    places::walk_to(s, net, world, u, REACH);
    places::arrived(s, net, world, clock);
}

/// A player has come to a hall's binding stone: refused, told it is short of the cost, or the
/// allegiance bound and the use carried on as the stone's own.
pub fn at_stone<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    u: Use,
    o: &yalain_content::ContentObject,
    clock: Clock,
) {
    let Some(hall) = stone(s, u.item).cloned() else {
        return;
    };
    let distance = places::reach(&u.from, o.cell, o.origin).unwrap_or(0.0);
    let Some(p) = s.players.get(&world) else {
        return;
    };
    let me = p.character;
    if !crate::allegiance::ensure(s) {
        return;
    }
    if s.allegiances.monarch(me) != Some(me) {
        let no = places::use_refused(
            &u,
            0,
            Some(places::USE_FLAGS_CANNOT_BE_USED),
            distance,
            crate::item_use::refusal::MONARCH_ONLY,
        );
        progress::private(net, world, &no);
        return;
    }
    if let Some(why) = places::quest_keeps_out(s, world, o, clock) {
        let no = places::use_refused(&u, 0, Some(places::USE_FLAGS_PERSON), distance, why);
        progress::private(net, world, &no);
        return;
    }
    let done = places::use_done(&u, me, places::USE_FLAGS_PERSON, false, distance);
    progress::private(net, world, &done);
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let money = crate::store::money(&p.qualities);
    if money < hall.cost {
        let told = crate::chat::show_text(SHOWN_AS, progress::text(TOO_POOR));
        progress::private(net, world, &told);
        return;
    }
    if hall.cost > 0 {
        let left = (money - hall.cost).cast_signed();
        quals::set_int(&mut p.qualities, crate::store::MONEY, left);
        progress::send_int(net, world, crate::store::MONEY, left);
    }
    s.allegiances.charter_mut(me).hall = Some(BoundHall {
        name: hall.name.clone(),
        place: yalain_store::StoredPosition {
            cell: hall.place.cell,
            origin: hall.place.origin,
            rotation: hall.place.rotation,
        },
    });
    crate::allegiance::keep(s);
    crate::world::keep_player(s, world);
    let told = crate::chat::show_text(SHOWN_AS, progress::text(BOUND));
    progress::private(net, world, &told);
    let named = s
        .catalog
        .get(o.template)
        .map(|t| crate::quests::named_for_use(s, &t.props))
        .unwrap_or_default();
    for e in named {
        crate::quests::moves(s, net, world, e, false, clock);
    }
    crate::store::talk(s, net, world, u.item, o, clock);
}
