//! Text tags — `<Type:Format:Data>` … `<\Type>`.
//!
//! This module parses the inline tags used by text, dialogs, and context menus.
//!
//! A tag turns a run of glyphs into a clickable link. In the original, **each glyph** carries
//! its tag so the hit test finds it by character position. This is the
//! mechanism behind clickable item names in chat, clickable player names and `@tell` links.
//!
//! `Type` and `Format` are names from the same `EnumMapper` table **0x18**. The shipped mapper is
//! `0x22000020` over base `0x22000031`: the base supplies `Invalid`, `DID`, `IID`, `IIDEnum` and
//! `IIDString`, while the derived table adds `Tell = 0x10000001`. The tag factory
//! requires both colons, maps **both** words, and chooses the payload kind from the second.
//!
//! **The end tag uses a backslash**, not a slash: the end-tag builder formats
//! `"<\\%ls>"`. Server-authored strings use it, so a parser that expects `</Tag>` finds nothing.
//! The *reader* is looser than the writer, though: glyph construction closes an open run on any following
//! `<…>` without testing the backslash or comparing the name — see [`parse`].

use crate::msg::NoticeId;

/// The four supported [`TextTag`] payload kinds, plus an invalid-format sentinel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagKind {
    /// A dat-file link. Data is `0x%08X`.
    Did,
    /// A world-object link. Data is one unsigned 32-bit id.
    Iid,
    /// Object plus an enum payload.
    IidEnum,
    /// Object plus a string payload. The only kind with a registered
    /// consumer in this build (the main chat panel).
    IidString,
    /// An invalid or unmapped format; `make_tag` rejects it before a run is constructed.
    Unknown,
}

impl TagKind {
    /// Match the **format** keyword. The tag factory selects by the second mapped word.
    #[must_use]
    pub fn from_format_keyword(k: &str) -> Self {
        match mapper_value(k) {
            Some(1) => Self::Did,
            Some(2) => Self::Iid,
            Some(3) => Self::IidEnum,
            Some(4) => Self::IidString,
            _ => Self::Unknown,
        }
    }

    /// The notice raised when the tag is clicked.
    #[must_use]
    pub const fn click_notice(self) -> Option<NoticeId> {
        Some(match self {
            Self::Did => NoticeId::DidTagClicked,
            Self::Iid => NoticeId::IidTagClicked,
            Self::IidEnum => NoticeId::IidEnumTagClicked,
            Self::IidString => NoticeId::IidStringTagClicked,
            Self::Unknown => return None,
        })
    }
}

/// `Tell`'s own `EnumMapper` value, the type of every tag the retail client
/// emits — and the **one** type that draws in the
/// element's tag colour rather than its text colour (the client compares the tag's type against
/// `0x10000001`).
/// See [`crate::text::TextElement::tag_font_color`].
pub const TELL: u32 = 0x1000_0001;

/// Resolve `word` through enum group `0x18` over the shipped `0x22000020` / `0x22000031` chain.
#[must_use]
fn mapper_value(word: &str) -> Option<u32> {
    Some(match word.to_ascii_lowercase().as_str() {
        "invalid" => 0,
        "did" => 1,
        "iid" => 2,
        "iidenum" => 3,
        "iidstring" => 4,
        "tell" => TELL,
        _ => return None,
    })
}

/// A tag's type, format and payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextTag {
    pub kind: TagKind,
    /// The mapped type keyword, retained for its notice payload value.
    pub type_keyword: String,
    /// The mapped format keyword that selects the tag's payload kind.
    pub format: String,
    /// Everything after the second colon, unparsed.
    pub data: String,
}

impl TextTag {
    /// The first word's `EnumMapper 0x18` value.
    #[must_use]
    pub fn type_id(&self) -> Option<u32> {
        mapper_value(&self.type_keyword)
    }

    /// The `0x%08X` prefix of the data, when there is one — the `DataID` or `ObjectID` every
    /// supported payload kind starts with.
    #[must_use]
    pub fn id(&self) -> Option<u32> {
        let s = self.data.split(':').next()?;
        parse_native_uint32(s)
    }

    /// The display text after the id, for the kinds whose data is `0x%08X:%ls`.
    #[must_use]
    pub fn payload(&self) -> Option<&str> {
        self.data.split_once(':').map(|(_, r)| r)
    }
}

