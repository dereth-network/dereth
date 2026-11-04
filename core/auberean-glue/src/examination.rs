//! Examination profiles, as scripts build and compare them: what the examination window shows of
//! something (an item, a skill, an effect, a recipe).
//!
//! A profile holds the examination request it answers, its lines (nodes) in order, and a word of
//! every tab its lines were made for. Each line has a kind, the tabs it shows on, the element it is
//! drawn with (its appearance) and a value: a number, a yes or no, a string reference, a picture, an
//! icon, an indent, a line break, a time left, or an effect to describe in turn. Adding a line notes
//! its tabs in the profile's word, and keeps the line only when it shows on a tab the request asks
//! for or on the first tab (`0x203`).
//!
//! It packs as the request, the count of lines and each line (its kind and tabs, then by kind: the
//! appearance and a word for a number, a float, a yes or no and an effect; the appearance and the
//! string reference; a word for a picture and an indent; the appearance and a 64-bit time for a
//! time left; the icon's layers; the appearance and a 64-bit number), then the tabs word. The script
//! machine holds a profile as its packed form, and, when its request asks for tabs its packed form
//! leaves out (a request about nothing with tabs set on it, as a profile of an effect's lines is
//! made), those tabs beside it.

use auberean_script::native::{Call, NativeTable, Status, Stop};
use auberean_script::native_object::{native_type, NativeObject, TextInfo};
use auberean_script::value::{tag, NULL};
use auberean_script::wire::{In, Out};

use crate::text::table_or_none;

const PROFILE: u32 = 0x99;

/// The first tab, always shown.
pub const FIRST_TAB: u32 = 0x203;

/// The kinds of line.
pub mod kind {
    pub const INT: u32 = 0x4000_0001;
    pub const FLOAT: u32 = 0x4000_0002;
    pub const BOOL: u32 = 0x4000_0003;
    pub const STRING: u32 = 0x4000_0004;
    pub const IMAGE: u32 = 0x4000_0005;
    pub const BREAK: u32 = 0x4000_0006;
    pub const INDENT: u32 = 0x4000_0007;
    pub const COUNTDOWN: u32 = 0x4000_0008;
    pub const ICON: u32 = 0x4000_0009;
    pub const LONG: u32 = 0x4000_000A;
    pub const EFFECT: u32 = 0x4000_000C;
}

/// The mapping a string table is found through by enumeration value.
const TABLE_ENUM_MAPPING: u32 = 0x4000_0004;

/// One line of a profile. A copy is a copy of every part; dropping it lets go of its string
/// reference and icon.
// D2-12182, D2-12192
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub kind: u32,
    /// The tabs it shows on.
    pub tabs: u32,
    /// The element it is drawn with (for the kinds that pack one).
    pub appearance: u32,
    /// A word or a 64-bit number (the low word for the word kinds).
    pub value: u64,
    /// A time left, in seconds (-1 for none).
    pub time: f64,
    pub text: TextInfo,
    /// An icon's layers: three words each.
    pub icon: Vec<[u32; 3]>,
}

/// The element a line is drawn with until it is given one: the examination's text template.
#[must_use]
pub fn default_appearance() -> u32 {
    auberean_assets::strings::token("ExaminationTextTemplate")
}

impl Default for Node {
    /// A line of no kind on no tab, drawn with the default appearance: no value, no time, an
    /// empty string reference and icon.
    // D2-12191
    fn default() -> Self {
        Self {
            kind: 0,
            tabs: 0,
            appearance: default_appearance(),
            value: 0,
            time: -1.0,
            text: TextInfo::default(),
            icon: Vec::new(),
        }
    }
}

impl Node {
    /// A new line of a kind on its tabs, otherwise as [`Node::default`].
    #[must_use]
    pub fn new(kind: u32, tabs: u32) -> Self {
        Self {
            kind,
            tabs,
            ..Self::default()
        }
    }

    /// The word value of the kinds that hold one (a number, an indent, an effect), else 0.
    // D2-12183
    #[must_use]
    pub fn int(&self) -> u32 {
        match self.kind {
            kind::INT | kind::INDENT | kind::EFFECT => (self.value & 0xFFFF_FFFF) as u32,
            _ => 0,
        }
    }

    /// The 64-bit number of a 64-bit line, else 0.
    #[must_use]
    pub fn long_int(&self) -> u64 {
        if self.kind == kind::LONG {
            self.value
        } else {
            0
        }
    }

