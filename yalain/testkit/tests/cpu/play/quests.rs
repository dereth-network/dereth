//! Quests beyond a person's step: an item used on an object that takes the player somewhere and
//! makes its update on arrival, an arrival through a portal that moves quests on, one use running
//! several quests' steps, and what a completion pays; a hidden quest held but never told, the
//! effects a quest's definition names for an update put on first, a timed quest failing when its
//! time runs out.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use auberean_script::native_object::QuestInfo;
use auberean_script::wire::In;
use yalain_content::quests::CompletionPay;
use yalain_content::quests::{Need, Place, QuestDef, QuestTable, Step, Trigger, Update};
use yalain_content::{ContentObject, ContentPack, Description, Facts, Kind};
use yalain_testkit::play::server_call;
use yalain_testkit::{test_catalog, Harness, TEST_ITEM};
use yalain_world::calls::{self, Value};
use yalain_world::graph::{Builder, Slot};
use yalain_world::levels::{Level, LevelTable};
use yalain_world::quests::{completion_pay, completion_xp, status};
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
            name_text: None,
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
        timed_phase: 0,
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
        gives: Vec::new(),
        rooms: Vec::new(),
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
        gives: Vec::new(),
        rooms: Vec::new(),
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

/// A completion below a player of level 50 falls as a kill does, sooner and faster: the 2017 vault
/// shards of challenge 44 paid level 50 60% and level 47 in full.
/// Behaviour: server.experience.from-level-50-the-fall-comes-sooner
#[test]
fn a_completion_below_a_level_50_player_is_weighed_as_a_kill_of_that_level_is() {
    assert_eq!(completion_xp(1_500_000, 44, 50), 900_000);
    assert_eq!(completion_xp(2_600_000, 44, 50), 1_560_000);
    assert_eq!(completion_xp(1_500_000, 44, 47), 1_500_000);
    assert_eq!(completion_xp(3_800_000, 46, 50), 3_800_000);
    assert_eq!(completion_xp(10_000_000, 40, 57), 100_000);
    assert_eq!(completion_xp(500, -999, 60), 500, "no challenge: whole");
}

/// A level table where a level takes the amounts given to reach the next, from level 1.
fn level_costs(costs: &[(i32, u64)]) -> LevelTable {
    let top = costs.iter().map(|(l, _)| *l).max().unwrap_or(0) + 1;
    let mut xp = 0;
    let levels = (1..=top)
        .map(|level| {
            let this = Level {
                xp,
                ..Level::default()
            };
            xp += costs
                .iter()
                .find(|(l, _)| *l == level)
                .map_or(1, |(_, cost)| *cost);
            this
        })
        .collect();
    LevelTable { levels }
}

fn conn_of(h: &Harness, me: u64) -> yalain_net::ConnId {
    *h.server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in the world")
        .0
}

fn paying(challenge: i32, pay: CompletionPay) -> QuestDef {
    QuestDef {
        pay,
        ..quest(HALL_QUEST, 0, challenge)
    }
}

/// A completion adds its quest's points to its player's quest rating: whole, whatever the quest's
/// challenge against the player's level, and every time the quest is completed. Other updates add
/// nothing, and the rating is kept with the character.
/// Behaviour: server.quests.completion-adds-points
#[test]
fn a_completion_adds_its_points_to_the_rating_every_time_whatever_the_level() {
    const QUEST: u32 = 0x4000_0033;
    let mut h = Harness::new();
    let def = QuestDef {
        pay: CompletionPay {
            points: 400,
            ..CompletionPay::default()
        },
        ..quest(QUEST, 800, 8)
    };
    h.server.quests = QuestTable {
        quests: vec![def],
        steps: Vec::new(),
        gives: Vec::new(),
        rooms: Vec::new(),
    };
    let (_, me) = h.player_in_world("one", "Elara");
    let world = conn_of(&h, me);
    yalain_world::quals::set_int(
        &mut h.server.players.get_mut(&world).unwrap().qualities,
        302,
        60,
    );
    let rating = |h: &Harness| h.server.players[&world].play.quest_rating;
    let run = |h: &mut Harness, up: Update| {
        let clock = h.clock();
        yalain_world::quests::apply(&mut h.server, &mut h.net, world, QUEST, up, 0, clock);
    };
    assert_eq!(rating(&h), 0, "a new character's");
    run(&mut h, Update::Bestow);
    run(&mut h, Update::Advance(2));
    assert_eq!(rating(&h), 0, "only a completion adds");
    for times in 1..=3 {
        run(&mut h, Update::Complete);
        assert_eq!(rating(&h), 400 * times, "every time, whatever the level");
    }
    let play = h.server.players[&world].play.clone();
    let back = yalain_world::state::PlayState::from_bytes(&play.to_bytes()).expect("reads");
    assert_eq!(back.quest_rating, 1200, "kept");
    // Saved before there was a rating: none.
    let mut old = play.to_bytes();
    old.truncate(old.len() - 4);
    old[..4].copy_from_slice(&14u32.to_le_bytes());
    let older = yalain_world::state::PlayState::from_bytes(&old).expect("reads");
    assert_eq!(older.quest_rating, 0, "an older save");
}

