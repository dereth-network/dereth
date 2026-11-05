//! Creatures' behavior, authored from their templates' settings (the client holds no behavior to
//! port, and the server's behavior tables were never shipped to it), every rule measured from the
//! January 2017 captures:
//!
//! - **Noticing.** Once a second, at a moment of its own, a creature at rest that attacks first
//!   takes the nearest player within its perception radius (2017's engaged at a median 0.92 of
//!   the radius, half within a second of the player coming in; players in portal space, or not yet
//!   in the world, are not noticed). Creatures of the friend tables that never attack first
//!   (rabbits, the peaceful) notice only a player that strikes them; a creature friendly to players
//!   (a practice target) never fights.
//! - **Joining.** When a creature goes after a player, resting creatures of its own friend table
//!   nearby may come too, about a second later: the drudges, buruns and mimbus two times in five
//!   within 10 m, the guroks one in five, loners, the undead and groups one in ten, half that from
//!   10 to 20 m, none further (2017: 36% of the same table within 10 m against 3% of others).
//! - **Going after.** It takes its combat mode and, if it shoots this fight
//!   ([`crate::missiles`]), stands and shoots; else it runs toward the player at 6.4 m/s times its
//!   scale (2017's measured speed), facing it, its place sent to the clients that see it every
//!   world step (no movement order: 2017 sent creatures none); within reach it strikes as often as
//!   its attacks allow.
//! - **Concealing effects.** A record on a player can change what creatures of a class make of it:
//!   one that spares it doesn't take it on, come to its kin's fight with it, or turn to it, unless
//!   it has hurt the creature; one that hates it is taken on even by a creature that never attacks
//!   first ([`regard`]).
//! - **Giving up.** When the player has been beyond its perception radius for a second (2017's gave
//!   up a median 1.17 s after the player left the radius, at a distance beyond it about the same
//!   for every radius), or is gone, it takes its peaceful mode and runs home; it is not made whole
//!   (it regenerates). There is no leash at the homesick radius and no giving up by time. A target
//!   that dies is replaced by the nearest player within the radius that has hurt the creature, if
//!   any (2017: 62 of 387 took another).
//! - **Turning on another.** At each of its attacks (once for each) it may draw its target again:
//!   while players have hurt it over the last ten seconds, about three times in ten (half that
//!   once it has fought one target for fifteen seconds), among those players by the harm each did
//!   it, the nearest of them three times as likely (its present target is among them by its own
//!   harm); while none has, now and then it turns to another player within its perception, more
//!   often when that one is nearer than its target (2017: 2,492 switches over 14,686 attacks; the
//!   draw by harm and nearness fitted over 8,756 attacks, its switches by the target's share of the
//!   harm within a few of the fit's in each fifth; with no harm, 0.110 an attack toward a player a
//!   meter or more nearer, 0.065 otherwise, over 2,751 attacks). Healing, buffing or striking its
//!   kin draws nothing.
//! - **Held and slowed.** Its effects' speed scales its running and walking: a hold stops it where
//!   it stands, where it still turns and attacks what it reaches (2017: held creatures moved 96 m
//!   in 21,877 held seconds and attacked 3,060 times).
//! - **Wandering.** At rest, now and then it walks to a point within its wandering range of home
//!   (25 m, the scripts' default, for a template that names none; 2017's without a range wandered
//!   a little farther than those of 20 m), setting out with its wandering chance (0.1 for one
//!   naming none: 2017's walked 8.3 times an hour near a player, those of 0.2 13.6; one naming 0
//!   always sets out). A creature whose
//!   homesick radius is under 5 m never wanders (2017: 0 of 40 walked at rest, the training
//!   halls' among them), nor does one whose template says it does not wander, nor one that only
//!   turns. Indoors a creature at rest stays where it is (2017: 4 of 324 watched five minutes or
//!   more near a player walked, against 406 of 914 outdoors). Where the walls are known it walks
//!   only to a point it can reach, by the way it found, and
//!   only while a player is in its landblock or one beside it (beyond anything a client is told
//!   of; its walls are then never read).
//! - **Settings.** How a creature may move is its template's movement type and settings, a
//!   camp's over them, each the game's scripts' default where neither says: one that never moves
//!   does nothing at all, one that only turns faces and shoots, one tied to a master stays put
//!   without one, and one free of its home ("free range") has no distance from home that ends a
//!   fight.
//! - **Routes.** A creature whose camp names a route walks it at rest instead of wandering: to
//!   each waypoint in turn at the route's gait, resting there as long as the waypoint says, then
//!   on to the next, back and forth or round a loop (2017's patrolling guards walked straight lines
//!   between fixed points, resting at each end). A route is walked indoors and by a creature that
//!   does not wander; not by one that never moves or only turns. Where the walls are known, only
//!   while a player is near. A fight takes it off its route; when it gives up it runs back to the
//!   route's nearest waypoint and carries on from there (2017: the one patrolling guard seen to
//!   fight and give up went back to its nearest waypoint, 1 of 1).
//! - **Walls.** Where the host knows the world ([`crate::walls`]), every step is the transition's:
//!   a creature slides along walls and what stands in the world, and climbs and goes down what its
//!   body may. Held up going after a player or home, it searches for a way around and follows it
//!   (2017's chasing creatures stayed in their rooms and went around walls to the player). A walk
//!   that meets something it cannot pass ends there; a run home held up for a second ends where it
//!   is.
//! - **Out of reach.** Going after a player that nothing leads to (its search finds no way, or a
//!   closed door it cannot open stands between), it counts that player as one it could not reach,
//!   for the rest of the fight: that player's kill of it is no fair fight ([`crate::death`]; 2017
//!   said so in the kill's own words, "Your opponent couldn't even reach you!").
//! - **Doors.** A closed door on its way holds it a meter and a half short; it opens the door as
//!   a player's use does and goes on once it is open, unless its template says it does not use
//!   doors or a lock holds it ([`crate::usable::doors`]; 2017: 81 openings by creatures and pets
//!   with no player at the door, none by a summoned creature in 24 comings to a closed door).
//! - **Stopping.** A walk or a run home that ends is told with the creature's place once more,
//!   standing (2017: the last place of 93.5% of the movements that ended was told standing).
//! - **Idling.** At rest and standing, a creature of a class of behavior now and then plays one of
//!   its two idle behaviors (2017: about 8 an hour near players, the second a little more often).

use crate::admin_calls::Attackable;
use auberean_protocol::physics::ObjectPosition;
use auberean_protocol::types::ObjectStamp;
use dereth_primitives::num::math;
use yalain_net::{ConnId, ServerNet};
use yalain_store::{Store, StoredPosition};

use crate::things::Mode;
use crate::usable::doors::Passage;
use crate::walls::Asked;
use crate::{presence, Clock, Server};

/// How fast a creature of scale 1 runs, and how fast it walks when it wanders, in meters a second
/// (2017: running 6.39 times the scale; wandering walks at 0.48 of a run, 3.1 times the scale).
pub const RUN_SPEED: f32 = 6.4;
pub const WALK_SPEED: f32 = 3.1;

/// How fast a creature of scale 1 walks a route at a walk, in meters a second, and the motion it
/// is told with (2017: the patrolling guards of scale 1.15 walked 1.87 m/s, the Sentries of scale
/// 1 1.62 to 1.63 m/s, the Skyport Attendants of scale 0.925 1.5 m/s, all with forward byte 0xBF).
pub const PATROL_WALK_SPEED: f32 = 1.62;
pub const PATROL_WALKING: u32 = crate::world::motion(0x80, 0xBF, 0x7F);

/// What a concealing effect makes creatures of a class make of its holder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Regard {
    /// They don't take it on unless it hurts them.
    Spares,
    /// They take it on, even those that never attack first.
    Hates,
}

