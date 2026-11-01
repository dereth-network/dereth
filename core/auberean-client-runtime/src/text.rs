//! String references from messages and scripts, for writing to text with `auberean-text`.
//!
//! A front end shows a message's string reference (a tell, a notice, a panel's text) as the client
//! does: convert it here and ask it for its text over a [`DatSource`] of the client's data files
//! (the portal file first, then the language's):
//!
//! ```no_run
//! # fn files() -> Vec<std::sync::Arc<auberean_dat::DatFile>> { Vec::new() }
//! # let info = auberean_protocol::text::StringInfo::literal("Asheron");
//! use auberean_client_runtime::text::from_wire;
//! let mut source = auberean_text::DatSource::new(files());
//! let line: String = from_wire(&info).text(&mut source);
//! ```

use auberean_protocol::text::{StringInfo as Wire, VariableValue as W};
use auberean_script::native_object::TextInfo;
use auberean_text::{StringInfo, Value, Variable};

pub use auberean_text::DatSource;

/// A message's string reference as the formatter takes it (field for field; a variable's kind
/// says how its value reads).
#[must_use]
#[allow(clippy::cast_possible_wrap)]
pub fn from_wire(s: &Wire) -> StringInfo {
    let value = |kind: u16, v: &W| match (kind, v) {
        (2, W::Word(a)) => Value::Int(*a as i32),
        (6, W::Word(a)) => Value::UInt(*a),
        (3, W::Words(a, g, b)) => Value::FormattedInt {
            value: *a as i32,
            grouping: g.cast_signed(),
            base: *b,
        },
        (7, W::Words(a, g, b)) => Value::FormattedUInt {
            value: *a,
            grouping: g.cast_signed(),
            base: *b,
        },
        (4, W::Object(v, d)) => Value::Float {
            value: f64::from_bits(*v),
            decimals: *d,
        },
        (5, W::ObjectWith(v, d, g, b)) => Value::FormattedFloat {
            value: f64::from_bits(*v),
            decimals: *d,
            grouping: g.cast_signed(),
            base: *b,
        },
        (9, W::Nested(n)) => Value::Text(Box::new(from_wire(n))),
        (10, W::Long(v)) => Value::Long(*v as i64),
        (12, W::Long(v)) => Value::ULong(*v),
        (11, W::LongWith(v, g, b)) => Value::FormattedLong {
            value: *v as i64,
            grouping: g.cast_signed(),
            base: *b,
        },
        (13, W::LongWith(v, g, b)) => Value::FormattedULong {
            value: *v,
            grouping: g.cast_signed(),
            base: *b,
        },
        (k, _) => Value::Nothing(k),
    };
    StringInfo {
        string_id: s.string_id,
        table: s.table_id,
        literal: (s.has_literal != 0).then(|| s.literal.clone().unwrap_or_default().0),
        variables: s
            .variables
            .iter()
            .map(|v| Variable {
                id: v.id,
                value: value(v.kind, &v.value),
            })
            .collect(),
    }
}

/// A script's string reference as the formatter takes it.
#[must_use]
pub fn from_script(t: &TextInfo) -> StringInfo {
    auberean_glue::text::to_formatter(t)
}