/// A hidden quest's completion pays what it pays with no text and no update told; a quest shown
/// names itself in the text of its pay.
/// Behaviour: server.quests.hidden-pays-quietly
#[test]
fn a_hidden_quests_completion_pays_with_no_text() {
    const HIDDEN: u32 = 0x4000_0033;
    const SHOWN: u32 = 0x4000_0035;
    let mut h = Harness::new();
    h.server.quests = QuestTable {
        quests: vec![
            QuestDef {
                hidden: true,
                ..quest(HIDDEN, 800, -999)
            },
            quest(SHOWN, 800, -999),
        ],
        steps: Vec::new(),
        gives: Vec::new(),
        rooms: Vec::new(),
    };
    let (b, me) = h.player_in_world("one", "Elara");
    let world = conn_of(&h, me);
    let total = |h: &Harness| {
        yalain_world::quals::long(&h.server.players[&world].qualities, 300).unwrap_or(0)
    };
    let run = |h: &mut Harness, q: u32| {
        let from = h.bots[b].inbox.len();
        let before = total(h);
        let clock = h.clock();
        yalain_world::quests::apply(
            &mut h.server,
            &mut h.net,
            world,
            q,
            Update::Complete,
            0,
            clock,
        );
        h.run(3);
        (total(h) - before, h.texts(b, from), h.functions(b, from))
    };
    let (paid, texts, calls) = run(&mut h, HIDDEN);
    assert_eq!(paid, 800, "its pay");
    assert!(texts.is_empty(), "no text: {texts:x?}");
    assert!(
        !calls.contains(&yalain_world::quests::UPDATE_QUEST),
        "no update"
    );
    let (paid, texts, calls) = run(&mut h, SHOWN);
    assert_eq!(paid, 800);
    assert_eq!(texts.len(), 1, "a shown quest's pay is named");
    assert!(calls.contains(&yalain_world::quests::UPDATE_QUEST));
}

/// A quest may pay a share of what its player's level takes to reach the next, rounded down: the
/// amounts below are what January 2017's completions paid, with each kind of limit a setup names.
/// Behaviour: server.quests.completion-pays-a-share
#[test]
fn a_completion_pays_a_share_of_what_the_level_takes_within_its_limits() {
    let levels = level_costs(&[
        (42, 21_000_000),
        (46, 83_998_037),
        (47, 110_785_010),
        (49, 191_266_097),
        (52, 600_000_000),
        (55, 1_491_900_877),
        (57, 2_992_628_674),
    ]);
    let share = |share: f32| CompletionPay {
        share,
        flat: true,
        ..CompletionPay::default()
    };
    assert_eq!(
        completion_pay(&levels, &paying(49, share(0.04)), 49),
        7_650_643,
        "the share"
    );
    let at_level = |s: f32, level: i32| CompletionPay {
        most_at_level: Some((s, level)),
        ..share(s)
    };
    assert_eq!(
        completion_pay(&levels, &paying(46, at_level(0.045, 46)), 49),
        3_779_911,
        "over the level its limit names, what that level comes to"
    );
    assert_eq!(
        completion_pay(&levels, &paying(52, at_level(0.03, 52)), 49),
        5_737_982,
        "under that level, its own"
    );
    let chaos = CompletionPay {
        least: 40_000_000,
        most: 100_000_000,
        ..share(0.02)
    };
    assert_eq!(
        completion_pay(&levels, &paying(-999, chaos), 47),
        40_000_000,
        "at least its least"
    );
    assert_eq!(
        completion_pay(&levels, &paying(-999, chaos), 57),
        59_852_572,
        "over its least, its own"
    );
    let most = CompletionPay {
        least: 1_000_000,
        most: 5_000_000,
        ..share(0.35)
    };
    assert_eq!(
        completion_pay(&levels, &paying(-999, most), 42),
        5_000_000,
        "at most its most"
    );
}