/// The concealing effects whose reach is known: each one's saved states, the class of creature it
/// is for (none: every creature; an effect for two classes has a row for each) and what they make of
/// its holder. The install says so of those with a state and its copy. The amulet of peace's
/// description names its moarsmen (2017: under it, 0 of 8 moarsmen within 20 m came at the player;
/// without it, 24 of 140 came at it first).
///
/// The last rows are read from words alone: the fanatics, the slave spawn, the Archon forces and
/// those loyal to Aerfalle and Geraine, whose classes hold those names and hardly any other. No
/// recording shows any of them held. The other effects of the kind do nothing: one names
/// no one, one names people who share their class with others, one is a guard's mark too seldom
/// recorded to read, and under the rabbits' disguise those that attack first went on doing so (5 of
/// 5).
const CONCEALING: [(u32, Option<u32>, Regard); 27] = [
    (0x6F00_1C46, Some(0x17), Regard::Spares),
    (0x6F00_1D4A, Some(0x1D), Regard::Spares),
    (0x7000_0701, Some(0x1D), Regard::Spares),
    (0x6F00_1EEB, Some(0x0E), Regard::Spares),
    (0x7000_0713, Some(0x0E), Regard::Spares),
    (0x6F00_0DC9, Some(0x3C), Regard::Spares),
    (0x7000_1D76, Some(0x3C), Regard::Spares),
    (0x6F00_1D4C, Some(0x16), Regard::Hates),
    (0x7000_0703, Some(0x16), Regard::Hates),
    (0x6F00_1EE8, Some(0x0E), Regard::Hates),
    (0x7000_0710, Some(0x0E), Regard::Hates),
    (0x6F00_1D2E, Some(0x6F), Regard::Hates),
    (0x7000_1DAE, Some(0x6F), Regard::Hates),
    (0x6F00_20BC, Some(0x06), Regard::Hates),
    (0x7000_1EB8, Some(0x06), Regard::Hates),
    (0x6F00_0D8C, None, Regard::Hates),
    (0x7000_1CB3, None, Regard::Hates),
    (0x6F00_19F1, Some(0x59), Regard::Spares),
    (0x6F00_1C48, Some(0x78), Regard::Hates),
    (0x6F00_1C77, Some(0x46), Regard::Hates),
    (0x6F00_1C77, Some(0x43), Regard::Hates),
    (0x6F00_1C78, Some(0x45), Regard::Hates),
    (0x6F00_1C78, Some(0x44), Regard::Hates),
    (0x6F00_2049, Some(0x46), Regard::Spares),
    (0x6F00_2049, Some(0x43), Regard::Spares),
    (0x6F00_204B, Some(0x45), Regard::Spares),
    (0x6F00_204B, Some(0x44), Regard::Spares),
];

/// What a creature of a class makes of a player, by the concealing effects it holds; hate over
/// sparing, where it holds both.
#[must_use]
pub fn regard(p: &crate::world::Player, class: Option<u32>) -> Option<Regard> {
    let mut found = None;
    for e in &p.play.effects {
        let rows = CONCEALING.iter().filter(|(state, ..)| *state == e.state);
        for (_, of, how) in rows {
            if of.is_none() || *of == class {
                if *how == Regard::Hates {
                    return Some(Regard::Hates);
                }
                found = Some(*how);
            }
        }
    }
    found
}

/// A creature's class of behavior.
fn class_of<S: Store>(s: &Server<S>, id: u64) -> Option<u32> {
    let t = s.things.get(id)?;
    s.catalog
        .get(t.template)?
        .props
        .word(yalain_content::props::creature_property::SUPER_CLASS)
}

/// How long a player must have been beyond a creature's perception radius for it to give up, in
/// seconds (2017: a median 1.17 s, the bulk from 0.8 to 1.6).
pub const GIVE_UP_AFTER: f64 = 1.0;

/// How often a creature at rest looks about for a player, in seconds (2017: about 14% of those
/// in reach engaged each quarter second, half by one second).
pub const LOOK_EVERY: f64 = 1.0;

/// How long after a creature goes after a player its kin come, in seconds (2017: a median 1.1 s).
pub const JOINS_AFTER: f64 = 1.1;

/// The wandering range of a creature whose settings give none above 0, in meters: the game's
/// scripts' default (2017: outdoor creatures of templates naming no range walked to a median 11.9 m
/// and p90 25.7 m from where they appeared, a little beyond those of a 20 m range, 10.8 and 22.7;
/// the one template naming 0 walked a median 50 m).
pub const DEFAULT_WANDER: f32 = yalain_content::movement::default::WANDER_RANGE;

/// The motion bytes of running and of walking forward (2017's full forward is 80 FE 7F; walking
/// a median 0xBC).
pub const RUNNING: u32 = crate::world::motion(0x80, 0xFE, 0x7F);
pub const WALKING: u32 = crate::world::motion(0x80, 0xBC, 0x7F);

/// The chance a resting creature of a friend table joins its kin's fight within 10 m (half that
/// from 10 to 20 m): drudges 0.39 (472 of 1,224), buruns 0.33, mimbus 0.48 (12 of 25), guroks 0.19,
/// loners 0.12, the undead 0.10, groups 0.09; others not above 2017's 3% of strangers.
#[must_use]
pub fn joins_chance(table: Option<u32>) -> f32 {
    match table {
        Some(0x06) => 0.39,
        Some(0x02) => 0.33,
        Some(0x09) => 0.48,
        Some(0x08) => 0.19,
        Some(0x16) => 0.12,
        Some(0x2A) => 0.10,
        Some(0x15) => 0.09,
        _ => 0.0,
    }
}

/// How close a creature fighting a pet must be to strike it, how near a shooter lets a player come
/// before it fights hand to hand, and what a turret's reach is at least, in meters (center to
/// center).
pub const MELEE_REACH: f32 = 2.5;

/// How far past its body's radius a creature running at a player who stands stops, in meters. 2017:
/// over 415 swings by a creature standing at a player standing, the median 0.71 whatever the body; a
/// run's first swing at one, 340 swings, median 0.91.
pub const STOPS_PAST_BODY: f32 = 0.7;

/// How far past its body's radius a creature running at a player on the move stops, in meters. 2017:
/// over 267 swings by a creature that had been moving, at a player going under 4 m/s, the medians
/// 1.85 to 2.42 as the player goes faster.
pub const STOPS_PAST_BODY_MOVING: f32 = 2.0;

/// How far past its body's radius a creature's melee swing reaches, in meters: within it, a creature
/// standing stays where it is and swings. 2017: 407 of those 415 swings came from under 3.0, and a
/// player walking saw the same edge (p90 3.12).
pub const REACHES_PAST_BODY: f32 = 3.0;

/// How far a player can shift between two of a creature's steps and still count as standing, in
/// meters.
const PLAYER_STANDS: f32 = 0.05;

/// How long a creature remembers the harm a player did it when it thinks again about whom it
/// fights, in seconds.
pub const HARM_REMEMBERED: f64 = 10.0;

/// How much more a skill's harm counts when a creature thinks again about whom it fights, in percent
/// of the harm. This server's own stand-in, not a recorded rule: the numbers are the skills' own
/// taunt bonuses and damage-notice figures, read at one point to one percent, and a later scale is
/// one line to change.
pub const HARM_COUNTS_PER_POINT: f64 = 1.0;
const HARM_POINTS: [(u32, f64); 26] = [
    // Taunt bonuses.
    (326, -1.0),
    (701, 1.0),
    (706, 1.0),
    (838, 2.0),
    (840, 2.0),
    (852, 1.0),
    (854, 1.0),
    (856, 1.0),
    (857, 1.5),
    (867, 2.0),
    (1076, 3.5),
    (1221, 1.0),
    (1377, 1.0),
    (1433, -0.5),
    (1434, -0.5),
    (2189, 5.0),
    (2190, 5.0),
    (2191, 5.0),
    (2192, 5.0),
    (2193, 5.0),
    // Damage-notice figures.
    (322, 0.5),
    (331, 0.5),
    (333, 0.5),
    (334, 0.5),
    (860, 0.5),
    (861, 0.5),
];

/// The harm that counts at a creature's draw when a skill does this much damage.
#[must_use]
pub fn harm_noticed(skill: u32, damage: i32) -> i64 {
    let points = HARM_POINTS
        .iter()
        .find(|(s, _)| *s == skill)
        .map_or(0.0, |(_, p)| *p);
    let counted = f64::from(damage) * (1.0 + points * HARM_COUNTS_PER_POINT / 100.0);
    #[allow(clippy::cast_possible_truncation)]
    let rounded = counted.round() as i64;
    rounded
}

/// How long a taunt counts when a creature thinks again about whom it fights, in seconds (2017:
/// one player's 1,041 taunts of two skills: turned on 130 of 630 attacks within this after one, 2
/// of 72 in the 25 s after that).
pub const TAUNT_REMEMBERED: f64 = 20.0;

/// The chance, at one of its attacks, that a creature players have hurt lately draws its target
/// again by their harm: while it has fought its target less than [`HELD_LONG`] seconds, and after
/// (2017: 1,048 switches against 950 at the pooled 0.28 before fifteen seconds, 90 against 188
/// after).
pub const REDRAW_CHANCE: f32 = 0.30;
pub const REDRAW_CHANCE_LATER: f32 = 0.13;
pub const HELD_LONG: f64 = 15.0;

/// How much likelier the nearest player is drawn than its harm (or its place) alone says (2017:
/// the nearest of two or more other players that hurt it taken 409 times against 268 by harm
/// alone; 67 of 141 by the unharmed against 36.5 by chance).
pub const NEAREST_WEIGHT: f64 = 3.0;

/// The chance, at one of its attacks, that a creature no player has hurt over the last ten seconds
/// turns to another player within its perception: when one is a meter or more nearer than its
/// target, and otherwise (2017: 0.110 and 0.065 over 2,751 such attacks).
pub const TURNS_TO_NEARER: f32 = 0.110;
pub const TURNS_TO_ANOTHER: f32 = 0.065;