/// One tagged run inside a string: the glyph range the tag covers, in **glyph** indices of the text
/// with the tag markup removed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagSpan {
    pub tag: TextTag,
    /// Inclusive start, exclusive end.
    pub start: usize,
    pub end: usize,
}

/// The result of stripping tags from a string.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TaggedText {
    /// The text with every `<…>` marker removed — what the glyph list actually holds.
    pub text: String,
    pub spans: Vec<TagSpan>,
}

/// The tag factory — "requires the string to start with `'<'` (0x3C) and be at
/// least 3 characters, then splits on `':'` and constructs the selected payload kind".
///
/// Takes the tag body **without** the angle brackets and returns `None` when it is an end tag or is
/// malformed.
#[must_use]
pub fn make_tag(raw: &str) -> Option<TextTag> {
    if raw.len() < 3 || !raw.starts_with('<') || !raw.ends_with('>') {
        return None;
    }
    let body = &raw[1..raw.len() - 1];
    if body.starts_with('\\') {
        return None; // an end tag
    }
    let (type_keyword, rest) = body.split_once(':')?;
    let (format, data) = rest.split_once(':')?;
    // The tag factory looks up both words in `EnumMapper` table 0x18. Either failed lookup rejects it;
    // after both succeed, only format values 1 through 4 construct a supported payload kind.
    mapper_value(type_keyword)?;
    let kind = TagKind::from_format_keyword(format);
    if kind == TagKind::Unknown {
        return None;
    }
    if kind == TagKind::IidString
        && !data
            .split_once(':')
            .is_some_and(|(id, value)| parse_native_uint32(id).is_some() && !value.is_empty())
    {
        return None;
    }
    Some(TextTag {
        kind,
        type_keyword: type_keyword.to_string(),
        format: format.to_string(),
        data: data.to_string(),
    })
}

/// The client's unsigned-32 test followed by its unsigned-32 parse:
/// `wcstoul(..., base=0)`, complete consumption, including its sign and base-prefix rules. The
/// retail process is 32-bit, and the test rejects `ERANGE`.
fn parse_native_uint32(s: &str) -> Option<u32> {
    let s = s.trim_start_matches(|c: char| c.is_ascii_whitespace());
    let (negative, digits) = match s.as_bytes().first() {
        Some(b'+') => (false, &s[1..]),
        Some(b'-') => (true, &s[1..]),
        _ => (false, s),
    };
    let (radix, digits) = if let Some(hex) = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        (16, hex)
    } else if digits.len() > 1 && digits.starts_with('0') {
        (8, digits)
    } else {
        (10, digits)
    };
    if digits.is_empty() {
        return None;
    }
    let mut value = 0_u32;
    for digit in digits.chars() {
        let digit = digit.to_digit(radix)?;
        value = value.checked_mul(radix)?.checked_add(digit)?;
    }
    Some(if negative {
        value.wrapping_neg()
    } else {
        value
    })
}

