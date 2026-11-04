//! The text tree: the bracketed text files the client reads and writes for itself, such as the
//! key maps beside the program (`Main.keymap` and its kin) and the gameplay options a player
//! exports.
//!
//! A tree is nodes, each with a name, any number of child nodes, a comment and where it was read.
//! A name is text when read; code that builds a tree may give a node a number, a flag or bytes as
//! its name, and reading a name as a number turns it into that number for good (see
//! [`Node::name_u32`]). Text is narrow (bytes in the Windows Latin-1 code page), as the client
//! keeps it.
//!
//! The text form ([`parse`] reads it, [`mod@write`] writes it):
//!
//! ```text
//! # a comment, kept with the node after it
//! VERSION [ 1 ]
//! "" [ mydefault ]
//! { MyKeys }
//! ```
//!
//! A node is a name, then optionally its children between `[ ]`, `{ }` or `( )`. Commas and white
//! space separate. A name in double quotes may hold any character but an unescaped quote or line
//! break; a backslash escapes the character after it in either form. A name starting with `!` is
//! a directive: `!include` puts another file's nodes in its place, `!error` fails the parse,
//! `!warning` is dropped.

pub mod number;
pub mod parse;
pub mod write;

use std::cmp::Ordering;

pub use parse::{parse_bytes, parse_file, parse_text, Includes, NoIncludes, Parser};
pub use write::{to_file_bytes, to_text, Writer};

/// The kind of a node's name, by the client's numbering.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum NameKind {
    Text = 0,
    U32 = 1,
    I32 = 2,
    F64 = 3,
    F32 = 4,
    CellId = 5,
    InstanceId = 6,
    Bool = 7,
    Hex = 8,
    Binary = 9,
    BlockId = 10,
    U64 = 11,
    I64 = 12,
    LongHex = 13,
    Enum = 14,
    /// No name at all.
    Null = 255,
}

/// A node's name. A copy is a deep copy; two names are equal when they are of one kind and hold
/// the same value (a float as floats compare, bytes byte for byte).
// D2-03388, D2-03395, D2-03396, D2-03473, D2-03508, D2-04236, D2-08722, D2-08723
// D2-12992, D2-13072, D2-14418, D2-14420, D2-14500
// D2-14490, D2-14493, D2-14495, D2-14497, D2-14499, D2-14504, D2-14505
#[derive(Clone, Debug, PartialEq)]
pub enum Name {
    /// Text, as read.
    Text(Vec<u8>),
    U32(u32),
    I32(i32),
    F64(f64),
    F32(f32),
    /// A 64-bit object id, written `0x` and sixteen capital hexadecimal digits.
    InstanceId(u64),
    /// A flag, kept as the client keeps one (any nonzero value is true).
    Bool(i32),
    /// A 32-bit number written `0x` and eight small hexadecimal digits.
    Hex(u32),
    /// Bytes with a word of their own, written as wrapped hexadecimal text.
    Binary {
        tag: u32,
        bytes: Vec<u8>,
    },
    U64(u64),
    I64(i64),
}

impl Name {
    /// The name's kind.
    // D2-14417, D2-14419, D2-08721, D2-12991, D2-10058
    #[must_use]
    pub const fn kind(&self) -> NameKind {
        match self {
            Self::Text(_) => NameKind::Text,
            Self::U32(_) => NameKind::U32,
            Self::I32(_) => NameKind::I32,
            Self::F64(_) => NameKind::F64,
            Self::F32(_) => NameKind::F32,
            Self::InstanceId(_) => NameKind::InstanceId,
            Self::Bool(_) => NameKind::Bool,
            Self::Hex(_) => NameKind::Hex,
            Self::Binary { .. } => NameKind::Binary,
            Self::U64(_) => NameKind::U64,
            Self::I64(_) => NameKind::I64,
        }
    }

