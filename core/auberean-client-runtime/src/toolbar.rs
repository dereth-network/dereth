//! The toolbar's shortcuts as the shortcut bar shows them, asked of the player's scripts.
//!
//! The shortcut bar asks the player's scripts, through the interface, for what it shows: every
//! slot's shortcut (copies of the ones in the player's options), each slot's tooltip and icon
//! (a skill's name and icon, a recipe's, the alias string for a line of text, the empty slot's
//! string), which set of ten the bar shows and whether it numbers its slots, the bar's height in
//! rows, whether the shortcuts are locked and whether the bar scrolls at once; for each skill a
//! slot holds, whether the player has it trained, whether what the player carries lets it be used
//! and how long until it may be used again; for each item, whether the player has it, and the
//! item's name and icon (asked of the item's scripts) while it is here.
//!
//! The client asks again when the scripts say something the bar hears changed (the shortcuts
//! reloaded, a recipe gone, a skill or the reset times changed, the inventory or a container
//! changed, the options reloaded), when objects came or went, and when the player put a shortcut
//! in a slot or showed another set; [`Toolbar::version`] counts the changes.

use auberean_script::native::{Status, Stop};
use auberean_script::native_object::{native_type, NativeObject};
use auberean_script::options::{kind, Shortcut};
use auberean_script::value::tag;

use crate::host::{CallResult, InterfaceArg, Scripts, BUDGET};

/// The interface's functions the bar asks, by id (each found in the player's package).
pub mod function {
    /// A slot's tooltip and icon (the slot, a string reference to fill, an icon to fill).
    pub const INQ_SHORTCUT_INFO: u32 = 0x00B4_006D;
    /// Whether the player has a skill trained (its enumerated value): 1 or 0.
    pub const IS_SKILL_TRAINED: u32 = 0x00B4_0021;
    /// How long a skill takes to be ready again and how long is left (the skill, two floats to
    /// fill): 1 when it is waiting.
    pub const INQ_SKILL_TIME_LEFT: u32 = 0x00B4_002F;
    /// Whether what the player carries lets a skill be used: 1 or 0.
    pub const IS_SKILL_USABLE: u32 = 0x00B4_0031;
    /// Whether the shortcuts are locked against dragging.
    pub const GET_SHORTCUT_LOCKED: u32 = 0x00B4_0102;
    /// Whether the bar moves between sets at once.
    pub const GET_SHORTCUT_INSTA_SCROLL: u32 = 0x00B4_0104;
    /// Which set of ten the bar shows (from 1), and showing one.
    pub const GET_WHICH_SHORTCUT_SET: u32 = 0x00B4_0072;
    pub const SET_WHICH_SHORTCUT_SET: u32 = 0x00B4_0071;
    /// Whether the bar numbers its slots and its sets.
    pub const GET_SHOW_SHORTCUT_NUMBERS: u32 = 0x00B4_0074;
    /// The bar's height in rows.
    pub const GET_SHORTCUT_HEIGHT: u32 = 0x00B4_0111;
}

/// The panel notices after which the bar asks again: the shortcuts reloaded, a recipe removed,
/// a skill or the reset times changed, the options reloaded, the inventory or a container
/// changed.
pub const NOTICED: [u32; 7] = [
    0x008F << 16 | 0x0001,
    0x008F << 16 | 0x0002,
    0x0085 << 16 | 0x0006,
    0x0085 << 16 | 0x0007,
    0x0085 << 16 | 0x0009,
    0x008B << 16 | 0x0001,
    0x008B << 16 | 0x0002,
];

/// The slots the bar asks a tooltip and an icon for (slots from here on are never used).
pub const SLOTS: u32 = 100;

/// A slot's tooltip and icon, as the scripts gave them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Slot {
    /// The interface's answer (1 when it had something to say).
    pub answer: u32,
    pub tooltip: String,
    pub icon: u32,
}

/// What the scripts say of a skill a slot holds.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Skill {
    pub skill: u32,
    pub trained: bool,
    pub usable: bool,
    /// When it waits to be ready: the whole wait and what is left of it, in seconds.
    pub waiting: Option<(f32, f32)>,
}

/// What the client knows of an item a slot holds: whether the player has it, and its name (for
/// as many as there are) and icon while it is here.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Item {
    pub item: u64,
    pub have: bool,
    pub here: Option<(String, u32)>,
}

