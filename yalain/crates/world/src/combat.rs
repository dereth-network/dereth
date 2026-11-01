//! Fighting: a player's basic attacks on creatures and creatures' attacks on players, each resolved
//! by [`crate::rules`] and told as a combat story to the clients that see the fight.
//!
//! **A player's attack run.** The client asks once to attack a target; the server answers that the
//! player is attacking, puts it in its weapon's combat mode, and swings for it until the target
//! dies, the client stops, or the target is gone. The swings are the player's basic attacks that
//! fit its weapon, drawn at random among those whose ranges hold the distance to the target.
//! With a weapon whose attacks walk the player (a sword's): each swing a movement order toward the
//! target (the client closes in), then the story: the attacker, its swing's behavior and time
//! scale, the skill, and the result on the target (a hit or a critical with the damage, or an
//! evade). With a bow or a stave the player stands: each swing stops it and turns it to the target;
//! a bow shoots beyond a meter between the bodies and strikes within one, a stave strikes within ten
//! meters and throws beyond nine; a thrown attack's story names the weapon, and its target tells how
//! it came out as it arrives (after the throw's windup and its flight); a strike lands at its hit
//! point. Out of range the run is told it is too far, and moving it cannot shoot, each a second
//! apart; its combat mode comes with its first swing. A creature hurt to 80% or less shows it. A
//! run that ends is answered with the player not attacking and at peace.
//!
//! **A creature's attack.** In reach of its target, a creature strikes as often as its attacks
//! allow, with one of its melee skills; the player's health goes down by the damage, and shows it
//! at 80% or less; with none left the player dies ([`crate::death`]). Creatures' missiles and
//! specials are [`crate::missiles`].

use auberean_primitives::Vec3;
use auberean_protocol::physics::{Fx, MoveOrder, ObjectEvent, Word};
use auberean_protocol::types::{
    BehaviorParams, MoveTarget, MovementParams, ObjectStamp, PhysicsStory, StoryHook,
};
use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_net::{BlobKind, ConnId, ServerNet};
use yalain_store::Store;

use crate::calls::Value;
use crate::player::quality;
use crate::rules::{self, Direction, Kind, Outcome};
use crate::world::Player;
use crate::{progress, quals, session, Clock, Server};

/// The combat functions.
pub mod function {
    /// The client's requests.
    pub const START_ATTACK: u32 = 0x8204_0008;
    pub const STOP_ATTACK: u32 = 0x8204_0010;
    pub const STOP_ATTACKING: u32 = 0x8204_0020;
    pub const SET_NEXT_TARGET: u32 = 0x8204_0001;
    /// The answers.
    pub const ATTACK_STATE: u32 = 0x8204_001B;
    pub const BASIC_ATTACKS_FAILED: u32 = 0x8204_001C;
}

/// Why basic attacks failed (the client writes the text).
pub mod error {
    pub const WRONG_IMPLEMENT: u32 = 30026;
    pub const TARGET_DEAD: u32 = 22000;
    pub const TARGET_GONE: u32 = 20002;
    pub const TOO_FAR: u32 = 30019;
}

/// Modes.
pub mod mode {
    pub const PEACE: u32 = 0x4000_0001;
    /// A player with a one-handed melee weapon (or none).
    pub const ONE_HANDED: u32 = 0x4100_000A;
    /// A creature without a weapon.
    pub const MONSTER: u32 = 0x4100_0012;
    /// A one-handed weapon with a shield, two wielded at once, a polearm, a handblade, a spear
    /// thrown or a bow, a vial or a boulder.
    pub const WITH_SHIELD: u32 = 0x4100_000B;
    pub const DUAL: u32 = 0x4100_000E;
    pub const POLEARM: u32 = 0x4100_000C;
    pub const HANDBLADE: u32 = 0x4100_000F;
    pub const THROWN: u32 = 0x4100_0010;
    pub const ALCHEMY: u32 = 0x4100_0015;
}

/// A creature's combat mode, from the weapon its template has it wield, as a player's comes from
/// what it wields: none, the monster's own; a weapon of one stance (a polearm, a handblade, a
/// javelin or bow, a vial or boulder, a gunblade) that stance; any other one-handed weapon alone,
/// with a shield beside it, or two of them (2017: 5,356 of 5,427 creatures with a weapon took this
/// mode, 10,362 of 10,362 without one the monster's). The stance is the weapon's kind's, as the
/// weapons' templates set it, read off the first word of the weapon's name. A creature that does
/// not wield implements fights as a monster whatever it names (2017: 4,390 of 4,421 with a weapon
/// so, seen fighting).
#[must_use]
pub fn creature_mode(stats: &yalain_content::CreatureStats) -> u32 {
    let Some(weapon) = stats.weapon.as_deref().filter(|_| stats.wields) else {
        return mode::MONSTER;
    };
    if let Some(m) = kind_stance(weapon) {
        return m;
    }
    let shield = stats.weapon_and_shield
        && stats
            .beside
            .as_deref()
            .is_some_and(|b| b.starts_with("Shield"));
    let second = stats
        .beside
        .as_deref()
        .is_some_and(|b| !b.starts_with("Shield"));
    if stats.dual_wields && (stats.weapons_held >= 2 || second) {
        mode::DUAL
    } else if shield {
        mode::WITH_SHIELD
    } else {
        mode::ONE_HANDED
    }
}

/// The stance a weapon's kind is fought with however it is held, read off the first word of its
/// name; `None` for the kinds held alone, with a shield or two at once.
#[must_use]
pub fn kind_stance(weapon: &str) -> Option<u32> {
    let word = weapon
        .char_indices()
        .skip(1)
        .find(|(_, c)| c.is_ascii_uppercase())
        .map_or(weapon, |(i, _)| &weapon[..i]);
    match word {
        "Polearm" | "Arc" => Some(mode::POLEARM),
        "Handblade" => Some(mode::HANDBLADE),
        "Javelin" | "Bow" => Some(mode::THROWN),
        "Vial" | "Phial" | "Boulder" => Some(mode::ALCHEMY),
        "Gunblade" => Some(mode::ONE_HANDED),
        _ => None,
    }
}

