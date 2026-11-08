//! Kingdom skills: what a use of one's own kingdom's faction shrine grants, by the player's kingdom
//! points.
//!
//! **The table** is the install's: each kingdom's change of kingdom (an effect of the library's kind
//! of change of kingdom, one for each kingdom it changes to) lists the skills it bestows, each with
//! the least kingdom points that earn it (2017's install: fifteen for each of the three kingdoms,
//! from 1,000 to 6,000 points; none for the neutral kingdom). Each listed effect names the skill it
//! trains in its own code, as the singleton it hands its training.
//!
//! **A use of one's own kingdom's shrine** grants every skill of its kingdom's table at or under
//! the player's points that it does not hold, as trained skills that cost nothing; the player is
//! told each skill's information and then "The shrine updates your Kingdom Skills!". A use that
//! grants nothing tells nothing more.
//!
//! **Kingdom points** are the player's rating (an integer quality, 500 while unset, kept with the
//! character). An effect the world's rows name as a points bonus raises them when it is put on a
//! player (a give's or a quest step's effect, say): by the row's amount, never above the row's cap,
//! and never lowering points already above it. The skills its own kingdom's shrine would grant at
//! the points it then holds are granted at once, as a use of the shrine grants them. A give that
//! gives nothing and whose only effects are such bonuses, every one at its cap already, takes
//! nothing: the item stays with the player.
//!
//! Granted skills are kept when the points fall or the player changes kingdom: nothing recorded
//! says what took them away.

use std::collections::BTreeMap;

use auberean_script::graph::Graph;
use auberean_script::library::Library;
use auberean_script::opcode::{length, low_byte};
use yalain_net::{ConnId, ServerNet};
use yalain_store::Store;

use crate::skills::{self, Held, SkillTable};
use crate::start::Nav;
use crate::{progress, quals, Server};

/// A player's kingdom points: an integer quality, and what it holds while unset.
pub const POINTS: u32 = 552;
pub const POINTS_UNSET: i32 = 500;

/// "The shrine updates your Kingdom Skills!", and the kind of text it is shown as.
pub const UPDATED: (u32, u32) = (0x0159_2B1B, 0x2500_000F);
pub const SHOWN_AS: u32 = 8;

/// The library's kind of a change of kingdom, and its fields: the kingdom it changes to, and the
/// effects it bestows, each with the least points (an entry's effect and its points).
const CHANGE: &str = "FactionChangeEffect";
const TO_KINGDOM: &str = "m_newFactionType";
const BESTOWS: &str = "m_listFactionEffects";
const ENTRY_EFFECT: &str = "m_eff";
const ENTRY_POINTS: &str = "m_rating";

/// The library's kind of an effect that trains skills, and its calls that name a skill to train.
const TRAINING: &str = "SkillTrainingEffect";
const TRAINS: [&str; 2] = ["AddTemporarySkillToTrain", "AddPermanentSkillToTrain"];
/// The call that names a singleton: the skill trained is the one named just before.
const SINGLETON: &str = "GetInstance";

/// The saved state of a package's singleton.
const SINGLETON_STATE: u32 = 0x7000_0000;

/// Each kingdom's skills, in its table's order: the least points and the skill.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KingdomSkills {
    pub by_kingdom: BTreeMap<u32, Vec<(u32, u32)>>,
}

impl KingdomSkills {
    /// Read from the library and the install's saved states (`graph`): every change of kingdom, its
    /// kingdom and the skills its bestowed effects train (each effect's package's code naming the
    /// package of the skill's definition, which `skills` holds). An entry whose skill is not found is left out.
    #[must_use]
    pub fn read(lib: &Library, graph: &dyn Fn(u32) -> Option<Graph>, skills: &SkillTable) -> Self {
        let mut out = Self::default();
        let Some(base) = lib.package_id(CHANGE) else {
            return out;
        };
        for p in &lib.packages {
            if p.id == base || !lib.derives_from(p.id, base) {
                continue;
            }
            let Some(g) = graph(SINGLETON_STATE | p.id) else {
                continue;
            };
            let nav = Nav::new(lib, &g);
            let Some(root) = g.root().and_then(|r| nav.object(r).ok()) else {
                continue;
            };
            let Ok(kingdom) = nav.field(root, TO_KINGDOM) else {
                continue;
            };
            let mut table = Vec::new();
            for entry in nav.list(root, BESTOWS).unwrap_or_default() {
                let (Ok(effect), Ok(least)) = (
                    nav.field(entry, ENTRY_EFFECT),
                    nav.field(entry, ENTRY_POINTS),
                ) else {
                    continue;
                };
                let Some(state) = nav.singleton(effect) else {
                    continue;
                };
                for trained in trained_by(lib, state & !SINGLETON_STATE) {
                    if let Some(skill) = skills.by_package.get(&trained) {
                        table.push((least, *skill));
                    }
                }
            }
            if !table.is_empty() {
                out.by_kingdom.insert(kingdom, table);
            }
        }
        out
    }

