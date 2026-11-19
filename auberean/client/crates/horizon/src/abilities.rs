//! The player's abilities as Horizon draws them: the shortcut bar (its slots, the set shown, each
//! skill's wait), attacking and the special attacks the bar lights, and the skill window's panel
//! with its skills and the player's points. Followed once a frame from the view and the notices.
//!
//! The special attacks light the slots holding their skill as the game's own bar lights them: a
//! special attack asked for (once for each of its numbers) lights them as asked, its start as
//! started, its end puts them back to how its skill's toggle leaves them, and a skill's special
//! attack turned on lights its slots until it is turned off.

use std::collections::{BTreeMap, BTreeSet};

use auberean_client_sdk::contract::notice::examination::kind as examine;
use auberean_client_sdk::contract::notice::{CombatNotice, ExaminationNotice, Said};
use auberean_client_sdk::contract::text::TextRef;
use auberean_client_sdk::contract::view::{self as v, skill_node};
use auberean_client_sdk::contract::{shortcut_kind, ToolbarShortcut, TOOLBAR_SLOTS};

use crate::state::{Frame, GameState};

/// A slot of the shortcut bar.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Slot {
    /// What it holds (kind 0: nothing).
    pub shortcut: v::Shortcut,
    /// Its icon, a picture of the game's records (0 for none), and what it is called.
    pub icon: u32,
    pub name: String,
    /// A skill's wait: how long it is in all, and the game's time it is ready at.
    pub wait: Option<(f64, f64)>,
    /// A skill the player cannot use with what it carries, or an item the player has not got.
    pub unusable: bool,
}

impl Slot {
    /// Whether it holds nothing.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.shortcut.kind == 0
    }

    /// What it holds, as a slot is given it.
    #[must_use]
    pub fn to_shortcut(&self) -> ToolbarShortcut {
        to_shortcut(&self.shortcut)
    }
}

/// A view's shortcut as a slot is given it.
#[must_use]
pub fn to_shortcut(s: &v::Shortcut) -> ToolbarShortcut {
    ToolbarShortcut {
        kind: s.kind,
        item: s.item,
        data_enum: s.data_enum,
        data_id: s.data_id,
        text: s.text.encode_utf16().collect(),
    }
}

/// A skill's shortcut.
#[must_use]
pub fn skill_shortcut(skill: u32) -> ToolbarShortcut {
    ToolbarShortcut {
        kind: shortcut_kind::SKILL,
        data_enum: skill,
        ..ToolbarShortcut::default()
    }
}

/// The shortcut bar.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bar {
    /// The view's toolbar this was made from.
    pub version: u64,
    /// Every slot the scripts keep, by its number.
    pub slots: Vec<Slot>,
    /// The set of ten shown (from 1), the rows shown, and whether the slots are locked (a slot is
    /// dragged off the bar only with Shift held).
    pub set: u32,
    pub rows: u32,
    pub locked: bool,
}

/// The rows the bar shows at most.
pub const MOST_ROWS: u32 = 4;

/// The game's short names for the kinds of skill (melee, missile, magic, the rest), as the skill
/// window's buttons read, by their names in [`crate::STRINGS`].
pub const KIND_LABELS: [&str; 4] = [
    "ID_Skill_Melee_Label",
    "ID_Skill_Missile_Label",
    "ID_Skill_Magic_Label",
    "ID_Skill_Passive_Label",
];

/// The sets of ten.
pub const SETS: u32 = TOOLBAR_SLOTS / 10;

impl Bar {
    /// The first slot of the set shown.
    #[must_use]
    pub fn first(&self) -> u32 {
        (self.set.clamp(1, SETS) - 1) * 10
    }

    /// The slot numbered `n`, when the scripts keep one.
    #[must_use]
    pub fn slot(&self, n: u32) -> Option<&Slot> {
        self.slots.get(usize::try_from(n).ok()?)
    }

    /// The set after the one shown (`step` 1) or before it (-1), round past either end.
    #[must_use]
    pub fn stepped(&self, step: i32) -> u32 {
        let sets = i32::try_from(SETS).unwrap_or(10);
        let at = i32::try_from(self.set.clamp(1, SETS)).unwrap_or(1) - 1;
        u32::try_from((at + step).rem_euclid(sets)).unwrap_or(0) + 1
    }