/// The perception radius of a creature whose settings give none above 0: the game's scripts'
/// default (2017's creatures of templates setting none engaged at a median 19.6 m).
pub const DEFAULT_PERCEPTION: f32 = yalain_content::movement::default::PERCEPTION;

/// The farthest a creature goes from home after a player before it gives up (authored: 2017's
/// fights drifted a median 10-30 m from where they began, at most about 60 m).
pub const MAX_FROM_HOME: f32 = 80.0;

/// How close to its goal a walk ends, in meters (on the ground: the height is the ground's).
pub const ARRIVED: f32 = 0.5;

/// A creature's two idle behaviors, and the share of its idles that are the first (2017: 2,987
/// of 6,840).
pub const IDLE_BEHAVIORS: [u32; 2] = [0x4100_001B, 0x4100_001C];
pub const FIRST_IDLE_SHARE: f32 = 0.44;

/// How long a creature at rest goes between its idle behaviors on average, in seconds (2017: 1,213
/// over 152 hours of creatures at rest within 50 m of a player; the gaps spread from seconds to
/// many minutes, as chance does).
pub const IDLE_EVERY: f32 = 450.0;

/// The position flag of an object touching the ground.
pub(crate) const ON_GROUND: u32 = 0x4;

/// Every creature acts.
pub fn step<S: Store>(s: &mut Server<S>, net: &mut ServerNet, clock: Clock) {
    let now = clock.local.0;
    let mut ids = Vec::new();
    s.fighting.clear();
    if let Some(w) = s.walls.as_mut() {
        w.new_pass();
    }
    for t in s.things.by_id.values() {
        let Some(c) = t.creature.as_ref() else {
            continue;
        };
        if t.alive() && !s.pets.is_pet(t.id) {
            ids.push(t.id);
            if c.mode == Mode::Chasing {
                s.fighting.push(t.id);
            }
        }
    }
    for id in ids {
        act(s, net, id, clock, now);
    }
}

/// The player a creature notices: the nearest within its perception that is in the world, out of
/// portal space, alive and not under lifestone protection.
fn notice<S: Store>(
    s: &Server<S>,
    at: &StoredPosition,
    radius: f32,
    class: Option<u32>,
    attacks_first: bool,
) -> Option<u64> {
    notices_among(s, at, radius, |p| {
        fightable(p)
            && p.admin.attackable == Attackable::Normal
            && match regard(p, class) {
                Some(Regard::Spares) => false,
                Some(Regard::Hates) => true,
                None => attacks_first,
            }
    })
}

/// The nearest player within `radius` that `may` passes. Where a player stands is known at once and
/// `may` reads its state, so it is asked only of those near: the world's every creature looks every
/// second, and nearly all have no player near.
#[doc(hidden)]
pub fn notices_among<S: Store>(
    s: &Server<S>,
    at: &StoredPosition,
    radius: f32,
    mut may: impl FnMut(&crate::world::Player) -> bool,
) -> Option<u64> {
    s.players
        .values()
        .filter_map(|p| presence::distance(at, &p.position).map(|d| (d, p)))
        .filter(|(d, _)| *d <= radius)
        .filter(|(_, p)| may(p))
        .map(|(d, p)| (d, p.character))
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, id)| id)
}

/// [`notice`] for tests of who is passed over.
#[doc(hidden)]
#[must_use]
pub fn notices<S: Store>(s: &Server<S>, at: &StoredPosition, radius: f32) -> Option<u64> {
    notice(s, at, radius, None, true)
}

/// A creature's attack comes when it is due, not at the world's next step, for one that attacks
/// from where it stands: within reach of its target, shooting within range, or fixed where it is
/// (2017: gaps between one creature's attacks don't fold on the step, a concentration of 0.06 or
/// less over 15,575 melee gaps and 9,659 of missiles and magic; on a pet 0.03 over 20,994). Going
/// toward its target stays on the step. Only those fighting at the last step are looked at: a
/// creature's first attack comes long after it takes on a player, so none is due before the next
/// step would list it.
pub fn attacks_due<S: Store>(s: &mut Server<S>, net: &mut ServerNet, clock: Clock) {
    let now = clock.local.0;
    let due: Vec<u64> = s
        .fighting
        .iter()
        .filter_map(|id| s.things.get(*id))
        .filter(|t| t.alive() && !s.pets.is_pet(t.id) && attacks_from_here(s, t, now))
        .map(|t| t.id)
        .collect();
    for id in due {
        // Its attack redraws its target: one it has to run to waits for the step.
        if let Some((at, perception)) = s.things.get(id).and_then(|t| {
            let c = t.creature.as_ref()?;
            Some((t.at?, perception_of(c)))
        }) {
            reconsider(s, id, &at, perception, now);
        }
        if s.things
            .get(id)
            .is_some_and(|t| attacks_from_here(s, t, now))
        {
            act(s, net, id, clock, now);
        }
    }
}

/// Is a creature fighting a player or a pet, its attack due, attacking from where it stands: within
/// reach, shooting within range, or fixed where it is?
fn attacks_from_here<S: Store>(s: &Server<S>, t: &crate::things::Thing, now: f64) -> bool {
    let Some(c) = t.creature.as_ref() else {
        return false;
    };
    if c.mode != Mode::Chasing || !c.stats.fights || now < c.next_attack {
        return false;
    }
    let (Some(at), Some(target)) = (t.at, c.target) else {
        return false;
    };
    if s.pets.is_pet(target) {
        return s
            .things
            .get(target)
            .filter(|p| p.alive())
            .and_then(|p| p.at)
            .and_then(|p| presence::distance(&at, &p))
            .is_some_and(|d| d <= MELEE_REACH);
    }
    let Some(p) = s.players.values().find(|p| p.character == target) else {
        return false;
    };
    let reach = body_radius_of(s, t.id) + REACHES_PAST_BODY;
    presence::distance(&at, &p.position).is_some_and(|d| {
        (d <= reach && c.closing.is_none())
            || (c.ranged && d <= crate::missiles::MISSILE_RANGE)
            || c.stats.turns_only()
    })
}

/// A creature's body's radius.
fn body_radius_of<S: Store>(s: &Server<S>, id: u64) -> f32 {
    s.things.get(id).map_or(0.0, |t| {
        let scale = t.creature.as_ref().map_or(1.0, |c| c.stats.scale);
        crate::walls::body_radius(s.walls.as_ref(), t.template, scale)
    })
}

/// How far a creature notices a player, and gives up beyond.
fn perception_of(c: &crate::things::Creature) -> f32 {
    if c.stats.movement.perception > 0.0 {
        c.stats.movement.perception
    } else {
        DEFAULT_PERCEPTION
    }
}