    /// The float of a float line, else 0.
    // D2-12185
    #[must_use]
    pub fn float(&self) -> f32 {
        if self.kind == kind::FLOAT {
            #[allow(clippy::cast_possible_truncation)]
            f32::from_bits(self.value as u32)
        } else {
            0.0
        }
    }

    /// The word of a yes-or-no line, else 0.
    // D2-12186
    #[must_use]
    pub fn bool(&self) -> u32 {
        self.word_of(kind::BOOL)
    }

    /// The picture of a picture line, else 0.
    // D2-12187
    #[must_use]
    pub fn image(&self) -> u32 {
        self.word_of(kind::IMAGE)
    }

    /// The time left of a time-left line, else -1.
    // D2-12188
    #[must_use]
    pub fn countdown(&self) -> f64 {
        if self.kind == kind::COUNTDOWN {
            self.time
        } else {
            -1.0
        }
    }

    /// The icon's layers, whatever the kind (empty for a line that is not an icon).
    // D2-12189
    #[must_use]
    pub fn icon(&self) -> &[[u32; 3]] {
        &self.icon
    }

    fn word_of(&self, k: u32) -> u32 {
        if self.kind == k {
            (self.value & 0xFFFF_FFFF) as u32
        } else {
            0
        }
    }

    /// Read one line.
    fn read(r: &mut In<'_>) -> auberean_script::wire::Result<Self> {
        let mut n = Self::new(r.u32()?, r.u32()?);
        match n.kind {
            kind::INT | kind::EFFECT | kind::FLOAT | kind::BOOL => {
                n.appearance = r.u32()?;
                n.value = u64::from(r.u32()?);
            }
            kind::STRING => {
                n.appearance = r.u32()?;
                n.text = TextInfo::unpack(r)?;
            }
            kind::IMAGE | kind::INDENT => n.value = u64::from(r.u32()?),
            kind::COUNTDOWN => {
                n.appearance = r.u32()?;
                n.time = f64::from_bits(r.u64()?);
            }
            kind::ICON => {
                let k = r.u32()? as usize;
                let k = r.count(k, 12)?;
                for _ in 0..k {
                    n.icon.push([r.u32()?, r.u32()?, r.u32()?]);
                }
            }
            kind::LONG => {
                n.appearance = r.u32()?;
                n.value = r.u64()?;
            }
            _ => {}
        }
        Ok(n)
    }

    /// Write one line.
    // D2-12193
    #[allow(clippy::cast_possible_truncation)]
    fn write(&self, w: &mut Out) {
        w.u32(self.kind);
        w.u32(self.tabs);
        match self.kind {
            kind::INT | kind::EFFECT | kind::FLOAT | kind::BOOL => {
                w.u32(self.appearance);
                w.u32(self.value as u32);
            }
            kind::STRING => {
                w.u32(self.appearance);
                w.bytes(&self.text.pack());
            }
            kind::IMAGE | kind::INDENT => w.u32(self.value as u32),
            kind::COUNTDOWN => {
                w.u32(self.appearance);
                w.u64(self.time.to_bits());
            }
            kind::ICON => {
                w.u32(self.icon.len() as u32);
                for l in &self.icon {
                    for v in l {
                        w.u32(*v);
                    }
                }
            }
            kind::LONG => {
                w.u32(self.appearance);
                w.u64(self.value);
            }
            _ => {}
        }
    }

    /// Two lines are the same: kind, appearance, tabs, value, time, string and icon.
    // D2-12181
    fn same(&self, o: &Self) -> bool {
        self.kind == o.kind
            && self.appearance == o.appearance
            && self.tabs == o.tabs
            && self.value == o.value
            && self.time == o.time
            && self.text == o.text
            && self.icon == o.icon
    }
}

/// A profile: the request's packed form, the lines, the tabs noted; the tabs the request asks for
/// when its packed form leaves them out. A copy is a copy of the request, every line in order and
/// the tabs noted; dropping it lets go of its lines.
// D2-12224, D2-12222, D2-12221, D2-12223
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Profile {
    pub request: Vec<u8>,
    pub nodes: Vec<Node>,
    pub tabs: u32,
    pub asked: Option<u32>,
}

impl Profile {
    /// A new profile: a request about nothing, no lines, no tabs.
    // D2-12220, D2-12225
    #[must_use]
    pub fn new() -> Self {
        Self {
            request: vec![0; 8],
            nodes: Vec::new(),
            tabs: 0,
            asked: None,
        }
    }

    /// A new profile whose request about nothing asks for these tabs.
    // D2-12220, D2-12225
    #[must_use]
    pub fn asking(tabs: u32) -> Self {
        Self {
            asked: Some(tabs),
            ..Self::new()
        }
    }

