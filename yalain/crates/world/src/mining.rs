//! Surveying and mining: a player summons a resource node with a delving stone, then mines it for
//! its material.
//!
//! **Surveying.** A delving stone used from the pack conjures the node its summoning effect names
//! two meters straight ahead of the player, at once: the surveying skill's experience, "A rich new
//! source of raw materials appears before you!", the stone's durability lowered by one (at nothing
//! it is destroyed through overuse), the use answered, and the node with its entry effect. Only
//! the one who summoned a node may mine it.
//!
//! **Mining.** The summoner uses the node, walking to within ten meters of it first. It plays the
//! node's mining behavior (the craft skill's own, a rock hammer's on stone when one is in hand)
//! and lets it go, and about two seconds later the result: the mining skill's experience, the use
//! answered "You extract %1 units of ... from %2.", the craft effect (the expert one for a rich
//! find, twice the units, more often the higher the miner's skill), the craft skill's own tool in
//! hand worn at even odds, and the units put on the player's stack of the material (or a new
//! stack made). A use while the work is under way is refused. A node gives three uses: at the
//! third result it flashes, a use after is refused, and three and a half seconds later it goes.
//!
//! **The units** a node gives are set when it is summoned: about the summoner's surveying level
//! and a tenth of the node's base quantity, never fewer than that quantity, spread either side by
//! the node's variance.
//!
//! **Veins.** A craft node placed in the world (a hoard's vein) is no one's: anyone may mine it,
//! the units are drawn for each miner (the node's base quantity and half the miner's level in its
//! craft skill), it never finds anything, and it goes after its last use like a summoned node; a
//! while after it goes a new one stands in its place.

use auberean_protocol::physics::{Fx, ObjectEvent, Release, Word};
use auberean_protocol::types::BehaviorParams;
use auberean_protocol::ServerMessage;
use auberean_script::native_object::Variable;
use yalain_net::{ConnId, ServerNet};
use yalain_store::{Store, StoredPosition};

use crate::craft::{self, fx};
use crate::places::Use;
use crate::things::{Kind, Node, Thing};
use crate::{presence, progress, Clock, Server, Timer};

/// The surveying craft skill (its saved state).
pub const SURVEYING: u32 = 0x6F00_0690;

/// How near the node the player works, in meters (2017: 367 uses within 10 m began at once, 21
/// farther walked to 10 m first).
pub const REACH: f32 = 10.0;

/// How long after the behavior starts the result comes, in seconds (2017: median 1.967 s, 387
/// uses).
pub const WORK_SECONDS: f64 = 1.97;

/// How long a summoned node stays unused, in seconds (its summoning effect's magnitude).
pub const NODE_SECONDS: f64 = 900.0;

/// How long a node stays after its last use's result, in seconds (2017: median 3.517 s, 129
/// nodes).
pub const SPENT_SECONDS: f64 = 3.5;

/// How long after a spent vein goes a new one stands in its place, in seconds (2017: 122.3 and
/// 123.0 s, 2 of 2).
pub const VEIN_BACK_SECONDS: f64 = 123.0;

/// The effect a node plays at its last use's result, stopped as it goes.
pub const SPENT_FX: u32 = 0x8100_0373;

/// How far in front of the player a summoned node appears, and how far below its feet, in meters.
pub const SUMMON_DISTANCE: f32 = 2.0;
pub const SUMMON_DROP: f32 = 0.1;

/// A craft skill's mining behavior and its own tools (a hand-made and a crude one): crystal the
/// chisels, iron the pickaxes, silver the mattocks, stone the rock hammers, wood the hatchets.
#[must_use]
pub fn skill_tools(skill: u32) -> Option<(u32, [u32; 2])> {
    Some(match skill {
        0x6F00_069B => (0x4100_0149, [0x4700_0CB5, 0x4700_0CB6]),
        0x6F00_069C => (0x4100_014D, [0x4700_0CC1, 0x4700_0CC2]),
        0x6F00_069D => (0x4100_014C, [0x4700_0CBF, 0x4700_0CC0]),
        0x6F00_069E => (0x4100_0149, [0x4700_0CC3, 0x4700_0CC4]),
        0x6F00_069F => (0x4100_014A, [0x4700_0CB9, 0x4700_0CBA]),
        _ => return None,
    })
}

