//! Shared social control availability over the current character and selection.

use crate::{AllegianceRoster, FellowshipView, GameView};
use dereth_primitives::ObjectId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FellowshipControls {
    pub leads: bool,
    /// An interface may enter its character-picking gesture.
    pub choose_recruit: bool,
    pub choose_member: bool,
    /// None preserves the existing control state until a known player is selected.
    pub recruit: Option<bool>,
    pub selected_member: bool,
}

#[must_use]
pub fn fellowship_controls(
    view: &dyn GameView,
    fellowship: &FellowshipView,
    selected_fellow: Option<ObjectId>,
) -> FellowshipControls {
    let player = view.player();
    let leads = player.is_some() && player == Some(fellowship.leader);
    let may_invite = leads || fellowship.open_fellow;
    let full = fellowship.members.len() >= 9;
    let selection = view.selected_object();
    let known_player = selection.is_some_and(|id| {
        view.selection_query_facts(id)
            .is_some_and(|facts| facts.is_player)
    });
    let selected_is_member =
        selection.is_some_and(|id| fellowship.members.iter().any(|member| member.id == id));
    let recruit = if !may_invite {
        Some(false)
    } else if !leads && !known_player {
        None
    } else {
        Some(known_player && !selected_is_member && !full)
    };
    FellowshipControls {
        leads,
        choose_recruit: may_invite && !full,
        choose_member: leads
            && fellowship
                .members
                .iter()
                .any(|member| Some(member.id) != player),
        recruit,
        selected_member: leads
            && selected_fellow.is_some_and(|id| {
                Some(id) != player && fellowship.members.iter().any(|member| member.id == id)
            }),
    }
}

#[must_use]
pub fn swear_target(view: &dyn GameView, roster: &AllegianceRoster) -> Option<ObjectId> {
    if roster.patron.is_some()
        || view.available_experience().max(0) < i64::from(view.oath_xp_cost().unwrap_or(0))
    {
        return None;
    }
    view.selected_object().filter(|id| {
        Some(*id) != view.player()
            && !view.allegiance_has_member(*id)
            && view
                .selection_query_facts(*id)
                .is_some_and(|facts| facts.is_player)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AllegianceEntry, FellowEntry, SelectionQueryFacts};

    #[derive(Debug)]
    struct View {
        selection: Option<ObjectId>,
        known_player: bool,
        cost: Option<u32>,
        experience: i64,
        allegiance_member: bool,
    }
    impl Default for View {
        fn default() -> Self {
            Self {
                selection: Some(ObjectId(100)),
                known_player: true,
                cost: None,
                experience: 0,
                allegiance_member: false,
            }
        }
    }
    impl GameView for View {
        fn player(&self) -> Option<ObjectId> {
            Some(ObjectId(1))
        }
        fn selected_object(&self) -> Option<ObjectId> {
            self.selection
        }
        fn selection_query_facts(&self, _: ObjectId) -> Option<SelectionQueryFacts> {
            self.known_player.then_some(SelectionQueryFacts {
                is_player: true,
                ..Default::default()
            })
        }
        fn oath_xp_cost(&self) -> Option<u32> {
            self.cost
        }
        fn available_experience(&self) -> i64 {
            self.experience
        }
        fn allegiance_has_member(&self, _: ObjectId) -> bool {
            self.allegiance_member
        }
    }
    fn fellowship(count: u32, leader: u32, open: bool) -> FellowshipView {
        FellowshipView {
            leader: ObjectId(leader),
            open_fellow: open,
            members: (1..=count)
                .map(|id| FellowEntry {
                    id: ObjectId(id),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
    }

    /// Behaviour: fellowship.buttons.recruit-follows-the-world-selection-and-not-the-list
    #[test]
    fn recruit_availability_uses_fullness_and_openness_without_a_locked_veto() {
        for (leader, open, count, expected) in [
            (1, false, 8, true),
            (1, true, 8, true),
            (1, false, 9, false),
            (1, true, 9, false),
            (2, false, 8, false),
            (2, true, 8, true),
            (2, false, 9, false),
            (2, true, 9, false),
        ] {
            for locked in [false, true] {
                let mut fellowship = fellowship(count, leader, open);
                fellowship.locked = locked;
                let controls =
                    fellowship_controls(&View::default(), &fellowship, Some(ObjectId(2)));
                assert_eq!(
                    controls.recruit,
                    Some(expected),
                    "{leader}/{open}/{count}/{locked}"
                );
                assert_eq!(controls.choose_recruit, expected);
            }
        }
        let mut view = View {
            selection: Some(ObjectId(2)),
            ..Default::default()
        };
        assert_eq!(
            fellowship_controls(&view, &fellowship(2, 1, false), None).recruit,
            Some(false)
        );
        view.selection = Some(ObjectId(1));
        assert_eq!(
            fellowship_controls(&view, &fellowship(2, 1, false), None).recruit,
            Some(false)
        );
        view.known_player = false;
        assert_eq!(
            fellowship_controls(&view, &fellowship(2, 1, true), None).recruit,
            Some(false)
        );
        assert_eq!(
            fellowship_controls(&view, &fellowship(2, 2, true), None).recruit,
            None
        );
        assert_eq!(
            fellowship_controls(&view, &fellowship(2, 2, false), None).recruit,
            Some(false)
        );
    }

    /// Behaviour: fellowship.buttons.disband-is-offered-only-to-the-leader
    #[test]
    fn member_controls_require_leadership_and_a_current_nonself_member() {
        let view = View::default();
        let party = fellowship(2, 1, false);
        let controls = fellowship_controls(&view, &party, Some(ObjectId(2)));
        assert!(controls.leads && controls.choose_member && controls.selected_member);
        for selection in [None, Some(ObjectId(1)), Some(ObjectId(99))] {
            assert!(!fellowship_controls(&view, &party, selection).selected_member);
        }
        let controls = fellowship_controls(&view, &fellowship(2, 2, true), Some(ObjectId(1)));
        assert!(!controls.leads && !controls.choose_member && !controls.selected_member);
        assert!(!fellowship_controls(&view, &fellowship(1, 1, false), None).choose_member);
    }

    /// Behaviour: allegiance.buttons.swear-is-dark-while-the-player-already-has-a-patron
    #[test]
    fn an_oath_requires_a_valid_outside_player_and_enough_unassigned_experience() {
        let mut view = View {
            cost: Some(10),
            experience: 9,
            ..Default::default()
        };
        let mut roster = AllegianceRoster::default();
        assert_eq!(swear_target(&view, &roster), None);
        view.experience = 10;
        assert_eq!(swear_target(&view, &roster), Some(ObjectId(100)));
        roster.patron = Some(AllegianceEntry::default());
        assert_eq!(swear_target(&view, &roster), None);
        roster.patron = None;
        view.allegiance_member = true;
        assert_eq!(swear_target(&view, &roster), None);
        view.allegiance_member = false;
        view.selection = Some(ObjectId(1));
        assert_eq!(swear_target(&view, &roster), None);
        view.selection = Some(ObjectId(100));
        view.known_player = false;
        assert_eq!(swear_target(&view, &roster), None);
        view.known_player = true;
        view.cost = None;
        view.experience = -1;
        assert_eq!(swear_target(&view, &roster), Some(ObjectId(100)));
    }
}
