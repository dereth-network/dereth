//! Escorts: a creature a quest step sets to follow its player, whose death fails the quest.
//!
//! **Made** as the step runs, after its update, while its quest is under way at a phase the escort
//! is wanted at: its player is told of it as of a new pet (of the quest companions' class, with no
//! time left in the world), and it appears two meters in front of its player, naming the player as
//! its summoner. It is a pet of its kind (the world's `[[pet]]` row for its template), at its
//! template's own level: it follows and fights as that kind does, and its player's pet orders
//! reach it. A player has one escort of a quest at a time. A step naming an escort its world has
//! no `[[pet]]` row for makes nothing, and the operator's log says why.
//!
//! **Used by its player**, it is a person used: the use is answered so, and runs the steps a use
//! of its template triggers (petting a lost pet led home). Anyone else's use of it is a use of a
//! creature.
//!
//! **Its end.** Its death (its player's death takes it, as it takes a pet) fails its quest; so does
//! its player coming out of portal space somewhere else, where the escort says so, and its player
//! found farther from it than it allows as it plans its way. Otherwise it comes out of portal space
//! beside its player, as a pet does; so it always does from the trip of the step that made it. As
//! its quest reaches the phase it lasts until, or is done, failed, cancelled or taken off, it is
//! taken off its player's list and leaves the world with nothing failed, in that tick (2017: the
//! escort petted home, its list, its leaving and its destruction in the completion's tick). A
//! player leaving the world takes it out of the world, and on entering again while its quest is
//! still at a phase it is wanted at, it is made again beside its player.
//!
//! 2017 recorded one escort, a lost Lasher led home: made as its den was used, it was its player's
//! pet in every way the wire shows (told as one, naming its summoner, at 1.75 times its speed),
//! fought what fought its player, kept within about 3 m of its player standing still and fell up to
//! 80 m behind one running, was never put beside its player, and went as it was petted. Its death,
//! a portal, a recall and its player's leaving the world were not recorded: what they do is this
//! server's.

use std::collections::BTreeMap;

use yalain_content::escorts::{phase_after, Escort};
use yalain_content::quests::Step;
use yalain_net::{ConnId, ServerNet};
use yalain_store::Store;

use crate::{Clock, Server};

/// The class of pet escorts are told as (2017: the escort led home and a quest's companion of
/// 2005, both this class, which no summoning skill names).
pub const CLASS: u32 = 0x0400_0000;

/// An escort following its player.
#[derive(Debug, Clone, PartialEq)]
pub struct Following {
    /// Its player (a character id).
    pub owner: u64,
    pub quest: u32,
    /// The phase the step that made it left its quest at.
    pub made_at: u32,
    pub escort: Escort,
    /// Made by a step that sends its player somewhere: that trip is its own, and fails nothing.
    pub rides: bool,
}

/// Every escort in the world, by its own id.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Escorts {
    pub by_pet: BTreeMap<u64, Following>,
}

impl Escorts {
    /// The escort of a player's quest, if one follows it.
    #[must_use]
    pub fn of(&self, owner: u64, quest: u32) -> Option<u64> {
        self.by_pet
            .iter()
            .find(|(_, f)| f.owner == owner && f.quest == quest)
            .map(|(id, _)| *id)
    }

    /// Is this an escort following this player?
    #[must_use]
    pub fn follows(&self, id: u64, owner: u64) -> bool {
        self.by_pet.get(&id).is_some_and(|f| f.owner == owner)
    }
}

fn character_of<S: Store>(s: &Server<S>, world: ConnId) -> Option<u64> {
    s.players.get(&world).map(|p| p.character)
}

fn conn_of<S: Store>(s: &Server<S>, character: u64) -> Option<ConnId> {
    s.players
        .iter()
        .find(|(_, p)| p.character == character)
        .map(|(c, _)| *c)
}

/// The phase a player's quest is at (0: not under way).
fn phase<S: Store>(s: &Server<S>, world: ConnId, quest: u32) -> u32 {
    s.players
        .get(&world)
        .map_or(0, |p| crate::quests::phase_of(&p.play.quests, quest))
}

/// A step has run: the escort it names follows its player, where its quest now wants one.
pub fn step_ran<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    step: &Step,
    clock: Clock,
) {
    let Some(escort) = s.content.escorts.of(step).cloned() else {
        return;
    };
    let Some(made_at) = phase_after(step) else {
        return;
    };
    if escort.wanted_at(made_at, phase(s, world, step.quest)) {
        make(s, net, world, step.quest, made_at, &escort, clock);
        if step.to.is_some() {
            if let Some(f) = character_of(s, world)
                .and_then(|o| s.escorts.of(o, step.quest))
                .and_then(|id| s.escorts.by_pet.get_mut(&id))
            {
                f.rides = true;
            }
        }
    }
}

