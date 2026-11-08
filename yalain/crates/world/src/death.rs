//! Death: a creature's death, the experience it is worth, its corpse and what the corpse holds;
//! corpses leaving the world.
//!
//! **The killing blow** (in one tick): the creature's experience is paid in parts, one for each
//! damager by the health it took ([`crate::xp`]): a player's to it, a pet's to its owner, each with
//! "You have gained %1 experience points!" and the totals (a hero is paid the creature's death focus
//! in the same parts: [`crate::focus`]; in a fellowship that shares experience,
//! shared out among the fellows without a text), another creature's to no one; then the killer is
//! told "You have killed the %1." and moves on any quest the kill triggers. Every client that sees
//! the creature sees it dead, wounded, unselectable, in its death pose, playing its death. Its camp's replacement is due after the
//! camp's period. **Two seconds later** its corpse appears where it fell (drawn as the creature,
//! named "Corpse of ..." the creature, its killer recorded, holding what the creature left), and
//! **three seconds after that** the creature leaves the world.
//!
//! **No fair fight.** The part of a damager the creature could not reach in its fight (going after
//! it, nothing led to it: no way through the walls, or a closed door it could not open) is held
//! back, from it and its fellows alike, and the player it was for is told once "You receive no
//! experience, since that wasn't a fair fight. Your opponent couldn't even reach you!" in its
//! place among the parts; the other damagers' parts are paid. A killer it could not reach moves no
//! quest on. (2017: 12 such texts, each before the kill text, 11 of 11; a pet's part held back
//! and its owner's other part paid, 2 of 2; a player's own held back and another part of the kill
//! paid to it, 4 of 4; a vault boss killed so did not complete its vault's quest, 8 of 8 uses of
//! the vault's portal after one refused.)
//!
//! **What a corpse holds**: the creature's fixed drops at their rates, the named extras of its
//! loot profile at theirs, and as many items again drawn from the profile's items as the
//! profile's count says (counting the drops and extras already in). Each item drawn carries what
//! the creature's level gives it of its own ([`crate::treasure`]).
//!
//! **A corpse leaves the world** by its creature's template's times, for most creatures these: three
//! minutes after it appeared, or fifteen seconds if it can hold nothing (the creature has no loot
//! profile and left nothing); fifteen seconds after it was closed empty, a minute after it was
//! closed with items left, if that is sooner. One still open when its time comes stays, looked at
//! again each time an empty one's time goes by, and goes at the first such look after it is closed.
//! A player's lasts a minute whatever is done to it.
//!
//! **Who may loot it** ([`may_loot`]): its killer and, in a fellowship that shares loot, the killer's
//! fellows as they stand at that moment. The first to open it ends the rights, as does two minutes
//! after the death, whoever is about by then; so no corpse is ever closed to everyone for longer
//! than that, and it leaves the world when it would have anyway. A creature killed by no player, and
//! a player's own corpse, which holds nothing, are open to all.
//!
//! **A character that leaves the world dead** comes back as it next enters, with no box and nothing
//! to ask for ([`back_after_entry`]).
//!
//! **A player left dead** with its box up is resurrected ten minutes after it was told it was dead,
//! as if it had asked ([`sent_back`]). This rests on one recording.

use auberean_protocol::physics::{CreateObject, Leave, ObjectEvent, ObjectOnly, Slide, Word};
use auberean_protocol::quality::VisualQuality;
use auberean_protocol::script::{ClientCall, FunctionId, ObjectCall, ScriptArgs};
use auberean_protocol::text::{StringInfo, Variable, VariableValue};
use auberean_protocol::types::{
    BehaviorParams, ObjectStamp, PhysicsDesc, RunningFx, SliderSetting, VisualDesc, WeenieDesc,
};
use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use auberean_script::native_object::TextInfo;
use yalain_net::{BlobKind, ConnId, ServerNet};
use yalain_store::Store;

use crate::combat::to_all_seeing;
use crate::inventory::visual;
use crate::progress::{self, Award};
use crate::things::{Corpse, Kind, Mode, Thing};
use crate::world::Player;
use crate::{quals, session, Clock, Server, Timer};

/// The corpse's template and script package.
pub const CORPSE_TEMPLATE: u32 = 0x4700_0539;
pub const CORPSE_PACKAGE: u32 = 0x182;

/// The corpse's name ("Corpse of %1") and its variable.
pub const CORPSE_NAME: (u32, u32) = (0x509A_8FF6, 0x2500_0011);
pub const CORPSE_OF: u32 = 0xE4EA_98E6;

/// The corpse's physics type, movement and placement ethereality (low words; the high words 0).
pub const CORPSE_NUMBERS: [i32; 6] = [0x30, 0, 0x0220_0010, 0, 0x0220_0010, 0];

/// Qualities a death changes.
pub mod q {
    pub const DEAD: u32 = 279;
    pub const SELECTABLE: u32 = 624;
    pub const ETHEREAL_LOW: u32 = 3;
    pub const PLUNDERER: u32 = 320;
    pub const ORIGINATOR: u32 = 321;
    pub const CLAIMANT: u32 = 322;
    /// An object's name.
    pub const NAME: u32 = 256;
    /// A thing's scale.
    pub use auberean_physics::body::profile::quality::SCALE;
}

/// The death's look.
pub mod look {
    pub const POSE: u32 = 0x4000_001D;
    pub const DEATH: u32 = 0x4100_001F;
    pub const DEAD_MODE: u32 = 0x4100_0005;
    pub const WOUNDED: u32 = 0x4100_0002;
    pub const CORPSE_FX: u32 = 0x4100_0004;
    pub const CORPSE_MODE: u32 = 0x4100_0003;
    pub const SLIDER: u32 = 0x4000_000A;
    /// A dead creature's ethereal physics type, and the bit a dead pet's own takes.
    pub const ETHEREAL: i32 = 0x40_0000 | 0x30;
    pub const DEAD_BIT: i32 = 0x10;
}

/// When the corpse appears after the death, and the creature leaves after it, in seconds.
pub const CORPSE_AFTER: f64 = 2.0;
pub const LEAVES_AFTER: f64 = 3.0;

/// How long after a death its corpse is kept for those with a right to it, in seconds.
pub const LOOT_LOCK: f64 = 120.0;

/// The most items a corpse holds.
pub const MOST_ITEMS: usize = 21;

/// The call that tells a fellow its blow killed a creature whose kill went to another fellow, with
/// the creature's name.
pub const KILLING_BLOW: u32 = 0x8207_002D;

/// Who a creature's kill goes to: the player in the world that did it the most damage (a pet's
/// counted as its owner's), else the one whose blow killed it.
#[must_use]
pub fn credited<S: Store>(s: &Server<S>, id: u64, blow: u64) -> u64 {
    let Some(c) = s.things.get(id).and_then(|t| t.creature.as_ref()) else {
        return blow;
    };
    let mut best: Option<(u64, i64)> = None;
    for (by, dealt) in &c.damage_by {
        if !s.players.values().any(|p| p.character == *by) {
            continue;
        }
        if best.is_none_or(|(_, d)| *dealt > d) {
            best = Some((*by, *dealt));
        }
    }
    best.map_or(blow, |(by, _)| by)
}

