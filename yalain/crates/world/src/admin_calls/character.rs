//! An admin's commands over a character's state: its qualities, experience, vitals, skills, hero
//! standing, killing and death.

use auberean_protocol::types::Qualities;
use yalain_net::ConnId;
use yalain_store::Store;

use super::{other, Args, Cx, Target};
use crate::player::quality;
use crate::progress::{self, Award};
use crate::skills::{self, mask};
use crate::{death, examine, quals};

/// The calls, by their function ids.
pub mod function {
    pub const DUMP: u32 = 0x8201_0024;
    pub const EDIT: u32 = 0x8201_005F;
    pub const XP_ADD: u32 = 0x8201_0011;
    pub const XP_CHECK: u32 = 0x8201_0114;
    pub const VITALS_REPORT: u32 = 0x8201_000D;
    pub const RESET_VITALS: u32 = 0x8201_0051;
    pub const SET_HEALTH: u32 = 0x8201_008E;
    pub const SET_VIGOR: u32 = 0x8201_0021;
    pub const ADJUST_HEALTH: u32 = 0x8201_0081;
    pub const ADJUST_VIGOR: u32 = 0x8201_0063;
    pub const PEACE_HEALTH_RATE: u32 = 0x8201_0140;
    pub const PEACE_VIGOR_RATE: u32 = 0x8201_0138;
    pub const COMBAT_HEALTH_RATE: u32 = 0x8201_0126;
    pub const COMBAT_VIGOR_RATE: u32 = 0x8201_0122;
    pub const NEVER_SAY_DIE: u32 = 0x8201_0057;
    pub const EXTERMINATE: u32 = 0x8201_013B;
    pub const ATTACKERS: u32 = 0x8201_008A;
    pub const GOD: u32 = 0x8201_007F;
    pub const SKILL_LIST: u32 = 0x8201_0132;
    pub const SKILL_INFO: u32 = 0x8201_011A;
    pub const SKILL_TRAIN: u32 = 0x8201_013A;
    pub const SKILL_UNTRAIN: u32 = 0x8201_0118;
    pub const SKILL_UNTRAIN_QUERY: u32 = 0x8201_012A;
    pub const SKILL_RAISE: u32 = 0x8201_011F;
    pub const SKILL_FORCE: u32 = 0x8201_0059;
    pub const SKILL_CREDITS: u32 = 0x8201_0002;
    pub const SKILL_TIMERS: u32 = 0x8201_0004;
    pub const SKILL_RESET: u32 = 0x8201_0128;
    pub const HERO_SHOW: u32 = 0x8201_0139;
    pub const HERO_SET: u32 = 0x8201_011D;
    pub const HERO_FOCUS: u32 = 0x8201_011B;
    pub const HERO_MAX_FOCUS: u32 = 0x8201_0117;
    pub const HERO_CREDITS: u32 = 0x8201_013D;
    pub const CRAFT_XP: u32 = 0x8201_0119;
    pub const CRAFT_TIMERS: u32 = 0x8201_012D;
    pub const RECIPE_ADD: u32 = 0x8201_004D;
    pub const RECIPE_REMOVE: u32 = 0x8201_0078;
    pub const RECIPE_LIST: u32 = 0x8201_0143;
    pub const RECIPE_TIMERS: u32 = 0x8201_013C;
    pub const CRAFT_LIST: u32 = 0x8201_0141;
}

