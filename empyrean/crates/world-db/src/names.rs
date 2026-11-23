//! ACE's names for the numbers a row keys by: a weenie's property types, attributes, skills, body
//! parts and position types, an emote's category and action type, a weenie's type. A number no name
//! is given for is written as itself.

use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;

use empyrean_entity::enums::{
    CombatBodyPart, EmoteCategory, EmoteType, PositionType, PropertyAttribute,
    PropertyAttribute2nd, PropertyBool, PropertyDataId, PropertyFloat, PropertyInstanceId,
    PropertyInt, PropertyInt64, PropertyString, Skill, WeenieType,
};

use crate::value::R;

/// An enumeration's names both ways.
#[derive(Debug, Default)]
pub struct Names {
    by_value: BTreeMap<i64, &'static str>,
    by_name: HashMap<&'static str, i64>,
}

impl Names {
    fn new(values: impl Iterator<Item = i64>, names: &'static [&'static str]) -> Self {
        let mut n = Self::default();
        for (v, name) in values.zip(names.iter().copied()) {
            n.by_value.entry(v).or_insert(name);
            n.by_name.entry(name).or_insert(v);
        }
        n
    }

    /// `v`'s name, or `v` itself.
    #[must_use]
    pub fn name(&self, v: i64) -> String {
        self.by_value
            .get(&v)
            .map_or_else(|| v.to_string(), |n| (*n).to_owned())
    }

    /// The number a name (or a number written as one) stands for.
    pub fn value(&self, name: &str) -> R<i64> {
        if let Some(v) = self.by_name.get(name) {
            return Ok(*v);
        }
        name.parse::<i64>()
            .map_err(|_| format!("{name:?} isn't a name this table knows"))
    }
}

/// The enumerations rows use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Int,
    Int64,
    Bool,
    Float,
    Str,
    Did,
    Iid,
    Position,
    Attribute,
    Vital,
    Skill,
    BodyPart,
    EmoteCategory,
    EmoteType,
    WeenieType,
}

macro_rules! table {
    ($ty:ty) => {
        Names::new(<$ty>::ALL.iter().map(|e| i64::from(e.0)), <$ty>::NAMES)
    };
}

/// The names of `kind`.
#[must_use]
pub fn of(kind: Kind) -> &'static Names {
    static TABLES: OnceLock<Vec<Names>> = OnceLock::new();
    let all = TABLES.get_or_init(|| {
        vec![
            table!(PropertyInt),
            table!(PropertyInt64),
            table!(PropertyBool),
            table!(PropertyFloat),
            table!(PropertyString),
            table!(PropertyDataId),
            table!(PropertyInstanceId),
            table!(PositionType),
            table!(PropertyAttribute),
            table!(PropertyAttribute2nd),
            table!(Skill),
            table!(CombatBodyPart),
            table!(EmoteCategory),
            table!(EmoteType),
            table!(WeenieType),
        ]
    });
    &all[kind as usize]
}
