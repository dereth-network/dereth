//! The placed objects whose use moves a quest: a quest whose phase is done by subquests (the
//! kingdom stones' doctrines), a failed quest's wait before it is given again (the Mist Shrine), a
//! quest's least level and the person's refusal (the Great Deru Shrine), a quest that fails when
//! its player dies (the Mist Opal), and a faction shrine used on a mount.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use auberean_script::native_object::{QuestInfo, TextInfo, VariableValue};
use auberean_script::wire::In;
use yalain_content::quests::{Need, QuestDef, QuestTable, Step, Subquests, Trigger, Update};
use yalain_content::talk::{How, Line, Role, TalkTable, When};
use yalain_content::{ContentObject, ContentPack, Description, Facts, Kind, Words};
use yalain_testkit::play::server_call;
use yalain_testkit::Harness;
use yalain_world::calls::{self, Node, Value};
use yalain_world::graph::{Builder, Slot};
use yalain_world::quests::{composed, status};
use yalain_world::state::QuestState;

const USE: u32 = 0x8216_0016;
const UPDATE_QUEST: u32 = 0x8213_0024;
const HEAR_TELL: u32 = 0x8205_0017;
const SHOW_TEXT: u32 = 0x8205_0011;
const DEATH_MESSAGE: u32 = 0x8207_002B;

/// The doctrines' quest, its three stones' subquests and its phases' journals.
const DOCTRINES: u32 = 0x4000_005E;
const DOCTRINES_TABLE: u32 = 0x2500_17D8;
const STONE_QUESTS: [(u32, u32); 3] = [
    (0x4000_012C, 0x2500_17DA),
    (0x4000_012D, 0x2500_17DC),
    (0x4000_012E, 0x2500_17DE),
];
const SPIRIT: u32 = 0x4700_3366;
const STONES: [u32; 3] = [0x4700_3363, 0x4700_3364, 0x4700_3365];
const JOURNAL: [u32; 3] = [0x2E5C_1E52, 0x2E5F_1E52, 0x2E5E_1E52];