/// A call of this kind: its answer; `None` for any other.
pub(super) fn call<S: Store>(cx: &mut Cx<'_, S>, f: u32, a: &Args<'_>) -> Option<Vec<String>> {
    Some(match f {
        function::DUMP => dump(cx, a),
        function::EDIT => edit(cx, a),
        function::XP_ADD => xp_add(cx, a),
        function::XP_CHECK => xp_check(cx, a),
        function::VITALS_REPORT => vitals(cx),
        function::RESET_VITALS => reset_vitals(cx, a),
        function::SET_HEALTH => set_vital(cx, a, quality::HEALTH, quality::HEALTH_MAX, false),
        function::SET_VIGOR => set_vital(cx, a, quality::VIGOR, quality::VIGOR_MAX, false),
        function::ADJUST_HEALTH => set_vital(cx, a, quality::HEALTH, quality::HEALTH_MAX, true),
        function::ADJUST_VIGOR => set_vital(cx, a, quality::VIGOR, quality::VIGOR_MAX, true),
        function::PEACE_HEALTH_RATE => set_rate(cx, a, crate::regen::rate::PEACE_HEALTH),
        function::PEACE_VIGOR_RATE => set_rate(cx, a, crate::regen::rate::PEACE_VIGOR),
        function::COMBAT_HEALTH_RATE => set_rate(cx, a, crate::regen::rate::COMBAT_HEALTH),
        function::COMBAT_VIGOR_RATE => set_rate(cx, a, crate::regen::rate::COMBAT_VIGOR),
        function::NEVER_SAY_DIE => {
            let on = a.flag(0).unwrap_or(false);
            cx.state().never_say_die = on;
            vec![if on {
                "Your health will never fall below one.".to_owned()
            } else {
                "You can die as usual.".to_owned()
            }]
        }
        function::EXTERMINATE => exterminate(cx),
        function::ATTACKERS => attackers(cx, a),
        other::SMITE => smite(cx, a),
        function::GOD => god(cx, a),
        function::SKILL_LIST | function::SKILL_INFO => skill_list(cx, a, f),
        function::SKILL_TRAIN => skill_train(cx, a),
        function::SKILL_UNTRAIN | function::SKILL_UNTRAIN_QUERY => {
            vec!["This server untrains a skill only as a player asks, not by command.".to_owned()]
        }
        function::SKILL_RAISE | function::SKILL_FORCE => skill_raise(cx, a, f),
        function::SKILL_CREDITS => skill_credits(cx, a),
        function::SKILL_TIMERS => skill_timers(cx),
        function::SKILL_RESET => {
            vec!["This server resets a skill's training only as a player untrains it.".to_owned()]
        }
        function::HERO_SHOW => hero_show(cx),
        function::HERO_SET => hero_set(cx, a),
        function::HERO_FOCUS => hero_focus(cx, a, false),
        function::HERO_MAX_FOCUS => hero_focus(cx, a, true),
        function::HERO_CREDITS => hero_credits(cx, a),
        function::CRAFT_XP => craft_xp(cx, a),
        function::CRAFT_TIMERS | function::RECIPE_TIMERS => craft_timers(cx, a),
        function::RECIPE_ADD => recipe_add(cx, a),
        function::RECIPE_REMOVE => recipe_remove(cx, a),
        function::RECIPE_LIST => recipe_list(cx, a),
        function::CRAFT_LIST => craft_list(cx, a),
        other::VITAE_SET | other::VITAE_RAISE | other::VITAE_LOWER => vitae(cx, a, f),
        other::CHALLENGE_SET | other::CHALLENGE_SET_OF | other::CHALLENGE_OF => challenge(cx, a, f),
        other::DEATH_XP_SET | other::DEATH_XP_SET_OF | other::DEATH_XP_OF => death_xp(cx, a, f),
        _ => return None,
    })
}

/// One table of a character's qualities as lines.
fn table<V: std::fmt::Debug>(
    lines: &mut Vec<String>,
    what: &str,
    t: &auberean_protocol::table::Table<V>,
) {
    if !t.entries.is_empty() {
        lines.push(format!("{what}: {}", t.entries.len()));
        lines.extend(t.entries.iter().map(|(k, v)| format!("  {k} = {v:?}")));
    }
}

