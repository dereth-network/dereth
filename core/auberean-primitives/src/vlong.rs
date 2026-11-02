//! Whole numbers of any size, as the client keeps the numbers of its key exchange: a sign and a
//! magnitude in 32-bit words, least significant first, with no zero words at the top.
//!
//! The arithmetic is the client's to the word, its quirks included: a difference whose borrowed
//! word wraps keeps the borrow and leaves that word as it was, a product is cut to the sum of its
//! factors' bit lengths, and the binary form carries the magnitude without its sign.

use alloc::vec::Vec;
use core::cmp::Ordering;

/// A magnitude: its words, least significant first, never a zero word at the top.
#[derive(Debug, Default, PartialEq, Eq)]
struct Magnitude {
    words: Vec<u32>,
}

impl Clone for Magnitude {
    fn clone(&self) -> Self {
        let mut m = Self::default();
        m.copy_from(self);
        m
    }
}

impl Magnitude {
    /// Room for `n` words; what is held is kept.
    // D2-14724
    fn reserve(&mut self, n: usize) {
        self.words.reserve(n.saturating_sub(self.words.len()));
    }

    /// Word `i`, zero past the top.
    fn get(&self, i: usize) -> u32 {
        self.words.get(i).copied().unwrap_or(0)
    }

    /// Sets word `i`: within the number, the word, then any zero words at the top dropped; past
    /// the top, a non-zero word with zeros filling the gap.
    // D2-14725
    fn set(&mut self, i: usize, x: u32) {
        if i < self.words.len() {
            self.words[i] = x;
            if x == 0 {
                while self.words.last() == Some(&0) {
                    self.words.pop();
                }
            }
        } else if x != 0 {
            self.reserve(i + 1);
            self.words.resize(i, 0);
            self.words.push(x);
        }
    }

    /// The number of bits up to the highest one set.
    // D2-14728
    fn bits(&self) -> u32 {
        let Some(&top) = self.words.last() else {
            return 0;
        };
        #[allow(clippy::cast_possible_truncation)]
        let mut n = (self.words.len() as u32 - 1) * 32;
        let mut x = top;
        let mut step = 32;
        loop {
            step >>= 1;
            if (1u32 << step) <= x {
                n += step;
                x >>= step;
            }
            if step <= 8 {
                break;
            }
        }
        // What is left is below 256: the client looks its bit length up in a table.
        n + (32 - x.leading_zeros())
    }

    /// How this magnitude compares with another's.
    // D2-14729
    fn compare(&self, x: &Self) -> Ordering {
        let (n, xn) = (self.words.len(), x.words.len());
        if xn < n {
            return Ordering::Greater;
        }
        if xn > n {
            return Ordering::Less;
        }
        let mut i = n;
        while i > 0 {
            i -= 1;
            if self.get(i) > x.get(i) {
                return Ordering::Greater;
            }
            if self.get(i) < x.get(i) {
                return Ordering::Less;
            }
        }
        Ordering::Equal
    }

    /// Adds `x`.
    // D2-14730
    fn add(&mut self, x: &Self) {
        let max = self.words.len().max(x.words.len());
        self.reserve(max);
        let mut carry = 0u32;
        for i in 0..=max {
            let u = self.get(i);
            let ux = x.get(i);
            let partial = u.wrapping_add(carry);
            let sum = partial.wrapping_add(ux);
            carry = u32::from(partial < carry) + u32::from(sum < ux);
            self.set(i, sum);
        }
    }

    /// Takes away `x` (no greater than this). A word of `x` that wraps when the borrow is added
    /// to it is passed over, the borrow kept.
    // D2-14731
    fn subtract(&mut self, x: &Self) {
        let mut carry = 0u32;
        let n = self.words.len();
        for i in 0..n {
            let ux = x.get(i).wrapping_add(carry);
            if ux >= carry {
                let u = self.get(i);
                let nu = u.wrapping_sub(ux);
                carry = u32::from(nu > u);
                self.set(i, nu);
            }
        }
    }

    /// Becomes a copy of `x`, set from its top word down.
    // D2-14732
    fn copy_from(&mut self, x: &Self) {
        self.words.clear();
        let mut i = x.words.len();
        while i > 0 {
            i -= 1;
            self.set(i, x.get(i));
        }
    }

    /// Becomes the product of `x` and `y` cut to its lowest `keep` bits.
    // D2-14727
    fn fast_mul(&mut self, x: &Self, y: &Self, keep: u32) {
        let limit = ((keep + 31) >> 5) as usize;
        self.words.clear();
        self.words.resize(limit, 0);
        let min = x.words.len().min(limit);
        for i in 0..min {
            let mut end = y.words.len() + i;
            if end > limit {
                end = limit;
            }
            let mut carry = inner(&mut self.words[i..end], x.words[i], &y.words);
            while carry != 0 && end < limit {
                self.words[end] = self.words[end].wrapping_add(carry);
                carry = u32::from(self.words[end] < carry);
                end += 1;
            }
        }
        if keep & 31 != 0 {
            let top = &mut self.words[limit - 1];
            *top &= (1u32 << (keep & 31)) - 1;
        }
        while self.words.last() == Some(&0) {
            self.words.pop();
        }
    }
}

