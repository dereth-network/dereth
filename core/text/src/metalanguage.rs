//! The string-table meta-language renderer — the pass every table row goes through.
//!
//! This is the subset of the string-table grammar the shipped rows actually use. It follows the
//! client's implementation exactly, because treating *choice alternatives* as if they were
//! *variables* differs by a space on the screen.
//!
//! # Where it sits
//!
//! String-table lookup with `(out, id, vars, use-meta-language)` has two paths. The one
//! [`StringResolver::resolve_variants`](crate::StringResolver::resolve_variants) models is
//! use-meta-language off: interleave the fragments and the values and stop. The one
//! the string info's internal query actually takes is on:
//!
//! ```text
//! values = [vars[variables[i]] or "" for i in 0..len(variables)]
//! render the fragments with values, variable names, outside variables, and flags = 1
//! ```
//!
//! The renderer tokenises, parses and renders, then — because `flags & 1` — collapses excess
//! spaces. **The render runs on the *escaped*
//! text**: the unescape is applied afterwards by the string info's outer query,
//! which is what lets a row write a literal `{` as `\{` and is why [`render`] takes escaped
//! fragments and the caller unescapes the answer.
//!
//! # The grammar
//!
//! The tokenizer's input builder makes one string out of the fragments and values:
//!
//! ```text
//! input = ""
//! for i in 0 .. len(fragments)-1:
//!     input += fragments[i]
//!     if i < len(values): input += U+0001 + values[i] + U+0001
//! ```
//!
//! so a `[` inside `U+0001`…`U+0001` came from the *value* and a `[` outside came from the
//! *template*. That single sentinel is the whole reason the two `[`-handling rules differ:
//!
//! | construct | rule | text |
//! |---|---|---|
//! | `U+0001 value[flags] U+0001` | the variable rule | split at the first unescaped `[`, then **right-trimmed only** |
//! | `{ alt[flags] \| alt[flags] }` | the text rule + the flags rule | **not trimmed** — the text rule only builds and appends a node, and the flags rule only stores the node's flags |
//!
//! The two text forms differ, and it
//! matters: `ID_DurationFormat`'s separator between two duration terms is the alternative
//! `{ [!b]}`, whose text is a **single space**. Right-trimming it would render *"1 year2 months"*.
//!
//! The number-label rule: `#` `ws*` `-`? `digit+` `ws*` `:` `ws*`, and it only counts as a
//! label when the next character is `U+0001`, `^` or `{` and when
//! the lexer is not inside a choice block. The id it parses is attached to the node that follows.
//!
//! The match score for `(var_flags, choice_flags)`, verbatim: an empty `choice_flags` scores
//! **1** (the default alternative); otherwise `!x` scores 2 when `x` is **absent** from
//! `var_flags` and a bare `x` scores 2 when it is **present**.
//!
//! The choice rule keeps the alternative with `score >= best` and
//! `score > 0`, so **the last** alternative with the maximum score wins and a block whose
//! alternatives all score 0 emits nothing at all.
//!
//! Auto-derived variable flags, the `no [` arm of the variable rule:
//! `wcstol(text, &end, 10)` with `*end == 0` and a singular `n` appends `'1'`; an **empty**
//! text appends `'b'`. The singular test has no language metadata in this build and
//! falls back to `n == 1`.
//!
//! # Outside-info blocks — `{{name}}`, not `$name`
//!
//! The **lexer** spells an outside variable `{{` … `}}`, not `$name`. The lexer
//! tests a `{` for a second `{`, and when not inside a choice block scans to the next
//! `}}` (or to the end of the input) and returns token `0x105`, which the grammar's case `0x11`
//! hands to the outside-info-block rule as an outside-variable node.
//!
//! One shipped row uses it — `{{keepspaces}}Lore Master  Quiz Night`, in table `0x2300000E` —
//! and it is the **only** row in the whole dat whose two readings differ by more than a space
//! collapse [measured over every shipped string table]. The option is what stops
//! the excess-space trim eating the two spaces the title is authored with.
//!
//! `item` (the treasure-name flag) is recognised and harvested and then ignored, because pass 4
//! is not here.
//!
//! # What is not here, and is named rather than guessed
//!
//! * The `PRE`/`MID`/`NAME`/`POST` treasure-letter machinery in the renderer's head and
//!   fourth pass. That is generated **item names**, not interface text.
//! * `M` / `F` meta-letter expansion in variable flags, which needs language metadata.
//! * `^` capitalisation is honoured, but the capitalise helper is modelled as
//!   `to_uppercase` on the first character.
//!
//! Each of those is a *reported absence*: the parser recognises the syntax and leaves it alone
//! rather than mangling it.

