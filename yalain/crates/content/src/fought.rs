//! The quest steps and gives that wait on their player having fought a camp's creature: a kill
//! whose credit goes to everyone who fought it, not only to whoever the kill went to.

use crate::quests::{Give, Step};

/// The steps and gives that wait on their player (or a fellow) having harmed a camp's last
/// creature to die, each with the camps it names, by their rows in the spawn table.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FoughtGates {
    /// Quest steps, by their quest and key.
    pub steps: Vec<(Step, Vec<u32>)>,
    /// Gives that move no quest.
    pub gives: Vec<(Give, Vec<u32>)>,
}

impl FoughtGates {
    /// Does no step or give wait on a fight?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty() && self.gives.is_empty()
    }

    /// The camps a step waits on its player having fought: the step of its quest and key.
    #[must_use]
    pub fn of_step(&self, step: &Step) -> Option<&[u32]> {
        self.steps
            .iter()
            .find(|(s, _)| s.quest == step.quest && s.key == step.key)
            .map(|(_, c)| c.as_slice())
    }

    /// The camps a give waits on its player having fought.
    #[must_use]
    pub fn of_give(&self, give: &Give) -> Option<&[u32]> {
        self.gives
            .iter()
            .find(|(g, _)| g == give)
            .map(|(_, c)| c.as_slice())
    }
}
