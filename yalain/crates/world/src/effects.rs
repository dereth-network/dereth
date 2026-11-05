//! Effects on players and creatures. On a player each is added on its client under an id of its
//! own, with a record of what it is (its saved state), who cast it, at what spellcraft, when and
//! for how long; taken away by that id. On a creature the server alone keeps the records; the
//! clients that see it see what they show.
//!
//! **What an effect does** ([`apply_on`]), by its kind ([`crate::effect_defs`]): an instant change
//! to vitals heals or hurts at once (health: the one changed tells it as a story, then its health;
//! vigor: its vigor alone); a quest moved on (or failed), a recipe learned or forgotten happen at
//! once; a teleport is no record, its player sent to its place when its time is up; a
//! combination looks at the target's effects and adds what it adds; any other effect that lasts
//! is added as a record. A player's ids count on from the last one it was given, across its
//! sessions.
//!
//! **A record** stays on its own clock: a new one never replaces or refreshes an older one, each
//! running out at its cast time plus its timeout, when its client is told it is gone. Records of a
//! class compete: the one of highest priority (then spellcraft, then the later) is the class's top
//! effect, and only top effects change the qualities. A player's client changes its qualities
//! itself from the records; the server sends no quality for it, except its speed (velocity and
//! acceleration, each as it changes). An effect's appearance shows from the first record holding
//! it to the last one's end. A record that changes vitals over time does so every few seconds, by
//! its rate times the seconds since the last change. A player's records still running are carried
//! in the session start; the ones a resurrection takes off go then.
//!
//! **On a creature** (2017: every lasting effect a player's skill put on one showed its appearance
//! from its add to cast plus duration, within 0.1 s, 9,621 of 9,621 that stopped): the appearance
//! shows to all who see it, its speed (velocity, acceleration) is sent as its effects change it, and
//! a change over time is told by the creature as a story of the caster's (one hook `0x981`, the
//! change), its first a share of ten seconds after the add, then every 5.6 to 11.3 seconds, and a
//! last one for the seconds left as it runs out. A dead creature's records run out as they would
//! until it leaves the world, when they go with it; its harm over time is still told (2017: 106 of
//! 170 damages over time that ran out after a death ticked after it), and changes nothing.
//!
//! **A combination** ([`crate::effect_defs::Combo`]) tells its caster's client whether it found what
//! it looks for on its target (a private effect: the found one's, or an empty one), takes off what
//! it found as its flags say, then adds its effect and gives back its own.
//!
//! **An aura** ([`aura_on`]) is a record its user holds while it is on (until taken off), with a
//! pulse at once and then every two minutes or so: each pulse puts the aura's pulsed effects on the
//! players it reaches (2017: each a record of three minutes, so two run at a time). One who holds a
//! record from an earlier pulse that this one missed is told (`Effect_PulseMissed`), and its client
//! shows that record running down. Turning it off takes its pulsed records and its own off; so does
//! going into portal space, but for a resurrection's: its auras stay on, and pulse again three
//! seconds after it is back, beside their own round (2017: 0 of 98 auras off at the 44 requests of
//! players holding one listed; 129 of 137 returns by players whose pulses came before it pulsed so,
//! a median 2.97 s after it was told it lives).
//!
//! **Lifestone protection.** A player is protected for three minutes as it is resurrected: the
//! protection is added in the request's tick, before portal space, and its glow (the effect the
//! resurrection plays on the player) stays on it. It ends, with "You are no longer protected by
//! the Lifestone's Magic.", the protection taken away and the glow stopped, when its three minutes
//! are up, when the player attacks (after the attack state is raised, before the attack), or when
//! the player comes out of portal space somewhere else (before the use is answered), or when it
//! makes a recipe (2017: 3 of the early ends). Creatures do not take on a protected player.
//!
//! **A city's recall protection** is the same for 30 s, put on as the recall ends (its own record,
//! flags 0x42, and its glow played), and its end says "You are no longer protected by the portal's
//! magic." (2017: 196 of 215 ran out at 30 s; of the 17 cut short with the text, 14 by a recipe
//! made).

use std::collections::HashMap;

use auberean_protocol::physics::{Fx, ObjectEvent, PrivateFx, Scale as WireScale, Word};
use auberean_protocol::types::{PhysicsStory, StoryHook};
use auberean_protocol::ServerMessage;
use auberean_script::args::Arg;
use auberean_script::native_object::TextInfo;
use yalain_net::{BlobKind, ConnId, ServerNet};
use yalain_store::Store;

use crate::effect_defs::{aura_target, combo_flag, vital, Aura, Combo, EffectDef, Kind};
use crate::graph::{tag, Builder, Slot};
use crate::player::quality;
use crate::quests::Move;
use crate::rules::Outcome;
use crate::state::EffectState;
use crate::{progress, quals, session, Clock, Server, Timer};

/// The calls that add an effect to the client's player and take one away, tell it that pulses
/// missed records it holds, and turn a skill's toggle on or off.
pub mod function {
    pub const ADD: u32 = 0x8208_0017;
    pub const REMOVE: u32 = 0x8208_0016;
    pub const PULSE_MISSED: u32 = 0x8208_002E;
    pub const TOGGLE: u32 = 0x8204_0029;
}

/// The script package of an effect's record.
pub const RECORD: u16 = 0x14E;

/// The record's slots (64-bit values take two), as the library declares them.
pub const RECORD_SLOTS: usize = 26;

/// A record's flags.
pub mod record_flag {
    /// It lasts until taken off.
    pub const INFINITE: u32 = 0x1;
    /// It runs out after its timeout.
    pub const NORMAL: u32 = 0x2;
    /// Its timeout was given rather than read from its definition.
    pub const SPECIFIED: u32 = 0x4;
    /// It goes when the player leaves the world.
    pub const LOGOUT: u32 = 0x10;
    pub const TRANSIENT: u32 = 0x40;
    /// It is an item's effect on the one who wears or wields it.
    pub const EQUIPPED: u32 = 0x80;
}

/// The flags of an item's effect on its wearer: held until the item comes off, kept across
/// sessions and deaths (2017: 0xC1 on all 2,865 such records, added and at entry).
pub const WORN: u32 = record_flag::INFINITE | record_flag::TRANSIENT | record_flag::EQUIPPED;

/// The flags of a timed effect a skill or a quest gives (2017: 66 on every timed buff, debuff,
/// change over time and quest reward).
pub const TIMED: u32 = record_flag::NORMAL | record_flag::TRANSIENT;

/// An aura's own record: held until taken off, gone at logout (2017: flags 0x251 and categories
/// 0x21 on all 242 auras turned on).
pub const AURA_FLAGS: u32 = 0x251;
pub const AURA_CATEGORIES: u32 = 0x21;
/// A pulse's record: timed, pulsed, gone at logout (2017: 0xC52 on every pulse).
pub const PULSE_FLAGS: u32 = 0xC52;

/// An aura's next pulse: the first within one period of its turning on (2017: 0.6-121.6 s, 5% to
/// 95%), then every 121 to 124 seconds (12,036 pulses: quartiles 121.0, 122.4, 123.9).
pub const PULSE_LEAST: f64 = 121.0;
pub const PULSE_MOST: f64 = 124.0;
/// How long after a player is back from a resurrection its auras pulse again.
pub const AURAS_PULSE_AFTER_RETURN: f64 = 3.0;

/// A creature's change over time: the first tick a share of this after the add, then every so
/// many seconds (2017, a creature's single change over time: the first 0.1-10.4 s, then 5.7-11.2 s,
/// 5% to 95% of 6,804 gaps; the last tick at the end).
pub const CREATURE_TICK_FIRST: f64 = 10.4;
pub const CREATURE_TICK_LEAST: f64 = 5.6;
pub const CREATURE_TICK_MOST: f64 = 11.3;

/// The private effect a combination found on its target plays for its caster.
pub const COMBO_FX: u32 = 0x4100_051A;

/// The text of a toggled skill turned off (type 2, the skill's name as its variable).
pub mod toggled_off {
    pub const TEXT: (u32, u32) = (0x1220_3326, 0x2500_0010);
    pub const SKILL_VAR: u32 = 0x67AE_72CD;
}

/// Whom an effect is put on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum On {
    /// A player, by its world connection.
    Player(ConnId),
    /// A creature, by its id.
    Creature(u64),
}

/// The records the server keeps on creatures, by creature, and the last id it gave one.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CreatureRecords {
    pub counter: u32,
    pub on: HashMap<u64, Vec<EffectState>>,
}

/// How a record is written: its flags, categories and timeout (none: its definition's at the
/// spellcraft), and for whom it was cast.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shape {
    pub flags: u32,
    pub categories: Option<u32>,
    pub timeout: Option<f32>,
    pub acting: u64,
}

impl Shape {
    /// A timed effect's (a skill's, a quest's).
    pub const TIMED: Self = Self {
        flags: TIMED,
        categories: None,
        timeout: None,
        acting: 0,
    };
}

/// A record's categories: an ordinary effect, a change over time.
pub mod category {
    pub const ORDINARY: u32 = 1;
    pub const OVER_TIME: u32 = 5;
}

/// The qualities of a player's speed (`type << 16 | quality`: floats 295, 296) and jump.
pub mod speed {
    pub const VELOCITY: u32 = 0x3_0127;
    pub const ACCELERATION: u32 = 0x3_0128;
    /// The jump scale (float 299).
    pub const JUMP: u32 = 0x3_012B;
}

/// A heal's story hook, told by the healed one: kind 1 with the target, its result and the
/// change to its health (a story told by one hurt or healed).
pub const HEAL_HOOK: u32 = 0x981;

/// A change over time's ticks: the first a share of the longest wait after it is added, then every
/// 3.5 to 7.5 seconds (2017: a median 5.2-5.5 s, the first 0-5.6 s after the add).
pub const TICK_FIRST: f64 = 5.6;
pub const TICK_LEAST: f64 = 3.5;
pub const TICK_MOST: f64 = 7.5;

/// A recipe learned from an effect with a name: its text (type 2, the effect's name and the
/// giver's as variables).
pub mod learned {
    pub const TEXT: (u32, u32) = (0xA727_FC98, 0x2500_1070);
    pub const EFFECT_VAR: u32 = 0xB69B_365C;
    pub const GIVER_VAR: u32 = 0x9FD3_465E;
}

/// The quality of damage added to a player's hits (an integer, 295).
pub const DAMAGE_ADD: u32 = 0x4_0127;

/// The quality of what a player's skills cost above their own (an integer, 296; negative lowers them).
pub const VIGOR_COST_ADD: u32 = 0x4_0128;

/// The quality of armor rating (an integer, 293), and of attack speed (a float, 297: a share of
/// the usual pace, 0 for a hold).
pub const ARMOR_RATING: u32 = 0x4_0125;
pub const ATTACK_SPEED: u32 = 0x3_0129;
/// The stats of skills: this with the skill's value.
pub const SKILL_STAT: u32 = 0x80_0000;