/// The mining craft skills.
pub const SKILLS: [u32; 5] = [
    0x6F00_069B,
    0x6F00_069C,
    0x6F00_069D,
    0x6F00_069E,
    0x6F00_069F,
];

/// A mining tool's durability when its template names none.
pub const TOOL_DURABILITY: u32 = 100;

/// Why a mining use is refused, as the use's answer says.
pub mod refusal {
    /// The node has given all its uses.
    pub const USED_UP: u32 = 10_060;
    /// No place in the pack for what it would give.
    pub const NO_ROOM: u32 = 10_057;
    /// Only the one who summoned the node may mine it.
    pub const SUMMONER_ONLY: u32 = 10_066;
}

/// The texts.
pub mod text {
    /// "A rich new source of raw materials appears before you! ...", with the effect's name and
    /// the player's.
    pub const SUMMONED: (u32, u32) = (0x912E_FF1D, 0x2500_2B5E);
    pub const SUMMONED_EFFECT: u32 = 0xB69B_365C;
    pub const SUMMONED_PLAYER: u32 = 0x9FD3_465E;
    /// The summoning effect's name, "Summon Delving Mine".
    pub const EFFECT_NAME: (u32, u32) = (0xA860_7CEC, 0x2500_2B5E);
    /// The kind of text it is shown as.
    pub const SHOWN_AS: u32 = 2;
    /// The answer's variables: the units and the node.
    pub const UNITS: u32 = 0x313C_55EA;
    pub const NODE: u32 = 0x9352_2CD0;
}

/// A find: one use of a node a surveyor's map led to may unearth an item in place of its material
/// (2017: 7 of 165 such uses; none of 162 on delving stones' nodes), a Tessera two times in seven,
/// else a rough lodestone of the material's family.
pub const FIND_CHANCE: f32 = 7.0 / 165.0;
pub const TESSERA_SHARE: f32 = 2.0 / 7.0;
pub const TESSERA: u32 = 0x4700_052F;

/// The rough lodestone a find of each craft skill's material unearths: crystal translucent, iron
/// rusty, silver clouded, stone marbled, wood mossy.
#[must_use]
pub fn lodestone(skill: u32) -> Option<u32> {
    Some(match skill {
        0x6F00_069B => 0x4700_0C1D,
        0x6F00_069C => 0x4700_0C17,
        0x6F00_069D => 0x4700_0C05,
        0x6F00_069E => 0x4700_0C0B,
        0x6F00_069F => 0x4700_0C11,
        _ => return None,
    })
}

/// "You unearth a %2 while mining!": the find's answer, its one unit and the item's name.
pub const FOUND: (u32, u32) = (0xCAE6_2208, 0x2500_000C);

/// The answer's kind of target (an object of the world).
pub const TARGET_KIND: u32 = 8;

/// The control flags of a delving stone's answer.
pub const SURVEY_FLAGS: u32 = 956;

/// A material's answers, plain and rich, by the craft skill that mines it.
#[must_use]
pub fn answers(skill: u32) -> Option<((u32, u32), (u32, u32))> {
    const T: u32 = 0x2500_0CEC;
    Some(match skill {
        0x6F00_069B => ((0x4862_57E3, T), (0x284F_06B2, T)),
        0x6F00_069C => ((0x79C2_76DB, T), (0x53EE_7371, T)),
        0x6F00_069D => ((0xEECA_8C2F, T), (0xF862_249F, T)),
        0x6F00_069E => ((0x8EA1_7A36, T), (0xDAF5_223D, T)),
        0x6F00_069F => ((0xF742_62DC, T), (0xF742_62DC, T)),
        _ => return None,
    })
}

/// The nodes delving stones conjure: crystal, iron, silver, stone, wood.
pub const SUMMONED_NODES: [u32; 5] = [
    0x4700_0C8A,
    0x4700_0C8C,
    0x4700_0C8E,
    0x4700_0C90,
    0x4700_0C92,
];

