//! Creatures' attacks beyond the plain melee strike: missiles, and the specials creatures use now
//! and then.
//!
//! **Which attacks a creature has** are its template's starting skills (those numbered 100 and up:
//! the lower ones are its combat ratings), each read from the skill's own definition: the behavior
//! it plays on another, whether it throws a missile (a hook naming one: a missile or a bolt of
//! magic, which arrives and is told by its target) or strikes where it stands, its kind by its
//! name's family, the damage its hook adds at the creature's rating in it, its greatest range, and
//! whether a use bars it for a while. Those marked as the template's basic attacks it uses at will;
//! its other attack skills are its specials; skills it uses on itself or its friends (heals, buffs)
//! are not attacks. A creature with a basic missile attack chooses how it fights as it engages:
//! from 15 m or more it shoots; nearer, it picks one of its basic attacks at random, and a missile
//! means it shoots this fight, a melee attack that it closes in (2017: 3 of 141 fights mixed the
//! two).
//!
//! **Shooting.** It stands where it is, facing its target, and shoots when its attack is due (the
//! first about 0.8 s after it engaged), running in only while the target is beyond its missiles'
//! 40 m; a target that comes into melee reach is struck instead for the rest of the fight. Each
//! shot is a story from the creature (its behavior and time scale, the skill, the missile's hook
//! naming the target); no missile object is made. The missile arrives after the creature's windup
//! and its flight at 35 m/s, and the target tells the outcome: a story from the target with the
//! hit, the evade or the miss, its health first when it is hurt. A target that has moved since the
//! shot is often missed.
//!
//! **Specials.** At each attack, a special within its range and not barred is used instead of a
//! basic attack by its chance: about one attack in ten for each special at hand (2017: 0.104,
//! 0.138, 0.207 with one, two, three), the start area's two by their own measured chances. A use
//! bars it for 40 s (2017: 1,013 of 1,027 gaps between one creature's uses 38 s or more), so most
//! fights see it once. It is told like a basic attack without the basic-attack flag; a special that
//! throws nothing strikes where the creature stands. A harming missile's hit also drains the
//! target (an instant vital effect: its effect, then a story of the drain).
//!
//! **Basic attacks** are drawn at each attack among those that fit the distance: evenly, or, for a
//! creature of the generic monsters' tables (one with a sub class: minion, protector, conqueror,
//! paragon), the plain attack four times as often as the most others and `Strong` twice (2017: such
//! creatures' melee plain, `Strong` and `Stronger` 0.546, 0.283, 0.171 over 16,117 attacks; plain
//! and `Strong` 0.663 and 0.337 over 7,790; the other creatures' 0.331, 0.347, 0.322 over 9,426).
//!
//! **Heals and buffs.** At one of its turns, a creature at or below its warning level of health
//! heals itself about one time in two; one with a buff uses it about one time in three at its first
//! turn of a fight and one in eight after. Either takes the turn: its story on itself (its own
//! behavior, the skill, a hit on itself), the effect a second later (a heal shows its appearance
//! and the creature tells its health back), the next attack 2.3 s after the story; a use bars the
//! skill 40 s.
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
/// its kind (for a skill without a definition), the chance a creature uses it at an attack when it
/// is a special (0: the general one), whether its hit drains the target too, its greatest range
/// and whether a use bars it for a while.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AttackSkill {
    pub skill: u32,
    pub behavior: u32,
    pub kind: Kind,
    pub missile: bool,
    pub added: f32,
    pub factor: f32,
    pub special_chance: f32,
    pub drains: bool,
    pub max_range: f32,
    pub barred_after: bool,
}

const fn attack(skill: u32, behavior: u32, kind: Kind, factor: f32) -> AttackSkill {
    AttackSkill {
        skill,
        behavior,
        kind,
        missile: !matches!(kind, Kind::Melee),
        added: 0.0,
        factor,
        special_chance: 0.0,
        drains: false,
        max_range: if matches!(kind, Kind::Melee) {
            2.0
        } else {
            MISSILE_RANGE
        },
        barred_after: true,
    }
}

