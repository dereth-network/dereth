//! The text rules every receiving world applies to every remote line, tells and room lines alike.
//!
//! A world never trusts another world's text. Before a remote line reaches a client it is made
//! safe for the oldest client that may read it, the final retail client, which:
//!
//! - draws chat in Windows-1252: a tell's text travels as 1252 bytes, and a room line's fields
//!   end at the first character that code page cannot hold, so an emoji would cut a line short;
//! - reads `<Tell:...>` runs as clickable name links in any line it draws, which a sender could
//!   forge, and drops a room line holding one;
//! - breaks a line at a line feed, ends one at a NUL, and drops a room line holding a line feed
//!   or 256 characters or more;
//! - takes a leading `+` on a name as the administrator marker, and reads `^` or `&` anywhere in
//!   a speaker's name as the Olthoi speech marks.
//!
//! [`sanitize_line`] and [`sanitize_name`] are those rules. Both are idempotent: a sanitised
//! string sanitises to itself.

mod cp1252_best_fit;

use cp1252_best_fit::{BEST_FIT, BEST_FIT_SINCE_PUBLISHED};

/// The longest remote line, in characters, after the other rules. Longer lines are cut, and the
/// cut is marked with nothing. The final retail client drops a room line of 256 characters or
/// more outright, so a cut line is one shorter than that.
pub const MAX_LINE_CHARS: usize = 255;

/// The longest text a link message may carry, in characters. Longer text is malformed; shorter
/// text is cut to [`MAX_LINE_CHARS`] by the receiving world's rules.
pub const MAX_TEXT_CHARS: usize = 1_024;

/// `0x80..=0x9F` of Windows-1252: the 32 code points where it differs from Latin-1.
const HIGH: [u16; 32] = [
    0x20AC, 0x0081, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, // 0x80..=0x87
    0x02C6, 0x2030, 0x0160, 0x2039, 0x0152, 0x008D, 0x017D, 0x008F, // 0x88..=0x8F
    0x0090, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, // 0x90..=0x97
    0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x009D, 0x017E, 0x0178, // 0x98..=0x9F
];

/// Whether `c` has a byte of its own in Windows-1252 (not a best fit).
#[must_use]
pub fn cp1252_exact(c: char) -> bool {
    let Ok(unit) = u16::try_from(u32::from(c)) else {
        return false;
    };
    matches!(unit, 0x0000..=0x007F | 0x00A0..=0x00FF) || HIGH.contains(&unit)
}

/// The character Windows narrows `c` to in code page 1252: `c` itself when the code page holds
/// it, its best fit when the published table has one (`ā` → `a`, `＜` → `<`), else `None`.
#[must_use]
pub fn cp1252_fit(c: char) -> Option<char> {
    if cp1252_exact(c) {
        return Some(c);
    }
    let unit = u16::try_from(u32::from(c)).ok()?;
    let byte = [&BEST_FIT[..], &BEST_FIT_SINCE_PUBLISHED[..]]
        .into_iter()
        .find_map(|table| {
            table
                .binary_search_by_key(&unit, |&(from, _)| from)
                .ok()
                .map(|index| table[index].1)
        })?;
    Some(decode_byte(byte))
}

fn decode_byte(byte: u8) -> char {
    let unit = match byte {
        0x80..=0x9F => HIGH[usize::from(byte - 0x80)],
        _ => u16::from(byte),
    };
    char::from_u32(u32::from(unit)).unwrap_or('?')
}

/// Whether `c` is a control character: U+0000 to U+001F, or U+007F to U+009F. (The five of those
/// code points Windows-1252 leaves undefined are among them.)
fn is_control(c: char) -> bool {
    matches!(u32::from(c), 0x00..=0x1F | 0x7F..=0x9F)
}

/// Whether `c` is a character the hub refuses in any text it keeps or publishes: a control
/// character, a line or paragraph separator, a bidirectional control, a zero-width or invisible
/// formatting character, or one of the two noncharacters XML 1.0 forbids. Text holding one could
/// hide what it says, or make two names look one.
#[must_use]
pub fn is_hidden(c: char) -> bool {
    matches!(
        u32::from(c),
        0x00..=0x1F
            | 0x7F..=0x9F
            | 0x061C
            | 0x180E
            | 0x200B..=0x200F
            | 0x2028..=0x202E
            | 0x2060..=0x2064
            | 0x2066..=0x2069
            | 0xFEFF
            | 0xFFF9..=0xFFFB
            | 0xFFFE
            | 0xFFFF
    )
}