fn act<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64, clock: Clock, now: f64) {
    let Some(t) = s.things.get(id) else { return };
    let Some(c) = t.creature.as_ref() else { return };
    let Some(at) = t.at else { return };
    let perception = perception_of(c);
    if !c.stats.fights {
        return;
    }
    let run = RUN_SPEED * c.stats.scale * speed_scale(s, id);
    let (mode, home, attacks_first) = (c.mode, c.home, c.stats.attacks_first);
    let leash = leash(c);
    let (joining, next_look) = (c.joining, c.next_look);
    let turns_only = c.stats.turns_only();
    let patrols = c.patrol.is_some() && c.stats.movement.walks();
    if mode == Mode::Chasing {
        reconsider(s, id, &at, perception, now);
    }
    let Some(target) = s
        .things
        .get(id)
        .and_then(|t| t.creature.as_ref())
        .map(|c| c.target)
    else {
        return;
    };
    match mode {
        Mode::Idle => {
            if let Some((p, when)) = joining {
                if now >= when {
                    if let Some(c) = s.things.get_mut(id).and_then(|t| t.creature.as_mut()) {
                        c.joining = None;
                    }
                    let class = class_of(s, id);
                    let here = s.players.values().any(|q| {
                        q.character == p
                            && !crate::combat::is_dead(q)
                            && q.trip.is_none()
                            && regard(q, class) != Some(Regard::Spares)
                    });
                    if here {
                        engage(s, net, id, p, clock);
                        return;
                    }
                }
            }
            if now >= next_look {
                if let Some(c) = s.things.get_mut(id).and_then(|t| t.creature.as_mut()) {
                    c.next_look = now + LOOK_EVERY;
                }
                if let Some(p) = notice(s, &at, perception, class_of(s, id), attacks_first) {
                    go_after(s, net, id, p, clock);
                    return;
                }
            }
            if patrols {
                patrol(s, net, id, clock, now);
            } else if !turns_only {
                wander(s, net, id, clock, now);
            }
            idle(s, net, id, now);
        }
        Mode::Chasing => {
            if let Some(pet) = target.filter(|t| s.pets.is_pet(*t)) {
                fights_pet(s, net, id, pet, &at, leash.as_ref(), perception, run, clock);
                return;
            }
            let player = target.and_then(|t| {
                s.players
                    .iter()
                    .find(|(_, p)| p.character == t)
                    .map(|(c, p)| {
                        (
                            *c,
                            p.position,
                            crate::combat::is_dead(p)
                                || p.trip.is_some()
                                || crate::effects::protected(p),
                        )
                    })
            });
            let far_from_home = too_far(&at, leash.as_ref());
            let Some((conn, place, gone)) = player else {
                give_up_or_turn(s, net, id, &at, perception, now);
                return;
            };
            if gone {
                give_up_or_turn(s, net, id, &at, perception, now);
                return;
            }
            let d = presence::distance(&at, &place);
            let beyond = d.is_none_or(|d| d > perception);
            let lost = {
                let Some(c) = s.things.get_mut(id).and_then(|t| t.creature.as_mut()) else {
                    return;
                };
                if beyond {
                    let since = *c.lost_since.get_or_insert(now);
                    now - since >= GIVE_UP_AFTER
                } else {
                    c.lost_since = None;
                    false
                }
            };
            if far_from_home || lost {
                go_home(s, net, id);
                return;
            }
            if turns_only {
                face(s, net, id, &place, clock);
                shoots_in_place(s, net, id, conn, d.unwrap_or(f32::MAX), clock);
                return;
            }
            let ranged = s
                .things
                .get(id)
                .and_then(|t| t.creature.as_ref())
                .is_some_and(|c| c.ranged);
            if ranged && d.is_some_and(|d| d > MELEE_REACH) {
                // Shooting: in range it stands and shoots; beyond it, it runs in.
                if d.is_some_and(|d| d > crate::missiles::MISSILE_RANGE) {
                    let near = crate::missiles::MISSILE_RANGE * 0.95;
                    step_toward(s, net, id, &place, run, near, RUNNING, clock);
                    cannot_reach(s, id, target);
                } else {
                    face(s, net, id, &place, clock);
                    crate::missiles::shoot(s, net, id, conn, clock);
                }
            } else {
                // Beyond its reach it runs at the player, and goes on to where it stops even once
                // within reach; one that got there, or never had to run, stands for as long as the
                // player stays within reach.
                let radius = body_radius_of(s, id);
                let reach = radius + REACHES_PAST_BODY;
                let d = d.unwrap_or(f32::MAX);
                let Some(c) = s.things.get_mut(id).and_then(|t| t.creature.as_mut()) else {
                    return;
                };
                let was = c.closing;
                // A player on the move it stops further from, and swings at on the run.
                let moved = was.is_some_and(|was| {
                    presence::distance(&was, &place).is_none_or(|m| m > PLAYER_STANDS)
                });
                let stop = radius
                    + if moved {
                        STOPS_PAST_BODY_MOVING
                    } else {
                        STOPS_PAST_BODY
                    };
                let (dx, dy, _) = offset(&at, &place);
                let there = (dx * dx + dy * dy).sqrt() <= stop + 0.01;
                if d > reach || (was.is_some() && !there) {
                    c.closing = Some(place);
                } else {
                    c.closing = None;
                }
                let runs = c.closing.is_some();
                if d <= reach {
                    // A target in melee reach is struck, for the rest of the fight.
                    c.ranged = false;
                }
                if runs {
                    let step = step_toward(s, net, id, &place, run, stop, RUNNING, clock);
                    cannot_reach(s, id, target);
                    // Within reach and held up, it comes no nearer: it stands and strikes from there.
                    if step == Step::Blocked && d <= reach {
                        if let Some(c) = s.things.get_mut(id).and_then(|t| t.creature.as_mut()) {
                            c.closing = None;
                        }
                    }
                } else {
                    face(s, net, id, &place, clock);
                }
                if d <= reach && (!runs || moved) {
                    crate::combat::creature_strikes(s, net, id, conn, clock);
                }
            }
        }
        Mode::Returning => {
            let home = back_to(s, id).unwrap_or(home);
            if arrived(&at, &home) {
                arrive_home(s, net, id, clock);
            } else {
                let step = step_toward(s, net, id, &home, run, 0.0, RUNNING, clock);
                let Some(c) = s.things.get_mut(id).and_then(|t| t.creature.as_mut()) else {
                    return;
                };
                c.blocked = if step == Step::Blocked {
                    c.blocked.saturating_add(1)
                } else {
                    0
                };
                // Held up on its way home, it comes to rest where it is.
                if c.blocked >= BLOCKED_HOME {
                    c.blocked = 0;
                    arrive_home(s, net, id, clock);
                }
            }
        }
        Mode::Dead => {}
    }
}

/// Going after a player, a creature held where it stands with nothing leading to the player (its
/// search found no way, or a closed door it cannot open stands between) could not reach that
/// player this fight: that player's kill of it is no fair fight ([`crate::death`]).
fn cannot_reach<S: Store>(s: &mut Server<S>, id: u64, target: Option<u64>) {
    let Some(player) = target else { return };
    let Some(c) = s.things.get_mut(id).and_then(|t| t.creature.as_mut()) else {
        return;
    };
    if c.way.no_way && !c.unreached.contains(&player) {
        c.unreached.push(player);
    }
}

/// Where a creature's distance in a fight is measured from, to give up beyond [`MAX_FROM_HOME`]:
/// its home, or where it left its route; none for a creature free of its home.
fn leash(c: &crate::things::Creature) -> Option<StoredPosition> {
    if c.stats.movement.kind == yalain_content::MovementType::FreeRange {
        return None;
    }
    Some(c.patrol.as_ref().and_then(|p| p.left_at).unwrap_or(c.home))
}

/// Has a fight carried a creature too far from where its leash is measured?
fn too_far(at: &StoredPosition, leash: Option<&StoredPosition>) -> bool {
    leash.is_some_and(|l| presence::distance(at, l).is_none_or(|d| d > MAX_FROM_HOME))
}

/// Where a creature that gave up runs back to when it walks a route: the waypoint it goes on from.
fn back_to<S: Store>(s: &Server<S>, id: u64) -> Option<StoredPosition> {
    s.things
        .get(id)
        .and_then(|t| t.creature.as_ref())
        .and_then(|c| c.patrol.as_ref())
        .and_then(Patrol::goal)
}

/// A creature fights a pet: it gives up as with a player when the pet is gone, dead or beyond its
/// perception, or it is too far from home; else it runs into reach and strikes it.
#[allow(clippy::too_many_arguments)]
fn fights_pet<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    id: u64,
    pet: u64,
    at: &StoredPosition,
    leash: Option<&StoredPosition>,
    perception: f32,
    run: f32,
    clock: Clock,
) {
    let place = s.things.get(pet).filter(|t| t.alive()).and_then(|t| t.at);
    let d = place.and_then(|p| presence::distance(at, &p));
    let far_from_home = too_far(at, leash);
    let (Some(place), Some(d)) = (place, d) else {
        give_up_or_turn(s, net, id, at, perception, clock.local.0);
        return;
    };
    if far_from_home || d > perception {
        go_home(s, net, id);
        return;
    }
    if d > MELEE_REACH {
        step_toward(s, net, id, &place, run, MELEE_REACH * 0.8, RUNNING, clock);
    } else {
        face(s, net, id, &place, clock);
        crate::combat::thing_strikes_thing(s, net, id, pet, None, clock);
    }
}

/// A creature's speed as its effects leave it: 0 held, below 1 slowed.
fn speed_scale<S: Store>(s: &Server<S>, id: u64) -> f32 {
    crate::effects::creature_speed(s, id).0.max(0.0)
}

/// Is a player one a creature may fight: in the world, out of portal space, alive and not under
/// lifestone protection?
fn fightable(p: &crate::world::Player) -> bool {
    p.scene_drawn
        && p.trip.is_none()
        && !crate::combat::is_dead(p)
        && !crate::effects::protected(p)
        && p.admin.attackable != Attackable::Super
}

/// A taunt lands on a creature: for a while it counts for its caster as harm done, a share of the
/// creature's whole health, or against it when below 0. It never forces a turn (2017: right after
/// a taunt, turned on at 26%, under the draw's own chance).
pub(crate) fn taunted<S: Store>(s: &mut Server<S>, id: u64, by: u64, share: f32, now: f64) {
    let Some(t) = s.things.get_mut(id) else {
        return;
    };
    let whole = t.max_health;
    let Some(c) = t.creature.as_mut() else {
        return;
    };
    c.taunts
        .retain(|(when, _, _)| now - when < TAUNT_REMEMBERED);
    let amount = (f64::from(share) * f64::from(whole)).round();
    #[allow(clippy::cast_possible_truncation)]
    c.taunts.push((now, by, amount as i64));
}

/// [`reconsider`] for tests: the creature thinks again now, where it stands.
#[doc(hidden)]
pub fn reconsider_for_test<S: Store>(s: &mut Server<S>, id: u64, now: f64) {
    if let Some((at, perception)) = s.things.get(id).and_then(|t| {
        let c = t.creature.as_ref()?;
        Some((t.at?, perception_of(c)))
    }) {
        reconsider(s, id, &at, perception, now);
    }
}

