//! Guarded objects: a placed object a world's row ties to a camp is usable only while every
//! creature of that camp is dead; a row may hide it while one lives, and take it from the world
//! for a while at each use; and a step may wait on its player having fought the camp's creature.
//! Fixture: the test hall beside where a character starts, a camp of one made-up keeper, and a rift
//! whose use gives a quest.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_content::guards::{GuardTable, Guarded};
use yalain_content::quests::{QuestDef, QuestTable, Step, Trigger, Update};
use yalain_content::{ContentObject, Description, Facts, Kind, Prop, Props};
use yalain_testkit::play::server_call;
use yalain_testkit::Harness;
use yalain_world::calls::{Node, Value};
use yalain_world::quests::phase_of;
use yalain_world::things::Mode;

const HEAR_TELL: u32 = 0x8205_0017;
const USE: u32 = 0x8216_0016;
const HALL: u32 = 0x0299_0109;
const RIFT: u64 = 0x4002_9900_0000_0E81;
const RIFT_T: u32 = 0x4700_2F65;
const KEEPER: u32 = 0x4700_250A;
const QUEST: u32 = 0x4000_0303;
/// What the rift says to a use it refuses.
const REFUSED: (u32, u32) = (0xC844_788A, 0x2500_28EF);
/// How long a use takes it from the world.
const GONE_FOR: f64 = 30.0;

fn rift() -> ContentObject {
    ContentObject {
        object: Some(RIFT),
        template: RIFT_T,
        description: Description {
            name_text: None,
            package: 0x13A4,
            look: 0x1F00_0297,
            enter_fx: 0,
            scale: None,
            name: Some((0xA860_7CEC, 0x2500_28EF)),
            numbers: [0; 6],
            source: "authored for a test".into(),
        },
        cell: HALL,
        origin: [58.0, 241.0, -44.6],
        rotation: [1.0, 0.0, 0.0, 0.0],
        kind: Kind::Npc,
        label: String::new(),
        placed: "authored for a test".into(),
        destination: None,
        facts: Facts::default(),
        walks: None,
        gate: None,
    }
}

fn quest() -> QuestDef {
    QuestDef {
        duration_by_calendar: false,
        id: QUEST,
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

/// The rift's use gives the quest.
fn closes() -> Step {
    Step {
        out_of_turn: false,
        skills: Vec::new(),
        key: "closes".into(),
        quest: QUEST,
        needs: 0,
        trigger: Trigger::Use(RIFT_T),
        tell: None,
        update: Update::Bestow,
        xp: 0,
        gives: Vec::new(),
        counts: Vec::new(),
        effects: Vec::new(),
        text: None,
        recipes: Vec::new(),
        placed: "authored for a test".into(),
        requires: Vec::new(),
        takes: Vec::new(),
        races: Vec::new(),
        effects_last: false,
        effects_quiet: false,
        wants: None,
        fellows: false,
        levels: None,
        xp_share: 0.0,
        to: None,
        transforms: None,
        chance: None,
        to_fellows: false,
    }
}

/// The hall with the rift and its keeper's camp (back `regen` seconds after its death), guarded as
/// the row says; and a player in it: its client and character.
fn hall(hidden: bool, gone_for: Option<f64>, regen: f64) -> (Harness, usize, u64) {
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    let mut k = Props::default();
    k.set("GameplayStatistics_Level", Prop::Int(1))
        .set("GameplayStatistics_MaxHealth", Prop::Int(100))
        .set("AI_MovementType", Prop::Word(0x4000_0001));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(KEEPER, 0x1169, k));
    h.server.spawns.camps.push(yalain_content::Camp {
        id: "keeper".into(),
        cell: HALL,
        origin: [70.0, 250.0, -44.6],
        radius: 0.0,
        population: 1,
        regen,
        templates: vec![(KEEPER, 1)],
        spots: vec![(HALL, [70.0, 250.0, -44.6])],
        placed: "authored for the tests".into(),
        confidence: "authored".into(),
        moves: yalain_content::CampMoves::default(),
        name: None,
    });
    h.server
        .camps
        .push(yalain_world::spawn::CampState::default());
    h.server.content.objects = vec![rift()];
    h.server.content.guards = GuardTable {
        rows: vec![Guarded {
            key: "rift".into(),
            camp: 0,
            objects: vec![0],
            refused: Some(REFUSED),
            hidden,
            gone_for,
        }],
    };
    h.server.quests = QuestTable {
        quests: vec![quest()],
        steps: vec![closes()],
        gives: Vec::new(),
        rooms: Vec::new(),
    };
    let (b, me) = h.player_in_world("one", "Elara");
    h.run(10);
    (h, b, me)
}

/// A use of an object, as the client asks for it.
fn use_of(item: u64, me: u64) -> Arg {
    use yalain_world::graph::{Builder, Slot};
    let mut b = Builder::new();
    b.object(
        0x245,
        &[
            Slot::Ref(None),
            Slot::Ref(None),
            Slot::Long(item),
            Slot::Ref(None),
            Slot::Int(0),
            Slot::Long(me),
            Slot::Int((-1.0f32).to_bits()),
            Slot::Long(0),
            Slot::Int(0),
            Slot::Long(0),
            Slot::Int(0),
            Slot::Ref(None),
            Slot::Int(0),
            Slot::Int(316),
            Slot::Int(0),
            Slot::Int(0),
        ],
    );
    Arg::Object(b.graph())
}

fn run_for(h: &mut Harness, seconds: f64) {
    let t = h.now.0 + seconds;
    while h.now.0 < t {
        h.step();
    }
}

fn send_use(h: &mut Harness, b: usize, me: u64) -> usize {
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &server_call(USE, &[use_of(RIFT, me)]));
    run_for(h, 4.0);
    from
}

