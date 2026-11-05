//! The inventory as the game's panels see it and change it, through the scripts: the player's pack
//! and what it wears, the containers the player has open, and each item in them.
//!
//! - **Asking an object's scripts** ([`Scripts::call_on`]): one of the interface's functions,
//!   called on any object's instance the way the client's interface calls it. The values the
//!   function writes into come first (each in a slot of its own: a number, or a new script object
//!   such as an empty list), then the result slot, then the arguments (numbers, 64-bit ids,
//!   references to those first slots) and the instance; afterwards the slots are read back.
//! - **What the panels show** ([`Inventory`]): asked of the scripts again whenever they said
//!   something about the inventory (the inventory, container, item, pack, money and container
//!   notices) or objects came or went: for the player and each container the scripts opened, how
//!   many items it holds and what it holds, segment by segment (the pack is shown as four packs,
//!   each a segment with its first slot and its size); for each item in them and each item worn,
//!   its quantity, icon, whether it stacks and how high, its name for that quantity, its wear, the
//!   place it would be worn, and whether the player has it and has it in the pack; the player's
//!   worn items by place, money, the most coins one stack holds, and whether melting items into
//!   money is to be confirmed.
//! - **What the panels ask** (moving an item to a slot of a container, wearing it, taking it off,
//!   melting it into money, taking coins out of the money bag, taking everything out of a
//!   container, melting everything in it, closing it): each is one function of the player's
//!   scripts ([`crate::Client::act`]).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use auberean_script::library::STATE_BASE;
use auberean_script::native::{Status, Stop};
use auberean_script::native_object::{Item, NativeObject};
use auberean_script::value::{tag, FunctionId, NULL};

use crate::host::{CallResult, Scripts, BUDGET};

/// The interface's functions the inventory panels call, by id: each is found in the package of
/// the instance it is called on.
pub mod function {
    /// Move an item: the item, the container it comes from, the slot it had, the container it
    /// goes to, its new slot, the quantity moved.
    pub const MOVE_ITEM: u32 = 0x00B4_000C;
    /// Wear an item at a place.
    pub const EQUIP_ITEM: u32 = 0x00B4_000D;
    /// Take a worn item off into a slot of the pack.
    pub const UNEQUIP_ITEM: u32 = 0x00B4_000E;
    /// The place an item is worn at by preference.
    pub const PREFERRED_LOCATION: u32 = 0x00B4_0011;
    /// The worn items by place.
    pub const WORN_BY_LOCATION: u32 = 0x00B4_0014;
    /// Whether the player has an item.
    pub const HAS_ITEM: u32 = 0x00B4_0015;
    /// Melt an item into money, and take coins out of the money bag.
    pub const INTO_MONEY: u32 = 0x00B4_0016;
    pub const OUT_OF_MONEY: u32 = 0x00B4_0017;
    /// An item's icon.
    pub const ICON: u32 = 0x00B4_001C;
    /// How many items a container holds at most.
    pub const CONTAINER_SIZE: u32 = 0x00B4_001D;
    /// Whether an item stacks, its quantity, how high it stacks.
    pub const STACKABLE: u32 = 0x00B4_0063;
    pub const QUANTITY: u32 = 0x00B4_0064;
    pub const STACK_SIZE: u32 = 0x00B4_0065;
    /// A container opened or closed for an object.
    pub const UPDATE_OPEN: u32 = 0x00B4_006C;
    /// An item's name for a quantity.
    pub const PLURAL_NAME: u32 = 0x00B4_00C7;
    /// What a container holds in a segment: its items, the segment's first slot and size.
    pub const CONTENTS: u32 = 0x00B4_00E7;
    /// A segment of the pack: its first slot and size.
    pub const SEGMENT: u32 = 0x00B4_00E8;
    /// Whether the player wears an item.
    pub const WORN: u32 = 0x00B4_00EA;
    /// Take everything out of a container; melt everything in it into money.
    pub const LOOT_ALL: u32 = 0x00B4_00EB;
    pub const MELT_ALL: u32 = 0x00B4_00EC;
    /// Whether an item wears, how worn it is, its greatest wear.
    pub const HAS_WEAR: u32 = 0x00B4_00ED;
    pub const MAX_WEAR: u32 = 0x00B4_00EE;
    pub const WEAR: u32 = 0x00B4_00EF;
    /// Whether melting items into money goes without asking.
    pub const NO_MELT_QUESTION: u32 = 0x00B4_0107;
    /// The player's selection.
    pub const SELECTION: u32 = 0x00B4_0003;
    /// The most coins one stack holds (the inventory's shared functions).
    pub const COIN_STACK: u32 = 0x00A6 << 16 | 0x0001;
}

