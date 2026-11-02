//! The skill window as it asks the player's scripts: the skill panels, each skill's state, and
//! what the player asks of a skill.
//!
//! - **A panel's nodes** come from the skill panel manager's shared instance: its node function
//!   fills a table of the panel's nodes by skill, each node a record (the skill or panel it
//!   stands for, its kind, its name and description, its icon, its place in the tree, the nodes
//!   it hangs from and those that depend on it, whether it is passive, whether it may be
//!   untrained, whether it is a perk); its name and icon functions answer through out arguments.
//! - **A skill's state** comes from the player's interface functions, each called with the skill:
//!   whether it is trained, being untrained, a hero skill; whether it may be trained, untrained or
//!   raised (0 when it may; else why not); what training costs; the experience needed to raise it;
//!   the experience it holds; its level (its own, and as buffs and debuffs leave it).
//! - **The player's points**: the experience to spend, the skill credits, the hero skill credits.
//! - **The root panels**: one per kind of skill (melee, missile, magic, the rest), by the player's
//!   race (none when the race has none of the kind).
//!
//! [`Skills`] keeps them for the skill window: asked again for the panel the examination last
//! asked for, whenever the scripts tell the panels something about skills or the examination.
//! What the player asks of a skill (train, untrain, raise, stop untraining, examine a skill or a
//! panel) is one function of the player's scripts each ([`crate::Client::act`]).

use std::collections::BTreeMap;
use std::sync::Arc;

use auberean_script::library::STATE_BASE;
use auberean_script::native::{Status, Stop};
use auberean_script::native_object::{Item, NativeObject, TextInfo};
use auberean_script::value::{tag, FunctionId, NULL};
use auberean_script::wire::In;

use crate::host::{CallResult, PanelNotice, Scripts, BUDGET};

/// The player's interface functions the skill window calls, by id.
pub mod function {
    /// The experience to spend (64 bits).
    pub const AVAILABLE_EXPERIENCE: u32 = 0x00B4_001E;
    /// The skill credits.
    pub const SKILL_CREDITS: u32 = 0x00B4_001F;
    /// The root panel of a kind of skill for the player's race, into an out argument: 1, or 0.
    pub const ROOT_PANEL_FROM_RACE: u32 = 0x00B4_0020;
    /// Whether a skill is trained; being untrained.
    pub const IS_TRAINED: u32 = 0x00B4_0021;
    pub const IS_BEING_UNTRAINED: u32 = 0x00B4_0022;
    /// Whether a skill may be trained, untrained, raised: 0 when it may, else why not.
    pub const CAN_TRAIN: u32 = 0x00B4_0023;
    pub const CAN_UNTRAIN: u32 = 0x00B4_0024;
    pub const CAN_RAISE: u32 = 0x00B4_0025;
    /// The experience needed to raise a skill (64 bits).
    pub const EXPERIENCE_TO_RAISE: u32 = 0x00B4_0026;
    /// A skill's level, its own and as it stands, into two out arguments.
    pub const LEVEL: u32 = 0x00B4_0027;
    /// What training a skill costs.
    pub const COST_TO_TRAIN: u32 = 0x00B4_0028;
    /// The experience a skill holds (64 bits).
    pub const SKILL_EXPERIENCE: u32 = 0x00B4_002A;
    /// Train, untrain, stop untraining, raise.
    pub const TRAIN: u32 = 0x00B4_002B;
    pub const UNTRAIN: u32 = 0x00B4_002C;
    pub const CANCEL_UNTRAIN: u32 = 0x00B4_002D;
    pub const RAISE: u32 = 0x00B4_002E;
    /// What the examination asks for (an examination request).
    pub const SET_EXAMINATION_REQUEST: u32 = 0x00B4_0066;
    /// The hero skill credits.
    pub const HERO_SKILL_CREDITS: u32 = 0x00B4_00F0;
    /// Whether a skill is a hero skill.
    pub const IS_HERO: u32 = 0x00B4_00F2;
    /// The skill panel manager's: a panel's name and icon (out arguments), its nodes (a table).
    pub const PANEL_NAME: u32 = 0x00AD << 16 | 0x0002;
    pub const PANEL_ICON: u32 = 0x00AD << 16 | 0x0003;
    pub const PANEL_NODES: u32 = 0x00AD << 16 | 0x0005;
}

/// The package whose shared instance knows the skill panels.
pub const SKILL_PANEL_MANAGER: &str = "SkillPanelManager";

