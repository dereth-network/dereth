//! Gives: an item of the pack dragged onto a person or an object. The walk and the handing over,
//! the answer; an item a quest step takes kept, one no step wants handed back with the person's
//! refusal; a person busy with another player's give, a walk that cannot get there; a part of a
//! stack; a quest moved on by an item a give brought; a vitae tap; a hero made; a pet refusing what
//! it is given; a step that pauses, or a line said with it that pauses the rest of it.

use auberean_protocol::types::BehaviorParams;
use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use auberean_script::native_object::QuestInfo;
use auberean_script::wire::In;
use yalain_content::quests::{
    Give, Need, QuestDef, QuestTable, Step, Subquests, Transformation, Trigger, Update,
};
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
        duration_by_calendar: false,
        id,
        name: Words::Ref(0xA860_7CEC, 0x2500_2958),
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

fn step(quest: u32, needs: u32, trigger: Trigger, update: Update) -> Step {
    Step {
        out_of_turn: false,
        skills: Vec::new(),
        key: String::new(),
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
        recipes: Vec::new(),
        placed: "authored for a test".into(),
        requires: Vec::new(),
        takes: Vec::new(),
        races: Vec::new(),
        effects_last: false,
        told_refused: false,
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
        pause_ms: 0,
        after_update_ms: 0,
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
    look: u32,
    appearance: u32,
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
                look: s[38].1,
                appearance: s[6].1,
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

/// What a give is told by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sent {
    Stopped,
    Container(u64),
    Answered,
    Handed,
}

/// What a client was sent of its player's give of an item since `from`, in the order the server
/// sent it.
fn give_sent(h: &Harness, b: usize, me: u64, item: u64, from: usize) -> Vec<Sent> {
    h.sent_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::MoveTo(e) if e.sender.id == me && e.action.params.flags == 0x40 => {
                Some(Sent::Stopped)
            }
            ServerMessage::UpdateInstanceIdVisual(q)
                if q.object.id == item && q.quality == CONTAINER =>
            {
                Some(Sent::Container(q.value))
            }
            ServerMessage::PlayerCall(c) if c.function.0 == MOVE_DONE => Some(Sent::Answered),
            ServerMessage::PlayBehavior(e) if e.sender.id == me && e.action.behavior == HANDING => {
                Some(Sent::Handed)
            }
            _ => None,
        })
        .collect()
}

/// A give is walked to 2 m of the person (the order sent even when the player is there); there,
/// sent in this order, the walk is stopped, the item's container is cleared and the player plays
/// its handing-over with the item's look in its hand in the walk's context, seen by the player
/// beside it; the move is answered as done, the animation played, 1.31 s later and not before.
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
    assert_eq!(
        give_sent(&h, b, me, item, from),
        [Sent::Stopped, Sent::Container(0), Sent::Handed],
        "in the order sent"
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

/// What the hand shows of an item given: a copy of it as its template draws it, the entity it is
/// drawn as and its appearance as it lies, in the giver's handing-over and in the person's handing
/// it back (2017: every give named both from the item's template); the give and the item back are
/// answered naming the item so.
///
/// Behaviour: server.hands.a-give-shows-the-item-in-the-hand
#[test]
fn an_item_given_and_handed_back_is_in_the_hand_as_its_template_draws_it() {
    use yalain_content::props::{LOOK_ENTITY, PILE_APPEARANCE};
    // A health potion, as the install draws it: an appearance other than the common one.
    const ITEM: u32 = 0x4700_11B1;
    const ITEM_ENTITY: u32 = 0x4700_11B0;
    const ITEM_APPEARANCE: u32 = 0x2000_011F;
    let in_hand = BehaviorParams::BEHAVIOR
        | BehaviorParams::TARGET
        | 0x100
        | BehaviorParams::CLONE
        | BehaviorParams::CONTEXT;
    let mut h = world();
    let mut props = Props::default();
    props
        .set(LOOK_ENTITY, yalain_content::Prop::Word(ITEM_ENTITY))
        .set(PILE_APPEARANCE, yalain_content::Prop::Word(ITEM_APPEARANCE));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(ITEM, 0x1067, props));
    let (b, me) = player(&mut h, "one", "Elara");
    let (item, slot) = hold(&mut h, me, ITEM, 1);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &give(me, item, slot, PERSON, 1));
    h.run(steps(HANDED_AFTER) + steps(gives::HANDED_BACK_AFTER) + 8);
    let handings: Vec<(u64, u32, u64, u32, u32)> = h
        .world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayBehavior(e) if e.action.behavior == HANDING => Some((
                e.sender.id,
                e.action.bits,
                e.action.target,
                e.action.clone_visual,
                e.action.clone_appearance,
            )),
            _ => None,
        })
        .collect();
    assert_eq!(
        handings,
        vec![
            (me, in_hand, item, ITEM_ENTITY, ITEM_APPEARANCE),
            (PERSON, in_hand, item, ITEM_ENTITY, ITEM_APPEARANCE)
        ],
        "the giver's handing-over, then the person's handing it back"
    );
    let a = answers(&h, b, from);
    assert_eq!(a.len(), 2, "answered and back: {a:?}");
    assert!(
        a.iter()
            .all(|x| (x.look, x.appearance) == (ITEM_ENTITY, ITEM_APPEARANCE)),
        "{a:?}"
    );
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

/// A walk to give that ends at rest 6.4 m from the far person's center hands the item over when the
/// person's body is 4.45 m wide (as wide as one 2017's players walked to and used from 9.4 m off its
/// center): the walk ends at its range from the person's side, here 1.95 m off. With the body not
/// known the walk measures to the center, and the same place is refused as one that could not get
/// there.
///
/// Behaviour: server.gives.walk-ends-at-the-persons-side
#[test]
fn a_give_whose_walk_ends_by_a_wide_persons_side_is_handed_over() {
    for wide in [true, false] {
        let mut h = world();
        if wide {
            h.server.catalog.bodies.insert(FAR_T, [4.45, 6.29]);
        }
        let (b, me) = player(&mut h, "one", "Elara");
        let (item, slot) = hold(&mut h, me, JUNK, 1);
        let from = h.bots[b].inbox.len();
        h.bots[b].send(true, queue::WEENIE, &give(me, item, slot, FAR, 1));
        h.run(4);
        assert!(answers(&h, b, from).is_empty(), "walking");
        let at = h.server.players[&world_of(&h, me)].position;
        let here = auberean_primitives::Vec3::new(59.0, 260.0 - 6.4, at.origin[2]);
        let stamp = h.server.players[&world_of(&h, me)].teleport_stamp;
        for _ in 0..2 {
            let idle = yalain_world::world::IDLE_MOTION;
            h.bots[b].send(true, queue::WEENIE, &report(at.cell, here, 0, idle, stamp));
            h.run(3);
        }
        h.run(steps(HANDED_AFTER) + 8);
        let a = answers(&h, b, from);
        assert_eq!(a.len(), 1, "{a:?}");
        let handing = h.world_messages(b, from).iter().any(|m| {
            matches!(m,
            ServerMessage::PlayBehavior(e) if e.action.behavior == HANDING)
        });
        if wide {
            assert_eq!(a[0].status, 0);
            assert!(handing, "handed over");
        } else {
            assert_eq!(a[0].status, status::CANT_GET_THERE);
            assert!(!handing, "not handed over");
        }
    }
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

/// A give's step tells its update, then keeps the item given and hands over the item it gives
/// (2017, in send order: the item given kept after the update 59 of 86 times, before it 27; a give's
/// new items after 37 of 57, before 20).
/// Behaviour: server.quests.a-steps-items-against-its-update
#[test]
fn a_give_step_tells_its_update_before_it_keeps_the_item_and_hands_over_its_own() {
    use yalain_world::quests::UPDATE_QUEST;
    const CLAIM_QUEST: u32 = 0x4000_032F;
    const SODDEN: u32 = 0x4700_14FF;
    const CLAIM: u32 = 0x4700_14F7;
    let mut h = world();
    h.server.quests = QuestTable {
        quests: vec![quest(CLAIM_QUEST, 8)],
        steps: vec![Step {
            takes: vec![SODDEN],
            gives: vec![CLAIM],
            ..step(
                CLAIM_QUEST,
                5,
                Trigger::Give(SODDEN, PERSON_T),
                Update::Advance(6),
            )
        }],
        gives: Vec::new(),
        rooms: Vec::new(),
    };
    let (b, me) = player(&mut h, "one", "Elara");
    at_phase(&mut h, CLAIM_QUEST, 5);
    let (item, slot) = hold(&mut h, me, SODDEN, 1);
    hold(&mut h, me, CLAIM, 1);
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
    let order = sent(&h, b, from);
    let up = order.iter().position(|x| x.0 == UPDATE_QUEST);
    let kept = order.iter().position(|x| x.0 == gives::DESTROY_CONTAINED);
    let handed = order
        .iter()
        .rposition(|x| x.0 == yalain_world::inventory::function::MOVE_DONE);
    assert!(up.is_some() && up < kept && up < handed, "{order:X?}");
}

/// A use's step hands over the item it gives, tells its update, then takes what it takes from the
/// pack (2017, in send order: a use's new items before the update 63 of 69 times; an item taken
/// from the pack after it 19 of 20).
/// Behaviour: server.quests.a-steps-items-against-its-update
#[test]
fn a_use_step_hands_over_its_item_before_its_update_and_takes_after_it() {
    use yalain_world::quests::UPDATE_QUEST;
    const CLAIM_QUEST: u32 = 0x4000_032F;
    const SODDEN: u32 = 0x4700_14FF;
    const CLAIM: u32 = 0x4700_14F7;
    let mut h = world();
    let used = Step {
        takes: vec![SODDEN],
        gives: vec![CLAIM],
        ..step(CLAIM_QUEST, 5, Trigger::Use(PERSON_T), Update::Advance(6))
    };
    h.server.quests = QuestTable {
        quests: vec![quest(CLAIM_QUEST, 8)],
        steps: vec![used.clone()],
        gives: Vec::new(),
        rooms: Vec::new(),
    };
    let (b, me) = player(&mut h, "one", "Elara");
    at_phase(&mut h, CLAIM_QUEST, 5);
    hold(&mut h, me, SODDEN, 1);
    hold(&mut h, me, CLAIM, 1);
    let world = world_of(&h, me);
    h.server
        .players
        .get_mut(&world)
        .expect("in")
        .play
        .items
        .retain(|x| x.template != CLAIM);
    let from = h.bots[b].inbox.len();
    let clock = h.clock();
    yalain_world::quests::run(&mut h.server, &mut h.net, world, &used, None, clock);
    h.run(2);
    let order = sent(&h, b, from);
    let up = order.iter().position(|x| x.0 == UPDATE_QUEST);
    let handed = order
        .iter()
        .position(|x| x.0 == yalain_world::inventory::function::MOVE_DONE);
    let taken = order
        .iter()
        .position(|x| x.0 == yalain_world::inventory::function::DISCRETE_DESTROY);
    assert!(
        handed.is_some() && handed < up && up.is_some() && up < taken,
        "{order:X?}"
    );
}

/// A step's tap of vitae leaves the player what its share leaves of its vitae (told, with what is
/// still to earn), the effect's own appearance played on the player.
///
/// Behaviour: server.gives.vitae-tap
#[test]
fn a_steps_vitae_tap_takes_a_tenth_of_the_vitae() {
    use yalain_testkit::{VITAE_TAP, VITAE_TAP_FX};
    let mut h = world();
    h.server.quests.steps[0].effects = vec![VITAE_TAP];
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
            ServerMessage::DoFx(e) if e.sender.id == me && e.action.fx == VITAE_TAP_FX)),
        "its appearance"
    );
}

