//! Gives: an item of the pack dragged onto a person or an object. The walk and the handing over,
//! the answer; an item a quest step takes kept, one no step wants handed back with the person's
//! refusal; a person busy with another player's give, a walk that cannot get there; a part of a
//! stack; a quest moved on by an item a give brought; a vitae tap; a hero made.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::native_object::QuestInfo;
use auberean_script::wire::In;
use yalain_content::quests::{Need, QuestDef, QuestTable, Step, Subquests, Trigger, Update};
use yalain_content::talk::{How, Line, Role, TalkTable, TalkUpdate, When, Words};
use yalain_content::{ContentObject, ContentPack, Description, Facts, Kind, Props};
use yalain_testkit::play::{report, server_call};
use yalain_testkit::Harness;
use yalain_world::calls::{self, Node, Value};
use yalain_world::gives::{self, status, GIVE, HANDED_AFTER, HANDING};
use yalain_world::inventory::{move_graph, Move};

const MOVE_DONE: u32 = 0x820E_005E;
const UPDATE_QUEST: u32 = 0x8213_0024;
const HEAR_TELL: u32 = 0x8205_0017;
const CONTAINER: u32 = 258;

/// A person and an object beside the start, and one farther off.
const PERSON: u64 = 0x8130_0000_0000_0701;
const OBJECT: u64 = 0x8130_0000_0000_0702;
const FAR: u64 = 0x8130_0000_0000_0703;
const PERSON_T: u32 = 0x4700_303A;
const OBJECT_T: u32 = 0x4700_3366;
const FAR_T: u32 = 0x4700_3042;

/// A quest and the items given in it.
const QUEST: u32 = 0x4000_024C;
const ASHES: u32 = 0x4700_1502;
const SLIVER: u32 = 0x4700_080F;
const IMBUED: u32 = 0x4700_1513;
const JUNK: u32 = 0x4700_2108;

fn place(id: u64, template: u32, kind: Kind, origin: [f32; 3]) -> ContentObject {
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
        kind,
        label: String::new(),
        placed: "authored for a test".into(),
        destination: None,
        facts: Facts::default(),
        walks: None,
        gate: None,
    }
}