fn qualities_text(name: &str, q: &Qualities) -> Vec<String> {
    let mut lines = vec![format!("{name}'s qualities, on record {:08X}:", q.did)];
    table(&mut lines, "numbers", &q.ints);
    table(&mut lines, "long numbers", &q.long_ints);
    table(&mut lines, "floats", &q.floats);
    table(&mut lines, "truth values", &q.bools);
    table(&mut lines, "data ids", &q.data_ids);
    table(&mut lines, "times", &q.timestamps);
    lines
}

fn dump<S: Store>(cx: &Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    match cx.target(a.long(0).unwrap_or(0)) {
        Some(Target::Player(w)) => cx.s.players.get(&w).map_or_else(Vec::new, |p| {
            qualities_text(&p.name, &quals::read(&p.qualities))
        }),
        Some(Target::Thing(id)) => cx.s.things.get(id).map_or_else(Vec::new, |t| {
            vec![format!(
                "{id:#x}: template {:08X}, health {} of {}, vigor {} of {}, quantity {}, value {}.",
                t.template, t.health, t.max_health, t.vigor, t.max_vigor, t.quantity, t.value
            )]
        }),
        None => vec!["No target.".to_owned()],
    }
}

/// `/edit <quality> <value>`: the quality by its number; a value with a point is a float's, and
/// otherwise a whole number's.
fn edit<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let w = match cx.player(a.long(0).unwrap_or(0)) {
        Ok(w) => w,
        Err(why) => return vec![why],
    };
    let word = a.text(1).unwrap_or_default();
    let Ok(key) = word.parse::<u32>() else {
        return vec![format!(
            "{word}: give a quality's number; this server has no names for them."
        )];
    };
    let value = a.text(2).unwrap_or_default();
    let Some(p) = cx.s.players.get_mut(&w) else {
        return Vec::new();
    };
    if value.contains('.') {
        let Ok(v) = value.parse::<f32>() else {
            return vec![format!("{value} is not a number.")];
        };
        quals::set_float(&mut p.qualities, key, v);
        progress::send_float(cx.net, w, key, v);
    } else {
        let Ok(v) = value.parse::<i32>() else {
            return vec![format!("{value} is not a number.")];
        };
        quals::set_int(&mut p.qualities, key, v);
        progress::send_int(cx.net, w, key, v);
    }
    let name = cx.name_of(w);
    crate::world::keep_player(cx.s, w);
    vec![format!("{name}'s quality {key} is {value}.")]
}

fn xp_add<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let w = match cx.player(a.long(0).unwrap_or(0)) {
        Ok(w) => w,
        Err(why) => return vec![why],
    };
    let amount = a.long(1).unwrap_or(0);
    progress::award(cx.s, cx.net, w, amount, Award::Part);
    let name = cx.name_of(w);
    crate::world::keep_player(cx.s, w);
    vec![format!("{name} is awarded {amount} experience.")]
}

fn xp_check<S: Store>(cx: &Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let w = match cx.player(a.long(0).unwrap_or(0)) {
        Ok(w) => w,
        Err(why) => return vec![why],
    };
    let Some(p) = cx.s.players.get(&w) else {
        return Vec::new();
    };
    vec![format!(
        "{} is level {} with {} experience.",
        p.name,
        examine::level_of(&p.qualities),
        quals::long(&p.qualities, progress::q::TOTAL_XP).unwrap_or(0)
    )]
}

fn vitals<S: Store>(cx: &Cx<'_, S>) -> Vec<String> {
    let Some(p) = cx.s.players.get(&cx.world) else {
        return Vec::new();
    };
    let v = |q| quals::int(&p.qualities, q).unwrap_or(0);
    vec![format!(
        "Health {} of {}; vigor {} of {}.",
        v(quality::HEALTH),
        v(quality::HEALTH_MAX),
        v(quality::VIGOR),
        v(quality::VIGOR_MAX)
    )]
}

/// Set one of a player's vitals, told to its client.
fn put_vital<S: Store>(cx: &mut Cx<'_, S>, w: ConnId, q: u32, v: i32) {
    if let Some(p) = cx.s.players.get_mut(&w) {
        quals::set_int(&mut p.qualities, q, v);
    }
    progress::send_int(cx.net, w, q, v);
}

