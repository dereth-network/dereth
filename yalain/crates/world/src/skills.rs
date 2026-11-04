//! Skills: the skill repository the client keeps (credits, the skills held), and training and
//! raising skills by the client's own rules.
//!
//! **The rules** are the script library's shared skill code, run here on each skill's saved
//! definition (its cost in credits, the level it may be trained from, its parents and
//! prerequisites, the races it is for, its cap and its advancement table):
//!
//! - **Training** a skill is refused, first failure first, when it is trained already, may not be
//!   trained by a player, the player's level is below its minimum, it is not for the player's race,
//!   it is a hero skill, a parent is untrained, a prerequisite is untrained or below its level, or the
//!   credits are short of its cost. Trained, the player pays its cost in credits; each trained
//!   parent counts one more trained child and each prerequisite one more dependent; the skill holds
//!   no experience, and is marked trained (and the player's to untrain when the skill may be).
//! - **A skill's rating** is its granted level, plus a tenth of its trained parents' ratings (each
//!   at most 50), plus the level its experience reaches in its advancement table (the highest level
//!   whose experience it has: none still reaches level 1), at most its cap (for an active skill of
//!   a player who is no hero, at most 50 as well: 2017, 1,268 records above 50, all of heroes, 0 of
//!   16,397 others').
//! - **Raising** a skill costs the experience from what it holds to its table's next level (none at
//!   the table's top). It is refused when the skill cannot be raised, is untrained, is at its cap,
//!   or the player's available experience is short of the cost; raised, the experience moves from
//!   the player to the skill.
//!
//! **The answers** are 2017's (79 trainings and 1,602 raises checked against the costs):
//! training, the parents' and prerequisites' updated information, the repository with the credits
//! left, the skill's information and "You train %1."; raising, the skill's information (its rating
//! before the raise cached, with the time), the available experience and "Your rating in %1 is now
//! %2."; a refusal, its text alone.

use std::collections::HashMap;

use auberean_protocol::ServerMessage;
use auberean_script::args::Arg;
use auberean_script::graph::{Graph, PackedObject};
use auberean_script::library::{Elem, Library};
use auberean_script::native_object::{Item, NativeObject, TextInfo};
use yalain_net::{ConnId, ServerNet};
use yalain_store::Store;

use crate::calls::Value;
use crate::graph::{hash, Builder, Slot};
use crate::progress::{self, nested, text_with, word};
use crate::session::{self, native, package, TABLE_SIZE};
use crate::start::Nav;
use crate::state::SkillState;
use crate::{quals, Clock, Server};

/// The client functions skills answer with.
pub mod function {
    /// The client's requests: train a skill, raise one.
    pub const TRAIN: u32 = 0x8214_0002;
    pub const RAISE: u32 = 0x8214_000C;
    /// One skill's information.
    pub const UPDATE_INFO: u32 = 0x8214_000D;
    /// The repository itself (its credits), without its skills.
    pub const UPDATE_REPOSITORY: u32 = 0x8214_0011;
    /// A skill forgotten.
    pub const REMOVE_INFO: u32 = 0x8214_0014;
    /// The client's requests to untrain a skill, and to cancel an untraining under way.
    pub const UNTRAIN: u32 = 0x8214_0007;
    pub const UNTRAIN_CANCEL: u32 = 0x8214_0001;
}

/// The untraining texts (in [`TEXTS`]): untrained, with the credits and experience regained;
/// untraining under way, with the experience still to earn; untraining cancelled.
pub mod untraining {
    pub const UNTRAINED: u32 = 0x480E_61DA;
    pub const UNDER_WAY: u32 = 0x48D3_7AD8;
    pub const CANCELLED: u32 = 0x7654_1B62;
    pub const VAR_CREDITS: u32 = 0xB263_AC49;
    pub const VAR_XP: u32 = 0xB140_0000;
    /// The kind of text each is shown as.
    pub const SHOWN_AS: u32 = 8;
    pub const CANCELLED_SHOWN_AS: u32 = 1;
    /// The quality keeping when the player last untrained a skill.
    pub const LAST_UNTRAIN: u32 = 304;
}

/// A skill being untrained: the skill, the experience the player must earn to finish it (the
/// skill's own), and the player's total experience when it began.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Untraining {
    pub skill: u32,
    pub xp: u64,
    pub total_at_start: u64,
}

/// Why a training or a raise is refused (the library's codes).
pub mod refusal {
    pub const TRAINED: u32 = 30_000;
    pub const LEVEL: u32 = 30_001;
    pub const RACE: u32 = 30_002;
    pub const PARENT: u32 = 30_003;
    pub const PREREQUISITE: u32 = 30_004;
    pub const CREDITS: u32 = 30_006;
    pub const UNTRAINED: u32 = 30_009;
    pub const CAP: u32 = 30_011;
    pub const EXPERIENCE: u32 = 30_012;
    pub const CANNOT_RAISE: u32 = 30_037;
    pub const NOT_BY_PLAYER: u32 = 30_040;
    pub const HERO: u32 = 30_047;
}

/// The texts of skills, in their table.
pub const TEXTS: u32 = 0x2500_04AA;
pub mod text {
    pub const TRAINED_IT: u32 = 0x5B66_1385;
    pub const RAISED_IT: u32 = 0x903D_FFF9;
}
/// The variables of those texts: the skill's name, its new rating.
pub const VAR_SKILL: u32 = 0x272E_4CC9;
pub const VAR_RATING: u32 = 0x1916_B459;

