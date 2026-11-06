//! What people say: a person used with nothing for the player, a step's line and the lines that
//! come with its update, new prose said as it is, sayings and emotes, a greeting to a player come
//! near and chatter while one is, and a creature's lines as it fights and dies.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_content::quests::{QuestDef, QuestTable, Step, Trigger, Update};
use yalain_content::talk::{How, Line, Role, TalkTable, TalkUpdate, When};
use yalain_content::{ContentObject, ContentPack, Description, Facts, Kind, Words};
use yalain_testkit::play::server_call;
use yalain_testkit::{Harness, TEST_CREATURE};
use yalain_world::calls::{self, Node, Value};
use yalain_world::graph::{Builder, Slot};
use yalain_world::quests::status;
use yalain_world::state::QuestState;

const USE: u32 = 0x8216_0016;
const HEAR_TELL: u32 = 0x8205_0017;
const SHOW_TEXT: u32 = 0x8205_0011;
const SAYING: u32 = 0x8205_0016;
const UPDATE_QUEST: u32 = 0x8213_0024;
const START_ATTACK: u32 = 0x8204_0008;

const QUEST: u32 = 0x4000_0016;
const PERSON: u64 = 0x8130_0000_0000_0301;
const TEMPLATE: u32 = 0x4700_31E6;
const TABLE: u32 = 0x2500_2A34;

fn person(origin: [f32; 3]) -> ContentObject {
    ContentObject {
        object: Some(PERSON),
        template: TEMPLATE,
        description: Description {
            name_text: None,
            package: 0x13A4,
            look: 0x1F00_0100,
            enter_fx: 0,
            scale: None,
            name: Some((0xA860_7CEC, TABLE)),
            numbers: [0; 6],
            source: "authored for a test".into(),
        },
        cell: 0x0299_0109,
        origin,
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

fn line(words: Words, role: Role, when: When, measured: bool) -> Line {
    Line {
        who: TEMPLATE,
        object: None,
        words,
        role: Some(role),
        how: How::Tell,
        when,
        quest: None,
        update: None,
        phase: None,
        item: None,
        order: 0,
        measured,
        placed: "authored for a test".into(),
    }
}

fn words(string: u32) -> Words {
    Words::Ref(string, TABLE)
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

/// The text a call's values carry first: its string id, or its literal.
fn first_text(v: &[Value]) -> Option<(u32, Option<String>)> {
    v.iter().find_map(|x| match x.root() {
        Some(Node::TextInfo(t)) => Some((
            t.string_id,
            t.literal.as_ref().map(|l| String::from_utf16_lossy(l)),
        )),
        _ => None,
    })
}

/// A call: its function and the text it carries first (string id, literal).
type Heard = (u32, Option<(u32, Option<String>)>);

/// The calls a client received since `from`, each with the text it carries first.
fn texts(h: &Harness, b: usize, from: usize) -> Vec<Heard> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| {
            let (f, args) = match m {
                ServerMessage::PlayerCall(c) => (c.function.0, c.args.as_ref()),
                ServerMessage::CellObjectCall(c) => (c.call.function.0, c.call.args.as_ref()),
                _ => return None,
            };
            let v = args
                .and_then(|a| calls::values(&a.0).ok())
                .unwrap_or_default();
            Some((f, first_text(&v)))
        })
        .collect()
}

/// A world with the person beside a new player who has drawn its scene.
fn world(talk: Vec<Line>, quests: QuestTable) -> (Harness, usize, u64) {
    let mut h = Harness::new();
    h.server.content = ContentPack {
        objects: vec![person([61.0, 241.0, -44.6])],
        ..ContentPack::default()
    };
    h.server.talk = TalkTable { lines: talk };
    h.server.quests = quests;
    let (b, me) = h.player_in_world("one", "Elara");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    (h, b, me)
}

fn use_person(h: &mut Harness, b: usize, me: u64) -> usize {
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(USE, &[use_of(PERSON, me)]),
    );
    h.run(10);
    from
}

fn tells(h: &Harness, b: usize, from: usize) -> Vec<u32> {
    texts(h, b, from)
        .into_iter()
        .filter(|(f, _)| *f == HEAR_TELL)
        .filter_map(|(_, t)| t.map(|(s, _)| s))
        .collect()
}