fn quest(id: u32, phases: u32) -> QuestDef {
    QuestDef {
        id,
        name: (0xA860_7CEC, 0x2500_2958),
        description: (0x6C45_9364, 0x2500_2958),
        icon: 0x4100_034B,
        play_fx: true,
        journals: (0..phases)
            .map(|i| (0x2E5C_1E52 + i, 0x2500_2958))
            .collect(),
        challenge: -999,
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
        fellows: yalain_content::quests::Fellows::default(),
        last_waits: false,
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

fn line(who: u32, string: u32, role: Option<Role>, quest: Option<u32>) -> Line {
    Line {
        who,
        object: None,
        words: Words::Ref(string, 0x2500_2958),
        role,
        how: How::Tell,
        when: When::Give,
        quest,
        update: quest.map(|_| TalkUpdate::Advance),
        phase: None,
        item: None,
        order: 0,
        measured: true,
        placed: "authored for a test".into(),
    }
}

/// A world of a person, an object and a far person; the quest at its second phase gives the
/// ashes to the person (kept, the third phase); the sliver at the third completes it, the person
/// giving the imbued sliver back.
fn world() -> Harness {
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    h.server.catalog.insert(yalain_testkit::test_template(
        IMBUED,
        0x1067,
        Props::default(),
    ));
    h.server.content = ContentPack {
        objects: vec![
            place(PERSON, PERSON_T, Kind::Npc, [60.0, 241.0, -44.6]),
            place(OBJECT, OBJECT_T, Kind::Static, [58.0, 241.0, -44.6]),
            place(FAR, FAR_T, Kind::Npc, [59.0, 260.0, -44.6]),
        ],
        ..ContentPack::default()
    };
    h.server.quests = QuestTable {
        quests: vec![quest(QUEST, 3)],
        steps: vec![
            Step {
                takes: vec![ASHES],
                ..step(QUEST, 2, Trigger::Give(ASHES, PERSON_T), Update::Advance(3))
            },
            Step {
                takes: vec![SLIVER],
                gives: vec![IMBUED],
                ..step(QUEST, 3, Trigger::Give(SLIVER, PERSON_T), Update::Complete)
            },
        ],
    };
    h.server.talk = TalkTable {
        lines: vec![
            line(PERSON_T, 0xE47F_CDAA, None, Some(QUEST)),
            line(PERSON_T, 0x1BCB_4AED, Some(Role::RefuseGive), None),
            line(OBJECT_T, 0x2E28_0FF0, Some(Role::Use), None),
        ],
    };
    h
}

/// A player in the world whose client has drawn it.
fn player(h: &mut Harness, account: &str, name: &str) -> (usize, u64) {
    let (b, me) = h.player_in_world(account, name);
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    (b, me)
}

fn world_of(h: &Harness, me: u64) -> yalain_net::ConnId {
    *h.server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .map(|(c, _)| c)
        .expect("in the world")
}

/// Put so many of a template in a player's pack (a template the catalog lacks made up, stacking
/// when more than one); returns the item and its slot.
fn hold(h: &mut Harness, me: u64, template: u32, quantity: u32) -> (u64, i32) {
    if h.server.catalog.get(template).is_none() {
        let mut props = Props::default();
        props.set("Item_MaxStackSize", yalain_content::Prop::Int(100));
        h.server
            .catalog
            .insert(yalain_testkit::test_template(template, 0x1067, props));
    }
    let world = world_of(h, me);
    let item = yalain_world::inventory::give(&mut h.server, &mut h.net, world, template, quantity)
        .expect("made");
    h.run(3);
    let slot = h.server.players[&world]
        .play
        .items
        .iter()
        .find(|x| x.id == item)
        .map_or(0, |x| x.slot);
    (item, i32::try_from(slot).unwrap_or(0))
}

/// The client's request to give `quantity` of `item` from `slot` to `target`, as the 2017
/// clients sent it.
fn give(
    me: u64,
    item: u64,
    slot: i32,
    target: u64,
    quantity: u32,
) -> auberean_protocol::ClientMessage {
    let m = Move {
        move_type: GIVE,
        merge: true,
        unlock: true,
        check_take: true,
        all_taken: true,
        grab: true,
        contain: true,
        auto_merge: true,
        target,
        actual_target: target,
        merge_container: target,
        from: me,
        actual_from: me,
        item,
        split_item: item,
        quantity,
        quantity_left: quantity,
        from_slot: slot,
        actual_from_slot: slot,
        ..Move::default()
    };
    server_call(yalain_world::drops::function::MOVE_ITEM, &[move_graph(&m)])
}

/// A move answered: its status, merge flag, animation played, from, target and item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Answer {
    status: u32,
    merge: bool,
    played: bool,
    move_type: u32,
    from: u64,
    target: u64,
    item: u64,
}

fn answers(h: &Harness, b: usize, from: usize) -> Vec<Answer> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) if c.function.0 == MOVE_DONE => c
                .args
                .as_ref()
                .and_then(|a| calls::values(&a.0).ok())
                .and_then(|v| {
                    v.iter().find_map(|x: &Value| match x.root() {
                        Some(Node::Script { slots, .. }) => Some(slots.clone()),
                        _ => None,
                    })
                }),
            _ => None,
        })
        .map(|s| {
            let long = |i: usize| u64::from(s[i].1) | u64::from(s[i + 1].1) << 32;
            Answer {
                status: s[40].1,
                merge: s[0].1 != 0,
                played: s[41].1 != 0,
                move_type: s[12].1,
                from: long(16),
                target: long(2),
                item: long(44),
            }
        })
        .collect()
}

/// The quest updates a client received since `from`: each one's kind, quest and phase.
fn updates(h: &Harness, b: usize, from: usize) -> Vec<(u32, u32, u32)> {
    h.calls(b, from)
        .into_iter()
        .filter(|(f, _)| *f == UPDATE_QUEST)
        .filter_map(|(_, v)| {
            let record = match v.get(1)?.root()? {
                Node::Native { bytes, .. } => QuestInfo::unpack(&mut In::new(bytes)).ok()?,
                _ => return None,
            };
            match v.last() {
                Some(Value::Int(k)) => Some((*k, record.id, record.phase)),
                _ => None,
            }
        })
        .collect()
}

