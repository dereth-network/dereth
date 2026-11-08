//! Native objects: the engine-side values scripts create and hold by reference (containers,
//! strings, string references, positions...), and the packed form each takes inside a packed
//! object graph.
//!
//! Every native type packs itself in its own format. The containers follow their names (see
//! [`crate::library::container_name`]); the rest are listed in [`Format`]. A type whose fields no
//! native reads yet is read for its extent only and kept as its bytes ([`NativeObject::Packed`]),
//! which still writes back exactly; one with no packed form at all cannot be packed.
//!
//! Hash tables are kept as the engine keeps them: an array of buckets, each a chain of entries.
//! The bucket count is one of a fixed list of primes ([`BUCKET_SIZES`]); a packed table's header
//! word holds the entry count in its low 24 bits and the bucket count's place in that list in its
//! top byte. A 32-bit key lands in bucket `key % buckets`, a 64-bit key in `key % buckets` of the
//! whole value, a hashed reference in the bucket its cached hash names. Packing walks the buckets
//! in order and each chain from its head; a new entry goes to the head of its chain; reading a
//! packed table appends each entry to its bucket's chain, so a table reads back to the order it
//! was written in.
//!
//! Multi-hashes are another kind of table ([`crate::multi_hash`]). A packed graph keeps one as
//! its entries in packed order ([`NativeObject::PackedMultiHash`]), so a graph writes back to its
//! own bytes; reading the graph into the reference table builds the engine's table from them
//! ([`NativeObject::MultiHash`]), which packs in its own order.

use crate::library::{Container, ContainerName, Elem, Library};
use crate::multi_hash::MultiHash;
use crate::value::NULL;
use crate::wire::{In, Out, Result};

/// The bucket counts a hash table may have, smallest first.
pub const BUCKET_SIZES: [u32; 23] = [
    11, 23, 47, 89, 191, 383, 761, 1531, 3067, 6143, 12281, 24571, 49139, 98299, 196597, 393209,
    786431, 1572853, 3145721, 6291449, 12582893, 25165813, 50331599,
];

/// Native type numbers this module knows a format for, beyond the containers.
pub mod native_type {
    pub const BEHAVIOR_PARAMS: u32 = 23;
    pub const DAMAGE_TEXT_BLOB: u32 = 151;
    pub const EFFECT_UI_NODE: u32 = 152;
    pub const ICON_DESC: u32 = 46;
    pub const APP_INFO_HASH: u32 = 15;
    pub const MISSILE_PARAMETERS: u32 = 70;
    pub const PACK_BUFFER: u32 = 80;
    pub const POSITION: u32 = 90;
    pub const RANDOM_SELECTION_TABLE: u32 = 96;
    pub const STRING_INFO: u32 = 106;
    pub const UI_SAVE_LOCATIONS: u32 = 111;
    pub const VECTOR: u32 = 112;
    pub const VISUAL_DESC: u32 = 113;
    pub const WP_STRING: u32 = 118;
    pub const EXAMINATION_PROFILE: u32 = 153;
    pub const ACT_INFO: u32 = 156;
    pub const ACT_INFO_LIST: u32 = 157;
    pub const KEYFRAME: u32 = 159;
    pub const LINK_REQUIREMENTS: u32 = 56;
    pub const EXAMINATION_REQUEST: u32 = 154;
    pub const GAMEPLAY_OPTIONS_PROFILE: u32 = 155;
    pub const QUEST_INFO: u32 = 160;
    pub const QUEST_INFO_LIST: u32 = 161;
    pub const RACE_SEX_INFO: u32 = 162;
    pub const SCENE_INFO: u32 = 163;
    pub const SCENE_INFO_LIST: u32 = 164;
    pub const SELECTION_INFO: u32 = 171;
    pub const SHORTCUT_INFO: u32 = 172;
    pub const SKILL_UI_NODE: u32 = 174;
    pub const CASE_INSENSITIVE_WP_STRING: u32 = 6863;
}

/// How a native type packs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// A container, by its name.
    Container(ContainerName),
    /// A wide string.
    Text,
    /// A reference to a string table entry with its variables.
    TextInfo,
    /// Three floats.
    Vector,
    /// A cell and a frame.
    Position,
    /// A byte buffer of packed values.
    Buffer,
    /// A quest record.
    Quest,
    /// A list of quest records.
    QuestList,
    /// Read for its extent and kept as bytes.
    Record(u32),
    /// No packed form.
    None,
}

/// The bucket size's place of a new hash or set: 47 buckets.
pub const NEW_TABLE_SIZE: u8 = 2;

/// The format of a native type in this library.
#[must_use]
pub fn format_of(lib: &Library, ty: u32) -> Format {
    use native_type::*;
    if let Some(c) = lib.container(ty) {
        return if c.iterator {
            Format::None
        } else {
            Format::Container(c)
        };
    }
    match ty {
        WP_STRING | CASE_INSENSITIVE_WP_STRING => Format::Text,
        STRING_INFO => Format::TextInfo,
        VECTOR => Format::Vector,
        POSITION => Format::Position,
        PACK_BUFFER => Format::Buffer,
        QUEST_INFO => Format::Quest,
        QUEST_INFO_LIST => Format::QuestList,
        BEHAVIOR_PARAMS
        | DAMAGE_TEXT_BLOB
        | EFFECT_UI_NODE
        | ICON_DESC
        | APP_INFO_HASH
        | MISSILE_PARAMETERS
        | RANDOM_SELECTION_TABLE
        | UI_SAVE_LOCATIONS
        | VISUAL_DESC
        | EXAMINATION_PROFILE
        | EXAMINATION_REQUEST
        | GAMEPLAY_OPTIONS_PROFILE
        | RACE_SEX_INFO
        | SCENE_INFO
        | SCENE_INFO_LIST
        | ACT_INFO
        | ACT_INFO_LIST
        | KEYFRAME
        | LINK_REQUIREMENTS
        | SELECTION_INFO
        | SHORTCUT_INFO
        | SKILL_UI_NODE => Format::Record(ty),
        _ => Format::None,
    }
}

/// One container element.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Item {
    Word(u32),
    Long(u64),
    Ref(u32),
    /// A reference with the hash it was filed under, and whether that hash is to be used.
    HashedRef {
        id: u32,
        hash: u32,
        use_hash: u32,
    },
}

impl Item {
    /// One element as packed. A hashed reference packs its id, its filed hash and whether that
    /// hash is to be used.
    // D2-01378
    fn read(r: &mut In<'_>, kind: Elem) -> Result<Self> {
        Ok(match kind {
            Elem::Word => Self::Word(r.u32()?),
            Elem::Long => Self::Long(r.u64()?),
            Elem::Ref => Self::Ref(r.u32()?),
            Elem::HashedRef => Self::HashedRef {
                id: r.u32()?,
                hash: r.u32()?,
                use_hash: r.u32()?,
            },
        })
    }

    fn write(&self, w: &mut Out) {
        match *self {
            Self::Word(v) | Self::Ref(v) => w.u32(v),
            Self::Long(v) => w.u64(v),
            Self::HashedRef { id, hash, use_hash } => {
                w.u32(id);
                w.u32(hash);
                w.u32(use_hash);
            }
        }
    }

    /// The bucket this item lands in as a key, among `n`.
    #[must_use]
    pub fn bucket(&self, n: usize) -> usize {
        if n == 0 {
            return 0;
        }
        #[allow(clippy::cast_possible_truncation)]
        match *self {
            Self::Word(v) | Self::Ref(v) => v as usize % n,
            Self::Long(v) => (v % n as u64) as usize,
            Self::HashedRef { hash, .. } => hash as usize % n,
        }
    }

    /// The reference id this item holds, if it holds one.
    #[must_use]
    pub fn reference(&self) -> Option<u32> {
        match *self {
            Self::Ref(id) | Self::HashedRef { id, .. } if id != NULL => Some(id),
            _ => None,
        }
    }

    /// The reference rewritten as a graph is read into the table. A hashed reference (a key)
    /// is from then on hashed through what it names rather than by its filed hash.
    // D2-04573
    pub(crate) fn remap(&mut self, f: &mut dyn FnMut(u32) -> u32) {
        match self {
            Self::Ref(id) if *id != NULL => *id = f(*id),
            Self::HashedRef { id, use_hash, .. } => {
                if *id != NULL {
                    *id = f(*id);
                }
                *use_hash = 0;
            }
            _ => {}
        }
    }
}

/// A hash table: buckets of chains, each entry a key and a value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hash {
    pub key: Elem,
    pub value: Elem,
    /// The bucket count's place in [`BUCKET_SIZES`].
    pub size_index: u8,
    pub buckets: Vec<Vec<(Item, Item)>>,
}

impl Hash {
    /// Every entry in packing order.
    pub fn entries(&self) -> impl Iterator<Item = &(Item, Item)> + '_ {
        self.buckets.iter().flatten()
    }

    /// How many entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.buckets.iter().map(Vec::len).sum()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A hash set: buckets of chains of items.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HashSet {
    pub item: Elem,
    /// The bucket count's place in [`BUCKET_SIZES`].
    pub size_index: u8,
    pub buckets: Vec<Vec<Item>>,
}

/// A packed table's header word: the count in the low 24 bits, the bucket size's place on top.
fn table_header(n: usize, size_index: u8) -> core::result::Result<u32, String> {
    let n = u32::try_from(n)
        .ok()
        .filter(|n| *n <= 0x00FF_FFFF)
        .ok_or("a table over 2^24 entries")?;
    Ok(n | u32::from(size_index) << 24)
}

/// Read a packed table's header word: the count and the bucket size's place.
fn read_table_header(r: &mut In<'_>, min_each: usize) -> Result<(usize, u8)> {
    let h = r.u32()?;
    #[allow(clippy::cast_possible_truncation)]
    let index = (h >> 24) as u8;
    if usize::from(index) >= BUCKET_SIZES.len() {
        return r.fail(format!("a table with bucket size number {index}"));
    }
    let n = r.count((h & 0x00FF_FFFF) as usize, min_each)?;
    Ok((n, index))
}