/// A world's tap of vitae takes the share its rule names: a half, put on by a step.
///
/// Behaviour: server.gives.vitae-tap
#[test]
fn a_vitae_tap_takes_the_share_its_rule_names() {
    use yalain_world::effect_defs::Kind;
    let mut h = world();
    let tap = h
        .server
        .effect_table
        .effects
        .get(&yalain_testkit::VITAE_TAP)
        .cloned()
        .expect("the test world's tap");
    h.server.effect_table.effects.insert(
        0x6F00_7777,
        yalain_world::effect_defs::EffectDef {
            state: 0x6F00_7777,
            kind: Kind::VitaeTap(0.5),
            ..tap
        },
    );
    h.server.quests.steps[0].effects = vec![0x6F00_7777];
    let (b, me) = player(&mut h, "one", "Elara");
    at_phase(&mut h, QUEST, 2);
    let world = world_of(&h, me);
    yalain_world::quals::set_float(
        &mut h.server.players.get_mut(&world).expect("in").qualities,
        yalain_world::death::vitae_q::VITAE,
        80.0,
    );
    let (ashes, slot) = hold(&mut h, me, ASHES, 1);
    h.bots[b].send(true, queue::WEENIE, &give(me, ashes, slot, PERSON, 1));
    h.run(steps(HANDED_AFTER) + 8);
    let vitae = yalain_world::quals::float(
        &h.server.players[&world].qualities,
        yalain_world::death::vitae_q::VITAE,
    );
    assert!((vitae.expect("vitae") - 40.0).abs() < 1e-3, "{vitae:?}");
}

/// An effect a world's rule makes give vitae back raises its player's vitae by its points, never
/// above full, the experience still to earn worked out again; with the fellows, each of the
/// player's fellows in the world gains the same, one at full nothing, and each who gains plays the
/// effect's appearance.
///
/// Behaviour: server.effects.vitae-restore
#[test]
fn a_vitae_restore_gives_its_points_back_to_its_player_and_fellows_never_above_full() {
    use yalain_world::death::vitae_q::{VITAE, XP_TO_RAISE};
    use yalain_world::effect_defs::{Kind, VitaeRestore};
    use yalain_world::fellowship::{Fellow, Fellowship, Vitals};
    const FX: u32 = 0x4100_00C0;
    const ALONE: u32 = 0x6F00_7777;
    const SHARED: u32 = 0x6F00_7778;
    let mut h = world();
    let tap = h
        .server
        .effect_table
        .effects
        .get(&yalain_testkit::VITAE_TAP)
        .cloned()
        .expect("the test world's tap");
    for (state, fellows) in [(ALONE, false), (SHARED, true)] {
        h.server.effect_table.effects.insert(
            state,
            yalain_world::effect_defs::EffectDef {
                state,
                kind: Kind::VitaeRestore(VitaeRestore {
                    points: 2.0,
                    most: None,
                    fellows,
                }),
                fx: FX,
                ..tap.clone()
            },
        );
    }
    let (a, me) = player(&mut h, "one", "Elara");
    let (b, you) = player(&mut h, "two", "Bran");
    let (c, them) = player(&mut h, "three", "Cade");
    let (_, other) = player(&mut h, "four", "Dorn");
    h.server.fellowships.list.push(Fellowship {
        name: "Seekers".into(),
        leader: me,
        flags: 0,
        room: 1,
        fellows: [me, you, them]
            .iter()
            .map(|&id| Fellow {
                id,
                joined: 0.0,
                sent: Vitals::default(),
                next_check: f64::INFINITY,
            })
            .collect(),
        last_claimant: 0,
    });
    let set = |h: &mut Harness, who: u64, v: f32| {
        let w = world_of(h, who);
        let p = h.server.players.get_mut(&w).expect("in");
        yalain_world::quals::set_float(&mut p.qualities, VITAE, v);
    };
    let vitae = |h: &Harness, who: u64| {
        yalain_world::quals::float(&h.server.players[&world_of(h, who)].qualities, VITAE)
            .expect("vitae")
    };
    for (who, v) in [(me, 80.0), (you, 99.0), (them, 100.0), (other, 90.0)] {
        set(&mut h, who, v);
    }
    let shown = |h: &Harness, bot: usize, from: usize, id: u64| {
        h.world_messages(bot, from)
            .iter()
            .any(|m| matches!(m, ServerMessage::DoFx(e) if e.sender.id == id && e.action.fx == FX))
    };
    let from = [a, b, c].map(|bot| h.bots[bot].inbox.len());
    let world = world_of(&h, me);
    let cast = yalain_world::effects::Cast {
        caster: me,
        spellcraft: 50.0,
        ..yalain_world::effects::Cast::default()
    };
    let clock = h.clock();
    // Its player alone.
    assert!(yalain_world::effects::apply(
        &mut h.server,
        &mut h.net,
        world,
        ALONE,
        cast,
        clock
    ));
    assert!((vitae(&h, me) - 82.0).abs() < 1e-3);
    assert!((vitae(&h, you) - 99.0).abs() < 1e-3, "no fellow");
    // With the fellows: never above full, one at full nothing, one outside the fellowship nothing.
    assert!(yalain_world::effects::apply(
        &mut h.server,
        &mut h.net,
        world,
        SHARED,
        cast,
        clock
    ));
    h.run(3);
    for (who, want) in [(me, 84.0), (you, 100.0), (them, 100.0), (other, 90.0)] {
        assert!(
            (vitae(&h, who) - want).abs() < 1e-3,
            "{who:X}: {}",
            vitae(&h, who)
        );
    }
    let to_earn = |h: &Harness, who: u64| {
        yalain_world::quals::long(&h.server.players[&world_of(h, who)].qualities, XP_TO_RAISE)
    };
    assert!(to_earn(&h, me).is_some_and(|x| x > 0), "worked out again");
    assert_eq!(to_earn(&h, you), Some(0), "none to earn at full");
    assert!(shown(&h, a, from[0], me));
    assert!(shown(&h, b, from[1], you), "the fellow who gained");
    assert!(!shown(&h, c, from[2], them), "the one at full: nothing");
}

const ELYSA: u32 = 0x4000_024A;
const BOJU_GEM: u32 = 0x4700_1505;

/// A transformation of the test's own: its time, its appearances and behavior, its text naming the
/// player.
fn transformation() -> Transformation {
    Transformation {
        seconds: 20.0,
        fx: vec![0x4100_04A5, 0x4100_04B5],
        behavior: Some(0x4100_0120),
        text: Some((0x2EE2_0FE4, 0x2500_2BE3)),
        name: 0x9D06_6C50,
    }
}

/// Elysa's last step, giving the gem to the shrine, with these effects and transformation.
fn hero_world(effects: Vec<u32>, transforms: Option<Transformation>) -> Harness {
    let mut h = world();
    h.server.quests = QuestTable {
        quests: vec![quest(ELYSA, 4)],
        steps: vec![Step {
            takes: vec![BOJU_GEM],
            effects,
            transforms,
            chance: None,
            to_fellows: false,
            ..step(
                ELYSA,
                4,
                Trigger::Give(BOJU_GEM, OBJECT_T),
                Update::Complete,
            )
        }],
        gives: Vec::new(),
        rooms: Vec::new(),
    };
    h
}