/// Lifestone protection: its saved state, how long it lasts, its record's flags and categories,
/// its glow, and the text its end shows (type 8).
pub mod protection {
    pub const STATE: u32 = 0x7000_0973;
    pub const SECONDS: f64 = 180.0;
    pub const FLAGS: u32 = 0x44;
    pub const CATEGORIES: u32 = 1;
    pub const GLOW: u32 = 0x4100_000F;
    pub const ENDED: (u32, u32) = (0x847E_5E83, 0x2500_000B);
    pub const ENDED_KIND: u32 = 8;
}

/// A protection: its saved state, how long it lasts, its record's flags, the text its end shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Protection {
    pub state: u32,
    pub seconds: f64,
    pub flags: u32,
    pub ended: (u32, u32),
}

/// Lifestone protection, as a player is resurrected.
pub const LIFESTONE_PROTECTION: Protection = Protection {
    state: protection::STATE,
    seconds: protection::SECONDS,
    flags: protection::FLAGS,
    ended: protection::ENDED,
};

/// A city's recall protection, as its recall ends.
pub const RECALL_PROTECTION: Protection = Protection {
    state: 0x7000_2014,
    seconds: 30.0,
    flags: 0x42,
    ended: (0x6E03_B375, 0x2500_000B),
};

/// The slots of a record (its state's singleton at `state_ref`).
#[must_use]
pub fn record_slots(e: &EffectState, state_ref: u32) -> Vec<Slot> {
    let cast = e.cast.to_bits();
    #[allow(clippy::cast_possible_truncation)]
    let (low, high) = (cast as u32, (cast >> 32) as u32);
    vec![
        // Never demoted.
        Slot::Time(-1.0),
        // The cast time as the 2017 servers wrote it: a double over two slots, the low one mixed.
        Slot::Tagged(tag::MIXED_LONG, low),
        Slot::Tagged(tag::HIGH, high),
        Slot::Long(e.caster),
        Slot::Tagged(tag::FLOAT, e.timeout.to_bits()),
        Slot::Tagged(tag::FLOAT, 0),
        Slot::Tagged(tag::FLOAT, e.spellcraft.to_bits()),
        Slot::Int(0),
        Slot::Int(u32::from(e.pk)),
        Slot::Ref(None),
        // Never promoted.
        Slot::Time(-1.0),
        Slot::Ref(Some(state_ref)),
        Slot::Long(e.acting),
        Slot::Int(e.skill),
        Slot::Long(e.item),
        Slot::Int(e.flags),
        Slot::Int(0),
        Slot::Int(0),
        Slot::Int(e.eid),
        Slot::Int(e.categories),
        Slot::Int(0),
    ]
}

/// The call that adds an effect: its record and its id.
#[must_use]
pub fn add_call(e: &EffectState) -> ServerMessage {
    let mut b = Builder::new();
    let root = b.reserve(RECORD);
    let state = b.singleton(e.state);
    b.fill(root, &record_slots(e, state));
    session::call(function::ADD, &[Arg::Object(b.graph()), Arg::Int(e.eid)])
        .expect("an effect record packs")
}

/// The call that takes an effect away.
#[must_use]
pub fn remove_call(eid: u32) -> ServerMessage {
    session::call(function::REMOVE, &[Arg::Int(eid)]).expect("an id packs")
}

/// The next effect id of a player (the last one given, counted on).
pub fn next_eid(p: &mut crate::world::Player) -> u32 {
    p.play.effect_counter = p.play.effect_counter.wrapping_add(1).max(1);
    p.play.effect_counter
}

/// How an effect is cast: by whom, at what spellcraft, as a fight's effect or not, from which
/// skill and item, and who gave it (its id and name, for the texts that name the giver).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Cast {
    pub caster: u64,
    pub spellcraft: f32,
    pub pk: bool,
    pub skill: u32,
    pub item: u64,
    pub giver: Option<(u64, Option<(u32, u32)>)>,
    /// It answers another effect, and so is not answered in turn: two holders can't go back and
    /// forth.
    pub answers: bool,
}

/// The class priority of a record.
fn priority(def: Option<&EffectDef>, e: &EffectState) -> f32 {
    def.and_then(|d| d.priority.at(e.spellcraft)).unwrap_or(0.0)
}

/// The top effects among records: per class, the highest priority, then the higher spellcraft,
/// then the later cast (a record of no class competes with none). Each class and its top record.
#[must_use]
pub fn tops(table: &crate::effect_defs::EffectTable, records: &[EffectState]) -> Vec<(u32, u32)> {
    let mut best: Vec<(u32, &EffectState)> = Vec::new();
    for e in records {
        let def = table.get(e.state);
        let class = def.map_or(0, |d| d.eq_class);
        if class == 0 {
            continue;
        }
        let better = |o: &EffectState| {
            let (a, b) = (priority(def, e), priority(table.get(o.state), o));
            a > b || (a == b && (e.spellcraft, e.cast) > (o.spellcraft, o.cast))
        };
        match best.iter_mut().find(|(c, _)| *c == class) {
            Some((_, o)) => {
                if better(o) {
                    *o = e;
                }
            }
            None => best.push((class, e)),
        }
    }
    best.into_iter().map(|(c, e)| (c, e.eid)).collect()
}

/// The records that change the player's qualities: the top one of each class and every record
/// of no class.
fn applying<'a>(
    table: &crate::effect_defs::EffectTable,
    records: &'a [EffectState],
) -> Vec<&'a EffectState> {
    let top = tops(table, records);
    records
        .iter()
        .filter(|e| {
            let class = table.get(e.state).map_or(0, |d| d.eq_class);
            class == 0 || top.iter().any(|(_, eid)| *eid == e.eid)
        })
        .collect()
}

/// What the player's effects make of one of its qualities (`type << 16 | quality`): the sum added
/// to it and the product multiplying it.
#[must_use]
pub fn modifier(
    table: &crate::effect_defs::EffectTable,
    records: &[EffectState],
    stat: u32,
) -> (f32, f32) {
    let (mut add, mut mult) = (0.0, 1.0);
    for e in applying(table, records) {
        let Some(d) = table.get(e.state) else {
            continue;
        };
        for (s, m, v) in d.changes(e.spellcraft) {
            if s == stat {
                if m {
                    mult *= v;
                } else {
                    add += v;
                }
            }
        }
    }
    (add, mult)
}

/// What a holder's effects make of its rating in a skill: each change in turn, a whole number of
/// points added or the rating multiplied, what is left never below nothing (2017: a change of 6.82
/// raised a rating of 40 to 46).
#[must_use]
pub fn skill_rating(
    table: &crate::effect_defs::EffectTable,
    records: &[EffectState],
    skill: u32,
    rating: u32,
) -> u32 {
    let stat = SKILL_STAT | skill;
    let mut out = i64::from(rating);
    for e in applying(table, records) {
        let Some(d) = table.get(e.state) else {
            continue;
        };
        for (s, multiplies, v) in d.changes(e.spellcraft) {
            if s == stat {
                #[allow(clippy::cast_precision_loss)]
                let then = if multiplies { out as f32 * v } else { v };
                let then = i64::from(dereth_primitives::num::to_i32(then.trunc()));
                out = if multiplies { then } else { out + then };
            }
        }
    }
    u32::try_from(out.max(0)).unwrap_or(rating)
}

/// A player's greatest health or vigor with its effects (the client counts them the same way).
#[must_use]
pub fn max_vital<S: Store>(s: &Server<S>, p: &crate::world::Player, max_quality: u32) -> i32 {
    // Focus's greatest is a base of 100 and the character's own focus quality above it.
    let base = if max_quality == quality::FOCUS_MAX {
        crate::focus::BASE + quals::int(&p.qualities, max_quality).unwrap_or(0)
    } else {
        quals::int(&p.qualities, max_quality).unwrap_or(100)
    };
    let (add, mult) = modifier(&s.effect_table, &p.play.effects, 0x4_0000 | max_quality);
    if add == 0.0 && (mult - 1.0).abs() < f32::EPSILON {
        return base;
    }
    #[allow(clippy::cast_precision_loss)]
    dereth_primitives::num::to_i32((base as f32 + add) * mult)
}

/// The damage a player's effects add to its hits.
#[must_use]
pub fn damage_added<S: Store>(s: &Server<S>, p: &crate::world::Player) -> i32 {
    let (add, _) = modifier(&s.effect_table, &p.play.effects, DAMAGE_ADD);
    dereth_primitives::num::to_i32(add)
}

/// What a player's effects add to its skills' costs (negative: what they take off).
#[must_use]
pub fn vigor_cost_added<S: Store>(s: &Server<S>, p: &crate::world::Player) -> f32 {
    modifier(&s.effect_table, &p.play.effects, VIGOR_COST_ADD).0
}

/// Put an effect on a player: what it does, as cast. Returns whether it did anything.
pub fn apply<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    state: u32,
    cast: Cast,
    clock: Clock,
) -> bool {
    apply_on(s, net, On::Player(world), state, cast, clock)
}