/// The empty buckets of a table of size number `index`.
#[must_use]
pub fn empty_buckets<T: Clone>(index: u8) -> Vec<Vec<T>> {
    vec![Vec::new(); BUCKET_SIZES[usize::from(index)] as usize]
}

/// A string table reference with its variables, as packed.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct TextInfo {
    pub string_id: u32,
    pub table: u32,
    /// The variable count word as stored (read as signed; a negative count means none).
    pub count: u16,
    /// Nonzero when a literal follows.
    pub has_literal: u16,
    pub literal: Option<Vec<u16>>,
    pub variables: Vec<Variable>,
}

/// One variable of a [`TextInfo`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Variable {
    pub id: u32,
    pub kind: u16,
    pub value: VariableValue,
}

/// A variable's value, by kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VariableValue {
    /// Kinds 2 and 6.
    Word(u32),
    /// Kinds 3 and 7.
    Words(u32, u32, u16),
    /// Kind 4: a 64-bit float and a word.
    Double(u64, u16),
    /// Kind 5.
    DoubleWords(u64, u16, u32, u16),
    /// Kind 9: another string reference.
    Nested(Box<TextInfo>),
    /// Kinds 10 and 12.
    Long(u64),
    /// Kinds 11 and 13.
    LongWords(u64, u32, u16),
}

/// The bucket count of a string reference's variable table.
pub const VARIABLE_BUCKETS: u32 = 47;

impl TextInfo {
    /// Set a variable, replacing one with the same id: it goes to the head of its bucket's chain
    /// (the table has [`VARIABLE_BUCKETS`] buckets, a variable's bucket its id modulo that).
    // D2-14151
    pub fn set_variable(&mut self, v: Variable) {
        self.variables.retain(|x| x.id != v.id);
        let b = v.id % VARIABLE_BUCKETS;
        let at = self
            .variables
            .iter()
            .position(|x| x.id % VARIABLE_BUCKETS >= b)
            .unwrap_or(self.variables.len());
        self.variables.insert(at, v);
        #[allow(clippy::cast_possible_truncation)]
        let n = self.variables.len() as u16;
        self.count = n;
    }

    /// The string reference's packed form, as it sits inside a record that carries one.
    #[must_use]
    pub fn pack(&self) -> Vec<u8> {
        let mut w = Out::new();
        self.write(&mut w);
        w.buf
    }

    /// Read one string reference in its packed form (for a reader that knows no library: the
    /// server's reading of a call's arguments).
    pub fn unpack(r: &mut In<'_>) -> Result<Self> {
        Self::read(r, 0)
    }

    // D2-14174
    pub(crate) fn read(r: &mut In<'_>, depth: u32) -> Result<Self> {
        if depth > 8 {
            return r.fail("string references nested too deep");
        }
        let string_id = r.u32()?;
        let table = r.u32()?;
        let count = r.u16()?;
        let has_literal = r.u16()?;
        let literal = if has_literal != 0 {
            Some(r.wide()?)
        } else {
            None
        };
        #[allow(clippy::cast_possible_wrap)]
        let n = (count as i16).max(0) as usize;
        let n = r.count(n, 8)?;
        let mut variables = Vec::with_capacity(n);
        for _ in 0..n {
            let id = r.u32()?;
            let kind = r.u16()?;
            r.align()?;
            let value = match kind {
                2 | 6 => VariableValue::Word(r.u32()?),
                3 | 7 => VariableValue::Words(r.u32()?, r.u32()?, r.u16()?),
                4 => VariableValue::Double(r.u64()?, r.u16()?),
                5 => VariableValue::DoubleWords(r.u64()?, r.u16()?, r.u32()?, r.u16()?),
                9 => VariableValue::Nested(Box::new(Self::read(r, depth + 1)?)),
                10 | 12 => VariableValue::Long(r.u64()?),
                11 | 13 => VariableValue::LongWords(r.u64()?, r.u32()?, r.u16()?),
                k => return r.fail(format!("string reference variable of kind {k}")),
            };
            r.align()?;
            variables.push(Variable { id, kind, value });
        }
        r.align()?;
        Ok(Self {
            string_id,
            table,
            count,
            has_literal,
            literal,
            variables,
        })
    }

    // D2-14142
    pub(crate) fn write(&self, w: &mut Out) {
        w.u32(self.string_id);
        w.u32(self.table);
        w.u16(self.count);
        w.u16(self.has_literal);
        // The literal follows only when the flag says so: one kept from earlier with the flag
        // since cleared is not written.
        if self.has_literal != 0 {
            w.wide(self.literal.as_deref().unwrap_or_default());
        }
        for v in &self.variables {
            w.u32(v.id);
            w.u16(v.kind);
            w.align();
            match &v.value {
                VariableValue::Word(a) => w.u32(*a),
                VariableValue::Words(a, b, c) => {
                    w.u32(*a);
                    w.u32(*b);
                    w.u16(*c);
                }
                VariableValue::Double(a, b) => {
                    w.u64(*a);
                    w.u16(*b);
                }
                VariableValue::DoubleWords(a, b, c, d) => {
                    w.u64(*a);
                    w.u16(*b);
                    w.u32(*c);
                    w.u16(*d);
                }
                VariableValue::Nested(t) => t.write(w),
                VariableValue::Long(a) => w.u64(*a),
                VariableValue::LongWords(a, b, c) => {
                    w.u64(*a);
                    w.u32(*b);
                    w.u16(*c);
                }
            }
            w.align();
        }
        w.align();
    }
}

/// A quest as the client keeps it for the journal: what the server sends of it, and one value the
/// client keeps that the server never sends (whether the quest may be taken no more times).
///
/// Times are game times in seconds, kept as the bits of the 64-bit floats they are. A copy is a
/// copy of every field; dropping one lets go of its texts.
// D2-12254, D2-12257, D2-12242, D2-12252
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuestInfo {
    pub id: u32,
    pub name: TextInfo,
    pub description: TextInfo,
    /// The journal's icon (a record id).
    pub icon: u32,
    /// How hard the quest is (-999: any level).
    pub challenge: i32,
    /// Under way `0x1000_0000`, failed `0x2000_0000`, completed `0x3000_0000`, cancelled
    /// `0x8000_0000`.
    pub status: u32,
    /// The phase it is at, while under way.
    pub phase: u32,
    /// The journal's entry for the phase it is at.
    pub journal: TextInfo,
    /// When it was given.
    pub bestowed: u64,
    /// When it was last done.
    pub done: u64,
    /// Whether it runs out.
    pub expires: u32,
    /// Whether it may be taken no more times: never sent, so only a script's own record sets it.
    pub maxed_out: u32,
    /// While under way, the time left before it runs out.
    pub seconds_remaining: u64,
    /// Once done, the time before it may be taken again.
    pub seconds_until_retry: u64,
    /// Whether an update plays the quest's effect on the player.
    pub plays_fx: u32,
    /// Whether the player may cancel it.
    pub cancellable: u32,
}

/// Minus one, as a 64-bit float's bits: a record's times before it is given.
const MINUS_ONE: u64 = 0xBFF0_0000_0000_0000;

/// A string reference read over one already holding something, as the engine reads one: the
/// string, the table and the literal flag are replaced, the literal only when one follows, and
/// each variable read is added to those already there (one with the same id replaced).
// D2-14174, D2-14151
fn read_text_over(into: &mut TextInfo, read: &TextInfo) {
    into.string_id = read.string_id;
    into.table = read.table;
    into.has_literal = read.has_literal;
    if read.has_literal != 0 {
        into.literal.clone_from(&read.literal);
    }
    for v in &read.variables {
        into.set_variable(v.clone());
    }
    #[allow(clippy::cast_possible_truncation)]
    let n = into.variables.len() as u16;
    into.count = n;
}

impl Default for QuestInfo {
    /// A new record, as the engine makes one: no quest, never given or done, an update plays the
    /// quest's effect.
    // D2-12251, D2-12253, D2-12244
    fn default() -> Self {
        Self {
            id: 0,
            name: TextInfo::default(),
            description: TextInfo::default(),
            icon: 0,
            challenge: 0,
            status: 0,
            phase: 0,
            journal: TextInfo::default(),
            bestowed: MINUS_ONE,
            done: MINUS_ONE,
            expires: 0,
            maxed_out: 0,
            seconds_remaining: 0,
            seconds_until_retry: 0,
            plays_fx: 1,
            cancellable: 0,
        }
    }
}

impl QuestInfo {
    /// The status of a quest under way.
    pub const UNDERWAY: u32 = 0x1000_0000;
    /// The status of a quest failed.
    pub const FAILED: u32 = 0x2000_0000;
    /// The status of a quest done.
    pub const COMPLETED: u32 = 0x3000_0000;
    /// The status of a quest given up.
    pub const CANCELLED: u32 = 0x8000_0000;

    /// Read one record in its packed form (its id, name, description, icon, status, phase,
    /// journal entry, challenge, the given and done times, whether it runs out, the time left,
    /// the time before a retry, the effect, whether it may be cancelled).
    // D2-12256
    pub fn unpack(r: &mut In<'_>) -> Result<Self> {
        let mut q = Self::default();
        q.read_over(r)?;
        Ok(q)
    }

