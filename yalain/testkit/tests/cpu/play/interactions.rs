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
        }],
    );
    h.server.quests.quests.push(QuestDef {
        id: ENTER_QUEST,
        name: (0x6EE1_B009, 0x2500_1446),
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
        once: false,
        fellows: yalain_content::quests::Fellows::default(),
        last_waits: false,
        hero_only: false,
        pay: yalain_content::quests::CompletionPay::default(),
        required: None,
    });
    h.server.quests.steps.push(Step {
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
        placed: "authored for a test".into(),
        requires: Vec::new(),
        takes: Vec::new(),
        to: None,
        races: Vec::new(),
        effects_last: false,
        wants: None,
        fellows: true,
        levels: None,
        xp_share: 0.0,
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
        wants: None,
        fellows: false,
        levels: None,
        xp_share: 0.0,
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
