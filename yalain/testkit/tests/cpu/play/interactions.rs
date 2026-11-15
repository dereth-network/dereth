//! Items used on objects their interaction table names, carried out at once: a hoard's key on its
//! stockpile opens the hoard to its user and the user's fellows and is used up, answered with the
//! table's own text; an item whose outcome names no text is answered that it was used on the object
//! and kept when its outcome keeps it; a player too far off, or one whose quest is not where the
//! item needs it, is refused and keeps the item.
//! Fixture: the test hall with a stockpile beside the start, the iron hoard key's table and entry
//! quest, and two players in one fellowship.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_content::quests::{QuestDef, Step, Trigger, Update};
use yalain_content::{ContentObject, ContentPack, Description, Facts, Kind, Prop, Props};
use yalain_net::ConnId;
use yalain_testkit::play::server_call;
use yalain_testkit::Harness;
use yalain_world::calls::{self, Node, Value};
use yalain_world::fellowship::{Fellow, Fellowship};
use yalain_world::quests::status;
use yalain_world::usable::interactions::{self, Interaction};

const USE: u32 = 0x8216_0016;
const USE_DONE: u32 = 0x8216_0021;
const UPDATE_QUEST: u32 = 0x8213_0024;

const STOCKPILE: u64 = 0x405A_5D00_0000_0001;
const STOCKPILE_TEMPLATE: u32 = 0x4700_12B5;
const KEY_TEMPLATE: u32 = 0x4700_12BA;
const BELL_TEMPLATE: u32 = 0x4700_1A3C;
const KEY_TABLE: u32 = 0x6F00_1204;
const BELL_TABLE: u32 = 0x6F00_127F;
const ENTER_QUEST: u32 = 0x8000_00EE;
const UNLOCKED: (u32, u32) = (0x7F6B_F22A, 0x2500_1446);
const CAN_BE_BESTOWED: u32 = 0x5000_0000;

fn use_of(item: u64, me: u64, on: u64) -> Arg {
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
            Slot::Long(on),
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

/// The hall with the iron hoard's stockpile a step from the start; the key's table (the stockpile
/// within 7 m, the key used up, the table's text), a bell's (the stockpile within 8 m, kept, no
/// text); the entry quest (ten minutes) given by the key on the stockpile to the fellowship.
fn hall() -> Harness {
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    h.server.content = ContentPack {
        objects: vec![ContentObject {
            object: Some(STOCKPILE),
            template: STOCKPILE_TEMPLATE,
            description: Description {
                name_text: None,
                package: 0x1A01,
                look: 0x1F00_0ED5,
                enter_fx: 0,
                scale: None,
                name: Some((0x3F7C_29E0, 0x2500_1446)),
                numbers: [0; 6],
                source: "authored for a test".into(),
            },
            cell: 0x0299_0109,
            origin: [60.0, 239.0, -44.6],
            rotation: [1.0, 0.0, 0.0, 0.0],
            kind: Kind::Static,
            label: String::new(),
            placed: "authored for a test".into(),
            destination: None,
            facts: Facts::default(),
            walks: None,
            gate: None,
        }],
        ..ContentPack::default()
    };
    let mut key = Props::default();
    key.set(interactions::TABLE, Prop::Word(KEY_TABLE))
        .set("Usage_RequiredQuest", Prop::Word(ENTER_QUEST))
        .set("Usage_RequiredQuestStatus", Prop::Word(CAN_BE_BESTOWED));
    let mut bell = Props::default();
    bell.set(interactions::TABLE, Prop::Word(BELL_TABLE));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(KEY_TEMPLATE, 0x18F, key));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(BELL_TEMPLATE, 0x18F, bell));
    h.server.interactions.tables.insert(
        KEY_TABLE,
        vec![Interaction {
            targets: vec![STOCKPILE_TEMPLATE],
            reach: 7.0,
            behavior: None,
            message: Some(UNLOCKED),
            uses_up: true,
            uses_up_target: false,
            gate: None,
        }],
    );
    h.server.interactions.tables.insert(
        BELL_TABLE,
        vec![Interaction {
            targets: vec![STOCKPILE_TEMPLATE],
            reach: 8.0,
            behavior: None,
            message: None,
            uses_up: false,
            uses_up_target: false,
            gate: None,
        }],
    );
    h.server.quests.quests.push(QuestDef {
        duration_by_calendar: false,
        id: ENTER_QUEST,
        name: yalain_content::Words::Ref(0x6EE1_B009, 0x2500_1446),
        description: (0x0121_67D6, 0x2500_1446),
        icon: 0x4100_034B,
        play_fx: false,
        journals: vec![(0x7E76_0E2A, 0x2500_1446)],
        challenge: -1,
        duration: 600.0,
        timed_phase: 0,
        retry: 590_400.0,
        xp: 0,
        cancellable: false,
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
    });
    h.server.quests.steps.push(Step {
        out_of_turn: false,
        key: String::new(),
        quest: ENTER_QUEST,
        needs: 0,
        trigger: Trigger::UseOn(KEY_TEMPLATE, STOCKPILE_TEMPLATE),
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
        to: None,
        races: Vec::new(),
        effects_last: false,
        effects_quiet: false,
        wants: None,
        fellows: true,
        levels: None,
        xp_share: 0.0,
        transforms: None,
        chance: None,
        to_fellows: false,
    });
    h
}

fn world_of(h: &Harness, me: u64) -> ConnId {
    *h.server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in the world")
        .0
}

/// A player in the hall holding an item of a template: its client, id, connection and item.
fn holding(h: &mut Harness, name: &str, template: u32) -> (usize, u64, ConnId, u64) {
    let (b, me) = h.player_in_world(name, name);
    h.run(10);
    let world = world_of(h, me);
    yalain_world::inventory::give(&mut h.server, &mut h.net, world, template, 1);
    h.run(3);
    let item = held(h, world, template).expect("given");
    (b, me, world, item)
}

fn held(h: &Harness, world: ConnId, template: u32) -> Option<u64> {
    h.server.players[&world]
        .play
        .items
        .iter()
        .find(|i| i.template == template)
        .map(|i| i.id)
}

/// The use answers a client was sent since `from`: (status, control flags, its text's string).
fn answers(h: &Harness, b: usize, from: usize) -> Vec<(u32, u32, u32)> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) if c.function.0 == USE_DONE => {
                let v = c.args.as_ref().and_then(|a| calls::values(&a.0).ok())?;
                let Some(Value::Graph(g)) = v.first() else {
                    return None;
                };
                let (status, flags) = g.iter().find_map(|(_, n)| match n {
                    Node::Script { slots, .. } => Some((slots[11].1, slots[17].1)),
                    _ => None,
                })?;
                let text = g
                    .iter()
                    .find_map(|(_, n)| match n {
                        Node::TextInfo(t) => Some(t.string_id),
                        _ => None,
                    })
                    .unwrap_or(0);
                Some((status, flags, text))
            }
            _ => None,
        })
        .collect()
}

