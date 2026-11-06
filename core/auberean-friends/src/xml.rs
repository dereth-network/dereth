//! The XML the stream carries: one element with double-quoted attributes and children at most one
//! level deep, each an empty element with attributes.
//!
//! The reader takes any XML 1.0 text of that shape: white space around the element, before `>` and
//! `/>` and between children, single quotes, the five named references and numeric ones. It
//! refuses everything else of XML (a declaration, a document type, a comment, a processing
//! instruction, CDATA, text in an element, deeper children), and an element of more than
//! [`MOST_ATTRIBUTES`] attributes. The writer writes one form: no white space but one space before
//! each attribute, and an element without children as `<Name .../>`.

use core::fmt;

/// The most attributes one element is read with: past any element of the stream, and few enough
/// that an attribute written twice is found at no great cost.
pub const MOST_ATTRIBUTES: usize = 32;

/// One element: its name, its attributes in the order written, and its children, which have none
/// of their own.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Node {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
}

impl Node {
    /// An element with no attributes and no children.
    #[must_use]
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            ..Self::default()
        }
    }

    /// This element with one more attribute.
    #[must_use]
    pub fn attr(mut self, name: &str, value: impl Into<String>) -> Self {
        self.attrs.push((name.to_owned(), value.into()));
        self
    }

    /// This element with one more child.
    #[must_use]
    pub fn child(mut self, child: Node) -> Self {
        self.children.push(child);
        self
    }

    /// The value of the attribute called `name`, if it has one.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    /// The element as text, in the writer's one form.
    #[must_use]
    pub fn write(&self) -> String {
        let mut out = String::new();
        self.write_into(&mut out);
        out
    }

    fn write_into(&self, out: &mut String) {
        out.push('<');
        out.push_str(&self.name);
        for (name, value) in &self.attrs {
            out.push(' ');
            out.push_str(name);
            out.push_str("=\"");
            escape_into(value, out);
            out.push('"');
        }
        if self.children.is_empty() {
            out.push_str("/>");
            return;
        }
        out.push('>');
        for c in &self.children {
            c.write_into(out);
        }
        out.push_str("</");
        out.push_str(&self.name);
        out.push('>');
    }
}

/// Write `value` as attribute text. The markup characters and the line breaks become references;
/// characters XML cannot carry at all (the other control characters) are left out.
fn escape_into(value: &str, out: &mut String) {
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\t' => out.push_str("&#9;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            c if is_xml_char(c) => out.push(c),
            _ => {}
        }
    }
}

/// Whether XML 1.0 can carry `c` at all.
fn is_xml_char(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | ' '..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}')
}

/// Why text is not one element of the subset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct XmlError {
    /// How far into the text, in bytes.
    pub at: usize,
    /// What was wrong there.
    pub what: &'static str,
}

impl fmt::Display for XmlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (byte {})", self.what, self.at)
    }
}

/// Read `text` as one element of the subset.
///
/// # Errors
/// [`XmlError`] when the text is anything else.
pub fn parse(text: &str) -> Result<Node, XmlError> {
    let mut p = Parser {
        s: text.as_bytes(),
        text,
        i: 0,
    };
    p.skip_space();
    let (mut root, empty) = p.open_tag()?;
    if !empty {
        loop {
            p.skip_space();
            if p.rest().starts_with(b"</") {
                p.close_tag(&root.name)?;
                break;
            }
            match p.peek() {
                Some(b'<') => {
                    let (child, child_empty) = p.open_tag()?;
                    if !child_empty {
                        return Err(p.err("a child holds content"));
                    }
                    root.children.push(child);
                }
                None => return Err(p.err("the element is not closed")),
                Some(_) => return Err(p.err("text inside the element")),
            }
        }
    }
    p.skip_space();
    if p.i < p.s.len() {
        return Err(p.err("more after the element"));
    }
    Ok(root)
}

struct Parser<'a> {
    s: &'a [u8],
    text: &'a str,
    i: usize,
}