/// The lines a person told a client since `from`.
fn told(h: &Harness, b: usize, from: usize) -> Vec<u32> {
    h.calls(b, from)
        .into_iter()
        .filter(|(f, _)| *f == HEAR_TELL)
        .filter_map(|(_, v)| {
            v.iter().find_map(|x| match x.root() {
                Some(Node::TextInfo(t)) => Some(t.string_id),
                _ => None,
            })
        })
        .collect()
}

fn at_phase(h: &mut Harness, quest: u32, phase: u32) {
    for p in h.server.players.values_mut() {
        p.play.quests.retain(|q| q.quest != quest);
        p.play.quests.push(yalain_world::state::QuestState {
            quest,
            status: yalain_world::quests::status::UNDERWAY,
            phase,
            started: 0,
            updated: 0,
        });
    }
}

fn holds(h: &Harness, me: u64, item: u64) -> bool {
    h.server.players[&world_of(h, me)]
        .play
        .items
        .iter()
        .any(|x| x.id == item)
}

/// How many of the harness's steps make `seconds` (rounded up).
fn steps(seconds: f64) -> usize {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = (seconds / yalain_testkit::STEP).ceil() as usize;
    n
}

/// A give is walked to 2 m of the person (the order sent even when the player is there), the
/// player plays its handing-over with the item's look in its hand in the walk's context, seen by
/// the player beside it, and the item's container is cleared; the move is answered as done, the
/// animation played, 1.31 s later and not before.
///
/// Behaviour: server.gives.walked-handed-over-and-answered
#[test]
fn an_item_given_is_walked_to_handed_over_and_answered() {
    let mut h = world();
    let (b, me) = player(&mut h, "one", "Elara");
    let (w, _) = player(&mut h, "two", "Bran");
    let (item, slot) = hold(&mut h, me, JUNK, 1);
    let (from, watched) = (h.bots[b].inbox.len(), h.bots[w].inbox.len());
    h.bots[b].send(true, queue::WEENIE, &give(me, item, slot, PERSON, 1));
    h.run(steps(HANDED_AFTER) - 4);
    let seen = h.world_messages(b, from);
    let walk = seen.iter().find_map(|m| match m {
        ServerMessage::MoveTo(e) if e.sender.id == me && e.action.params.flags == 0x61 => {
            Some(e.action.params.clone())
        }
        _ => None,
    });
    let walk = walk.expect("walked");
    assert_eq!(
        (walk.desired_distance, walk.max_success_distance),
        (2.0, 2.0)
    );
    let handing = seen.iter().find_map(|m| match m {
        ServerMessage::PlayBehavior(e) if e.sender.id == me && e.action.behavior == HANDING => {
            Some(e.action.clone())
        }
        _ => None,
    });
    let handing = handing.expect("handed over");
    assert_eq!(handing.target, item);
    assert_eq!(handing.context, walk.context, "in the walk's context");
    assert!(
        h.world_messages(w, watched).iter().any(|m| matches!(m,
            ServerMessage::PlayBehavior(e) if e.action.behavior == HANDING)),
        "seen by the player beside it"
    );
    assert!(
        seen.iter().any(|m| matches!(m,
            ServerMessage::UpdateInstanceIdVisual(q) if q.object.id == item
                && q.quality == CONTAINER && q.value == 0)),
        "the container cleared"
    );
    assert!(answers(&h, b, from).is_empty(), "not answered yet");
    h.run(8);
    let a = answers(&h, b, from);
    assert_eq!(a.len(), 1, "answered: {a:?}");
    assert_eq!(
        (a[0].status, a[0].merge, a[0].played, a[0].move_type),
        (0, false, true, GIVE)
    );
    assert_eq!((a[0].item, a[0].target), (item, PERSON));
}