fn reset_vitals<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    match cx.target(a.long(0).unwrap_or(0)) {
        Some(Target::Player(w)) => {
            let Some(p) = cx.s.players.get(&w) else {
                return Vec::new();
            };
            let max = crate::effects::max_vital(cx.s, p, quality::HEALTH_MAX);
            let vigor = crate::effects::max_vital(cx.s, p, quality::VIGOR_MAX);
            put_vital(cx, w, quality::HEALTH, max);
            put_vital(cx, w, quality::VIGOR, vigor);
            vec![format!("{}'s vitals are full.", cx.name_of(w))]
        }
        Some(Target::Thing(id)) => {
            let Some(t) = cx.s.things.get_mut(id) else {
                return Vec::new();
            };
            t.health = t.max_health;
            t.vigor = t.max_vigor;
            vec![format!("{id:#x}'s vitals are full.")]
        }
        None => vec!["No target.".to_owned()],
    }
}

/// A vital set to a value, or moved by one; never above its greatest, nor below one.
fn set_vital<S: Store>(
    cx: &mut Cx<'_, S>,
    a: &Args<'_>,
    which: u32,
    greatest: u32,
    by: bool,
) -> Vec<String> {
    let Some(value) = a.float(0).filter(|v| v.is_finite()) else {
        return vec!["Give a number.".to_owned()];
    };
    let world = cx.world;
    let Some(p) = cx.s.players.get(&world) else {
        return Vec::new();
    };
    let max = crate::effects::max_vital(cx.s, p, greatest);
    let now = quals::int(&p.qualities, which).unwrap_or(max);
    let v = dereth_primitives::num::to_i32(value);
    let v = if by { now.saturating_add(v) } else { v }.clamp(1, max.max(1));
    put_vital(cx, world, which, v);
    vec![format!("It is now {v} of {max}.")]
}

fn set_rate<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>, which: u32) -> Vec<String> {
    let Some(rate) = a.float(0).filter(|v| v.is_finite() && *v >= 0.0) else {
        return vec!["A rate is a number from 0.".to_owned()];
    };
    let world = cx.world;
    let Some(p) = cx.s.players.get_mut(&world) else {
        return Vec::new();
    };
    quals::set_float(&mut p.qualities, which, rate);
    progress::send_float(cx.net, world, which, rate);
    vec![format!("The rate is now {rate}.")]
}

/// `/exterminate`: every creature in the admin's landblock dies by its hand.
fn exterminate<S: Store>(cx: &mut Cx<'_, S>) -> Vec<String> {
    let me = cx.me();
    let Some(block) = cx.s.players.get(&cx.world).map(|p| p.position.cell >> 16) else {
        return Vec::new();
    };
    let here: Vec<u64> =
        cx.s.things
            .placed()
            .filter(|t| t.kind == crate::things::Kind::Creature && t.health > 0)
            .filter(|t| t.at.is_some_and(|at| at.cell >> 16 == block))
            .map(|t| t.id)
            .collect();
    for id in &here {
        if let Some(t) = cx.s.things.get_mut(*id) {
            t.health = 0;
        }
        death::creature_dies(cx.s, cx.net, *id, me, cx.clock);
    }
    vec![format!("{} creatures exterminated.", here.len())]
}

fn smite<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let me = cx.me();
    let id = a.long(0).unwrap_or(0);
    let target = if id == 0 {
        Some(Target::Player(cx.world))
    } else {
        cx.target(id)
    };
    match target {
        Some(Target::Player(w)) => {
            let name = cx.name_of(w);
            put_vital(cx, w, quality::HEALTH, 0);
            death::player_dies(cx.s, cx.net, w, Some(me), cx.clock);
            vec![format!("{name} is smitten.")]
        }
        Some(Target::Thing(t)) => {
            let alive =
                cx.s.things
                    .get(t)
                    .is_some_and(|x| x.kind == crate::things::Kind::Creature && x.health > 0);
            if !alive {
                return vec!["That is not alive.".to_owned()];
            }
            if let Some(x) = cx.s.things.get_mut(t) {
                x.health = 0;
            }
            death::creature_dies(cx.s, cx.net, t, me, cx.clock);
            vec![format!("{t:#x} is smitten.")]
        }
        None => vec!["No target.".to_owned()],
    }
}

