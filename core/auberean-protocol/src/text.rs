//! Text on the wire: narrow and wide strings, each scrambled, and the string-table reference a
//! message names localized text with.
//!
//! A string is a `u16` count of characters (`0xFFFF`, then a `u32`, for a longer one), the
//! characters' bytes scrambled ([`auberean_primitives::scramble`]), then padding to four from the
//! message start. The types here hold the plain characters; the scramble is applied on the way
//! out and undone on the way in.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

use auberean_primitives::scramble;

use crate::wire::{Field, Reader, Result, Writer};

/// The longest string any message carries, in characters; a longer count is a corrupt message.
pub const MAX_CHARS: usize = 0x1_0000;

fn read_count(r: &mut Reader<'_>) -> Result<usize> {
    let n = r.u16()?;
    let n = if n == 0xFFFF {
        r.u32()? as usize
    } else {
        usize::from(n)
    };
    if n > MAX_CHARS {
        return Err(r.invalid("string length"));
    }
    Ok(n)
}

fn write_count(w: &mut Writer, n: usize) {
    match u16::try_from(n) {
        Ok(n) if n != 0xFFFF => w.u16(n),
        _ => {
            w.u16(0xFFFF);
            w.count(n);
        }
    }
}

/// A narrow string: one byte per character (Latin-1 for anything past ASCII).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NarrowString(pub Vec<u8>);

impl NarrowString {
    /// The text, each byte read as the Latin-1 character of the same value.
    #[must_use]
    pub fn text(&self) -> String {
        self.0.iter().map(|&b| char::from(b)).collect()
    }
}

impl From<&str> for NarrowString {
    /// Characters past Latin-1 become `?`.
    fn from(s: &str) -> Self {
        Self(
            s.chars()
                .map(|c| u8::try_from(u32::from(c)).unwrap_or(b'?'))
                .collect(),
        )
    }
}

impl Field for NarrowString {
    // D2-00700
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let n = read_count(r)?;
        let plain = scramble::reveal(r.bytes(n)?);
        r.align()?;
        Ok(Self(plain))
    }

    // D2-00247
    fn write(&self, w: &mut Writer) {
        write_count(w, self.0.len());
        w.bytes(&scramble::conceal(&self.0));
        w.align();
    }
}

/// A wide string: UTF-16 code units, kept as sent (an unpaired surrogate survives a round trip).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WideString(pub Vec<u16>);

impl WideString {
    /// The text (an unpaired surrogate becomes U+FFFD).
    #[must_use]
    pub fn text(&self) -> String {
        char::decode_utf16(self.0.iter().copied())
            .map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER))
            .collect()
    }
}

impl From<&str> for WideString {
    fn from(s: &str) -> Self {
        Self(s.encode_utf16().collect())
    }
}

impl Field for WideString {
    // D2-00380
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let n = read_count(r)?;
        let plain = scramble::reveal(r.bytes(n * 2)?);
        r.align()?;
        Ok(Self(
            plain
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| u16::from_le_bytes(*c))
                .collect(),
        ))
    }

    // D2-00373
    fn write(&self, w: &mut Writer) {
        write_count(w, self.0.len());
        let bytes: Vec<u8> = self.0.iter().flat_map(|u| u.to_le_bytes()).collect();
        w.bytes(&scramble::conceal(&bytes));
        w.align();
    }
}

/// The value of one variable a [`StringInfo`] fills its text with. Which shape it has follows
/// from the variable's kind.
#[derive(Clone, Debug, PartialEq)]
pub enum VariableValue {
    /// Kinds with no stored value.
    Empty,
    /// Kinds 2 and 6: one word.
    Word(u32),
    /// Kinds 3 and 7: two words and a half-word.
    Words(u32, u32, u16),
    /// Kind 4: an object id and its stamp.
    Object(u64, u16),
    /// Kind 5: an object id, its stamp, a word and a half-word.
    ObjectWith(u64, u16, u32, u16),
    /// Kind 9: another string reference.
    Nested(Box<StringInfo>),
    /// Kinds 10 and 12: a 64-bit value.
    Long(u64),
    /// Kinds 11 and 13: a 64-bit value, a word and a half-word.
    LongWith(u64, u32, u16),
}

