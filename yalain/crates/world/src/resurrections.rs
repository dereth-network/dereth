//! Resurrection skills: a player brings another back where it fell, if that one agrees.
//!
//! **The skill** is aimed at the corpse of a player still dead of that death and in the world, within
//! the skill's range. Its use ends by offering the dead player a return: who offers, at what share of
//! focus and with what look, the share being the skill's effect at its user's rating (2017: 6 of 6
//! offers, at ratings of 45 and 30). A dead player with an offer standing can't be given another.
//!
//! **Answered** by the dead player's client: its request to be resurrected takes the offer up; its
//! clearing of the request drops it, and it stays dead with its usual choice. An offer nobody answers
//! does nothing: a player left dead goes back to its lifestone in time as it always does, and an
//! offer ends with a player leaving the world.
//!
//! **Taken up**, the player gets up in the same tick where it fell, with no trip and no protection
//! (2017: 6 of 6): what its death ended taken off, the same 5 vitae, three quarters of health and
//! vigor as a lifestone's return, and focus of no more than ten plus the share of its greatest (2017:
//! 20 and 17 at shares of 0.094 and 0.066; 4 more were told nothing). It may move a moment later,
//! and its auras pulse as after any return. Its corpse stays and rots in its own time (2017: 60 s
//! after it was made, whatever was offered). Nothing moves between the corpse and the pack: a player's
//! corpse holds nothing, and neither who offers nor who is raised is given or loses an item.

use auberean_script::args::Arg;
use auberean_script::native_object::{NativeObject, TextInfo};
use yalain_net::{ConnId, ServerNet};
use yalain_store::Store;

use crate::effect_defs::{Kind, Scale};
use crate::graph::{tag, Builder, Slot};
use crate::skills::Usage;
use crate::{progress, session, Clock, Server, Timer};

/// The calls of an offer: the server's telling a dead player of one, the client's dropping it.
pub mod function {
    pub const OFFERED: u32 = 0x8207_0044;
    pub const DECLINE: u32 = 0x8207_003F;
}

/// The script package of the request an offer tells of: who, their name, the share, the look.
pub const REQUEST: u16 = 0x1B51;

/// How long after it gets up a player raised may move, in seconds (2017: 1.47 to 3.05, 6 of 6).
pub const MOVES_AFTER: f64 = 2.5;

/// An offer standing on a dead player.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Offer {
    /// Who offered it.
    pub by: u64,
    /// The share of its greatest focus the player keeps above a lifestone's return.
    pub focus_share: f32,
    /// The look of its getting up.
    pub fx: u32,
}

/// What a skill's resurrecting effect says, when it has one.
fn resurrecting<S: Store>(s: &Server<S>, u: &Usage) -> Option<(Scale, u32)> {
    u.target_effects
        .iter()
        .find_map(|e| match s.effect_table.get(*e).map(|d| &d.kind) {
            Some(Kind::Resurrect { focus, fx }) => Some((focus.clone(), *fx)),
            _ => None,
        })
}

/// Is this skill one that brings a dead player back?
#[must_use]
pub fn raises<S: Store>(s: &Server<S>, u: &Usage) -> bool {
    resurrecting(s, u).is_some()
}

/// The dead player this corpse is the corpse of, when it may be offered a return: in the world, out
/// of portal space, still dead of this death, with no offer standing.
#[must_use]
pub fn may_be_offered<S: Store>(s: &Server<S>, corpse: u64) -> Option<ConnId> {
    let of = s.things.get(corpse)?.corpse.as_ref()?.originator;
    s.players
        .iter()
        .find(|(_, p)| {
            p.character == of
                && p.dying.is_some_and(|d| d.corpse == Some(corpse))
                && p.trip.is_none()
                && p.offer.is_none()
        })
        .map(|(c, _)| *c)
}

/// The use of a resurrecting skill has reached the corpse: its owner is offered a return, if it can
/// still be. Nothing happens to the corpse, or to anyone until the offer is taken up.
pub fn offers<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    corpse: u64,
    u: &Usage,
    rating: f32,
) {
    let Some((focus, fx)) = resurrecting(s, u) else {
        return;
    };
    let Some(by) = s.players.get(&world) else {
        return;
    };
    let (me, name) = (by.character, crate::player::world_name(&by.name, by.sex));
    let Some(to) = may_be_offered(s, corpse) else {
        return;
    };
    let focus_share = focus.at(rating).unwrap_or(0.0);
    if let Some(p) = s.players.get_mut(&to) {
        p.offer = Some(Offer {
            by: me,
            focus_share,
            fx,
        });
    }
    progress::private(net, to, &offered(me, &name, focus_share, fx));
}

/// The call telling a dead player of an offer.
#[must_use]
pub fn offered(by: u64, name: &str, focus_share: f32, fx: u32) -> auberean_protocol::ServerMessage {
    let mut b = Builder::new();
    let text = b.next_id() + 1;
    b.object(
        REQUEST,
        &[
            // A value a native wrote (2017: 6 of 6, this tag).
            Slot::Tagged(tag::MIXED_LONG, (by & 0xFFFF_FFFF) as u32),
            Slot::Tagged(tag::HIGH, (by >> 32) as u32),
            Slot::Ref(Some(text)),
            Slot::Tagged(tag::FLOAT, focus_share.to_bits()),
            Slot::Int(fx),
        ],
    );
    b.native(
        session::native::TEXT,
        NativeObject::TextInfo(TextInfo {
            has_literal: 1,
            literal: Some(name.encode_utf16().collect()),
            ..TextInfo::default()
        }),
    );
    session::call(function::OFFERED, &[Arg::Object(b.graph())]).expect("an offer packs")
}

/// A dead player's client drops an offer: it stays dead, with its usual choice.
pub fn declined<S: Store>(s: &mut Server<S>, world: ConnId) {
    if let Some(p) = s.players.get_mut(&world) {
        p.offer = None;
    }
}

/// A dead player takes up an offer: it gets up where it fell, in this tick.
pub fn rises<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    offer: Offer,
    clock: Clock,
) {
    let stop = crate::effects::resurrected(s, net, world);
    crate::death::stands_at_peace(s, net, world, &stop);
    crate::death::returns_keeping(s, net, world, offer.focus_share);
    crate::death::returned_with(s, net, world, Some(offer.fx));
    s.after(clock, MOVES_AFTER, Timer::RaisedMoves { world });
    s.after(
        clock,
        crate::effects::AURAS_PULSE_AFTER_RETURN,
        Timer::AurasPulseAgain { world },
    );
    crate::world::keep_player(s, world);
}

/// A raised player may move, unless it has died again by now.
pub fn raised_moves<S: Store>(s: &Server<S>, net: &mut ServerNet, world: ConnId) {
    if s.players.get(&world).is_some_and(|p| p.dying.is_none()) {
        progress::private(net, world, &session::movement_frozen(false));
    }
}