/// Parse a string containing `<Type:Format:Data>` … `<\Type>` runs.
///
/// This is the text element's glyph-building loop, which *is* the markup parser — the
/// glyph tail it feeds is [`crate::text::TextElement::stamp_tag_spans`]. Its whole state is one
/// open tag and one source index:
///
/// 1. A character that is not `'<'` becomes a glyph carrying the open tag, if any.
/// 2. On `'<'` the inner scan accumulates markup up to and including the first `'>'`,
///    stopping at the last real character, one short of the end.
///    With no `'>'` the scan falls out, the index is still the `'<'`'s, and the `'<'` is drawn.
/// 3. At `'>'` **with nothing open**, the tag factory is called on the whole
///    `<…>`. Failure leaves the index on the `'<'`, so the markup is drawn
///    character for character; success opens the run.
/// 4. At `'>'` **with a run open**, the run is closed — the tag factory is *not* called, the `'\'` is
///    not tested and the end-tag name is not compared. The original client called the open tag's
///    end-tag formatter and discarded its answer; it cleared the open tag
///    unconditionally. So there is no nesting and no matching: **any** `<…>` closes.
/// 5. Either way the index jumps past the `'>'` by the accumulated length **including** its
///    terminator, so the markup never becomes glyphs — and
///    the loop then falls straight into the glyph tail on the character it landed
///    on, **with no second `'<'` test**. The character after a completed tag is always drawn.
/// 6. The loop returns as soon as that jump reaches the end of the
///    string, so a run still open there simply covers no further glyphs.
///
/// Every run the retail client and this build compose is
/// `<Tell:IIDString:id:Name>Name<\Tell>`, whose name always matches and is never empty, so steps 4
/// and 5 are invisible on shipped content; they are transcribed for server-sent markup.
#[must_use]
pub fn parse(s: &str) -> TaggedText {
    let mut out = TaggedText::default();
    // Hold exactly one open tag, with the glyph index it opened at.
    let mut open: Option<(TextTag, usize)> = None;
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        // The inner scan runs to the first `'>'`, or falls out with the index unmoved.
        let close = if chars[i] == '<' {
            chars[i..].iter().position(|c| *c == '>').map(|p| i + p)
        } else {
            None
        };
        let Some(close) = close else {
            out.text.push(chars[i]);
            i += 1;
            continue;
        };
        if let Some((tag, start)) = open.take() {
            // A run is open: close it, whatever this `<…>` says.
            out.spans.push(TagSpan {
                tag,
                start,
                end: out.text.chars().count(),
            });
        } else {
            let raw: String = chars[i..=close].iter().collect();
            let Some(t) = make_tag(&raw) else {
                // The tag factory rejected it: the `'<'` falls through to the glyph tail as a character.
                out.text.push(chars[i]);
                i += 1;
                continue;
            };
            open = Some((t, out.text.chars().count()));
        }
        i = close + 1;
        if i >= chars.len() {
            break; // The skip reached the end: the function returns without another glyph.
        }
        // The glyph tail, reached with no `'<'` test on this character.
        out.text.push(chars[i]);
        i += 1;
    }
    // A run open at the end of the string covers the glyphs already emitted and no more.
    if let Some((tag, start)) = open {
        out.spans.push(TagSpan {
            tag,
            start,
            end: out.text.chars().count(),
        });
    }
    out.spans.sort_by_key(|s| (s.start, s.end));
    out
}

/// Build the start tag.
#[must_use]
pub fn build_start_tag(t: &TextTag) -> String {
    format!("<{}:{}:{}>", t.type_keyword, t.format, t.data)
}