/// A step that transforms its player runs when the transformation is through, its time after the
/// give's answer: the player a hero, the quest completed, the hero's text naming it; before, only
/// the transformation's appearances and behavior, the gift kept.
///
/// Behaviour: server.gives.hero-transformation
#[test]
fn a_give_that_makes_a_hero_does_so_when_the_transformation_is_through() {
    let t = transformation();
    let mut h = hero_world(Vec::new(), Some(t.clone()));
    let (b, me) = player(&mut h, "one", "Elara");
    at_phase(&mut h, ELYSA, 4);
    let (gem, slot) = hold(&mut h, me, BOJU_GEM, 1);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &give(me, gem, slot, OBJECT, 1));
    h.run(steps(HANDED_AFTER) + 8);
    let world = world_of(&h, me);
    assert!(!holds(&h, me, gem), "kept");
    let shown = h.world_messages(b, from);
    for fx in &t.fx {
        assert!(
            shown.iter().any(|m| matches!(m,
                ServerMessage::DoFx(e) if e.sender.id == me && e.action.fx == *fx)),
            "transforming: {fx:08X}"
        );
    }
    assert!(
        shown
            .iter()
            .any(|m| matches!(m, ServerMessage::PlayBehavior(e) if e.sender.id == me)),
        "its behavior"
    );
    h.run(steps(t.seconds) - 10);
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
            .any(|(_, id)| Some(*id) == t.text.map(|(s, _)| s)),
        "the hero's text"
    );
}

/// A give's step whose row names no transformation runs at once and makes no hero, even one that
/// puts on the hero's transformation effect.
///
/// Behaviour: server.gives.hero-transformation
#[test]
fn a_step_whose_row_names_no_transformation_runs_at_once_and_makes_no_hero() {
    const HERO_TRANSFORMATION: u32 = 0x6F00_1C6F;
    let mut h = hero_world(vec![HERO_TRANSFORMATION], None);
    let (b, me) = player(&mut h, "one", "Elara");
    at_phase(&mut h, ELYSA, 4);
    let (gem, slot) = hold(&mut h, me, BOJU_GEM, 1);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &give(me, gem, slot, OBJECT, 1));
    h.run(steps(HANDED_AFTER) + 8);
    assert_eq!(updates(&h, b, from), vec![(0x3000_0000, ELYSA, 0)]);
    let world = world_of(&h, me);
    assert!(!yalain_world::focus::is_hero(
        &h.server.players[&world].qualities
    ));
}