/// A person used with no quest step for the player says one of its lines for a use, the ones heard
/// said in the recordings before those its names give it; with one of its quests under way, its
/// reminder instead.
/// Behaviour: server.talk.used-with-nothing-to-give
#[test]
fn a_person_used_with_nothing_for_the_player_says_one_of_its_lines_or_a_reminder() {
    let mut remind = line(words(0xC0), Role::Remind, When::Use, false);
    remind.quest = Some(QUEST);
    let talk = vec![
        line(words(0xA0), Role::RefuseUse, When::Use, false),
        line(words(0xA1), Role::RefuseUse, When::Use, true),
        remind,
        line(words(0xD0), Role::Near, When::Near, true),
    ];
    let (mut h, b, me) = world(talk, QuestTable::default());
    let from = use_person(&mut h, b, me);
    assert_eq!(
        tells(&h, b, from),
        [0xA1],
        "the line heard said, to the player"
    );
    for p in h.server.players.values_mut() {
        p.play.quests.push(QuestState {
            quest: QUEST,
            status: status::UNDERWAY,
            phase: 1,
            started: 0,
            updated: 0,
        });
    }
    let from = use_person(&mut h, b, me);
    assert_eq!(
        tells(&h, b, from),
        [0xC0],
        "the quest under way: its reminder"
    );
}

/// A player standing still a little farther from a person than its reach (within the reach and
/// the slack a client at rest is allowed) is answered at once: its client counts the walk over and
/// reports nothing more.
/// Behaviour: server.places.use-from-rest-in-reach
#[test]
fn a_player_at_rest_a_little_out_of_reach_is_answered_at_once() {
    let (mut h, b, me) = world(
        vec![line(words(0xA1), Role::RefuseUse, When::Use, true)],
        QuestTable::default(),
    );
    for p in h.server.players.values_mut() {
        p.position.origin = [61.0 - 7.0, 241.0, -44.6];
        p.heading_motion = yalain_world::world::IDLE_MOTION;
    }
    let from = use_person(&mut h, b, me);
    assert_eq!(
        tells(&h, b, from),
        [0xA1],
        "answered without a further report"
    );
    for p in h.server.players.values_mut() {
        p.position.origin = [61.0 - 20.0, 241.0, -44.6];
    }
    let from = use_person(&mut h, b, me);
    assert!(tells(&h, b, from).is_empty(), "20 m away: walked first");
}

/// A step a person's use runs says its own line, then the lines heard said with that update of
/// that quest, in their order (its own line not twice); lines only derived from their names are
/// left out when heard ones are there.
/// Behaviour: server.talk.with-a-quest-step
#[test]
fn a_step_says_its_line_then_the_lines_heard_with_its_update_in_order() {
    let with_update = |s: u32, order: u32, measured: bool| {
        let mut l = line(words(s), Role::Offer, When::Use, measured);
        l.quest = Some(QUEST);
        l.update = Some(TalkUpdate::Bestow);
        l.order = order;
        l
    };
    let talk = vec![
        with_update(0xB2, 1, true),
        with_update(0xB0, 0, true),
        with_update(0xB9, 0, false),
    ];
    let quests = QuestTable {
        quests: vec![QuestDef {
            id: QUEST,
            name: (0xA860_7CEC, 0x2500_0A7E),
            description: (0x6C45_9364, 0x2500_0A7E),
            icon: 0x4100_034B,
            play_fx: true,
            journals: vec![(1, 0x2500_0A7E)],
            challenge: 4,
            duration: 86400.0,
            timed_phase: 0,
            retry: 72000.0,
            xp: 80,
            cancellable: true,
            placed: "authored for a test".into(),
            hidden: false,
            effects: Vec::new(),
            subquests: Vec::new(),
            min_level: 0,
            fails_on_death: false,
            once: false,
            fellows: yalain_content::quests::Fellows::default(),
            last_waits: false,
            hero_only: false,
            pay: yalain_content::quests::CompletionPay::default(),
            required: None,
        }],
        steps: vec![Step {
            quest: QUEST,
            needs: 0,
            trigger: Trigger::Use(TEMPLATE),
            tell: Some(words(0xB0)),
            update: Update::Bestow,
            xp: 0,
            gives: Vec::new(),
            counts: Vec::new(),
            effects: Vec::new(),
            text: None,
            placed: "authored for a test".into(),
            requires: Vec::new(),
            takes: Vec::new(),
            to: None,
            races: Vec::new(),
            effects_last: false,
            wants: None,
            fellows: false,
            levels: None,
            xp_share: 0.0,
        }],
        gives: Vec::new(),
        rooms: Vec::new(),
    };
    let (mut h, b, me) = world(talk, quests);
    let from = use_person(&mut h, b, me);
    assert_eq!(tells(&h, b, from), [0xB0, 0xB2]);
    assert!(texts(&h, b, from).iter().any(|(f, _)| *f == UPDATE_QUEST));
}