/// Every character of `s` narrowed to one Windows-1252 can hold: kept, best-fitted, or `?`.
/// Control characters are dropped, except that a line break or tab becomes one space.
fn narrow(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '\r' | '\n' | '\t' | '\u{2028}' | '\u{2029}' | '\u{85}') {
            out.push(' ');
            continue;
        }
        if is_control(c) {
            continue;
        }
        match cp1252_fit(c) {
            // A best fit may itself be a control character (none is, in the published table);
            // dropping it keeps the rule.
            Some(f) if is_control(f) => {}
            Some(f) => out.push(f),
            None => out.push('?'),
        }
    }
    out
}

/// `s` with every name-link run taken out: any `<tell:...>` opening up to its `>`, and any
/// `<\tell>` closing, matched without regard to case. An opening with no `>` loses its `<`.
fn strip_markup(s: &str) -> String {
    let lower: Vec<char> = s.chars().flat_map(char::to_lowercase).collect();
    let chars: Vec<char> = s.chars().collect();
    // `to_lowercase` can lengthen a character; markup is ASCII, so compare on a char-for-char
    // lower-cased copy only when the lengths agree, else fall back to an ASCII-only fold.
    let fold: Vec<char> = if lower.len() == chars.len() {
        lower
    } else {
        chars.iter().map(char::to_ascii_lowercase).collect()
    };
    let starts = |i: usize, pat: &str| {
        pat.chars()
            .enumerate()
            .all(|(k, p)| fold.get(i + k) == Some(&p))
    };
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '<' && (starts(i, "<tell:") || starts(i, "<tell ")) {
            match chars[i..].iter().position(|&c| c == '>') {
                Some(end) => i += end + 1,
                None => i += 1,
            }
            continue;
        }
        if chars[i] == '<' && (starts(i, "<\\tell>") || starts(i, "</tell>")) {
            i += 7;
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// A remote line made safe: line breaks become spaces, control characters go, name-link markup
/// goes, every character becomes one Windows-1252 holds (its best fit, else `?`), runs of spaces
/// at the ends are trimmed, and the result is cut at [`MAX_LINE_CHARS`].
#[must_use]
pub fn sanitize_line(text: &str) -> String {
    // Markup first, so a link split by a control character cannot reassemble after it goes;
    // and again after narrowing, so a fullwidth bracket that best-fits to `<` cannot either.
    let mut s = strip_markup(&narrow(&strip_markup(text)));
    loop {
        let again = strip_markup(&s);
        if again == s {
            break;
        }
        s = again;
    }
    // Cut, then trim again: a cut just after a space would leave one at the end, which a second
    // pass would take off.
    let cut: String = s.trim().chars().take(MAX_LINE_CHARS).collect();
    cut.trim_end().to_owned()
}

/// Why a remote name was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NameRefused {
    /// It holds `^` or `&`, the Olthoi speech marks.
    OlthoiMark,
    /// Nothing is left of it once the other rules have run.
    Empty,
}

/// A remote sender's display name made safe, or refused. The line rules' characters go, and every
/// leading `+` (the administrator marker) and white space before or between them; a name holding
/// `^` or `&` anywhere is refused rather than changed, because a world that sends one is either
/// broken or impersonating Olthoi speech.
///
/// # Errors
/// [`NameRefused`] when the name holds an Olthoi mark or nothing remains of it.
pub fn sanitize_name(name: &str) -> Result<String, NameRefused> {
    let s = sanitize_line(name);
    let s = s.trim_start_matches(|c: char| c == '+' || c.is_whitespace());
    if s.contains('^') || s.contains('&') {
        return Err(NameRefused::OlthoiMark);
    }
    if s.is_empty() {
        return Err(NameRefused::Empty);
    }
    Ok(s.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Spec: text rules: line breaks become spaces and control characters go.
    #[test]
    fn line_breaks_become_spaces_and_control_characters_are_dropped() {
        assert_eq!(sanitize_line("one\r\ntwo\tthree"), "one  two three");
        assert_eq!(sanitize_line("a\u{0}b\u{7}c\u{9b}d"), "abcd");
    }

    /// Spec: text rules: name-link markup is taken out, in any case.
    #[test]
    fn name_link_markup_is_taken_out_in_any_case() {
        assert_eq!(
            sanitize_line("hi <Tell:IIDString:0:Admin>Admin<\\Tell> there"),
            "hi Admin there"
        );
        assert_eq!(sanitize_line("<TELL:x:0:y>y<\\TELL>"), "y");
        assert_eq!(sanitize_line("<tell:unclosed"), "tell:unclosed");
        assert_eq!(
            sanitize_line("<te\nll:x>"),
            "<te ll:x>",
            "a broken run is not markup"
        );
        assert_eq!(
            sanitize_line("\u{ff1c}tell:x\u{ff1e}y"),
            "y",
            "fullwidth brackets best-fit to markup and are taken out too"
        );
        assert_eq!(
            sanitize_line("a < b > c"),
            "a < b > c",
            "plain brackets stay"
        );
    }

    /// Spec: text rules: every character becomes one Windows-1252 can hold.
    #[test]
    fn every_character_becomes_one_windows_1252_holds() {
        assert_eq!(sanitize_line("caf\u{e9} \u{20ac}5"), "caf\u{e9} \u{20ac}5");
        assert_eq!(sanitize_line("\u{101}\u{142}"), "al", "best fit");
        assert_eq!(
            sanitize_line("ok \u{1f600}!"),
            "ok ?!",
            "no fit: one ? a character"
        );
        assert_eq!(sanitize_line("\u{4e2d}\u{6587}"), "??");
        assert!(sanitize_line("\u{1f600}\u{4e2d}abc")
            .chars()
            .all(cp1252_exact));
    }

    /// Spec: text rules: a line is cut at 255 characters.
    #[test]
    fn a_line_is_cut_at_255_characters() {
        let long = "x".repeat(300);
        assert_eq!(sanitize_line(&long).chars().count(), MAX_LINE_CHARS);
        assert_eq!(sanitize_line("  padded  "), "padded");
    }

    /// Spec: text rules: names lose the administrator marker and refuse the Olthoi marks.
    #[test]
    fn a_name_loses_the_administrator_marker_and_refuses_the_olthoi_marks() {
        assert_eq!(
            sanitize_name("++Admin@coldeve").as_deref(),
            Ok("Admin@coldeve")
        );
        assert_eq!(sanitize_name("Bob^"), Err(NameRefused::OlthoiMark));
        assert_eq!(sanitize_name("Bob&"), Err(NameRefused::OlthoiMark));
        assert_eq!(sanitize_name("Bo&b"), Err(NameRefused::OlthoiMark));
        assert_eq!(sanitize_name("^Bob"), Err(NameRefused::OlthoiMark));
        assert_eq!(sanitize_name("+ +Bob").as_deref(), Ok("Bob"));
        assert_eq!(sanitize_name("+"), Err(NameRefused::Empty));
    }

    /// Spec: text rules: a cut just after a space leaves no space at the end.
    #[test]
    fn a_cut_that_lands_after_a_space_leaves_none_at_the_end() {
        let line = format!("{} more", "x".repeat(MAX_LINE_CHARS - 1));
        let once = sanitize_line(&line);
        assert_eq!(once, "x".repeat(MAX_LINE_CHARS - 1));
        assert_eq!(sanitize_line(&once), once);
    }

    /// Spec: text rules: the rules are idempotent.
    #[test]
    fn sanitising_twice_changes_nothing() {
        for s in [
            "<tell:a>b<\\tell>",
            "x\u{1f600}y",
            "\u{ff1c}\u{ff1c}tell:x\u{ff1e}\u{ff1e}",
            "<<tell:x>tell:y>z",
            "plain",
        ] {
            let once = sanitize_line(s);
            assert_eq!(sanitize_line(&once), once, "{s:?}");
        }
        for n in ["+ +Bob", "++Admin@coldeve", " Ann "] {
            let once = sanitize_name(n).expect("a name");
            assert_eq!(sanitize_name(&once).as_deref(), Ok(once.as_str()), "{n:?}");
        }
    }
}
