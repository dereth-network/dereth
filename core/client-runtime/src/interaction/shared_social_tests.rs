use super::*;
use {
    dereth_client_model::weenie::Weenie, dereth_client_model::World,
    dereth_rules::fellowship::Fellow, dereth_rules::fellowship::Fellowship,
    dereth_rules::weenie::bitfield,
};

fn actors() -> (World, Interaction) {
    let mut world = World::new();
    world.player = Some(ObjectId(1));
    for id in 1..=10 {
        let mut row = Weenie::new(ObjectId(id));
        row.pwd.bitfield = bitfield::PLAYER;
        world.tables.weenies.insert(ObjectId(id), row);
    }
    world.fellowship = Some(Fellowship {
        members: (1..=9)
            .map(|id| (ObjectId(id), Fellow::default()))
            .collect(),
        leader: ObjectId(1),
        locked: true,
        ..Default::default()
    });
    (world, Interaction::new())
}

fn deliver(
    world: &mut World,
    interaction: &mut Interaction,
    requests: Vec<UiRequest>,
) -> Vec<Request> {
    interaction.queue(vec![], requests);
    assert!(interaction
        .run_ui_requests(world, false, ServerTime(0.0))
        .is_empty());
    interaction.take_pending_requests()
}

/// Behaviour: fellowship.actions.shared-sender-guards-and-refusals
#[test]
fn direct_social_actions_keep_sender_guards_separate_from_button_availability() {
    let (mut world, mut interaction) = actors();
    let requests = deliver(
        &mut world,
        &mut interaction,
        vec![UiRequest::FellowshipRecruit {
            target: ObjectId(10),
        }],
    );
    assert!(
        matches!(requests.as_slice(), [Request::FellowshipRecruit(r)] if r.target == ObjectId(10)),
        "full locked membership does not add a sender guard"
    );
    let requests = deliver(
        &mut world,
        &mut interaction,
        vec![
            UiRequest::FellowshipRecruit {
                target: ObjectId(1),
            },
            UiRequest::FellowshipRecruit {
                target: ObjectId(2),
            },
            UiRequest::FellowshipDismiss {
                target: ObjectId(10),
            },
            UiRequest::FellowshipDismiss {
                target: ObjectId(1),
            },
            UiRequest::FellowshipAssignNewLeader {
                target: ObjectId(1),
            },
        ],
    );
    assert!(requests.is_empty());
    let lines: Vec<_> = world
        .scroll
        .pending()
        .iter()
        .map(|f| (f.chat_type, f.body.as_str()))
        .collect();
    assert_eq!(
        lines,
        [
            (0x1A, "You can't recruit yourself"),
            (0x1A, "That person is already in your fellowship"),
            (0x1A, "That person is not in your fellowship"),
            (0x1A, "You can't dismiss yourself"),
            (0x1A, "You are already the leader"),
        ]
    );
    assert!(deliver(
        &mut world,
        &mut interaction,
        vec![UiRequest::FellowshipRecruit {
            target: ObjectId(50)
        }]
    )
    .is_empty());
}

/// Behaviour: fellowship.buttons.a-leader-who-leaves-hands-the-lead-on-first
#[test]
fn one_quit_intent_sends_the_leadership_transfer_before_the_quit() {
    let (mut world, mut interaction) = actors();
    let sent = deliver(
        &mut world,
        &mut interaction,
        vec![UiRequest::FellowshipQuit { disband: false }],
    );
    assert!(
        matches!(sent.as_slice(), [Request::FellowshipAssignNewLeader(leader), Request::FellowshipQuit(quit)] if leader.target == ObjectId(2) && quit.disband == 0)
    );
    let sent = deliver(
        &mut world,
        &mut interaction,
        vec![UiRequest::FellowshipQuit { disband: true }],
    );
    assert!(matches!(sent.as_slice(), [Request::FellowshipQuit(quit)] if quit.disband == 1));
}