/// The lines the client was told since `from`.
fn told(h: &Harness, b: usize, from: usize) -> Vec<u32> {
    h.calls(b, from)
        .into_iter()
        .filter(|(f, _)| *f == HEAR_TELL)
        .filter_map(|(_, v)| {
            v.iter().find_map(|x: &Value| match x.root() {
                Some(Node::TextInfo(t)) => Some(t.string_id),
                _ => None,
            })
        })
        .collect()
}

/// What the client was told of the rift since `from`: made, forgotten.
fn seen(h: &Harness, b: usize, from: usize) -> Vec<&'static str> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::CreateObject(o) if o.id == RIFT => Some("made"),
            ServerMessage::ForgetObject(o) if o.0.id == RIFT => Some("forgotten"),
            _ => None,
        })
        .collect()
}

fn given(h: &Harness) -> bool {
    h.server
        .players
        .values()
        .any(|p| phase_of(&p.play.quests, QUEST) == 1)
}

fn keeper(h: &Harness) -> Option<u64> {
    h.server
        .things
        .by_id
        .values()
        .find(|t| t.template == KEEPER && t.creature.as_ref().is_some_and(|c| c.mode != Mode::Dead))
        .map(|t| t.id)
}

fn kill_keeper(h: &mut Harness) {
    let id = keeper(h).expect("the keeper stands");
    let clock = h.clock();
    yalain_world::death::creature_dies(&mut h.server, &mut h.net, id, 0, clock);
}

/// While the camp's keeper lives, a use of the rift is refused once the player has come to it:
/// the rift says the row's line and its quest step does not run. Once the keeper is dead the use
/// runs the step; once the keeper is back it is refused again.
/// Behaviour: server.places.an-object-works-only-while-its-guards-are-dead
#[test]
fn a_guarded_object_is_refused_while_its_camps_creature_lives_and_works_once_it_is_dead() {
    let (mut h, b, me) = hall(false, None, 60.0);
    assert!(keeper(&h).is_some());
    let from = send_use(&mut h, b, me);
    assert_eq!(told(&h, b, from), [REFUSED.0], "the row's line");
    assert!(!given(&h), "no step ran");

    kill_keeper(&mut h);
    run_for(&mut h, 1.0);
    let from = send_use(&mut h, b, me);
    assert!(!told(&h, b, from).contains(&REFUSED.0));
    assert!(given(&h), "the step ran once the keeper was dead");
    for p in h.server.players.values_mut() {
        p.play.quests.clear();
    }

    run_for(&mut h, 60.0);
    assert!(keeper(&h).is_some(), "the keeper back");
    let from = send_use(&mut h, b, me);
    assert_eq!(told(&h, b, from), [REFUSED.0], "guarded again");
    assert!(!given(&h));
}