fn attackers<S: Store>(cx: &Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let id = a.long(0).unwrap_or(0);
    let Some(c) = cx.s.things.get(id).and_then(|t| t.creature.as_ref()) else {
        return vec![format!("{id:#x} is not a creature here.")];
    };
    let mut lines = vec![format!("{id:#x} is fighting {:?}.", c.target)];
    lines.extend(
        c.damage_by
            .iter()
            .map(|(who, amount)| format!("{who:#x} did {amount} damage")),
    );
    lines
}

/// The skill a parameter names: a skill's singleton, by its saved state.
fn skill_at<S: Store>(cx: &Cx<'_, S>, a: &Args<'_>, i: usize) -> Option<u32> {
    let state = a.singleton(i)?;
    cx.s.skill_table
        .skills
        .values()
        .find(|d| d.state == state)
        .map(|d| d.id)
}

/// The experience a skill holds at a level, by its advancement table: its last for a level beyond.
fn xp_at<S: Store>(cx: &Cx<'_, S>, skill: u32, level: u32) -> u64 {
    let table =
        cx.s.skill_table
            .skills
            .get(&skill)
            .and_then(|d| cx.s.skill_table.tables.get(&d.table));
    table.map_or(0, |t| {
        t.get(level as usize)
            .or_else(|| t.last())
            .copied()
            .unwrap_or(0)
    })
}

/// A player's skills as held.
fn held_of<S: Store>(cx: &Cx<'_, S>, w: ConnId) -> Vec<skills::Held> {
    cx.s.players.get(&w).map_or_else(Vec::new, |p| {
        let starting = cx.s.start.skills.get(&p.race).cloned().unwrap_or_default();
        skills::held(&starting, &p.play.skills)
    })
}

fn skill_list<S: Store>(cx: &Cx<'_, S>, a: &Args<'_>, f: u32) -> Vec<String> {
    let w = if f == function::SKILL_INFO {
        cx.player(a.long(0).unwrap_or(0))
    } else {
        Ok(cx.world)
    };
    let w = match w {
        Ok(w) => w,
        Err(why) => return vec![why],
    };
    let held = held_of(cx, w);
    let mut lines = vec![format!("{} holds {} skills.", cx.name_of(w), held.len())];
    lines.extend(held.iter().map(|h| {
        format!(
            "{:08X}: granted {}, experience {}, mask {:#x}, cost {}",
            h.skill, h.granted, h.xp, h.mask, h.cost
        )
    }));
    lines
}

/// `/god <level>`: every trained skill raised to a level, as experience; 0 is refused, since this
/// server keeps no skills to go back to.
fn god<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let level = a.int(0).unwrap_or(0);
    if level == 0 {
        return vec!["Skills raised by /god stay raised on this server.".to_owned()];
    }
    let world = cx.world;
    let mut kept = held_of(cx, world);
    let mut changed = 0;
    let xps: Vec<u64> = kept.iter().map(|h| xp_at(cx, h.skill, level)).collect();
    for (h, xp) in kept.iter_mut().zip(xps) {
        if h.mask & mask::TRAINED != 0 && h.xp < xp {
            h.xp = xp;
            progress::private(cx.net, world, &skills::info_update(h, None));
            changed += 1;
        }
    }
    let Some(p) = cx.s.players.get_mut(&world) else {
        return Vec::new();
    };
    let starting = cx.s.start.skills.get(&p.race).cloned().unwrap_or_default();
    p.play.skills = skills::kept(&starting, &kept);
    crate::world::keep_player(cx.s, world);
    vec![format!("{changed} skills raised to level {level}.")]
}