/// An item a quest step takes is kept: it leaves the pack and the world (forgotten, and destroyed
/// in the person), the person says its line with the update, and the quest moves on; an item the
/// step gives comes from the person. Nothing is handed back.
///
/// Behaviour: server.gives.a-step-keeps-the-item
#[test]
fn an_item_a_step_takes_is_kept_and_moves_the_quest() {
    let mut h = world();
    let (b, me) = player(&mut h, "one", "Elara");
    at_phase(&mut h, QUEST, 2);
    let (ashes, slot) = hold(&mut h, me, ASHES, 1);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &give(me, ashes, slot, PERSON, 1));
    h.run(steps(HANDED_AFTER) + 8);
    assert!(!holds(&h, me, ashes), "kept");
    assert!(h.server.things.get(ashes).is_none(), "out of the world");
    let seen = h.world_messages(b, from);
    assert!(
        seen.iter()
            .any(|m| matches!(m, ServerMessage::ForgetObject(o) if o.0.id == ashes)),
        "forgotten"
    );
    assert!(
        seen.iter().any(|m| matches!(m,
            ServerMessage::VisualObjectCall(c) if c.object.id == PERSON
                && c.call.function.0 == gives::DESTROY_CONTAINED)),
        "destroyed in the person"
    );
    assert_eq!(told(&h, b, from), vec![0xE47F_CDAA], "its line");
    assert_eq!(updates(&h, b, from), vec![(0x2000_0003, QUEST, 3)]);
    h.run(steps(gives::HANDED_BACK_AFTER) + 4);
    assert_eq!(answers(&h, b, from).len(), 1, "nothing handed back");
    assert!(
        !h.world_messages(b, from).iter().any(|m| matches!(m,
            ServerMessage::PlayBehavior(e) if e.sender.id == PERSON && e.action.behavior == HANDING)),
        "the person hands nothing back"
    );
    // The sliver: the quest completed, the imbued sliver made from the person.
    let (sliver, slot) = hold(&mut h, me, SLIVER, 1);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &give(me, sliver, slot, PERSON, 1));
    h.run(steps(HANDED_AFTER) + 8);
    assert_eq!(updates(&h, b, from), vec![(0x3000_0000, QUEST, 0)]);
    let a = answers(&h, b, from);
    let made = a.iter().find(|x| x.item != sliver).expect("an item given");
    assert_eq!(
        (made.move_type, made.from, made.merge),
        (GIVE, PERSON, true)
    );
    assert!(h.server.players[&world_of(&h, me)]
        .play
        .items
        .iter()
        .any(|x| x.template == IMBUED));
}

/// An item no step wants (or one given at the wrong phase) is answered as given, the person says
/// its refusal of a give and plays its handing-over, and 1.30 s later the item is back in the
/// pack, from the person; an object hands it back at once with its own line. No quest moves.
///
/// Behaviour: server.gives.no-step-hands-back
#[test]
fn an_item_no_step_wants_is_handed_back_with_the_persons_refusal() {
    let mut h = world();
    let (b, me) = player(&mut h, "one", "Elara");
    at_phase(&mut h, QUEST, 1);
    let (ashes, slot) = hold(&mut h, me, ASHES, 1);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &give(me, ashes, slot, PERSON, 1));
    h.run(steps(HANDED_AFTER) + 8);
    assert_eq!(told(&h, b, from), vec![0x1BCB_4AED], "its refusal");
    assert!(updates(&h, b, from).is_empty());
    assert!(
        h.world_messages(b, from).iter().any(|m| matches!(m,
            ServerMessage::PlayBehavior(e) if e.sender.id == PERSON && e.action.behavior == HANDING
                && e.action.target == ashes)),
        "the person hands it back"
    );
    assert_eq!(answers(&h, b, from).len(), 1, "not back yet");
    h.run(steps(gives::HANDED_BACK_AFTER));
    let a = answers(&h, b, from);
    assert_eq!(a.len(), 2);
    assert_eq!(
        (a[1].from, a[1].target, a[1].item, a[1].status),
        (PERSON, me, ashes, 0)
    );
    assert!(holds(&h, me, ashes), "in the pack");
    // An object: its line at once and the item back in the same moment.
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &give(me, ashes, slot, OBJECT, 1));
    h.run(steps(HANDED_AFTER) + 8);
    assert_eq!(told(&h, b, from), vec![0x2E28_0FF0]);
    let a = answers(&h, b, from);
    assert_eq!(a.len(), 2, "answered and back at once: {a:?}");
    assert_eq!(a[1].from, OBJECT);
    assert!(holds(&h, me, ashes));
}