/// Creatures' attack skills as they are known without their definitions: the behaviors are the
/// skills' own; the factors are 2017's (`Strong` 1.03 melee and 1.07 missile, `Stronger` 1.11
/// melee, over `Normal` against the same player), the missile `Stronger` and the harming and
/// poisoned missiles taken as their melee or `Strong` kin. The specials' chances are 2017's per
/// attack until used, kept for the skills with their definitions too (the poisoned missile 0.13 of
/// the wasps' attacks, 14 of 108; the spreading harm 0.34 of the armoredillos', 22 of 65); and the
/// harming missiles drain.
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
    AttackSkill {
        special_chance: 0.13,
        ..attack(1863, 0x4100_007B, Kind::Missile, 1.07)
    },
    AttackSkill {
        special_chance: 0.34,
        drains: true,
        ..attack(1852, 0x4100_0075, Kind::Missile, 1.07)
    },
];

/// The chance a creature uses one of its specials at an attack, for each special at hand (2017:
/// 0.104 with one, 0.138 with two, 0.207 with three, over 56,499 attacks).
pub const SPECIAL_CHANCE: f32 = 0.10;
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
        special_chance: known.map_or(0.0, |k| k.special_chance),
        drains: known.is_some_and(|k| k.drains),
        max_range: u.max_range,
        barred_after: u.barred_after,
    })
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

/// A creature's attack skills: its basic attacks and its specials.
#[must_use]
pub fn attacks_of(
    table: &crate::skills::SkillTable,
    stats: &CreatureStats,
) -> (Vec<AttackSkill>, Vec<AttackSkill>) {
    let mut basic = Vec::new();
    let mut special = Vec::new();
    for (skill, rating) in &stats.skills {
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

/// The special a creature uses at this attack, if any: of its specials within reach of the target
/// and not barred, one by its own measured chance, else by the general chance for each at hand,
/// drawn among them. Its use is recorded (it is barred for 40 s after).
pub fn pick_special<S: Store>(
    s: &mut Server<S>,
    id: u64,
    specials: &[AttackSkill],
    distance: f32,
    now: f64,
) -> Option<AttackSkill> {
    let barred = s
        .things
        .get(id)
        .and_then(|t| t.creature.as_ref())
        .map(|c| c.barred.clone())
        .unwrap_or_default();
    let at_hand: Vec<AttackSkill> = specials
        .iter()
        .copied()
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
    let own: Vec<AttackSkill> = at_hand
        .iter()
        .copied()
        .filter(|a| a.special_chance > 0.0)
        .collect();
    let pick = if own.is_empty() {
        #[allow(clippy::cast_precision_loss)]
        let p = (SPECIAL_CHANCE * at_hand.len() as f32).min(1.0);
        s.rng.chance(p).then(|| at_hand[s.rng.below(at_hand.len())])
    } else {
        own.iter().copied().find(|a| s.rng.chance(a.special_chance))
    }?;
    if let Some(c) = s.things.get_mut(id).and_then(|t| t.creature.as_mut()) {
        c.barred
            .retain(|(k, until)| *k != pick.skill && *until > now);
        if pick.barred_after {
            c.barred.push((pick.skill, now + SPECIAL_BARRED));
        }
    }
    Some(pick)
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

/// The start area's shooters' rhythms by template: the beetles' (Edge, Cutter and Sharp Cutter
/// Beetles: gap 1.63, windup 0.55 x 1.296) for any other; the drudges' (Outcast, Downtrodden: 2.37,
/// 0.79 x 1.296); the wasps' (Green Phyntos, Red Soldier: 1.33, 0.45 x 1.296); the Reedshark
/// Runt's windup 0.98 x 1.296; the Sandy Armoredillo's (3.58, 1.14 x 1.296).
#[must_use]
pub fn rhythm(template: u32) -> Rhythm {
    let r = |gap: f32, windup: f32| Rhythm {
        gap,
        windup: windup * CREATURE_TIME_SCALE,
    };
    match template {
        0x4700_2398 | 0x4700_23A0 => r(2.37, 0.79),
        0x4700_2963 | 0x4700_296C => r(1.33, 0.45),
        0x4700_29EE => r(1.63, 0.98),
        0x4700_21BE => r(3.58, 1.14),
        _ => r(1.63, 0.55),
    }
}

/// A missile in flight: who loosed it at whom (character id), with which skill, whether a special,
/// and where the target stood when it was loosed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Flight {
    pub creature: u64,
    pub target: u64,
    pub skill: AttackSkill,
    pub special: bool,
    pub from: StoredPosition,
}

/// A creature engages a player at a distance: does it shoot this fight?
pub fn chooses_to_shoot<S: Store>(s: &mut Server<S>, stats: &CreatureStats, distance: f32) -> bool {
    let (basic, _) = attacks_of(&s.skill_table, stats);
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
    let (basic, specials) = attacks_of(&s.skill_table, &c.stats);
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
            c.next_attack = now + f64::from(rhythm(template).gap / (CREATURE_TIME_SCALE * pace));
        }
        combat::creature_strike_with(s, net, id, target, skill, true, clock);
        return;
    }
    launch(s, net, id, target, skill, special.is_some(), clock);
}

