//! A character being made: an avatar for every race and sex, made once with random choices as the
//! game makes them (height and build in the middle), kept while the player goes back and forth;
//! the race and sex chosen, the name, and the way the figure is turned. What it sends is the
//! character the chosen avatar makes, as the game's own creation screens send it.

use std::collections::BTreeMap;

use auberean_client_sdk::NewCharacter;
use auberean_primitives::random::Random;
use auberean_primitives::DataId;
use auberean_world::chargen::{self, Avatar, Choice};
use auberean_world::look::{Desc, LookRecords};
use auberean_world::place::PlaceRecords;
use auberean_world::records::WorldRecords;

/// The races in the order the window offers them, with their names; the last two only for an
/// account that may make them.
pub const RACES: [(u32, &str); 5] = [
    (chargen::HUMAN, "Human"),
    (chargen::LUGIAN, "Lugian"),
    (chargen::TUMEROK, "Tumerok"),
    (chargen::EMPYREAN, "Empyrean"),
    (chargen::DRUDGE, "Drudge"),
];

/// The sexes, with their names.
pub const SEXES: [(u32, &str); 2] = [(chargen::MALE, "Male"), (chargen::FEMALE, "Female")];

/// The kinds of appearance choice, in the order the window lists them, each with Horizon's name
/// for it.
pub const KINDS: [(u32, &str); 13] = [
    (chargen::HEIGHT, "Height"),
    (chargen::BUILD, "Build"),
    (chargen::SKIN_TONE, "Skin tone"),
    (chargen::SKIN_DETAIL, "Skin detail"),
    (chargen::FACE_DETAIL, "Face"),
    (chargen::EYE_COLOR, "Eye color"),
    (chargen::HEAD_DETAIL, "Head"),
    (chargen::HEAD_FRILL, "Hair style"),
    (chargen::FRILL_COLOR, "Hair color"),
    (chargen::SPECIAL, "Markings"),
    (chargen::SHIRT, "Shirt"),
    (chargen::PANTS, "Pants"),
    (chargen::BOOTS, "Boots"),
];

/// The longest name the window takes.
pub const NAME_LIMIT: usize = 32;

/// One kind of choice as the window shows it: its kind, its name, how many stops it has and the
/// stop the chosen avatar stands at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub kind: u32,
    pub name: &'static str,
    pub stops: usize,
    pub at: usize,
}

/// A character being made.
#[derive(Clone, Debug)]
pub struct Creation {
    avatars: BTreeMap<(u32, u32), Avatar>,
    choices: BTreeMap<(u32, u32), Vec<Choice>>,
    /// The race and sex chosen.
    pub chosen: (u32, u32),
    /// The races the account may make beyond the first three.
    pub may_make: u32,
    /// The name typed.
    pub name: String,
    /// An administrator's character, where the client offers one.
    pub admin: bool,
    /// The figure's heading, degrees: 0 sees its front.
    pub turn: f32,
    random: Random,
}

impl Creation {
    /// A character made over the world's records: an avatar for every race and sex, its looks
    /// picked with `random`.
    #[must_use]
    pub fn new(records: &dyn LookRecords, may_make: u32, random: Random) -> Self {
        let mut choices = BTreeMap::new();
        for (race, _) in RACES {
            for (sex, _) in SEXES {
                choices.insert((race, sex), chargen::choices(records, race, sex));
            }
        }
        Self::with_choices(choices, may_make, random)
    }

    /// A character made over these choices for each race and sex.
    #[must_use]
    pub fn with_choices(
        choices: BTreeMap<(u32, u32), Vec<Choice>>,
        may_make: u32,
        mut random: Random,
    ) -> Self {
        let avatars = choices
            .iter()
            .map(|(&(race, sex), c)| ((race, sex), Avatar::new(c, race, sex, &mut random)))
            .collect();
        Self {
            avatars,
            choices,
            chosen: (chargen::HUMAN, chargen::MALE),
            may_make,
            name: String::new(),
            admin: false,
            turn: 0.0,
            random,
        }
    }