/// Put an effect on a player or a creature: what it does, as cast. Returns whether it did
/// anything (a creature that is gone or dead takes nothing; quests and recipes are players').
pub fn apply_on<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    on: On,
    state: u32,
    cast: Cast,
    clock: Clock,
) -> bool {
    let Some(def) = s.effect_table.get(state).cloned() else {
        s.unhandled
            .note(format!("an effect {state:08X} the server does not know"));
        return false;
    };
    if !present(s, on) {
        return false;
    }
    reply_to(s, net, on, &def, cast, clock);
    if warded(s, on, &def) {
        return false;
    }
    match (&def.kind, on) {
        (Kind::InstantVital { flags, change }, On::Player(world)) => {
            let amount =
                dereth_primitives::num::floor_to_i32(change.at(cast.spellcraft).unwrap_or(0.0));
            heal(s, net, world, &def, (*flags, amount), cast, clock);
            true
        }
        (Kind::InstantVital { flags, change }, On::Creature(id)) => {
            let amount =
                dereth_primitives::num::floor_to_i32(change.at(cast.spellcraft).unwrap_or(0.0));
            if def.fx != 0 {
                show_fx_on(s, net, on, def.fx);
            }
            if flags & vital::HEALTH != 0 {
                creature_health(s, net, id, amount, cast.caster, clock);
            }
            true
        }
        (Kind::Quest { quest, update }, On::Player(world)) => {
            let act = match update & 0xF000_0000 {
                crate::quests::update::BESTOW => Move::Bestow,
                crate::quests::update::COMPLETE => Move::Complete,
                crate::quests::update::FAIL => Move::Fail,
                crate::quests::update::INCREMENT => Move::Increment,
                _ => Move::Advance(update & 0x0FFF_FFFF),
            };
            let e = crate::quests::QuestEffect { quest: *quest, act };
            crate::quests::moves(s, net, world, e, true, clock);
            true
        }
        // A teleport is no record: its target goes when its time is up (2017: 10.0 s after the
        // effect was put on, with no add of it told).
        (Kind::Teleport(to), On::Player(world)) => {
            let after = def.duration.at(cast.spellcraft).unwrap_or(0.0);
            s.after(
                clock,
                f64::from(after),
                crate::Timer::SentAway { world, to: *to },
            );
            true
        }
        (Kind::GrantRecipe(recipe), On::Player(world)) => {
            crate::craft::learn(s, net, world, *recipe, clock);
            if def.fx != 0 {
                show_fx(s, net, world, def.fx);
            }
            if let (Some(name), Some((_, giver))) = (def.name, cast.giver) {
                let mut vars = vec![progress::nested(learned::EFFECT_VAR, progress::text(name))];
                if let Some(g) = giver {
                    vars.push(progress::nested(learned::GIVER_VAR, progress::text(g)));
                }
                progress::private(
                    net,
                    world,
                    &crate::chat::show_text(2, progress::text_with(learned::TEXT, vars)),
                );
            }
            true
        }
        (Kind::RemoveRecipe(recipe), On::Player(world)) => {
            crate::craft::forget(s, net, world, *recipe);
            true
        }
        (Kind::Taunt(share), On::Creature(id)) => {
            crate::ai::taunted(s, id, cast.caster, *share, clock.local.0);
            true
        }
        (
            Kind::Quest { .. } | Kind::GrantRecipe(_) | Kind::RemoveRecipe(_) | Kind::Teleport(_),
            On::Creature(_),
        )
        | (Kind::Taunt(_), On::Player(_))
        | (Kind::Aura(_), _) => false,
        // A pet is summoned as its skill's use ends ([`crate::pets`]), never as an effect; a
        // recall is its skill's trip ([`crate::skill_use`]).
        (Kind::Pet { .. } | Kind::Recall(_), _) => false,
        (Kind::Combo(c), _) => {
            combine(s, net, on, &def, c, cast, clock);
            true
        }
        (
            Kind::Other
            | Kind::Counter { .. }
            | Kind::Reply { .. }
            | Kind::Watcher(_)
            | Kind::Ward { .. },
            _,
        ) if !def.lasts() => false,
        _ => {
            add_on(s, net, on, &def, cast, Shape::TIMED, clock);
            true
        }
    }
}

/// Does a ward its target holds keep this effect off? The top of each class of ward draws, by its
/// chance at its spellcraft. A use's own effects come on in the order its data lists them, the ward
/// after what it would keep off, so it doesn't keep off its own.
fn warded<S: Store>(s: &mut Server<S>, on: On, def: &EffectDef) -> bool {
    let chances: Vec<f32> = applying(&s.effect_table, records_of(s, on))
        .into_iter()
        .filter_map(|r| match &s.effect_table.get(r.state)?.kind {
            Kind::Ward { classes, chance }
                if classes.is_empty() || classes.contains(&def.eq_class) =>
            {
                chance.at(r.spellcraft)
            }
            _ => None,
        })
        .collect();
    chances.into_iter().any(|c| s.rng.chance(c))
}

/// A harmful effect comes at its target from another: each reply the target holds may answer, by its
/// chance at its spellcraft, with its effect on who sent it, whether or not a ward keeps this one
/// off. An answer isn't answered. No recording holds a holder of one: their lists and chances are
/// the data's, and the rest our choice, the harmful alone being answered so that helping a holder
/// costs nothing.
fn reply_to<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    on: On,
    def: &EffectDef,
    cast: Cast,
    clock: Clock,
) {
    if cast.answers || def.internal_flags & crate::effect_defs::HARMFUL == 0 {
        return;
    }
    let holder = match on {
        On::Player(world) => s.players.get(&world).map_or(0, |p| p.character),
        On::Creature(id) => id,
    };
    if cast.caster == holder {
        return;
    }
    let replies: Vec<(u32, f32, EffectState)> = applying(&s.effect_table, records_of(s, on))
        .into_iter()
        .filter_map(|r| match &s.effect_table.get(r.state)?.kind {
            Kind::Reply {
                effect,
                chance,
                classes,
            } if classes.is_empty() || classes.contains(&def.eq_class) => {
                Some((*effect, chance.at(r.spellcraft)?, *r))
            }
            _ => None,
        })
        .collect();
    let source = conn_of(s, cast.caster).map_or(On::Creature(cast.caster), On::Player);
    for (effect, chance, record) in replies {
        if s.rng.chance(chance) {
            let answer = Cast {
                caster: holder,
                spellcraft: record.spellcraft,
                pk: matches!(on, On::Player(_)),
                answers: true,
                ..Cast::default()
            };
            apply_on(s, net, source, effect, answer, clock);
        }
    }
}

/// An attack has reached a player: each counter it holds may answer, by its chance at its
/// spellcraft, with its effect on the attacker. An effect's harm over time is no attack.
pub fn answer_attack<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    by: u64,
    clock: Clock,
) {
    let Some(p) = s.players.get(&world) else {
        return;
    };
    let me = p.character;
    let counters: Vec<(u32, f32, f32, u64)> = p
        .play
        .effects
        .iter()
        .filter_map(|r| match &s.effect_table.get(r.state)?.kind {
            Kind::Counter { effect, chance } => {
                let at = chance.at(r.spellcraft)?;
                Some((*effect, at, r.spellcraft, r.item))
            }
            _ => None,
        })
        .collect();
    for (effect, chance, spellcraft, item) in counters {
        if !s.rng.chance(chance) {
            continue;
        }
        let on = conn_of(s, by).map_or(On::Creature(by), On::Player);
        let cast = Cast {
            caster: me,
            spellcraft,
            pk: true,
            item,
            ..Cast::default()
        };
        apply_on(s, net, on, effect, cast, clock);
    }
}

/// What an attack's harm comes to once its target's effects have watched it: what is left of it
/// (below nothing, it heals), and what goes back to the attacker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Harm {
    pub left: i32,
    pub back: i32,
}

impl Harm {
    /// Harm no effect has touched.
    #[must_use]
    pub const fn plain(left: i32) -> Self {
        Self { left, back: 0 }
    }
}

/// A side of a watcher, drawn: nothing unless its chance comes up, else its amount at the spellcraft
/// plus up to its spread.
fn drawn<S: Store>(s: &mut Server<S>, side: &crate::effect_defs::Share, spellcraft: f32) -> f32 {
    if !s.rng.chance(side.chance) {
        return 0.0;
    }
    side.amount.at(spellcraft).unwrap_or(0.0) + s.rng.between(0.0, side.spread)
}

/// What the watchers a target holds make of an attack's harm, drawn as the attack is told: the top
/// of each class, the higher classes first, each seeing what those before it left. Each takes
/// from the harm and sends back, by its own draws, what its amounts come to of the harm it saw,
/// rounded up (2017: 317 of 317 strikes on a wearer of a fifth, 360 sending back 72, so reckoned in
/// single precision; two classes add, a tenth and 0.19 sending back 0.3). Sending back a player's
/// attack is multiplied by what the watcher says for that. A creature is not healed by it. What
/// is left is not stopped at nothing: an amount larger than the harm heals (2017: a strike told as
/// 106 health gained).
pub fn answer_harm<S: Store>(s: &mut Server<S>, on: On, harm: i32, by_a_player: bool) -> Harm {
    let mut watchers: Vec<(u32, crate::effect_defs::Watcher, f32)> =
        applying(&s.effect_table, records_of(s, on))
            .into_iter()
            .filter_map(|r| {
                let d = s.effect_table.get(r.state)?;
                match &d.kind {
                    Kind::Watcher(w) => Some((d.eq_class, (**w).clone(), r.spellcraft)),
                    _ => None,
                }
            })
            .collect();
    watchers.sort_by_key(|w| std::cmp::Reverse(w.0));
    let (mut left, mut back) = (harm, 0);
    for (_, w, spellcraft) in watchers {
        let mut sent = drawn(s, &w.sends_back, spellcraft);
        if by_a_player {
            sent *= w.against_a_player;
        }
        let absorbed = drawn(s, &w.absorbs, spellcraft);
        if !w.watches || left <= 0 {
            continue;
        }
        #[allow(clippy::cast_precision_loss)]
        let of = if w.shares { left as f32 } else { 1.0 };
        back += dereth_primitives::num::to_i32((of * sent).ceil());
        left -= dereth_primitives::num::to_i32((of * absorbed).ceil());
    }
    if matches!(on, On::Creature(_)) {
        left = left.max(0);
    }
    Harm { left, back }
}

/// What a target's effects sent back reaches its attacker: a player loses that health, and may die
/// of it; a creature or a pet is hurt by the target (2017: where it took a fifth of a creature's
/// health or more, 17 kills, the attacks alone came to 0.89 of it, with what went back 1.14; a
/// player's health fell by it, 51 of 98 alone between two updates).
pub(crate) fn harm_back<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    attacker: u64,
    target: u64,
    back: i32,
    clock: Clock,
) {
    if back <= 0 {
        return;
    }
    if let Some(world) = conn_of(s, attacker) {
        if s.players
            .get(&world)
            .is_some_and(|p| p.trip.is_some() || crate::combat::is_dead(p))
        {
            return;
        }
        let (share, was) = crate::combat::lose_health(s, net, world, back);
        crate::combat::wounded(s, net, attacker, share, was);
        crate::combat::after_hurt(s, net, world, target, clock);
    } else if s
        .things
        .get(attacker)
        .is_some_and(crate::things::Thing::alive)
    {
        if conn_of(s, target).is_some() {
            crate::combat::hurt_creature(s, net, attacker, target, back, clock);
        } else {
            crate::pets::struck(s, net, attacker, (target, 0), back, clock);
        }
    }
}

/// Is the one an effect is put on there (a player in the world, a creature alive)?
fn present<S: Store>(s: &Server<S>, on: On) -> bool {
    match on {
        On::Player(world) => s.players.contains_key(&world),
        On::Creature(id) => s.things.get(id).is_some_and(|t| t.alive()),
    }
}

/// The world connection of the player with this character, if it is in the world.
fn conn_of<S: Store>(s: &Server<S>, character: u64) -> Option<ConnId> {
    s.players
        .iter()
        .find(|(_, p)| p.character == character)
        .map(|(c, _)| *c)
}

/// The records on a player or a creature.
#[must_use]
pub fn records_of<S: Store>(s: &Server<S>, on: On) -> &[EffectState] {
    match on {
        On::Player(world) => s
            .players
            .get(&world)
            .map_or(&[][..], |p| p.play.effects.as_slice()),
        On::Creature(id) => s
            .creature_effects
            .on
            .get(&id)
            .map_or(&[][..], Vec::as_slice),
    }
}

