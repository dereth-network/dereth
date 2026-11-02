//! Skill panel nodes, as scripts fill them for the skill window: one node of a race's skill tree.
//!
//! A node holds its skill's enumerated value, its type, its name and description (string
//! references), its icon, its place on the panel (x and y), the nodes it hangs from (its parents)
//! and the skills it needs (its dependencies), and three flags: passive, untrainable, a perk. A new
//! or cleared node is all zero and empty but for its place, which is -1 on both axes.
//!
//! The script machine holds a node as its packed form: the enumerated value, the type, the name,
//! the description, the icon, x, y, the parents and the dependencies (each a count and the
//! words), then the three flags.
//!
//! The natives read and set each part; a name or description is read back as a new string
//! reference holding a copy, and set as a copy of the one given; the place is read into two out
//! references; a parent or dependency already listed is not added again (adding a dependency
//! answers whether it was added); the parents and dependencies are read back as a new list of
//! words. A node is set from another whole, or cleared. A null receiver crashes the call; a null
//! node or string reference argument stops it.

use std::sync::Arc;

use auberean_script::library::Elem;
use auberean_script::native::{Call, NativeFn, NativeTable, Status, Stop};
use auberean_script::native_object::{native_type, Item, NativeObject, TextInfo};
use auberean_script::value::{tag, NULL};
use auberean_script::wire::{In, Out};

const SKILL_UI_NODE: u32 = 0xAE;
/// The native type of a list of words.
const WORD_LIST: u32 = 10;

/// One node of a skill panel. Dropping one lets go of its texts and lists.
// D2-01525
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkillNode {
    pub skill: u32,
    pub kind: u32,
    pub name: TextInfo,
    pub description: TextInfo,
    pub icon: u32,
    pub x: u32,
    pub y: u32,
    pub parents: Vec<u32>,
    pub dependencies: Vec<u32>,
    pub passive: u32,
    pub untrainable: u32,
    pub perk: u32,
}

impl Default for SkillNode {
    /// A new node: nothing set, its place -1 on both axes.
    // D2-12160
    fn default() -> Self {
        Self {
            skill: 0,
            kind: 0,
            name: TextInfo::default(),
            description: TextInfo::default(),
            icon: 0,
            x: u32::MAX,
            y: u32::MAX,
            parents: Vec::new(),
            dependencies: Vec::new(),
            passive: 0,
            untrainable: 0,
            perk: 0,
        }
    }
}

impl SkillNode {
    /// Back to a new node.
    // D2-12157
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// A parent added at the end, unless it is listed.
    // D2-12158
    pub fn add_parent(&mut self, parent: u32) {
        if !self.parents.contains(&parent) {
            self.parents.push(parent);
        }
    }

    /// A dependency added at the end, unless it is listed; whether it was added.
    // D2-12159
    pub fn add_dependency(&mut self, skill: u32) -> bool {
        if self.dependencies.contains(&skill) {
            return false;
        }
        self.dependencies.push(skill);
        true
    }

    /// Read a packed node.
    ///
    /// # Errors
    /// A node that does not read.
    // D2-12156
    pub fn read(r: &mut In<'_>) -> auberean_script::wire::Result<Self> {
        let mut n = Self {
            skill: r.u32()?,
            kind: r.u32()?,
            name: TextInfo::unpack(r)?,
            description: TextInfo::unpack(r)?,
            icon: r.u32()?,
            x: r.u32()?,
            y: r.u32()?,
            ..Self::default()
        };
        for list in [&mut n.parents, &mut n.dependencies] {
            let k = r.u32()? as usize;
            let k = r.count(k, 4)?;
            for _ in 0..k {
                list.push(r.u32()?);
            }
        }
        n.passive = r.u32()?;
        n.untrainable = r.u32()?;
        n.perk = r.u32()?;
        Ok(n)
    }

    /// The packed form.
    // D2-12156
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn pack(&self) -> Vec<u8> {
        let mut w = Out::new();
        w.u32(self.skill);
        w.u32(self.kind);
        w.bytes(&self.name.pack());
        w.bytes(&self.description.pack());
        w.u32(self.icon);
        w.u32(self.x);
        w.u32(self.y);
        for list in [&self.parents, &self.dependencies] {
            w.u32(list.len() as u32);
            list.iter().for_each(|v| w.u32(*v));
        }
        w.u32(self.passive);
        w.u32(self.untrainable);
        w.u32(self.perk);
        w.buf
    }

    /// The node a native object holds (a new one for one never written).
    #[must_use]
    pub fn of(o: &NativeObject) -> Option<Self> {
        match o {
            NativeObject::Opaque { ty } if *ty == native_type::SKILL_UI_NODE => {
                Some(Self::default())
            }
            NativeObject::Packed { ty, bytes } if *ty == native_type::SKILL_UI_NODE => {
                Self::read(&mut In::new(bytes)).ok()
            }
            _ => None,
        }
    }
}

