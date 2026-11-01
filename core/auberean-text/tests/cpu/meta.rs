//! The meta language, construct by construct.

use auberean_text::meta::{render_string, RenderCode};

use crate::{english, wide};

/// Render fragments and values (no variable names) into a marker, answering the error code and
/// what the output holds.
fn render(fragments: &[&str], values: &[&str]) -> (RenderCode, String) {
    render_named(fragments, values, &[])
}

fn render_named(fragments: &[&str], values: &[&str], names: &[&str]) -> (RenderCode, String) {
    let f: Vec<Option<Vec<u16>>> = fragments.iter().map(|s| Some(wide(s))).collect();
    let v: Vec<Vec<u16>> = values.iter().map(|s| wide(s)).collect();
    let n: Vec<Vec<u16>> = names.iter().map(|s| wide(s)).collect();
    let mut out = wide("<untouched>");
    let mut outside = Vec::new();
    let lang = english();
    let err = render_string(&f, &v, &n, &mut out, &mut outside, 1, Some(&lang));
    (err.code, String::from_utf16_lossy(&out))
}

fn ok(s: &str) -> (RenderCode, String) {
    (RenderCode::Success, s.to_owned())
}

/// A value is written without its flags (`[...]`, the text before them trimmed on the right); a
/// value without flags of its own gets them from what it is: `1` when it is a number the language
/// takes as singular, `+` when it is a number not below zero, an empty value counting as zero.
///
/// Behaviour: text.meta.value-flags
#[test]
fn a_value_is_written_without_its_flags_and_a_bare_number_gets_its_own() {
    assert_eq!(
        render(&["You see ", "."], &["a sword  [ms]"]),
        ok("You see a sword.")
    );
    let coins = |v: &str| render(&["", " {coin[1+]|coins[+]|debt}"], &[v]);
    assert_eq!(coins("1"), ok("1 coin"), "one: singular and not negative");
    assert_eq!(coins("2"), ok("2 coins"));
    assert_eq!(
        coins("-3"),
        ok("-3 debt"),
        "a negative number has neither flag"
    );
    assert_eq!(
        coins(""),
        ok("coins"),
        "empty is zero: not singular in English, not negative"
    );
    assert_eq!(
        coins("5 "),
        ok("5 debt"),
        "a number with anything after it is not one"
    );
}

/// A choice writes the alternative whose flags best match its variable's: 2 for each flag the
/// variable has and each `!flag` it lacks, 1 for an alternative without flags; the last of the
/// best wins, and a choice nothing scores for writes nothing.
///
/// Behaviour: text.meta.choice-by-flags
#[test]
fn a_choice_writes_the_last_best_scoring_alternative_or_nothing() {
    assert_eq!(
        render(&["", "{a[+]|b[+]}"], &["2"]),
        ok("2b"),
        "a tie: the last"
    );
    assert_eq!(
        render(&["", "{a[1]|b[!+]}"], &["2"]),
        ok("2"),
        "nothing scores"
    );
    assert_eq!(
        render(&["", "{a[!1]|b}"], &["2"]),
        ok("2a"),
        "a lacked flag scores"
    );
    assert_eq!(
        render(&["", "{a[1+]|b[+]}"], &["1"]),
        ok("1a"),
        "more matched flags win"
    );
}

/// `#n:` numbers the variable or the choice after it; a choice follows the variable with its
/// number, an unnumbered one the unnumbered variable or else the one numbered -1, and a variable
/// numbered below zero is not written.
///
/// Behaviour: text.meta.numbered-choices
#[test]
fn a_numbered_choice_follows_its_variable_and_a_negative_variable_is_not_written() {
    assert_eq!(render(&["#2:", " #2:{cat[1]|cats}"], &["1"]), ok("1 cat"));
    assert_eq!(render(&["#2:", " #2:{cat[1]|cats}"], &["4"]), ok("4 cats"));
    assert_eq!(render(&["#-1:", " {is[1]|are}"], &["1"]), ok("is"));
    assert_eq!(render(&["#-1:", " {is[1]|are}"], &["2"]), ok("are"));
}

/// A rule's error keeps the reading going and only the last error counts; any error leaves the
/// output as it was, unless it comes while writing, which leaves what was written before it: two
/// alternatives without flags in one choice, two variables with one number, two unnumbered
/// variables with an unnumbered choice, a choice whose variable is missing, a syntax error.
///
/// Behaviour: text.meta.render-errors
#[test]
fn an_error_leaves_the_output_as_it_was_or_written_up_to_it() {
    let untouched = |c: RenderCode| (c, "<untouched>".to_owned());
    assert_eq!(
        render(&["", "{a|b}"], &["1"]),
        untouched(RenderCode::DuplicateChoices)
    );
    assert_eq!(
        render(&["#1:", "#1:", ""], &["a", "b"]),
        untouched(RenderCode::DuplicateIds)
    );
    assert_eq!(
        render(&["", " and ", ""], &["a", "b"]),
        ok("a and b"),
        "two unnumbered variables are fine without an unnumbered choice"
    );
    assert_eq!(
        render(&["", " and ", " {x|y[1]}"], &["a", "b"]),
        untouched(RenderCode::DuplicateIds)
    );
    assert_eq!(render(&["{oops"], &[]), untouched(RenderCode::Syntax));
    assert_eq!(
        render(&["Count: #3:", " {a[x]|b}!"], &["7"]),
        (RenderCode::Syntax, "Count: 7 ".to_owned()),
        "the choice has no variable: written up to it"
    );
}