/// One who logs off with its gift given and the transformation under way (the linger is shorter)
/// has what the gift was for: it is a hero when it comes back, not out of its gift for nothing.
/// Behaviour: server.gives.a-step-under-way-runs-as-its-player-leaves
#[test]
fn a_hero_step_under_way_runs_as_its_player_leaves() {
    let mut h = hero_world(Vec::new(), Some(transformation()));
    let (b, me) = player(&mut h, "one", "Elara");
    at_phase(&mut h, ELYSA, 4);
    let (gem, slot) = hold(&mut h, me, BOJU_GEM, 1);
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
        effects_quiet: false,
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

/// A give that moves no quest and adds to a tally adds as it takes each item, never past the tally's
/// full; one that waits on the tally not yet full refuses the item once it is, handing it back.
///
/// Behaviour: server.tallies.steps-and-gives-fill-a-shared-count
#[test]
fn a_give_moving_no_quest_adds_to_a_tally_until_it_is_full() {
    use yalain_content::tallies::{Adds, Gate, Holds, Reset, Tally, TallyTable};
    let mut h = world();
    let row = taking(Some(OFFERING));
    h.server.quests.gives.push(row.clone());
    h.server.content.tallies = TallyTable {
        tallies: vec![Tally {
            key: "supplies".into(),
            full: 3,
            starts: None,
            resets: Reset::Never,
            clears: Vec::new(),
        }],
        gives: vec![(
            row.clone(),
            Gate {
                tally: "supplies".into(),
                holds: Holds::NotFull,
            },
        )],
        give_adds: vec![(
            row,
            Adds {
                tally: "supplies".into(),
                by: 2,
            },
        )],
        ..TallyTable::default()
    };
    let (b, me) = player(&mut h, "one", "Elara");
    let count = |h: &Harness| yalain_world::tallies::count(&h.server, "supplies");
    let offering = hold(&mut h, me, OFFERING, 1);
    give_ashes(&mut h, b, me, offering, 1);
    assert!(!holds(&h, me, offering.0), "taken");
    assert_eq!(count(&h), 2);
    let offering = hold(&mut h, me, OFFERING, 1);
    give_ashes(&mut h, b, me, offering, 1);
    assert!(!holds(&h, me, offering.0), "taken");
    assert_eq!(count(&h), 3, "never past full");
    let offering = hold(&mut h, me, OFFERING, 1);
    let (lines, _) = give_ashes(&mut h, b, me, offering, 1);
    assert!(holds(&h, me, offering.0), "refused once it is full");
    assert_eq!(lines, vec![PLAIN_REFUSAL]);
    assert_eq!(count(&h), 3);
}

/// A give that moves no quest and waits on a craft skill refuses the item to a player under its
/// level, handing it back; at the level in one of the skills it names it takes it.
///
/// Behaviour: server.quests.craft-skill-gate
#[test]
fn a_give_waiting_on_a_craft_skill_takes_the_item_only_at_its_level() {
    use yalain_content::crafts::{CraftGate, CraftGates};
    use yalain_world::state::CraftSkillState;
    const TOOLS: u32 = 0x6F00_06A7;
    let mut h = world();
    let row = taking(Some(OFFERING));
    h.server.quests.gives.push(row.clone());
    h.server.content.craft_gates = CraftGates {
        steps: Vec::new(),
        gives: vec![(
            row,
            CraftGate {
                skills: vec![TOOLS],
                at_least: 100,
            },
        )],
    };
    let (b, me) = player(&mut h, "one", "Elara");
    let at = |h: &mut Harness, level: u32| {
        let world = world_of(h, me);
        h.server
            .players
            .get_mut(&world)
            .expect("in the world")
            .play
            .craft_skills = vec![CraftSkillState {
            skill: TOOLS,
            level,
            today: 0,
            available: 0,
        }];
    };
    at(&mut h, 99);
    let offering = hold(&mut h, me, OFFERING, 1);
    let (lines, _) = give_ashes(&mut h, b, me, offering, 1);
    assert!(holds(&h, me, offering.0), "refused under the level");
    assert_eq!(lines, vec![PLAIN_REFUSAL]);
    at(&mut h, 100);
    give_ashes(&mut h, b, me, offering, 1);
    assert!(!holds(&h, me, offering.0), "taken at the level");
}

/// A give that moves no quest and is for heroes alone refuses the item to a player who is none,
/// handing it back; from a hero it takes it.
///
/// Behaviour: server.quests.a-step-for-heroes-alone
#[test]
fn a_give_for_heroes_alone_takes_the_item_only_from_a_hero() {
    use yalain_content::heroes::HeroGates;
    let mut h = world();
    let row = taking(Some(OFFERING));
    h.server.quests.gives.push(row.clone());
    h.server.content.hero_gates = HeroGates {
        steps: Vec::new(),
        gives: vec![row],
    };
    let (b, me) = player(&mut h, "one", "Elara");
    let offering = hold(&mut h, me, OFFERING, 1);
    let (lines, _) = give_ashes(&mut h, b, me, offering, 1);
    assert!(holds(&h, me, offering.0), "refused to one who is no hero");
    assert_eq!(lines, vec![PLAIN_REFUSAL]);
    let world = world_of(&h, me);
    yalain_world::focus::set_hero_kept(
        &mut h
            .server
            .players
            .get_mut(&world)
            .expect("in the world")
            .qualities,
        true,
    );
    give_ashes(&mut h, b, me, offering, 1);
    assert!(!holds(&h, me, offering.0), "taken from a hero");
}

/// A quest step or a give naming several items to pick from gives one of them each time it runs,
/// by their weights, beside what it always gives: an offering handed in again and again brings
/// one gem or the other, never both and never none.
///
/// Behaviour: server.gives.one-of-several
#[test]
fn a_step_or_a_give_naming_several_items_gives_one_of_them_each_time() {
    const TIMES: u32 = 24;
    let held_of = |h: &Harness, me: u64, template: u32| -> u32 {
        h.server.players[&world_of(h, me)]
            .play
            .items
            .iter()
            .filter(|x| {
                h.server
                    .things
                    .get(x.id)
                    .is_some_and(|t| t.template == template)
            })
            .map(|x| x.quantity.max(1))
            .sum()
    };
    for by_step in [true, false] {
        let mut h = world();
        h.server
            .catalog
            .insert(yalain_testkit::test_template(GEM, 0x1067, Props::default()));
        let pick = vec![(GEM, 1), (IMBUED, 1)];
        if by_step {
            let st = Step {
                key: "pick".into(),
                takes: vec![OFFERING],
                ..step(QUEST, 0, Trigger::Give(OFFERING, PERSON_T), Update::Nothing)
            };
            h.server.quests.steps.push(st.clone());
            h.server.item_rows.step_picks = vec![(st, pick)];
        } else {
            let row = taking(Some(OFFERING));
            h.server.quests.gives.push(row.clone());
            h.server.item_rows.give_picks = vec![(row, pick)];
        }
        let (b, me) = player(&mut h, "one", "Elara");
        for _ in 0..TIMES {
            let offering = hold(&mut h, me, OFFERING, 1);
            give_ashes(&mut h, b, me, offering, 1);
            assert!(!holds(&h, me, offering.0), "step {by_step}: taken");
        }
        let (gems, imbued) = (held_of(&h, me, GEM), held_of(&h, me, IMBUED));
        assert_eq!(gems + imbued, TIMES, "step {by_step}: one a time");
        assert!(
            gems > 0 && imbued > 0,
            "step {by_step}: {gems} and {imbued}"
        );
    }
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

/// A give that requires a quest failed runs only with it failed; under way, the item is refused
/// and handed back.
///
/// Behaviour: server.quests.a-step-requires-a-quest-failed
#[test]
fn a_give_requiring_a_quest_failed_runs_only_once_it_has_failed() {
    const TIMER: u32 = 0x4000_7777;
    let mut h = world();
    h.server.quests.gives.push(Give {
        requires: vec![(TIMER, Need::Failed)],
        ..taking(Some(ASHES))
    });
    h.server
        .talk
        .lines
        .push(taking_line(Some(ASHES), TAKING_LINE, 0));
    let (b, me) = player(&mut h, "one", "Elara");
    let ashes = hold(&mut h, me, ASHES, 1);
    at_phase(&mut h, TIMER, 1);
    let (lines, _) = give_ashes(&mut h, b, me, ashes, 1);
    assert_eq!(lines, vec![PLAIN_REFUSAL], "under way");
    assert!(holds(&h, me, ashes.0), "handed back");
    for p in h.server.players.values_mut() {
        for q in &mut p.play.quests {
            if q.quest == TIMER {
                q.status = yalain_world::quests::status::FAILED;
                q.phase = 0;
            }
        }
    }
    let (lines, _) = give_ashes(&mut h, b, me, ashes, 1);
    assert_eq!(lines, vec![TAKING_LINE], "failed");
    assert!(!holds(&h, me, ashes.0));
}

/// A give that requires a quest never completed takes the item from one who never completed it;
/// from one who completed it, even given it again since, the item is refused and handed back.
///
/// Behaviour: server.quests.a-step-waits-on-a-quest-never-completed
#[test]
fn a_give_requiring_a_quest_never_completed_refuses_one_who_ever_completed_it() {
    const DESCENT: u32 = 0x4000_02A0;
    let mut h = world();
    h.server.quests.gives.push(Give {
        requires: vec![(DESCENT, Need::NeverDone)],
        ..taking(Some(ASHES))
    });
    h.server
        .talk
        .lines
        .push(taking_line(Some(ASHES), TAKING_LINE, 0));
    let (b, me) = player(&mut h, "one", "Elara");
    let ashes = hold(&mut h, me, ASHES, 1);
    at_phase(&mut h, DESCENT, 1);
    for p in h.server.players.values_mut() {
        p.play.completed_before = vec![DESCENT];
    }
    let (lines, _) = give_ashes(&mut h, b, me, ashes, 1);
    assert_eq!(lines, vec![PLAIN_REFUSAL], "completed before");
    assert!(holds(&h, me, ashes.0), "handed back");
    for p in h.server.players.values_mut() {
        p.play.completed_before.clear();
        for q in &mut p.play.quests {
            if q.quest == DESCENT {
                q.status = yalain_world::quests::status::COMPLETED;
                q.phase = 0;
            }
        }
    }
    let (lines, _) = give_ashes(&mut h, b, me, ashes, 1);
    assert_eq!(lines, vec![PLAIN_REFUSAL], "completed");
    assert!(holds(&h, me, ashes.0), "handed back");
    at_phase(&mut h, DESCENT, 1);
    let (lines, _) = give_ashes(&mut h, b, me, ashes, 1);
    assert_eq!(lines, vec![TAKING_LINE], "never completed");
    assert!(!holds(&h, me, ashes.0));
}

/// A give that waits on its player having fought a camp's creature takes the item from one who
/// harmed the camp's last creature to die; from anyone else it is refused and handed back.
///
/// Behaviour: server.quests.a-step-waits-on-its-player-having-fought
#[test]
fn a_give_waiting_on_a_fight_takes_the_item_only_from_one_who_fought() {
    use yalain_content::fought::FoughtGates;
    let mut h = world();
    let give = taking(Some(ASHES));
    h.server.quests.gives.push(give.clone());
    h.server.content.fought_gates = FoughtGates {
        steps: Vec::new(),
        gives: vec![(give, vec![0])],
    };
    h.server
        .camps
        .push(yalain_world::spawn::CampState::default());
    h.server
        .talk
        .lines
        .push(taking_line(Some(ASHES), TAKING_LINE, 0));
    let (b, me) = player(&mut h, "one", "Elara");
    let ashes = hold(&mut h, me, ASHES, 1);
    h.server.camps[0].fought = vec![me + 1];
    let (lines, _) = give_ashes(&mut h, b, me, ashes, 1);
    assert_eq!(lines, vec![PLAIN_REFUSAL], "another fought it");
    assert!(holds(&h, me, ashes.0), "handed back");
    h.server.camps[0].fought = vec![me + 1, me];
    let (lines, _) = give_ashes(&mut h, b, me, ashes, 1);
    assert_eq!(lines, vec![TAKING_LINE], "fought it");
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

/// Put players in one fellowship.
fn fellowship(h: &mut Harness, ids: &[u64]) {
    use yalain_world::fellowship::{Fellow, Fellowship, Vitals};
    h.server.fellowships.list.push(Fellowship {
        name: "Hunters".into(),
        leader: ids[0],
        flags: 0,
        room: 1,
        fellows: ids
            .iter()
            .map(|&id| Fellow {
                id,
                joined: 0.0,
                sent: Vitals::default(),
                next_check: f64::INFINITY,
            })
            .collect(),
        last_claimant: 0,
    });
}

/// Put one player's quest under way at a phase.
fn player_at_phase(h: &mut Harness, me: u64, quest: u32, phase: u32) {
    let world = world_of(h, me);
    let p = h.server.players.get_mut(&world).expect("in the world");
    p.play.quests.retain(|q| q.quest != quest);
    p.play.quests.push(yalain_world::state::QuestState {
        quest,
        status: yalain_world::quests::status::UNDERWAY,
        phase,
        started: 0,
        updated: 0,
    });
}

/// A player's quest: its status and phase, if held.
fn quest_of(h: &Harness, me: u64, quest: u32) -> Option<(u32, u32)> {
    h.server.players[&world_of(h, me)]
        .play
        .quests
        .iter()
        .find(|q| q.quest == quest)
        .map(|q| (q.status, q.phase))
}

/// An item handed in for a step the fellowship shares: the giver's item kept, every fellow at
/// the phase the step needs moved on and told it alone, one holding no such item among them and
/// one holding its own, which is left in its pack; a fellow at another phase not moved.
///
/// Behaviour: server.gives.fellowship-hand-in
#[test]
fn a_hand_in_shared_with_the_fellowship_moves_each_fellow_at_its_phase_and_takes_the_givers_item_alone(
) {
    let mut h = world();
    h.server.quests.steps[0].fellows = true;
    let (a, me) = player(&mut h, "one", "Elara");
    let (b, you) = player(&mut h, "two", "Bran");
    let (c, them) = player(&mut h, "three", "Cade");
    let (_, holder) = player(&mut h, "four", "Dara");
    fellowship(&mut h, &[me, you, them, holder]);
    player_at_phase(&mut h, me, QUEST, 2);
    player_at_phase(&mut h, you, QUEST, 2);
    player_at_phase(&mut h, them, QUEST, 1);
    player_at_phase(&mut h, holder, QUEST, 2);
    let (ashes, slot) = hold(&mut h, me, ASHES, 1);
    let (theirs, _) = hold(&mut h, holder, ASHES, 1);
    let (fa, fb, fc) = (
        h.bots[a].inbox.len(),
        h.bots[b].inbox.len(),
        h.bots[c].inbox.len(),
    );
    h.bots[a].send(true, queue::WEENIE, &give(me, ashes, slot, PERSON, 1));
    h.run(steps(HANDED_AFTER) + 8);
    let underway = yalain_world::quests::status::UNDERWAY;
    assert!(!holds(&h, me, ashes), "the giver's item kept");
    assert_eq!(updates(&h, a, fa), vec![(0x2000_0003, QUEST, 3)]);
    assert_eq!(
        quest_of(&h, you, QUEST),
        Some((underway, 3)),
        "the fellow moved"
    );
    assert_eq!(updates(&h, b, fb), vec![(0x2000_0003, QUEST, 3)], "told it");
    assert_eq!(quest_of(&h, holder, QUEST), Some((underway, 3)));
    assert!(holds(&h, holder, theirs), "a fellow's own item left");
    assert_eq!(
        quest_of(&h, them, QUEST),
        Some((underway, 1)),
        "a fellow at another phase not moved"
    );
    assert!(updates(&h, c, fc).is_empty());
}

/// What a step shared with the fellowship gives is the giver's alone: a fellow is moved on with the
/// giver and given nothing.
///
/// Behaviour: server.gives.fellowship-hand-in-gives-to-the-giver
#[test]
fn a_hand_in_shared_with_the_fellowship_gives_its_items_to_the_giver_alone() {
    let mut h = world();
    h.server.quests.steps[1].fellows = true;
    let (a, me) = player(&mut h, "one", "Elara");
    let (b, you) = player(&mut h, "two", "Bran");
    fellowship(&mut h, &[me, you]);
    player_at_phase(&mut h, me, QUEST, 3);
    player_at_phase(&mut h, you, QUEST, 3);
    let (sliver, slot) = hold(&mut h, me, SLIVER, 1);
    let fb = h.bots[b].inbox.len();
    h.bots[a].send(true, queue::WEENIE, &give(me, sliver, slot, PERSON, 1));
    h.run(steps(HANDED_AFTER) + 8);
    let imbued = |h: &Harness, who: u64| {
        h.server.players[&world_of(h, who)]
            .play
            .items
            .iter()
            .any(|x| x.template == IMBUED)
    };
    assert!(imbued(&h, me), "the giver given the item");
    assert_eq!(
        updates(&h, b, fb),
        vec![(0x3000_0000, QUEST, 0)],
        "the fellow's quest done"
    );
    assert!(!imbued(&h, you), "the fellow given nothing");
}

/// A give's step that leads somewhere keeps the item, tells its update and then takes the player
/// through portal space to its place; so does a take's step, as the item comes into the pack.
///
/// Behaviour: server.quests.a-give-or-take-that-leads-on
#[test]
fn a_give_or_a_takes_step_that_leads_somewhere_tells_its_update_then_takes_the_player() {
    const ENTER_PORTAL_SPACE: u32 = 0x8212_0020;
    let at = |cell: u32| yalain_content::quests::Place {
        cell,
        origin: [100.0, 20.0, 50.0],
        heading: 0,
    };
    let (there, then) = (0x6CB9_0029, 0x6CB9_0030);
    let mut h = world();
    h.server.quests.steps = vec![
        Step {
            takes: vec![ASHES],
            to: Some(at(there)),
            ..step(QUEST, 2, Trigger::Give(ASHES, PERSON_T), Update::Advance(3))
        },
        Step {
            to: Some(at(then)),
            ..step(QUEST, 3, Trigger::Take(SLIVER), Update::Complete)
        },
    ];
    let (b, me) = player(&mut h, "one", "Elara");
    at_phase(&mut h, QUEST, 2);
    let (ashes, slot) = hold(&mut h, me, ASHES, 1);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &give(me, ashes, slot, PERSON, 1));
    h.run(steps(HANDED_AFTER) + 8);
    assert!(!holds(&h, me, ashes), "kept");
    let calls = h.functions(b, from);
    let told = calls.iter().position(|f| *f == UPDATE_QUEST);
    let went = calls.iter().position(|f| *f == ENTER_PORTAL_SPACE);
    assert!(
        told.is_some() && went.is_some() && told < went,
        "the update, then portal space: {calls:08X?}"
    );
    h.run(steps(12.0));
    let p = &h.server.players[&world_of(&h, me)];
    assert_eq!(p.position.cell, there, "at the give's place");
    assert!(p.trip.is_none());
    // The take: the sliver comes into the pack.
    hold(&mut h, me, SLIVER, 1);
    let from = h.bots[b].inbox.len();
    let (world, clock) = (world_of(&h, me), h.clock());
    yalain_world::quests::took(&mut h.server, &mut h.net, world, SLIVER, clock);
    h.run(5);
    let calls = h.functions(b, from);
    let told = calls.iter().position(|f| *f == UPDATE_QUEST);
    let went = calls.iter().position(|f| *f == ENTER_PORTAL_SPACE);
    assert!(
        told.is_some() && went.is_some() && told < went,
        "the update, then portal space: {calls:08X?}"
    );
    assert_eq!(updates(&h, b, from), vec![(0x3000_0000, QUEST, 0)]);
    h.run(steps(12.0));
    assert_eq!(
        h.server.players[&world_of(&h, me)].position.cell,
        then,
        "at the take's place"
    );
}

/// A give's step that takes the fellowship along sends every fellow in the world to its place with
/// the giver, the one beside it and the one far off in another landblock alike, whatever quests they
/// hold; a fellow who is dead stays, as do one whose scene isn't drawn yet, one already on its way
/// elsewhere and a player who is no fellow. Without it, the giver goes alone.
///
/// Behaviour: server.quests.a-step-takes-the-fellowship-along
#[test]
fn a_step_that_takes_the_fellowship_along_sends_every_fellow_wherever_it_is() {
    const THERE: u32 = 0x04A9_0100;
    const ELSEWHERE: u32 = 0x7F91_0034;
    for along in [true, false] {
        let mut h = world();
        h.server.quests.steps = vec![Step {
            takes: vec![ASHES],
            to: Some(yalain_content::quests::Place {
                cell: THERE,
                origin: [10.0, 20.0, 5.0],
                heading: 0,
            }),
            to_fellows: along,
            ..step(QUEST, 2, Trigger::Give(ASHES, PERSON_T), Update::Advance(3))
        }];
        let (b, me) = player(&mut h, "one", "Elara");
        let (_, near) = player(&mut h, "two", "Bran");
        let (_, far) = player(&mut h, "three", "Cael");
        let (_, dead) = player(&mut h, "four", "Dun");
        let (_, other) = player(&mut h, "five", "Eir");
        let (_, busy) = player(&mut h, "six", "Fen");
        // In the world, its scene not yet drawn.
        let (_, loading) = h.player_in_world("seven", "Gil");
        fellowship(&mut h, &[me, near, far, dead, busy, loading]);
        player_at_phase(&mut h, me, QUEST, 2);
        let w = world_of(&h, far);
        let p = h.server.players.get_mut(&w).expect("in the world");
        p.position.cell = ELSEWHERE;
        p.position.origin = [100.0, 100.0, 40.0];
        let w = world_of(&h, dead);
        let p = h.server.players.get_mut(&w).expect("in the world");
        yalain_world::quals::set_int(&mut p.qualities, yalain_world::player::quality::HEALTH, 0);
        let w = world_of(&h, busy);
        let p = h.server.players.get_mut(&w).expect("in the world");
        let mut away = p.position;
        away.cell = ELSEWHERE;
        p.trip = Some(yalain_world::places::Trip {
            to: away,
            used: None,
            step: None,
            resurrect: false,
            sent: true,
            said: None,
            updates: Vec::new(),
        });
        let cell = |h: &Harness, who: u64| h.server.players[&world_of(h, who)].position.cell;
        let before = [near, dead, other, busy, loading].map(|who| cell(&h, who));
        let (ashes, slot) = hold(&mut h, me, ASHES, 1);
        h.bots[b].send(true, queue::WEENIE, &give(me, ashes, slot, PERSON, 1));
        h.run(steps(HANDED_AFTER) + 8);
        h.run(steps(12.0));
        assert_eq!(cell(&h, me), THERE, "the giver went");
        if along {
            assert_eq!(cell(&h, near), THERE, "the fellow beside it went");
            assert_eq!(cell(&h, far), THERE, "the fellow far off went");
        } else {
            assert_eq!(cell(&h, near), before[0], "alone: the fellow stays");
            assert_eq!(cell(&h, far), ELSEWHERE);
        }
        assert_eq!(cell(&h, dead), before[1], "a dead fellow stays");
        assert_eq!(cell(&h, other), before[2], "no fellow stays");
        assert_eq!(
            cell(&h, loading),
            before[4],
            "a fellow not yet in the scene stays"
        );
        let p = &h.server.players[&world_of(&h, busy)];
        assert_eq!(
            (p.position.cell, p.trip.as_ref().map(|t| t.to.cell)),
            (before[3], Some(ELSEWHERE)),
            "a fellow on its way elsewhere keeps going there"
        );
        let holds_quest = |h: &Harness, who: u64| {
            h.server.players[&world_of(h, who)]
                .play
                .quests
                .iter()
                .any(|q| q.quest == QUEST)
        };
        assert!(!holds_quest(&h, near), "sent, its quests not moved");
        assert!(h
            .server
            .players
            .values()
            .all(|p| p.character == busy || p.trip.is_none()));
    }
}

/// A step's give of a class runs for an item of any template of the class (its package the class,
/// or one deriving from it) and hands the item back; an item of no template of the class is refused
/// and handed back with no quest moved. A give that moves no quest naming a class keeps any item
/// of it, and takes no other.
///
/// Behaviour: server.gives.any-item-of-a-class
#[test]
fn a_give_of_a_class_runs_for_any_item_of_it_and_for_no_other() {
    const WEAPON: u32 = 0x1049;
    const AXE_PACKAGE: u32 = 0x1058;
    const AXE: u32 = 0x4700_1601;
    const BLADE: u32 = 0x4700_1602;
    let mut h = world();
    h.server.catalog.insert(yalain_testkit::test_template(
        AXE,
        AXE_PACKAGE,
        Props::default(),
    ));
    h.server.catalog.insert(yalain_testkit::test_template(
        BLADE,
        WEAPON,
        Props::default(),
    ));
    h.server
        .item_classes
        .bases
        .insert(AXE_PACKAGE, vec![WEAPON]);
    h.server.quests.steps = vec![step(
        QUEST,
        1,
        Trigger::GiveClass(WEAPON, PERSON_T),
        Update::Advance(2),
    )];
    let (b, me) = player(&mut h, "one", "Elara");
    at_phase(&mut h, QUEST, 1);
    let junk = hold(&mut h, me, JUNK, 1);
    let (_, moved) = give_ashes(&mut h, b, me, junk, 1);
    assert!(moved.is_empty(), "no weapon: refused");
    assert!(holds(&h, me, junk.0), "handed back");
    let axe = hold(&mut h, me, AXE, 1);
    let (_, moved) = give_ashes(&mut h, b, me, axe, 1);
    assert_eq!(
        moved,
        vec![(0x2000_0002, QUEST, 2)],
        "a weapon by its class"
    );
    assert!(holds(&h, me, axe.0), "handed back");
    // A give that moves no quest, for any weapon.
    h.server.quests.steps.clear();
    let row = Give {
        gives: vec![IMBUED],
        ..taking(None)
    };
    h.server.quests.gives.push(row);
    h.server.content.class_gives.insert(0, WEAPON);
    let blade = hold(&mut h, me, BLADE, 1);
    give_ashes(&mut h, b, me, blade, 1);
    assert!(!holds(&h, me, blade.0), "kept");
    assert!(h.server.players[&world_of(&h, me)]
        .play
        .items
        .iter()
        .any(|x| x.template == IMBUED));
    let junk = hold(&mut h, me, JUNK, 1);
    give_ashes(&mut h, b, me, junk, 1);
    assert!(holds(&h, me, junk.0), "no weapon: handed back");
}

/// A give's step that hands its item back puts the effects its row gives it on the item, each for its
/// own time on the game clock (an imbuer's spell of thirty minutes on the weapon given); another
/// step of the same quest that names none puts nothing on it, and the effect runs out at its time.
///
/// Behaviour: server.quests.a-step-puts-effects-on-the-item-handed-back
#[test]
fn a_give_step_puts_its_effects_on_the_item_it_hands_back() {
    use yalain_world::effect_defs::{EffectDef, Kind, Scale};
    const WEAPON: u32 = 0x1049;
    const AXE: u32 = 0x4700_1601;
    const SPELL: u32 = 0x6F00_7411;
    const APPLIED: u32 = 0x6F00_7412;
    let mut h = world();
    h.server
        .catalog
        .insert(yalain_testkit::test_template(AXE, WEAPON, Props::default()));
    h.server.effect_table.effects.insert(
        SPELL,
        EffectDef {
            state: SPELL,
            kind: Kind::Applies(APPLIED),
            duration: Scale(vec![(0.0, 1800.0)]),
            priority: Scale(vec![(0.0, 1.0)]),
            fx: 0,
            external_flags: 0,
            internal_flags: 0,
            enum_value: 0,
            eq_class: 4056,
            name: None,
            ends_at_logout: false,
            ends_at_teleport: false,
        },
    );
    let imbue = Step {
        key: "imbue".into(),
        ..step(
            QUEST,
            1,
            Trigger::GiveClass(WEAPON, PERSON_T),
            Update::Advance(2),
        )
    };
    let plain = Step {
        key: "plain".into(),
        ..step(
            QUEST,
            2,
            Trigger::GiveClass(WEAPON, PERSON_T),
            Update::Advance(3),
        )
    };
    h.server.quests.steps = vec![imbue, plain];
    h.server
        .content
        .given_effects
        .insert((QUEST, "imbue".into()), vec![SPELL]);
    let (b, me) = player(&mut h, "one", "Elara");
    at_phase(&mut h, QUEST, 1);
    let axe = hold(&mut h, me, AXE, 1);
    let (_, moved) = give_ashes(&mut h, b, me, axe, 1);
    assert_eq!(moved, vec![(0x2000_0002, QUEST, 2)], "imbued");
    assert!(holds(&h, me, axe.0), "handed back");
    let on = |h: &Harness| {
        let t = h.server.things.get(axe.0).expect("the axe");
        yalain_world::effects::running_on_item(&h.server, t)
    };
    let held = on(&h);
    assert_eq!(held.len(), 1, "{held:?}");
    assert_eq!((held[0].state, held[0].spellcraft), (SPELL, 1.0));
    let left = held[0].until - h.server.game_now;
    assert!((1795.0..=1800.0).contains(&left), "{left} s left");
    // The other step puts nothing more on it.
    let other = hold(&mut h, me, AXE, 1);
    let (_, moved) = give_ashes(&mut h, b, me, other, 1);
    assert_eq!(
        moved.iter().map(|m| (m.1, m.2)).collect::<Vec<_>>(),
        [(QUEST, 3)]
    );
    assert!(h
        .server
        .things
        .get(other.0)
        .is_some_and(|t| t.timed.is_empty()));
    h.server.game_now += 1800.0;
    assert!(on(&h).is_empty(), "run out");
}

/// Two gives that move no quest, alike but for the class each takes, each take an item of their
/// own class: one of the second class is kept by the second, not refused for the first's.
///
/// Behaviour: server.gives.any-item-of-a-class
#[test]
fn two_gives_alike_but_for_their_class_each_take_their_own_class() {
    const WEAPON: u32 = 0x1049;
    const ARMOR: u32 = 0x1100;
    const AXE: u32 = 0x4700_1601;
    const HELM: u32 = 0x4700_1603;
    let mut h = world();
    h.server
        .catalog
        .insert(yalain_testkit::test_template(AXE, WEAPON, Props::default()));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(HELM, ARMOR, Props::default()));
    h.server.quests.steps.clear();
    for (i, class) in [WEAPON, ARMOR].into_iter().enumerate() {
        h.server.quests.gives.push(Give {
            gives: vec![IMBUED],
            ..taking(None)
        });
        h.server.content.class_gives.insert(i, class);
    }
    let (b, me) = player(&mut h, "one", "Elara");
    let helm = hold(&mut h, me, HELM, 1);
    give_ashes(&mut h, b, me, helm, 1);
    assert!(!holds(&h, me, helm.0), "kept by the second");
    let axe = hold(&mut h, me, AXE, 1);
    give_ashes(&mut h, b, me, axe, 1);
    assert!(!holds(&h, me, axe.0), "kept by the first");
}

/// The behaviors a player played since `from`, each one's behavior and target, with their place
/// among the messages in the order sent.
fn handings(h: &Harness, b: usize, me: u64, from: usize) -> Vec<(usize, u64)> {
    h.sent_messages(b, from)
        .iter()
        .enumerate()
        .filter_map(|(n, m)| match m {
            ServerMessage::PlayBehavior(e) if e.sender.id == me && e.action.behavior == HANDING => {
                Some((n, e.action.target))
            }
            _ => None,
        })
        .collect()
}

/// The containers a client was told an item is in since `from`, in order.
fn containers(h: &Harness, b: usize, item: u64, from: usize) -> Vec<u64> {
    h.sent_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::UpdateInstanceIdVisual(q)
                if q.object.id == item && q.quality == CONTAINER =>
            {
                Some(q.value)
            }
            _ => None,
        })
        .collect()
}

