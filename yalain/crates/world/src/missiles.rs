//! Creatures' attacks beyond the plain melee strike: missiles, and the specials creatures use now
//! and then.
//!
//! **Which attacks a creature has** are its template's starting skills (those numbered 100 and up:
//! the lower ones are its combat ratings), each read from the skill's own definition: the behavior
//! it plays on another, whether it throws a missile (a hook naming one: a missile or a bolt of
//! magic, which arrives and is told by its target) or strikes where it stands, its kind by its
//! name's family, the damage its hook adds at the creature's rating in it, its greatest range, and
//! whether a use bars it for a while. It has none of those its behavior table never uses (the
//! world's rows: 2017's darkenfowl never shot, 0 of 308 attacks; the common vermin never cast their
//! bolt nor struck their plain strike, 0 of 319). Those marked as the template's basic attacks it
//! uses at will; its other attack skills are its specials; skills it uses on itself or its friends
//! (heals, buffs) are not attacks. A creature with a basic missile attack chooses how it fights as it engages:
//! from 15 m or more it shoots; nearer, it picks one of its basic attacks at random, and a missile
//! means it shoots this fight, a melee attack that it closes in (2017: 3 of 141 fights mixed the
//! two).
//!
//! **Shooting.** It stands where it is, facing its target, and shoots when its attack is due (the
//! first about 0.8 s after it engaged), running in only while the target is beyond its missiles'
//! 40 m or out of its sight ([`crate::ai`]); a target that comes into melee reach is struck instead
//! for the rest of the fight (2017: 2,955 shots at players, a median 18.6 m, 99 in 100 within 42.4
//! m, centre to centre). Each
//! shot is a story from the creature (its behavior and time scale, the skill, the missile's hook
//! naming the target); no missile object is made. The missile arrives after the creature's windup
//! and its flight at 35 m/s, and the target tells the outcome: a story from the target with the
//! hit, the evade or the miss, then its health when it is hurt (2017: 9,851 of 9,944 hits, in the
//! order sent). A target that has moved since the shot is more often missed, and one that has gone
//! out of sight of where the missile was loosed from (behind a wall, a rock or a closed door) always
//! is: the missile meets what stands between them.
//!
//! **Specials.** At each attack, each special within its range, not barred and paid for has its
//! own chance ([`special_weight`]): the special's weight, its chance at a generic monster's attack,
//! which a creature with no sub class moves by its AI class's term. None is used with the product of
//! their misses, else one drawn by weight, so the more at hand the likelier one is (2017: the
//! generic monsters 0.081 with one at hand, 0.142 with two, 0.201 with three). A use
//! bars it for 40 s (2017: 1,013 of 1,027 gaps between one creature's uses 38 s or more), so most
//! fights see it once. A special costs the creature the vigor its skill names, a share of its
//! greatest vigor less the skill's own add, and one it lacks the vigor for is not at hand (2017:
//! the install's cost exact 251 of 332 times, nearly every miss a player's drain besides; 777 of
//! 782 specials begun with the vigor for them). It is told like a basic attack without the
//! basic-attack flag; a special that throws nothing strikes where the creature stands. A harming missile's hit also drains the
//! target (an instant vital effect: its effect, then a story of the drain).
//!
//! **What an attack puts on.** A strike or a missile that lands puts its skill's target effects on
//! each player it reaches, cast by the creature at its rating in the skill (a debuff, a harm over
//! time, a stun); a harming missile's is its drain, put on once.
//!
//! **Basic attacks** are drawn at each attack among those that fit the distance: evenly, or, for a
//! creature of the generic monsters' tables (one with a sub class: minion, protector, conqueror,
//! paragon), the plain attack four times as often as the most others and `Strong` twice (2017: such
//! creatures' melee plain, `Strong` and `Stronger` 0.546, 0.283, 0.171 over 16,117 attacks; plain
//! and `Strong` 0.663 and 0.337 over 7,790; the other creatures' 0.331, 0.347, 0.322 over 9,426).
//!
//! **Heals and buffs.** At one of its turns, a creature at or below its warning level of health
//! (two fifths where its template names none) heals itself, three times in five for the generic
//! monsters and about one time in five for the rest; one with a buff free buffs itself by one chance a turn for
//! how many of its buffs are free, about one time in six at its first turn of a fight and one in
//! twelve after with one, one in four and one in six with two or more. One under its vigor warning
//! level (two fifths of its vigor where its template names none) restores its vigor with a restore
//! free about one time in two (2017: 14 of 28 such turns, drudges' alone); a restore of focus is
//! not used. A protector buffs one of its kin in a fight near it, by one
//! chance a turn for how many are near (about one in five at its first turn, one in fifty after), the kin drawn evenly,
//! and next turn buffs that kin again a little under one time in two; a healer heals its badly hurt
//! kin now and then. Heals on kin are otherwise not made (2017: 56 of 3,306 heals on kin, too few
//! to read a rule from). Each takes the turn: its story on the one it helps (its own behavior, the
//! skill, a hit on that one), the effect a second later (a heal shows its appearance and the
//! creature tells its health back), the next attack 2.3 s after the story; a use on itself bars the
//! skill 40 s, one on kin nothing.
//!
//! **Damage.** A creature's hit is its natural damage rolled across its variance, plus what the
//! skill's hook adds at the creature's rating (`Strong` 3 at rating 1 to 75 at 1,000, `Stronger` 7
//! to 200) and what its effects add (a damage debuff takes away), through the player's armor (2017: a `Strong` or `Stronger` hit over a plain one gained
//! about the whole add at the start area's levels: the diggers' +2.5 against 3.0, +6.5 against 7.0).
//!
//! Every number is 2017's; the windups and the gaps between shots are per kind of creature, scaled
//! by each shot's time scale.

use auberean_protocol::physics::{Fx, ObjectEvent};
use auberean_protocol::types::{BehaviorParams, PhysicsStory, StoryHook};
use auberean_protocol::ServerMessage;
use yalain_content::CreatureStats;
use yalain_net::{ConnId, ServerNet};
use yalain_store::{Store, StoredPosition};

use crate::combat::{self, to_all_seeing, CREATURE_TIME_SCALE, SWING_BEHAVIOR_BITS};
use crate::player::quality;
use crate::rules::{self, Kind, Outcome};
use crate::{presence, quals, Clock, Server, Timer};