fn object(id: u64, template: u32, origin: [f32; 3]) -> ContentObject {
    ContentObject {
        object: Some(id),
        template,
        description: Description {
            package: 0x103E,
            look: 0x1F00_0100,
            enter_fx: 0,
            scale: None,
            name: Some((0xA860_7CEC, 0x2500_1B44)),
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

fn quest(id: u32, table: u32, phases: usize) -> QuestDef {
    QuestDef {
        id,
        name: (0xA860_7CEC, table),
        description: (0x6C45_9364, table),
        icon: 0x4100_034B,
        play_fx: true,
        journals: JOURNAL[..phases].iter().map(|j| (*j, table)).collect(),
        challenge: 15,
        duration: 0.0,
        retry: 0.0,
        xp: 0,
        cancellable: true,
        placed: "authored for a test".into(),
        hidden: false,
        effects: Vec::new(),
        subquests: Vec::new(),
        min_level: 0,
        fails_on_death: false,
        once: false,
    }
}

fn step(quest: u32, needs: u32, trigger: Trigger, update: Update) -> Step {
    Step {
        quest,
        needs,
        trigger,
        tell: None,
        update,
        xp: 0,
        gives: Vec::new(),
        counts: Vec::new(),
        effects: Vec::new(),
        text: None,
        placed: "authored for a test".into(),
        requires: Vec::new(),
        takes: Vec::new(),
        races: Vec::new(),
        effects_last: false,
        fellows: false,
        to: None,
    }
}

fn line(who: u32, string: u32, role: Role, how: How, quest: Option<u32>) -> Line {
    Line {
        who,
        object: None,
        words: Words::Ref(string, 0x2500_13B2),
        role: Some(role),
        how,
        when: When::Use,
        quest,
        update: None,
        phase: None,
        item: None,
        order: 0,
        measured: true,
        placed: "authored for a test".into(),
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

fn calls_of(h: &Harness, b: usize, from: usize) -> Vec<(u32, Vec<Value>)> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) => Some((
                c.function.0,
                c.args
                    .as_ref()
                    .and_then(|a| calls::values(&a.0).ok())
                    .unwrap_or_default(),
            )),
            _ => None,
        })
        .collect()
}

/// The quest updates a client received since `from`: each one's kind and record.
fn updates(h: &Harness, b: usize, from: usize) -> Vec<(u32, QuestInfo)> {
    calls_of(h, b, from)
        .into_iter()
        .filter(|(f, _)| *f == UPDATE_QUEST)
        .filter_map(|(_, v)| {
            let record = match v.get(1)?.root()? {
                Node::Native { bytes, .. } => QuestInfo::unpack(&mut In::new(bytes)).ok()?,
                _ => return None,
            };
            match v.last() {
                Some(Value::Int(k)) => Some((*k, record)),
                _ => None,
            }
        })
        .collect()
}

/// The string ids of the texts a client was shown since `from` (the call each came as).
fn texts(h: &Harness, b: usize, from: usize) -> Vec<(u32, u32)> {
    calls_of(h, b, from)
        .into_iter()
        .filter(|(f, _)| *f == SHOW_TEXT || *f == HEAR_TELL)
        .filter_map(|(f, v)| {
            v.iter().find_map(|x| match x.root() {
                Some(Node::TextInfo(t)) => Some((f, t.string_id)),
                _ => None,
            })
        })
        .collect()
}

/// A text's variable that holds another text.
fn var(t: &TextInfo, id: u32) -> Option<&TextInfo> {
    t.variables.iter().find_map(|v| match &v.value {
        VariableValue::Nested(n) if v.id == id => Some(&**n),
        _ => None,
    })
}

/// The entries a phase's journal shows, the phase's own last: its subquests' tables, then its own.
fn entries(journal: &TextInfo) -> Vec<u32> {
    assert_eq!(
        (journal.string_id, journal.table),
        composed::WHOLE,
        "a journal put together"
    );
    let mut out = Vec::new();
    let mut list = var(journal, composed::LIST).expect("its subquests");
    while (list.string_id, list.table) == composed::PAIR {
        out.push(var(list, composed::ENTRY).expect("an entry").table);
        list = var(list, composed::SO_FAR).expect("those so far");
    }
    if list.string_id != 0 {
        out.push(list.table);
    }
    out.push(
        var(journal, composed::ENTRY)
            .expect("the phase's own")
            .table,
    );
    out
}

/// A world of the doctrines: the Spirit and three stones beside the start, the quest's first
/// phase done by the three stones' subquests.
fn doctrines_world() -> Harness {
    let mut h = Harness::new();
    let mut objects = vec![object(0x8130_0000_0000_0401, SPIRIT, [61.0, 241.0, -44.6])];
    for (i, t) in STONES.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let x = 62.0 + i as f32;
        objects.push(object(
            0x8130_0000_0000_0402 + i as u64,
            *t,
            [x, 241.0, -44.6],
        ));
    }
    h.server.content = ContentPack { objects };
    let mut quests = vec![QuestDef {
        subquests: vec![Subquests {
            phase: 1,
            quests: STONE_QUESTS.iter().map(|(q, _)| *q).collect(),
            any: false,
        }],
        ..quest(DOCTRINES, DOCTRINES_TABLE, 3)
    }];
    let mut steps = vec![step(DOCTRINES, 0, Trigger::Use(SPIRIT), Update::Bestow)];
    for ((q, table), stone) in STONE_QUESTS.iter().zip(STONES) {
        // Not marked hidden: a subquest is never told all the same.
        quests.push(quest(*q, *table, 1));
        steps.push(Step {
            requires: vec![(DOCTRINES, Need::Phase(1))],
            ..step(*q, 1, Trigger::Use(stone), Update::Complete)
        });
    }
    h.server.quests = QuestTable { quests, steps };
    h
}

fn use_it(h: &mut Harness, b: usize, me: u64, id: u64) -> usize {
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &server_call(USE, &[use_of(id, me)]));
    h.run(20);
    from
}