/// An item given to the player's own pet is walked to and handed over as any give, and 1.31 s
/// later refused as one the pet is not accepting, sent in this order: the item back in its pack,
/// the answer still merging, the animation played, then the player handing the item over again.
///
/// Behaviour: server.gives.a-pet-refuses
#[test]
fn an_item_given_to_a_pet_is_handed_over_then_refused_and_comes_back() {
    let mut h = world();
    let (b, me, pet) = super::pets::with_lasher(&mut h);
    let (item, slot) = hold(&mut h, me, JUNK, 1);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &give(me, item, slot, pet, 1));
    h.run(steps(HANDED_AFTER) - 4);
    let walked = h.world_messages(b, from).iter().any(|m| {
        matches!(m, ServerMessage::MoveTo(e) if e.sender.id == me
            && e.action.params.flags == 0x61
            && e.action.params.target == auberean_protocol::types::MoveTarget::Object(pet))
    });
    assert!(walked, "walked to the pet");
    assert_eq!(handings(&h, b, me, from).len(), 1, "handed over");
    assert_eq!(
        containers(&h, b, item, from),
        vec![0],
        "the container cleared"
    );
    assert_eq!(
        give_sent(&h, b, me, item, from),
        [Sent::Stopped, Sent::Container(0), Sent::Handed],
        "handed over in the order sent"
    );
    assert!(answers(&h, b, from).is_empty(), "not answered yet");
    let before = h.bots[b].inbox.len();
    h.run(8);
    let a = answers(&h, b, before);
    assert_eq!(a.len(), 1, "answered: {a:?}");
    assert_eq!(
        (a[0].status, a[0].merge, a[0].played, a[0].move_type),
        (status::NOT_ACCEPTING, true, true, GIVE)
    );
    assert_eq!((a[0].item, a[0].target, a[0].from), (item, pet, me));
    assert_eq!(
        handings(&h, b, me, before)
            .iter()
            .map(|x| x.1)
            .collect::<Vec<_>>(),
        vec![item],
        "handed over again as it is back"
    );
    assert_eq!(
        containers(&h, b, item, before),
        vec![me],
        "back in the pack"
    );
    assert_eq!(
        give_sent(&h, b, me, item, before),
        [Sent::Container(me), Sent::Answered, Sent::Handed],
        "refused in the order sent"
    );
    assert!(holds(&h, me, item));
    assert!(h.server.players[&world_of(&h, me)].giving.is_none());
}