/// A row that hides its objects keeps the rift from the world while the keeper lives: it is made
/// for the player as the keeper dies, and forgotten as the keeper comes back. A row that names a
/// time takes the rift from the world at a use, and makes it again once the time is over.
/// Behaviour: server.places.an-object-works-only-while-its-guards-are-dead
#[test]
fn a_hidden_guarded_object_stands_only_while_its_guard_is_dead_and_goes_a_while_at_a_use() {
    let (mut h, b, me) = hall(true, Some(GONE_FOR), 600.0);
    assert!(
        yalain_world::events::content_standing(&h.server, RIFT).is_none(),
        "hidden while the keeper lives"
    );
    let from = h.bots[b].inbox.len();
    kill_keeper(&mut h);
    run_for(&mut h, 1.0);
    assert_eq!(seen(&h, b, from), ["made"], "made as the keeper dies");

    let from = send_use(&mut h, b, me);
    assert!(given(&h), "its step ran");
    assert_eq!(seen(&h, b, from), ["forgotten"], "gone at the use");
    assert!(yalain_world::events::content_standing(&h.server, RIFT).is_none());
    let from = h.bots[b].inbox.len();
    run_for(&mut h, GONE_FOR - 6.0);
    assert!(seen(&h, b, from).is_empty(), "still gone");
    run_for(&mut h, 4.0);
    assert_eq!(seen(&h, b, from), ["made"], "back once its time is over");

    // The keeper back: the rift forgotten again.
    let from = h.bots[b].inbox.len();
    run_for(&mut h, 600.0);
    assert!(keeper(&h).is_some());
    assert_eq!(seen(&h, b, from), ["forgotten"]);
}

/// Kills the keeper, harmed by these characters, each by its share of the harm.
fn kill_keeper_harmed_by(h: &mut Harness, by: &[u64]) {
    let id = keeper(h).expect("the keeper stands");
    if let Some(c) = h
        .server
        .things
        .get_mut(id)
        .and_then(|t| t.creature.as_mut())
    {
        c.taken_by = by.iter().map(|b| (*b, 50)).collect();
    }
    let clock = h.clock();
    yalain_world::death::creature_dies(&mut h.server, &mut h.net, id, by[0], clock);
}

fn given_to(h: &Harness, me: u64) -> bool {
    h.server
        .players
        .values()
        .any(|p| p.character == me && phase_of(&p.play.quests, QUEST) == 1)
}

/// A step waiting on its player having fought a camp's creature runs, once that creature is dead,
/// for a player who harmed it and for a fellow of one, not for another; the keeper killed again by
/// someone else, the credit is theirs alone.
/// Behaviour: server.quests.a-step-waits-on-its-player-having-fought
#[test]
fn a_step_waiting_on_a_fight_runs_only_for_those_who_harmed_the_camps_creature_and_their_fellows() {
    use yalain_content::fought::FoughtGates;
    use yalain_world::fellowship::{Fellow, Fellowship};
    let (mut h, b, me) = hall(true, None, 60.0);
    h.server.content.fought_gates = FoughtGates {
        steps: vec![(closes(), vec![0])],
        gives: Vec::new(),
    };
    let (b2, other) = h.player_in_world("two", "Tomas");
    let (b3, third) = h.player_in_world("three", "Brin");
    h.run(10);
    kill_keeper_harmed_by(&mut h, &[me]);
    run_for(&mut h, 1.0);

    send_use(&mut h, b2, other);
    assert!(!given_to(&h, other), "not for one who did not fight");
    send_use(&mut h, b, me);
    assert!(given_to(&h, me), "for the one who harmed it");
    h.server.fellowships.list.push(Fellowship {
        name: "Fighters".into(),
        leader: me,
        flags: 0,
        room: 1,
        fellows: [me, other]
            .iter()
            .map(|&id| Fellow {
                id,
                joined: 0.0,
                sent: yalain_world::fellowship::Vitals::default(),
                next_check: f64::INFINITY,
            })
            .collect(),
        last_claimant: 0,
    });
    send_use(&mut h, b2, other);
    assert!(given_to(&h, other), "for a fellow of one who harmed it");

    // Back and killed by the third alone: the credit is the third's.
    for p in h.server.players.values_mut() {
        p.play.quests.clear();
    }
    run_for(&mut h, 60.0);
    kill_keeper_harmed_by(&mut h, &[third]);
    run_for(&mut h, 1.0);
    send_use(&mut h, b, me);
    assert!(!given_to(&h, me), "the earlier fight counts no more");
    send_use(&mut h, b3, third);
    assert!(given_to(&h, third));
}
