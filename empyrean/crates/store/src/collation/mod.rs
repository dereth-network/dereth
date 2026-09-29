//! MariaDB's `utf8mb4_uca1400_ai_ci`, the collation of every text column in ACE's shard and auth
//! databases. Not ACE: ACE leaves it to the database server.
//!
//! ACE's `ShardBase.sql` and `AuthenticationBase.sql` declare `DEFAULT CHARSET=utf8mb4` with no
//! collation, so the tables take the server's default. On the MariaDB 12.3 that ACE runs against here
//! that is `utf8mb4_uca1400_ai_ci` (measured with read-only queries against that server;
//! the EF models' `UseCollation(...)` only affects migrations ACE does not run). The columns ACE
//! compares or keys by (`character.name`, `character_properties_quest_registry.quest_Name`,
//! `config_properties_*.key`, `account.accountName`) use it in empyrean-store's SQLite schema through
//! [`register`], and the in-memory backends through [`compare`] and [`CollationKey`].
//!
//! The collation, as MariaDB implements it:
//! * each character maps to zero or more level-1 (primary) weights of the Unicode 14.0 DUCET
//!   ([`uca1400_table`]): case and accents are not weighed; ignorable characters (controls other
//!   than TAB..CR, soft hyphen, combining marks) have no weight; `ß` weighs as `ss`, `æ` as `ae`;
//! * characters without a table entry get UCA's implicit weights (two per character);
//! * a contraction (`L·`, Cyrillic letter + breve, Thai prevowel + consonant, ...) weighs as a unit;
//!   the longest match wins;
//! * PAD SPACE: the shorter weight string is compared as if padded with the space weight
//!   (0x0209), so trailing spaces are insignificant, while a trailing TAB (0x0201) sorts before
//!   the end of the string.

mod uca1400_table;

use std::cmp::Ordering;

use rusqlite::Connection;

use uca1400_table::{CONTRACTIONS, IMPLICIT, RUNS};

/// The collation's name, in MariaDB and in empyrean-store's SQLite schema.
pub const NAME: &str = "utf8mb4_uca1400_ai_ci";

/// The weight of U+0020 SPACE, which PAD SPACE pads the shorter string with.
pub const SPACE_WEIGHT: u16 = 0x0209;

/// Registers the collation on `conn` (every empyrean-store connection, before its schema is used).
///
/// # Errors
/// When SQLite refuses the registration.
pub fn register(conn: &Connection) -> rusqlite::Result<()> {
    conn.create_collation(NAME, compare)
}

/// The level-1 weights of one character outside any contraction.
fn push_char_weights(c: char, out: &mut Vec<u16>) {
    let cp = u32::from(c);
    let i = RUNS.partition_point(|r| r.0 <= cp);
    if i > 0 {
        let r = &RUNS[i - 1];
        if cp - r.0 < r.1 {
            if r.2 == 1 {
                // The run's weights rise by one per code point; the table keeps them within u16.
                out.push(r.3[0] + u16::try_from(cp - r.0).unwrap_or(0));
            } else {
                out.extend_from_slice(r.3);
            }
            return;
        }
    }
    let (base, origin) = IMPLICIT
        .iter()
        .find(|r| r.0 <= cp && cp <= r.1)
        .map_or((0xFBC0, 0), |r| (r.2, r.3));
    let d = cp - origin;
    // d < 0x110000, so d >> 15 < 0x22 and d & 0x7FFF fit in u16.
    out.push(base + u16::try_from(d >> 15).unwrap_or(0));
    out.push(u16::try_from(d & 0x7FFF).unwrap_or(0) | 0x8000);
}

/// The longest contraction starting at `rest[0]`: its length in characters and weights.
fn contraction_at(rest: &[char]) -> Option<(usize, &'static [u16])> {
    let head = u32::from(rest[0]);
    let start = CONTRACTIONS.partition_point(|c| c.0[0] < head);
    let mut best: Option<(usize, &'static [u16])> = None;
    for (seq, weights) in CONTRACTIONS[start..].iter().take_while(|c| c.0[0] == head) {
        let n = seq.len();
        if n <= rest.len()
            && seq.iter().zip(rest).all(|(&a, &b)| a == u32::from(b))
            && best.is_none_or(|(m, _)| n > m)
        {
            best = Some((n, weights));
        }
    }
    best
}

/// The string's level-1 weight string (what MariaDB's `WEIGHT_STRING` returns, as u16s).
#[must_use]
pub fn weights(s: &str) -> Vec<u16> {
    let chars: Vec<char> = s.chars().collect();
    let mut out = Vec::with_capacity(chars.len());
    let mut i = 0;
    while i < chars.len() {
        if let Some((n, w)) = contraction_at(&chars[i..]) {
            out.extend_from_slice(w);
            i += n;
        } else {
            push_char_weights(chars[i], &mut out);
            i += 1;
        }
    }
    out
}

/// Compares two weight strings with PAD SPACE (MariaDB's `strnncollsp` for one level).
fn compare_weights(a: &[u16], b: &[u16]) -> Ordering {
    let n = a.len().min(b.len());
    match a[..n].cmp(&b[..n]) {
        Ordering::Equal => {}
        o => return o,
    }
    if a.len() > n {
        a[n..]
            .iter()
            .find(|&&w| w != SPACE_WEIGHT)
            .map_or(Ordering::Equal, |w| w.cmp(&SPACE_WEIGHT))
    } else {
        b[n..]
            .iter()
            .find(|&&w| w != SPACE_WEIGHT)
            .map_or(Ordering::Equal, |w| SPACE_WEIGHT.cmp(w))
    }
}

/// Compares `a` and `b` as MariaDB's `utf8mb4_uca1400_ai_ci` does (`STRCMP`, `ORDER BY`, `=`).
#[must_use]
pub fn compare(a: &str, b: &str) -> Ordering {
    compare_weights(&weights(a), &weights(b))
}

/// Whether `a = b` under the collation.
#[must_use]
pub fn eq(a: &str, b: &str) -> bool {
    compare(a, b) == Ordering::Equal
}

/// A string ordered and compared by the collation, for keys of in-memory maps: two keys are equal
/// exactly when MariaDB's unique index would treat them as duplicates.
#[derive(Debug, Clone)]
pub struct CollationKey {
    text: String,
    weights: Vec<u16>,
}

impl CollationKey {
    /// The key of `s`.
    #[must_use]
    pub fn new(s: &str) -> Self {
        Self {
            text: s.to_owned(),
            weights: weights(s),
        }
    }

    /// The string the key was made from.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }
}

impl PartialEq for CollationKey {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for CollationKey {}

impl std::hash::Hash for CollationKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Equal keys differ at most by trailing space weights (PAD SPACE).
        let end = self
            .weights
            .iter()
            .rposition(|&w| w != SPACE_WEIGHT)
            .map_or(0, |i| i + 1);
        self.weights[..end].hash(state);
    }
}

impl PartialOrd for CollationKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CollationKey {
    fn cmp(&self, other: &Self) -> Ordering {
        compare_weights(&self.weights, &other.weights)
    }
}
