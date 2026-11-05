//! Kingdoms, and the kind of player conflict each place allows.
//!
//! **A player's kingdom.** A character starts in none: the character it is made from names none,
//! and its description carries none (2017: 2 of 2 new characters). It joins one at a faction
//! shrine ([`crate::usable::shrines`]): the Shadow, the Order of Dereth, the Dominion, or the
//! neutral kingdom. The kingdom is kept in the player's description: its faction (a character in
//! none has no faction at all) and its physics type's kingdom bit (one bit per kingdom; the
//! neutral bit for a character in none). Its profile names it (2017: 9,114 of 9,114 player
//! profiles), and so does the profile of a plant it planted; its chat channel is its kingdom's
//! (3,063 of 3,068 channel lists), whose lines reach its kingdom's players alone.
//!
//! **Where players may fight.** Each landblock allows a kind of conflict ([`Conflict`]): peace,
//! controlled (players of different kingdoms, neutral players kept out), kingdom against kingdom,
//! or free for all; the world database's areas say which, every other landblock being a peace
//! area (2017: the status a client held matched one kind per landblock over 2,827 of 2,830
//! landblocks visited). The client is told the status of where it is:
//!
//! - as a character enters, in the session start (425 of 425), and a new character is told it
//!   once more right after its description (2 of 2);
//! - as the player walks into a landblock of another status (99 of 99), or arrives in one
//!   (68 of 70 portal trips and returns from death: as the arrival is through, when the client
//!   has drawn the world);
//! - never when it holds the same status already (0 of 274 calls repeated it).
//!
//! **Who passes through whom.** A player's movement ethereality says the kingdoms of players it
//! passes through: those it may not fight where it stands, by the client's own rule of who may
//! fight whom ([`may_fight`]; a player in no kingdom counts as neutral). It changes with the status
//! and the kingdom, and every client that knows the player is told, after the status (256 of 263
//! status calls).
//!
//! **Who may harm whom.** A player may hurt another only as the client's own check of a harmful act
//! allows ([`may_harm`]): never a dead or protected player, never where the target stands in
//! peace, never across two places of different statuses, anyone where all fight, else only a
//! player of another kingdom, and in a controlled area neither of them neutral. A player whose
//! permissions let kingdoms fight it anywhere (its "always true" permissions holding
//! [`permission::ANYWHERE`]) holds a peace area as kingdom against kingdom anywhere, unless it is
//! protected: such players of different kingdoms fight in peace areas (2017: every recorded fight
//! between players, 108 harmful hooks of 108, was between such players of different kingdoms in a
//! peace area), and its ethereality follows, as a protection begins and ends too (32 of 34).

use std::collections::BTreeMap;

use auberean_protocol::quality::VisualQuality;
use auberean_protocol::types::{Qualities, WeenieDesc};
use auberean_protocol::ServerMessage;
use auberean_script::args::Arg;
use yalain_content::Conflict;
use yalain_net::{ConnId, ServerNet};
use yalain_store::Store;

use crate::world::Player;
use crate::{progress, quals, session, Clock, Server, Timer};

/// The kingdoms, as a description's faction names them.
pub mod kingdom {
    /// The Shadow.
    pub const SHADOW: u32 = 1;
    /// The Order of Dereth.
    pub const ORDER: u32 = 2;
    /// The Dominion.
    pub const DOMINION: u32 = 4;
    /// The neutral kingdom.
    pub const NEUTRAL: u32 = 0x8000_0000;
    /// Every kingdom, the neutral one first.
    pub const ALL: [u32; 4] = [NEUTRAL, SHADOW, ORDER, DOMINION];
}

/// The faction shrines' script kinds, and the kingdom each one's use joins.
pub const SHRINES: [(&str, u32); 4] = [
    ("FactionShrineShadow", kingdom::SHADOW),
    ("FactionShrineOrder", kingdom::ORDER),
    ("FactionShrineDominion", kingdom::DOMINION),
    ("FactionShrineNeutral", kingdom::NEUTRAL),
];

/// The client function that tells the player the conflict status of where it is (one number).
pub const STATUS: u32 = 0x8210_0022;

/// Qualities of a player its kingdom sets (the property list's numbers): its kingdom, its
/// physics type and its movement ethereality.
pub mod quality {
    pub const KINGDOM: u32 = 2000;
    pub const PHYSICS_TYPE: u32 = 3;
    pub const ETHEREALITY: u32 = 7;
}