/// A quest whose phase is done by subquests: given, its record's journal shows the phase's entry
/// with each subquest's; each subquest done (a stone used) tells the quest's record again with its
/// phase kept and that subquest's entry gone; the last moves it on to its next phase. No subquest
/// is ever told on its own, nor listed, and a stone used twice does nothing more.
/// Behaviour: server.quests.subquests-told-as-their-quest
#[test]
fn a_quests_subquests_are_told_as_its_record_and_the_last_moves_it_on() {
    let mut h = doctrines_world();
    let (b, me) = h.player_in_world("one", "Elara");
    let from = use_it(&mut h, b, me, 0x8130_0000_0000_0401);
    let given = updates(&h, b, from);
    assert_eq!(given.len(), 1, "the quest alone is told");
    let (kind, rec) = &given[0];
    assert_eq!((*kind, rec.id, rec.phase), (0x1000_0000, DOCTRINES, 1));
    assert_eq!(
        entries(&rec.journal),
        vec![0x2500_17DA, 0x2500_17DC, 0x2500_17DE, DOCTRINES_TABLE],
        "each stone's entry, then the phase's"
    );
    // The middle stone, then the first: the phase kept, the entries left.
    let from = use_it(&mut h, b, me, 0x8130_0000_0000_0403);
    let told = updates(&h, b, from);
    assert_eq!(told.len(), 1);
    assert_eq!(
        (told[0].0, told[0].1.id, told[0].1.phase),
        (0x2000_0000, DOCTRINES, 1)
    );
    assert_eq!(
        entries(&told[0].1.journal),
        vec![0x2500_17DA, 0x2500_17DE, DOCTRINES_TABLE]
    );
    let from = use_it(&mut h, b, me, 0x8130_0000_0000_0403);
    assert!(updates(&h, b, from).is_empty(), "a stone recorded twice");
    let from = use_it(&mut h, b, me, 0x8130_0000_0000_0402);
    let told = updates(&h, b, from);
    assert_eq!((told[0].0, told[0].1.phase), (0x2000_0000, 1));
    assert_eq!(
        entries(&told[0].1.journal),
        vec![0x2500_17DE, DOCTRINES_TABLE]
    );
    // The last stone: the next phase, its own journal.
    let from = use_it(&mut h, b, me, 0x8130_0000_0000_0404);
    let told = updates(&h, b, from);
    assert_eq!(told.len(), 1);
    assert_eq!(
        (told[0].0, told[0].1.id, told[0].1.phase),
        (0x2000_0002, DOCTRINES, 2)
    );
    assert_eq!(
        (told[0].1.journal.string_id, told[0].1.journal.table),
        (JOURNAL[1], DOCTRINES_TABLE)
    );
    let p = h.server.players.values().next().expect("a player");
    let listed = yalain_world::quests::list(&h.server.quests, &p.play.quests, 0.0, 0.0);
    assert_eq!(
        u32::from_le_bytes(listed[..4].try_into().expect("a count")),
        1,
        "the subquests are never listed"
    );
}

/// A quest that failed is not given again while its wait runs: the shrine says its refusal and
/// nothing more; once the wait is over it gives the quest again.
/// Behaviour: server.quests.failed-waits-to-be-given-again
#[test]
fn a_failed_quest_is_given_again_only_once_its_wait_is_over() {
    const MIST: u32 = 0x4000_005B;
    const SHRINE: u32 = 0x4700_31C6;
    const GO_AWAY: u32 = 0x6369_1D8F;
    let mut h = Harness::new();
    h.server.content = ContentPack {
        objects: vec![object(0x8130_0000_0000_0501, SHRINE, [61.0, 241.0, -44.6])],
    };
    h.server.quests = QuestTable {
        quests: vec![QuestDef {
            retry: 72_000.0,
            ..quest(MIST, 0x2500_16C8, 2)
        }],
        steps: vec![step(MIST, 0, Trigger::Use(SHRINE), Update::Bestow)],
    };
    h.server.talk = TalkTable {
        lines: vec![line(SHRINE, GO_AWAY, Role::RefuseUse, How::Emote, None)],
    };
    let (b, me) = h.player_in_world("one", "Elara");
    let failed = |h: &mut Harness, ago: i64| {
        let at = h.unix - ago;
        for p in h.server.players.values_mut() {
            p.play.quests = vec![QuestState {
                quest: MIST,
                status: status::FAILED,
                phase: 0,
                started: at - 7200,
                updated: at,
            }];
        }
    };
    failed(&mut h, 69_197);
    let from = use_it(&mut h, b, me, 0x8130_0000_0000_0501);
    assert!(
        updates(&h, b, from).is_empty(),
        "not given while its wait runs"
    );
    assert_eq!(
        texts(&h, b, from),
        vec![(SHOW_TEXT, GO_AWAY)],
        "the refusal"
    );
    failed(&mut h, 72_001);
    let from = use_it(&mut h, b, me, 0x8130_0000_0000_0501);
    let given = updates(&h, b, from);
    assert_eq!(given.len(), 1);
    assert_eq!(given[0].0, 0x1000_0000, "given again");
}