fn records_mut<S: Store>(s: &mut Server<S>, on: On) -> Option<&mut Vec<EffectState>> {
    match on {
        On::Player(world) => s.players.get_mut(&world).map(|p| &mut p.play.effects),
        On::Creature(id) => Some(s.creature_effects.on.entry(id).or_default()),
    }
}

/// Does one of these records show this appearance?
fn shows<S: Store>(s: &Server<S>, records: &[EffectState], fx: u32) -> bool {
    records
        .iter()
        .any(|e| s.effect_table.get(e.state).is_some_and(|d| d.fx == fx))
}

/// Play an effect's appearance on a player for all who see it.
fn show_fx<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, fx: u32) {
    show_fx_on(s, net, On::Player(world), fx);
}

/// Play an effect's appearance on a player or a creature for all who see it.
fn show_fx_on<S: Store>(s: &mut Server<S>, net: &mut ServerNet, on: On, fx: u32) {
    let Some((id, sender)) = stamp_on(s, on) else {
        return;
    };
    let m = ServerMessage::DoFx(ObjectEvent {
        sender,
        action: Fx { fx, scalar: 1.0 },
    });
    crate::combat::to_all_seeing(s, net, id, &m);
}

/// Stop an effect's appearance on a player or a creature for all who see it.
fn stop_fx_on<S: Store>(s: &mut Server<S>, net: &mut ServerNet, on: On, fx: u32) {
    let Some((id, sender)) = stamp_on(s, on) else {
        return;
    };
    let m = ServerMessage::StopFx(ObjectEvent {
        sender,
        action: Word(fx),
    });
    crate::combat::to_all_seeing(s, net, id, &m);
}

/// The object an event about a player or a creature names, with its next visual stamp.
fn stamp_on<S: Store>(
    s: &mut Server<S>,
    on: On,
) -> Option<(u64, auberean_protocol::types::ObjectStamp)> {
    match on {
        On::Player(world) => {
            let p = s.players.get_mut(&world)?;
            let id = p.character;
            Some((id, crate::combat::player_event(p, ()).sender))
        }
        On::Creature(id) => {
            s.things.get(id)?;
            Some((id, crate::inventory::visual(s, id)))
        }
    }
}

/// An instant change to a player's vitals: its appearance, then for health the healed one's story
/// of it and its health (and how wounded it shows); for vigor or focus, that vital alone.
fn heal<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    def: &EffectDef,
    (flags, amount): (u32, i32),
    cast: Cast,
    clock: Clock,
) {
    if def.fx != 0 {
        show_fx(s, net, world, def.fx);
    }
    for (bit, cur, max) in [
        (vital::HEALTH, quality::HEALTH, quality::HEALTH_MAX),
        (vital::VIGOR, quality::VIGOR, quality::VIGOR_MAX),
        (vital::FOCUS, quality::FOCUS, quality::FOCUS_MAX),
    ] {
        if flags & bit != 0 {
            change_vital(
                s,
                net,
                world,
                (cur, max),
                amount,
                (cast.caster, cast.pk),
                clock,
            );
        }
    }
}

/// The story of a change to a creature's or a player's health, told by it: its caster's, one hook
/// (kind 1, the one changed, a hit, the change).
fn health_story(
    sender: auberean_protocol::types::ObjectStamp,
    caster: u64,
    amount: i32,
) -> ServerMessage {
    let mut hook = StoryHook {
        bits: HEAL_HOOK,
        target: sender.id,
        ..StoryHook::default()
    };
    hook.numbers[0] = Outcome::Hit.code().cast_signed();
    hook.numbers[3] = amount;
    ServerMessage::DoStory(ObjectEvent {
        sender,
        action: PhysicsStory {
            bits: PhysicsStory::ATTACKER,
            attacker: caster,
            hooks: vec![hook],
            ..PhysicsStory::default()
        },
    })
}

/// Change a creature's health by an amount: its story of it, then a hurt (which may kill it) or a
/// heal (not past its greatest health).
pub(crate) fn creature_health<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    id: u64,
    amount: i32,
    caster: u64,
    clock: Clock,
) {
    let Some(alive) = s.things.get(id).map(crate::things::Thing::alive) else {
        return;
    };
    if amount == 0 || (!alive && amount > 0) {
        return;
    }
    let sender = crate::inventory::visual(s, id);
    let told = health_story(sender, caster, amount);
    crate::combat::to_all_seeing(s, net, id, &told);
    // A dead creature's harm over time is still told; it changes nothing.
    if !alive {
        return;
    }
    if amount < 0 {
        crate::combat::hurt_creature(s, net, id, caster, -amount, clock);
        return;
    }
    let Some(t) = s.things.get_mut(id) else {
        return;
    };
    #[allow(clippy::cast_precision_loss)]
    let was = t.health as f32 / t.max_health.max(1) as f32;
    t.health = (t.health + amount).min(t.max_health.max(t.health));
    #[allow(clippy::cast_precision_loss)]
    let share = t.health as f32 / t.max_health.max(1) as f32;
    crate::combat::wounded(s, net, id, share, was);
}

/// Is a change to a player's health harm another player did it (an effect a player cast, on
/// another; told as PK damage too, and it may kill)? 2017: every harmful change between two
/// players, 63 of 63 ticks, gave the health lost as PK damage; no creature's harm to a player did
/// (49,170 of 49,170).
fn harm_by_player(me: u64, amount: i32, (caster, pk): (u64, bool)) -> bool {
    pk && amount < 0 && caster != me
}

/// Change a player's vital by an amount (a heal: positive), not past its greatest value; the
/// story of a health change tells the whole amount. Harm another player did is told as PK damage
/// too, and with no health left the player dies, the caster its killer.
fn change_vital<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    (cur, max_quality): (u32, u32),
    amount: i32,
    (caster, pk): (u64, bool),
    clock: Clock,
) {
    let Some(p) = s.players.get(&world) else {
        return;
    };
    let max = max_vital(s, p, max_quality).max(1);
    let was = quals::int(&p.qualities, cur).unwrap_or(max);
    let now = (was + amount).clamp(0, max.max(was));
    let by_player = cur == quality::HEALTH && harm_by_player(p.character, amount, (caster, pk));
    if cur == quality::HEALTH {
        let Some(p) = s.players.get_mut(&world) else {
            return;
        };
        let me = p.character;
        let sender = crate::combat::player_event(p, ()).sender;
        let mut told = health_story(sender, caster, amount);
        if by_player {
            crate::combat::with_pk_damage(&mut told, me);
        }
        crate::combat::to_all_seeing(s, net, me, &told);
    }
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    quals::set_int(&mut p.qualities, cur, now);
    progress::send_int(net, world, cur, now);
    if cur == quality::HEALTH {
        let me = p.character;
        #[allow(clippy::cast_precision_loss)]
        let (share, before) = (now as f32 / max as f32, was as f32 / max as f32);
        crate::combat::wounded(s, net, me, share, before);
        if by_player && now == 0 && was > 0 {
            crate::combat::after_hurt(s, net, world, caster, clock);
        }
    }
}

/// Add a lasting effect as a record: its call (a player's), its appearance when it is the first to
/// show it, the speed when it changes it; its end at its cast time plus its timeout, and its first
/// change over time. Returns its id.
pub fn add_on<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    on: On,
    def: &EffectDef,
    cast: Cast,
    shape: Shape,
    clock: Clock,
) -> Option<u32> {
    let eid = add_quietly(s, net, on, def, cast, shape, clock)?;
    if def.fx != 0 && shows_once(s, on, def.fx) {
        show_fx_on(s, net, on, def.fx);
    }
    Some(eid)
}

/// Is this appearance shown by one record alone (the one just added)?
fn shows_once<S: Store>(s: &Server<S>, on: On, fx: u32) -> bool {
    records_of(s, on)
        .iter()
        .filter(|e| s.effect_table.get(e.state).is_some_and(|d| d.fx == fx))
        .count()
        == 1
}

/// [`add_on`] but for the appearance, which the caller shows (or not) when it is due.
pub(crate) fn add_quietly<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    on: On,
    def: &EffectDef,
    cast: Cast,
    shape: Shape,
    clock: Clock,
) -> Option<u32> {
    let now = match on {
        On::Player(_) => net.game_time(clock.local),
        On::Creature(_) => clock.local.0,
    };
    let timeout = shape
        .timeout
        .unwrap_or_else(|| def.duration.at(cast.spellcraft).unwrap_or(f32::MAX));
    let speed_before = speeds_on(s, on);
    let over_time = matches!(def.kind, Kind::VitalOverTime { .. });
    let eid = match on {
        On::Player(world) => next_eid(s.players.get_mut(&world)?),
        On::Creature(_) => {
            let c = &mut s.creature_effects.counter;
            *c = c.wrapping_add(1).max(1);
            *c
        }
    };
    let e = EffectState {
        eid,
        state: def.state,
        cast: now,
        timeout,
        caster: cast.caster,
        spellcraft: cast.spellcraft,
        pk: cast.pk,
        acting: shape.acting,
        skill: cast.skill,
        item: cast.item,
        flags: shape.flags,
        // Filed by its definition's flags (2017: 54,046 of 54,046 records); a change over time
        // always beats.
        categories: shape.categories.unwrap_or(
            def.categories()
                | if over_time {
                    category::OVER_TIME
                } else {
                    category::ORDINARY
                },
        ),
    };
    records_mut(s, on)?.push(e);
    if let On::Player(world) = on {
        progress::private(net, world, &add_call(&e));
    }
    send_speed(s, net, on, speed_before, &e);
    schedule_on(s, on, &e, now, clock);
    if over_time {
        let first = f64::from(s.rng.between(0.0, 1.0));
        let (wait, timer) = match on {
            On::Player(world) => (
                first * TICK_FIRST,
                Timer::EffectTick {
                    world,
                    eid,
                    since: clock.local.0,
                },
            ),
            On::Creature(creature) => (
                first * CREATURE_TICK_FIRST,
                Timer::CreatureEffectTick {
                    creature,
                    eid,
                    since: clock.local.0,
                },
            ),
        };
        // A creature's change over time runs to its end, where its last tick is (a first tick due
        // after the end is held for the end to read).
        let wait = match on {
            On::Creature(_) if f64::from(timeout) <= wait => f64::from(timeout) + 1.0,
            _ => wait,
        };
        s.after(clock, wait, timer);
    }
    if let On::Player(world) = on {
        crate::world::keep_player(s, world);
    }
    Some(eid)
}

/// Schedule a record's end, from the time now (a player's game time, a creature's world time).
fn schedule_on<S: Store>(s: &mut Server<S>, on: On, e: &EffectState, now: f64, clock: Clock) {
    if e.timeout >= f32::MAX || e.flags & record_flag::INFINITE != 0 {
        return;
    }
    let left = (e.cast + f64::from(e.timeout) - now).max(0.0);
    let t = match on {
        On::Player(world) => Timer::EffectEnds { world, eid: e.eid },
        On::Creature(creature) => Timer::CreatureEffectEnds {
            creature,
            eid: e.eid,
        },
    };
    s.after(clock, left, t);
}