/// The description's numbers a kingdom changes: its faction, its physics type, its movement
/// ethereality (their places in the weenie description's numbers); and its permissions, the kinds
/// of attacker always let through and always kept out.
const FACTION_NUMBER: usize = 2;
const ALLOW_NUMBER: usize = 3;
const DENY_NUMBER: usize = 4;
const PHYSICS_NUMBER: usize = 5;
const ETHEREAL_NUMBER: usize = 7;

/// A player's physics type and movement ethereality without their kingdom bits (the physics
/// type: the player template's, less its neutral bit).
pub const PHYSICS_BASE: i32 = crate::player::PLAYER_NUMBERS[0] & !0x0400_0000;
pub const ETHEREAL_BASE: i32 = 0x0220_0010;

/// How long after its user arrives a shrine's join is made, in seconds (2017: 3.83, 1 of 1).
pub const JOIN_SECONDS: f64 = 3.83;
/// How long after the join the player's channels are sent again (2017: 0.06 s, 1 of 1).
pub const CHANNELS_SECONDS: f64 = 0.06;

/// "You join {the} %1.", the kingdom's name its variable, and the kind of text it is shown as.
pub const JOINED: (u32, u32) = (0xA213_1988, 0x2500_000F);
pub const JOINED_KINGDOM: u32 = 0xF670_1F4D;
pub const SHOWN_AS: u32 = 8;

/// The kingdom a kingdom counts as: a player in none counts as neutral.
#[must_use]
pub const fn counted(kingdom: Option<u32>) -> u32 {
    match kingdom {
        Some(k @ (kingdom::SHADOW | kingdom::ORDER | kingdom::DOMINION)) => k,
        _ => kingdom::NEUTRAL,
    }
}

/// A kingdom's bit, in a physics type and in a movement ethereality.
#[must_use]
pub const fn bit(kingdom: u32) -> i32 {
    match kingdom {
        kingdom::SHADOW | kingdom::ORDER | kingdom::DOMINION => (kingdom << 27).cast_signed(),
        _ => 0x0400_0000,
    }
}

/// Whether players of two kingdoms may fight where the conflict status is `status`, by the
/// client's own rule: never in peace; in a controlled area, players of different kingdoms neither
/// of them neutral; kingdom against kingdom (or anywhere so), players of different kingdoms; free
/// for all, anyone.
#[must_use]
pub const fn may_fight(status: u32, a: u32, b: u32) -> bool {
    match status {
        1 => false,
        2 => a != b && a != kingdom::NEUTRAL && b != kingdom::NEUTRAL,
        3 | 5 => a != b,
        _ => true,
    }
}

/// A player's permissions' bits.
pub mod permission {
    /// A player's kind, as the permissions name an attacker (a target's kind is it four bits up).
    pub const PLAYER: u32 = 1;
    /// Among the "always kept out" ones: players of its own kingdom kept from harming it, and it
    /// from harming them.
    pub const SAME_KINGDOM_TARGET: u32 = 0x2_0000;
    pub const SAME_KINGDOM_ATTACKER: u32 = 0x1_0000;
    /// Players of other kingdoms kept from harming it, and it from harming them.
    pub const OTHER_KINGDOM_TARGET: u32 = 0x8_0000;
    pub const OTHER_KINGDOM_ATTACKER: u32 = 0x4_0000;
    /// Among the "always true" ones: kingdoms may fight it anywhere.
    pub const ANYWHERE: u32 = 0xC_0000;
}

/// Why a harmful act on a player is refused (the client writes the text).
pub mod refusal {
    /// The target is dead.
    pub const DEAD: u32 = 22_000;
    /// The target is protected (a lifestone's or a recall's protection).
    pub const PROTECTED: u32 = 22_201;
    /// The target's permissions keep a player out; the attacker's keep it from harming a player.
    pub const KEPT_OUT: u32 = 21_000;
    pub const MAY_NOT_HARM: u32 = 21_020;
    /// A peace area.
    pub const PEACE: u32 = 21_080;
    /// Both of one kingdom.
    pub const SAME_KINGDOM: u32 = 21_081;
    /// A controlled area: the attacker neutral, or the target.
    pub const ATTACKER_NEUTRAL: u32 = 21_082;
    pub const TARGET_NEUTRAL: u32 = 21_083;
    /// The two stand in places of different statuses.
    pub const OTHER_STATUS: u32 = 21_087;
}