/// A quest with a least level is not given to a player under it: the person says its refusal of
/// that quest instead, and nothing of its other lines; at the level it gives the quest.
/// Behaviour: server.quests.least-level-refused
#[test]
fn a_quest_is_not_given_under_its_least_level_and_the_person_says_why() {
    const TREE: u32 = 0x4000_002C;
    const DERU: u32 = 0x4700_2FD9;
    const TOO_GREEN: u32 = 0xC4F9_188E;
    const OTHER: u32 = 0x6369_1D8F;
    let mut h = Harness::new();
    h.server.content = ContentPack {
        objects: vec![object(0x8130_0000_0000_0601, DERU, [61.0, 241.0, -44.6])],
    };
    h.server.quests = QuestTable {
        quests: vec![QuestDef {
            min_level: 25,
            ..quest(TREE, 0x2500_14F5, 2)
        }],
        steps: vec![step(TREE, 0, Trigger::Use(DERU), Update::Bestow)],
    };
    h.server.talk = TalkTable {
        lines: vec![
            line(DERU, TOO_GREEN, Role::RefuseUse, How::Tell, Some(TREE)),
            line(DERU, OTHER, Role::RefuseUse, How::Tell, None),
        ],
    };
    let (b, me) = h.player_in_world("one", "Elara");
    let at_level = |h: &mut Harness, level: i32| {
        for p in h.server.players.values_mut() {
            yalain_world::quals::set_int(&mut p.qualities, yalain_world::examine::LEVEL, level);
        }
    };
    at_level(&mut h, 18);
    let from = use_it(&mut h, b, me, 0x8130_0000_0000_0601);
    assert!(updates(&h, b, from).is_empty(), "not given at level 18");
    assert_eq!(texts(&h, b, from), vec![(HEAR_TELL, TOO_GREEN)]);
    at_level(&mut h, 25);
    let from = use_it(&mut h, b, me, 0x8130_0000_0000_0601);
    assert_eq!(updates(&h, b, from).len(), 1, "given at level 25");
    assert!(
        !texts(&h, b, from).contains(&(HEAR_TELL, TOO_GREEN)),
        "and not refused"
    );
}