/// Schedule a player's record's end, from the game time now.
fn schedule<S: Store>(s: &mut Server<S>, world: ConnId, e: &EffectState, now: f64, clock: Clock) {
    schedule_on(s, On::Player(world), e, now, clock);
}

/// The speed and acceleration scales a player's or a creature's effects give it.
fn speeds_on<S: Store>(s: &Server<S>, on: On) -> (f32, f32) {
    let records = records_of(s, on);
    let v = modifier(&s.effect_table, records, speed::VELOCITY);
    let a = modifier(&s.effect_table, records, speed::ACCELERATION);
    ((1.0 + v.0) * v.1, (1.0 + a.0) * a.1)
}

/// The speed and acceleration scales a player's effects give it (1 and 1 with none).
#[must_use]
pub fn player_speed<S: Store>(s: &Server<S>, world: ConnId) -> (f32, f32) {
    speeds_on(s, On::Player(world))
}

/// The speed and acceleration scales a creature's effects give it (1 and 1 with none).
#[must_use]
pub fn creature_speed<S: Store>(s: &Server<S>, id: u64) -> (f32, f32) {
    speeds_on(s, On::Creature(id))
}

/// What a creature's effects make of one of its qualities (`type << 16 | quality`): the sum added
/// to it and the product multiplying it.
#[must_use]
pub fn creature_modifier<S: Store>(s: &Server<S>, id: u64, stat: u32) -> (f32, f32) {
    modifier(&s.effect_table, records_of(s, On::Creature(id)), stat)
}

/// Does a record change speed or acceleration?
fn touches_speed(table: &crate::effect_defs::EffectTable, e: &EffectState) -> bool {
    table.get(e.state).is_some_and(|d| {
        d.changes(e.spellcraft)
            .into_iter()
            .any(|(stat, _, _)| stat == speed::VELOCITY || stat == speed::ACCELERATION)
    })
}

/// Tell the speed and acceleration scales of a player or a creature, for all who see it, each
/// that its effects changed (2017: a creature held still is sent its speed alone). A player is
/// told both whenever a record that changes either is put on or taken off, changed or not (2017:
/// 11,673 of 11,783 of a player's own scales came with a record's coming or going, and most
/// repeated the scale before, a buff cast again).
fn send_speed<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    on: On,
    before: (f32, f32),
    record: &EffectState,
) {
    let (v, a) = speeds_on(s, on);
    let always = matches!(on, On::Player(_)) && touches_speed(&s.effect_table, record);
    if always || (v - before.0).abs() >= 1e-6 {
        if let Some((id, sender)) = stamp_on(s, on) {
            let m = ServerMessage::VelocityScale(ObjectEvent {
                sender,
                action: WireScale(v),
            });
            crate::combat::to_all_seeing(s, net, id, &m);
        }
    }
    if always || (a - before.1).abs() >= 1e-6 {
        if let Some((id, sender)) = stamp_on(s, on) {
            let m = ServerMessage::AccelerationScale(ObjectEvent {
                sender,
                action: WireScale(a),
            });
            crate::combat::to_all_seeing(s, net, id, &m);
        }
    }
    // A record that changes the jump tells the jump scale at its coming and its going (2017: 11 of
    // 11, 1.6 at a wild magic's add and 1.0 at its end).
    let jumps = s.effect_table.get(record.state).is_some_and(|d| {
        d.changes(record.spellcraft)
            .into_iter()
            .any(|(stat, _, _)| stat == speed::JUMP)
    });
    if jumps {
        let j = modifier(&s.effect_table, records_of(s, on), speed::JUMP);
        if let Some((id, sender)) = stamp_on(s, on) {
            let m = ServerMessage::JumpScale(ObjectEvent {
                sender,
                action: WireScale((1.0 + j.0) * j.1),
            });
            crate::combat::to_all_seeing(s, net, id, &m);
        }
    }
}

/// Take a record off a player: its client told, its speed sent when that changes, its appearance
/// stopped when no other record shows it.
pub fn remove<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, eid: u32) -> bool {
    remove_on(s, net, On::Player(world), eid)
}

/// Take a record off a player or a creature: a player's client told, the speed sent when that
/// changes, its appearance stopped when no other record shows it.
pub fn remove_on<S: Store>(s: &mut Server<S>, net: &mut ServerNet, on: On, eid: u32) -> bool {
    match take_off(s, net, on, eid) {
        Some(stopped) => {
            if let Some(fx) = stopped {
                stop_fx_on(s, net, on, fx);
            }
            true
        }
        None => false,
    }
}

/// Take a record off and tell what that changes, but for its appearance: the appearance no other
/// record shows now, which the caller stops when it is due. `None`: no such record.
pub(crate) fn take_off<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    on: On,
    eid: u32,
) -> Option<Option<u32>> {
    let speed_before = speeds_on(s, on);
    let records = records_mut(s, on)?;
    let i = records.iter().position(|e| e.eid == eid)?;
    let e = records.remove(i);
    if let On::Player(world) = on {
        progress::private(net, world, &remove_call(eid));
    }
    send_speed(s, net, on, speed_before, &e);
    let fx = s.effect_table.get(e.state).map_or(0, |d| d.fx);
    Some((fx != 0 && !shows(s, records_of(s, on), fx)).then_some(fx))
}

/// A player's record's time is up: it is taken off, if it is still on.
pub fn ends<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, eid: u32) {
    if remove(s, net, world, eid) {
        crate::world::keep_player(s, world);
    }
}

/// A creature's record's time is up: a change over time's last tick (for the seconds since the
/// one before), then the record taken off. A creature gone from the world takes its records along.
pub fn creature_ends<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    creature: u64,
    eid: u32,
    clock: Clock,
) {
    if s.things.get(creature).is_none() {
        s.creature_effects.on.remove(&creature);
        return;
    }
    let on = On::Creature(creature);
    let Some(e) = records_of(s, on).iter().find(|e| e.eid == eid).copied() else {
        return;
    };
    if let Some(since) = last_tick(s, creature, eid) {
        creature_change(s, net, creature, &e, clock.local.0 - since, clock);
    }
    remove_on(s, net, on, eid);
    let empty = records_of(s, on).is_empty();
    if empty {
        s.creature_effects.on.remove(&creature);
    }
}

/// When a creature's change over time last ticked (its add, before the first).
fn last_tick<S: Store>(s: &Server<S>, creature: u64, eid: u32) -> Option<f64> {
    s.timers.iter().find_map(|(_, t)| match t {
        Timer::CreatureEffectTick {
            creature: c,
            eid: e,
            since,
        } if *c == creature && *e == eid => Some(*since),
        _ => None,
    })
}

/// A creature's change over time over so many seconds: its rate times them, told by the creature
/// (nothing for a dead one).
fn creature_change<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    creature: u64,
    e: &EffectState,
    seconds: f64,
    clock: Clock,
) {
    let Some(Kind::VitalOverTime { flags, per_second }) =
        s.effect_table.get(e.state).map(|d| d.kind.clone())
    else {
        return;
    };
    if flags & vital::HEALTH == 0 {
        return;
    }
    let rate = per_second.at(e.spellcraft).unwrap_or(0.0);
    let amount = dereth_primitives::num::to_i32_f64((f64::from(rate) * seconds).floor());
    creature_health(s, net, creature, amount, e.caster, clock);
}

/// A creature's change over time ticks: its rate times the seconds since its last; the next a few
/// seconds on, while it runs.
pub fn creature_tick<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    creature: u64,
    eid: u32,
    since: f64,
    clock: Clock,
) {
    let Some(e) = records_of(s, On::Creature(creature))
        .iter()
        .find(|e| e.eid == eid)
        .copied()
    else {
        return;
    };
    creature_change(s, net, creature, &e, clock.local.0 - since, clock);
    let wait = f64::from(s.rng.between(0.0, 1.0));
    let next = CREATURE_TICK_LEAST + wait * (CREATURE_TICK_MOST - CREATURE_TICK_LEAST);
    let left = e.cast + f64::from(e.timeout) - clock.local.0;
    // The last tick is the record's end's.
    let timer = Timer::CreatureEffectTick {
        creature,
        eid,
        since: clock.local.0,
    };
    if left > next {
        s.after(clock, next, timer);
    } else {
        // Held for the end to read when the last one was.
        s.after(clock, left + 1.0, timer);
    }
}

/// A player's change over time's tick: its rate times the seconds since its last, to its vitals;
/// the next tick a few seconds on.
pub fn tick<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    eid: u32,
    since: f64,
    clock: Clock,
) {
    let Some(e) = s
        .players
        .get(&world)
        .and_then(|p| p.play.effects.iter().find(|e| e.eid == eid))
        .copied()
    else {
        return;
    };
    let Some(Kind::VitalOverTime { flags, per_second }) =
        s.effect_table.get(e.state).map(|d| d.kind.clone())
    else {
        return;
    };
    let dead = s
        .players
        .get(&world)
        .is_some_and(|p| crate::combat::is_dead(p) || p.dying.is_some());
    let rate = per_second.at(e.spellcraft).unwrap_or(0.0);
    let amount =
        dereth_primitives::num::to_i32_f64((f64::from(rate) * (clock.local.0 - since)).floor());
    if !dead {
        if amount != 0 {
            for (bit, cur, max) in [
                (vital::HEALTH, quality::HEALTH, quality::HEALTH_MAX),
                (vital::VIGOR, quality::VIGOR, quality::VIGOR_MAX),
            ] {
                if flags & bit != 0 {
                    change_vital(s, net, world, (cur, max), amount, (e.caster, e.pk), clock);
                }
            }
        }
    } else if flags & vital::HEALTH != 0 {
        // A dead player's harm over time from another player is still told, and changes nothing
        // (2017: Gromlin's, 8 hooks in 5 ticks between his death and his return).
        if let Some(p) = s.players.get_mut(&world) {
            let me = p.character;
            if harm_by_player(me, amount, (e.caster, e.pk)) {
                let sender = crate::combat::player_event(p, ()).sender;
                let mut told = health_story(sender, e.caster, amount);
                crate::combat::with_pk_damage(&mut told, me);
                crate::combat::to_all_seeing(s, net, me, &told);
            }
        }
    }
    let wait = s.rng.between(0.0, 1.0);
    s.after(
        clock,
        TICK_LEAST + f64::from(wait) * (TICK_MOST - TICK_LEAST),
        Timer::EffectTick {
            world,
            eid,
            since: clock.local.0,
        },
    );
}