/// One of creatures' attack skills: the behavior it plays, its kind, whether it throws a missile,
/// what its hook adds at the creature's rating, how much harder it hits than the plain attack of
/// its kind (for a skill without a definition), whether its hit drains the target too, its greatest range
/// and whether a use bars it for a while; and its chance of a critical among its landed attacks,
/// before what the creature's weapon adds (its class's, [`rules::skill_critical`]; for a skill
/// without a definition, its kind's measured share), and how much more a critical of it does before
/// armor (its class's modifier, [`rules::CRITICAL_BEFORE_ARMOR`] where its class sets none); and the
/// radius its area hook reaches round its target (0 for the target alone).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AttackSkill {
    pub skill: u32,
    pub behavior: u32,
    pub kind: Kind,
    pub missile: bool,
    pub added: f32,
    pub factor: f32,
    pub critical: f64,
    pub critical_size: f32,
    pub drains: bool,
    pub max_range: f32,
    pub barred_after: bool,
    pub radius: f32,
}

const fn attack(skill: u32, behavior: u32, kind: Kind, factor: f32) -> AttackSkill {
    AttackSkill {
        skill,
        behavior,
        kind,
        missile: !matches!(kind, Kind::Melee),
        added: 0.0,
        factor,
        critical: rules::fit(rules::Direction::CreatureOnPlayer, kind).critical,
        critical_size: rules::CRITICAL_BEFORE_ARMOR,
        drains: false,
        max_range: if matches!(kind, Kind::Melee) {
            2.0
        } else {
            MISSILE_RANGE
        },
        barred_after: true,
        radius: 0.0,
    }
}

/// Creatures' attack skills as they are known without their definitions: the behaviors are the
/// skills' own; the factors are 2017's (`Strong` 1.03 melee and 1.07 missile, `Stronger` 1.11
/// melee, over `Normal` against the same player), the missile `Stronger` and the harming and
/// poisoned missiles taken as their melee or `Strong` kin; and the harming missiles drain.
pub const CREATURE_ATTACKS: [AttackSkill; 9] = [
    attack(1845, 0x4100_0076, Kind::Melee, 1.0),
    attack(1846, 0x4100_0077, Kind::Melee, 1.03),
    attack(1847, 0x4100_0078, Kind::Melee, 1.11),
    attack(1861, 0x4100_0079, Kind::Missile, 1.0),
    attack(1865, 0x4100_007A, Kind::Missile, 1.07),
    attack(1866, 0x4100_007B, Kind::Missile, 1.11),
    AttackSkill {
        drains: true,
        ..attack(1860, 0x4100_007B, Kind::Missile, 1.07)
    },
    attack(1863, 0x4100_007B, Kind::Missile, 1.07),
    AttackSkill {
        drains: true,
        ..attack(1852, 0x4100_0075, Kind::Missile, 1.07)
    },
];

/// The weight of a special the world gives no `[[special]]` row: the mean of 2017's, fitted with
/// the rest (each special's chance at a generic monster's attack with it at hand, over 42,444
/// attacks).
pub const SHARED_SPECIAL_WEIGHT: f32 = 0.087;

/// The chance a creature uses a special at an attack, with the special at hand: the special's
/// weight (the world's `[[special]]` row, else [`SHARED_SPECIAL_WEIGHT`]), its chance at a generic
/// monster's attack, which a creature with no sub class moves by its AI class's term (`[[ai_class]]`
/// `special_term`, added on the logit scale; none for a class the world gives none).
#[must_use]
pub fn special_weight<S: Store>(
    s: &Server<S>,
    skill: u32,
    class: Option<u32>,
    generic: bool,
) -> f32 {
    let w = s
        .creature_rows
        .special_weights
        .get(&skill)
        .copied()
        .unwrap_or(SHARED_SPECIAL_WEIGHT);
    let term = if generic {
        None
    } else {
        class.and_then(|c| s.creature_rows.special_terms.get(&c).copied())
    };
    match term {
        Some(t) if w > 0.0 && w < 1.0 => {
            use dereth_primitives::num::math;
            1.0 / (1.0 + math::expf(-(math::logf(w / (1.0 - w)) + t)))
        }
        _ => w,
    }
}

/// How long a special's use bars it, in seconds (2017: the least gaps 39.6-40.2 s).
pub const SPECIAL_BARRED: f64 = 40.0;
/// The skills below this number are a creature's combat ratings, never used.
pub const FIRST_USED_SKILL: u32 = 100;

/// One of a creature's skills as an attack, from its definition (else the table above): none for a
/// skill that is no attack (a rating, a heal or buff on itself or its friends, one unknown).
#[must_use]
pub fn attack_skill(
    table: &crate::skills::SkillTable,
    skill: u32,
    rating: i64,
) -> Option<AttackSkill> {
    if skill < FIRST_USED_SKILL {
        return None;
    }
    let known = CREATURE_ATTACKS.iter().find(|a| a.skill == skill).copied();
    let Some(d) = table.skills.get(&skill) else {
        return known;
    };
    let Some(u) = d.usage.as_ref() else {
        return known;
    };
    if d.helps || crate::skill_use::helpful(u) || u.hooks.is_empty() || u.behavior_other[0] == 0 {
        return None;
    }
    let missile = u.hooks.iter().any(|h| h.missile != 0);
    #[allow(clippy::cast_precision_loss)]
    let added = u
        .hooks
        .iter()
        .map(|h| h.added_at(rating.max(1) as f32))
        .fold(0.0f32, f32::max);
    Some(AttackSkill {
        skill,
        behavior: u.behavior_other[0],
        kind: d
            .kind
            .unwrap_or(if missile { Kind::Missile } else { Kind::Melee }),
        missile,
        added,
        factor: 1.0,
        critical: d.critical_threshold.map_or(0.0, rules::skill_critical),
        critical_size: d.critical_modifier.unwrap_or(rules::CRITICAL_BEFORE_ARMOR),
        drains: known.is_some_and(|k| k.drains),
        max_range: u.max_range,
        barred_after: u.barred_after,
        radius: u
            .hooks
            .iter()
            .filter(|h| h.flags & crate::skill_use::hook_flag::AREA != 0)
            .map(|h| h.radius)
            .fold(0.0f32, f32::max),
    })
}