/// The tokenizer input's sentinel — `U+0001` around every substituted value.
const SENTINEL: char = '\u{1}';

/// One parsed node. The root node has no representation here; see the header.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Node {
    /// A text node at the top level: literal output.
    Text(String),
    /// A variable node — a substituted value with its grammar flags.
    Var {
        id: i32,
        text: String,
        flags: String,
        capital: bool,
    },
    /// A choice block — the alternatives, each `(text, flags)`, in source order.
    Choice {
        id: i32,
        alts: Vec<(String, String)>,
    },
    /// An outside variable — a `{{name}}` out-of-band render option. Emits nothing; its name is
    /// harvested into the outside-variable list by the client's pass 1.
    OutsideVar(String),
}

/// The match score.
///
/// Transcribed rather than summarised because the `!` arm consumes **two** characters and a loop
/// that advanced by one would score `!b` as "`!` absent" plus "`b` present".
#[must_use]
fn match_score(var_flags: &str, choice_flags: &str) -> i32 {
    if choice_flags.is_empty() {
        return 1;
    }
    let f: Vec<char> = choice_flags.chars().collect();
    let mut score = 0;
    let mut i = 0;
    while i < f.len() {
        if f[i] == '!' {
            if let Some(d) = f.get(i + 1) {
                if !var_flags.contains(*d) {
                    score += 2;
                }
            }
            i += 1;
        } else if var_flags.contains(f[i]) {
            score += 2;
        }
        i += 1;
    }
    score
}

/// The choice rule — `>=` over a `best` seeded at `-1`, with
/// a `score > 0` gate, so the **last** maximum wins and an all-zero block chooses nothing.
#[must_use]
fn choose<'a>(var_flags: &str, alts: &'a [(String, String)]) -> Option<&'a str> {
    let mut best = -1;
    let mut chosen = None;
    for (text, flags) in alts {
        let s = match_score(var_flags, flags);
        if s >= best && s > 0 {
            best = s;
            chosen = Some(text.as_str());
        }
    }
    chosen
}

/// The singular-number test used when no language metadata is loaded.
#[must_use]
fn is_number_singular(n: i64) -> bool {
    n == 1
}

/// The variable rule's no-`[` arm: `wcstol` must consume the **whole** text, and then the
/// singular test.
#[must_use]
fn auto_flags(text: &str) -> String {
    let mut f = String::new();
    if let Ok(n) = text.parse::<i64>() {
        if is_number_singular(n) {
            f.push('1');
        }
    }
    if text.is_empty() {
        f.push('b');
    }
    f
}

