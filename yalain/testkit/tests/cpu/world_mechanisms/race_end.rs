//! An event a quest's completion ends: the first player to complete it holds what the event stood
//! for, and the event stays off for the rest of its window.

use auberean_protocol::ServerMessage;
use yalain_content::events::{Event, EventTable, ObjectGate, Window};
use yalain_content::quests::{QuestTable, Update};
use yalain_testkit::Harness;

use super::common::{
    object, phase, quest, run_to_unix, step, under_way, use_it, OBJECT, QUEST, START,
};

/// An event `race`, on in two windows, gating the object and the quest's last step (a use of the
/// object completes it), and ended by the quest's completion; two players under way at that step.
fn race() -> (Harness, usize, u64, u64) {
    let mut h = Harness::new();
    h.server.content.objects = vec![object()];
    let finish = step(1, Update::Complete);
    h.server.quests = QuestTable {
        quests: vec![quest()],
        steps: vec![finish.clone()],
        ..QuestTable::default()
    };
    h.server.content.events = EventTable {
        events: vec![Event {
            key: "race".into(),
            game_event: None,
            windows: vec![
                Window::Dated {
                    from: START,
                    until: START + 600,
                },
                Window::Dated {
                    from: START + 1000,
                    until: START + 1600,
                },
            ],
            on_players: Vec::new(),
            failure: None,
            hides: Vec::new(),
            ended_by: vec![QUEST],
        }],
        camps: Vec::new(),
        objects: vec![ObjectGate {
            object: 0,
            event: "race".into(),
        }],
        steps: vec![(finish, "race".into())],
        lines: Vec::new(),
    };
    let (b, first) = h.player_in_world("one", "Elara");
    let (_, second) = h.player_in_world("two", "Babyfare");
    under_way(&mut h, first, 1);
    under_way(&mut h, second, 1);
    run_to_unix(&mut h, START);
    (h, b, first, second)
}

fn forgotten(h: &Harness, b: usize, from: usize) -> bool {
    h.world_messages(b, from)
        .iter()
        .any(|m| matches!(m, ServerMessage::ForgetObject(o) if o.0.id == OBJECT))
}

/// The first player to complete the quest ends the event at once: the object it gates is
/// forgotten, and the second player's use, in the same moment, completes nothing. The event stays
/// off to the end of its window, and comes on with its next.
/// Behaviour: server.events.ended-by-a-quests-completion
#[test]
fn the_first_to_complete_a_quest_that_ends_an_event_ends_it_for_everyone() {
    let (mut h, b, first, second) = race();
    assert!(yalain_world::events::is_on(&h.server, "race"));
    let from = h.bots[b].inbox.len();
    use_it(&mut h, first);
    assert_eq!(phase(&h, first), 0, "the first completes it");
    assert!(yalain_world::quests::done(
        &h.server.players[&super::common::world_of(&h, first)]
            .play
            .quests,
        QUEST
    ));
    assert!(
        !yalain_world::events::is_on(&h.server, "race"),
        "ended at once"
    );
    assert!(forgotten(&h, b, from), "its object is forgotten");
    use_it(&mut h, second);
    assert_eq!(phase(&h, second), 1, "the second completes nothing");
    run_to_unix(&mut h, START + 900);
    assert!(
        !yalain_world::events::is_on(&h.server, "race"),
        "off for the rest of its window"
    );
    run_to_unix(&mut h, START + 1000);
    assert!(
        yalain_world::events::is_on(&h.server, "race"),
        "on again with its next window"
    );
    use_it(&mut h, second);
    assert_eq!(phase(&h, second), 0, "the second completes it in the next");
}

/// A completion while the event is off ends nothing: the event comes on with its window.
/// Behaviour: server.events.ended-by-a-quests-completion
#[test]
fn a_completion_while_the_event_is_off_changes_nothing() {
    let (mut h, _, first, _) = race();
    run_to_unix(&mut h, START + 700);
    let world = super::common::world_of(&h, first);
    let clock = h.clock();
    yalain_world::quests::apply(
        &mut h.server,
        &mut h.net,
        world,
        QUEST,
        Update::Complete,
        0,
        clock,
    );
    run_to_unix(&mut h, START + 1000);
    assert!(yalain_world::events::is_on(&h.server, "race"));
}