    /// Read a record's packed form over this one: every packed field is replaced, the rest (the
    /// maxed-out flag) stay as they were.
    // D2-12256
    fn read_over(&mut self, r: &mut In<'_>) -> Result<()> {
        self.id = r.u32()?;
        self.name = TextInfo::read(r, 0)?;
        self.description = TextInfo::read(r, 0)?;
        self.icon = r.u32()?;
        self.status = r.u32()?;
        self.phase = r.u32()?;
        self.journal = TextInfo::read(r, 0)?;
        self.challenge = r.u32()?.cast_signed();
        self.bestowed = r.u64()?;
        self.done = r.u64()?;
        self.expires = r.u32()?;
        self.seconds_remaining = r.u64()?;
        self.seconds_until_retry = r.u64()?;
        self.plays_fx = r.u32()?;
        self.cancellable = r.u32()?;
        Ok(())
    }

    /// Write this record's packed form.
    // D2-12256
    pub fn write(&self, w: &mut Out) {
        w.u32(self.id);
        self.name.write(w);
        self.description.write(w);
        w.u32(self.icon);
        w.u32(self.status);
        w.u32(self.phase);
        self.journal.write(w);
        w.u32(self.challenge.cast_unsigned());
        w.u64(self.bestowed);
        w.u64(self.done);
        w.u32(self.expires);
        w.u64(self.seconds_remaining);
        w.u64(self.seconds_until_retry);
        w.u32(self.plays_fx);
        w.u32(self.cancellable);
    }

    /// A packed list's records as the engine reads them into a list: each is read over one and
    /// the same record and a copy of that record is added, so what a record's reading does not
    /// replace stays from the records before it. Its maxed-out flag stays the new record's (none);
    /// a string reference's variables are added to those the earlier records left (one with the
    /// same id replaced), and its literal, when it has none, stays the earlier one's, unused.
    // D2-12270, D2-14174, D2-01622
    #[must_use]
    pub fn read_list_over_one(packed: &[QuestInfo]) -> Vec<QuestInfo> {
        let mut one = Self::default();
        packed
            .iter()
            .map(|q| {
                one.id = q.id;
                read_text_over(&mut one.name, &q.name);
                read_text_over(&mut one.description, &q.description);
                one.icon = q.icon;
                one.status = q.status;
                one.phase = q.phase;
                read_text_over(&mut one.journal, &q.journal);
                one.challenge = q.challenge;
                one.bestowed = q.bestowed;
                one.done = q.done;
                one.expires = q.expires;
                one.seconds_remaining = q.seconds_remaining;
                one.seconds_until_retry = q.seconds_until_retry;
                one.plays_fx = q.plays_fx;
                one.cancellable = q.cancellable;
                one.clone()
            })
            .collect()
    }

    /// This record's packed form.
    #[must_use]
    pub fn pack(&self) -> Vec<u8> {
        let mut w = Out::new();
        self.write(&mut w);
        w.buf
    }

    /// When it was given, in game seconds.
    #[must_use]
    pub fn bestowed_at(&self) -> f64 {
        f64::from_bits(self.bestowed)
    }

    /// When it was last done, in game seconds.
    #[must_use]
    pub fn done_at(&self) -> f64 {
        f64::from_bits(self.done)
    }

    /// Whether it is under way.
    #[must_use]
    pub fn underway(&self) -> bool {
        self.status == Self::UNDERWAY
    }

    /// Whether it may be taken no more times: only once it is no longer under way.
    // D2-12246
    #[must_use]
    pub fn maxed_out(&self) -> bool {
        self.status != Self::UNDERWAY && self.maxed_out != 0
    }

    /// While under way, the seconds left before it runs out; else the seconds before it may be
    /// taken again: cut toward zero to a 64-bit integer, of which the low word is kept.
    // D2-12247
    #[must_use]
    pub fn seconds_remaining(&self) -> i32 {
        let t = if self.underway() {
            self.seconds_remaining
        } else {
            self.seconds_until_retry
        };
        #[allow(clippy::cast_possible_truncation)]
        let low = auberean_text::crt::ftol(f64::from_bits(t)) as u32;
        low.cast_signed()
    }
}

/// One effect on an object as the effects panel lists it: its number, whether it is shown enabled,
/// toggled or pulsed (bits 0x1, 0x2, 0x4), its name and description, its icon, and how long it
/// lasts and when it was cast (64-bit floats' bits; -1 for none).
///
/// A new one is all zero and empty, with both times -1. Its packed form is the number, the bits,
/// the name, the description, the icon, the duration and the cast time. A copy is a copy of every
/// field.
// D2-12173, D2-12172
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectNode {
    pub eid: u32,
    pub flags: u32,
    pub name: Vec<u16>,
    pub description: Vec<u16>,
    pub icon: u32,
    pub duration: u64,
    pub time_cast: u64,
}

impl Default for EffectNode {
    /// A new one.
    // D2-12170, D2-12171
    fn default() -> Self {
        Self {
            eid: 0,
            flags: 0,
            name: Vec::new(),
            description: Vec::new(),
            icon: 0,
            duration: (-1.0f64).to_bits(),
            time_cast: (-1.0f64).to_bits(),
        }
    }
}

impl EffectNode {
    /// Shown enabled.
    pub const ENABLED: u32 = 0x1;
    /// A toggled effect.
    pub const TOGGLED: u32 = 0x2;
    /// A pulsed effect.
    pub const PULSED: u32 = 0x4;

    /// Whether it pulses.
    // D2-12168
    #[must_use]
    pub const fn pulsed(&self) -> bool {
        self.flags & Self::PULSED != 0
    }

    /// Read a packed one.
    // D2-12174
    pub fn read(r: &mut In<'_>) -> Result<Self> {
        Ok(Self {
            eid: r.u32()?,
            flags: r.u32()?,
            name: r.wide()?,
            description: r.wide()?,
            icon: r.u32()?,
            duration: r.u64()?,
            time_cast: r.u64()?,
        })
    }

    /// The packed form.
    // D2-12174
    pub fn write(&self, w: &mut Out) {
        w.u32(self.eid);
        w.u32(self.flags);
        w.wide(&self.name);
        w.wide(&self.description);
        w.u32(self.icon);
        w.u64(self.duration);
        w.u64(self.time_cast);
    }

    /// The packed form, as bytes.
    #[must_use]
    pub fn pack(&self) -> Vec<u8> {
        let mut w = Out::new();
        self.write(&mut w);
        w.buf
    }
}

/// The floating text a hit shows over its target: the string, its color (four floats' bits: red,
/// green, blue, alpha), how long it shows (a float's bits) and its scale (a float's bits).
///
/// A new one (or one cleared) is an empty string, a color all zero, a duration of -1 and a scale
/// of 0. Its packed form is the string, the color's four floats, the duration and the scale. A
/// copy is the whole of it.
// D2-12236, D2-12239, D2-12240
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DamageText {
    pub text: Vec<u16>,
    pub color: [u32; 4],
    pub duration: u32,
    pub scale: u32,
}

impl Default for DamageText {
    /// A new one: cleared.
    // D2-12238, D2-12237
    fn default() -> Self {
        Self {
            text: Vec::new(),
            color: [0; 4],
            duration: (-1.0f32).to_bits(),
            scale: 0,
        }
    }
}

impl DamageText {
    /// Read a packed one.
    // D2-12241
    pub fn read(r: &mut In<'_>) -> Result<Self> {
        Ok(Self {
            text: r.wide()?,
            color: [r.u32()?, r.u32()?, r.u32()?, r.u32()?],
            duration: r.u32()?,
            scale: r.u32()?,
        })
    }

    /// The packed form.
    // D2-12241
    pub fn write(&self, w: &mut Out) {
        w.wide(&self.text);
        self.color.iter().for_each(|c| w.u32(*c));
        w.u32(self.duration);
        w.u32(self.scale);
    }

    /// The packed form, as bytes.
    #[must_use]
    pub fn pack(&self) -> Vec<u8> {
        let mut w = Out::new();
        self.write(&mut w);
        w.buf
    }
}

/// How a behavior is to be played on an object (an animation by id, with the camera, the
/// effects and the motion it brings), as scripts and the server fill it in.
///
/// Its packed form leads with a word whose bits say which fields follow; a field is packed when it
/// differs from a new one's ([`Default`]): a time scale of 1, one hold cycle, an event sent when it
/// is done, everything else zero or off.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BehaviorParams {
    pub behavior: u32,
    /// A 32-bit float's bits.
    pub time_scale: u32,
    pub mode: u32,
    pub hold_cycles: u32,
    pub fx_script: u32,
    /// The effect started with the behavior and stopped after it.
    pub fx: u32,
    pub target: u64,
    pub camera_target: u64,
    pub camera_behavior: u32,
    pub visual_to_clone: u32,
    pub cloned_appearance: u32,
    /// The cloned appearance keys: the packed table's header word and its pairs.
    pub cloned_keys: (u32, Vec<(u32, u32)>),
    /// Three 32-bit floats' bits.
    pub impulse: [u32; 3],
    pub early_callback: u32,
    pub move_to_cancels: u32,
    pub camera_parented: u32,
    pub camera_targeted: u32,
    pub camera_hold: u32,
    pub camera_restore: u32,
    pub fade_children: u32,
    pub propagate: u32,
    pub lock_actions: u32,
    pub send_event: u32,
    pub weenie_emote: u32,
    pub leave_idle_alone: u32,
    pub destroy_on_completion: u32,
    pub context: u32,
}

impl Default for BehaviorParams {
    /// A new one, as the engine's defaults have it.
    // D2-14912
    fn default() -> Self {
        Self {
            behavior: 0,
            time_scale: 1f32.to_bits(),
            mode: 0,
            hold_cycles: 1,
            fx_script: 0,
            fx: 0,
            target: 0,
            camera_target: 0,
            camera_behavior: 0,
            visual_to_clone: 0,
            cloned_appearance: 0,
            cloned_keys: (0, Vec::new()),
            impulse: [0; 3],
            early_callback: 0,
            move_to_cancels: 0,
            camera_parented: 0,
            camera_targeted: 0,
            camera_hold: 0,
            camera_restore: 0,
            fade_children: 0,
            propagate: 0,
            lock_actions: 0,
            send_event: 1,
            weenie_emote: 0,
            leave_idle_alone: 0,
            destroy_on_completion: 0,
            context: 0,
        }
    }
}