/// The package of the inventory's shared functions, by name.
pub const INVENTORY_GLOBALS: &str = "InventoryGlobals";

/// The script object type of a list of 64-bit ids, and of a table of places to ids.
pub const ID_LIST: u32 = 0x3B;
pub const ID_TABLE: u32 = 8;

/// The player's money (a whole-number quality).
pub const MONEY: u32 = 0xC1C;

/// An item's kind bits (a whole-number quality), and the bits of the kinds that melt into money
/// without a question.
pub const ITEM_KIND: u32 = 0x14;
pub const MELTS_AT_ONCE: i32 = 0xF_0081;

/// How many segments the pack is shown in (the main pack and three more).
pub const PACK_SEGMENTS: u32 = 4;

/// The packages of the scripts' notices about the inventory and money.
pub const INVENTORY_NOTICES: u32 = 0x008B;
pub const MONEY_NOTICES: u32 = 0x008C;
/// A container opened or closed for the player (its state a byte: 0 closed).
pub const CONTAINER_STATE_UPDATE: u32 = INVENTORY_NOTICES << 16 | 3;
/// The player's selection changed (the scripts' notice to the panels).
pub const SELECTION_CHANGED: u32 = 0x0085 << 16 | 3;

/// A slot the called function may write into, pushed before the result slot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Out {
    /// A number.
    Word(u32),
    /// A new, empty script object of a type.
    New(u32),
}

/// An argument, pushed after the result slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arg {
    /// A 32-bit value.
    Word(u32),
    /// A 64-bit value (an instance id).
    Long(u64),
    /// A reference to the slot of the `n`th [`Out`].
    Out(usize),
}

/// What a call on an object's scripts left: how it went, the result slot, each [`Out`] slot's
/// value, and the script object each holds (when it holds one).
#[derive(Clone, Debug, PartialEq)]
pub struct Answer {
    pub result: CallResult,
    /// The result slot (its low word, for a 64-bit result; the high word next).
    pub value: u32,
    pub value_high: u32,
    pub outs: Vec<u32>,
    pub objects: Vec<Option<NativeObject>>,
    /// The object the result slot refers to, when it refers to one.
    pub value_object: Option<NativeObject>,
}

impl Scripts {
    /// Call one of the interface's functions on an object's instance, as the client's interface
    /// glue does: a new context of the object's process; the slots the function writes into; the
    /// result slot; the arguments in the order given; the instance; then the function, looked up
    /// in the instance's package. `None` when the object is not here.
    pub fn call_on(
        &mut self,
        object: u64,
        function: u32,
        outs: &[Out],
        args: &[Arg],
    ) -> Option<Answer> {
        self.host.model.object(object)?;
        let p = self.process_of(object).ok()?;
        let receiver = self.vm.instance_of(p);
        Some(self.call_with(p, receiver, function, outs, args, false))
    }

    /// Call a function of an object's instance whose result is 64 bits (an instance id), with no
    /// arguments. `None` when the object is not here.
    pub fn call_on_long(&mut self, object: u64, function: u32) -> Option<u64> {
        self.host.model.object(object)?;
        let p = self.process_of(object).ok()?;
        let receiver = self.vm.instance_of(p);
        let a = self.call_with(p, receiver, function, &[], &[], true);
        Some(u64::from(a.value) | u64::from(a.value_high) << 32)
    }

    /// Call a function of a package's shared instance (its singleton), from a context of the
    /// player's process. `None` when there is no player here or no such package.
    pub fn call_shared(&mut self, package: &str, function: u32) -> Option<Answer> {
        let player = self
            .host
            .model
            .player
            .filter(|p| self.host.model.object(*p).is_some())?;
        let id = self.host.program.library.package_id(package)?;
        if id == NULL {
            return None;
        }
        let p = self.process_of(player).ok()?;
        let receiver = self.vm.heap.singleton(STATE_BASE | (id & 0xFFFF));
        Some(self.call_with(p, receiver, function, &[], &[], false))
    }