/// The effect of a wounded object (its scalar how wounded), and the share of health at or below
/// which it shows.
pub const WOUNDED_FX: u32 = 0x4100_0002;
pub const WOUNDED_AT: f32 = 0.8;

/// A player's basic melee attacks (with a one-handed weapon, a bow up close, or nothing): five
/// skills and the behavior each plays.
pub const MELEE_SKILLS: [(u32, u32); 5] = [
    (122, 0x4100_0089),
    (123, 0x4100_008A),
    (124, 0x4100_008B),
    (125, 0x4100_008C),
    (126, 0x4100_008D),
];

/// A story's bits: the attacker, its behavior, the skill, a basic attack.
pub const BASIC_SWING: u32 =
    PhysicsStory::ATTACKER | PhysicsStory::BEHAVIOR | PhysicsStory::SKILL | 0x20;

/// A hook's bits: its kind is the low four bits.
pub mod hook {
    /// The first hook of every swing: kind 2, nothing more.
    pub const OPENING: u32 = 0x2;
    /// A landed hit: kind 1 with the target, result, animation hook and target health change.
    pub const HIT: u32 = 0xB81;
    /// An evade: kind 1 with the target, result and animation hook.
    pub const EVADE: u32 = 0x381;
}

/// The behavior bits of a swing: behavior, time scale, and the two words of flags 0x400 (1) and
/// 0x200 (0).
pub const SWING_BEHAVIOR_BITS: u32 = 0x605;

/// A fresh human's swing time scale, about which each swing varies by a tenth, and a creature's.
pub const PLAYER_TIME_SCALE: f32 = 1.05;
pub const CREATURE_TIME_SCALE: f32 = 1.296;

/// A melee swing's length at time scale 1, in seconds (the sword's and the bow's up close
/// measured 1.64 and 1.32).
pub const SWING_SECONDS: f32 = 1.5;

/// How far a player's melee attack reaches, in meters (2017's bow-up-close swings 1.3-2.4 m, at
/// most 3.2).
pub const PLAYER_REACH: f32 = 3.2;

/// When a player's first swing comes after it asks to attack, in seconds.
pub const FIRST_SWING: f64 = 0.23;

/// When a too-far attack is tried again, in seconds.
pub const RETRY_SECONDS: f64 = 1.0;

/// Seconds between a creature's melee attacks (the start area's measured gaps: 1.24-1.42 s, 2.0
/// for the dagger wielders).
pub const CREATURE_ATTACK_SECONDS: f64 = 1.35;

/// Seconds from a creature's basic strike to its next attack, by the strike's skill (2017's medians:
/// the plain strike 1.39 s, `Strong` 1.52, `Stronger` 1.63); others as [`CREATURE_ATTACK_SECONDS`].
#[must_use]
pub fn strike_gap(skill: u32) -> f64 {
    match skill {
        1845 => 1.39,
        1846 => 1.52,
        1847 => 1.63,
        _ => CREATURE_ATTACK_SECONDS,
    }
}

/// A player's attack run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Run {
    pub target: u64,
    pub next_swing: f64,
    pub context: u32,
    /// Its combat mode has been sent (a run begun out of range sends it with its first swing).
    pub mode_sent: bool,
}

/// Why a standing run cannot swing (the client writes the text): moving.
pub const MOVING: u32 = 30_027;

/// When a run told it is too far, or moving, tries again (2017: 1.01 and 1.02 s).
pub const STANDING_RETRY: f64 = 1.01;

/// A swing's period over its time scale, by what is swung (2017: a bow shooting 1.789, a bow
/// striking 1.339, a stave 1.65, a sword 1.64).
#[must_use]
pub fn swing_seconds(implement: Option<u32>, thrown: bool) -> f64 {
    use crate::skill_use::implement as im;
    match implement {
        Some(im::BOW) if thrown => 1.789,
        Some(im::BOW) => 1.339,
        Some(im::STAVE) => 1.65,
        _ => 1.64,
    }
}

/// When a thrown basic attack leaves (over the time scale), and its speed, by the weapon: a bow's
/// arrow 0.831 s and its hook's speed (2017: 0.839 s at 35.1 m/s fitted over 67 shots at still
/// targets); a stave's 0.817 s (161).
#[must_use]
pub fn windup_seconds(implement: Option<u32>) -> f64 {
    if implement == Some(crate::skill_use::implement::BOW) {
        0.831
    } else {
        0.817
    }
}

/// When a basic strike lands after its story, over the time scale (2017: a bow's 0.572, a stave's
/// 0.817).
#[must_use]
pub fn hit_seconds(implement: Option<u32>) -> f64 {
    if implement == Some(crate::skill_use::implement::BOW) {
        0.572
    } else {
        0.817
    }
}

/// A basic attack on its way to its target.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Landing {
    pub world: ConnId,
    pub target: u64,
    pub skill: u32,
    pub outcome: Outcome,
    pub damage: i32,
    /// Its result hook's index among the skill's hooks, and whether it is thrown (its target tells
    /// it as it arrives).
    pub index: u32,
    pub thrown: bool,
}

/// A thrown basic attack's arrival, told by its target: the attacker, the skill, a basic attack.
pub const BASIC_ARRIVAL: u32 = PhysicsStory::ATTACKER | PhysicsStory::SKILL | 0x20;

/// The basic attacks a player's weapon swings: the skills it holds that are basic attacks and fit
/// what it wields (none for a weapon the server does not know).
pub(crate) fn auto_attacks<S: Store>(
    s: &Server<S>,
    world: ConnId,
) -> Vec<(u32, crate::skills::Usage)> {
    let Some(p) = s.players.get(&world) else {
        return Vec::new();
    };
    let held = crate::skill_use::wielded(s, p.character);
    if held
        .first()
        .is_none_or(|(_, t)| crate::skill_use::implement_of(*t).is_none())
    {
        return Vec::new();
    }
    let starting = s.start.skills.get(&p.race).cloned().unwrap_or_default();
    crate::skills::held(&starting, &p.play.skills)
        .iter()
        .filter_map(|h| {
            let d = s.skill_table.skills.get(&h.skill)?;
            let u = d.usage.clone()?;
            (d.auto_attack && crate::skill_use::implement_allowed(&u, &held))
                .then_some((h.skill, u))
        })
        .collect()
}