fn updates(h: &Harness, b: usize, from: usize) -> usize {
    h.world_messages(b, from)
        .iter()
        .filter(|m| matches!(m, ServerMessage::PlayerCall(c) if c.function.0 == UPDATE_QUEST))
        .count()
}

fn under_way(h: &Harness, world: ConnId) -> bool {
    h.server.players[&world]
        .play
        .quests
        .iter()
        .any(|q| q.quest == ENTER_QUEST && q.status == status::UNDERWAY)
}

fn send_use(h: &mut Harness, b: usize, me: u64, item: u64, on: u64) {
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(USE, &[use_of(item, me, on)]),
    );
    h.run(3);
}

/// Put a player `dx` meters east of the stockpile.
fn east_of_stockpile(h: &mut Harness, world: ConnId, dx: f32) {
    let p = h.server.players.get_mut(&world).expect("in the world");
    p.position.cell = 0x0299_0109;
    p.position.origin = [60.0 + dx, 239.0, -44.6];
}

fn fellowship(h: &mut Harness, ids: &[u64]) {
    h.server.fellowships.list.push(Fellowship {
        name: "Miners".into(),
        leader: ids[0],
        flags: 0,
        room: 1,
        fellows: ids
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
}

/// A hoard's key used on its stockpile: answered at once with the table's text and the flags of an
/// item used up, the key gone, and the hoard's entry quest given to its user and to each fellow
/// whose quest may be given (told the update alone), not to one holding it already.
/// Behaviour: server.places.hoard-key-opens-the-hoard-to-the-fellowship
#[test]
fn a_hoard_key_on_its_stockpile_gives_the_entry_quest_to_its_user_and_fellows_and_is_used_up() {
    let mut h = hall();
    let (a, me, world, key) = holding(&mut h, "one", KEY_TEMPLATE);
    let (b, you) = h.player_in_world("two", "two");
    let (c, them) = h.player_in_world("three", "three");
    h.run(10);
    let world_b = world_of(&h, you);
    // The third fellow's entry quest is under way already: it is not given again.
    let world_c = world_of(&h, them);
    h.server
        .players
        .get_mut(&world_c)
        .expect("in")
        .play
        .quests
        .push(yalain_world::state::QuestState {
            quest: ENTER_QUEST,
            status: status::UNDERWAY,
            phase: 1,
            started: 0,
            updated: 0,
        });
    fellowship(&mut h, &[me, you, them]);
    east_of_stockpile(&mut h, world, 3.0);
    let (fa, fb, fc) = (
        h.bots[a].inbox.len(),
        h.bots[b].inbox.len(),
        h.bots[c].inbox.len(),
    );
    send_use(&mut h, a, me, key, STOCKPILE);
    assert_eq!(
        answers(&h, a, fa),
        [(0, interactions::FLAGS_USED_UP, UNLOCKED.0)],
        "answered with the table's text"
    );
    assert_eq!(held(&h, world, KEY_TEMPLATE), None, "the key used up");
    assert!(under_way(&h, world), "the user's entry quest given");
    assert!(under_way(&h, world_b), "the fellow's entry quest given");
    assert_eq!(updates(&h, b, fb), 1, "the fellow told the update");
    assert!(answers(&h, b, fb).is_empty(), "and nothing else of the use");
    assert_eq!(
        updates(&h, c, fc),
        0,
        "a fellow holding it already is not given it again"
    );
}

/// An item whose outcome names no text and keeps the item: answered "You successfully use ... on
/// ...", the flags of an item kept, the item still held.
/// Behaviour: server.places.item-used-on-an-object-at-once
#[test]
fn an_item_used_on_an_object_its_table_names_is_answered_at_once_and_kept_when_its_outcome_keeps_it(
) {
    let mut h = hall();
    let (a, me, world, bell) = holding(&mut h, "one", BELL_TEMPLATE);
    east_of_stockpile(&mut h, world, 7.5);
    let fa = h.bots[a].inbox.len();
    send_use(&mut h, a, me, bell, STOCKPILE);
    assert_eq!(
        answers(&h, a, fa),
        [(0, interactions::FLAGS_KEPT, interactions::text::USED_ON.0)]
    );
    assert_eq!(held(&h, world, BELL_TEMPLATE), Some(bell), "kept");
}

/// An item with no table for the object, whose quest step on it waits for another phase of the
/// player's quest (a glyph used on its portal before the step that takes it there): answered at
/// once as done, the flags of an item kept, nothing moved, and the item still held, so it is free
/// to use again.
/// Behaviour: server.places.item-used-on-an-object-moving-nothing-is-answered
#[test]
fn an_item_used_on_an_object_while_its_step_waits_for_another_phase_is_answered_and_kept() {
    const GLYPH_TEMPLATE: u32 = 0x4700_1AED;
    const HALL_QUEST: u32 = 0x4000_0088;
    let mut h = hall();
    h.server.catalog.insert(yalain_testkit::test_template(
        GLYPH_TEMPLATE,
        0x18F,
        Props::default(),
    ));
    h.server.quests.steps.push(Step {
        out_of_turn: false,
        key: String::new(),
        quest: HALL_QUEST,
        needs: 6,
        trigger: Trigger::UseOn(GLYPH_TEMPLATE, STOCKPILE_TEMPLATE),
        tell: None,
        update: Update::Advance(7),
        xp: 0,
        gives: Vec::new(),
        counts: Vec::new(),
        effects: Vec::new(),
        text: None,
        recipes: Vec::new(),
        placed: "authored for a test".into(),
        requires: Vec::new(),
        takes: vec![GLYPH_TEMPLATE],
        to: Some(yalain_content::quests::Place {
            cell: 0x0299_0109,
            origin: [10.0, 10.0, -44.6],
            heading: 0,
        }),
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
    });
    let (a, me, world, glyph) = holding(&mut h, "one", GLYPH_TEMPLATE);
    h.server
        .players
        .get_mut(&world)
        .expect("in")
        .play
        .quests
        .push(yalain_world::state::QuestState {
            quest: HALL_QUEST,
            status: status::UNDERWAY,
            phase: 5,
            started: 0,
            updated: 0,
        });
    east_of_stockpile(&mut h, world, 2.0);
    let at = h.server.players[&world].position;
    let fa = h.bots[a].inbox.len();
    send_use(&mut h, a, me, glyph, STOCKPILE);
    assert_eq!(answers(&h, a, fa), [(0, interactions::FLAGS_KEPT, 0)]);
    assert_eq!(updates(&h, a, fa), 0, "nothing moved");
    assert_eq!(held(&h, world, GLYPH_TEMPLATE), Some(glyph), "kept");
    assert_eq!(h.server.players[&world].position, at, "not taken anywhere");
}

/// Too far from the object for the table, or the item's quest not where it needs it (the hoard's
/// entry quest already under way): refused at once, the key kept and no quest given.
/// Behaviour: server.places.item-used-on-an-object-refused
#[test]
fn a_key_too_far_from_its_stockpile_or_whose_quest_is_under_way_is_refused_and_kept() {
    let mut h = hall();
    let (a, me, world, key) = holding(&mut h, "one", KEY_TEMPLATE);
    east_of_stockpile(&mut h, world, 7.5);
    let fa = h.bots[a].inbox.len();
    send_use(&mut h, a, me, key, STOCKPILE);
    assert_eq!(
        answers(&h, a, fa),
        [(interactions::TOO_FAR, interactions::FLAGS_KEPT, 0)],
        "too far"
    );
    assert_eq!(held(&h, world, KEY_TEMPLATE), Some(key), "kept");
    assert!(!under_way(&h, world), "no quest given");
    // Under way already: the key's quest is not able to be given.
    h.server
        .players
        .get_mut(&world)
        .expect("in")
        .play
        .quests
        .push(yalain_world::state::QuestState {
            quest: ENTER_QUEST,
            status: status::UNDERWAY,
            phase: 1,
            started: 0,
            updated: 0,
        });
    east_of_stockpile(&mut h, world, 3.0);
    let fa = h.bots[a].inbox.len();
    send_use(&mut h, a, me, key, STOCKPILE);
    assert_eq!(
        answers(&h, a, fa),
        [(
            yalain_world::item_use::refusal::WRONG_QUEST_STATUS,
            interactions::FLAGS_KEPT,
            0
        )],
        "the quest under way"
    );
    assert_eq!(held(&h, world, KEY_TEMPLATE), Some(key), "kept");
}

const CAN_TEMPLATE: u32 = 0x4700_145F;
const POLLEN_TEMPLATE: u32 = 0x4700_1E5C;
const REQUIESCAT_TEMPLATE: u32 = 0x4700_1C8D;
const WEAPON_TEMPLATE: u32 = 0x4700_0845;
const CAN_TABLE: u32 = 0x6F00_124F;
const POLLEN_TABLE: u32 = 0x6F00_124E;
const REQUIESCAT_TABLE: u32 = 0x6F00_1289;

/// The hall, its stockpile standing for a dye plant (its use behavior the plants'): a watering can
/// (durability 100) and a pollen that may be used on it from 4 m, the can kept and the pollen used
/// up; the Requiescat (its own use behavior) that may be used on a weapon, used up.
fn garden() -> Harness {
    let mut h = hall();
    let mut plant = Props::default();
    plant.set(
        interactions::USER_BEHAVIOR,
        Prop::Word(interactions::PLANT_BEHAVIOR),
    );
    h.server.catalog.insert(yalain_testkit::test_template(
        STOCKPILE_TEMPLATE,
        0x283D,
        plant,
    ));
    let mut can = Props::default();
    can.set(interactions::TABLE, Prop::Word(CAN_TABLE))
        .set("Item_Durability_MaxLevel", Prop::Int(100));
    let mut pollen = Props::default();
    pollen.set(interactions::TABLE, Prop::Word(POLLEN_TABLE));
    let mut requiescat = Props::default();
    requiescat
        .set(interactions::TABLE, Prop::Word(REQUIESCAT_TABLE))
        .set(
            interactions::USER_BEHAVIOR,
            Prop::Word(interactions::REQUIESCAT_BEHAVIOR),
        );
    for (t, p) in [
        (CAN_TEMPLATE, can),
        (POLLEN_TEMPLATE, pollen),
        (REQUIESCAT_TEMPLATE, requiescat),
        (WEAPON_TEMPLATE, Props::default()),
    ] {
        h.server
            .catalog
            .insert(yalain_testkit::test_template(t, 0x1B34, p));
    }
    for (table, targets, uses_up) in [
        (CAN_TABLE, vec![STOCKPILE_TEMPLATE], false),
        (POLLEN_TABLE, vec![STOCKPILE_TEMPLATE], true),
        (REQUIESCAT_TABLE, vec![WEAPON_TEMPLATE], true),
    ] {
        h.server.interactions.tables.insert(
            table,
            vec![Interaction {
                targets,
                reach: if table == REQUIESCAT_TABLE { 3.0 } else { 4.0 },
                behavior: None,
                message: None,
                uses_up,
                uses_up_target: false,
                gate: None,
            }],
        );
    }
    h
}

/// The behaviors a player was seen to play since `from`: (behavior, bits, its words of flags 0x20
/// and 0x100).
fn played(h: &Harness, b: usize, from: usize, me: u64) -> Vec<(u32, u32, u32, u32)> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayBehavior(e) if e.sender.id == me => Some((
                e.action.behavior,
                e.action.bits,
                e.action.words[1],
                e.action.words[6],
            )),
            _ => None,
        })
        .collect()
}