/// A person takes one give at a time: a second player's give to it while the first's is under way
/// is refused at once (target busy), with no walk; an object takes both. A give asked while the
/// giver's own is under way is refused as its actions locked.
///
/// Behaviour: server.gives.one-give-at-a-time
#[test]
fn a_person_takes_one_give_at_a_time() {
    let mut h = world();
    let (b, me) = player(&mut h, "one", "Elara");
    let (c, other) = player(&mut h, "two", "Bran");
    let (mine, slot) = hold(&mut h, me, JUNK, 1);
    let (theirs, their_slot) = hold(&mut h, other, JUNK, 1);
    h.bots[b].send(true, queue::WEENIE, &give(me, mine, slot, PERSON, 1));
    h.run(2);
    let from = h.bots[c].inbox.len();
    h.bots[c].send(
        true,
        queue::WEENIE,
        &give(other, theirs, their_slot, PERSON, 1),
    );
    h.run(6);
    let a = answers(&h, c, from);
    assert_eq!(a.len(), 1);
    assert_eq!(a[0].status, status::TARGET_BUSY);
    assert!(
        !h.world_messages(c, from).iter().any(|m| matches!(m,
            ServerMessage::MoveTo(e) if e.sender.id == other)),
        "no walk"
    );
    let mine_from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &give(me, mine, slot, PERSON, 1));
    h.run(6);
    assert_eq!(answers(&h, b, mine_from)[0].status, status::ACTIONS_LOCKED);
    h.run(steps(HANDED_AFTER + gives::HANDED_BACK_AFTER) + 4);
    // Both at once to the object.
    let (fb, fc) = (h.bots[b].inbox.len(), h.bots[c].inbox.len());
    h.bots[b].send(true, queue::WEENIE, &give(me, mine, slot, OBJECT, 1));
    h.run(2);
    h.bots[c].send(
        true,
        queue::WEENIE,
        &give(other, theirs, their_slot, OBJECT, 1),
    );
    h.run(steps(HANDED_AFTER) + 8);
    assert_eq!(answers(&h, b, fb)[0].status, 0);
    assert_eq!(answers(&h, c, fc)[0].status, 0);
}

/// A walk to give that ends at rest out of reach (the player stands where it last stood still) is
/// refused as one that could not get there: no handing over, the item never leaves the pack.
///
/// Behaviour: server.gives.cannot-get-there
#[test]
fn a_give_whose_walk_ends_out_of_reach_is_refused() {
    let mut h = world();
    let (b, me) = player(&mut h, "one", "Elara");
    let (item, slot) = hold(&mut h, me, JUNK, 1);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &give(me, item, slot, FAR, 1));
    h.run(4);
    assert!(answers(&h, b, from).is_empty(), "walking");
    let at = h.server.players[&world_of(&h, me)].position;
    let here = auberean_primitives::Vec3::new(at.origin[0], at.origin[1] + 3.0, at.origin[2]);
    let stamp = h.server.players[&world_of(&h, me)].teleport_stamp;
    for _ in 0..2 {
        let idle = yalain_world::world::IDLE_MOTION;
        h.bots[b].send(true, queue::WEENIE, &report(at.cell, here, 0, idle, stamp));
        h.run(3);
    }
    let a = answers(&h, b, from);
    assert_eq!(a.len(), 1, "{a:?}");
    assert_eq!(a[0].status, status::CANT_GET_THERE);
    assert!(
        !h.world_messages(b, from).iter().any(|m| matches!(m,
            ServerMessage::PlayBehavior(e) if e.action.behavior == HANDING)),
        "not handed over"
    );
    assert!(holds(&h, me, item));
}

