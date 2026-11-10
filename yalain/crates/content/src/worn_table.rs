//! The worn appearances of item templates whose data names none: their classes name them in code,
//! by file names the data files cannot turn into ids. A world gives them as `[[worn]]` rows, each a
//! template, the races and sexes it fits, and the appearance; in a world with no such rows, such a
//! template has no worn look.

use std::collections::BTreeMap;

/// How many worn looks a template holds: its properties number them from 1 to this.
pub const LOOKS_HELD: usize = 16;

/// One worn look of a template: the races and the sexes it fits (masks), and the appearance it
/// gives them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Worn {
    pub template: u32,
    pub races: u32,
    pub sexes: u32,
    pub appearance: u32,
}

/// A world's worn looks, by template, each template's in the order its rows stand: a wearer takes
/// the first that fits it, and a dye colors the first.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WornLooks {
    pub by_template: BTreeMap<u32, Vec<Worn>>,
}

impl WornLooks {
    /// The looks of these rows, in their order.
    #[must_use]
    pub fn of_rows(rows: impl IntoIterator<Item = Worn>) -> Self {
        let mut out = Self::default();
        for w in rows {
            out.push(w);
        }
        out
    }

    /// Add a look after the template's others.
    pub fn push(&mut self, w: Worn) {
        self.by_template.entry(w.template).or_default().push(w);
    }

    /// A template's looks, in order.
    #[must_use]
    pub fn of(&self, template: u32) -> &[Worn] {
        self.by_template.get(&template).map_or(&[], Vec::as_slice)
    }

    /// Every look, by template and in order.
    pub fn iter(&self) -> impl Iterator<Item = &Worn> + '_ {
        self.by_template.values().flatten()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_template.is_empty()
    }
}
