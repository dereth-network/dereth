//! The quest steps and gives that run only for heroes: a raid given only to those who may use what
//! it gives.

use crate::quests::{Give, Step};

/// The steps and gives that wait on their player being a hero.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HeroGates {
    /// Quest steps, by their quest and key.
    pub steps: Vec<Step>,
    /// Gives that move no quest.
    pub gives: Vec<Give>,
}

impl HeroGates {
    /// Does no step or give wait on a hero?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty() && self.gives.is_empty()
    }

    /// Does a step run only for heroes: the step of its quest and key?
    #[must_use]
    pub fn of_step(&self, step: &Step) -> bool {
        self.steps
            .iter()
            .any(|s| s.quest == step.quest && s.key == step.key)
    }

    /// Does a give take its item only from heroes?
    #[must_use]
    pub fn of_give(&self, give: &Give) -> bool {
        self.gives.contains(give)
    }
}
