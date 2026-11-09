//! What the world database says of items beyond their templates: what an item's use does where its
//! template names nothing, what a book, card or person shows when used, and what every new
//! character is given as it first enters.

use std::collections::BTreeMap;

/// An item's use where its template names no effect for it: the effects it puts on its user, the
/// behavior it plays, and how many uses it wears out after.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemUse {
    pub template: u32,
    /// Each effect put on, in order: its saved state and the spellcraft it is cast at.
    pub effects: Vec<(u32, f32)>,
    /// The behavior its use plays, where its template names none.
    pub behavior: Option<u32>,
    /// It wears by its uses rather than going at its first: the uses it has when it was not made
    /// with a durability of its own.
    pub wears: Option<u32>,
}

/// What a book, a card or a person shows when used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shows {
    /// A book: its pages' string table, with its page controls.
    Book(u32),
    /// A picture, with no page controls.
    Picture(u32),
}

/// The world database's item rows.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ItemRows {
    /// Item uses, by template.
    pub uses: BTreeMap<u32, ItemUse>,
    /// What each template shows when used.
    pub books: BTreeMap<u32, Shows>,
    /// What every new character is given as it first enters: template, count.
    pub start_items: BTreeMap<u32, u32>,
}

impl ItemRows {
    /// The effects an item of this template's use puts on that its template doesn't name: saved
    /// state and spellcraft.
    pub fn effects(&self, template: u32) -> impl Iterator<Item = (u32, f32)> + '_ {
        self.uses
            .get(&template)
            .into_iter()
            .flat_map(|u| u.effects.iter().copied())
    }

    /// The behavior an item of this template's use plays, where its template names none.
    #[must_use]
    pub fn behavior(&self, template: u32) -> Option<u32> {
        self.uses.get(&template).and_then(|u| u.behavior)
    }

    /// The uses an item of this template wears out after, when it wears by its uses.
    #[must_use]
    pub fn wears(&self, template: u32) -> Option<u32> {
        self.uses.get(&template).and_then(|u| u.wears)
    }

    /// Every saved state the rows' uses put on.
    pub fn states(&self) -> impl Iterator<Item = u32> + '_ {
        self.uses
            .values()
            .flat_map(|u| u.effects.iter().map(|(s, _)| *s))
    }

    /// Every template the rows give.
    pub fn given(&self) -> impl Iterator<Item = u32> + '_ {
        self.start_items.keys().copied()
    }
}
