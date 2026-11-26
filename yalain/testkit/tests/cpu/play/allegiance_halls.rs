//! Allegiance halls: a monarch binds its allegiance at a hall's binding stone, paying the hall's
//! cost, and Allegiance Recall then takes every member to the hall.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_content::halls::{Hall, HallTable};
use yalain_content::{ContentObject, ContentPack, Description, Facts, Kind, Place, Prop, Props};
use yalain_testkit::play::{server_call, use_of};
use yalain_testkit::Harness;
use yalain_world::allegiance::Node;
use yalain_world::calls::{self, Node as Graph, Value};
use yalain_world::item_use::refusal::MYSTERIOUS_USE;
use yalain_world::requests::function;
use yalain_world::skill_use::{function as skill, ALLEGIANCE_RECALL};
use yalain_world::usable::halls::{BOUND, TOO_POOR};

const STONE: u64 = 0x4002_9900_0000_0071;
const STONE_TEMPLATE: u32 = 0x4700_11BF;
const HALL_QUEST: u32 = 0x4000_01D1;
const COST: u32 = 20_000;
const SWORD: u32 = 0x4700_051C;
const SHOW_TEXT: u32 = 0x8205_0011;
/// "You recall to your bound Allegiance Hall." and "Your allegiance is not bound to an Allegiance
/// Hall.", in Allegiance Recall's table.
const RECALLED: (u32, u32) = (0x912E_FF1D, 0x2500_114F);
const NOT_BOUND: (u32, u32) = (0xCF46_E5A6, 0x2500_114F);
/// Where the hall's recall takes a member.
const HALL: Place = Place {
    cell: 0x0DFA_0107,
    origin: [-138.9, 16.4, 19.9],
    rotation: [1.0, 0.0, 0.0, 0.0],
};

/// The hidden quest reading the hall's sign gives, which the stone needs under way.
fn hall_quest(h: &mut Harness) {
    h.server
        .quests
        .quests
        .push(yalain_content::quests::QuestDef {
            duration_by_calendar: false,
            id: HALL_QUEST,
            name: yalain_content::Words::Ref(0, 0),
            description: (0, 0),
            icon: 0,
            play_fx: false,
            journals: vec![(1, 0)],
            challenge: 0,
            duration: 0.0,
            timed_phase: 0,
            retry: 259_200.0,
            xp: 0,
            cancellable: false,
            placed: "authored for a test".into(),
            hidden: true,
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
    h.server
        .quest_ids
        .insert("QuestTestHall".into(), HALL_QUEST);
}

/// A world with one hall's stone beside where players stand, a monarch holding `money` (and the
/// sign's quest under way when `permitted`) and its vassal; both armed.
fn hall_world(money: u32, permitted: bool) -> (Harness, usize, u64, usize, u64) {
    let mut h = Harness::new();
    hall_quest(&mut h);
    let mut props = Props::default();
    props
        .set("Usage_MonarchOnly", Prop::Bool(true))
        .set("Usage_RequiredQuest", Prop::Word(HALL_QUEST))
        .set("Usage_RequiredQuestStatus", Prop::Word(0x1000_0000))
        .set(
            "Usage_Effect1",
            Prop::Text("Eff_CompleteQuestTestHall".into()),
        );
    h.server
        .catalog
        .insert(yalain_testkit::test_template(STONE_TEMPLATE, 0x1B31, props));
    h.server.content = ContentPack {
        objects: vec![ContentObject {
            object: Some(STONE),
            template: STONE_TEMPLATE,
            description: Description {
                name_text: None,
                package: 0x1B31,
                look: 0x1F00_0ED5,
                enter_fx: 0,
                scale: None,
                name: None,
                numbers: [0; 6],
                source: "authored for a test".into(),
            },
            cell: 0x0299_0109,
            origin: [60.0, 240.2, -44.6],
            rotation: [1.0, 0.0, 0.0, 0.0],
            kind: Kind::Static,
            label: String::new(),
            placed: "authored for a test".into(),
            destination: None,
            facts: Facts::default(),
            walks: None,
            gate: None,
        }],
        halls: HallTable {
            rows: vec![Hall {
                key: "ondekodo".into(),
                stone: 0,
                place: HALL,
                name: "dng_test_allegiancerecall1".into(),
                cost: COST,
            }],
        },
        ..ContentPack::default()
    };
    let (a, monarch) = h.player_in_world("one", "Elara");
    let (b, vassal) = h.player_in_world("two", "Brann");
    h.run(10);
    h.arm(a, monarch, SWORD);
    h.arm(b, vassal, SWORD);
    assert!(h.server.allegiances.loaded, "read as the players entered");
    let node = |patron: u64| Node {
        patron,
        ..Node::default()
    };
    h.server.allegiances.nodes.insert(monarch, node(0));
    h.server.allegiances.nodes.insert(vassal, node(monarch));
    for p in h.server.players.values_mut() {
        if p.character != monarch {
            continue;
        }
        yalain_world::quals::set_int(
            &mut p.qualities,
            yalain_world::store::MONEY,
            money.cast_signed(),
        );
        if permitted {
            p.play.quests.push(yalain_world::state::QuestState {
                quest: HALL_QUEST,
                status: yalain_world::quests::status::UNDERWAY,
                phase: 1,
                started: 0,
                updated: 0,
            });
        }
    }
    (h, a, monarch, b, vassal)
}

fn use_stone(h: &mut Harness, a: usize, me: u64) -> usize {
    let from = h.bots[a].inbox.len();
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(function::USE, &[use_of(STONE, me)]),
    );
    h.run(60);
    from
}

fn recall(h: &mut Harness, b: usize) -> usize {
    h.all_hold(ALLEGIANCE_RECALL);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(
            skill::USE,
            &[Arg::Int(1), Arg::Int(ALLEGIANCE_RECALL), Arg::LongInt(0)],
        ),
    );
    h.run(400);
    from
}