/// A creature looses a missile (a basic attack's or a special's) at its target: the story, and the
/// missile's arrival after the windup and its flight.
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
    let rh = rhythm(template);
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
    let mut missile = StoryHook {
        bits: MISSILE_HOOK,
        target: pid,
        ..StoryHook::default()
    };
    // A creature holding a weapon names it.
    if let Some(w) = crate::arms::held(s, id) {
        missile.bits |= WEAPON_NAMED;
        missile.weapon = w;
    }
    missile.numbers[1] = 1;
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
            hooks: vec![
                StoryHook {
                    bits: combat::hook::OPENING,
                    ..StoryHook::default()
                },
                missile,
            ],
        },
    });
    to_all_seeing(s, net, id, &launch);
    let d = s
        .things
        .get(id)
        .and_then(|t| t.at)
        .and_then(|at| presence::distance(&at, &from))
        .unwrap_or(0.0);
    let flight = rh.windup / ts + d / MISSILE_SPEED;
    s.after(
        clock,
        f64::from(flight),
        Timer::Missile(Flight {
            creature: id,
            target: pid,
            skill,
            special: special.is_some(),
            from,
        }),
    );
}

/// A missile arrives: its target tells how it came out, its health first when it is hurt.
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
    let (level, natural, variance) = (
        c.stats.level,
        c.stats.natural_damage,
        c.stats.natural_variance,
    );
    let Some(p) = s.players.get(&world) else {
        return;
    };
    if combat::is_dead(p) || p.trip.is_some() {
        return;
    }
    let my_level = crate::examine::level_of(&p.qualities);
    let moved = presence::distance(&f.from, &p.position).is_none_or(|d| d > MOVED);
    let outcome = rules::roll_missile(&mut s.rng, f.skill.kind, level, my_level, moved);
    let more = effects_add(s, f.creature);
    let damage = if outcome.landed() {
        rules::creature_hit(
            &mut s.rng,
            f.skill.kind,
            rules::Natural {
                damage: natural,
                variance,
                added: f.skill.added + more,
                factor: f.skill.factor,
            },
            my_level,
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
    let hurt = outcome
        .landed()
        .then(|| combat::lose_health(s, net, world, harm.left));
    let swing = combat::Swing {
        skill: f.skill.skill,
        behavior: f.skill.behavior,
        time_scale: 1.0,
        target: f.target,
        outcome,
        damage: harm.left,
        back: harm.back,
    };
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let told = ServerMessage::DoStory(combat::player_event(
        p,
        PhysicsStory {
            bits: if f.special { SPECIAL_ARRIVAL } else { ARRIVAL },
            attacker: f.creature,
            skill: f.skill.skill,
            hooks: vec![swing.hook()],
            ..PhysicsStory::default()
        },
    ));
    to_all_seeing(s, net, f.target, &told);
    if let Some((share, was)) = hurt {
        combat::wounded(s, net, f.target, share, was);
    }
    if outcome.landed() && f.skill.drains {
        drain(s, net, world, f.creature, level);
    }
    combat::after_hurt(s, net, world, f.creature, clock);
    if outcome.landed() {
        crate::effects::answer_attack(s, net, world, f.creature, clock);
        crate::effects::harm_back(s, net, f.creature, f.target, harm.back, clock);
    }
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
}

/// The chance a creature at or below its warning level of health heals itself at one of its turns
/// (2017: shown at or below 60%, 2,148 of 4,045 turns with the heal free).
pub const HEAL_CHANCE: f32 = 0.53;
/// The chance a creature uses a buff of its at its first turn of a fight, and at a later one (2017:
/// 1,362 of 4,398 first turns with a buff free, 0.31; later turns 0.11-0.18).
pub const BUFF_FIRST_CHANCE: f32 = 0.31;
pub const BUFF_CHANCE: f32 = 0.13;
/// When a heal or a buff takes effect after its story, and when the creature's next attack comes,
/// in seconds (2017: a heal told a median 0.96 s after its story, 2,925 heals; the next attack a
/// median 2.28 s after it).
pub const HELP_LANDS: f64 = 0.96;
pub const HELP_RECOVERY: f64 = 2.3;
/// The story bits of a creature's hook on itself: kind 1 with its target, the result and the
/// animation hook.
pub const SELF_HOOK: u32 = 0x381;

/// A creature's heals and buffs: its skills (numbered 100 and up) that help, with their effects.
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
            })
        })
        .collect()
}