/// The toolbar's shortcuts as the bar shows them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Toolbar {
    /// Counts the changes.
    pub version: u64,
    /// Every slot's shortcut.
    pub shortcuts: Vec<Shortcut>,
    /// The first [`SLOTS`] slots' tooltips and icons.
    pub slots: Vec<Slot>,
    pub set: u32,
    pub show_numbers: bool,
    pub height: i32,
    pub locked: bool,
    pub insta_scroll: bool,
    pub skills: Vec<Skill>,
    pub items: Vec<Item>,
}

/// An out argument's storage: a word, or a new native object of a type.
#[derive(Clone, Copy, Debug)]
enum Out {
    Word(u32),
    Native(u32),
}

impl Scripts {
    /// Call one of the interface's functions with out arguments, as the client's interface glue
    /// does: a new context of the player's process holding each out argument's storage in a slot,
    /// then the result slot, a reference to each storage slot, the plain arguments in order (a
    /// word, or a long as two), and the player's instance. Returns how it went, the result and
    /// what each storage slot then holds. `None` when there is no player here.
    fn call_with_outs(
        &mut self,
        function: u32,
        outs: &[Out],
        args: &[InterfaceArg],
    ) -> Option<(CallResult, u32, Vec<u32>)> {
        let player = self
            .host
            .model
            .player
            .filter(|p| self.host.model.object(*p).is_some())?;
        self.vm.budget = BUDGET;
        let mut result = 0;
        let mut held = Vec::new();
        let outcome = self.process_of(player).and_then(|p| {
            let mut cx = self.vm.new_context(p);
            let mut at = Vec::new();
            for o in outs {
                at.push(cx.sp);
                match *o {
                    Out::Word(v) => cx.push(v, tag::MIXED)?,
                    Out::Native(ty) => {
                        let fresh = NativeObject::new(&self.host.program.library, ty);
                        let r = self.vm.heap.add_native(ty, fresh);
                        cx.push(r, tag::REFERENCE)?;
                    }
                }
            }
            let slot = cx.sp;
            cx.push(0, tag::INT)?;
            for s in &at {
                #[allow(clippy::cast_possible_truncation)]
                let r = self
                    .vm
                    .heap
                    .add_relative(auberean_script::value::NULL, *s as u32);
                cx.push(r, tag::REFERENCE)?;
            }
            for a in args {
                match a {
                    InterfaceArg::Word(v) => cx.push(*v, tag::MIXED)?,
                    InterfaceArg::Long(v) => cx.push_long(*v, tag::LONG | tag::MIXED)?,
                    _ => return Err(Stop::Host("only words and longs here".into())),
                }
            }
            let receiver = self.vm.instance_of(p);
            cx.push(receiver, tag::REFERENCE)?;
            let prog = std::sync::Arc::clone(&self.host.program);
            let package = self.vm.package_of(&mut self.host, receiver, &prog)?;
            let status = self.vm.call(
                &mut self.host,
                &mut cx,
                package,
                auberean_script::value::FunctionId(function),
            )?;
            result = cx.slot(slot).map_or(0, |(v, _)| v);
            for s in &at {
                held.push(cx.slot(*s).map_or(0, |(v, _)| v));
            }
            Ok(status)
        });
        let r = match outcome {
            Ok(Status::Terminated) => CallResult::Completed,
            Ok(s) => CallResult::Ended(s),
            Err(Stop::NotPorted { native, .. }) => CallResult::NotPorted(native),
            Err(Stop::NullReference) => CallResult::NullReference,
            Err(e) => CallResult::Stopped(e.to_string()),
        };
        self.vm.heap.abandon_builds();
        self.vm.stopped_at = None;
        Some((r, result, held))
    }

    /// A slot's tooltip and icon: the interface's function fills a string reference and an icon
    /// (the icon starts at 0, the tooltip empty); the tooltip written as the client writes it.
    // D2-00532, D2-01178
    pub fn shortcut_info(&mut self, slot: u32) -> Option<Slot> {
        let outs = [Out::Word(0), Out::Native(native_type::STRING_INFO)];
        let (_, answer, held) = self.call_with_outs(
            function::INQ_SHORTCUT_INFO,
            &outs,
            &[InterfaceArg::Word(slot)],
        )?;
        let icon = held.first().copied().unwrap_or(0);
        let tooltip = held
            .get(1)
            .and_then(|r| match self.vm.heap.native(*r) {
                Ok(NativeObject::TextInfo(t)) => Some(t.clone()),
                _ => None,
            })
            .map(|t| crate::text::from_script(&t).text(&mut self.host.text))
            .unwrap_or_default();
        Some(Slot {
            answer,
            tooltip,
            icon,
        })
    }