/// Is the character at `i` an **unescaped** occurrence of `c`? The scan skips the character after
/// every backslash, so `\]` does not close a flag list.
#[must_use]
fn find_unescaped(s: &[char], from: usize, c: char) -> Option<usize> {
    let mut i = from;
    while i < s.len() {
        if s[i] == '\\' {
            i += 2;
            continue;
        }
        if s[i] == c {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// The number-label rule — `Some((id, index after the label))`, or `None` when this `#` is
/// plain text.
#[must_use]
fn number_label(s: &[char], at: usize) -> Option<(i32, usize)> {
    let mut i = at;
    if s.get(i) != Some(&'#') {
        return None;
    }
    i += 1;
    while matches!(s.get(i), Some(' ' | '\t')) {
        i += 1;
    }
    let mut digits = String::new();
    if s.get(i) == Some(&'-') {
        digits.push('-');
        i += 1;
    }
    // `iswdigit` must succeed at least once, before and after the optional sign.
    if !s.get(i).is_some_and(|c| c.is_ascii_digit()) {
        return None;
    }
    while s.get(i).is_some_and(|c| c.is_ascii_digit()) {
        digits.push(s[i]);
        i += 1;
    }
    while matches!(s.get(i), Some(' ' | '\t')) {
        i += 1;
    }
    if s.get(i) != Some(&':') {
        return None;
    }
    i += 1;
    while matches!(s.get(i), Some(' ' | '\t')) {
        i += 1;
    }
    // The label only binds when a variable, a `^` or a choice block follows it.
    match s.get(i) {
        Some(&SENTINEL | &'^' | &'{') => Some((digits.parse().ok()?, i)),
        _ => None,
    }
}

/// Split `value[flags]` the way the variable rule does: text before the first unescaped `[`, **right**
/// trimmed, and the flags up to the first unescaped `]`.
#[must_use]
fn split_var(content: &[char]) -> (String, String) {
    match find_unescaped(content, 0, '[') {
        Some(b) => {
            let text: String = content[..b].iter().collect();
            let text = text.trim_end().to_owned();
            let end = find_unescaped(content, b + 1, ']').unwrap_or(content.len());
            // `' '` is dropped; `M`/`F` are the meta-letter expansions this build cannot make and
            // are passed through, which is a reported absence (see the header).
            let flags: String = content[b + 1..end].iter().filter(|c| **c != ' ').collect();
            (text, flags)
        }
        None => {
            let text: String = content.iter().collect();
            let flags = auto_flags(&text);
            (text, flags)
        }
    }
}

/// Split one choice alternative. **No trim** — see the header's table.
#[must_use]
fn split_alt(content: &[char]) -> (String, String) {
    match find_unescaped(content, 0, '[') {
        Some(b) => {
            let text: String = content[..b].iter().collect();
            let end = find_unescaped(content, b + 1, ']').unwrap_or(content.len());
            let flags: String = content[b + 1..end].iter().filter(|c| **c != ' ').collect();
            (text, flags)
        }
        None => (content.iter().collect(), String::new()),
    }
}

/// The grammar's parse over the assembled input.
#[must_use]
fn parse(input: &str) -> Vec<Node> {
    let s: Vec<char> = input.chars().collect();
    let mut out: Vec<Node> = Vec::new();
    let mut text = String::new();
    let mut pending_id = 0i32;
    let mut capital = false;
    let mut i = 0;
    while i < s.len() {
        let c = s[i];
        if c == '\\' {
            // The escape survives the render and is removed by the unescape afterwards.
            text.push(c);
            if let Some(n) = s.get(i + 1) {
                text.push(*n);
            }
            i += 2;
            continue;
        }
        if c == '#' {
            if let Some((id, next)) = number_label(&s, i) {
                pending_id = id;
                i = next;
                continue;
            }
            text.push(c);
            i += 1;
            continue;
        }
        if c == '^' {
            capital = true;
            i += 1;
            continue;
        }
        if c == SENTINEL {
            let end = find_unescaped(&s, i + 1, SENTINEL).unwrap_or(s.len());
            if !text.is_empty() {
                out.push(Node::Text(std::mem::take(&mut text)));
            }
            let (t, flags) = split_var(&s[i + 1..end]);
            out.push(Node::Var {
                id: pending_id,
                text: t,
                flags,
                capital,
            });
            pending_id = 0;
            capital = false;
            i = end + 1;
            continue;
        }
        if c == '{' {
            // The lexer's **first** test on a `{`: a second `{`
            // outside a choice block makes this token `0x105`, the outside-info block. The
            // scanner then runs to the next `}}` — or to the end of the string, which falls into
            // the same emit — and consumes both braces.
            // The shipped row `ID_Title_LoreMaster` is `{{keepspaces}}Lore
            // Master  Quiz Night`, and without this arm the parser would read `{keepspaces}` as a
            // choice block with no matching variable, emit nothing for it, leave the stray `}`
            // behind and then collapse the two spaces the row exists to protect.
            if s.get(i + 1) == Some(&'{') {
                let body_start = i + 2;
                let mut j = body_start;
                while j + 1 < s.len() && !(s[j] == '}' && s[j + 1] == '}') {
                    j += 1;
                }
                if !text.is_empty() {
                    out.push(Node::Text(std::mem::take(&mut text)));
                }
                let name: String = s[body_start..j.min(s.len())].iter().collect();
                out.push(Node::OutsideVar(name));
                i = if j + 1 < s.len() { j + 2 } else { s.len() };
                continue;
            }
            let end = find_unescaped(&s, i + 1, '}').unwrap_or(s.len());
            if !text.is_empty() {
                out.push(Node::Text(std::mem::take(&mut text)));
            }
            let body = &s[i + 1..end];
            let mut alts = Vec::new();
            let mut start = 0;
            loop {
                match find_unescaped(body, start, '|') {
                    Some(bar) => {
                        alts.push(split_alt(&body[start..bar]));
                        start = bar + 1;
                    }
                    None => {
                        alts.push(split_alt(&body[start..]));
                        break;
                    }
                }
            }
            out.push(Node::Choice {
                id: pending_id,
                alts,
            });
            pending_id = 0;
            i = end + 1;
            continue;
        }
        text.push(c);
        i += 1;
    }
    if !text.is_empty() {
        out.push(Node::Text(text));
    }
    out
}

/// The capitalise-first-letter helper.
#[must_use]
fn capitalize(s: &str) -> String {
    let mut it = s.chars();
    match it.next() {
        Some(c) => c.to_uppercase().collect::<String>() + it.as_str(),
        None => String::new(),
    }
}

/// The excess-space trim — a run of spaces and tabs collapses to the **first**
/// character of the run, and nothing else is touched.
#[must_use]
fn collapse_spaces(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_space = false;
    for c in s.chars() {
        let is_space = c == ' ' || c == '\t';
        if is_space && last_space {
            continue;
        }
        out.push(c);
        last_space = is_space;
    }
    out
}

/// The render over `(fragments, values, …, flags = 1)`, minus the
/// item-name machinery — see the module header.
///
/// `fragments` and `values` are **escaped**, as the dat stores them, and so is the answer: the
/// caller applies [`dereth_assets::escape::unescape`] afterwards, exactly where
/// the string info's outer query does.
#[must_use]
pub fn render(fragments: &[String], values: &[String]) -> String {
    // The tokenizer's input builder.
    let mut input = String::new();
    for (i, f) in fragments.iter().enumerate() {
        input.push_str(f);
        if let Some(v) = values.get(i) {
            input.push(SENTINEL);
            input.push_str(v);
            input.push(SENTINEL);
        }
    }
    let nodes = parse(&input);
    // The node render's **pass 1** harvests every outside variable into a list, which is the
    // list the end of the render searches for `"keepspaces"`.
    let keep_spaces = nodes
        .iter()
        .any(|n| matches!(n, Node::OutsideVar(name) if name == "keepspaces"));
    // The node render's pass 3, in source order.
    let mut out = String::new();
    for n in &nodes {
        match n {
            Node::Text(t) => out.push_str(t),
            // Pass 3's outside-variable row: *"nothing (already harvested)"*.
            Node::OutsideVar(_) => {}
            Node::Var { text, capital, .. } => {
                if *capital {
                    out.push_str(&capitalize(text));
                } else {
                    out.push_str(text);
                }
            }
            Node::Choice { id, alts } => {
                // "find the variable whose `ID` equals the block's `ID`; if none and `ID == 0`,
                // look for the variable with `ID == -1`".
                let var = nodes
                    .iter()
                    .find_map(|m| match m {
                        Node::Var { id: v, flags, .. } if v == id => Some(flags.as_str()),
                        _ => None,
                    })
                    .or_else(|| {
                        if *id != 0 {
                            return None;
                        }
                        nodes.iter().find_map(|m| match m {
                            Node::Var { id: -1, flags, .. } => Some(flags.as_str()),
                            _ => None,
                        })
                    });
                // No matching variable is a syntax error, which aborts the render. Nothing this
                // workspace composes hits it; emitting nothing is the nearest non-destructive
                // answer and is stated rather than hidden.
                if let Some(flags) = var {
                    if let Some(t) = choose(flags, alts) {
                        out.push_str(t);
                    }
                }
            }
        }
    }
    // When `flags & 1` is set, trim the left side and then collapse excess spaces. The leading
    // trim is what turns `ID_DurationFormat`'s
    // `" 5 days 3 hours"` into `"5 days 3 hours"`.
    //
    // …**unless** `keepspaces` was harvested. The client checks flag bit 1 first, then suppresses
    // the complete trim operation when that option is present.
    // so the option suppresses **both** halves — the leading trim and the collapse — because the
    // guard wraps the whole trim call and that function is
    // a left-only trim followed by the excess-space collapse.
    if keep_spaces {
        return out;
    }
    collapse_spaces(out.trim_start())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: arm by arm.
    #[test]
    fn the_match_score_counts_two_per_satisfied_test_and_one_for_the_default() {
        assert_eq!(
            match_score("1", ""),
            1,
            "an empty alternative is the default"
        );
        assert_eq!(match_score("1", "1!b"), 4, "'1' present and 'b' absent");
        assert_eq!(match_score("", "1!b"), 2, "'1' absent, 'b' absent");
        assert_eq!(match_score("b", "1!b"), 0, "'1' absent and 'b' present");
        assert_eq!(match_score("b", "!b"), 0);
        assert_eq!(match_score("", "!b"), 2);
    }

    /// Oracle: the client's `>=` and its `score > 0` gate.
    #[test]
    fn a_tie_goes_to_the_last_alternative_and_a_zero_block_chooses_nothing() {
        let alts = vec![
            (" day".to_owned(), "1!b".to_owned()),
            (" days".to_owned(), "!b".to_owned()),
        ];
        assert_eq!(choose("1", &alts), Some(" day"), "singular wins 4 to 2");
        assert_eq!(
            choose("", &alts),
            Some(" days"),
            "2 and 2 -- the last one wins"
        );
        assert_eq!(
            choose("b", &alts),
            None,
            "a blank value chooses no term at all"
        );
    }

    /// Oracle: the variable rule's no-`[` arm.
    #[test]
    fn the_auto_flags_are_singular_and_blank_and_nothing_else() {
        assert_eq!(auto_flags("1"), "1");
        assert_eq!(auto_flags("2"), "");
        assert_eq!(auto_flags("0"), "");
        assert_eq!(auto_flags(""), "b");
        assert_eq!(
            auto_flags("5 "),
            "",
            "`wcstol` did not consume the whole text"
        );
        assert_eq!(
            auto_flags("-1"),
            "",
            "the singular test falls back to `n == 1`"
        );
    }

    /// Oracle: including the three characters that may follow.
    #[test]
    fn a_number_label_needs_a_colon_and_a_variable_or_a_choice_block_after_it() {
        let s: Vec<char> = "#1:{a}".chars().collect();
        assert_eq!(number_label(&s, 0), Some((1, 3)));
        let s: Vec<char> = "#12 : \u{1}x\u{1}".chars().collect();
        assert_eq!(number_label(&s, 0), Some((12, 6)));
        let s: Vec<char> = "#1:plain".chars().collect();
        assert_eq!(
            number_label(&s, 0),
            None,
            "text after the colon is not a label"
        );
        let s: Vec<char> = "#x:{a}".chars().collect();
        assert_eq!(number_label(&s, 0), None, "no digits");
    }

    /// **The alternative's text is not trimmed and the variable's is.** This distinction,
    /// missing from the earlier description, puts the space between two duration
    /// terms — see the module header.
    #[test]
    fn a_choice_alternative_keeps_its_spaces_and_a_variable_is_right_trimmed() {
        assert_eq!(
            split_alt(&" [!b]".chars().collect::<Vec<_>>()),
            (" ".to_owned(), "!b".to_owned())
        );
        assert_eq!(
            split_var(&"5  [1]".chars().collect::<Vec<_>>()),
            ("5".to_owned(), "1".to_owned())
        );
    }

    /// The shipped `ID_DurationFormat` (`0x23000006`), with the client's seven
    /// values in its order: years, months, weeks, days, hours, minutes, seconds.
    fn duration_fragments() -> Vec<String> {
        [
            "#1:",
            "#1:{ year[1!b]| years[!b]}#2:{ [!b]}#2:",
            "#2:{ month[1!b]| months[!b]}#3:{ [!b]}#3:",
            "#3:{ week[1!b]| weeks[!b]}#4:{ [!b]}#4:",
            "#4:{ day[1!b]| days[!b]}#5:{ [!b]}#5:",
            "#5:{ hour[1!b]| hours[!b]}#6:{ [!b]}#6:",
            "#6:{ minute[1!b]| minutes[!b]}#7:{ [!b]}#7:",
            "#7:{ second[1!b]| seconds[!b]}",
        ]
        .iter()
        .map(|s| (*s).to_owned())
        .collect()
    }

    fn duration(values: &[&str]) -> String {
        let v: Vec<String> = values.iter().map(|s| (*s).to_owned()).collect();
        render(&duration_fragments(), &v)
    }

    /// Oracle: the shipped row, rendered by hand through the node render's three passes.
    #[test]
    fn the_duration_row_renders_only_the_terms_that_are_not_blank() {
        assert_eq!(
            duration(&["", "", "", "5", "3", "", "42"]),
            "5 days 3 hours 42 seconds"
        );
        assert_eq!(duration(&["1", "", "", "", "", "", ""]), "1 year");
        assert_eq!(
            duration(&["", "2", "", "", "", "", "1"]),
            "2 months 1 second"
        );
        assert_eq!(
            duration(&["", "", "", "", "", "", ""]),
            "",
            "a brand new character"
        );
        assert_eq!(
            duration(&["2", "11", "3", "6", "23", "59", "59"]),
            "2 years 11 months 3 weeks 6 days 23 hours 59 minutes 59 seconds"
        );
    }

    /// The `{time[1]|times}` block every `ID_CharacterInfo_Augmentation_*` row carries: an
    /// **unnumbered** block, matched against the unnumbered variable.
    #[test]
    fn the_augmentation_plural_block_matches_the_default_variable() {
        let f: Vec<String> = [
            "You have augmented your Innate Strength ",
            " {time[1]|times}.\\n\\n",
        ]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
        assert_eq!(
            render(&f, &["1".to_owned()]),
            "You have augmented your Innate Strength 1 time.\\n\\n"
        );
        assert_eq!(
            render(&f, &["3".to_owned()]),
            "You have augmented your Innate Strength 3 times.\\n\\n"
        );
    }

    /// A row without markup is unchanged escapes and all.
    #[test]
    fn a_row_without_markup_is_unchanged_escapes_and_all() {
        let f: Vec<String> = ["Innate Strength: ", "\\nInnate Endurance: ", "\\n"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        let v = vec!["290".to_owned(), "285".to_owned()];
        assert_eq!(
            render(&f, &v),
            "Innate Strength: 290\\nInnate Endurance: 285\\n",
            "the trailing `\\n` is two characters, not whitespace, so the left trim cannot reach it"
        );
    }

    /// Excess-space trimming is a **global** collapse, and it is visible on rows that ship
    /// a double space. Pinned so the behaviour is a decision and not a surprise.
    #[test]
    fn a_run_of_spaces_collapses_to_one() {
        assert_eq!(collapse_spaces("a.  B"), "a. B");
        assert_eq!(collapse_spaces("a\t \tb"), "a\tb");
        assert_eq!(collapse_spaces("keep\\n  me"), "keep\\n me");
    }

    /// A literal `{` written as `\{` must survive the parse as text.
    #[test]
    fn an_escaped_brace_is_not_markup() {
        let f = vec!["\\{ not a block \\}".to_owned()];
        assert_eq!(render(&f, &[]), "\\{ not a block \\}");
    }
}