fn run_for(h: &mut Harness, seconds: f64) {
    let t = h.now.0 + seconds;
    while h.now.0 < t {
        h.step();
    }
}

/// A watering can used on a dye plant plays the plant's use behavior at once, with no walk; the
/// use is answered only when it has played out (1.88 s), kept (316) with "You successfully use
/// ... on ...", the can's durability one less; another use while it plays is refused as one made
/// while the player's actions are locked.
/// Behaviour: server.places.item-used-on-an-object-plays-a-behavior
#[test]
fn a_watering_can_on_a_dye_plant_plays_the_plants_behavior_and_is_answered_when_it_has_played() {
    let mut h = garden();
    let (a, me, world, can) = holding(&mut h, "one", CAN_TEMPLATE);
    east_of_stockpile(&mut h, world, 3.0);
    let fa = h.bots[a].inbox.len();
    send_use(&mut h, a, me, can, STOCKPILE);
    assert_eq!(
        played(&h, a, fa, me),
        [(interactions::PLANT_BEHAVIOR, 0x40_0131, 1, 1)],
        "the plant's behavior, played at once"
    );
    assert!(answers(&h, a, fa).is_empty(), "not answered yet");
    run_for(&mut h, 0.5);
    let fb = h.bots[a].inbox.len();
    send_use(&mut h, a, me, can, STOCKPILE);
    assert_eq!(
        answers(&h, a, fb).first().map(|x| x.0),
        Some(yalain_world::places::ACTIONS_LOCKED),
        "busy"
    );
    assert!(played(&h, a, fb, me).is_empty(), "no second behavior");
    run_for(&mut h, 0.7);
    assert!(
        answers(&h, a, fa).iter().all(|x| x.0 != 0),
        "still playing 1.6 s in"
    );
    run_for(&mut h, 0.5);
    let got: Vec<_> = answers(&h, a, fa)
        .into_iter()
        .filter(|x| x.0 == 0)
        .collect();
    assert_eq!(
        got,
        [(0, interactions::FLAGS_KEPT, interactions::text::USED_ON.0)],
        "answered once it played out"
    );
    assert_eq!(held(&h, world, CAN_TEMPLATE), Some(can), "kept");
    assert_eq!(
        h.server.things.get(can).and_then(|t| t.durability),
        Some(99),
        "worn by one"
    );
}