/// A combination put on a player or a creature: the effects it looks for found or not, and what
/// it adds either way (see [`crate::effect_defs::Combo`]).
fn combine<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    on: On,
    def: &EffectDef,
    c: &Combo,
    cast: Cast,
    clock: Clock,
) {
    let found: Vec<u32> = records_of(s, on)
        .iter()
        .filter(|e| {
            s.effect_table.get(e.state).is_some_and(|d| {
                c.types.contains(&d.enum_value)
                    || (d.eq_class != 0 && c.classes.contains(&d.eq_class))
            })
        })
        .map(|e| e.eid)
        .collect();
    let caster = conn_of(s, cast.caster);
    let (add, give_back, adjust) = if found.is_empty() {
        // Its caster's client is told it found nothing: an empty private effect (2017: four with
        // each landed Lightning Bolt, one for each of its combinations, 1,413 of 1,418).
        if let Some(world) = caster {
            progress::private(net, world, &ServerMessage::PrivateFx(PrivateFx(Vec::new())));
        }
        (
            c.if_not_present,
            c.give_back_if_not_present,
            c.adjust_if_not_present,
        )
    } else {
        if let Some(world) = caster {
            progress::private(
                net,
                world,
                &ServerMessage::PrivateFx(PrivateFx(vec![COMBO_FX])),
            );
        }
        let gone: &[u32] = if def.external_flags & combo_flag::REMOVE_ALL != 0 {
            &found
        } else if def.external_flags & combo_flag::REMOVE_OLDEST != 0 {
            &found[..1]
        } else {
            &[]
        };
        for eid in gone {
            remove_on(s, net, on, *eid);
        }
        (c.if_present, c.give_back_if_present, c.adjust_if_present)
    };
    #[allow(clippy::cast_precision_loss)]
    let adjusted = Cast {
        spellcraft: cast.spellcraft + adjust as f32,
        ..cast
    };
    if let Some(state) = add {
        apply_on(s, net, on, state, adjusted, clock);
    }
    if let (Some(state), Some(world)) = (give_back, caster) {
        apply_on(s, net, On::Player(world), state, adjusted, clock);
    }
}

/// The records that ran out (or went with the player's last session) by the game time `now`.
pub fn prune(play: &mut crate::state::PlayState, now: f64) {
    play.effects.retain(|e| {
        e.flags & record_flag::LOGOUT == 0
            && (e.timeout >= f32::MAX || e.cast + f64::from(e.timeout) > now)
    });
}

/// A player enters: its records are scheduled to run out (and to change vitals over time) on
/// the game's clock.
pub fn enter<S: Store>(s: &mut Server<S>, net: &ServerNet, world: ConnId, clock: Clock) {
    let now = net.game_time(clock.local);
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    prune(&mut p.play, now);
    let records = p.play.effects.clone();
    for e in &records {
        schedule(s, world, e, now, clock);
        if matches!(
            s.effect_table.get(e.state).map(|d| &d.kind),
            Some(Kind::VitalOverTime { .. })
        ) {
            s.after(
                clock,
                TICK_LEAST,
                Timer::EffectTick {
                    world,
                    eid: e.eid,
                    since: clock.local.0,
                },
            );
        }
    }
}

/// A resurrection takes off the records whose effects a death ends, as it is asked for (2017: of the
/// records still running at 227 requests, 1,664 of 1,664 such were gone before the protection's add,
/// 192 of 192 worn ones and 432 of 432 others stayed). A ride ends as rides end. Returns the
/// appearances no record shows now, for the caller to stop when they are due.
pub fn resurrected<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId) -> Vec<u32> {
    let gone: Vec<u32> = s.players.get(&world).map_or_else(Vec::new, |p| {
        p.play
            .effects
            .iter()
            // An item's effect stays while the item is worn.
            .filter(|e| e.flags & record_flag::EQUIPPED == 0)
            .filter(|e| {
                s.effect_table
                    .get(e.state)
                    .is_some_and(EffectDef::removed_on_death)
            })
            .map(|e| e.eid)
            .collect()
    });
    let mut stop = Vec::new();
    for eid in gone {
        // A ride ends as rides end.
        if !crate::mounts::record_ends(s, net, world, eid) {
            if let Some(Some(fx)) = take_off(s, net, On::Player(world), eid) {
                stop.push(fx);
            }
        }
    }
    stop
}

/// The effect registry a session start carries: the records, the id counter, each class's top
/// effect, the records by category, the qualities and appearances the top effects apply.
#[must_use]
pub fn registry<S: Store>(s: &Server<S>, p: &crate::world::Player) -> session::Registry {
    let records = p.play.effects.clone();
    let top = tops(&s.effect_table, &records);
    let mut categories: Vec<(u32, u32)> = records.iter().map(|e| (e.categories, e.eid)).collect();
    categories.sort_unstable();
    let mut qualities: Vec<(u32, u32)> = Vec::new();
    let mut fx: Vec<(u32, u32)> = Vec::new();
    for e in applying(&s.effect_table, &records) {
        let Some(d) = s.effect_table.get(e.state) else {
            continue;
        };
        for (stat, _, _) in d.changes(e.spellcraft) {
            match qualities.iter_mut().find(|(k, _)| *k == stat) {
                Some(q) => q.1 += 1,
                None => qualities.push((stat, 1)),
            }
        }
        if d.fx != 0 {
            match fx.iter_mut().find(|(k, _)| *k == d.fx) {
                Some(q) => q.1 += 1,
                None => fx.push((d.fx, 1)),
            }
        }
    }
    session::Registry {
        counter: p.play.effect_counter.max(1),
        records,
        top,
        categories,
        qualities,
        fx,
    }
}

/// A player's protection: the effect's id, when it runs out (on the server's clock), and the text
/// its end shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Protected {
    pub eid: u32,
    pub until: f64,
    pub ended: (u32, u32),
}

/// Protect a player as it is resurrected: the effect added on its client, and its end in three
/// minutes.
pub fn protect<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, clock: Clock) {
    protect_with(s, net, world, LIFESTONE_PROTECTION, (0, 0), clock);
}

/// Protect a player: the effect added on its client (cast by a caster, from a skill's saved state:
/// none for a resurrection's), and its end when its time is up. A protection it had already is
/// replaced.
pub fn protect_with<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    kind: Protection,
    (caster, skill): (u64, u32),
    clock: Clock,
) {
    let cast = net.game_time(clock.local);
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let eid = next_eid(p);
    p.protected = Some(Protected {
        eid,
        until: clock.local.0 + kind.seconds,
        ended: kind.ended,
    });
    #[allow(clippy::cast_possible_truncation)]
    let e = EffectState {
        eid,
        state: kind.state,
        cast,
        timeout: kind.seconds as f32,
        caster,
        spellcraft: 0.0,
        pk: false,
        acting: 0,
        skill,
        item: 0,
        flags: kind.flags,
        categories: protection::CATEGORIES,
    };
    net.send(
        world,
        auberean_protocol::queue::EVENT,
        BlobKind::PRIVATE,
        add_call(&e).encode(),
    );
    // A player whom kingdoms may fight anywhere may not be fought while protected: its ethereality
    // follows.
    crate::kingdoms::settle_ethereality(s, net, world);
    s.after(clock, kind.seconds, Timer::ProtectionEnds { world, eid });
}

/// Is a player under lifestone protection?
#[must_use]
pub fn protected(p: &crate::world::Player) -> bool {
    p.protected.is_some()
}

/// A player's protection ends (if it has one): the text, the effect taken away; its glow stopped
/// for everyone who sees it once `then` has been sent (the caller's messages between the effect and
/// the glow go in that closure).
pub fn unprotect<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    then: impl FnOnce(&mut Server<S>, &mut ServerNet),
) -> bool {
    let Some(p) = s.players.get_mut(&world) else {
        return false;
    };
    let Some(prot) = p.protected.take() else {
        return false;
    };
    let ended = TextInfo {
        string_id: prot.ended.0,
        table: prot.ended.1,
        ..TextInfo::default()
    };
    let private = |net: &mut ServerNet, m: &ServerMessage| {
        net.send(
            world,
            auberean_protocol::queue::EVENT,
            BlobKind::PRIVATE,
            m.encode(),
        );
    };
    private(net, &crate::chat::show_text(protection::ENDED_KIND, ended));
    private(net, &remove_call(prot.eid));
    then(s, net);
    // Its ethereality follows, before its glow stops (2017: in the same instant, 26 of 26 players
    // whom kingdoms may fight anywhere).
    crate::kingdoms::settle_ethereality(s, net, world);
    let Some(p) = s.players.get_mut(&world) else {
        return true;
    };
    p.visual = p.visual.wrapping_add(1);
    let stop = ServerMessage::StopFx(ObjectEvent {
        sender: auberean_protocol::types::ObjectStamp {
            id: p.character,
            instance_stamp: p.instance,
            other_stamp: p.visual,
        },
        action: Word(protection::GLOW),
    });
    let id = p.character;
    crate::combat::to_all_seeing(s, net, id, &stop);
    true
}

/// A protection's three minutes are up: it ends, if it is still that one.
pub fn runs_out<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, eid: u32) {
    let current = s
        .players
        .get(&world)
        .and_then(|p| p.protected)
        .is_some_and(|p| p.eid == eid);
    if current {
        unprotect(s, net, world, |_, _| {});
    }
}

/// The native type of a list of words (a pulse's missed records' ids).
const WORD_LIST: u16 = 10;

/// The call turning a toggled skill on or off on its user's client.
#[must_use]
pub fn toggle_call(on: bool, skill: u32) -> ServerMessage {
    session::call(
        function::TOGGLE,
        &[Arg::Int(u32::from(on)), Arg::Int(skill)],
    )
    .expect("packs")
}

/// The call telling a player that pulses missed these records of its.
#[must_use]
pub fn pulse_missed_call(eids: &[u32]) -> ServerMessage {
    let mut b = Builder::new();
    b.native(
        WORD_LIST,
        crate::graph::list(
            auberean_script::library::Elem::Word,
            eids.iter()
                .map(|e| auberean_script::native_object::Item::Word(*e))
                .collect(),
        ),
    );
    session::call(function::PULSE_MISSED, &[Arg::Object(b.graph())]).expect("packs")
}

/// The record of an aura of this definition a player holds, if it is on.
#[must_use]
pub fn aura_held<S: Store>(s: &Server<S>, world: ConnId, aura: u32) -> Option<EffectState> {
    s.players
        .get(&world)?
        .play
        .effects
        .iter()
        .find(|e| e.state == aura && e.flags == AURA_FLAGS)
        .copied()
}

/// An aura's definition, when this saved state is one.
fn aura_def<S: Store>(s: &Server<S>, state: u32) -> Option<Aura> {
    match s.effect_table.get(state).map(|d| &d.kind) {
        Some(Kind::Aura(a)) => Some(a.clone()),
        _ => None,
    }
}