/// An attuned item given to a pet is refused as the walk to it ends, as attuned: nothing handed
/// over, the item never leaving the pack.
///
/// Behaviour: server.gives.a-pet-refuses-an-attuned-item
#[test]
fn an_attuned_item_given_to_a_pet_is_refused_at_the_walks_end_with_nothing_handed_over() {
    let mut h = world();
    let (b, me, pet) = super::pets::with_lasher(&mut h);
    let (item, slot) = hold(&mut h, me, JUNK, 1);
    h.server.things.get_mut(item).expect("made").own.attuned = true;
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &give(me, item, slot, pet, 1));
    h.run(steps(HANDED_AFTER) + 8);
    let a = answers(&h, b, from);
    assert_eq!(a.len(), 1, "answered: {a:?}");
    assert_eq!(
        (a[0].status, a[0].item, a[0].target),
        (status::ATTUNED, item, pet)
    );
    assert!(handings(&h, b, me, from).is_empty(), "nothing handed over");
    assert!(
        containers(&h, b, item, from).is_empty(),
        "never left the pack"
    );
    assert!(holds(&h, me, item));
}

/// A give whose creature dies on the way is refused as one that could not get there at the
/// player's next report, nothing handed over, and the player may give again.
///
/// Behaviour: server.gives.what-it-walks-to-is-gone
#[test]
fn a_give_to_a_pet_that_dies_on_the_way_is_refused_as_one_that_cannot_get_there() {
    let mut h = world();
    let (b, me, pet) = super::pets::with_lasher(&mut h);
    let (item, slot) = hold(&mut h, me, JUNK, 1);
    let world = world_of(&h, me);
    h.server
        .players
        .get_mut(&world)
        .expect("in")
        .position
        .origin[1] += 20.0;
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &give(me, item, slot, pet, 1));
    h.run(4);
    assert!(answers(&h, b, from).is_empty(), "walking");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(yalain_world::pets::function::DIE, &[Arg::LongInt(0)]),
    );
    h.run(10);
    let at = h.server.players[&world].position;
    let here = auberean_primitives::Vec3::new(at.origin[0], at.origin[1], at.origin[2]);
    let stamp = h.server.players[&world].teleport_stamp;
    let idle = yalain_world::world::IDLE_MOTION;
    h.bots[b].send(true, queue::WEENIE, &report(at.cell, here, 0, idle, stamp));
    h.run(3);
    let a = answers(&h, b, from);
    assert_eq!(a.len(), 1, "{a:?}");
    assert_eq!(a[0].status, status::CANT_GET_THERE);
    assert!(handings(&h, b, me, from).is_empty(), "nothing handed over");
    assert!(holds(&h, me, item));
    assert!(
        h.server.players[&world].giving.is_none(),
        "free to give again"
    );
}

