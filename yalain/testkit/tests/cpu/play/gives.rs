//! Gives: an item of the pack dragged onto a person or an object. The walk and the handing over,
//! the answer; an item a quest step takes kept, one no step wants handed back with the person's
//! refusal; a person busy with another player's give, a walk that cannot get there; a part of a
//! stack; a quest moved on by an item a give brought; a vitae tap; a hero made.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::native_object::QuestInfo;
use auberean_script::wire::In;
use yalain_content::quests::{Give, Need, QuestDef, QuestTable, Step, Subquests, Trigger, Update};
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
            name_text: None,
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
        once: false,
        fellows: yalain_content::quests::Fellows::default(),
        last_waits: false,
        hero_only: false,
        pay: yalain_content::quests::CompletionPay::default(),
        required: None,
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
        wants: None,
        fellows: false,
        levels: None,
        xp_share: 0.0,
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
        gives: Vec::new(),
        rooms: Vec::new(),
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

/// A person is held by the give being handed to it, not by one still on its way: a player walking
/// to it from afar, however long, keeps no one else waiting.
///
/// Behaviour: server.gives.a-walk-holds-no-one
#[test]
fn a_give_still_on_its_way_keeps_no_one_from_the_person() {
    let mut h = world();
    let (b, me) = player(&mut h, "one", "Elara");
    let (c, other) = player(&mut h, "two", "Bran");
    let (mine, slot) = hold(&mut h, me, JUNK, 1);
    let (theirs, their_slot) = hold(&mut h, other, JUNK, 1);
    h.bots[b].send(true, queue::WEENIE, &give(me, mine, slot, FAR, 1));
    h.run(steps(60.0));
    assert!(
        h.server.players[&world_of(&h, me)].giving.is_some(),
        "on its way"
    );
    // The other stands no nearer: refused for that, in time, and never as the person being busy.
    let from = h.bots[c].inbox.len();
    h.bots[c].send(
        true,
        queue::WEENIE,
        &give(other, theirs, their_slot, FAR, 1),
    );
    h.run(4);
    assert!(
        answers(&h, c, from)
            .iter()
            .all(|a| a.status != status::TARGET_BUSY),
        "{:?}",
        answers(&h, c, from)
    );
    assert!(
        h.server.players[&world_of(&h, other)].giving.is_some(),
        "on its way too"
    );
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

/// The world's quest wanting exactly three ashes at its second phase, with the person's line for
/// the wrong number when asked for.
fn wanting_three(h: &mut Harness, with_line: bool) {
    h.server.quests.steps[0].wants = Some(3);
    if with_line {
        h.server.talk.lines.push(Line {
            quest: Some(QUEST),
            update: None,
            item: Some(ASHES),
            ..line(PERSON_T, WRONG_NUMBER, Some(Role::RefuseGive), None)
        });
    }
}

const WRONG_NUMBER: u32 = 0x4F33_0AB6;
const OTHER_WRONG_NUMBER: u32 = 0x65DE_CD8E;
const PLAIN_REFUSAL: u32 = 0x1BCB_4AED;

/// Give so many ashes to the person and wait until the person is through with them. Returns the
/// lines told and the quest updates.
fn give_ashes(
    h: &mut Harness,
    b: usize,
    me: u64,
    ashes: (u64, i32),
    count: u32,
) -> (Vec<u32>, Vec<(u32, u32, u32)>) {
    let from = h.bots[b].inbox.len();
    let m = give(me, ashes.0, ashes.1, PERSON, count);
    h.bots[b].send(true, queue::WEENIE, &m);
    h.run(steps(HANDED_AFTER) + steps(gives::HANDED_BACK_AFTER) + 8);
    (told(h, b, from), updates(h, b, from))
}

fn quantity_of(h: &Harness, me: u64, item: u64) -> Option<u32> {
    h.server.players[&world_of(h, me)]
        .play
        .items
        .iter()
        .find(|x| x.id == item)
        .map(|x| x.quantity)
}

/// A step wanting a count takes exactly that many: fewer, and a whole stack of more, are refused
/// with the person's own line for it and handed back, the quest not moved and the stack whole; that
/// count split off the same stack is taken, the rest staying.
///
/// Behaviour: server.gives.wants-a-count
#[test]
fn a_step_wanting_a_count_takes_that_many_and_refuses_any_other() {
    let mut h = world();
    wanting_three(&mut h, true);
    let (b, me) = player(&mut h, "one", "Elara");
    at_phase(&mut h, QUEST, 2);
    let ashes = hold(&mut h, me, ASHES, 5);

    for count in [2, 4, 5] {
        let (lines, moved) = give_ashes(&mut h, b, me, ashes, count);
        assert_eq!(lines, vec![WRONG_NUMBER], "a count of {count}");
        assert!(moved.is_empty(), "a count of {count} moves nothing");
        assert_eq!(quantity_of(&h, me, ashes.0), Some(5), "a count of {count}");
    }

    let (lines, moved) = give_ashes(&mut h, b, me, ashes, 3);
    assert!(
        !lines.contains(&WRONG_NUMBER),
        "the count wanted: {lines:?}"
    );
    assert_eq!(moved, vec![(0x2000_0003, QUEST, 3)]);
    assert_eq!(quantity_of(&h, me, ashes.0), Some(2), "the rest stays");
}

/// A whole stack of exactly the count wanted is taken whole, and leaves the pack.
///
/// Behaviour: server.gives.wants-a-count
#[test]
fn a_whole_stack_of_the_count_wanted_leaves_the_pack() {
    let mut h = world();
    wanting_three(&mut h, true);
    let (b, me) = player(&mut h, "one", "Elara");
    at_phase(&mut h, QUEST, 2);
    let ashes = hold(&mut h, me, ASHES, 3);
    let (_, moved) = give_ashes(&mut h, b, me, ashes, 3);
    assert_eq!(moved, vec![(0x2000_0003, QUEST, 3)]);
    assert!(!holds(&h, me, ashes.0));
}

/// A person with no line for the wrong number says its plain refusal, and hands the stack back
/// (a person given three fins she wanted one at a time).
///
/// Behaviour: server.gives.wrong-count-line
#[test]
fn a_wrong_count_hears_the_plain_refusal_from_a_person_with_no_line_for_it() {
    let mut h = world();
    wanting_three(&mut h, false);
    // Its lines for the wrong number of another item, and of this item for another quest.
    h.server.talk.lines.push(Line {
        quest: Some(QUEST),
        update: None,
        item: Some(JUNK),
        ..line(PERSON_T, OTHER_WRONG_NUMBER, Some(Role::RefuseGive), None)
    });
    h.server.talk.lines.push(Line {
        quest: Some(QUEST + 1),
        update: None,
        item: Some(ASHES),
        ..line(PERSON_T, OTHER_WRONG_NUMBER, Some(Role::RefuseGive), None)
    });
    let (b, me) = player(&mut h, "one", "Elara");
    at_phase(&mut h, QUEST, 2);
    let ashes = hold(&mut h, me, ASHES, 5);
    let from = h.bots[b].inbox.len();
    let (lines, moved) = give_ashes(&mut h, b, me, ashes, 5);
    assert_eq!(lines, vec![PLAIN_REFUSAL]);
    assert!(moved.is_empty());
    let a = answers(&h, b, from);
    assert_eq!(a.len(), 2, "answered, then back: {a:?}");
    assert_eq!((a[1].from, a[1].target), (PERSON, me));
    assert_eq!(quantity_of(&h, me, ashes.0), Some(5));
}

/// The line for the wrong number is for a player the person asked: with the quest at another
/// phase, or not under way, the same stack is only an item not wanted.
///
/// Behaviour: server.gives.wrong-count-line
#[test]
fn the_wrong_number_line_is_said_only_with_the_step_waiting_for_the_item() {
    let mut h = world();
    wanting_three(&mut h, true);
    let (b, me) = player(&mut h, "one", "Elara");
    let ashes = hold(&mut h, me, ASHES, 5);
    let (lines, _) = give_ashes(&mut h, b, me, ashes, 2);
    assert_eq!(lines, vec![PLAIN_REFUSAL], "the quest not under way");
    at_phase(&mut h, QUEST, 1);
    let (lines, _) = give_ashes(&mut h, b, me, ashes, 2);
    assert_eq!(lines, vec![PLAIN_REFUSAL], "the quest at another phase");
    assert_eq!(quantity_of(&h, me, ashes.0), Some(5));
    at_phase(&mut h, QUEST, 2);
    let (lines, _) = give_ashes(&mut h, b, me, ashes, 2);
    assert_eq!(lines, vec![WRONG_NUMBER], "its own line and no other");
}

/// A give runs only the steps whose count it fits: of two quests moved by the same item, the one
/// wanting a count not given stays where it is.
///
/// Behaviour: server.gives.wants-a-count
#[test]
fn a_give_runs_only_the_steps_whose_count_it_fits() {
    const OTHER: u32 = 0x4000_024D;
    let mut h = world();
    h.server.quests.quests.push(quest(OTHER, 2));
    h.server.quests.steps.insert(
        0,
        Step {
            wants: Some(3),
            ..step(OTHER, 1, Trigger::Give(ASHES, PERSON_T), Update::Advance(2))
        },
    );
    let (b, me) = player(&mut h, "one", "Elara");
    at_phase(&mut h, QUEST, 2);
    at_phase(&mut h, OTHER, 1);
    let ashes = hold(&mut h, me, ASHES, 5);
    h.server.quests.steps[1].takes.clear();
    let (_, moved) = give_ashes(&mut h, b, me, ashes, 2);
    assert_eq!(moved, vec![(0x2000_0003, QUEST, 3)], "only the one fitting");
}

/// A step wanting no count runs for whatever is given and takes all of it (a pile of stone: 82 in
/// a stack, or 1).
///
/// Behaviour: server.gives.any-count-taken-whole
#[test]
fn a_step_wanting_no_count_takes_whatever_is_given() {
    for (stack, count, left) in [(82, 82, None), (82, 1, Some(81))] {
        let mut h = world();
        let (b, me) = player(&mut h, "one", "Elara");
        at_phase(&mut h, QUEST, 2);
        let ashes = hold(&mut h, me, ASHES, stack);
        let (_, moved) = give_ashes(&mut h, b, me, ashes, count);
        assert_eq!(moved, vec![(0x2000_0003, QUEST, 3)], "{count} of {stack}");
        assert_eq!(quantity_of(&h, me, ashes.0), left, "{count} of {stack}");
    }
}

/// A part of a stack never leaves the pack: its container is not cleared at the handing over, and
/// it comes back merged with its container untouched, the stack as it was. A whole stack's is
/// cleared and told back.
///
/// Behaviour: server.gives.part-stays-in-the-pack
#[test]
fn a_part_of_a_stack_refused_comes_back_merged_with_its_container_untouched() {
    let containers = |h: &Harness, b: usize, from: usize, item: u64| -> usize {
        h.world_messages(b, from)
            .iter()
            .filter(|m| {
                matches!(m, ServerMessage::UpdateInstanceIdVisual(q)
                    if q.object.id == item && q.quality == CONTAINER)
            })
            .count()
    };
    let mut h = world();
    let (b, me) = player(&mut h, "one", "Elara");
    let (ashes, slot) = hold(&mut h, me, ASHES, 5);

    let from = h.bots[b].inbox.len();
    let (lines, _) = give_ashes(&mut h, b, me, (ashes, slot), 2);
    assert_eq!(lines, vec![PLAIN_REFUSAL]);
    assert_eq!(
        containers(&h, b, from, ashes),
        0,
        "the part: no container told"
    );
    let a = answers(&h, b, from);
    assert_eq!(a.len(), 2, "answered, then back: {a:?}");
    assert_eq!(
        (a[1].from, a[1].target, a[1].merge),
        (PERSON, me, true),
        "merged back"
    );
    assert_eq!(quantity_of(&h, me, ashes), Some(5));

    let from = h.bots[b].inbox.len();
    let (lines, _) = give_ashes(&mut h, b, me, (ashes, slot), 5);
    assert_eq!(lines, vec![PLAIN_REFUSAL]);
    assert_eq!(
        containers(&h, b, from, ashes),
        4,
        "the whole stack: cleared, then the person's, none and the player's"
    );
    let a = answers(&h, b, from);
    assert!(!a[1].merge, "the whole stack is not merged back");
    assert_eq!(quantity_of(&h, me, ashes), Some(5));
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
        gives: Vec::new(),
        rooms: Vec::new(),
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

/// The last of a phase's subquests is said as the quest's next phase where the person has a line
/// for that; the others, and a person with no such line, as the subquest's own.
///
/// Behaviour: server.gives.the-last-subquest-says-the-next-phase
#[test]
fn the_give_closing_a_phase_is_said_as_the_next_phase_where_a_line_says_it() {
    const MAIN: u32 = 0x4000_005E;
    const SUBS: [u32; 2] = [0x4000_005F, 0x4000_0060];
    const ITEMS: [u32; 2] = [0x4700_1A54, 0x4700_1A55];
    const OWN: u32 = 0x1111_0001;
    const NEXT: u32 = 0x1111_0002;
    let mut h = world();
    let mut quests = vec![QuestDef {
        subquests: vec![Subquests {
            phase: 2,
            quests: SUBS.to_vec(),
            any: false,
        }],
        ..quest(MAIN, 3)
    }];
    let mut steps_ = Vec::new();
    for (q, item) in SUBS.iter().zip(ITEMS) {
        quests.push(quest(*q, 1));
        steps_.push(Step {
            takes: vec![item],
            ..step(*q, 1, Trigger::Give(item, PERSON_T), Update::Complete)
        });
        h.server.talk.lines.push(Line {
            update: Some(TalkUpdate::Complete),
            ..line(PERSON_T, OWN, Some(Role::Complete), Some(*q))
        });
    }
    h.server.talk.lines.push(Line {
        phase: Some(3),
        ..line(PERSON_T, NEXT, Some(Role::Advance), Some(MAIN))
    });
    steps_.push(step(MAIN, 1, Trigger::Use(PERSON_T), Update::Advance(2)));
    h.server.quests = QuestTable {
        quests,
        steps: steps_,
        gives: Vec::new(),
        rooms: Vec::new(),
    };
    let (b, me) = player(&mut h, "one", "Elara");
    at_phase(&mut h, MAIN, 1);
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
    let mut said = Vec::new();
    for item in ITEMS {
        let (id, slot) = hold(&mut h, me, item, 1);
        let from = h.bots[b].inbox.len();
        h.bots[b].send(true, queue::WEENIE, &give(me, id, slot, PERSON, 1));
        h.run(steps(HANDED_AFTER) + 8);
        said.push(told(&h, b, from));
    }
    assert_eq!(said, vec![vec![OWN], vec![NEXT]]);
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
        gives: Vec::new(),
        rooms: Vec::new(),
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
        gives: Vec::new(),
        rooms: Vec::new(),
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

/// One who logs off with its gift given and the transformation under way (the linger is shorter)
/// has what the gift was for: it is a hero when it comes back, not out of its gift for nothing.
/// Behaviour: server.gives.a-step-under-way-runs-as-its-player-leaves
#[test]
fn a_hero_step_under_way_runs_as_its_player_leaves() {
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
        gives: Vec::new(),
        rooms: Vec::new(),
    };
    let (b, me) = player(&mut h, "one", "Elara");
    at_phase(&mut h, ELYSA, 4);
    let (gem, slot) = hold(&mut h, me, GEM, 1);
    h.bots[b].send(true, queue::WEENIE, &give(me, gem, slot, OBJECT, 1));
    h.run(steps(HANDED_AFTER) + 8);
    assert!(!holds(&h, me, gem), "kept");
    let w = world_of(&h, me);
    yalain_world::world::leave(&mut h.server, &mut h.net, w);
    h.run(2);
    let st = yalain_store::Store::world_state(&mut h.server.store, me)
        .expect("reads")
        .expect("kept");
    assert!(yalain_world::focus::is_hero(&st.qualities), "a hero");
}

const NAMED_LINE: u32 = 0x3DE6_3E47;
const TAKING_LINE: u32 = 0x4DAF_1F70;

/// A person with a plain refusal and lines naming an item.
fn with_lines_for_ashes(h: &mut Harness, role: Role) {
    h.server.talk.lines.push(Line {
        item: Some(ASHES),
        ..line(PERSON_T, NAMED_LINE, Some(role), None)
    });
}

/// A line naming an item is said of that item, over the plain refusal, and of no other.
///
/// Behaviour: server.gives.a-line-for-an-item
#[test]
fn a_line_naming_an_item_is_said_of_that_item_and_no_other() {
    let mut h = world();
    with_lines_for_ashes(&mut h, Role::RefuseGive);
    let (b, me) = player(&mut h, "one", "Elara");
    let junk = hold(&mut h, me, JUNK, 1);
    let (lines, _) = give_ashes(&mut h, b, me, junk, 1);
    assert_eq!(lines, vec![PLAIN_REFUSAL], "another item");
    let ashes = hold(&mut h, me, ASHES, 1);
    let (lines, _) = give_ashes(&mut h, b, me, ashes, 1);
    assert_eq!(lines, vec![NAMED_LINE], "the item it names");
}

/// Refused, a person does not say its line for taking the item.
///
/// Behaviour: server.gives.a-line-for-an-item
#[test]
fn a_line_for_taking_an_item_is_not_said_as_it_is_refused() {
    let mut h = world();
    h.server.talk.lines.push(Line {
        item: Some(ASHES),
        ..line(PERSON_T, TAKING_LINE, Some(Role::Accept), None)
    });
    let (b, me) = player(&mut h, "one", "Elara");
    let ashes = hold(&mut h, me, ASHES, 1);
    let (lines, _) = give_ashes(&mut h, b, me, ashes, 1);
    assert_eq!(lines, vec![PLAIN_REFUSAL]);
}

/// A person with no plain refusal and a line naming an item says nothing when given another.
///
/// Behaviour: server.gives.a-line-for-an-item
#[test]
fn a_person_with_only_a_line_for_another_item_says_nothing_of_this_one() {
    let mut h = world();
    h.server.talk.lines.retain(|l| l.who != PERSON_T);
    with_lines_for_ashes(&mut h, Role::Use);
    let (b, me) = player(&mut h, "one", "Elara");
    let junk = hold(&mut h, me, JUNK, 1);
    let (lines, _) = give_ashes(&mut h, b, me, junk, 1);
    assert_eq!(lines, Vec::<u32>::new());
    assert!(holds(&h, me, junk.0), "handed back");
}

const OFFERING: u32 = 0x4700_1775;
const GEM: u32 = 0x4700_1758;
const TOKEN_FX: u32 = 0x4100_05EB;

/// A give that moves no quest, for the person: the ashes, wanting nothing further.
fn taking(item: Option<u32>) -> Give {
    Give {
        to: PERSON_T,
        item,
        wants: None,
        gives: Vec::new(),
        counts: Vec::new(),
        effects: Vec::new(),
        text: None,
        fx: None,
        requires: Vec::new(),
        placed: "authored for a test".into(),
    }
}

fn taking_line(item: Option<u32>, string: u32, order: u32) -> Line {
    Line {
        item,
        order,
        ..line(PERSON_T, string, Some(Role::Accept), None)
    }
}

/// A give that moves no quest keeps the item, says the person's lines for taking it in their order,
/// and gives what it gives from the person, so many of each; no quest moves and nothing is handed
/// back, whatever is given being taken whole.
///
/// Behaviour: server.gives.row-keeps-and-gives
#[test]
fn a_give_moving_no_quest_keeps_the_item_says_its_lines_and_gives_its_items() {
    let mut h = world();
    h.server.quests.gives.push(Give {
        gives: vec![GEM],
        counts: vec![8],
        fx: Some(TOKEN_FX),
        ..taking(Some(OFFERING))
    });
    h.server
        .catalog
        .insert(yalain_testkit::test_template(GEM, 0x1067, Props::default()));
    h.server
        .talk
        .lines
        .push(taking_line(Some(OFFERING), TAKING_LINE, 1));
    h.server
        .talk
        .lines
        .push(taking_line(Some(OFFERING), NAMED_LINE, 0));
    let (b, me) = player(&mut h, "one", "Elara");
    let offering = hold(&mut h, me, OFFERING, 3);
    let from = h.bots[b].inbox.len();
    let (lines, moved) = give_ashes(&mut h, b, me, offering, 3);
    assert_eq!(lines, vec![NAMED_LINE, TAKING_LINE], "in their order");
    assert!(moved.is_empty(), "no quest moves");
    assert!(!holds(&h, me, offering.0), "kept whole");
    let gem = h.server.players[&world_of(&h, me)]
        .play
        .items
        .iter()
        .find(|x| h.server.things.get(x.id).is_some_and(|t| t.template == GEM))
        .map(|x| x.quantity);
    assert_eq!(gem, Some(8), "the gems from the person");
    let played = h.world_messages(b, from).iter().any(
        |m| matches!(m, ServerMessage::DoFx(e) if e.sender.id == PERSON && e.action.fx == TOKEN_FX),
    );
    assert!(played, "the person plays what the row says");
}

/// A give whose gift this world can't make takes nothing: the item goes back to its giver, whole.
///
/// Behaviour: server.makes.nothing-used-up-for-what-cannot-be-made
#[test]
fn a_give_whose_gift_cannot_be_made_hands_the_item_back() {
    let mut h = world();
    h.server.quests.gives.push(Give {
        gives: vec![GEM],
        ..taking(Some(OFFERING))
    });
    let (b, me) = player(&mut h, "one", "Elara");
    let offering = hold(&mut h, me, OFFERING, 3);
    let _ = give_ashes(&mut h, b, me, offering, 3);
    assert!(holds(&h, me, offering.0), "handed back");
    assert_eq!(quantity_of(&h, me, offering.0), Some(3), "whole");
}

/// A quest's step whose gift this world can't make doesn't run: the quest stays where it is and the
/// item goes back to its giver.
///
/// Behaviour: server.makes.nothing-used-up-for-what-cannot-be-made
#[test]
fn a_step_whose_gift_cannot_be_made_does_not_run_and_hands_the_item_back() {
    const QUEST: u32 = 0x4000_032F;
    const SODDEN: u32 = 0x4700_14FF;
    const CLAIM: u32 = 0x4700_14F7;
    let mut h = world();
    h.server.quests = QuestTable {
        quests: vec![quest(QUEST, 8)],
        steps: vec![Step {
            takes: vec![SODDEN],
            gives: vec![CLAIM],
            ..step(
                QUEST,
                5,
                Trigger::Give(SODDEN, PERSON_T),
                Update::Advance(6),
            )
        }],
        gives: Vec::new(),
        rooms: Vec::new(),
    };
    let (b, me) = player(&mut h, "one", "Elara");
    at_phase(&mut h, QUEST, 5);
    let sodden = hold(&mut h, me, SODDEN, 1);
    let (_, moved) = give_ashes(&mut h, b, me, sodden, 1);
    assert!(moved.is_empty(), "the quest stays");
    assert!(holds(&h, me, sodden.0), "handed back");
}

/// A give that moves no quest and wants a count refuses any other with the person's line for the
/// item, handing the stack back; it takes that count, the rest staying.
///
/// Behaviour: server.gives.row-wants-a-count
#[test]
fn a_give_moving_no_quest_may_want_a_count_and_refuses_any_other() {
    let mut h = world();
    h.server.quests.gives.push(Give {
        wants: Some(50),
        ..taking(Some(OFFERING))
    });
    h.server.talk.lines.push(Line {
        item: Some(OFFERING),
        ..line(PERSON_T, WRONG_NUMBER, Some(Role::RefuseGive), None)
    });
    h.server
        .talk
        .lines
        .push(taking_line(Some(OFFERING), TAKING_LINE, 0));
    let (b, me) = player(&mut h, "one", "Elara");
    let coins = hold(&mut h, me, OFFERING, 60);
    let (lines, _) = give_ashes(&mut h, b, me, coins, 60);
    assert_eq!(lines, vec![WRONG_NUMBER], "a whole stack of more");
    assert_eq!(quantity_of(&h, me, coins.0), Some(60));
    let (lines, _) = give_ashes(&mut h, b, me, coins, 40);
    assert_eq!(lines, vec![WRONG_NUMBER], "too few");
    assert_eq!(quantity_of(&h, me, coins.0), Some(60));
    let (lines, _) = give_ashes(&mut h, b, me, coins, 50);
    assert_eq!(lines, vec![TAKING_LINE], "the count wanted");
    assert_eq!(quantity_of(&h, me, coins.0), Some(10), "the rest stays");
}

/// A give that names no item takes anything, with one of the person's lines for taking any item;
/// a row for the item given comes first.
///
/// Behaviour: server.gives.row-any-item
#[test]
fn a_give_naming_no_item_takes_anything_and_a_row_for_the_item_comes_first() {
    let mut h = world();
    h.server.quests.gives.push(taking(None));
    h.server.quests.gives.push(Give {
        gives: vec![GEM],
        ..taking(Some(OFFERING))
    });
    h.server
        .catalog
        .insert(yalain_testkit::test_template(GEM, 0x1067, Props::default()));
    h.server.talk.lines.push(taking_line(None, TAKING_LINE, 0));
    h.server
        .talk
        .lines
        .push(taking_line(Some(OFFERING), NAMED_LINE, 0));
    let (b, me) = player(&mut h, "one", "Elara");
    let junk = hold(&mut h, me, JUNK, 1);
    let (lines, _) = give_ashes(&mut h, b, me, junk, 1);
    assert_eq!(lines, vec![TAKING_LINE], "any item");
    assert!(!holds(&h, me, junk.0), "kept");
    let offering = hold(&mut h, me, OFFERING, 1);
    let (lines, _) = give_ashes(&mut h, b, me, offering, 1);
    assert_eq!(lines, vec![NAMED_LINE], "its own row and line");
    let gems = h.server.players[&world_of(&h, me)]
        .play
        .items
        .iter()
        .filter(|x| h.server.things.get(x.id).is_some_and(|t| t.template == GEM))
        .count();
    assert_eq!(gems, 1);
}

/// A give that requires a quest at a phase runs at that phase alone; at another the item is refused
/// and handed back. A quest's step for the same give comes first.
///
/// Behaviour: server.gives.row-requires
#[test]
fn a_give_requiring_a_phase_runs_at_it_and_a_step_comes_first() {
    let mut h = world();
    h.server.quests.gives.push(Give {
        requires: vec![(QUEST, Need::Phase(3))],
        ..taking(Some(ASHES))
    });
    h.server
        .talk
        .lines
        .push(taking_line(Some(ASHES), TAKING_LINE, 0));
    let (b, me) = player(&mut h, "one", "Elara");
    let ashes = hold(&mut h, me, ASHES, 1);
    at_phase(&mut h, QUEST, 1);
    let (lines, moved) = give_ashes(&mut h, b, me, ashes, 1);
    assert_eq!(lines, vec![PLAIN_REFUSAL], "the wrong phase");
    assert!(moved.is_empty());
    assert!(holds(&h, me, ashes.0));

    at_phase(&mut h, QUEST, 2);
    let (_, moved) = give_ashes(&mut h, b, me, ashes, 1);
    assert_eq!(moved, vec![(0x2000_0003, QUEST, 3)], "the quest's step");
    assert!(!holds(&h, me, ashes.0));

    let ashes = hold(&mut h, me, ASHES, 1);
    let (lines, moved) = give_ashes(&mut h, b, me, ashes, 1);
    assert_eq!(lines, vec![TAKING_LINE], "the phase it requires");
    assert!(moved.is_empty());
    assert!(!holds(&h, me, ashes.0));
}

/// A give's effects go on the player, the person the caster, then its text.
///
/// Behaviour: server.gives.row-keeps-and-gives
#[test]
fn a_give_moving_no_quest_puts_its_text_on_the_player_after_keeping_the_item() {
    let mut h = world();
    h.server.quests.gives.push(Give {
        text: Some((NAMED_LINE, 0x2500_2958)),
        ..taking(Some(OFFERING))
    });
    let (b, me) = player(&mut h, "one", "Elara");
    let offering = hold(&mut h, me, OFFERING, 1);
    let from = h.bots[b].inbox.len();
    give_ashes(&mut h, b, me, offering, 1);
    let shown = h.calls(b, from).into_iter().any(|(_, v)| {
        v.iter().any(|x| match x.root() {
            Some(Node::TextInfo(t)) => t.string_id == NAMED_LINE,
            _ => false,
        })
    });
    assert!(shown, "its text");
    assert!(!holds(&h, me, offering.0));
}

/// A quest's step for a give comes before a give that moves no quest, both able to run; with the
/// step out of reach the row runs.
///
/// Behaviour: server.gives.row-requires
#[test]
fn a_quests_step_comes_before_a_give_moving_no_quest() {
    let mut h = world();
    h.server.quests.gives.push(taking(Some(ASHES)));
    h.server
        .talk
        .lines
        .push(taking_line(Some(ASHES), TAKING_LINE, 0));
    let (b, me) = player(&mut h, "one", "Elara");
    at_phase(&mut h, QUEST, 2);
    let ashes = hold(&mut h, me, ASHES, 1);
    let (lines, moved) = give_ashes(&mut h, b, me, ashes, 1);
    assert_eq!(moved, vec![(0x2000_0003, QUEST, 3)], "the quest's step");
    assert!(!lines.contains(&TAKING_LINE), "{lines:?}");
    let ashes = hold(&mut h, me, ASHES, 1);
    let (lines, moved) = give_ashes(&mut h, b, me, ashes, 1);
    assert!(moved.is_empty());
    assert_eq!(lines, vec![TAKING_LINE], "the row");
}