/// At one of its attacks (once for each), a creature fighting a player may draw its target again:
/// by the harm players did it over the last ten seconds, the taunts of the last twenty and their
/// nearness while some did, else toward another player within its perception now and then.
fn reconsider<S: Store>(
    s: &mut Server<S>,
    id: u64,
    at: &StoredPosition,
    perception: f32,
    now: f64,
) {
    let Some(c) = s.things.get_mut(id).and_then(|t| t.creature.as_mut()) else {
        return;
    };
    if now < c.next_attack || c.considered >= c.next_attack {
        return;
    }
    c.considered = c.next_attack;
    let Some(target) = c.target else { return };
    let held_long = now - c.target_since >= HELD_LONG;
    let mut harm: Vec<(u64, i64)> = Vec::new();
    let harms = c.harm.iter().filter(|e| now - e.0 < HARM_REMEMBERED);
    let taunts = c.taunts.iter().filter(|e| now - e.0 < TAUNT_REMEMBERED);
    for (_, who, h) in harms.chain(taunts) {
        match harm.iter_mut().find(|(p, _)| p == who) {
            Some(e) => e.1 += h,
            None => harm.push((*who, *h)),
        }
    }
    // The players it may fight, with how far each is: it passes over one it spares that hasn't
    // hurt it.
    let class = class_of(s, id);
    let near: Vec<(u64, f32)> = s
        .players
        .values()
        .filter_map(|p| presence::distance(at, &p.position).map(|d| (p, d)))
        .filter(|(p, d)| *d <= perception && fightable(p))
        .filter(|(p, _)| {
            regard(p, class) != Some(Regard::Spares)
                || harm.iter().any(|(q, h)| *q == p.character && *h > 0)
        })
        .map(|(p, d)| (p.character, d))
        .collect();
    let harmers: Vec<(u64, f32, f64)> = harm
        .iter()
        .filter(|(_, h)| *h > 0)
        .filter_map(|(p, h)| {
            let d = near.iter().find(|(q, _)| q == p)?.1;
            #[allow(clippy::cast_precision_loss)]
            Some((*p, d, *h as f64))
        })
        .collect();
    let pick = if harmers.is_empty() {
        let mine = near.iter().find(|(p, _)| *p == target).map(|e| e.1);
        let others: Vec<(u64, f32, f64)> = near
            .iter()
            .filter(|(p, _)| *p != target)
            .map(|(p, d)| (*p, *d, 1.0))
            .collect();
        if others.is_empty() {
            return;
        }
        let nearer = mine.is_some_and(|m| others.iter().any(|(_, d, _)| *d + 1.0 <= m));
        let chance = if nearer {
            TURNS_TO_NEARER
        } else {
            TURNS_TO_ANOTHER
        };
        if !s.rng.chance(chance) {
            return;
        }
        draw(s, &others)
    } else {
        let chance = if held_long {
            REDRAW_CHANCE_LATER
        } else {
            REDRAW_CHANCE
        };
        if !s.rng.chance(chance) {
            return;
        }
        draw(s, &harmers)
    };
    if let Some(p) = pick.filter(|p| *p != target) {
        if let Some(c) = s.things.get_mut(id).and_then(|t| t.creature.as_mut()) {
            c.target = Some(p);
            c.target_since = now;
            c.lost_since = None;
        }
    }
}

/// Draw one of some players (each with its distance and weight) by their weights, the nearest of
/// them three times as likely.
pub fn draw<S: Store>(s: &mut Server<S>, from: &[(u64, f32, f64)]) -> Option<u64> {
    let nearest = from
        .iter()
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|e| e.0)?;
    let weight = |e: &(u64, f32, f64)| {
        if e.0 == nearest {
            e.2 * NEAREST_WEIGHT
        } else {
            e.2
        }
    };
    let total: f64 = from.iter().map(weight).sum();
    if total <= 0.0 {
        return None;
    }
    let mut r = f64::from(s.rng.unit()) * total;
    for e in from {
        r -= weight(e);
        if r < 0.0 {
            return Some(e.0);
        }
    }
    from.last().map(|e| e.0)
}

/// A creature goes after a player (it noticed it, or the player struck it), and its kin nearby
/// may come too.
pub fn go_after<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    id: u64,
    player: u64,
    clock: Clock,
) {
    let fresh = s
        .things
        .get(id)
        .and_then(|t| t.creature.as_ref())
        .is_some_and(|c| !matches!(c.mode, Mode::Dead | Mode::Chasing) && c.stats.fights);
    if !fresh {
        return;
    }
    engage(s, net, id, player, clock);
    call_kin(s, id, player, clock.local.0);
}

/// How near a creature going after a player must be for a turret to take on a player of its own,
/// in meters (2017: 26 of 26 turrets players came near fired exactly when a creature within 40 m
/// of them fought, another turret among them).
pub const TURRETS_ROUSED_WITHIN: f32 = 40.0;

/// A creature that never moves shoots its missile at its target when its attack is due and the
/// target is within the missile's reach; beyond it, it only turns to face it.
fn shoots_in_place<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    id: u64,
    target: ConnId,
    distance: f32,
    clock: Clock,
) {
    let Some(c) = s.things.get(id).and_then(|t| t.creature.as_ref()) else {
        return;
    };
    if clock.local.0 < c.next_attack {
        return;
    }
    let (basic, special) = crate::missiles::attacks_of(&s.skill_table, &c.stats);
    let is_basic = |a: &crate::missiles::AttackSkill| basic.iter().any(|b| b.skill == a.skill);
    let Some(a) = basic.iter().chain(&special).find(|a| a.missile).copied() else {
        return;
    };
    if distance > a.max_range.max(MELEE_REACH) {
        return;
    }
    let special = !is_basic(&a);
    crate::missiles::launch(s, net, id, target, a, special, clock);
}

/// Creatures that turn but never move (turrets), resting within reach of a creature that goes after
/// a player, each take on the nearest player within their missile's reach; each one roused rouses
/// those near it in turn.
fn rouse_turrets<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64, clock: Clock) {
    let Some(at) = s.things.get(id).and_then(|t| t.at) else {
        return;
    };
    let near: Vec<(u64, StoredPosition, f32)> = s
        .things
        .by_id
        .values()
        .filter(|t| t.id != id && t.alive() && !s.pets.is_pet(t.id))
        .filter_map(|t| {
            let c = t.creature.as_ref()?;
            let there = t.at?;
            let d = presence::distance(&at, &there)?;
            (c.stats.turns_only()
                && c.stats.fights
                && c.mode == Mode::Idle
                && d <= TURRETS_ROUSED_WITHIN)
                .then(|| {
                    let (basic, special) = crate::missiles::attacks_of(&s.skill_table, &c.stats);
                    let reach = basic
                        .iter()
                        .chain(&special)
                        .filter(|a| a.missile)
                        .map(|a| a.max_range)
                        .fold(MELEE_REACH, f32::max);
                    (t.id, there, reach)
                })
        })
        .collect();
    for (turret, there, reach) in near {
        if let Some(p) = notice(s, &there, reach, class_of(s, turret), true) {
            engage(s, net, turret, p, clock);
        }
    }
}

/// Resting creatures of a creature's friend table near it may join its fight with a player, about
/// a second later.
fn call_kin<S: Store>(s: &mut Server<S>, id: u64, player: u64, now: f64) {
    let Some((at, table)) = s
        .things
        .get(id)
        .and_then(|t| Some((t.at?, t.creature.as_ref()?.stats.friend_table)))
    else {
        return;
    };
    let chance = joins_chance(table);
    if chance <= 0.0 {
        return;
    }
    let kin: Vec<(u64, f32)> = s
        .things
        .by_id
        .values()
        .filter(|t| t.id != id && t.alive())
        .filter_map(|t| {
            let c = t.creature.as_ref()?;
            let d = presence::distance(&at, &t.at?)?;
            (c.mode == Mode::Idle
                && c.joining.is_none()
                && c.stats.fights
                && c.stats.friend_table == table
                && d <= 20.0)
                .then_some((t.id, d))
        })
        .collect();
    for (k, d) in kin {
        let p = if d <= 10.0 { chance } else { chance / 2.0 };
        if s.rng.chance(p) {
            if let Some(c) = s.things.get_mut(k).and_then(|t| t.creature.as_mut()) {
                c.joining = Some((player, now + JOINS_AFTER));
            }
        }
    }
}

