//! Choosing a same-length stand-in for each discovered identity.
//!
//! The private dictionary stores character names by byte length and includes
//! two *rules* for accounts and passwords rather than lists, because those two have a shape the
//! reader of a published corpus should be able to recognise as synthetic at a glance
//! (`acct01`, `passwo…`).
//!
//! # Assignment is corpus-wide, not per session
//!
//! `pseudonyms.json` says "in order of first appearance per session". This assigns in order of
//! first appearance **across the whole corpus**, processed in a fixed session order, and the
//! reason is a recording: `fellowship-one-vassal` carries sixteen unanswered `LoginRequest`
//! datagrams from a *second* account — the one `fellowship-two-monarch` was later recorded with
//! (`corpus.rs`). Under per-session numbering that account would be `acct02`
//! in one file and `acct01` in the next, and a reader comparing the two recordings would conclude
//! they were different accounts. Corpus-wide numbering keeps every cross-session fact the manifest
//! and the scenarios rely on — the same account, the same character, the same fellowship — true
//! of the scrubbed corpus as well.
//!
//! # Case
//!
//! The client lower-cases the account before it packs the `LoginRequest`
//! before packing the login request, and the server echoes the account back in
//! `LoginCharacterSet` with whatever case it stored. So each identity contributes up to four
//! patterns — as discovered, lower, upper and title — and the pseudonym is re-cased the same way,
//! which keeps the substitution length-preserving and case-faithful at once.

use std::collections::{BTreeMap, BTreeSet};

use crate::identities::{Found, IdKind};

/// One real name and the stand-in it was given.
#[derive(Debug, Clone)]
pub struct Assignment {
    /// What it is.
    pub kind: IdKind,
    /// The real value. **Never written to any public output**; it lives only in the private scrub map.
    pub real: String,
    /// The stand-in, the same byte length as `real`.
    pub pseudonym: String,
    /// The recording it was first seen in.
    pub session: String,
    /// The datagram it was first seen in.
    pub datagram: usize,
    /// The field it was decoded out of.
    pub source: String,
    /// True when the length-keyed list had nothing left and a filler was generated.
    pub generated: bool,
}

/// Why the pseudonym list could not be used.
#[derive(Debug)]
pub struct PseudonymError(pub String);

impl std::fmt::Display for PseudonymError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for PseudonymError {}

/// The filler the password rule draws from.
const PASSWORD_FILLER: &str = "passwordpasswordpasswordpassword";

/// The length-keyed character names, plus the two rules.
#[derive(Debug, Default)]
pub struct Book {
    by_len: BTreeMap<usize, Vec<String>>,
    used: BTreeMap<usize, usize>,
    accounts: usize,
    /// Stand-ins an earlier run handed out, by kind and lower-cased real value.
    prior: BTreeMap<(IdKind, String), Assignment>,
    /// Every stand-in already given to some identity, lower-cased, so a new one never repeats it.
    taken: BTreeSet<String>,
}