/// The script object type of a table of numbers to references.
pub const NUMBER_TABLE: u32 = 0x11;

/// The kinds of skill a root panel is for, by the number its function asks with: melee, missile,
/// magic, the rest.
pub const ROOT_KINDS: [u32; 4] = [0x8000_0000, 0x8000_0001, 0x8000_0002, 0x8000_0003];

/// An examination request for a skill, and for a skill panel.
pub const EXAMINE_SKILL: u32 = 0x4000_0003;
pub const EXAMINE_PANEL: u32 = 0x4000_0004;

/// The scripts' notices that tell the skill window something: a skill changed, the player's
/// points changed (and every node is to be refreshed), the examination's request changed.
pub const UPDATE_SKILL_PANEL: u32 = 0x0085 << 16 | 0x0005;
pub const UPDATE_SKILL: u32 = 0x0085 << 16 | 0x0006;
pub const UPDATE_SKILL_PANEL_FULL: u32 = 0x0085 << 16 | 0x000F;
pub const EXAMINATION_REQUEST_CHANGED: u32 = 0x0089 << 16 | 0x0002;

/// A node of a skill panel.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SkillNode {
    /// The skill (or, for a node that opens a panel, the panel) it stands for.
    pub skill: u32,
    /// A skill, or a node that opens another panel.
    pub kind: u32,
    pub name: TextInfo,
    pub description: TextInfo,
    pub icon: u32,
    pub x: i32,
    pub y: i32,
    /// The nodes it hangs from, and those whose state depends on it, by skill.
    pub parents: Vec<u32>,
    pub dependencies: Vec<u32>,
    pub passive: bool,
    /// Training it may be undone.
    pub untrainable: bool,
    pub perk: bool,
}

impl SkillNode {
    /// A node from its packed form.
    #[allow(clippy::cast_possible_wrap)]
    pub fn unpack(bytes: &[u8]) -> auberean_script::wire::Result<Self> {
        let mut r = In::new(bytes);
        let skill = r.u32()?;
        let kind = r.u32()?;
        let name = TextInfo::unpack(&mut r)?;
        let description = TextInfo::unpack(&mut r)?;
        let icon = r.u32()?;
        let x = r.u32()? as i32;
        let y = r.u32()? as i32;
        let list = |r: &mut In<'_>| -> auberean_script::wire::Result<Vec<u32>> {
            let n = r.u32()? as usize;
            let n = r.count(n, 4)?;
            (0..n).map(|_| r.u32()).collect()
        };
        let parents = list(&mut r)?;
        let dependencies = list(&mut r)?;
        let passive = r.u32()? != 0;
        let untrainable = r.u32()? != 0;
        let perk = r.u32()? != 0;
        Ok(Self {
            skill,
            kind,
            name,
            description,
            icon,
            x,
            y,
            parents,
            dependencies,
            passive,
            untrainable,
            perk,
        })
    }
}

/// A skill's state as the player's scripts answer for it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SkillState {
    pub trained: bool,
    pub being_untrained: bool,
    pub hero: bool,
    /// 0 when it may be trained, untrained, raised; else why not.
    pub can_train: u32,
    pub can_untrain: u32,
    pub can_raise: u32,
    pub cost_to_train: u32,
    pub experience_to_raise: u64,
    pub experience: u64,
    /// Its own level, and its level as it stands.
    pub base_level: u32,
    pub level: u32,
}

/// A skill panel: its name, its icon, its nodes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SkillPanel {
    pub id: u32,
    pub name: TextInfo,
    pub icon: u32,
    pub nodes: Vec<SkillNode>,
}

/// What the skill window shows, as last asked of the scripts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Skills {
    /// Counts each asking, so a reader sees when it changed.
    pub version: u64,
    /// The panel the examination last asked for, and every panel's name and icon read so far.
    pub panel: Option<SkillPanel>,
    pub panels: BTreeMap<u32, (TextInfo, u32)>,
    /// The state of each skill of the panel.
    pub states: BTreeMap<u32, SkillState>,
    /// The root panel of each kind of skill for the player's race.
    pub roots: [Option<u32>; 4],
    pub experience: u64,
    pub credits: u32,
    pub hero_credits: u32,
    /// The calls of the last asking that did not run to their end.
    pub incomplete: Vec<(u32, CallResult)>,
}