/// Is this saved state an aura's?
#[must_use]
pub fn is_aura<S: Store>(s: &Server<S>, state: u32) -> bool {
    aura_def(s, state).is_some()
}

/// Turn an aura on: its first pulse's records on its user, then its own record (2017's order: the
/// aura's id is the lower, its record sent second). Returns the appearances the pulse shows, for
/// the caller to show when they are due (after its toggle and the skill's information).
pub fn aura_on<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    aura: u32,
    skill_state: u32,
    spellcraft: f32,
    clock: Clock,
) -> Vec<u32> {
    let Some(a) = aura_def(s, aura) else {
        return Vec::new();
    };
    let Some(p) = s.players.get_mut(&world) else {
        return Vec::new();
    };
    let me = p.character;
    let eid = next_eid(p);
    let fx = if pulses_on_held(&a) {
        Vec::new()
    } else {
        pulse_records(s, net, world, me, &a, spellcraft, clock)
    };
    let e = EffectState {
        eid,
        state: aura,
        cast: net.game_time(clock.local),
        timeout: f32::MAX,
        caster: me,
        spellcraft,
        pk: false,
        acting: 0,
        skill: skill_state,
        item: 0,
        flags: AURA_FLAGS,
        categories: AURA_CATEGORIES,
    };
    if let Some(p) = s.players.get_mut(&world) {
        p.play.effects.push(e);
    }
    progress::private(net, world, &add_call(&e));
    let first = f64::from(s.rng.between(0.0, 1.0)) * PULSE_MOST;
    s.after(clock, first, Timer::AuraPulse { world, eid });
    crate::world::keep_player(s, world);
    fx
}

/// A pulse's records on its user (the players it reaches): each pulsed effect, timed, pulsed. The
/// appearances it shows anew.
fn pulse_records<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    me: u64,
    a: &Aura,
    spellcraft: f32,
    clock: Clock,
) -> Vec<u32> {
    let mut fx = Vec::new();
    for state in &a.pulsed {
        let Some(d) = s.effect_table.get(*state).cloned() else {
            continue;
        };
        let cast = Cast {
            caster: me,
            spellcraft,
            pk: true,
            ..Cast::default()
        };
        let shape = Shape {
            flags: PULSE_FLAGS,
            categories: None,
            timeout: None,
            acting: me,
        };
        if warded(s, On::Player(world), &d) {
            continue;
        }
        if add_quietly(s, net, On::Player(world), &d, cast, shape, clock).is_some()
            && d.fx != 0
            && shows_once(s, On::Player(world), d.fx)
        {
            fx.push(d.fx);
        }
    }
    fx
}

/// Whether an aura's pulses go on what its user holds, not on anyone: no record of one is ever on a
/// player (2017: 0 records of their pulses beside 11 of such auras), and the item's examination
/// lists them.
fn pulses_on_held(a: &Aura) -> bool {
    a.targets & (aura_target::WEAPON | aura_target::SHIELD | aura_target::ITEM) != 0
}

/// The pulsed effects auras hold on a thing in a player's hands, each with its aura's spellcraft:
/// a weapon's or a shield's by what the aura names, anything held by an aura for items, from the
/// player's own auras and those of its fellows within the aura's range (2017: Human skills' auras
/// named on 6 weapons of other races' players, in 8 captures). Of one effect from several auras,
/// the strongest.
#[must_use]
pub fn on_held<S: Store>(s: &Server<S>, player: u64, item: u64, shield: bool) -> Vec<(u32, f32)> {
    let Some(p) = s.players.values().find(|p| p.character == player) else {
        return Vec::new();
    };
    if !crate::skill_use::wielded(s, player)
        .iter()
        .any(|(i, _)| *i == item)
    {
        return Vec::new();
    }
    let kind = if shield {
        aura_target::SHIELD
    } else {
        aura_target::WEAPON
    };
    let mut held: Vec<(u32, f32)> = Vec::new();
    let fellows: Vec<&crate::world::Player> = s
        .fellowships
        .of(player)
        .into_iter()
        .flat_map(|i| s.fellowships.list[i].fellows.iter())
        .filter(|f| f.id != player && p.scene_drawn && p.trip.is_none())
        .filter_map(|f| s.players.values().find(|o| o.character == f.id))
        .collect();
    for from in std::iter::once(p).chain(fellows) {
        let auras = from
            .play
            .effects
            .iter()
            .filter(|e| e.flags == AURA_FLAGS)
            .filter_map(|e| Some((aura_def(s, e.state)?, e.spellcraft)))
            .filter(|(a, _)| a.targets & (kind | aura_target::ITEM) != 0)
            .filter(|(a, _)| {
                from.character == player
                    || crate::presence::distance(&p.position, &from.position)
                        .is_some_and(|d| d <= a.range)
            });
        for (a, spellcraft) in auras {
            for state in a.pulsed {
                match held.iter_mut().find(|(st, _)| *st == state) {
                    Some(h) => h.1 = h.1.max(spellcraft),
                    None => held.push((state, spellcraft)),
                }
            }
        }
    }
    held
}

/// The effects held on a thing in the world: those skills put on it, wherever it is, and those auras
/// hold on it when a player has it in hand.
#[must_use]
pub fn on_thing<S: Store>(s: &Server<S>, t: &crate::things::Thing) -> Vec<(u32, f32)> {
    let mut held: Vec<(u32, f32)> = running_on_item(s, t)
        .into_iter()
        .map(|e| (e.state, e.spellcraft))
        .collect();
    held.extend(auras_on_thing(s, t));
    held
}

/// The effects auras hold on a thing in the world: nothing unless a player has it in hand.
#[must_use]
pub fn auras_on_thing<S: Store>(s: &Server<S>, t: &crate::things::Thing) -> Vec<(u32, f32)> {
    let Some((holder, _)) = t.wielded else {
        return Vec::new();
    };
    let shield = crate::skill_use::implement_of(&s.catalog, t.template)
        == Some(crate::skill_use::implement::SHIELD);
    on_held(s, holder, t.id, shield)
}

/// The class of an effect that attunes what it is on to whoever holds it.
const ATTUNEMENT_CLASS: u32 = 114;

/// The effects skills put on an item that are still running: the top one of each class, by its
/// priority, then its spellcraft, then the later to end.
#[must_use]
pub fn running_on_item<S: Store>(
    s: &Server<S>,
    t: &crate::things::Thing,
) -> Vec<crate::things::TimedEffect> {
    let mut top: Vec<(u32, f32, crate::things::TimedEffect)> = Vec::new();
    for e in t.timed.iter().filter(|e| e.until > s.game_now) {
        let Some(d) = s.effect_table.get(e.state) else {
            continue;
        };
        let priority = d.priority.at(e.spellcraft).unwrap_or(0.0);
        let rank = |p: f32, x: &crate::things::TimedEffect| (p, x.spellcraft, x.until);
        match top.iter_mut().find(|(class, _, _)| *class == d.eq_class) {
            Some(held) => {
                if rank(priority, e) > rank(held.1, &held.2) {
                    (held.1, held.2) = (priority, *e);
                }
            }
            None => top.push((d.eq_class, priority, *e)),
        }
    }
    top.into_iter().map(|(_, _, e)| e).collect()
}

/// Does an effect that attunes an item run on this one?
#[must_use]
pub fn attuned_by_effect<S: Store>(s: &Server<S>, t: &crate::things::Thing) -> bool {
    t.timed.iter().any(|e| {
        e.until > s.game_now
            && s.effect_table
                .get(e.state)
                .is_some_and(|d| d.eq_class == ATTUNEMENT_CLASS)
    })
}

/// A message about an item to all who see it: its holder alone unless it is in hand.
fn tell_of_item<S: Store>(s: &mut Server<S>, net: &mut ServerNet, item: u64, m: &ServerMessage) {
    let Some(t) = s.things.get(item) else {
        return;
    };
    let holder = t.wielded.or(t.container).map(|(holder, _)| holder);
    if let Some(world) = holder.and_then(|h| crate::social::world_of(s, h)) {
        crate::inventory::to_holder(s, net, world, item, m);
    }
}

/// A skill puts an effect on an item, for its time on the game clock whoever holds it after. An item
/// carries one of each effect: a new one takes the place of an older one no stronger, and a weaker
/// one changes nothing. Its look plays as the first effect showing it goes on (2017: 25 of 32 first
/// stories on an item played it, 2 of 23 with one running), and stops as the last ends.
pub fn put_on_item<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    item: u64,
    state: u32,
    spellcraft: f32,
    clock: Clock,
) {
    let Some(def) = s.effect_table.get(state).cloned() else {
        s.unhandled
            .note(format!("an effect {state:08X} the server does not know"));
        return;
    };
    let Some(seconds) = def.duration.at(spellcraft) else {
        return;
    };
    let now = net.game_time(clock.local);
    let until = now + f64::from(seconds);
    let Some(t) = s.things.get_mut(item) else {
        return;
    };
    t.timed.retain(|e| e.until > now);
    if t.timed
        .iter()
        .any(|e| e.state == state && e.spellcraft > spellcraft)
    {
        return;
    }
    let showing = def.fx != 0
        && s.things.get(item).is_some_and(|t| {
            t.timed
                .iter()
                .any(|e| s.effect_table.get(e.state).is_some_and(|d| d.fx == def.fx))
        });
    let Some(t) = s.things.get_mut(item) else {
        return;
    };
    t.timed.retain(|e| e.state != state);
    t.timed.push(crate::things::TimedEffect {
        state,
        spellcraft,
        until,
    });
    if def.fx != 0 && !showing {
        let m = ServerMessage::DoFx(ObjectEvent {
            sender: crate::inventory::visual(s, item),
            action: Fx {
                fx: def.fx,
                scalar: 1.0,
            },
        });
        tell_of_item(s, net, item, &m);
        s.after(
            clock,
            f64::from(seconds),
            crate::Timer::ItemEffectEnds { item, fx: def.fx },
        );
    }
}

/// The time of the look an item's effects show is up: it stops, unless an effect showing it runs on.
pub fn item_effect_ends<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    item: u64,
    fx: u32,
    clock: Clock,
) {
    let now = net.game_time(clock.local);
    let Some(t) = s.things.get_mut(item) else {
        return;
    };
    t.timed.retain(|e| e.until > now);
    let last = s.things.get(item).and_then(|t| {
        t.timed
            .iter()
            .filter(|e| s.effect_table.get(e.state).is_some_and(|d| d.fx == fx))
            .map(|e| e.until)
            .max_by(f64::total_cmp)
    });
    if let Some(last) = last {
        s.after(clock, last - now, crate::Timer::ItemEffectEnds { item, fx });
        return;
    }
    let m = ServerMessage::StopFx(ObjectEvent {
        sender: crate::inventory::visual(s, item),
        action: Word(fx),
    });
    tell_of_item(s, net, item, &m);
}