impl BehaviorParams {
    /// The bits of the fields that differ from a new one's: the time scale compared as a float
    /// (a not-a-number always differs), the impulse within 0.0002 on each axis, the camera's
    /// target alone standing for its behavior too, the visual to clone for the cloned appearance.
    // D2-14910
    #[must_use]
    pub fn bits(&self) -> u32 {
        let d = Self::default();
        let f = f32::from_bits;
        #[allow(clippy::float_cmp)]
        let scale_differs = f(self.time_scale) != f(d.time_scale);
        let far = |a: u32, b: u32| 0.0002 <= (f(a) - f(b)).abs();
        let impulse_differs = (0..3).any(|i| far(self.impulse[i], d.impulse[i]));
        [
            (0x1, self.behavior != d.behavior),
            (0x2, self.mode != d.mode),
            (0x4, scale_differs),
            (0x8, self.early_callback != d.early_callback),
            (0x10, self.hold_cycles != d.hold_cycles),
            (0x20, self.move_to_cancels != d.move_to_cancels),
            (0x40, self.camera_targeted != d.camera_targeted),
            (0x80, self.camera_parented != d.camera_parented),
            (0x4_0000, self.camera_hold != d.camera_hold),
            (0x8_0000, self.camera_restore != d.camera_restore),
            (0x20_0000, self.camera_target != d.camera_target),
            (0x100, self.fade_children != d.fade_children),
            (0x400, self.propagate != d.propagate),
            (0x800, self.lock_actions != d.lock_actions),
            (0x1000, self.fx_script != d.fx_script),
            (0x10_0000, self.fx != d.fx),
            (0x200, self.send_event != d.send_event),
            (0x2000, self.target != d.target),
            (0x1_0000, self.visual_to_clone != d.visual_to_clone),
            (0x4000, impulse_differs),
            (0x2_0000, self.leave_idle_alone != d.leave_idle_alone),
            (
                0x8000,
                self.destroy_on_completion != d.destroy_on_completion,
            ),
            (0x80_0000, self.weenie_emote != d.weenie_emote),
            (0x40_0000, self.context != d.context),
        ]
        .iter()
        .filter(|(_, differs)| *differs)
        .fold(0, |b, (bit, _)| b | bit)
    }

    /// The flag fields after the first ones, in their packed order, with their bits.
    fn flags_mut(&mut self) -> [(u32, &mut u32); 14] {
        [
            (0x8, &mut self.early_callback),
            (0x20, &mut self.move_to_cancels),
            (0x80, &mut self.camera_parented),
            (0x40, &mut self.camera_targeted),
            (0x4_0000, &mut self.camera_hold),
            (0x8_0000, &mut self.camera_restore),
            (0x100, &mut self.fade_children),
            (0x400, &mut self.propagate),
            (0x800, &mut self.lock_actions),
            (0x200, &mut self.send_event),
            (0x2_0000, &mut self.leave_idle_alone),
            (0x8000, &mut self.destroy_on_completion),
            (0x80_0000, &mut self.weenie_emote),
            (0x40_0000, &mut self.context),
        ]
    }

    /// Read a packed form over this one: the fields its bits name are replaced.
    // D2-14911
    pub fn read_over(&mut self, r: &mut In<'_>) -> Result<()> {
        let b = r.u32()?;
        let has = |bit: u32| b & bit != 0;
        for (bit, field) in [
            (0x1, &mut self.behavior),
            (0x4, &mut self.time_scale),
            (0x2, &mut self.mode),
            (0x10, &mut self.hold_cycles),
            (0x1000, &mut self.fx_script),
            (0x10_0000, &mut self.fx),
        ] {
            if has(bit) {
                *field = r.u32()?;
            }
        }
        if has(0x2000) {
            self.target = r.u64()?;
        }
        if has(0x20_0000) {
            self.camera_target = r.u64()?;
            self.camera_behavior = r.u32()?;
        }
        if has(0x1_0000) {
            self.visual_to_clone = r.u32()?;
            self.cloned_appearance = r.u32()?;
            let header = r.u32()?;
            let n = r.count((header & 0x00FF_FFFF) as usize, 8)?;
            let mut pairs = Vec::with_capacity(n);
            for _ in 0..n {
                pairs.push((r.u32()?, r.u32()?));
            }
            self.cloned_keys = (header, pairs);
        }
        if has(0x4000) {
            for v in &mut self.impulse {
                *v = r.u32()?;
            }
        }
        for (bit, field) in self.flags_mut() {
            if has(bit) {
                *field = r.u32()?;
            }
        }
        Ok(())
    }

    /// Write the packed form: the bits of the fields that differ from a new one's, then those
    /// fields.
    // D2-14911, D2-14910
    pub fn write(&self, w: &mut Out) {
        let b = self.bits();
        let has = |bit: u32| b & bit != 0;
        w.u32(b);
        for (bit, v) in [
            (0x1, self.behavior),
            (0x4, self.time_scale),
            (0x2, self.mode),
            (0x10, self.hold_cycles),
            (0x1000, self.fx_script),
            (0x10_0000, self.fx),
        ] {
            if has(bit) {
                w.u32(v);
            }
        }
        if has(0x2000) {
            w.u64(self.target);
        }
        if has(0x20_0000) {
            w.u64(self.camera_target);
            w.u32(self.camera_behavior);
        }
        if has(0x1_0000) {
            w.u32(self.visual_to_clone);
            w.u32(self.cloned_appearance);
            w.u32(self.cloned_keys.0);
            for (k, v) in &self.cloned_keys.1 {
                w.u32(*k);
                w.u32(*v);
            }
        }
        if has(0x4000) {
            self.impulse.iter().for_each(|v| w.u32(*v));
        }
        let mut copy = self.clone();
        for (bit, v) in copy.flags_mut() {
            if has(bit) {
                w.u32(*v);
            }
        }
    }

    /// The packed form.
    #[must_use]
    pub fn pack(&self) -> Vec<u8> {
        let mut w = Out::new();
        self.write(&mut w);
        w.buf
    }
}

/// The value of a native object another crate keeps whole: an engine object (a missile's flight,
/// what a body passes through) that the crate's natives make and fill and the engine reads back.
/// The machine only holds it: it copies it with the object, compares it and packs it.
pub trait EngineValue: core::any::Any + core::fmt::Debug + Send + Sync {
    /// The native type it is.
    fn native_type(&self) -> u32;
    /// A copy of it.
    fn clone_value(&self) -> Box<dyn EngineValue>;
    /// Whether it equals another engine value.
    fn same(&self, other: &dyn EngineValue) -> bool;
    /// Its packed form; `None` for a value with none.
    fn pack(&self) -> Option<Vec<u8>>;
    fn as_any(&self) -> &dyn core::any::Any;
    fn as_any_mut(&mut self) -> &mut dyn core::any::Any;
}

/// An engine object's value, held for the crate that knows it ([`EngineValue`]).
#[derive(Debug)]
pub struct Engine(pub Box<dyn EngineValue>);

impl Engine {
    #[must_use]
    pub fn new(value: impl EngineValue) -> Self {
        Self(Box::new(value))
    }

    /// The native type it is.
    #[must_use]
    pub fn ty(&self) -> u32 {
        self.0.native_type()
    }

    /// The value, when it is a `T`.
    #[must_use]
    pub fn get<T: 'static>(&self) -> Option<&T> {
        self.0.as_any().downcast_ref()
    }

    /// The value, when it is a `T`.
    pub fn get_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.0.as_any_mut().downcast_mut()
    }
}

impl Clone for Engine {
    fn clone(&self) -> Self {
        Self(self.0.clone_value())
    }
}

impl PartialEq for Engine {
    fn eq(&self, other: &Self) -> bool {
        self.0.same(&*other.0)
    }
}

impl Eq for Engine {}

/// A native object's value.
#[derive(Clone, Debug, PartialEq, Eq)]
// D2-00040, D2-01040, D2-01041, D2-01042, D2-01289, D2-04437, D2-04459, D2-04460, D2-04495, D2-01037
// D2-04544, D2-04545, D2-04546, D2-04547, D2-04596, D2-04605, D2-04606, D2-00785, D2-00786, D2-00789
// D2-04629, D2-00791, D2-16448, D2-09049, D2-09050
pub enum NativeObject {
    Hash(Hash),
    /// A multi-hash as the engine holds it.
    MultiHash(MultiHash),
    /// A multi-hash as a packed graph holds it: its entries in packed order. Reading the graph
    /// into the reference table makes it a [`NativeObject::MultiHash`].
    PackedMultiHash {
        key: Elem,
        value: Elem,
        entries: Vec<(Item, Item)>,
    },
    /// A multi-map as the engine holds it: entries in key order.
    MultiMap(crate::multi_map::MultiMap),
    HashSet(HashSet),
    /// A list or an array.
    Sequence {
        item: Elem,
        items: Vec<Item>,
    },
    /// A container iterator: where it stands in its container.
    Iterator {
        container: u32,
        position: u32,
    },
    /// A wide string.
    Text(Vec<u16>),
    TextInfo(TextInfo),
    /// Three floats, as their bits.
    Vector([u32; 3]),
    /// A cell id, an origin and a rotation (scalar first), as bits.
    Position {
        cell: u32,
        frame: [u32; 7],
    },
    /// Packed values (tag and value pairs).
    // D2-16734
    Buffer(Vec<u8>),
    /// A quest record.
    Quest(Box<QuestInfo>),
    /// A list of quest records, in list order: a copy holds a copy of each, in order.
    // D2-12280, D2-12277, D2-12276, D2-12279, D2-12275
    QuestList(Vec<QuestInfo>),
    /// How a behavior is to be played, as the engine holds it (a packed graph keeps its bytes:
    /// [`NativeObject::Packed`]).
    Behavior(Box<BehaviorParams>),
    /// A shortcut, as the engine holds it (a packed graph keeps its bytes).
    Shortcut(crate::options::Shortcut),
    /// The player's options, as the engine holds them (a packed graph keeps its bytes).
    Options(Box<crate::options::GameplayOptions>),
    /// One effect as the effects panel lists it (a packed graph keeps its bytes).
    EffectNode(Box<EffectNode>),
    /// A hit's floating text (a packed graph keeps its bytes).
    DamageText(Box<DamageText>),
    /// A step of a scene of the story (a packed graph keeps its bytes).
    Keyframe(Box<crate::story::Keyframe>),
    /// A scene of an act (a packed graph keeps its bytes).
    Scene(Box<crate::story::SceneInfo>),
    /// A list of scenes, kept by value (a packed graph keeps its bytes).
    Scenes(Vec<crate::story::SceneInfo>),
    /// An act of the story (a packed graph keeps its bytes).
    Act(Box<crate::story::ActInfo>),
    /// A list of acts, kept by value (a packed graph keeps its bytes).
    Acts(Vec<crate::story::ActInfo>),
    /// An examination profile whose request asks for tabs its packed form leaves out (a request
    /// about nothing with tabs set on it): the packed form, and the tabs asked for (a packed graph
    /// keeps the bytes).
    Profile {
        bytes: Vec<u8>,
        asked: u32,
    },
    /// A native read for its extent only: its type and bytes.
    Packed {
        ty: u32,
        bytes: Vec<u8>,
    },
    /// A native with no packed form.
    Opaque {
        ty: u32,
    },
    /// An object scripts fill for the interface (no packed form): the values its setters were
    /// given, by field.
    Adaptor {
        ty: u32,
        fields: std::collections::BTreeMap<String, Field>,
    },
    /// An engine object another crate keeps whole (see [`Engine`]).
    Engine(Engine),
}

