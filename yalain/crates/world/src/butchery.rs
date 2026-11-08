//! Butchery: a player uses a hunting knife on a corpse and gathers a material from it.
//!
//! **The request** is a use of the knife the player holds, on the corpse. The player walks to the
//! corpse (to 3 m, as for any object used: the order is sent even when it is there already), and
//! there the use is refused with its answer alone when another player is on its way to open the
//! corpse, when the creature's killer was 9 or more levels above it (the corpse is too damaged),
//! when the corpse has been butchered, or when the creature had nothing to butcher. Else the player
//! plays its butchering behavior and lets it go, and about two seconds later the result: the
//! butchery skill's experience (its amount for the level, less as the day's grows), the craft
//! effect (the expert one for an expert cut), the material put on the player's stack of it (or a
//! new stack made), and the use answered with "You butcher %1 units of %3 from %2." ("You expertly
//! butcher %2, collecting %1 units of %3." for an expert cut).
//!
//! **The yield** is one of the creature's butchery profile's materials, by their shares; so many
//! units by the knife, ten above the yield its recipe's item level gave it (a crude one's 1 gives
//! 11), twice that for an expert cut, which comes more often the higher the player's butchery.
//! A profile may also hold a share of nothing: a cut that
//! draws it gathers nothing, pays nothing, and is answered "empty" at the result, the corpse left
//! to butcher again. The knife stays in the pack until the result; one gone by then cuts nothing.

use auberean_protocol::physics::Release;
use auberean_protocol::types::BehaviorParams;
use auberean_protocol::ServerMessage;
use auberean_script::native_object::Variable;
use yalain_net::{BlobKind, ConnId, ServerNet};
use yalain_store::Store;

use crate::craft::{self, fx};
use crate::places::Use;
use crate::things::Kind;
use crate::{progress, Clock, Server, Timer};

/// The butchery craft skill (its saved state).
pub const SKILL: u32 = 0x6F00_068F;

/// How near the corpse the player works, in meters.
pub const REACH: f32 = 3.0;

/// The butchering behavior, and how long after it starts the result comes (2017: median 1.95 s,
/// 597 butcheries).
pub const BEHAVIOR: u32 = 0x4100_0157;
pub const WORK_SECONDS: f64 = 1.95;

/// Why a butchery is refused, as the use's answer says.
pub mod refusal {
    /// The corpse is another's yet.
    pub const NO_RIGHT: u32 = 10_000;
    /// Another player is on its way to open the corpse.
    pub const LOCKED: u32 = 1030;
    /// The corpse has been butchered.
    pub const BUTCHERED: u32 = 10_061;
    /// The creature had nothing to butcher.
    pub const NOTHING: u32 = 10_062;
    /// The creature's killer was far above it: the corpse is too damaged.
    pub const TOO_DAMAGED: u32 = 10_063;
    /// The cut found nothing (at the result).
    pub const EMPTY: u32 = 10_064;
    /// No place in the pack for the material (at the result; 2017: 1 of 1, with a full pack).
    pub const NO_ROOM: u32 = 10_065;
    /// The knife asks more of the player's craft skill than it has.
    pub const CRAFT_TOO_LOW: u32 = 10_067;
}

/// How many levels above the creature its killer may be before the corpse is too damaged to
/// butcher (2017: 108 of 114 corpses killed 9 or more levels above refused, none below of 2,680).
pub const TOO_DAMAGED_LEVELS: i32 = 9;

/// A material template standing for a profile's share of nothing.
pub const NOTHING_FOUND: u32 = 0;

/// A knife's wear: each cut lowers its durability (a visual quality) by one at even odds, a crude
/// knife's as a hunting knife's (2017: 1,448 of 3,005 cuts, 615 of 1,307 crude, 833 of 1,698
/// hunting); one worn to nothing is destroyed, "Your %1 has been destroyed through overuse!" (16
/// knives).
pub mod wear {
    pub const CHANCE: f32 = 0.5;
    pub const DURABILITY: u32 = 619;
    pub const DESTROYED: (u32, u32) = (0x31B4_7005, 0x2500_0014);
    pub const ITEM: u32 = 0x9352_2CD0;
    /// The kind of text it is shown as.
    pub const SHOWN_AS: u32 = 8;
}

/// The answer's texts and their variables.
pub mod text {
    pub const BUTCHERED: (u32, u32) = (0x3A4D_4800, 0x2500_0014);
    pub const EXPERTLY: (u32, u32) = (0x1061_4DAA, 0x2500_0014);
    pub const UNITS: u32 = 0x313C_55EA;
    pub const CORPSE: u32 = 0x1C06_9478;
    pub const MATERIAL: u32 = 0xA966_8450;
}