/// A creature takes on a player: its combat mode, how it fights, when it first attacks.
fn engage<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64, player: u64, clock: Clock) {
    let Some(c) = s.things.get_mut(id).and_then(|t| t.creature.as_mut()) else {
        return;
    };
    if matches!(c.mode, Mode::Dead | Mode::Chasing) || !c.stats.fights {
        return;
    }
    c.mode = Mode::Chasing;
    c.lost_since = None;
    c.joining = None;
    c.target = Some(player);
    c.closing = None;
    let here = s.things.get(id).and_then(|t| t.at);
    let Some(c) = s.things.get_mut(id).and_then(|t| t.creature.as_mut()) else {
        return;
    };
    if let Some(p) = c.patrol.as_mut() {
        p.left_at = p.left_at.or(here);
        p.walking = false;
    }
    c.target_since = clock.local.0;
    c.actions = 0;
    let stats = c.stats.clone();
    crate::talk::fights(s, net, id, player);
    let at = s.things.get(id).and_then(|t| t.at);
    let distance = s
        .players
        .values()
        .find(|p| p.character == player)
        .and_then(|p| at.and_then(|a| presence::distance(&a, &p.position)))
        .unwrap_or(0.0);
    let ranged = crate::missiles::chooses_to_shoot(s, &stats, distance);
    let now = clock.local.0;
    if let Some(c) = s.things.get_mut(id).and_then(|t| t.creature.as_mut()) {
        c.ranged = ranged;
        c.next_attack = if ranged {
            c.next_attack.max(now + crate::missiles::FIRST_SHOT)
        } else {
            c.next_attack.max(now + 1.0)
        };
    }
    crate::combat::creature_engages(s, net, id, player);
    rouse_turrets(s, net, id, clock);
}

/// A creature's target is dead or gone: it takes the nearest player within its perception that has
/// hurt it, else it gives up.
fn give_up_or_turn<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    id: u64,
    at: &StoredPosition,
    perception: f32,
    now: f64,
) {
    let hurt_by: Vec<u64> = s
        .things
        .get(id)
        .and_then(|t| t.creature.as_ref())
        .map(|c| c.damage_by.iter().map(|(p, _)| *p).collect())
        .unwrap_or_default();
    let next = s
        .players
        .values()
        .filter(|p| {
            hurt_by.contains(&p.character)
                && p.trip.is_none()
                && !crate::combat::is_dead(p)
                && !crate::effects::protected(p)
        })
        .filter_map(|p| presence::distance(at, &p.position).map(|d| (d, p.character)))
        .filter(|(d, _)| *d <= perception)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, p)| p);
    match next {
        Some(p) => {
            if let Some(c) = s.things.get_mut(id).and_then(|t| t.creature.as_mut()) {
                c.target = Some(p);
                c.target_since = now;
                c.lost_since = None;
            }
        }
        None => go_home(s, net, id),
    }
}

/// A creature gives up: it is at peace, and goes home, or to its route's nearest waypoint.
fn go_home<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64) {
    let at = s.things.get(id).and_then(|t| t.at);
    if let Some(c) = s.things.get_mut(id).and_then(|t| t.creature.as_mut()) {
        c.mode = Mode::Returning;
        c.target = None;
        c.moving_to = None;
        if let (Some(p), Some(at)) = (c.patrol.as_mut(), at) {
            p.to = p.nearest(&at);
            p.walking = false;
        }
    }
    crate::combat::creature_calms(s, net, id);
}

/// A creature is home again: at rest (it regenerates), standing.
fn arrive_home<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64, clock: Clock) {
    if let Some(c) = s.things.get_mut(id).and_then(|t| t.creature.as_mut()) {
        c.mode = Mode::Idle;
        c.damage_by.clear();
        c.taken_by.clear();
        c.harm.clear();
        c.taunts.clear();
        c.unreached.clear();
        if let Some(p) = c.patrol.as_mut() {
            p.left_at = None;
        }
    }
    stands(s, net, id, clock);
}

/// Has a walk reached its goal: within [`ARRIVED`] of it on the ground (the height left to the
/// ground, which on a slope need not meet the goal's)?
fn arrived(at: &StoredPosition, goal: &StoredPosition) -> bool {
    let (dx, dy, _) = offset(at, goal);
    (dx * dx + dy * dy).sqrt() <= ARRIVED
}

/// A creature stops where it is: its place told once more with its heading, standing.
pub(crate) fn stands<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64, clock: Clock) {
    let Some(t) = s.things.get(id) else { return };
    let Some(at) = t.at else { return };
    #[allow(clippy::cast_possible_truncation)]
    let heading = (t.heading_motion >> 24) as u8;
    place(s, net, id, at, heading, crate::world::IDLE_MOTION, clock);
}

/// When a creature first plays an idle behavior: never for one without idles.
pub(crate) fn first_idle<S: Store>(
    s: &mut Server<S>,
    stats: &yalain_content::CreatureStats,
    now: f64,
) -> f64 {
    if stats.idles {
        now + idle_gap(s)
    } else {
        f64::INFINITY
    }
}

/// The time to a creature's next idle behavior: drawn as chance spaces them, [`IDLE_EVERY`] on
/// average.
fn idle_gap<S: Store>(s: &mut Server<S>) -> f64 {
    let u = s.rng.unit().clamp(0.0, 0.999_999);
    f64::from(-IDLE_EVERY * math::logf(1.0 - u))
}

/// A creature at rest and standing plays one of its idle behaviors when its time comes, for every
/// client that sees it.
fn idle<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64, now: f64) {
    let Some(c) = s.things.get(id).and_then(|t| t.creature.as_ref()) else {
        return;
    };
    let walking = c.wander_to.is_some() || c.patrol.as_ref().is_some_and(|p| p.walking);
    if c.mode != Mode::Idle || walking || now < c.next_idle {
        return;
    }
    let behavior = if s.rng.chance(FIRST_IDLE_SHARE) {
        IDLE_BEHAVIORS[0]
    } else {
        IDLE_BEHAVIORS[1]
    };
    let next = now + idle_gap(s);
    if let Some(c) = s.things.get_mut(id).and_then(|t| t.creature.as_mut()) {
        c.next_idle = next;
    }
    s.next_context = s.next_context.wrapping_add(1);
    let m =
        auberean_protocol::ServerMessage::PlayBehavior(auberean_protocol::physics::ObjectEvent {
            sender: crate::inventory::visual(s, id),
            action: auberean_protocol::types::BehaviorParams {
                bits: auberean_protocol::types::BehaviorParams::BEHAVIOR
                    | auberean_protocol::types::BehaviorParams::CONTEXT,
                behavior,
                context: s.next_context,
                ..auberean_protocol::types::BehaviorParams::default()
            },
        });
    crate::combat::to_all_seeing(s, net, id, &m);
}

/// Now and then, a creature at rest takes a step toward a point within its wandering range of
/// home (one step each world step until it is there).
fn wander<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64, clock: Clock, now: f64) {
    let Some(t) = s.things.get(id) else { return };
    let Some(c) = t.creature.as_ref() else { return };
    if let Some(goal) = c.wander_to {
        let ends = if t.at.is_none_or(|at| arrived(&at, &goal)) {
            true
        } else {
            let walk = WALK_SPEED * c.stats.scale * speed_scale(s, id);
            // A walk that meets something it cannot pass, or has no way through the rooms, ends.
            step_toward(s, net, id, &goal, walk, 0.0, WALKING, clock) == Step::Blocked
        };
        if ends {
            if let Some(c) = s.things.get_mut(id).and_then(|t| t.creature.as_mut()) {
                c.wander_to = None;
            }
            stands(s, net, id, clock);
        }
        return;
    }
    // Indoors a creature at rest stays where it is (2017: 4 of 324 creatures at home indoors watched
    // five minutes or more within 100 m of a player walked, against 406 of 914 outdoors).
    if now < c.next_wander || c.stats.settled() || presence::indoors(c.home.cell) {
        return;
    }
    // Where the walls are known, a creature no player could see rests: its walls are not read.
    if s.walls.is_some() && !player_near(s, &c.home) {
        let next = now + f64::from(s.rng.between(30.0, 86.0));
        if let Some(c) = s.things.get_mut(id).and_then(|t| t.creature.as_mut()) {
            c.next_wander = next;
        }
        return;
    }
    let range = if c.stats.movement.wander_range > 0.0 {
        c.stats.movement.wander_range
    } else {
        DEFAULT_WANDER
    };
    let (home, chance, scale) = (c.home, c.stats.movement.wander_chance, c.stats.scale);
    let (template, from) = (t.template, t.at.unwrap_or(c.home));
    // Between walks it rests (2017: a median 58 s for creatures of a 20 m range).
    let next = now + f64::from(s.rng.between(30.0, 86.0));
    let go = chance <= 0.0 || s.rng.chance(chance);
    let r = range * s.rng.unit().sqrt();
    let a = s.rng.between(0.0, std::f32::consts::TAU);
    let (x, y) = (
        home.origin[0] + r * math::cosf(a),
        home.origin[1] + r * math::sinf(a),
    );
    let goal = Some(StoredPosition {
        origin: [x, y, home.origin[2]],
        ..home
    });
    // Where the walls are known, only a point it can walk to, by the way it will walk.
    let (goal, way) = match (goal.filter(|_| go), s.walls.as_mut()) {
        (Some(g), Some(w)) => match w.way_within(
            template,
            scale,
            &from,
            &g,
            ARRIVED,
            crate::walls::REST_WALK_MOVES,
        ) {
            Some(places) => (Some(g), places),
            None => (None, Vec::new()),
        },
        (g, _) => (g, Vec::new()),
    };
    if let Some(c) = s.things.get_mut(id).and_then(|t| t.creature.as_mut()) {
        c.next_wander = next;
        if go {
            c.wander_to = goal;
            c.way = Way {
                to: goal.filter(|_| !way.is_empty()),
                places: way,
                next_search: now,
                ..Way::default()
            };
        }
    }
}

