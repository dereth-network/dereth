//! What a character's vitals regain while it is out of the world: per vital, so many points for
//! each whole span of seconds it was away, given back as it enters again. A vital no row names
//! regains nothing while its character is away.

use std::collections::BTreeMap;

/// A vital, as the world database names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Vital {
    Health,
    Vigor,
    Focus,
}

impl Vital {
    /// Every vital.
    pub const ALL: [Self; 3] = [Self::Health, Self::Vigor, Self::Focus];

    /// Its word, as the world database writes it.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Health => "health",
            Self::Vigor => "vigor",
            Self::Focus => "focus",
        }
    }

    /// The vital a word names.
    #[must_use]
    pub fn from_word(w: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|v| v.word() == w)
    }
}

/// How a vital comes back while its character is away: `points` for each whole `every` seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AwayRegen {
    /// Seconds, above 0.
    pub every: u32,
    /// Above 0.
    pub points: u32,
}

impl AwayRegen {
    /// The points regained in `away` seconds: `points` for each whole `every`.
    #[must_use]
    pub fn after(self, away: f64) -> i32 {
        if self.every == 0 || away <= 0.0 {
            return 0;
        }
        let spans = (away / f64::from(self.every)).floor();
        let points = spans * f64::from(self.points);
        if points >= f64::from(i32::MAX) {
            i32::MAX
        } else {
            #[allow(clippy::cast_possible_truncation)]
            let p = points as i32;
            p
        }
    }
}

/// What each vital regains while its character is away, by the vital.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AwayRegens {
    pub by_vital: BTreeMap<Vital, AwayRegen>,
}

impl AwayRegens {
    /// How a vital comes back while away, if a row says.
    #[must_use]
    pub fn of(&self, v: Vital) -> Option<AwayRegen> {
        self.by_vital.get(&v).copied()
    }
}