/// The node a delving stone's summoning effect conjures.
#[must_use]
pub fn summoned_by(effect: &str) -> Option<u32> {
    Some(match effect {
        "Eff_SummonMine_Crystal_Delving" => SUMMONED_NODES[0],
        "Eff_SummonMine_Iron_Delving" => SUMMONED_NODES[1],
        "Eff_SummonMine_Silver_Delving" => SUMMONED_NODES[2],
        "Eff_SummonMine_Stone_Delving" => SUMMONED_NODES[3],
        "Eff_SummonMine_Wood_Delving" => SUMMONED_NODES[4],
        _ => return None,
    })
}

/// The units a node summoned at a surveying level gives a use, on average: the level and a tenth
/// of the node's base quantity, never fewer than the base.
#[must_use]
pub fn units_for(base: u32, surveying: i32) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let base = base as f32;
    #[allow(clippy::cast_precision_loss)]
    let level = surveying.max(0) as f32;
    (level + base / 10.0).max(base)
}

/// The units a vein gives a use, on average: its base quantity and half the miner's level in its
/// craft skill (2017: a median of 135 for 38 plain uses of quantity 60 by a miner of level 150;
/// one level only).
#[must_use]
pub fn vein_units(base: u32, level: i32) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let base = base as f32;
    #[allow(clippy::cast_precision_loss)]
    let level = level.max(0) as f32;
    base + level / 2.0
}

/// The chance that a use is a rich find, yielding twice the units, at the miner's level in the
/// node's craft skill: a 440th of the level (2017: 49 of 388; 4 of 114 below level 20, 34 of 94
/// at 150).
#[must_use]
pub fn rich_chance(level: i32) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let l = level.max(0) as f32;
    (l / 440.0).min(1.0)
}

/// Is a thing a resource node?
fn node_of<S: Store>(s: &Server<S>, id: u64) -> Option<&Node> {
    s.things.get(id).and_then(|t| t.node.as_ref())
}

/// A mining use under way: the use, the node, the behavior played.
#[derive(Debug, Clone, PartialEq)]
pub struct Work {
    pub used: Use,
    pub node: u64,
}

/// A player's craft level in a skill (0 when it has none).
fn level_in<S: Store>(s: &Server<S>, world: ConnId, skill: u32) -> i32 {
    s.players
        .get(&world)
        .and_then(|p| p.play.craft_skills.iter().find(|c| c.skill == skill))
        .map_or(0, |c| i32::try_from(c.level).unwrap_or(i32::MAX))
}

/// A player uses a delving stone it holds: the node its effect names appears in front of it.
/// Returns false for an item that summons no node.
pub fn survey<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    u: &Use,
    clock: Clock,
) -> bool {
    let Some(stone) = s.things.get(u.item).map(|t| t.template) else {
        return false;
    };
    let Some(node_template) = s
        .catalog
        .get(stone)
        .and_then(|t| t.props.text("Usage_Effect1"))
        .and_then(summoned_by)
    else {
        return false;
    };
    if s.catalog.get(node_template).is_none() {
        return false;
    }
    let Some(p) = s.players.get(&world) else {
        return false;
    };
    let (me, name) = (p.character, p.name.clone());
    // The surveying skill's experience, the text, the stone spent, the answer, the node.
    surveying_xp(s, net, world);
    let effect = progress::nested(text::SUMMONED_EFFECT, progress::text(text::EFFECT_NAME));
    let said = progress::text_with(
        text::SUMMONED,
        vec![
            effect,
            progress::nested(text::SUMMONED_PLAYER, crate::profile::literal(&name)),
        ],
    );
    progress::private(net, world, &crate::chat::show_text(text::SHOWN_AS, said));
    crate::butchery::wear_tool_by(s, net, world, u.item, 1.0, 1);
    let done = crate::places::use_done(u, me, SURVEY_FLAGS, false, 0.0);
    progress::private(net, world, &done);
    conjure(s, net, world, node_template, false, clock);
    true
}