    /// The races the account may make, in the window's order.
    #[must_use]
    pub fn races(&self) -> Vec<(u32, &'static str)> {
        RACES
            .iter()
            .copied()
            .filter(|(r, _)| {
                let restricted = r & (chargen::EMPYREAN | chargen::DRUDGE);
                restricted == 0 || self.may_make & restricted != 0
            })
            .collect()
    }

    /// Choose a race and sex: its own avatar, as it was left.
    pub fn choose(&mut self, race: u32, sex: u32) {
        if self.avatars.contains_key(&(race, sex)) {
            self.chosen = (race, sex);
        }
    }

    /// The chosen avatar.
    #[must_use]
    pub fn avatar(&self) -> Option<&Avatar> {
        self.avatars.get(&self.chosen)
    }

    /// The chosen avatar's kinds of choice, in the window's order: those its race has.
    #[must_use]
    pub fn rows(&self) -> Vec<Row> {
        let (Some(a), Some(cs)) = (
            self.avatars.get(&self.chosen),
            self.choices.get(&self.chosen),
        ) else {
            return Vec::new();
        };
        KINDS
            .iter()
            .filter_map(|&(kind, name)| {
                let c = cs.iter().find(|c| c.physique_type == kind)?;
                if c.values.len() < 2 {
                    return None;
                }
                let at = a
                    .value(kind)
                    .and_then(|v| chargen::stop_of(&c.values, v))
                    .unwrap_or(0);
                Some(Row {
                    kind,
                    name,
                    stops: c.values.len(),
                    at,
                })
            })
            .collect()
    }

    /// Set a kind of choice to one of its stops.
    pub fn set(&mut self, kind: u32, stop: usize) {
        let v = self
            .choices
            .get(&self.chosen)
            .and_then(|cs| cs.iter().find(|c| c.physique_type == kind))
            .and_then(|c| c.values.get(stop).copied());
        if let (Some(v), Some(a)) = (v, self.avatars.get_mut(&self.chosen)) {
            a.values.insert(kind, v);
        }
    }

    /// Pick the kinds of choice in `mask` at random, as the game's own Randomize does.
    pub fn randomize(&mut self, mask: u32) {
        if let (Some(a), Some(cs)) = (
            self.avatars.get_mut(&self.chosen),
            self.choices.get(&self.chosen),
        ) {
            a.randomize(cs, mask, &mut self.random);
        }
    }

    /// The chosen avatar's look: its body with its choices.
    #[must_use]
    pub fn look(&self, records: &WorldRecords) -> Option<Desc> {
        let (race, sex) = self.chosen;
        let body = records
            .entity(DataId(chargen::body_entity(race, sex)?))?
            .object?;
        let a = self.avatars.get(&self.chosen)?;
        Some(a.look(self.choices.get(&self.chosen)?, DataId(body)))
    }

    /// The tallest the chosen race and sex may be made: the highest stop of its height, as the
    /// figure's height scale (none when the race has no height to choose).
    #[must_use]
    pub fn tallest(&self) -> Option<f32> {
        self.choices
            .get(&self.chosen)?
            .iter()
            .find(|c| c.physique_type == chargen::HEIGHT)?
            .values
            .iter()
            .copied()
            .reduce(f32::max)
    }

    /// The name, as it is sent: without the spaces at its ends.
    #[must_use]
    pub fn trimmed_name(&self) -> &str {
        self.name.trim()
    }

    /// The character the chosen avatar makes under the name typed, once there is one.
    #[must_use]
    pub fn character(&self) -> Option<NewCharacter> {
        let name = self.trimmed_name();
        if name.is_empty() {
            return None;
        }
        auberean_shell::dev::new_character(name, self.admin, self.avatar()?)
    }
}