    /// The name as text: a number in its `printf` form (`%lu`, `%ld`, `%.16g`, `%.8g`,
    /// `0x%016I64X`, `0x%08x`, `%I64u`, `%I64d`), a flag `true` or `false`, bytes as wrapped
    /// hexadecimal.
    // D2-12944, D2-14517, D2-14518, D2-14519, D2-14520, D2-14521
    // D2-14522, D2-14523, D2-14524, D2-14525, D2-14526
    #[must_use]
    pub fn to_text(&self) -> Vec<u8> {
        match self {
            Self::Text(t) => t.clone(),
            Self::U32(v) => v.to_string().into_bytes(),
            Self::I32(v) => v.to_string().into_bytes(),
            Self::F64(v) => number::general(*v, 16).into_bytes(),
            Self::F32(v) => number::general(f64::from(*v), 8).into_bytes(),
            Self::InstanceId(v) => format!("0x{v:016X}").into_bytes(),
            Self::Bool(v) => if *v != 0 { "true" } else { "false" }.into(),
            Self::Hex(v) => format!("0x{v:08x}").into_bytes(),
            Self::Binary { tag, bytes } => binary_text(*tag, bytes),
            Self::U64(v) => v.to_string().into_bytes(),
            Self::I64(v) => v.to_string().into_bytes(),
        }
    }

    /// A name of `kind` read from text, when the text reads as one: a number read whole and in
    /// range, a flag (`true` or `1`, any case, is true; anything else false), text as it is.
    // D2-03397, D2-14510, D2-14511, D2-14512, D2-14513, D2-14514, D2-14515, D2-14516
    #[must_use]
    pub fn from_text(kind: NameKind, t: &[u8]) -> Option<Self> {
        Some(match kind {
            NameKind::Text => Self::Text(t.to_vec()),
            NameKind::U32 => Self::U32(whole(number::strtoul(t), t)?),
            NameKind::Hex => Self::Hex(whole(number::strtoul(t), t)?),
            NameKind::I32 => Self::I32(whole(number::strtol(t), t)?),
            NameKind::U64 => Self::U64(whole(number::strtoui64(t), t)?),
            NameKind::InstanceId => Self::InstanceId(whole(number::strtoui64(t), t)?),
            NameKind::I64 => Self::I64(whole(number::strtoi64(t), t)?),
            NameKind::F64 => Self::F64(whole(number::strtod(t), t)?),
            #[allow(clippy::cast_possible_truncation)] // the client stores the float of the double
            NameKind::F32 => Self::F32(whole(number::strtod(t), t)? as f32),
            NameKind::Bool => Self::Bool(i32::from(
                eq_ignore_case(t, b"true") || eq_ignore_case(t, b"1"),
            )),
            _ => return None,
        })
    }
}

fn whole<T>(s: number::Scan<T>, t: &[u8]) -> Option<T> {
    s.whole(t).then_some(s.value)
}

/// A path's folder: up to and including its last `\\`, `/` or `:`; empty when it has none.
// D2-17786
#[must_use]
pub fn directory_of(path: &[u8]) -> &[u8] {
    match path.iter().rposition(|c| matches!(c, b'\\' | b'/' | b':')) {
        Some(at) => &path[..=at],
        None => &[],
    }
}

/// The text of a binary name: its word and its bytes as two small hexadecimal digits a byte (the
/// word low byte first), broken into lines of 100 characters, each full line ending in a line
/// break.
// D2-17790, D2-17799
fn binary_text(tag: u32, bytes: &[u8]) -> Vec<u8> {
    let mut raw = Vec::with_capacity(bytes.len() + 4);
    raw.extend_from_slice(&tag.to_le_bytes());
    raw.extend_from_slice(bytes);
    let hex: Vec<u8> = raw
        .iter()
        .flat_map(|b| format!("{b:02x}").into_bytes())
        .collect();
    let mut out = Vec::with_capacity(hex.len() + hex.len() / 100 + 1);
    for line in hex.chunks(100) {
        out.extend_from_slice(line);
        // A short last line is copied with the string's end, so its line break is never seen.
        if line.len() == 100 {
            out.push(b'\n');
        }
    }
    out
}