/// May this character open or cut this corpse now?
#[must_use]
pub fn may_loot<S: Store>(s: &Server<S>, me: u64, corpse: u64, now: f64) -> bool {
    let Some(c) = s.things.get(corpse).and_then(|t| t.corpse.as_ref()) else {
        return false;
    };
    if c.first_looted_by.is_some() || now >= c.unlocks {
        return true;
    }
    if c.claimant != 0 {
        return c.claimant == me;
    }
    if c.killer == me || c.killer == 0 || s.things.get(c.killer).is_some() {
        return true;
    }
    let f = s.fellowships.of(me);
    f.is_some_and(|i| s.fellowships.list[i].shares_loot()) && f == s.fellowships.of(c.killer)
}

/// A creature dies by a player's blow.
pub fn creature_dies<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    id: u64,
    blow: u64,
    clock: Clock,
) {
    if s.pets.is_pet(id) {
        // A pet killed leaves no corpse and no award: it falls and is gone.
        crate::pets::dies(s, net, id, blow, clock);
        return;
    }
    // The kill goes to the player that did the most damage; a fellow of its whose blow killed the
    // creature is told so (2017: 5,038 of 5,362 fellowship kills credited to the most damage; 511
    // of 511 killing-blow calls to a fellow given no kill text).
    let killer = credited(s, id, blow);
    let blow_fellow = (blow != killer
        && s.fellowships
            .of(blow)
            .is_some_and(|f| s.fellowships.of(killer) == Some(f)))
    .then(|| {
        s.players
            .iter()
            .find(|(_, p)| p.character == blow)
            .map(|(w, _)| *w)
    })
    .flatten();
    let Some(t) = s.things.get_mut(id) else {
        return;
    };
    let Some(c) = t.creature.as_mut() else { return };
    if c.mode == Mode::Dead {
        return;
    }
    c.mode = Mode::Dead;
    c.target = None;
    let unfair = c.unreached.contains(&killer);
    let parts = std::mem::take(&mut c.taken_by);
    let unreached = c.unreached.clone();
    let death_effects = c.stats.death_effects.clone();
    let (template, level, death_xp, death_focus, camp, name, group) = (
        t.template,
        c.stats.level,
        c.stats.death_xp,
        c.stats.death_focus,
        c.camp,
        t.facts.name,
        t.facts.group,
    );
    paid_in_parts(
        s,
        net,
        &parts,
        &unreached,
        Worth {
            xp: death_xp,
            focus: death_focus,
            level,
            group,
        },
    );
    if let Some(world) = world_of(s, killer) {
        progress::killed(net, world, name);
        // A killer the creature could not reach moves no quest on.
        if !unfair {
            crate::quests::killed(s, net, world, (id, template), &death_effects, clock);
            crate::death_spawn::killed(s, net, world, id, template, &death_effects, clock);
        }
    }
    if let Some(w) = blow_fellow {
        let victim = name.map_or_else(Default::default, progress::text);
        let m =
            session::call(KILLING_BLOW, &[crate::chat::text_arg(victim)]).expect("a name packs");
        progress::private(net, w, &m);
    }
    crate::talk::dies(s, net, id, killer);
    let pose = falls(s, net, id, clock);
    if let Some(i) = camp {
        crate::spawn::died(s, i, id, clock.local.0);
    }
    s.after(
        clock,
        CORPSE_AFTER,
        Timer::Corpse {
            creature: id,
            killer,
            pose,
        },
    );
}

/// The world connection of a player in the world, by character.
fn world_of<S: Store>(s: &Server<S>, character: u64) -> Option<ConnId> {
    s.players
        .iter()
        .find(|(_, p)| p.character == character)
        .map(|(w, _)| *w)
}

/// What a creature's death is worth: its experience and focus, its level, and whether it is a
/// group creature.
#[derive(Debug, Clone, Copy)]
struct Worth {
    xp: i64,
    focus: i32,
    level: i32,
    group: bool,
}

/// A kill's experience and focus, paid in parts in the order its damagers first hurt it: each
/// damager's part (a pet's paid to its owner, any other's to no one) is the creature's experience
/// by the level of the player it is paid to, and its focus, times the health the damager took over
/// all the health taken, rounded to the nearest; in a fellowship that shares experience it is
/// shared out. A part worth nothing is not sent. A part of a damager the creature could not reach
/// is held back, and the player it was for told once that the fight was not fair; the other parts
/// are paid.
fn paid_in_parts<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    parts: &[(u64, i64)],
    unreached: &[u64],
    worth: Worth,
) {
    let all: i64 = parts.iter().map(|(_, t)| *t).sum();
    let mut told: Vec<ConnId> = Vec::new();
    let hurt: Vec<u64> = parts
        .iter()
        .map(|(by, _)| s.pets.by_id.get(by).map_or(*by, |p| p.owner))
        .collect();
    let group = worth.group.then_some(hurt.as_slice());
    for &(by, taken) in parts {
        let owner = s.pets.by_id.get(&by).map_or(by, |p| p.owner);
        let Some(world) = world_of(s, owner) else {
            continue;
        };
        if unreached.contains(&by) {
            if !told.contains(&world) {
                told.push(world);
                progress::unfair(net, world);
            }
            continue;
        }
        if let Some(shares) =
            crate::fellowship::shares(s, owner, worth.xp, worth.level, group, (taken, all))
        {
            let focus = crate::fellowship::focus_shares(s, owner, worth.focus, (taken, all));
            for (w, amount) in shares {
                let f = focus.iter().find(|(x, _)| *x == w).map_or(0, |(_, f)| *f);
                if amount > 0 || f > 0 {
                    progress::award_with_focus(s, net, w, amount, f, Award::Share);
                }
            }
        } else {
            let own_level = s
                .players
                .get(&world)
                .map_or(1, |p| crate::examine::level_of(&p.qualities));
            let award = crate::xp::kill_award(worth.xp, worth.level, own_level);
            let part = crate::xp::part(award, taken, all);
            let focus = crate::xp::part(i64::from(worth.focus), taken, all);
            let focus = i32::try_from(focus).unwrap_or(i32::MAX);
            if part > 0 || focus > 0 {
                progress::award_with_focus(s, net, world, part.cast_unsigned(), focus, Award::Part);
            }
        }
    }
}