/// `/skill train`: as a player's training, the credits it takes given first, so it costs nothing.
fn skill_train<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    const GIVEN: u32 = 1_000;
    let Some(skill) = skill_at(cx, a, 0) else {
        return vec!["That is not a skill.".to_owned()];
    };
    let world = cx.world;
    let Some(p) = cx.s.players.get_mut(&world) else {
        return Vec::new();
    };
    p.play.credits = p.play.credits.saturating_add(GIVEN);
    p.play.hero_credits = p.play.hero_credits.saturating_add(GIVEN);
    skills::train(cx.s, cx.net, world, &[crate::calls::Value::Int(skill)]);
    let Some(p) = cx.s.players.get_mut(&world) else {
        return Vec::new();
    };
    p.play.credits = p.play.credits.saturating_sub(GIVEN);
    p.play.hero_credits = p.play.hero_credits.saturating_sub(GIVEN);
    let (c, h) = (p.play.credits, p.play.hero_credits);
    progress::private(cx.net, world, &skills::repository_update(c, h));
    crate::world::keep_player(cx.s, world);
    vec![format!("Asked to train skill {skill:08X}.")]
}

fn skill_raise<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>, f: u32) -> Vec<String> {
    let Some(skill) = skill_at(cx, a, 0) else {
        return vec!["That is not a skill.".to_owned()];
    };
    let n = a.int(1).unwrap_or(0);
    let world = cx.world;
    let mut kept = held_of(cx, world);
    let Some(i) = kept.iter().position(|h| h.skill == skill) else {
        return vec![format!("You do not hold skill {skill:08X}.")];
    };
    let table =
        cx.s.skill_table
            .skills
            .get(&skill)
            .and_then(|d| cx.s.skill_table.tables.get(&d.table))
            .cloned()
            .unwrap_or_default();
    let level = if f == function::SKILL_FORCE {
        skills::table_level(&table, kept[i].xp).saturating_add(n)
    } else {
        n
    };
    kept[i].xp = xp_at(cx, skill, level);
    let done = kept[i];
    progress::private(cx.net, world, &skills::info_update(&done, None));
    let Some(p) = cx.s.players.get_mut(&world) else {
        return Vec::new();
    };
    let starting = cx.s.start.skills.get(&p.race).cloned().unwrap_or_default();
    p.play.skills = skills::kept(&starting, &kept);
    crate::world::keep_player(cx.s, world);
    vec![format!("Skill {skill:08X} is at level {level}.")]
}

fn skill_credits<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let add = a.int(0).unwrap_or(0);
    let Some(p) = cx.s.players.get_mut(&cx.world) else {
        return Vec::new();
    };
    p.play.credits = p.play.credits.saturating_add(add);
    let (c, h) = (p.play.credits, p.play.hero_credits);
    progress::private(cx.net, cx.world, &skills::repository_update(c, h));
    crate::world::keep_player(cx.s, cx.world);
    vec![format!("You hold {c} skill credits.")]
}

fn skill_timers<S: Store>(cx: &mut Cx<'_, S>) -> Vec<String> {
    let Some(p) = cx.s.players.get_mut(&cx.world) else {
        return Vec::new();
    };
    p.last_used.clear();
    vec!["Your skills' waits are over.".to_owned()]
}

fn hero_show<S: Store>(cx: &Cx<'_, S>) -> Vec<String> {
    let Some(p) = cx.s.players.get(&cx.world) else {
        return Vec::new();
    };
    vec![format!(
        "Hero: {}; focus {} of {}; hero skill credits {}.",
        crate::focus::is_hero(&p.qualities),
        quals::int(&p.qualities, quality::FOCUS).unwrap_or(0),
        quals::int(&p.qualities, quality::FOCUS_MAX).unwrap_or(0),
        p.play.hero_credits
    )]
}

