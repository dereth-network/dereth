//! The `.keymap` text file: its node tree, read and write, and every validation
//! message.
//!
//! File nodes and their names are the serialisation primitives' file-node form. For the
//! byte-level text form this follows the client's own file-node formatter -- the node and sub-node writers, the indent step, the
//! separator, the quoted-string writer and the wrap test -- plus the file-node conversions for
//! the master map, one input map and a control specification.
//!
//! # The tree shape is not the obvious one
//!
//! It is natural to read `MetaKeys` as
//! `<ControlCode tree> -> <bit index>` and a binding as
//! `<ControlCode tree> [-> <metamode>] -> <activation> -> <action>`, and `Devices` as
//! `<index:uint32> -> <GUID name>`. Retail uses the opposite nesting in all three cases:
//!
//! * `Devices` — one child per device **named by the device type** (`Keyboard`), whose single child
//!   is the GUID name (the reader calls
//!   its device-type-from-string lookup on the node's *own* name).
//! * `MetaKeys` — one child **named by the bit index 1..32**, whose own 2-or-3 children are the
//!   `ControlCode` (the reader reads the name as a uint32 and then parses that *same* node
//!   as a control specification).
//! * `Bindings` — one child per input map named by the input-map enum, and inside it one child per
//!   binding **named by the action enum**, whose children are the (nameless) control-specification
//!   node, then optionally the meta-mode, then optionally the activation name.
//!
//! The meta-mode of a binding is stored **bit-reversed**, on
//! both the write and the read side, so Shift (`0x80000000`) is written
//! as 1. The document does not mention that at all.

use std::fmt::Write as _;

use crate::error::InputError;
use crate::keymap::{
    DeviceKeyMapEntry, InputMap, MasterInputMap, GUID_SYS_KEYBOARD, GUID_SYS_MOUSE, GUID_VIRTUAL,
};
use crate::names::{enum_name_for_action, enum_name_for_input_map, input_map_for_enum_name};
use crate::spec::{
    activation, ControlChord, ControlCode, ControlNames, DeviceType, SubControlIndex,
};
use crate::{ActionId, InputMapId};

/// The polymorphic node key, in the four shapes a `.keymap` file uses.
///
/// The text form carries no type tag: the parser decides by trying and
/// friends, so a round-trip re-derives the type from the spelling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeName {
    PString(String),
    UInt32(u32),
    /// A hexadecimal node name. This writer uses `0x%08X`; the reader accepts any
    /// `0x`-prefixed value.
    Hex(u32),
}

impl NodeName {
    #[must_use]
    pub fn as_text(&self) -> String {
        match self {
            Self::PString(s) => s.clone(),
            Self::UInt32(v) => v.to_string(),
            Self::Hex(v) => format!("0x{v:08X}"),
        }
    }

    /// A `Hex` node answers too.
    #[must_use]
    pub fn as_u32(&self) -> Option<u32> {
        match self {
            Self::UInt32(v) | Self::Hex(v) => Some(*v),
            Self::PString(_) => None,
        }
    }
}

/// One node of the tree. The file is a forest: the formatter formats the
/// root's *children*, never the root itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileNode {
    pub name: NodeName,
    pub children: Vec<FileNode>,
}

impl FileNode {
    #[must_use]
    pub fn text(name: &str) -> Self {
        Self {
            name: NodeName::PString(name.to_owned()),
            children: Vec::new(),
        }
    }

    #[must_use]
    pub fn uint(v: u32) -> Self {
        Self {
            name: NodeName::UInt32(v),
            children: Vec::new(),
        }
    }

    #[must_use]
    pub fn hex(v: u32) -> Self {
        Self {
            name: NodeName::Hex(v),
            children: Vec::new(),
        }
    }

    /// The name a nameless node produces. Written as `""` because
    /// the writer's string quoting quotes an empty string.
    #[must_use]
    pub fn anonymous() -> Self {
        Self::text("")
    }

    #[must_use]
    pub fn with(mut self, child: Self) -> Self {
        self.children.push(child);
        self
    }

    #[must_use]
    pub fn child(&self, i: usize) -> Option<&Self> {
        self.children.get(i)
    }

    #[must_use]
    pub fn name_text(&self) -> String {
        self.name.as_text()
    }
}