/// A creature falls, for every client that sees it: dead, no longer selectable (the wounded
/// effect between), its death pose's slider, its death behavior, and its place standing still.
/// Returns the pose.
pub(crate) fn falls<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    id: u64,
    clock: Clock,
) -> f32 {
    for (quality, value) in [(q::DEAD, 1), (q::SELECTABLE, 0)] {
        let m = ServerMessage::UpdateBoolVisual(VisualQuality {
            object: visual(s, id),
            quality,
            value,
        });
        to_all_seeing(s, net, id, &m);
        if quality == q::DEAD {
            let fx = ServerMessage::DoFx(ObjectEvent {
                sender: visual(s, id),
                action: auberean_protocol::physics::Fx {
                    fx: look::WOUNDED,
                    scalar: 1.0,
                },
            });
            to_all_seeing(s, net, id, &fx);
        }
    }
    // One of the six death poses, a whole number (2017: every creature death so, 0 to 5).
    #[allow(clippy::cast_precision_loss)]
    let pose = s.rng.below(6) as f32;
    let slide = ServerMessage::MoveSlider(ObjectEvent {
        sender: visual(s, id),
        action: Slide {
            slider: look::POSE,
            value: pose,
            time: 0.0,
        },
    });
    to_all_seeing(s, net, id, &slide);
    s.next_context = s.next_context.wrapping_add(1);
    let behave = ServerMessage::PlayBehavior(ObjectEvent {
        sender: visual(s, id),
        action: BehaviorParams {
            bits: BehaviorParams::BEHAVIOR | BehaviorParams::MODE | BehaviorParams::CONTEXT,
            behavior: look::DEATH,
            mode: look::DEAD_MODE,
            context: s.next_context,
            ..BehaviorParams::default()
        },
    });
    to_all_seeing(s, net, id, &behave);
    if let Some(t) = s.things.get_mut(id) {
        t.pose = Some(pose);
    }
    // It stops where it fell: its place told once more, standing still (2017: 9,676 of 11,062
    // deaths, a median 0.07 s after the death, at the same place).
    crate::ai::stands(s, net, id, clock);
    pose
}

/// The corpse's object as clients are told of it, from the corpse's own state: its killer and the
/// one it was, its name, its pose (the pose's slider left out when it is the first pose: 2017's
/// corpses never carried a pose of 0), the look it is drawn with.
#[must_use]
pub fn corpse_create(t: &Thing) -> CreateObject {
    let at = t.at.unwrap_or(yalain_store::StoredPosition {
        cell: 0,
        origin: [0.0; 3],
        rotation: [1.0, 0.0, 0.0, 0.0],
    });
    let (killer, originator) = t
        .corpse
        .as_ref()
        .map_or((0, 0), |c| (c.killer, c.originator));
    let mut sliders = vec![SliderSetting {
        slider: look::SLIDER,
        value: 1.0,
        time: 0.0,
    }];
    if let Some(pose) = t.pose.filter(|p| *p != 0.0) {
        sliders.push(SliderSetting {
            slider: look::POSE,
            value: pose,
            time: 0.0,
        });
    }
    let physics = PhysicsDesc {
        bits: PhysicsDesc::SLIDERS | PhysicsDesc::MODE | PhysicsDesc::POSITION | PhysicsDesc::FX,
        instance_stamp: 0,
        visual_ordering_stamp: t.visual,
        mode: look::CORPSE_MODE,
        sliders,
        fx: vec![
            RunningFx {
                fx: look::WOUNDED,
                scalar: 1.0,
                target: 0,
            },
            RunningFx {
                fx: look::CORPSE_FX,
                scalar: 1.0,
                target: 0,
            },
        ],
        position: crate::player::placement(&at),
        stamps: [1, 0, 0, 0],
        ..PhysicsDesc::default()
    };
    let name = t.named.clone().unwrap_or(StringInfo {
        string_id: CORPSE_NAME.0,
        table_id: CORPSE_NAME.1,
        ..StringInfo::default()
    });
    let mut weenie = crate::player::weenie_with_numbers(
        WeenieDesc {
            mask: WeenieDesc::PACKAGE
                | WeenieDesc::ENTITY
                | WeenieDesc::NAME
                | WeenieDesc::SCALE
                | WeenieDesc::OBJECT_BITS[3]
                | WeenieDesc::OBJECT_BITS[4]
                | WeenieDesc::OBJECT_BITS[5],
            package_id: CORPSE_PACKAGE,
            entity_did: CORPSE_TEMPLATE,
            name,
            scale: t.description.scale.unwrap_or(1.0),
            ..WeenieDesc::default()
        },
        CORPSE_NUMBERS,
    );
    // The originator, the claimant, the killer.
    weenie.objects[3] = originator;
    weenie.objects[4] = t.corpse.as_ref().map_or(0, |c| c.claimant);
    weenie.objects[5] = killer;
    CreateObject {
        id: t.id,
        visual: t.drawn_as.clone().unwrap_or(VisualDesc {
            bits: VisualDesc::PARENT,
            parent: t.description.look,
            ..VisualDesc::default()
        }),
        physics,
        weenie,
    }
}

/// A corpse's name: "Corpse of" the name given (the creature's, from a string table).
#[must_use]
pub fn corpse_name(creature_name: Option<(u32, u32)>) -> StringInfo {
    let mut name = StringInfo {
        string_id: CORPSE_NAME.0,
        table_id: CORPSE_NAME.1,
        ..StringInfo::default()
    };
    if let Some((sid, table)) = creature_name {
        name.variable_count = 1;
        name.variables = vec![Variable {
            id: CORPSE_OF,
            kind: 9,
            value: VariableValue::Nested(Box::new(StringInfo {
                string_id: sid,
                table_id: table,
                ..StringInfo::default()
            })),
        }];
    }
    name
}

/// A dead creature's object as clients are told of it in the seconds before it leaves: as it
/// stood, but in its dead mode and its death pose (the pose's slider left out for the first pose).
#[must_use]
pub fn dead_create(t: &Thing) -> CreateObject {
    let mut o = crate::things::standing_create(t);
    o.physics.bits |= PhysicsDesc::MODE | PhysicsDesc::SLIDERS;
    o.physics.mode = look::DEAD_MODE;
    if let Some(pose) = t.pose.filter(|p| *p != 0.0) {
        o.physics.sliders.push(SliderSetting {
            slider: look::POSE,
            value: pose,
            time: 0.0,
        });
    }
    o
}

/// The value of a treasure item a creature of a level leaves (the 2017 values' middle half by the
/// dropper's level: 1-4 under level 10, then 4-11, 10-24, 21-34, 41-84, 75-156).
fn treasure_value<S: Store>(s: &mut Server<S>, level: i32) -> u32 {
    let (lo, hi): (u32, u32) = match level {
        ..=9 => (1, 4),
        10..=19 => (4, 11),
        20..=29 => (10, 24),
        30..=39 => (21, 34),
        40..=49 => (41, 84),
        _ => (75, 156),
    };
    lo + u32::try_from(s.rng.below((hi - lo + 1) as usize)).unwrap_or(0)
}