/// A refusal's text: 2017's for the three it showed (already trained, short of experience, at the
/// cap), the rest the table's lines that say the same (never seen in 2017).
#[must_use]
pub const fn refusal_text(code: u32) -> u32 {
    match code {
        refusal::TRAINED => 0x83AA_CFBE,
        refusal::EXPERIENCE => 0xF08B_04F9,
        refusal::CAP => 0x67D4_B0E9,
        refusal::LEVEL => 0xD344_4C77,
        refusal::RACE => 0x38A0_983D,
        refusal::PARENT => 0x75B4_0A93,
        refusal::PREREQUISITE => 0xD766_7171,
        refusal::CREDITS => 0xC036_658E,
        refusal::UNTRAINED => 0xD14A_05D7,
        refusal::NOT_BY_PLAYER => 0x337D_77B9,
        _ => 0xD8E1_92AB,
    }
}

/// Skill flags.
pub mod flag {
    pub const MAY_UNTRAIN: u32 = 0x1;
    pub const CANNOT_RAISE: u32 = 0x4;
    pub const NOT_BY_PLAYER: u32 = 0x8;
    pub const HERO: u32 = 0x10;
    /// At no vigor a player rates nothing in it.
    pub const NOTHING_AT_NO_VIGOR: u32 = 0x40;
}

/// A skill information's mask.
pub mod mask {
    pub const TRAINED: u32 = 0x1;
    pub const PLAYERS_TO_UNTRAIN: u32 = 0x2;
    pub const CANNOT_RAISE: u32 = 0x10;
}

/// One of a used skill's hooks: what lands on the target when the skill's animation reaches it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Hook {
    /// A hook with its own damage numbers (the static kind), else one that plays effects only.
    pub damages: bool,
    pub flags: u32,
    /// Its damage: added, and the share of the weapon's (the static kind's).
    pub add: f32,
    pub mult: f32,
    /// Its chance to fire, and the radius it reaches around the target (0 for the target alone).
    pub chance: f32,
    pub radius: f32,
    pub anim_hook: u32,
    /// The missile it throws (its template; 0 for none) and its speed.
    pub missile: u32,
    pub missile_speed: f32,
    /// The target's behaviors and effects on a hit, a miss and a critical.
    pub target_behavior: [u32; 3],
    pub target_fx: [u32; 3],
    /// The attacker's effects on a hit, a miss and a critical.
    pub attacker_fx: [u32; 3],
    /// The damage it adds by the user's rating in the skill (the linear kind's: its points, each a
    /// rating and the damage added there); empty for a hook that adds none by rating.
    pub add_by_rating: Vec<(f32, f32)>,
}

impl Hook {
    /// The damage this hook adds at a rating in its skill: along the lines between its points, held
    /// at the ends, rounded down; the static kind's own add for a hook without points.
    #[must_use]
    pub fn added_at(&self, rating: f32) -> f32 {
        let pts = &self.add_by_rating;
        let (Some(first), Some(last)) = (pts.first(), pts.last()) else {
            return self.add;
        };
        if rating <= first.0 {
            return first.1.floor();
        }
        if rating >= last.0 {
            return last.1.floor();
        }
        let i = pts
            .iter()
            .position(|p| rating < p.0)
            .unwrap_or(pts.len() - 1);
        let ((r0, v0), (r1, v1)) = (pts[i.saturating_sub(1)], pts[i]);
        let t = (rating - r0) / (r1 - r0).max(f32::EPSILON);
        (v0 + t * (v1 - v0)).floor()
    }

    /// What this hook adds to its user's hit at a rating: by its points; nothing for a hook without
    /// (2017: the plain hits of 16 skills with points above the same player's unrated basic hits
    /// on the same target within 6 s: 168 for 158 at a rating of 30, n 149; 143 for 140 at 35, n
    /// 117; 160 for 160 at 40, n 45).
    #[must_use]
    pub fn rated_add(&self, rating: f32) -> i32 {
        if self.add_by_rating.is_empty() {
            0
        } else {
            dereth_primitives::num::to_i32(self.added_at(rating))
        }
    }
}

/// How a skill that is used works: its reach, its waits, its vigor, the implements it is used
/// with, what it plays and what it does to its target.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Usage {
    /// The distances it is used between, in meters (-1: none).
    pub min_range: f32,
    pub max_range: f32,
    /// How long after a use it may be used again, and how long its own timer runs (a recall's),
    /// in seconds; the least time a use takes, and how long it powers up first.
    pub recovery: f32,
    pub reset: f32,
    pub min_use: f32,
    pub powerup: f32,
    pub powerup_behavior: u32,
    /// Its vigor: added to the cost, the share of the greatest vigor, the least the user needs.
    pub vigor_add: f32,
    pub vigor_mult: f32,
    pub min_vigor: f32,
    /// The implements it is used with, in the right hand and the left (bits of implement kinds;
    /// negative: any).
    pub implements_right: i32,
    pub implements_left: i32,
    /// The kinds of target it takes.
    pub valid_targets: u32,
    /// What the user plays on another: a hit, a miss, a critical; and on itself: a hit, a miss.
    pub behavior_other: [u32; 3],
    pub behavior_self: [u32; 2],
    /// The effects it puts on its target and on its user (their saved states), and the effects
    /// its user must carry to use it (a combination's earlier skill leaves one).
    pub target_effects: Vec<u32>,
    pub user_effects: Vec<u32>,
    pub required_effects: Vec<u32>,
    /// The effects whose carrier may not use it.
    pub barring_effects: Vec<u32>,
    /// The skills its user must have trained to use it, each at the rating named.
    pub use_prereqs: Vec<(u32, u32)>,
    pub hooks: Vec<Hook>,
    pub ui_flags: u32,
    /// How much faster its definition says it goes, for the few that say.
    pub combat_speed: Option<f32>,
    /// Its use bars it for a while after (an effect the use leaves on its user blocks the skill):
    /// a creature's special waits 40 s before it is used again.
    pub barred_after: bool,
}

