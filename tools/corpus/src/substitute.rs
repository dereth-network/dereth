//! Finding and replacing a name in a payload, in both encodings, with the guards that keep a
//! coincidence from being treated as a name.
//!
//! # Longest match wins
//!
//! The patterns are matched longest-first at each position, and a substituted run is never
//! re-scanned. For example, take the synthetic account `seed` and password
//! `seed12`, so a left-to-right scan that took the shorter pattern first would turn the password
//! into `ac0112` and then fail to recognise it as the password at all. Longest-first makes the two
//! independent.
//!
//! # The guards
//!
//! A four-byte name matches a random byte run with probability 2^-32, which over a ten-megabyte
//! corpus is a fraction of an occurrence; a two-byte name matches roughly once every 65 kB, which
//! over the same corpus is hundreds. So:
//!
//! * **word boundary**, always: the byte before and the byte after the match must not be ASCII
//!   alphanumeric, so `seed` does not match inside `seed12` (which is matched as itself) or inside
//!   a longer word that happens to contain it;
//! * **a printable run or a length prefix**, for names shorter than four bytes: the match must sit
//!   inside a printable-ASCII run of at least four bytes, or be immediately preceded by a length
//!   prefix equal to its length — one byte, or a little-endian `u16`, which is how the protocol
//!   introduces a string.
//!
//! Sites the guards reject are **counted**, not discarded silently: a name that is rejected
//! everywhere is a name that was not scrubbed, and that has to be visible.

/// One substitution to perform.
#[derive(Debug, Clone)]
pub struct Pattern {
    /// The bytes to look for.
    pub find: Vec<u8>,
    /// The bytes to write, the same length.
    pub replace: Vec<u8>,
    /// Which assignment this came from, as an index into the assignment list.
    pub owner: usize,
    /// How many bytes one character occupies: 1 for ASCII, 2 for UTF-16LE.
    pub stride: usize,
    /// The identity's length in characters, which is what the short-name guard keys on.
    pub chars: usize,
}

/// Every pattern, indexed by first byte.
#[derive(Debug)]
pub struct Matcher {
    patterns: Vec<Pattern>,
    by_first: Vec<Vec<usize>>,
}

/// What one scan did.
///
/// The two rejection counts are kept apart because they mean different things. A **boundary**
/// rejection is the match sitting inside a longer alphanumeric word, which is by construction not
/// the name as a token. A **short-name** rejection is a name of fewer than four bytes turning up
/// in binary noise, which at those lengths happens by chance hundreds of times over a corpus this
/// size. Adding them together would hide which of the two a number is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Hits {
    /// Sites substituted.
    pub applied: usize,
    /// Sites rejected because the match was inside a longer alphanumeric word.
    pub rejected_boundary: usize,
    /// Sites rejected because a short name had neither a printable run nor a length prefix.
    pub rejected_short: usize,
}

impl Hits {
    /// Every rejected site, however it was rejected.
    #[must_use]
    pub fn rejected(self) -> usize {
        self.rejected_boundary + self.rejected_short
    }
}

impl Matcher {
    /// Build a matcher over `(find, replace)` pairs, each pair tagged with its assignment index.
    ///
    /// Each pair contributes two patterns: the ASCII bytes, and the UTF-16LE bytes (each character
    /// followed by a zero). A pair whose two sides differ in length is refused, because a
    /// different-length substitution would shift every byte after it and invalidate the whole
    /// recording rather than one field.
    #[must_use]
    pub fn new(pairs: &[(String, String, usize)]) -> Self {
        let mut patterns = Vec::new();
        for (find, replace, owner) in pairs {
            if find.is_empty() || find.len() != replace.len() || !find.is_ascii() {
                continue;
            }
            patterns.push(Pattern {
                find: find.as_bytes().to_vec(),
                replace: replace.as_bytes().to_vec(),
                owner: *owner,
                stride: 1,
                chars: find.len(),
            });
            patterns.push(Pattern {
                find: utf16le(find),
                replace: utf16le(replace),
                owner: *owner,
                stride: 2,
                chars: find.len(),
            });
        }
        // Longest first, so the scan below can take the first match it finds.
        patterns.sort_by(|a, b| b.find.len().cmp(&a.find.len()).then(a.find.cmp(&b.find)));
        let mut by_first = vec![Vec::new(); 256];
        for (i, p) in patterns.iter().enumerate() {
            by_first[usize::from(p.find[0])].push(i);
        }
        Self { patterns, by_first }
    }