    fn call_with(
        &mut self,
        p: usize,
        receiver: u32,
        function: u32,
        outs: &[Out],
        args: &[Arg],
        long_result: bool,
    ) -> Answer {
        self.vm.budget = BUDGET;
        let mut answer = Answer {
            result: CallResult::Completed,
            value: 0,
            value_high: 0,
            outs: vec![0; outs.len()],
            objects: vec![None; outs.len()],
            value_object: None,
        };
        let outcome = (|| -> Result<Status, Stop> {
            let mut cx = self.vm.new_context(p);
            let mut at = Vec::with_capacity(outs.len());
            for o in outs {
                at.push(cx.sp);
                match o {
                    Out::Word(v) => cx.push(*v, tag::MIXED)?,
                    Out::New(ty) => {
                        let fresh = NativeObject::new(&self.host.program.library, *ty);
                        let r = self.vm.heap.add_native(*ty, fresh);
                        cx.push(r, tag::REFERENCE)?;
                    }
                }
            }
            let result = cx.sp;
            if long_result {
                cx.push_long(0, tag::LONG_INT)?;
            } else {
                cx.push(0, tag::INT)?;
            }
            for a in args {
                match a {
                    Arg::Word(v) => cx.push(*v, tag::MIXED)?,
                    Arg::Long(v) => cx.push_long(*v, tag::LONG | tag::MIXED)?,
                    Arg::Out(k) => {
                        let slot = at.get(*k).copied().unwrap_or(result);
                        #[allow(clippy::cast_possible_truncation)]
                        let r = self.vm.heap.add_relative(NULL, slot as u32);
                        cx.push(r, tag::REFERENCE)?;
                    }
                }
            }
            cx.push(receiver, tag::REFERENCE)?;
            let prog = Arc::clone(&self.host.program);
            let package = self.vm.package_of(&mut self.host, receiver, &prog)?;
            let status = self
                .vm
                .call(&mut self.host, &mut cx, package, FunctionId(function))?;
            let (v, t) = cx.slot(result)?;
            answer.value = v;
            if long_result {
                answer.value_high = cx.slot(result + 1)?.0;
            }
            if t == tag::REFERENCE {
                answer.value_object = self.vm.heap.native(v).ok().cloned();
            }
            for (k, slot) in at.iter().enumerate() {
                let (v, t) = cx.slot(*slot)?;
                answer.outs[k] = v;
                if t == tag::REFERENCE {
                    answer.objects[k] = self.vm.heap.native(v).ok().cloned();
                }
            }
            if status == Status::Interrupted {
                self.vm.park(cx);
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
        self.vm.heap.abandon_builds();
        self.vm.stopped_at = None;
        answer
    }

    /// A function of an object's scripts answering one number.
    pub(crate) fn number(&mut self, object: u64, function: u32, args: &[Arg]) -> u32 {
        self.call_on(object, function, &[], args)
            .map_or(0, |a| a.value)
    }

    /// What a container holds in a segment: its items, the segment's first slot and how many
    /// slots it has.
    // D2-00473, D2-01115, D2-01017
    pub fn contents(&mut self, container: u64, segment: u32) -> Contents {
        let outs = [Out::Word(0), Out::Word(0), Out::New(ID_LIST)];
        let args = [Arg::Out(0), Arg::Out(1), Arg::Out(2), Arg::Word(segment)];
        let Some(a) = self.call_on(container, function::CONTENTS, &outs, &args) else {
            return Contents::default();
        };
        Contents {
            items: a
                .objects
                .get(2)
                .cloned()
                .flatten()
                .map_or_else(Vec::new, |o| ids(&o)),
            first: a.outs[1],
            count: a.outs[0],
        }
    }

    /// A segment of a container (the player's pack): its first slot and how many slots it has.
    // D2-00474, D2-01114
    pub fn segment(&mut self, container: u64, segment: u32) -> (u32, u32) {
        let outs = [Out::Word(0), Out::Word(0)];
        let args = [Arg::Out(0), Arg::Out(1), Arg::Word(segment)];
        self.call_on(container, function::SEGMENT, &outs, &args)
            .map_or((0, 0), |a| (a.outs[1], a.outs[0]))
    }

    /// The items the player wears, by place.
    // D2-00476, D2-01179, D2-01018
    pub fn worn(&mut self, player: u64) -> Vec<(u32, u64)> {
        let outs = [Out::New(ID_TABLE)];
        let Some(a) = self.call_on(player, function::WORN_BY_LOCATION, &outs, &[Arg::Out(0)])
        else {
            return Vec::new();
        };
        let mut worn = Vec::new();
        if let Some(Some(NativeObject::Hash(h))) = a.objects.first() {
            for (k, v) in h.entries() {
                if let (Some(loc), Some(id)) = (word(k), long(v)) {
                    worn.push((loc, id));
                }
            }
        }
        worn.sort_unstable();
        worn
    }

    /// An item's name for a quantity, as its scripts write it (empty when they give none).
    pub(crate) fn name_for(&mut self, id: u64, quantity: u32) -> String {
        self.call_on(id, function::PLURAL_NAME, &[], &[Arg::Word(quantity)])
            .and_then(|a| match a.value_object {
                Some(NativeObject::TextInfo(t)) => Some(t),
                _ => None,
            })
            .map(|t| crate::text::from_script(&t).text(&mut self.host.text))
            .unwrap_or_default()
    }

    /// What the panels show of one item.
    // D2-00487, D2-01122, D2-00520, D2-01150, D2-00521, D2-01132, D2-00522, D2-01128
    // D2-00417, D2-01130, D2-00484, D2-01154, D2-00485, D2-01117, D2-00486, D2-01126
    // D2-00475, D2-01131, D2-00478, D2-01096, D2-00479, D2-01186
    pub fn item(&mut self, player: u64, id: u64) -> Option<ItemFacts> {
        self.host.model.object(id)?;
        let quantity = self.number(id, function::QUANTITY, &[]);
        let name = self.name_for(id, quantity);
        let wear = (self.number(id, function::HAS_WEAR, &[]) != 0).then(|| {
            (
                self.number(id, function::WEAR, &[]),
                self.number(id, function::MAX_WEAR, &[]),
            )
        });
        Some(ItemFacts {
            quantity,
            icon: self.number(id, function::ICON, &[]),
            stackable: self.number(id, function::STACKABLE, &[]) != 0,
            stack_size: self.number(id, function::STACK_SIZE, &[]),
            name,
            wear,
            preferred_location: self.number(id, function::PREFERRED_LOCATION, &[]),
            had: self.number(player, function::HAS_ITEM, &[Arg::Long(id)]) != 0,
            worn: self.number(player, function::WORN, &[Arg::Long(id)]) != 0,
            melts_at_once: self
                .host
                .int_quality(id, ITEM_KIND)
                .is_some_and(|k| k & MELTS_AT_ONCE == MELTS_AT_ONCE),
            offered_name: None,
        })
    }

    /// Everything the inventory panels show: the player's pack in its segments and what it wears,
    /// the containers in `open`, and each item in them and in `also` (items a trade offers, by the
    /// number offered: its name is written for that number too, and those of the partner's are
    /// still in the partner's pack), and the player's selection (dragged from the world by its
    /// icon).
    // D2-00488, D2-01125, D2-00554, D2-01112, D2-01493
    pub fn inventory(&mut self, open: &BTreeSet<u64>, also: &BTreeMap<u64, u32>) -> Inventory {
        let Some(player) = self
            .host
            .model
            .player
            .filter(|p| self.host.model.object(*p).is_some())
        else {
            return Inventory::default();
        };
        let mut inv = Inventory {
            player,
            ..Inventory::default()
        };
        let mut shown = Vec::new();
        let containers =
            std::iter::once(player).chain(open.iter().copied().filter(|c| *c != player));
        for c in containers.collect::<Vec<_>>() {
            if self.host.model.object(c).is_none() {
                continue;
            }
            let segments = if c == player { PACK_SEGMENTS } else { 1 };
            let mut held = Container {
                size: self.number(c, function::CONTAINER_SIZE, &[]),
                segments: Vec::new(),
                slots: Vec::new(),
            };
            for s in 0..segments {
                let contents = self.contents(c, s);
                shown.extend(contents.items.iter().copied());
                held.segments.push(contents);
                held.slots.push(self.segment(c, s));
            }
            inv.containers.insert(c, held);
        }
        inv.segments = inv
            .containers
            .get(&player)
            .map(|c| c.slots.clone())
            .unwrap_or_default();
        // D2-00420, D2-01134
        inv.selection = self.call_on_long(player, function::SELECTION).unwrap_or(0);
        // The selection, held anywhere in the world: pulled on in the world, its icon is dragged
        // to the pack.
        if inv.selection != player && self.host.model.object(inv.selection).is_some() {
            shown.push(inv.selection);
        }
        inv.worn = self.worn(player);
        shown.extend(inv.worn.iter().map(|(_, id)| *id));
        shown.extend(
            also.keys()
                .filter(|id| self.host.model.object(**id).is_some()),
        );
        for id in shown {
            if let std::collections::btree_map::Entry::Vacant(e) = inv.items.entry(id) {
                if let Some(f) = self.item(player, id) {
                    e.insert(f);
                }
            }
        }
        for (id, offered) in also {
            if inv.items.contains_key(id) {
                let name = self.name_for(*id, *offered);
                if let Some(f) = inv.items.get_mut(id) {
                    f.offered_name = Some(name);
                }
            }
        }
        inv.money = self.host.int_quality(player, MONEY).unwrap_or(0);
        inv.coin_stack = self
            .call_shared(INVENTORY_GLOBALS, function::COIN_STACK)
            .map_or(0, |a| a.value);
        inv.no_melt_question = self.number(player, function::NO_MELT_QUESTION, &[]) != 0;
        inv.open = open.clone();
        inv
    }
}

/// The ids a script list (or set) holds, in order.
fn ids(o: &NativeObject) -> Vec<u64> {
    match o {
        NativeObject::Sequence { items, .. } => items.iter().filter_map(long).collect(),
        _ => Vec::new(),
    }
}

fn long(i: &Item) -> Option<u64> {
    match i {
        Item::Long(v) => Some(*v),
        Item::Word(v) => Some(u64::from(*v)),
        _ => None,
    }
}

fn word(i: &Item) -> Option<u32> {
    match i {
        Item::Word(v) => Some(*v),
        #[allow(clippy::cast_possible_truncation)]
        Item::Long(v) => Some(*v as u32),
        _ => None,
    }
}

/// What a container holds in one segment.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Contents {
    /// Every item it holds, in order.
    pub items: Vec<u64>,
    /// The segment's first slot and how many slots it shows.
    pub first: u32,
    pub count: u32,
}

/// A container: how many items it holds at most, and what it holds, segment by segment.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Container {
    pub size: u32,
    pub segments: Vec<Contents>,
    /// Each segment's first slot and size, as the container says.
    pub slots: Vec<(u32, u32)>,
}

/// What the panels show of an item.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ItemFacts {
    pub quantity: u32,
    /// Its icon (a picture's data id).
    pub icon: u32,
    pub stackable: bool,
    /// How high it stacks.
    pub stack_size: u32,
    /// Its name for its quantity.
    pub name: String,
    /// Its wear and greatest wear, when it wears.
    pub wear: Option<(u32, u32)>,
    /// The place it is worn at by preference (0 for none).
    pub preferred_location: u32,
    /// The player has it; wears it.
    pub had: bool,
    pub worn: bool,
    /// Dropped on the money bag it melts without a question.
    pub melts_at_once: bool,
    /// A trade offers it: its name for the number offered.
    pub offered_name: Option<String>,
}

/// The inventory as the panels show it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Inventory {
    /// Counts the times it changed.
    pub version: u64,
    pub player: u64,
    /// The player and the containers it has open.
    pub containers: BTreeMap<u64, Container>,
    /// The player's pack segments: first slot and size, by segment.
    pub segments: Vec<(u32, u32)>,
    /// The worn items by place.
    pub worn: Vec<(u32, u64)>,
    /// Every item shown.
    pub items: BTreeMap<u64, ItemFacts>,
    pub money: i32,
    /// The most coins one stack holds.
    pub coin_stack: u32,
    /// Melting items into money goes without asking.
    pub no_melt_question: bool,
    /// The containers the scripts opened for the player.
    pub open: BTreeSet<u64>,
    /// The player's selection when it was last asked.
    pub selection: u64,
    /// The last name asked for a number of an item: the item, the number, the name.
    pub amount_name: Option<(u64, u32, String)>,
}

impl Inventory {
    /// The notices that change what the inventory panels show (the selection's among them).
    #[must_use]
    pub fn noticed(native: u32) -> bool {
        let package = native >> 16;
        package == INVENTORY_NOTICES || package == MONEY_NOTICES || native == SELECTION_CHANGED
    }
}
