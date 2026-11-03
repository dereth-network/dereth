//! Text: string tables (`.stt`, one per language file), enum maps (`.emp`) and string states
//! (`.sti`).

use auberean_dat::reader::Result;
use auberean_dat::{Reader, Writer};

use crate::{Context, Decode, Encode};

/// The string table record type.
pub const STRING_TABLE_TYPE: u32 = 0x4000_0026;
/// The type asking for a record's string data (a string table asked for it names no other).
pub const STRING_DATA_TYPE: u32 = 0x4000_0008;

/// A table of localized strings (`.stt`, local files `0x25000000`-`0x26FFFFFF`), keyed by token.
///
/// A new or emptied table has every number setting unset (all ones: "the language's").
// D2-14099, D2-14083, D2-14093, D2-14082, D2-05047, D2-14084, D2-14102
#[derive(Clone, Debug, PartialEq)]
pub struct StringTable {
    pub state: u32,
    /// How numbers substituted into the strings are written: their base, the number of decimals,
    /// and whether to keep a leading zero.
    pub number_base: u16,
    pub decimal_digits: u16,
    pub leading_zero: u32,
    pub entries: Vec<StringEntry>,
}

impl Default for StringTable {
    fn default() -> Self {
        Self {
            state: 0,
            number_base: 0xFFFF,
            decimal_digits: 0xFFFF,
            leading_zero: 0xFFFF_FFFF,
            entries: Vec::new(),
        }
    }
}

/// One token's text: its variants (a slot may be empty), the variables it substitutes, and
/// optionally the variables' names.
// D2-14077, D2-14078, D2-14089
#[derive(Clone, Debug, PartialEq, Default)]
pub struct StringEntry {
    pub token: u32,
    pub table: u32,
    pub strings: Vec<Option<String>>,
    pub variables: Vec<u32>,
    pub variable_names: Vec<String>,
}

/// How many bytes a wide string packs to from an aligned offset: its length (two bytes, six past
/// 65,534 units), its units, then alignment.
fn wide_pack_size(s: &str) -> usize {
    let units = s.encode_utf16().count();
    (2 + if units >= 0xFFFF { 4 } else { 0 } + units * 2).next_multiple_of(4)
}

impl StringEntry {
    /// Whether the variables' names are worth storing: when at least two of them are `PRE`,
    /// `NAME`, `MID` or `POST`, the names an item's generated name is put together from.
    // D2-14081
    #[must_use]
    pub fn names_worth_packing(&self) -> bool {
        self.variable_names
            .iter()
            .filter(|n| matches!(n.as_str(), "PRE" | "NAME" | "MID" | "POST"))
            .count()
            > 1
    }

    /// Empty the entry: no strings, variables or names, and no private table (its token stays).
    // D2-14073
    pub fn clear(&mut self) {
        self.strings.clear();
        self.variable_names.clear();
        self.variables.clear();
        self.table = 0;
    }

    /// Copy the entry into another, emptied first: its private table, its strings without the
    /// empty slots, its variables and its variables' names (the other's token stays).
    // D2-14095
    pub fn copy_into(&self, other: &mut StringEntry) {
        other.clear();
        other.table = self.table;
        other.strings = self
            .strings
            .iter()
            .filter(|s| s.is_some())
            .cloned()
            .collect();
        other.variables.clone_from(&self.variables);
        other.variable_names.clone_from(&self.variable_names);
    }

    /// How many bytes [`StringEntry::write`] packs the entry to (its token not counted).
    // D2-14090
    #[must_use]
    pub fn pack_size(&self) -> usize {
        let mut n: usize = 6;
        for s in &self.strings {
            n = (n + 2).next_multiple_of(4);
            if let Some(s) = s {
                n += wide_pack_size(s);
            }
        }
        n = (n.next_multiple_of(4) + 2 + self.variables.len() * 4).next_multiple_of(4);
        if self.names_worth_packing() {
            n += 4 + self
                .variable_names
                .iter()
                .map(|s| wide_pack_size(s))
                .sum::<usize>();
            n = n.next_multiple_of(4);
        }
        n
    }
}