    /// The profile as the script machine holds it.
    #[must_use]
    pub fn native(&self) -> NativeObject {
        match self.asked {
            Some(asked) => NativeObject::Profile {
                bytes: self.pack(),
                asked,
            },
            None => NativeObject::Packed {
                ty: native_type::EXAMINATION_PROFILE,
                bytes: self.pack(),
            },
        }
    }

    /// A profile as the script machine holds it (`None` for anything else).
    #[must_use]
    pub fn of(o: &NativeObject) -> Option<Self> {
        match o {
            NativeObject::Packed { ty, bytes } if *ty == native_type::EXAMINATION_PROFILE => {
                Self::unpack(bytes).ok()
            }
            NativeObject::Profile { bytes, asked } => Self::unpack(bytes).ok().map(|p| Self {
                asked: Some(*asked),
                ..p
            }),
            _ => None,
        }
    }

    /// The tabs the request asks for (a request about an object or a target carries them; any
    /// other asks for those set on it, the first tab's when new).
    #[must_use]
    pub fn asked(&self) -> u32 {
        if let Some(a) = self.asked {
            return a;
        }
        let word = |i: usize| {
            self.request
                .get(i * 4..i * 4 + 4)
                .map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        };
        match word(0) {
            0x4000_0002 => word(4),
            0x4000_000D => word(6),
            _ => FIRST_TAB,
        }
    }

    /// A profile from its packed form.
    // D2-12216
    pub fn unpack(bytes: &[u8]) -> auberean_script::wire::Result<Self> {
        let mut r = In::new(bytes);
        let start = r.pos();
        let mut probe = r.clone();
        auberean_script::native_object::step_over(&mut probe, native_type::EXAMINATION_REQUEST)?;
        let n = probe.pos() - start;
        let request = r.bytes(n)?.to_vec();
        let k = r.u32()? as usize;
        let k = r.count(k, 8)?;
        let mut nodes = Vec::with_capacity(k);
        for _ in 0..k {
            nodes.push(Node::read(&mut r)?);
        }
        let tabs = r.u32()?;
        Ok(Self {
            request,
            nodes,
            tabs,
            asked: None,
        })
    }

    /// The packed form.
    // D2-12190, D2-12214, D2-12215
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn pack(&self) -> Vec<u8> {
        let mut w = Out::new();
        w.bytes(&self.request);
        w.u32(self.nodes.len() as u32);
        for n in &self.nodes {
            n.write(&mut w);
        }
        w.u32(self.tabs);
        w.buf
    }

    /// A line added: its tabs noted, and the line kept at the end when it shows on a tab the
    /// request asks for or on the first tab.
    // D2-12198
    pub fn add(&mut self, node: Node) {
        self.tabs |= node.tabs;
        if node.tabs & (self.asked() | FIRST_TAB) != 0 {
            self.nodes.push(node);
        }
    }

    /// Two profiles are the same: the request, the tabs noted and every line.
    // D2-12194
    #[must_use]
    pub fn same(&self, o: &Self) -> bool {
        self.request == o.request
            && self.tabs == o.tabs
            && self.nodes.len() == o.nodes.len()
            && self.nodes.iter().zip(&o.nodes).all(|(a, b)| a.same(b))
    }
}

/// The natives: package [`PROFILE`], number, function.
const NATIVES: &[(u32, u32, auberean_script::native::NativeFn)] = &[
    (PROFILE, 0x01, eq),
    (PROFILE, 0x02, ne),
    (PROFILE, 0x03, set_request),
    (PROFILE, 0x05, add_bool),
    (PROFILE, 0x06, add_countdown),
    (PROFILE, 0x07, add_float),
    (PROFILE, 0x08, add_int),
    (PROFILE, 0x09, add_string),
    (PROFILE, 0x0A, add_simple_string),
    (PROFILE, 0x11, add_image),
    (PROFILE, 0x12, add_tab),
    (PROFILE, 0x13, add_break),
    (PROFILE, 0x14, add_icon),
    (PROFILE, 0x15, add_long),
    (PROFILE, 0x16, add_tabs),
    (PROFILE, 0x17, add_effect),
];

/// Bind the natives.
pub fn bind(t: &mut NativeTable) {
    crate::bind_table(t, NATIVES);
}