/// One variable: its id, its kind, its value.
#[derive(Clone, Debug, PartialEq)]
pub struct Variable {
    pub id: u32,
    pub kind: u16,
    pub value: VariableValue,
}

/// Localized text by reference: an entry of a string table (or a literal), with variables.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct StringInfo {
    pub string_id: u32,
    pub table_id: u32,
    /// The variable count as sent (a signed half-word; a negative count carries none).
    pub variable_count: i16,
    /// Non-zero when a literal follows; kept as sent.
    pub has_literal: u16,
    pub literal: Option<WideString>,
    pub variables: Vec<Variable>,
}

/// How deep string references may nest inside each other.
const MAX_NESTING: u32 = 8;

impl StringInfo {
    /// A literal text, as servers send names they make up (a world's name, a player's).
    #[must_use]
    pub fn literal(text: &str) -> Self {
        Self {
            has_literal: 1,
            literal: Some(WideString::from(text)),
            ..Self::default()
        }
    }

    fn read_at(r: &mut Reader<'_>, depth: u32) -> Result<Self> {
        if depth > MAX_NESTING {
            return Err(r.invalid("string reference nesting"));
        }
        let string_id = r.u32()?;
        let table_id = r.u32()?;
        let variable_count = r.i16()?;
        let has_literal = r.u16()?;
        if variable_count > 256 {
            return Err(r.invalid("string reference variables"));
        }
        let literal = if has_literal != 0 {
            Some(WideString::read(r)?)
        } else {
            None
        };
        let n = usize::try_from(variable_count.max(0)).unwrap_or(0);
        let mut variables = Vec::with_capacity(n);
        for _ in 0..n {
            let id = r.u32()?;
            let kind = r.u16()?;
            r.align()?;
            let value = match kind {
                2 | 6 => VariableValue::Word(r.u32()?),
                3 | 7 => VariableValue::Words(r.u32()?, r.u32()?, r.u16()?),
                4 => VariableValue::Object(r.u64()?, r.u16()?),
                5 => VariableValue::ObjectWith(r.u64()?, r.u16()?, r.u32()?, r.u16()?),
                9 => VariableValue::Nested(Box::new(Self::read_at(r, depth + 1)?)),
                10 | 12 => VariableValue::Long(r.u64()?),
                11 | 13 => VariableValue::LongWith(r.u64()?, r.u32()?, r.u16()?),
                _ => VariableValue::Empty,
            };
            r.align()?;
            variables.push(Variable { id, kind, value });
        }
        r.align()?;
        Ok(Self {
            string_id,
            table_id,
            variable_count,
            has_literal,
            literal,
            variables,
        })
    }
}

impl Field for StringInfo {
    // D2-14174
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Self::read_at(r, 0)
    }

    // D2-14142
    fn write(&self, w: &mut Writer) {
        w.u32(self.string_id);
        w.u32(self.table_id);
        w.i16(self.variable_count);
        w.u16(self.has_literal);
        if self.has_literal != 0 {
            self.literal.clone().unwrap_or_default().write(w);
        }
        for v in &self.variables {
            w.u32(v.id);
            w.u16(v.kind);
            w.align();
            match &v.value {
                VariableValue::Empty => {}
                VariableValue::Word(a) => w.u32(*a),
                VariableValue::Words(a, b, c) => {
                    w.u32(*a);
                    w.u32(*b);
                    w.u16(*c);
                }
                VariableValue::Object(a, b) => {
                    w.u64(*a);
                    w.u16(*b);
                }
                VariableValue::ObjectWith(a, b, c, d) => {
                    w.u64(*a);
                    w.u16(*b);
                    w.u32(*c);
                    w.u16(*d);
                }
                VariableValue::Nested(s) => s.write(w),
                VariableValue::Long(a) => w.u64(*a),
                VariableValue::LongWith(a, b, c) => {
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