    /// The bar as the view's toolbar has it, a skill's wait counted from the game's time `now`.
    #[must_use]
    pub fn from_view(t: &v::Toolbar, now: f64) -> Self {
        let n = usize::try_from(TOOLBAR_SLOTS).unwrap_or(100);
        let slots = (0..n)
            .map(|i| {
                let shortcut = t.shortcuts.get(i).cloned().unwrap_or_default();
                let said = t.slots.get(i).cloned().unwrap_or_default();
                let mut slot = Slot {
                    icon: said.icon,
                    name: said.tooltip,
                    ..Slot::default()
                };
                match shortcut.kind {
                    shortcut_kind::SKILL => {
                        if let Some(s) = t.skills.iter().find(|s| s.skill == shortcut.data_enum) {
                            slot.unusable = !s.usable;
                            slot.wait = s
                                .waiting
                                .filter(|(_, left)| *left > 0.0)
                                .map(|(total, left)| (f64::from(total), now + f64::from(left)));
                        }
                    }
                    shortcut_kind::ITEM => {
                        if let Some(it) = t.items.iter().find(|it| it.item == shortcut.item) {
                            slot.unusable = !it.have;
                            if let Some((name, icon)) = &it.here {
                                slot.name.clone_from(name);
                                slot.icon = *icon;
                            }
                        }
                    }
                    _ => {}
                }
                if slot.name.is_empty() {
                    slot.name.clone_from(&shortcut.text);
                }
                slot.shortcut = shortcut;
                slot
            })
            .collect();
        Self {
            version: t.version,
            slots,
            set: t.set.clamp(1, SETS),
            rows: u32::try_from(t.height).unwrap_or(1).clamp(1, MOST_ROWS),
            locked: t.locked,
        }
    }
}

/// How a slot holding a skill is lit for its special attack.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Light {
    #[default]
    Plain,
    /// Asked for, not started.
    Asked,
    Started,
    /// Turned on.
    Toggled,
}

/// Attacking, and the special attacks.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Combat {
    /// The player is attacking, as its scripts last said.
    pub attacking: bool,
    /// The special attacks asked for and not ended: each its number, its skill, and whether it
    /// started.
    pub specials: Vec<(u32, u32, bool)>,
    /// The skills whose special attack is turned on.
    pub toggled: BTreeSet<u32>,
}

impl Combat {
    /// How the slots holding `skill` are lit.
    #[must_use]
    pub fn light(&self, skill: u32) -> Light {
        let mut asked = false;
        for &(_, s, started) in &self.specials {
            if s == skill {
                if started {
                    return Light::Started;
                }
                asked = true;
            }
        }
        if asked {
            Light::Asked
        } else if self.toggled.contains(&skill) {
            Light::Toggled
        } else {
            Light::Plain
        }
    }

    /// What the scripts said of combat.
    pub fn notice(&mut self, n: &CombatNotice) {
        match *n {
            CombatNotice::Attacking { attacking } => self.attacking = attacking,
            CombatNotice::SpecialAsked { skill, special } => {
                if !self.specials.iter().any(|(s, _, _)| *s == special) {
                    self.specials.push((special, skill, false));
                }
            }
            CombatNotice::SpecialStarted { special } => {
                for s in self.specials.iter_mut().filter(|s| s.0 == special) {
                    s.2 = true;
                }
            }
            CombatNotice::SpecialEnded { special } => self.specials.retain(|s| s.0 != special),
            CombatNotice::SpecialToggled { skill, on } => {
                if on {
                    self.toggled.insert(skill);
                } else {
                    self.toggled.remove(&skill);
                }
            }
            _ => {}
        }
    }
}

/// A node of the skill panel shown.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Node {
    /// The skill it stands for, or the panel it opens ([`skill_node`]).
    pub skill: u32,
    pub kind: u32,
    pub name: String,
    pub icon: u32,
    /// Its column and row in the tree.
    pub x: i32,
    pub y: i32,
    /// The nodes it hangs from.
    pub parents: Vec<u32>,
    pub passive: bool,
    /// Its training may be undone.
    pub untrainable: bool,
    pub perk: bool,
}

impl Node {
    /// Whether it stands for a skill (the others open panels).
    #[must_use]
    pub const fn is_skill(&self) -> bool {
        self.kind == skill_node::SKILL
    }
}