/// A skill's definition.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SkillDef {
    pub id: u32,
    /// Its definition's saved state.
    pub state: u32,
    pub name: (u32, u32),
    /// Its icon (a picture); a summoning skill's pets show it.
    pub icon: u32,
    pub cost: u32,
    pub hero_cost: u32,
    pub min_level: u32,
    pub cap: u32,
    pub table: u32,
    pub flags: u32,
    /// Its parents and prerequisites, each with the rating it needs.
    pub parents: Vec<(u32, u32)>,
    pub prereqs: Vec<(u32, u32)>,
    /// The races it is for (0: all).
    pub species: u32,
    /// An active skill (one used), capped at 50 for players who are no heroes.
    pub active: bool,
    /// One of the basic attacks a run swings (it has weights to be drawn by).
    pub auto_attack: bool,
    /// How it is used, for a skill that is.
    pub usage: Option<Usage>,
    /// The kind of attack it is (melee, missile, magic), by its name's family; none for a skill
    /// that is no attack of a kind.
    pub kind: Option<crate::rules::Kind>,
    /// A skill a creature uses on itself or its friends (a heal, a buff), by its name.
    pub helps: bool,
    /// The attribute it rates (0: none; [`ARCANE_LORE_CATEGORY`] Arcane Lore), and its measure of
    /// it: the attribute is its rating times that.
    pub category: u32,
    pub attribute_mod: f32,
}

/// The attribute category of the skills that rate a player's Arcane Lore.
pub const ARCANE_LORE_CATEGORY: u32 = 5;

/// Every skill's definition, and the advancement tables (the experience each level needs).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SkillTable {
    pub skills: HashMap<u32, SkillDef>,
    pub tables: HashMap<u32, Vec<u64>>,
}

impl SkillTable {
    /// Every skill's definition (each skill's saved state, found by its name in the skills'
    /// enumeration: the package of that name, its state `0x70000000` + the package) and the
    /// advancement tables they name. A skill whose state does not read is left out.
    #[must_use]
    pub fn read(
        lib: &Library,
        names: &[(u32, String)],
        graph: &dyn Fn(u32) -> Option<Graph>,
    ) -> Self {
        let mut t = Self::default();
        for (skill, name) in names {
            let Some(g) = lib
                .package_id(name)
                .and_then(|pkg| graph(0x7000_0000 | pkg))
            else {
                continue;
            };
            if let Ok(mut d) = SkillDef::from_state(*skill, lib, &g) {
                d.kind = crate::rules::kind_of_name(name);
                d.helps = crate::rules::helps_by_name(name);
                t.skills.insert(*skill, d);
            }
        }
        let tables: std::collections::BTreeSet<u32> = t.skills.values().map(|d| d.table).collect();
        for id in tables {
            if let Some(rows) = graph(id).and_then(|g| table_from_state(lib, &g).ok()) {
                t.tables.insert(id, rows);
            }
        }
        t
    }
}

/// The skills' names (an enumeration: a skill's value and its definition's package name).
pub const SKILL_NAMES: u32 = 0x2300_000F;
/// A reference to nothing.
const NULL: u32 = u32::MAX;

/// The definition's field naming its advancement table.
const ADVANCEMENT_FIELD: &str = concat!("mdidAdv", "Table");

/// The package skills that are used derive from.
pub const ACTIVE_SKILL: &str = "ActiveSkill";

impl SkillDef {
    /// Read a skill's definition from its saved state's graph.
    ///
    /// # Errors
    /// A field the definition should hold is missing.
    pub fn from_state(id: u32, lib: &Library, g: &Graph) -> Result<Self, String> {
        let nav = Nav::new(lib, g);
        let root = nav.object(g.root().ok_or("an empty graph")?)?;
        let pairs = |name: &str| -> Result<Vec<(u32, u32)>, String> {
            let r = nav.field(root, name)?;
            if r == 0 || r == NULL {
                return Ok(Vec::new());
            }
            nav.entries(r)?
                .into_iter()
                .map(|(k, v)| Ok((Nav::word(k)?, Nav::word(v)?)))
                .collect()
        };
        let name = match nav
            .field(root, "mName")
            .ok()
            .filter(|r| *r != 0 && *r != NULL)
        {
            Some(r) => match nav.native(r)? {
                NativeObject::TextInfo(t) => (t.string_id, t.table),
                _ => (0, 0),
            },
            None => (0, 0),
        };
        let active = lib
            .package_id(ACTIVE_SKILL)
            .is_some_and(|a| lib.derives_from(root.package_id(), a));
        Ok(Self {
            id,
            state: root.state,
            name,
            icon: nav.field(root, "mIcon").unwrap_or(0),
            cost: nav.field(root, "mCost")?,
            hero_cost: nav.field(root, "mHeroCost").unwrap_or(0),
            min_level: nav.field(root, "mMinCharLevel")?,
            cap: nav.field(root, "mAdvCap")?,
            table: nav.field(root, ADVANCEMENT_FIELD)?,
            flags: nav.field(root, "m_skillFlags")?,
            parents: pairs("mParents")?,
            prereqs: pairs("mPrereqs")?,
            species: nav.field(root, "mAllowedSpecies").unwrap_or(0),
            active,
            auto_attack: active
                && nav
                    .field(root, "m_WeightHash")
                    .ok()
                    .filter(|r| *r != 0 && *r != NULL)
                    .and_then(|r| nav.entries(r).ok())
                    .is_some_and(|e| !e.is_empty()),
            usage: if active {
                Usage::from_object(&nav, root).ok()
            } else {
                None
            },
            kind: None,
            helps: false,
            category: nav.field(root, "m_category").unwrap_or(0),
            attribute_mod: nav.number(root, "m_attributeMod").unwrap_or(1.0),
        })
    }
}

/// The skills a definition names to be trained before it is used, with their ratings.
fn use_prereqs(nav: &Nav<'_>, o: &PackedObject) -> Vec<(u32, u32)> {
    nav.field(o, "m_UsePrereqSkills")
        .ok()
        .filter(|r| *r != 0 && *r != NULL)
        .and_then(|r| nav.entries(r).ok())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(k, v)| Some((Nav::word(k).ok()?, Nav::word(v).ok()?)))
        .collect()
}