/// Does a basic attack reach across this distance between the bodies (beyond its least range,
/// within its greatest)?
#[must_use]
pub fn reaches(u: &crate::skills::Usage, gap: f32) -> bool {
    (u.min_range < 0.0 || gap > u.min_range) && gap <= u.max_range
}

/// Do a weapon's basic attacks walk the player to its target (a sword's), or does it stand?
fn walks(autos: &[(u32, crate::skills::Usage)]) -> bool {
    autos.is_empty()
        || autos
            .iter()
            .any(|(_, u)| u.ui_flags & crate::skill_use::ui_flag::MOVE_TO != 0)
}

/// The distance between a player's body and a target's.
fn gap_to<S: Store>(s: &Server<S>, world: ConnId, target: u64) -> Option<f32> {
    let p = s.players.get(&world)?;
    let at = s.things.get(target)?.at?;
    crate::reach::gap(
        &p.position,
        crate::reach::PLAYER,
        &at,
        crate::reach::body_of(s, target),
    )
}

/// Is a player dead (its health gone)?
#[must_use]
pub fn is_dead(p: &Player) -> bool {
    crate::examine::Vitals::of_qualities(&p.qualities).is_some_and(|v| v.health == 0)
}

/// Send a visual event about a player or a thing to its own client (a player) and every client
/// that knows it.
pub fn to_all_seeing<S: Store>(s: &Server<S>, net: &mut ServerNet, id: u64, m: &ServerMessage) {
    let bytes = m.encode();
    for (conn, q) in &s.players {
        if q.character == id || q.known.contains_key(&id) {
            net.send(*conn, queue::EVENT, BlobKind::VISUAL, bytes.clone());
        }
    }
}

pub(crate) fn player_event<A>(p: &mut Player, action: A) -> ObjectEvent<A> {
    p.visual = p.visual.wrapping_add(1);
    ObjectEvent {
        sender: ObjectStamp {
            other_stamp: p.visual,
            ..p.stamp()
        },
        action,
    }
}

pub(crate) fn attack_state(on: bool) -> ServerMessage {
    session::call(function::ATTACK_STATE, &[Arg::Int(u32::from(on))]).expect("packs")
}

fn failed(err: u32) -> ServerMessage {
    session::call(function::BASIC_ATTACKS_FAILED, &[Arg::Int(err)]).expect("packs")
}

/// Put a player in a mode, for all who see it.
pub(crate) fn set_player_mode<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    m: u32,
) {
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let e = ServerMessage::DoMode(player_event(p, Word(m)));
    let id = p.character;
    to_all_seeing(s, net, id, &e);
}

/// The client asks to attack a target.
pub fn start<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    v: &[Value],
    clock: Clock,
) {
    let target = v.iter().find_map(|x| match x {
        Value::Long(l) => Some(*l),
        _ => None,
    });
    let Some(target) = target else {
        s.unhandled.note("an attack that does not read");
        return;
    };
    begin(s, net, world, target, clock);
}

/// A player begins attacking a target.
pub fn begin<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    target: u64,
    clock: Clock,
) {
    // Any living creature may be attacked: a practice target that never fights back (the halls'
    // Straw Drudges) is struck like any other.
    let fightable = s
        .things
        .get(target)
        .map(|t| t.alive() && t.creature.is_some());
    if fightable != Some(true) {
        let err = if fightable.is_some() {
            error::TARGET_DEAD
        } else {
            error::TARGET_GONE
        };
        progress::private(net, world, &failed(err));
        progress::private(net, world, &attack_state(false));
        return;
    }
    // A human has no basic attack with an empty hand.
    let me = s.players.get(&world).map_or(0, |p| p.character);
    let armed = s
        .players
        .get(&world)
        .is_some_and(|p| p.play.items.iter().any(|h| h.location != 0));
    let was = s.players.get(&world).is_some_and(|p| p.attacking.is_some());
    if !armed && !was {
        progress::private(net, world, &attack_state(true));
        progress::private(net, world, &failed(error::WRONG_IMPLEMENT));
        progress::private(net, world, &attack_state(false));
        return;
    }
    // Fighting binds what binds when used in combat.
    crate::equip::in_combat(s, world);
    let mode = crate::skill_use::player_mode(s, me);
    s.next_context = s.next_context.wrapping_add(1);
    let context = s.next_context;
    let autos = auto_attacks(s, world);
    let stands = !walks(&autos);
    let in_range = !stands
        || gap_to(s, world, target).is_some_and(|g| autos.iter().any(|(_, u)| reaches(u, g)));
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let mode_sent = p.attacking.is_some_and(|r| r.mode_sent) || in_range;
    p.attacking = Some(Run {
        target,
        next_swing: clock.local.0 + FIRST_SWING,
        context,
        mode_sent,
    });
    if !was {
        progress::private(net, world, &attack_state(true));
        // Attacking ends a lifestone protection, before the attack's mode.
        crate::effects::unprotect(s, net, world, |_, _| {});
        if in_range {
            set_player_mode(s, net, world, mode);
        }
        if stands {
            if let Some(p) = s.players.get_mut(&world) {
                let turn = crate::skill_use::turn_to(p, target, context);
                to_all_seeing(s, net, me, &turn);
            }
        }
    }
}

/// A player's attack run ends: it is not attacking, and at peace.
pub fn end<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId) {
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    if p.attacking.take().is_none() {
        return;
    }
    // A special waiting for the run's next swing will not come.
    if let Some(r) = p.in_run.take() {
        progress::private(net, world, &crate::skill_use::ended(r.id));
    }
    progress::private(net, world, &attack_state(false));
    set_player_mode(s, net, world, mode::PEACE);
}