/// The skill window's panel.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Skills {
    /// The view's skills this was made from.
    pub version: u64,
    /// The panel shown (0: none yet), its name and icon, and its nodes.
    pub panel: u32,
    pub panel_name: String,
    pub panel_icon: u32,
    pub nodes: Vec<Node>,
    /// The state of each skill of the panel.
    pub states: BTreeMap<u32, v::SkillState>,
    /// The root panel of each kind of skill (melee, missile, magic, the rest), its name and
    /// icon, where the player's race has one.
    pub roots: [Option<(u32, String, u32)>; 4],
    /// The game's own short names for the kinds, as their buttons read (empty where its strings
    /// do not name one).
    pub labels: [String; 4],
    pub experience: u64,
    pub credits: u32,
    pub hero_credits: u32,
    /// The skill selected (0: none): the one the examination last asked about, on this panel.
    pub selected: u32,
    /// The panels visited before this one, the last first.
    pub history: Vec<u32>,
    /// A name was not written out yet (its strings are not here).
    pub unwritten: bool,
}

impl Skills {
    /// The node of `skill` on the panel.
    #[must_use]
    pub fn node(&self, skill: u32) -> Option<&Node> {
        self.nodes.iter().find(|n| n.skill == skill)
    }

    /// The panel as the view has it, its names written by `write`; the selection and the panels
    /// visited carried over from `before`.
    fn from_view(s: &v::Skills, before: Option<&Self>, f: &Frame<'_>) -> Self {
        let mut unwritten = false;
        let mut write = |t: &auberean_client_sdk::contract::text::TextRef| {
            (f.write)(t).unwrap_or_else(|| {
                unwritten = true;
                String::new()
            })
        };
        let panel = s.nodes.first().map_or(s.panel, |_| s.panel);
        let (panel_name, panel_icon) = s
            .panels
            .get(&panel)
            .map(|(n, i)| (write(n), *i))
            .unwrap_or_default();
        let nodes = s
            .nodes
            .iter()
            .map(|n| Node {
                skill: n.skill,
                kind: n.kind,
                name: write(&n.name),
                icon: n.icon,
                x: n.x,
                y: n.y,
                parents: n.parents.clone(),
                passive: n.passive,
                untrainable: n.untrainable,
                perk: n.perk,
            })
            .collect::<Vec<_>>();
        let roots = s.roots.map(|r| {
            r.map(|id| {
                let (name, icon) = s
                    .panels
                    .get(&id)
                    .map(|(n, i)| (write(n), *i))
                    .unwrap_or_default();
                (id, name, icon)
            })
        });
        let labels = KIND_LABELS.map(|name| {
            (f.write)(&TextRef {
                string: auberean_ui::token::Token::of(name).raw(),
                table: crate::STRINGS,
                ..TextRef::default()
            })
            .unwrap_or_default()
        });
        let mut history = before.map(|b| b.history.clone()).unwrap_or_default();
        let mut selected = before.map_or(0, |b| b.selected);
        if let Some(b) = before.filter(|b| b.panel != panel && b.panel != 0) {
            if history.first() == Some(&panel) {
                history.remove(0);
            } else {
                history.insert(0, b.panel);
            }
            selected = 0;
        }
        if !nodes.iter().any(|n| n.skill == selected && n.is_skill()) {
            selected = 0;
        }
        Self {
            version: s.version,
            panel,
            panel_name,
            panel_icon,
            nodes,
            states: s.states.clone(),
            roots,
            labels,
            experience: s.experience,
            credits: s.credits,
            hero_credits: s.hero_credits,
            selected,
            history,
            unwritten,
        }
    }
}

/// The bar, combat and the skill window followed for one frame.
pub fn follow(state: &mut GameState, f: &Frame<'_>) {
    for n in f.notices {
        match &n.said {
            Said::Combat(c) if n.about.is_none() || n.about == state.player => {
                state.combat.notice(c);
            }
            // The examination asked about a skill: the skill window selects it.
            Said::Examination(ExaminationNotice::RequestChanged(Some(r)))
                if r.kind == examine::SKILL =>
            {
                if let Some(s) = &mut state.skills {
                    if s.node(r.data_enum).is_some_and(Node::is_skill) {
                        s.selected = r.data_enum;
                    }
                }
            }
            _ => {}
        }
    }
    let Some(view) = f.view else {
        return;
    };
    let now = view.clock.game_time;
    match &view.toolbar {
        Some(t) if state.bar.as_ref().map(|b| b.version) != Some(t.version) => {
            state.bar = Some(Bar::from_view(t, now));
        }
        Some(_) => {}
        None => state.bar = None,
    }
    match &view.skills {
        Some(s)
            if state
                .skills
                .as_ref()
                .is_none_or(|k| k.version != s.version || k.unwritten) =>
        {
            state.skills = Some(Skills::from_view(s, state.skills.as_ref(), f));
        }
        Some(_) => {}
        None => state.skills = None,
    }
    if !state.in_world {
        state.combat = Combat::default();
    }
}