/// A quest whose setup says its challenge is ignored pays its amount whole, and any other is
/// weighed: at level 13, a challenge 1 quest of 300 pays 300 or 210.
/// Behaviour: server.quests.completion-paid-whole
#[test]
fn a_completion_its_setup_says_ignores_its_challenge_is_paid_whole() {
    let levels = LevelTable::default();
    let weighed = quest(HALL_QUEST, 300, 1);
    assert_eq!(completion_pay(&levels, &weighed, 13), 210);
    let whole = QuestDef {
        pay: CompletionPay {
            flat: true,
            ..CompletionPay::default()
        },
        ..weighed
    };
    assert_eq!(completion_pay(&levels, &whole, 13), 300);
}

/// Completing a quest that pays a share adds that much to the player's experience.
/// Behaviour: server.quests.completion-pays-a-share
#[test]
fn completing_a_quest_that_pays_a_share_adds_it_to_the_players_experience() {
    let mut h = Harness::new();
    h.server.levels = level_costs(&[(1, 1_000)]);
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
        quests: vec![paying(
            -999,
            CompletionPay {
                share: 0.5,
                flat: true,
                ..CompletionPay::default()
            },
        )],
        steps: vec![step(
            HALL_QUEST,
            1,
            Trigger::Use(EXPLORER_TEMPLATE),
            Update::Complete,
        )],
        gives: Vec::new(),
        rooms: Vec::new(),
    };
    let (b, me) = h.player_in_world("one", "Elara");
    at_phase(&mut h, HALL_QUEST, 1);
    let total = |h: &Harness| {
        h.server
            .players
            .values()
            .next()
            .and_then(|p| {
                yalain_world::quals::long(&p.qualities, yalain_world::progress::q::TOTAL_XP)
            })
            .unwrap_or(0)
    };
    let before = total(&h);
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(USE, &[use_on(EXPLORER, 0, me)]),
    );
    h.run(20);
    assert_eq!(total(&h) - before, 500);
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
        gives: Vec::new(),
        rooms: Vec::new(),
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
    h.server.quests = QuestTable {
        quests,
        steps,
        gives: Vec::new(),
        rooms: Vec::new(),
    };
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
        timed_phase: 0,
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

/// The records of a quest a client was told since `from`.
fn records(h: &Harness, b: usize, from: usize) -> Vec<QuestInfo> {
    calls_of(h, b, from)
        .into_iter()
        .filter(|(f, _)| *f == UPDATE_QUEST)
        .filter_map(|(_, v)| match v.get(1)?.root()? {
            yalain_world::calls::Node::Native { bytes, .. } => {
                QuestInfo::unpack(&mut In::new(bytes)).ok()
            }
            _ => None,
        })
        .collect()
}

/// A quest of a minute's time in one phase, moved on by each use of the explorer.
fn timed_in(phase: u32) -> Harness {
    let timed = QuestDef {
        duration: 60.0,
        timed_phase: phase,
        retry: 3600.0,
        ..quest(HALL_QUEST, 0, -999)
    };
    let explorer = Trigger::Use(EXPLORER_TEMPLATE);
    explorer_world(
        vec![timed],
        vec![
            step(HALL_QUEST, 0, explorer, Update::Bestow),
            step(HALL_QUEST, 1, explorer, Update::Advance(2)),
            step(HALL_QUEST, 2, explorer, Update::Advance(3)),
        ],
    )
}