/// The profile a reference holds.
fn profile_at(c: &mut Call<'_>, r: u32) -> Result<Profile, Stop> {
    let native = c.native;
    let o = c.vm.heap.native(r)?;
    match o {
        NativeObject::Packed { .. } | NativeObject::Profile { .. } => {
            Profile::of(o).ok_or_else(|| Stop::Native {
                native,
                what: "an examination profile that does not read".into(),
            })
        }
        _ => Ok(Profile::new()),
    }
}

/// The receiver's profile changed by `f`; the null receiver crashes the context.
fn change(
    c: &mut Call<'_>,
    f: impl FnOnce(&mut Call<'_>, &mut Profile) -> Result<(), Stop>,
) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let mut p = profile_at(c, r)?;
    f(c, &mut p)?;
    *c.vm.heap.native_mut(r)? = p.native();
    Ok(Status::Success)
}

/// Whether the receiver and the profile below it are the same, into the slot below that.
fn compare(c: &mut Call<'_>, same: bool) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let other = c.arg(0)?;
    let a = profile_at(c, r)?;
    let b = profile_at(c, other)?;
    c.write(-3, u32::from(a.same(&b) == same), tag::MIXED)?;
    Ok(Status::Success)
}

// D2-01360
fn eq(c: &mut Call<'_>) -> Result<Status, Stop> {
    compare(c, true)
}

// D2-01361
fn ne(c: &mut Call<'_>) -> Result<Status, Stop> {
    compare(c, false)
}

/// The request below the receiver copied into the profile.
// D2-01359
fn set_request(c: &mut Call<'_>) -> Result<Status, Stop> {
    let q = c.arg(0)?;
    let request = match c.vm.heap.native(q)? {
        NativeObject::Packed { bytes, .. } => bytes.clone(),
        _ => vec![0; 8],
    };
    change(c, |_, p| {
        p.request = request;
        p.asked = None;
        Ok(())
    })
}

/// A line of a kind with a word value (`fp[-2]`), an appearance (`fp[-3]`) and its tabs
/// (`fp[-4]`).
fn add_word(c: &mut Call<'_>, k: u32) -> Result<Status, Stop> {
    let (value, appearance, tabs) = (c.arg(0)?, c.arg(1)?, c.arg(2)?);
    change(c, |_, p| {
        let mut n = Node::new(k, tabs);
        n.value = u64::from(value);
        n.appearance = appearance;
        p.add(n);
        Ok(())
    })
}

// D2-01346, D2-12199
fn add_bool(c: &mut Call<'_>) -> Result<Status, Stop> {
    add_word(c, kind::BOOL)
}

// D2-01350, D2-12201
fn add_float(c: &mut Call<'_>) -> Result<Status, Stop> {
    add_word(c, kind::FLOAT)
}

// D2-01353, D2-12202
fn add_int(c: &mut Call<'_>) -> Result<Status, Stop> {
    add_word(c, kind::INT)
}

// D2-01349, D2-12207
fn add_effect(c: &mut Call<'_>) -> Result<Status, Stop> {
    add_word(c, kind::EFFECT)
}

/// A time left: the 64-bit float at `fp[-3]`, the appearance (`fp[-4]`), its tabs (`fp[-5]`).
// D2-01348, D2-12200
fn add_countdown(c: &mut Call<'_>) -> Result<Status, Stop> {
    let time = f64::from_bits(c.long_at(-3)?);
    let (appearance, tabs) = (c.arg(2)?, c.arg(3)?);
    change(c, |_, p| {
        let mut n = Node::new(kind::COUNTDOWN, tabs);
        n.time = time;
        n.appearance = appearance;
        p.add(n);
        Ok(())
    })
}

/// A 64-bit number at `fp[-3]`, the appearance (`fp[-4]`), its tabs (`fp[-5]`).
// D2-01354, D2-12203
fn add_long(c: &mut Call<'_>) -> Result<Status, Stop> {
    let value = c.long_at(-3)?;
    let (appearance, tabs) = (c.arg(2)?, c.arg(3)?);
    change(c, |_, p| {
        let mut n = Node::new(kind::LONG, tabs);
        n.value = value;
        n.appearance = appearance;
        p.add(n);
        Ok(())
    })
}

/// A copy of the string reference at `fp[-2]`, the appearance (`fp[-3]`), its tabs (`fp[-4]`).
// D2-01356, D2-12204
fn add_string(c: &mut Call<'_>) -> Result<Status, Stop> {
    let (s, appearance, tabs) = (c.arg(0)?, c.arg(1)?, c.arg(2)?);
    let text = match c.vm.heap.native(s)? {
        NativeObject::TextInfo(t) => t.clone(),
        _ => TextInfo::default(),
    };
    change(c, |_, p| {
        let mut n = Node::new(kind::STRING, tabs);
        n.text = text;
        n.appearance = appearance;
        p.add(n);
        Ok(())
    })
}

