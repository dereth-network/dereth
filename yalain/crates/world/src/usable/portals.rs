//! Portals a player summons: Summon Lifestone opens one to the lifestone the summoner is bound to,
//! Summon Portal one to where the summoner's last portal took it. Anyone may use it while it stands.
//!
//! **Summoning.** Both skills are used as the lifestone recall is: they power up, then their story
//! and their vigor (half of it, as their definitions say). A little after the story (2017: a median
//! 0.69 s over 1,377 portals) the summoner is told "You summon a portal to your lifestone." (or
//! "... to your last used portal."), and the portal appears to everyone who sees its place, 3 m
//! straight ahead of the summoner at its height, facing its way (777 of 777 at 3.00 m; its heading
//! within a degree of the summoner's 762 of 774). The use ends 0.90 s after the text. A summoner
//! with no place of the kind opens no portal: its use ends as it would.
//!
//! **Its life.** It leaves the world 30 s after it appears, used or not (2017: 1,210 of 1,224
//! within 29.8-30.2 s).
//!
//! **Using it.** Anyone may use it (2017: 120 uses answered: the summoner's own 68, its fellows'
//! 46, other players' 4): walked to within a portal's reach, then through portal space to the
//! summoner's place as through any portal, answered with the teleported flags. It is not walked
//! into.

use yalain_net::{ConnId, ServerNet};
use yalain_store::{Store, StoredPosition};

use crate::places::{self, Use};
use crate::things::{Kind, Thing};
use crate::{presence, progress, Clock, Server, Timer};

/// The summoned portal's template.
pub const TEMPLATE: u32 = 0x4700_0604;

/// The skills that summon one.
pub const SUMMON_LIFESTONE: u32 = 871;
pub const SUMMON_PORTAL: u32 = 872;

/// From the skill's story to the portal, from the portal to the use's end, and its life, in
/// seconds.
pub const APPEARS_AFTER: f64 = 0.69;
pub const ENDS_AFTER: f64 = 0.90;
pub const LASTS: f64 = 30.0;

/// How far ahead of its summoner it stands, in meters.
pub const AHEAD: f32 = 3.0;

/// The summoning's text: "You summon a portal to ...", the effect's name in it, and the
/// summoner's.
pub mod text {
    pub const SUMMONED: u32 = 0x912E_FF1D;
    pub const EFFECT_NAME: u32 = 0xA860_7CEC;
    pub const EFFECT: u32 = 0xB69B_365C;
    pub const PLAYER: u32 = 0x9FD3_465E;
    /// The kind of text it is shown as.
    pub const SHOWN_AS: u32 = 2;
}

/// A summoning skill's kind of place the portal leads to, and the string table of its text.
#[must_use]
pub fn summons(skill: u32) -> Option<(u32, u32)> {
    match skill {
        SUMMON_LIFESTONE => Some((places::kind::LIFESTONE, 0x2500_115C)),
        SUMMON_PORTAL => Some((places::kind::LAST_PORTAL, 0x2500_115D)),
        _ => None,
    }
}

/// A summoning skill's story is over: the text and the portal, when the summoner keeps a place of
/// the skill's kind. Returns the portal's id.
pub fn summon<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    skill: u32,
    clock: Clock,
) -> Option<u64> {
    let (kind, table) = summons(skill)?;
    let template = s.catalog.get(TEMPLATE).cloned()?;
    let p = s.players.get(&world)?;
    let to = p
        .places
        .iter()
        .find(|(k, _)| *k == kind)
        .map(|(_, at)| *at)?;
    let (at, name) = (p.position, crate::player::world_name(&p.name, p.sex));
    let said = progress::text_with(
        (text::SUMMONED, table),
        vec![
            progress::nested(text::EFFECT, progress::text((text::EFFECT_NAME, table))),
            progress::nested(text::PLAYER, crate::profile::literal(&name)),
        ],
    );
    progress::private(net, world, &crate::chat::show_text(text::SHOWN_AS, said));
    let id = s.things.new_id();
    let mut t = Thing::of(id, &template, Kind::Portal);
    t.at = Some(ahead(&at));
    t.destination = Some(to);
    t.fresh = true;
    s.things.insert(t);
    presence::thing_appears(s, net, id);
    s.after(clock, LASTS, Timer::Vanish { id });
    Some(id)
}

/// Where a summoned portal stands: [`AHEAD`] straight ahead of the summoner, at its height, facing
/// as it faces.
#[must_use]
pub fn ahead(at: &StoredPosition) -> StoredPosition {
    let mut p = crate::drops::landing(at, AHEAD);
    p.origin[2] = at.origin[2];
    p.rotation = at.rotation;
    p
}

/// Is an object a summoned portal standing in the world?
#[must_use]
pub fn portal<S: Store>(s: &Server<S>, id: u64) -> Option<&Thing> {
    s.things.get(id).filter(|t| t.kind == Kind::Portal)
}

/// A player asks to use a summoned portal: carried out in reach, else walked to it first.
pub fn used<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, u: Use, clock: Clock) {
    let Some(at) = portal(s, u.item).and_then(|t| t.at) else {
        return;
    };
    let near = s
        .players
        .get(&world)
        .and_then(|p| presence::distance(&p.position, &at))
        .is_some_and(|d| d <= places::PORTAL_REACH);
    if near {
        at_portal(s, net, world, u, clock);
    } else {
        places::walk_to(s, net, world, u, places::PORTAL_REACH);
        places::arrived(s, net, world, clock);
    }
}

/// A player in reach of a summoned portal goes through portal space to where it leads.
pub fn at_portal<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    u: Use,
    clock: Clock,
) {
    let Some(to) = portal(s, u.item).and_then(|t| t.destination) else {
        return;
    };
    if let Some(p) = s.players.get_mut(&world) {
        p.walking_to = None;
    }
    places::through_portal(s, net, world, u, to, clock);
}