impl StringEntry {
    /// A table word; a 16-bit word whose low 15 bits count the string slots and whose top bit
    /// says variable names follow; each slot a 16-bit flag, alignment, and when the flag is set a
    /// wide string; alignment; a 16-bit count of variable words, the words; alignment; and the
    /// variable names (a counted list of wide strings, then alignment) when the top bit was set.
    // D2-14092, D2-04004
    fn read(r: &mut Reader<'_>, token: u32) -> Result<Self> {
        let table = r.u32()?;
        let slots = r.u16()?;
        let mut strings = Vec::new();
        for _ in 0..(slots & 0x7FFF) {
            let present = r.u16()?;
            r.align()?;
            strings.push(if present != 0 {
                Some(r.wide_string()?)
            } else {
                None
            });
        }
        r.align()?;
        let n = r.count16(4)?;
        let mut variables = Vec::with_capacity(n);
        for _ in 0..n {
            variables.push(r.u32()?);
        }
        r.align()?;
        let mut variable_names = Vec::new();
        if slots & 0x8000 != 0 {
            variable_names = r.list(2, Reader::wide_string)?;
            r.align()?;
        }
        Ok(Self {
            token,
            table,
            strings,
            variables,
            variable_names,
        })
    }
}

impl StringEntry {
    /// Write the entry as [`StringEntry::read`] reads it. The variables' names are written only
    /// when they are worth it ([`StringEntry::names_worth_packing`]); otherwise they are dropped.
    // D2-14092, D2-04004, D2-14091
    pub fn write(&self, w: &mut Writer) {
        w.u32(self.table);
        let slots = u16::try_from(self.strings.len()).unwrap_or(0x7FFF) & 0x7FFF;
        let named = if self.names_worth_packing() {
            0x8000
        } else {
            0
        };
        w.u16(slots | named);
        for s in &self.strings {
            w.u16(u16::from(s.is_some()));
            w.align();
            if let Some(s) = s {
                w.wide_string(s);
            }
        }
        w.align();
        w.u16(u16::try_from(self.variables.len()).unwrap_or(u16::MAX));
        for v in &self.variables {
            w.u32(*v);
        }
        w.align();
        if named != 0 {
            w.list(&self.variable_names, |w, n| w.wide_string(n));
            w.align();
        }
    }
}

impl StringTable {
    /// The first entry of a token, as the table finds it.
    // D2-14094
    #[must_use]
    pub fn entry(&self, token: u32) -> Option<&StringEntry> {
        self.entries.iter().find(|e| e.token == token)
    }

    /// Empty the table: no entries, state 0, every number setting unset.
    // D2-14080
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Copy the table into another, emptied first: its state and number settings, and a copy of
    /// each entry (empty string slots dropped, [`StringEntry::copy_into`]); an entry whose token
    /// the other already holds is not added again.
    // D2-14101
    pub fn copy_into(&self, other: &mut StringTable) {
        other.clear();
        other.state = self.state;
        other.number_base = self.number_base;
        other.leading_zero = self.leading_zero;
        other.decimal_digits = self.decimal_digits;
        for e in &self.entries {
            if other.entries.iter().any(|o| o.token == e.token) {
                continue;
            }
            let mut copy = StringEntry {
                token: e.token,
                ..StringEntry::default()
            };
            e.copy_into(&mut copy);
            other.entries.push(copy);
        }
    }

    /// The records a table needs loaded with it when asked for `kind`: the private tables its
    /// entries name (none when string data is asked for).
    // D2-14079
    pub fn sub_ids(&self, kind: u32, out: &mut auberean_dat::QualifiedIds) {
        if kind == STRING_DATA_TYPE {
            return;
        }
        for e in &self.entries {
            out.add(e.table);
        }
    }

    /// How many bytes the table packs to (its id included).
    // D2-14075
    #[must_use]
    pub fn pack_size(&self) -> usize {
        let mut n = (4 + 12usize).next_multiple_of(4) + 4;
        for e in &self.entries {
            n += 4 + e.pack_size();
        }
        n
    }
}

impl Encode for StringTable {
    // D2-14100, D2-14076
    fn encode(&self, id: u32, w: &mut Writer) {
        w.u32(id);
        w.u32(self.state);
        w.u16(self.number_base);
        w.u32(self.leading_zero);
        w.u16(self.decimal_digits);
        w.align();
        w.list(&self.entries, |w, e| {
            w.u32(e.token);
            e.write(w);
        });
    }
}

impl Decode for StringTable {
    /// The id, the state word, the number base (16 bits), the leading-zero word, the decimal digits
    /// (16 bits), alignment, then a counted list of token words each followed by its entry.
    // D2-14100
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        r.expect_id(id)?;
        let state = r.u32()?;
        let number_base = r.u16()?;
        let leading_zero = r.u32()?;
        let decimal_digits = r.u16()?;
        r.align()?;
        let entries = r.list(8, |r| {
            let token = r.u32()?;
            StringEntry::read(r, token)
        })?;
        Ok(Self {
            state,
            number_base,
            decimal_digits,
            leading_zero,
            entries,
        })
    }
}

