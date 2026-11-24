//! What the world database says of creatures' behavior: whom a concealing effect spares or makes
//! hated, which behavior tables heal their kin, which friend tables never attack a player first,
//! which friend tables fight which, a shooter's rhythm by its template, the creatures that mark the
//! players they see, those that take on only the players they hate, those players may not harm, the
//! camps and kinds that let pass a player holding an item or bearing an effect, the effects
//! creatures hold from their first moment, and how often the creatures of a class idle at rest and
//! as they notice a player.

use std::collections::{BTreeMap, BTreeSet};

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

/// A line said of a player marked: the race it is for (none: any other) and the string reference
/// (string id, table).
pub type MarkLine = (Option<u32>, (u32, u32));

/// What a kind of creature does to a player that comes within its perception: the effect it puts on
/// the player (its saved state), and the line it says of the player as it does, by the player's race.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Mark {
    pub effect: u32,
    /// The lines, each for one race (1 human, 2 Lugian, 4 Tumerok, 8 Drudge, 16 Empyrean) or, with
    /// none, for any race no other line is for: a string reference (string id, table).
    pub lines: Vec<MarkLine>,
}

impl Mark {
    /// The line said of a player of a race: its race's, else the one for any race.
    #[must_use]
    pub fn line_for(&self, race: u32) -> Option<(u32, u32)> {
        self.lines
            .iter()
            .find(|(r, _)| *r == Some(race))
            .or_else(|| self.lines.iter().find(|(r, _)| r.is_none()))
            .map(|(_, line)| *line)
    }
}

/// The creatures a safe passage is for: a camp's, by its row in the spawn table, or every creature
/// of a template.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Guards {
    Camp(u32),
    Template(u32),
}

/// What lets a player pass: an item of a template it holds (in its pack or worn), or an effect it
/// bears, by its saved state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pass {
    Item(u32),
    Effect(u32),
}

/// Creatures that leave alone a player who holds an item or bears an effect, as a concealing
/// effect's spared class does: they don't take it on, come to their kin's fight with it, or turn to
/// it, until it hurts them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SafePassage {
    pub guards: Guards,
    pub pass: Pass,
}

impl SafePassage {
    /// Is it for a creature of this camp (by its row in the spawn table) and template?
    #[must_use]
    pub fn is_for(&self, camp: Option<u32>, template: u32) -> bool {
        match self.guards {
            Guards::Camp(c) => camp == Some(c),
            Guards::Template(t) => t == template,
        }
    }
}

/// An effect creatures of these templates hold from their first moment, for as long as they stand:
/// the effect (its saved state), at a spellcraft by the creature's level, read along the line
/// between the points (level, spellcraft) and held at the ends; one point is the same at every
/// level.
#[derive(Debug, Clone, PartialEq)]
pub struct CreatureEffect {
    pub templates: Vec<u32>,
    pub effect: u32,
    pub spellcraft: Vec<(f32, f32)>,
}

