//! Reading a text tree, character by character, as the client's parser does.
//!
//! The input is a file's bytes or a string. A file is read as the C library reads one in text
//! mode: a carriage return before a line feed is dropped and a Ctrl-Z ends it ([`text_mode`]).
//! A file's end is the byte 0xFF or the end of its bytes; a string's end is its first zero. A
//! parse that ends anywhere but at the file's end fails ("unexpected close brace": a stray
//! closing bracket, a zero byte), and so does a file with nothing in it.
//!
//! `!include` names files to read in the directive's place. The parser asks for them through
//! [`Includes`]: the host's search, by the directive's name and the including file's name, of
//! the folders the client looks in. With [`NoIncludes`] every include fails the parse.

use super::{stricmp, Node};
use std::cmp::Ordering;

/// The name a string is parsed under.
pub const STRING_FILENAME: &[u8] = b"-string-";
/// The directives.
pub const INCLUDE: &[u8] = b"!include";
pub const ERROR: &[u8] = b"!error";
pub const WARNING: &[u8] = b"!warning";

/// Where the files an `!include` names are found.
pub trait Includes {
    /// Every file an include of `name` finds, read whole, each with the name it is parsed under;
    /// `from` is the name of the file being parsed. The first is read; more than one is a
    /// warning the client logs.
    fn find(&self, name: &[u8], from: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)>;
}

/// No file is ever found.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoIncludes;

impl Includes for NoIncludes {
    fn find(&self, _name: &[u8], _from: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)> {
        Vec::new()
    }
}

/// A file's bytes as the C library hands them to a reader in text mode: each carriage return
/// that comes before a line feed dropped, and nothing after the first Ctrl-Z.
#[must_use]
pub fn text_mode(bytes: &[u8]) -> Vec<u8> {
    let end = bytes.iter().position(|&b| b == 0x1A).unwrap_or(bytes.len());
    let bytes = &bytes[..end];
    let mut out = Vec::with_capacity(bytes.len());
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'\r' && bytes.get(i + 1) == Some(&b'\n') {
            continue;
        }
        out.push(b);
    }
    out
}

/// The characters the parser reads, one ahead: the current one, its line and column, and
/// whether the input is done.
#[derive(Debug)]
struct Input<'a> {
    bytes: &'a [u8],
    /// Where the next character is.
    at: usize,
    /// A string's end is its first zero; a file's, its last byte (or a byte 0xFF).
    from_string: bool,
    line: u16,
    column: u16,
    chars: u32,
    /// The current character, as the client's signed `char`.
    current: i8,
    done: bool,
}

impl<'a> Input<'a> {
    /// No input yet.
    // D2-14576, D2-14577
    const fn new() -> Self {
        Self {
            bytes: &[],
            at: 0,
            from_string: false,
            line: 1,
            column: 1,
            chars: 0,
            current: 0,
            done: true,
        }
    }

    /// A file's bytes (already as text mode reads them): the first character read. An empty
    /// file reads the end at once but is not done, so its parse fails.
    // D2-14575
    fn set_file(&mut self, bytes: &'a [u8]) {
        *self = Self {
            bytes,
            from_string: false,
            ..Self::new()
        };
        self.current = self.read_file();
        self.done = false;
    }

    /// A string: the first character read.
    // D2-14578
    fn set_string(&mut self, bytes: &'a [u8]) {
        let len = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        *self = Self {
            bytes: &bytes[..len],
            from_string: true,
            ..Self::new()
        };
        self.current = self.read_string();
        self.done = false;
    }

    fn read_file(&mut self) -> i8 {
        let c = self.bytes.get(self.at).map_or(-1, |&b| b.cast_signed());
        self.at += 1;
        c
    }

    fn read_string(&mut self) -> i8 {
        let c = self.bytes.get(self.at).map_or(0, |&b| b.cast_signed());
        if c != 0 {
            self.at = (self.at + 1).min(self.bytes.len());
        }
        c
    }