/// `^` capitalizes a value's first letter after any white space or punctuation (a `\`, `/` or `@`
/// counts as the first letter and stays as it is).
///
/// Behaviour: text.meta.capitals
#[test]
fn a_capital_is_the_first_letter_after_spaces_and_punctuation() {
    assert_eq!(
        render(&["^", " arrives."], &["...bob"]),
        ok("...Bob arrives.")
    );
    assert_eq!(render(&["^", ""], &["/bob"]), ok("/bob"));
    assert_eq!(render(&["^", ""], &["\u{e9}lan"]), ok("\u{c9}lan"));
}

/// The result is trimmed on the left and each run of spaces and tabs becomes its first, unless
/// the text says `{{keepspaces}}`.
///
/// Behaviour: text.meta.spaces
#[test]
fn excess_spaces_collapse_unless_the_text_keeps_them() {
    assert_eq!(render(&["  Two  spaces \t "], &[]), ok("Two spaces "));
    assert_eq!(
        render(&["{{keepspaces}}  Two  spaces"], &[]),
        ok("  Two  spaces")
    );
}

/// An escaped mark is text, kept escaped (the caller unescapes the whole result), and `\q` is a
/// double quote.
///
/// Behaviour: text.meta.escapes
#[test]
fn an_escaped_mark_is_text_and_stays_escaped() {
    assert_eq!(
        render(&["a \\{b\\} \\q c\\[x\\]"], &[]),
        ok("a \\{b\\} \" c\\[x\\]")
    );
}

/// A text marked `{{item}}` is a generated item name: its prefix, middle and suffix variables
/// (named `PRE`, `NAME` or `MID`, `POST`) are written, then in brackets the letters of their flags
/// that the language allows each.
///
/// Behaviour: text.meta.item-names
#[test]
fn an_item_name_carries_its_parts_letters() {
    assert_eq!(
        render_named(
            &["{{item}}", " ", " ", ""],
            &["Ancient[vx]", "Sword[mnq]", "of Doom[s]"],
            &["PRE", "NAME", "POST"]
        ),
        ok("Ancient Sword of Doom[vmns]")
    );
}

/// `M` and `F` in a value's flags stand for the language's male and female letters (and
/// themselves).
///
/// Behaviour: text.meta.gender-letters
#[test]
fn m_and_f_stand_for_the_languages_letters() {
    let says = |who: &str| render(&["", " says {he[M]|she[F]|it} will."], &[who]);
    assert_eq!(says("Jane[F]"), ok("Jane says she will."));
    assert_eq!(says("John[M]"), ok("John says he will."));
    assert_eq!(
        render(&["", "{x[mnM]}"], &["a[M]"]),
        ok("ax"),
        "M is m, n and M"
    );
}

fn text(s: &[u16]) -> String {
    String::from_utf16_lossy(s)
}

/// A player's name becomes a value of the meta language: its own flags cut off, trimmed, and
/// `[M]` or `[F]` after it, with `v` for a name beginning with a vowel. A name ending in `s` gets
/// no `s` (the client looks one unit past the name's end).
///
/// Behaviour: text.meta.player-name-tags
#[test]
fn a_player_name_carries_its_sex_and_a_leading_vowel_and_never_an_s() {
    use auberean_text::meta::apply_meta_tags_to_name;
    let tag = |s: &str, male: bool| text(&apply_meta_tags_to_name(&wide(s), male));
    assert_eq!(tag("  Bass [x]  ", true), "Bass [M]");
    assert_eq!(tag(" Alice ", false), "Alice [Fv]");
    assert_eq!(tag("Ess", true), "Ess [Mv]", "no s for the last letter");
    assert_eq!(tag("", true), " [M]");
    assert_eq!(
        tag("Ugo\\[q]", false),
        "Ugo\\[q] [Fv]",
        "an escaped bracket is no flags"
    );
}

/// A text splits into what comes before its flags (trimmed on the right) and the flags: from
/// its first `[` (escapes not considered) to the next `]` or the end. A `[` first leaves the text
/// whole; a `[` last leaves the flags as `[`.
///
/// Behaviour: text.meta.retrieve-flags
#[test]
fn a_text_splits_into_its_words_and_its_flags() {
    use auberean_text::meta::retrieve_meta_letters;
    let split = |s: &str| {
        let (a, b) = retrieve_meta_letters(&wide(s));
        (text(&a), text(&b))
    };
    let pair = |a: &str, b: &str| (a.to_owned(), b.to_owned());
    assert_eq!(split("Sword [mfn]"), pair("Sword", "mfn"));
    assert_eq!(split("plain"), pair("plain", ""));
    assert_eq!(split("t\t[mf] y"), pair("t", "mf"));
    assert_eq!(split("x []"), pair("x", ""));
    assert_eq!(split("a [b"), pair("a", "b"), "no closing bracket");
    assert_eq!(split("[ab]"), pair("[ab]", "ab"), "a bracket first");
    assert_eq!(split("abc["), pair("abc", "["), "a bracket last");
}