fn hero_set<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let name = cx.name_of(cx.world);
    let on = a.flag(0).unwrap_or(false);
    vec![crate::focus::set_hero(cx.s, cx.net, &name, on).unwrap_or_else(|e| e)]
}

fn hero_focus<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>, greatest: bool) -> Vec<String> {
    let n = a.signed(0).unwrap_or(0);
    let world = cx.world;
    let Some(p) = cx.s.players.get_mut(&world) else {
        return Vec::new();
    };
    let (which, v) = if greatest {
        (quality::FOCUS_MAX, n.max(0))
    } else {
        let max = quals::int(&p.qualities, quality::FOCUS_MAX).unwrap_or(0);
        let now = quals::int(&p.qualities, quality::FOCUS).unwrap_or(0);
        (quality::FOCUS, now.saturating_add(n).clamp(0, max))
    };
    quals::set_int(&mut p.qualities, which, v);
    progress::send_int(cx.net, world, which, v);
    vec![format!("It is now {v}.")]
}

fn hero_credits<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let n = a.int(0).unwrap_or(0);
    let Some(p) = cx.s.players.get_mut(&cx.world) else {
        return Vec::new();
    };
    p.play.hero_credits = p.play.hero_credits.saturating_add(n);
    let (c, h) = (p.play.credits, p.play.hero_credits);
    progress::private(cx.net, cx.world, &skills::repository_update(c, h));
    vec![format!("You hold {h} hero skill credits.")]
}

/// Experience for every craft skill a player holds.
fn craft_xp<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let w = match cx.player(a.long(0).unwrap_or(0)) {
        Ok(w) => w,
        Err(why) => return vec![why],
    };
    let xp = a.long(1).unwrap_or(0);
    let held: Vec<u32> = cx.s.players.get(&w).map_or_else(Vec::new, |p| {
        p.play.craft_skills.iter().map(|c| c.skill).collect()
    });
    for skill in &held {
        crate::craft::award(cx.s, cx.net, w, *skill, xp);
    }
    vec![format!("{} craft skills awarded {xp}.", held.len())]
}

fn craft_timers<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let w = match cx.player(a.long(0).unwrap_or(0)) {
        Ok(w) => w,
        Err(why) => return vec![why],
    };
    let Some(p) = cx.s.players.get_mut(&w) else {
        return Vec::new();
    };
    p.play.craft_reset = 0.0;
    for r in &mut p.play.recipes {
        r.use_again = -1.0;
    }
    let name = p.name.clone();
    crate::world::keep_player(cx.s, w);
    vec![format!("{name}'s craft waits are over.")]
}

fn recipe_add<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let w = match cx.player(a.long(0).unwrap_or(0)) {
        Ok(w) => w,
        Err(why) => return vec![why],
    };
    let recipe = a.int(1).unwrap_or(0);
    let Some(p) = cx.s.players.get(&w) else {
        return Vec::new();
    };
    let name = p.name.clone();
    if p.play.recipes.iter().any(|r| r.recipe == recipe) {
        return vec![format!("{name} knows that recipe.")];
    }
    crate::craft::learn(cx.s, cx.net, w, recipe, cx.clock);
    vec![format!("{name} knows recipe {recipe:08X}.")]
}

fn recipe_remove<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let w = match cx.player(a.long(0).unwrap_or(0)) {
        Ok(w) => w,
        Err(why) => return vec![why],
    };
    let recipe = a.int(1).unwrap_or(0);
    let Some(p) = cx.s.players.get(&w) else {
        return Vec::new();
    };
    let name = p.name.clone();
    if !p.play.recipes.iter().any(|r| r.recipe == recipe) {
        return vec![format!("{name} does not know recipe {recipe:08X}.")];
    }
    crate::craft::forget(cx.s, cx.net, w, recipe);
    vec![format!("{name} no longer knows recipe {recipe:08X}.")]
}