/// Names for numbers (`.emp`): an optional base map whose names this one extends, and a name per
/// value.
// D2-14371, D2-14373, D2-14374, D2-14366
#[derive(Clone, Debug, PartialEq, Default)]
pub struct EnumMap {
    pub id: u32,
    pub base: u32,
    pub names: Vec<(u32, String)>,
}

impl EnumMap {
    /// The records it needs: its base map, when it has one.
    // D2-16815
    pub fn sub_ids(&self, out: &mut auberean_dat::QualifiedIds) {
        if self.base != 0 {
            out.add(self.base);
        }
    }

    /// The map's own id, the base map's id (or 0), then a counted list of a value and its name (a
    /// narrow string). Also read embedded in other records, id included.
    // D2-14375, D2-14365
    pub fn read(r: &mut Reader<'_>) -> Result<Self> {
        let id = r.u32()?;
        let base = r.u32()?;
        let names = r.list(6, |r| Ok((r.u32()?, r.narrow_string()?)))?;
        Ok(Self { id, base, names })
    }

    /// The name of a value, if this map holds it (the base map is not consulted).
    #[must_use]
    pub fn name(&self, value: u32) -> Option<&str> {
        self.names
            .iter()
            .find(|(v, _)| *v == value)
            .map(|(_, n)| n.as_str())
    }

    /// The value a name has in this map (the base map is not consulted). Letters compare
    /// without case; of several values whose names compare equal, the greatest has the name.
    #[must_use]
    pub fn enum_of(&self, name: &str) -> Option<u32> {
        self.names
            .iter()
            .filter(|(_, n)| n.eq_ignore_ascii_case(name))
            .map(|(v, _)| *v)
            .max()
    }

    /// Write the map as [`EnumMap::read`] reads it.
    // D2-14375
    pub fn write(&self, w: &mut Writer) {
        w.u32(self.id);
        w.u32(self.base);
        w.list(&self.names, |w, (v, n)| {
            w.u32(*v);
            w.narrow_string(n);
        });
    }
}

/// The token a string table keys a name's text by (`ID_Hello` is `0x2824938C`): for the name's
/// character at place `i`, its low seven bits placed at the word's top and turned right by `7 * i`
/// bits (modulo 32), all of them combined by exclusive or. The client keys every table entry so;
/// a name's token can be computed from the name alone, and two names can share one.
// D2-14275
#[must_use]
pub fn token(name: &str) -> u32 {
    let mut h = 0u32;
    let mut turn = 0u32;
    for b in name.bytes() {
        h ^= (u32::from(b) << 25).rotate_right(turn);
        turn = (turn + 7) & 31;
    }
    h
}

impl Encode for EnumMap {
    // D2-14375
    fn encode(&self, _id: u32, w: &mut Writer) {
        self.write(w);
    }
}

// D2-05028, D2-14372, D2-14377
impl Decode for EnumMap {
    fn decode(r: &mut Reader<'_>, id: u32, _ctx: &Context) -> Result<Self> {
        let m = Self::read(r)?;
        if m.id != id {
            return Err(auberean_dat::DecodeError {
                at: 0,
                what: format!("id {:08X}, filed as {id:08X}", m.id),
            });
        }
        Ok(m)
    }
}

/// A string state (`.sti`, local files `0x58000000`+): how a language writes numbers, plurals and
/// generated names, and how its input method is drawn. The record does not store its own id.
// D2-14191, D2-14197
#[derive(Clone, Debug, PartialEq, Default)]
pub struct StringState {
    pub version: u32,
    pub number_base: u16,
    pub decimal_digits: u16,
    pub leading_zero: u32,
    pub grouping_size: u16,
    pub numerals: String,
    pub decimal_separator: String,
    pub grouping_separator: String,
    pub negative_format: String,
    /// Whether 0, 1, -1, 2 and up, and -2 and down take the singular.
    pub singular: [u32; 5],
    /// The letters generated item names are built from (prefix, middle, suffix), and the letters
    /// of generated male and female player names.
    pub name_letters: [String; 5],
    /// Display and input-method settings: gore and blood, the input method's symbol, candidate and
    /// composition colors and translucence, and word wrapping.
    pub settings: [u32; 20],
    pub additional_settings: String,
    pub additional_flags: u32,
}