/// The surveying skill's experience for a node summoned.
pub(crate) fn surveying_xp<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId) {
    let Some(def) = s
        .crafting
        .skills
        .iter()
        .find(|d| d.did == SURVEYING)
        .cloned()
    else {
        return;
    };
    if let Some(record) = s
        .players
        .get(&world)
        .and_then(|p| p.play.craft_skills.iter().find(|c| c.skill == SURVEYING))
        .copied()
    {
        craft::award(s, net, world, SURVEYING, craft::usage_xp(&def, &record));
    }
}

/// A node of a template appears in front of a player, its units set by the player's surveying
/// level (one a map leads to may give finds). Returns its id.
pub(crate) fn conjure<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    node_template: u32,
    finds: bool,
    clock: Clock,
) -> Option<u64> {
    let template = s.catalog.get(node_template).cloned()?;
    let p = s.players.get(&world)?;
    let (me, at) = (p.character, p.position);
    let surveying = level_in(s, world, SURVEYING);
    let props = &template.props;
    let base = props
        .int("Mine_ObjectQuantity")
        .and_then(|v| u32::try_from(v).ok())
        .unwrap_or(1);
    let node = Node {
        summoner: me,
        units: units_for(base, surveying),
        variance: props.float("Mine_ObjectQuantityVariance").unwrap_or(0.0),
        material: props.word("Mine_Object").unwrap_or(0),
        skill: props.word("CraftSkill").unwrap_or(0),
        max_uses: props
            .int("Mine_MaxUses")
            .and_then(|v| u32::try_from(v).ok())
            .unwrap_or(1),
        uses: 0,
        busy: false,
        expires: clock.local.0 + NODE_SECONDS,
        finds,
        public: false,
    };
    let id = s.things.new_id();
    let mut t = Thing::of(id, &template, Kind::Node);
    t.at = Some(in_front(&at));
    t.fresh = true;
    t.node = Some(node);
    s.things.insert(t);
    presence::thing_appears(s, net, id);
    crate::world::keep_player(s, world);
    Some(id)
}

/// Where a summoned node stands: [`SUMMON_DISTANCE`] straight ahead of the player, facing as it
/// faces, [`SUMMON_DROP`] below its feet.
fn in_front(at: &StoredPosition) -> StoredPosition {
    let mut p = crate::drops::landing(at, SUMMON_DISTANCE);
    p.origin[2] = at.origin[2] - SUMMON_DROP;
    p.rotation = at.rotation;
    p
}

/// The item in a player's primary hand: its id and template.
fn in_hand<S: Store>(s: &Server<S>, world: ConnId) -> Option<(u64, u32)> {
    s.players.get(&world).and_then(|p| {
        p.play
            .items
            .iter()
            .find(|h| h.location & crate::equip::PRIMARY_HAND != 0)
            .map(|h| (h.id, h.template))
    })
}

/// Whether a character's pack and its bags hold an item in every place.
fn pack_full_of<S: Store>(s: &Server<S>, me: u64) -> bool {
    s.players
        .values()
        .find(|p| p.character == me)
        .is_some_and(|p| crate::inventory::pack_full(&p.play.items))
}

/// Why a player at a node may not mine it (none: it may).
fn refusal_at<S: Store>(s: &Server<S>, me: u64, node: u64) -> Option<u32> {
    let n = node_of(s, node)?;
    if !n.public && n.summoner != me {
        Some(refusal::SUMMONER_ONLY)
    } else if n.uses >= n.max_uses {
        Some(refusal::USED_UP)
    } else if pack_full_of(s, me) {
        Some(refusal::NO_ROOM)
    } else {
        None
    }
}