/// A value an adaptor's setter was given.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Field {
    Word(u32),
    Long(u64),
    /// A copy of another native object's value (a string, a string reference, an adaptor).
    Value(Box<NativeObject>),
    /// Several values one setter was given, or the entries of a list.
    Many(Vec<Field>),
}

impl NativeObject {
    /// A new, empty native object of a type, as the native factory makes one: the game's types
    /// and the engine's alike.
    // D2-00784, D2-04649
    #[must_use]
    pub fn new(lib: &Library, ty: u32) -> Self {
        match format_of(lib, ty) {
            // The factory's hashes and sets ask for 32 buckets, which the bucket list makes 47.
            // D2-04534, D2-04539
            Format::Container(c) => match c.container {
                // D2-01061, D2-01065, D2-04592, D2-01300, D2-04561, D2-04569, D2-04576
                Container::Hash(key, value) => Self::Hash(Hash {
                    key,
                    value,
                    size_index: NEW_TABLE_SIZE,
                    buckets: empty_buckets(NEW_TABLE_SIZE),
                }),
                // A new multi-hash asks for 100 buckets ([`MultiHash::new`]).
                // D2-04645, D2-04646, D2-04647, D2-04643, D2-04644
                Container::MultiHash(key, value) => Self::MultiHash(MultiHash::new(key, value)),
                Container::MultiMap(key, value) => {
                    Self::MultiMap(crate::multi_map::MultiMap::new(key, value))
                }
                // D2-04588
                Container::HashSet(item) => Self::HashSet(HashSet {
                    item,
                    size_index: NEW_TABLE_SIZE,
                    buckets: empty_buckets(NEW_TABLE_SIZE),
                }),
                // D2-01047, D2-01043, D2-01293
                Container::List(item) | Container::Array(item) => Self::Sequence {
                    item,
                    items: Vec::new(),
                },
            },
            Format::Text => Self::Text(Vec::new()),
            Format::TextInfo => Self::TextInfo(TextInfo::default()),
            Format::Vector => Self::Vector([0; 3]),
            Format::Position => Self::Position {
                cell: 0,
                frame: [0, 0, 0, 0x3F80_0000, 0, 0, 0],
            },
            Format::Buffer => Self::Buffer(Vec::new()),
            Format::Quest => Self::Quest(Box::default()),
            // D2-12274, D2-12281
            Format::QuestList => Self::QuestList(Vec::new()),
            // A new examination request asks about nothing: its kind and its admin word, both
            // zero, are all it packs.
            // D2-12229, D2-12231
            Format::Record(native_type::EXAMINATION_REQUEST) => Self::Packed {
                ty,
                bytes: vec![0; 8],
            },
            // A new examination profile answers a request about nothing, with no lines and no
            // tabs noted.
            // D2-12220, D2-12190
            Format::Record(native_type::EXAMINATION_PROFILE) => Self::Packed {
                ty,
                bytes: vec![0; 16],
            },
            // A new icon description has no layers: its count, zero, is all it packs.
            // D2-13884, D2-13895, D2-13896
            Format::Record(native_type::ICON_DESC) => Self::Packed {
                ty,
                bytes: vec![0; 4],
            },
            // A new one has the defaults.
            Format::Record(native_type::BEHAVIOR_PARAMS) => Self::Behavior(Box::default()),
            // D2-12177, D2-12175
            Format::Record(native_type::SHORTCUT_INFO) => Self::Shortcut(Default::default()),
            Format::Record(native_type::GAMEPLAY_OPTIONS_PROFILE) => Self::Options(Box::default()),
            Format::Record(native_type::EFFECT_UI_NODE) => Self::EffectNode(Box::default()),
            Format::Record(native_type::DAMAGE_TEXT_BLOB) => Self::DamageText(Box::default()),
            // D2-12338
            Format::Record(native_type::KEYFRAME) => Self::Keyframe(Box::default()),
            // A link's requirements: whether it is an input, its type, how many links, whether
            // required, how many are required; all zero.
            // D2-13049
            Format::Record(native_type::LINK_REQUIREMENTS) => Self::Packed {
                ty,
                bytes: vec![0; 20],
            },
            // D2-12298
            Format::Record(native_type::SCENE_INFO) => Self::Scene(Box::default()),
            // D2-00271
            Format::Record(native_type::SCENE_INFO_LIST) => Self::Scenes(Vec::new()),
            // D2-12310
            Format::Record(native_type::ACT_INFO) => Self::Act(Box::default()),
            // D2-00773
            Format::Record(native_type::ACT_INFO_LIST) => Self::Acts(Vec::new()),
            // D2-04285, D2-04286, D2-04287, D2-04288, D2-04289, D2-04325, D2-04326, D2-04327, D2-04328, D2-04329
            // D2-04330, D2-04331, D2-04332
            Format::Record(_) | Format::None => {
                if lib.container(ty).is_some_and(|c| c.iterator) {
                    Self::Iterator {
                        container: NULL,
                        position: 0,
                    }
                } else {
                    Self::Opaque { ty }
                }
            }
        }
    }

    /// Read one native object of type `ty` in its packed form. An iterator packs its container
    /// and how many steps from the container's first entry it stands (see
    /// [`NativeObject::iterator_offset`]); it stands there again once its container is read into
    /// the table ([`NativeObject::iterator_place`]).
    // D2-16451, D2-04650, D2-00787
    // D2-04248, D2-04249, D2-04250, D2-04394, D2-04396, D2-04404, D2-04415
    pub fn read(r: &mut In<'_>, lib: &Library, ty: u32) -> Result<Self> {
        if lib.container(ty).is_some_and(|c| c.iterator) {
            return Ok(Self::Iterator {
                container: r.u32()?,
                position: r.u32()?,
            });
        }
        Ok(match format_of(lib, ty) {
            Format::Container(c) => read_container(r, c.container)?,
            Format::Text => Self::Text(r.wide()?),
            Format::TextInfo => Self::TextInfo(TextInfo::read(r, 0)?),
            Format::Vector => Self::Vector([r.u32()?, r.u32()?, r.u32()?]),
            Format::Position => {
                let cell = r.u32()?;
                let mut frame = [0; 7];
                for f in &mut frame {
                    *f = r.u32()?;
                }
                Self::Position { cell, frame }
            }
            // D2-16745
            Format::Buffer => {
                let n = r.u32()? as usize;
                Self::Buffer(r.bytes(n)?.to_vec())
            }
            Format::Quest => Self::Quest(Box::new(QuestInfo::unpack(r)?)),
            Format::QuestList => Self::QuestList(read_quest_list(r)?),
            Format::Record(ty) => {
                let start = r.pos();
                let mut probe = r.clone();
                step_over(&mut probe, ty)?;
                let n = probe.pos() - start;
                Self::Packed {
                    ty,
                    bytes: r.bytes(n)?.to_vec(),
                }
            }
            Format::None => {
                return r.fail(format!(
                    "native type {ty:#x} ({}) has no packed form",
                    lib.package_name(ty)
                ))
            }
        })
    }