/// Whom a creature's special with an area reaches besides its target, a player: every other player
/// and every pet whose body is within `radius` of the target's (2017, the specials with an area:
/// the other players they reached a median 3 to 9 m from the target, 15 to 28 m from the creature;
/// 920 of 1,001 players within the radius of 1822's target reached; pets reached among them). A
/// player dead, dying, in portal space or protected is not reached. By id, players then pets.
#[must_use]
pub fn area_victims<S: Store>(
    s: &Server<S>,
    target: ConnId,
    radius: f32,
) -> (Vec<ConnId>, Vec<u64>) {
    let Some(center) = s.players.get(&target).map(|p| p.position) else {
        return (Vec::new(), Vec::new());
    };
    let body = crate::reach::PLAYER;
    let within = |at: &StoredPosition, b| {
        crate::reach::gap(&center, body, at, b).is_some_and(|d| d <= radius)
    };
    let mut players: Vec<ConnId> = s
        .players
        .iter()
        .filter(|(c, p)| {
            **c != target
                && p.dying.is_none()
                && p.trip.is_none()
                && !combat::is_dead(p)
                && !crate::effects::protected(p)
                && within(&p.position, body)
        })
        .map(|(c, _)| *c)
        .collect();
    players.sort_unstable();
    let mut pets: Vec<u64> = s
        .pets
        .by_id
        .keys()
        .copied()
        .filter(|id| {
            s.things.get(*id).is_some_and(|t| {
                t.alive()
                    && t.at
                        .as_ref()
                        .is_some_and(|at| within(at, crate::reach::body_of(s, *id)))
            })
        })
        .collect();
    pets.sort_unstable();
    (players, pets)
}

/// How often a creature draws one of its basic attacks against the others: for a creature of the
/// generic monsters' tables, the plain attack 4, `Strong` 2, any other 1; for any other creature,
/// all alike.
#[must_use]
pub fn basic_weight(stats: &CreatureStats, skill: u32) -> u32 {
    if stats.sub_class.is_none() {
        return 1;
    }
    match skill {
        1840 | 1845 | 1861 => 4,
        1841 | 1846 | 1865 => 2,
        _ => 1,
    }
}

/// Draw one of a creature's basic attacks by their weights.
pub fn pick_basic<S: Store>(
    s: &mut Server<S>,
    stats: &CreatureStats,
    from: &[AttackSkill],
) -> Option<AttackSkill> {
    let total: u32 = from.iter().map(|a| basic_weight(stats, a.skill)).sum();
    if total == 0 {
        return None;
    }
    let mut r = u32::try_from(s.rng.below(total as usize)).unwrap_or(0);
    for a in from {
        let w = basic_weight(stats, a.skill);
        if r < w {
            return Some(*a);
        }
        r -= w;
    }
    from.last().copied()
}

/// What a creature's effects add to its hits (a damage debuff a player put on it takes away; 2017:
/// a creature under Blood Loather hit a median 26 less, over 156 of them).
#[must_use]
pub fn effects_add<S: Store>(s: &Server<S>, id: u64) -> f32 {
    crate::effects::creature_modifier(s, id, crate::effects::DAMAGE_ADD).0
}

/// A creature's attack skills: its basic attacks and its specials, less those its behavior table
/// never uses (`skips`, the world's rows).
#[must_use]
pub fn attacks_of(
    table: &crate::skills::SkillTable,
    skips: &[u32],
    stats: &CreatureStats,
) -> (Vec<AttackSkill>, Vec<AttackSkill>) {
    let mut basic = Vec::new();
    let mut special = Vec::new();
    for (skill, rating) in stats.skills.iter().filter(|(k, _)| !skips.contains(k)) {
        if let Some(a) = attack_skill(table, *skill, *rating) {
            if stats.basic.contains(skill) {
                basic.push(a);
            } else {
                special.push(a);
            }
        }
    }
    (basic, special)
}

/// The special a creature uses at this attack, if any: of its specials within reach of the target,
/// not barred and paid for, none with the product of their misses ([`special_weight`]), else one
/// drawn by weight. Its use is recorded (it is barred for 40 s after) and paid for.
pub fn pick_special<S: Store>(
    s: &mut Server<S>,
    id: u64,
    specials: &[AttackSkill],
    distance: f32,
    now: f64,
) -> Option<AttackSkill> {
    let (barred, generic, class) = s
        .things
        .get(id)
        .and_then(|t| t.creature.as_ref())
        .map(|c| {
            (
                c.barred.clone(),
                c.stats.sub_class.is_some(),
                c.stats.super_class,
            )
        })
        .unwrap_or_default();
    let vigor = s.things.get(id).map_or(0, |t| t.vigor);
    let at_hand: Vec<AttackSkill> = specials
        .iter()
        .copied()
        .filter(|a| special_cost(s, id, a.skill) <= vigor)
        .filter(|a| distance <= a.max_range.max(crate::ai::MELEE_REACH) + SPECIAL_SLACK)
        .filter(|a| {
            !barred
                .iter()
                .any(|(k, until)| *k == a.skill && *until > now)
        })
        .collect();
    if at_hand.is_empty() {
        return None;
    }
    let weights: Vec<f32> = at_hand
        .iter()
        .map(|a| special_weight(s, a.skill, class, generic))
        .collect();
    let none: f32 = weights.iter().map(|w| 1.0 - w).product();
    if !s.rng.chance(1.0 - none) {
        return None;
    }
    let pick = at_hand[s.rng.weighted(weights.iter().copied())?];
    let cost = special_cost(s, id, pick.skill);
    if let Some(t) = s.things.get_mut(id) {
        t.vigor = (t.vigor - cost).max(0);
    }
    if let Some(c) = s.things.get_mut(id).and_then(|t| t.creature.as_mut()) {
        c.barred
            .retain(|(k, until)| *k != pick.skill && *until > now);
        if pick.barred_after {
            c.barred.push((pick.skill, now + SPECIAL_BARRED));
        }
    }
    Some(pick)
}

/// What a special costs a creature in vigor: its skill's share of the creature's greatest vigor, in
/// single precision and cut to a whole number, less the skill's own add (stored below nothing for a
/// cost), nothing below nothing.
#[must_use]
pub fn special_cost<S: Store>(s: &Server<S>, id: u64, skill: u32) -> i32 {
    let max = s.things.get(id).map_or(0, |t| t.max_vigor);
    s.skill_table
        .skills
        .get(&skill)
        .and_then(|d| d.usage.as_ref())
        .map_or(0, |u| {
            crate::skill_use::vigor_cost(u, max, crate::skill_use::HeldVigor::default()).max(0)
        })
}

/// How far beyond a special's greatest range a creature still uses it, in meters (2017: 0-5% of
/// uses lay beyond the range and 1.5 m, centre to centre).
pub const SPECIAL_SLACK: f32 = 1.5;

