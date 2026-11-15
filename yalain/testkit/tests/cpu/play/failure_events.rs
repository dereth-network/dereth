//! Events a quest's failure starts: as the quest fails for any player the event comes on for
//! everyone, keeping its person from the world and filling its camp, until the camp's creature is
//! dead or its time has passed. Fixture: the test world's camp of one beetle, gated by the event,
//! one static object beside the players that the event hides, a quest of the world's and two
//! players in it.

use auberean_protocol::ServerMessage;
use yalain_content::events::{Event, EventTable, Failure};
use yalain_content::quests::{QuestDef, QuestTable, Update};
use yalain_content::{ContentObject, Description, Facts, Kind};
use yalain_testkit::Harness;
use yalain_world::calls::Value;
use yalain_world::state::QuestState;

const OBJECT: u64 = 0x4002_9900_0000_0E21;
const TEMPLATE: u32 = 0x4700_1154;
const CELL: u32 = 0x0299_0109;
const ORIGIN: [f32; 3] = [61.0, 241.0, -44.6];
const QUEST: u32 = 0x4000_0E21;
const OTHER: u32 = 0x4000_0E22;

fn person() -> ContentObject {
    ContentObject {
        object: Some(OBJECT),
        template: TEMPLATE,
        description: Description {
            name_text: None,
            package: 0x13C1,
            look: 0x1F00_0ED5,
            enter_fx: 0,
            scale: None,
            name: None,
            numbers: [0; 6],
            source: "authored for a test".into(),
        },
        cell: CELL,
        origin: ORIGIN,
        rotation: [1.0, 0.0, 0.0, 0.0],
        kind: Kind::Static,
        label: String::new(),
        placed: "authored for a test".into(),
        destination: None,
        facts: Facts::default(),
        walks: None,
        gate: None,
    }
}

fn quest(id: u32) -> QuestDef {
    QuestDef {
        duration_by_calendar: false,
        id,
        name: yalain_content::Words::Ref(0xA860_7CEC, 0x2500_28F0),
        description: (0x6C45_9364, 0x2500_28F0),
        icon: 0x4100_034B,
        play_fx: true,
        journals: vec![(0x2E5C_1E52, 0x2500_28F0)],
        challenge: -999,
        duration: 0.0,
        timed_phase: 0,
        retry: 0.0,
        xp: 0,
        cancellable: true,
        placed: "authored for a test".into(),
        hidden: false,
        effects: Vec::new(),
        subquests: Vec::new(),
        min_level: 0,
        fails_on_death: false,
        fails_on_logout: false,
        fails_on_teleport: false,
        once: false,
        fellows: yalain_content::quests::Fellows::default(),
        ranges: Vec::new(),
        hero_only: false,
        pay: yalain_content::quests::CompletionPay::default(),
        required: None,
        cancel_wait: None,
    }
}

/// The world with an event `turned` that the quest's failure starts, for `lasts` or until the
/// camp's beetle is dead (`until_killed`); it hides the person and keeps the camp. Two players in
/// it, each under way at the quest and at another: their clients and characters.
fn world(lasts: Option<u32>, until_killed: bool) -> (Harness, [(usize, u64); 2]) {
    let mut h = Harness::new();
    h.add_creatures(1);
    h.server.content.objects = vec![person()];
    h.server.content.events = EventTable {
        events: vec![Event {
            ended_by: Vec::new(),
            key: "turned".into(),
            game_event: None,
            windows: Vec::new(),
            on_players: Vec::new(),
            failure: Some(Failure {
                quest: QUEST,
                lasts,
                until_killed: until_killed.then_some(0),
            }),
            hides: vec![0],
        }],
        camps: vec![("test".into(), "turned".into())],
        objects: Vec::new(),
        steps: Vec::new(),
        lines: Vec::new(),
    };
    h.server.quests = QuestTable {
        quests: vec![quest(QUEST), quest(OTHER)],
        steps: Vec::new(),
        gives: Vec::new(),
        rooms: Vec::new(),
    };
    let one = h.player_in_world("one", "Elara");
    let two = h.player_in_world("two", "Babyfare");
    for (_, me) in [one, two] {
        let world = world_of(&h, me);
        if let Some(p) = h.server.players.get_mut(&world) {
            for q in [QUEST, OTHER] {
                p.play.quests.push(QuestState {
                    quest: q,
                    status: yalain_world::quests::status::UNDERWAY,
                    phase: 1,
                    started: 0,
                    updated: 0,
                });
            }
        }
    }
    run_for(&mut h, 2.0);
    (h, [one, two])
}

fn world_of(h: &Harness, me: u64) -> yalain_net::ConnId {
    *h.server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in the world")
        .0
}

fn run_for(h: &mut Harness, seconds: f64) {
    let t = h.now.0 + seconds;
    while h.now.0 < t {
        h.step();
    }
}

/// Run the clock on to `unix`, a second at a time.
fn run_to_unix(h: &mut Harness, unix: i64) {
    while h.unix < unix {
        h.step_by(1.0);
    }
    run_for(h, 1.0);
}

fn on(h: &Harness) -> bool {
    yalain_world::events::is_on(&h.server, "turned")
}

fn stands(h: &Harness) -> bool {
    yalain_world::events::content_standing(&h.server, OBJECT).is_some()
}

fn beetles(h: &Harness) -> usize {
    h.server
        .things
        .by_id
        .values()
        .filter(|t| {
            t.creature
                .as_ref()
                .is_some_and(|c| c.mode != yalain_world::things::Mode::Dead)
        })
        .count()
}

/// What a client was told of the person since `from`: made, forgotten.
fn seen(h: &Harness, b: usize, from: usize) -> Vec<&'static str> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::CreateObject(o) if o.id == OBJECT => Some("made"),
            ServerMessage::ForgetObject(o) if o.0.id == OBJECT => Some("forgotten"),
            _ => None,
        })
        .collect()
}

