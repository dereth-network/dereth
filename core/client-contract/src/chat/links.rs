//! Web addresses in chat lines: which runs of a line are links, and what the player is asked
//! before one is opened.
//!
//! The final client had no web links in its chat. This client makes an `http://` or `https://`
//! address in any chat line a link the player can click (`CD-chat-links`); a click asks first
//! ([`confirm_prompt`](crate::chat::links::confirm_prompt)) and only a yes hands the address to
//! the browser.
//!
//! Only those two schemes make a link. Anything else stays text: another scheme (`file:`,
//! `javascript:`, `mailto:`), a bare `www.` name, a dotted name, a `Name@world` address. A link
//! starts where its scheme does, at the start of a word, and runs to the first space, quote,
//! angle bracket or other character an address cannot hold. The punctuation that ends a sentence
//! after it is not part of it, nor a closing bracket the address did not open, so
//! `(see https://example.com/a.)` links `https://example.com/a`.

use std::ops::Range;

/// The schemes that make a link, lowercase; a line may spell them in either case.
const SCHEMES: [&str; 2] = ["http://", "https://"];

/// Whether `c` ends the address it follows: a space, a control character, a quote, an angle
/// bracket, or a character an address cannot hold.
fn ends_address(c: char) -> bool {
    c.is_whitespace()
        || c.is_control()
        || matches!(
            c,
            '"' | '<'
                | '>'
                | '`'
                | '{'
                | '}'
                | '|'
                | '\\'
                | '^'
                | '\u{2018}'
                | '\u{2019}'
                | '\u{201C}'
                | '\u{201D}'
                | '\u{00AB}'
                | '\u{00BB}'
        )
}

/// How far the address in `text[start..end]` really runs: the sentence's closing punctuation and
/// any closing bracket the address did not open come off its end.
fn trimmed_end(text: &str, start: usize, mut end: usize) -> usize {
    while let Some(last) = text[start..end].chars().next_back() {
        let unopened = |open: char, close: char| {
            let run = &text[start..end];
            run.matches(close).count() > run.matches(open).count()
        };
        let drop = match last {
            '.' | ',' | '!' | '?' | ':' | ';' | '\'' | '*' => true,
            ')' => unopened('(', ')'),
            ']' => unopened('[', ']'),
            _ => false,
        };
        if !drop {
            break;
        }
        end -= last.len_utf8();
    }
    end
}

/// The links in `text`, as byte ranges, in order.
#[must_use]
pub fn find(text: &str) -> Vec<Range<usize>> {
    // ASCII lowercasing keeps every byte where it was, so a position in one is a position in both.
    let lower = text.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(found) = lower[at..].find("http") {
        let start = at + found;
        // "http" is ASCII, so the byte after it is a character boundary.
        at = start + "http".len();
        let Some(scheme) = SCHEMES.iter().find(|s| lower[start..].starts_with(*s)) else {
            continue;
        };
        // At the start of a word: `xhttp://` is not a link.
        if text[..start]
            .chars()
            .next_back()
            .is_some_and(char::is_alphanumeric)
        {
            continue;
        }
        let host = start + scheme.len();
        let end = text[host..]
            .find(ends_address)
            .map_or(text.len(), |n| host + n);
        let end = trimmed_end(text, host, end);
        // Something after the scheme that can begin a host name.
        if text[host..end]
            .chars()
            .next()
            .is_some_and(|c| c.is_alphanumeric() || c == '[')
        {
            out.push(start..end);
            at = end;
        }
    }
    out
}

/// The links in `text`, as character ranges, in order: the first character of each and the one
/// after its last.
#[must_use]
pub fn find_chars(text: &str) -> Vec<(usize, usize)> {
    let char_at = |byte: usize| text[..byte].chars().count();
    find(text)
        .into_iter()
        .map(|r| (char_at(r.start), char_at(r.end)))
        .collect()
}

/// Whether `url` is one whole link and nothing else: the only kind of address a click may ask to
/// open.
#[must_use]
pub fn is_link(url: &str) -> bool {
    matches!(find(url).as_slice(), [only] if *only == (0..url.len()))
}