/// The answer's kind of target (an object of the world).
pub const TARGET_KIND: u32 = 8;

/// A material a butchery profile yields, and its share.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Yield {
    pub template: u32,
    pub weight: u32,
}

/// What a knife's own yield adds to every cut: a cut gives ten more than its yield says (2017: 2,955
/// of 2,985 cuts of knives whose yield is known, an expert cut's twice that; yields 1 to 33, and no
/// tie to the creature).
const CUT_ABOVE_YIELD: u32 = 10;

/// How many units a cut gives by the knife's yield.
#[must_use]
pub const fn units(tool_yield: u32) -> u32 {
    tool_yield + CUT_ABOVE_YIELD
}

/// A knife's yield: a made one's own; else a crude knife's 1, a hunting knife's the 6 of the lowest
/// level it is made at.
#[must_use]
pub fn knife_yield(template: u32, made: Option<u32>) -> u32 {
    made.unwrap_or(if template == HUNTING_KNIFE { 6 } else { 1 })
}

/// The knives.
const HUNTING_KNIFE: u32 = 0x4700_0CBB;
const CRUDE_KNIFE: u32 = 0x4700_0CBD;

/// Is this template a knife?
#[must_use]
pub const fn is_knife(template: u32) -> bool {
    matches!(template, HUNTING_KNIFE | CRUDE_KNIFE)
}

/// The chance of an expert cut, by the player's butchery level alone: a six hundredth of it (2017:
/// 250 of 2,999 cuts, 2% below level 20, 20% at 100 to 119; within a band of level the creature's
/// makes no difference).
#[must_use]
pub fn expert_chance(level: u32) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    (level as f32 / 600.0).min(1.0)
}

/// Is this use a knife the player holds, used on a corpse?
#[must_use]
pub fn is_butchery<S: Store>(s: &Server<S>, u: &Use) -> bool {
    u.target != 0
        && s.things
            .get(u.target)
            .is_some_and(|t| t.kind == Kind::Corpse)
        && s.things.get(u.item).is_some_and(|t| is_knife(t.template))
}

/// A butchery under way: the use, the corpse, the knife.
#[derive(Debug, Clone, PartialEq)]
pub struct Work {
    pub used: Use,
    pub corpse: u64,
    pub knife: u32,
}

/// The creature a corpse was: its template.
fn creature_of<S: Store>(s: &Server<S>, corpse: u64) -> Option<u32> {
    s.things
        .get(corpse)
        .and_then(|t| t.corpse.as_ref())
        .map(|c| c.creature)
}

/// Is another player on its way to open a corpse (its use of the corpse under way)?
fn being_opened<S: Store>(s: &Server<S>, world: ConnId, corpse: u64) -> bool {
    s.players.iter().any(|(w, p)| {
        *w != world
            && p.walking_to
                .as_ref()
                .is_some_and(|(u, _)| u.item == corpse && u.target == 0)
    })
}

/// Was a corpse's creature killed by a player too far above it to leave it fit to butcher?
#[must_use]
pub fn too_damaged(killer_level: i32, creature_level: i64) -> bool {
    killer_level > 0 && i64::from(killer_level) - creature_level >= i64::from(TOO_DAMAGED_LEVELS)
}

/// Why a player at a corpse may not butcher it (none: it may).
fn refusal_at<S: Store>(s: &Server<S>, world: ConnId, corpse: u64) -> Option<u32> {
    let Some(c) = s.things.get(corpse).and_then(|t| t.corpse.as_ref()) else {
        return Some(refusal::NOTHING);
    };
    let template = s.catalog.get(c.creature);
    let level = template
        .and_then(|t| t.props.int("GameplayStatistics_Level"))
        .unwrap_or(1);
    let has_yield = template
        .and_then(|t| t.props.word("ButcheryProfile"))
        .is_some_and(|p| s.crafting.butchery.contains_key(&p));
    if being_opened(s, world, corpse) {
        Some(refusal::LOCKED)
    } else if too_damaged(c.killer_level, level) {
        Some(refusal::TOO_DAMAGED)
    } else if c.butchered {
        Some(refusal::BUTCHERED)
    } else if !has_yield {
        Some(refusal::NOTHING)
    } else {
        None
    }
}