fn fail(h: &mut Harness, me: u64, quest: u32) {
    let world = world_of(h, me);
    let clock = h.clock();
    yalain_world::quests::fail(&mut h.server, &mut h.net, world, quest, clock);
}

fn kill_beetle(h: &mut Harness) {
    let id = h
        .server
        .things
        .by_id
        .values()
        .find(|t| {
            t.creature
                .as_ref()
                .is_some_and(|c| c.mode != yalain_world::things::Mode::Dead)
        })
        .map(|t| t.id)
        .expect("the beetle stands");
    let clock = h.clock();
    yalain_world::death::creature_dies(&mut h.server, &mut h.net, id, 0, clock);
}

/// Give a player the quest under way again.
fn under_way(h: &mut Harness, me: u64) {
    let world = world_of(h, me);
    if let Some(p) = h.server.players.get_mut(&world) {
        p.play.quests.retain(|q| q.quest != QUEST);
        p.play.quests.push(QuestState {
            quest: QUEST,
            status: yalain_world::quests::status::UNDERWAY,
            phase: 1,
            started: 0,
            updated: 0,
        });
    }
}

/// Before the quest fails the person stands and the camp keeps no beetle. As it fails for one
/// player the event comes on for everyone: the person is forgotten by both players' clients, a use
/// of it or a give to it finds nothing, and the camp fills. As the beetle dies the event goes off:
/// the person is made again for both and the camp is empty. A later failure, by the other player,
/// turns it on again.
/// Behaviour: server.events.a-quests-failure-starts-an-event
#[test]
fn a_quests_failure_turns_its_event_on_for_everyone_hiding_its_person_until_the_camp_is_killed() {
    let (mut h, [(b1, me1), (b2, me2)]) = world(None, true);
    assert!(!on(&h));
    assert!(stands(&h), "the person stands before");
    assert_eq!(beetles(&h), 0, "the camp waits on the event");

    let (f1, f2) = (h.bots[b1].inbox.len(), h.bots[b2].inbox.len());
    fail(&mut h, me1, QUEST);
    run_for(&mut h, 2.0);
    assert!(on(&h), "on at the step after the failure");
    assert!(!stands(&h), "the person is kept from the world");
    assert_eq!(seen(&h, b1, f1), ["forgotten"]);
    assert_eq!(seen(&h, b2, f2), ["forgotten"], "for the other player too");
    assert_eq!(beetles(&h), 1, "the camp fills");

    run_for(&mut h, 120.0);
    assert!(on(&h), "nothing but the beetle's death ends it");

    let (f1, f2) = (h.bots[b1].inbox.len(), h.bots[b2].inbox.len());
    kill_beetle(&mut h);
    run_for(&mut h, 2.0);
    assert!(!on(&h), "off once the camp's creature is dead");
    assert!(stands(&h));
    assert_eq!(seen(&h, b1, f1), ["made"]);
    assert_eq!(seen(&h, b2, f2), ["made"]);
    assert_eq!(beetles(&h), 0);
    run_for(&mut h, 120.0);
    assert_eq!(beetles(&h), 0, "the camp stays empty while it is off");

    fail(&mut h, me2, QUEST);
    run_for(&mut h, 2.0);
    assert!(on(&h), "another failure turns it on again");
    assert!(!stands(&h));
    assert_eq!(beetles(&h), 1);
}

/// An event a failure starts for a time goes off once that time has passed; a failure while it is
/// on, by another player, changes nothing, and once it is off a failure starts it again.
/// Behaviour: server.events.a-quests-failure-starts-an-event
#[test]
fn an_event_a_failure_starts_goes_off_once_its_time_has_passed_and_a_failure_while_on_changes_nothing(
) {
    let (mut h, [(_, me1), (_, me2)]) = world(Some(120), false);
    let start = h.unix;
    fail(&mut h, me1, QUEST);
    run_for(&mut h, 2.0);
    assert!(on(&h));
    run_to_unix(&mut h, start + 60);
    fail(&mut h, me2, QUEST);
    run_to_unix(&mut h, start + 110);
    assert!(on(&h));
    run_to_unix(&mut h, start + 121);
    assert!(
        !on(&h),
        "off at its time, not put back by the later failure"
    );
    assert!(stands(&h));
    assert_eq!(beetles(&h), 0, "its camp's beetle leaves the world with it");

    under_way(&mut h, me1);
    fail(&mut h, me1, QUEST);
    run_for(&mut h, 2.0);
    assert!(on(&h), "a failure once it is off starts it again");
}

/// A quest completed or cancelled starts no event, nor does another quest's failure, nor a failure
/// of a quest the player does not hold under way.
/// Behaviour: server.events.a-quests-failure-starts-an-event
#[test]
fn a_quest_completed_or_cancelled_or_another_quests_failure_starts_no_event() {
    let (mut h, [(_, me1), (_, me2)]) = world(Some(600), true);
    fail(&mut h, me1, OTHER);
    run_for(&mut h, 2.0);
    assert!(!on(&h), "another quest's failure");

    let world = world_of(&h, me1);
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
    run_for(&mut h, 2.0);
    assert!(!on(&h), "a completion");
    fail(&mut h, me1, QUEST);
    run_for(&mut h, 2.0);
    assert!(!on(&h), "a failure of a quest not under way");

    let world = world_of(&h, me2);
    let clock = h.clock();
    yalain_world::quests::cancel(
        &mut h.server,
        &mut h.net,
        world,
        &[Value::Int(QUEST)],
        clock,
    );
    run_for(&mut h, 2.0);
    assert!(!on(&h), "a cancel");
    assert!(stands(&h));
}