/// The status a player holds as who may fight it is reckoned: its area's, but a peace area is
/// kingdom against kingdom anywhere (5) for a player whose permissions let kingdoms fight it
/// anywhere and who is not protected.
#[must_use]
pub const fn status_with(area: u32, allow: u32, protected: bool) -> u32 {
    if area == 1 && allow & permission::ANYWHERE != 0 && !protected {
        5
    } else {
        area
    }
}

/// A player's permissions, as its qualities keep them: always let through, always kept out.
#[must_use]
pub fn permissions_of(q: &Qualities) -> (u32, u32) {
    let w = &q.weenie;
    let number = |i: usize| {
        if w.mask & WeenieDesc::NUMBER_BITS[i] != 0 {
            w.numbers[i].cast_unsigned()
        } else {
            0
        }
    };
    (number(ALLOW_NUMBER), number(DENY_NUMBER))
}

/// Set a player's permissions in its qualities (none at all: both numbers left out).
pub fn set_permissions(qualities: &mut Vec<u8>, allow: u32, deny: u32) {
    let mut q = quals::read(qualities);
    let w = &mut q.weenie;
    for (i, v) in [(ALLOW_NUMBER, allow), (DENY_NUMBER, deny)] {
        if v == 0 {
            w.mask &= !WeenieDesc::NUMBER_BITS[i];
            w.numbers[i] = 0;
        } else {
            w.mask |= WeenieDesc::NUMBER_BITS[i];
            w.numbers[i] = v.cast_signed();
        }
    }
    q.header |= Qualities::WEENIE;
    *qualities = quals::write(&q);
}

/// The status a player holds as who may fight it is reckoned (see [`status_with`]).
#[must_use]
pub fn status_of(p: &Player) -> u32 {
    let (allow, _) = permissions_of(&quals::read(&p.qualities));
    status_with(p.conflict, allow, p.protected.is_some())
}

/// A player as who may harm whom is reckoned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fighter {
    pub id: u64,
    /// Its kingdom as it counts (a player in none: the neutral one).
    pub kingdom: u32,
    /// The status it holds ([`status_with`]).
    pub status: u32,
    pub allow: u32,
    pub deny: u32,
    pub dead: bool,
    pub protected: bool,
}

/// A player in the world, as who may harm whom is reckoned.
#[must_use]
pub fn fighter(p: &Player) -> Fighter {
    let q = quals::read(&p.qualities);
    let (allow, deny) = permissions_of(&q);
    Fighter {
        id: p.character,
        kingdom: counted(of_qualities(&q)),
        status: status_with(p.conflict, allow, p.protected.is_some()),
        allow,
        deny,
        dead: p.dying.is_some() || crate::combat::is_dead(p),
        protected: p.protected.is_some(),
    }
}

/// May one player harm another (by the client's own check of a harmful act, between two players):
/// itself always; never a dead target, nor a protected one; never past either one's permissions
/// that keep players or kingdoms out (unless the other's let it through); never where the target
/// stands in peace, nor across places of different statuses; anyone where all fight; else only a
/// player of another kingdom, and in a controlled area neither of them neutral. The refusal says
/// why.
///
/// # Errors
/// The reason the act is refused ([`refusal`]).
pub fn may_harm(target: &Fighter, attacker: &Fighter) -> Result<(), u32> {
    use permission as pm;
    if target.id == attacker.id {
        return Ok(());
    }
    if target.dead {
        return Err(refusal::DEAD);
    }
    if target.protected {
        return Err(refusal::PROTECTED);
    }
    if pm::PLAYER & target.deny != 0 {
        return Err(refusal::KEPT_OUT);
    }
    if (pm::PLAYER << 4) & attacker.deny != 0 {
        return Err(refusal::MAY_NOT_HARM);
    }
    let same = target.kingdom == attacker.kingdom;
    if same {
        if target.deny & pm::SAME_KINGDOM_TARGET != 0
            || attacker.deny & pm::SAME_KINGDOM_ATTACKER != 0
        {
            return Err(refusal::SAME_KINGDOM);
        }
    } else if target.deny & pm::OTHER_KINGDOM_TARGET != 0
        || attacker.deny & pm::OTHER_KINGDOM_ATTACKER != 0
    {
        return Err(refusal::OTHER_STATUS);
    }
    if pm::PLAYER & target.allow != 0 || (pm::PLAYER << 4) & attacker.allow != 0 {
        return Ok(());
    }
    if target.status == 1 {
        return Err(refusal::PEACE);
    }
    if target.status != attacker.status {
        return Err(refusal::OTHER_STATUS);
    }
    if target.status == 4 {
        return Ok(());
    }
    if same {
        return Err(refusal::SAME_KINGDOM);
    }
    if target.status == 2 {
        if attacker.kingdom == kingdom::NEUTRAL {
            return Err(refusal::ATTACKER_NEUTRAL);
        }
        if target.kingdom == kingdom::NEUTRAL {
            return Err(refusal::TARGET_NEUTRAL);
        }
    }
    Ok(())
}