/// A part of a stack given to a step that keeps it is taken, the rest staying in the pack with
/// its quantity told.
///
/// Behaviour: server.gives.part-of-a-stack
#[test]
fn a_part_of_a_stack_given_is_taken_and_the_rest_stays() {
    let mut h = world();
    let (b, me) = player(&mut h, "one", "Elara");
    at_phase(&mut h, QUEST, 2);
    let (ashes, slot) = hold(&mut h, me, ASHES, 5);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &give(me, ashes, slot, PERSON, 2));
    h.run(steps(HANDED_AFTER) + 8);
    assert_eq!(updates(&h, b, from), vec![(0x2000_0003, QUEST, 3)]);
    let p = &h.server.players[&world_of(&h, me)];
    let left = p
        .play
        .items
        .iter()
        .find(|x| x.id == ashes)
        .expect("the rest");
    assert_eq!(left.quantity, 3);
    assert!(
        h.world_messages(b, from).iter().any(|m| matches!(m,
            ServerMessage::UpdateIntVisual(q) if q.object.id == ashes && q.value == 3)),
        "its quantity told"
    );
}

/// Items given that close a phase's subquests: each moves the quest's record with its phase kept,
/// the last moves it on (the doctrines handed to the Spirit, any order).
///
/// Behaviour: server.gives.subquests
#[test]
fn items_given_close_a_phases_subquests_and_the_last_moves_it_on() {
    const DOCTRINES: u32 = 0x4000_005E;
    const SUBS: [u32; 3] = [0x4000_005F, 0x4000_0060, 0x4000_0061];
    const ITEMS: [u32; 3] = [0x4700_1A54, 0x4700_1A55, 0x4700_1A53];
    let mut h = world();
    let mut quests = vec![QuestDef {
        subquests: vec![Subquests {
            phase: 2,
            quests: SUBS.to_vec(),
            any: false,
        }],
        ..quest(DOCTRINES, 3)
    }];
    let mut steps_ = Vec::new();
    for (q, item) in SUBS.iter().zip(ITEMS) {
        quests.push(quest(*q, 1));
        steps_.push(Step {
            takes: vec![item],
            requires: vec![(DOCTRINES, Need::Phase(2))],
            ..step(*q, 1, Trigger::Give(item, OBJECT_T), Update::Complete)
        });
    }
    // Moved to its second phase by a use, which gives its subquests.
    steps_.push(step(
        DOCTRINES,
        1,
        Trigger::Use(PERSON_T),
        Update::Advance(2),
    ));
    h.server.quests = QuestTable {
        quests,
        steps: steps_,
    };
    let (b, me) = player(&mut h, "one", "Elara");
    at_phase(&mut h, DOCTRINES, 1);
    let world = world_of(&h, me);
    let clock = h.clock();
    yalain_world::quests::triggered(
        &mut h.server,
        &mut h.net,
        world,
        Trigger::Use(PERSON_T),
        None,
        clock,
    );
    let mut told_ = Vec::new();
    for item in [ITEMS[2], ITEMS[0], ITEMS[1]] {
        let (id, slot) = hold(&mut h, me, item, 1);
        let from = h.bots[b].inbox.len();
        h.bots[b].send(true, queue::WEENIE, &give(me, id, slot, OBJECT, 1));
        h.run(steps(HANDED_AFTER) + 8);
        assert!(!holds(&h, me, id), "kept");
        told_.extend(updates(&h, b, from));
    }
    assert_eq!(
        told_,
        vec![
            (0x2000_0000, DOCTRINES, 2),
            (0x2000_0000, DOCTRINES, 2),
            (0x2000_0003, DOCTRINES, 3)
        ]
    );
}

/// An item a give's step hands the player that moves the same quest on when received moves it
/// after the step's own update: two records, in that order (the Land Claim for the ashes).
///
/// Behaviour: server.gives.received-moves-on
#[test]
fn an_item_a_give_brings_moves_its_quest_on_after_the_step() {
    const CLAIM_QUEST: u32 = 0x4000_032F;
    const SODDEN: u32 = 0x4700_14FF;
    const CLAIM: u32 = 0x4700_14F7;
    let mut h = world();
    h.server.quests = QuestTable {
        quests: vec![quest(CLAIM_QUEST, 8)],
        steps: vec![
            Step {
                takes: vec![SODDEN],
                gives: vec![CLAIM],
                ..step(
                    CLAIM_QUEST,
                    5,
                    Trigger::Give(SODDEN, PERSON_T),
                    Update::Advance(6),
                )
            },
            step(CLAIM_QUEST, 6, Trigger::Take(CLAIM), Update::Advance(7)),
        ],
    };
    let (b, me) = player(&mut h, "one", "Elara");
    at_phase(&mut h, CLAIM_QUEST, 5);
    let (item, slot) = hold(&mut h, me, SODDEN, 1);
    hold(&mut h, me, CLAIM, 1);
    // The claim made up for the catalog; the one held is let go.
    let world = world_of(&h, me);
    h.server
        .players
        .get_mut(&world)
        .expect("in")
        .play
        .items
        .retain(|x| x.template != CLAIM);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &give(me, item, slot, PERSON, 1));
    h.run(steps(HANDED_AFTER) + 8);
    assert_eq!(
        updates(&h, b, from),
        vec![(0x2000_0006, CLAIM_QUEST, 6), (0x2000_0007, CLAIM_QUEST, 7)]
    );
}