// ---------------------------------------------------------------------------------------------
// The text form -- writer
// ---------------------------------------------------------------------------------------------

/// The keymap parser's formatting state. Saving starts it at `indentation = 0`,
/// `separate_subnodes = true`.
#[derive(Debug)]
struct Formatter {
    indentation: u32,
    separate_subnodes: bool,
    want_blank: bool,
    first_subnode: bool,
    out: String,
}

/// The parser's indent string -- two spaces.
const INDENT: &str = "  ";
/// `format_node`'s call to `check_wrap_tree(node, 0x50, 3)`.
const WRAP_WIDTH: u32 = 0x50;
const WRAP_DEPTH: u32 = 3;

impl Formatter {
    fn new() -> Self {
        Self {
            indentation: 0,
            separate_subnodes: true,
            want_blank: false,
            first_subnode: true,
            out: String::new(),
        }
    }

    /// Indent if the writer is at the start of a line.
    fn maybe_indent(&mut self) {
        if self.separate_subnodes {
            for _ in 0..self.indentation {
                self.out.push_str(INDENT);
            }
        }
    }

    /// The separator -- a newline when subnodes are separated, a space
    /// otherwise. That single choice is what makes a short subtree collapse onto one line.
    fn separator(&mut self) {
        self.out
            .push(if self.separate_subnodes { '\n' } else { ' ' });
    }

    /// Write a quoted string.
    fn put_quoted(&mut self, s: &str) {
        let needs_quote = s.is_empty()
            || s.chars().any(|c| {
                matches!(
                    c,
                    '\t' | ' ' | '#' | '(' | ')' | ',' | '[' | ']' | '{' | '}'
                )
            });
        if needs_quote {
            self.out.push('"');
        }
        for c in s.chars() {
            if matches!(c, '\n' | '\r' | '"' | '\\') {
                self.out.push('\\');
            }
            self.out.push(c);
        }
        if needs_quote {
            self.out.push('"');
        }
    }

    /// Format a node's subnodes.
    fn format_subnodes(&mut self, node: &FileNode) {
        self.first_subnode = true;
        for child in &node.children {
            if self.indentation == 0 {
                self.want_blank = true;
            }
            self.format_node(child);
            self.first_subnode = false;
        }
    }

    /// Format one node.
    fn format_node(&mut self, node: &FileNode) {
        let wrap =
            self.separate_subnodes && check_wrap_tree(node, WRAP_WIDTH, WRAP_DEPTH) > WRAP_WIDTH;
        if self.want_blank || (!self.first_subnode && wrap) {
            self.want_blank = false;
            // The original also requires the node's comment string to be empty; nothing this crate
            // builds carries a comment, so the blank line is always emitted here.
            self.out.push('\n');
        }
        self.maybe_indent();
        self.put_quoted(&node.name_text());
        if !node.children.is_empty() {
            let saved = self.separate_subnodes;
            self.separate_subnodes = wrap;
            self.separator();
            self.maybe_indent();
            self.out.push('[');
            self.separator();
            self.indentation += 1;
            self.format_subnodes(node);
            self.indentation -= 1;
            self.maybe_indent();
            self.out.push(']');
            self.want_blank = self.separate_subnodes;
            self.separate_subnodes = saved;
        }
        self.separator();
    }
}

/// The wrap test -- the flattened width of a subtree, capped at `budget`
/// and `depth` levels deep. `budget + 1` means "wider than the budget", which is the wrap signal.
fn check_wrap_tree(node: &FileNode, budget: u32, depth: u32) -> u32 {
    if depth == 0 {
        return budget + 1;
    }
    let name_len = u32::try_from(node.name_text().len()).unwrap_or(budget + 1);
    let mut w = name_len;
    if !node.children.is_empty() {
        // The client's length *includes* the NUL, so its `len + 5` is `name_len + 6`: the name plus
        // " [ " and " ] ".
        w = name_len + 6;
        if w <= budget {
            for child in &node.children {
                w += check_wrap_tree(child, budget.saturating_sub(w), depth - 1);
                if budget < w {
                    break;
                }
            }
        }
    }
    w
}

/// Format the root's children.
#[must_use]
pub fn write_file_nodes(root: &FileNode) -> String {
    let mut f = Formatter::new();
    f.format_subnodes(root);
    f.out
}

