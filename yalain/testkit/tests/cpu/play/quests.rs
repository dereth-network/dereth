//! Quests beyond a person's step: an item used on an object that takes the player somewhere and
//! makes its update on arrival, an arrival through a portal that moves quests on, one use running
//! several quests' steps, and what a completion pays; a hidden quest held but never told, the
//! effects a quest's definition names for an update put on first, a timed quest failing when its
//! time runs out.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use auberean_script::native_object::QuestInfo;
use auberean_script::wire::In;
use yalain_content::quests::{Need, Place, QuestDef, QuestTable, Step, Trigger, Update};
use yalain_content::{ContentObject, ContentPack, Description, Facts, Kind};
use yalain_testkit::play::server_call;
use yalain_testkit::{test_catalog, Harness, TEST_ITEM};
use yalain_world::calls::{self, Value};
use yalain_world::graph::{Builder, Slot};
use yalain_world::quests::{completion_xp, status};
use yalain_world::state::QuestState;

const USE: u32 = 0x8216_0016;
const UPDATE_QUEST: u32 = 0x8213_0024;
const ENTER_PORTAL_SPACE: u32 = 0x8212_0020;
const DISCRETE_DESTROY: u32 = 0x820E_0058;

const HALL_QUEST: u32 = 0x4000_0088;
const NEXT_QUEST: u32 = 0x4000_0085;
const VAULT: u64 = 0x4002_9900_0000_0057;
const VAULT_TEMPLATE: u32 = 0x4700_1CE8;
const EXPLORER: u64 = 0x8130_0000_0000_0201;
const EXPLORER_TEMPLATE: u32 = 0x4700_31E6;

