//! A table of words picked at random by weight, each picked once until the table starts over: the
//! engine's random selection table, as scripts fill it and choose from it.
//!
//! The table keeps its unchosen entries as a binary heap laid out in an array (an entry's children
//! at `2i + 1` and `2i + 2`), each entry holding its word, its weight and the weight of its whole
//! subtree, and the chosen entries apart, in the order they were chosen.
//!
//! - **Adding** an entry of a weight above 0 (a weight of 0 or less adds nothing) puts it last and
//!   adds its weight up the path to the root; adding fast puts it last with no subtree weight,
//!   for the weights to be worked out once afterwards.
//! - **Choosing** rolls the engine's dice from 1 to the total weight and walks down from the root:
//!   the root's own weight first, then left or right by the running sum. The chosen entry is moved
//!   to the chosen list, the last entry takes its place, and the subtree weights are mended.
//!   Choosing from an empty table answers no and leaves the word as it was.
//! - **Starting over** puts the chosen entries back in the order they were chosen; **clearing**
//!   empties both lists; the table is **done** when nothing is left to choose.
//!
//! Its packed form: the unchosen entries counted (word, weight, subtree weight each), then the
//! chosen ones likewise.

use std::sync::Arc;

use auberean_script::heap::Heap;
use auberean_script::native::{Call, NativeFn, NativeTable, Status, Stop};
use auberean_script::native_object::{Engine, EngineValue, NativeObject};
use auberean_script::value::{tag, NULL};

use crate::numbers::roll_int;

/// The random selection table.
pub const TABLE: u32 = 0x60;

/// An entry: its word, its weight, and its subtree's weight.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Entry {
    pub object: u32,
    pub weight: u32,
    pub subtree: u32,
}

/// A table, as the engine keeps it.
// D2-04290, D2-04416
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SelectionTable {
    pub unused: Vec<Entry>,
    pub used: Vec<Entry>,
}

impl SelectionTable {
    /// The table a packed form holds; `None` for bytes that are not one.
    // D2-04366, D2-04367
    #[must_use]
    pub fn unpack(bytes: &[u8]) -> Option<Self> {
        let (chunks, rest) = bytes.as_chunks::<4>();
        if !rest.is_empty() {
            return None;
        }
        let mut words = chunks.iter().map(|c| u32::from_le_bytes(*c));
        let mut list = || -> Option<Vec<Entry>> {
            let n = words.next()? as usize;
            (0..n)
                .map(|_| {
                    Some(Entry {
                        object: words.next()?,
                        weight: words.next()?,
                        subtree: words.next()?,
                    })
                })
                .collect()
        };
        let unused = list()?;
        let used = list()?;
        Some(Self { unused, used })
    }

    /// Adds `weight` to every ancestor of entry `k`'s subtree weight.
    fn add_up(&mut self, mut k: usize, weight: u32) {
        while k != 0 {
            k = (k - 1) >> 1;
            self.unused[k].subtree = self.unused[k].subtree.wrapping_add(weight);
        }
    }

    /// An entry put last, its weight added up the path to the root.
    // D2-08926
    pub fn add(&mut self, object: u32, weight: u32) {
        let k = self.unused.len();
        self.unused.push(Entry {
            object,
            weight,
            subtree: weight,
        });
        self.add_up(k, weight);
    }

    /// Every subtree weight worked out from the root down.
    // D2-08921
    pub fn initialize_weights(&mut self, k: usize) {
        self.unused[k].subtree = self.unused[k].weight;
        for child in [2 * k + 1, 2 * k + 2] {
            if child < self.unused.len() {
                self.initialize_weights(child);
                self.unused[k].subtree = self.unused[k]
                    .subtree
                    .wrapping_add(self.unused[child].subtree);
            }
        }
    }