/// What the player is asked before a link opens, the whole address in it.
#[must_use]
pub fn confirm_prompt(url: &str) -> String {
    format!("Open {url} in your browser?")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn links(text: &str) -> Vec<&str> {
        find(text).into_iter().map(|r| &text[r]).collect()
    }

    /// Behaviour: chat.links.only-a-web-address-in-a-chat-line-is-a-link
    #[test]
    fn only_an_http_or_https_address_is_a_link() {
        assert_eq!(
            links("see https://example.com/a and http://example.org"),
            ["https://example.com/a", "http://example.org"]
        );
        assert_eq!(
            links("HTTPS://Example.com/Path"),
            ["HTTPS://Example.com/Path"]
        );
        for not_a_link in [
            "file:///c:/windows/system32",
            "javascript:alert(1)",
            "mailto:someone@example.com",
            "ftp://example.com/file",
            "www.example.com",
            "go to example.com today",
            "Borin@Leafcull says hi",
            "x.y and a.b.c are names",
            "data:text/html,hello",
            "https://",
            "https:// spaced",
            "https://.",
            "xhttps://example.com",
            "httpx://example.com",
        ] {
            assert!(
                find(not_a_link).is_empty(),
                "{not_a_link:?} has no link, found {:?}",
                links(not_a_link)
            );
        }
    }

    /// Behaviour: chat.links.only-a-web-address-in-a-chat-line-is-a-link
    #[test]
    fn the_punctuation_ending_a_sentence_is_not_part_of_the_link() {
        for (line, link) in [
            ("Go to https://example.com.", "https://example.com"),
            ("https://example.com/a, then", "https://example.com/a"),
            (
                "Really? https://example.com/q?x=1!",
                "https://example.com/q?x=1",
            ),
            ("(see https://example.com/a.)", "https://example.com/a"),
            ("[https://example.com]", "https://example.com"),
            ("'https://example.com/it's'", "https://example.com/it's"),
            (
                "https://en.example.org/wiki/Foo_(bar).",
                "https://en.example.org/wiki/Foo_(bar)",
            ),
            (
                "Borin says, \"look at https://example.com/x\"",
                "https://example.com/x",
            ),
            ("<https://example.com/y>", "https://example.com/y"),
            ("https://example.com:8080/z;", "https://example.com:8080/z"),
        ] {
            assert_eq!(links(line), [link], "in {line:?}");
        }
    }

    /// Behaviour: chat.links.only-a-web-address-in-a-chat-line-is-a-link
    #[test]
    fn a_line_with_several_addresses_has_a_link_for_each() {
        let line = "[General] Ana@Leafcull says, \"https://a.example/1 or http://b.example/2, \
                    and not www.c.example\"";
        assert_eq!(links(line), ["https://a.example/1", "http://b.example/2"]);
        assert_eq!(
            links("https://a.example/1https://b.example/2 https://c.example"),
            [
                "https://a.example/1https://b.example/2",
                "https://c.example"
            ],
            "an address runs to the space, whatever it holds"
        );
    }

    /// Behaviour: none (the character positions the drawn interfaces place links by)
    #[test]
    fn a_link_s_characters_are_counted_past_wider_characters_before_it() {
        let line = "Café ☕ https://example.com/é ok";
        let (s, e) = find_chars(line)[0];
        let chars: Vec<char> = line.chars().collect();
        assert_eq!(
            chars[s..e].iter().collect::<String>(),
            "https://example.com/é"
        );
    }

    /// Behaviour: none (the guard a click passes before anything is asked)
    #[test]
    fn only_a_whole_link_may_be_opened() {
        assert!(is_link("https://example.com/a?b=c#d"));
        assert!(is_link("http://example.com"));
        assert!(!is_link("https://example.com/a."));
        assert!(!is_link(" https://example.com"));
        assert!(!is_link("https://example.com and more"));
        assert!(!is_link("javascript:alert(1)"));
        assert!(!is_link("file:///etc/passwd"));
        assert!(!is_link(""));
    }

    /// Behaviour: none (the wording of the question)
    #[test]
    fn the_question_names_the_whole_address() {
        assert_eq!(
            confirm_prompt("https://example.com/a?b=c"),
            "Open https://example.com/a?b=c in your browser?"
        );
    }
}
