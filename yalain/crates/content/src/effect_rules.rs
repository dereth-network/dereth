//! What the world database says of effects where their own data says nothing: what a ward keeps
//! off, the share of its holder's vitae an effect takes or the vitae it gives back, what an
//! immunity leaves as it ends, the complete reset of skills an effect grants, the untraining an
//! effect finishes, the skill an effect trains, the items an effect gives, and what an effect
//! places in the world.

use std::collections::{BTreeMap, BTreeSet};

/// What a ward keeps off its holder: effects of these classes, and these effects by their saved
/// states, each by its chance.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Ward {
    pub classes: Vec<u32>,
    pub effects: Vec<u32>,
    /// From 0 to 1, at any spellcraft.
    pub chance: f32,
}

/// Vitae an effect gives back when it is put on: points of it, never above full, to its player and,
/// where it reaches them, the player's fellows in the world.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct VitaeRestore {
    /// Above 0 and at most 100.
    pub points: f32,
    pub fellows: bool,
}

/// Which of its skills a complete reset gives a player back: those bought with skill credits, or
/// its hero skills.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Reset {
    Skills,
    HeroSkills,
}

impl Reset {
    /// The word a world's rows name it by.
    #[must_use]
    pub fn from_word(w: &str) -> Option<Self> {
        match w {
            "skills" => Some(Self::Skills),
            "hero-skills" => Some(Self::HeroSkills),
            _ => None,
        }
    }
}

/// The items an effect gives its player when it is put on: so many of a template, made in its
/// pack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Grant {
    pub template: u32,
    /// Above 0.
    pub count: u32,
}

/// What an effect places in the world at its user: an object of a template, at an offset in
/// meters from the user as it faces (`[across, ahead]`), on the ground and facing as the user
/// faces, standing so many seconds (none: until a tally clears it, or the world stops), named as
/// the row names it (none: its template's name).
#[derive(Debug, Clone, PartialEq)]
pub struct Placing {
    pub template: u32,
    pub offset: [f32; 2],
    pub lasts: Option<u32>,
    pub name: Option<crate::Words>,
}

/// The world's effect rules, by the effect's saved state.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EffectRules {
    pub wards: BTreeMap<u32, Ward>,
    /// The share of its player's vitae an effect takes when it is put on, above 0 and at most 1.
    pub vitae_taps: BTreeMap<u32, f32>,
    /// The vitae an effect gives back when it is put on.
    pub vitae_restores: BTreeMap<u32, VitaeRestore>,
    /// The effects that are an immunity under way: hidden from their holder, each leaves this
    /// effect as it ends (the ward that keeps its kind off).
    pub immunities: BTreeMap<u32, u32>,
    /// The effects that grant their player one complete reset of a kind of its skills when they
    /// are put on.
    pub resets: BTreeMap<u32, Reset>,
    /// The effects that finish their player's untraining under way at once when they are put on,
    /// as earning its experience again would.
    pub untrains: BTreeSet<u32>,
    /// The effects that train their player a skill for good when they are put on: the skill, by
    /// its number.
    pub skills: BTreeMap<u32, u32>,
    /// The effects that give their player items when they are put on.
    pub grants: BTreeMap<u32, Grant>,
    /// The effects that place an object in the world at their player when they are put on.
    pub places: BTreeMap<u32, Placing>,
    /// The same, for effects the install names (an item's use naming one) but holds no saved
    /// state of, by the name.
    pub places_named: BTreeMap<String, Placing>,
}