/// The client stops attacking (a target, or whatever it is attacking).
pub fn stop<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, v: &[Value]) {
    let target = v.iter().find_map(|x| match x {
        Value::Long(l) => Some(*l),
        _ => None,
    });
    let attacking = s
        .players
        .get(&world)
        .and_then(|p| p.attacking)
        .map(|r| r.target);
    if target.is_none() || target == attacking {
        end(s, net, world);
    }
}

/// The swings due now, for every player attacking.
pub fn swing_all<S: Store>(s: &mut Server<S>, net: &mut ServerNet, clock: Clock) {
    let due: Vec<ConnId> = s
        .players
        .iter()
        .filter(|(_, p)| p.attacking.is_some_and(|r| r.next_swing <= clock.local.0))
        .map(|(c, _)| *c)
        .collect();
    for world in due {
        swing(s, net, world, clock);
    }
}

/// A movement order taking a player into reach of its target.
pub(crate) fn close_in(p: &mut Player, target: u64, context: u32) -> ServerMessage {
    p.move_to_stamp = p.move_to_stamp.wrapping_add(1);
    let stamp = p.move_to_stamp;
    ServerMessage::MoveTo(player_event(
        p,
        MoveOrder {
            params: MovementParams {
                flags: 0x46B,
                desired_velocity: 1.0,
                fail_distance: f32::MAX,
                desired_distance: 1.0,
                max_success_distance: 1.0,
                min_success_distance: 0.0,
                target: MoveTarget::Object(target),
                offset: Vec3::ZERO,
                offset_id: 0,
                waypoints: Vec::new(),
                context,
            },
            stamp,
        },
    ))
}

/// One swing of a player's run.
fn swing<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, clock: Clock) {
    let now = clock.local.0;
    let Some(run) = s.players.get(&world).and_then(|p| p.attacking) else {
        return;
    };
    let target = s.things.get(run.target).filter(|t| t.alive()).map(|t| {
        (
            t.at,
            t.level(),
            t.creature.as_ref().map_or(0, |c| c.stats.armor),
        )
    });
    let Some((Some(at), level, armor)) = target else {
        progress::private(net, world, &failed(error::TARGET_DEAD));
        end(s, net, world);
        return;
    };
    let autos = auto_attacks(s, world);
    if !walks(&autos) {
        return stand_and_swing(s, net, world, run, &autos, (at, level, armor), clock);
    }
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let order = close_in(p, run.target, run.context);
    let id = p.character;
    let special = p.in_run.take();
    to_all_seeing(s, net, id, &order);
    // A special waiting for this swing takes it.
    if let Some(r) = special {
        if crate::skill_use::slot(s, net, world, r, clock) {
            return;
        }
    }
    let Some(p) = s.players.get(&world) else {
        return;
    };
    let far = crate::presence::distance(&p.position, &at).is_none_or(|d| d > PLAYER_REACH);
    if far {
        progress::private(net, world, &failed(error::TOO_FAR));
        if let Some(r) = s.players.get_mut(&world).and_then(|p| p.attacking.as_mut()) {
            r.next_swing = now + RETRY_SECONDS;
        }
        return;
    }
    let my_level = crate::examine::level_of(&p.qualities);
    let (skill, behavior) = if autos.is_empty() {
        MELEE_SKILLS[s.rng.below(MELEE_SKILLS.len())]
    } else {
        let (k, u) = &autos[s.rng.below(autos.len())];
        (*k, u.behavior_other[0])
    };
    let kind = kind_of(s, skill, false);
    let outcome = rules::roll_outcome(
        &mut s.rng,
        Direction::PlayerOnCreature,
        kind,
        my_level,
        level,
    );
    let Some(p) = s.players.get(&world) else {
        return;
    };
    let damage = if outcome.landed() {
        let me = hitter(s, p);
        rules::player_hit(&mut s.rng, me, armor, outcome == Outcome::Critical)
            + armor_taken(s, run.target)
    } else {
        0
    };
    let ts = PLAYER_TIME_SCALE * s.rng.between(0.9, 1.1);
    let st = story(
        s,
        Attacker::Player(world),
        Swing {
            skill,
            behavior,
            time_scale: ts,
            target: run.target,
            outcome,
            damage,
        },
    );
    to_all_seeing(s, net, id, &st);
    if let Some(r) = s.players.get_mut(&world).and_then(|p| p.attacking.as_mut()) {
        r.next_swing = now + swing_seconds(None, false) / f64::from(ts);
    }
    if outcome.landed() {
        hurt_creature(s, net, run.target, id, damage, clock);
    }
    // A creature struck goes after the one who struck it.
    if s.things.get(run.target).is_some_and(|t| t.alive()) {
        crate::ai::go_after(s, net, run.target, id, clock);
    }
}