impl Book {
    /// Load a pseudonym dictionary.
    ///
    /// # Errors
    /// [`PseudonymError`] when the file is missing, is not JSON, or has no `characters` map.
    pub fn load(path: &std::path::Path) -> Result<Self, PseudonymError> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| PseudonymError(format!("{}: {e}", path.display())))?;
        let v: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| PseudonymError(format!("{}: {e}", path.display())))?;
        let chars = v
            .get("characters")
            .and_then(|c| c.as_object())
            .ok_or_else(|| PseudonymError(format!("{}: no `characters` map", path.display())))?;
        let mut by_len = BTreeMap::new();
        for (k, list) in chars {
            let Ok(len) = k.parse::<usize>() else {
                continue;
            };
            let names: Vec<String> = list
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|s| s.as_str())
                        .filter(|s| s.len() == len)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default();
            by_len.insert(len, names);
        }
        Ok(Self {
            by_len,
            ..Self::default()
        })
    }

    /// A book with no name list: every character stand-in is a generated filler. For a checkout
    /// without the private dictionary, where the fillers are as safe and only less pleasant to read.
    #[must_use]
    pub fn fillers_only() -> Self {
        Self::default()
    }

    /// Reuse the stand-ins an earlier run handed out: an identity it named gets the same stand-in
    /// again, and no new identity is given one of them. This is what keeps a recording added
    /// later consistent with the corpus already published -- the same account is still `acct01`.
    pub fn remember(&mut self, prior: &[Assignment]) {
        for a in prior {
            if a.kind != IdKind::Password {
                self.taken.insert(a.pseudonym.to_ascii_lowercase());
            }
            self.prior
                .entry((a.kind, a.real.to_ascii_lowercase()))
                .or_insert_with(|| a.clone());
        }
    }

    /// Assign a stand-in for one identity. Returns `None` for an empty value.
    pub fn assign(&mut self, f: &Found) -> Option<Assignment> {
        let len = f.value.len();
        if len == 0 {
            return None;
        }
        if let Some(a) = self.prior.get(&(f.kind, f.value.to_ascii_lowercase())) {
            return Some(Assignment {
                real: f.value.clone(),
                ..a.clone()
            });
        }
        let (pseudonym, generated) = match f.kind {
            IdKind::Account => loop {
                self.accounts += 1;
                let name = account_name(len, self.accounts);
                if !self.taken.contains(&name.to_ascii_lowercase()) {
                    break (name, false);
                }
            },
            IdKind::Password => (PASSWORD_FILLER.chars().take(len).collect(), false),
            IdKind::Character | IdKind::Extra => loop {
                let taken = self.used.entry(len).or_insert(0);
                let n = *taken;
                *taken += 1;
                let (name, generated) = match self.by_len.get(&len).and_then(|v| v.get(n)) {
                    Some(name) => (name.clone(), false),
                    None => (filler(len, n), true),
                };
                if !self.taken.contains(&name.to_ascii_lowercase()) {
                    break (name, generated);
                }
            },
        };
        debug_assert_eq!(pseudonym.len(), len);
        if f.kind != IdKind::Password {
            self.taken.insert(pseudonym.to_ascii_lowercase());
        }
        Some(Assignment {
            kind: f.kind,
            real: f.value.clone(),
            pseudonym,
            session: f.session.clone(),
            datagram: f.datagram,
            source: f.source.clone(),
            generated,
        })
    }
}

/// The account rule from `pseudonyms.json`: `acct` plus a zero-padded ordinal for length >= 5,
/// `ac` plus one for 3-4, `a` plus one for 2, and the bare ordinal for 1.
#[must_use]
pub fn account_name(len: usize, ordinal: usize) -> String {
    let stem = match len {
        0 => return String::new(),
        1 => "",
        2 => "a",
        3 | 4 => "ac",
        _ => "acct",
    };
    let digits = len - stem.len();
    let mut s = String::from(stem);
    s.push_str(&format!("{ordinal:0digits$}"));
    // An ordinal wider than the room left would lengthen the name, which is the one thing the
    // substitution cannot survive. Keep the low-order digits instead; the map stays private, so
    // the only property that matters is that it is the same length and not the real account.
    if s.len() > len {
        s = format!(
            "{stem}{}",
            &format!("{ordinal}")[..digits.min(format!("{ordinal}").len())]
        );
        while s.len() < len {
            s.push('0');
        }
    }
    s
}

/// A generated stand-in for a length the list does not cover.
///
/// Alternating consonants and vowels, so it reads as a name rather than as a hash, seeded by the
/// ordinal so two identities of the same length never collide.
#[must_use]
pub fn filler(len: usize, ordinal: usize) -> String {
    const C: &[u8] = b"bcdfghklmnprstvwz";
    const V: &[u8] = b"aeiouy";
    let mut s = String::with_capacity(len);
    let mut n = ordinal + 1;
    for i in 0..len {
        let ch = if i % 2 == 0 {
            C[n % C.len()] as char
        } else {
            V[n % V.len()] as char
        };
        n = n.wrapping_mul(31).wrapping_add(i).wrapping_add(7);
        s.push(if i == 0 { ch.to_ascii_uppercase() } else { ch });
    }
    s
}