impl Usage {
    /// Read how a skill is used from its definition's object.
    ///
    /// # Errors
    /// A field every used skill holds is missing.
    pub fn from_object(nav: &Nav<'_>, o: &PackedObject) -> Result<Self, String> {
        let num = |name: &str| nav.number(o, name);
        let word = |name: &str| nav.field(o, name);
        let hooks = nav
            .list(o, "m_Hooks")?
            .into_iter()
            .map(|h| Hook::from_object(nav, h))
            .collect::<Result<_, _>>()?;
        let target_effects = nav.states(o, "m_TargetEffects")?;
        let user_effects = nav.states(o, "m_UserEffects").unwrap_or_default();
        let required_effects = nav.states(o, "m_ReqEffects").unwrap_or_default();
        Ok(Self {
            min_range: num("m_MinRange")?,
            max_range: num("m_MaxRange")?,
            recovery: num("m_RecoveryTime")?,
            reset: num("m_ResetTime")?,
            min_use: num("m_MinUseTime")?,
            powerup: num("m_PowerupTime")?,
            powerup_behavior: word("m_powerUpBehavior")?,
            vigor_add: num("m_VigorModAdd")?,
            vigor_mult: num("m_VigorModMult")?,
            min_vigor: num("m_MinVigor")?,
            implements_right: word("m_AllowedImplementsRight")?.cast_signed(),
            implements_left: word("m_AllowedImplementsLeft")?.cast_signed(),
            valid_targets: word("m_validTargets")?,
            behavior_other: [
                word("m_BehaviorOtherHit")?,
                word("m_BehaviorOtherMiss")?,
                word("m_BehaviorOtherCrit")?,
            ],
            behavior_self: [word("m_BehaviorSelfHit")?, word("m_BehaviorSelfMiss")?],
            target_effects,
            user_effects,
            required_effects,
            barring_effects: nav.states(o, "m_BarringEffects").unwrap_or_default(),
            use_prereqs: use_prereqs(nav, o),
            hooks,
            ui_flags: word("m_uiFlags")?,
            combat_speed: num("m_fCombatSpeedModifier").ok().filter(|m| *m != 1.0),
            barred_after: !nav
                .states(o, "m_BarringEffects")
                .unwrap_or_default()
                .is_empty(),
        })
    }
}

impl Hook {
    /// Read one of a skill's hooks.
    ///
    /// # Errors
    /// A field every hook holds is missing.
    pub fn from_object(nav: &Nav<'_>, o: &PackedObject) -> Result<Self, String> {
        let num = |name: &str| nav.number(o, name);
        let word = |name: &str| nav.field(o, name);
        let damages = nav.number(o, "m_AddMod").is_ok();
        Ok(Self {
            damages,
            flags: word("m_flags")?,
            add: num("m_AddMod").unwrap_or(0.0),
            mult: num("m_MultMod").unwrap_or(0.0),
            chance: num("m_chanceToFire")?,
            radius: num("mAttackAreaRadius")?,
            anim_hook: word("m_animHookIndex")?,
            missile: word("m_missileEntityDID")?,
            missile_speed: num("m_missileInitialSpeed")?,
            target_behavior: [
                word("mTargetBehaviorHit")?,
                word("mTargetBehaviorMiss")?,
                word("mTargetBehaviorCrit")?,
            ],
            target_fx: [
                word("mTargetFXHit")?,
                word("mTargetFXMiss")?,
                word("mTargetFXCrit")?,
            ],
            attacker_fx: [
                word("mAttackerFXHit")?,
                word("mAttackerFXMiss")?,
                word("mAttackerFXCrit")?,
            ],
            add_by_rating: nav
                .list(o, "m_addDmgData")
                .unwrap_or_default()
                .into_iter()
                .filter_map(|d| {
                    Some((
                        nav.number(d, "m_level").ok()?,
                        nav.number(d, "m_value").ok()?,
                    ))
                })
                .collect(),
        })
    }
}

/// Read an advancement table's graph: the experience of each level, from 0.
///
/// # Errors
/// The table's list is missing.
pub fn table_from_state(lib: &Library, g: &Graph) -> Result<Vec<u64>, String> {
    let nav = Nav::new(lib, g);
    let root = nav.object(g.root().ok_or("an empty graph")?)?;
    nav.items(nav.field(root, "mMap")?)?
        .into_iter()
        .map(|i| match i {
            Item::Long(v) => Ok(v),
            Item::Word(v) => Ok(u64::from(v)),
            other => Err(format!("{other:?} in an advancement table")),
        })
        .collect()
}

/// The level an amount of experience reaches in an advancement table: the highest whose
/// experience it has (0 when even the first is more).
#[must_use]
pub fn table_level(table: &[u64], xp: u64) -> u32 {
    let n = table.iter().take_while(|v| **v <= xp).count();
    u32::try_from(n.saturating_sub(1)).unwrap_or(0)
}

/// The experience a raise costs from what a skill holds: to its table's next level, none at the
/// top.
#[must_use]
pub fn raise_cost(table: &[u64], xp: u64) -> u64 {
    let l = table_level(table, xp) as usize;
    table.get(l + 1).map_or(0, |next| next.saturating_sub(xp))
}

/// A skill a player holds: its definition's id, its granted level, its experience, its mask, its
/// cost when learned, its trained children and dependents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Held {
    pub skill: u32,
    pub granted: u32,
    pub xp: u64,
    pub mask: u32,
    pub cost: u32,
    pub children: u32,
    pub dependents: u32,
}

impl From<SkillState> for Held {
    fn from(s: SkillState) -> Self {
        Self {
            skill: s.skill,
            granted: s.granted,
            xp: s.spent,
            mask: s.mask,
            cost: s.cost,
            children: s.children,
            dependents: s.dependents,
        }
    }
}