    /// Entry `k` moved to the chosen list: the last entry takes its place, its weight taken off
    /// the last place's ancestors and the difference added up from `k`.
    // D2-08927
    pub fn move_to_used(&mut self, k: usize) {
        self.used.push(self.unused[k]);
        let last = self.unused.len() - 1;
        let w_last = self.unused[last].weight;
        let diff = w_last.wrapping_sub(self.unused[k].weight);
        let mut a = last;
        while a != 0 {
            a = (a - 1) >> 1;
            self.unused[a].subtree = self.unused[a].subtree.wrapping_sub(w_last);
        }
        self.unused[k].weight = w_last;
        self.unused[k].object = self.unused[last].object;
        if diff != 0 {
            self.unused[k].subtree = self.unused[k].subtree.wrapping_add(diff);
            self.add_up(k, diff);
        }
        self.unused.truncate(last);
    }

    /// The chosen entries put back, in the order they were chosen.
    // D2-08928
    pub fn start_over(&mut self) {
        for e in std::mem::take(&mut self.used) {
            let k = self.unused.len();
            self.unused.push(Entry {
                subtree: e.weight,
                ..e
            });
            self.add_up(k, e.weight);
        }
    }

    /// The entry a roll from 1 to the total weight picks: the root's weight first, then down the
    /// left child when the running sum with its subtree reaches the roll, else the right. `None`
    /// when the walk runs past the last entry (weights not worked out: the client reads past its
    /// array there).
    // D2-08929
    #[must_use]
    pub fn pick(&self, roll: u32) -> Option<usize> {
        let e = &self.unused;
        let (mut i, mut acc) = (0, e.first()?.weight);
        while acc < roll {
            let l = 2 * i + 1;
            let t = e.get(l)?.subtree.wrapping_add(acc);
            if t < roll {
                i = l + 1;
                acc = e.get(i)?.weight.wrapping_add(t);
            } else {
                i = l;
                acc = acc.wrapping_add(e[l].weight);
            }
        }
        Some(i)
    }

    fn pack(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for list in [&self.unused, &self.used] {
            #[allow(clippy::cast_possible_truncation)]
            out.extend((list.len() as u32).to_le_bytes());
            for e in list {
                for w in [e.object, e.weight, e.subtree] {
                    out.extend(w.to_le_bytes());
                }
            }
        }
        out
    }
}

impl EngineValue for SelectionTable {
    fn native_type(&self) -> u32 {
        TABLE
    }

    fn clone_value(&self) -> Box<dyn EngineValue> {
        Box::new(self.clone())
    }

    fn same(&self, other: &dyn EngineValue) -> bool {
        other.as_any().downcast_ref::<Self>() == Some(self)
    }

    fn pack(&self) -> Option<Vec<u8>> {
        Some(Self::pack(self))
    }

    fn as_any(&self) -> &dyn core::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn core::any::Any {
        self
    }
}

/// This module's natives: package, number, function.
const NATIVES: &[(u32, u32, NativeFn)] = &[
    (TABLE, 0x01, add_entry),
    (TABLE, 0x02, fast_add_entry),
    (TABLE, 0x03, initialize_weights),
    (TABLE, 0x04, clear),
    (TABLE, 0x05, done),
    (TABLE, 0x06, start_over),
    (TABLE, 0x07, choose),
];

/// Bind this module's natives.
pub fn bind(t: &mut NativeTable) {
    crate::bind_table(t, NATIVES);
}

/// The table a reference names, made the engine's own the first time a native touches it.
fn table(heap: &mut Heap, native: u32, r: u32) -> Result<&mut SelectionTable, Stop> {
    let o = heap.native_mut(r)?;
    let made = match o {
        NativeObject::Opaque { ty } if *ty == TABLE => Some(SelectionTable::default()),
        NativeObject::Packed { ty, bytes } if *ty == TABLE => SelectionTable::unpack(bytes),
        _ => None,
    };
    if let Some(t) = made {
        *o = NativeObject::Engine(Engine::new(t));
    }
    match o {
        NativeObject::Engine(e) => e.get_mut::<SelectionTable>(),
        _ => None,
    }
    .ok_or_else(|| Stop::Native {
        native,
        what: "not a random selection table".into(),
    })
}

