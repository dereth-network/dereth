//! People turn to face whoever uses them: their aim told at once and again as the user moves, what
//! the use makes them say and do when the turn at their body's rate is done; within a few degrees
//! no turn and a short wait; an object answering as a person does at once; the heading kept.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_content::quests::{QuestDef, QuestTable, Step, Trigger, Update};
use yalain_content::talk::{How, Line, Role, TalkTable, When};
use yalain_content::{ContentObject, ContentPack, Description, Facts, Kind, Prop, Props, Words};
use yalain_testkit::play::server_call;
use yalain_testkit::Harness;
use yalain_world::calls::{self, Node};
use yalain_world::graph::{Builder, Slot};
use yalain_world::world::heading_of_rotation;

const USE: u32 = 0x8216_0016;
const HEAR_TELL: u32 = 0x8205_0017;
const UPDATE_QUEST: u32 = 0x8213_0024;
const USE_DONE: u32 = 0x8216_0021;

const PERSON: u64 = 0x8130_0000_0000_0301;
const TEMPLATE: u32 = 0x4700_31E6;
const TABLE: u32 = 0x2500_2A34;
const PEOPLE: u32 = 0x13A4;
/// The movement types: never turning, and turning where it stands.
const STATIONARY: u32 = 0x4000_0001;
const TURN_ONLY: u32 = 0x4000_0002;
const QUEST: u32 = 0x4000_0016;
const AT: [f32; 3] = [61.0, 241.0, -44.6];