    /// Write this native object's packed form.
    // D2-16450, D2-01027, D2-01873, D2-01874
    // D2-04486, D2-00757, D2-04276, D2-00756, D2-16449, D2-01063, D2-01067, D2-04590, D2-04594, D2-04371
    pub fn write(&self, w: &mut Out) -> core::result::Result<(), String> {
        match self {
            Self::Hash(h) => {
                w.u32(table_header(h.len(), h.size_index)?);
                for (k, v) in h.entries() {
                    k.write(w);
                    v.write(w);
                }
            }
            Self::MultiHash(t) => write_multi(w, &t.packed())?,
            Self::MultiMap(t) => write_multi(w, &t.packed())?,
            Self::PackedMultiHash { entries, .. } => write_multi(w, entries)?,
            Self::HashSet(s) => {
                let n: usize = s.buckets.iter().map(Vec::len).sum();
                w.u32(table_header(n, s.size_index)?);
                for i in s.buckets.iter().flatten() {
                    i.write(w);
                }
            }
            Self::Sequence { items, .. } => {
                w.u32(u32::try_from(items.len()).map_err(|e| e.to_string())?);
                for i in items {
                    i.write(w);
                }
            }
            Self::Text(units) => w.wide(units),
            Self::TextInfo(t) => t.write(w),
            Self::Vector(v) => v.iter().for_each(|x| w.u32(*x)),
            Self::Position { cell, frame } => {
                w.u32(*cell);
                frame.iter().for_each(|x| w.u32(*x));
            }
            // D2-16744
            Self::Buffer(b) => {
                w.u32(u32::try_from(b.len()).map_err(|e| e.to_string())?);
                w.bytes(b);
            }
            Self::Quest(q) => q.write(w),
            Self::Behavior(b) => b.write(w),
            Self::Shortcut(s) => s.write(w),
            Self::Options(o) => o.write(w),
            Self::EffectNode(e) => e.write(w),
            Self::DamageText(d) => d.write(w),
            Self::Keyframe(k) => k.write(w),
            Self::Scene(sc) => sc.write(w),
            Self::Scenes(list) => crate::story::write_scenes(w, list),
            Self::Act(a) => a.write(w),
            Self::Acts(list) => crate::story::write_acts(w, list),
            Self::Profile { bytes, .. } => w.bytes(bytes),
            // D2-12268, D2-12269
            Self::QuestList(list) => {
                w.u32(u32::try_from(list.len()).map_err(|e| e.to_string())?);
                for q in list {
                    q.write(w);
                }
            }
            // D2-13895, D2-13896
            Self::Packed { bytes, .. } => w.bytes(bytes),
            // A packed iterator's place is a count of steps (the table writes it so).
            // D2-04364, D2-04365, D2-04398, D2-04406, D2-04474, D2-04477
            Self::Iterator {
                container,
                position,
            } => {
                w.u32(*container);
                w.u32(*position);
            }
            Self::Opaque { ty } | Self::Adaptor { ty, .. } => {
                return Err(format!("native type {ty:#x} has no packed form"))
            }
            Self::Engine(e) => match e.0.pack() {
                Some(bytes) => w.bytes(&bytes),
                None => return Err(format!("native type {:#x} has no packed form", e.ty())),
            },
        }
        Ok(())
    }

    /// The references this object holds, in the order the collector and the packer visit them:
    /// a table's keys first, then its values, each in packing order.
    // D2-16446, D2-04379, D2-00754
    // D2-01045, D2-04372, D2-01302, D2-01303, D2-04563, D2-04564, D2-04571, D2-04579, D2-04578, D2-04580, D2-04360, D2-04361, D2-04362, D2-04363
    #[must_use]
    pub fn references(&self) -> Vec<u32> {
        let mut v = Vec::new();
        match self {
            Self::Hash(h) => {
                v.extend(h.entries().filter_map(|(k, _)| k.reference()));
                v.extend(h.entries().filter_map(|(_, x)| x.reference()));
            }
            Self::MultiHash(t) => v = t.references(),
            Self::MultiMap(t) => v = t.references(),
            Self::PackedMultiHash { entries, .. } => {
                v.extend(entries.iter().filter_map(|(k, _)| k.reference()));
                v.extend(entries.iter().filter_map(|(_, x)| x.reference()));
            }
            Self::HashSet(s) => v.extend(s.buckets.iter().flatten().filter_map(Item::reference)),
            Self::Sequence { items, .. } => v.extend(items.iter().filter_map(Item::reference)),
            Self::Iterator { container, .. } if *container != NULL => v.push(*container),
            _ => {}
        }
        v
    }

    /// Replace every reference this object holds, as a graph is read into the table.
    // D2-16447, D2-04485, D2-00755
    // D2-01046, D2-04422, D2-01304, D2-01305, D2-04565, D2-04566, D2-04572, D2-04581, D2-04582
    pub fn remap(&mut self, f: &mut dyn FnMut(u32) -> u32) {
        match self {
            Self::Hash(h) => {
                for (k, x) in h.buckets.iter_mut().flatten() {
                    k.remap(f);
                    x.remap(f);
                }
            }
            Self::MultiHash(t) => t.remap(f),
            Self::MultiMap(t) => t.remap(f),
            Self::PackedMultiHash { entries, .. } => {
                for (k, x) in entries {
                    k.remap(f);
                    x.remap(f);
                }
            }
            Self::HashSet(s) => s.buckets.iter_mut().flatten().for_each(|i| i.remap(f)),
            Self::Sequence { items, .. } => items.iter_mut().for_each(|i| i.remap(f)),
            Self::Iterator { container, .. } if *container != NULL => *container = f(*container),
            _ => {}
        }
    }

    /// A copy as the native factory makes one. A hash table or set gets as many buckets as the
    /// original and is filled by walking the original bucket by bucket, each chain from its head,
    /// adding every entry at the head of its chain: each chain of the copy is the original's
    /// reversed. A multi-hash is copied chain for chain in order, a multi-map entry for entry; a list, an array and an iterator
    /// (its container and its place in it) are copied as they are, as is every other value (the
    /// engine's values each copy every part).
    // D2-04648, D2-00790, D2-16442, D2-12349, D2-12345, D2-12374, D2-12372, D2-11943
    #[must_use]
    pub fn copied(&self) -> Self {
        match self {
            // D2-04597, D2-04598, D2-04600, D2-04560, D2-04568, D2-04575, D2-04584, D2-04537
            Self::Hash(h) => Self::Hash(Hash {
                key: h.key,
                value: h.value,
                size_index: h.size_index,
                buckets: h
                    .buckets
                    .iter()
                    .map(|c| c.iter().rev().cloned().collect())
                    .collect(),
            }),
            // D2-04527, D2-04599
            Self::HashSet(s) => Self::HashSet(HashSet {
                item: s.item,
                size_index: s.size_index,
                buckets: s
                    .buckets
                    .iter()
                    .map(|c| c.iter().rev().cloned().collect())
                    .collect(),
            }),
            // The multi-hashes and multi-maps, the list and the array, the iterators, and the rest:
            // D2-04630, D2-04632, D2-04635, D2-04636, D2-04638, D2-04602, D2-04603, D2-04604
            // D2-04531, D2-04458, D2-04526, D2-01295, D2-04382, D2-04421, D2-16743, D2-16735
            // D2-04262, D2-04263, D2-04264, D2-04265, D2-04266, D2-04267, D2-04273, D2-04308
            // D2-04309, D2-04310, D2-04311, D2-04312, D2-04313, D2-04314, D2-04315, D2-04316
            // D2-04317
            // A link's requirements: its five words.
            // D2-13047
            _ => self.clone(),
        }
    }
}

/// A multi-hash's or multi-map's packed form: the entry count, then each entry's key and value.
// D2-04608, D2-04615, D2-04619, D2-04622, D2-04607, D2-04614, D2-04621
fn write_multi(w: &mut Out, entries: &[(Item, Item)]) -> core::result::Result<(), String> {
    w.u32(u32::try_from(entries.len()).map_err(|e| e.to_string())?);
    for (k, v) in entries {
        k.write(w);
        v.write(w);
    }
    Ok(())
}

/// Where an iterator stands when it is past its container's last entry.
pub const ITERATOR_END: u32 = u32::MAX;

impl NativeObject {
    /// How many steps from this container's first entry an iterator at `position` stands, as a
    /// packed iterator keeps it: its entry's place in walking order (a multi-hash's or
    /// multi-map's iterator names its entry), the entry count when it is past the last (or
    /// stands on an entry no longer there). `position` itself when this is not a container.
    // D2-04370, D2-04418, D2-04397
    #[must_use]
    pub fn iterator_offset(&self, position: u32) -> u32 {
        let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        match self {
            Self::Hash(_) | Self::HashSet(_) | Self::Sequence { .. } => {
                let n = count(self.entry_count());
                if position == ITERATOR_END || position > n {
                    n
                } else {
                    position
                }
            }
            Self::MultiHash(t) => count(
                t.entries()
                    .position(|e| e.id == position)
                    .unwrap_or(t.len()),
            ),
            Self::MultiMap(t) => count(
                t.entries()
                    .iter()
                    .position(|e| e.id == position)
                    .unwrap_or(t.len()),
            ),
            _ => position,
        }
    }

    /// Where an iterator read from a packed graph stands in this container once both are in the
    /// table: `offset` steps from the first entry, past the last when there are not that many.
    // D2-04297, D2-04298, D2-04349, D2-04368, D2-04369, D2-04403, D2-04411, D2-04414, D2-04419
    // D2-04454
    #[must_use]
    pub fn iterator_place(&self, offset: u32) -> u32 {
        let at = offset as usize;
        match self {
            Self::Hash(_) | Self::HashSet(_) | Self::Sequence { .. } => {
                if at < self.entry_count() {
                    offset
                } else {
                    ITERATOR_END
                }
            }
            Self::MultiHash(t) => t.entries().nth(at).map_or(ITERATOR_END, |e| e.id),
            Self::MultiMap(t) => t.entries().get(at).map_or(ITERATOR_END, |e| e.id),
            _ => offset,
        }
    }

    /// The entry count of a hash table, a set, a list or an array.
    fn entry_count(&self) -> usize {
        match self {
            Self::Hash(h) => h.len(),
            Self::HashSet(s) => s.buckets.iter().map(Vec::len).sum(),
            Self::Sequence { items, .. } => items.len(),
            _ => 0,
        }
    }
}