/// New prose with no string of its own goes as literal text, shown as it is; a line said aloud
/// reaches every client that knows the speaker, on the speaker; an emote is text of the chat
/// window's emote kind.
/// Behaviour: server.talk.literal-sayings-and-emotes
#[test]
fn new_prose_goes_as_literal_text_and_a_line_can_be_said_aloud_or_emoted() {
    let mut aloud = line(
        Words::Literal("Fine day for it.".into()),
        Role::RefuseUse,
        When::Use,
        true,
    );
    aloud.how = How::Say;
    let (mut h, b, me) = world(vec![aloud], QuestTable::default());
    let from = use_person(&mut h, b, me);
    let said: Vec<_> = texts(&h, b, from)
        .into_iter()
        .filter(|(f, _)| *f == SAYING)
        .collect();
    assert_eq!(
        said,
        [(SAYING, Some((0, Some("Fine day for it.".to_string()))))]
    );
    let mut emote = line(words(0xE0), Role::Use, When::Use, true);
    emote.how = How::Emote;
    h.server.talk = TalkTable { lines: vec![emote] };
    let from = use_person(&mut h, b, me);
    let shown: Vec<_> = h
        .world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) if c.function.0 == SHOW_TEXT => {
                let v = c
                    .args
                    .as_ref()
                    .and_then(|a| calls::values(&a.0).ok())
                    .unwrap_or_default();
                Some((first_text(&v).map(|t| t.0), v.get(1).cloned()))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        shown,
        [(Some(0xE0), Some(Value::Int(yalain_world::talk::EMOTE_TEXT)))]
    );
}

/// A person with lines for a player near greets a player within reach once, and not again soon;
/// one with idle lines says one now and then while a player is near.
/// Behaviour: server.talk.near-and-idle
#[test]
fn a_person_greets_a_player_come_near_once_and_chatters_while_one_is_near() {
    let mut chatter = line(words(0xF1), Role::Idle, When::Idle, true);
    chatter.how = How::Say;
    let talk = vec![line(words(0xF0), Role::Near, When::Near, true), chatter];
    let (mut h, b, _) = world(talk, QuestTable::default());
    h.run(4000);
    let all = texts(&h, b, 0);
    let greetings = all
        .iter()
        .filter(|(f, t)| *f == HEAR_TELL && t.as_ref().is_some_and(|t| t.0 == 0xF0))
        .count();
    assert_eq!(greetings, 1, "greeted once in 200 s");
    let sayings = all
        .iter()
        .filter(|(f, t)| *f == SAYING && t.as_ref().is_some_and(|t| t.0 == 0xF1))
        .count();
    assert!(
        (1..=3).contains(&sayings),
        "chatter every 60 to 120 s after a first wait: {sayings} in 200 s"
    );
}