/// A record's time left, if it says it has one.
fn time_left(q: &QuestInfo) -> Option<f64> {
    (q.expires != 0).then(|| f64::from_bits(q.seconds_remaining))
}

/// A quest whose duration runs in its first phase shows its time there. Once its next phase comes it
/// shows none, and its time running out then does not fail it.
/// Behaviour: server.quests.timed-in-one-phase
#[test]
fn a_quest_timed_in_its_first_phase_only_cannot_fail_after_it() {
    let mut h = timed_in(1);
    let (b, me) = h.player_in_world("one", "Elara");
    let from = use_explorer(&mut h, b, me);
    let left = time_left(&records(&h, b, from)[0]).expect("timed");
    assert!((left - 60.0).abs() < 2.0, "{left} s left");
    let from = use_explorer(&mut h, b, me);
    let second = records(&h, b, from);
    assert_eq!(second[0].phase, 2);
    assert_eq!(time_left(&second[0]), None, "no time at its next phase");
    let t = h.now.0 + 70.0;
    h.run_to(yalain_net::LocalTime(t));
    assert_eq!(
        held(&h, HALL_QUEST),
        Some((status::UNDERWAY, 2)),
        "not lost"
    );
}

/// A quest timed in a later phase shows no time before it, and cannot fail then. As that phase comes
/// the whole of its time starts, with when the quest was given unchanged, and its running out fails
/// the quest.
/// Behaviour: server.quests.timed-in-one-phase
#[test]
fn a_quest_timed_in_a_later_phase_counts_from_when_that_phase_comes() {
    let mut h = timed_in(2);
    let (b, me) = h.player_in_world("one", "Elara");
    let from = use_explorer(&mut h, b, me);
    let first = records(&h, b, from);
    assert_eq!(time_left(&first[0]), None, "no time yet");
    let t = h.now.0 + 70.0;
    h.run_to(yalain_net::LocalTime(t));
    assert_eq!(
        held(&h, HALL_QUEST),
        Some((status::UNDERWAY, 1)),
        "not lost"
    );
    let from = use_explorer(&mut h, b, me);
    let second = records(&h, b, from);
    assert_eq!(second[0].phase, 2);
    let left = time_left(&second[0]).expect("timed now");
    assert!((left - 60.0).abs() < 2.0, "the whole of it: {left} s");
    assert_eq!(
        second[0].bestowed, first[0].bestowed,
        "when it was given is unchanged"
    );
    let t = h.now.0 + 66.0;
    h.run_to(yalain_net::LocalTime(t));
    assert_eq!(held(&h, HALL_QUEST), Some((status::FAILED, 0)));
}

/// A move of a subquest in the phase a quest is timed in does not start its time again.
/// Behaviour: server.quests.timed-in-one-phase
#[test]
fn a_subquests_move_leaves_a_timed_phases_time_running() {
    const SUB: u32 = 0x4000_0F01;
    const OTHER_SUB: u32 = 0x4000_0F02;
    const ELSEWHERE: u32 = 0x4700_0F01;
    let timed = QuestDef {
        duration: 60.0,
        timed_phase: 2,
        retry: 3600.0,
        subquests: vec![yalain_content::quests::Subquests {
            phase: 2,
            quests: vec![SUB, OTHER_SUB],
            any: false,
        }],
        ..quest(HALL_QUEST, 0, -999)
    };
    let hidden = |id| QuestDef {
        duration: 0.0,
        hidden: true,
        ..quest(id, 0, -999)
    };
    let explorer = Trigger::Use(EXPLORER_TEMPLATE);
    let mut h = explorer_world(
        vec![timed, hidden(SUB), hidden(OTHER_SUB)],
        vec![
            step(HALL_QUEST, 0, explorer, Update::Bestow),
            step(HALL_QUEST, 1, explorer, Update::Advance(2)),
            step(SUB, 1, Trigger::Use(ELSEWHERE), Update::Complete),
        ],
    );
    let (b, me) = h.player_in_world("one", "Elara");
    use_explorer(&mut h, b, me);
    let from = use_explorer(&mut h, b, me);
    let began = time_left(&records(&h, b, from)[0]).expect("timed");
    let t = h.now.0 + 30.0;
    h.run_to(yalain_net::LocalTime(t));
    let from = h.bots[b].inbox.len();
    let world = *h.server.players.keys().next().expect("the player");
    let clock = h.clock();
    yalain_world::quests::triggered(
        &mut h.server,
        &mut h.net,
        world,
        Trigger::Use(ELSEWHERE),
        None,
        clock,
    );
    h.run(20);
    let moved = records(&h, b, from);
    assert_eq!(moved[0].phase, 2, "its phase kept");
    let left = time_left(&moved[0]).expect("still timed");
    assert!(
        (began - left - 31.0).abs() < 2.0,
        "{began} s to begin with, {left} s now"
    );
}