impl NativeObject {
    /// This object as the engine holds it once read into the reference table: a packed
    /// multi-hash becomes the engine's table, read entry by entry; a quest list is read as the
    /// engine reads one ([`QuestInfo::read_list_over_one`]); anything else is itself.
    #[must_use]
    pub fn unpacked(&self) -> Self {
        match self {
            Self::PackedMultiHash {
                key,
                value,
                entries,
            } => Self::MultiHash(MultiHash::unpacked(*key, *value, entries)),
            Self::QuestList(list) => Self::QuestList(QuestInfo::read_list_over_one(list)),
            Self::Packed { ty, bytes } if *ty == native_type::BEHAVIOR_PARAMS => {
                let mut b = BehaviorParams::default();
                let mut r = In::new(bytes);
                match b.read_over(&mut r) {
                    Ok(()) if r.at_end() => Self::Behavior(Box::new(b)),
                    _ => self.clone(),
                }
            }
            Self::Packed { ty, bytes } if *ty == native_type::SHORTCUT_INFO => {
                let mut r = In::new(bytes);
                match crate::options::Shortcut::read(&mut r) {
                    Ok(s) if r.at_end() => Self::Shortcut(s),
                    _ => self.clone(),
                }
            }
            Self::Packed { ty, bytes } if *ty == native_type::GAMEPLAY_OPTIONS_PROFILE => {
                let mut r = In::new(bytes);
                match crate::options::GameplayOptions::read(&mut r) {
                    Ok(o) if r.at_end() => Self::Options(Box::new(o)),
                    _ => self.clone(),
                }
            }
            Self::Packed { ty, bytes } if *ty == native_type::EFFECT_UI_NODE => {
                let mut r = In::new(bytes);
                match EffectNode::read(&mut r) {
                    Ok(e) if r.at_end() => Self::EffectNode(Box::new(e)),
                    _ => self.clone(),
                }
            }
            Self::Packed { ty, bytes } if *ty == native_type::DAMAGE_TEXT_BLOB => {
                let mut r = In::new(bytes);
                match DamageText::read(&mut r) {
                    Ok(d) if r.at_end() => Self::DamageText(Box::new(d)),
                    _ => self.clone(),
                }
            }
            Self::Packed { ty, bytes } => {
                use crate::story;
                let mut r = In::new(bytes);
                let read = match *ty {
                    native_type::KEYFRAME => {
                        story::Keyframe::read(&mut r).map(|k| Self::Keyframe(Box::new(k)))
                    }
                    native_type::SCENE_INFO => {
                        story::SceneInfo::read(&mut r).map(|sc| Self::Scene(Box::new(sc)))
                    }
                    native_type::SCENE_INFO_LIST => story::read_scenes(&mut r).map(Self::Scenes),
                    native_type::ACT_INFO => {
                        story::ActInfo::read(&mut r).map(|a| Self::Act(Box::new(a)))
                    }
                    native_type::ACT_INFO_LIST => story::read_acts(&mut r).map(Self::Acts),
                    _ => return self.clone(),
                };
                match read {
                    Ok(o) if r.at_end() => o,
                    _ => self.clone(),
                }
            }
            other => other.clone(),
        }
    }

    /// This object as a packed graph holds it: a multi-hash becomes its entries in packing
    /// order; anything else is itself.
    #[must_use]
    pub fn packed(&self) -> Self {
        match self {
            Self::MultiHash(t) => Self::PackedMultiHash {
                key: t.key,
                value: t.value,
                entries: t.packed(),
            },
            Self::Behavior(b) => Self::Packed {
                ty: native_type::BEHAVIOR_PARAMS,
                bytes: b.pack(),
            },
            Self::Shortcut(sc) => Self::Packed {
                ty: native_type::SHORTCUT_INFO,
                bytes: sc.pack(),
            },
            Self::Options(o) => Self::Packed {
                ty: native_type::GAMEPLAY_OPTIONS_PROFILE,
                bytes: o.pack(),
            },
            Self::EffectNode(e) => Self::Packed {
                ty: native_type::EFFECT_UI_NODE,
                bytes: e.pack(),
            },
            Self::DamageText(d) => Self::Packed {
                ty: native_type::DAMAGE_TEXT_BLOB,
                bytes: d.pack(),
            },
            Self::Profile { bytes, .. } => Self::Packed {
                ty: native_type::EXAMINATION_PROFILE,
                bytes: bytes.clone(),
            },
            Self::Keyframe(_) | Self::Scene(_) | Self::Scenes(_) | Self::Act(_) | Self::Acts(_) => {
                let ty = match self {
                    Self::Keyframe(_) => native_type::KEYFRAME,
                    Self::Scene(_) => native_type::SCENE_INFO,
                    Self::Scenes(_) => native_type::SCENE_INFO_LIST,
                    Self::Act(_) => native_type::ACT_INFO,
                    _ => native_type::ACT_INFO_LIST,
                };
                let mut w = Out::new();
                // A story record always writes.
                let _ = self.write(&mut w);
                Self::Packed { ty, bytes: w.buf }
            }
            Self::Engine(e) => match e.0.pack() {
                Some(bytes) => Self::Packed { ty: e.ty(), bytes },
                None => self.clone(),
            },
            other => other.clone(),
        }
    }
}

// D2-00256, D2-01029, D2-01057, D2-01058, D2-03062, D2-04359, D2-04493, D2-04494, D2-04496, D2-04498, D2-04504
// D2-01049, D2-04497
fn read_container(r: &mut In<'_>, c: Container) -> Result<NativeObject> {
    let width = |e: Elem| match e {
        Elem::Word | Elem::Ref => 4,
        Elem::Long => 8,
        Elem::HashedRef => 12,
    };
    Ok(match c {
        Container::Hash(key, value) => {
            let (n, size_index) = read_table_header(r, width(key) + width(value))?;
            let mut buckets = empty_buckets(size_index);
            let b = buckets.len();
            for _ in 0..n {
                let k = Item::read(r, key)?;
                let v = Item::read(r, value)?;
                buckets[k.bucket(b)].push((k, v));
            }
            NativeObject::Hash(Hash {
                key,
                value,
                size_index,
                buckets,
            })
        }
        Container::MultiHash(key, value) | Container::MultiMap(key, value) => {
            let n = r.u32()? as usize;
            let n = r.count(n, width(key) + width(value))?;
            let mut entries = Vec::with_capacity(n);
            for _ in 0..n {
                entries.push((Item::read(r, key)?, Item::read(r, value)?));
            }
            if matches!(c, Container::MultiHash(..)) {
                NativeObject::PackedMultiHash {
                    key,
                    value,
                    entries,
                }
            } else {
                NativeObject::MultiMap(crate::multi_map::MultiMap::unpacked(key, value, &entries))
            }
        }
        Container::HashSet(item) => {
            let (n, size_index) = read_table_header(r, width(item))?;
            let mut buckets = empty_buckets(size_index);
            let b = buckets.len();
            for _ in 0..n {
                let i = Item::read(r, item)?;
                buckets[i.bucket(b)].push(i);
            }
            NativeObject::HashSet(HashSet {
                item,
                size_index,
                buckets,
            })
        }
        Container::List(item) | Container::Array(item) => {
            let n = r.u32()? as usize;
            let n = r.count(n, width(item))?;
            let mut items = Vec::with_capacity(n);
            for _ in 0..n {
                items.push(Item::read(r, item)?);
            }
            NativeObject::Sequence { item, items }
        }
    })
}

// ---------------------------------------------------------------------------------------------
// The extent of the formats kept as bytes
// ---------------------------------------------------------------------------------------------

fn words(r: &mut In<'_>, n: usize) -> Result<()> {
    r.bytes(n.saturating_mul(4)).map(|_| ())
}

fn counted(r: &mut In<'_>, each_words: usize) -> Result<usize> {
    let n = r.u32()? as usize;
    r.count(n, each_words * 4)
}

/// A table's header word (the count in the low 24 bits), for the formats kept as bytes.
fn short_table(r: &mut In<'_>, pair_words: usize) -> Result<usize> {
    Ok(read_table_header(r, pair_words * 4)?.0)
}

fn text_info(r: &mut In<'_>) -> Result<()> {
    TextInfo::read(r, 0).map(|_| ())
}

/// The fields a flag word selects, in a fixed order.
fn missile_parameters(r: &mut In<'_>) -> Result<()> {
    let f = r.u32()?;
    // Bit 12 selects two groups, the third and the thirteenth.
    for (bit, n) in [
        (0, 2),
        (1, 6),
        (2, 3),
        (3, 2),
        (4, 1),
        (5, 1),
        (6, 1),
        (7, 2),
        (12, 3),
        (9, 2),
        (10, 3),
        (11, 1),
        (12, 1),
        (13, 2),
        (14, 1),
        (15, 1),
        (16, 1),
        (17, 2),
    ] {
        if f & (1 << bit) != 0 {
            words(r, n)?;
        }
    }
    Ok(())
}

fn behavior_params(r: &mut In<'_>) -> Result<()> {
    let f = r.u32()?;
    let has = |b: u32| f & (1 << b) != 0;
    for (bit, n) in [(0, 1), (2, 1), (1, 1), (4, 1), (12, 1), (20, 1), (13, 2)] {
        if has(bit) {
            words(r, n)?;
        }
    }
    if has(21) {
        words(r, 3)?;
    }
    if has(16) {
        words(r, 2)?;
        let n = short_table(r, 2)?;
        words(r, 2 * n)?;
    }
    for (bit, n) in [
        (14, 3),
        (3, 1),
        (5, 1),
        (7, 1),
        (6, 1),
        (18, 1),
        (19, 1),
        (8, 1),
        (10, 1),
        (11, 1),
        (9, 1),
        (17, 1),
        (15, 1),
        (23, 1),
        (22, 1),
    ] {
        if has(bit) {
            words(r, n)?;
        }
    }
    Ok(())
}

fn keyframe(r: &mut In<'_>) -> Result<()> {
    words(r, 2)?;
    words(r, 2)?;
    words(r, 3)?;
    text_info(r)?;
    r.wide()?;
    words(r, 1)
}