fn object(id: u64, template: u32, kind: Kind, origin: [f32; 3]) -> ContentObject {
    ContentObject {
        object: Some(id),
        template,
        description: Description {
            package: 0x103E,
            look: 0x1F00_0100,
            enter_fx: 0,
            scale: None,
            name: Some((0xAE3C_41DB, 0x2500_1B44)),
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

fn quest(id: u32, xp: u64, challenge: i32) -> QuestDef {
    QuestDef {
        id,
        name: (0xA860_7CEC, 0x2500_17EE),
        description: (0x6C45_9364, 0x2500_17EE),
        icon: 0x4100_034B,
        play_fx: true,
        journals: (1..=8).map(|n| (n, 0x2500_17EE)).collect(),
        challenge,
        duration: 72000.0,
        retry: 0.0,
        xp,
        cancellable: false,
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

/// A use of an item, on an object or on none.
fn use_on(item: u64, target: u64, player: u64) -> Arg {
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
            Slot::Long(target),
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

/// The quest update kinds a client received since `from`.
fn updates(h: &Harness, b: usize, from: usize) -> Vec<u32> {
    calls_of(h, b, from)
        .into_iter()
        .filter(|(f, _)| *f == UPDATE_QUEST)
        .filter_map(|(_, v)| match v.last() {
            Some(Value::Int(k)) => Some(*k),
            _ => None,
        })
        .collect()
}

/// Put a player's quest at a phase.
fn at_phase(h: &mut Harness, quest: u32, phase: u32) {
    for p in h.server.players.values_mut() {
        p.play.quests.push(QuestState {
            quest,
            status: status::UNDERWAY,
            phase,
            started: 0,
            updated: 0,
        });
    }
}

/// An item a player holds, given it by the world.
fn held_item(h: &mut Harness, template: u32) -> u64 {
    let world = *h.server.players.keys().next().expect("a player");
    yalain_world::inventory::give(&mut h.server, &mut h.net, world, template, 1);
    h.server.players[&world]
        .play
        .items
        .iter()
        .find(|i| i.template == template)
        .expect("held")
        .id
}

/// A held item used on an object, when a quest's step is triggered by that pair and takes the
/// player somewhere: the player plays the use; two and a half seconds later the object flashes and
/// the player enters portal space; out of it, the quest moves on, the item is gone and the player
/// is at the step's place, in that order.
/// Behaviour: server.quests.item-on-object-takes-the-player
#[test]
fn an_item_used_on_an_object_takes_the_player_and_moves_the_quest_on_arrival() {
    let mut h = Harness::new();
    h.server.catalog = test_catalog();
    h.server.content = ContentPack {
        objects: vec![object(
            VAULT,
            VAULT_TEMPLATE,
            Kind::Static,
            [61.0, 241.0, -44.6],
        )],
        ..ContentPack::default()
    };
    let to = Place {
        cell: 0x0299_014D,
        origin: [1.15, -7.59, 1.30],
        heading: 0,
    };
    h.server.quests = QuestTable {
        quests: vec![quest(HALL_QUEST, 0, -999)],
        steps: vec![Step {
            takes: vec![TEST_ITEM],
            races: Vec::new(),
            to: Some(to),
            ..step(
                HALL_QUEST,
                6,
                Trigger::UseOn(TEST_ITEM, VAULT_TEMPLATE),
                Update::Advance(7),
            )
        }],
    };
    let (b, me) = h.player_in_world("one", "Elara");
    at_phase(&mut h, HALL_QUEST, 6);
    let glyph = held_item(&mut h, TEST_ITEM);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(USE, &[use_on(glyph, VAULT, me)]),
    );
    h.run(20);
    let first = h.world_messages(b, from);
    assert!(
        first
            .iter()
            .any(|m| matches!(m, ServerMessage::PlayBehavior(e)
            if e.sender.id == me && e.action.behavior == 0x4100_00FC)),
        "the player plays the use"
    );
    assert!(
        updates(&h, b, from).is_empty(),
        "nothing moves on before the trip"
    );
    h.run(100);
    let msgs = h.world_messages(b, from);
    let flash = msgs.iter().position(|m| {
        matches!(m, ServerMessage::DoFx(e)
        if e.sender.id == VAULT && e.action.fx == 0x4100_0044)
    });
    let enter = msgs.iter().position(|m| {
        matches!(m, ServerMessage::PlayerCall(c)
        if c.function.0 == ENTER_PORTAL_SPACE)
    });
    assert!(
        flash.is_some() && enter.is_some() && flash < enter,
        "the flash, then portal space"
    );
    assert_eq!(
        updates(&h, b, from),
        vec![0x2000_0007],
        "moved on to the seventh"
    );
    let gone = calls_of(&h, b, from)
        .iter()
        .any(|(f, v)| *f == DISCRETE_DESTROY && v.first() == Some(&Value::Long(glyph)));
    assert!(gone, "the item is taken");
    let p = h.server.players.values().next().expect("the player");
    assert_eq!(p.position.cell, 0x0299_014D, "at the step's place");
    assert!(p.play.items.iter().all(|i| i.id != glyph));
}

/// One use runs every step it triggers, each quest at most once: an explorer completes the quest
/// that led to it and gives the next, whose step requires the first done.
/// Behaviour: server.quests.one-event-several-steps
#[test]
fn one_use_completes_a_quest_and_gives_the_next_that_requires_it() {
    let mut h = Harness::new();
    h.server.content = ContentPack {
        objects: vec![object(
            EXPLORER,
            EXPLORER_TEMPLATE,
            Kind::Npc,
            [61.0, 241.0, -44.6],
        )],
        ..ContentPack::default()
    };
    h.server.quests = QuestTable {
        quests: vec![quest(NEXT_QUEST, 0, -999), quest(HALL_QUEST, 0, -999)],
        steps: vec![
            Step {
                requires: vec![(NEXT_QUEST, Need::Done)],
                ..step(
                    HALL_QUEST,
                    0,
                    Trigger::Use(EXPLORER_TEMPLATE),
                    Update::Bestow,
                )
            },
            step(
                NEXT_QUEST,
                1,
                Trigger::Use(EXPLORER_TEMPLATE),
                Update::Complete,
            ),
        ],
    };
    let (b, me) = h.player_in_world("one", "Elara");
    at_phase(&mut h, NEXT_QUEST, 1);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(USE, &[use_on(EXPLORER, 0, me)]),
    );
    h.run(20);
    assert_eq!(
        updates(&h, b, from),
        vec![0x3000_0000, 0x1000_0000],
        "the first completed, then the next given"
    );
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(USE, &[use_on(EXPLORER, 0, me)]),
    );
    h.run(20);
    assert!(updates(&h, b, from).is_empty(), "neither again");
}

/// A completion pays the quest's experience weighed by its challenge against the player's level
/// as a kill is (full from ten levels under; 15% less a level further; none weighed without a
/// challenge).
/// Behaviour: server.quests.completion-pays-by-challenge
#[test]
fn a_completion_pays_its_experience_weighed_by_challenge_against_level() {
    assert_eq!(completion_xp(100, 3, 13), 100);
    assert_eq!(completion_xp(100, 3, 14), 85);
    assert_eq!(completion_xp(100, 3, 16), 55);
    assert_eq!(completion_xp(1000, 3, 40), 10);
    assert_eq!(completion_xp(500, -999, 40), 500);
}

/// A step for some races runs only for a player of one of them: of two steps on the same use, one
/// for Tumeroks and one for humans, a human gets the humans' and a Tumerok the Tumeroks'.
/// Behaviour: server.quests.step-for-some-races
#[test]
fn a_step_for_some_races_runs_only_for_a_player_of_one_of_them() {
    let mut h = Harness::new();
    h.server.catalog = test_catalog();
    let trigger = Trigger::Use(VAULT_TEMPLATE);
    h.server.quests = QuestTable {
        quests: vec![quest(HALL_QUEST, 0, -999)],
        steps: vec![
            Step {
                races: vec![4],
                xp: 4,
                ..step(HALL_QUEST, 0, trigger, Update::Bestow)
            },
            Step {
                races: vec![1],
                xp: 1,
                ..step(HALL_QUEST, 0, trigger, Update::Bestow)
            },
        ],
    };
    let (_b, _me) = h.player_in_world("one", "Elara");
    let world = *h.server.players.keys().next().expect("a player");
    let chosen =
        |h: &Harness| yalain_world::quests::step_for(&h.server, world, trigger, 0).map(|s| s.xp);
    for (race, want) in [(1, Some(1)), (4, Some(4)), (2, None)] {
        if let Some(p) = h.server.players.get_mut(&world) {
            p.race = race;
        }
        assert_eq!(chosen(&h), want, "race {race}");
    }
}

/// The explorer standing beside the start, and the test world's quest table.
fn explorer_world(quests: Vec<QuestDef>, steps: Vec<Step>) -> Harness {
    let mut h = Harness::new();
    h.server.content = ContentPack {
        objects: vec![object(
            EXPLORER,
            EXPLORER_TEMPLATE,
            Kind::Npc,
            [61.0, 241.0, -44.6],
        )],
        ..ContentPack::default()
    };
    h.server.quests = QuestTable { quests, steps };
    h
}

/// Use the explorer and let the world run a second.
fn use_explorer(h: &mut Harness, b: usize, me: u64) -> usize {
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(USE, &[use_on(EXPLORER, 0, me)]),
    );
    h.run(20);
    from
}

/// A player's status and phase of a quest, if it holds it.
fn held(h: &Harness, quest: u32) -> Option<(u32, u32)> {
    h.server
        .players
        .values()
        .next()
        .and_then(|p| p.play.quests.iter().find(|q| q.quest == quest))
        .map(|q| (q.status, q.phase))
}

/// A hidden quest is held and moved on as any other, but no update of it is ever told: given, it
/// is under way at its first phase; completed, it is done; the client hears of neither.
/// Behaviour: server.quests.hidden-never-told
#[test]
fn a_hidden_quest_is_held_and_moved_on_but_never_told() {
    let hidden = QuestDef {
        hidden: true,
        ..quest(HALL_QUEST, 0, -999)
    };
    let mut h = explorer_world(
        vec![hidden],
        vec![
            step(
                HALL_QUEST,
                0,
                Trigger::Use(EXPLORER_TEMPLATE),
                Update::Bestow,
            ),
            step(
                HALL_QUEST,
                1,
                Trigger::Use(EXPLORER_TEMPLATE),
                Update::Complete,
            ),
        ],
    );
    let (b, me) = h.player_in_world("one", "Elara");
    let from = use_explorer(&mut h, b, me);
    assert!(updates(&h, b, from).is_empty(), "its giving is not told");
    assert_eq!(held(&h, HALL_QUEST), Some((status::UNDERWAY, 1)));
    let from = use_explorer(&mut h, b, me);
    assert!(updates(&h, b, from).is_empty(), "nor its completion");
    assert_eq!(held(&h, HALL_QUEST), Some((status::COMPLETED, 0)));
}

/// An update puts on the player the effects the quest's definition names for that kind of update
/// before the update is told: a completion whose effect gives another quest tells that quest given,
/// then the completion.
/// Behaviour: server.quests.update-effects-come-first
#[test]
fn an_update_puts_on_its_definitions_effects_before_it_is_told() {
    use yalain_world::effect_defs::{EffectDef, Kind as EffectKind, Scale};
    const GIVES_HALL: u32 = 0x6F00_4484;
    let rewarding = QuestDef {
        effects: vec![(0x3000_0000, GIVES_HALL)],
        ..quest(NEXT_QUEST, 0, -999)
    };
    let mut h = explorer_world(
        vec![rewarding, quest(HALL_QUEST, 0, -999)],
        vec![step(
            NEXT_QUEST,
            1,
            Trigger::Use(EXPLORER_TEMPLATE),
            Update::Complete,
        )],
    );
    h.server.effect_table.effects.insert(
        GIVES_HALL,
        EffectDef {
            state: GIVES_HALL,
            kind: EffectKind::Quest {
                quest: HALL_QUEST,
                update: 0x1000_0000,
            },
            duration: Scale::default(),
            priority: Scale::default(),
            fx: 0,
            external_flags: 0,
            internal_flags: 0,
            enum_value: 0,
            eq_class: 0,
            name: None,
        },
    );
    let (b, me) = h.player_in_world("one", "Elara");
    at_phase(&mut h, NEXT_QUEST, 1);
    let from = use_explorer(&mut h, b, me);
    assert_eq!(
        updates(&h, b, from),
        vec![0x1000_0000, 0x3000_0000],
        "the effect's quest given, then the completion told"
    );
    assert_eq!(held(&h, HALL_QUEST), Some((status::UNDERWAY, 1)));
}

/// A quest with a duration fails when its time runs out: nothing just before, then one update
/// of the fail kind, the quest failed and its whole wait to be taken again in the record.
/// Behaviour: server.quests.fails-when-its-time-runs-out
#[test]
fn a_timed_quest_fails_when_its_time_runs_out() {
    let timed = QuestDef {
        duration: 60.0,
        retry: 3600.0,
        ..quest(HALL_QUEST, 0, -999)
    };
    let mut h = explorer_world(
        vec![timed],
        vec![step(
            HALL_QUEST,
            0,
            Trigger::Use(EXPLORER_TEMPLATE),
            Update::Bestow,
        )],
    );
    let (b, me) = h.player_in_world("one", "Elara");
    let asked = h.now.0;
    let from = use_explorer(&mut h, b, me);
    assert_eq!(updates(&h, b, from), vec![0x1000_0000]);
    let from = h.bots[b].inbox.len();
    h.run_to(yalain_net::LocalTime(asked + 58.0));
    assert!(updates(&h, b, from).is_empty(), "not yet");
    assert_eq!(held(&h, HALL_QUEST), Some((status::UNDERWAY, 1)));
    h.run_to(yalain_net::LocalTime(asked + 64.0));
    assert_eq!(updates(&h, b, from), vec![0x4000_0000], "failed");
    assert_eq!(held(&h, HALL_QUEST), Some((status::FAILED, 0)));
    let record = calls_of(&h, b, from)
        .into_iter()
        .filter(|(f, _)| *f == UPDATE_QUEST)
        .find_map(|(_, v)| match v.get(1)?.root()? {
            yalain_world::calls::Node::Native { bytes, .. } => {
                QuestInfo::unpack(&mut In::new(bytes)).ok()
            }
            _ => None,
        })
        .expect("the failed quest's record");
    assert_eq!((record.status, record.phase), (status::FAILED, 0));
    assert!(
        (f64::from_bits(record.seconds_until_retry) - 3600.0).abs() < 1e-6,
        "its whole wait to be taken again"
    );
}
