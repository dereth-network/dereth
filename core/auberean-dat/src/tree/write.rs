//! Writing a text tree as the client writes one.
//!
//! The root's comment first, then its children. Each child of the root is preceded by a blank
//! line. A node is its name, quoted when it is empty or holds a space, a tab, `#`, `,` or a
//! bracket, with a backslash before each line break, carriage return, quote and backslash; then,
//! when it has children, `[`, the children and `]`. A node's children go on one line, separated by
//! spaces, unless the node is too wide or a child carries a comment: then each child goes on a
//! line of its own, indented two spaces a level, and a blank line follows the bracket. A comment is
//! written before its node, a `#` starting each of its lines.
//!
//! A file is written as the C library writes one in text mode: each line break as a carriage
//! return and a line feed ([`to_file_bytes`]).

use super::Node;

/// The width a node's line may take before its children are put on lines of their own.
const WRAP_WIDTH: usize = 80;
/// How deep the width is measured: a tree deeper than this is always spread.
const WRAP_DEPTH: u32 = 3;
const INDENT: &[u8] = b"  ";

/// The writer's state: the indentation, whether the children being written each take a line,
/// whether a blank line is due, and whether the next node is the first of its parent's.
#[derive(Debug)]
pub struct Writer {
    out: Vec<u8>,
    indent: usize,
    separate: bool,
    first: bool,
    blank_due: bool,
}

impl Default for Writer {
    // D2-14568
    fn default() -> Self {
        Self {
            out: Vec::new(),
            indent: 0,
            separate: true,
            first: false,
            blank_due: false,
        }
    }
}

impl Writer {
    /// The text of a tree: its root's comment and its root's children. Names read as text along
    /// the way stay text (see [`Node::name_text`]).
    // D2-14571, D2-14554, D2-14573
    #[must_use]
    pub fn write(root: &mut Node) -> Vec<u8> {
        let mut w = Self::default();
        w.insert_comments(root);
        w.format_children(root);
        w.out
    }

    /// The width of a node's line, as far as `width` and `depth` matter: its name, and when it has
    /// children six more and theirs, until the width is passed. Out of depth: past the width.
    // D2-14556
    fn wrap_width(node: &mut Node, width: usize, depth: u32) -> usize {
        if depth == 0 {
            return width + 1;
        }
        let mut len = node.name_text().map_or(0, |t| t.len());
        if !node.is_empty() {
            len += 6;
            if len <= width {
                for c in node.children_mut() {
                    len += Self::wrap_width(c, width - len, depth - 1);
                    if len > width {
                        break;
                    }
                }
            }
        }
        len
    }

    /// Whether a node's children each take a line: the node is too wide, or a child has a
    /// comment.
    // D2-14558
    fn should_separate(node: &mut Node) -> bool {
        if Self::wrap_width(node, WRAP_WIDTH, WRAP_DEPTH) > WRAP_WIDTH {
            return true;
        }
        node.children().iter().any(|c| !c.comment().is_empty())
    }

    // D2-14560
    fn maybe_indent(&mut self) {
        if self.separate {
            for _ in 0..self.indent {
                self.out.extend_from_slice(INDENT);
            }
        }
    }

    // D2-14562
    fn new_line(&mut self) {
        self.out.push(b'\n');
    }

    // D2-14563
    fn insert_separator(&mut self) {
        self.out.push(if self.separate { b'\n' } else { b' ' });
    }

    /// A node's comment: a line break, then each line indented and after a `#`; a line break at
    /// the end unless the comment ends in one.
    // D2-14564
    fn insert_comments(&mut self, node: &Node) {
        let comment = node.comment();
        if comment.is_empty() {
            return;
        }
        self.out.push(b'\n');
        self.maybe_indent();
        self.out.push(b'#');
        let mut line_start = false;
        for &c in super::number::cstr(comment) {
            if line_start {
                self.maybe_indent();
                self.out.push(b'#');
                line_start = false;
            }
            self.out.push(c);
            if c == b'\n' {
                line_start = true;
            }
        }
        if !line_start {
            self.out.push(b'\n');
        }
    }

    /// A name, quoted when it must be, escaped where it must be.
    // D2-14561
    fn put_quoted(&mut self, name: &[u8]) {
        let name = super::number::cstr(name);
        let quote = name.is_empty()
            || name.iter().any(|c| {
                matches!(
                    c,
                    b'\t' | b' ' | b'#' | b'(' | b')' | b',' | b'[' | b']' | b'{' | b'}'
                )
            });
        if quote {
            self.out.push(b'"');
        }
        for &c in name {
            if matches!(c, b'\n' | b'\r' | b'"' | b'\\') {
                self.out.push(b'\\');
            }
            self.out.push(c);
        }
        if quote {
            self.out.push(b'"');
        }
    }

    // D2-14565
    fn format_node(&mut self, node: &mut Node) {
        let sep = self.separate && Self::should_separate(node);
        if self.blank_due || (!self.first && sep) {
            self.blank_due = false;
            if node.comment().is_empty() {
                self.new_line();
            }
        }
        self.insert_comments(node);
        self.maybe_indent();
        let name = node.name_text().unwrap_or_default();
        self.put_quoted(&name);
        if !node.is_empty() {
            let saved = self.separate;
            self.separate = sep;
            self.insert_separator();
            self.maybe_indent();
            self.out.push(b'[');
            self.insert_separator();
            self.indent += 1;
            self.format_children(node);
            self.indent -= 1;
            self.maybe_indent();
            self.out.push(b']');
            self.blank_due = self.separate;
            self.separate = saved;
        }
        self.insert_separator();
    }

    // D2-14550
    fn format_children(&mut self, node: &mut Node) {
        self.first = true;
        for c in node.children_mut() {
            if self.indent == 0 {
                self.blank_due = true;
            }
            self.format_node(c);
            self.first = false;
        }
    }
}

/// The text of a tree, its line breaks bare.
#[must_use]
pub fn to_text(root: &mut Node) -> Vec<u8> {
    Writer::write(root)
}

/// The bytes of a tree's file: its text with each line break written as a carriage return and a
/// line feed.
// D2-14569, D2-14570
#[must_use]
pub fn to_file_bytes(root: &mut Node) -> Vec<u8> {
    let text = Writer::write(root);
    let mut out = Vec::with_capacity(text.len() + text.len() / 8);
    for b in text {
        if b == b'\n' {
            out.push(b'\r');
        }
        out.push(b);
    }
    out
}
