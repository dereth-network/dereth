//! A string reference as plain data: a string of a string table, or a literal, and the variables
//! written into it ([`TextRef`]).
//!
//! The views give what is shown written out where it can be; a name or a label whose string tables
//! the client may not have yet comes as its reference too, so an interface that waits for the
//! tables can write it once they are at hand.

/// A string reference: the string's id in its table (0 for none), the table, or the literal text;
/// and its variables, in the reference's own order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextRef {
    pub string: u32,
    pub table: u32,
    /// The literal text, as UTF-16 units, when the reference is one.
    pub literal: Option<Vec<u16>>,
    pub variables: Vec<TextVariable>,
}

/// One variable of a reference: its id and its value.
#[derive(Clone, Debug, PartialEq)]
pub struct TextVariable {
    pub id: u32,
    pub value: TextValue,
}

/// A variable's value. A formatted number carries its grouping and its base (negative for the
/// table's or the language's); a float, how many decimals it is written with.
#[derive(Clone, Debug, PartialEq)]
pub enum TextValue {
    Int(i32),
    FormattedInt {
        value: i32,
        grouping: i32,
        base: u16,
    },
    Float {
        value: f64,
        decimals: u16,
    },
    FormattedFloat {
        value: f64,
        decimals: u16,
        grouping: i32,
        base: u16,
    },
    UInt(u32),
    FormattedUInt {
        value: u32,
        grouping: i32,
        base: u16,
    },
    /// Another reference.
    Text(Box<TextRef>),
    Long(i64),
    FormattedLong {
        value: i64,
        grouping: i32,
        base: u16,
    },
    ULong(u64),
    FormattedULong {
        value: u64,
        grouping: i32,
        base: u16,
    },
    /// A kind of value the reference carries none of, by its number.
    Nothing(u16),
}

impl TextRef {
    /// A literal.
    #[must_use]
    pub fn literal(text: &str) -> Self {
        Self {
            literal: Some(text.encode_utf16().collect()),
            ..Self::default()
        }
    }
}