/// The player is at the node: refused, or the work begins.
pub fn begin<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    u: Use,
    clock: Clock,
) {
    let me = s.players.get(&world).map_or(0, |p| p.character);
    // A node its quest keeps the player from (the hoard's veins, while the hoard's quest is under
    // way) is refused at once (2017: 3 of 3 after the quest ran out, the mysterious word).
    if let Some((why, at)) = s.things.get(u.item).and_then(|t| {
        let props = &s.catalog.get(t.template)?.props;
        Some((crate::item_use::quest_gate(s, world, props, clock)?, t.at))
    }) {
        let distance = at
            .and_then(|at| crate::presence::distance(&u.from, &at))
            .unwrap_or(0.0);
        let m = crate::places::use_refused(&u, 0, Some(quest_refusal::FLAGS), distance, why);
        progress::private(net, world, &m);
        return;
    }
    if let Some(why) = refusal_at(s, me, u.item) {
        let m = crate::places::use_answer_to(&u, me, 0, TARGET_KIND, 0.0, why);
        progress::private(net, world, &m);
        return;
    }
    let skill = node_of(s, u.item).map_or(0, |n| n.skill);
    let (own, tools) = skill_tools(skill).unwrap_or((0x4100_014D, [0, 0]));
    let behavior = in_hand(s, world)
        .filter(|(_, t)| tools.contains(t))
        .and_then(|(_, t)| s.catalog.get(t))
        .and_then(|t| t.props.word("Usage_UserAnimation"))
        .unwrap_or(own);
    if let Some(n) = s.things.get_mut(u.item).and_then(|t| t.node.as_mut()) {
        n.busy = true;
    }
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
        behavior,
        context,
        ..BehaviorParams::default()
    };
    b.words[1] = 1;
    let plays = ServerMessage::PlayBehavior(crate::combat::player_event(p, b));
    let release = Release {
        behavior,
        params: BehaviorParams {
            bits: BehaviorParams::CONTEXT,
            context,
            ..BehaviorParams::default()
        },
    };
    let lets_go = ServerMessage::LetGoBehavior(crate::combat::player_event(p, release));
    crate::combat::to_all_seeing(s, net, me, &plays);
    crate::combat::to_all_seeing(s, net, me, &lets_go);
    let node = u.item;
    s.after(
        clock,
        WORK_SECONDS,
        Timer::Mine {
            world,
            work: Box::new(Work { used: u, node }),
        },
    );
}

/// The work is done: the experience, the answer, the effect, the tool's wear, the material; at
/// the last use the node flashes and goes a little later.
pub fn done<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    w: &Work,
    clock: Clock,
) {
    let Some(n) = node_of(s, w.node).cloned() else {
        return;
    };
    let node_name = s.things.get(w.node).and_then(|t| t.facts.name);
    let me = s.players.get(&world).map_or(0, |p| p.character);
    let Some((plain, rich)) = answers(n.skill) else {
        return;
    };
    let find = if n.finds && s.rng.chance(FIND_CHANCE) {
        if s.rng.chance(TESSERA_SHARE) {
            Some(TESSERA)
        } else {
            lodestone(n.skill)
        }
    } else {
        None
    };
    let level = level_in(s, world, n.skill);
    let expert = find.is_none() && s.rng.chance(rich_chance(level));
    let spread = s.rng.between(-n.variance, n.variance);
    // A vein's units are drawn for its miner; a summoned node's were set for its summoner.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let draw = if n.public {
        vein_units(n.units as u32, level)
    } else {
        n.units
    };
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let units = if find.is_some() {
        1
    } else {
        ((draw * (1.0 + spread)).round().max(1.0) as u32) * if expert { 2 } else { 1 }
    };
    if let Some(def) = s.crafting.skills.iter().find(|d| d.did == n.skill).cloned() {
        if let Some(record) = s
            .players
            .get(&world)
            .and_then(|p| p.play.craft_skills.iter().find(|c| c.skill == n.skill))
            .copied()
        {
            craft::award(s, net, world, n.skill, craft::usage_xp(&def, &record));
        }
    }
    // A find is named by the item unearthed; the material by the node.
    let named = match find {
        Some(t) => s.catalog.get(t).and_then(|t| t.facts.name),
        None => node_name,
    };
    // The find comes into the pack before the answer.
    if let Some(t) = find {
        crate::inventory::give(s, net, world, t, 1);
    }
    let vars: Vec<Variable> = [
        Some(progress::word(text::UNITS, 2, units)),
        named.map(|n| progress::nested(text::NODE, progress::text(n))),
    ]
    .into_iter()
    .flatten()
    .collect();
    let said = progress::text_with(
        match find {
            Some(_) => FOUND,
            None if expert => rich,
            None => plain,
        },
        vars,
    );
    let mut used = w.used.clone();
    used.message = Some(said);
    let distance = s
        .things
        .get(w.node)
        .and_then(|t| t.at)
        .zip(s.players.get(&world).map(|p| p.position))
        .and_then(|(at, me)| presence::distance(&me, &at))
        .unwrap_or(0.0);
    let answer = crate::places::use_answer_to(&used, me, 0, TARGET_KIND, distance, 0);
    progress::private(net, world, &answer);
    craft::play_fx(s, net, world, if expert { fx::MADE_WELL } else { fx::MADE });
    let tools = skill_tools(n.skill).map_or([0, 0], |(_, t)| t);
    if let Some((tool, _)) = in_hand(s, world)
        .filter(|(_, t)| tools.contains(t))
        .filter(|_| find.is_none())
    {
        crate::butchery::wear_tool_by(
            s,
            net,
            world,
            tool,
            crate::butchery::wear::CHANCE,
            TOOL_DURABILITY,
        );
    }
    if find.is_none() {
        crate::inventory::give_stacked(s, net, world, n.material, units);
    }
    let last = if let Some(n) = s.things.get_mut(w.node).and_then(|t| t.node.as_mut()) {
        n.uses += 1;
        n.busy = false;
        if n.uses >= n.max_uses {
            n.expires = clock.local.0 + SPENT_SECONDS;
            true
        } else {
            false
        }
    } else {
        false
    };
    if last {
        let m = ServerMessage::DoFx(ObjectEvent {
            sender: crate::inventory::visual(s, w.node),
            action: Fx {
                fx: SPENT_FX,
                scalar: 1.0,
            },
        });
        crate::combat::to_all_seeing(s, net, w.node, &m);
    }
    crate::world::keep_player(s, world);
}