/// A pollen on a dye plant plays the plant's behavior and is used up when it has played out
/// (319); the Requiescat on a weapon its user wields plays its own behavior, where its user
/// stands, and is used up when that has played out (2.42 s).
/// Behaviour: server.places.item-used-on-an-object-plays-a-behavior
#[test]
fn a_pollen_on_a_plant_and_the_requiescat_on_a_held_weapon_play_and_are_used_up() {
    let mut h = garden();
    let (a, me, world, pollen) = holding(&mut h, "one", POLLEN_TEMPLATE);
    east_of_stockpile(&mut h, world, 3.5);
    let fa = h.bots[a].inbox.len();
    send_use(&mut h, a, me, pollen, STOCKPILE);
    assert_eq!(played(&h, a, fa, me).len(), 1);
    assert_eq!(
        held(&h, world, POLLEN_TEMPLATE),
        Some(pollen),
        "held while it plays"
    );
    run_for(&mut h, 2.0);
    assert_eq!(
        answers(&h, a, fa),
        [(
            0,
            interactions::FLAGS_USED_UP,
            interactions::text::USED_ON.0
        )]
    );
    assert_eq!(held(&h, world, POLLEN_TEMPLATE), None, "used up");
    yalain_world::inventory::give(&mut h.server, &mut h.net, world, REQUIESCAT_TEMPLATE, 1);
    yalain_world::inventory::give(&mut h.server, &mut h.net, world, WEAPON_TEMPLATE, 1);
    h.run(3);
    let requiescat = held(&h, world, REQUIESCAT_TEMPLATE).expect("given");
    let weapon = held(&h, world, WEAPON_TEMPLATE).expect("given");
    let fb = h.bots[a].inbox.len();
    send_use(&mut h, a, me, requiescat, weapon);
    assert_eq!(
        played(&h, a, fb, me).first().map(|x| x.0),
        Some(interactions::REQUIESCAT_BEHAVIOR),
        "its own behavior"
    );
    run_for(&mut h, 2.0);
    assert!(answers(&h, a, fb).is_empty(), "still playing 2 s in");
    run_for(&mut h, 0.5);
    assert_eq!(
        answers(&h, a, fb),
        [(
            0,
            interactions::FLAGS_USED_UP,
            interactions::text::USED_ON.0
        )]
    );
    assert_eq!(held(&h, world, REQUIESCAT_TEMPLATE), None, "used up");
    assert_eq!(
        held(&h, world, WEAPON_TEMPLATE),
        Some(weapon),
        "the weapon kept"
    );
}

/// An item put down while its use on an object plays: nothing comes of it, and it is not used up.
/// Behaviour: server.interactions.an-item-gone-while-it-plays-does-nothing
#[test]
fn an_item_put_down_while_its_use_plays_does_nothing() {
    use yalain_world::drops;
    use yalain_world::inventory::{move_graph, Move};
    let mut h = garden();
    let (a, me, world, pollen) = holding(&mut h, "one", POLLEN_TEMPLATE);
    east_of_stockpile(&mut h, world, 3.5);
    let fa = h.bots[a].inbox.len();
    send_use(&mut h, a, me, pollen, STOCKPILE);
    assert_eq!(played(&h, a, fa, me).len(), 1, "it plays");
    let m = Move {
        move_type: drops::DROP,
        item: pollen,
        quantity: 1,
        ..Move::default()
    };
    h.ask(a, drops::function::MOVE_ITEM, &[move_graph(&m)], 2);
    run_for(&mut h, 3.0);
    assert!(answers(&h, a, fa).is_empty(), "no answer of it done");
    assert!(h.server.things.get(pollen).is_some(), "still in the world");
}

const BLANK_TEMPLATE: u32 = 0x4700_15BC;
const BLANK_TABLE: u32 = 0x6F00_125E;
const SWORD_TEMPLATE: u32 = 0x4700_0846;
const DAGGER_TEMPLATE: u32 = 0x4700_0847;
const BOW_TEMPLATE: u32 = 0x4700_0848;
const SWORD_EMBLEM: u32 = 0x4700_15D1;
const DAGGER_EMBLEM: u32 = 0x4700_15B8;
const SWORDS: u32 = 4441;
const BOWS: u32 = 4205;
const IMPRINTED: (u32, u32) = (0x0E50_4022, 0x2500_1FE8);

/// The hall with a blank emblem whose table names the swords' class (two weapons count in it) and
/// the bows' (one), each used up with its own text, held 3 m from its target; rows make the swords'
/// emblem of the swords' class, and another emblem of the dagger itself.
fn forge() -> Harness {
    use yalain_content::item_rows::{Interaction as Made, On};
    let mut h = hall();
    let mut blank = Props::default();
    blank.set(interactions::TABLE, Prop::Word(BLANK_TABLE));
    for (t, p) in [
        (BLANK_TEMPLATE, blank),
        (SWORD_TEMPLATE, Props::default()),
        (DAGGER_TEMPLATE, Props::default()),
        (BOW_TEMPLATE, Props::default()),
        (SWORD_EMBLEM, Props::default()),
        (DAGGER_EMBLEM, Props::default()),
    ] {
        h.server
            .catalog
            .insert(yalain_testkit::test_template(t, 0x1B34, p));
    }
    let entry = |targets: Vec<u32>| Interaction {
        targets,
        reach: 3.0,
        behavior: None,
        message: Some(IMPRINTED),
        uses_up: true,
        uses_up_target: false,
        gate: None,
    };
    h.server.interactions.tables.insert(
        BLANK_TABLE,
        vec![
            entry(vec![SWORD_TEMPLATE, DAGGER_TEMPLATE]),
            entry(vec![BOW_TEMPLATE]),
        ],
    );
    h.server
        .interactions
        .members
        .insert(SWORDS, vec![SWORD_TEMPLATE, DAGGER_TEMPLATE]);
    h.server
        .interactions
        .members
        .insert(BOWS, vec![BOW_TEMPLATE]);
    h.server.item_rows.interactions = vec![
        Made {
            item: BLANK_TEMPLATE,
            on: On::Class(SWORDS),
            gives: vec![(SWORD_EMBLEM, 1)],
            count: 1,
            gated_gives: Vec::new(),
            places: Vec::new(),
            effects_on: None,
            effects_here: Vec::new(),
            tell: None,
        },
        Made {
            item: BLANK_TEMPLATE,
            on: On::Template(DAGGER_TEMPLATE),
            gives: vec![(DAGGER_EMBLEM, 1)],
            count: 1,
            gated_gives: Vec::new(),
            places: Vec::new(),
            effects_on: None,
            effects_here: Vec::new(),
            tell: None,
        },
    ];
    h
}