/// What a dead creature leaves: its fixed drops, its profile's extras, and the profile's draw.
fn roll_loot<S: Store>(s: &mut Server<S>, creature: &Thing) -> Vec<(u32, u32)> {
    let Some(c) = creature.creature.as_ref() else {
        return Vec::new();
    };
    let mut out: Vec<(u32, u32)> = Vec::new();
    for (t, rate) in c.stats.trophies.clone() {
        if s.rng.chance(rate) {
            out.push((t, 0));
        }
    }
    let profile = c
        .stats
        .loot_profile
        .and_then(|v| s.catalog.profile_name(v))
        .and_then(|n| s.loot.profile(n))
        .cloned();
    if let Some(p) = profile {
        for (t, rate) in &p.extras {
            if s.rng.chance(*rate) {
                out.push((*t, 0));
            }
        }
        let count = s.rng.weighted(p.counts.iter().copied()).unwrap_or(0);
        let min = c
            .stats
            .loot_min
            .and_then(|m| usize::try_from(m).ok())
            .unwrap_or(0);
        let max = c
            .stats
            .loot_max
            .and_then(|m| usize::try_from(m).ok())
            .unwrap_or(MOST_ITEMS);
        let count = count.clamp(min.min(max), max);
        let level = c.stats.level;
        while out.len() < count && !p.items.is_empty() {
            #[allow(clippy::cast_precision_loss)]
            let Some(i) = s.rng.weighted(p.items.iter().map(|(_, w)| *w as f32)) else {
                break;
            };
            let value = treasure_value(s, level);
            out.push((p.items[i].0, value));
        }
    }
    out.truncate(MOST_ITEMS);
    out
}

/// Two seconds after a death: the corpse appears where the creature fell.
pub fn corpse_appears<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    creature: u64,
    killer: u64,
    pose: f32,
    clock: Clock,
) {
    let Some(dead) = s.things.get(creature).cloned() else {
        return;
    };
    // A pet keeps its own physics type, kingdom and all, with the dead bit.
    let value = if dead.summoner.is_some() {
        dead.description.numbers[0] | look::DEAD_BIT
    } else {
        look::ETHEREAL
    };
    let ethereal = ServerMessage::UpdateIntVisual(VisualQuality {
        object: visual(s, creature),
        quality: q::ETHEREAL_LOW,
        value,
    });
    to_all_seeing(s, net, creature, &ethereal);
    let Some(corpse_t) = s.catalog.get(CORPSE_TEMPLATE).cloned().or_else(|| {
        // A corpse looks like the creature whatever its own template says.
        s.catalog.get(dead.template).cloned()
    }) else {
        return;
    };
    let id = s.things.new_id();
    let mut corpse = Thing::of(id, &corpse_t, Kind::Corpse);
    corpse.description = dead.description.clone();
    corpse.description.package = CORPSE_PACKAGE;
    corpse.at = dead.at;
    corpse.instance = 0;
    corpse.visual = 10;
    let loot = roll_loot(s, &dead);
    let (level, quality, rot, has_profile) = dead.creature.as_ref().map_or(
        (0, None, yalain_content::CorpseRot::default(), false),
        |c| {
            (
                c.stats.level,
                c.stats.loot_quality,
                c.stats.rot,
                c.stats.loot_profile.is_some(),
            )
        },
    );
    let mut contents = Vec::new();
    for (template, value) in loot {
        if let Some(item) = crate::inventory::make_item(s, template, 1, value) {
            // A treasure item (one drawn from the profile, with a value) carries its own.
            if value > 0 {
                crate::treasure::endow(s, item, level, quality);
            }
            if let Some(t) = s.things.get_mut(item) {
                t.container = Some((id, u32::try_from(contents.len()).unwrap_or(0)));
            }
            contents.push(item);
        }
    }
    let killer_level = s
        .players
        .values()
        .find(|p| p.character == killer)
        .map_or(0, |p| crate::examine::level_of(&p.qualities));
    // One that can hold nothing goes as a corpse closed empty does.
    let lasts = if contents.is_empty() && !has_profile {
        rot.empty
    } else {
        rot.untouched
    };
    let group = dead.facts.group;
    let claimant = s.fellowships.of(killer).map_or(0, |i| {
        let Server {
            fellowships, rng, ..
        } = &mut *s;
        fellowships.list[i].next_claimant(group, rng)
    });
    corpse.corpse = Some(Corpse {
        killer,
        originator: creature,
        contents,
        expires: clock.local.0 + lasts,
        rot,
        creature: dead.template,
        butchered: false,
        killer_level,
        claimant,
        unlocks: clock.local.0 + LOOT_LOCK - CORPSE_AFTER,
        killed_by: Some(killer_name(s, Some(killer))),
        ..Corpse::default()
    });
    corpse.named = Some(corpse_name(dead.facts.name));
    corpse.pose = Some(pose);
    let scale = corpse.description.scale.unwrap_or(1.0);
    let create = ServerMessage::CreateObject(corpse_create(&corpse));
    let at = corpse.at;
    s.things.insert(corpse);
    if let Some(at) = at {
        let viewers: Vec<_> = s
            .players
            .iter()
            .filter(|(_, p)| crate::presence::in_sight(s.ground.as_deref(), p, at.cell, at.origin))
            .map(|(c, _)| *c)
            .collect();
        for c in viewers {
            net.send(c, queue::EVENT, BlobKind::UNORDERED, create.encode());
            if let Some(p) = s.players.get_mut(&c) {
                p.known.insert(
                    id,
                    crate::presence::Seen {
                        position_stream: net.new_stream(c).unwrap_or(0),
                        head_stream: net.new_stream(c).unwrap_or(0),
                    },
                );
            }
        }
    }
    let name = s
        .things
        .get(id)
        .and_then(|t| t.named.clone())
        .unwrap_or_else(|| corpse_name(None));
    for m in corpse_settles(id, scale, pose, (killer, claimant), creature, name) {
        to_all_seeing(s, net, id, &m);
    }
    s.after(clock, LEAVES_AFTER, Timer::Vanish { id: creature });
}

/// What follows a corpse's object whenever a client is told of it, a creature's or a player's, as it
/// appears or later, in 2017's order (9,601 of 10,856 corpses; 9,934 with the first five in some
/// order): its scale, its two effects, its pose, its mode, then who may claim it (nobody), its
/// killer, the creature or player it was, and its name. Their visual stamps run from 2 to 10, the
/// object's own, as the corpse was made up of them (the name at 10: 10,663 of 10,663 creatures'
/// corpses, 496 of 506 players').
#[must_use]
pub fn corpse_settles(
    id: u64,
    scale: f32,
    pose: f32,
    (killer, claimant): (u64, u64),
    creature: u64,
    name: StringInfo,
) -> Vec<ServerMessage> {
    let at = |n: u16| ObjectStamp {
        id,
        instance_stamp: 0,
        other_stamp: n,
    };
    let fx = |n: u16, fx: u32| {
        ServerMessage::DoFx(ObjectEvent {
            sender: at(n),
            action: auberean_protocol::physics::Fx { fx, scalar: 1.0 },
        })
    };
    let mut out = vec![
        ServerMessage::UpdateFloatVisual(VisualQuality {
            object: at(2),
            quality: q::SCALE,
            value: scale,
        }),
        fx(3, look::WOUNDED),
        fx(4, look::CORPSE_FX),
        ServerMessage::MoveSlider(ObjectEvent {
            sender: at(5),
            action: Slide {
                slider: look::POSE,
                value: pose,
                time: 0.0,
            },
        }),
        ServerMessage::SetMode(ObjectEvent {
            sender: at(6),
            action: Word(look::CORPSE_MODE),
        }),
    ];
    for (n, quality, value) in [
        (7, q::CLAIMANT, claimant),
        (8, q::PLUNDERER, killer),
        (9, q::ORIGINATOR, creature),
    ] {
        out.push(ServerMessage::UpdateInstanceIdVisual(VisualQuality {
            object: at(n),
            quality,
            value,
        }));
    }
    out.push(ServerMessage::UpdateStringInfoVisual(VisualQuality {
        object: at(10),
        quality: q::NAME,
        value: name,
    }));
    out
}