/// The player is at the corpse: refused, or the work begins.
pub fn begin<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    u: Use,
    clock: Clock,
) {
    let corpse = u.target;
    let me = s.players.get(&world).map_or(0, |p| p.character);
    // A knife that asks more of the craft skill than the player has does not cut (2017: 4 of 4; of
    // 1,678 cuts where both are known, none below it).
    let needs = s
        .things
        .get(u.item)
        .and_then(|t| t.own.made)
        .and_then(|m| m.craft_needed);
    let level = s
        .players
        .get(&world)
        .and_then(|p| p.play.craft_skills.iter().find(|c| c.skill == SKILL))
        .map_or(1, |c| c.level);
    let refused = if !crate::death::may_loot(s, me, corpse, clock.local.0) {
        Some(refusal::NO_RIGHT)
    } else if needs.is_some_and(|n| level < n) {
        Some(refusal::CRAFT_TOO_LOW)
    } else {
        refusal_at(s, world, corpse)
    };
    if let Some(why) = refused {
        let m = crate::places::use_answer_to(&u, me, 0, TARGET_KIND, 0.0, why);
        progress::private(net, world, &m);
        return;
    }
    let knife = s.things.get(u.item).map_or(0, |t| t.template);
    s.next_context = s.next_context.wrapping_add(1);
    let context = s.next_context;
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let mut b = BehaviorParams {
        bits: BehaviorParams::BEHAVIOR
            | 0x20
            | BehaviorParams::CONTEXT
            | BehaviorParams::HOLD_CYCLES,
        behavior: BEHAVIOR,
        context,
        ..BehaviorParams::default()
    };
    b.words[1] = 1;
    let plays = ServerMessage::PlayBehavior(crate::combat::player_event(p, b));
    let release = Release {
        behavior: BEHAVIOR,
        params: BehaviorParams {
            bits: BehaviorParams::CONTEXT,
            context,
            ..BehaviorParams::default()
        },
    };
    let lets_go = ServerMessage::LetGoBehavior(crate::combat::player_event(p, release));
    crate::combat::to_all_seeing(s, net, me, &plays);
    crate::combat::to_all_seeing(s, net, me, &lets_go);
    s.after(
        clock,
        WORK_SECONDS,
        Timer::Butcher {
            world,
            work: Box::new(Work {
                used: u,
                corpse,
                knife,
            }),
        },
    );
}