/// An emblem used on a sword in its user's pack is imprinted from it: the world's row for the
/// sword's class gives its user the swords' emblem, the blank is used up, the sword kept, and the
/// use is answered with the table's own text; on the dagger, of the same class, the row naming the
/// dagger itself comes first; on a bow, whose class no row names, the blank is used up as its table
/// says and nothing is made.
/// Behaviour: server.interactions.an-item-used-on-another-makes-what-its-row-gives
#[test]
fn an_emblem_used_on_a_held_weapon_makes_what_the_row_for_the_weapon_or_its_class_gives() {
    let mut h = forge();
    let (a, me, world, blank) = holding(&mut h, "one", BLANK_TEMPLATE);
    for t in [SWORD_TEMPLATE, DAGGER_TEMPLATE, BOW_TEMPLATE] {
        yalain_world::inventory::give(&mut h.server, &mut h.net, world, t, 1);
    }
    h.run(3);
    let sword = held(&h, world, SWORD_TEMPLATE).expect("given");
    let from = h.bots[a].inbox.len();
    send_use(&mut h, a, me, blank, sword);
    assert_eq!(
        answers(&h, a, from),
        [(0, interactions::FLAGS_USED_UP, IMPRINTED.0)]
    );
    assert!(
        held(&h, world, SWORD_EMBLEM).is_some(),
        "the swords' emblem"
    );
    assert_eq!(held(&h, world, BLANK_TEMPLATE), None, "the blank used up");
    assert_eq!(
        held(&h, world, SWORD_TEMPLATE),
        Some(sword),
        "the sword kept"
    );

    yalain_world::inventory::give(&mut h.server, &mut h.net, world, BLANK_TEMPLATE, 1);
    h.run(3);
    let blank = held(&h, world, BLANK_TEMPLATE).expect("given");
    let dagger = held(&h, world, DAGGER_TEMPLATE).expect("given");
    send_use(&mut h, a, me, blank, dagger);
    assert!(
        held(&h, world, DAGGER_EMBLEM).is_some(),
        "the dagger's own row"
    );
    let swords = h.server.players[&world]
        .play
        .items
        .iter()
        .filter(|i| i.template == SWORD_EMBLEM)
        .count();
    assert_eq!(swords, 1, "not its class's as well");

    yalain_world::inventory::give(&mut h.server, &mut h.net, world, BLANK_TEMPLATE, 1);
    h.run(3);
    let blank = held(&h, world, BLANK_TEMPLATE).expect("given");
    let bow = held(&h, world, BOW_TEMPLATE).expect("given");
    let before = h.server.players[&world].play.items.len();
    let from = h.bots[a].inbox.len();
    send_use(&mut h, a, me, blank, bow);
    assert_eq!(
        answers(&h, a, from),
        [(0, interactions::FLAGS_USED_UP, IMPRINTED.0)]
    );
    assert_eq!(
        h.server.players[&world].play.items.len(),
        before - 1,
        "the blank gone, nothing made"
    );
}

const SIGNIFICATOR_TEMPLATE: u32 = 0x4700_13D7;
const GATEWAY_TEMPLATE: u32 = 0x4700_13A9;
const SIGNIFICATOR_TABLE: u32 = 0x6F00_123E;
/// "Your significator passes through the gateway!"
const PASSES: (u32, u32) = (0xBA0F_3786, 0x2500_1F24);

/// An item used on another the user holds, where the table's outcome destroys its target (a
/// significator on a gateway), keeps the item and takes the target, one off its stack, and is
/// answered kept with the table's own text; where the outcome keeps its target, it stays.
/// Behaviour: server.interactions.an-outcome-that-destroys-its-target-takes-a-held-one
#[test]
fn a_significator_passed_through_a_held_gateway_is_kept_and_the_gateway_is_taken() {
    for destroys in [true, false] {
        let mut h = hall();
        let mut significator = Props::default();
        significator.set(interactions::TABLE, Prop::Word(SIGNIFICATOR_TABLE));
        for (t, p) in [
            (SIGNIFICATOR_TEMPLATE, significator),
            (GATEWAY_TEMPLATE, Props::default()),
        ] {
            h.server
                .catalog
                .insert(yalain_testkit::test_template(t, 0x1B34, p));
        }
        h.server.interactions.tables.insert(
            SIGNIFICATOR_TABLE,
            vec![Interaction {
                targets: vec![GATEWAY_TEMPLATE],
                reach: 3.0,
                behavior: None,
                message: Some(PASSES),
                uses_up: false,
                uses_up_target: destroys,
                gate: None,
            }],
        );
        let (a, me, world, significator) = holding(&mut h, "one", SIGNIFICATOR_TEMPLATE);
        yalain_world::inventory::give(&mut h.server, &mut h.net, world, GATEWAY_TEMPLATE, 2);
        h.run(3);
        let gateway = held(&h, world, GATEWAY_TEMPLATE).expect("given");
        let from = h.bots[a].inbox.len();
        send_use(&mut h, a, me, significator, gateway);
        assert_eq!(
            answers(&h, a, from),
            [(0, interactions::FLAGS_KEPT, PASSES.0)],
            "destroys {destroys}"
        );
        assert_eq!(
            held(&h, world, SIGNIFICATOR_TEMPLATE),
            Some(significator),
            "the significator kept"
        );
        let gateways: u32 = h.server.players[&world]
            .play
            .items
            .iter()
            .filter(|i| i.template == GATEWAY_TEMPLATE)
            .map(|i| i.quantity.max(1))
            .sum();
        assert_eq!(
            gateways,
            if destroys { 1 } else { 2 },
            "destroys {destroys}"
        );
    }
}