/// A swing of a run that stands (a bow's, a stave's): the player stopped and turned to its target;
/// a special waiting takes the swing; else, moving, it cannot shoot, and out of every basic
/// attack's range it is too far; else a basic attack in range is drawn and swung.
fn stand_and_swing<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    run: Run,
    autos: &[(u32, crate::skills::Usage)],
    (at, level, armor): (yalain_store::StoredPosition, i32, i32),
    clock: Clock,
) {
    let now = clock.local.0;
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let id = p.character;
    let stop = crate::skill_use::stop_moving(p);
    let turn = crate::skill_use::turn_to(p, run.target, run.context);
    let special = p.in_run.take();
    to_all_seeing(s, net, id, &stop);
    to_all_seeing(s, net, id, &turn);
    if let Some(r) = special {
        if crate::skill_use::slot(s, net, world, r, clock) {
            return;
        }
    }
    let Some(p) = s.players.get(&world) else {
        return;
    };
    let gap = crate::reach::gap(
        &p.position,
        crate::reach::PLAYER,
        &at,
        crate::reach::body_of(s, run.target),
    )
    .unwrap_or(f32::MAX);
    let moving = crate::skill_use::moving(p);
    let in_range: Vec<&(u32, crate::skills::Usage)> =
        autos.iter().filter(|(_, u)| reaches(u, gap)).collect();
    let usable: Vec<&(u32, crate::skills::Usage)> = in_range
        .iter()
        .copied()
        .filter(|(_, u)| !moving || u.ui_flags & crate::skill_use::ui_flag::USE_WHILE_MOVING != 0)
        .collect();
    if usable.is_empty() {
        let err = if in_range.is_empty() {
            error::TOO_FAR
        } else {
            MOVING
        };
        progress::private(net, world, &failed(err));
        if let Some(r) = s.players.get_mut(&world).and_then(|p| p.attacking.as_mut()) {
            r.next_swing = now + STANDING_RETRY;
        }
        return;
    }
    let my_level = crate::examine::level_of(&p.qualities);
    let added = crate::effects::damage_added(s, p);
    let held = crate::skill_use::wielded(s, id);
    let (weapon, implement) = held
        .first()
        .map_or((0, None), |(i, t)| (*i, crate::skill_use::implement_of(*t)));
    if !run.mode_sent {
        let mode = crate::skill_use::player_mode(s, id);
        set_player_mode(s, net, world, mode);
        if let Some(r) = s.players.get_mut(&world).and_then(|p| p.attacking.as_mut()) {
            r.mode_sent = true;
        }
    }
    let (skill, u) = usable[s.rng.below(usable.len())].clone();
    let thrown = crate::skill_use::missile_hook(&u).cloned();
    let kind = kind_of(s, skill, thrown.is_some());
    let outcome = rules::roll_outcome(
        &mut s.rng,
        Direction::PlayerOnCreature,
        kind,
        my_level,
        level,
    );
    let damage = if outcome.landed() {
        let me = rules::Hitter {
            level: my_level,
            weapon: weapon_damage(s, weapon),
            added,
        };
        rules::player_hit(&mut s.rng, me, armor, outcome == Outcome::Critical)
            + armor_taken(s, run.target)
    } else {
        0
    };
    let ts = PLAYER_TIME_SCALE * s.rng.between(0.9, 1.1);
    let behavior = u.behavior_other[0];
    let hooks = crate::skill_use::story_hooks(&u, run.target, weapon, Some((outcome, damage)));
    let index = hooks.len().saturating_sub(2);
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let told = ServerMessage::DoStory(player_event(
        p,
        PhysicsStory {
            bits: BASIC_SWING,
            attacker: id,
            attacker_behavior: crate::skill_use::story_params(behavior, ts),
            attack_context: 0,
            skill,
            hooks,
        },
    ));
    to_all_seeing(s, net, id, &told);
    if crate::skill_use::WITH_WORD_20.contains(&behavior) && weapon != 0 {
        let plays = ServerMessage::PlayBehavior(ObjectEvent {
            sender: crate::inventory::visual(s, weapon),
            action: BehaviorParams::behavior(behavior),
        });
        to_all_seeing(s, net, id, &plays);
    }
    let landing = Landing {
        world,
        target: run.target,
        skill,
        outcome,
        damage,
        index: u32::try_from(index).unwrap_or(0),
        thrown: thrown.is_some(),
    };
    let after = match &thrown {
        Some(h) => {
            let centre = s
                .players
                .get(&world)
                .and_then(|p| crate::presence::distance(&p.position, &at))
                .unwrap_or(gap);
            let speed = if h.missile_speed > 0.0 {
                h.missile_speed
            } else {
                35.0
            };
            windup_seconds(implement) / f64::from(ts) + f64::from(centre / speed)
        }
        None => hit_seconds(implement) / f64::from(ts),
    };
    s.after(clock, after, crate::Timer::Landing(landing));
    if let Some(r) = s.players.get_mut(&world).and_then(|p| p.attacking.as_mut()) {
        r.next_swing = now + swing_seconds(implement, thrown.is_some()) / f64::from(ts);
    }
}

/// The kind of a player's swing, for its landing chance: its skill's (a bow's strikes up close are
/// missile attacks, a stave's magic), else a missile when thrown and a melee attack when struck.
fn kind_of<S: Store>(s: &Server<S>, skill: u32, thrown: bool) -> Kind {
    s.skill_table
        .skills
        .get(&skill)
        .and_then(|d| d.kind)
        .unwrap_or(if thrown { Kind::Missile } else { Kind::Melee })
}

/// A basic attack lands: a thrown one's target tells how it came out (with no result when it died
/// on the way), then the hit hurts it; a creature struck goes after the one who struck it.
pub fn lands<S: Store>(s: &mut Server<S>, net: &mut ServerNet, l: Landing, clock: Clock) {
    let Some(me) = s.players.get(&l.world).map(|p| p.character) else {
        return;
    };
    let Some(alive) = s.things.get(l.target).map(crate::things::Thing::alive) else {
        return;
    };
    let hurts = alive && l.outcome.landed() && l.damage > 0;
    if l.thrown {
        let hooks = if alive {
            let extra = if l.index > 0 {
                crate::skill_use::story_hook::ATTACK_HOOK
            } else {
                0
            };
            let mut h = StoryHook {
                bits: if hurts { hook::HIT } else { hook::EVADE } | extra,
                target: l.target,
                ..StoryHook::default()
            };
            h.numbers[0] = l.outcome.code().cast_signed();
            h.numbers[1] = 1;
            h.numbers[2] = l.index.cast_signed();
            if hurts {
                h.numbers[3] = -l.damage;
            }
            vec![h]
        } else {
            Vec::new()
        };
        let told = ServerMessage::DoStory(ObjectEvent {
            sender: crate::inventory::visual(s, l.target),
            action: PhysicsStory {
                bits: BASIC_ARRIVAL,
                attacker: me,
                skill: l.skill,
                hooks,
                ..PhysicsStory::default()
            },
        });
        to_all_seeing(s, net, l.target, &told);
    }
    if hurts {
        hurt_creature(s, net, l.target, me, l.damage, clock);
    }
    if s.things
        .get(l.target)
        .is_some_and(crate::things::Thing::alive)
    {
        crate::ai::go_after(s, net, l.target, me, clock);
    }
}

