//! A world's escorts: creatures a quest step sets to follow its player (a lost pet led home, a
//! freed captive walked to safety), whose death fails the quest.
//!
//! **Made** as the step runs, after its update: the creature appears beside its player as one of
//! its pets, of its own kind of pet (the world's `[[pet]]` row for its template), and follows and
//! fights as that kind does. A player has one escort of a quest at a time.
//!
//! **Its end.** Its death fails its quest; so does its player going through portal space, where
//! the escort says so, or its player getting too far from it, where it names how far. It goes,
//! with nothing failed, as its quest reaches the phase it lasts until, or as the quest is done,
//! failed or cancelled. A player leaving the world takes it out with it, and finds it beside it
//! again on coming back while the quest still wants it.
//!
//! A world with no escorts does none of this.

use crate::quests::Step;

/// What follows a step's player, and what ends it.
#[derive(Debug, Clone, PartialEq)]
pub struct Escort {
    /// The template of the creature, whose `[[pet]]` row says how it follows and fights.
    pub template: u32,
    /// The icon its player's list of pets shows it with (0: none).
    pub icon: u32,
    /// The phase of its quest it goes at (none: it stays until the quest is no longer under way).
    pub until: Option<u32>,
    /// Its player more than this many meters from it fails the quest (none: never).
    pub far: Option<f32>,
    /// Its player going through portal space fails the quest; otherwise it goes along, as a pet
    /// does.
    pub portal_fails: bool,
}

impl Escort {
    /// Does it still follow its player with its quest at this phase (0: no longer under way)?
    #[must_use]
    pub fn wanted_at(&self, made_at: u32, phase: u32) -> bool {
        phase >= made_at.max(1) && self.until.is_none_or(|u| phase < u)
    }
}

/// The phase a step leaves its quest at, where it leaves it under way: a giving's first, an
/// advance's own; none for a completion, a failure or a step that moves nothing.
#[must_use]
pub fn phase_after(step: &Step) -> Option<u32> {
    match step.update {
        crate::quests::Update::Bestow => Some(1),
        crate::quests::Update::Advance(n) => Some(n),
        crate::quests::Update::Nothing => (step.needs > 0).then_some(step.needs),
        crate::quests::Update::Complete | crate::quests::Update::Fail => None,
    }
}

/// A world's escorts: the steps that make one, each with what it makes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EscortTable {
    pub steps: Vec<(Step, Escort)>,
}

impl EscortTable {
    /// What a step makes follow its player, if anything: the escort of the step of its quest and
    /// key, whatever else two steps share.
    #[must_use]
    pub fn of(&self, step: &Step) -> Option<&Escort> {
        self.steps
            .iter()
            .find(|(s, _)| s.quest == step.quest && s.key == step.key)
            .map(|(_, e)| e)
    }

    /// The escorts of a quest that follow its player at a phase (0: not under way), each with the
    /// step that makes it.
    pub fn wanted(&self, quest: u32, phase: u32) -> impl Iterator<Item = (&Step, &Escort)> + '_ {
        self.steps.iter().filter_map(move |(s, e)| {
            let made_at = phase_after(s)?;
            (s.quest == quest && e.wanted_at(made_at, phase)).then_some((s, e))
        })
    }
}