impl From<Held> for SkillState {
    fn from(h: Held) -> Self {
        Self {
            skill: h.skill,
            granted: h.granted,
            mask: h.mask,
            cost: h.cost,
            children: h.children,
            dependents: h.dependents,
            spent: h.xp,
        }
    }
}

/// A player's skills: its race's starting skills (trained, at their granted levels), each as its
/// play has changed it, and the skills it has trained since.
#[must_use]
pub fn held(starting: &[(u32, u32)], kept: &[SkillState]) -> Vec<Held> {
    let mut out: Vec<Held> = starting
        .iter()
        .map(|(skill, granted)| Held {
            skill: *skill,
            granted: *granted,
            xp: 0,
            mask: mask::TRAINED,
            cost: 0,
            children: 0,
            dependents: 0,
        })
        .collect();
    for k in kept {
        match out.iter_mut().find(|h| h.skill == k.skill) {
            Some(h) => *h = Held::from(*k),
            None => out.push(Held::from(*k)),
        }
    }
    out
}

impl SkillDef {
    /// The most a player's rating in this skill may be: its cap, and for an active skill of a player
    /// who is no hero, at most 50.
    #[must_use]
    pub fn cap_for(&self, hero: bool) -> u32 {
        if self.active && !hero {
            self.cap.min(50)
        } else {
            self.cap
        }
    }
}

impl SkillTable {
    /// The skills a player at no vigor still rates in: those that do not say otherwise. What it
    /// rates nothing in adds nothing to the skills it is a parent of.
    #[must_use]
    pub fn at_no_vigor(&self, mut skills: Vec<Held>) -> Vec<Held> {
        skills.retain(|h| {
            self.skills
                .get(&h.skill)
                .is_none_or(|d| d.flags & flag::NOTHING_AT_NO_VIGOR == 0)
        });
        skills
    }

    /// A held skill's rating (see the module's rules), for a hero or a player who is none.
    #[must_use]
    pub fn rating(&self, skills: &[Held], skill: u32, hero: bool) -> u32 {
        self.rating_at(skills, skill, hero, 0, &|_, raw| raw)
    }

    /// A held skill's rating with what its holder's effects make of each skill's own (`changed`,
    /// given a skill and its rating, [`crate::effects::skill_rating`]): its parents' count as
    /// changed, and the skill's cap still holds (2017: one character's skill at one experience,
    /// rated 46 under an item's 6.82 and 40 without, 21 and 15 uses).
    #[must_use]
    pub fn rating_with(
        &self,
        skills: &[Held],
        skill: u32,
        hero: bool,
        changed: &dyn Fn(u32, u32) -> u32,
    ) -> u32 {
        self.rating_at(skills, skill, hero, 0, changed)
    }

    fn rating_at(
        &self,
        skills: &[Held],
        skill: u32,
        hero: bool,
        depth: u32,
        changed: &dyn Fn(u32, u32) -> u32,
    ) -> u32 {
        let Some(h) = skills.iter().find(|h| h.skill == skill) else {
            return 0;
        };
        let Some(d) = self.skills.get(&skill) else {
            return changed(skill, h.granted);
        };
        let trained = |p: u32| {
            skills
                .iter()
                .any(|h| h.skill == p && h.mask & mask::TRAINED != 0)
        };
        let parents: u32 = if depth < 8 {
            d.parents
                .iter()
                .filter(|(p, _)| trained(*p))
                .map(|(p, _)| self.rating_at(skills, *p, hero, depth + 1, changed).min(50))
                .sum()
        } else {
            0
        };
        let level = self
            .tables
            .get(&d.table)
            .map_or(0, |t| table_level(t, h.xp));
        let bonus = parents / 10;
        let cap = d.cap_for(hero);
        changed(skill, (h.granted + bonus + level).min(cap)).min(cap)
    }

    /// Why a player may not train a skill, if it may not: a hero skill only by a hero, paid in
    /// hero skill credits (`credits.hero`, none for a player who is no hero); any other in skill
    /// credits.
    #[must_use]
    pub fn train_refusal(
        &self,
        skills: &[Held],
        skill: u32,
        level: i32,
        race: u32,
        credits: Credits,
    ) -> Option<u32> {
        let trained = |s: u32| {
            skills
                .iter()
                .any(|h| h.skill == s && h.mask & mask::TRAINED != 0)
        };
        if trained(skill) {
            return Some(refusal::TRAINED);
        }
        let Some(d) = self.skills.get(&skill) else {
            return Some(refusal::NOT_BY_PLAYER);
        };
        if d.flags & flag::NOT_BY_PLAYER != 0 {
            return Some(refusal::NOT_BY_PLAYER);
        }
        if d.min_level > 1 && i64::from(level) < i64::from(d.min_level) {
            return Some(refusal::LEVEL);
        }
        if d.species != 0 && d.species & race != race {
            return Some(refusal::RACE);
        }
        let hero_skill = d.flags & flag::HERO != 0;
        if hero_skill && credits.hero.is_none() {
            return Some(refusal::HERO);
        }
        if d.parents.iter().any(|(p, _)| !trained(*p)) {
            return Some(refusal::PARENT);
        }
        if d.prereqs
            .iter()
            .any(|(p, at)| !trained(*p) || self.rating(skills, *p, credits.hero.is_some()) < *at)
        {
            return Some(refusal::PREREQUISITE);
        }
        let short = if hero_skill {
            credits.hero.unwrap_or(0) < d.hero_cost
        } else {
            credits.skill < d.cost
        };
        if short {
            return Some(refusal::CREDITS);
        }
        None
    }