/// A creature with fighting lines says one as it goes after a player, and one with dying lines
/// one as it dies.
/// Behaviour: server.talk.fighting-and-dying
#[test]
fn a_creature_says_a_fighting_line_as_it_goes_after_a_player_and_a_dying_line_as_it_dies() {
    let mut h = Harness::new();
    h.add_creatures(1);
    let creature_line = |s: u32, role: Role, when: When| {
        let mut l = line(words(s), role, when, true);
        l.who = TEST_CREATURE;
        l.how = How::Emote;
        l
    };
    h.server.talk = TalkTable {
        lines: vec![
            creature_line(0x71, Role::Combat, When::Attack),
            creature_line(0x72, Role::Death, When::Death),
        ],
    };
    let (b, me) = h.player_in_world("one", "Elara");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    h.arm(b, me, 0x4700_051C);
    let beetle = h
        .world_messages(b, 0)
        .iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.weenie.entity_did == TEST_CREATURE => Some(o.id),
            _ => None,
        })
        .expect("the camp's creature");
    // The lines are the claim, not the fight: the beetle hits too softly to win it.
    if let Some(c) = h
        .server
        .things
        .get_mut(beetle)
        .and_then(|t| t.creature.as_mut())
    {
        c.stats.natural_damage = 1.0;
    }
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(START_ATTACK, &[Arg::LongInt(beetle)]),
    );
    let dead = h.run_until(4000, |h| {
        h.world_messages(b, 0).iter().any(|m| {
            matches!(m, ServerMessage::UpdateBoolVisual(q) if q.object.id == beetle && q.quality == 279 && q.value == 1)
        })
    });
    assert!(dead, "the beetle dies");
    h.run(5);
    let emoted: Vec<u32> = texts(&h, b, 0)
        .into_iter()
        .filter(|(f, _)| *f == SHOW_TEXT)
        .filter_map(|(_, t)| t.map(|t| t.0))
        .filter(|s| *s == 0x71 || *s == 0x72)
        .collect();
    assert_eq!(emoted, [0x71, 0x72]);
}

/// A creature whose idle lines are broadcast says one every 20 to 50 s, a player near or not; each
/// line reaches every player in its landblock and the eight around it, as a local broadcast (kind
/// 256) from the speaker, and no player farther off.
/// Behaviour: server.talk.idle-broadcast
#[test]
fn an_idle_broadcast_reaches_the_speakers_landblock_and_the_eight_around_it_every_twenty_to_fifty_seconds(
) {
    use yalain_world::talk::{BROADCAST, BROADCAST_KIND};
    const OUTDOORS: u32 = 0x7F64_001E;
    let mut idle = line(words(0x0820_4D9C), Role::Idle, When::Idle, true);
    idle.how = How::Broadcast;
    let mut h = Harness::new();
    let mut p = person([30.0, 30.0, 0.0]);
    p.cell = OUTDOORS;
    h.server.content = ContentPack {
        objects: vec![p],
        ..ContentPack::default()
    };
    h.server.talk = TalkTable { lines: vec![idle] };
    // One player in the speaker's landblock but far from it, one in the landblock beside it, one
    // two landblocks off.
    let places = [0x7F64_0040, 0x8065_0001, 0x8164_001E];
    let mut bots = Vec::new();
    for (n, cell) in places.iter().enumerate() {
        let (b, me) = h.player_in_world(&format!("p{n}"), &format!("Elara{}", ["a", "b", "c"][n]));
        h.bots[b].send(
            true,
            queue::WEENIE,
            &auberean_protocol::ClientMessage::SceneRenderingComplete(
                auberean_protocol::login::Empty,
            ),
        );
        h.run(5);
        let p = h
            .server
            .players
            .values_mut()
            .find(|p| p.character == me)
            .expect("in the world");
        p.position.cell = *cell;
        p.position.origin = [80.0, 80.0, 0.0];
        bots.push(b);
    }
    let from: Vec<usize> = bots.iter().map(|b| h.bots[*b].inbox.len()).collect();
    h.run(8000);
    let heard = |i: usize| -> Vec<Vec<Value>> {
        h.world_messages(bots[i], from[i])
            .iter()
            .filter_map(|m| match m {
                ServerMessage::CellObjectCall(c) if c.call.function.0 == BROADCAST => {
                    assert_eq!(c.object.id, PERSON, "from the speaker");
                    Some(
                        c.call
                            .args
                            .as_ref()
                            .and_then(|a| calls::values(&a.0).ok())
                            .unwrap_or_default(),
                    )
                }
                _ => None,
            })
            .collect()
    };
    let (same, beside, far) = (heard(0), heard(1), heard(2));
    // 400 s: after a first wait, a line every 20 to 50 s: 6 at the fewest, 19 at the most.
    assert!((6..=19).contains(&same.len()), "{} lines", same.len());
    assert_eq!(beside.len(), same.len(), "the landblock beside hears each");
    assert!(far.is_empty(), "two landblocks off hears none");
    assert_eq!(same[0].first(), Some(&Value::Int(BROADCAST_KIND)));
    assert_eq!(first_text(&same[0]).map(|t| t.0), Some(0x0820_4D9C));
    assert!(names_in(&same[0]).is_empty(), "it names no one");
}

