//! A preferences file's text, read and written as the September 2013 client's system reads and
//! writes its `Preferences.ini`: sections headed `[name]`, each a run of `key=value` lines.
//!
//! **Reading.** The first section of a name holds its keys (a later one of the same name is never
//! read); its keys run to the next header. A line is a key's when it holds `=` and does not start
//! with `;` (a comment) or `[` (a header); the key is what comes before the first `=`, the value
//! what comes after, each with the spaces and tabs at its ends taken off, and a value quoted at
//! both ends with the same mark (`'` or `"`) loses the marks. Names compare without letter case.
//! Lines before the first header belong to no section.
//!
//! **Writing.** A key already there has its value replaced in place: what follows the `=` up to
//! the value's end, so the key as written, the spaces before the `=` and the spaces after the
//! value stay. A key that is not there goes after its section's last key line (right after the
//! header when it has none), or at the end of the text when its section is the last. A section
//! that is not there goes at the end. Lines written anew end in a carriage return and a line
//! feed; the text's own lines keep their endings.

/// A preferences file's text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Profile {
    text: String,
}

/// The line break written after a line written anew.
const NEW_LINE: &str = "\r\n";

/// A line of the text: where it starts, where its content ends (before its line break) and where
/// it ends (after it).
#[derive(Clone, Copy, Debug)]
struct Line {
    start: usize,
    content_end: usize,
    end: usize,
}

/// What a line is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind<'a> {
    /// A header and the section it names.
    Header(&'a str),
    /// A key's line: the key, where its `=` is (from the line's start), and the value as written.
    Key {
        key: &'a str,
        equals: usize,
        value: &'a str,
    },
    /// Anything else: a blank line, a comment, a line with no `=`.
    Other,
}

const fn is_blank(c: char) -> bool {
    c == ' ' || c == '\t'
}

fn trim(s: &str) -> &str {
    s.trim_matches(is_blank)
}

/// A value as read: its ends' spaces and tabs off, then the quotes around it.
fn unquote(v: &str) -> &str {
    let v = trim(v);
    let b = v.as_bytes();
    if b.len() >= 2 && (b[0] == b'"' || b[0] == b'\'') && b[b.len() - 1] == b[0] {
        &v[1..v.len() - 1]
    } else {
        v
    }
}

fn same(a: &str, b: &str) -> bool {
    trim(a).eq_ignore_ascii_case(trim(b))
}