/// Build the end tag — `"<\\%ls>"`, a **backslash**.
#[must_use]
pub fn build_end_tag(t: &TextTag) -> String {
    format!("<\\{}>", t.type_keyword)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tag parsing requires the string to start with `'<'` and
    /// be at least 3 characters, then splits on `':'`".
    #[test]
    fn make_tag_enforces_the_documented_shape() {
        assert!(make_tag("<>").is_none(), "under three characters");
        assert!(make_tag("Tell:IIDString:0>").is_none(), "must start with <");
        assert!(
            make_tag("<\\Tell>").is_none(),
            "an end tag is not a start tag"
        );
        assert!(
            make_tag("<option>").is_none(),
            "retail requires both colons"
        );
        assert!(
            make_tag("<Unknown:IIDString:0:Bob>").is_none(),
            "the type must map"
        );
        assert!(
            make_tag("<Tell:Unknown:0:Bob>").is_none(),
            "the format must map"
        );
        assert!(
            make_tag("<Tell:IIDString:nope:Bob>").is_none(),
            "the subclass parses its id"
        );
        assert!(
            make_tag("<Tell:IIDString::Bob>").is_none(),
            "the IID may not be empty"
        );
        assert!(
            make_tag("<Tell:IIDString:0:>").is_none(),
            "the string may not be empty"
        );
        let t = make_tag("<Tell:IIDString:0x50001234:Bob>").unwrap();
        assert_eq!(t.kind, TagKind::IidString);
        assert_eq!(t.type_id(), Some(0x1000_0001));
        assert_eq!(t.format, "IIDString");
        assert_eq!(t.data, "0x50001234:Bob");
        assert_eq!(t.id(), Some(0x5000_1234));
        assert_eq!(t.payload(), Some("Bob"));
        assert_eq!(
            make_tag("<Tell:IIDString:1342177282:Alba>").unwrap().id(),
            Some(1_342_177_282)
        );
        assert_eq!(make_tag("<Tell:IIDString:010:Alba>").unwrap().id(), Some(8));
        assert_eq!(
            make_tag("<Tell:IIDString:+10:Alba>").unwrap().id(),
            Some(10)
        );
        assert_eq!(
            make_tag("<Tell:IIDString:-1:Alba>").unwrap().id(),
            Some(u32::MAX)
        );
        assert!(make_tag("<Tell:IIDString:+4294967296:Alba>").is_none());
        assert!(make_tag("<Tell:IIDString:-4294967296:Alba>").is_none());
    }

    /// Oracle: preserve `<\Tag>` (backslash) when parsing existing string tables;
    /// server-authored strings use it.
    #[test]
    fn the_end_tag_uses_a_backslash_and_the_span_lands_on_the_right_glyphs() {
        // Glyph construction emits a glyph for the character it lands on *after* the skip, with no
        // second `'<'` test (the loop falls straight into the glyph tail). A start tag butted
        // against its own end tag therefore eats that end tag's
        // `'<'` as a tagged glyph, and the run never closes. No shipped string has this shape.
        let r = parse("You give <Tell:IIDString:0x50001234:Shard><\\Tell> to Bob");
        assert_eq!(r.text, "You give <\\Tell> to Bob");
        assert_eq!(r.spans.len(), 1);
        assert_eq!((r.spans[0].start, r.spans[0].end), (9, 23));

        let r = parse("You give <Tell:IIDString:0x50001234:x>Shard<\\Tell> to Bob");
        assert_eq!(r.text, "You give Shard to Bob");
        assert_eq!((r.spans[0].start, r.spans[0].end), (9, 14));
        assert_eq!(&r.text[r.spans[0].start..r.spans[0].end], "Shard");
        assert_eq!(r.spans[0].tag.kind, TagKind::IidString);

        // A forward slash is not a *start* tag either, so with nothing open it stays verbatim.
        assert_eq!(parse("x</Tell>").text, "x</Tell>");
        // But glyph construction never looks at the backslash when a run is open: `</Tell>` closes it
        // exactly as `<\\Tell>` would, and is skipped with it.
        let r = parse("<Tell:IID:0x1>x</Tell>");
        assert_eq!(r.text, "x");
        assert_eq!(r.spans.len(), 1);
        assert_eq!((r.spans[0].start, r.spans[0].end), (0, 1));
    }

    /// Oracle: the four tag subclasses and their click notices.
    #[test]
    fn each_kind_raises_its_documented_notice() {
        assert_eq!(TagKind::Did.click_notice(), Some(NoticeId::DidTagClicked));
        assert_eq!(TagKind::Iid.click_notice(), Some(NoticeId::IidTagClicked));
        assert_eq!(
            TagKind::IidEnum.click_notice(),
            Some(NoticeId::IidEnumTagClicked)
        );
        assert_eq!(
            TagKind::IidString.click_notice(),
            Some(NoticeId::IidStringTagClicked)
        );
        assert_eq!(TagKind::Unknown.click_notice(), None);
    }

    /// Oracle: round-tripping is the cheap
    /// check that the parser and the writer agree.
    #[test]
    fn a_tag_round_trips_through_build_and_parse() {
        let t = TextTag {
            kind: TagKind::Did,
            type_keyword: "Tell".into(),
            format: "DID".into(),
            data: "0x06001234".into(),
        };
        let s = format!("{}link{}", build_start_tag(&t), build_end_tag(&t));
        assert_eq!(s, "<Tell:DID:0x06001234>link<\\Tell>");
        let r = parse(&s);
        assert_eq!(r.text, "link");
        assert_eq!(r.spans.len(), 1);
        assert_eq!(r.spans[0].tag, t);
    }

    /// Malformed markup is left verbatim and a second open tag closes the first.
    #[test]
    fn malformed_markup_is_left_verbatim_and_a_second_open_tag_closes_the_first() {
        assert_eq!(parse("2 < 3").text, "2 < 3");
        assert_eq!(parse("<>").text, "<>");
        assert_eq!(parse("use <option> <value>").text, "use <option> <value>");
        let r = parse("<Tell:IID:0x1>A<IID:DID:0x2>B<\\IID>C<\\Tell>");
        // `<IID:DID:0x2>` closes the `Tell:IID` run (no tag-factory call at all), so by the time
        // `<\IID>` and `<\Tell>` are reached nothing is open and both are drawn verbatim.
        assert_eq!(r.text, "AB<\\IID>C<\\Tell>");
        assert_eq!(r.spans.len(), 1);
        assert_eq!((r.spans[0].start, r.spans[0].end), (0, 1));
        assert_eq!(r.spans[0].tag.kind, TagKind::Iid);
    }
    /// Any following tag closes the open run without comparing its name.
    #[test]
    fn any_following_tag_closes_the_open_run_without_comparing_its_name() {
        // A mismatched end tag closes the run and is consumed, exactly as `<\Tell>` would be.
        let r = parse("<Tell:IIDString:1:A>x<\\Squelch>y");
        assert_eq!(r.text, "xy");
        assert_eq!(r.spans.len(), 1);
        assert_eq!((r.spans[0].start, r.spans[0].end), (0, 1));
        assert_eq!(r.spans[0].tag.type_id(), Some(TELL));

        // So does an empty `<>`, which the tag factory would reject outright.
        let r = parse("<Tell:IID:0x1>A<>B");
        assert_eq!(r.text, "AB");
        assert_eq!(r.spans.len(), 1);
        assert_eq!((r.spans[0].start, r.spans[0].end), (0, 1));

        // And so does a word that maps to nothing at all.
        let r = parse("<Tell:IID:0x1>A<option>B");
        assert_eq!(r.text, "AB");
        assert_eq!(r.spans.len(), 1);
        assert_eq!((r.spans[0].start, r.spans[0].end), (0, 1));

        // With nothing open the very same `<…>` are ordinary characters: the tag factory rejects them
        // and the failure leaves the source index on the `'<'`.
        assert_eq!(parse("A<\\Squelch>B").text, "A<\\Squelch>B");
        assert_eq!(parse("A<option>B").text, "A<option>B");
    }

    /// An unterminated bracket is a character and a run open at the end just ends.
    #[test]
    fn an_unterminated_bracket_is_a_character_and_a_run_open_at_the_end_just_ends() {
        let r = parse("<Tell:IID:0x1>A<B");
        assert_eq!(r.text, "A<B", "no '>' -> the '<' is an ordinary glyph");
        assert_eq!(r.spans.len(), 1);
        assert_eq!(
            (r.spans[0].start, r.spans[0].end),
            (0, 3),
            "and the open run still covers it"
        );

        let r = parse("A<Tell:IID:0x1>B");
        assert_eq!(r.text, "AB");
        assert_eq!((r.spans[0].start, r.spans[0].end), (1, 2));

        let r = parse("A<Tell:IID:0x1>");
        assert_eq!(
            r.text, "A",
            "the markup is skipped and the function returns"
        );
        assert_eq!(r.spans.len(), 1);
        assert_eq!((r.spans[0].start, r.spans[0].end), (1, 1), "an empty run");
    }

    /// The shipped shape is unaffected by the end tag rule.
    #[test]
    fn the_shipped_shape_is_unaffected_by_the_end_tag_rule() {
        for line in [
            "[General] <Tell:IIDString:0:Sender>Sender<\\Tell> says, \"hello\"",
            "[Fellowship] <Tell:IIDString:0:Bob>Bob<\\Tell> says, \"hi\"",
            "Your patron <Tell:IIDString:0:Bob>Bob<\\Tell> says to you, \"hi\"",
            "<Tell:IIDString:1342177282:Alba>Alba<\\Tell> tells you, \"hi\"",
            "<Tell:IIDString:0:Bob>Bob<\\Tell> says on the Trade channel, \"hi\"",
        ] {
            let r = parse(line);
            assert!(
                !r.text.contains('<'),
                "{line}: no markup survives -> {:?}",
                r.text
            );
            assert_eq!(r.spans.len(), 1, "{line}");
            let s = &r.spans[0];
            assert_eq!(s.tag.type_id(), Some(TELL), "{line}");
            assert_eq!(s.tag.kind, TagKind::IidString, "{line}");
            assert_eq!(
                r.text
                    .chars()
                    .skip(s.start)
                    .take(s.end - s.start)
                    .collect::<String>(),
                s.tag.payload().unwrap(),
                "{line}: the run covers exactly the name"
            );
        }
    }
}