/// The work is done: the experience, the effect, the material, the answer.
pub fn done<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, w: &Work) {
    let Some(creature) = creature_of(s, w.corpse) else {
        return;
    };
    let Some(t) = s.catalog.get(creature) else {
        return;
    };
    let name = t.facts.name;
    let Some(yields) = t
        .props
        .word("ButcheryProfile")
        .and_then(|p| s.crafting.butchery.get(&p))
        .cloned()
    else {
        return;
    };
    let Some(def) = s.crafting.skills.iter().find(|d| d.did == SKILL).cloned() else {
        return;
    };
    let Some(record) = s
        .players
        .get(&world)
        .and_then(|p| p.play.craft_skills.iter().find(|c| c.skill == SKILL))
        .copied()
    else {
        return;
    };
    if s.things
        .get(w.corpse)
        .and_then(|t| t.corpse.as_ref())
        .is_some_and(|c| c.butchered)
    {
        return;
    }
    let holds_knife = s
        .players
        .get(&world)
        .is_some_and(|p| p.play.items.iter().any(|i| i.id == w.used.item));
    if !holds_knife {
        // What cut is gone: nothing found, nothing paid, the corpse still to butcher.
        let me = s.players.get(&world).map_or(0, |p| p.character);
        let m = crate::places::use_answer_to(&w.used, me, 0, TARGET_KIND, 0.0, refusal::EMPTY);
        progress::private(net, world, &m);
        return;
    }
    #[allow(clippy::cast_precision_loss)]
    let pick = s
        .rng
        .weighted(yields.iter().map(|y| y.weight as f32))
        .unwrap_or(0);
    let material = yields[pick.min(yields.len() - 1)].template;
    let me = s.players.get(&world).map_or(0, |p| p.character);
    if material == NOTHING_FOUND {
        // Nothing found: answered so, nothing paid, the corpse still to butcher.
        let m = crate::places::use_answer_to(&w.used, me, 0, TARGET_KIND, 0.0, refusal::EMPTY);
        progress::private(net, world, &m);
        return;
    }
    if s.players
        .get(&world)
        .is_some_and(|p| crate::inventory::pack_full(&p.play.items))
    {
        // No place for it: the corpse still to butcher, the knife unworn, nothing paid.
        let m = crate::places::use_answer_to(&w.used, me, 0, TARGET_KIND, 0.0, refusal::NO_ROOM);
        progress::private(net, world, &m);
        return;
    }
    let butcher = crate::social::name_of(s, world);
    if let Some(c) = s.things.get_mut(w.corpse).and_then(|t| t.corpse.as_mut()) {
        c.butchered = true;
        c.butchered_by = Some(butcher);
    }
    let expert = s.rng.chance(expert_chance(record.level));
    let own = s
        .things
        .get(w.used.item)
        .and_then(|t| t.own.made)
        .and_then(|m| m.tool_yield);
    let n = units(knife_yield(w.knife, own)) * if expert { 2 } else { 1 };
    craft::award(s, net, world, SKILL, craft::usage_xp(&def, &record));
    craft::play_fx(s, net, world, if expert { fx::MADE_WELL } else { fx::MADE });
    crate::inventory::give_stacked(s, net, world, material, n);
    wear_tool(s, net, world, w.used.item);
    let corpse_name = progress::nested(
        text::CORPSE,
        progress::text_with(
            crate::death::CORPSE_NAME,
            name.map(|n| vec![progress::nested(crate::death::CORPSE_OF, progress::text(n))])
                .unwrap_or_default(),
        ),
    );
    let material_name = s
        .catalog
        .get(material)
        .and_then(|m| m.facts.name)
        .map(|n| progress::nested(text::MATERIAL, progress::text(n)));
    let vars: Vec<Variable> = [
        Some(progress::word(text::UNITS, 2, n)),
        Some(corpse_name),
        material_name,
    ]
    .into_iter()
    .flatten()
    .collect();
    let said = progress::text_with(
        if expert {
            text::EXPERTLY
        } else {
            text::BUTCHERED
        },
        vars,
    );
    let mut used = w.used.clone();
    used.message = Some(said);
    let distance = s
        .things
        .get(w.corpse)
        .and_then(|t| t.at)
        .zip(s.players.get(&world).map(|p| p.position))
        .and_then(|(at, me)| crate::presence::distance(&me, &at))
        .unwrap_or(0.0);
    let answer = crate::places::use_answer_to(&used, me, 0, TARGET_KIND, distance, 0);
    net.send(
        world,
        auberean_protocol::queue::EVENT,
        BlobKind::PRIVATE,
        answer.encode(),
    );
    crate::world::keep_player(s, world);
}

/// A tool the player holds is worn by a use: at even odds its durability falls by one (told the
/// client), and at nothing the tool is destroyed. A tool without durability does not wear.
pub fn wear_tool<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, item: u64) {
    wear_tool_by(s, net, world, item, wear::CHANCE, 0);
}

/// An item the player holds is worn by a use at a chance: its durability falls by one (told the
/// client), and at nothing it is destroyed through overuse. Its durability is its template's
/// greatest, else `default` (none: it does not wear).
pub fn wear_tool_by<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    item: u64,
    chance: f32,
    default: u32,
) {
    let Some(t) = s.things.get(item) else {
        return;
    };
    let Some(most) = t
        .own
        .made
        .and_then(|m| m.durability)
        .map(i64::from)
        .or_else(|| {
            s.catalog
                .get(t.template)
                .and_then(|tp| tp.props.int("Item_Durability_MaxLevel"))
        })
        .and_then(|m| u32::try_from(m).ok())
        .or(Some(default))
        .filter(|m| *m > 0)
    else {
        return;
    };
    let (template, now, name) = (t.template, t.durability.unwrap_or(most), t.facts.name);
    if chance < 1.0 && !s.rng.chance(chance) {
        return;
    }
    let left = now.saturating_sub(1);
    if left == 0 {
        let vars = name
            .map(|n| vec![progress::nested(wear::ITEM, progress::text(n))])
            .unwrap_or_default();
        let said = progress::text_with(wear::DESTROYED, vars);
        progress::private(net, world, &crate::chat::show_text(wear::SHOWN_AS, said));
        crate::inventory::take_item(s, net, world, item, template);
        return;
    }
    if let Some(t) = s.things.get_mut(item) {
        t.durability = Some(left);
    }
    let m = ServerMessage::UpdateIntVisual(auberean_protocol::quality::VisualQuality {
        object: crate::inventory::visual(s, item),
        quality: wear::DURABILITY,
        value: i32::try_from(left).unwrap_or(i32::MAX),
    });
    crate::inventory::to_holder(s, net, world, item, &m);
    crate::world::keep_player(s, world);
}
