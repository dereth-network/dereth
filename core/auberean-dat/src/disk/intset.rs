//! A set of data versions as the client keeps one (a data file's iteration list, a download's
//! lists): integers, mostly consecutive, packed as runs.
//!
//! Packed: the count, then, in ascending order, each run of three or more consecutive values as
//! its length negated and its first value, and each other value alone with its top bit cleared.
//! Unpacked, a lone value with bit 30 set is negative (bit 31 set back); a set of more than
//! 100,000 values is refused.

use crate::{Reader, Writer};

/// The most values a packed set may hold.
pub const MAX_VALUES: u32 = 100_000;

/// A set of data versions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionSet {
    pub values: Vec<i32>,
    /// Whether `values` is known to be in order with no repeats.
    pub sorted: bool,
}

impl Default for VersionSet {
    fn default() -> Self {
        Self::new()
    }
}

/// The order the set sorts in: ascending, as signed numbers.
// D2-17655
#[must_use]
pub fn compare_ints(a: i32, b: i32) -> core::cmp::Ordering {
    a.cmp(&b)
}

impl VersionSet {
    /// An empty set, in order.
    // D2-17656, D2-17657
    #[must_use]
    pub fn new() -> Self {
        Self {
            values: Vec::new(),
            sorted: true,
        }
    }

    /// Add a value (in any order; repeats are dropped when the set is next sorted).
    // D2-17660
    pub fn add(&mut self, v: i32) {
        self.sorted = false;
        self.values.push(v);
    }

    /// Put the values in order and drop repeats: sorted, then from the end down each value equal
    /// to the one before it replaced by the last value (the set one shorter), sorted again when
    /// any went.
    // D2-17658
    pub fn sort(&mut self) {
        if self.sorted {
            return;
        }
        if !self.values.is_empty() {
            self.values.sort_unstable_by(|a, b| compare_ints(*a, *b));
            let mut dropped = false;
            let mut k = self.values.len() - 1;
            while k > 0 {
                if self.values[k] == self.values[k - 1] {
                    if k < self.values.len() {
                        let last = self.values.len() - 1;
                        if k != last {
                            self.values[k] = self.values[last];
                        }
                        self.values.pop();
                    }
                    dropped = true;
                }
                k -= 1;
            }
            if dropped {
                self.values.sort_unstable_by(|a, b| compare_ints(*a, *b));
            }
        }
        self.sorted = true;
    }

    /// The packed words (sorting the set first).
    fn words(&mut self) -> Vec<u32> {
        self.sort();
        let v = &self.values;
        let n = v.len();
        let mut out = vec![u32::try_from(n).unwrap_or(u32::MAX)];
        let mut i = 0;
        while i < n {
            let mut j = i;
            let mut want = v[i];
            while j < n && v[j] == want {
                j += 1;
                want = want.wrapping_add(1);
            }
            let run = j - i;
            if run >= 3 {
                out.push(
                    0i32.wrapping_sub(i32::try_from(run).unwrap_or(i32::MAX))
                        .cast_unsigned(),
                );
                out.push(v[i].cast_unsigned());
                i = j;
            } else {
                out.push(v[i].cast_unsigned() & 0x7FFF_FFFF);
                i += 1;
            }
        }
        out
    }

    /// How many bytes the set packs to.
    // D2-17659
    pub fn pack_size(&mut self) -> usize {
        4 * self.words().len()
    }

    /// Pack the set.
    // D2-17659
    pub fn pack(&mut self, w: &mut Writer) {
        for word in self.words() {
            w.u32(word);
        }
    }

    /// Unpack a set; `false` (the set as far as it got) for a count over [`MAX_VALUES`], a run
    /// longer than the count left, or the data ending early.
    // D2-17659
    pub fn unpack(&mut self, r: &mut Reader<'_>) -> bool {
        let Ok(total) = r.u32() else {
            return false;
        };
        if total > MAX_VALUES {
            return false;
        }
        let total = total as usize;
        self.values = vec![0; total];
        if total == 0 {
            return true;
        }
        let mut at = 0;
        loop {
            let Ok(word) = r.u32() else {
                return false;
            };
            let word = word.cast_signed();
            if word < 0 {
                let Ok(first) = r.u32() else {
                    return false;
                };
                let mut v = first.cast_signed();
                for _ in 0..word.wrapping_neg().max(0) {
                    if at >= total {
                        return false;
                    }
                    self.values[at] = v;
                    at += 1;
                    v = v.wrapping_add(1);
                }
            } else {
                let mut v = word.cast_unsigned();
                if v & 0x4000_0000 != 0 {
                    v |= 0x8000_0000;
                }
                self.values[at] = v.cast_signed();
                at += 1;
            }
            if at >= total {
                return true;
            }
        }
    }
}