/// A node the world database's reload takes away: a player's work on it is answered, the node
/// used up, and nothing of it comes.
pub(crate) fn release<S: Store>(s: &mut Server<S>, net: &mut ServerNet, node: u64) {
    let mut i = 0;
    while i < s.timers.len() {
        let Timer::Mine { world, work } = &s.timers[i].1 else {
            i += 1;
            continue;
        };
        if work.node != node {
            i += 1;
            continue;
        }
        let (world, used) = (*world, work.used.clone());
        s.timers.remove(i);
        let me = s.players.get(&world).map_or(0, |p| p.character);
        let m = crate::places::use_answer_to(&used, me, 0, TARGET_KIND, 0.0, refusal::USED_UP);
        progress::private(net, world, &m);
    }
}

/// A node that goes: its id, whether it is spent, and a vein's template and place to come back to.
type Going = (u64, bool, Option<(u32, StoredPosition)>);

/// Nodes whose time is up leave the world (a spent one stops its flash first); a vein that goes
/// comes back where it stood [`VEIN_BACK_SECONDS`] later.
pub fn expire<S: Store>(s: &mut Server<S>, net: &mut ServerNet, now: f64) {
    let gone: Vec<Going> = s
        .things
        .by_id
        .values()
        .filter_map(|t| {
            let n = t.node.as_ref()?;
            let back = n.public.then_some(t.template).zip(t.at);
            (n.expires <= now && !n.busy).then_some((t.id, n.uses >= n.max_uses, back))
        })
        .collect();
    for (id, spent, back) in gone {
        if spent {
            let m = ServerMessage::StopFx(ObjectEvent {
                sender: crate::inventory::visual(s, id),
                action: Word(SPENT_FX),
            });
            crate::combat::to_all_seeing(s, net, id, &m);
        }
        crate::death::vanish(s, net, id);
        if let Some((template, at)) = back {
            s.timers.push((
                yalain_net::LocalTime(now + VEIN_BACK_SECONDS),
                Timer::VeinBack { template, at },
            ));
        }
    }
}

/// Is a template a craft node's (a node mined by a craft skill for a material)?
#[must_use]
pub fn is_craft_node(t: &yalain_content::Template) -> bool {
    t.props.word("CraftSkill").is_some() && t.props.word("Mine_Object").is_some()
}