/// Adds `m` times the words of `y` into `acc` (one word of `acc` for each), answering the carry
/// out of the last.
// D2-14726
fn inner(acc: &mut [u32], m: u32, y: &[u32]) -> u32 {
    let mut carry = 0u32;
    for (a, &v) in acc.iter_mut().zip(y) {
        let p = u64::from(v) * u64::from(m);
        #[allow(clippy::cast_possible_truncation)]
        let (lo, hi) = (p as u32, (p >> 32) as u32);
        let t = lo.wrapping_add(carry);
        let old = *a;
        *a = old.wrapping_add(t);
        carry = hi + u32::from(t < lo) + u32::from(*a < old);
    }
    carry
}

/// A whole number of any size, with a sign. Dropped, its words are let go (the client clears them
/// first).
// D2-14739, D2-14746
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Vlong {
    value: Magnitude,
    negative: bool,
}

impl Vlong {
    /// The number `x`. A copy takes another's value and sign.
    // D2-14737, D2-14738, D2-14734
    #[must_use]
    pub fn new(x: u32) -> Self {
        let mut value = Magnitude::default();
        value.set(0, x);
        Self {
            value,
            negative: false,
        }
    }

    /// The number with its sign set: below zero when `negative`.
    #[must_use]
    pub fn with_sign(mut self, negative: bool) -> Self {
        self.negative = negative;
        self
    }

    /// Whether the number is below zero.
    #[must_use]
    pub const fn is_negative(&self) -> bool {
        self.negative
    }

    /// The magnitude's words, least significant first.
    #[must_use]
    pub fn words(&self) -> &[u32] {
        &self.value.words
    }

    /// The magnitude's lowest `n` words, zeros past its top.
    // D2-14733
    #[must_use]
    pub fn store(&self, n: usize) -> Vec<u32> {
        (0..n).map(|i| self.value.get(i)).collect()
    }

    /// Becomes the magnitude of these words (least significant first), its sign kept.
    // D2-14736
    pub fn load(&mut self, words: &[u32]) {
        self.value.words.clear();
        for (i, &x) in words.iter().enumerate() {
            self.value.set(i, x);
        }
    }

    /// Adds `x`: the magnitudes added when the signs agree, else the smaller taken from the
    /// larger and that one's sign kept.
    // D2-14740
    pub fn add_assign(&mut self, x: &Self) {
        if self.negative == x.negative {
            self.value.add(&x.value);
        } else if self.value.compare(&x.value) != Ordering::Less {
            self.value.subtract(&x.value);
        } else {
            let old = core::mem::replace(self, x.clone());
            self.add_assign(&old);
        }
    }

    /// The product, cut to the sum of the two magnitudes' bit lengths; its sign the two signs
    /// differing.
    // D2-14741
    #[must_use]
    pub fn mul(&self, x: &Self) -> Self {
        let mut value = Magnitude::default();
        value.fast_mul(&self.value, &x.value, self.value.bits() + x.value.bits());
        Self {
            value,
            negative: self.negative != x.negative,
        }
    }

    /// Becomes the number a string of hexadecimal digits writes, digit by digit: times 16, plus
    /// the digit (one that is not a hexadecimal digit counting as 0).
    // D2-14742
    pub fn read_hex(&mut self, s: &[u8]) -> bool {
        *self = Self::new(0);
        let sixteen = Self::new(16);
        for &c in s.iter().take_while(|&&c| c != 0) {
            let digit = char::from(c).to_digit(16).unwrap_or(0);
            *self = self.mul(&sixteen);
            self.add_assign(&Self::new(digit));
        }
        true
    }

    /// The binary form's size: a count of words, then the words.
    // D2-14735
    #[must_use]
    pub fn pack_size(&self) -> usize {
        self.value.words.len() * 4 + 4
    }

    /// The binary form: the count of the magnitude's words, then the words (the sign is not
    /// written).
    // D2-14743
    #[must_use]
    pub fn pack(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.pack_size());
        #[allow(clippy::cast_possible_truncation)]
        out.extend_from_slice(&(self.value.words.len() as u32).to_le_bytes());
        for w in self.store(self.value.words.len()) {
            out.extend_from_slice(&w.to_le_bytes());
        }
        out
    }

    /// Reads the binary form from the start of `buf`, answering the bytes read; none when the
    /// count or the words it promises do not fit.
    // D2-14744
    pub fn unpack(&mut self, buf: &[u8]) -> Option<usize> {
        let count = u32::from_le_bytes(buf.get(..4)?.try_into().ok()?) as usize;
        let body = buf.get(4..)?;
        if body.len() < count * 4 {
            return None;
        }
        let words: Vec<u32> = body[..count * 4]
            .chunks(4)
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        self.load(&words);
        Some(4 + count * 4)
    }
}