/// From this far a creature with a basic missile attack always shoots, in meters.
pub const SHOOTS_FROM: f32 = 15.0;
/// How far creatures' missiles reach, and how fast they fly, in meters and meters a second.
pub const MISSILE_RANGE: f32 = 40.0;
pub const MISSILE_SPEED: f32 = 35.0;
/// When the first shot comes after the creature engaged, in seconds (2017's median, 40 fights).
pub const FIRST_SHOT: f64 = 0.82;
/// How far a target must have moved since a shot for it to count as having moved away, in meters.
pub const MOVED: f32 = 1.0;
/// The story bits of a special: the attacker, its behavior, the skill (no basic-attack flag).
pub const SPECIAL: u32 = PhysicsStory::ATTACKER | PhysicsStory::BEHAVIOR | PhysicsStory::SKILL;
/// The story bits of a missile's arrival, told by its target: the attacker and the skill, with the
/// basic-attack flag for a basic attack.
pub const ARRIVAL: u32 = PhysicsStory::ATTACKER | PhysicsStory::SKILL | 0x20;
pub const SPECIAL_ARRIVAL: u32 = PhysicsStory::ATTACKER | PhysicsStory::SKILL;
/// A missile's hook: kind 3 with its target and the animation hook; with the weapon thrown from
/// (2017: 20,569 missile stories of creatures holding one named it).
pub const MISSILE_HOOK: u32 = 0x303;
pub const WEAPON_NAMED: u32 = 0x20;
/// How long after a skill's throw its next one goes, in seconds at a creature's usual time scale,
/// as its windup is (2017: the two
/// arrivals of 1792's double throws a median 1.02 and 1.39 s after it, 108 of them; 1848's 1.63
/// and 2.00 s, 22).
pub const THROW_APART: f32 = 0.37;
/// The drain of a harming missile: its effect on the target, and its story's hook (kind 1 with the
/// target, the result and the health change).
pub const DRAIN_FX: u32 = 0x4100_003E;
pub const DRAIN_HOOK: u32 = 0x981;

/// A kind of creature's shooting rhythm: the gap between its shots and its windup before a
/// missile leaves, each times its shot's time scale, in seconds (2017: the gap is about constant
/// so, and the windup measured at the median time scale 1.296).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rhythm {
    pub gap: f32,
    pub windup: f32,
}

/// A creature's shooting rhythm by its template, the world's rows', with the beetles' (Edge, Cutter
/// and Sharp Cutter Beetles: gap 1.63, windup 0.55 x 1.296) for one they give none of.
#[must_use]
pub fn rhythm(rows: &yalain_content::creature_rows::CreatureRows, template: u32) -> Rhythm {
    let r = rows
        .rhythms
        .get(&template)
        .copied()
        .unwrap_or(crate::creature_rows::OTHER_SHOOTERS);
    Rhythm {
        gap: r.gap,
        windup: r.windup * CREATURE_TIME_SCALE,
    }
}

/// A missile in flight: who loosed it at whom (character id), with which skill, whether a special,
/// where the target stood when it was loosed, and where it was loosed from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Flight {
    pub creature: u64,
    pub target: u64,
    pub skill: AttackSkill,
    pub special: bool,
    pub from: StoredPosition,
    pub loosed_at: Option<StoredPosition>,
}

/// A creature engages a player at a distance: does it shoot this fight?
pub fn chooses_to_shoot<S: Store>(s: &mut Server<S>, stats: &CreatureStats, distance: f32) -> bool {
    let (basic, _) = attacks_of(
        &s.skill_table,
        s.creature_rows.skips(stats.npc_table.as_deref()),
        stats,
    );
    if !basic.iter().any(|a| a.missile) {
        return false;
    }
    if distance >= SHOOTS_FROM {
        return true;
    }
    pick_basic(s, stats, &basic).is_some_and(|a| a.missile)
}

/// A creature that shoots this fight looses a missile at its target, when its attack is due.
pub fn shoot<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    id: u64,
    target: ConnId,
    clock: Clock,
) {
    let now = clock.local.0;
    let Some(t) = s.things.get(id) else { return };
    let Some(c) = t.creature.as_ref() else { return };
    if now < c.next_attack {
        return;
    }
    let template = t.template;
    let at = t.at;
    let (basic, specials) = attacks_of(
        &s.skill_table,
        s.creature_rows.skips(c.stats.npc_table.as_deref()),
        &c.stats,
    );
    let missiles: Vec<AttackSkill> = basic.into_iter().filter(|a| a.missile).collect();
    let distance = s
        .players
        .get(&target)
        .and_then(|p| at.and_then(|a| presence::distance(&a, &p.position)))
        .unwrap_or(0.0);
    if helps(s, net, id, clock) {
        return;
    }
    let special = pick_special(s, id, &specials, distance, now);
    let stats = s
        .things
        .get(id)
        .and_then(|t| t.creature.as_ref())
        .map(|c| c.stats.clone())
        .unwrap_or_default();
    let skill = match special {
        Some(a) => a,
        None => match pick_basic(s, &stats, &missiles) {
            Some(a) => a,
            None => return,
        },
    };
    if !skill.missile {
        let pace = combat::attack_pace(s, id);
        // A special that throws nothing strikes where the creature stands.
        if let Some(c) = s.things.get_mut(id).and_then(|t| t.creature.as_mut()) {
            c.next_attack = now
                + f64::from(rhythm(&s.creature_rows, template).gap / (CREATURE_TIME_SCALE * pace));
        }
        combat::creature_strike_with(s, net, id, target, skill, true, clock);
        return;
    }
    launch(s, net, id, target, skill, special.is_some(), clock);
}