/// A step's tap of vitae leaves the player that share of its vitae (told, with what is still to
/// earn), its appearance played on the player.
///
/// Behaviour: server.gives.vitae-tap
#[test]
fn a_steps_vitae_tap_takes_a_tenth_of_the_vitae() {
    let mut h = world();
    h.server.quests.steps[0].effects = vec![gives::VITAE_TAPS[0].0];
    let (b, me) = player(&mut h, "one", "Elara");
    at_phase(&mut h, QUEST, 2);
    let world = world_of(&h, me);
    yalain_world::quals::set_float(
        &mut h.server.players.get_mut(&world).expect("in").qualities,
        yalain_world::death::vitae_q::VITAE,
        81.0,
    );
    let (ashes, slot) = hold(&mut h, me, ASHES, 1);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &give(me, ashes, slot, PERSON, 1));
    h.run(steps(HANDED_AFTER) + 8);
    let vitae = yalain_world::quals::float(
        &h.server.players[&world].qualities,
        yalain_world::death::vitae_q::VITAE,
    );
    assert!((vitae.expect("vitae") - 72.9).abs() < 1e-3, "{vitae:?}");
    assert!(
        h.world_messages(b, from).iter().any(|m| matches!(m,
            ServerMessage::DoFx(e) if e.sender.id == me && e.action.fx == gives::VITAE_TAPS[0].2)),
        "its appearance"
    );
}

/// A step that makes a hero runs when the transformation is through, 25.59 s after the give's
/// answer: the player a hero, the quest completed, the hero's text; before, only the
/// transformation's appearances, the gift kept.
///
/// Behaviour: server.gives.hero-transformation
#[test]
fn a_give_that_makes_a_hero_does_so_when_the_transformation_is_through() {
    const ELYSA: u32 = 0x4000_024A;
    const GEM: u32 = 0x4700_1505;
    let mut h = world();
    h.server.quests = QuestTable {
        quests: vec![quest(ELYSA, 4)],
        steps: vec![Step {
            takes: vec![GEM],
            effects: vec![gives::HERO_TRANSFORMATION],
            ..step(ELYSA, 4, Trigger::Give(GEM, OBJECT_T), Update::Complete)
        }],
    };
    let (b, me) = player(&mut h, "one", "Elara");
    at_phase(&mut h, ELYSA, 4);
    let (gem, slot) = hold(&mut h, me, GEM, 1);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &give(me, gem, slot, OBJECT, 1));
    h.run(steps(HANDED_AFTER) + 8);
    let world = world_of(&h, me);
    assert!(!holds(&h, me, gem), "kept");
    assert!(
        h.world_messages(b, from).iter().any(|m| matches!(m,
            ServerMessage::DoFx(e) if e.sender.id == me && e.action.fx == gives::TRANSFORMATION_FX[0])),
        "transforming"
    );
    h.run(steps(gives::TRANSFORMATION_SECONDS) - 10);
    assert!(updates(&h, b, from).is_empty(), "not yet");
    assert!(!yalain_world::focus::is_hero(
        &h.server.players[&world].qualities
    ));
    h.run(12);
    assert_eq!(updates(&h, b, from), vec![(0x3000_0000, ELYSA, 0)]);
    assert!(yalain_world::focus::is_hero(
        &h.server.players[&world].qualities
    ));
    assert!(
        h.texts(b, from)
            .iter()
            .any(|(_, id)| *id == gives::HERO_TEXT.0),
        "the hero's text"
    );
}