/// The literal words held by the variables of a call's first text.
fn names_in(values: &[Value]) -> Vec<String> {
    use auberean_script::native_object::VariableValue;
    values
        .iter()
        .find_map(|x| match x.root() {
            Some(Node::TextInfo(t)) => Some(t.variables.clone()),
            _ => None,
        })
        .unwrap_or_default()
        .iter()
        .filter_map(|v| match &v.value {
            VariableValue::Nested(t) => t.literal.as_ref().map(|l| String::from_utf16_lossy(l)),
            _ => None,
        })
        .collect()
}

/// A broadcast a player's act brought on names that player, to everyone who hears it.
/// Behaviour: server.talk.broadcast-names-the-player
#[test]
fn a_broadcast_a_player_brought_on_names_that_player_to_all_who_hear_it() {
    use yalain_world::talk::{content_speaker, say, BROADCAST};
    let mut brought = line(words(0x9C2D_B3BC), Role::Accept, When::Give, true);
    brought.how = How::Broadcast;
    let mut h = Harness::new();
    h.server.content = ContentPack {
        objects: vec![person([30.0, 30.0, 0.0])],
        ..ContentPack::default()
    };
    let (a, me) = h.player_in_world("one", "Elara");
    let (other, _) = h.player_in_world("two", "Kestrel");
    let cell = h.server.content.objects[0].cell;
    for p in h.server.players.values_mut() {
        p.position.cell = cell;
    }
    let from = (h.bots[a].inbox.len(), h.bots[other].inbox.len());
    let world = h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .map(|(w, _)| *w)
        .expect("in the world");
    let named = h.server.players.get(&world).map_or_else(String::new, |p| {
        yalain_world::player::world_name(&p.name, p.sex)
    });
    let speaker = content_speaker(&h.server, 0);
    say(
        &h.server,
        &mut h.net,
        world,
        &speaker,
        &brought.words,
        brought.how,
    );
    h.run(3);
    for (b, from) in [(a, from.0), (other, from.1)] {
        let heard: Vec<Vec<Value>> = h
            .world_messages(b, from)
            .iter()
            .filter_map(|m| match m {
                ServerMessage::CellObjectCall(c) if c.call.function.0 == BROADCAST => {
                    c.call.args.as_ref().and_then(|x| calls::values(&x.0).ok())
                }
                _ => None,
            })
            .collect();
        assert_eq!(heard.len(), 1);
        assert_eq!(names_in(&heard[0]), [named.as_str()]);
    }
}

/// A person who sends whoever uses it says its line as it does.
/// Behaviour: server.quests.a-person-sends-whoever-uses-it-speaks
#[test]
fn a_person_who_sends_whoever_uses_it_says_its_line_first() {
    let mut h = Harness::new();
    let mut o = person([61.0, 241.0, -44.6]);
    o.destination = Some(yalain_content::Place {
        cell: 0x0499_0126,
        origin: [10.0, 20.0, 5.0],
        rotation: [1.0, 0.0, 0.0, 0.0],
    });
    h.server.content = ContentPack {
        objects: vec![o],
        ..ContentPack::default()
    };
    h.server.talk = TalkTable {
        lines: vec![line(words(0x0000_0B01), Role::Use, When::Use, true)],
    };
    let (b, me) = h.player_in_world("one", "Elara");
    let from = use_person(&mut h, b, me);
    assert_eq!(tells(&h, b, from), vec![0x0000_0B01], "its line");
    let until = h.now.0 + 8.0;
    h.run_to(yalain_net::LocalTime(until));
    let p = h.server.players.values().next().expect("a player");
    assert_eq!(p.position.cell, 0x0499_0126, "sent");
}