    /// Step on: the character stepped past, the line and column moved, the next one read (the
    /// input done when it is the end).
    // D2-14574
    fn next(&mut self) -> i8 {
        let previous = self.current;
        if previous == b'\n'.cast_signed() {
            self.line = self.line.wrapping_add(1);
            self.column = 1;
        } else {
            self.column = self.column.wrapping_add(1);
        }
        self.chars = self.chars.wrapping_add(1);
        let c = if self.from_string {
            self.read_string()
        } else {
            self.read_file()
        };
        self.current = c;
        let end = if self.from_string { c == 0 } else { c == -1 };
        if end {
            self.done = true;
        }
        previous
    }
}

fn is_separator(c: i8) -> bool {
    matches!(c.cast_unsigned(), b',' | b' ' | b'\t' | b'\r' | b'\n')
}

const fn b(c: u8) -> i8 {
    c.cast_signed()
}

/// The parser: its input, the comment waiting for the next node, the file's name, and whether an
/// error has been found.
#[derive(Debug)]
pub struct Parser<'a> {
    input: Input<'a>,
    /// An error was found: the parse fails.
    error: bool,
    comment: Vec<u8>,
    filename: Vec<u8>,
    expand_includes: bool,
    includes: &'a dyn Includes,
}

impl std::fmt::Debug for dyn Includes + '_ {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Includes")
    }
}

impl<'a> Parser<'a> {
    /// A parser that finds included files through `includes`.
    // D2-14566, D2-14567, D2-14568, D2-14572
    #[must_use]
    pub fn new(includes: &'a dyn Includes) -> Self {
        Self {
            input: Input::new(),
            error: false,
            comment: Vec::new(),
            filename: Vec::new(),
            expand_includes: true,
            includes,
        }
    }

    /// Whether `!include` reads the files it names (on by default); off, an include is kept as a
    /// node.
    pub fn set_expand_includes(&mut self, on: bool) {
        self.expand_includes = on;
    }

    /// Ready for a file of this name: no comment waiting, no error.
    // D2-14579
    fn init(&mut self, filename: &[u8]) {
        filename.clone_into(&mut self.filename);
        self.comment.clear();
        self.error = false;
    }

    /// After a parse: the input dropped, the name and comment cleared.
    // D2-14580
    fn cleanup(&mut self) {
        self.input = Input::new();
        self.filename.clear();
        self.comment.clear();
    }

    /// Parse a file's bytes (as text mode reads them) into `root`.
    // D2-14588, D2-14549
    pub fn parse_file(&mut self, root: &mut Node, filename: &[u8], bytes: &'a [u8]) -> bool {
        self.init(filename);
        self.input.set_file(bytes);
        let ok = self.parse(root);
        self.cleanup();
        ok
    }

    /// Parse a string into `root`.
    // D2-14589, D2-14555
    pub fn parse_string(&mut self, root: &mut Node, text: &'a [u8]) -> bool {
        self.init(STRING_FILENAME);
        self.input.set_string(text);
        let ok = self.parse(root);
        self.cleanup();
        ok
    }

    fn c(&self) -> i8 {
        self.input.current
    }

    fn skip_separators(&mut self) {
        while is_separator(self.c()) {
            self.input.next();
        }
    }

    /// The root named by the file, the file's leading comment, its nodes, the comment after
    /// them; the parse succeeds when it ends at the input's end with no error.
    // D2-14587
    fn parse(&mut self, root: &mut Node) -> bool {
        let filename = self.filename.clone();
        root.init_file_info(&filename);
        root.set_filename(filename.clone());
        root.set_name_text(filename);
        self.parse_comment(false);
        root.add_comment(&self.comment);
        self.parse_nodes(root);
        root.add_comment(&self.comment);
        if self.error {
            return false;
        }
        if self.input.done {
            return true;
        }
        // "unexpected close brace"
        self.error = true;
        false
    }