impl Profile {
    /// The profile of a text.
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }

    /// The text.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    fn lines(&self) -> Vec<Line> {
        let mut out = Vec::new();
        let mut start = 0;
        let bytes = self.text.as_bytes();
        while start < bytes.len() {
            let end = bytes[start..]
                .iter()
                .position(|&c| c == b'\n')
                .map_or(bytes.len(), |i| start + i + 1);
            let mut content_end = end;
            if content_end > start && bytes[content_end - 1] == b'\n' {
                content_end -= 1;
            }
            if content_end > start && bytes[content_end - 1] == b'\r' {
                content_end -= 1;
            }
            out.push(Line {
                start,
                content_end,
                end,
            });
            start = end;
        }
        out
    }

    fn kind(&self, l: Line) -> Kind<'_> {
        let content = &self.text[l.start..l.content_end];
        let lead = content.trim_start_matches(is_blank);
        if let Some(rest) = lead.strip_prefix('[') {
            let name = rest.find(']').map_or(rest, |i| &rest[..i]);
            return Kind::Header(trim(name));
        }
        if lead.starts_with(';') {
            return Kind::Other;
        }
        match content.find('=') {
            Some(equals) => Kind::Key {
                key: trim(&content[..equals]),
                equals,
                value: &content[equals + 1..],
            },
            None => Kind::Other,
        }
    }

    /// The first section of a name: the index of its header's line and of the line after its last
    /// (the next header's, or the count of lines).
    fn section(&self, lines: &[Line], section: &str) -> Option<(usize, usize)> {
        let header = lines
            .iter()
            .position(|l| matches!(self.kind(*l), Kind::Header(n) if same(n, section)))?;
        let end = lines[header + 1..]
            .iter()
            .position(|l| matches!(self.kind(*l), Kind::Header(_)))
            .map_or(lines.len(), |i| header + 1 + i);
        Some((header, end))
    }

    /// A key's line in a section: its index.
    fn key_line(&self, lines: &[Line], at: (usize, usize), key: &str) -> Option<usize> {
        (at.0 + 1..at.1)
            .find(|&i| matches!(self.kind(lines[i]), Kind::Key { key: k, .. } if same(k, key)))
    }

    /// A key's value in a section, as read; none when the section or the key is not there.
    #[must_use]
    pub fn value(&self, section: &str, key: &str) -> Option<&str> {
        let lines = self.lines();
        let at = self.section(&lines, section)?;
        let i = self.key_line(&lines, at, key)?;
        match self.kind(lines[i]) {
            Kind::Key { value, .. } => Some(unquote(value)),
            _ => None,
        }
    }

    /// A key's value as the client asks for one: the value, else `default` less the spaces at its
    /// end, cut to fit a buffer of `size` bytes with its terminating zero.
    #[must_use]
    pub fn read(&self, section: &str, key: &str, default: &str, size: usize) -> String {
        let v = self
            .value(section, key)
            .unwrap_or_else(|| default.trim_end_matches(is_blank));
        let mut n = v.len().min(size.saturating_sub(1));
        while !v.is_char_boundary(n) {
            n -= 1;
        }
        v[..n].to_owned()
    }

    /// Set a key's value (`Some`), or take its line out (`None`).
    pub fn write(&mut self, section: &str, key: &str, value: Option<&str>) {
        self.end_line();
        let lines = self.lines();
        let Some(at) = self.section(&lines, section) else {
            if let Some(v) = value {
                self.text
                    .push_str(&format!("[{section}]{NEW_LINE}{key}={v}{NEW_LINE}"));
            }
            return;
        };
        if let Some(i) = self.key_line(&lines, at, key) {
            let l = lines[i];
            let Kind::Key {
                equals, value: old, ..
            } = self.kind(l)
            else {
                return;
            };
            match value {
                Some(v) => {
                    let from = l.start + equals + 1;
                    let to = from + old.trim_end_matches(is_blank).len();
                    self.text.replace_range(from..to, v);
                }
                None => {
                    self.text.replace_range(self.lead(l)..l.end, "");
                }
            }
            return;
        }
        let Some(v) = value else { return };
        let at = self.extent(&lines, at);
        let to = if at < lines.len() {
            lines[at].start
        } else {
            self.text.len()
        };
        self.text.insert_str(to, &format!("{key}={v}{NEW_LINE}"));
    }

    /// Take a section out: its header through its last key line, or through the end of the text
    /// when the section is the last and ends in a blank line. Comments and other lines after its
    /// last key stay.
    pub fn remove_section(&mut self, section: &str) {
        self.end_line();
        let lines = self.lines();
        if let Some(at) = self.section(&lines, section) {
            let h = at.0;
            let e = self.extent(&lines, at);
            let end = if e < lines.len() {
                lines[e].start
            } else {
                self.text.len()
            };
            self.text.replace_range(self.lead(lines[h])..end, "");
        }
    }

    /// A section's extent, the index of the line after it: past its last key line (its header
    /// when it has none), or every line to the end of the text when the section is the last and
    /// its last line is blank. A key added goes there.
    fn extent(&self, lines: &[Line], at: (usize, usize)) -> usize {
        let last = lines[at.1 - 1];
        if at.1 == lines.len() && trim(&self.text[last.start..last.content_end]).is_empty() {
            return lines.len();
        }
        (at.0 + 1..at.1)
            .rev()
            .find(|&i| matches!(self.kind(lines[i]), Kind::Key { .. }))
            .unwrap_or(at.0)
            + 1
    }

    /// Where a line's content starts, past the spaces before it: a line taken out leaves those.
    fn lead(&self, l: Line) -> usize {
        let content = &self.text[l.start..l.content_end];
        l.start + content.len() - content.trim_start_matches(is_blank).len()
    }

    /// Before any change, a last line with no line break loses the spaces at its end and is ended
    /// (or goes, when nothing else was on it).
    fn end_line(&mut self) {
        if !self.text.is_empty() && !self.text.ends_with('\n') {
            let keep = self.text.trim_end_matches(is_blank).len();
            self.text.truncate(keep);
            if !self.text.is_empty() && !self.text.ends_with('\n') {
                self.text.push_str(NEW_LINE);
            }
        }
    }
}
