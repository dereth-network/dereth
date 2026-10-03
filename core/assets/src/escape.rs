//! The string-table escape pass — the one every shipped string-table row goes through
//! before a text element sees it.
//!
//! The dat stores a string-table row in its *escaped* form: a line break is two
//! characters, a backslash and an `n`, and the meta-language's metacharacters are backslash-quoted
//! so the tokenizer does not read them as syntax. The string info's query, its literal-value
//! getter and the meta-language's render
//! all end in the meta-language's unescape, so **the unescape belongs to
//! the lookup, not to the element** — which is why it lives on `dereth_text::StringResolver`'s
//! contract in this rebuild rather than in any one host's resolver, and here, beside the string
//! table itself, so that a lookup made with no UI (the error box a failed first connection shows)
//! unescapes the same way. `dereth_text` and `dereth_ui::text` re-export both functions.
//!
//! # The single-character unescape
//!
//! The client maps `q`, `n`, `r`, and `t` to double quote, newline, carriage return, and tab. It
//! also accepts the ten metacharacters in `[]!{}#\|^$` as escaped versions of themselves. NUL and
//! every other character are reported as not being an escape.
//!
//! The switch maps `n`, `q`, `r` and `t` to their four characters and sends the other three
//! straight to the metacharacter table — so of the seven letters the range covers only **four**
//! are escapes and
//! `o`, `p` and `s` fall through to the metacharacter table like any other letter. The literal the
//! table scans is the UTF-16 string `[]!{}#\|^$`, the same ten characters
//! the is-escape and to-escape helpers are built around.
//!
//! **Fourteen escapes in total, and no numeric form.** There is no `\0`, no `\x41`, no `\123`: the
//! function is a switch and a `wcschr`, and everything else returns 0.
//!
//! # What `return 0` means to the caller
//!
//! The unescape walks the string one character at a time and only consumes a pair
//! when **both** `s[i] == '\\'` and `un_escaped_char(s[i + 1])` is an escape; otherwise it
//! appends `s[i]` and advances one. So a backslash in front of anything that is not in the set
//! **stays**, and so does the character after it, and a trailing backslash at the end of the
//! string survives as itself. That is why `unescape` is a scan and not a chain of `replace`s.

/// One character of the meta-language's single-character unescape.
///
/// `None` is the function's `return 0`: the character after the backslash is not an escape, so
/// neither character is consumed.
#[must_use]
pub fn un_escaped_char(c: char) -> Option<char> {
    match c {
        // The switch's four live arms.
        'n' => Some('\n'),
        'q' => Some('"'),
        'r' => Some('\r'),
        't' => Some('\t'),
        // `wcschr(L"[]!{}#\|^$", c)` — the meta-language's own metacharacters, each standing for
        // itself. `\\` is in this set, which is how a literal backslash is written in a table.
        '[' | ']' | '!' | '{' | '}' | '#' | '\\' | '|' | '^' | '$' => Some(c),
        _ => None,
    }
}

/// The whole-string unescape — [`un_escaped_char`] over a whole string.
///
/// Every string-table row reaches a text element through this. Table `0x23000002`'s copyright line
/// ships as `"...reserved.\nThis program is protected..."` with a real backslash in the dat and
/// `ID_CharGen_NamePrompt` ships as `\[ Name \]`; retail draws a line break in the first and
/// `[ Name ]` in the second, which is exactly this pass.
#[must_use]
pub fn unescape(s: String) -> String {
    // The retail unescape copies character by character either way; this is the same answer without
    // the allocation for the overwhelmingly common row that holds no escape at all.
    if !s.contains('\\') {
        return s;
    }
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        let mut peek = it.clone();
        match peek.next().and_then(un_escaped_char) {
            Some(u) => {
                out.push(u);
                it = peek;
            }
            // The single-character unescape returned 0, so the pair is not an escape: the
            // backslash is text and the character after it is offered to the next turn of the
            // loop on its own.
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the retail escape switch and its metacharacter literal.
    /// **All fourteen**, not the one that bit us.
    #[test]
    fn the_escape_set_is_the_four_letters_and_the_ten_metacharacters() {
        assert_eq!(un_escaped_char('n'), Some('\n'));
        assert_eq!(un_escaped_char('q'), Some('"'));
        assert_eq!(un_escaped_char('r'), Some('\r'));
        assert_eq!(un_escaped_char('t'), Some('\t'));
        for c in "[]!{}#\\|^$".chars() {
            assert_eq!(un_escaped_char(c), Some(c), "{c:?} is in wcschr's table");
        }
        // The three letters the switch's range covers whose arms are the default.
        for c in ['o', 'p', 's'] {
            assert_eq!(
                un_escaped_char(c),
                None,
                "{c:?} is inside 'n'..'t' but is not an escape"
            );
        }
        // No numeric form and no C-style extras.
        for c in ['0', '1', '7', 'x', 'u', 'a', 'b', 'f', 'v', 'e', ' ', '\0'] {
            assert_eq!(un_escaped_char(c), None, "{c:?} is not one of the fourteen");
        }
    }

    /// The shipped shapes: a section break, a quoted bracket, a literal backslash.
    #[test]
    fn the_shipped_rows_unescape_the_way_retail_draws_them() {
        assert_eq!(
            unescape("reserved.\\nThis program".into()),
            "reserved.\nThis program"
        );
        assert_eq!(unescape("\\[ Name \\]".into()), "[ Name ]");
        assert_eq!(unescape("a\\\\b".into()), "a\\b");
        assert_eq!(unescape("\\q".into()), "\"");
        assert_eq!(unescape("a\\tb".into()), "a\tb");
        assert_eq!(unescape("a\\rb".into()), "a\rb");
    }

    /// A pair is consumed only when the first character is `\` and the second is an escape — a
    /// backslash in front of anything
    /// outside the set is **not** consumed, and neither is the character after it.
    #[test]
    fn a_backslash_before_anything_else_stays_and_so_does_what_follows_it() {
        assert_eq!(unescape("a\\zb".into()), "a\\zb");
        assert_eq!(unescape("50\\% off".into()), "50\\% off");
        // A trailing backslash: the single-character unescape is handed the terminator and
        // answers 0.
        assert_eq!(unescape("trailing\\".into()), "trailing\\");
        // `\\n` is the escaped backslash followed by a plain `n`, not a line break.
        assert_eq!(unescape("a\\\\nb".into()), "a\\nb");
        // A row with no backslash at all comes back untouched.
        assert_eq!(
            unescape("Innate Strength: 290".into()),
            "Innate Strength: 290"
        );
    }
}