const EMOTE_LINE: u32 = 0x8A69_7D3E;
const AFTER_LINE: u32 = 0x42E6_0A37;
const SHOW_TEXT: u32 = 0x8205_0011;

/// The emotes a person showed a client since `from`.
fn emoted(h: &Harness, b: usize, from: usize) -> Vec<u32> {
    h.calls(b, from)
        .into_iter()
        .filter(|(f, _)| *f == SHOW_TEXT)
        .filter_map(|(_, v)| {
            v.iter().find_map(|x| match x.root() {
                Some(Node::TextInfo(t)) => Some(t.string_id),
                _ => None,
            })
        })
        .collect()
}

/// The world of [`world`] whose ashes step is said with two lines, an emote that pauses
/// `pause_ms` and a line after it, and (with `step_ms`) pauses itself first.
fn pausing_world(pause_ms: u32, step_ms: Option<u32>) -> Harness {
    let mut h = world();
    h.server.talk.lines.retain(|l| l.quest != Some(QUEST));
    h.server.talk.lines.push(Line {
        how: How::Emote,
        phase: Some(3),
        pause_ms,
        ..line(PERSON_T, EMOTE_LINE, None, Some(QUEST))
    });
    h.server.talk.lines.push(Line {
        phase: Some(3),
        order: 1,
        ..line(PERSON_T, AFTER_LINE, None, Some(QUEST))
    });
    for st in &mut h.server.quests.steps {
        if st.needs == 2 {
            st.key = "ashes".into();
        }
    }
    if let Some(ms) = step_ms {
        h.server
            .content
            .step_pauses
            .insert((QUEST, "ashes".into()), ms);
    }
    h
}

/// A give's answer for the ashes, and the inbox mark before it.
fn ashes_given(h: &mut Harness, b: usize, me: u64) -> (u64, usize) {
    at_phase(h, QUEST, 2);
    let (ashes, slot) = hold(h, me, ASHES, 1);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &give(me, ashes, slot, PERSON, 1));
    h.run(steps(HANDED_AFTER) + 4);
    assert_eq!(answers(h, b, from).len(), 1, "answered");
    (ashes, from)
}

/// A line said with a step that pauses holds the rest of the step: the person's emote comes with
/// the give's answer and the item is kept then, and its next line and the quest's update come 2 s
/// later, not before.
/// Behaviour: server.quests.a-line-pauses-its-step
#[test]
fn a_line_that_pauses_holds_the_rest_of_its_step() {
    let mut h = pausing_world(2000, None);
    let (b, me) = player(&mut h, "one", "Elara");
    let (ashes, from) = ashes_given(&mut h, b, me);
    assert_eq!(emoted(&h, b, from), vec![EMOTE_LINE], "the emote at once");
    assert!(!holds(&h, me, ashes), "the ashes kept at once");
    assert!(told(&h, b, from).is_empty(), "the next line waits");
    assert!(updates(&h, b, from).is_empty(), "the update waits");
    h.run(steps(2.0) - 10);
    assert!(updates(&h, b, from).is_empty(), "not before 2 s");
    h.run(12);
    assert_eq!(told(&h, b, from), vec![AFTER_LINE]);
    assert_eq!(updates(&h, b, from), vec![(0x2000_0003, QUEST, 3)]);
}

/// A step that pauses says and does nothing until its pause is through: not its lines, not its
/// update; then all of it, its own pauses after.
/// Behaviour: server.quests.a-step-pauses
#[test]
fn a_step_that_pauses_says_and_does_nothing_until_its_pause_is_through() {
    let mut h = pausing_world(0, Some(1000));
    let (b, me) = player(&mut h, "one", "Elara");
    let (_, from) = ashes_given(&mut h, b, me);
    assert!(
        emoted(&h, b, from).is_empty() && told(&h, b, from).is_empty(),
        "nothing said"
    );
    assert!(updates(&h, b, from).is_empty(), "nothing moved");
    h.run(steps(1.0) + 2);
    assert_eq!(emoted(&h, b, from), vec![EMOTE_LINE]);
    assert_eq!(told(&h, b, from), vec![AFTER_LINE]);
    assert_eq!(updates(&h, b, from), vec![(0x2000_0003, QUEST, 3)]);
}

/// While a step of a quest is paused for a player, no other step of that quest runs for the
/// player: a second gift in the pause moves nothing and is handed back, and the quest moves once.
/// Behaviour: server.quests.a-pause-keeps-its-step-whole
#[test]
fn a_paused_steps_quest_runs_no_other_step_until_it_is_done() {
    let mut h = pausing_world(2000, None);
    let (b, me) = player(&mut h, "one", "Elara");
    let (_, from) = ashes_given(&mut h, b, me);
    let (second, slot) = hold(&mut h, me, ASHES, 1);
    h.bots[b].send(true, queue::WEENIE, &give(me, second, slot, PERSON, 1));
    h.run(steps(3.0) + steps(gives::HANDED_BACK_AFTER));
    assert_eq!(
        updates(&h, b, from),
        vec![(0x2000_0003, QUEST, 3)],
        "moved once"
    );
    assert!(holds(&h, me, second), "the second handed back");
}

/// One who leaves the world in a step's pause has what the step was for: its quest moved as it
/// left, kept for when it comes back.
/// Behaviour: server.quests.a-pause-keeps-its-step-whole
#[test]
fn a_step_paused_as_its_player_leaves_runs_then() {
    let mut h = pausing_world(2000, None);
    let (b, me) = player(&mut h, "one", "Elara");
    ashes_given(&mut h, b, me);
    let w = world_of(&h, me);
    yalain_world::world::leave(&mut h.server, &mut h.net, w);
    h.run(2);
    let st = yalain_store::Store::world_state(&mut h.server.store, me)
        .expect("reads")
        .expect("kept");
    let play = yalain_world::state::PlayState::from_bytes(&st.play).expect("reads");
    assert_eq!(
        play.quests
            .iter()
            .find(|q| q.quest == QUEST)
            .map(|q| q.phase),
        Some(3),
        "moved on"
    );
}

const NEXT: u32 = 0x4000_0BC0;
const TOOK: u32 = 0x4000_0BC1;
const LAST_LINE: u32 = 0xA303_0C8E;

/// The calls and the destroying of a gift in the order they were sent (a blob id's low half): the
/// function of each, with a tell's or a text's string.
fn sent(h: &Harness, b: usize, from: usize) -> Vec<(u32, u32)> {
    let mut v: Vec<(u32, (u32, u32))> = h.bots[b].inbox[from..]
        .iter()
        .filter(|r| r.world)
        .filter_map(|r| {
            let call = match r.message()? {
                ServerMessage::PlayerCall(c) => c,
                ServerMessage::VisualObjectCall(c) => c.call,
                _ => return None,
            };
            let text = call
                .args
                .as_ref()
                .and_then(|a| calls::values(&a.0).ok())
                .and_then(|v| {
                    v.iter().find_map(|x| match x.root() {
                        Some(Node::TextInfo(t)) => Some(t.string_id),
                        _ => None,
                    })
                })
                .unwrap_or(0);
            #[allow(clippy::cast_possible_truncation)]
            let seq = r.blob_id as u32;
            Some((seq, (call.function.0, text)))
        })
        .collect();
    v.sort_by_key(|(seq, _)| *seq);
    v.into_iter().map(|(_, x)| x).collect()
}

/// A player's quest, as it stands: its status.
fn status_of(h: &Harness, me: u64, quest: u32) -> Option<u32> {
    h.server.players[&world_of(h, me)]
        .play
        .quests
        .iter()
        .find(|q| q.quest == quest)
        .map(|q| q.status)
}

/// The quests kept of a player who left the world: each one's status and phase.
fn kept_quests(h: &mut Harness, me: u64) -> Vec<(u32, u32, u32)> {
    let st = yalain_store::Store::world_state(&mut h.server.store, me)
        .expect("reads")
        .expect("kept");
    yalain_world::state::PlayState::from_bytes(&st.play)
        .expect("reads")
        .quests
        .iter()
        .map(|q| (q.quest, q.status, q.phase))
        .collect()
}

/// The world of [`world`] with two lines at the sliver's completion, and a quest given as that one
/// is done, its step pausing 1.5 s.
fn finishing_world() -> Harness {
    let mut h = world();
    h.server.talk.lines.retain(|l| l.quest != Some(QUEST));
    h.server.talk.lines.push(Line {
        update: Some(TalkUpdate::Complete),
        ..line(PERSON_T, AFTER_LINE, None, Some(QUEST))
    });
    h.server.talk.lines.push(Line {
        update: Some(TalkUpdate::Complete),
        order: 1,
        after_update_ms: 1500,
        ..line(PERSON_T, LAST_LINE, None, Some(QUEST))
    });
    h.server.quests.quests.push(quest(NEXT, 2));
    h.server.quests.steps.push(Step {
        key: "next".into(),
        ..step(NEXT, 0, Trigger::Done(QUEST), Update::Bestow)
    });
    h.server
        .content
        .step_pauses
        .insert((NEXT, "next".into()), 1500);
    h
}

/// The sliver given at the quest's last phase and answered; the inbox mark before it.
fn sliver_given(h: &mut Harness, b: usize, me: u64) -> usize {
    at_phase(h, QUEST, 3);
    let (sliver, slot) = hold(h, me, SLIVER, 1);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &give(me, sliver, slot, PERSON, 1));
    h.run(steps(HANDED_AFTER) + 4);
    assert!(!answers(h, b, from).is_empty(), "answered");
    from
}