const MELD_TEMPLATE: u32 = 0x4700_13B2;
const MELD_TABLE: u32 = 0x6F00_1230;
/// The fourteen drums, Ace to King.
const ACE_OF_DRUMS: u32 = 0x4700_134B;
const DRUMS: u32 = 10259;
const LESSER_FATE_CARD: u32 = 0x4700_13A6;
const FATE_CARD: u32 = 0x4700_13A4;
/// "You transform the lesser trump into an optimal fate card!" and "... into a fate card!"
const OPTIMAL: (u32, u32) = (0x3EBB_8F3B, 0x2500_1F24);
const PLAIN: (u32, u32) = (0xAF1E_F93B, 0x2500_1F24);

/// The hall with a card meld whose table names each of the fourteen drums for the characters whose
/// number leaves its rank divided by 14, with the optimal text, then the drums' class for anyone,
/// with the plain one, each keeping the meld and taking the drum; the world's row for the class
/// gives the lesser fate card, and the fate card where the interaction met was for some characters
/// only.
fn parlor() -> Harness {
    use yalain_content::item_rows::{Interaction as Made, On};
    let mut h = hall();
    let mut meld = Props::default();
    meld.set(interactions::TABLE, Prop::Word(MELD_TABLE));
    let drums: Vec<u32> = (0..14).map(|r| ACE_OF_DRUMS + r).collect();
    h.server
        .catalog
        .insert(yalain_testkit::test_template(MELD_TEMPLATE, 0x1B34, meld));
    for &t in drums.iter().chain(&[LESSER_FATE_CARD, FATE_CARD]) {
        h.server
            .catalog
            .insert(yalain_testkit::test_template(t, 0x1B34, Props::default()));
    }
    let entry = |targets: Vec<u32>, message, gate| Interaction {
        targets,
        reach: 3.0,
        behavior: None,
        message: Some(message),
        uses_up: false,
        uses_up_target: true,
        gate,
    };
    let mut table: Vec<Interaction> = (0..14)
        .map(|r| {
            entry(
                vec![drums[r]],
                OPTIMAL,
                interactions::Gate::of(14, i32::try_from(r).expect("a rank")),
            )
        })
        .collect();
    table.push(entry(drums.clone(), PLAIN, None));
    h.server.interactions.tables.insert(MELD_TABLE, table);
    h.server.interactions.members.insert(DRUMS, drums);
    h.server.item_rows.interactions = vec![Made {
        item: MELD_TEMPLATE,
        on: On::Class(DRUMS),
        gives: vec![(LESSER_FATE_CARD, 1)],
        count: 1,
        gated_gives: vec![(FATE_CARD, 1)],
        places: Vec::new(),
        effects_on: None,
        effects_here: Vec::new(),
        tell: None,
    }];
    h
}

fn count_of(h: &Harness, world: ConnId, template: u32) -> u32 {
    h.server.players[&world]
        .play
        .items
        .iter()
        .filter(|i| i.template == template)
        .map(|i| i.quantity.max(1))
        .sum()
}

/// A card meld used on each of the fourteen drums, twice over, by three characters: each character
/// meets the interaction for its own rank alone (its number divided by 14 leaves the rank), and
/// the same one both times; there it is answered with the optimal text and made the fate card, on
/// every other drum the plain text and the lesser fate card; the meld is kept and each drum taken.
/// The three characters, numbered one after another, have three different ranks.
/// Behaviour: server.interactions.an-interaction-for-some-characters-only
#[test]
fn a_card_meld_on_a_drum_makes_the_fate_card_only_for_the_characters_whose_rank_it_is() {
    let mut h = parlor();
    let mut ranks = Vec::new();
    for name in ["one", "two", "three"] {
        let (a, me, world, meld) = holding(&mut h, name, MELD_TEMPLATE);
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let own = (me as u32 as i32 % 14).unsigned_abs();
        for round in 0..2 {
            let mut optimal = Vec::new();
            for r in 0..14 {
                yalain_world::inventory::give(
                    &mut h.server,
                    &mut h.net,
                    world,
                    ACE_OF_DRUMS + r,
                    1,
                );
                h.run(3);
                let drum = held(&h, world, ACE_OF_DRUMS + r).expect("given");
                let from = h.bots[a].inbox.len();
                send_use(&mut h, a, me, meld, drum);
                let said = answers(&h, a, from);
                assert_eq!(said.len(), 1, "{name} rank {r}: {said:?}");
                let (status, flags, text) = said[0];
                assert_eq!((status, flags), (0, interactions::FLAGS_KEPT));
                if text == OPTIMAL.0 {
                    optimal.push(r);
                } else {
                    assert_eq!(text, PLAIN.0, "{name} rank {r}");
                }
                assert_eq!(held(&h, world, ACE_OF_DRUMS + r), None, "the drum taken");
            }
            assert_eq!(optimal, [own], "{name}, round {round}");
            assert_eq!(count_of(&h, world, FATE_CARD), round + 1, "{name}");
            assert_eq!(
                count_of(&h, world, LESSER_FATE_CARD),
                13 * (round + 1),
                "{name}"
            );
        }
        assert_eq!(held(&h, world, MELD_TEMPLATE), Some(meld), "the meld kept");
        ranks.push(own);
    }
    ranks.sort_unstable();
    ranks.dedup();
    assert_eq!(ranks.len(), 3, "three ranks");
}

/// A row naming several things a use makes makes one of them each use, picked by their weights,
/// and never anything else: a card meld whose row makes either fortune's lesser card, three to
/// one, on drums of ranks other than its user's makes both over many uses, the likelier about
/// three times in four; where the interaction met is the user's own rank it picks among the row's gated ones.
/// Behaviour: server.interactions.a-use-makes-one-of-several
#[test]
fn a_meld_whose_row_names_two_cards_makes_one_of_them_each_use_by_their_weights() {
    const BONA: u32 = 0x4700_1385;
    const MALA: u32 = 0x4700_1395;
    const BONA_CARD: u32 = 0x4700_1383;
    const MALA_CARD: u32 = 0x4700_1393;
    let mut h = parlor();
    for t in [BONA, MALA, BONA_CARD, MALA_CARD] {
        h.server
            .catalog
            .insert(yalain_testkit::test_template(t, 0x1B34, Props::default()));
    }
    let row = &mut h.server.item_rows.interactions[0];
    row.gives = vec![(BONA, 3), (MALA, 1)];
    row.gated_gives = vec![(BONA_CARD, 1), (MALA_CARD, 1)];
    let (a, me, world, meld) = holding(&mut h, "one", MELD_TEMPLATE);
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    let own = (me as u32 as i32 % 14).unsigned_abs();
    let other = (own + 1) % 14;
    let uses = |h: &mut Harness, rank: u32, n: u32| {
        for _ in 0..n {
            yalain_world::inventory::give(&mut h.server, &mut h.net, world, ACE_OF_DRUMS + rank, 1);
            h.run(3);
            let drum = held(h, world, ACE_OF_DRUMS + rank).expect("given");
            send_use(h, a, me, meld, drum);
        }
    };
    uses(&mut h, other, 800);
    let (bona, mala) = (count_of(&h, world, BONA), count_of(&h, world, MALA));
    assert_eq!(bona + mala, 800, "one card a use");
    // Three in four the likelier (600, 12 either way is one standard deviation).
    assert!((520..=680).contains(&bona), "{bona} to {mala}");
    assert_eq!(count_of(&h, world, LESSER_FATE_CARD), 0, "nothing else");
    uses(&mut h, own, 40);
    let (bona, mala) = (
        count_of(&h, world, BONA_CARD),
        count_of(&h, world, MALA_CARD),
    );
    assert_eq!(bona + mala, 40, "the gated ones on its own rank");
    assert!(bona > 0 && mala > 0, "{bona} and {mala}");
    assert_eq!(count_of(&h, world, FATE_CARD), 0);
}