/// Make a vein of a template stand at a place: a node anyone may mine, as many times as its
/// template allows, that stays until it is spent. Returns its id.
pub fn vein<S: Store>(s: &mut Server<S>, template: u32, at: StoredPosition) -> Option<u64> {
    let template = s.catalog.get(template).cloned()?;
    let props = &template.props;
    #[allow(clippy::cast_precision_loss)]
    let base = props
        .int("Mine_ObjectQuantity")
        .and_then(|v| u32::try_from(v).ok())
        .unwrap_or(1) as f32;
    let node = Node {
        summoner: 0,
        units: base,
        variance: props.float("Mine_ObjectQuantityVariance").unwrap_or(0.0),
        material: props.word("Mine_Object").unwrap_or(0),
        skill: props.word("CraftSkill").unwrap_or(0),
        max_uses: props
            .int("Mine_MaxUses")
            .and_then(|v| u32::try_from(v).ok())
            .unwrap_or(1),
        uses: 0,
        busy: false,
        expires: f64::INFINITY,
        finds: false,
        public: true,
    };
    let id = s.things.new_id();
    let mut t = Thing::of(id, &template, Kind::Node);
    t.at = Some(at);
    t.node = Some(node);
    s.things.insert(t);
    Some(id)
}

/// The world's placed craft nodes become veins: each placed object of a craft node's template
/// leaves the world's objects and stands as a vein in its place.
pub fn place_veins<S: Store>(s: &mut Server<S>) {
    let (veins, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut s.content.objects)
        .into_iter()
        .partition(|o| s.catalog.get(o.template).is_some_and(is_craft_node));
    s.content.objects = rest;
    for o in veins {
        let at = StoredPosition {
            cell: o.cell,
            origin: o.origin,
            rotation: o.rotation,
        };
        vein(s, o.template, at);
    }
}

/// A spent vein's time away is over: a new one stands where it stood.
pub fn vein_back<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    template: u32,
    at: StoredPosition,
) {
    if let Some(id) = vein(s, template, at) {
        presence::thing_appears(s, net, id);
    }
}

/// A mine placed in the world that a quest or a recipe works rather than a craft skill: what it
/// gives, the quest it needs, whether a recipe mines it, and the use's behavior and its cycles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuestMine {
    pub material: u32,
    pub quantity: u32,
    pub quest: u32,
    pub by_recipe: bool,
    pub behavior: u32,
    pub cycles: u32,
}

/// The uses a quest mine's quest gives: the last one completes it (2017: the fifth, 2 of 2).
pub const QUEST_MINE_USES: u32 = 5;

/// How long after its behavior begins a quest mine's use gives its material, in seconds (2017:
/// median 7.86 s, 10 uses).
pub const QUEST_MINE_SECONDS: f64 = 7.86;

/// Why a quest mine refuses: the player's quest is not under way (or done); the mine is worked by
/// a recipe, not by a use. Both answer with these control flags.
pub mod quest_refusal {
    pub const WRONG_QUEST: u32 = 10_020;
    pub const BY_RECIPE: u32 = 1039;
    pub const FLAGS: u32 = 820;
}

/// "You extract some raw material from %2.", with the units and the mine's name.
pub const EXTRACTED: (u32, u32) = (0x3308_AA76, 0x2500_0014);

/// Is a template a quest mine (a mine with no craft skill)?
#[must_use]
pub fn quest_mine<S: Store>(s: &Server<S>, template: u32) -> Option<QuestMine> {
    let p = &s.catalog.get(template)?.props;
    if p.word("CraftSkill").is_some() {
        return None;
    }
    Some(QuestMine {
        material: p.word("Mine_Object")?,
        quantity: p
            .int("Mine_ObjectQuantity")
            .and_then(|v| u32::try_from(v).ok())
            .unwrap_or(1),
        quest: p.word("Usage_RequiredQuest").unwrap_or(0),
        by_recipe: p.text("Mine_Recipe").is_some(),
        behavior: p.word("Usage_UserAnimation").unwrap_or(0x4100_014D),
        cycles: p
            .int("Usage_UserAnimationRepeatCount")
            .and_then(|v| u32::try_from(v).ok())
            .unwrap_or(0),
    })
}