/// Who a story is from.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Attacker {
    Player(ConnId),
    Thing(u64),
}

/// One swing: its skill and behavior, its time scale, its target and how it came out.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Swing {
    pub skill: u32,
    pub behavior: u32,
    pub time_scale: f32,
    pub target: u64,
    pub outcome: Outcome,
    pub damage: i32,
}

impl Swing {
    pub(crate) fn hook(&self) -> StoryHook {
        let mut h = StoryHook {
            bits: if self.outcome.landed() {
                hook::HIT
            } else {
                hook::EVADE
            },
            target: self.target,
            ..StoryHook::default()
        };
        // The numbers: the result, the animation hook (1), the target's health change.
        h.numbers[0] = self.outcome.code().cast_signed();
        h.numbers[1] = 1;
        if self.outcome.landed() {
            h.numbers[3] = -self.damage;
        }
        h
    }
}

/// A swing's story.
pub(crate) fn story<S: Store>(s: &mut Server<S>, who: Attacker, sw: Swing) -> ServerMessage {
    let mut b = BehaviorParams {
        bits: SWING_BEHAVIOR_BITS,
        behavior: sw.behavior,
        time_scale: sw.time_scale,
        ..BehaviorParams::default()
    };
    // Flag 0x400's word (the eighth of the flags followed by a word) is 1; flag 0x200's 0.
    b.words[7] = 1;
    b.words[9] = 0;
    let (sender, attacker) = match who {
        Attacker::Player(world) => {
            let Some(p) = s.players.get_mut(&world) else {
                return ServerMessage::DoStory(ObjectEvent::default());
            };
            p.visual = p.visual.wrapping_add(1);
            (
                ObjectStamp {
                    other_stamp: p.visual,
                    ..p.stamp()
                },
                p.character,
            )
        }
        Attacker::Thing(id) => (crate::inventory::visual(s, id), id),
    };
    ServerMessage::DoStory(ObjectEvent {
        sender,
        action: PhysicsStory {
            bits: BASIC_SWING,
            attacker,
            attacker_behavior: b,
            attack_context: 0,
            skill: sw.skill,
            hooks: vec![
                StoryHook {
                    bits: hook::OPENING,
                    ..StoryHook::default()
                },
                sw.hook(),
            ],
        },
    })
}

/// The next visual stamp of a player or a thing.
fn stamp_of<S: Store>(s: &mut Server<S>, id: u64) -> Option<ObjectStamp> {
    if let Some(p) = s.players.values_mut().find(|p| p.character == id) {
        p.visual = p.visual.wrapping_add(1);
        return Some(ObjectStamp {
            other_stamp: p.visual,
            ..p.stamp()
        });
    }
    s.things
        .get(id)
        .is_some()
        .then(|| crate::inventory::visual(s, id))
}

/// The wounded effect of an object at a share of its health (none above 80%; ended when it
/// recovers above it).
pub(crate) fn wounded<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    id: u64,
    share: f32,
    was: f32,
) {
    let m = if share <= WOUNDED_AT {
        let Some(sender) = stamp_of(s, id) else {
            return;
        };
        ServerMessage::DoFx(ObjectEvent {
            sender,
            action: Fx {
                fx: WOUNDED_FX,
                scalar: 1.0 - share,
            },
        })
    } else if was <= WOUNDED_AT {
        let Some(sender) = stamp_of(s, id) else {
            return;
        };
        ServerMessage::StopFx(ObjectEvent {
            sender,
            action: Word(WOUNDED_FX),
        })
    } else {
        return;
    };
    to_all_seeing(s, net, id, &m);
}

/// A creature takes damage from a player; at no health it dies.
pub fn hurt_creature<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    id: u64,
    by: u64,
    damage: i32,
    clock: Clock,
) {
    hurt_creature_as(s, net, id, (by, by), damage, clock);
}

/// A creature takes damage from a player, or from its pet (the player and the damager): the hurt is
/// the player's; the health taken, no more than the creature had left, is the damager's part of
/// its kill.
pub(crate) fn hurt_creature_as<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    id: u64,
    (by, damager): (u64, u64),
    damage: i32,
    clock: Clock,
) {
    // Its hurter's pets take it on.
    if damage > 0 {
        crate::pets::owner_hurt(s, by, id);
    }
    let Some(t) = s.things.get_mut(id) else {
        return;
    };
    #[allow(clippy::cast_precision_loss)]
    let was = t.health as f32 / t.max_health.max(1) as f32;
    let taken = i64::from(damage.clamp(0, t.health.max(0)));
    t.health = (t.health - damage).max(0);
    t.last_hurt = clock.local.0;
    #[allow(clippy::cast_precision_loss)]
    let share = t.health as f32 / t.max_health.max(1) as f32;
    if let Some(c) = t.creature.as_mut() {
        match c.damage_by.iter_mut().find(|(p, _)| *p == by) {
            Some(e) => e.1 += i64::from(damage),
            None => c.damage_by.push((by, i64::from(damage))),
        }
        if taken > 0 {
            match c.taken_by.iter_mut().find(|(p, _)| *p == damager) {
                Some(e) => e.1 += taken,
                None => c.taken_by.push((damager, taken)),
            }
        }
        let now = clock.local.0;
        c.harm
            .retain(|(when, _, _)| now - when < crate::ai::HARM_REMEMBERED);
        c.harm.push((now, by, i64::from(damage)));
    }
    if t.health == 0 {
        crate::death::creature_dies(s, net, id, by, clock);
    } else {
        wounded(s, net, id, share, was);
    }
}

/// A creature has noticed a player and goes after it: it takes its combat mode.
pub fn creature_engages<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64, _player: u64) {
    let m = crate::arms::stance(s, id);
    // Its weapons drawn with its mode.
    crate::arms::draw(s, net, id, m);
}