    /// How many patterns are loaded. Diagnostics.
    #[must_use]
    pub fn len(&self) -> usize {
        self.patterns.len()
    }

    /// Whether anything is loaded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }

    /// Scan `buf` and report every site, without writing anything.
    ///
    /// `on_hit(offset, pattern)` is called for each accepted site, left to right, and the scan
    /// resumes past the match.
    pub fn scan(&self, buf: &[u8], mut on_hit: impl FnMut(usize, &Pattern)) -> Vec<Hits> {
        let mut per_owner = vec![Hits::default(); self.owners()];
        let mut i = 0usize;
        while i < buf.len() {
            let mut matched = None;
            for &pi in &self.by_first[usize::from(buf[i])] {
                let p = &self.patterns[pi];
                if buf.len() - i < p.find.len() || &buf[i..i + p.find.len()] != p.find.as_slice() {
                    continue;
                }
                match guard(buf, i, p) {
                    Guard::Ok => {
                        matched = Some(pi);
                        break;
                    }
                    Guard::Boundary => per_owner[p.owner].rejected_boundary += 1,
                    Guard::Short => per_owner[p.owner].rejected_short += 1,
                }
            }
            match matched {
                Some(pi) => {
                    let p = &self.patterns[pi];
                    per_owner[p.owner].applied += 1;
                    on_hit(i, p);
                    i += p.find.len();
                }
                None => i += 1,
            }
        }
        per_owner
    }

    fn owners(&self) -> usize {
        self.patterns.iter().map(|p| p.owner + 1).max().unwrap_or(0)
    }
}

/// UTF-16LE of an ASCII string.
#[must_use]
pub fn utf16le(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len() * 2);
    for b in s.as_bytes() {
        out.push(*b);
        out.push(0);
    }
    out
}

/// Which guard, if any, refused a site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Guard {
    /// Substitute it.
    Ok,
    /// The match is inside a longer alphanumeric word.
    Boundary,
    /// A short name with neither a printable run nor a length prefix around it.
    Short,
}

/// The word-boundary and short-name guards.
#[must_use]
fn guard(buf: &[u8], at: usize, p: &Pattern) -> Guard {
    let end = at + p.find.len();
    if !boundary_ok(buf, at, end, p.stride) {
        return Guard::Boundary;
    }
    if p.chars >= 4 || length_prefix_ok(buf, at, p) || printable_run(buf, at, end) >= 4 {
        return Guard::Ok;
    }
    Guard::Short
}

/// The character before the match and the character after it must not be ASCII alphanumeric.
fn boundary_ok(buf: &[u8], at: usize, end: usize, stride: usize) -> bool {
    let before = at.checked_sub(stride).map(|i| char_at(buf, i, stride));
    let after = (end + stride <= buf.len()).then(|| char_at(buf, end, stride));
    !matches!(before, Some(Some(c)) if c.is_ascii_alphanumeric())
        && !matches!(after, Some(Some(c)) if c.is_ascii_alphanumeric())
}

/// The ASCII character at `at`, or `None` when the unit there is not one.
fn char_at(buf: &[u8], at: usize, stride: usize) -> Option<u8> {
    let b = *buf.get(at)?;
    if stride == 2 && *buf.get(at + 1)? != 0 {
        return None;
    }
    b.is_ascii().then_some(b)
}

/// Immediately preceded by a length prefix equal to the name's length.
fn length_prefix_ok(buf: &[u8], at: usize, p: &Pattern) -> bool {
    let want = p.chars;
    if at >= 1 && usize::from(buf[at - 1]) == want {
        return true;
    }
    at >= 2 && usize::from(u16::from_le_bytes([buf[at - 2], buf[at - 1]])) == want
}