/// A quest mine's use under way: the use and the mine.
#[derive(Debug, Clone, PartialEq)]
pub struct MineWork {
    pub used: Use,
    pub mine: u64,
    pub template: u32,
}

/// A player at a quest mine: refused (a recipe mines it; its quest is not under way), or the work
/// begins: the mine's behavior held for its cycles, the material some seconds later.
pub fn at_quest_mine<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    u: Use,
    template: u32,
    clock: Clock,
) {
    let Some(m) = quest_mine(s, template) else {
        return;
    };
    let (me, phase) = s.players.get(&world).map_or((0, 0), |p| {
        (
            p.character,
            crate::quests::phase_of(&p.play.quests, m.quest),
        )
    });
    let why = if m.by_recipe {
        Some(quest_refusal::BY_RECIPE)
    } else if phase == 0 && crate::quests::hidden(s, m.quest) {
        Some(crate::item_use::refusal::MYSTERIOUS_USE)
    } else if phase == 0 {
        Some(quest_refusal::WRONG_QUEST)
    } else if pack_full_of(s, me) {
        Some(refusal::NO_ROOM)
    } else {
        None
    };
    if let Some(why) = why {
        progress::private(
            net,
            world,
            &crate::places::use_refused(&u, 0, Some(quest_refusal::FLAGS), 0.0, why),
        );
        return;
    }
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
        behavior: m.behavior,
        hold_cycles: m.cycles,
        context,
        ..BehaviorParams::default()
    };
    b.words[1] = 1;
    let plays = ServerMessage::PlayBehavior(crate::combat::player_event(p, b));
    crate::combat::to_all_seeing(s, net, me, &plays);
    let mine = u.item;
    s.after(
        clock,
        QUEST_MINE_SECONDS,
        Timer::QuestMine {
            world,
            work: Box::new(MineWork {
                used: u,
                mine,
                template,
            }),
        },
    );
}

/// A quest mine's work is done: the quest moves on (the last use completes it), the use is
/// answered with the material extracted, the craft effect plays, and the material comes into the
/// pack. No experience; the mine stays.
pub fn quest_mine_done<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    w: &MineWork,
    clock: Clock,
) {
    let Some(m) = quest_mine(s, w.template) else {
        return;
    };
    let Some(p) = s.players.get(&world) else {
        return;
    };
    let (me, phase) = (
        p.character,
        crate::quests::phase_of(&p.play.quests, m.quest),
    );
    if phase > 0 {
        let update = if phase >= QUEST_MINE_USES {
            yalain_content::quests::Update::Complete
        } else {
            yalain_content::quests::Update::Advance(phase + 1)
        };
        let step = yalain_content::quests::Step {
            quest: m.quest,
            needs: phase,
            trigger: yalain_content::quests::Trigger::Use(w.template),
            tell: None,
            update,
            xp: 0,
            gives: Vec::new(),
            counts: Vec::new(),
            effects: Vec::new(),
            text: None,
            placed: String::new(),
            requires: Vec::new(),
            takes: Vec::new(),
            to: None,
            races: Vec::new(),
            effects_last: false,
            wants: None,
            fellows: false,
            levels: None,
            xp_share: 0.0,
        };
        crate::quests::run(s, net, world, &step, None, clock);
    }
    let name = presence::content_by_id(&s.content.objects, w.mine)
        .and_then(|i| s.content.objects[i].description.name);
    let vars: Vec<Variable> = [
        Some(progress::word(text::UNITS, 2, m.quantity)),
        name.map(|n| progress::nested(text::NODE, progress::text(n))),
    ]
    .into_iter()
    .flatten()
    .collect();
    let mut used = w.used.clone();
    used.message = Some(progress::text_with(EXTRACTED, vars));
    let answer = crate::places::use_answer_to(&used, me, 0, TARGET_KIND, 0.0, 0);
    progress::private(net, world, &answer);
    craft::play_fx(s, net, world, fx::MADE);
    crate::inventory::give_stacked(s, net, world, m.material, m.quantity);
    crate::world::keep_player(s, world);
}
