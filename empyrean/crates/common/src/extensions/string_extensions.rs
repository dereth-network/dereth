// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Common/Extensions/StringExtensions.cs
//! `StringExtensions`.

// ACE: StringExtensions.StartsWithVowel
/// Whether the first letter (lower-cased) is one of `aeiou`; false for an empty string.
#[must_use]
pub fn starts_with_vowel(s: &str) -> bool {
    let Some(first) = s.chars().next() else {
        return false;
    };
    let first_letter = first.to_lowercase().next().unwrap_or(first);
    "aeiou".contains(first_letter)
}

// ACE: StringExtensions.Pluralize
/// The English plural for objects without a `PluralName`.
#[must_use]
pub fn pluralize(name: &str) -> String {
    if name.ends_with("us") {
        // This should be i but pcap shows "You have killed 4 Sarcophaguss! Your task is complete!"
        name.to_owned() + "s"
    } else if name.ends_with("ch")
        || name.ends_with('s')
        || name.ends_with("sh")
        || name.ends_with('x')
        || name.ends_with('z')
    {
        name.to_owned() + "es"
    } else if name.ends_with("th") {
        name.to_owned()
    } else {
        name.to_owned() + "s"
    }
}

/// `StringComparison.OrdinalIgnoreCase` on one character (simple upper-case mapping).
fn ordinal_ignore_case_eq(a: char, b: char) -> bool {
    a == b || a.to_uppercase().eq(b.to_uppercase())
}

// ACE: StringExtensions.TrimStart
/// Removes `trim_start` from the start of `result` if it is there, ignoring case (ordinal).
#[must_use]
pub fn trim_start(result: &str, trim_start: &str) -> String {
    let mut chars = result.char_indices();
    for t in trim_start.chars() {
        match chars.next() {
            Some((_, c)) if ordinal_ignore_case_eq(c, t) => {}
            _ => return result.to_owned(),
        }
    }
    let rest = chars.next().map_or(result.len(), |(i, _)| i);
    result[rest..].to_owned()
}

// ACE: StringExtensions.TrimEnd
/// Removes `trim_end` from the end of `result` if it is there, ignoring case (ordinal).
#[must_use]
pub fn trim_end(result: &str, trim_end: &str) -> String {
    let mut chars = result.char_indices().rev();
    let mut cut = result.len();
    for t in trim_end.chars().rev() {
        match chars.next() {
            Some((i, c)) if ordinal_ignore_case_eq(c, t) => cut = i,
            _ => return result.to_owned(),
        }
    }
    result[..cut].to_owned()
}

/// `Regex.Escape`: backslash-escapes `\ * + ? | { [ ( ) ^ $ . #` and space, and writes tab, new
/// line, carriage return and form feed as `\t`, `\n`, `\r`, `\f`.
fn regex_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '\\' | '*' | '+' | '?' | '|' | '{' | '[' | '(' | ')' | '^' | '$' | '.' | '#' | ' ' => {
                out.push('\\');
                out.push(c);
            }
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\u{c}' => out.push_str("\\f"),
            _ => out.push(c),
        }
    }
    out
}

// ACE: StringExtensions.WildCardToRegular
/// A `*` wildcard pattern as an anchored .NET regular expression.
#[must_use]
pub fn wild_card_to_regular(value: &str) -> String {
    "^".to_owned() + &regex_escape(value).replace("\\*", ".*") + "$"
}
