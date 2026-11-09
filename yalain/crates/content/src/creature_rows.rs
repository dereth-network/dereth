//! What the world database says of creatures' behavior: whom a concealing effect spares or makes
//! hated, which behavior tables heal their kin, which friend tables never attack a player first,
//! and a shooter's rhythm by its template.

use std::collections::BTreeMap;

/// What a concealing effect makes creatures of a class make of its holder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Regard {
    /// They don't take it on unless it hurts them.
    Spares,
    /// They take it on, even those that never attack first.
    Hates,
}

impl Regard {
    /// The regard a row's word names: `spares` or `hates`.
    #[must_use]
    pub fn from_word(w: &str) -> Option<Self> {
        match w {
            "spares" => Some(Self::Spares),
            "hates" => Some(Self::Hates),
            _ => None,
        }
    }

    /// The word a row writes it as.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Spares => "spares",
            Self::Hates => "hates",
        }
    }
}

/// A kind of creature's shooting rhythm: the gap between its shots, times its shot's time scale,
/// and its windup before a missile leaves, in seconds at the creatures' median time scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rhythm {
    pub gap: f32,
    pub windup: f32,
}

/// The world's creature rows.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CreatureRows {
    /// What creatures of a class (none: every creature) make of the holder of a concealing
    /// effect, by its saved state and the class.
    pub concealing: BTreeMap<(u32, Option<u32>), Regard>,
    /// Whether creatures of a behavior table heal their kin, by the table's name.
    pub heals_kin: BTreeMap<String, bool>,
    /// Whether creatures of a friend table may attack a player first, by the table.
    pub attacks_first: BTreeMap<u32, bool>,
    /// The shooting rhythms of creatures' templates.
    pub rhythms: BTreeMap<u32, Rhythm>,
}

impl CreatureRows {
    /// What a creature of a class makes of the holder of these effects (saved states): hate over
    /// sparing, where they give both.
    #[must_use]
    pub fn regard(
        &self,
        states: impl IntoIterator<Item = u32>,
        class: Option<u32>,
    ) -> Option<Regard> {
        let mut found = None;
        for state in states {
            for (_, how) in self
                .concealing
                .range((state, None)..=(state, Some(u32::MAX)))
                .filter(|((_, of), _)| of.is_none() || *of == class)
            {
                if *how == Regard::Hates {
                    return Some(Regard::Hates);
                }
                found = Some(*how);
            }
        }
        found
    }

    /// Whether creatures of a behavior table heal their kin.
    #[must_use]
    pub fn heals_kin(&self, table: Option<&str>) -> bool {
        table.is_some_and(|t| self.heals_kin.get(t) == Some(&true))
    }

    /// Whether creatures of a friend table may attack a player first.
    #[must_use]
    pub fn may_attack_first(&self, table: Option<u32>) -> bool {
        table.is_none_or(|t| self.attacks_first.get(&t) != Some(&false))
    }
}
