//! The quest steps and gives that run only for a player untraining a skill: a person who finishes
//! an untraining under way for a price takes the price only from a player who has one under way.

use crate::quests::{Give, Step};

/// The steps and gives that wait on their player having an untraining under way.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UntrainingGates {
    /// Quest steps, by their quest and key.
    pub steps: Vec<Step>,
    /// Gives that move no quest.
    pub gives: Vec<Give>,
}

impl UntrainingGates {
    /// Does no step or give wait on an untraining?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty() && self.gives.is_empty()
    }

    /// Does a step wait on one: the step of its quest and key?
    #[must_use]
    pub fn of_step(&self, step: &Step) -> bool {
        self.steps
            .iter()
            .any(|s| s.quest == step.quest && s.key == step.key)
    }

    /// Does a give wait on one?
    #[must_use]
    pub fn of_give(&self, give: &Give) -> bool {
        self.gives.contains(give)
    }
}