    /// The comment lines at the input: the separators before them skipped; each `#` line's text
    /// after the `#` (the line break kept); between lines, separators skipped, line breaks only
    /// when `across_blank_lines`. Whether there was one.
    // D2-14581
    fn parse_comment(&mut self, across_blank_lines: bool) -> bool {
        self.comment.clear();
        self.skip_separators();
        if self.c() != b(b'#') {
            return false;
        }
        loop {
            self.input.next();
            if self.input.done {
                return true;
            }
            let mut c = self.input.next();
            while c != b(b'\n') && c != -1 && c != 0 {
                self.comment.push(c.cast_unsigned());
                c = self.input.next();
            }
            if self.input.done {
                return true;
            }
            self.comment.push(b'\n');
            while matches!(self.c().cast_unsigned(), b',' | b' ' | b'\t' | b'\r')
                || (self.c() == b(b'\n') && across_blank_lines)
            {
                self.input.next();
            }
            if self.c() != b(b'#') {
                return true;
            }
        }
    }

    /// A node's name, quoted or not. Never fails but at the input's end.
    // D2-14582
    fn parse_node_name(&mut self, node: &mut Node) -> bool {
        self.skip_separators();
        if self.input.done {
            return false;
        }
        node.set_filename(self.filename.clone());
        node.set_place(self.input.line, self.input.column);
        let mut name = Vec::new();
        if self.c() == b(b'"') {
            loop {
                self.input.next();
                loop {
                    let c = self.c();
                    if c >= b(b'#') && c != b(b'\\') {
                        name.push(c.cast_unsigned());
                        break;
                    }
                    if c == b(b'\\') {
                        self.input.next();
                        if self.input.done {
                            // "unexpected EOF after '\'"
                            node.set_name_text(name);
                            return true;
                        }
                        let e = self.input.next();
                        name.push(e.cast_unsigned());
                        continue;
                    }
                    if c == b(b'"') {
                        self.input.next();
                        node.set_name_text(name);
                        return true;
                    }
                    if c > -2 && (c < 1 || c == b(b'\n')) {
                        // "unterminated string"
                        node.set_name_text(name);
                        return true;
                    }
                    name.push(c.cast_unsigned());
                    break;
                }
            }
        }
        loop {
            let c = self.c();
            let ends = c == -1
                || matches!(
                    c.cast_unsigned(),
                    0 | b'\t'
                        | b'\n'
                        | b'\r'
                        | b' '
                        | b'"'
                        | b'#'
                        | b'('
                        | b')'
                        | b','
                        | b'['
                        | b']'
                        | b'{'
                        | b'}'
                );
            if ends {
                break;
            }
            if c == b(b'\\') {
                self.input.next();
                let e = self.input.next();
                if e == 0 || e == -1 {
                    // "unexpected EOF after '\'"
                    break;
                }
                name.push(e.cast_unsigned());
                continue;
            }
            name.push(c.cast_unsigned());
            self.input.next();
        }
        node.set_name_text(name);
        true
    }

    /// One node: the comment waiting given to it, its name, the comment after the name, and
    /// its children when an opening bracket follows. False at a closing bracket or the end, or
    /// when its children fail or their bracket does not match ("mismatched paren", an error).
    // D2-14583
    fn parse_node(&mut self, node: &mut Node) -> bool {
        self.skip_separators();
        let c = self.c();
        if c == 0 || c == -1 || matches!(c.cast_unsigned(), b']' | b'}' | b')') {
            return false;
        }
        node.add_comment(&self.comment);
        if !self.parse_node_name(node) {
            return false;
        }
        if self.input.done {
            return true;
        }
        self.parse_comment(true);
        self.skip_separators();
        let close = match self.c().cast_unsigned() {
            b'(' => b')',
            b'[' => b']',
            b'{' => b'}',
            _ => {
                let c = self.c();
                if c == 0 || c == -1 || matches!(c.cast_unsigned(), b')' | b']' | b'}') {
                    node.add_comment(&self.comment);
                    self.comment.clear();
                }
                return true;
            }
        };
        node.add_comment(&self.comment);
        self.comment.clear();
        self.input.next();
        if !self.parse_nodes(node) {
            return false;
        }
        self.skip_separators();
        if self.c() != b(close) {
            // "mismatched paren"
            self.error = true;
            return false;
        }
        self.input.next();
        self.parse_comment(true);
        let c = self.c();
        if c == 0 || c == -1 || matches!(c.cast_unsigned(), b')' | b']' | b'}') {
            node.add_comment(&self.comment);
            self.comment.clear();
        }
        true
    }