impl Skills {
    /// The scripts' notices of a call: when one is about skills or the examination's request, the
    /// panel the examination last asked for (a request for a panel names a new one) and every
    /// skill of it asked again.
    pub fn follow(&mut self, scripts: &mut Scripts, notices: &[PanelNotice]) {
        let mut ask = false;
        for n in notices {
            match n.native {
                UPDATE_SKILL | UPDATE_SKILL_PANEL | UPDATE_SKILL_PANEL_FULL => ask = true,
                EXAMINATION_REQUEST_CHANGED => {
                    ask = true;
                    let request = n.objects.iter().flatten().find_map(|o| match o {
                        NativeObject::Packed { ty, bytes }
                            if *ty
                                == auberean_script::native_object::native_type::EXAMINATION_REQUEST =>
                        {
                            Some(crate::examination::Request::unpack(bytes))
                        }
                        _ => None,
                    });
                    if let Some(r) = request.filter(|r| r.kind == EXAMINE_PANEL) {
                        self.panel = Some(SkillPanel {
                            id: r.data_enum,
                            ..SkillPanel::default()
                        });
                    }
                }
                _ => {}
            }
        }
        if ask {
            self.ask(scripts);
        }
    }

    /// Everything asked again of the scripts: the panel's name, icon and nodes, each of its
    /// skills' state, the player's points, the root panels.
    pub fn ask(&mut self, scripts: &mut Scripts) {
        self.version += 1;
        let mut a = Asker {
            s: scripts,
            incomplete: Vec::new(),
        };
        if let Some(id) = self.panel.as_ref().map(|p| p.id) {
            let p = a.skill_panel(id);
            self.panels.insert(id, (p.name.clone(), p.icon));
            self.states = p
                .nodes
                .iter()
                .filter(|n| n.kind == SKILL_NODE)
                .map(|n| (n.skill, a.skill_state(n.skill)))
                .collect();
            self.panel = Some(p);
        }
        for (i, kind) in ROOT_KINDS.iter().enumerate() {
            self.roots[i] = a.root_panel(*kind);
        }
        for r in self.roots.into_iter().flatten() {
            if let std::collections::btree_map::Entry::Vacant(e) = self.panels.entry(r) {
                let (name, icon) = (a.panel_name(r), a.panel_icon(r));
                e.insert((name, icon));
            }
        }
        self.experience = a.long_of(function::AVAILABLE_EXPERIENCE, None);
        self.credits = a.word_of(function::SKILL_CREDITS, None);
        self.hero_credits = a.word_of(function::HERO_SKILL_CREDITS, None);
        self.incomplete = a.incomplete;
    }
}

/// The scripts asked, with the calls that did not run to their end.
pub(crate) struct Asker<'a> {
    pub(crate) s: &'a mut Scripts,
    pub(crate) incomplete: Vec<(u32, CallResult)>,
}

/// A node that stands for a skill (the others open panels).
pub const SKILL_NODE: u32 = 0x4000_0001;

/// A slot a called function writes into, pushed before the result slot: a word, a new native
/// object of a type, or a given native object of a type.
#[derive(Clone, Debug)]
pub(crate) enum Slot {
    Word(u32),
    New(u32),
    Given(u32, Box<NativeObject>),
}

/// What a call left: how it went, its result (both words of a 64-bit one), and what it wrote
/// into the slots.
#[derive(Clone, Debug)]
pub(crate) struct Answer {
    pub(crate) result: CallResult,
    pub(crate) value: u64,
    pub(crate) outs: Vec<u32>,
    pub(crate) objects: Vec<Option<NativeObject>>,
}