    /// The skills of a kingdom's table at or under these points, in its order.
    #[must_use]
    pub fn earned(&self, kingdom: u32, points: i32) -> Vec<u32> {
        self.by_kingdom
            .get(&kingdom)
            .into_iter()
            .flatten()
            .filter(|(least, _)| i64::from(*least) <= i64::from(points))
            .map(|(_, skill)| *skill)
            .collect()
    }
}

/// The packages of the skills an effect's package trains: in its constructor, each call that hands
/// a skill to train takes the singleton named by the call just before it.
fn trained_by(lib: &Library, package: u32) -> Vec<u32> {
    let Some(f) = lib
        .package(package)
        .and_then(|p| p.functions.iter().find(|f| f.name == "constructor"))
    else {
        return Vec::new();
    };
    let Some(offset) = f.offset else {
        return Vec::new();
    };
    let start = (offset / 4) as usize;
    let end = (start + (f.size / 4) as usize).min(lib.code.len());
    let trains: Vec<String> = TRAINS.iter().map(|t| format!("{TRAINING}::{t}")).collect();
    let mut named: Option<u32> = None;
    let mut out = Vec::new();
    let mut at = start;
    while at < end {
        if matches!(low_byte(lib.code[at]), 0x0B | 0x4F | 0x51 | 0x53) {
            let id = lib.code.get(at + 1).copied().unwrap_or(0);
            let name = lib
                .function_name(auberean_script::value::FunctionId(id))
                .unwrap_or_default();
            if name.ends_with(&format!("::{SINGLETON}")) {
                named = Some((id & 0x7FFF_FFFF) >> 16);
            } else if trains.contains(&name) {
                out.extend(named.take());
            }
        }
        at += length(&lib.code, at).unwrap_or(1);
    }
    out
}

/// A player's kingdom points.
#[must_use]
pub fn points(qualities: &[u8]) -> i32 {
    quals::int(qualities, POINTS).unwrap_or(POINTS_UNSET)
}

/// A player used its own kingdom's shrine: every skill its points earn that it does not hold is
/// granted, its information told, then the shrine's line; nothing when none is.
pub fn shrine_used<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, kingdom: u32) {
    let Some(p) = s.players.get(&world) else {
        return;
    };
    let earned = s.kingdom_skills.earned(kingdom, points(&p.qualities));
    if earned.is_empty() {
        return;
    }
    let starting = s.start.skills.get(&p.race).cloned().unwrap_or_default();
    let mut held = skills::held(&starting, &p.play.skills);
    let mut told: Vec<Held> = Vec::new();
    for skill in earned {
        if let Some((new, changed)) = skills::bestowed(&s.skill_table, &mut held, skill) {
            for h in changed {
                told.retain(|t| t.skill != h.skill);
                told.push(h);
            }
            told.push(new);
        }
    }
    if told.is_empty() {
        return;
    }
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    p.play.skills = skills::kept(&starting, &held);
    for h in &told {
        progress::private(net, world, &skills::info_update(h, None));
    }
    progress::private(
        net,
        world,
        &crate::chat::show_text(SHOWN_AS, progress::text(UPDATED)),
    );
    crate::world::keep_player(s, world);
}

/// An effect was put on a player: when the world's rows name it as a points bonus, the player's
/// kingdom points are raised by it, kept, and the kingdom skills its own kingdom's table grants at
/// them are granted as at its shrine. Returns whether the rows name the effect.
pub fn points_bonus<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    state: u32,
) -> bool {
    let Some(bonus) = s.kingdom_rows.points.get(&state).copied() else {
        return false;
    };
    let Some(p) = s.players.get_mut(&world) else {
        return true;
    };
    let held = points(&p.qualities);
    let raised = bonus.raised(held);
    if raised != held {
        quals::set_int(&mut p.qualities, POINTS, raised);
        crate::world::keep_player(s, world);
    }
    let kingdom = s
        .players
        .get(&world)
        .map(|p| crate::kingdoms::counted_of(&p.qualities));
    if let Some(k) = kingdom {
        shrine_used(s, net, world, k);
    }
    true
}

/// Would a give do nothing for a player of these qualities: it gives nothing, and it has effects, each
/// a points bonus its kingdom points are already at or above the cap of. Such a give takes nothing.
#[must_use]
pub fn would_do_nothing<S: Store>(
    s: &Server<S>,
    qualities: &[u8],
    give: &yalain_content::quests::Give,
) -> bool {
    let held = points(qualities);
    give.gives.is_empty()
        && !give.effects.is_empty()
        && give.effects.iter().all(|e| {
            s.kingdom_rows
                .points
                .get(e)
                .is_some_and(|b| b.raised(held) == held)
        })
}