/// A creature looses a missile (a basic attack's or a special's) at its target: the story, and the
/// missile's arrival after the windup and its flight. A skill whose definition throws from several
/// hooks looses one missile for each, each a hook of the story (a later one marked with its
/// index) and an arrival of its own, [`THROW_APART`] after the one before (2017: 1,326 creature
/// stories of 1792, 1848, 857 and 1015, each with two missile hooks, the second 0x703 or 0x723;
/// two arrivals, each with one result, for 106 of 1792's at the session's player).
pub fn launch<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    id: u64,
    target: ConnId,
    skill: AttackSkill,
    special: bool,
    clock: Clock,
) {
    let now = clock.local.0;
    let Some(template) = s.things.get(id).map(|t| t.template) else {
        return;
    };
    let special = special.then_some(skill);
    let Some(p) = s.players.get(&target) else {
        return;
    };
    let (pid, from) = (p.character, p.position);
    let pace = combat::attack_pace(s, id);
    let ts = CREATURE_TIME_SCALE * s.rng.between(0.9, 1.1) * pace;
    let rh = rhythm(&s.creature_rows, template);
    // A missile's gap goes with its time scale; a bolt of magic keeps its own (2017: the gaps of
    // magic under an effect slowing attacks 1.33 s against 1.28 s).
    let paced = if skill.kind == Kind::Magic {
        ts / pace
    } else {
        ts
    };
    if let Some(c) = s.things.get_mut(id).and_then(|t| t.creature.as_mut()) {
        c.next_attack = now + f64::from(rh.gap / paced);
    }
    let mut b = BehaviorParams {
        bits: SWING_BEHAVIOR_BITS,
        behavior: skill.behavior,
        time_scale: ts,
        ..BehaviorParams::default()
    };
    b.words[7] = 1;
    b.words[9] = 0;
    // Each throw: its hook's index among the skill's and its animation hook.
    let mut throws: Vec<(u32, u32)> = s
        .skill_table
        .skills
        .get(&skill.skill)
        .and_then(|d| d.usage.as_ref())
        .map(|u| {
            crate::skill_use::throws_of(u)
                .into_iter()
                .map(|(i, h)| (u32::try_from(i).unwrap_or(0), h.anim_hook.max(1)))
                .collect()
        })
        .unwrap_or_default();
    if throws.is_empty() {
        throws.push((0, 1));
    }
    let weapon = crate::arms::held(s, id);
    let mut hooks = vec![StoryHook {
        bits: combat::hook::OPENING,
        ..StoryHook::default()
    }];
    for (k, &(index, anim_hook)) in throws.iter().enumerate() {
        let mut missile = StoryHook {
            bits: MISSILE_HOOK,
            target: pid,
            ..StoryHook::default()
        };
        // A creature holding a weapon names it.
        if let Some(w) = weapon {
            missile.bits |= WEAPON_NAMED;
            missile.weapon = w;
        }
        missile.numbers[1] = anim_hook.cast_signed();
        if k > 0 {
            missile.bits |= crate::skill_use::story_hook::ATTACK_HOOK;
            missile.numbers[2] = index.cast_signed();
        }
        hooks.push(missile);
    }
    let launch = ServerMessage::DoStory(ObjectEvent {
        sender: crate::inventory::visual(s, id),
        action: PhysicsStory {
            bits: if special.is_some() {
                SPECIAL
            } else {
                combat::BASIC_SWING
            },
            attacker: id,
            attacker_behavior: b,
            attack_context: 0,
            skill: skill.skill,
            hooks,
        },
    });
    to_all_seeing(s, net, id, &launch);
    let loosed_at = s.things.get(id).and_then(|t| t.at);
    let d = loosed_at
        .and_then(|at| presence::distance(&at, &from))
        .unwrap_or(0.0);
    let flight = rh.windup / ts + d / MISSILE_SPEED;
    for k in 0..throws.len() {
        #[allow(clippy::cast_precision_loss)]
        let later = k as f32 * THROW_APART * CREATURE_TIME_SCALE / ts;
        s.after(
            clock,
            f64::from(flight + later),
            Timer::Missile(Flight {
                creature: id,
                target: pid,
                skill,
                special: special.is_some(),
                from,
                loosed_at,
            }),
        );
    }
}

/// A missile arrives: its target tells how it came out, then each player it hurt its health. A
/// special with an area reaches every other player and pet in it too, all told in the target's one
/// story (2017: one arrival naming each target, each with its health change).
pub fn arrives<S: Store>(s: &mut Server<S>, net: &mut ServerNet, f: Flight, clock: Clock) {
    let Some(world) = s
        .players
        .iter()
        .find(|(_, p)| p.character == f.target)
        .map(|(c, _)| *c)
    else {
        return;
    };
    let Some(t) = s.things.get(f.creature) else {
        return;
    };
    let Some(c) = t.creature.as_ref() else { return };
    let (level, stats) = (c.stats.level, c.stats.clone());
    let Some(p) = s.players.get(&world) else {
        return;
    };
    if combat::is_dead(p) || p.trip.is_some() {
        return;
    }
    let moved = presence::distance(&f.from, &p.position).is_none_or(|d| d > MOVED);
    let mut players = vec![lands_on_player(s, &f, &stats, world, moved)];
    let (others, pets) = if f.special && f.skill.radius > 0.0 {
        area_victims(s, world, f.skill.radius)
    } else {
        (Vec::new(), Vec::new())
    };
    players.extend(
        others
            .into_iter()
            .map(|c| lands_on_player(s, &f, &stats, c, false)),
    );
    let pets: Vec<(u64, Outcome, crate::effects::Harm)> = pets
        .into_iter()
        .map(|pet| {
            let (o, h) = combat::creature_blow_on_thing(s, f.creature, &stats, f.skill, pet);
            (pet, o, h)
        })
        .collect();
    let swing = |target, outcome, harm: crate::effects::Harm| combat::Swing {
        skill: f.skill.skill,
        behavior: f.skill.behavior,
        time_scale: 1.0,
        target,
        outcome,
        damage: harm.left,
        back: harm.back,
    };
    let hooks = players
        .iter()
        .flatten()
        .map(|l| swing(l.character, l.outcome, l.harm).hook())
        .chain(pets.iter().map(|&(t, o, h)| swing(t, o, h).hook()))
        .collect();
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let told = ServerMessage::DoStory(combat::player_event(
        p,
        PhysicsStory {
            bits: if f.special { SPECIAL_ARRIVAL } else { ARRIVAL },
            attacker: f.creature,
            skill: f.skill.skill,
            hooks,
            ..PhysicsStory::default()
        },
    ));
    to_all_seeing(s, net, f.target, &told);
    for l in players.into_iter().flatten() {
        if l.outcome.landed() {
            let (share, was) = combat::lose_health(s, net, l.world, l.harm.left);
            combat::wounded(s, net, l.character, share, was);
        }
        if l.outcome.landed() && f.skill.drains {
            drain(s, net, l.world, f.creature, level);
        }
        combat::after_hurt(s, net, l.world, f.creature, clock);
        if l.outcome.landed() {
            crate::effects::answer_attack(s, net, l.world, f.creature, clock);
            crate::effects::harm_back(s, net, f.creature, l.character, l.harm.back, clock);
            combat::creature_blow_landed(s, net, f.creature, l.world, clock);
            combat::creature_skill_landed(s, net, f.creature, f.skill, l.world, clock);
        }
    }
    for (pet, outcome, harm) in pets {
        if outcome.landed() {
            crate::pets::struck(s, net, pet, (f.creature, f.skill.skill), harm.left, clock);
            crate::effects::harm_back(s, net, f.creature, pet, harm.back, clock);
        }
    }
}

/// How a creature's missile came out on one player it reached, its health still to be taken.
struct Landing {
    world: ConnId,
    character: u64,
    outcome: Outcome,
    harm: crate::effects::Harm,
}