impl Asker<'_> {
    /// Call a function as the interface glue does: a new context of the player's process; the
    /// slots it writes into; the result slot (two for a 64-bit result); a reference to each slot,
    /// in order; the arguments; the receiver (the player's instance, or a package's shared
    /// instance); then the function, looked up in the receiver's package. `None` when there is no
    /// player here, or no such package.
    pub(crate) fn ask_with(
        &mut self,
        shared: Option<&str>,
        function: u32,
        slots: &[Slot],
        args: &[u32],
        long: bool,
    ) -> Option<Answer> {
        let args: Vec<(u32, u8)> = args.iter().map(|a| (*a, tag::MIXED)).collect();
        self.ask_tagged(shared, function, slots, &args, long)
    }

    /// As [`Asker::ask_with`], each argument pushed with its own tag (a 64-bit value's two
    /// slots marked as one).
    pub(crate) fn ask_tagged(
        &mut self,
        shared: Option<&str>,
        function: u32,
        slots: &[Slot],
        args: &[(u32, u8)],
        long: bool,
    ) -> Option<Answer> {
        let player = self.s.host.model.player?;
        self.ask_object(player, shared, function, slots, args, long)
    }

    /// As [`Asker::ask_tagged`], on an object's own process and instance, or a package's shared
    /// instance in a process of its own made for the call and ended after it: `None` when the
    /// object is not here or has no scripts.
    // D2-16490
    pub(crate) fn ask_object(
        &mut self,
        object: u64,
        shared: Option<&str>,
        function: u32,
        slots: &[Slot],
        args: &[(u32, u8)],
        long: bool,
    ) -> Option<Answer> {
        self.s.host.model.object(object)?;
        let (p, receiver, own) = match shared {
            None => {
                let p = self.s.process_of(object).ok()?;
                (p, self.s.vm.instance_of(p), false)
            }
            Some(name) => {
                let id = self.s.host.program.library.package_id(name)?;
                if id == NULL {
                    return None;
                }
                let s = self.s.vm.heap.singleton(STATE_BASE | (id & 0xFFFF));
                (self.s.vm.create_process(s), s, true)
            }
        };
        self.s.vm.budget = BUDGET;
        let mut answer = Answer {
            result: CallResult::Completed,
            value: 0,
            outs: vec![0; slots.len()],
            objects: vec![None; slots.len()],
        };
        let outcome = (|| -> Result<Status, Stop> {
            let mut cx = self.s.vm.new_context(p);
            let mut at = Vec::with_capacity(slots.len());
            for s in slots {
                at.push(cx.sp);
                match s {
                    Slot::Word(v) => cx.push(*v, tag::MIXED)?,
                    Slot::New(ty) => {
                        let fresh = NativeObject::new(&self.s.host.program.library, *ty);
                        let r = self.s.vm.heap.add_native(*ty, fresh);
                        cx.push(r, tag::REFERENCE)?;
                    }
                    Slot::Given(ty, o) => {
                        let r = self.s.vm.heap.add_native(*ty, (**o).clone());
                        cx.push(r, tag::REFERENCE)?;
                    }
                }
            }
            let result = cx.sp;
            if long {
                cx.push_long(0, tag::LONG_INT)?;
            } else {
                cx.push(0, tag::INT)?;
            }
            for a in &at {
                #[allow(clippy::cast_possible_truncation)]
                let r = self.s.vm.heap.add_relative(NULL, *a as u32);
                cx.push(r, tag::REFERENCE)?;
            }
            for (a, t) in args {
                cx.push(*a, *t)?;
            }
            cx.push(receiver, tag::REFERENCE)?;
            let prog = Arc::clone(&self.s.host.program);
            let package = self.s.vm.package_of(&mut self.s.host, receiver, &prog)?;
            let status =
                self.s
                    .vm
                    .call(&mut self.s.host, &mut cx, package, FunctionId(function))?;
            let (lo, _) = cx.slot(result)?;
            let hi = if long { cx.slot(result + 1)?.0 } else { 0 };
            answer.value = u64::from(lo) | u64::from(hi) << 32;
            for (i, a) in at.iter().enumerate() {
                let (v, t) = cx.slot(*a)?;
                answer.outs[i] = v;
                if tag::is_reference(t) && v != NULL {
                    answer.objects[i] = self.s.vm.heap.native(v).ok().cloned();
                }
            }
            Ok(status)
        })();
        answer.result = match outcome {
            Ok(Status::Terminated) => CallResult::Completed,
            Ok(s) => CallResult::Ended(s),
            Err(Stop::NotPorted { native, .. }) => CallResult::NotPorted(native),
            Err(Stop::NullReference) => CallResult::NullReference,
            Err(e) => CallResult::Stopped(e.to_string()),
        };
        if own {
            self.s.vm.delete_process(p);
        }
        self.s.vm.heap.abandon_builds();
        self.s.vm.stopped_at = None;
        if answer.result != CallResult::Completed {
            self.incomplete.push((function, answer.result.clone()));
        }
        Some(answer)
    }

    /// A word a player's function answers, with an argument or none (0 when it could not run).
    // D2-00490, D2-00491, D2-00494, D2-00495, D2-00496, D2-00497, D2-00498, D2-00499, D2-00502
    // D2-01147, D2-01121, D2-01187, D2-01185, D2-01076, D2-01077, D2-01075, D2-01146
    pub(crate) fn word_of(&mut self, function: u32, arg: Option<u32>) -> u32 {
        let args: Vec<u32> = arg.into_iter().collect();
        #[allow(clippy::cast_possible_truncation)]
        self.ask_with(None, function, &[], &args, false)
            .map_or(0, |a| a.value as u32)
    }

    /// A 64-bit number a player's function answers.
    // D2-00489, D2-00500, D2-00503, D2-01111, D2-01148, D2-01149
    pub(crate) fn long_of(&mut self, function: u32, arg: Option<u32>) -> u64 {
        let args: Vec<u32> = arg.into_iter().collect();
        self.ask_with(None, function, &[], &args, true)
            .map_or(0, |a| a.value)
    }

    /// The root panel of a kind of skill for the player's race, when it has one.
    // D2-00493, D2-01176
    fn root_panel(&mut self, kind: u32) -> Option<u32> {
        let a = self.ask_with(
            None,
            function::ROOT_PANEL_FROM_RACE,
            &[Slot::Word(0)],
            &[kind],
            false,
        )?;
        (a.value != 0).then_some(a.outs[0])
    }

    /// A skill's level: its own, and as it stands.
    // D2-00501, D2-01180
    fn skill_level(&mut self, skill: u32) -> (u32, u32) {
        let Some(a) = self.ask_with(
            None,
            function::LEVEL,
            &[Slot::Word(0), Slot::Word(0)],
            &[skill],
            false,
        ) else {
            return (0, 0);
        };
        (a.outs[1], a.outs[0])
    }

    /// A skill's state, every answer asked.
    fn skill_state(&mut self, skill: u32) -> SkillState {
        let s = Some(skill);
        let (base_level, level) = self.skill_level(skill);
        SkillState {
            trained: self.word_of(function::IS_TRAINED, s) != 0,
            being_untrained: self.word_of(function::IS_BEING_UNTRAINED, s) != 0,
            hero: self.word_of(function::IS_HERO, s) != 0,
            can_train: self.word_of(function::CAN_TRAIN, s),
            can_untrain: self.word_of(function::CAN_UNTRAIN, s),
            can_raise: self.word_of(function::CAN_RAISE, s),
            cost_to_train: self.word_of(function::COST_TO_TRAIN, s),
            experience_to_raise: self.long_of(function::EXPERIENCE_TO_RAISE, s),
            experience: self.long_of(function::SKILL_EXPERIENCE, s),
            base_level,
            level,
        }
    }

    /// A skill panel's name, through an out argument (a new string reference).
    // D2-01530
    fn panel_name(&mut self, panel: u32) -> TextInfo {
        let a = self.ask_with(
            Some(SKILL_PANEL_MANAGER),
            function::PANEL_NAME,
            &[Slot::New(
                auberean_script::native_object::native_type::STRING_INFO,
            )],
            &[panel],
            false,
        );
        match a.and_then(|a| a.objects.into_iter().next().flatten()) {
            Some(NativeObject::TextInfo(t)) => t,
            _ => TextInfo::default(),
        }
    }

    /// A skill panel's icon, through an out argument.
    // D2-01529
    fn panel_icon(&mut self, panel: u32) -> u32 {
        self.ask_with(
            Some(SKILL_PANEL_MANAGER),
            function::PANEL_ICON,
            &[Slot::Word(0)],
            &[panel],
            false,
        )
        .map_or(0, |a| a.outs[0])
    }

    /// A skill panel: its name, icon and nodes (the manager fills a new table of nodes by skill;
    /// each node is copied from its record).
    // D2-01531, D2-01526, D2-01528
    fn skill_panel(&mut self, panel: u32) -> SkillPanel {
        let name = self.panel_name(panel);
        let icon = self.panel_icon(panel);
        let a = self.ask_with(
            Some(SKILL_PANEL_MANAGER),
            function::PANEL_NODES,
            &[Slot::New(NUMBER_TABLE)],
            &[panel],
            false,
        );
        let mut nodes = Vec::new();
        if let Some(NativeObject::Hash(h)) = a.and_then(|a| a.objects.into_iter().next().flatten())
        {
            for (_, v) in h.entries() {
                let r = match v {
                    Item::Ref(r) | Item::HashedRef { id: r, .. } => *r,
                    _ => continue,
                };
                if let Ok(NativeObject::Packed { bytes, .. }) = self.s.vm.heap.native(r) {
                    if let Ok(n) = SkillNode::unpack(bytes) {
                        nodes.push(n);
                    }
                }
            }
        }
        SkillPanel {
            id: panel,
            name,
            icon,
            nodes,
        }
    }
}