/// The texts shown since `from`: each one's string id and table.
fn texts(h: &Harness, b: usize, from: usize) -> Vec<(u32, u32)> {
    h.calls(b, from)
        .into_iter()
        .filter(|(f, _)| *f == SHOW_TEXT)
        .filter_map(|(_, v)| {
            v.iter().find_map(|x| match x.root() {
                Some(Graph::TextInfo(t)) => Some((t.string_id, t.table)),
                _ => None,
            })
        })
        .collect()
}

/// The use answers since `from`: each one's status.
fn use_statuses(h: &Harness, b: usize, from: usize) -> Vec<u32> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c)
                if c.function.0 == yalain_world::places::function::USE_DONE =>
            {
                let v = c.args.as_ref().and_then(|a| calls::values(&a.0).ok())?;
                let Some(Value::Graph(g)) = v.first() else {
                    return None;
                };
                g.iter().find_map(|(_, n)| match n {
                    Graph::Script { slots, .. } => Some(slots[11].1),
                    _ => None,
                })
            }
            _ => None,
        })
        .collect()
}

/// Where the client's player was put with a newer teleport stamp since `from`, if anywhere.
fn landed(h: &Harness, b: usize, from: usize, me: u64) -> Option<u32> {
    h.world_messages(b, from).iter().find_map(|m| match m {
        ServerMessage::Position(p) if p.sender.id == me && p.teleport_stamp > 0 => Some(p.cell),
        _ => None,
    })
}

fn money(h: &Harness, me: u64) -> u32 {
    h.server
        .players
        .values()
        .find(|p| p.character == me)
        .map_or(0, |p| yalain_world::store::money(&p.qualities))
}

fn quest_phase(h: &Harness, me: u64) -> u32 {
    h.server
        .players
        .values()
        .find(|p| p.character == me)
        .map_or(0, |p| {
            yalain_world::quests::phase_of(&p.play.quests, HALL_QUEST)
        })
}

/// A monarch with the sign's permission who uses a hall's binding stone pays the hall's cost, is
/// told "You succesfully bind your allegiance to the Allegiance Recall Stone.", and the quest that
/// gave the permission is done; from then on its vassal's Allegiance Recall takes it to the hall,
/// "You recall to your bound Allegiance Hall.", and the allegiance's record keeps the hall.
/// Behaviour: server.places.allegiance-hall-binding
#[test]
fn a_monarch_binds_its_allegiance_at_a_halls_stone_and_a_vassal_recalls_there() {
    let (mut h, a, monarch, b, vassal) = hall_world(25_000, true);
    let from = use_stone(&mut h, a, monarch);
    assert!(
        texts(&h, a, from).contains(&(BOUND.0, BOUND.1)),
        "told it is bound"
    );
    assert_eq!(money(&h, monarch), 25_000 - COST, "the cost paid");
    assert_eq!(quest_phase(&h, monarch), 0, "the permission's quest done");
    let kept = h.server.allegiances.to_bytes();
    let read = yalain_world::allegiance::Allegiances::from_bytes(&kept).expect("reads");
    let hall = read.hall_of(vassal).expect("kept for the whole allegiance");
    assert_eq!(hall.name, "dng_test_allegiancerecall1");
    assert_eq!(hall.place.cell, HALL.cell);

    let from = recall(&mut h, b);
    assert_eq!(landed(&h, b, from, vassal), Some(HALL.cell), "to the hall");
    assert!(texts(&h, b, from).contains(&RECALLED));
    assert_eq!(exported_hall(&mut h, a), "dng_test_allegiancerecall1");
}