/// A player's physics type in a kingdom (none: the neutral bit).
#[must_use]
pub const fn physics_type(kingdom: Option<u32>) -> i32 {
    PHYSICS_BASE | bit(counted(kingdom))
}

/// A player's movement ethereality where the status is `status`: the kingdoms of the players it
/// may not fight there, which it passes through.
#[must_use]
pub fn ethereality(status: u32, kingdom: Option<u32>) -> i32 {
    let mine = counted(kingdom);
    kingdom::ALL
        .into_iter()
        .filter(|k| !may_fight(status, mine, *k))
        .fold(ETHEREAL_BASE, |m, k| m | bit(k))
}

/// The kingdom of a player's qualities (none for a character that never joined one).
#[must_use]
pub fn of_qualities(q: &Qualities) -> Option<u32> {
    let w = &q.weenie;
    (w.mask & WeenieDesc::NUMBER_BITS[FACTION_NUMBER] != 0)
        .then(|| w.numbers[FACTION_NUMBER].cast_unsigned())
}

/// The kingdom of a player's encoded qualities.
#[must_use]
pub fn of(qualities: &[u8]) -> Option<u32> {
    of_qualities(&quals::read(qualities))
}

/// A player's movement ethereality, as its encoded qualities keep it.
#[must_use]
pub fn kept_ethereality(qualities: &[u8]) -> i32 {
    quals::read(qualities).weenie.numbers[ETHEREAL_NUMBER]
}

/// Set a player's kingdom in its qualities: its faction and its physics type.
pub fn set_kingdom(qualities: &mut Vec<u8>, k: u32) {
    let mut q = quals::read(qualities);
    let w = &mut q.weenie;
    w.mask |= WeenieDesc::NUMBER_BITS[FACTION_NUMBER] | WeenieDesc::NUMBER_BITS[PHYSICS_NUMBER];
    w.numbers[FACTION_NUMBER] = k.cast_signed();
    w.numbers[PHYSICS_NUMBER] = physics_type(Some(k));
    q.header |= Qualities::WEENIE;
    *qualities = quals::write(&q);
}

/// Set a player's movement ethereality in its qualities.
pub fn set_ethereality(qualities: &mut Vec<u8>, v: i32) {
    let mut q = quals::read(qualities);
    q.weenie.mask |= WeenieDesc::NUMBER_BITS[ETHEREAL_NUMBER];
    q.weenie.numbers[ETHEREAL_NUMBER] = v;
    q.header |= Qualities::WEENIE;
    *qualities = quals::write(&q);
}

/// The conflict status of a cell's landblock.
#[must_use]
pub fn status_at(areas: &BTreeMap<u16, Conflict>, cell: u32) -> u32 {
    #[allow(clippy::cast_possible_truncation)]
    let landblock = (cell >> 16) as u16;
    areas.get(&landblock).copied().unwrap_or_default().status()
}

/// The call that tells the player the conflict status of where it is.
#[must_use]
pub fn status_call(status: u32) -> ServerMessage {
    session::call(STATUS, &[Arg::Int(status)]).expect("a number packs")
}

/// A visual quality of the player's own object, with its next visual stamp.
fn visual(p: &mut Player, quality: u32, value: i32) -> ServerMessage {
    p.visual = p.visual.wrapping_add(1);
    ServerMessage::UpdateIntVisual(VisualQuality {
        object: auberean_protocol::types::ObjectStamp {
            other_stamp: p.visual,
            ..p.stamp()
        },
        quality,
        value,
    })
}

