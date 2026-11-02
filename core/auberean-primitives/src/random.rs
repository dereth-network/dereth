//! The client's random numbers: one generator for the whole program.
//!
//! The generator is two multiplicative congruential sequences (moduli 2^31 - 85 and 2^31 - 249)
//! whose difference is drawn through a shuffle table of 32. The client never seeds it: every run
//! starts from the same state, and every draw anywhere in the program takes the next number,
//! whatever draws it: particles, effects, animations, the interface's media, sounds, the camera's
//! shake, portal space, the scripts and the character creation they drive. So what one part draws
//! moves what every other part draws next, and a new character's look depends on everything drawn
//! before it was picked.
//!
//! [`Random`] is a hold on one generator: a clone draws from the same numbers. The program makes
//! one and hands a clone to everything that draws; a test makes its own, which starts where the
//! client's does.
//!
//! The rolls between two bounds are worked out from a draw at double precision, as the client
//! does its floating-point work, and rounded once where it stores them.

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicI32, Ordering::Relaxed};

const M1: i32 = 2_147_483_563;
const M2: i32 = 2_147_483_399;
const TABLE: usize = 32;

/// The generator's state: both sequences, the last number drawn, and the shuffle table. The
/// program draws from one thread at a time; each word is kept so a hold can be shared.
struct State {
    first: AtomicI32,
    second: AtomicI32,
    last: AtomicI32,
    table: [AtomicI32; TABLE],
}

/// A hold on one generator of the client's random numbers. Cloned, it draws from the same
/// numbers; a new one starts where every run of the client starts.
#[derive(Clone)]
pub struct Random(Arc<State>);

impl Default for Random {
    fn default() -> Self {
        Self::new()
    }
}

impl core::fmt::Debug for Random {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Random")
            .field("first", &self.0.first.load(Relaxed))
            .field("second", &self.0.second.load(Relaxed))
            .finish_non_exhaustive()
    }
}

/// One step of a sequence: `x` times `a`, less `m` times `x / q`, brought back above zero.
fn step(x: i32, a: i32, q: i32, m: i32) -> i32 {
    let v = x
        .wrapping_mul(a)
        .wrapping_add((x / q).wrapping_mul(m.wrapping_neg()));
    if v < 0 {
        v.wrapping_add(m)
    } else {
        v
    }
}

impl Random {
    /// A generator in the state every run of the client starts in (it is never seeded).
    #[must_use]
    pub fn new() -> Self {
        Self(Arc::new(State {
            first: AtomicI32::new(1),
            second: AtomicI32::new(123_456_789),
            last: AtomicI32::new(0),
            table: core::array::from_fn(|_| AtomicI32::new(0)),
        }))
    }

    /// Whether `other` holds the same generator.
    #[must_use]
    pub fn same_as(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    /// The next number, in `(0, 1)`: at most `0.99999988`.
    // D2-12861
    pub fn draw(&self) -> f64 {
        let s = &self.0;
        let first = step(s.first.load(Relaxed), 40_014, 53_668, M1);
        let second = step(s.second.load(Relaxed), 40_692, 52_774, M2);
        s.first.store(first, Relaxed);
        s.second.store(second, Relaxed);
        let j = usize::try_from(s.last.load(Relaxed) / 67_108_862)
            .unwrap_or(0)
            .min(TABLE - 1);
        let mut last = s.table[j].load(Relaxed).wrapping_sub(second);
        s.table[j].store(first, Relaxed);
        if last < 1 {
            last = last.wrapping_add(M1 - 1);
        }
        s.last.store(last, Relaxed);
        let x = f64::from(last) * 4.656_613_057_391_769e-10;
        if x > 0.999_999_88 {
            0.999_999_88
        } else {
            x
        }
    }

    /// A whole number between two bounds, either order, both included; equal bounds give the
    /// bound without drawing.
    pub fn roll_int(&self, a: i32, b: i32) -> i32 {
        if a == b {
            return a;
        }
        roll_int(self.draw(), a, b)
    }

    /// A number between two bounds, either order; equal bounds give the bound without drawing.
    pub fn roll_float(&self, a: f32, b: f32) -> f32 {
        if a == b {
            return a;
        }
        roll_float(self.draw(), a, b)
    }

    /// A whole number from 1 to 100, each as likely.
    pub fn roll_percentile(&self) -> i32 {
        percentile(self.draw())
    }

    /// Whether a roll between 0 and 1 comes up at most `chance`.
    // D2-03362
    pub fn check(&self, chance: f32) -> bool {
        self.draw() <= f64::from(chance)
    }
}

/// Two bounds low first: swapped when the second is less (so when either is not a number, they
/// stay as given).
fn ordered<T: PartialOrd>(a: T, b: T) -> (T, T) {
    if b < a {
        (b, a)
    } else {
        (a, b)
    }
}

/// A number between two different bounds, either order, from a draw `r`: the span times the draw
/// plus the low bound, rounded once.
// D2-12863
#[must_use]
pub fn roll_float(r: f64, a: f32, b: f32) -> f32 {
    let (lo, hi) = ordered(a, b);
    #[allow(clippy::cast_possible_truncation)]
    let v = ((f64::from(hi) - f64::from(lo)) * r + f64::from(lo)) as f32;
    v
}

/// A whole number between two different bounds, either order, both included, from a draw `r`:
/// the draw times the bounds' difference plus one (as an unsigned number), cut toward zero, plus
/// the low bound.
// D2-12862
#[must_use]
pub fn roll_int(r: f64, a: i32, b: i32) -> i32 {
    let (lo, hi) = ordered(a, b);
    #[allow(clippy::cast_sign_loss)]
    let span = f64::from(hi.wrapping_sub(lo).wrapping_add(1) as u32);
    // LINT-OK: the client cuts the product toward zero to 64 bits and keeps the low word.
    #[allow(clippy::cast_possible_truncation)]
    let k = (r * span) as i64 as i32;
    k.wrapping_add(lo)
}

/// A whole number from 1 to 100 from a draw `r`: 1 less the draw times -100, cut toward zero.
// D2-12865
#[must_use]
pub fn percentile(r: f64) -> i32 {
    // LINT-OK: the product lies in (-100, 0]; the client cuts it toward zero.
    #[allow(clippy::cast_possible_truncation)]
    let k = (r * -100.0) as i32;
    1 - k
}

/// One entry of a [`SelectionTable`]: the object, its weight, and the weight of its subtree (it
/// and every entry below it in the table's heap order). A new entry weighs nothing.
// D2-04253
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SelectionEntry<T> {
    pub object: T,
    pub weight: u32,
    pub subtree: u32,
}

/// Objects drawn at random by weight, each at most once until the table starts over: the entries
/// not yet drawn kept as a heap (each entry's children at `2i + 1` and `2i + 2`) with each one's
/// subtree weight, and the entries drawn. A new table is empty; a copy has the same entries.
// D2-04290, D2-04416
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SelectionTable<T> {
    pub unused: Vec<SelectionEntry<T>>,
    pub used: Vec<SelectionEntry<T>>,
}

impl<T: Copy> SelectionTable<T> {
    /// Adds an object of weight `weight`, adding the weight to every entry above it.
    // D2-08926
    pub fn add(&mut self, object: T, weight: u32) {
        let mut i = self.unused.len();
        self.unused.push(SelectionEntry {
            object,
            weight,
            subtree: weight,
        });
        while i != 0 {
            i = (i - 1) >> 1;
            self.unused[i].subtree = self.unused[i].subtree.wrapping_add(weight);
        }
    }