/// A player enters the world: each quest under way at a phase an escort is wanted at has it again.
pub fn at_entry<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, clock: Clock) {
    let held: Vec<(u32, u32)> = s.players.get(&world).map_or_else(Vec::new, |p| {
        p.play
            .quests
            .iter()
            .filter(|q| q.status == crate::quests::status::UNDERWAY)
            .map(|q| (q.quest, q.phase))
            .collect()
    });
    for (quest, at) in held {
        let wanted: Vec<(u32, Escort)> = s
            .content
            .escorts
            .wanted(quest, at)
            .filter_map(|(st, e)| Some((phase_after(st)?, e.clone())))
            .collect();
        if let Some((made_at, escort)) = wanted.into_iter().next() {
            make(s, net, world, quest, made_at, &escort, clock);
        }
    }
}

/// Make a player's escort of a quest, unless one follows it already.
fn make<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    quest: u32,
    made_at: u32,
    escort: &Escort,
    clock: Clock,
) {
    let Some(owner) = character_of(s, world) else {
        return;
    };
    if s.escorts.of(owner, quest).is_some() {
        return;
    }
    let Some(kind) = s.pets.kind_of(escort.template).cloned() else {
        tracing::warn!(
            quest = format_args!("{quest:08X}"),
            template = format_args!("{:08X}", escort.template),
            "an escort names a creature this world has no [[pet]] row for: nothing follows"
        );
        s.unhandled.note(format!(
            "an escort {:08X} with no [[pet]] row",
            escort.template
        ));
        return;
    };
    let level = s
        .catalog
        .get(escort.template)
        .and_then(|t| t.props.int(yalain_content::props::creature_property::LEVEL))
        .filter(|l| *l > 0)
        .map_or(1.0, |l| f32::from(u16::try_from(l).unwrap_or(u16::MAX)));
    let Some(id) = crate::pets::escort(s, net, world, &kind, (CLASS, escort.icon), level, clock)
    else {
        return;
    };
    s.escorts.by_pet.insert(
        id,
        Following {
            owner,
            quest,
            made_at,
            escort: escort.clone(),
            rides: false,
        },
    );
    crate::pets::appear(s, net, id);
}

/// A player's quest has moved, or is no longer under way: its escorts it no longer wants go, taken
/// off the player's list, with nothing failed.
pub fn quest_moved<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, quest: u32) {
    let Some(owner) = character_of(s, world) else {
        return;
    };
    let at = phase(s, world, quest);
    let going: Vec<u64> = s
        .escorts
        .by_pet
        .iter()
        .filter(|(_, f)| f.owner == owner && f.quest == quest)
        .filter(|(_, f)| !f.escort.wanted_at(f.made_at, at))
        .map(|(id, _)| *id)
        .collect();
    for id in going {
        s.escorts.by_pet.remove(&id);
        crate::pets::expires(s, net, id);
    }
}

/// A pet has died: if it was an escort, its quest fails.
pub fn died<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64, clock: Clock) {
    let Some(f) = s.escorts.by_pet.remove(&id) else {
        return;
    };
    if let Some(world) = conn_of(s, f.owner) {
        crate::quests::fail(s, net, world, f.quest, clock);
    }
}

/// Fail the quest of a player's escort, which then goes.
fn fails<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64, clock: Clock) {
    let Some(f) = s.escorts.by_pet.get(&id).cloned() else {
        return;
    };
    if let Some(world) = conn_of(s, f.owner) {
        crate::quests::fail(s, net, world, f.quest, clock);
    }
    // A quest already over leaves nothing to fail: the escort goes all the same.
    if s.escorts.by_pet.remove(&id).is_some() {
        crate::pets::expires(s, net, id);
    }
}

/// A player has come out of portal space somewhere else: each of its escorts that says so fails
/// its quest, and goes before its pets are put beside it; but for one made by the step that sent
/// it, whose trip it was: that one comes along.
pub fn owner_arrives<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    clock: Clock,
) {
    let Some(owner) = character_of(s, world) else {
        return;
    };
    let mut failing = Vec::new();
    for (id, f) in &mut s.escorts.by_pet {
        if f.owner != owner || std::mem::take(&mut f.rides) {
            continue;
        }
        if f.escort.portal_fails {
            failing.push(*id);
        }
    }
    for id in failing {
        fails(s, net, id, clock);
    }
}

/// An escort plans its way to its player, so far from it: farther than it allows fails its quest.
/// Returns whether it did.
pub fn too_far<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    id: u64,
    away: Option<f32>,
    clock: Clock,
) -> bool {
    let Some(far) = s.escorts.by_pet.get(&id).and_then(|f| f.escort.far) else {
        return false;
    };
    if away.is_some_and(|d| d <= far) {
        return false;
    }
    fails(s, net, id, clock);
    true
}

/// A player leaves the world, its pets with it: its escorts are no longer followed (each is made
/// again as it enters, where its quest still wants it).
pub fn owner_leaves<S: Store>(s: &mut Server<S>, owner: u64) {
    s.escorts.by_pet.retain(|_, f| f.owner != owner);
}
