//! Portals a player summons: Summon Lifestone opens one to the lifestone the summoner is bound to,
//! Summon Portal one to where the last portal it used that may be recalled to took it (the place
//! Portal Recall returns to). Anyone may use it while it stands. An item's use may make one too.
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
//!
//! **Portals an item's use makes.** A world's [`yalain_content::DeathSpawn`] may make a portal of a
//! template that names a destination: an item whose use names its effect makes it before its user,
//! at the row's offset (2017: the one such use recorded, a master vault's flawed gem, its portal
//! 2.0 m straight ahead at the user's height, made in the use's answer's tick). It leads where its
//! template's destination names: the place the world names for the template, else the place of the
//! destination's name; a template whose place the world lacks makes nothing, and the item is kept.
//! It stands as long as the row says, else until the server stops (2017: of the 13 such portals the
//! recordings saw, 12 still stood when their session ended, 128 to 728 s after they were first seen,
//! and one went 300 s after).
//!
//! Anyone may use it while it stands, as a placed portal of its template: refused at once, wherever
//! the user stands, below the template's least level, of another kingdom than the row's (or, where
//! the row names none, than the one the template requires), no hero when the template is for
//! heroes alone, or without the quest the template needs. What the template names for its use comes
//! on its user as it comes out, and the quests its template's arrival moves move, as a placed
//! portal's do; its portal flags say whether its user may recall to where it led (2017: the gem's
//! maker came out through its portal 9.1 s after it was made).

use yalain_net::{ConnId, ServerNet};
use yalain_store::{Store, StoredPosition};

use crate::places::{self, Use};
use crate::things::{Kind, Thing};
use crate::{presence, progress, Clock, Server, Timer};

/// The summoned portal's template.
pub const TEMPLATE: u32 = 0x4700_0604;

/// The script package every portal derives from, shards and vaults among them.
pub const PACKAGE: &str = "Portal";

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

/// Where a portal of this template leads in this world, if its template names a destination the
/// world holds: the place named for the template, else the place of the destination's name.
#[must_use]
pub fn leads_to<S: Store>(s: &Server<S>, template: u32) -> Option<StoredPosition> {
    let name = s.catalog.get(template)?.props.text("PortalDestination")?;
    let p = s
        .places_named
        .get(&template)
        .or_else(|| s.places_by_name.get(&name.to_ascii_lowercase()))?;
    Some(StoredPosition {
        cell: p.cell,
        origin: p.origin,
        rotation: p.rotation,
    })
}

/// Does a template name a destination, as a portal's does?
#[must_use]
pub fn is_portal(props: &yalain_content::Props) -> bool {
    props.text("PortalDestination").is_some()
}

/// A portal of a template a world's row makes stands at `at`, leading to the row's place, else
/// where its template names, for `kingdom` alone when the row names one, else for the kingdom its
/// template requires, for `lasts` seconds (until the server stops when it gives none), and named as
/// the row names it. Returns its id; none is made where the world holds no place it leads to.
pub fn made<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    (template, to): (u32, Option<StoredPosition>),
    at: StoredPosition,
    (kingdom, lasts): (Option<u32>, Option<f32>),
    name: Option<&yalain_content::Words>,
    clock: Clock,
) -> Option<u64> {
    use yalain_content::portal::{RECALL, SUMMON};
    let to = match to {
        Some(to) => to,
        None => leads_to(s, template)?,
    };
    let of = s.catalog.get(template).cloned()?;
    let id = s.things.new_id();
    let mut t = Thing::of(id, &of, Kind::Portal);
    t.at = Some(at);
    t.destination = Some(to);
    t.fresh = true;
    let flags = of.props.word("PortalFlags").unwrap_or(0);
    t.facts.portal = flags & (RECALL | SUMMON);
    t.facts.hero = of.props.flag("Usage_HeroOnly");
    t.facts.kingdom = kingdom.or_else(|| required_kingdom(&of.props));
    t.facts.min_level = t.facts.min_level.or_else(|| {
        of.props
            .int("Usage_MinLevel")
            .and_then(|l| i32::try_from(l).ok())
    });
    if let Some(name) = name {
        t.name_as(name);
    }
    s.things.insert(t);
    presence::thing_appears(s, net, id);
    if let Some(seconds) = lasts {
        s.after(clock, f64::from(seconds), Timer::Vanish { id });
    }
    Some(id)
}

/// The kingdom a template's use requires, if it requires one.
#[must_use]
pub fn required_kingdom(props: &yalain_content::Props) -> Option<u32> {
    props.word("Usage_RequiredFaction").filter(|k| *k != 0)
}

/// Why a player may not use a portal standing in the world, if it may not, as a placed portal of
/// its template would refuse: below its least level, of another kingdom than the one it is for, no
/// hero when it is for heroes alone, or the quest its template needs not where it needs it.
#[must_use]
pub fn keeps_out<S: Store>(s: &Server<S>, world: ConnId, t: &Thing, clock: Clock) -> Option<u32> {
    use crate::item_use::refusal;
    let p = s.players.get(&world)?;
    if t.facts
        .min_level
        .is_some_and(|least| crate::examine::level_of(&p.qualities) < least)
    {
        return Some(refusal::LEVEL_TOO_LOW);
    }
    if t.facts
        .kingdom
        .is_some_and(|k| crate::kingdoms::of(&p.qualities) != Some(k))
    {
        return Some(refusal::WRONG_KINGDOM);
    }
    if t.facts.hero && !crate::focus::is_hero(&p.qualities) {
        return Some(refusal::HERO_ONLY);
    }
    let props = &s.catalog.get(t.template)?.props;
    crate::item_use::quest_gate(s, world, props, clock)
}

/// A use of a portal standing in the world refused, if it keeps the player out: answered at once,
/// wherever the player stands, as a placed portal's refusal is. Returns whether it was.
fn refused<S: Store>(
    s: &Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    u: &Use,
    clock: Clock,
) -> bool {
    let Some(t) = portal(s, u.item) else {
        return false;
    };
    let Some(why) = keeps_out(s, world, t, clock) else {
        return false;
    };
    let distance =
        t.at.and_then(|at| presence::distance(&u.from, &at))
            .unwrap_or(0.0);
    let no = places::use_refused(u, 0, Some(places::USE_FLAGS_QUEST_KEEPS_OUT), distance, why);
    progress::private(net, world, &no);
    true
}

/// A player asks to use a summoned portal: carried out in reach, else walked to it first.
pub fn used<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, u: Use, clock: Clock) {
    let Some(at) = portal(s, u.item).and_then(|t| t.at) else {
        return;
    };
    if refused(s, net, world, &u, clock) {
        return;
    }
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