/// A quest that fails when its player dies fails as the player dies, told before the death's
/// message; one that does not is kept.
/// Behaviour: server.quests.fails-on-death
#[test]
fn a_quest_that_fails_on_death_fails_as_its_player_dies() {
    const MIST: u32 = 0x4000_005B;
    const KEPT: u32 = 0x4000_0016;
    let mut h = Harness::new();
    h.server.quests = QuestTable {
        quests: vec![
            QuestDef {
                fails_on_death: true,
                once: false,
                retry: 72_000.0,
                ..quest(MIST, 0x2500_16C8, 2)
            },
            quest(KEPT, 0x2500_2A34, 2),
        ],
        steps: Vec::new(),
    };
    let (b, _me) = h.player_in_world("one", "Elara");
    let world = *h.server.players.keys().next().expect("a player");
    for p in h.server.players.values_mut() {
        for q in [MIST, KEPT] {
            p.play.quests.push(QuestState {
                quest: q,
                status: status::UNDERWAY,
                phase: 1,
                started: 0,
                updated: 0,
            });
        }
    }
    let from = h.bots[b].inbox.len();
    let clock = h.clock();
    yalain_world::death::player_dies(&mut h.server, &mut h.net, world, None, clock);
    h.run(5);
    let order: Vec<u32> = calls_of(&h, b, from)
        .into_iter()
        .map(|(f, _)| f)
        .filter(|f| *f == UPDATE_QUEST || *f == DEATH_MESSAGE)
        .collect();
    assert_eq!(
        order,
        vec![UPDATE_QUEST, DEATH_MESSAGE],
        "failed, then the message"
    );
    let told = updates(&h, b, from);
    assert_eq!(
        (told[0].0, told[0].1.id, told[0].1.status),
        (0x4000_0000, MIST, status::FAILED)
    );
    let p = h.server.players.values().next().expect("a player");
    let kept = p
        .play
        .quests
        .iter()
        .find(|q| q.quest == KEPT)
        .expect("held");
    assert_eq!(kept.status, status::UNDERWAY, "the other kept");
}

/// A player's quests, each (quest, status, phase) given `ago` seconds before now.
fn holding(h: &mut Harness, quests: &[(u32, u32, u32)], ago: i64) {
    let at = h.unix - ago;
    for p in h.server.players.values_mut() {
        p.play.quests = quests
            .iter()
            .map(|(quest, status, phase)| QuestState {
                quest: *quest,
                status: *status,
                phase: *phase,
                started: at,
                updated: at,
            })
            .collect();
    }
}

/// A done quest with no wait is given again at once; one its definition says is done once is
/// never given again.
/// Behaviour: server.quests.given-again-no-wait
#[test]
fn a_done_quest_with_no_wait_is_given_again_at_once_unless_it_is_done_once() {
    const AGAIN: u32 = 0x4000_018F;
    const ONCE: u32 = 0x4000_0088;
    const ORLEN: u32 = 0x4700_31B4;
    const GUARD: u32 = 0x4700_2DFD;
    let mut h = Harness::new();
    h.server.content = ContentPack {
        objects: vec![
            object(0x8130_0000_0000_0701, ORLEN, [61.0, 241.0, -44.6]),
            object(0x8130_0000_0000_0702, GUARD, [62.0, 241.0, -44.6]),
        ],
    };
    h.server.quests = QuestTable {
        quests: vec![
            quest(AGAIN, 0x2500_16BE, 3),
            QuestDef {
                once: true,
                ..quest(ONCE, 0x2500_17EE, 3)
            },
        ],
        steps: vec![
            step(AGAIN, 0, Trigger::Use(ORLEN), Update::Bestow),
            step(ONCE, 0, Trigger::Use(GUARD), Update::Bestow),
        ],
    };
    let (b, me) = h.player_in_world("one", "Elara");
    holding(
        &mut h,
        &[(AGAIN, status::COMPLETED, 0), (ONCE, status::COMPLETED, 0)],
        1,
    );
    let from = use_it(&mut h, b, me, 0x8130_0000_0000_0701);
    let given = updates(&h, b, from);
    assert_eq!(given.len(), 1, "given again at once");
    assert_eq!((given[0].0, given[0].1.id), (0x1000_0000, AGAIN));
    holding(&mut h, &[(ONCE, status::COMPLETED, 0)], 10_000_000);
    let from = use_it(&mut h, b, me, 0x8130_0000_0000_0702);
    assert!(updates(&h, b, from).is_empty(), "done once: never again");
}