fn scene_info(r: &mut In<'_>) -> Result<()> {
    words(r, 1)?;
    text_info(r)?;
    words(r, 4)?;
    let n = counted(r, 1)?;
    for _ in 0..n {
        keyframe(r)?;
    }
    Ok(())
}

fn shortcut(r: &mut In<'_>) -> Result<()> {
    crate::options::Shortcut::read(r).map(|_| ())
}

fn gameplay_options(r: &mut In<'_>) -> Result<()> {
    let f = r.u32()?;
    r.u32()?;
    let has = |b: u32| f & (1 << b) != 0;
    if has(0) {
        let n = short_table(r, 2)?;
        for _ in 0..n {
            r.wide()?;
            r.wide()?;
        }
    }
    if has(1) {
        let n = counted(r, 1)?;
        for _ in 0..n {
            shortcut(r)?;
        }
    }
    for bit in [2, 12, 3] {
        if has(bit) {
            words(r, 1)?;
        }
    }
    if has(4) {
        let n = short_table(r, 2)?;
        for _ in 0..n {
            r.u32()?;
            let m = short_table(r, 6)?;
            words(r, 6 * m)?;
        }
    }
    if has(5) {
        words(r, 1)?;
    }
    if has(6) {
        let n = short_table(r, 2)?;
        words(r, 2 * n)?;
    }
    if has(7) {
        words(r, 1)?;
    }
    words(r, 1)?;
    for bit in [8, 9, 13, 10, 11] {
        if has(bit) {
            let n = short_table(r, 2)?;
            words(r, 2 * n)?;
        }
    }
    Ok(())
}

fn examination_request(r: &mut In<'_>) -> Result<()> {
    let ty = r.u32()?;
    r.u32()?;
    match ty {
        0 => Ok(()),
        0x4000_0001 | 0x4000_0003..=0x4000_0007 | 0x4000_000B => words(r, 1),
        0x4000_0002 => words(r, 3),
        0x4000_000D => words(r, 5),
        t => r.fail(format!("examination request of type {t:#x}")),
    }
}

fn icon_desc(r: &mut In<'_>) -> Result<()> {
    let n = counted(r, 3)?;
    words(r, 3 * n)
}

fn examination_node(r: &mut In<'_>) -> Result<()> {
    let ty = r.u32()?;
    r.u32()?;
    match ty {
        0 | 0x4000_0006 => Ok(()),
        0x4000_0001 | 0x4000_000C | 0x4000_0002 | 0x4000_0003 => words(r, 2),
        0x4000_0004 => {
            r.u32()?;
            text_info(r)
        }
        0x4000_0005 | 0x4000_0007 => words(r, 1),
        0x4000_0008 | 0x4000_000A => words(r, 3),
        0x4000_0009 => icon_desc(r),
        t => r.fail(format!("examination node of type {t:#x}")),
    }
}

/// A packed list of quest records: the count, then each record.
// D2-12270
fn read_quest_list(r: &mut In<'_>) -> Result<Vec<QuestInfo>> {
    let n = r.u32()? as usize;
    let n = r.count(n, 4)?;
    let mut list = Vec::with_capacity(n);
    for _ in 0..n {
        list.push(QuestInfo::unpack(r)?);
    }
    Ok(list)
}

fn quest_info(r: &mut In<'_>) -> Result<()> {
    words(r, 1)?;
    text_info(r)?;
    text_info(r)?;
    words(r, 3)?;
    text_info(r)?;
    words(r, 1)?;
    words(r, 4)?;
    words(r, 1)?;
    words(r, 4)?;
    words(r, 2)
}

/// A float table as visual descriptions pack it: a word whose low 24 bits count the pairs.
fn float_table(r: &mut In<'_>) -> Result<()> {
    let n = (r.u32()? & 0x00FF_FFFF) as usize;
    let n = r.count(n, 8)?;
    words(r, 2 * n)
}

fn part_group(r: &mut In<'_>) -> Result<()> {
    let bits = r.u32()?;
    for b in [0x1, 0x2, 0x4, 0x8, 0x10] {
        if bits & b != 0 {
            words(r, 1)?;
        }
    }
    if bits & 0x20 != 0 {
        let n = (r.u32()? & 0x00FF_FFFF) as usize;
        let n = r.count(n, 8)?;
        for _ in 0..n {
            r.u32()?;
            float_table(r)?;
        }
    }
    if bits & 0x40 != 0 {
        words(r, 1)?;
    }
    if bits & 0x80 != 0 {
        float_table(r)?;
    }
    if bits & 0x100 != 0 {
        return r.fail("part group effect overrides");
    }
    Ok(())
}

fn visual_desc(r: &mut In<'_>) -> Result<()> {
    let bits = r.u32()?;
    for b in [0x1, 0x2, 0x4, 0x8, 0x10] {
        if bits & b != 0 {
            words(r, 1)?;
        }
    }
    if bits & 0x20 != 0 {
        words(r, 3)?;
    }
    if bits & 0x40 != 0 {
        words(r, 1)?;
    }
    if bits & 0x400 != 0 {
        words(r, 1)?;
    }
    if bits & 0x80 != 0 {
        icon_desc(r)?;
    }
    if bits & 0x100 != 0 {
        part_group(r)?;
    }
    if bits & 0x200 != 0 {
        let n = (r.u32()? & 0x00FF_FFFF) as usize;
        let n = r.count(n, 8)?;
        for _ in 0..n {
            r.u32()?;
            part_group(r)?;
        }
    }
    Ok(())
}

/// Step over one native of a type kept as bytes (or read for its extent elsewhere, as message
/// decoders do for the forms they share with natives).
pub fn step_over(r: &mut In<'_>, ty: u32) -> Result<()> {
    use native_type::*;
    match ty {
        MISSILE_PARAMETERS => missile_parameters(r),
        BEHAVIOR_PARAMS => behavior_params(r),
        SKILL_UI_NODE => {
            words(r, 2)?;
            text_info(r)?;
            text_info(r)?;
            words(r, 3)?;
            let n = counted(r, 1)?;
            words(r, n)?;
            let n = counted(r, 1)?;
            words(r, n)?;
            words(r, 3)
        }
        RANDOM_SELECTION_TABLE => {
            let n = counted(r, 3)?;
            words(r, 3 * n)?;
            let n = counted(r, 3)?;
            words(r, 3 * n)
        }
        SCENE_INFO => scene_info(r),
        SCENE_INFO_LIST => {
            let n = counted(r, 1)?;
            for _ in 0..n {
                scene_info(r)?;
            }
            Ok(())
        }
        KEYFRAME => keyframe(r),
        ACT_INFO => crate::story::ActInfo::read(r).map(|_| ()),
        ACT_INFO_LIST => crate::story::read_acts(r).map(|_| ()),
        // D2-11935
        RACE_SEX_INFO => {
            words(r, 3)?;
            let n = short_table(r, 2)?;
            words(r, 2 * n)
        }
        GAMEPLAY_OPTIONS_PROFILE => gameplay_options(r),
        EFFECT_UI_NODE => EffectNode::read(r).map(|_| ()),
        DAMAGE_TEXT_BLOB => DamageText::read(r).map(|_| ()),
        SHORTCUT_INFO => shortcut(r),
        EXAMINATION_REQUEST => examination_request(r),
        // D2-13091
        LINK_REQUIREMENTS => words(r, 5),
        EXAMINATION_PROFILE => {
            examination_request(r)?;
            let n = counted(r, 2)?;
            for _ in 0..n {
                examination_node(r)?;
            }
            words(r, 1)
        }
        SELECTION_INFO => match r.u32()? {
            0 => Ok(()),
            0x4000_0001 => words(r, 6),
            t => r.fail(format!("selection of type {t:#x}")),
        },
        UI_SAVE_LOCATIONS => {
            let n = short_table(r, 2)?;
            for _ in 0..n {
                r.u32()?;
                let m = short_table(r, 6)?;
                words(r, 6 * m)?;
            }
            Ok(())
        }
        ICON_DESC => icon_desc(r),
        VISUAL_DESC => visual_desc(r),
        APP_INFO_HASH => {
            let n = short_table(r, 2)?;
            for _ in 0..n {
                r.u32()?;
                let m = short_table(r, 2)?;
                words(r, 2 * m)?;
            }
            Ok(())
        }
        QUEST_INFO => quest_info(r),
        QUEST_INFO_LIST => {
            let n = counted(r, 1)?;
            for _ in 0..n {
                quest_info(r)?;
            }
            Ok(())
        }
        _ => r.fail(format!("native type {ty:#x}: no format")),
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (formats on hand-built bytes)
    use super::*;

    #[test]
    fn a_text_info_with_a_nested_variable_writes_back_exactly() {
        let inner = TextInfo {
            string_id: 3,
            table: 0x2300_0001,
            ..TextInfo::default()
        };
        let t = TextInfo {
            string_id: 7,
            table: 0x2300_0002,
            count: 2,
            has_literal: 1,
            literal: Some("x".encode_utf16().collect()),
            variables: vec![
                Variable {
                    id: 1,
                    kind: 4,
                    value: VariableValue::Double(0x4000_0000_0000_0000, 9),
                },
                Variable {
                    id: 2,
                    kind: 9,
                    value: VariableValue::Nested(Box::new(inner)),
                },
            ],
        };
        let mut w = Out::new();
        t.write(&mut w);
        let mut r = In::new(&w.buf);
        assert_eq!(TextInfo::read(&mut r, 0).unwrap(), t);
        assert!(r.at_end());
    }

    #[test]
    fn a_word_key_lands_in_its_remainder_bucket() {
        assert_eq!(Item::Word(23).bucket(7), 2);
        assert_eq!(
            Item::Long(0x1_0000_0000).bucket(7),
            (0x1_0000_0000u64 % 7) as usize
        );
        assert_eq!(
            Item::HashedRef {
                id: 5,
                hash: 3,
                use_hash: 1
            }
            .bucket(7),
            3
        );
    }
}