/// A quest moved on past the phase its duration runs in is not failed by that phase's timer.
/// Behaviour: server.quests.timed-in-one-phase
#[test]
fn a_quest_moved_on_past_its_timed_phase_keeps_going() {
    let mut h = timed_in(2);
    let (b, me) = h.player_in_world("one", "Elara");
    use_explorer(&mut h, b, me);
    use_explorer(&mut h, b, me);
    let from = use_explorer(&mut h, b, me);
    assert_eq!(time_left(&records(&h, b, from)[0]), None);
    let t = h.now.0 + 70.0;
    h.run_to(yalain_net::LocalTime(t));
    assert_eq!(
        held(&h, HALL_QUEST),
        Some((status::UNDERWAY, 3)),
        "not lost"
    );
}

/// A use of a person or an object whose step leads somewhere tells its update and takes the player
/// in the same instant: first the update, then portal space, then the place. A step that does not
/// lead on, and a use its step does not run for, take no one.
/// Behaviour: server.quests.a-use-that-leads-on
#[test]
fn a_persons_step_that_leads_somewhere_tells_its_update_then_takes_the_player() {
    const THERE: u32 = 0x6CB9_0029;
    let mut h = explorer_world(
        vec![quest(HALL_QUEST, 0, -999)],
        vec![
            Step {
                to: Some(Place {
                    cell: THERE,
                    origin: [100.0, 20.0, 50.0],
                    heading: 0,
                }),
                ..step(
                    HALL_QUEST,
                    0,
                    Trigger::Use(EXPLORER_TEMPLATE),
                    Update::Bestow,
                )
            },
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
    assert_eq!(updates(&h, b, from), vec![0x1000_0000], "told at once");
    let until = h.now.0 + 1.0;
    while h.now.0 < until {
        h.step();
    }
    let calls: Vec<u32> = calls_of(&h, b, from).into_iter().map(|(f, _)| f).collect();
    let told = calls.iter().position(|f| *f == UPDATE_QUEST);
    let went = calls.iter().position(|f| *f == ENTER_PORTAL_SPACE);
    assert!(
        told.is_some() && went.is_some() && told < went,
        "the update, then portal space, within a second: {calls:08X?}"
    );
    let until = h.now.0 + 12.0;
    while h.now.0 < until {
        h.step();
    }
    let p = h.server.players.values().next().expect("a player");
    assert_eq!(p.position.cell, THERE, "at the place");
    assert!(p.trip.is_none(), "the trip is over");
    p_walk_back(&mut h);
    let from = use_explorer(&mut h, b, me);
    assert_eq!(updates(&h, b, from), vec![0x3000_0000], "its last step");
    let until = h.now.0 + 12.0;
    while h.now.0 < until {
        h.step();
    }
    assert!(
        !calls_of(&h, b, from)
            .iter()
            .any(|(f, _)| *f == ENTER_PORTAL_SPACE),
        "a step that does not lead on takes no one"
    );
}

/// Back to where the explorer stands.
fn p_walk_back(h: &mut Harness) {
    let at = h.server.content.objects[0].clone();
    for p in h.server.players.values_mut() {
        p.position.cell = at.cell;
        p.position.origin = at.origin;
    }
}

/// A step may require another quest free to be given: never had, or its wait over; the quest
/// done with its wait running, or under way, holds the step back.
/// Behaviour: server.quests.a-step-requires-a-quest-free
#[test]
fn a_step_that_requires_a_quest_free_waits_for_that_quests_wait() {
    let timer = QuestDef {
        retry: 3600.0,
        ..quest(NEXT_QUEST, 0, -999)
    };
    let mut h = explorer_world(
        vec![quest(HALL_QUEST, 0, -999), timer],
        vec![Step {
            requires: vec![(NEXT_QUEST, Need::Free)],
            ..step(
                HALL_QUEST,
                0,
                Trigger::Use(EXPLORER_TEMPLATE),
                Update::Bestow,
            )
        }],
    );
    let (b, me) = h.player_in_world("one", "Elara");
    let now = h.unix;
    let set = |h: &mut Harness, st: u32, updated: i64| {
        for p in h.server.players.values_mut() {
            p.play.quests.clear();
            p.play.quests.push(QuestState {
                quest: NEXT_QUEST,
                status: st,
                phase: u32::from(st == status::UNDERWAY),
                started: 0,
                updated,
            });
        }
    };
    set(&mut h, status::COMPLETED, now - 60);
    let from = use_explorer(&mut h, b, me);
    assert!(updates(&h, b, from).is_empty(), "its wait is running");
    set(&mut h, status::UNDERWAY, now - 60);
    let from = use_explorer(&mut h, b, me);
    assert!(updates(&h, b, from).is_empty(), "it is under way");
    set(&mut h, status::COMPLETED, now - 7200);
    let from = use_explorer(&mut h, b, me);
    assert_eq!(updates(&h, b, from), vec![0x1000_0000], "its wait is over");
}

/// A step may fail its quest: a quest under way fails, told as a failure, and with no wait of its
/// own is given again by the next use.
/// Behaviour: server.quests.a-step-may-fail-its-quest
#[test]
fn a_step_may_fail_its_quest_and_it_may_then_be_given_again() {
    let mut h = explorer_world(
        vec![quest(HALL_QUEST, 0, -999)],
        vec![
            step(
                HALL_QUEST,
                0,
                Trigger::Use(EXPLORER_TEMPLATE),
                Update::Bestow,
            ),
            step(HALL_QUEST, 1, Trigger::Use(EXPLORER_TEMPLATE), Update::Fail),
        ],
    );
    let (b, me) = h.player_in_world("one", "Elara");
    let from = use_explorer(&mut h, b, me);
    assert_eq!(updates(&h, b, from), vec![0x1000_0000]);
    let from = use_explorer(&mut h, b, me);
    assert_eq!(updates(&h, b, from), vec![0x4000_0000], "failed and told");
    assert_eq!(held(&h, HALL_QUEST), Some((status::FAILED, 0)));
    let from = use_explorer(&mut h, b, me);
    assert_eq!(updates(&h, b, from), vec![0x1000_0000], "given again");
    assert_eq!(held(&h, HALL_QUEST), Some((status::UNDERWAY, 1)));
}

/// A player of this level, in a world holding these steps.
fn stepping(steps: Vec<Step>, level: i32) -> Harness {
    let mut h = Harness::new();
    h.server.levels = level_costs(&[(1, 1_000), (30, 100_000)]);
    let mut q = quest(HALL_QUEST, 0, -999);
    q.retry = 0.0;
    let mut other = quest(HALL_QUEST + 1, 0, -999);
    other.retry = 0.0;
    h.server.quests = QuestTable {
        quests: vec![q, other],
        steps,
        gives: Vec::new(),
        rooms: Vec::new(),
    };
    let _ = h.player_in_world("one", "Elara");
    h.run(5);
    for p in h.server.players.values_mut() {
        yalain_world::quals::set_int(&mut p.qualities, yalain_world::progress::q::LEVEL, level);
    }
    h
}

fn use_person(h: &mut Harness) {
    let world = *h.server.players.keys().next().expect("in the world");
    let clock = h.clock();
    yalain_world::quests::triggered(
        &mut h.server,
        &mut h.net,
        world,
        Trigger::Use(EXPLORER_TEMPLATE),
        None,
        clock,
    );
}

fn phases(h: &Harness) -> Vec<(u32, u32)> {
    let p = h.server.players.values().next().expect("in the world");
    p.play.quests.iter().map(|q| (q.quest, q.phase)).collect()
}

/// A person's quests by level: a player of a level is given the one for that level.
/// Behaviour: server.quests.step-levels
#[test]
fn a_step_for_some_levels_runs_for_those_alone() {
    let band = |q: u32, levels: (i32, i32)| Step {
        levels: Some(levels),
        ..step(q, 0, Trigger::Use(EXPLORER_TEMPLATE), Update::Bestow)
    };
    let given = |level: i32| {
        let mut h = stepping(
            vec![band(HALL_QUEST, (20, 24)), band(HALL_QUEST + 1, (25, 29))],
            level,
        );
        use_person(&mut h);
        phases(&h)
    };
    assert_eq!(given(19), vec![], "under the least");
    assert_eq!(given(20), vec![(HALL_QUEST, 1)]);
    assert_eq!(given(24), vec![(HALL_QUEST, 1)]);
    assert_eq!(given(25), vec![(HALL_QUEST + 1, 1)]);
    assert_eq!(given(30), vec![], "over the most");
}

/// A step that moves no quest gives what it gives and leaves the quest as it was.
/// Behaviour: server.quests.step-levels
#[test]
fn a_step_that_moves_no_quest_gives_its_item() {
    let mut h = stepping(
        vec![Step {
            gives: vec![TEST_ITEM],
            ..step(
                HALL_QUEST,
                0,
                Trigger::Use(EXPLORER_TEMPLATE),
                Update::Nothing,
            )
        }],
        25,
    );
    h.server.catalog = test_catalog();
    let world = *h.server.players.keys().next().expect("in the world");
    assert!(!yalain_world::inventory::holds(&h.server, world, TEST_ITEM));
    use_person(&mut h);
    assert!(yalain_world::inventory::holds(&h.server, world, TEST_ITEM));
    assert_eq!(phases(&h), vec![], "no quest moved");
}

/// A person's gift kept to one in a wait by a quest done at once: it gives one at a use, none again
/// until the quest's wait is over, however little of it the player keeps.
/// Behaviour: server.quests.practice-gems
#[test]
fn a_gift_a_quest_done_at_once_keeps_comes_once_in_its_wait() {
    let mut h = stepping(
        vec![Step {
            gives: vec![TEST_ITEM],
            ..step(
                HALL_QUEST,
                0,
                Trigger::Use(EXPLORER_TEMPLATE),
                Update::Complete,
            )
        }],
        25,
    );
    h.server.catalog = test_catalog();
    h.server.quests.quests[0].retry = 604_800.0;
    let items = |h: &Harness| {
        let p = h.server.players.values().next().expect("in the world");
        p.play.items.len()
    };
    let before = items(&h);
    use_person(&mut h);
    assert_eq!(items(&h), before + 1, "the first");
    let p = h.server.players.values().next().expect("in the world");
    assert_eq!(p.play.quests[0].status, status::COMPLETED);
    use_person(&mut h);
    assert_eq!(items(&h), before + 1, "none in the wait");
    let long_ago = h.server.players.values_mut().next().expect("in the world");
    long_ago.play.quests[0].updated -= 604_801;
    long_ago.play.quests[0].started -= 604_801;
    use_person(&mut h);
    assert_eq!(items(&h), before + 2, "another after it");
}

/// A step that fails its quest pays a share of what the player's level takes.
/// Behaviour: server.quests.step-share
#[test]
fn a_failing_step_pays_its_share_of_the_level() {
    let mut h = stepping(
        vec![Step {
            xp_share: 0.14,
            ..step(HALL_QUEST, 4, Trigger::Use(EXPLORER_TEMPLATE), Update::Fail)
        }],
        30,
    );
    at_phase(&mut h, HALL_QUEST, 4);
    let total = |h: &Harness| {
        let p = h.server.players.values().next().expect("in the world");
        yalain_world::quals::long(&p.qualities, yalain_world::progress::q::TOTAL_XP).unwrap_or(0)
    };
    let before = total(&h);
    use_person(&mut h);
    assert_eq!(total(&h) - before, 14_000);
    let p = h.server.players.values().next().expect("in the world");
    assert_eq!(p.play.quests[0].status, status::FAILED);
}
