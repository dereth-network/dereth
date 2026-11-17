//! A step that answers a use out of turn: a use of a quest's stop at a phase other than its own puts
//! the step's effect on and moves no quest.

use yalain_content::quests::{QuestTable, Update};
use yalain_testkit::{Harness, COURAGE_EFFECT};

use super::common::{phase, quest, step, under_way, use_it, world_of};

/// The quest's stop: at its first phase a use moves the quest on to its second; at any other phase under way, the
/// out-of-turn step puts the effect on. A player in the world.
fn course() -> (Harness, u64) {
    let mut h = Harness::new();
    h.server.content.objects = vec![super::common::object()];
    let flung = yalain_content::quests::Step {
        out_of_turn: true,
        skills: Vec::new(),
        effects: vec![COURAGE_EFFECT],
        ..step(1, Update::Nothing)
    };
    h.server.quests = QuestTable {
        quests: vec![quest()],
        steps: vec![step(1, Update::Advance(2)), flung],
        ..QuestTable::default()
    };
    let (_, me) = h.player_in_world("one", "Elara");
    h.run(5);
    (h, me)
}

fn holds_effect(h: &Harness, me: u64) -> bool {
    h.server.players[&world_of(h, me)]
        .play
        .effects
        .iter()
        .any(|e| e.state == COURAGE_EFFECT)
}

/// Used in turn, at its own phase, the stop moves the quest on and the out-of-turn step does not
/// run.
/// Behaviour: server.quests.step-out-of-turn
#[test]
fn a_stop_used_in_turn_moves_the_quest_on() {
    let (mut h, me) = course();
    under_way(&mut h, me, 1);
    use_it(&mut h, me);
    assert_eq!(phase(&h, me), 2);
    assert!(!holds_effect(&h, me));
}

/// Used out of turn, the quest at another phase, the out-of-turn step puts its effect on and the
/// quest stays where it was; with the quest not under way, nothing runs.
/// Behaviour: server.quests.step-out-of-turn
#[test]
fn a_stop_used_out_of_turn_answers_with_its_effect_and_moves_nothing() {
    let (mut h, me) = course();
    use_it(&mut h, me);
    assert_eq!(phase(&h, me), 0);
    assert!(!holds_effect(&h, me), "nothing for a player not racing");
    under_way(&mut h, me, 3);
    use_it(&mut h, me);
    assert_eq!(phase(&h, me), 3, "the quest stays where it was");
    assert!(holds_effect(&h, me), "the effect put on");
}