    /// The cost of raising a skill, or why a player may not.
    ///
    /// # Errors
    /// The refusal's code.
    pub fn raise(
        &self,
        skills: &[Held],
        skill: u32,
        available: u64,
        hero: bool,
    ) -> Result<u64, u32> {
        let d = self.skills.get(&skill);
        if d.is_some_and(|d| d.flags & flag::CANNOT_RAISE != 0) {
            return Err(refusal::CANNOT_RAISE);
        }
        let Some(h) = skills
            .iter()
            .find(|h| h.skill == skill && h.mask & mask::TRAINED != 0)
        else {
            return Err(refusal::UNTRAINED);
        };
        if h.mask & mask::CANNOT_RAISE != 0 {
            return Err(refusal::CANNOT_RAISE);
        }
        let Some(d) = d else {
            return Err(refusal::CANNOT_RAISE);
        };
        if self.rating(skills, skill, hero) >= d.cap_for(hero) {
            return Err(refusal::CAP);
        }
        let cost = self.tables.get(&d.table).map_or(0, |t| raise_cost(t, h.xp));
        if cost == 0 {
            return Err(refusal::CAP);
        }
        if available < cost {
            return Err(refusal::EXPERIENCE);
        }
        Ok(cost)
    }
}

/// The repository with its credits and hero skill credits and empty perk and category tables, its
/// skills left out (the client keeps the ones it has).
#[must_use]
pub fn repository_update(credits: u32, hero_credits: u32) -> ServerMessage {
    repository_untraining(credits, hero_credits, None)
}

/// The repository with its credits and hero skill credits and the skill being untrained with the
/// experience to earn.
#[must_use]
pub fn repository_untraining(
    credits: u32,
    hero_credits: u32,
    untraining: Option<(u32, u64)>,
) -> ServerMessage {
    let (skill, xp) = untraining.unwrap_or((0, 0));
    let mut b = Builder::new();
    let repo = b.reserve(package::SKILL_REPOSITORY.0);
    let perks = b.native(
        native::WORD_WORD_HASH,
        hash(Elem::Word, Elem::Word, TABLE_SIZE, &[]),
    );
    let categories = b.native(
        native::WORD_WORD_HASH,
        hash(Elem::Word, Elem::Word, TABLE_SIZE, &[]),
    );
    b.fill(
        repo,
        &[
            Slot::Int(credits),
            Slot::Long(xp),
            Slot::Int(hero_credits),
            Slot::Ref(Some(perks)),
            Slot::Int(skill),
            Slot::Ref(Some(categories)),
            Slot::Ref(None),
        ],
    );
    session::call(function::UPDATE_REPOSITORY, &[Arg::Object(b.graph())])
        .expect("a repository packs")
}

/// A skill's information as the client keeps it (its cached rating and when it was cached, as a
/// raise answers; none otherwise).
#[must_use]
pub fn info_slots(h: &Held, cached: Option<(u32, f64)>) -> Vec<Slot> {
    let (level, at) = cached.unwrap_or((0, 0.0));
    vec![
        Slot::Time(-1.0),
        Slot::Int(level),
        Slot::Double(at),
        Slot::Long(h.xp),
        Slot::Int(h.mask),
        Slot::Time(-1.0),
        Slot::Int(h.children),
        Slot::Int(h.dependents),
        Slot::Int(h.cost),
        Slot::Int(h.granted),
        Slot::Int(h.skill),
    ]
}

/// One skill's information after a use: when it was last used (game time), its rating cached
/// with when the use began.
#[must_use]
pub fn used_update(h: &Held, rating: u32, began: f64, used: f64) -> ServerMessage {
    let mut slots = info_slots(h, Some((rating, began)));
    slots[0] = Slot::Time(used);
    let mut b = Builder::new();
    b.object(package::SKILL.0, &slots);
    session::call(function::UPDATE_INFO, &[Arg::Object(b.graph())]).expect("a skill packs")
}

/// One skill's information.
#[must_use]
pub fn info_update(h: &Held, cached: Option<(u32, f64)>) -> ServerMessage {
    let mut b = Builder::new();
    b.object(package::SKILL.0, &info_slots(h, cached));
    session::call(function::UPDATE_INFO, &[Arg::Object(b.graph())]).expect("a skill packs")
}

fn skill_name(t: &SkillTable, skill: u32) -> TextInfo {
    progress::text(t.skills.get(&skill).map_or((0, 0), |d| d.name))
}

fn refuse(net: &mut ServerNet, world: ConnId, t: &SkillTable, skill: u32, code: u32) {
    let msg = text_with(
        (refusal_text(code), TEXTS),
        vec![nested(VAR_SKILL, skill_name(t, skill))],
    );
    progress::private(net, world, &crate::chat::show_text(1, msg));
}

/// The skill a request names.
fn skill_of(v: &[Value]) -> Option<u32> {
    v.iter().find_map(|x| match x {
        Value::Int(i) => Some(*i),
        _ => None,
    })
}

/// Keep a player's skills where they differ from its race's start.
fn keep(p: &mut crate::world::Player, starting: &[(u32, u32)], skills: &[Held]) {
    p.play.skills = kept(starting, skills);
}

/// The skills a character keeps: those it holds beyond its race's starting skills, or changed
/// since.
#[must_use]
pub fn kept(starting: &[(u32, u32)], skills: &[Held]) -> Vec<SkillState> {
    skills
        .iter()
        .filter(|h| {
            starting
                .iter()
                .find(|(s, _)| *s == h.skill)
                .is_none_or(|(_, g)| {
                    *g != h.granted
                        || h.xp != 0
                        || h.mask != mask::TRAINED
                        || h.children != 0
                        || h.dependents != 0
                })
        })
        .map(|h| SkillState::from(*h))
        .collect()
}

/// A skill trained: the new skill, the parents and prerequisites it changed, and what it cost in
/// skill credits and in hero skill credits.
pub type Trained = (Held, Vec<Held>, (u32, u32));