/// A name as its examination shows it, from the string reference the world tells clients.
#[must_use]
pub fn shown_name(i: &StringInfo) -> TextInfo {
    let mut t = TextInfo {
        string_id: i.string_id,
        table: i.table_id,
        has_literal: i.has_literal,
        literal: i.literal.as_ref().map(|l| l.0.clone()),
        ..TextInfo::default()
    };
    for v in &i.variables {
        if let VariableValue::Nested(n) = &v.value {
            t.set_variable(progress::nested(v.id, shown_name(n)));
        }
    }
    t
}

/// A corpse's block (see [`corpse_settles`]), from the corpse as the world keeps it; nothing for a
/// thing that is no corpse.
#[must_use]
pub fn corpse_block(t: &Thing) -> Vec<ServerMessage> {
    let Some(c) = t.corpse.as_ref() else {
        return Vec::new();
    };
    corpse_settles(
        t.id,
        t.description.scale.unwrap_or(1.0),
        t.pose.unwrap_or(0.0),
        (c.killer, c.claimant),
        c.originator,
        t.named.clone().unwrap_or_else(|| corpse_name(None)),
    )
}

/// A thing leaves the world: every client that knows it forgets it and sees it leave.
pub fn vanish<S: Store>(s: &mut Server<S>, net: &mut ServerNet, id: u64) {
    let Some(t) = s.things.remove(id) else { return };
    let forget = ServerMessage::ForgetObject(ObjectOnly(ObjectStamp {
        id,
        instance_stamp: t.instance,
        other_stamp: 0,
    }))
    .encode();
    let leave = ServerMessage::ObjectLeaves(ObjectEvent {
        sender: ObjectStamp {
            id,
            instance_stamp: t.instance,
            other_stamp: 0,
        },
        action: Leave {
            stamp: t.position_stamp.wrapping_add(1),
        },
    })
    .encode();
    // A creature's weapons go with it: forgotten after it, before it is seen to leave.
    let weapons = t
        .creature
        .as_ref()
        .map(|c| crate::arms::take_out(s, &c.weapons))
        .unwrap_or_default();
    let forget_weapons: Vec<(u64, Vec<u8>)> = weapons
        .iter()
        .map(|(w, instance)| {
            let m = ServerMessage::ForgetObject(ObjectOnly(ObjectStamp {
                id: *w,
                instance_stamp: *instance,
                other_stamp: 0,
            }));
            (*w, m.encode())
        })
        .collect();
    for (conn, p) in &mut s.players {
        if let Some(seen) = p.known.remove(&id) {
            net.send(*conn, queue::EVENT, BlobKind::UNORDERED, forget.clone());
            for (w, m) in &forget_weapons {
                if p.known.remove(w).is_some() {
                    net.send(*conn, queue::EVENT, BlobKind::UNORDERED, m.clone());
                }
            }
            net.send_ephemeral(*conn, queue::EVENT, seen.position_stream, leave.clone());
        }
    }
    if let Some(c) = t.corpse {
        for item in c.contents {
            s.things.remove(item);
        }
    }
}

/// A corpse has been closed: it goes soon, sooner when it is empty, never later than it would have.
pub fn closed<S: Store>(s: &mut Server<S>, corpse: u64) {
    let now = s.last_step.map_or(0.0, |t| t.0);
    if let Some(c) = s.things.get_mut(corpse).and_then(|t| t.corpse.as_mut()) {
        let after = if c.contents.is_empty() {
            c.rot.empty
        } else {
            c.rot.items_left
        };
        c.expires = c.expires.min(now + after);
    }
}

/// Corpses whose time is up leave the world; one that is open is looked at again later.
pub fn expire_corpses<S: Store>(s: &mut Server<S>, net: &mut ServerNet, now: f64) {
    let open: Vec<u64> = s
        .players
        .values()
        .filter_map(|p| p.open_container)
        .collect();
    let mut due = Vec::new();
    for t in s.things.by_id.values_mut() {
        let Some(c) = t.corpse.as_mut().filter(|c| c.expires <= now) else {
            continue;
        };
        if open.contains(&t.id) {
            c.expires = now.max(c.expires + c.rot.empty);
        } else {
            due.push(t.id);
        }
    }
    for id in due {
        vanish(s, net, id);
    }
}

/// What a player's death has come to: who killed it, its pose, its corpse once made.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Dying {
    pub killer: Option<u64>,
    pub pose: f32,
    /// The corpse, once it is made (the client may then ask to be resurrected).
    pub corpse: Option<u64>,
}

/// The client functions of a player's death.
pub mod player_fn {
    /// "You died": the last attacker's name.
    pub const DEATH_MESSAGE: u32 = 0x8207_002B;
    /// Who killed the player, told on the player to it and everyone who sees it.
    pub const WHO_KILLED: u32 = 0x8207_0030;
    /// The player's death state (1 dead, 0 alive again).
    pub const DEATH_STATE: u32 = 0x8207_0033;
    /// The client asks to be resurrected.
    pub const RESURRECT: u32 = 0x8207_000E;
}

/// What killed a player that no creature killed ("misadventure").
pub const MISADVENTURE: (u32, u32) = (0x8A15_568C, 0x2500_0015);

/// The look of a player's death and return.
pub mod player_look {
    /// The death behavior's flags beside behavior, mode and context (0x40, 0x80, 0x4_0000, each
    /// with a word of 1).
    pub const DEATH_FLAGS: u32 = 0x4_00C0;
    /// The return: its behavior and its flag (0x8_0000, a word of 1).
    pub const REVIVE: u32 = 0x4100_0085;
    pub const REVIVE_FLAG: u32 = 0x8_0000;
    /// The resurrection's effect as the player goes: the lifestone protection's glow, which stays
    /// until the protection ends.
    pub const RESURRECTING: u32 = crate::effects::protection::GLOW;
}

/// When a player's corpse is made after its death (2017: 10.07 s for a human or tumerok body; a
/// lugian's in most poses 5.09 s).
pub const PLAYER_CORPSE_AFTER: f64 = 10.07;

/// The vitae a resurrection takes, and the share of health and vigor a player returns with (2017:
/// 191 of 191 resurrections took 5; 202 of 202 returned at 0.75 of the greatest health).
pub const VITAE_LOST: f32 = 5.0;
pub const RETURNS_WITH: f32 = 0.75;