/// The hall the allegiance's tree export names, as the monarch asks for it.
fn exported_hall(h: &mut Harness, a: usize) -> String {
    use yalain_world::allegiance::function as allegiance;
    let mut file = yalain_world::graph::Builder::new();
    file.native(
        0x76,
        auberean_script::native_object::NativeObject::Text(
            "allegianceexport.xml".encode_utf16().collect(),
        ),
    );
    let from = h.bots[a].inbox.len();
    h.ask(a, allegiance::TREE_EXPORT, &[Arg::Object(file.graph())], 10);
    let calls = h.calls(a, from);
    let (_, v) = calls
        .iter()
        .find(|(f, _)| *f == allegiance::EXPORT)
        .expect("the export");
    let g = v
        .iter()
        .find_map(|x| match x {
            Value::Graph(g) if matches!(x.root(), Some(Graph::Script { package: 0x1B2, .. })) => {
                Some(g)
            }
            _ => None,
        })
        .expect("the allegiance");
    let Some(Graph::Script { slots, .. }) = g.first().map(|(_, n)| n) else {
        panic!("its root");
    };
    g.iter()
        .find_map(|(i, n)| match n {
            Graph::Text(t) if *i == slots[2].1 => Some(t.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

/// A monarch short of the hall's cost is told "You do not have enough money to bind to this
/// allegiance stone."; nothing is paid, the permission's quest stays under way, and its vassal's
/// Allegiance Recall still says the allegiance is not bound.
/// Behaviour: server.places.allegiance-hall-binding
#[test]
fn a_monarch_short_of_the_cost_is_told_so_and_nothing_is_bound() {
    let (mut h, a, monarch, b, vassal) = hall_world(COST - 1, true);
    let from = use_stone(&mut h, a, monarch);
    assert!(texts(&h, a, from).contains(&(TOO_POOR.0, TOO_POOR.1)));
    assert!(!texts(&h, a, from).contains(&(BOUND.0, BOUND.1)));
    assert_eq!(money(&h, monarch), COST - 1, "nothing paid");
    assert_eq!(quest_phase(&h, monarch), 1, "still under way");
    let from = recall(&mut h, b);
    assert_eq!(landed(&h, b, from, vassal), None);
    assert!(texts(&h, b, from).contains(&NOT_BOUND));
}

/// A monarch who has not read the hall's sign (the stone's quest not under way) is refused at the
/// stone with the refusal of a hidden quest, and nothing is paid or bound.
/// Behaviour: server.places.allegiance-hall-binding
#[test]
fn a_monarch_without_the_signs_permission_is_refused_at_the_stone() {
    let (mut h, a, monarch, _, vassal) = hall_world(25_000, false);
    let from = use_stone(&mut h, a, monarch);
    assert_eq!(use_statuses(&h, a, from), [MYSTERIOUS_USE]);
    assert!(!texts(&h, a, from).contains(&(BOUND.0, BOUND.1)));
    assert_eq!(money(&h, monarch), 25_000);
    assert!(h.server.allegiances.hall_of(vassal).is_none());
}

/// An allegiance's record kept before halls could be bound reads back with no hall; one bound
/// reads back with its hall's name and place.
/// Behaviour: server.places.allegiance-hall-binding
#[test]
fn an_allegiances_bound_hall_is_kept_and_an_older_record_reads_with_none() {
    use yalain_world::allegiance::{Allegiances, BoundHall};
    let mut all = Allegiances::default();
    all.nodes.insert(7, Node::default());
    all.nodes.insert(
        8,
        Node {
            patron: 7,
            ..Node::default()
        },
    );
    // Version 2: no members, no names, one charter (monarch 7: no message, no officers, no
    // restriction), no titles.
    let mut older = vec![2u8];
    older.extend(0u32.to_le_bytes());
    older.extend(0u32.to_le_bytes());
    older.extend(1u32.to_le_bytes());
    older.extend(7u64.to_le_bytes());
    older.push(0);
    older.extend(0u32.to_le_bytes());
    older.push(0);
    older.extend(0u32.to_le_bytes());
    let read = Allegiances::from_bytes(&older).expect("an older record reads");
    assert!(read.charters.get(&7).is_some_and(|c| c.hall.is_none()));
    all.charter_mut(7).hall = Some(BoundHall {
        name: "dng_test_allegiancerecall1".into(),
        place: yalain_store::StoredPosition {
            cell: HALL.cell,
            origin: HALL.origin,
            rotation: HALL.rotation,
        },
    });
    let read = Allegiances::from_bytes(&all.to_bytes()).expect("reads");
    let hall = read.hall_of(8).expect("its monarch's");
    assert_eq!(hall.place.origin, HALL.origin);
    assert_eq!(hall.name, "dng_test_allegiancerecall1");
}