/// The C library's case-blind comparison of two narrow strings (`_stricmp`), up to each one's
/// first zero: ASCII letters fold to lower case.
#[must_use]
pub fn stricmp(a: &[u8], b: &[u8]) -> Ordering {
    let (a, b) = (number::cstr(a), number::cstr(b));
    let fold = |c: u8| c.to_ascii_lowercase();
    a.iter().map(|&c| fold(c)).cmp(b.iter().map(|&c| fold(c)))
}

fn eq_ignore_case(a: &[u8], b: &[u8]) -> bool {
    stricmp(a, b) == Ordering::Equal
}

/// What a parse knows of the file a tree came from.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileInfo {
    pub filename: Vec<u8>,
    pub errors: u32,
    pub warnings: u32,
    pub compatibility_version: u32,
}

/// A node of a text tree.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Node {
    name: Option<Name>,
    /// Where the node's name began: its line and column, from 1.
    pub line: u16,
    pub column: u16,
    comment: Vec<u8>,
    filename: Vec<u8>,
    children: Vec<Node>,
    file_info: Option<FileInfo>,
}

impl Node {
    /// A node with no name, no children and no comment.
    // D2-14462, D2-14476
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A node named by text.
    #[must_use]
    pub fn named(name: impl Into<Vec<u8>>) -> Self {
        let mut n = Self::new();
        n.set_name_text(name);
        n
    }

    /// The node made empty: its name, children, place, comment and file gone.
    // D2-14463
    pub fn destroy(&mut self) {
        let info = self.file_info.take();
        *self = Self::default();
        drop(info);
    }

    /// A copy of another node and its children, with a fresh file record of the same name and
    /// version.
    // D2-14464, D2-14477
    pub fn assign(&mut self, other: &Self) {
        self.destroy();
        self.file_info = other.file_info.as_ref().map(|f| FileInfo {
            filename: f.filename.clone(),
            compatibility_version: f.compatibility_version,
            ..FileInfo::default()
        });
        self.name.clone_from(&other.name);
        self.line = other.line;
        self.column = other.column;
        self.comment.clone_from(&other.comment);
        self.filename.clone_from(&other.filename);
        for c in &other.children {
            self.make_child().assign(c);
        }
    }

    // ---- the name ------------------------------------------------------------------------------

    /// The name as it is held, if any.
    #[must_use]
    pub const fn name(&self) -> Option<&Name> {
        self.name.as_ref()
    }

    /// The name's kind.
    // D2-14487
    #[must_use]
    pub const fn name_kind(&self) -> NameKind {
        match &self.name {
            Some(n) => n.kind(),
            None => NameKind::Null,
        }
    }

    /// Set the name.
    pub fn set_name(&mut self, name: Name) {
        self.name = Some(name);
    }

    /// Name the node by text.
    // D2-04876, D2-04881, D2-14445
    pub fn set_name_text(&mut self, name: impl Into<Vec<u8>>) {
        self.name = Some(Name::Text(name.into()));
    }

    // D2-06949
    pub fn set_name_u32(&mut self, v: u32) {
        self.name = Some(Name::U32(v));
    }

    // D2-14529
    pub fn set_name_i32(&mut self, v: i32) {
        self.name = Some(Name::I32(v));
    }

    // D2-06944
    pub fn set_name_f32(&mut self, v: f32) {
        self.name = Some(Name::F32(v));
    }

    // D2-14530
    pub fn set_name_f64(&mut self, v: f64) {
        self.name = Some(Name::F64(v));
    }

    // D2-08724
    pub fn set_name_bool(&mut self, v: i32) {
        self.name = Some(Name::Bool(v));
    }

    // D2-13073
    pub fn set_name_hex(&mut self, v: u32) {
        self.name = Some(Name::Hex(v));
    }

    // D2-12993
    pub fn set_name_instance_id(&mut self, v: u64) {
        self.name = Some(Name::InstanceId(v));
    }