/// How long a player may sit dead with its box up, in seconds, before the server resurrects it as if
/// it had asked (2017: one recording, 599.99 s after it was told it was dead, with no request; the
/// longest wait of the 227 that asked was 251.3 s).
pub const SENT_BACK_AFTER: f64 = 600.0;

/// How long after its description a character that left the world dead comes back (2017: 1.43 s,
/// 1 of 1).
pub const BACK_AFTER_ENTRY: f64 = 1.43;

/// The vitae cost table's saved state.
pub const VITAE_COSTS: u32 = 0x7000_1BE6;

/// Read the vitae cost table's graph: its map, the cost of each level from 1.
///
/// # Errors
/// The map is missing.
pub fn vitae_costs_from_state(
    lib: &auberean_script::library::Library,
    g: &auberean_script::graph::Graph,
) -> Result<Vec<u64>, String> {
    use auberean_script::native_object::Item;
    let nav = crate::start::Nav::new(lib, g);
    let root = nav.object(g.root().ok_or("an empty graph")?)?;
    nav.items(nav.field(root, "m_map")?)?
        .into_iter()
        .map(|i| match i {
            Item::Long(v) => Ok(v),
            Item::Word(v) => Ok(u64::from(v)),
            other => Err(format!("{other:?} in the vitae cost table")),
        })
        .collect()
}

/// The vitae quality and the experience still to earn to raise it.
pub mod vitae_q {
    pub const VITAE: u32 = 286;
    pub const XP_TO_RAISE: u32 = 316;
}

/// The least vitae a player of a level may have: none lost below level 6, else 3 less a level
/// from 100, between 60 and 99 (the library's death rules).
#[must_use]
pub fn min_vitae(level: i32) -> f32 {
    if level < 6 {
        return 100.0;
    }
    #[allow(clippy::cast_precision_loss)]
    let v = (100 - 3 * (level - 1)) as f32;
    v.clamp(60.0, 99.0)
}

/// The experience to earn to raise vitae by one, at a level and vitae: the level's cost (its row
/// of the vitae cost table) times the vitae's share, rounded, at least 1 (the library's death
/// rules); none at full vitae.
#[must_use]
pub fn xp_to_raise(costs: &[u64], level: i32, vitae: f32) -> Option<u64> {
    if !(1.0..100.0).contains(&vitae) {
        return None;
    }
    let row = usize::try_from(level - 1)
        .ok()
        .and_then(|i| costs.get(i).copied())
        .unwrap_or(1_000_000);
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    let x = (row as f64 * f64::from(vitae) / 100.0 + 0.5) as u64;
    Some(x.max(1))
}

fn player_stamp(p: &mut Player) -> ObjectStamp {
    p.visual = p.visual.wrapping_add(1);
    ObjectStamp {
        other_stamp: p.visual,
        ..p.stamp()
    }
}

/// What killed a player, as its death names it: a creature by its name, another player by its name
/// as the world writes it (2017: `Sighclops [M]`, the killer's own name with its mark, 1 of 1), else
/// misadventure.
fn killer_name<S: Store>(s: &Server<S>, killer: Option<u64>) -> TextInfo {
    let Some(k) = killer else {
        return progress::text(MISADVENTURE);
    };
    if let Some(w) = crate::social::world_of(s, k) {
        return crate::social::name_text(&crate::social::name_of(s, w));
    }
    progress::text(
        s.things
            .get(k)
            .and_then(|t| t.facts.name)
            .unwrap_or(MISADVENTURE),
    )
}

/// A player dies: in one tick its death message (the last attacker's name), and for everyone who
/// sees it, it is dead, wounded, its killer announced, unselectable, in a death pose, playing its
/// death. Its corpse is made ten seconds later.
pub fn player_dies<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    killer: Option<u64>,
    clock: Clock,
) {
    let Some(p) = s.players.get(&world) else {
        return;
    };
    if p.dying.is_some() {
        return;
    }
    let me = p.character;
    crate::places::walk_cut_short(s, net, world);
    crate::fellowship::died(s, net, me);
    // A skill's use under way is cut short (2017: the use over, the move interrupted).
    crate::skill_use::interrupt(
        s,
        net,
        world,
        crate::skill_use::refusal::MOVE_INTERRUPTED,
        clock,
    );
    // Its quests that fail when it dies fail first (2017: in the death's tick, before its
    // message).
    crate::quests::died(s, net, world, clock);
    let Some(p) = s.players.get(&world) else {
        return;
    };
    let name = killer_name(s, killer);
    let attacking = p.attacking.is_some();
    if attacking {
        if let Some(p) = s.players.get_mut(&world) {
            p.attacking = None;
        }
        progress::private(
            net,
            world,
            &session::call(crate::combat::function::ATTACK_STATE, &[Arg::Int(0)]).expect("packs"),
        );
    }
    let said = session::call(
        player_fn::DEATH_MESSAGE,
        &[crate::chat::text_arg(name.clone())],
    )
    .expect("packs");
    progress::private(net, world, &said);
    // So is an item's use, after the message.
    crate::item_use::cut_short(s, net, world);
    #[allow(clippy::cast_precision_loss)]
    let pose = s.rng.below(6) as f32;
    s.next_context = s.next_context.wrapping_add(1);
    let context = s.next_context;
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    p.dying = Some(Dying {
        killer,
        pose,
        corpse: None,
    });
    let id = p.character;
    let dead = ServerMessage::UpdateBoolVisual(VisualQuality {
        object: player_stamp(p),
        quality: q::DEAD,
        value: 1,
    });
    let wounded = ServerMessage::DoFx(ObjectEvent {
        sender: player_stamp(p),
        action: auberean_protocol::physics::Fx {
            fx: look::WOUNDED,
            scalar: 1.0,
        },
    });
    let who = ServerMessage::VisualObjectCall(ObjectCall {
        object: player_stamp(p),
        call: ClientCall {
            function: FunctionId(player_fn::WHO_KILLED),
            args: Some(
                ScriptArgs::from_values(&[
                    crate::chat::text_arg(name),
                    Arg::LongInt(killer.unwrap_or(0)),
                ])
                .expect("packs"),
            ),
        },
    });
    let unselectable = ServerMessage::UpdateBoolVisual(VisualQuality {
        object: player_stamp(p),
        quality: q::SELECTABLE,
        value: 0,
    });
    let slide = ServerMessage::MoveSlider(ObjectEvent {
        sender: player_stamp(p),
        action: Slide {
            slider: look::POSE,
            value: pose,
            time: 0.0,
        },
    });
    let mut b = BehaviorParams {
        bits: BehaviorParams::BEHAVIOR
            | BehaviorParams::MODE
            | player_look::DEATH_FLAGS
            | BehaviorParams::CONTEXT,
        behavior: look::DEATH,
        mode: look::DEAD_MODE,
        context,
        ..BehaviorParams::default()
    };
    // The words of flags 0x80, 0x40 and 0x4_0000.
    b.words[2] = 1;
    b.words[3] = 1;
    b.words[4] = 1;
    let behave = ServerMessage::PlayBehavior(ObjectEvent {
        sender: player_stamp(p),
        action: b,
    });
    for m in [dead, wounded, who, unselectable, slide, behave] {
        to_all_seeing(s, net, id, &m);
    }
    // Its pets die with it, in its death tick.
    crate::pets::owner_dies(s, net, id, clock);
    s.after(clock, PLAYER_CORPSE_AFTER, Timer::PlayerCorpse { world });
}