/// A creature gives up: it is at peace again.
pub fn creature_calms<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64) {
    // Its weapons put away first, then the monster's mode before peace (2017: 957 of the calms of
    // a creature holding one weapon so).
    let armed = s
        .things
        .get(id)
        .and_then(|t| t.creature.as_ref())
        .is_some_and(|c| !c.weapons.is_empty());
    if armed {
        crate::arms::put_away(s, net, id);
        let m = ServerMessage::DoMode(ObjectEvent {
            sender: crate::inventory::visual(s, id),
            action: Word(mode::MONSTER),
        });
        to_all_seeing(s, net, id, &m);
    }
    let m = ServerMessage::DoMode(ObjectEvent {
        sender: crate::inventory::visual(s, id),
        action: Word(mode::PEACE),
    });
    to_all_seeing(s, net, id, &m);
}

/// The damage of a weapon (a thing in the world), as its template sets it.
#[must_use]
pub fn weapon_damage<S: Store>(s: &Server<S>, weapon: u64) -> Option<rules::WeaponDamage> {
    let t = s.things.get(weapon)?;
    let props = &s.catalog.get(t.template)?.props;
    let most = props.int("Weapon_Damage").filter(|d| *d > 0)?;
    Some(rules::WeaponDamage {
        most: i32::try_from(most).unwrap_or(i32::MAX),
        variance: props.float("Weapon_Variance").unwrap_or(0.0),
    })
}

/// A player as its hits on creatures are reckoned: its level, the first weapon it wields, what its
/// effects add.
#[must_use]
pub fn hitter<S: Store>(s: &Server<S>, p: &Player) -> rules::Hitter {
    let weapon = crate::skill_use::wielded(s, p.character)
        .first()
        .and_then(|(i, _)| weapon_damage(s, *i));
    rules::Hitter {
        level: crate::examine::level_of(&p.qualities),
        weapon,
        added: crate::effects::damage_added(s, p),
    }
}

/// The slowest a creature's effects make its attacks: a fifth of their pace (2017's held creatures
/// still attacked, their stories at a fifth of the time scale or more).
pub const SLOWEST_ATTACKS: f32 = 0.2;

/// The pace of a creature's attacks under its effects: its effects' attack speed (a product: a
/// hold's 0 too), of whose slowing the creature takes its template's share, never below a fifth.
/// 2017's creatures told their attacks at the time scale times this pace, and struck that much
/// less often (Still Water on creatures taking 0.7 of it: 544 of 545 stories within the band it
/// gives; Paralyze 98 of 101 on those taking 0.65; stories outside any such effect 54,394 of
/// 56,137 at the plain time scale).
#[must_use]
pub fn attack_pace<S: Store>(s: &Server<S>, id: u64) -> f32 {
    let Some(c) = s.things.get(id).and_then(|t| t.creature.as_ref()) else {
        return 1.0;
    };
    let (add, mult) = crate::effects::creature_modifier(s, id, crate::effects::ATTACK_SPEED);
    let speed = ((1.0 + add) * mult).max(0.0);
    (1.0 - c.stats.speed_resistance * (1.0 - speed)).max(SLOWEST_ATTACKS)
}

/// The damage a player's direct hit on a creature gains from the armor rating the creature's
/// effects have taken away, a point for each point (2017: Rend Armor's 150 at its casters'
/// spellcraft, +144 on the same player's same strikes; Gossamer Flesh's 170, +181; Vacuum's 100,
/// +98; the same gain whatever the creature's plain hits). A change over time is not a hit and
/// gains nothing.
#[must_use]
pub fn armor_taken<S: Store>(s: &Server<S>, id: u64) -> i32 {
    let (add, _) = crate::effects::creature_modifier(s, id, crate::effects::ARMOR_RATING);
    dereth_primitives::num::to_i32((-add).max(0.0).round())
}

/// A creature within reach of its target strikes, when its next attack is due: a heal or a buff on
/// itself by its chance ([`crate::missiles::helps`]), else a special at hand by its chance (one that
/// throws a missile looses it even this near), else one of its basic melee attacks drawn by its
/// weights (the plain strike for a creature that names none).
pub fn creature_strikes<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    id: u64,
    target: ConnId,
    clock: Clock,
) {
    let now = clock.local.0;
    let Some(c) = s.things.get(id).and_then(|t| t.creature.as_ref()) else {
        return;
    };
    if now < c.next_attack {
        return;
    }
    let (basic, specials) = crate::missiles::attacks_of(&s.skill_table, &c.stats);
    let melee: Vec<crate::missiles::AttackSkill> =
        basic.into_iter().filter(|a| !a.missile).collect();
    let distance = s
        .things
        .get(id)
        .and_then(|t| t.at)
        .zip(s.players.get(&target).map(|p| p.position))
        .and_then(|(a, b)| crate::presence::distance(&a, &b))
        .unwrap_or(0.0);
    if crate::missiles::helps(s, net, id, clock) {
        return;
    }
    let pace = attack_pace(s, id);
    if let Some(c) = s.things.get_mut(id).and_then(|t| t.creature.as_mut()) {
        c.next_attack = now + CREATURE_ATTACK_SECONDS / f64::from(pace);
    }
    if let Some(sp) = crate::missiles::pick_special(s, id, &specials, distance, now) {
        if sp.missile {
            crate::missiles::launch(s, net, id, target, sp, true, clock);
        } else {
            creature_strike_with(s, net, id, target, sp, true, clock);
        }
        return;
    }
    let stats = s
        .things
        .get(id)
        .and_then(|t| t.creature.as_ref())
        .map(|c| c.stats.clone())
        .unwrap_or_default();
    let skill = crate::missiles::pick_basic(s, &stats, &melee)
        .unwrap_or(crate::missiles::CREATURE_ATTACKS[0]);
    if let Some(c) = s.things.get_mut(id).and_then(|t| t.creature.as_mut()) {
        c.next_attack = now + strike_gap(skill.skill) / f64::from(pace);
    }
    creature_strike_with(s, net, id, target, skill, false, clock);
}