    /// The name read as `kind`: as it is when it already is one; else its text read as one, and
    /// then the name **becomes** that value (the text form of it is what a later read or a writer
    /// sees). None when there is no name or its text does not read as `kind`.
    fn name_as(&mut self, kind: NameKind) -> Option<&Name> {
        let current = self.name.as_ref()?;
        if current.kind() != kind {
            let converted = Name::from_text(kind, &current.to_text())?;
            self.name = Some(converted);
        }
        self.name.as_ref()
    }

    /// The name as text (converting a number to its text form keeps the text as the name).
    // D2-03400, D2-14446
    pub fn name_text(&mut self) -> Option<Vec<u8>> {
        match self.name_as(NameKind::Text)? {
            Name::Text(t) => Some(t.clone()),
            _ => None,
        }
    }

    /// The name's text without keeping any conversion: what [`Self::name_text`] answers.
    #[must_use]
    pub fn name_text_now(&self) -> Option<Vec<u8>> {
        self.name.as_ref().map(Name::to_text)
    }

    /// The name as an unsigned 32-bit number.
    // D2-03541
    pub fn name_u32(&mut self) -> Option<u32> {
        match self.name_as(NameKind::U32)? {
            Name::U32(v) => Some(*v),
            _ => None,
        }
    }

    // D2-04244
    pub fn name_i32(&mut self) -> Option<i32> {
        match self.name_as(NameKind::I32)? {
            Name::I32(v) => Some(*v),
            _ => None,
        }
    }

    // D2-03399
    pub fn name_f32(&mut self) -> Option<f32> {
        match self.name_as(NameKind::F32)? {
            Name::F32(v) => Some(*v),
            _ => None,
        }
    }

    // D2-03540
    pub fn name_bool(&mut self) -> Option<bool> {
        match self.name_as(NameKind::Bool)? {
            Name::Bool(v) => Some(*v != 0),
            _ => None,
        }
    }

    /// Whether the name, as text, is `word` (any case).
    // D2-14449, D2-14450
    #[must_use]
    pub fn is_keyword(&self, word: &[u8]) -> bool {
        self.name_text_now()
            .is_some_and(|t| stricmp(&t, word) == Ordering::Equal)
    }

    // ---- children ------------------------------------------------------------------------------

    /// The children, in order.
    #[must_use]
    pub fn children(&self) -> &[Node] {
        &self.children
    }

    /// The children, to change.
    pub fn children_mut(&mut self) -> &mut [Node] {
        &mut self.children
    }

    /// How many children the node has.
    #[must_use]
    pub fn len(&self) -> usize {
        self.children.len()
    }