// ---------------------------------------------------------------------------------------------
// The text form -- reader
// ---------------------------------------------------------------------------------------------

#[derive(Debug, PartialEq, Eq)]
enum Token {
    Word(String, bool),
    Open,
    Close,
}

fn tokenize(text: &str) -> Result<Vec<Token>, InputError> {
    let mut out = Vec::new();
    let mut it = text.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '#' => {
                for c in it.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            }
            '[' => out.push(Token::Open),
            ']' => out.push(Token::Close),
            '"' => {
                let mut s = String::new();
                loop {
                    match it.next() {
                        None => return Err(InputError::KeymapFile("unterminated string".into())),
                        Some('\\') => match it.next() {
                            Some('n') => s.push('\n'),
                            Some('r') => s.push('\r'),
                            Some('t') => s.push('\t'),
                            Some(e) => s.push(e),
                            None => {
                                return Err(InputError::KeymapFile("unterminated escape".into()))
                            }
                        },
                        Some('"') => break,
                        Some(other) => s.push(other),
                    }
                }
                out.push(Token::Word(s, true));
            }
            c if c.is_whitespace() || c == ',' => {}
            _ => {
                let mut s = String::new();
                s.push(c);
                while let Some(&n) = it.peek() {
                    if n.is_whitespace() || matches!(n, '[' | ']' | '#' | ',') {
                        break;
                    }
                    s.push(n);
                    it.next();
                }
                out.push(Token::Word(s, false));
            }
        }
    }
    Ok(out)
}

fn classify(word: String, quoted: bool) -> NodeName {
    if quoted {
        return NodeName::PString(word);
    }
    if let Some(h) = word.strip_prefix("0x").or_else(|| word.strip_prefix("0X")) {
        if let Ok(v) = u32::from_str_radix(h, 16) {
            return NodeName::Hex(v);
        }
    }
    if let Ok(v) = word.parse::<u32>() {
        return NodeName::UInt32(v);
    }
    NodeName::PString(word)
}

/// The file parser, reduced to the subset a
/// `.keymap` file uses. Returns a synthetic root whose children are the file's top-level nodes.
///
/// # Errors
/// [`InputError::KeymapFile`] on unbalanced brackets or an unterminated string.
pub fn read_file_nodes(text: &str) -> Result<FileNode, InputError> {
    let tokens = tokenize(text)?;
    let mut stack: Vec<FileNode> = vec![FileNode {
        name: NodeName::PString(String::new()),
        children: Vec::new(),
    }];
    let mut i = 0usize;
    while i < tokens.len() {
        match &tokens[i] {
            Token::Word(w, q) => {
                let node = FileNode {
                    name: classify(w.clone(), *q),
                    children: Vec::new(),
                };
                if matches!(tokens.get(i + 1), Some(Token::Open)) {
                    stack.push(node);
                    i += 1;
                } else {
                    let Some(parent) = stack.last_mut() else {
                        return Err(InputError::KeymapFile("node outside the root".into()));
                    };
                    parent.children.push(node);
                }
            }
            Token::Open => return Err(InputError::KeymapFile("unexpected '['".into())),
            Token::Close => {
                if stack.len() < 2 {
                    return Err(InputError::KeymapFile("unexpected ']'".into()));
                }
                let Some(done) = stack.pop() else {
                    return Err(InputError::KeymapFile("unexpected ']'".into()));
                };
                let Some(parent) = stack.last_mut() else {
                    return Err(InputError::KeymapFile("unexpected ']'".into()));
                };
                parent.children.push(done);
            }
        }
        i += 1;
    }
    match stack.pop() {
        Some(root) if stack.is_empty() => Ok(root),
        _ => Err(InputError::KeymapFile("unterminated '['".into())),
    }
}

// ---------------------------------------------------------------------------------------------
// Master input map <-> keymap node tree
// ---------------------------------------------------------------------------------------------