/// A string reference made of a string id (`fp[-2]`) and a table by enumeration value
/// (`fp[-3]`), the appearance (`fp[-4]`), its tabs (`fp[-5]`); its tabs are noted, and the line
/// is made only when it would be kept.
// D2-01355, D2-12212
fn add_simple_string(c: &mut Call<'_>) -> Result<Status, Stop> {
    let (id, e, appearance, tabs) = (c.arg(0)?, c.arg(1)?, c.arg(2)?, c.arg(3)?);
    let table = table_or_none(c.host.data_id_by_enum(e, TABLE_ENUM_MAPPING));
    change(c, |_, p| {
        let mut n = Node::new(kind::STRING, tabs);
        n.text = TextInfo {
            string_id: id,
            table,
            ..TextInfo::default()
        };
        n.appearance = appearance;
        p.add(n);
        Ok(())
    })
}

/// A picture (`fp[-2]`) on its tabs (`fp[-3]`).
// D2-01352, D2-12205
fn add_image(c: &mut Call<'_>) -> Result<Status, Stop> {
    let (value, tabs) = (c.arg(0)?, c.arg(1)?);
    change(c, |_, p| {
        let mut n = Node::new(kind::IMAGE, tabs);
        n.value = u64::from(value);
        p.add(n);
        Ok(())
    })
}

/// An icon (the description at `fp[-2]`) on its tabs (`fp[-3]`).
// D2-01351, D2-12206
fn add_icon(c: &mut Call<'_>) -> Result<Status, Stop> {
    let (d, tabs) = (c.arg(0)?, c.arg(1)?);
    let icon = match c.vm.heap.native(d)? {
        NativeObject::Packed { bytes, .. } => {
            let mut r = In::new(bytes);
            let mut layers = Vec::new();
            if let Ok(k) = r.u32() {
                for _ in 0..k {
                    match (r.u32(), r.u32(), r.u32()) {
                        (Ok(a), Ok(b), Ok(e)) => layers.push([a, b, e]),
                        _ => break,
                    }
                }
            }
            layers
        }
        _ => Vec::new(),
    };
    change(c, |_, p| {
        let mut n = Node::new(kind::ICON, tabs);
        n.icon = icon;
        p.add(n);
        Ok(())
    })
}

/// One indent on its tabs (`fp[-2]`).
// D2-01357, D2-12208
fn add_tab(c: &mut Call<'_>) -> Result<Status, Stop> {
    let tabs = c.arg(0)?;
    change(c, |_, p| {
        p.add(Node::new(kind::INDENT, tabs));
        Ok(())
    })
}

/// Indents (`fp[-2]` of them) on their tabs (`fp[-3]`).
// D2-01358, D2-12209
fn add_tabs(c: &mut Call<'_>) -> Result<Status, Stop> {
    let (value, tabs) = (c.arg(0)?, c.arg(1)?);
    change(c, |_, p| {
        let mut n = Node::new(kind::INDENT, tabs);
        n.value = u64::from(value);
        p.add(n);
        Ok(())
    })
}

/// A line break on its tabs (`fp[-2]`).
// D2-01347, D2-12210
fn add_break(c: &mut Call<'_>) -> Result<Status, Stop> {
    let tabs = c.arg(0)?;
    change(c, |_, p| {
        p.add(Node::new(kind::BREAK, tabs));
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (a profile's kept lines on hand-built profiles)
    use super::*;

    #[test]
    fn a_profile_asking_for_tabs_its_request_does_not_pack_keeps_their_lines() {
        let mut plain = Profile::new();
        let mut asking = Profile::asking(0x10);
        for p in [&mut plain, &mut asking] {
            p.add(Node::new(kind::BREAK, 0x10));
            p.add(Node::new(kind::BREAK, 0x203));
        }
        assert_eq!(plain.nodes.len(), 1, "a new request asks for the first tab");
        assert_eq!(asking.nodes.len(), 2);
        assert_eq!(asking.tabs, 0x213);
        let back = Profile::of(&asking.native()).expect("reads");
        assert_eq!(back, asking, "the machine keeps the tabs asked for");
        assert_eq!(
            asking.native().packed(),
            NativeObject::Packed {
                ty: native_type::EXAMINATION_PROFILE,
                bytes: asking.pack()
            },
            "a packed graph keeps only the packed form"
        );
    }
}