/// How a language's input method draws: the symbol showing its mode, the candidate list and the
/// composition string. Colors are 32-bit words as stored; heights, placements and translucences
/// are bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ImeAppearance {
    pub symbol_color: u32,
    pub symbol_color_text: u32,
    pub symbol_height: u8,
    pub symbol_translucence: u8,
    pub symbol_placement: u8,
    pub cand_color_base: u32,
    pub cand_color_border: u32,
    pub cand_color_text: u32,
    pub comp_color_input: u32,
    pub comp_color_target_conv: u32,
    pub comp_color_converted: u32,
    pub comp_color_target_not_conv: u32,
    pub comp_color_input_err: u32,
    pub comp_translucence: u8,
    pub comp_color_text: u32,
}

impl StringState {
    /// The highest gore level the language allows (setting 0).
    #[must_use]
    pub fn top_gore_level(&self) -> u32 {
        self.settings[0]
    }

    /// The highest blood color the language allows (setting 1).
    #[must_use]
    pub fn top_blood_color(&self) -> u32 {
        self.settings[1]
    }

    /// Whether the language turns its input method on (setting 2).
    #[must_use]
    pub fn ime_enabled(&self) -> u32 {
        self.settings[2]
    }

    /// The input method's look (settings 3 to 17; the byte-sized ones are each a word's low
    /// byte).
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn ime_appearance(&self) -> ImeAppearance {
        let s = &self.settings;
        ImeAppearance {
            symbol_color: s[3],
            symbol_color_text: s[4],
            symbol_height: s[5] as u8,
            symbol_translucence: s[6] as u8,
            symbol_placement: s[7] as u8,
            cand_color_base: s[8],
            cand_color_border: s[9],
            cand_color_text: s[10],
            comp_color_input: s[11],
            comp_color_target_conv: s[12],
            comp_color_converted: s[13],
            comp_color_target_not_conv: s[14],
            comp_color_input_err: s[15],
            comp_translucence: s[16] as u8,
            comp_color_text: s[17],
        }
    }

    /// Another input method's setting (setting 18).
    #[must_use]
    pub fn other_ime(&self) -> u32 {
        self.settings[18]
    }

    /// Whether the language's text wraps at spaces (setting 19).
    #[must_use]
    pub fn word_wrap_on_space(&self) -> bool {
        self.settings[19] != 0
    }

    /// How many bytes the record packs to.
    // D2-14187, D2-14194
    #[must_use]
    pub fn pack_size(&self) -> usize {
        crate::encode(0, self).len()
    }
}

impl Encode for StringState {
    // D2-14189, D2-14207, D2-14188, D2-14195
    fn encode(&self, _id: u32, w: &mut Writer) {
        w.u32(self.version);
        w.u16(self.number_base);
        w.u16(self.decimal_digits);
        w.u32(self.leading_zero);
        w.u16(self.grouping_size);
        w.align();
        w.wide_string(&self.numerals);
        w.wide_string(&self.decimal_separator);
        w.wide_string(&self.grouping_separator);
        w.wide_string(&self.negative_format);
        for s in self.singular {
            w.u32(s);
        }
        for s in &self.name_letters {
            w.wide_string(s);
        }
        for s in self.settings {
            w.u32(s);
        }
        w.wide_string(&self.additional_settings);
        w.u32(self.additional_flags);
    }
}

// D2-05075, D2-14192, D2-14205
impl Decode for StringState {
    /// The version, the number base and decimal digits (16 bits each), the leading-zero word, the
    /// grouping size (16 bits), alignment, four wide strings (numerals and separators), five
    /// singular words, five wide strings of name letters, twenty setting words, a wide string and
    /// a flags word.
    // D2-14189, D2-14207, D2-14196
    fn decode(r: &mut Reader<'_>, _id: u32, _ctx: &Context) -> Result<Self> {
        let version = r.u32()?;
        let number_base = r.u16()?;
        let decimal_digits = r.u16()?;
        let leading_zero = r.u32()?;
        let grouping_size = r.u16()?;
        r.align()?;
        let numerals = r.wide_string()?;
        let decimal_separator = r.wide_string()?;
        let grouping_separator = r.wide_string()?;
        let negative_format = r.wide_string()?;
        let mut singular = [0u32; 5];
        for s in &mut singular {
            *s = r.u32()?;
        }
        let name_letters = [
            r.wide_string()?,
            r.wide_string()?,
            r.wide_string()?,
            r.wide_string()?,
            r.wide_string()?,
        ];
        let mut settings = [0u32; 20];
        for s in &mut settings {
            *s = r.u32()?;
        }
        let additional_settings = r.wide_string()?;
        let additional_flags = r.u32()?;
        Ok(Self {
            version,
            number_base,
            decimal_digits,
            leading_zero,
            grouping_size,
            numerals,
            decimal_separator,
            grouping_separator,
            negative_format,
            singular,
            name_letters,
            settings,
            additional_settings,
            additional_flags,
        })
    }
}