/// The device name -- the three names the client understands, else a
/// literal GUID.
fn guid_name(guid: [u8; 16]) -> String {
    match guid {
        GUID_SYS_KEYBOARD => "GUID_SysKeyboard".to_owned(),
        GUID_SYS_MOUSE => "GUID_SysMouse".to_owned(),
        GUID_VIRTUAL => "GUID_Virtual".to_owned(),
        g => {
            let mut s = String::new();
            let d1 = u32::from_le_bytes([g[0], g[1], g[2], g[3]]);
            let d2 = u16::from_le_bytes([g[4], g[5]]);
            let d3 = u16::from_le_bytes([g[6], g[7]]);
            let _ = write!(s, "{d1:08X}-{d2:04X}-{d3:04X}-{:02X}{:02X}-", g[8], g[9]);
            for b in &g[10..] {
                let _ = write!(s, "{b:02X}");
            }
            s
        }
    }
}

/// Read a GUID from a file node.
fn guid_from_name(s: &str) -> Option<[u8; 16]> {
    match s {
        "GUID_SysKeyboard" => Some(GUID_SYS_KEYBOARD),
        "GUID_SysMouse" => Some(GUID_SYS_MOUSE),
        "GUID_Virtual" => Some(GUID_VIRTUAL),
        _ => {
            let hex: Vec<u8> = s
                .chars()
                .filter(|c| c.is_ascii_hexdigit())
                .collect::<String>()
                .as_bytes()
                .chunks(2)
                .filter_map(|p| u8::from_str_radix(std::str::from_utf8(p).ok()?, 16).ok())
                .collect();
            if hex.len() != 16 {
                return None;
            }
            let mut g = [0u8; 16];
            g[0..4].copy_from_slice(&u32::from_str_radix(&s[0..8], 16).ok()?.to_le_bytes());
            g[4..6].copy_from_slice(&u16::from_str_radix(&s[9..13], 16).ok()?.to_le_bytes());
            g[6..8].copy_from_slice(&u16::from_str_radix(&s[14..18], 16).ok()?.to_le_bytes());
            g[8..].copy_from_slice(&hex[8..]);
            Some(g)
        }
    }
}

/// Write a control specification -- 2 or 3 children **into** `node`.
fn control_to_file_node(node: &mut FileNode, cs: ControlCode, dt: DeviceType) {
    node.children
        .push(FileNode::uint(u32::from(cs.device_index())));
    let name = ControlNames::name_by_semantic(dt, cs.offset())
        .map_or_else(|| format!("0x{:04X}", cs.offset()), ToOwned::to_owned);
    node.children.push(FileNode::text(&name));
    if let Some(sub) = cs.sub_control().name() {
        node.children.push(FileNode::text(sub));
    }
}

/// Read a control specification from a file node.
fn control_from_file_node(node: &FileNode) -> Result<ControlCode, InputError> {
    let n = node.children.len();
    if !(2..=3).contains(&n) {
        return Err(InputError::KeymapFile(format!(
            "ERROR - Must have 2 or 3 sub nodes, has {n} instead.\n"
        )));
    }
    let device = node.children[0]
        .name
        .as_u32()
        .filter(|v| *v < 0x100)
        .ok_or_else(|| {
            InputError::KeymapFile(format!(
                "ERROR - invalid device index \"{}\"\n",
                node.children[0].name_text()
            ))
        })?;
    let name = node.children[1].name_text();
    let (_dt, offset) = ControlNames::semantic_by_name(&name).ok_or_else(|| {
        InputError::KeymapFile(format!("ERROR - invalid device constant name \"{name}\"\n"))
    })?;
    let sub = if n == 3 {
        let s = node.children[2].name_text();
        SubControlIndex::from_name(&s).ok_or_else(|| {
            InputError::KeymapFile(format!("ERROR - invalid sub control index \"{s}\"\n"))
        })?
    } else {
        SubControlIndex::None
    };
    // The device index is validated below 0x100 immediately above, so the narrowing is exact.
    #[allow(clippy::cast_possible_truncation)]
    Ok(ControlCode::new(device as u8, sub, offset))
}

/// Reverse the bits of a word.
#[must_use]
pub const fn reverse_bits(v: u32) -> u32 {
    v.reverse_bits()
}