fn recipe_list<S: Store>(cx: &Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let w = match cx.player(a.long(0).unwrap_or(0)) {
        Ok(w) => w,
        Err(why) => return vec![why],
    };
    let Some(p) = cx.s.players.get(&w) else {
        return Vec::new();
    };
    let mut lines = vec![format!(
        "{} knows {} recipes.",
        p.name,
        p.play.recipes.len()
    )];
    lines.extend(p.play.recipes.iter().map(|r| {
        format!(
            "{:08X}: successes {}, charges {}, use again {}",
            r.recipe, r.successes, r.charges, r.use_again
        )
    }));
    lines
}

fn craft_list<S: Store>(cx: &Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let w = match cx.player(a.long(0).unwrap_or(0)) {
        Ok(w) => w,
        Err(why) => return vec![why],
    };
    let Some(p) = cx.s.players.get(&w) else {
        return Vec::new();
    };
    let mut lines = vec![format!(
        "{} holds {} craft skills.",
        p.name,
        p.play.craft_skills.len()
    )];
    lines.extend(p.play.craft_skills.iter().map(|c| {
        format!(
            "{:08X}: level {}, today {}, available {}",
            c.skill, c.level, c.today, c.available
        )
    }));
    lines
}

/// `/vitae`: set, raised and lowered by a share.
fn vitae<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>, f: u32) -> Vec<String> {
    let Some(n) = a.float(0).filter(|v| v.is_finite()) else {
        return vec!["Give a number.".to_owned()];
    };
    let world = cx.world;
    let Some(p) = cx.s.players.get_mut(&world) else {
        return Vec::new();
    };
    let now = quals::float(&p.qualities, death::vitae_q::VITAE).unwrap_or(100.0);
    let v = match f {
        other::VITAE_SET => n,
        other::VITAE_RAISE => now + n,
        _ => now - n,
    }
    .clamp(0.0, 100.0);
    quals::set_float(&mut p.qualities, death::vitae_q::VITAE, v);
    progress::send_float(cx.net, world, death::vitae_q::VITAE, v);
    vec![format!("Your vitae is {v}.")]
}

/// A player's challenge level.
fn challenge<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>, f: u32) -> Vec<String> {
    const CHALLENGE: u32 = 313;
    let (w, n) = match f {
        other::CHALLENGE_SET => (Ok(cx.world), a.signed(0)),
        other::CHALLENGE_SET_OF => (cx.player(a.long(0).unwrap_or(0)), a.signed(1)),
        _ => (cx.player(a.long(0).unwrap_or(0)), None),
    };
    let w = match w {
        Ok(w) => w,
        Err(why) => return vec![why],
    };
    let Some(p) = cx.s.players.get_mut(&w) else {
        return Vec::new();
    };
    if let Some(n) = n {
        quals::set_int(&mut p.qualities, CHALLENGE, n);
        progress::send_int(cx.net, w, CHALLENGE, n);
    }
    vec![format!(
        "{}'s challenge level is {}.",
        p.name,
        quals::int(&p.qualities, CHALLENGE).unwrap_or(0)
    )]
}

/// A player's experience given up at death.
fn death_xp<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>, f: u32) -> Vec<String> {
    const DEATH_XP: u32 = 312;
    let (w, n) = match f {
        other::DEATH_XP_SET => (Ok(cx.world), a.long(0)),
        other::DEATH_XP_SET_OF => (cx.player(a.long(0).unwrap_or(0)), a.long(1)),
        _ => (cx.player(a.long(0).unwrap_or(0)), None),
    };
    let w = match w {
        Ok(w) => w,
        Err(why) => return vec![why],
    };
    let Some(p) = cx.s.players.get_mut(&w) else {
        return Vec::new();
    };
    if let Some(n) = n {
        quals::set_long(&mut p.qualities, DEATH_XP, n);
        progress::send_long(cx.net, w, DEATH_XP, n);
    }
    vec![format!(
        "{}'s death experience is {}.",
        p.name,
        quals::long(&p.qualities, DEATH_XP).unwrap_or(0)
    )]
}