/// What a player may spend on training: its skill credits, and a hero's hero skill credits (none
/// for a player who is no hero). As a cost: what a training spends of each.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Credits {
    pub skill: u32,
    pub hero: Option<u32>,
}

impl Credits {
    /// A player's: its skill credits, and its hero skill credits when it is a hero.
    #[must_use]
    pub fn of(p: &crate::world::Player) -> Self {
        Self {
            skill: p.play.credits,
            hero: crate::focus::is_hero(&p.qualities).then_some(p.play.hero_credits),
        }
    }
}

/// Train a skill into the skills a player holds, by the rules a player's training follows: its
/// trained parents count one more child, its prerequisites one more dependent, and the new skill
/// is pushed, its cost when learned its cost in the credits it is paid in (a hero skill's in hero
/// skill credits; 2017: 7 of 7 hero skills trained). Returns the new skill, the parents and
/// prerequisites it changed, and what it costs of each kind of credit; or the refusal's code,
/// nothing changed.
///
/// # Errors
/// Why the player may not train it ([`refusal`]).
pub fn trained(
    table: &SkillTable,
    skills: &mut Vec<Held>,
    skill: u32,
    level: i32,
    race: u32,
    credits: Credits,
) -> Result<Trained, u32> {
    if let Some(code) = table.train_refusal(skills, skill, level, race, credits) {
        return Err(code);
    }
    let Some(d) = table.skills.get(&skill) else {
        return Err(refusal::NOT_BY_PLAYER);
    };
    let hero_skill = d.flags & flag::HERO != 0;
    let mut changed = Vec::new();
    for (parent, _) in &d.parents {
        if let Some(h) = skills.iter_mut().find(|h| h.skill == *parent) {
            h.children += 1;
            changed.push(*h);
        }
    }
    for (pre, _) in &d.prereqs {
        if let Some(h) = skills.iter_mut().find(|h| h.skill == *pre) {
            h.dependents += 1;
            changed.push(*h);
        }
    }
    let new = Held {
        skill,
        granted: 0,
        xp: 0,
        mask: mask::TRAINED
            | if d.flags & flag::MAY_UNTRAIN == 0 {
                0
            } else {
                mask::PLAYERS_TO_UNTRAIN
            },
        cost: if hero_skill { d.hero_cost } else { d.cost },
        children: 0,
        dependents: 0,
    };
    skills.push(new);
    Ok((
        new,
        changed,
        if hero_skill {
            (0, d.hero_cost)
        } else {
            (d.cost, 0)
        },
    ))
}

/// A player asks to train a skill.
pub fn train<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, v: &[Value]) {
    let Some(skill) = skill_of(v) else {
        s.unhandled.note("a training that does not read");
        return;
    };
    let Some(p) = s.players.get(&world) else {
        return;
    };
    let starting = s.start.skills.get(&p.race).cloned().unwrap_or_default();
    let mut skills = held(&starting, &p.play.skills);
    let level = crate::examine::level_of(&p.qualities);
    let (new, changed, (cost, hero_cost)) = match trained(
        &s.skill_table,
        &mut skills,
        skill,
        level,
        p.race,
        Credits::of(p),
    ) {
        Ok(t) => t,
        Err(code) => {
            refuse(net, world, &s.skill_table, skill, code);
            return;
        }
    };
    let Some(d) = s.skill_table.skills.get(&skill).cloned() else {
        return;
    };
    for h in &changed {
        progress::private(net, world, &info_update(h, None));
    }
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    p.play.credits -= cost;
    p.play.hero_credits -= hero_cost;
    let (credits, hero_credits) = (p.play.credits, p.play.hero_credits);
    keep(p, &starting, &skills);
    progress::private(net, world, &repository_update(credits, hero_credits));
    progress::private(net, world, &info_update(&new, None));
    let said = text_with(
        (text::TRAINED_IT, TEXTS),
        vec![nested(VAR_SKILL, progress::text(d.name))],
    );
    progress::say(net, world, said);
    crate::world::keep_player(s, world);
}

/// A player asks to raise a skill.
pub fn raise<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    v: &[Value],
    clock: Clock,
) {
    let Some(skill) = skill_of(v) else {
        s.unhandled.note("a raise that does not read");
        return;
    };
    let Some(p) = s.players.get(&world) else {
        return;
    };
    let starting = s.start.skills.get(&p.race).cloned().unwrap_or_default();
    let mut skills = held(&starting, &p.play.skills);
    let available = quals::long(&p.qualities, progress::q::AVAILABLE_XP).unwrap_or(0);
    let hero = crate::focus::is_hero(&p.qualities);
    let cost = match s.skill_table.raise(&skills, skill, available, hero) {
        Ok(c) => c,
        Err(code) => {
            refuse(net, world, &s.skill_table, skill, code);
            return;
        }
    };
    let before = s.skill_table.rating(&skills, skill, hero);
    let Some(h) = skills.iter_mut().find(|h| h.skill == skill) else {
        return;
    };
    h.xp += cost;
    let h = *h;
    let after = s.skill_table.rating(&skills, skill, hero);
    let time = net.game_time(clock.local);
    let name = skill_name(&s.skill_table, skill);
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let left = available - cost;
    quals::set_long(&mut p.qualities, progress::q::AVAILABLE_XP, left);
    keep(p, &starting, &skills);
    progress::private(net, world, &info_update(&h, Some((before, time))));
    progress::send_long(net, world, progress::q::AVAILABLE_XP, left);
    let said = text_with(
        (text::RAISED_IT, TEXTS),
        vec![nested(VAR_SKILL, name), word(VAR_RATING, 2, after)],
    );
    progress::say(net, world, said);
    crate::world::keep_player(s, world);
}