/// A creature's missile reaches a player: rolled (`moved` when the player has left where it was
/// aimed at), the damage through its armor and its effects; none for a player not in the world,
/// dead or in portal space.
fn lands_on_player<S: Store>(
    s: &mut Server<S>,
    f: &Flight,
    stats: &CreatureStats,
    world: ConnId,
    moved: bool,
) -> Option<Landing> {
    let p = s.players.get(&world)?;
    if combat::is_dead(p) || p.trip.is_some() {
        return None;
    }
    let (character, my_level) = (p.character, crate::examine::level_of(&p.qualities));
    let now_at = p.position;
    // A target that has gone out of sight of where the missile was loosed from since is not reached:
    // the missile meets what stands between them, and misses.
    let reached = !moved
        || f.loosed_at.is_none_or(|from| {
            crate::ai::sees(s.ground.as_deref(), s.walls.as_mut(), &from, &now_at)
        });
    let critical = f.skill.critical + combat::creature_critical_bonus(s, f.creature);
    let ratings = crate::ratings::of(
        s,
        f.skill.skill,
        crate::ratings::Rater::Thing(f.creature),
        crate::ratings::Rater::Player(world),
    );
    let outcome = if reached {
        rules::roll_missile(&mut s.rng, ratings, moved, critical)
    } else {
        Outcome::Miss
    };
    let more = effects_add(s, f.creature);
    let damage = if outcome.landed() {
        let from = s.things.get(f.creature).and_then(|t| t.at);
        let armor = combat::armor_against(s, world, from.as_ref());
        rules::creature_hit_on_player(
            &mut s.rng,
            f.skill.kind,
            rules::Natural {
                damage: stats.natural_damage,
                variance: stats.natural_variance,
                added: f.skill.added + more,
                factor: f.skill.factor,
                threshold: rules::threshold_of(stats),
                critical: f.skill.critical_size,
            },
            my_level,
            armor,
            outcome == Outcome::Critical,
        )
    } else {
        0
    };
    let harm = if outcome.landed() {
        crate::effects::answer_harm(s, crate::effects::On::Player(world), damage, false)
    } else {
        crate::effects::Harm::default()
    };
    Some(Landing {
        world,
        character,
        outcome,
        harm,
    })
}

/// How much a harming missile's drain takes at the attacker's level: 2 at level 1 to 150 at level
/// 50, along a line (the effect's own scale; 2017: 14 at level 5, 7 of 7).
#[must_use]
pub fn drain_amount(level: i32) -> i32 {
    #[allow(clippy::cast_precision_loss)]
    let l = level.clamp(1, 50) as f32;
    dereth_primitives::num::to_i32((2.0 + (l - 1.0) * 148.0 / 49.0).floor())
}

/// A harming missile's drain on the player it hit: the effect, a story of the drain, the health.
fn drain<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    attacker: u64,
    level: i32,
) {
    let amount = drain_amount(level);
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let id = p.character;
    let fx = ServerMessage::DoFx(combat::player_event(
        p,
        Fx {
            fx: DRAIN_FX,
            scalar: 1.0,
        },
    ));
    let mut h = StoryHook {
        bits: DRAIN_HOOK,
        target: id,
        ..StoryHook::default()
    };
    h.numbers[0] = Outcome::Hit.code().cast_signed();
    h.numbers[3] = -amount;
    let story = ServerMessage::DoStory(combat::player_event(
        p,
        PhysicsStory {
            bits: PhysicsStory::ATTACKER,
            attacker,
            hooks: vec![h],
            ..PhysicsStory::default()
        },
    ));
    to_all_seeing(s, net, id, &fx);
    to_all_seeing(s, net, id, &story);
    let health = s
        .players
        .get(&world)
        .and_then(|p| quals::int(&p.qualities, quality::HEALTH))
        .unwrap_or(0);
    if health > 0 {
        let (share, was) = combat::lose_health(s, net, world, amount);
        combat::wounded(s, net, id, share, was);
    }
}

/// One of a creature's skills it uses on itself: a heal, or a buff.
#[derive(Debug, Clone, PartialEq)]
pub struct HelpSkill {
    pub skill: u32,
    /// What it plays on itself, and on another.
    pub behavior: u32,
    pub behavior_other: u32,
    /// How far from it another may be.
    pub range: f32,
    /// The effects it puts on (their saved states), at its rating.
    pub effects: Vec<u32>,
    pub rating: i64,
    /// It heals (else it is a buff).
    pub heals: bool,
    /// It restores its user's vigor, and helps no other way.
    pub restores: bool,
}

/// The chance a creature at or below its warning level of health heals itself at one of its turns
/// with its heal free: one of the generic monsters' tables (one with a sub class), and any other
/// (2017, health by its last reading, every one of the first naming 0.65 and none of the others
/// naming a level: 888 of 1,468 turns under 0.65, 0.58 to 0.63; 94 of 424 under 0.4, 0.19 to 0.26).
pub const GENERIC_HEAL_CHANCE: f32 = 0.6;
pub const OLDER_HEAL_CHANCE: f32 = 0.21;

/// The chance a creature at or below its warning level heals itself at a turn.
#[must_use]
pub fn heal_chance(generic: bool) -> f32 {
    if generic {
        GENERIC_HEAL_CHANCE
    } else {
        OLDER_HEAL_CHANCE
    }
}
/// The chance a creature under its vigor warning level restores its vigor at one of its turns, with
/// a restore free (2017: 14 of 28 turns, drudges' alone).
pub const RESTORE_CHANCE: f32 = 0.5;
/// The chance a creature buffs itself at a turn, by how many of its buffs are free (one, two or
/// more), at its first turn of a fight and at a later one; one draw for the turn, the buff drawn
/// evenly from those free (2017, every action a turn, a fight's first after 20 s without one, the
/// wounded turns its heal takes left out, by distinct event: at the first turn 270 of 1,715 with
/// one free, 292 of 1,103 with two or more; later 1,234 of 13,783 with one, 0.090, and 620 of 3,863
/// with two or more).
pub const BUFF_FIRST_CHANCE: [f32; 2] = [0.155, 0.249];
pub const BUFF_CHANCE: [f32; 2] = [0.090, 0.157];

/// The chance a creature buffs itself at a turn with so many of its buffs free.
#[must_use]
pub fn buff_chance(free: usize, first: bool) -> f32 {
    let table = if first {
        BUFF_FIRST_CHANCE
    } else {
        BUFF_CHANCE
    };
    free.checked_sub(1)
        .map_or(0.0, |i| table[i.min(table.len() - 1)])
}
/// When a heal or a buff takes effect after its story, and when the creature's next attack comes,
/// in seconds (2017: a heal told a median 0.96 s after its story, 2,925 heals; the next attack a
/// median 2.28 s after it).
pub const HELP_LANDS: f64 = 0.96;
pub const HELP_RECOVERY: f64 = 2.3;
/// The story bits of a creature's hook on itself: kind 1 with its target, the result and the
/// animation hook.
pub const SELF_HOOK: u32 = 0x381;