/// A line said so long after its update holds nothing: the first line and the completion come at
/// once, and that line and the quest given as that one is done 1.5 s later, the line first (2017,
/// by the order sent: 18 of 18). With that quest under way already the line still comes, and
/// nothing is given.
/// Behaviour: server.quests.a-line-after-its-update-holds-nothing
#[test]
fn a_line_after_its_update_holds_neither_it_nor_the_completion() {
    for held in [false, true] {
        let mut h = finishing_world();
        let (b, me) = player(&mut h, "one", "Elara");
        if held {
            at_phase(&mut h, NEXT, 1);
        }
        let from = sliver_given(&mut h, b, me);
        assert_eq!(told(&h, b, from), [AFTER_LINE], "the first line at once");
        assert_eq!(
            updates(&h, b, from),
            [(0x3000_0000, QUEST, 0)],
            "completed at once, held by nothing"
        );
        h.run(steps(1.5) - 10);
        assert_eq!(told(&h, b, from), [AFTER_LINE], "the next line waits");
        assert_eq!(updates(&h, b, from).len(), 1, "the giving waits");
        h.run(14);
        assert_eq!(told(&h, b, from), [AFTER_LINE, LAST_LINE]);
        let mut want = vec![(0x3000_0000, QUEST, 0)];
        if !held {
            want.push((0x1000_0000, NEXT, 1));
            let order = sent(&h, b, from);
            let line = order.iter().rposition(|x| *x == (HEAR_TELL, LAST_LINE));
            let giving = order.iter().rposition(|x| x.0 == UPDATE_QUEST);
            assert!(line < giving, "the line, then the giving: {order:X?}");
        }
        assert_eq!(updates(&h, b, from), want, "held: {held}");
    }
}

/// A quest cancelled or failed while its step pauses is neither said on, completed nor paid, and
/// nothing is given for it, whether the pause runs out or the player leaves in it.
/// Behaviour: server.quests.a-pause-keeps-its-step-whole
#[test]
fn a_quest_cancelled_or_failed_in_a_pause_is_neither_completed_nor_paid() {
    for (cancels, leaves) in [(true, false), (false, false), (true, true)] {
        let mut h = world();
        h.server.quests.quests[0].xp = 500;
        for st in &mut h.server.quests.steps {
            st.key = format!("needs-{}", st.needs);
        }
        h.server
            .content
            .step_pauses
            .insert((QUEST, "needs-3".into()), 2000);
        h.server.talk.lines.push(Line {
            update: Some(TalkUpdate::Complete),
            ..line(PERSON_T, AFTER_LINE, None, Some(QUEST))
        });
        let (b, me) = player(&mut h, "one", "Elara");
        let from = sliver_given(&mut h, b, me);
        let w = world_of(&h, me);
        let clock = h.clock();
        let xp = |h: &Harness| yalain_world::quals::long(&h.server.players[&w].qualities, 300);
        let before = xp(&h);
        if cancels {
            let quest = [Value::Int(QUEST)];
            yalain_world::quests::cancel(&mut h.server, &mut h.net, w, &quest, clock);
        } else {
            yalain_world::quests::fail(&mut h.server, &mut h.net, w, QUEST, clock);
        }
        let stands = status_of(&h, me, QUEST);
        assert_ne!(
            stands,
            Some(yalain_world::quests::status::UNDERWAY),
            "it ended"
        );
        if leaves {
            yalain_world::world::leave(&mut h.server, &mut h.net, w);
            h.run(2);
            let kept = kept_quests(&mut h, me);
            let status = kept.iter().find(|q| q.0 == QUEST).map(|q| q.1);
            assert_eq!(status, stands, "left: still cancelled");
            continue;
        }
        h.run(steps(2.5));
        assert_eq!(status_of(&h, me, QUEST), stands, "not completed");
        assert!(told(&h, b, from).is_empty(), "nothing said");
        assert!(
            !updates(&h, b, from).iter().any(|u| u.0 == 0x3000_0000),
            "no completion"
        );
        assert_eq!(xp(&h), before, "not paid");
        let given = &h.server.players[&w].play.items;
        assert!(!given.iter().any(|x| x.template == IMBUED), "nothing given");
    }
}

/// One who leaves in a pause has what the step was for: the item it gives moves the quest it moves
/// when received, and a step that pauses as that one's quest is done is done too.
/// Behaviour: server.quests.a-pause-keeps-its-step-whole
#[test]
fn a_player_who_leaves_in_a_pause_receives_and_finishes_what_follows() {
    let mut h = finishing_world();
    h.server.quests.quests.push(quest(TOOK, 2));
    h.server
        .quests
        .steps
        .push(step(TOOK, 0, Trigger::Take(IMBUED), Update::Bestow));
    for st in &mut h.server.quests.steps {
        if st.needs == 3 {
            st.key = "sliver".into();
        }
    }
    h.server
        .content
        .step_pauses
        .insert((QUEST, "sliver".into()), 2000);
    let (b, me) = player(&mut h, "one", "Elara");
    sliver_given(&mut h, b, me);
    let w = world_of(&h, me);
    yalain_world::world::leave(&mut h.server, &mut h.net, w);
    h.run(2);
    let kept = kept_quests(&mut h, me);
    let underway = yalain_world::quests::status::UNDERWAY;
    assert!(kept.contains(&(TOOK, underway, 1)), "received: {kept:X?}");
    assert!(
        kept.contains(&(NEXT, underway, 1)),
        "the pause after it: {kept:X?}"
    );
}

/// A trigger's run of steps carries on once a paused one is through: a later step of the same
/// trigger that waits on the paused one's completion runs then.
/// Behaviour: server.quests.a-pause-keeps-its-step-whole
#[test]
fn a_run_of_steps_carries_on_after_a_pause() {
    let mut h = world();
    h.server.quests.quests.push(quest(NEXT, 2));
    h.server.quests.steps.push(Step {
        requires: vec![(QUEST, Need::Done)],
        ..step(NEXT, 0, Trigger::Give(SLIVER, PERSON_T), Update::Bestow)
    });
    for st in &mut h.server.quests.steps {
        if st.needs == 3 {
            st.key = "sliver".into();
        }
    }
    h.server
        .content
        .step_pauses
        .insert((QUEST, "sliver".into()), 1000);
    let (b, me) = player(&mut h, "one", "Elara");
    let from = sliver_given(&mut h, b, me);
    assert!(updates(&h, b, from).is_empty(), "paused");
    h.run(steps(1.0) + 4);
    assert_eq!(
        updates(&h, b, from),
        [(0x3000_0000, QUEST, 0), (0x1000_0000, NEXT, 1)]
    );
}

/// A step that pauses takes all it takes as it begins, so none of it can be dropped or traded in
/// the pause and the reward still had.
/// Behaviour: server.quests.a-pause-keeps-its-step-whole
#[test]
fn a_step_that_pauses_takes_all_it_takes_as_it_begins() {
    let mut h = pausing_world(2000, None);
    for st in &mut h.server.quests.steps {
        if st.needs == 2 {
            st.takes = vec![ASHES, JUNK];
        }
    }
    let (b, me) = player(&mut h, "one", "Elara");
    let (other, _) = hold(&mut h, me, JUNK, 1);
    let (_, from) = ashes_given(&mut h, b, me);
    assert!(updates(&h, b, from).is_empty(), "paused");
    assert!(!holds(&h, me, other), "taken as it began");
    h.run(steps(2.0) + 4);
    assert_eq!(updates(&h, b, from), [(0x2000_0003, QUEST, 3)]);
}

/// A line pauses no step that leads somewhere: its trip doesn't wait, so neither does its update.
/// Behaviour: server.quests.a-pause-keeps-its-step-whole
#[test]
fn a_step_that_leads_somewhere_is_held_by_no_line() {
    let mut h = pausing_world(2000, None);
    for st in &mut h.server.quests.steps {
        if st.needs == 2 {
            st.to = Some(yalain_content::quests::Place {
                cell: 0x0299_0109,
                origin: [62.0, 243.0, -44.6],
                heading: 0,
            });
        }
    }
    let (b, me) = player(&mut h, "one", "Elara");
    let (_, from) = ashes_given(&mut h, b, me);
    assert_eq!(updates(&h, b, from), [(0x2000_0003, QUEST, 3)], "not held");
}

/// A world reloaded in a pause, the person's place in the list moved by it: the line after the
/// pause is still that person's.
/// Behaviour: server.quests.a-pause-keeps-its-step-whole
#[test]
fn a_world_reloaded_in_a_pause_leaves_the_line_with_its_person() {
    let mut h = pausing_world(2000, None);
    let (b, me) = player(&mut h, "one", "Elara");
    let (_, from) = ashes_given(&mut h, b, me);
    let mut t = crate::operator::reload::tables_of(&h);
    let other = place(
        0x8130_0000_0000_0709,
        0x4700_3043,
        Kind::Npc,
        [70.0, 241.0, -44.6],
    );
    t.content.objects.insert(0, other);
    crate::operator::reload::reload(&mut h, t);
    h.run(steps(2.0) + 4);
    let speakers: Vec<Vec<Value>> = h
        .calls(b, from)
        .into_iter()
        .filter(|(f, _)| *f == HEAR_TELL)
        .map(|(_, v)| v)
        .collect();
    assert_eq!(speakers.len(), 1, "the line after");
    let by = |id: u64| format!("{:?}", speakers[0]).contains(&format!("Long({id})"));
    assert!(by(PERSON), "the person's: {:?}", speakers[0]);
}

/// A line that pauses sends the gift's going before it, by the order sent (2017: 22 of 22 whose
/// going was seen).
/// Behaviour: server.quests.a-line-pauses-its-step
#[test]
fn the_gift_of_a_step_that_pauses_goes_before_the_first_line() {
    let mut h = pausing_world(2000, None);
    let (b, me) = player(&mut h, "one", "Elara");
    let (_, from) = ashes_given(&mut h, b, me);
    let order = sent(&h, b, from);
    let gone = order.iter().position(|x| x.0 == gives::DESTROY_CONTAINED);
    let said = order.iter().position(|x| *x == (SHOW_TEXT, EMOTE_LINE));
    assert!(gone.is_some() && gone < said, "{order:X?}");
}