/// An item whose table names an object only for characters of another number is refused (10053),
/// the item and the object kept, nothing made; the character the interaction is for is answered.
/// Behaviour: server.interactions.an-interaction-for-some-characters-only
#[test]
fn an_item_whose_table_names_the_object_only_for_other_characters_is_refused_and_kept() {
    let mut h = parlor();
    let (a, me, world, meld) = holding(&mut h, "one", MELD_TEMPLATE);
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    let own = me as u32 as i32 % 14;
    for (leaves, answered) in [((own + 1) % 14, false), (own, true)] {
        h.server.interactions.tables.insert(
            MELD_TABLE,
            vec![Interaction {
                targets: vec![ACE_OF_DRUMS],
                reach: 3.0,
                behavior: None,
                message: Some(OPTIMAL),
                uses_up: false,
                uses_up_target: true,
                gate: interactions::Gate::of(14, leaves),
            }],
        );
        yalain_world::inventory::give(&mut h.server, &mut h.net, world, ACE_OF_DRUMS, 1);
        h.run(3);
        let drum = held(&h, world, ACE_OF_DRUMS).expect("given");
        let drums = count_of(&h, world, ACE_OF_DRUMS);
        let from = h.bots[a].inbox.len();
        send_use(&mut h, a, me, meld, drum);
        if answered {
            assert_eq!(
                answers(&h, a, from),
                [(0, interactions::FLAGS_KEPT, OPTIMAL.0)]
            );
            assert_eq!(count_of(&h, world, ACE_OF_DRUMS), drums - 1, "a drum taken");
            assert_eq!(count_of(&h, world, FATE_CARD), 1, "the fate card made");
        } else {
            let said = answers(&h, a, from);
            assert_eq!(said.len(), 1, "{said:?}");
            assert_eq!(
                (said[0].0, said[0].1),
                (
                    interactions::NOT_FOR_THIS_CHARACTER,
                    interactions::FLAGS_KEPT
                )
            );
            assert_eq!(held(&h, world, ACE_OF_DRUMS), Some(drum), "the drum kept");
            assert_eq!(count_of(&h, world, FATE_CARD), 0, "nothing made");
        }
        assert_eq!(held(&h, world, MELD_TEMPLATE), Some(meld), "the meld kept");
    }
}

/// A seal whose table names the stockpile (within 5 m, used up), and the world's row for it: the
/// camp's creatures given a dispel of their heal and a ward against it, the stockpile glowing a
/// while, and its line to those near.
const SEAL_TEMPLATE: u32 = 0x4700_7A01;
const SEAL_TABLE: u32 = 0x6F00_7A02;
const HEAL: u32 = 0x6F00_7A10;
const DISPEL: u32 = 0x6F00_7A11;
const BAR: u32 = 0x6F00_7A12;
const GLOW: u32 = 0x6F00_7A13;
const GLOW_FX: u32 = 0x4100_0411;
const PRESSED: (u32, u32) = (0xBF64_A8DB, 0x2500_1446);

fn effect(
    state: u32,
    kind: yalain_world::effect_defs::Kind,
    lasts: f32,
) -> yalain_world::effect_defs::EffectDef {
    use yalain_world::effect_defs::{EffectDef, Scale};
    EffectDef {
        state,
        kind,
        duration: if lasts > 0.0 {
            Scale(vec![(0.0, lasts)])
        } else {
            Scale::default()
        },
        priority: Scale(vec![(0.0, 1.0)]),
        fx: 0,
        external_flags: 0,
        internal_flags: 0x1100_0003,
        ends_at_logout: false,
        ends_at_teleport: false,
        enum_value: 0,
        eq_class: 4090,
        name: None,
    }
}

/// The hall with one of the test creatures' camps, the creature holding its heal; a player three
/// meters from the stockpile holding two seals. The client, the player, its connection and the
/// creature.
fn stone_circle() -> (Harness, usize, u64, ConnId, u64) {
    use yalain_content::item_rows::{EffectsOn, Interaction as Row, On};
    use yalain_world::effect_defs::{vital, Kind, Scale};
    use yalain_world::effects::{self, Cast};
    let mut h = hall();
    h.add_creatures(1);
    let mut seal = Props::default();
    seal.set(interactions::TABLE, Prop::Word(SEAL_TABLE));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(SEAL_TEMPLATE, 0x18F, seal));
    h.server.interactions.tables.insert(
        SEAL_TABLE,
        vec![Interaction {
            targets: vec![STOCKPILE_TEMPLATE],
            reach: 5.0,
            behavior: None,
            message: None,
            uses_up: true,
            uses_up_target: false,
            gate: None,
        }],
    );
    let t = &mut h.server.effect_table.effects;
    t.insert(
        HEAL,
        effect(
            HEAL,
            Kind::VitalOverTime {
                flags: vital::HEALTH,
                per_second: Scale(vec![(0.0, 200.0)]),
            },
            900.0,
        ),
    );
    t.insert(DISPEL, effect(DISPEL, Kind::Dispel(vec![HEAL]), 0.0));
    let mut bar = effect(
        BAR,
        Kind::Ward {
            classes: Vec::new(),
            effects: vec![HEAL],
            chance: Scale(vec![(0.0, 1.0)]),
        },
        1200.0,
    );
    bar.priority = Scale(vec![(0.0, 2.0)]);
    t.insert(BAR, bar);
    let mut glow = effect(GLOW, Kind::Other, 1200.0);
    glow.fx = GLOW_FX;
    t.insert(GLOW, glow);
    h.server.item_rows.interactions.push(Row {
        item: SEAL_TEMPLATE,
        on: On::Template(STOCKPILE_TEMPLATE),
        gives: Vec::new(),
        count: 1,
        gated_gives: Vec::new(),
        places: Vec::new(),
        effects_on: Some(EffectsOn {
            camp: 0,
            effects: vec![DISPEL, BAR],
        }),
        effects_here: vec![GLOW],
        tell: Some(PRESSED),
    });
    let (b, me) = h.player_in_world("one", "Elara");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(10);
    let world = world_of(&h, me);
    for _ in 0..2 {
        yalain_world::inventory::give(&mut h.server, &mut h.net, world, SEAL_TEMPLATE, 1);
    }
    h.run(3);
    let creature = h.server.camps[0].alive[0];
    let t = h.server.things.get_mut(creature).expect("made");
    t.health = 100_000;
    t.max_health = 1_000_000;
    let clock = h.clock();
    assert!(effects::apply_on(
        &mut h.server,
        &mut h.net,
        effects::On::Creature(creature),
        HEAL,
        Cast::default(),
        clock,
    ));
    east_of_stockpile(&mut h, world, 3.0);
    (h, b, me, world, creature)
}