/// A creature strikes its target where it stands with a skill (a special told without the
/// basic-attack flag): the story with its result, the player hurt by a hit.
pub fn creature_strike_with<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    id: u64,
    target: ConnId,
    skill: crate::missiles::AttackSkill,
    special: bool,
    clock: Clock,
) {
    let Some(c) = s.things.get(id).and_then(|t| t.creature.as_ref()) else {
        return;
    };
    let (level, natural, variance) = (
        c.stats.level,
        c.stats.natural_damage,
        c.stats.natural_variance,
    );
    let Some(p) = s.players.get(&target) else {
        return;
    };
    let (pid, my_level) = (p.character, crate::examine::level_of(&p.qualities));
    let outcome = rules::roll_outcome(
        &mut s.rng,
        Direction::CreatureOnPlayer,
        skill.kind,
        level,
        my_level,
    );
    // Strikes are evaded, never missed.
    let outcome = if outcome == Outcome::Miss {
        Outcome::Evade
    } else {
        outcome
    };
    let more = crate::missiles::effects_add(s, id);
    let damage = if outcome.landed() {
        rules::creature_hit(
            &mut s.rng,
            skill.kind,
            rules::Natural {
                damage: natural,
                variance,
                added: skill.added + more,
                factor: skill.factor,
            },
            my_level,
            outcome == Outcome::Critical,
        )
    } else {
        0
    };
    let ts = CREATURE_TIME_SCALE * s.rng.between(0.9, 1.1) * attack_pace(s, id);
    let mut st = story(
        s,
        Attacker::Thing(id),
        Swing {
            skill: skill.skill,
            behavior: skill.behavior,
            time_scale: ts,
            target: pid,
            outcome,
            damage,
        },
    );
    if special {
        if let ServerMessage::DoStory(e) = &mut st {
            e.action.bits = crate::missiles::SPECIAL;
        }
    }
    to_all_seeing(s, net, id, &st);
    if OPENS_AFTER_ATTACK.contains(&skill.skill) {
        crate::skill_use::creature_attacked(s, id, clock);
    }
    if outcome.landed() {
        hurt_player(s, net, target, id, damage, clock);
    }
}

/// The creatures' attacks after which they are open to a special that exploits openings (their
/// plain and strong melee attacks: 2017's openings followed them, 724 and 611 times).
pub const OPENS_AFTER_ATTACK: [u32; 2] = [1845, 1846];

/// A player loses health (down to none): its health is sent to it. Returns its share of health
/// after and before.
pub(crate) fn lose_health<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    damage: i32,
) -> (f32, f32) {
    let Some(p) = s.players.get_mut(&world) else {
        return (1.0, 1.0);
    };
    let max = quals::int(&p.qualities, quality::HEALTH_MAX)
        .unwrap_or(100)
        .max(1);
    let health = quals::int(&p.qualities, quality::HEALTH).unwrap_or(max);
    let now = (health - damage).max(0);
    quals::set_int(&mut p.qualities, quality::HEALTH, now);
    progress::send_int(net, world, quality::HEALTH, now);
    #[allow(clippy::cast_precision_loss)]
    let shares = (now as f32 / max as f32, health as f32 / max as f32);
    shares
}

/// A player has been hurt by a creature: with no health left it dies.
pub fn after_hurt<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    by: u64,
    clock: Clock,
) {
    // Being hit binds what binds when used in combat.
    crate::equip::in_combat(s, world);
    if s.players.get(&world).is_some_and(is_dead) {
        crate::death::player_dies(s, net, world, Some(by), clock);
    }
}

/// A player takes a creature's melee hit.
fn hurt_player<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    by: u64,
    damage: i32,
    clock: Clock,
) {
    let (share, was) = lose_health(s, net, world, damage);
    let Some(id) = s.players.get(&world).map(|p| p.character) else {
        return;
    };
    wounded(s, net, id, share, was);
    after_hurt(s, net, world, by, clock);
}

/// A creature or a pet strikes another thing in its reach (a pet its target, a creature a pet that
/// fights it): with the attack given (a pet's), else when its next attack is due with one of its
/// basic melee attacks drawn by its weights; the story with its result, the target hurt by a hit.
pub fn thing_strikes_thing<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    id: u64,
    target: u64,
    with: Option<crate::missiles::AttackSkill>,
    clock: Clock,
) {
    let now = clock.local.0;
    let Some(c) = s.things.get(id).and_then(|t| t.creature.as_ref()) else {
        return;
    };
    let stats = c.stats.clone();
    let skill = if let Some(w) = with {
        w
    } else {
        if now < c.next_attack {
            return;
        }
        let (basic, _) = crate::missiles::attacks_of(&s.skill_table, &stats);
        let melee: Vec<crate::missiles::AttackSkill> =
            basic.into_iter().filter(|a| !a.missile).collect();
        let skill = crate::missiles::pick_basic(s, &stats, &melee)
            .unwrap_or(crate::missiles::CREATURE_ATTACKS[0]);
        if let Some(c) = s.things.get_mut(id).and_then(|t| t.creature.as_mut()) {
            c.next_attack = now + strike_gap(skill.skill);
        }
        skill
    };
    let their_level = s.things.get(target).map_or(1, crate::things::Thing::level);
    let outcome = rules::roll_outcome(
        &mut s.rng,
        Direction::CreatureOnPlayer,
        skill.kind,
        stats.level,
        their_level,
    );
    let outcome = if outcome == Outcome::Miss {
        Outcome::Evade
    } else {
        outcome
    };
    let damage = if outcome.landed() {
        rules::creature_hit(
            &mut s.rng,
            skill.kind,
            rules::Natural {
                damage: stats.natural_damage,
                variance: stats.natural_variance,
                added: skill.added,
                factor: skill.factor,
            },
            their_level,
            outcome == Outcome::Critical,
        )
    } else {
        0
    };
    let ts = CREATURE_TIME_SCALE * s.rng.between(0.9, 1.1);
    let st = story(
        s,
        Attacker::Thing(id),
        Swing {
            skill: skill.skill,
            behavior: skill.behavior,
            time_scale: ts,
            target,
            outcome,
            damage,
        },
    );
    to_all_seeing(s, net, id, &st);
    if outcome.landed() {
        crate::pets::struck(s, net, target, id, damage, clock);
    }
}
