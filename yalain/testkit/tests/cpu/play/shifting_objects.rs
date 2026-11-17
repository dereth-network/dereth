//! Shifting objects: of the placed objects a world's row names one stands at a time; a use of it
//! that runs its quest step takes it from the world and another stands in its place, at once or
//! after the row's time, and one left unused its time moves on too. Fixture: the test hall beside
//! where a character starts, three made-up remains of one template whose use gives a quest.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_content::quests::{QuestDef, QuestTable, Step, Trigger, Update};
use yalain_content::shifting::{ShiftTable, Shifting};
use yalain_content::{ContentObject, Description, Facts, Kind};
use yalain_testkit::play::server_call;
use yalain_testkit::Harness;
use yalain_world::quests::phase_of;

const USE: u32 = 0x8216_0016;
const HALL: u32 = 0x0299_0109;
const REMAINS_T: u32 = 0x4700_12FC;
const QUEST: u32 = 0x4000_033B;
/// The remains' ids and places.
const REMAINS: [(u64, [f32; 3]); 3] = [
    (0x4002_9900_0000_0E91, [58.0, 241.0, -44.6]),
    (0x4002_9900_0000_0E92, [59.0, 242.0, -44.6]),
    (0x4002_9900_0000_0E93, [57.0, 240.0, -44.6]),
];