impl MasterInputMap {
    /// Write the master input map -- the four-part tree.
    #[must_use]
    pub fn to_file_node(&self) -> FileNode {
        let mut root = FileNode {
            name: NodeName::PString(String::new()),
            children: Vec::new(),
        };

        root.children
            .push(FileNode::text(&self.name).with(FileNode::text(&guid_name(self.guid))));

        let mut devices = FileNode::text("Devices");
        for d in &self.devices {
            devices.children.push(
                FileNode::text(d.device_type.name()).with(FileNode::text(&guid_name(d.guid))),
            );
        }
        root.children.push(devices);

        let mut meta = FileNode::text("MetaKeys");
        for (cs, mask) in &self.meta_keys {
            // 0x20 - trailing_zeros(mask), i.e. the 1..32 bit index the reader validates.
            let index = 0x20 - mask.trailing_zeros();
            let mut node = FileNode::uint(index);
            let dt = self.device_type_of(*cs).unwrap_or(DeviceType::Keyboard);
            control_to_file_node(&mut node, *cs, dt);
            meta.children.push(node);
        }
        root.children.push(meta);

        let mut bindings = FileNode::text("Bindings");
        for sec in &self.sections {
            let mut section = FileNode::text(&enum_name_for_input_map(sec.input_map_id));
            for (qc, action) in sec.bindings() {
                let mut node = FileNode::text(&enum_name_for_action(*action));
                let mut cs_node = FileNode::anonymous();
                let dt = self
                    .device_type_of(qc.control)
                    .unwrap_or(DeviceType::Keyboard);
                control_to_file_node(&mut cs_node, qc.control, dt);
                node.children.push(cs_node);
                // The client's own rule: the meta-mode node appears when there is a
                // modifier OR a non-default activation; the activation node only when it is not
                // Click. The mask is written bit-REVERSED.
                if qc.meta_mode != 0 || qc.activation != activation::CLICK {
                    node.children
                        .push(FileNode::hex(reverse_bits(qc.meta_mode)));
                    if qc.activation != activation::CLICK {
                        if let Some(name) = activation::to_name(qc.activation) {
                            node.children.push(FileNode::text(name));
                        }
                    }
                }
                section.children.push(node);
            }
            bindings.children.push(section);
        }
        root.children.push(bindings);
        root
    }

    /// Read the master input map back.
    ///
    /// # Errors
    /// The client's verbatim validation messages.
    pub fn from_file_node(root: &FileNode) -> Result<Self, InputError> {
        if root.children.len() != 4 {
            return Err(InputError::KeymapFile(
                "ERROR - must have exactly 4 parts (Name, Devices, MetaKeys, Bindings)\n".into(),
            ));
        }
        let mut out = Self {
            name: root.children[0].name_text(),
            ..Self::default()
        };
        if let Some(g) = root.children[0].child(0) {
            out.guid = guid_from_name(&g.name_text()).unwrap_or([0; 16]);
        }

        for d in &root.children[1].children {
            let name = d.name_text();
            let device_type = DeviceType::from_name(&name).ok_or_else(|| {
                InputError::KeymapFile(format!("ERROR - not a valid device \"{name}\"\n"))
            })?;
            let guid = d
                .child(0)
                .and_then(|g| guid_from_name(&g.name_text()))
                .ok_or_else(|| InputError::KeymapFile("ERROR - not a valid device\n".into()))?;
            out.devices.push(DeviceKeyMapEntry { device_type, guid });
        }

        for m in &root.children[2].children {
            let index = m
                .name
                .as_u32()
                .filter(|v| (1..=32).contains(v))
                .ok_or_else(|| {
                    InputError::KeymapFile(format!(
                        "ERROR - Invalid index {} , index must be between 1 and 32\n",
                        m.name_text()
                    ))
                })?;
            let mask = 1u32 << (0x20 - index);
            let cs = control_from_file_node(m)
                .map_err(|_| InputError::KeymapFile("ERROR - Invalid device.\n".into()))?;
            out.meta_keys.push((cs, mask));
            out.used_meta_keys |= mask;
        }

        let mut seen: Vec<InputMapId> = Vec::new();
        for s in &root.children[3].children {
            let name = s.name_text();
            let id = input_map_for_enum_name(&name).ok_or_else(|| {
                InputError::KeymapFile(format!(
                    "ERROR - invalid input map name \"{name}\" found.\n"
                ))
            })?;
            if seen.contains(&id) {
                return Err(InputError::KeymapFile(format!(
                    "ERROR - duplicate input map name \"{name}\" found.\n"
                )));
            }
            seen.push(id);
            let mut map = InputMap::new(id);
            for b in &s.children {
                let action =
                    crate::names::action_for_enum_name(&b.name_text()).ok_or_else(|| {
                        InputError::KeymapFile(format!(
                            "Control '{}' is not allowed in this InputMap ('{name}')\n",
                            b.name_text()
                        ))
                    })?;
                let cs_node = b.child(0).ok_or_else(|| {
                    InputError::KeymapFile("ERROR - invalid Control Specification\n".into())
                })?;
                let control = control_from_file_node(cs_node)?;
                let meta_mode = b
                    .child(1)
                    .and_then(|n| n.name.as_u32())
                    .map_or(0, reverse_bits);
                if b.child(1).is_some_and(|n| n.name.as_u32().is_none()) {
                    return Err(InputError::KeymapFile(
                        "ERROR - invalid MetaKey flags\n".into(),
                    ));
                }
                let act = match b.child(2) {
                    None => activation::CLICK,
                    Some(n) => {
                        let t = n.name_text();
                        activation::from_name(&t).ok_or_else(|| {
                            InputError::KeymapFile(format!(
                                "ERROR - invalid control range \"{t}\"\n"
                            ))
                        })?
                    }
                };
                map.add_mapping(ControlChord::new(control, meta_mode, act), action);
            }
            out.sections.push(map);
        }
        Ok(out)
    }