    /// Whether the node has no children.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.children.is_empty()
    }

    /// The child at `i`.
    // D2-14432, D2-14426, D2-14439, D2-14425, D2-14459, D2-14438
    #[must_use]
    pub fn child(&self, i: usize) -> Option<&Node> {
        self.children.get(i)
    }

    /// The child at `i`, to change.
    pub fn child_mut(&mut self, i: usize) -> Option<&mut Node> {
        self.children.get_mut(i)
    }

    /// The only child, when there is exactly one and it has no children itself.
    // D2-14433
    #[must_use]
    pub fn leaf(&self) -> Option<&Node> {
        match self.children.as_slice() {
            [c] if c.children.is_empty() => Some(c),
            _ => None,
        }
    }

    /// The only childless child, to read as a number.
    pub fn leaf_mut(&mut self) -> Option<&mut Node> {
        match self.children.as_mut_slice() {
            [c] if c.children.is_empty() => Some(c),
            _ => None,
        }
    }

    /// The leaf's name as text.
    // D2-14447
    pub fn leaf_text(&mut self) -> Option<Vec<u8>> {
        self.leaf_mut()?.name_text()
    }

    /// The name of the child at `i`, as text.
    // D2-14444, D2-14448
    pub fn child_text(&mut self, i: usize) -> Option<Vec<u8>> {
        self.children.get_mut(i)?.name_text()
    }

    /// The first child named `word` (any case).
    // D2-14452
    #[must_use]
    pub fn find(&self, word: &[u8]) -> Option<&Node> {
        self.children.iter().find(|c| c.is_keyword(word))
    }

    /// Append a child.
    // D2-14467, D2-14468
    pub fn adopt(&mut self, child: Node) {
        self.children.push(child);
    }

    /// Append a new, empty child and give it back.
    // D2-14478
    pub fn make_child(&mut self) -> &mut Node {
        self.children.push(Node::new());
        self.children.last_mut().unwrap_or_else(|| unreachable!())
    }

    /// Append a child named by text.
    // D2-04877, D2-04882
    pub fn make_child_text(&mut self, name: impl Into<Vec<u8>>) -> &mut Node {
        let c = self.make_child();
        c.set_name_text(name);
        c
    }

    // D2-04872
    pub fn make_child_u32(&mut self, v: u32) -> &mut Node {
        let c = self.make_child();
        c.set_name_u32(v);
        c
    }

    // D2-07905
    pub fn make_child_i32(&mut self, v: i32) -> &mut Node {
        let c = self.make_child();
        c.set_name_i32(v);
        c
    }

    // D2-07467
    pub fn make_child_f32(&mut self, v: f32) -> &mut Node {
        let c = self.make_child();
        c.set_name_f32(v);
        c
    }

    // D2-11944
    pub fn make_child_bool(&mut self, v: bool) -> &mut Node {
        let c = self.make_child();
        c.set_name_bool(i32::from(v));
        c
    }

    /// Take the child at `i` out.
    // D2-14434
    pub fn remove_child(&mut self, i: usize) -> Option<Node> {
        (i < self.children.len()).then(|| self.children.remove(i))
    }

    // ---- comments and place --------------------------------------------------------------------

    /// The node's comment: its lines, each ending in a line break.
    #[must_use]
    pub fn comment(&self) -> &[u8] {
        &self.comment
    }

    /// Add a comment: nothing for an empty one; else its text, and a line break unless it ends in
    /// one.
    // D2-14479
    pub fn add_comment(&mut self, c: &[u8]) {
        if c.is_empty() {
            return;
        }
        self.comment.extend_from_slice(c);
        if c.last() != Some(&b'\n') {
            self.comment.push(b'\n');
        }
    }

    /// Clear the comment, and with `deep` every descendant's too.
    // D2-14453
    pub fn clear_comments(&mut self, deep: bool) {
        self.comment.clear();
        if deep {
            for c in &mut self.children {
                c.clear_comments(deep);
            }
        }
    }

    /// Where the node's name began.
    // D2-14422, D2-14423
    pub fn set_place(&mut self, line: u16, column: u16) {
        self.line = line;
        self.column = column;
    }

    /// The file the node was read from (the parse's name for it).
    #[must_use]
    pub fn filename(&self) -> &[u8] {
        &self.filename
    }

    pub fn set_filename(&mut self, f: impl Into<Vec<u8>>) {
        self.filename = f.into();
    }

    /// The root's record of its file.
    // D2-14429
    #[must_use]
    pub fn file_info(&self) -> Option<&FileInfo> {
        self.file_info.as_ref()
    }

    /// The root's record of its file, made when missing, named `filename`.
    // D2-14469
    pub fn init_file_info(&mut self, filename: &[u8]) {
        filename.clone_into(
            &mut self
                .file_info
                .get_or_insert_with(FileInfo::default)
                .filename,
        );
    }

    /// A root counts an error found in it.
    // D2-14454, D2-14456, D2-14471
    pub fn report_error(&mut self) {
        if let Some(f) = &mut self.file_info {
            f.errors += 1;
        }
    }

    /// A root counts a warning found in it.
    // D2-14455, D2-14457, D2-14472
    pub fn report_warning(&mut self) {
        if let Some(f) = &mut self.file_info {
            f.warnings += 1;
        }
    }
}
