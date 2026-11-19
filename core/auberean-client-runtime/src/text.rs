//! String references from messages and scripts, for writing to text with `auberean-text`, and as
//! the contract's views carry them ([`to_view`], [`from_view`]).
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

use auberean_client_contract::text::{TextRef, TextValue, TextVariable};
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

/// A string reference as the contract's views carry it.
#[must_use]
pub fn to_view(s: &StringInfo) -> TextRef {
    let value = |v: &Value| match v {
        Value::Int(a) => TextValue::Int(*a),
        Value::FormattedInt {
            value,
            grouping,
            base,
        } => TextValue::FormattedInt {
            value: *value,
            grouping: *grouping,
            base: *base,
        },
        Value::Float { value, decimals } => TextValue::Float {
            value: *value,
            decimals: *decimals,
        },
        Value::FormattedFloat {
            value,
            decimals,
            grouping,
            base,
        } => TextValue::FormattedFloat {
            value: *value,
            decimals: *decimals,
            grouping: *grouping,
            base: *base,
        },
        Value::UInt(a) => TextValue::UInt(*a),
        Value::FormattedUInt {
            value,
            grouping,
            base,
        } => TextValue::FormattedUInt {
            value: *value,
            grouping: *grouping,
            base: *base,
        },
        Value::Text(t) => TextValue::Text(Box::new(to_view(t))),
        Value::Long(a) => TextValue::Long(*a),
        Value::FormattedLong {
            value,
            grouping,
            base,
        } => TextValue::FormattedLong {
            value: *value,
            grouping: *grouping,
            base: *base,
        },
        Value::ULong(a) => TextValue::ULong(*a),
        Value::FormattedULong {
            value,
            grouping,
            base,
        } => TextValue::FormattedULong {
            value: *value,
            grouping: *grouping,
            base: *base,
        },
        Value::Nothing(k) => TextValue::Nothing(*k),
    };
    TextRef {
        string: s.string_id,
        table: s.table,
        literal: s.literal.clone(),
        variables: s
            .variables
            .iter()
            .map(|v| TextVariable {
                id: v.id,
                value: value(&v.value),
            })
            .collect(),
    }
}

/// A view's string reference as the formatter takes it: the same reference [`to_view`] was given.
#[must_use]
pub fn from_view(t: &TextRef) -> StringInfo {
    let value = |v: &TextValue| match v {
        TextValue::Int(a) => Value::Int(*a),
        TextValue::FormattedInt {
            value,
            grouping,
            base,
        } => Value::FormattedInt {
            value: *value,
            grouping: *grouping,
            base: *base,
        },
        TextValue::Float { value, decimals } => Value::Float {
            value: *value,
            decimals: *decimals,
        },
        TextValue::FormattedFloat {
            value,
            decimals,
            grouping,
            base,
        } => Value::FormattedFloat {
            value: *value,
            decimals: *decimals,
            grouping: *grouping,
            base: *base,
        },
        TextValue::UInt(a) => Value::UInt(*a),
        TextValue::FormattedUInt {
            value,
            grouping,
            base,
        } => Value::FormattedUInt {
            value: *value,
            grouping: *grouping,
            base: *base,
        },
        TextValue::Text(t) => Value::Text(Box::new(from_view(t))),
        TextValue::Long(a) => Value::Long(*a),
        TextValue::FormattedLong {
            value,
            grouping,
            base,
        } => Value::FormattedLong {
            value: *value,
            grouping: *grouping,
            base: *base,
        },
        TextValue::ULong(a) => Value::ULong(*a),
        TextValue::FormattedULong {
            value,
            grouping,
            base,
        } => Value::FormattedULong {
            value: *value,
            grouping: *grouping,
            base: *base,
        },
        TextValue::Nothing(k) => Value::Nothing(*k),
    };
    StringInfo {
        string_id: t.string,
        table: t.table,
        literal: t.literal.clone(),
        variables: t
            .variables
            .iter()
            .map(|v| Variable {
                id: v.id,
                value: value(&v.value),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (a reference carried as the views' plain data and back, field for field)
    use super::*;

    #[test]
    fn a_reference_carried_by_a_view_comes_back_the_same() {
        let nested = StringInfo {
            string_id: 7,
            table: 0x2300_0001,
            literal: None,
            variables: vec![Variable {
                id: 2,
                value: Value::FormattedUInt {
                    value: 1200,
                    grouping: 3,
                    base: 10,
                },
            }],
        };
        let info = StringInfo {
            string_id: 0x0123_4567,
            table: 0x2300_0012,
            literal: Some("Aldric".encode_utf16().collect()),
            variables: vec![
                Variable {
                    id: 1,
                    value: Value::Int(-4),
                },
                Variable {
                    id: 3,
                    value: Value::FormattedFloat {
                        value: 2.5,
                        decimals: 1,
                        grouping: -1,
                        base: 0xFFFF,
                    },
                },
                Variable {
                    id: 4,
                    value: Value::Text(Box::new(nested)),
                },
                Variable {
                    id: 5,
                    value: Value::FormattedLong {
                        value: -9,
                        grouping: 0,
                        base: 16,
                    },
                },
                Variable {
                    id: 6,
                    value: Value::Nothing(8),
                },
            ],
        };
        let view = to_view(&info);
        assert_eq!(view.string, 0x0123_4567);
        assert_eq!(view.variables.len(), 5);
        assert_eq!(view.variables[0].value, TextValue::Int(-4));
        assert_eq!(from_view(&view), info);
    }
}