/// Ten seconds after a player's death: frozen, its vigor gone, dead as far as its client knows
/// (which may now ask to be resurrected), and its corpse made where it fell: an empty corpse that
/// looks like the player, named for it, which rots a minute later.
pub fn player_corpse<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    clock: Clock,
) {
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let Some(d) = p.dying.filter(|d| d.corpse.is_none()) else {
        return;
    };
    progress::private(net, world, &session::movement_frozen(true));
    quals::set_int(&mut p.qualities, crate::player::quality::VIGOR, 0);
    progress::send_int(net, world, crate::player::quality::VIGOR, 0);
    progress::private(
        net,
        world,
        &session::call(player_fn::DEATH_STATE, &[Arg::Int(1)]).expect("packs"),
    );
    let (character, at, look, name, sex) = (
        p.character,
        p.position,
        p.look.clone(),
        p.name.clone(),
        p.sex,
    );
    // A player's corpse is the corpse template's object drawn as the player.
    let corpse_t =
        s.catalog
            .get(CORPSE_TEMPLATE)
            .cloned()
            .unwrap_or_else(|| yalain_content::Template {
                id: CORPSE_TEMPLATE,
                description: yalain_content::Description {
                    name_text: None,
                    package: CORPSE_PACKAGE,
                    look: 0,
                    enter_fx: 0,
                    scale: Some(1.0),
                    name: Some(CORPSE_NAME),
                    numbers: CORPSE_NUMBERS,
                    source: "the corpse template".into(),
                },
                facts: yalain_content::Facts::default(),
                props: yalain_content::Props::default(),
            });
    let id = s.things.new_id();
    let mut corpse = Thing::of(id, &corpse_t, Kind::Corpse);
    corpse.description.package = CORPSE_PACKAGE;
    corpse.at = Some(at);
    corpse.instance = 0;
    corpse.visual = 10;
    corpse.corpse = Some(Corpse {
        killer: d.killer.unwrap_or(0),
        originator: character,
        contents: Vec::new(),
        expires: clock.local.0 + yalain_content::CorpseRot::PLAYER.untouched,
        rot: yalain_content::CorpseRot::PLAYER,
        creature: 0,
        butchered: false,
        killer_level: 0,
        killed_by: Some(killer_name(s, d.killer)),
        ..Corpse::default()
    });
    // Drawn as the player, "Corpse of" the character's name as the world shows it.
    corpse.drawn_as = Some(look);
    corpse.pose = Some(d.pose);
    let mut named = corpse_name(None);
    named.variable_count = 1;
    named.variables = vec![Variable {
        id: CORPSE_OF,
        kind: 9,
        value: VariableValue::Nested(Box::new(StringInfo {
            has_literal: 1,
            literal: Some(auberean_protocol::text::WideString(
                crate::player::world_name(&name, sex)
                    .encode_utf16()
                    .collect(),
            )),
            ..StringInfo::default()
        })),
    }];
    corpse.named = Some(named);
    let create = corpse_create(&corpse);
    s.things.insert(corpse);
    if let Some(p) = s.players.get_mut(&world) {
        p.dying = Some(Dying {
            corpse: Some(id),
            ..d
        });
    }
    let m = ServerMessage::CreateObject(create);
    let viewers: Vec<_> = s
        .players
        .iter()
        .filter(|(_, q)| crate::presence::in_sight(s.ground.as_deref(), q, at.cell, at.origin))
        .map(|(c, _)| *c)
        .collect();
    for c in viewers {
        net.send(c, queue::EVENT, BlobKind::UNORDERED, m.encode());
        if let Some(q) = s.players.get_mut(&c) {
            q.known.insert(
                id,
                crate::presence::Seen {
                    position_stream: net.new_stream(c).unwrap_or(0),
                    head_stream: net.new_stream(c).unwrap_or(0),
                },
            );
        }
    }
    let block = s.things.get(id).map(corpse_block).unwrap_or_default();
    for m in block {
        to_all_seeing(s, net, id, &m);
    }
    s.after(
        clock,
        SENT_BACK_AFTER,
        Timer::SentBack { world, corpse: id },
    );
}

/// Ten minutes after a player's client was told it was dead: still dead of the same death, it is
/// resurrected as if it had asked.
pub fn sent_back<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    corpse: u64,
    clock: Clock,
) {
    let waiting = s
        .players
        .get(&world)
        .is_some_and(|p| p.trip.is_none() && p.dying.is_some_and(|d| d.corpse == Some(corpse)));
    if waiting {
        // Sent back to its lifestone: an offer it left unanswered is not taken up for it.
        crate::resurrections::declined(s, world);
        resurrect(s, net, world, clock);
    }
}

/// A dead player's client asks to be resurrected (only once its corpse is made): what its death
/// ended taken off, protected (see `effects`); for everyone who sees it, set standing at peace at once (2017: the mode set 223 of 227
/// resurrections, played 0), the resurrection's effect and, into portal space, the arrival's (219 of
/// 227, in that order 216); a second and a half later it is back at the
/// lifestone it is bound to (or, never bound, the nearest lifestone the world has, else where it
/// fell), with 5 vitae less and three quarters of its health and vigor.
pub fn resurrect<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, clock: Clock) {
    let Some(p) = s.players.get(&world) else {
        return;
    };
    if !p.dying.is_some_and(|d| d.corpse.is_some()) || p.trip.is_some() {
        s.unhandled
            .note("a resurrection asked for by a player not dead");
        return;
    }
    if let Some(offer) = s.players.get_mut(&world).and_then(|p| p.offer.take()) {
        crate::resurrections::rises(s, net, world, offer, clock);
        return;
    }
    let to = crate::places::resurrection_place(s, world);
    // What a death ends goes first, then it is protected for three minutes, before portal space.
    let stop = crate::effects::resurrected(s, net, world);
    crate::effects::protect(s, net, world, clock);
    resurrection_look(s, net, world, &stop);
    crate::places::resurrection_trip(s, net, world, to, clock);
}

/// For everyone who sees it, a player coming back is set standing at peace and the appearances of
/// what its death ended stop.
pub(crate) fn stands_at_peace<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    stop: &[u32],
) {
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let id = p.character;
    let peace = ServerMessage::SetMode(ObjectEvent {
        sender: player_stamp(p),
        action: auberean_protocol::physics::Word(crate::combat::mode::PEACE),
    });
    to_all_seeing(s, net, id, &peace);
    crate::effects::stop_fx(s, net, world, stop);
}