/// The player's movement ethereality brought in line with its kingdom and the status it holds:
/// when it changes, every client that knows the player is told.
pub fn settle_ethereality<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId) {
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    // An admin's override stands until the admin lifts it.
    if p.admin.ethereality != crate::admin_calls::Ethereality::Normal {
        return;
    }
    let want = ethereality(status_of(p), of(&p.qualities));
    tell_ethereality(s, net, world, want);
}

/// Set a player's movement ethereality: when it changes, every client that knows the player is
/// told.
pub fn tell_ethereality<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    want: i32,
) {
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    if kept_ethereality(&p.qualities) == want {
        return;
    }
    set_ethereality(&mut p.qualities, want);
    let m = visual(p, quality::ETHEREALITY, want);
    let me = p.character;
    crate::places::to_all_knowing(s, net, me, &m);
}

/// A character's qualities as it enters where the status is `status`: a character that has been
/// in the world before keeps the ethereality of where it stands; a new one keeps its template's
/// until its object is made, and is then told.
pub fn entering(qualities: &mut Vec<u8>, status: u32, new_character: bool) {
    if !new_character {
        let (allow, _) = permissions_of(&quals::read(qualities));
        let want = ethereality(status_with(status, allow, false), of(qualities));
        if kept_ethereality(qualities) != want {
            set_ethereality(qualities, want);
        }
    }
}

/// A new character, just described: it is told the status of where it is (its description held
/// none).
pub fn told_first(net: &mut ServerNet, world: ConnId, status: u32) {
    progress::private(net, world, &status_call(status));
}

/// The player may have come into a landblock of another status: walked in, or its arrival through
/// (the client has drawn the world). It is told the new status, and its ethereality follows.
pub fn check<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId) {
    let Some(p) = s.players.get(&world) else {
        return;
    };
    let now = status_at(&s.content.conflict, p.position.cell);
    if now == p.conflict {
        return;
    }
    if let Some(p) = s.players.get_mut(&world) {
        p.conflict = now;
    }
    progress::private(net, world, &status_call(now));
    settle_ethereality(s, net, world);
}

/// Whether a player is of a shrine's kingdom already (a player in none is of the neutral one).
#[must_use]
pub fn of_shrines_kingdom(player: Option<u32>, shrine: u32) -> bool {
    player == Some(shrine) || (player.is_none() && shrine == kingdom::NEUTRAL)
}

/// A player arrived at a faction shrine of a kingdom it is not of: it joins it, a little later.
pub fn join_after(s: &mut Server<impl Store>, world: ConnId, k: u32, clock: Clock) {
    s.after(
        clock,
        JOIN_SECONDS,
        Timer::KingdomJoin { world, kingdom: k },
    );
}

/// A player joins a kingdom: its kingdom and physics type change (every client that knows it is
/// told), it is told it joined, its ethereality follows, and its channels are sent again a moment
/// later with its kingdom's.
pub fn join<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, k: u32, clock: Clock) {
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    set_kingdom(&mut p.qualities, k);
    let kingdom_now = visual(p, quality::KINGDOM, k.cast_signed());
    let physics_now = visual(p, quality::PHYSICS_TYPE, physics_type(Some(k)));
    let me = p.character;
    crate::places::to_all_knowing(s, net, me, &kingdom_now);
    crate::places::to_all_knowing(s, net, me, &physics_now);
    let name = crate::profile::string(crate::profile::line::KINGDOMS, crate::profile::kingdom(k));
    progress::private(
        net,
        world,
        &crate::chat::show_text(
            SHOWN_AS,
            progress::text_with(JOINED, vec![progress::nested(JOINED_KINGDOM, name)]),
        ),
    );
    settle_ethereality(s, net, world);
    s.after(clock, CHANNELS_SECONDS, Timer::KingdomChannels { world });
}

/// The chat room of a kingdom's channel (a player in none: the neutral kingdom's).
#[must_use]
pub const fn room(kingdom: Option<u32>) -> u32 {
    match counted(kingdom) {
        kingdom::SHADOW => 3,
        kingdom::ORDER => 4,
        kingdom::DOMINION => 5,
        _ => 6,
    }
}

/// The players a kingdom's room reaches: those of its kingdom. `None` for a room no kingdom's.
pub fn room_members<S: Store>(s: &Server<S>, r: u32) -> Option<Vec<u64>> {
    if !(3..=6).contains(&r) {
        return None;
    }
    Some(
        s.players
            .values()
            .filter(|p| room(of(&p.qualities)) == r)
            .map(|p| p.character)
            .collect(),
    )
}