    /// Parse a `.keymap` file's text.
    ///
    /// # Errors
    /// Whatever [`read_file_nodes`] and [`MasterInputMap::from_file_node`] report.
    pub fn from_keymap_text(text: &str) -> Result<Self, InputError> {
        Self::from_file_node(&read_file_nodes(text)?)
    }

    /// Render a `.keymap` file.
    #[must_use]
    pub fn to_keymap_text(&self) -> String {
        write_file_nodes(&self.to_file_node())
    }
}

/// `ActionId`/`InputMapId` names, exported for tests and tools that print them.
#[must_use]
pub fn action_enum_name(a: ActionId) -> String {
    enum_name_for_action(a)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the quoting rule -- the ten characters that force a quote,
    /// and the four that are backslash-escaped.
    #[test]
    fn quoting_follows_the_binarys_character_set() {
        let mut f = Formatter::new();
        f.put_quoted("DIK_W");
        f.put_quoted("");
        f.put_quoted("a b");
        f.put_quoted("say \"hi\"");
        // `say "hi"` contains a space, so it is quoted *and* its quotes are escaped.
        assert_eq!(f.out, "DIK_W\"\"\"a b\"\"say \\\"hi\\\"\"");
    }

    /// Oracle: the bit reversal on the write side -- Shift's `0x80000000`
    /// becomes 1 on disk, and the read side reverses it back.
    #[test]
    fn meta_mode_is_stored_bit_reversed() {
        assert_eq!(reverse_bits(0x8000_0000), 1);
        assert_eq!(reverse_bits(0x4000_0000), 2);
        assert_eq!(reverse_bits(0xC000_0000), 3);
        assert_eq!(reverse_bits(reverse_bits(0x2000_0000)), 0x2000_0000);
    }

    /// Oracle: the client's wrapping check collapses a short subtree onto one line, while a long
    /// subtree remains expanded.
    #[test]
    fn short_subtrees_collapse() {
        let n = FileNode::text("Keyboard").with(FileNode::text("GUID_SysKeyboard"));
        let mut root = FileNode {
            name: NodeName::PString(String::new()),
            children: Vec::new(),
        };
        root.children.push(n);
        let text = write_file_nodes(&root);
        assert!(text.contains("Keyboard [ GUID_SysKeyboard ]"), "{text}");
    }

    /// Oracle: `read_file_nodes` — a written tree reads back identically, which is the property the
    /// three-way merge and the save-on-exit depend on.
    #[test]
    fn the_text_form_round_trips() {
        let mut root = FileNode {
            name: NodeName::PString(String::new()),
            children: Vec::new(),
        };
        root.children.push(
            FileNode::text("Devices")
                .with(FileNode::text("Keyboard").with(FileNode::text("GUID_SysKeyboard"))),
        );
        root.children.push(
            FileNode::text("MetaKeys").with(
                FileNode::uint(1)
                    .with(FileNode::uint(0))
                    .with(FileNode::text("DIK_LSHIFT")),
            ),
        );
        let text = write_file_nodes(&root);
        assert_eq!(read_file_nodes(&text).unwrap(), root, "{text}");
    }
}
