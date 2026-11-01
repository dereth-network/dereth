//! Character names: the written form a name is kept in, the names a character may have, and the
//! operator's banned words.
//!
//! As the January 2017 servers kept them:
//!
//! - **Written form.** White space is trimmed from both ends (with nothing to trim, each run of
//!   spaces is made one instead), and each letter is a capital after anything that is not a letter
//!   and small after a letter: `" Hand Of Doom"` is kept as `"Hand Of Doom"`, `"berndine"` as
//!   `"Berndine"`, `"SLASHER"` as `"Slasher"`. It is the client's own written form for names,
//!   which the client never applies itself: its data holds no word filter.
//! - **What a name may hold:** two to sixteen characters (sixteen is all the client's name entry
//!   takes), English letters, spaces, apostrophes and hyphens, beginning and ending with a letter,
//!   never two spaces together.
//! - **Taken names** are refused in any case (the store's check).
//! - **Banned words.** Which words the 2017 servers refused is not known. The operator may give
//!   patterns (the client's own: `*` stands for any run of letters); a name whose letters, small,
//!   match one is refused. There are none unless the operator gives them.
//!
//! What players say is never filtered: the 2017 servers passed every saying, tell and channel line
//! on as it was typed.

use auberean_assets::script::WordFilter;
use auberean_text::names::{
    check_censors, format_for_audience, is_name_character, ENGLISH, NAME_BAD,
};

/// The most characters a name may have: all the client's name entry takes.
pub const MAX_LENGTH: usize = 16;
/// The fewest characters a name may have.
pub const MIN_LENGTH: usize = 2;

/// The audience the banned words are kept for, and how a name is cut before it is matched: its
/// letters only, so spaces and marks between them do not hide a word.
const AUDIENCE: u32 = 1;
const LETTERS_ONLY: u32 = 2;

/// Why a name was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// Too short, too long, or holding what a name may not.
    NotAName,
    /// It holds a banned word: the pattern it matched.
    Banned(String),
}

/// The operator's rules for names: the banned words' patterns.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NameRules {
    pub banned: Vec<String>,
}

impl NameRules {
    /// The rules with these banned patterns, read from text: one pattern a line, small letters
    /// and `*`; blank lines and lines starting with `#` left out.
    #[must_use]
    pub fn from_text(text: &str) -> Self {
        Self {
            banned: text
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .map(str::to_lowercase)
                .collect(),
        }
    }

    /// The name a typed one is kept as, or why it is refused.
    ///
    /// # Errors
    /// The name is not a name, or holds a banned word.
    pub fn check(&self, typed: &str) -> Result<String, Refusal> {
        let units: Vec<u16> = typed.encode_utf16().collect();
        let (written, _) = format_for_audience(&units);
        let n = written.len();
        let letter = |c: Option<&u16>| c.is_some_and(|&c| is_ascii_letter(c));
        let fits = (MIN_LENGTH..=MAX_LENGTH).contains(&n)
            && written
                .iter()
                .all(|&c| c != 0 && is_name_character(c, ENGLISH))
            && letter(written.first())
            && letter(written.last())
            && !written.windows(2).any(|w| w == [0x20, 0x20]);
        if !fits {
            return Err(Refusal::NotAName);
        }
        if !self.banned.is_empty() {
            let filter = WordFilter {
                buckets: 0,
                audiences: vec![(AUDIENCE, self.banned.clone())],
            };
            if let (NAME_BAD, Some(p)) =
                check_censors(Some(&filter), &written, AUDIENCE, LETTERS_ONLY)
            {
                return Err(Refusal::Banned(p));
            }
        }
        Ok(String::from_utf16_lossy(&written))
    }
}

fn is_ascii_letter(c: u16) -> bool {
    u8::try_from(c).is_ok_and(|b| b.is_ascii_alphabetic())
}
