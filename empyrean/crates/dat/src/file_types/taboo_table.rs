// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.DatLoader/FileTypes/TabooTable.cs, Source/ACE.DatLoader/Entity/TabooTableEntry.cs
//! The character-name censor: `TabooTable.ContainsBadWord`.
//!
//! **Shape.** ACE reads `0x0E00001E` as `byte, count byte, Dictionary<uint, TabooTableEntry>` with
//! each entry `u32 Unknown1, u16 Unknown2, u32 count, strings`. The shared decoder reads the same
//! bytes as the client does: a hash table of audiences, each a hash table of pattern lists. The
//! two agree exactly when every audience holds one list — ACE's `Unknown1`/`Unknown2` are then the
//! inner table's two-byte header and its key — and the retail table has that shape (the
//! real-content tier asserts it). ACE's "first entry's `BannedPatterns`" is therefore the first
//! audience's first list.

use dereth_assets::TabooTable;

/// ACE's `TabooTable` helpers.
pub trait TabooTableExt {
    /// ACE's `TabooTableEntries.First().BannedPatterns`, or `None` for an empty table.
    fn first_entry_banned_patterns(&self) -> Option<&[String]>;

    /// Whether `input` has a word the table forbids. ACE searches only the first entry, "because
    /// they're all the same".
    fn contains_bad_word(&self, input: &str) -> bool;
}

impl TabooTableExt for TabooTable {
    fn first_entry_banned_patterns(&self) -> Option<&[String]> {
        let (_, audience) = self.audiences.first()?;
        audience.first().map(|(_, patterns)| patterns.as_slice())
    }

    // ACE: TabooTable.ContainsBadWord
    fn contains_bad_word(&self, input: &str) -> bool {
        // foreach ... { if (kvp.Value.ContainsBadWord(input)) return true; break; }
        if let Some(patterns) = self.first_entry_banned_patterns() {
            if entry_contains_bad_word(patterns, input) {
                return true;
            }
        }
        false
    }
}

/// ACE's `TabooTableEntry.ContainsBadWord`: lower-case the input, split it on spaces, and test each
/// word against each pattern as the regex `^` + pattern-with-`*`-as-`.*` + `$`.
// ACE: TabooTableEntry.ContainsBadWord
#[must_use]
pub fn entry_contains_bad_word(banned_patterns: &[String], input: &str) -> bool {
    let input = input.to_lowercase();
    let words = input.split(' ');
    for word in words {
        for banned_pattern in banned_patterns {
            if regex_is_match(word, banned_pattern) {
                return true;
            }
        }
    }
    false
}

/// `Regex.IsMatch(word, "^" + pattern.Replace("*", ".*") + "$")` for the patterns the table holds.
///
/// After the replacement the only regex syntax a pattern can carry is `.` (any character but a
/// newline) and `.*`; every other character is matched literally. The real-content tier asserts
/// that no retail pattern contains any other regex metacharacter, which is what makes this
/// matcher the same function as ACE's regex.
fn regex_is_match(word: &str, pattern: &str) -> bool {
    #[derive(Clone, Copy)]
    enum Tok {
        Any,
        AnyRun,
        Lit(char),
    }
    let mut toks = Vec::new();
    for c in pattern.chars() {
        match c {
            '*' => {
                // "*" became ".*": a '.' already emitted before it is still a single '.'
                toks.push(Tok::AnyRun);
            }
            '.' => toks.push(Tok::Any),
            c => toks.push(Tok::Lit(c)),
        }
    }
    let w: Vec<char> = word.chars().collect();
    // Classic wildcard match with backtracking over the last `AnyRun`.
    let (mut wi, mut ti) = (0usize, 0usize);
    let mut star: Option<(usize, usize)> = None;
    while wi < w.len() {
        match toks.get(ti) {
            Some(Tok::AnyRun) => {
                star = Some((ti, wi));
                ti += 1;
            }
            Some(Tok::Any) if w[wi] != '\n' => {
                wi += 1;
                ti += 1;
            }
            Some(Tok::Lit(c)) if *c == w[wi] => {
                wi += 1;
                ti += 1;
            }
            _ => match star {
                Some((st, sw)) if w[sw] != '\n' => {
                    star = Some((st, sw + 1));
                    ti = st + 1;
                    wi = sw + 1;
                }
                _ => return false,
            },
        }
    }
    toks[ti..].iter().all(|t| matches!(t, Tok::AnyRun))
}

/// Characters that are regex syntax in a .NET pattern, other than the `.` and `*` handled above.
pub const OTHER_REGEX_METACHARACTERS: &[char] =
    &['\\', '^', '$', '|', '?', '+', '(', ')', '[', ']', '{', '}'];