/// A person who gives several quests gives one of them at a use (not always the same one), none
/// while one is under way, and, when they have no wait of their own, none until two hours after
/// the last it gave: until then it says its too-soon line. A person whose quests have waits gives
/// the next in the use that completes one.
/// Behaviour: server.quests.one-giving-at-a-time
#[test]
fn a_person_with_several_quests_gives_one_at_a_time_and_none_until_its_wait_is_over() {
    const STUBBY: u32 = 0x4700_335B;
    const QUESTS: [u32; 3] = [0x4000_0313, 0x4000_0336, 0x4000_037A];
    const TOO_SOON: u32 = 0x3867_28F6;
    const GO_AWAY: u32 = 0x6369_1D8F;
    const STARTER: u32 = 0x4700_31E6;
    const FIRST: u32 = 0x4000_0016;
    const NEXT: u32 = 0x4000_00C0;
    let mut h = Harness::new();
    h.server.content = ContentPack {
        objects: vec![
            object(0x8130_0000_0000_0801, STUBBY, [61.0, 241.0, -44.6]),
            object(0x8130_0000_0000_0802, STARTER, [62.0, 241.0, -44.6]),
        ],
    };
    let mut quests: Vec<QuestDef> = QUESTS
        .iter()
        .map(|q| QuestDef {
            duration: 7200.0,
            ..quest(*q, 0x2500_17D4, 2)
        })
        .collect();
    let mut steps: Vec<Step> = QUESTS
        .iter()
        .map(|q| step(*q, 0, Trigger::Use(STUBBY), Update::Bestow))
        .collect();
    for q in [FIRST, NEXT] {
        quests.push(QuestDef {
            retry: 72_000.0,
            ..quest(q, 0x2500_1409, 2)
        });
    }
    steps.push(step(FIRST, 1, Trigger::Use(STARTER), Update::Complete));
    steps.push(step(NEXT, 0, Trigger::Use(STARTER), Update::Bestow));
    steps.push(step(FIRST, 0, Trigger::Use(STARTER), Update::Bestow));
    h.server.quests = QuestTable { quests, steps };
    h.server.talk = TalkTable {
        lines: vec![
            line(STUBBY, TOO_SOON, Role::TooSoon, How::Tell, None),
            line(STUBBY, GO_AWAY, Role::RefuseUse, How::Tell, None),
            line(STUBBY, GO_AWAY + 1, Role::Use, How::Tell, None),
            line(STUBBY, GO_AWAY + 2, Role::Use, How::Tell, None),
        ],
    };
    let (b, me) = h.player_in_world("one", "Elara");
    let stubby = 0x8130_0000_0000_0801;
    let mut picked = Vec::new();
    for _ in 0..8 {
        holding(&mut h, &[], 0);
        let from = use_it(&mut h, b, me, stubby);
        let given = updates(&h, b, from);
        assert_eq!(given.len(), 1, "one quest at a use");
        assert_eq!(given[0].0, 0x1000_0000);
        picked.push(given[0].1.id);
    }
    picked.sort_unstable();
    picked.dedup();
    assert!(picked.len() >= 2, "not always the same one: {picked:x?}");
    let one = picked[0];
    holding(&mut h, &[(one, status::UNDERWAY, 1)], 60);
    let from = use_it(&mut h, b, me, stubby);
    assert!(
        updates(&h, b, from).is_empty(),
        "none while one is under way"
    );
    for ago in [100, 3_000, 7_000] {
        holding(&mut h, &[(one, status::COMPLETED, 0)], ago);
        let from = use_it(&mut h, b, me, stubby);
        assert!(updates(&h, b, from).is_empty(), "none within the wait");
        assert_eq!(texts(&h, b, from), vec![(HEAR_TELL, TOO_SOON)], "too soon");
    }
    holding(&mut h, &[(one, status::COMPLETED, 0)], 7_300);
    let from = use_it(&mut h, b, me, stubby);
    let given = updates(&h, b, from);
    assert_eq!(given.len(), 1, "one again once the wait is over");
    assert_eq!(given[0].0, 0x1000_0000);
    // Quests with waits of their own: the use that completes one gives the next.
    holding(&mut h, &[(FIRST, status::UNDERWAY, 1)], 60);
    let from = use_it(&mut h, b, me, 0x8130_0000_0000_0802);
    let got: Vec<(u32, u32)> = updates(&h, b, from)
        .iter()
        .map(|(k, r)| (*k, r.id))
        .collect();
    assert_eq!(got, vec![(0x3000_0000, FIRST), (0x1000_0000, NEXT)]);
}