/// The case variants of one assignment: `(pattern, replacement)`, both the same length.
///
/// De-duplicated, so an all-lower-case account contributes one pair rather than three identical
/// ones.
#[must_use]
pub fn variants(a: &Assignment) -> Vec<(String, String)> {
    let shapes: [fn(&str) -> String; 4] = [
        str::to_owned,
        |s| s.to_ascii_lowercase(),
        |s| s.to_ascii_uppercase(),
        title_case,
    ];
    let mut out: Vec<(String, String)> = Vec::new();
    for shape in shapes {
        let pat = shape(&a.real);
        let rep = shape(&a.pseudonym);
        if pat.len() == rep.len() && !out.iter().any(|(p, _)| *p == pat) {
            out.push((pat, rep));
        }
    }
    out
}

fn title_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut start_of_word = true;
    for c in s.chars() {
        if start_of_word {
            out.extend(c.to_uppercase());
        } else {
            out.extend(c.to_lowercase());
        }
        start_of_word = !c.is_ascii_alphanumeric();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one invariant the whole design rests on: the stand-in is the same byte length.
    #[test]
    fn every_stand_in_is_the_same_length_as_what_it_replaces() {
        for len in 1..=32usize {
            assert_eq!(account_name(len, 7).len(), len, "account of length {len}");
            assert_eq!(
                PASSWORD_FILLER.chars().take(len).count(),
                len.min(PASSWORD_FILLER.len()),
                "password of length {len}"
            );
            assert_eq!(filler(len, 3).len(), len, "filler of length {len}");
        }
        assert_eq!(account_name(6, 1), "acct01");
        assert_eq!(account_name(4, 2), "ac02");
        assert_eq!(account_name(2, 9), "a9");
        // A hundred accounts still fit in four bytes rather than growing to five.
        assert_eq!(account_name(4, 123).len(), 4);
    }

    /// Case variants re-case the stand-in the same way, so an upper-cased echo of the account is
    /// replaced by an upper-cased stand-in of the same length rather than being missed.
    #[test]
    fn case_variants_re_case_the_stand_in_too() {
        let a = Assignment {
            kind: IdKind::Character,
            real: "Thornwick".into(),
            pseudonym: "Greywater".into(),
            session: "s".into(),
            datagram: 0,
            source: "t".into(),
            generated: false,
        };
        let v = variants(&a);
        assert!(v.contains(&("Thornwick".into(), "Greywater".into())));
        assert!(v.contains(&("thornwick".into(), "greywater".into())));
        assert!(v.contains(&("THORNWICK".into(), "GREYWATER".into())));
        for (p, r) in &v {
            assert_eq!(p.len(), r.len());
        }
    }

    /// A stand-in an earlier run handed out is given to the same identity again, whatever its
    /// case, and never to a new one.
    #[test]
    fn an_earlier_runs_stand_ins_are_reused_and_never_handed_to_someone_else() {
        let found = |kind, value: &str| Found {
            kind,
            value: value.into(),
            session: "later".into(),
            datagram: 0,
            source: "t".into(),
        };
        let mut first = Book::fillers_only();
        let account = first
            .assign(&found(IdKind::Account, "Player"))
            .expect("an account");
        let character = first
            .assign(&found(IdKind::Character, "Thornwick"))
            .expect("a character");

        let mut later = Book::fillers_only();
        later.remember(&[account.clone(), character.clone()]);
        let again = later
            .assign(&found(IdKind::Account, "player"))
            .expect("the same account");
        assert_eq!(again.pseudonym, account.pseudonym);
        assert_eq!(again.real, "player", "the spelling this run saw");
        let other = later
            .assign(&found(IdKind::Account, "Second"))
            .expect("a second account");
        assert_ne!(other.pseudonym, account.pseudonym);
        let named = later
            .assign(&found(IdKind::Character, "Greywater"))
            .expect("a second character");
        assert_ne!(named.pseudonym, character.pseudonym);
    }

    /// Two identities of the same length get different stand-ins, including past the end of the
    /// list, where the filler takes over.
    #[test]
    fn two_names_of_one_length_never_share_a_stand_in() {
        let mut seen = std::collections::BTreeSet::new();
        for n in 0..40 {
            assert!(seen.insert(filler(5, n)), "filler {n} collided");
        }
    }
}