/// A creature's heals, buffs and restores of vigor: its skills (numbered 100 and up) that help, with
/// their effects. A restore of focus alone is none of them.
#[must_use]
pub fn helps_of(
    table: &crate::skills::SkillTable,
    effects: &crate::effect_defs::EffectTable,
    stats: &CreatureStats,
) -> Vec<HelpSkill> {
    stats
        .skills
        .iter()
        .filter(|(skill, _)| *skill >= FIRST_USED_SKILL)
        .filter_map(|(skill, rating)| {
            let d = table.skills.get(skill)?;
            let u = d.usage.as_ref()?;
            if !d.helps || u.target_effects.is_empty() {
                return None;
            }
            let restores_only = |vital: u32| {
                u.target_effects.iter().all(|e| {
                    matches!(
                        effects.get(*e).map(|d| &d.kind),
                        Some(crate::effect_defs::Kind::InstantVital { flags, .. })
                            if flags & crate::effect_defs::vital::HEALTH == 0 && flags & vital != 0
                    )
                })
            };
            let restores = restores_only(crate::effect_defs::vital::VIGOR);
            if !restores && restores_only(crate::effect_defs::vital::FOCUS) {
                return None;
            }
            let heals = u.target_effects.iter().any(|e| {
                matches!(
                    effects.get(*e).map(|d| &d.kind),
                    Some(crate::effect_defs::Kind::InstantVital { flags, .. })
                        if flags & crate::effect_defs::vital::HEALTH != 0
                )
            });
            Some(HelpSkill {
                skill: *skill,
                behavior: u.behavior_self[0],
                behavior_other: u.behavior_other[0],
                range: u.max_range,
                effects: u.target_effects.clone(),
                rating: *rating,
                heals,
                restores,
            })
        })
        .collect()
}

/// The chance a creature that used a heal or buff on one of its kin at its last turn uses it on
/// the same one again, while it is near (2017: protectors' next turns, 196 of 444 by distinct
/// event, alike with one to four kin near).
pub const KIN_REPEAT: f32 = 0.43;
/// The chance a protector buffs one of its kin at a turn, by how many of its kin in a fight are
/// within the buff's range (one, two, three, four or more), at its first turn of a fight and at a
/// later one; one draw for the turn, not one for each kin (2017's protectors, turns not just after a
/// kin buff, by distinct event: at the first turn 60 of 344, 39 of 139, 24 of 80, 33 of 107; later
/// 40 of 3,303, 43 of 1,554, 24 of 865, 51 of 990).
pub const KIN_BUFF_FIRST: [f32; 4] = [0.19, 0.24, 0.29, 0.32];
pub const KIN_BUFF_LATER: [f32; 4] = [0.016, 0.028, 0.03, 0.043];

/// The chance a protector buffs one of its kin at a turn with so many in a fight near.
#[must_use]
pub fn kin_buff_chance(kin: usize, first: bool) -> f32 {
    let table = if first {
        KIN_BUFF_FIRST
    } else {
        KIN_BUFF_LATER
    };
    kin.checked_sub(1)
        .map_or(0.0, |i| table[i.min(table.len() - 1)])
}
/// The share of health at or below which a healer heals its most wounded kin, and its chance at a
/// turn (2017: 12 kin heals in 143 turns with a kin at 0.4 or less, 1 in 85 between 0.4 and 0.65).
pub const KIN_WOUNDED_AT: f32 = 0.4;
pub const KIN_HEAL_CHANCE: f32 = 0.08;
/// The generic monsters' sub class of protectors, which buff their kin (2017: 432 of 433 buffs on
/// kin); the world's rows say which behavior tables' healers heal theirs.
pub const PROTECTOR: u32 = 7;
/// The chance a heal's or buff's hook takes a point of its target's health (2017: 1,771 of 3,603
/// self helps without an armor debuff named a loss of 1, the rest none, alike wounded or whole).
pub const HELP_TAKES_A_POINT: f32 = 0.5;
/// The health change field of a story's hook.
pub const HEALTH_CHANGE: u32 = 0x800;

/// A creature's kin within a distance: alive, of its friend table, with each one's share of
/// health and whether it is in a fight.
fn kin_near<S: Store>(s: &Server<S>, id: u64, range: f32) -> Vec<(u64, f32, bool)> {
    let Some((at, table)) = s
        .things
        .get(id)
        .and_then(|t| Some((t.at?, t.creature.as_ref()?.stats.friend_table?)))
    else {
        return Vec::new();
    };
    s.things
        .by_id
        .values()
        .filter(|t| t.id != id && t.alive())
        .filter_map(|t| {
            let c = t.creature.as_ref()?;
            let d = presence::distance(&at, &t.at?)?;
            #[allow(clippy::cast_precision_loss)]
            let share = t.health as f32 / t.max_health.max(1) as f32;
            (c.stats.friend_table == Some(table) && d <= range).then_some((
                t.id,
                share,
                c.mode == crate::things::Mode::Chasing,
            ))
        })
        .collect()
}