/// The length of the maximal printable-ASCII run containing `at..end`.
fn printable_run(buf: &[u8], at: usize, end: usize) -> usize {
    let printable = |b: u8| (0x20..0x7F).contains(&b);
    let mut lo = at;
    while lo > 0 && printable(buf[lo - 1]) {
        lo -= 1;
    }
    let mut hi = end;
    while hi < buf.len() && printable(buf[hi]) {
        hi += 1;
    }
    hi - lo
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(m: &Matcher, buf: &[u8]) -> (Vec<u8>, Vec<Hits>) {
        let mut out = buf.to_vec();
        let hits = m.scan(buf, |at, p| {
            out[at..at + p.replace.len()].copy_from_slice(&p.replace);
        });
        (out, hits)
    }

    /// The case the two-pattern overlap was found in: an account that is a prefix of the password.
    #[test]
    fn the_longest_pattern_wins_at_each_position() {
        let m = Matcher::new(&[
            ("seed".into(), "ac01".into(), 0),
            ("seed12".into(), "passwo".into(), 1),
        ]);
        let (out, hits) = apply(&m, b"\x04seed\x00\x06seed12\x00");
        assert_eq!(out, b"\x04ac01\x00\x06passwo\x00");
        assert_eq!(hits[0].applied, 1);
        assert_eq!(hits[1].applied, 1);
    }

    /// A name inside a longer word is not the name. The boundary check is what keeps the corpus's
    /// ordinary English text intact.
    #[test]
    fn a_name_inside_a_longer_word_is_not_substituted() {
        let m = Matcher::new(&[("Rue".into(), "Pip".into(), 0)]);
        let (out, hits) = apply(&m, b"  Rueful Rue, Rue7 and \x03Rue ");
        assert_eq!(out, b"  Rueful Pip, Rue7 and \x03Pip ");
        assert_eq!(hits[0].applied, 2);
        assert!(hits[0].rejected_boundary >= 1);
    }

    /// A short name in the middle of binary noise fails the printable-run guard and is counted
    /// rather than substituted -- but the same short name behind a length prefix is taken, which
    /// is how the protocol actually introduces one.
    #[test]
    fn a_short_name_needs_a_printable_run_or_a_length_prefix() {
        let m = Matcher::new(&[("Ao".into(), "Ix".into(), 0)]);
        let (out, hits) = apply(&m, b"\xFF\xFFAo\xFF\xFF");
        assert_eq!(
            out, b"\xFF\xFFAo\xFF\xFF",
            "binary coincidence is left alone"
        );
        assert_eq!(hits[0].applied, 0);
        assert_eq!(hits[0].rejected_short, 1);

        let (out, hits) = apply(&m, b"\xFF\x02\x00Ao\xFF");
        assert_eq!(out, b"\xFF\x02\x00Ix\xFF", "a u16 length prefix anchors it");
        assert_eq!(hits[0].applied, 1);

        let (out, _) = apply(&m, b"\x00 Ao says \x00");
        assert_eq!(out, b"\x00 Ix says \x00", "a printable run anchors it");
    }

    /// Both encodings are scanned. UTF-16LE never occurs in these recordings, but a scrubber that
    /// only looked at one encoding would not be able to say so.
    #[test]
    fn utf16le_occurrences_are_found_too() {
        let m = Matcher::new(&[("Aldis".into(), "Brann".into(), 0)]);
        let mut buf = vec![0u8];
        buf.extend_from_slice(&utf16le("Aldis"));
        buf.push(0);
        let (out, hits) = apply(&m, &buf);
        assert_eq!(&out[1..11], utf16le("Brann").as_slice());
        assert_eq!(hits[0].applied, 1);
    }

    /// Substitution is length-preserving by construction: a pair whose sides differ in length is
    /// refused rather than truncated.
    #[test]
    fn a_different_length_replacement_is_refused() {
        let m = Matcher::new(&[("Aldis".into(), "Ao".into(), 0)]);
        assert!(m.is_empty());
    }
}