/// This module's natives: package, number, function.
const NATIVES: &[(u32, u32, NativeFn)] = &[
    (SKILL_UI_NODE, 0x01, get_skill),
    (SKILL_UI_NODE, 0x02, set_skill),
    (SKILL_UI_NODE, 0x03, get_kind),
    (SKILL_UI_NODE, 0x04, set_kind),
    (SKILL_UI_NODE, 0x05, get_name),
    (SKILL_UI_NODE, 0x06, set_name),
    (SKILL_UI_NODE, 0x07, get_icon),
    (SKILL_UI_NODE, 0x08, set_icon),
    (SKILL_UI_NODE, 0x09, get_description),
    (SKILL_UI_NODE, 0x0A, set_description),
    (SKILL_UI_NODE, 0x0B, inq_coordinates),
    (SKILL_UI_NODE, 0x0C, set_coordinates),
    (SKILL_UI_NODE, 0x0D, get_x),
    (SKILL_UI_NODE, 0x0E, get_y),
    (SKILL_UI_NODE, 0x0F, add_parent),
    (SKILL_UI_NODE, 0x10, parent_list),
    (SKILL_UI_NODE, 0x11, add_dependency),
    (SKILL_UI_NODE, 0x12, dependency_list),
    (SKILL_UI_NODE, 0x13, set_passive),
    (SKILL_UI_NODE, 0x14, get_passive),
    (SKILL_UI_NODE, 0x15, set_untrainable),
    (SKILL_UI_NODE, 0x16, get_untrainable),
    (SKILL_UI_NODE, 0x17, clear),
    (SKILL_UI_NODE, 0x18, set_whole),
    (SKILL_UI_NODE, 0x19, set_perk),
    (SKILL_UI_NODE, 0x1A, get_perk),
];

/// Bind this module's natives.
pub fn bind(t: &mut NativeTable) {
    crate::bind_table(t, NATIVES);
}

/// The node a reference names.
fn node_at(c: &Call<'_>, r: u32) -> Result<SkillNode, Stop> {
    if r == NULL {
        return Err(Stop::NullReference);
    }
    SkillNode::of(c.vm.heap.native(r)?).ok_or_else(|| c.fail("not a skill panel node"))
}

/// The node a reference names, changed by `f`, stored back packed.
fn change<T>(c: &mut Call<'_>, r: u32, f: impl FnOnce(&mut SkillNode) -> T) -> Result<T, Stop> {
    let mut n = node_at(c, r)?;
    let out = f(&mut n);
    *c.vm.heap.native_mut(r)? = NativeObject::Packed {
        ty: native_type::SKILL_UI_NODE,
        bytes: n.pack(),
    };
    Ok(out)
}

/// A word of the receiver's node into the slot below it.
fn get(c: &mut Call<'_>, f: fn(&SkillNode) -> u32) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let v = f(&node_at(c, r)?);
    c.write(-2, v, tag::MIXED)?;
    Ok(Status::Success)
}

/// The word argument (`fp[-2]`) stored in the receiver's node.
fn set(c: &mut Call<'_>, f: fn(&mut SkillNode, u32)) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let v = c.arg(0)?;
    change(c, r, |n| f(n, v))?;
    Ok(Status::Success)
}

// D2-18959
fn get_skill(c: &mut Call<'_>) -> Result<Status, Stop> {
    get(c, |n| n.skill)
}

// D2-01370
fn set_skill(c: &mut Call<'_>) -> Result<Status, Stop> {
    set(c, |n, v| n.skill = v)
}

// D2-08505
fn get_kind(c: &mut Call<'_>) -> Result<Status, Stop> {
    get(c, |n| n.kind)
}

// D2-01365
fn set_kind(c: &mut Call<'_>) -> Result<Status, Stop> {
    set(c, |n, v| n.kind = v)
}

// D2-01542
fn get_icon(c: &mut Call<'_>) -> Result<Status, Stop> {
    get(c, |n| n.icon)
}

// D2-01503
fn set_icon(c: &mut Call<'_>) -> Result<Status, Stop> {
    set(c, |n, v| n.icon = v)
}

// D2-01543
fn get_x(c: &mut Call<'_>) -> Result<Status, Stop> {
    get(c, |n| n.x)
}

// D2-08787
fn get_y(c: &mut Call<'_>) -> Result<Status, Stop> {
    get(c, |n| n.y)
}

// D2-01538
fn set_passive(c: &mut Call<'_>) -> Result<Status, Stop> {
    set(c, |n, v| n.passive = v)
}

// D2-08788
fn get_passive(c: &mut Call<'_>) -> Result<Status, Stop> {
    get(c, |n| n.passive)
}