fn person() -> ContentObject {
    ContentObject {
        object: Some(PERSON),
        template: TEMPLATE,
        description: Description {
            name_text: None,
            package: PEOPLE,
            look: 0x1F00_0100,
            enter_fx: 0,
            scale: None,
            name: Some((0xA860_7CEC, TABLE)),
            numbers: [0; 6],
            source: "authored for a test".into(),
        },
        cell: 0x0299_0109,
        origin: AT,
        // Facing north (heading 0).
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

fn line(string: u32) -> Line {
    Line {
        who: TEMPLATE,
        object: None,
        words: Words::Ref(string, TABLE),
        role: Some(Role::RefuseUse),
        how: How::Tell,
        when: When::Use,
        quest: None,
        update: None,
        phase: None,
        item: None,
        order: 0,
        measured: true,
        pause_ms: 0,
        after_update_ms: 0,
        placed: "authored for a test".into(),
    }
}

/// A quest the person gives at a use, with its line.
fn giving() -> QuestTable {
    QuestTable {
        quests: vec![QuestDef {
            duration_by_calendar: false,
            id: QUEST,
            name: Words::Ref(0xA860_7CEC, 0x2500_0A7E),
            description: (0x6C45_9364, 0x2500_0A7E),
            icon: 0x4100_034B,
            play_fx: true,
            journals: vec![(1, 0x2500_0A7E)],
            challenge: 4,
            duration: 0.0,
            timed_phase: 0,
            retry: 72000.0,
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
        }],
        steps: vec![Step {
            out_of_turn: false,
            skills: Vec::new(),
            key: String::new(),
            quest: QUEST,
            needs: 0,
            trigger: Trigger::Use(TEMPLATE),
            tell: Some(Words::Ref(0xB0, TABLE)),
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
            to: None,
            races: Vec::new(),
            effects_last: false,
            effects_quiet: false,
            wants: None,
            fellows: false,
            levels: None,
            xp_share: 0.0,
            transforms: None,
            chance: None,
            to_fellows: false,
        }],
        gives: Vec::new(),
        rooms: Vec::new(),
    }
}

fn use_of(item: u64, player: u64) -> Arg {
    let mut b = Builder::new();
    b.object(
        0x245,
        &[
            Slot::Ref(None),
            Slot::Ref(None),
            Slot::Long(item),
            Slot::Ref(None),
            Slot::Int(0),
            Slot::Long(player),
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

/// A world with the person (of the people's script package, as objects that answer as people are
/// too; its template naming a movement type, or none; its body turning at `rate`, or giving no rate)
/// and a new player standing `off` it, who has drawn its scene.
fn world(
    movement: Option<u32>,
    rate: Option<f32>,
    off: [f32; 2],
    quests: QuestTable,
) -> (Harness, usize, u64) {
    let mut h = Harness::new();
    h.server.content = ContentPack {
        objects: vec![person()],
        ..ContentPack::default()
    };
    h.server.person_packages.insert(PEOPLE);
    let mut props = Props::default();
    if let Some(kind) = movement {
        props.set("AI_MovementType", Prop::Word(kind));
    }
    h.server
        .catalog
        .insert(yalain_testkit::test_template(TEMPLATE, PEOPLE, props));
    if let Some(rate) = rate {
        h.server.catalog.still_turns.insert(TEMPLATE, rate);
    }
    h.server.talk = TalkTable {
        lines: vec![line(0xA1)],
    };
    h.server.quests = quests;
    let (b, me) = h.player_in_world("one", "Elara");
    stand(&mut h, off);
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    (h, b, me)
}

/// The player stands `off` the person.
fn stand(h: &mut Harness, off: [f32; 2]) {
    for p in h.server.players.values_mut() {
        p.position.cell = 0x0299_0109;
        p.position.origin = [AT[0] + off[0], AT[1] + off[1], AT[2]];
        p.heading_motion = yalain_world::world::IDLE_MOTION;
    }
}

/// `degrees` off north, 3 m from the person.
fn off_by(degrees: f32) -> [f32; 2] {
    let r = degrees.to_radians();
    use dereth_primitives::num::math::{cosf, sinf};
    [3.0 * sinf(r), 3.0 * cosf(r)]
}

/// What a client was sent of the use, as it came.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Seen {
    Answer,
    Tell(u32),
    Update(u32),
    /// The person's place, with its heading.
    Place(u8),
}

/// The calls and places a client received since `from` while the clock runs `seconds` more, each
/// with when it came (seconds after the use's answer) and its send sequence (a blob id's low half;
/// none for a place, which goes on an ephemeral stream).
fn watch(h: &mut Harness, b: usize, from: usize, seconds: f64) -> Vec<(f64, Seen, Option<u32>)> {
    let seen = watch_at(h, b, from, seconds);
    let t0 = seen
        .iter()
        .find(|(_, w, _)| *w == Seen::Answer)
        .map_or(0.0, |x| x.0);
    seen.iter().map(|(t, w, q)| (t - t0, *w, *q)).collect()
}

/// As [`watch`], each with the clock's time when it came.
fn watch_at(h: &mut Harness, b: usize, from: usize, seconds: f64) -> Vec<(f64, Seen, Option<u32>)> {
    let mut out: Vec<(f64, Seen, Option<u32>)> = Vec::new();
    let mut seen = from;
    let end = h.now.0 + seconds;
    while h.now.0 < end {
        h.step_by(TICK);
        let now = h.now.0;
        while seen < h.bots[b].inbox.len() {
            let r = &h.bots[b].inbox[seen];
            seen += 1;
            if !r.world {
                continue;
            }
            #[allow(clippy::cast_possible_truncation)]
            let seq = Some(r.blob_id as u32);
            let what = match r.message() {
                Some(ServerMessage::PlayerCall(c)) => {
                    let v = c
                        .args
                        .as_ref()
                        .and_then(|a| calls::values(&a.0).ok())
                        .unwrap_or_default();
                    let text = v.iter().find_map(|x| match x.root() {
                        Some(Node::TextInfo(t)) => Some(t.string_id),
                        _ => None,
                    });
                    match c.function.0 {
                        USE_DONE => (Seen::Answer, seq),
                        HEAR_TELL => (Seen::Tell(text.unwrap_or(0)), seq),
                        UPDATE_QUEST => (Seen::Update(c.function.0), seq),
                        _ => continue,
                    }
                }
                Some(ServerMessage::Position(p) | ServerMessage::PositionCell(p))
                    if p.sender.id == PERSON =>
                {
                    (
                        Seen::Place(u8::try_from(p.heading_motion >> 24).unwrap_or(0)),
                        None,
                    )
                }
                _ => continue,
            };
            out.push((now, what.0, what.1));
        }
    }
    out
}

/// Use the person; returns the inbox mark before the use.
fn use_person(h: &mut Harness, b: usize, me: u64) -> usize {
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(USE, &[use_of(PERSON, me)]),
    );
    from
}

fn heading(h: &Harness) -> u8 {
    heading_of_rotation(h.server.content.objects[0].rotation)
}

/// The angle between two heading bytes, in degrees.
fn apart(a: u8, b: u8) -> f32 {
    yalain_world::facing::angle(f32::from(a) * 360.0 / 255.0, f32::from(b) * 360.0 / 255.0)
}

fn places(seen: &[(f64, Seen, Option<u32>)]) -> Vec<(f64, u8)> {
    seen.iter()
        .filter_map(|(t, w, _)| match w {
            Seen::Place(h) => Some((*t, *h)),
            _ => None,
        })
        .collect()
}

fn told(seen: &[(f64, Seen, Option<u32>)]) -> Option<f64> {
    seen.iter()
        .find(|(_, w, _)| matches!(w, Seen::Tell(_)))
        .map(|x| x.0)
}

/// The step the clock is run by here, and the tolerance of a time measured by stepping it (a
/// message sent in one step comes in the next).
const TICK: f64 = 0.01;
const STEP: f64 = 2.0 * TICK + 1e-9;

/// 2017's: a turn's beat besides its angle at its body's rate, the first place told after the
/// use's answer, the places told after it by turns, how long one facing its user takes to speak.
const BEAT: f64 = 0.152;
const FIRST: f64 = 0.1;
const AGAIN: [f64; 2] = [0.2, 0.3];
const FACING: f64 = 0.17;

/// A person turns at its body's own rate, a quarter turn taking the beat and 90 degrees at it, and
/// speaks when the turn is done: a body of 90 degrees a second over half a second later than one of
/// 200.
/// Behaviour: server.places.person-turns-at-its-bodys-rate
/// Behaviour: server.places.person-turn-ends-ours
#[test]
fn a_person_turns_at_its_bodys_rate_and_speaks_when_the_turn_is_done() {
    let mut said = Vec::new();
    for rate in [90.0, 200.0] {
        let (mut h, b, me) = world(
            Some(TURN_ONLY),
            Some(rate),
            off_by(90.0),
            QuestTable::default(),
        );
        let from = use_person(&mut h, b, me);
        let seen = watch(&mut h, b, from, 3.0);
        let at = told(&seen).expect("the line");
        let want = BEAT + 90.0 / f64::from(rate);
        assert!(
            (at - want).abs() <= STEP,
            "rate {rate}: the line at {at} s, not {want} s"
        );
        said.push(at);
    }
    assert!(
        said[0] - said[1] > 0.5,
        "the slower body speaks later: {said:?}"
    );
}

/// A turning person's place is told with its aim, the heading toward its user, a moment after the
/// use's answer; again every 0.2 s and 0.3 s by turns, aimed where the user is then; and once more
/// as the turn is done, with or after its line, and no more. The line is sent after the answer.
/// Behaviour: server.places.person-re-aims-ours
#[test]
fn a_turning_persons_aim_is_told_at_once_and_again_where_its_user_moves() {
    let (mut h, b, me) = world(
        Some(TURN_ONLY),
        Some(90.0),
        off_by(180.0),
        QuestTable::default(),
    );
    let from = use_person(&mut h, b, me);
    let mut seen = watch_at(&mut h, b, from, 0.8);
    // The player steps round to the east part way through the turn.
    stand(&mut h, off_by(90.0));
    let mark = h.bots[b].inbox.len();
    seen.extend(watch_at(&mut h, b, mark, 3.0));
    let t0 = seen
        .iter()
        .find(|x| x.1 == Seen::Answer)
        .map_or(0.0, |x| x.0);
    let seen: Vec<_> = seen.into_iter().map(|(t, w, q)| (t - t0, w, q)).collect();
    let p = places(&seen);
    assert!(p.len() >= 5, "told as it turns: {p:?}");
    assert!((p[0].0 - FIRST).abs() <= STEP, "first told at {} s", p[0].0);
    assert!(apart(p[0].1, 128) <= 2.0, "aimed south at once: {}", p[0].1);
    let gaps: Vec<f64> = p.windows(2).map(|w| w[1].0 - w[0].0).collect();
    for (i, g) in gaps.iter().enumerate() {
        let want = AGAIN[i % 2];
        assert!(
            (g - want).abs() <= STEP,
            "told again after {g} s, not {want}: {gaps:?}"
        );
    }
    let last = p.last().expect("a place");
    assert!(
        apart(last.1, 64) <= 2.0,
        "aimed where the user went: {}",
        last.1
    );
    let line = told(&seen).expect("the line");
    let after: Vec<_> = p.iter().filter(|x| x.0 >= line - 1e-9).collect();
    assert_eq!(
        after.len(),
        1,
        "told once more, with or after the line, and no more: {p:?} {line}"
    );
    let seq = |w: Seen| seen.iter().find(|x| x.1 == w).and_then(|x| x.2);
    assert!(
        seq(Seen::Answer) < seq(Seen::Tell(0xA1)),
        "the answer sent before the line"
    );
}

/// A person within ten degrees of facing its user doesn't turn: its place is told once as it
/// stands, and it speaks a moment after the answer. Past ten degrees it turns.
/// Behaviour: server.places.person-turns-past-ten-degrees-ours
/// Behaviour: server.places.person-facing-speaks-ours
#[test]
fn a_person_within_ten_degrees_of_its_user_speaks_after_one_place() {
    let (mut h, b, me) = world(
        Some(TURN_ONLY),
        Some(90.0),
        off_by(8.0),
        QuestTable::default(),
    );
    let from = use_person(&mut h, b, me);
    let seen = watch(&mut h, b, from, 1.5);
    assert_eq!(
        places(&seen).iter().map(|x| x.1).collect::<Vec<_>>(),
        [0],
        "told once, as it stands"
    );
    let at = told(&seen).expect("the line");
    assert!((at - FACING).abs() <= STEP, "the line at {at} s");
    assert_eq!(heading(&h), 0, "still facing north");
    let (mut h, b, me) = world(
        Some(TURN_ONLY),
        Some(90.0),
        off_by(12.0),
        QuestTable::default(),
    );
    let from = use_person(&mut h, b, me);
    let seen = watch(&mut h, b, from, 1.5);
    let at = told(&seen).expect("the line");
    let want = BEAT + 12.0 / 90.0;
    assert!(
        (at - want).abs() <= STEP,
        "12 degrees: the line at {at} s, not {want}"
    );
    assert!(
        places(&seen).first().is_some_and(|x| x.1 != 0),
        "aimed: {seen:?}"
    );
}

/// A quest step the use runs waits for the turn as a line does: its line and its update come once
/// the turn is done, none before.
/// Behaviour: server.places.person-turns-to-its-user
#[test]
fn a_quest_step_at_a_person_waits_for_its_turn() {
    let (mut h, b, me) = world(Some(TURN_ONLY), Some(90.0), off_by(90.0), giving());
    let from = use_person(&mut h, b, me);
    let seen = watch(&mut h, b, from, 2.0);
    let done = BEAT + 1.0;
    let line = seen
        .iter()
        .find(|x| x.1 == Seen::Tell(0xB0))
        .expect("its line");
    let update = seen
        .iter()
        .find(|x| matches!(x.1, Seen::Update(_)))
        .expect("its update");
    assert!(
        (line.0 - done).abs() <= STEP && (update.0 - done).abs() <= STEP,
        "{seen:?}"
    );
    assert!(line.2 < update.2, "the line sent before the update");
}

/// A turned person keeps the heading it turned to: long after, it still faces where its user stood,
/// and is told nothing more.
/// Behaviour: server.places.person-turns-to-its-user
#[test]
fn a_turned_person_keeps_its_new_heading() {
    let (mut h, b, me) = world(
        Some(TURN_ONLY),
        Some(200.0),
        off_by(270.0),
        QuestTable::default(),
    );
    let from = use_person(&mut h, b, me);
    watch(&mut h, b, from, 2.0);
    assert!(
        apart(heading(&h), 191) <= 2.0,
        "it faces west: {}",
        heading(&h)
    );
    let mark = h.bots[b].inbox.len();
    let seen = watch(&mut h, b, mark, 30.0);
    assert!(apart(heading(&h), 191) <= 2.0, "kept");
    assert!(places(&seen).is_empty(), "nothing more told");
}

/// An object that answers a use as a person does, of the people's script package, doesn't turn:
/// neither one that is stationary nor one naming no movement type (a creature's default would turn
/// it), whatever its body's rate. Its line comes with the answer, and its place isn't told.
/// Behaviour: server.places.person-turns-to-its-user
#[test]
fn an_object_answering_as_a_person_does_not_turn() {
    for movement in [Some(STATIONARY), None] {
        let (mut h, b, me) = world(movement, Some(90.0), off_by(90.0), QuestTable::default());
        let from = use_person(&mut h, b, me);
        let seen = watch(&mut h, b, from, 1.5);
        let at = told(&seen).expect("the line");
        assert!(at <= STEP, "{movement:?}, at once: {at} s");
        assert!(places(&seen).is_empty(), "{movement:?}: no place told");
        assert_eq!(heading(&h), 0, "{movement:?}: still facing north");
    }
}

/// A person whose body gives no turn rate turns no more than one facing its user: told once as it
/// stands, it speaks a moment after the answer.
/// Behaviour: server.places.person-facing-speaks-ours
/// Behaviour: server.places.person-turns-at-its-bodys-rate
#[test]
fn a_person_whose_body_gives_no_rate_answers_as_one_facing_its_user() {
    let (mut h, b, me) = world(Some(TURN_ONLY), None, off_by(90.0), QuestTable::default());
    let from = use_person(&mut h, b, me);
    let seen = watch(&mut h, b, from, 1.5);
    assert_eq!(places(&seen).iter().map(|x| x.1).collect::<Vec<_>>(), [0]);
    let at = told(&seen).expect("the line");
    assert!((at - FACING).abs() <= STEP, "the line at {at} s");
}