    /// The nodes up to a closing bracket or the end, each adopted by `parent`, a directive's
    /// nodes in its place. False when an error was found.
    // D2-14586, D2-14585
    fn parse_nodes(&mut self, parent: &mut Node) -> bool {
        self.parse_comment(true);
        loop {
            let mut node = Node::new();
            if !self.parse_node(&mut node) {
                break;
            }
            let directive = node
                .name_text_now()
                .is_some_and(|t| t.first() == Some(&b'!'));
            if !self.error && directive {
                for n in self.post_process_node(node) {
                    parent.adopt(n);
                }
            } else {
                parent.adopt(node);
                if self.error {
                    return false;
                }
            }
        }
        !self.error
    }

    /// A directive's outcome, the nodes to put in its place: an include's files' nodes (the
    /// first carrying the directive's comment, the file's leading comment and its own); `!error`
    /// fails the parse and `!warning` is dropped; any other `!` name stays as it is.
    // D2-14590, D2-14584
    fn post_process_node(&mut self, node: Node) -> Vec<Node> {
        let is = |w: &[u8]| node.is_keyword(w);
        if is(INCLUDE) && self.expand_includes {
            let mut out = Vec::new();
            for c in node.children() {
                let name = c.name_text_now().unwrap_or_default();
                let mut found = self.includes.find(&name, &self.filename).into_iter();
                let Some((path, bytes)) = found.next() else {
                    // "Couldn't find !include file"
                    self.error = true;
                    continue;
                };
                let bytes = text_mode(&bytes);
                let mut sub = Parser::new(self.includes);
                let mut root = Node::new();
                if !sub.parse_file(&mut root, &path, &bytes) {
                    // "Failed to parse include"
                    self.error = true;
                    continue;
                }
                let file_comment = root.comment().to_vec();
                let mut nodes = root.children.drain(..);
                if let Some(mut first) = nodes.next() {
                    let own = first.comment().to_vec();
                    first.clear_comments(false);
                    first.add_comment(node.comment());
                    first.add_comment(&file_comment);
                    first.add_comment(&own);
                    out.push(first);
                    out.extend(nodes);
                }
            }
            return out;
        }
        if is(ERROR) {
            self.error = true;
            return Vec::new();
        }
        if is(WARNING) {
            return Vec::new();
        }
        vec![node]
    }
}

/// Parse a file's bytes (read in text mode first) into a tree whose root is named `filename`.
/// None when the parse fails.
#[must_use]
pub fn parse_file(filename: &[u8], bytes: &[u8], includes: &dyn Includes) -> Option<Node> {
    let text = text_mode(bytes);
    let mut root = Node::new();
    Parser::new(includes)
        .parse_file(&mut root, filename, &text)
        .then_some(root)
}

/// Parse bytes as a file is parsed, with no includes found.
#[must_use]
pub fn parse_bytes(filename: &[u8], bytes: &[u8]) -> Option<Node> {
    parse_file(filename, bytes, &NoIncludes)
}

/// Parse a string (it ends at its first zero) into a tree whose root is named `-string-`.
#[must_use]
pub fn parse_text(text: &[u8]) -> Option<Node> {
    let mut root = Node::new();
    Parser::new(&NoIncludes)
        .parse_string(&mut root, text)
        .then_some(root)
}

/// Whether two names are the same word, case-blind.
#[must_use]
pub fn same_word(a: &[u8], b: &[u8]) -> bool {
    stricmp(a, b) == Ordering::Equal
}