/// The chance a creature that used a heal or buff on one of its kin at its last turn uses it on
/// the same one again (2017: 0.43-0.65 by kind, 293 of 332 repeats on the same kin; a healer's 9 of
/// 15).
pub const KIN_REPEAT: f32 = 0.55;
/// The chance a protector buffs each of its kin in a fight within the buff's range, at its first
/// turn of a fight and at a later one (2017: at the first turn 74 of 293 with one such kin, 51 of
/// 123 with two, 53 of 107 with three or more; later 0.02-0.08).
pub const KIN_BUFF_FIRST: f32 = 0.25;
pub const KIN_BUFF_LATER: f32 = 0.02;
/// The share of health at or below which a healer heals its most wounded kin, and its chance at a
/// turn (2017: 12 kin heals in 143 turns with a kin at 0.4 or less, 1 in 85 between 0.4 and 0.65).
pub const KIN_WOUNDED_AT: f32 = 0.4;
pub const KIN_HEAL_CHANCE: f32 = 0.08;
/// The generic monsters' sub class of protectors, which buff their kin (2017: 432 of 433 buffs on
/// kin), and the behavior tables of the healers, which heal theirs.
pub const PROTECTOR: u32 = 7;
pub const HEALERS: [&str; 5] = [
    "NPCTable_DrudgeGloom",
    "NPCTable_BurunLeader",
    "NPCTable_MoarsmanHealer",
    "NPCTable_MimbuHealer",
    "NPCTable_Crone",
];
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
/// hurt; a protector a kin of its in a fight; else itself, a heal when its health is at or below
/// its warning level, a buff otherwise. Each by its chance; a use on itself is barred 40 s after,
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
    let first = c.actions == 0;
    let warning = c.stats.warning_level;
    let protector = c.stats.sub_class == Some(PROTECTOR);
    let healer = c
        .stats
        .npc_table
        .as_deref()
        .is_some_and(|n| HEALERS.contains(&n));
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
        let buffs: Vec<HelpSkill> = all.iter().filter(|h| !h.heals).cloned().collect();
        if !buffs.is_empty() {
            let h = buffs[s.rng.below(buffs.len())].clone();
            let p = if first {
                KIN_BUFF_FIRST
            } else {
                KIN_BUFF_LATER
            };
            let kin: Vec<u64> = kin_near(s, id, h.range)
                .into_iter()
                .filter(|k| k.2)
                .map(|k| k.0)
                .collect();
            let taken: Vec<u64> = kin.into_iter().filter(|_| s.rng.chance(p)).collect();
            if !taken.is_empty() {
                let k = taken[s.rng.below(taken.len())];
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
    let pick = if wounded {
        free.iter().find(|h| h.heals).cloned()
    } else {
        None
    };
    let pick = match pick {
        Some(h) => s.rng.chance(HEAL_CHANCE).then_some(h),
        None => {
            let buffs: Vec<&HelpSkill> = free.iter().filter(|h| !h.heals).collect();
            if buffs.is_empty() {
                None
            } else {
                let p = if first {
                    BUFF_FIRST_CHANCE
                } else {
                    BUFF_CHANCE
                };
                s.rng
                    .chance(p)
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
