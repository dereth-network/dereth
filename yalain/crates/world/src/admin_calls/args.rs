//! An admin call's values read in the order its parameters are declared.

use auberean_script::wire::In;

use super::float_of;
use crate::calls::{Node, Value};

/// A call's values, first parameter first (they arrive last first).
#[derive(Debug, Clone)]
pub struct Args<'a> {
    values: Vec<&'a Value>,
}

impl<'a> Args<'a> {
    /// The values as they arrive.
    #[must_use]
    pub fn of(values: &'a [Value]) -> Self {
        Self {
            values: values.iter().rev().collect(),
        }
    }

    /// How many values the call holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// The call holds no value.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// A parameter as it arrived.
    #[must_use]
    pub fn value(&self, i: usize) -> Option<&'a Value> {
        self.values.get(i).copied()
    }

    /// The singleton a parameter names: its saved state.
    #[must_use]
    pub fn singleton(&self, i: usize) -> Option<u32> {
        match self.values.get(i)?.root()? {
            Node::Singleton(id) => Some(*id),
            _ => None,
        }
    }

    /// A whole number parameter (a truth value too).
    #[must_use]
    pub fn int(&self, i: usize) -> Option<u32> {
        match self.values.get(i)? {
            Value::Int(n) => Some(*n),
            _ => None,
        }
    }

    /// A truth value parameter.
    #[must_use]
    pub fn flag(&self, i: usize) -> Option<bool> {
        self.int(i).map(|n| n != 0)
    }

    /// A signed whole number parameter.
    #[must_use]
    pub fn signed(&self, i: usize) -> Option<i32> {
        self.int(i).map(u32::cast_signed)
    }

    /// A 64-bit parameter: an object's id, or a long whole number.
    #[must_use]
    pub fn long(&self, i: usize) -> Option<u64> {
        match self.values.get(i)? {
            Value::Long(n) => Some(*n),
            _ => None,
        }
    }

    /// A float parameter.
    #[must_use]
    pub fn float(&self, i: usize) -> Option<f32> {
        float_of(self.values.get(i)?)
    }

    /// A string parameter, or the literal of a string reference.
    #[must_use]
    pub fn text(&self, i: usize) -> Option<String> {
        match self.values.get(i)?.root()? {
            Node::Text(t) => Some(t.clone()),
            Node::TextInfo(t) => t.literal.as_ref().map(|l| String::from_utf16_lossy(l)),
            _ => None,
        }
    }

    /// The strings of a list parameter, in order.
    #[must_use]
    pub fn list(&self, i: usize) -> Vec<String> {
        let Some(v) = self.values.get(i) else {
            return Vec::new();
        };
        let Some(Node::Native { ty, bytes }) = v.root() else {
            return Vec::new();
        };
        if u32::from(*ty) != crate::calls::REF_LIST {
            return Vec::new();
        }
        let mut r = In::new(bytes);
        let n = r.u32().unwrap_or(0) as usize;
        let mut out = Vec::with_capacity(n.min(64));
        for _ in 0..n {
            let Ok(id) = r.u32() else { break };
            if let Some(Node::Text(t)) = v.entry(id) {
                out.push(t.clone());
            }
        }
        out
    }
}