/// The world's creature rows.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CreatureRows {
    /// What creatures of a class (none: every creature) make of the holder of a concealing
    /// effect, by its saved state and the class.
    pub concealing: BTreeMap<(u32, Option<u32>), Regard>,
    /// Whether creatures of a behavior table heal their kin, by the table's name.
    pub heals_kin: BTreeMap<String, bool>,
    /// The attack skills creatures of a behavior table never use, by the table's name.
    pub skips: BTreeMap<String, Vec<u32>>,
    /// Whether creatures of a friend table may attack a player first, by the table.
    pub attacks_first: BTreeMap<u32, bool>,
    /// The shooting rhythms of creatures' templates.
    pub rhythms: BTreeMap<u32, Rhythm>,
    /// What creatures of a template do to the players that come within their perception.
    pub marks: BTreeMap<u32, Mark>,
    /// The templates whose creatures take on only a player a concealing effect makes them hate.
    pub only_hated: BTreeSet<u32>,
    /// The templates whose creatures never play their victory over a player they kill.
    pub no_victory: BTreeSet<u32>,
    /// The friend tables whose creatures the creatures of a friend table take on, by the table.
    pub fights: BTreeMap<u32, BTreeSet<u32>>,
    /// The templates whose creatures players may not harm, with the line a player's attack on one
    /// is told (a string reference: string id, table), if any.
    pub kept_out: BTreeMap<u32, Option<(u32, u32)>>,
    /// The templates whose creatures hold the kingdom their template names for a while after they
    /// appear, and how long, in seconds: players of that kingdom may not harm one meanwhile.
    pub aligned: BTreeMap<u32, f64>,
    /// The camps and kinds of creature that let pass a player holding an item or bearing an effect.
    pub safe_passage: Vec<SafePassage>,
    /// The effects creatures of some templates hold from their first moment.
    pub effects: Vec<CreatureEffect>,
    /// The mean gap between the idle behaviors at rest of the creatures of an AI class (their
    /// template's AI superclass), in seconds, by the class.
    pub idle_every: BTreeMap<u32, f32>,
    /// The weight of each special, by its skill: the chance it is used at a generic monster's attack
    /// with it at hand.
    pub special_weights: BTreeMap<u32, f32>,
    /// How much an AI class's creatures with no sub class move their specials' weights, by the
    /// class: a term on the logit scale.
    pub special_terms: BTreeMap<u32, f32>,
    /// The chance that a creature of an AI class plays an idle behavior as it notices a player, by
    /// the class: it goes after the player once the idle is through.
    pub notice_idle: BTreeMap<u32, f32>,
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

    /// Does a creature of this camp (by its row in the spawn table) and template let pass a player
    /// holding these items (by template) and bearing these effects (saved states)?
    #[must_use]
    pub fn lets_pass(
        &self,
        camp: Option<u32>,
        template: u32,
        holds: impl Fn(u32) -> bool,
        bears: impl Fn(u32) -> bool,
    ) -> bool {
        self.safe_passage
            .iter()
            .filter(|r| r.is_for(camp, template))
            .any(|r| match r.pass {
                Pass::Item(t) => holds(t),
                Pass::Effect(e) => bears(e),
            })
    }

    /// Whether a row makes creatures regard a player by this effect (a saved state): a concealing
    /// effect's, or a safe passage's.
    #[must_use]
    pub fn reads_effect(&self, state: u32) -> bool {
        self.concealing
            .range((state, None)..=(state, Some(u32::MAX)))
            .next()
            .is_some()
            || self
                .safe_passage
                .iter()
                .any(|r| r.pass == Pass::Effect(state))
    }

    /// The effects a creature of this template holds from its first moment, in the rows' order.
    pub fn effects_of(&self, template: u32) -> impl Iterator<Item = &CreatureEffect> {
        self.effects
            .iter()
            .filter(move |e| e.templates.contains(&template))
    }

    /// Whether creatures of a behavior table heal their kin.
    #[must_use]
    pub fn heals_kin(&self, table: Option<&str>) -> bool {
        table.is_some_and(|t| self.heals_kin.get(t) == Some(&true))
    }

    /// The attack skills creatures of a behavior table never use.
    #[must_use]
    pub fn skips(&self, table: Option<&str>) -> &[u32] {
        table
            .and_then(|t| self.skips.get(t))
            .map_or(&[], Vec::as_slice)
    }

    /// Whether creatures of a friend table may attack a player first.
    #[must_use]
    pub fn may_attack_first(&self, table: Option<u32>) -> bool {
        table.is_none_or(|t| self.attacks_first.get(&t) != Some(&false))
    }

    /// Whether creatures of one friend table take on creatures of another.
    #[must_use]
    pub fn fights(&self, table: Option<u32>, other: Option<u32>) -> bool {
        match (table, other) {
            (Some(t), Some(o)) => self.fights.get(&t).is_some_and(|f| f.contains(&o)),
            _ => false,
        }
    }

    /// Whether creatures of a friend table take on creatures of any other.
    #[must_use]
    pub fn fights_any(&self, table: Option<u32>) -> bool {
        table.is_some_and(|t| self.fights.get(&t).is_some_and(|f| !f.is_empty()))
    }
}