/// What these effects on an item multiply a stat of it by.
#[must_use]
pub fn held_multiplier<S: Store>(s: &Server<S>, held: &[(u32, f32)], stat: u32) -> f32 {
    held.iter()
        .filter_map(|(state, spellcraft)| Some((s.effect_table.get(*state)?, *spellcraft)))
        .flat_map(|(d, spellcraft)| d.changes(spellcraft))
        .filter(|(st, multiply, _)| *st == stat && *multiply)
        .map(|(_, _, by)| by)
        .product()
}

/// A weapon's greatest damage under these effects, rounded down: each multiplier, then what is added
/// (2017: the shown span of 23 of 23 weapons plain and under one of them, the least following from
/// the greatest by the weapon's variance; hits rolled that span, 44 wide where its plain span is 35).
#[must_use]
pub fn held_damage<S: Store>(s: &Server<S>, held: &[(u32, f32)], most: i32) -> i32 {
    let added: f32 = held
        .iter()
        .filter_map(|(state, spellcraft)| Some((s.effect_table.get(*state)?, *spellcraft)))
        .flat_map(|(d, spellcraft)| d.changes(spellcraft))
        .filter(|(st, multiply, _)| *st == crate::made::stat::DAMAGE && !*multiply)
        .map(|(_, _, by)| by)
        .sum();
    #[allow(clippy::cast_precision_loss)]
    let whole = most as f32 * held_multiplier(s, held, crate::made::stat::DAMAGE) + added;
    dereth_primitives::num::to_i32(whole.floor())
}

/// What these effects on an item add to its shield rating, rounded down.
#[must_use]
pub fn held_rating<S: Store>(s: &Server<S>, held: &[(u32, f32)]) -> i32 {
    let added: f32 = held
        .iter()
        .filter_map(|(state, spellcraft)| Some((s.effect_table.get(*state)?, *spellcraft)))
        .flat_map(|(d, spellcraft)| d.changes(spellcraft))
        .filter(|(st, multiply, _)| *st == crate::made::stat::ARMOR && !*multiply)
        .map(|(_, _, by)| by)
        .sum();
    dereth_primitives::num::floor_to_i32(added)
}

/// The players an aura's pulse reaches, when it reaches players: its user, and each of its fellows
/// in the world, out of portal space and within the aura's range of it (2017: the 949 records a
/// later pulse missed that could be read were all a fellow's of the aura's user).
fn pulse_reaches<S: Store>(s: &Server<S>, world: ConnId, a: &Aura) -> Vec<ConnId> {
    if a.targets & (aura_target::PLAYER | aura_target::OTHER_PLAYER) == 0 {
        return Vec::new();
    }
    let Some(me) = s.players.get(&world) else {
        return Vec::new();
    };
    let fellows: Vec<u64> = s
        .fellowships
        .of(me.character)
        .map(|i| s.fellowships.list[i].fellows.iter().map(|f| f.id).collect())
        .unwrap_or_default();
    let mut out = vec![world];
    let mut others: Vec<ConnId> = s
        .players
        .iter()
        .filter(|(c, p)| {
            **c != world
                && fellows.contains(&p.character)
                && p.scene_drawn
                && p.trip.is_none()
                && crate::presence::distance(&me.position, &p.position)
                    .is_some_and(|d| d <= a.range)
        })
        .map(|(c, _)| *c)
        .collect();
    others.sort_unstable();
    out.extend(others);
    out
}

/// An aura pulses (if it is still on): its pulsed effects on the players it reaches, shown; every
/// other player holding a record of an earlier pulse of its is told the pulse missed it; the next
/// pulse two minutes or so on.
pub fn pulse<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    eid: u32,
    clock: Clock,
) {
    if !pulse_once(s, net, world, eid, clock) {
        return;
    }
    let wait = f64::from(s.rng.between(0.0, 1.0));
    s.after(
        clock,
        PULSE_LEAST + wait * (PULSE_MOST - PULSE_LEAST),
        Timer::AuraPulse { world, eid },
    );
}

/// Three seconds after a player is back from a resurrection, each aura it held through it pulses
/// beside its own round.
pub fn auras_pulse_again<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    clock: Clock,
) {
    let held: Vec<u32> = s.players.get(&world).map_or_else(Vec::new, |p| {
        p.play
            .effects
            .iter()
            .filter(|e| e.flags == AURA_FLAGS)
            .map(|e| e.eid)
            .collect()
    });
    for eid in held {
        pulse_once(s, net, world, eid, clock);
    }
}

/// One pulse of an aura, without its next: false if it is no longer on.
fn pulse_once<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    eid: u32,
    clock: Clock,
) -> bool {
    let Some(aura) = s
        .players
        .get(&world)
        .and_then(|p| p.play.effects.iter().find(|e| e.eid == eid))
        .copied()
    else {
        return false;
    };
    let Some(a) = aura_def(s, aura.state) else {
        return false;
    };
    let reached = pulse_reaches(s, world, &a);
    for to in &reached {
        let fx = pulse_records(s, net, *to, aura.caster, &a, aura.spellcraft, clock);
        for f in fx {
            show_fx_on(s, net, On::Player(*to), f);
        }
    }
    let missed: Vec<(ConnId, Vec<u32>)> = s
        .players
        .iter()
        .filter(|(c, _)| !reached.contains(c))
        .map(|(c, p)| {
            let eids = p
                .play
                .effects
                .iter()
                .filter(|e| {
                    e.caster == aura.caster && a.pulsed.contains(&e.state) && e.flags == PULSE_FLAGS
                })
                .map(|e| e.eid)
                .collect::<Vec<_>>();
            (*c, eids)
        })
        .filter(|(_, eids)| !eids.is_empty())
        .collect();
    for (to, eids) in missed {
        progress::private(net, to, &pulse_missed_call(&eids));
    }
    true
}

/// Turn an aura off: the records of its pulses its user holds, oldest first, then its own; the
/// appearances no record shows now, for the caller to stop when they are due.
pub fn aura_off<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    eid: u32,
) -> Vec<u32> {
    let Some(aura) = s
        .players
        .get(&world)
        .and_then(|p| p.play.effects.iter().find(|e| e.eid == eid))
        .copied()
    else {
        return Vec::new();
    };
    let pulsed = aura_def(s, aura.state)
        .map(|a| a.pulsed)
        .unwrap_or_default();
    let mut gone: Vec<u32> = s.players.get(&world).map_or_else(Vec::new, |p| {
        p.play
            .effects
            .iter()
            .filter(|e| {
                e.caster == aura.caster && pulsed.contains(&e.state) && e.flags == PULSE_FLAGS
            })
            .map(|e| e.eid)
            .collect()
    });
    gone.push(eid);
    let mut stop = Vec::new();
    for e in gone {
        if let Some(Some(fx)) = take_off(s, net, On::Player(world), e) {
            stop.push(fx);
        }
    }
    crate::world::keep_player(s, world);
    stop
}

/// A player goes into portal space, other than to come back from its death: every aura it holds
/// turns off (its toggle, its records, the appearances they showed).
pub fn auras_off<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId) {
    let held: Vec<EffectState> = s.players.get(&world).map_or_else(Vec::new, |p| {
        p.play
            .effects
            .iter()
            .filter(|e| e.flags == AURA_FLAGS)
            .copied()
            .collect()
    });
    for aura in held {
        if !is_aura(s, aura.state) {
            continue;
        }
        if let Some(skill) = s
            .skill_table
            .skills
            .values()
            .find(|d| d.state == aura.skill)
            .map(|d| d.id)
        {
            progress::private(net, world, &toggle_call(false, skill));
        }
        for fx in aura_off(s, net, world, aura.eid) {
            stop_fx_on(s, net, On::Player(world), fx);
        }
    }
}

/// Stop these appearances on a player for all who see it.
pub fn stop_fx<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, fx: &[u32]) {
    for f in fx {
        stop_fx_on(s, net, On::Player(world), *f);
    }
}

/// Show these appearances on a player for all who see it.
pub fn show_fxs<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, fx: &[u32]) {
    for f in fx {
        show_fx_on(s, net, On::Player(world), *f);
    }
}

/// An item's effects come onto the player wearing or wielding it, before the equipping answer:
/// each that its client is shown (2017: an instrument's own appearance effects, never shown,
/// sent nothing in 21 of 21 wields) a record of its own, cast by the item at its spellcraft and
/// from it, held until the item comes off (flags [`WORN`]); then, for an effect a death does not
/// end, "The `<effect>` applies to you." naming the item (2017: 176 of 200 such records; 276 of 277
/// of the others had none); its appearance shown when no other record shows it. Returns the ids.
pub fn put_on_worn<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    item: u64,
    effects: &[(u32, f32)],
    clock: Clock,
) -> Vec<u32> {
    let name = s.things.get(item).and_then(|t| t.facts.name);
    let mut ids = Vec::new();
    for (state, spellcraft) in effects {
        let Some(def) = s
            .effect_table
            .get(*state)
            .filter(|d| d.client_visible())
            .cloned()
        else {
            continue;
        };
        let cast = Cast {
            caster: item,
            spellcraft: *spellcraft,
            item,
            ..Cast::default()
        };
        let shape = Shape {
            flags: WORN,
            categories: None,
            timeout: Some(f32::MAX),
            acting: 0,
        };
        let Some(eid) = add_quietly(s, net, On::Player(world), &def, cast, shape, clock) else {
            continue;
        };
        ids.push(eid);
        if let (false, Some(effect)) = (def.removed_on_death(), def.name) {
            let mut vars = vec![progress::nested(
                learned::EFFECT_VAR,
                progress::text(effect),
            )];
            if let Some(n) = name {
                vars.push(progress::nested(learned::GIVER_VAR, progress::text(n)));
            }
            progress::private(
                net,
                world,
                &crate::chat::show_text(2, progress::text_with(learned::TEXT, vars)),
            );
        }
        if def.fx != 0 && shows_once(s, On::Player(world), def.fx) {
            show_fx_on(s, net, On::Player(world), def.fx);
        }
    }
    ids
}

/// An item's effects come off the player as the item comes off, before the answer: each record
/// it put on taken away (2017: 256 of 257 removals of such records were a take-off's, right
/// before its answer). Returns the appearances no record shows any more, which the caller stops
/// after the answer.
pub fn take_off_worn<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    item: u64,
) -> Vec<u32> {
    let ids: Vec<u32> = s.players.get(&world).map_or_else(Vec::new, |p| {
        p.play
            .effects
            .iter()
            .filter(|e| e.flags & record_flag::EQUIPPED != 0 && e.item == item)
            .map(|e| e.eid)
            .collect()
    });
    if ids.is_empty() {
        return Vec::new();
    }
    let mut stopped = Vec::new();
    for eid in ids {
        if let Some(Some(fx)) = take_off(s, net, On::Player(world), eid) {
            stopped.push(fx);
        }
    }
    crate::world::keep_player(s, world);
    stopped
}