impl Parser<'_> {
    fn err(&self, what: &'static str) -> XmlError {
        XmlError { at: self.i, what }
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn rest(&self) -> &[u8] {
        &self.s[self.i..]
    }

    fn skip_space(&mut self) -> bool {
        let start = self.i;
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.i += 1;
        }
        self.i > start
    }

    fn expect(&mut self, b: u8, what: &'static str) -> Result<(), XmlError> {
        if self.peek() == Some(b) {
            self.i += 1;
            Ok(())
        } else {
            Err(self.err(what))
        }
    }

    /// A name: a letter or `_` (or any character past ASCII), then those, digits, `-`, `.` and `:`.
    fn name(&mut self) -> Result<String, XmlError> {
        let start = self.i;
        match self.peek() {
            Some(b) if b.is_ascii_alphabetic() || b == b'_' || b >= 0x80 => self.i += 1,
            _ => return Err(self.err("a name was expected")),
        }
        while let Some(b) = self.peek() {
            if b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b':') || b >= 0x80 {
                self.i += 1;
            } else {
                break;
            }
        }
        Ok(self.text[start..self.i].to_owned())
    }

    /// `<Name attr="value" ...` up to `>` (false) or `/>` (true).
    fn open_tag(&mut self) -> Result<(Node, bool), XmlError> {
        self.expect(b'<', "an element was expected")?;
        let mut node = Node::new(&self.name()?);
        loop {
            let spaced = self.skip_space();
            match self.peek() {
                Some(b'>') => {
                    self.i += 1;
                    return Ok((node, false));
                }
                Some(b'/') => {
                    self.i += 1;
                    self.expect(b'>', "`/` not followed by `>`")?;
                    return Ok((node, true));
                }
                None => return Err(self.err("the tag is not closed")),
                Some(_) if !spaced => return Err(self.err("no space before an attribute")),
                Some(_) => {
                    let name = self.name()?;
                    self.skip_space();
                    self.expect(b'=', "an attribute without `=`")?;
                    self.skip_space();
                    let value = self.value()?;
                    if node.get(&name).is_some() {
                        return Err(self.err("an attribute written twice"));
                    }
                    if node.attrs.len() == MOST_ATTRIBUTES {
                        return Err(self.err("more attributes than an element holds"));
                    }
                    node.attrs.push((name, value));
                }
            }
        }
    }

    fn close_tag(&mut self, name: &str) -> Result<(), XmlError> {
        self.i += 2;
        let closing = self.name()?;
        if closing != name {
            return Err(self.err("the closing tag names another element"));
        }
        self.skip_space();
        self.expect(b'>', "the closing tag is not closed")
    }

    /// A quoted attribute value, its references resolved and its white space normalized as XML
    /// does: a line break, a carriage return and a tab written as themselves each read as a space.
    fn value(&mut self) -> Result<String, XmlError> {
        let quote = match self.peek() {
            Some(q @ (b'"' | b'\'')) => q,
            _ => return Err(self.err("an attribute value is not quoted")),
        };
        self.i += 1;
        let mut out = String::new();
        loop {
            let Some(b) = self.peek() else {
                return Err(self.err("an attribute value is not closed"));
            };
            match b {
                _ if b == quote => {
                    self.i += 1;
                    return Ok(out);
                }
                b'<' => return Err(self.err("`<` in an attribute value")),
                b'&' => out.push(self.reference()?),
                b'\r' => {
                    self.i += 1;
                    if self.peek() == Some(b'\n') {
                        self.i += 1;
                    }
                    out.push(' ');
                }
                b'\n' | b'\t' => {
                    self.i += 1;
                    out.push(' ');
                }
                _ if b < 0x20 => return Err(self.err("a control character in an attribute value")),
                _ => {
                    // The text is UTF-8, so the next character starts here.
                    let c = self.text[self.i..].chars().next().unwrap_or('\u{FFFD}');
                    if !is_xml_char(c) {
                        return Err(self.err("a character XML cannot carry"));
                    }
                    out.push(c);
                    self.i += c.len_utf8();
                }
            }
        }
    }

    /// `&name;`, `&#digits;` or `&#xhex;`.
    fn reference(&mut self) -> Result<char, XmlError> {
        let start = self.i;
        let Some(len) = self.rest().iter().take(12).position(|&b| b == b';') else {
            return Err(self.err("`&` that begins no reference"));
        };
        let body = &self.text[start + 1..start + len];
        self.i = start + len + 1;
        let c = match body {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => {
                let n = if let Some(hex) = body.strip_prefix("#x") {
                    u32::from_str_radix(hex, 16).ok()
                } else if let Some(dec) = body.strip_prefix('#') {
                    dec.parse::<u32>().ok()
                } else {
                    None
                };
                n.and_then(char::from_u32).filter(|&c| is_xml_char(c))
            }
        };
        c.ok_or(XmlError {
            at: start,
            what: "a reference to no character",
        })
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the reader and writer of the subset, held to XML's rules)
    use super::*;

    #[test]
    fn a_written_element_reads_back() {
        let n = Node::new("UTellEv")
            .attr("User", "Ann@dawnsong")
            .attr("Text", "a <b> & \"c\"\t'd'\n")
            .child(Node::new("Extra").attr("Val", "00"));
        let text = n.write();
        assert_eq!(
            text,
            "<UTellEv User=\"Ann@dawnsong\" Text=\"a &lt;b&gt; &amp; &quot;c&quot;&#9;'d'&#10;\">\
             <Extra Val=\"00\"/></UTellEv>"
        );
        assert_eq!(parse(&text), Ok(n));
    }

    #[test]
    fn white_space_is_read_where_xml_allows_it() {
        let n = parse(" <A b = 'x' c=\"y\" >\r\n <C d=\"1\" /> </A > ").unwrap();
        assert_eq!(n.get("b"), Some("x"));
        assert_eq!(n.children.len(), 1);
        assert_eq!(parse("<A b=\"x\ny\"/>").unwrap().get("b"), Some("x y"));
    }

    #[test]
    fn what_is_not_the_subset_is_refused() {
        for bad in [
            "",
            "<?xml version=\"1.0\"?><A/>",
            "<!-- c --><A/>",
            "<A>text</A>",
            "<A><B><C/></B></A>",
            "<A b=\"1\" b=\"2\"/>",
            "<A b=1/>",
            "<A b=\"&nbsp;\"/>",
            "<A b=\"&#0;\"/>",
            "<A b=\"<\"/>",
            "<A></B>",
            "<A/><B/>",
            "<A b=\"1\"c=\"2\"/>",
            "<A b=\"\u{1}\"/>",
            "<A><![CDATA[x]]></A>",
        ] {
            assert!(parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn an_element_of_too_many_attributes_is_refused() {
        let with = |n: usize| {
            let attrs: String = (0..n).map(|i| format!(" a{i}=\"\"")).collect();
            format!("<A{attrs}/>")
        };
        assert_eq!(
            parse(&with(MOST_ATTRIBUTES)).unwrap().attrs.len(),
            MOST_ATTRIBUTES
        );
        assert!(parse(&with(MOST_ATTRIBUTES + 1)).is_err());
        let child = format!("<A>{}</A>", with(MOST_ATTRIBUTES + 1));
        assert!(parse(&child).is_err(), "nor a child");
    }

    #[test]
    fn control_characters_xml_cannot_carry_are_left_out_when_writing() {
        let text = Node::new("A").attr("b", "x\u{1}\u{0}y").write();
        assert_eq!(text, "<A b=\"xy\"/>");
    }
}