/// At one of a creature's turns, it may heal or buff instead of attacking: again on the kin it
/// helped at its last turn, by its chance; a healer its most wounded kin when that one is badly
/// hurt; a protector a kin of its in a fight, by one chance for how many are near, the kin drawn
/// evenly; else itself, a heal when its health is at or below
/// its warning level, a buff otherwise, by one chance for how many of its buffs are free. Each by
/// its chance; a use on itself is barred 40 s after,
/// a use on kin bars nothing. Returns whether it did (its turn is spent).
pub fn helps<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64, clock: Clock) -> bool {
    let now = clock.local.0;
    let Some(t) = s.things.get(id) else {
        return false;
    };
    let Some(c) = t.creature.as_ref() else {
        return false;
    };
    #[allow(clippy::cast_precision_loss)]
    let share = t.health as f32 / t.max_health.max(1) as f32;
    #[allow(clippy::cast_precision_loss)]
    let short_of_vigor =
        t.max_vigor > 0 && (t.vigor as f32) < c.stats.vigor_warning_level * t.max_vigor as f32;
    let first = c.actions == 0;
    let warning = c.stats.warning_level;
    let generic = c.stats.sub_class.is_some();
    let protector = c.stats.sub_class == Some(PROTECTOR);
    let healer = s.creature_rows.heals_kin(c.stats.npc_table.as_deref());
    let barred = c.barred.clone();
    let last = c.kin_helped;
    let all = helps_of(&s.skill_table, &s.effect_table, &c.stats);
    if let Some(c) = s.things.get_mut(id).and_then(|t| t.creature.as_mut()) {
        c.actions = c.actions.saturating_add(1);
        c.kin_helped = None;
    }
    if all.is_empty() {
        return false;
    }
    // Again on the same kin.
    if let Some((skill, kin)) = last {
        let h = all.iter().find(|h| h.skill == skill).cloned();
        if let Some(h) = h {
            let near = kin_near(s, id, h.range).iter().any(|k| k.0 == kin);
            if near && s.rng.chance(KIN_REPEAT) {
                use_help(s, net, id, &h, kin, clock);
                return true;
            }
        }
    }
    let wounded = share <= warning;
    // A healer whole enough heals its most wounded kin.
    if healer && !wounded {
        if let Some(h) = all.iter().find(|h| h.heals).cloned() {
            let worst = kin_near(s, id, h.range)
                .into_iter()
                .filter(|k| k.1 <= KIN_WOUNDED_AT)
                .min_by(|a, b| a.1.total_cmp(&b.1));
            if let Some((kin, _, _)) = worst {
                if s.rng.chance(KIN_HEAL_CHANCE) {
                    use_help(s, net, id, &h, kin, clock);
                    return true;
                }
            }
        }
    }
    // A protector buffs a kin of its in a fight.
    if protector {
        let buffs: Vec<HelpSkill> = all
            .iter()
            .filter(|h| !h.heals && !h.restores)
            .cloned()
            .collect();
        if !buffs.is_empty() {
            let h = buffs[s.rng.below(buffs.len())].clone();
            let kin: Vec<u64> = kin_near(s, id, h.range)
                .into_iter()
                .filter(|k| k.2)
                .map(|k| k.0)
                .collect();
            if !kin.is_empty() && s.rng.chance(kin_buff_chance(kin.len(), first)) {
                let k = kin[s.rng.below(kin.len())];
                use_help(s, net, id, &h, k, clock);
                return true;
            }
        }
    }
    let free: Vec<HelpSkill> = all
        .into_iter()
        .filter(|h| {
            !barred
                .iter()
                .any(|(k, until)| *k == h.skill && *until > now)
        })
        .collect();
    // A wounded one with a heal free, or one short of vigor with a restore free, takes the turn for
    // it by its chance; else a buff by its own.
    let heal = if wounded {
        free.iter().find(|h| h.heals).cloned()
    } else {
        None
    };
    let restore = if short_of_vigor {
        free.iter().find(|h| h.restores).cloned()
    } else {
        None
    };
    let pick = match (heal, restore) {
        (Some(h), _) => s.rng.chance(heal_chance(generic)).then_some(h),
        (None, Some(h)) => s.rng.chance(RESTORE_CHANCE).then_some(h),
        (None, None) => {
            let buffs: Vec<&HelpSkill> = free.iter().filter(|h| !h.heals && !h.restores).collect();
            if buffs.is_empty() {
                None
            } else {
                s.rng
                    .chance(buff_chance(buffs.len(), first))
                    .then(|| buffs[s.rng.below(buffs.len())].clone())
            }
        }
    };
    let Some(h) = pick else {
        return false;
    };
    if let Some(c) = s.things.get_mut(id).and_then(|t| t.creature.as_mut()) {
        c.barred.retain(|(k, until)| *k != h.skill && *until > now);
        c.barred.push((h.skill, now + SPECIAL_BARRED));
    }
    use_help(s, net, id, &h, id, clock);
    true
}

/// A creature uses a heal or buff on itself or one of its kin: its story (its behavior, the skill,
/// the opening hook and a hit on the one it helps, which loses a point of health one time in two),
/// its next attack 2.3 s after, the effect a second later.
fn use_help<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    id: u64,
    h: &HelpSkill,
    on: u64,
    clock: Clock,
) {
    let now = clock.local.0;
    if let Some(c) = s.things.get_mut(id).and_then(|t| t.creature.as_mut()) {
        c.next_attack = now + HELP_RECOVERY;
        if on != id {
            c.kin_helped = Some((h.skill, on));
        }
    }
    let ts = CREATURE_TIME_SCALE * s.rng.between(0.9, 1.1);
    let mut b = BehaviorParams {
        bits: SWING_BEHAVIOR_BITS,
        behavior: if on == id {
            h.behavior
        } else {
            h.behavior_other
        },
        time_scale: ts,
        ..BehaviorParams::default()
    };
    b.words[7] = 1;
    b.words[9] = 0;
    let mut hook = StoryHook {
        bits: SELF_HOOK,
        target: on,
        ..StoryHook::default()
    };
    hook.numbers[0] = Outcome::Hit.code().cast_signed();
    hook.numbers[1] = 1;
    let point = s.rng.chance(HELP_TAKES_A_POINT);
    if point {
        hook.bits |= HEALTH_CHANGE;
        hook.numbers[3] = -1;
        if let Some(t) = s.things.get_mut(on).filter(|t| t.health > 1) {
            t.health -= 1;
        }
    }
    let story = ServerMessage::DoStory(ObjectEvent {
        sender: crate::inventory::visual(s, id),
        action: PhysicsStory {
            bits: SPECIAL,
            attacker: id,
            attacker_behavior: b,
            attack_context: 0,
            skill: h.skill,
            hooks: vec![
                StoryHook {
                    bits: combat::hook::OPENING,
                    ..StoryHook::default()
                },
                hook,
            ],
        },
    });
    to_all_seeing(s, net, id, &story);
    s.after(
        clock,
        HELP_LANDS,
        Timer::CreatureHelps {
            creature: id,
            skill: h.skill,
            on,
        },
    );
}

/// A creature's heal or buff takes effect on itself or the kin it was used on, at its rating in
/// the skill.
pub fn helped<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    id: u64,
    skill: u32,
    on: u64,
    clock: Clock,
) {
    let Some(c) = s
        .things
        .get(id)
        .filter(|t| t.alive())
        .and_then(|t| t.creature.as_ref())
    else {
        return;
    };
    let Some(h) = helps_of(&s.skill_table, &s.effect_table, &c.stats)
        .into_iter()
        .find(|h| h.skill == skill)
    else {
        return;
    };
    #[allow(clippy::cast_precision_loss)]
    let spellcraft = h.rating.max(1) as f32;
    for state in h.effects {
        crate::effects::apply_on(
            s,
            net,
            crate::effects::On::Creature(on),
            state,
            crate::effects::Cast {
                caster: id,
                spellcraft,
                ..crate::effects::Cast::default()
            },
            clock,
        );
    }
}