/// The receiver's table through `f`; `None` for the null receiver.
fn with<R>(c: &mut Call<'_>, f: impl FnOnce(&mut SelectionTable) -> R) -> Result<Option<R>, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(None);
    }
    let native = c.native;
    Ok(Some(f(table(&mut c.vm.heap, native, r)?)))
}

fn done_or_crash(v: Option<()>) -> Status {
    v.map_or(Status::Crash, |()| Status::Success)
}

/// An entry: the word at `fp[-2]`, the weight at `fp[-3]` (nothing added for a weight of 0 or
/// less).
// D2-08930
fn add_entry(c: &mut Call<'_>) -> Result<Status, Stop> {
    let (object, weight) = (c.arg(0)?, c.arg(1)?);
    #[allow(clippy::cast_possible_wrap)]
    let positive = weight as i32 > 0;
    Ok(done_or_crash(with(c, |t| {
        if positive {
            t.add(object, weight);
        }
    })?))
}

/// An entry put last with no subtree weight (the word at `fp[-2]`, the weight at `fp[-3]`; nothing
/// added for a weight of 0 or less).
// D2-08931
fn fast_add_entry(c: &mut Call<'_>) -> Result<Status, Stop> {
    let (object, weight) = (c.arg(0)?, c.arg(1)?);
    #[allow(clippy::cast_possible_wrap)]
    let positive = weight as i32 > 0;
    Ok(done_or_crash(with(c, |t| {
        if positive {
            t.unused.push(Entry {
                object,
                weight,
                subtree: 0,
            });
        }
    })?))
}

/// Every subtree weight worked out, when there are entries.
// D2-08924
fn initialize_weights(c: &mut Call<'_>) -> Result<Status, Stop> {
    Ok(done_or_crash(with(c, |t| {
        if !t.unused.is_empty() {
            t.initialize_weights(0);
        }
    })?))
}

// D2-08923
fn clear(c: &mut Call<'_>) -> Result<Status, Stop> {
    Ok(done_or_crash(with(c, |t| {
        t.unused.clear();
        t.used.clear();
    })?))
}

/// Whether nothing is left to choose, the low byte of `fp[-2]`.
// D2-08922
fn done(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some(empty) = with(c, |t| t.unused.is_empty())? else {
        return Ok(Status::Crash);
    };
    let i = c.cx.frame_index(-2)?;
    c.cx.data[i] = (c.cx.data[i] & !0xFF) | u32::from(empty);
    c.cx.tags[i] = tag::MIXED;
    Ok(Status::Success)
}

// D2-08932
fn start_over(c: &mut Call<'_>) -> Result<Status, Stop> {
    Ok(done_or_crash(with(c, SelectionTable::start_over)?))
}

/// A word chosen at random by weight into the number the reference at `fp[-2]` reaches (tagged as
/// written by a native, chosen or not), the entry moved to the chosen list; whether one was
/// chosen, the low byte of `fp[-3]`.
// D2-08933
fn choose(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let out = c.arg(0)?;
    let native = c.native;
    let total = {
        let t = table(&mut c.vm.heap, native, r)?;
        t.unused.first().map(|e| e.subtree)
    };
    let chosen = match total {
        None => None,
        Some(total) => {
            // The engine's dice from 1 to the total: a total of 1 draws nothing.
            #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
            let roll = if total as i32 == 1 {
                1
            } else {
                roll_int(c.host.random()?, 1, total as i32) as u32
            };
            let t = table(&mut c.vm.heap, native, r)?;
            let Some(k) = t.pick(roll) else {
                return Err(c.fail("the weights lead past the last entry"));
            };
            let object = t.unused[k].object;
            t.move_to_used(k);
            Some(object)
        }
    };
    let prog = Arc::clone(c.vm.program());
    c.vm.write_slot(c.host, c.cx, out, 0, chosen, tag::MIXED, &prog)?;
    let i = c.cx.frame_index(-3)?;
    c.cx.data[i] = (c.cx.data[i] & !0xFF) | u32::from(chosen.is_some());
    c.cx.tags[i] = tag::MIXED;
    Ok(Status::Success)
}