/// A creature's way along its route: the route, the waypoint it is going to and which way it goes
/// on, until when it rests, and where it left the route to fight.
#[derive(Debug, Clone, PartialEq)]
pub struct Patrol {
    pub route: yalain_content::Route,
    pub to: usize,
    pub backward: bool,
    /// Until when it rests at the waypoint it last reached (world seconds).
    pub rests_until: f64,
    /// Whether it is under way (else it rests, or has yet to set out).
    pub walking: bool,
    /// Its steps in a row that something stood in the way of.
    pub blocked: u8,
    /// Where it was when a fight took it off its route.
    pub left_at: Option<StoredPosition>,
}

impl Patrol {
    /// Setting off toward waypoint `start` of a route (its last when the route is shorter), going
    /// on the way `backward` says.
    #[must_use]
    pub fn new(route: yalain_content::Route, start: usize, backward: bool) -> Self {
        let to = start.min(route.waypoints.len().saturating_sub(1));
        Self {
            route,
            to,
            backward,
            rests_until: f64::NEG_INFINITY,
            walking: false,
            blocked: 0,
            left_at: None,
        }
    }

    /// The waypoint it is going to, as a place.
    #[must_use]
    pub fn goal(&self) -> Option<StoredPosition> {
        self.goal_at(self.to)
    }

    fn goal_at(&self, i: usize) -> Option<StoredPosition> {
        self.route.waypoints.get(i).map(|w| StoredPosition {
            cell: w.cell,
            origin: w.at,
            rotation: [1.0, 0.0, 0.0, 0.0],
        })
    }

    /// The waypoint of its route nearest a place (on the ground).
    #[must_use]
    pub fn nearest(&self, at: &StoredPosition) -> usize {
        let gap = |i: usize| self.goal_at(i).map_or(f32::MAX, |g| flat_gap(at, &g));
        (0..self.route.waypoints.len())
            .min_by(|a, b| gap(*a).total_cmp(&gap(*b)))
            .unwrap_or(self.to)
    }
}

/// A creature at rest walks its route: on to the waypoint it is going to, or, there, it stands
/// and rests the waypoint's rest, and goes on to the next. Held up for a second on its way, it
/// takes the waypoint as reached. Where the walls are known it moves only while a player is near.
fn patrol<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64, clock: Clock, now: f64) {
    let Some(t) = s.things.get(id) else { return };
    let (Some(at), Some(c)) = (t.at, t.creature.as_ref()) else {
        return;
    };
    let Some(p) = c.patrol.as_ref() else { return };
    if now < p.rests_until {
        return;
    }
    let Some(goal) = p.goal() else { return };
    let (walking, gait, pace, scale) = (p.walking, p.route.gait, p.route.speed, c.stats.scale);
    let reached = |s: &mut Server<S>, net: &mut ServerNet| {
        if let Some(p) = s
            .things
            .get_mut(id)
            .and_then(|t| t.creature.as_mut())
            .and_then(|c| c.patrol.as_mut())
        {
            let rest = p.route.waypoints.get(p.to).map_or(0.0, |w| w.rest);
            (p.to, p.backward) = p.route.next(p.to, p.backward);
            p.rests_until = now + rest;
            p.walking = false;
            p.blocked = 0;
        }
        if walking {
            stands(s, net, id, clock);
        }
    };
    if arrived(&at, &goal) {
        reached(s, net);
        return;
    }
    if s.walls.is_some() && !player_near(s, &at) {
        return;
    }
    let (speed, motion) = match gait {
        yalain_content::movement::Gait::Walk => (PATROL_WALK_SPEED, PATROL_WALKING),
        yalain_content::movement::Gait::Run => (RUN_SPEED, RUNNING),
    };
    let speed = pace.unwrap_or(speed);
    let speed = speed * scale * speed_scale(s, id);
    let step = step_toward(s, net, id, &goal, speed, 0.0, motion, clock);
    let held = {
        let Some(p) = s
            .things
            .get_mut(id)
            .and_then(|t| t.creature.as_mut())
            .and_then(|c| c.patrol.as_mut())
        else {
            return;
        };
        p.walking = true;
        p.blocked = if step == Step::Blocked {
            p.blocked.saturating_add(1)
        } else {
            0
        };
        p.blocked >= BLOCKED_HOME
    };
    if held {
        reached(s, net);
    }
}

/// Is a player in the world in the landblock of a place or one beside it (farther than anything
/// a client is told of)?
pub(crate) fn player_near<S: Store>(s: &Server<S>, at: &StoredPosition) -> bool {
    let block = |cell: u32| (i64::from(cell >> 24), i64::from(cell >> 16 & 0xFF));
    let (x, y) = block(at.cell);
    s.players.values().any(|p| {
        let (px, py) = block(p.position.cell);
        (px - x).abs() <= 1 && (py - y).abs() <= 1
    })
}

/// The heading byte (degrees clockwise from north, times 255/360) of a direction on the ground.
#[must_use]
pub fn heading_toward(dx: f32, dy: f32) -> u8 {
    let deg = math::atan2f(dx, dy).to_degrees().rem_euclid(360.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let b = (deg * 255.0 / 360.0).round() as u32;
    u8::try_from(b % 256).unwrap_or(0)
}

/// The vector from one place to another, across landblocks.
#[must_use]
pub fn offset(from: &StoredPosition, to: &StoredPosition) -> (f32, f32, f32) {
    let (la, lb) = (from.cell >> 16, to.cell >> 16);
    #[allow(clippy::cast_precision_loss)]
    let ox = (i64::from(lb >> 8) - i64::from(la >> 8)) as f32 * crate::spawn::BLOCK_SIDE;
    #[allow(clippy::cast_precision_loss)]
    let oy = (i64::from(lb & 0xFF) - i64::from(la & 0xFF)) as f32 * crate::spawn::BLOCK_SIDE;
    (
        to.origin[0] + ox - from.origin[0],
        to.origin[1] + oy - from.origin[1],
        to.origin[2] - from.origin[2],
    )
}

/// Turn a creature to face a place (its place sent with the new heading when it changed).
pub(crate) fn face<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    id: u64,
    goal: &StoredPosition,
    clock: Clock,
) {
    let Some(t) = s.things.get(id) else { return };
    let Some(at) = t.at else { return };
    let (dx, dy, _) = offset(&at, goal);
    let h = heading_toward(dx, dy);
    #[allow(clippy::cast_possible_truncation)]
    let same = (t.heading_motion >> 24) as u8 == h
        && t.heading_motion & 0x00FF_FFFF == crate::world::IDLE_MOTION;
    if !same {
        place(s, net, id, at, h, crate::world::IDLE_MOTION, clock);
    }
}

/// How a step toward a place went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// It moved (or had no need to: it is there, and faces it).
    Moved,
    /// Something stood in its way: it got less than [`BLOCKED_SHARE`] of its stride, or no way
    /// through the rooms leads there.
    Blocked,
}

/// The share of its stride below which a step counts as stopped by what it met.
pub const BLOCKED_SHARE: f32 = 0.25;

/// How many steps in a row a creature running home may be held up before it comes to rest where
/// it is (a second's worth).
pub const BLOCKED_HOME: u8 = 4;

