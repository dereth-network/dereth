//! `PrefValueConst` — the `const`-constructible preference value the tables are written with.
//!
//! `dereth_ui_screens::options::config` re-exports it. `super::store::init` and
//! `super::preferences::UI_PREFERENCES` both name it, so it lives with them; the option *page* —
//! `ConfigRow`, `Control`, `CONFIG_PAGE` and the element ids — stays in
//! the UI.

use crate::view::PrefValue;

/// A `const`-constructible [`PrefValue`]; `PrefValue::Text` never appears as a UI default.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PrefValueConst {
    Bool(bool),
    Int(i32),
    Float(f32),
}

impl From<PrefValueConst> for PrefValue {
    fn from(v: PrefValueConst) -> Self {
        match v {
            PrefValueConst::Bool(b) => Self::Bool(b),
            PrefValueConst::Int(i) => Self::Int(i),
            PrefValueConst::Float(f) => Self::Float(f),
        }
    }
}