/// A player asks to untrain a skill it trained. With no experience put into it, it is untrained at
/// once: told so with the credits it regains, its parent's trained children one fewer, the time
/// kept, the skill forgotten, the credits back. With experience in it, the untraining waits until
/// the player has earned that much again: the repository names the skill and the experience, and
/// the player is told.
pub fn untrain<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    v: &[Value],
    clock: Clock,
) {
    let Some(skill) = skill_of(v) else {
        s.unhandled.note("an untraining that does not read");
        return;
    };
    let Some(p) = s.players.get(&world) else {
        return;
    };
    let starting = s.start.skills.get(&p.race).cloned().unwrap_or_default();
    let skills = held(&starting, &p.play.skills);
    let Some(h) = skills.iter().find(|h| h.skill == skill).copied() else {
        return;
    };
    if h.granted != 0 || p.untraining.is_some() {
        return;
    }
    if h.xp == 0 {
        let now = net.game_time(clock.local);
        untrained(s, net, world, skill, 0, now);
        return;
    }
    let total = quals::long(&p.qualities, progress::q::TOTAL_XP).unwrap_or(0);
    let (credits, hero_credits) = (p.play.credits, p.play.hero_credits);
    let name = skill_name(&s.skill_table, skill);
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    p.untraining = Some(Untraining {
        skill,
        xp: h.xp,
        total_at_start: total,
    });
    progress::private(
        net,
        world,
        &repository_untraining(credits, hero_credits, Some((skill, h.xp))),
    );
    let said = text_with(
        (untraining::UNDER_WAY, TEXTS),
        vec![
            progress::long(untraining::VAR_XP, 12, h.xp),
            nested(VAR_SKILL, name),
        ],
    );
    progress::private(
        net,
        world,
        &crate::chat::show_text(untraining::SHOWN_AS, said),
    );
}

/// The skill is untrained: told so (with the experience it gives back), its parents' trained
/// children one fewer, the time kept, the skill forgotten, its credits back.
fn untrained<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    skill: u32,
    xp_back: u64,
    now: f64,
) {
    let Some(p) = s.players.get(&world) else {
        return;
    };
    let starting = s.start.skills.get(&p.race).cloned().unwrap_or_default();
    let mut skills = held(&starting, &p.play.skills);
    let Some(h) = skills.iter().find(|h| h.skill == skill).copied() else {
        return;
    };
    let def = s.skill_table.skills.get(&skill).cloned();
    let said = text_with(
        (untraining::UNTRAINED, TEXTS),
        vec![
            nested(VAR_SKILL, skill_name(&s.skill_table, skill)),
            word(untraining::VAR_CREDITS, 6, h.cost),
            progress::long(untraining::VAR_XP, 12, xp_back),
        ],
    );
    progress::private(
        net,
        world,
        &crate::chat::show_text(untraining::SHOWN_AS, said),
    );
    let mut changed = Vec::new();
    for (parent, _) in def.iter().flat_map(|d| d.parents.iter()) {
        if let Some(x) = skills.iter_mut().find(|x| x.skill == *parent) {
            x.children = x.children.saturating_sub(1);
            changed.push(*x);
        }
    }
    for (pre, _) in def.iter().flat_map(|d| d.prereqs.iter()) {
        if let Some(x) = skills.iter_mut().find(|x| x.skill == *pre) {
            x.dependents = x.dependents.saturating_sub(1);
            changed.push(*x);
        }
    }
    skills.retain(|x| x.skill != skill);
    for x in &changed {
        progress::private(net, world, &info_update(x, None));
    }
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    quals::set_timestamp(&mut p.qualities, untraining::LAST_UNTRAIN, now);
    progress::private(
        net,
        world,
        &ServerMessage::UpdateTimestamp(auberean_protocol::quality::PrivateQuality {
            quality: untraining::LAST_UNTRAIN,
            value: now,
        }),
    );
    let gone = session::call(function::REMOVE_INFO, &[Arg::Int(skill)]).expect("a skill packs");
    progress::private(net, world, &gone);
    // A hero skill's cost goes back to the hero skill credits it was paid in.
    if def.as_ref().is_some_and(|d| d.flags & flag::HERO != 0) {
        p.play.hero_credits += h.cost;
    } else {
        p.play.credits += h.cost;
    }
    p.untraining = None;
    let (credits, hero_credits) = (p.play.credits, p.play.hero_credits);
    keep(p, &starting, &skills);
    progress::private(net, world, &repository_update(credits, hero_credits));
    crate::world::keep_player(s, world);
}

/// A player has earned experience: an untraining under way is finished once the player has earned
/// its experience since it began, the skill's experience given back to spend.
pub fn earned<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, total: u64) {
    let Some(u) = s.players.get(&world).and_then(|p| p.untraining) else {
        return;
    };
    if total.saturating_sub(u.total_at_start) < u.xp {
        return;
    }
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let available = quals::long(&p.qualities, progress::q::AVAILABLE_XP).unwrap_or(0) + u.xp;
    quals::set_long(&mut p.qualities, progress::q::AVAILABLE_XP, available);
    let now = s.game_now;
    untrained(s, net, world, u.skill, u.xp, now);
}

/// A player cancels its untraining under way: the repository names none, the skill is shown as it
/// stands, and the player is told.
pub fn cancel_untraining<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId) {
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let Some(u) = p.untraining.take() else {
        return;
    };
    let (credits, hero_credits) = (p.play.credits, p.play.hero_credits);
    let starting = s.start.skills.get(&p.race).cloned().unwrap_or_default();
    let skills = held(&starting, &p.play.skills);
    progress::private(net, world, &repository_update(credits, hero_credits));
    if let Some(h) = skills.iter().find(|h| h.skill == u.skill) {
        progress::private(net, world, &info_update(h, None));
    }
    let said = progress::text((untraining::CANCELLED, TEXTS));
    progress::private(
        net,
        world,
        &crate::chat::show_text(untraining::CANCELLED_SHOWN_AS, said),
    );
}