/// Move a creature a step toward a place, stopping `short` of it: through the walls where the host
/// knows them (by way of the doorways to a place in another room), else in a straight line; a
/// closed door on its way holds it, and it opens the door if it may.
#[allow(clippy::too_many_arguments)]
pub(crate) fn step_toward<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    id: u64,
    goal: &StoredPosition,
    speed: f32,
    short: f32,
    motion: u32,
    clock: Clock,
) -> Step {
    let Some(at) = s.things.get(id).and_then(|t| t.at) else {
        return Step::Blocked;
    };
    let (dx, dy, dz) = offset(&at, goal);
    let flat = (dx * dx + dy * dy).sqrt();
    #[allow(clippy::cast_possible_truncation)]
    let stride = speed * crate::spawn::STEP_SECONDS as f32;
    let mut go = (flat - short).clamp(0.0, stride);
    if let Some(c) = s.things.get_mut(id).and_then(|t| t.creature.as_mut()) {
        c.way.no_way = false;
    }
    if go > 0.001 {
        // A closed door on its way holds it, opens for it, or keeps it out.
        let aim = s
            .things
            .get(id)
            .and_then(|t| t.creature.as_ref())
            .filter(|c| c.way.to.is_some_and(|to| flat_gap(&to, goal) <= WAY_KEPT))
            .and_then(|c| c.way.places.first().copied())
            .unwrap_or(*goal);
        match crate::usable::doors::creature_passage(s, net, id, &at, &aim, go, clock) {
            Passage::Clear => {}
            Passage::UpTo(most) => go = go.min(most),
            Passage::Waits => go = 0.0,
            Passage::Shut => {
                if let Some(c) = s.things.get_mut(id).and_then(|t| t.creature.as_mut()) {
                    c.way.no_way = true;
                }
                face(s, net, id, goal, clock);
                return Step::Blocked;
            }
        }
    }
    if go <= 0.001 {
        face(s, net, id, goal, clock);
        return Step::Moved;
    }
    if s.walls.is_some() {
        if let Some(step) = walk_through(s, net, id, &at, goal, go, short, motion, clock) {
            return step;
        }
    }
    let (ux, uy) = (dx / flat, dy / flat);
    let (x, y) = (at.origin[0] + ux * go, at.origin[1] + uy * go);
    let (cell, x, y, z) = if presence::indoors(at.cell) {
        // Indoors the floors are level: the height moves toward the goal's, and the creature takes
        // the goal's cell once it is within a few meters of it.
        let cell = if flat - go < 3.0 { goal.cell } else { at.cell };
        (cell, x, y, at.origin[2] + dz * (go / flat))
    } else {
        let (cell, x, y) = crate::spawn::outdoor(at.cell >> 16, x, y);
        let z = s
            .ground
            .as_ref()
            .and_then(|g| g.height(cell, [x, y, at.origin[2]]))
            .unwrap_or(at.origin[2] + dz * (go / flat));
        (cell, x, y, z)
    };
    let h = heading_toward(dx, dy);
    let next = StoredPosition {
        cell,
        origin: [x, y, z],
        rotation: crate::world::rotation_of_heading(h),
    };
    place(s, net, id, next, h, motion, clock);
    Step::Moved
}

/// The way a creature follows around what stands between it and where it goes: the places to walk
/// through in turn, where the way leads, and when it may next search for one.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Way {
    pub places: Vec<StoredPosition>,
    pub to: Option<StoredPosition>,
    pub next_search: f64,
    /// At its last step nothing led where it goes: its search found no way, or a closed door it
    /// cannot open stood in its way.
    pub no_way: bool,
    /// Where it stood and where it was going when its last search found no way: searching again
    /// from there to there would find none either.
    pub none_found: Option<(StoredPosition, StoredPosition)>,
}

/// How far a goal may move from where a creature's way leads before the way is dropped, in meters.
pub const WAY_KEPT: f32 = 1.5;

/// How near one of its way's places a creature must come to go on to the next, in meters.
pub const WAY_PLACE: f32 = 0.4;

/// How often a creature held up with no way found searches again, in seconds.
pub const SEARCH_EVERY: f64 = 1.0;

/// How far a creature or its goal must move before a search that found no way is worth trying
/// again, in meters.
pub const NO_WAY_KEPT: f32 = 0.5;

fn flat_gap(a: &StoredPosition, b: &StoredPosition) -> f32 {
    let (dx, dy, _) = offset(a, b);
    (dx * dx + dy * dy).sqrt()
}

/// A step of up to `go` meters through the walls toward a goal: along the creature's way to it
/// when it has one, else straight; held up, it asks for a way around (at most once a
/// [`SEARCH_EVERY`], and not again while it and its goal stay where a search found none) and steps
/// along it, standing until it has an answer. None when the walls cannot place the creature (its
/// place was never read).
#[allow(clippy::too_many_arguments)]
fn walk_through<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    id: u64,
    at: &StoredPosition,
    goal: &StoredPosition,
    go: f32,
    short: f32,
    motion: u32,
    clock: Clock,
) -> Option<Step> {
    let now = clock.local.0;
    let (template, scale, mut way) = s.things.get(id).map(|t| {
        let c = t.creature.as_ref();
        (
            t.template,
            c.map_or(1.0, |c| c.stats.scale),
            c.map(|c| c.way.clone()).unwrap_or_default(),
        )
    })?;
    if way.to.is_none_or(|to| flat_gap(&to, goal) > WAY_KEPT) {
        way.places.clear();
        way.to = None;
    }
    while way
        .places
        .first()
        .is_some_and(|p| flat_gap(at, p) <= WAY_PLACE)
    {
        way.places.remove(0);
    }
    let mut stepped = stride_to(s, id, at, way.places.first().unwrap_or(goal), go)?;
    let held = stepped.1 < go * BLOCKED_SHARE;
    // Held up on the way it found: a search's longer legs took it where a walk cannot go.
    let careful = held && !way.places.is_empty();
    let mut waiting = false;
    if held
        && way.none_found.is_some_and(|(from, to)| {
            flat_gap(&from, at) <= NO_WAY_KEPT && flat_gap(&to, goal) <= NO_WAY_KEPT
        })
    {
        way.no_way = true;
    } else if held && now >= way.next_search {
        let close = short.max(ARRIVED);
        match s
            .walls
            .as_mut()
            .map(|w| w.ask(id, template, scale, at, goal, close, careful))
        {
            // It stands while it waits, so the way it is told starts where it is.
            Some(Asked::Waiting) => {
                waiting = true;
                stepped = (*at, 0.0, stepped.2);
            }
            Some(Asked::Way(places)) if !places.is_empty() => {
                way.next_search = now + SEARCH_EVERY;
                way.no_way = false;
                way.none_found = None;
                way.places = places;
                way.to = Some(*goal);
                stepped = stride_to(s, id, at, &way.places[0], go)?;
            }
            _ => {
                way.next_search = now + SEARCH_EVERY;
                way.no_way = true;
                way.none_found = Some((*at, *goal));
            }
        }
    }
    if let Some(c) = s.things.get_mut(id).and_then(|t| t.creature.as_mut()) {
        c.way = way;
    }
    let (next, moved, h) = stepped;
    if moved < 0.01 {
        // Stopped where it is: it stands facing the goal (no running in place).
        face(s, net, id, goal, clock);
        return Some(if waiting { Step::Moved } else { Step::Blocked });
    }
    place(s, net, id, next, h, motion, clock);
    Some(if moved < go * BLOCKED_SHARE && !waiting {
        Step::Blocked
    } else {
        Step::Moved
    })
}

/// One step through the walls of up to `go` meters straight toward `aim` (no farther than it):
/// where it ended, how far it got, its heading.
fn stride_to<S: Store>(
    s: &mut Server<S>,
    id: u64,
    at: &StoredPosition,
    aim: &StoredPosition,
    go: f32,
) -> Option<(StoredPosition, f32, u8)> {
    let (template, scale) = s.things.get(id).map(|t| {
        (
            t.template,
            t.creature.as_ref().map_or(1.0, |c| c.stats.scale),
        )
    })?;
    let (dx, dy, _) = offset(at, aim);
    let len = (dx * dx + dy * dy).sqrt();
    if len <= 0.001 {
        return Some((*at, 0.0, 0));
    }
    let go = go.min(len);
    let fighting = s
        .things
        .get(id)
        .and_then(|t| t.creature.as_ref())
        .is_some_and(|c| c.mode == Mode::Chasing);
    let walls = s.walls.as_mut()?;
    let d = [dx / len * go, dy / len * go];
    let stepped = if fighting {
        walls.step_fighting(template, scale, at, d)?
    } else {
        walls.step(template, scale, at, d)?
    };
    let h = heading_toward(dx, dy);
    let next = StoredPosition {
        rotation: crate::world::rotation_of_heading(h),
        ..stepped.at
    };
    Some((next, stepped.moved, h))
}

/// Put a creature at a place with a heading and motion, and tell the clients that see it.
fn place<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    id: u64,
    at: StoredPosition,
    heading: u8,
    motion: u32,
    clock: Clock,
) {
    let time = net.game_time(clock.local);
    let Some(t) = s.things.get_mut(id) else {
        return;
    };
    let new_cell = t.at.is_none_or(|a| a.cell != at.cell);
    t.at = Some(StoredPosition {
        rotation: crate::world::rotation_of_heading(heading),
        ..at
    });
    t.heading_motion = u32::from(heading) << 24 | (motion & 0x00FF_FFFF);
    t.position_stamp = t.position_stamp.wrapping_add(1);
    let m = ObjectPosition {
        sender: ObjectStamp {
            id,
            instance_stamp: t.instance,
            other_stamp: 0,
        },
        time,
        cell: at.cell,
        origin: auberean_primitives::Vec3::new(at.origin[0], at.origin[1], at.origin[2]),
        heading_motion: t.heading_motion,
        flags: ON_GROUND,
        position_stamp: t.position_stamp,
        ..ObjectPosition::default()
    };
    presence::thing_moves(s, net, id, &m, new_cell);
}