fn remains(id: u64, origin: [f32; 3]) -> ContentObject {
    ContentObject {
        object: Some(id),
        template: REMAINS_T,
        description: Description {
            name_text: None,
            package: 0x13A4,
            look: 0x1F00_0297,
            enter_fx: 0,
            scale: None,
            name: Some((0xA860_7CEC, 0x2500_1F0A)),
            numbers: [0; 6],
            source: "authored for a test".into(),
        },
        cell: HALL,
        origin,
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

fn quest() -> QuestDef {
    QuestDef {
        duration_by_calendar: false,
        id: QUEST,
        name: yalain_content::Words::Ref(0xA860_7CEC, 0x2500_1F0B),
        description: (0x6C45_9364, 0x2500_1F0B),
        icon: 0x4100_034B,
        play_fx: true,
        journals: vec![(0x2E5C_1E52, 0x2500_1F0B)],
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

/// A use of the remains gives the quest.
fn searched() -> Step {
    Step {
        out_of_turn: false,
        skills: Vec::new(),
        key: "searched".into(),
        quest: QUEST,
        needs: 0,
        trigger: Trigger::Use(REMAINS_T),
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

/// The hall with the three remains, shifting as the row says, and a player in it: its client and
/// character.
fn hall(gone_for: Option<f64>, moves_after: Option<f64>) -> (Harness, usize, u64) {
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    h.server.content.objects = REMAINS.iter().map(|(id, at)| remains(*id, *at)).collect();
    h.server.content.shifting = ShiftTable {
        rows: vec![Shifting {
            key: "remains".into(),
            objects: vec![0, 1, 2],
            gone_for,
            moves_after,
        }],
    };
    h.server.quests = QuestTable {
        quests: vec![quest()],
        steps: vec![searched()],
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

fn send_use(h: &mut Harness, b: usize, me: u64, item: u64) -> usize {
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &server_call(USE, &[use_of(item, me)]));
    run_for(h, 4.0);
    from
}

/// The remains standing in the world now, by id.
fn standing(h: &Harness) -> Vec<u64> {
    REMAINS
        .iter()
        .map(|(id, _)| *id)
        .filter(|id| yalain_world::events::content_standing(&h.server, *id).is_some())
        .collect()
}

/// What the client was told of the remains since `from`, in order: each made or forgotten.
fn seen(h: &Harness, b: usize, from: usize) -> Vec<(&'static str, u64)> {
    let ours = |id: u64| REMAINS.iter().any(|(r, _)| *r == id);
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::CreateObject(o) if ours(o.id) => Some(("made", o.id)),
            ServerMessage::ForgetObject(o) if ours(o.0.id) => Some(("forgotten", o.0.id)),
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

fn forget_quests(h: &mut Harness) {
    for p in h.server.players.values_mut() {
        p.play.quests.clear();
    }
}

/// Of the row's three remains one stands, the only one the player is told of. A use of it that
/// runs its step takes it from the world and another, never the same, stands at once; a use that
/// runs no step leaves it where it lies; a use of one that does not stand does nothing.
/// Behaviour: server.places.an-object-turns-up-elsewhere-once-used
#[test]
fn one_of_a_shifting_rows_objects_stands_and_a_use_that_runs_its_step_moves_it_elsewhere() {
    let (mut h, b, me) = hall(None, None);
    let first = standing(&h);
    assert_eq!(first.len(), 1, "one stands");
    let told: Vec<u64> = seen(&h, b, 0)
        .into_iter()
        .filter(|(what, _)| *what == "made")
        .map(|(_, id)| id)
        .collect();
    assert_eq!(told, first, "the player is told of that one alone");

    let mut at = first[0];
    for _ in 0..4 {
        let from = send_use(&mut h, b, me, at);
        assert!(given(&h), "its step ran");
        let now = standing(&h);
        assert_eq!(now.len(), 1, "one stands after the use");
        assert_ne!(now[0], at, "somewhere else");
        assert_eq!(
            seen(&h, b, from),
            [("forgotten", at), ("made", now[0])],
            "gone, and the next made"
        );
        // Used again with the quest under way, its step does not run: it stays.
        let from = send_use(&mut h, b, me, now[0]);
        assert_eq!(standing(&h), now, "a use that runs no step leaves it");
        assert!(seen(&h, b, from).is_empty());
        forget_quests(&mut h);
        at = now[0];
    }

    // One that does not stand does nothing, whatever a client asks.
    let hidden = REMAINS
        .iter()
        .map(|(id, _)| *id)
        .find(|id| *id != at)
        .expect("another");
    send_use(&mut h, b, me, hidden);
    assert!(!given(&h), "no step ran");
    assert_eq!(standing(&h), [at]);
}

/// A row with a time gone stands none for that long after a use, then another; one with a time it
/// stands moves an unused one on after it.
/// Behaviour: server.places.an-object-turns-up-elsewhere-once-used
#[test]
fn a_shifting_object_comes_back_elsewhere_after_its_time_gone_and_moves_on_unused() {
    let (mut h, b, me) = hall(Some(30.0), Some(120.0));
    let first = standing(&h);
    assert_eq!(first.len(), 1);
    send_use(&mut h, b, me, first[0]);
    assert!(given(&h));
    assert!(standing(&h).is_empty(), "none stands while it is gone");
    run_for(&mut h, 30.0 - 6.0);
    assert!(standing(&h).is_empty(), "still gone");
    run_for(&mut h, 4.0);
    let next = standing(&h);
    assert_eq!(next.len(), 1, "back once its time is over");
    assert_ne!(next[0], first[0], "somewhere else");

    let from = h.bots[b].inbox.len();
    run_for(&mut h, 120.0 - 2.0);
    assert_eq!(standing(&h), next, "unused, it lies a while");
    run_for(&mut h, 4.0);
    let moved = standing(&h);
    assert_eq!(moved.len(), 1, "moved on at once");
    assert_ne!(moved, next);
    assert_eq!(
        seen(&h, b, from),
        [("forgotten", next[0]), ("made", moved[0])]
    );
}

/// A player's report of its place, at rest where it stands, under a walk's stamp.
fn report(
    at: yalain_store::StoredPosition,
    move_to_stamp: u16,
) -> auberean_protocol::ClientMessage {
    auberean_protocol::ClientMessage::Position(auberean_protocol::physics::ClientPosition {
        time: 0.0,
        cell: at.cell,
        origin: auberean_primitives::Vec3::new(at.origin[0], at.origin[1], at.origin[2]),
        heading_motion: yalain_world::world::IDLE_MOTION,
        flags: 0,
        instance_stamp: 1,
        teleport_stamp: 0,
        force_position_stamp: 0,
        move_to_stamp,
        jump_velocity: auberean_primitives::Vec3::ZERO,
    })
}

/// A player walking to the one that stands when another's use moves it on finds nothing there:
/// come to where it lay, its use is answered and its step does not run.
/// Behaviour: server.places.an-object-turns-up-elsewhere-once-used
#[test]
fn a_player_who_comes_to_a_shifting_object_after_it_moved_on_finds_nothing_done() {
    let (mut h, a, ida) = hall(None, None);
    let (b, idb) = h.player_in_world("two", "Tysis");
    h.run(10);
    let wb = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == idb)
        .expect("in the world")
        .0;
    h.server.players.get_mut(&wb).expect("in").position.origin[1] += 15.0;
    let first = standing(&h)[0];
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(USE, &[use_of(first, idb)]),
    );
    h.run(10);
    let stamp = h
        .world_messages(b, from)
        .iter()
        .find_map(|m| match m {
            ServerMessage::MoveTo(o) if o.sender.id == idb => Some(o.action.stamp),
            _ => None,
        })
        .expect("walked toward it");
    send_use(&mut h, a, ida, first);
    assert_ne!(standing(&h), [first], "moved on at the other's use");
    let mut there = h.server.players[&wb].position;
    let lay = REMAINS.iter().find(|(id, _)| *id == first).expect("ours").1;
    there.origin = [lay[0] + 1.0, lay[1], lay[2]];
    h.bots[b].send(true, queue::WEENIE, &report(there, stamp));
    run_for(&mut h, 4.0);
    assert_eq!(
        phase_of(&h.server.players[&wb].play.quests, QUEST),
        0,
        "no step ran for the one who came too late"
    );
}