    /// Works out the subtree weights below entry `i` from the entries' own weights.
    // D2-08921
    pub fn initialize_weights(&mut self, i: usize) {
        self.unused[i].subtree = self.unused[i].weight;
        for child in [2 * i + 1, 2 * i + 2] {
            if child < self.unused.len() {
                self.initialize_weights(child);
                self.unused[i].subtree = self.unused[i]
                    .subtree
                    .wrapping_add(self.unused[child].subtree);
            }
        }
    }

    /// Moves entry `i` to the drawn entries: the last entry takes its place, the weights above
    /// both brought up to date.
    // D2-08927
    pub fn move_to_used(&mut self, i: usize) {
        self.used.push(self.unused[i]);
        let last = self.unused.len() - 1;
        let last_weight = self.unused[last].weight;
        let diff = last_weight.wrapping_sub(self.unused[i].weight);
        let mut j = last;
        while j != 0 {
            j = (j - 1) >> 1;
            self.unused[j].subtree = self.unused[j].subtree.wrapping_sub(last_weight);
        }
        self.unused[i].weight = last_weight;
        self.unused[i].object = self.unused[last].object;
        if diff != 0 {
            self.unused[i].subtree = self.unused[i].subtree.wrapping_add(diff);
            let mut j = i;
            while j != 0 {
                j = (j - 1) >> 1;
                self.unused[j].subtree = self.unused[j].subtree.wrapping_add(diff);
            }
        }
        self.unused.pop();
    }

    /// Puts every drawn entry back, in the order drawn.
    // D2-08928
    pub fn start_over(&mut self) {
        let used = core::mem::take(&mut self.used);
        for e in used {
            self.add(e.object, e.weight);
        }
    }

    /// Draws an object by weight from those not yet drawn: a roll from 1 to the total weight, and
    /// down the heap to the entry whose span of the running total holds it. None when every
    /// entry has been drawn.
    // D2-08929
    pub fn choose(&mut self, random: &Random) -> Option<T> {
        let root = *self.unused.first()?;
        #[allow(clippy::cast_possible_wrap)]
        let roll = random.roll_int(1, root.subtree as i32) as u32;
        let mut cum = root.weight;
        let mut i = 0;
        while cum < roll {
            let left = 2 * i + 1;
            // Only a table holding entries of no weight runs past its last entry here (the
            // client then reads what its array held before); the descent stops instead.
            let Some(l) = self.unused.get(left) else {
                break;
            };
            let through = l.subtree.wrapping_add(cum);
            if through < roll {
                let Some(r) = self.unused.get(left + 1) else {
                    break;
                };
                i = left + 1;
                cum = r.weight.wrapping_add(through);
            } else {
                i = left;
                cum = cum.wrapping_add(self.unused[i].weight);
            }
        }
        let object = self.unused[i].object;
        self.move_to_used(i);
        Some(object)
    }
}

impl SelectionTable<u32> {
    /// The binary form: the count of entries not yet drawn, each entry's object, weight and
    /// subtree weight; then the same for the drawn entries.
    // D2-04366, D2-04367
    #[must_use]
    pub fn pack(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for list in [&self.unused, &self.used] {
            #[allow(clippy::cast_possible_truncation)]
            out.extend_from_slice(&(list.len() as u32).to_le_bytes());
            for e in list {
                for w in [e.object, e.weight, e.subtree] {
                    out.extend_from_slice(&w.to_le_bytes());
                }
            }
        }
        out
    }
}