    /// One of the interface's functions that answers a word.
    fn answer(&mut self, function: u32, args: &[InterfaceArg]) -> u32 {
        self.call_with_outs(function, &[], args)
            .map_or(0, |(_, a, _)| a)
    }

    /// What the scripts say of a skill: trained, usable with what the player carries, and how
    /// long until it is ready (the whole wait, then what is left, filled as floats).
    // D2-00494, D2-01189, D2-00509, D2-01190, D2-00508, D2-01181
    pub fn skill_state(&mut self, skill: u32) -> Skill {
        let s = InterfaceArg::Word(skill);
        let trained = self.answer(function::IS_SKILL_TRAINED, std::slice::from_ref(&s)) != 0;
        let usable = self.answer(function::IS_SKILL_USABLE, std::slice::from_ref(&s)) != 0;
        let outs = [Out::Word(0), Out::Word(0)];
        let waiting = self
            .call_with_outs(function::INQ_SKILL_TIME_LEFT, &outs, &[s])
            .filter(|(_, a, _)| *a != 0)
            .map(|(_, _, held)| {
                let f = |i: usize| f32::from_bits(held.get(i).copied().unwrap_or(0));
                (f(1), f(0))
            });
        Skill {
            skill,
            trained,
            usable,
            waiting,
        }
    }

    /// The toolbar as the bar shows it, asked of the scripts; `None` when there is no player
    /// here.
    // D2-00536, D2-01151, D2-00538, D2-01145, D2-00552, D2-01135, D2-00540, D2-01137
    // D2-00542, D2-01136, D2-00478, D2-01096
    pub fn toolbar(&mut self) -> Option<Toolbar> {
        let (_, shortcuts) = self.shortcuts()?;
        let mut slots = Vec::new();
        for i in 0..SLOTS {
            slots.push(self.shortcut_info(i).unwrap_or_default());
        }
        let set = self.answer(function::GET_WHICH_SHORTCUT_SET, &[]);
        let show_numbers = self.answer(function::GET_SHOW_SHORTCUT_NUMBERS, &[]) != 0;
        let height = self
            .call_with_outs(function::GET_SHORTCUT_HEIGHT, &[], &[])
            .map_or(1, |(_, a, _)| a);
        let locked = self.answer(function::GET_SHORTCUT_LOCKED, &[]) != 0;
        let insta_scroll = self.answer(function::GET_SHORTCUT_INSTA_SCROLL, &[]) != 0;
        let mut skills: Vec<Skill> = Vec::new();
        let mut items: Vec<Item> = Vec::new();
        for s in &shortcuts {
            match s.kind {
                kind::SKILL if !skills.iter().any(|k| k.skill == s.data_enum) => {
                    let k = self.skill_state(s.data_enum);
                    skills.push(k);
                }
                kind::ITEM if !items.iter().any(|i| i.item == s.item) => {
                    let have = self.answer(
                        crate::interface::function::DO_I_HAVE_ITEM,
                        &[InterfaceArg::Long(s.item)],
                    ) != 0;
                    let here = self.item_look(s.item);
                    items.push(Item {
                        item: s.item,
                        have,
                        here,
                    });
                }
                _ => {}
            }
        }
        #[allow(clippy::cast_possible_wrap)]
        Some(Toolbar {
            version: 0,
            shortcuts,
            slots,
            set,
            show_numbers,
            height: height as i32,
            locked,
            insta_scroll,
            skills,
            items,
        })
    }

    /// An item's name (for as many as there are) and icon as its scripts give them, the same
    /// as the inventory shows it, while it is here.
    fn item_look(&mut self, id: u64) -> Option<(String, u32)> {
        use crate::inventory::function;
        self.host.model.object(id)?;
        let quantity = self.number(id, function::QUANTITY, &[]);
        let name = self.name_for(id, quantity);
        let icon = self.number(id, function::ICON, &[]);
        Some((name, icon))
    }
}