// D2-01540
fn set_untrainable(c: &mut Call<'_>) -> Result<Status, Stop> {
    set(c, |n, v| n.untrainable = v)
}

// D2-01510
fn get_untrainable(c: &mut Call<'_>) -> Result<Status, Stop> {
    get(c, |n| n.untrainable)
}

// D2-01539
fn set_perk(c: &mut Call<'_>) -> Result<Status, Stop> {
    set(c, |n, v| n.perk = v)
}

// D2-08713
fn get_perk(c: &mut Call<'_>) -> Result<Status, Stop> {
    get(c, |n| n.perk)
}

/// A new string reference holding a copy of the node's name or description, below the receiver.
fn get_text(c: &mut Call<'_>, f: fn(&SkillNode) -> &TextInfo) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let t = f(&node_at(c, r)?).clone();
    let id =
        c.vm.heap
            .add_native(native_type::STRING_INFO, NativeObject::TextInfo(t));
    c.write(-2, id, tag::REFERENCE)?;
    Ok(Status::Success)
}

/// The node's name or description set to a copy of the string reference at `fp[-2]`.
fn set_text(c: &mut Call<'_>, f: fn(&mut SkillNode, TextInfo)) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let s = c.arg(0)?;
    if s == NULL {
        return Err(Stop::NullReference);
    }
    let t = match c.vm.heap.native(s)? {
        NativeObject::TextInfo(t) => t.clone(),
        _ => return Err(c.fail("not a string reference")),
    };
    change(c, r, |n| f(n, t))?;
    Ok(Status::Success)
}

// D2-01547
fn get_name(c: &mut Call<'_>) -> Result<Status, Stop> {
    get_text(c, |n| &n.name)
}

// D2-01537
fn set_name(c: &mut Call<'_>) -> Result<Status, Stop> {
    set_text(c, |n, t| n.name = t)
}

// D2-01546
fn get_description(c: &mut Call<'_>) -> Result<Status, Stop> {
    get_text(c, |n| &n.description)
}

// D2-01536
fn set_description(c: &mut Call<'_>) -> Result<Status, Stop> {
    set_text(c, |n, t| n.description = t)
}

/// The node's x and y into the slots the out references at `fp[-2]` and `fp[-3]` reach.
// D2-01550
fn inq_coordinates(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let n = node_at(c, r)?;
    let prog = Arc::clone(c.vm.program());
    for (k, v) in [(-2, n.x), (-3, n.y)] {
        let out = c.cx.at(k)?;
        c.vm.write_slot(c.host, c.cx, out, 0, Some(v), tag::MIXED, &prog)?;
    }
    Ok(Status::Success)
}

/// The node's place: x at `fp[-2]`, y at `fp[-3]`.
// D2-01535
fn set_coordinates(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let (x, y) = (c.arg(0)?, c.arg(1)?);
    change(c, r, |n| {
        n.x = x;
        n.y = y;
    })?;
    Ok(Status::Success)
}

/// A parent (`fp[-2]`) added unless listed. No answer.
// D2-01532
fn add_parent(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let p = c.arg(0)?;
    change(c, r, |n| n.add_parent(p))?;
    Ok(Status::Success)
}

/// A dependency (`fp[-2]`) added unless listed; whether it was, into `fp[-3]`.
// D2-01541
fn add_dependency(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let d = c.arg(0)?;
    let added = change(c, r, |n| n.add_dependency(d))?;
    c.write(-3, u32::from(added), tag::MIXED)?;
    Ok(Status::Success)
}

/// A new list of words copying the node's parents or dependencies, below the receiver.
fn list(c: &mut Call<'_>, f: fn(&SkillNode) -> &Vec<u32>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let words = f(&node_at(c, r)?).clone();
    let l = NativeObject::Sequence {
        item: Elem::Word,
        items: words.into_iter().map(Item::Word).collect(),
    };
    let id = c.vm.heap.add_native(WORD_LIST, l);
    c.write(-2, id, tag::REFERENCE)?;
    Ok(Status::Success)
}

// D2-01548
fn parent_list(c: &mut Call<'_>) -> Result<Status, Stop> {
    list(c, |n| &n.parents)
}

// D2-01545
fn dependency_list(c: &mut Call<'_>) -> Result<Status, Stop> {
    list(c, |n| &n.dependencies)
}

// D2-01533
fn clear(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    change(c, r, SkillNode::clear)?;
    Ok(Status::Success)
}

/// The node set from another whole (`fp[-2]`): a copy of every part.
// D2-01534, D2-12162, D2-12163
fn set_whole(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let from = node_at(c, c.arg(0)?)?;
    change(c, r, |n| *n = from)?;
    Ok(Status::Success)
}