/// For everyone who sees it, a player coming back is set standing at peace, the appearances of what
/// its death ended stop, and it glows (2017: in that order, 172 of 227 requests).
fn resurrection_look<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    stop: &[u32],
) {
    stands_at_peace(s, net, world, stop);
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let id = p.character;
    let fx = ServerMessage::DoFx(ObjectEvent {
        sender: player_stamp(p),
        action: auberean_protocol::physics::Fx {
            fx: player_look::RESURRECTING,
            scalar: 1.0,
        },
    });
    to_all_seeing(s, net, id, &fx);
}

/// A character that left the world dead, entered where it returns to and at no health, comes back
/// where it stands: what its death ends taken off, protected, its vitae, health and vigor, alive as
/// far as its client knows, standing, glowing, playing its return. Its client was never told it was
/// dead, so no box is up and it asks for nothing. Our choice, on one recording: the one such entry
/// of 434, of a character that died during its own log-off, 1.43 s after its description and at the
/// place of its two resurrections before. Logging off with the box up, a client gone while dead and
/// a restart while dead are unrecorded, and take the same way. It is kept at once, so a server that
/// stops now does not take its vitae again.
pub fn back_after_entry<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    clock: Clock,
) {
    let stop = crate::effects::resurrected(s, net, world);
    crate::effects::protect(s, net, world, clock);
    returns(s, net, world);
    resurrection_look(s, net, world, &stop);
    returned(s, net, world);
    crate::world::keep_player(s, world);
}

/// A share of a player's vitae is taken (a person's tap of it): its vitae is that share of what it
/// was, and the experience still to earn to raise it worked out again, both told (2017: 81 to 72.9
/// by a tap leaving nine tenths, with 80,660 to earn, 1 of 1).
pub fn tap_vitae<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, share: f32) {
    let costs = s.vitae_costs.clone();
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let level = crate::examine::level_of(&p.qualities);
    let was = quals::float(&p.qualities, vitae_q::VITAE).unwrap_or(100.0);
    let vitae = was * share;
    quals::set_float(&mut p.qualities, vitae_q::VITAE, vitae);
    progress::send_float(net, world, vitae_q::VITAE, vitae);
    if let Some(x) = xp_to_raise(&costs, level, vitae) {
        quals::set_long(&mut p.qualities, vitae_q::XP_TO_RAISE, x);
        progress::send_long(net, world, vitae_q::XP_TO_RAISE, x);
    }
    crate::world::keep_player(s, world);
}

/// Out of portal space after a resurrection, before the player is placed: its vitae, health and
/// vigor, and alive as far as its client knows.
pub fn returns<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId) {
    returns_keeping(s, net, world, 0.0);
}

/// As [`returns`], keeping this share of its greatest focus above what it would come back with.
pub(crate) fn returns_keeping<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    kept_focus: f32,
) {
    let costs = s.vitae_costs.clone();
    let most_focus = s.players.get(&world).map_or(0, |p| {
        crate::focus::back_with(crate::focus::greatest(s, p), kept_focus)
    });
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let level = crate::examine::level_of(&p.qualities);
    let was = quals::float(&p.qualities, vitae_q::VITAE).unwrap_or(100.0);
    let vitae = (was - VITAE_LOST).max(min_vitae(level)).min(100.0);
    if (vitae - was).abs() > f32::EPSILON {
        quals::set_float(&mut p.qualities, vitae_q::VITAE, vitae);
        progress::send_float(net, world, vitae_q::VITAE, vitae);
        if let Some(x) = xp_to_raise(&costs, level, vitae) {
            quals::set_long(&mut p.qualities, vitae_q::XP_TO_RAISE, x);
            progress::send_long(net, world, vitae_q::XP_TO_RAISE, x);
        }
    }
    let share = vitae / 100.0;
    let max_health = quals::int(&p.qualities, crate::player::quality::HEALTH_MAX).unwrap_or(100);
    let max_vigor = quals::int(&p.qualities, crate::player::quality::VIGOR_MAX).unwrap_or(100);
    #[allow(clippy::cast_precision_loss)]
    let back =
        |max: i32| dereth_primitives::num::to_i32((max as f32 * share).floor() * RETURNS_WITH);
    let (health, vigor) = (back(max_health), back(max_vigor));
    quals::set_int(&mut p.qualities, crate::player::quality::HEALTH, health);
    quals::set_int(&mut p.qualities, crate::player::quality::VIGOR, vigor);
    progress::send_int(net, world, crate::player::quality::HEALTH, health);
    progress::send_int(net, world, crate::player::quality::VIGOR, vigor);
    crate::focus::resurrected(p, net, world, most_focus);
    progress::private(
        net,
        world,
        &session::call(player_fn::DEATH_STATE, &[Arg::Int(0)]).expect("packs"),
    );
}

/// Placed after a resurrection: for everyone who sees it, the player is alive, wounded as its
/// health shows, selectable, playing its return.
pub fn returned<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId) {
    returned_with(s, net, world, None);
}

/// As [`returned`], with this look just before its return plays.
pub(crate) fn returned_with<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    look: Option<u32>,
) {
    s.next_context = s.next_context.wrapping_add(1);
    let context = s.next_context;
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    p.dying = None;
    let id = p.character;
    let max = quals::int(&p.qualities, crate::player::quality::HEALTH_MAX)
        .unwrap_or(100)
        .max(1);
    let health = quals::int(&p.qualities, crate::player::quality::HEALTH).unwrap_or(max);
    #[allow(clippy::cast_precision_loss)]
    let hurt = 1.0 - health as f32 / max as f32;
    let alive = ServerMessage::UpdateBoolVisual(VisualQuality {
        object: player_stamp(p),
        quality: q::DEAD,
        value: 0,
    });
    let wounded = ServerMessage::DoFx(ObjectEvent {
        sender: player_stamp(p),
        action: auberean_protocol::physics::Fx {
            fx: look::WOUNDED,
            scalar: hurt,
        },
    });
    let selectable = ServerMessage::UpdateBoolVisual(VisualQuality {
        object: player_stamp(p),
        quality: q::SELECTABLE,
        value: 1,
    });
    let mut b = BehaviorParams {
        bits: BehaviorParams::BEHAVIOR | player_look::REVIVE_FLAG | BehaviorParams::CONTEXT,
        behavior: player_look::REVIVE,
        context,
        ..BehaviorParams::default()
    };
    b.words[5] = 1;
    let revive = ServerMessage::PlayBehavior(ObjectEvent {
        sender: player_stamp(p),
        action: b,
    });
    for m in [alive, wounded, selectable] {
        to_all_seeing(s, net, id, &m);
    }
    if let (Some(fx), Some(p)) = (look, s.players.get_mut(&world)) {
        let m = ServerMessage::DoFx(ObjectEvent {
            sender: player_stamp(p),
            action: auberean_protocol::physics::Fx { fx, scalar: 1.0 },
        });
        to_all_seeing(s, net, id, &m);
    }
    to_all_seeing(s, net, id, &revive);
}