/// The effects a creature holds, by their saved states.
fn holding_now(h: &Harness, creature: u64) -> Vec<u32> {
    yalain_world::effects::records_of(&h.server, yalain_world::effects::On::Creature(creature))
        .iter()
        .map(|e| e.state)
        .collect()
}

/// The stockpile's glow started and stopped as a client was told since `from`, and the string of
/// each line it broadcast.
fn glows_and_lines(h: &Harness, b: usize, from: usize) -> (usize, usize, Vec<u32>) {
    use yalain_world::talk::BROADCAST;
    let msgs = h.world_messages(b, from);
    let started = msgs
        .iter()
        .filter(|m| matches!(m, ServerMessage::DoFx(e) if e.sender.id == STOCKPILE && e.action.fx == GLOW_FX))
        .count();
    let stopped = msgs
        .iter()
        .filter(|m| matches!(m, ServerMessage::StopFx(e) if e.sender.id == STOCKPILE && e.action.0 == GLOW_FX))
        .count();
    let lines = msgs
        .iter()
        .filter_map(|m| match m {
            ServerMessage::CellObjectCall(c)
                if c.call.function.0 == BROADCAST && c.object.id == STOCKPILE =>
            {
                let v = c
                    .call
                    .args
                    .as_ref()
                    .and_then(|a| calls::values(&a.0).ok())?;
                v.iter().find_map(|x| match x.root() {
                    Some(Node::TextInfo(t)) => Some(t.string_id),
                    _ => None,
                })
            }
            _ => None,
        })
        .collect();
    (started, stopped, lines)
}

/// An item used on an object whose row names a camp's effects and the object's: the use is carried
/// out and answered as any (the seal used up), the camp's living creature loses its heal to the
/// dispel and holds the ward, which keeps the heal off after; the object glows for everyone who
/// sees it and says its line to those near.
/// Behaviour: server.interactions.an-interaction-puts-effects-on-a-camps-creatures-and-its-object
#[test]
fn a_seal_on_its_stone_strips_the_camps_creature_of_its_heal_and_wards_it_and_the_stone_glows() {
    use yalain_world::effects::{self, Cast};
    let (mut h, b, me, world, creature) = stone_circle();
    assert_eq!(holding_now(&h, creature), [HEAL]);
    let seal = held(&h, world, SEAL_TEMPLATE).expect("held");
    let from = h.bots[b].inbox.len();
    send_use(&mut h, b, me, seal, STOCKPILE);
    assert_eq!(
        answers(&h, b, from),
        [(
            0,
            interactions::FLAGS_USED_UP,
            interactions::text::USED_ON.0
        )]
    );
    assert_eq!(count_of(&h, world, SEAL_TEMPLATE), 1, "one used up");
    assert_eq!(holding_now(&h, creature), [BAR], "dispelled, and warded");
    assert_eq!(
        glows_and_lines(&h, b, from),
        (1, 0, vec![PRESSED.0]),
        "the stone glows and says its line"
    );
    let clock = h.clock();
    assert!(
        !effects::apply_on(
            &mut h.server,
            &mut h.net,
            effects::On::Creature(creature),
            HEAL,
            Cast::default(),
            clock,
        ),
        "the ward keeps the heal off"
    );
    assert_eq!(holding_now(&h, creature), [BAR]);
}

/// While the object holds what a use left on it, another use is refused as one in use, the item
/// kept and nothing done to the camp's creature; once it has run out the glow stops for everyone
/// who sees it, and a use is carried out again, the creature holding one ward (its heal not back).
/// Behaviour: server.interactions.an-object-holding-what-a-use-left-refuses-another
#[test]
fn a_glowing_stone_refuses_a_second_seal_until_its_glow_runs_out() {
    let (mut h, b, me, world, creature) = stone_circle();
    let seal = held(&h, world, SEAL_TEMPLATE).expect("held");
    send_use(&mut h, b, me, seal, STOCKPILE);
    let seal = held(&h, world, SEAL_TEMPLATE).expect("the second");
    let from = h.bots[b].inbox.len();
    send_use(&mut h, b, me, seal, STOCKPILE);
    assert_eq!(
        answers(&h, b, from),
        [(
            yalain_world::places::USE_STATUS_IN_USE,
            interactions::FLAGS_KEPT,
            0
        )],
        "refused as in use"
    );
    assert_eq!(count_of(&h, world, SEAL_TEMPLATE), 1, "kept");
    assert_eq!(holding_now(&h, creature), [BAR], "nothing more done");
    assert_eq!(glows_and_lines(&h, b, from), (0, 0, Vec::new()));
    let from = h.bots[b].inbox.len();
    run_for(&mut h, 1201.0);
    assert_eq!(glows_and_lines(&h, b, from).1, 1, "the glow stops");
    assert!(holding_now(&h, creature).is_empty(), "the ward ran out too");
    east_of_stockpile(&mut h, world, 3.0);
    let from = h.bots[b].inbox.len();
    send_use(&mut h, b, me, seal, STOCKPILE);
    assert_eq!(
        answers(&h, b, from),
        [(
            0,
            interactions::FLAGS_USED_UP,
            interactions::text::USED_ON.0
        )]
    );
    assert_eq!(holding_now(&h, creature), [BAR], "one ward, no heal");
    assert_eq!(glows_and_lines(&h, b, from).0, 1, "glowing again");
}
