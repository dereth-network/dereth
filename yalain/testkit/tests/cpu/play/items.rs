//! Items used from the pack for the effects their templates name: a potion drunk (its behavior
//! played, then its effects, the potion spent and the answer at once), a charm used at once off
//! its stack, an egg's mount that lasts until taken off, is never spent and costs focus, and
//! nothing else done while a potion is drunk; the uses refused (moving, a mount with full hands
//! or on a mount, far from the item's place, without its quest, a hidden one's as mysterious,
//! below its level), before any quest step its use moves; a reward quest done with the quest it
//! rewards; an item only looked at.
//! Fixture: the test world with made-up potion, charm, egg, rod, token and trophy templates and
//! their effects as the data defines them.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_content::quests::{QuestDef, QuestTable};
use yalain_content::{Prop, Props};
use yalain_testkit::play::server_call;
use yalain_testkit::Harness;
use yalain_world::calls::{self, Node, Value};
use yalain_world::effect_defs::{EffectDef, Kind, Scale};
use yalain_world::effects::{record_flag, speed};
use yalain_world::item_use::{self, flags, refusal, UsageEffect};
use yalain_world::quests::status;
use yalain_world::state::QuestState;

const POTION: u32 = 0x4700_11B1;
const CHARM: u32 = 0x4700_190C;
const EGG: u32 = 0x4700_14CC;
const OVER_TIME: u32 = 0x6F00_2659;
const TENACITY: u32 = 0x6F00_2665;
const LUCK: u32 = 0x6F00_1F1F;
const MOUNT: u32 = 0x6F00_1C4C;
const FOCUS_COST: u32 = 0x6F00_1C4D;
const ROD: u32 = 0x4700_14EF;
const TOKEN: u32 = 0x4700_1C8B;
const TROPHY: u32 = 0x4700_0607;
const TOKEN_QUEST: u32 = 0x4000_02D4;
const FOCUS: u32 = 266;
const LEVEL: u32 = 302;
/// The rod's place, its radius and its table of messages.
const ROD_CELL: u32 = 0x6969_000C;
const ROD_TABLE: u32 = 0x2500_20AB;
const USE: u32 = 0x8216_0016;
const USE_DONE: u32 = 0x8216_0021;
const ITEM_NAME: (u32, u32) = (0xA860_7CEC, 0x2500_1E8E);

fn def(state: u32, kind: Kind, seconds: Option<f32>) -> EffectDef {
    EffectDef {
        state,
        kind,
        duration: Scale(seconds.map(|s| vec![(0.0, s)]).unwrap_or_default()),
        priority: Scale::default(),
        fx: 0,
        external_flags: 0,
        internal_flags: 0,
        enum_value: 0,
        eq_class: 0,
        name: None,
    }
}

fn template(h: &mut Harness, id: u32, props: Props) {
    let mut t = yalain_testkit::test_template(id, 0x3A1, props);
    t.description.name = Some(ITEM_NAME);
    h.server.catalog.insert(t);
}

/// The test world with a potion (a health over time and a greater health, destroyed on use), a
/// charm (luck, used at once, stacking, destroyed on use) and an egg (a mount of no duration).
fn world() -> Harness {
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    let mut potion = Props::default();
    potion
        .set("Item_UsageAction", Prop::Word(item_use::POTION_ACTION))
        .set("Item_IsDestroyOnUse", Prop::Bool(true));
    template(&mut h, POTION, potion);
    let mut charm = Props::default();
    charm
        .set("Item_UsageAction", Prop::Word(0x1F))
        .set("Item_IsDestroyOnUse", Prop::Bool(true))
        .set("Item_IsStackable", Prop::Bool(true))
        .set("Item_MaxStackSize", Prop::Int(1000));
    template(&mut h, CHARM, charm);
    let mut egg = Props::default();
    egg.set("Item_UsageAction", Prop::Word(item_use::MOUNT_ACTION))
        .set("Item_CanUseWhileMoving", Prop::Bool(false))
        .set("Usage_MinLevel", Prop::Int(51));
    template(&mut h, EGG, egg);
    // A rod used only within 5 m of its place, telling how far it is; not while moving; letting
    // its user go for its effects.
    let mut rod = Props::default();
    rod.set("Item_UsageAction", Prop::Word(0x1F))
        .set("Item_CanUseWhileMoving", Prop::Bool(false))
        .set("Usage_ShouldUnlockUserForUsageEffects", Prop::Bool(true))
        .set(
            "Usage_RequiredLocation",
            Prop::Place(ROD_CELL, [32.5, 63.1, 95.2]),
        )
        .set("Usage_RequiredLocationRadius", Prop::Float(5.0))
        .set("Usage_RequiredLocationFeedbackType", Prop::Word(2))
        .set("Usage_ErrorMessagesTableID", Prop::Word(ROD_TABLE));
    template(&mut h, ROD, rod);
    // A token used only while its quest is under way.
    let mut token = Props::default();
    token
        .set("Item_UsageAction", Prop::Word(0x1F))
        .set("Usage_RequiredQuest", Prop::Word(TOKEN_QUEST))
        .set("Usage_RequiredQuestStatus", Prop::Word(status::UNDERWAY));
    template(&mut h, TOKEN, token);
    let mut trophy = Props::default();
    trophy.set("Item_UsageAction", Prop::Word(item_use::EXAMINE_ACTION));
    template(&mut h, TROPHY, trophy);
    h.server.quests = QuestTable {
        quests: vec![QuestDef {
            id: TOKEN_QUEST,
            name: (0xA860_7CEC, 0x2500_17EE),
            description: (0x6C45_9364, 0x2500_17EE),
            icon: 0,
            play_fx: false,
            journals: Vec::new(),
            challenge: -999,
            duration: 0.0,
            timed_phase: 0,
            retry: 0.0,
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
        }],
        steps: Vec::new(),
        gives: Vec::new(),
        rooms: Vec::new(),
    };
    let effect = |state, spellcraft| UsageEffect { state, spellcraft };
    h.server.item_uses.insert(
        POTION,
        vec![effect(OVER_TIME, 50.0), effect(TENACITY, 100.0)],
    );
    h.server.item_uses.insert(CHARM, vec![effect(LUCK, 1.0)]);
    h.server
        .item_uses
        .insert(EGG, vec![effect(MOUNT, 1.0), effect(FOCUS_COST, 1.0)]);
    h.server.item_uses.insert(ROD, vec![effect(LUCK, 1.0)]);
    h.server.item_uses.insert(TOKEN, vec![effect(LUCK, 1.0)]);
    for d in [
        def(
            OVER_TIME,
            Kind::VitalOverTime {
                flags: 0x1,
                per_second: Scale(vec![(0.0, 2.0)]),
            },
            Some(120.0),
        ),
        def(
            TENACITY,
            Kind::Numeric {
                flags: 0x4,
                stats: vec![0x4_0103],
                magnitude: Scale(vec![(0.0, 50.0)]),
            },
            Some(300.0),
        ),
        def(
            LUCK,
            Kind::Numeric {
                flags: 0x4,
                stats: vec![0x4_0125],
                magnitude: Scale(vec![(0.0, 20.0)]),
            },
            Some(900.0),
        ),
        def(
            MOUNT,
            Kind::Numeric {
                flags: 0x9,
                stats: vec![speed::VELOCITY],
                magnitude: Scale(vec![(0.0, 1.9)]),
            },
            None,
        ),
        def(
            FOCUS_COST,
            Kind::InstantVital {
                flags: 0xC,
                change: Scale(vec![(0.0, -5.0)]),
            },
            None,
        ),
    ] {
        h.server.effect_table.effects.insert(d.state, d);
    }
    h
}

/// Set a player's level.
fn at_level(h: &mut Harness, world: yalain_net::ConnId, level: i32) {
    let p = h.server.players.get_mut(&world).expect("in the world");
    yalain_world::quals::set_int(&mut p.qualities, LEVEL, level);
}

/// The focus values a client was sent since `from`.
fn focus_sent(h: &Harness, b: usize, from: usize) -> Vec<i32> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::UpdateInt(q) if q.quality == FOCUS => Some(q.value),
            _ => None,
        })
        .collect()
}

/// Put a player somewhere, standing still.
fn stand_at(h: &mut Harness, world: yalain_net::ConnId, cell: u32, origin: [f32; 3]) {
    let p = h.server.players.get_mut(&world).expect("in the world");
    p.position.cell = cell;
    p.position.origin = origin;
}

/// A player of the test world in it, its scene drawn; the client, the player, its connection.
fn in_world(h: &mut Harness) -> (usize, u64, yalain_net::ConnId) {
    let (b, me) = h.player_in_world("one", "Elara");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in the world")
        .0;
    // Outdoors: the test world starts its players in a hall, and a mount is refused indoors.
    let p = h.server.players.get_mut(&world).expect("in the world");
    p.position.cell = p.position.cell & 0xFFFF_0000 | 0x0001;
    (b, me, world)
}

/// Give a player so many of a template; the item's id.
fn give(h: &mut Harness, world: yalain_net::ConnId, template: u32, n: u32) -> u64 {
    yalain_world::inventory::give_stacked(&mut h.server, &mut h.net, world, template, n);
    h.run(3);
    held(h, world, template).expect("given").0
}

/// The item of a template a player holds: its id and quantity.
fn held(h: &Harness, world: yalain_net::ConnId, template: u32) -> Option<(u64, u32)> {
    h.server.players[&world]
        .play
        .items
        .iter()
        .find(|i| i.template == template)
        .map(|i| (i.id, i.quantity.max(1)))
}

/// A use of an item, as the client asks for it.
fn use_of(item: u64, me: u64) -> Arg {
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

/// Ask to use an item and run the world `seconds` on; the inbox mark before.
fn send_use(h: &mut Harness, b: usize, me: u64, item: u64, seconds: f64) -> usize {
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &server_call(USE, &[use_of(item, me)]));
    run_for(h, seconds);
    from
}

fn run_for(h: &mut Harness, seconds: f64) {
    let t = h.now.0 + seconds;
    while h.now.0 < t {
        h.step();
    }
}

/// An answer to a use: its status, control flags and message's string id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Answer {
    status: u32,
    flags: u32,
    text: u32,
}

/// The use answers a client was sent since `from`.
fn answers(h: &Harness, b: usize, from: usize) -> Vec<Answer> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) if c.function.0 == USE_DONE => {
                let v = c.args.as_ref().and_then(|a| calls::values(&a.0).ok())?;
                let Some(Value::Graph(g)) = v.first() else {
                    return None;
                };
                let slots = g.iter().find_map(|(_, n)| match n {
                    Node::Script { slots, .. } => Some(slots.clone()),
                    _ => None,
                })?;
                let text = g.iter().find_map(|(_, n)| match n {
                    Node::TextInfo(t) => Some(t.string_id),
                    _ => None,
                });
                Some(Answer {
                    status: slots[11].1,
                    flags: slots[17].1,
                    text: text.unwrap_or(0),
                })
            }
            _ => None,
        })
        .collect()
}

/// The records a player holds from an item.
fn records_from(
    h: &Harness,
    world: yalain_net::ConnId,
    item: u64,
) -> Vec<yalain_world::state::EffectState> {
    h.server.players[&world]
        .play
        .effects
        .iter()
        .filter(|e| e.item == item)
        .copied()
        .collect()
}

/// A potion used from the pack: the drink played with the potion in hand for all to see, nothing
/// more until it has played out (4.24 s), then at once its two effects on the drinker at their
/// spellcraft, the potion gone, and "You drink {the} %1.".
/// Behaviour: server.items.a-potion-is-drunk-then-takes-effect
#[test]
fn a_potion_is_drunk_and_at_the_drinks_end_takes_effect_and_is_spent() {
    let mut h = world();
    let (b, me, world) = in_world(&mut h);
    let potion = give(&mut h, world, POTION, 1);
    let from = send_use(&mut h, b, me, potion, 3.5);
    let drink = h.world_messages(b, from).iter().find_map(|m| match m {
        ServerMessage::PlayBehavior(p) if p.action.behavior == item_use::DRINK => {
            Some(p.action.clone())
        }
        _ => None,
    });
    let drink = drink.expect("the drink is played");
    assert_eq!(drink.target, potion, "toward the potion");
    assert_eq!(drink.bits, item_use::BEHAVIOR_BITS);
    assert!(
        answers(&h, b, from).is_empty(),
        "nothing before the drink's end"
    );
    assert!(records_from(&h, world, potion).is_empty());
    assert!(held(&h, world, POTION).is_some());
    run_for(&mut h, 1.0);
    assert_eq!(
        answers(&h, b, from),
        vec![Answer {
            status: 0,
            flags: flags::PLAYED | flags::SPENT,
            text: item_use::text::DRINK.0,
        }]
    );
    let recs = records_from(&h, world, potion);
    let states: Vec<(u32, f32)> = recs.iter().map(|e| (e.state, e.spellcraft)).collect();
    assert_eq!(states, vec![(OVER_TIME, 50.0), (TENACITY, 100.0)]);
    assert!(recs.iter().all(|e| e.caster == me && e.flags == 66));
    assert!(held(&h, world, POTION).is_none(), "the potion is gone");
}

/// A charm with no use behavior acts in the request's instant: its luck put on, one off its stack,
/// the answer keeping the request's message with the flags of a use at once that spent it.
/// Behaviour: server.items.an-item-without-a-use-behavior-acts-at-once
#[test]
fn a_charm_without_a_use_behavior_acts_at_once_and_goes_one_off_its_stack() {
    let mut h = world();
    let (b, me, world) = in_world(&mut h);
    let charm = give(&mut h, world, CHARM, 2);
    let from = send_use(&mut h, b, me, charm, 0.2);
    assert_eq!(
        answers(&h, b, from),
        vec![Answer {
            status: 0,
            flags: flags::AT_ONCE | flags::SPENT,
            text: 0,
        }]
    );
    assert!(h
        .world_messages(b, from)
        .iter()
        .all(|m| !matches!(m, ServerMessage::PlayBehavior(p) if p.action.target == charm)));
    assert_eq!(records_from(&h, world, charm).len(), 1);
    assert_eq!(
        held(&h, world, CHARM),
        Some((charm, 1)),
        "one off the stack"
    );
}

/// An egg (an item not destroyed on use) is kept; its mount, an effect of no duration, is a record
/// that never runs out and goes at logout, and its speed is sent.
/// Behaviour: server.items.an-effect-without-a-duration-lasts-until-taken-off
#[test]
fn an_eggs_mount_lasts_until_taken_off_its_speed_sent_and_the_egg_kept() {
    let mut h = world();
    let (b, me, world) = in_world(&mut h);
    at_level(&mut h, world, 60);
    let egg = give(&mut h, world, EGG, 1);
    let from = send_use(&mut h, b, me, egg, 0.2);
    assert_eq!(
        answers(&h, b, from),
        vec![Answer {
            status: 0,
            flags: flags::MOUNT,
            text: 0,
        }]
    );
    let recs = records_from(&h, world, egg);
    assert_eq!(recs.len(), 1);
    assert_eq!(recs[0].timeout, f32::MAX);
    assert_eq!(
        recs[0].flags,
        record_flag::INFINITE | record_flag::LOGOUT | record_flag::TRANSIENT
    );
    let sped = h
        .world_messages(b, from)
        .iter()
        .any(|m| matches!(m, ServerMessage::VelocityScale(v) if (v.action.0 - 1.9).abs() < 1e-4));
    assert!(sped, "the mount's speed is sent");
    assert!(held(&h, world, EGG).is_some(), "the egg is kept");
    run_for(&mut h, 30.0);
    assert_eq!(records_from(&h, world, egg).len(), 1, "it does not run out");
}

/// While a potion is drunk another use is refused (1029, the answer alone, nothing done); the
/// potion still takes effect at the drink's end.
/// Behaviour: server.items.nothing-else-while-drinking
#[test]
fn another_use_while_a_potion_is_drunk_is_refused_and_does_nothing() {
    let mut h = world();
    let (b, me, world) = in_world(&mut h);
    let potion = give(&mut h, world, POTION, 1);
    let charm = give(&mut h, world, CHARM, 2);
    send_use(&mut h, b, me, potion, 1.0);
    let from = send_use(&mut h, b, me, charm, 0.2);
    assert_eq!(
        answers(&h, b, from)
            .iter()
            .map(|a| a.status)
            .collect::<Vec<_>>(),
        vec![1029]
    );
    assert!(records_from(&h, world, charm).is_empty());
    assert_eq!(held(&h, world, CHARM), Some((charm, 2)));
    run_for(&mut h, 4.0);
    assert_eq!(
        records_from(&h, world, potion).len(),
        2,
        "the drink still ends"
    );
}

/// An egg's use takes five from its user's focus as its mount goes on, and five again at the next
/// use; the focus is sent each time.
/// Behaviour: server.items.an-eggs-mount-costs-focus
#[test]
fn an_eggs_use_takes_five_from_the_focus() {
    let mut h = world();
    let (b, me, world) = in_world(&mut h);
    at_level(&mut h, world, 60);
    let egg = give(&mut h, world, EGG, 1);
    let from = send_use(&mut h, b, me, egg, 0.2);
    assert_eq!(focus_sent(&h, b, from), vec![95]);
    h.server
        .players
        .get_mut(&world)
        .expect("in the world")
        .play
        .effects
        .clear();
    let from = send_use(&mut h, b, me, egg, 0.2);
    assert_eq!(focus_sent(&h, b, from), vec![90]);
}

/// An item that may not be used moving is refused while its user moves (10055), its answer the
/// flags its use would have, nothing done; standing still, the same use goes through.
/// Behaviour: server.items.not-while-moving
#[test]
fn an_egg_used_moving_is_refused_and_does_nothing() {
    let mut h = world();
    let (b, me, world) = in_world(&mut h);
    at_level(&mut h, world, 60);
    let egg = give(&mut h, world, EGG, 1);
    h.server
        .players
        .get_mut(&world)
        .expect("in the world")
        .heading_motion = yalain_world::world::motion(127, 255, 127);
    let from = send_use(&mut h, b, me, egg, 0.2);
    assert_eq!(
        answers(&h, b, from),
        vec![Answer {
            status: refusal::MOVING,
            flags: flags::MOUNT,
            text: 0,
        }]
    );
    assert!(records_from(&h, world, egg).is_empty());
    assert!(focus_sent(&h, b, from).is_empty());
    h.server
        .players
        .get_mut(&world)
        .expect("in the world")
        .heading_motion = yalain_world::world::IDLE_MOTION;
    let from = send_use(&mut h, b, me, egg, 0.2);
    assert_eq!(answers(&h, b, from)[0].status, 0);
}

/// A mount's item is refused while anything is in its user's hands (10045), and while a mount is
/// already on (10047); nothing is done either time.
/// Behaviour: server.items.a-mount-needs-empty-hands-and-no-mount
#[test]
fn a_mount_is_refused_with_full_hands_and_on_a_mount() {
    let mut h = world();
    let (b, me, world) = in_world(&mut h);
    at_level(&mut h, world, 60);
    let egg = give(&mut h, world, EGG, 1);
    let charm = give(&mut h, world, CHARM, 1);
    let set_hands = |h: &mut Harness, at: u32| {
        let p = h.server.players.get_mut(&world).expect("in the world");
        p.play
            .items
            .iter_mut()
            .find(|i| i.id == charm)
            .expect("held")
            .location = at;
    };
    set_hands(&mut h, 0x40);
    let from = send_use(&mut h, b, me, egg, 0.2);
    assert_eq!(
        answers(&h, b, from)
            .iter()
            .map(|a| (a.status, a.flags))
            .collect::<Vec<_>>(),
        vec![(refusal::HANDS_NOT_EMPTY, flags::MOUNT)]
    );
    assert!(records_from(&h, world, egg).is_empty());
    set_hands(&mut h, 0);
    let from = send_use(&mut h, b, me, egg, 0.2);
    assert_eq!(answers(&h, b, from)[0].status, 0);
    let from = send_use(&mut h, b, me, egg, 0.2);
    assert_eq!(
        answers(&h, b, from)
            .iter()
            .map(|a| a.status)
            .collect::<Vec<_>>(),
        vec![refusal::MOUNTED]
    );
    assert_eq!(records_from(&h, world, egg).len(), 1, "no second mount");
    assert!(focus_sent(&h, b, from).is_empty());
}

/// An item used farther from its place than its radius is refused (10054) with the flags its use
/// would have but for being carried out, and the item says the player is outside its bands; within
/// the radius it is carried out.
/// Behaviour: server.items.used-only-near-its-place
#[test]
fn an_item_used_far_from_its_place_is_refused_and_says_so() {
    let mut h = world();
    let (b, me, world) = in_world(&mut h);
    let rod = give(&mut h, world, ROD, 1);
    let base = flags::AT_ONCE | flags::NOT_WHILE_MOVING | flags::UNLOCKS;
    stand_at(&mut h, world, ROD_CELL, [32.5, 69.1, 95.2]);
    let from = send_use(&mut h, b, me, rod, 0.2);
    assert_eq!(
        answers(&h, b, from),
        vec![Answer {
            status: refusal::NOT_THERE,
            flags: base & !flags::CARRIED_OUT,
            text: 0,
        }]
    );
    assert_eq!(
        h.texts(b, from),
        vec![(1, yalain_world::usable::maps::text::OUTSIDE)]
    );
    assert!(records_from(&h, world, rod).is_empty());
    stand_at(&mut h, world, ROD_CELL, [33.5, 61.1, 95.2]);
    let from = send_use(&mut h, b, me, rod, 0.2);
    assert_eq!(
        answers(&h, b, from)[0],
        Answer {
            status: 0,
            flags: base,
            text: 0,
        }
    );
    assert_eq!(records_from(&h, world, rod).len(), 1);
}

/// An item whose use moves a quest is held to its own requirements first: away from its place it
/// is refused (10054) as any item is, told how far, and the quest is not moved; at its place the
/// use moves the quest.
/// Behaviour: server.items.place-before-its-quest-step
#[test]
fn a_rod_whose_use_moves_a_quest_is_refused_away_from_its_place_and_moves_it_there() {
    use yalain_content::quests::{Step, Trigger, Update};
    let mut h = world();
    h.server.quests.steps.push(Step {
        quest: TOKEN_QUEST,
        needs: 0,
        trigger: Trigger::Use(ROD),
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
        races: Vec::new(),
        effects_last: false,
        wants: None,
        fellows: false,
        levels: None,
        xp_share: 0.0,
        to: None,
    });
    let (b, me, world) = in_world(&mut h);
    let rod = give(&mut h, world, ROD, 1);
    let quests = |h: &Harness| -> Vec<u32> {
        h.server.players[&world]
            .play
            .quests
            .iter()
            .map(|q| q.quest)
            .collect()
    };
    stand_at(&mut h, world, ROD_CELL, [32.5, 69.1, 95.2]);
    let from = send_use(&mut h, b, me, rod, 0.2);
    let base = flags::AT_ONCE | flags::NOT_WHILE_MOVING | flags::UNLOCKS;
    assert_eq!(
        answers(&h, b, from),
        vec![Answer {
            status: refusal::NOT_THERE,
            flags: base & !flags::CARRIED_OUT,
            text: 0,
        }]
    );
    assert_eq!(
        h.texts(b, from),
        vec![(1, yalain_world::usable::maps::text::OUTSIDE)]
    );
    assert!(quests(&h).is_empty(), "the quest not moved");
    stand_at(&mut h, world, ROD_CELL, [33.5, 61.1, 95.2]);
    let from = send_use(&mut h, b, me, rod, 0.2);
    assert_eq!(answers(&h, b, from)[0].status, 0);
    assert_eq!(quests(&h), [TOKEN_QUEST], "moved at its place");
}

/// An item that needs its quest under way is refused when the player never had it (10019) and
/// when it is done (10020), and goes through while it is under way.
/// Behaviour: server.items.needs-its-quest
#[test]
fn an_item_that_needs_a_quest_under_way_is_refused_without_it() {
    let mut h = world();
    let (b, me, world) = in_world(&mut h);
    let token = give(&mut h, world, TOKEN, 1);
    let from = send_use(&mut h, b, me, token, 0.2);
    assert_eq!(answers(&h, b, from)[0].status, refusal::LACKING_QUEST);
    let set = |h: &mut Harness, st: u32| {
        h.server
            .players
            .get_mut(&world)
            .expect("in the world")
            .play
            .quests = vec![QuestState {
            quest: TOKEN_QUEST,
            status: st,
            phase: 1,
            started: 0,
            updated: 0,
        }];
    };
    set(&mut h, status::COMPLETED);
    let from = send_use(&mut h, b, me, token, 0.2);
    assert_eq!(answers(&h, b, from)[0].status, refusal::WRONG_QUEST_STATUS);
    assert!(records_from(&h, world, token).is_empty());
    set(&mut h, status::UNDERWAY);
    let from = send_use(&mut h, b, me, token, 0.2);
    assert_eq!(answers(&h, b, from)[0].status, 0);
    assert_eq!(records_from(&h, world, token).len(), 1);
}

/// An item that needs a hidden quest is refused as mysterious (10043), never as lacking the quest
/// or as at the wrong status; with the quest where it needs it, the item is used.
/// Behaviour: server.items.hidden-quest-refused-as-mysterious
#[test]
fn an_item_that_needs_a_hidden_quest_is_refused_as_mysterious() {
    let mut h = world();
    h.server.quests.quests[0].hidden = true;
    let (b, me, world) = in_world(&mut h);
    let token = give(&mut h, world, TOKEN, 1);
    let from = send_use(&mut h, b, me, token, 0.2);
    assert_eq!(
        answers(&h, b, from)[0].status,
        refusal::MYSTERIOUS_USE,
        "never had"
    );
    let set = |h: &mut Harness, st: u32| {
        h.server
            .players
            .get_mut(&world)
            .expect("in the world")
            .play
            .quests = vec![QuestState {
            quest: TOKEN_QUEST,
            status: st,
            phase: 1,
            started: 0,
            updated: 0,
        }];
    };
    set(&mut h, status::COMPLETED);
    let from = send_use(&mut h, b, me, token, 0.2);
    assert_eq!(
        answers(&h, b, from)[0].status,
        refusal::MYSTERIOUS_USE,
        "done"
    );
    assert!(records_from(&h, world, token).is_empty());
    set(&mut h, status::UNDERWAY);
    let from = send_use(&mut h, b, me, token, 0.2);
    assert_eq!(answers(&h, b, from)[0].status, 0);
    assert_eq!(records_from(&h, world, token).len(), 1);
}

/// A reward quest is completed with the quest it rewards: completing the rewarded quest tells that
/// completion alone, the hidden reward is held done with it, and an item that needs the reward done
/// (refused as mysterious before) is used.
/// Behaviour: server.quests.reward-done-with-its-quest
#[test]
fn a_reward_quest_is_completed_with_the_quest_it_rewards() {
    use yalain_content::quests::{Step, Trigger, Update};
    const REWARDED: u32 = 0x4000_0263;
    const REWARD: u32 = 0x4000_0269;
    let mut h = world();
    let base = h.server.quests.quests[0].clone();
    h.server.quests = QuestTable {
        quests: vec![
            QuestDef {
                id: REWARDED,
                ..base.clone()
            },
            QuestDef {
                id: REWARD,
                hidden: true,
                ..base
            },
        ],
        steps: vec![Step {
            quest: REWARD,
            needs: 0,
            trigger: Trigger::Done(REWARDED),
            tell: None,
            update: Update::Complete,
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
    let mut token = Props::default();
    token
        .set("Item_UsageAction", Prop::Word(0x1F))
        .set("Usage_RequiredQuest", Prop::Word(REWARD))
        .set("Usage_RequiredQuestStatus", Prop::Word(status::COMPLETED));
    template(&mut h, TOKEN, token);
    let (b, me, world) = in_world(&mut h);
    let token = give(&mut h, world, TOKEN, 1);
    let from = send_use(&mut h, b, me, token, 0.2);
    assert_eq!(answers(&h, b, from)[0].status, refusal::MYSTERIOUS_USE);
    h.server
        .players
        .get_mut(&world)
        .expect("in the world")
        .play
        .quests = vec![QuestState {
        quest: REWARDED,
        status: status::UNDERWAY,
        phase: 1,
        started: 0,
        updated: 0,
    }];
    let from = h.bots[b].inbox.len();
    let clock = h.clock();
    yalain_world::quests::apply(
        &mut h.server,
        &mut h.net,
        world,
        REWARDED,
        Update::Complete,
        0,
        clock,
    );
    run_for(&mut h, 0.2);
    let told: Vec<u32> = h
        .functions(b, from)
        .into_iter()
        .filter(|f| *f == yalain_world::quests::UPDATE_QUEST)
        .collect();
    assert_eq!(
        told.len(),
        1,
        "the rewarded quest's completion alone is told"
    );
    let held = |q: u32| {
        h.server.players[&world]
            .play
            .quests
            .iter()
            .find(|s| s.quest == q)
            .map(|s| s.status)
    };
    assert_eq!(held(REWARDED), Some(status::COMPLETED));
    assert_eq!(
        held(REWARD),
        Some(status::COMPLETED),
        "the reward done with it"
    );
    let from = send_use(&mut h, b, me, token, 0.2);
    assert_eq!(answers(&h, b, from)[0].status, 0);
}

/// An item for monarchs alone used by anyone else is refused (10038) and does nothing.
/// Behaviour: server.items.monarch-only
#[test]
fn an_item_for_monarchs_alone_is_refused_to_anyone_else() {
    use yalain_world::allegiance::Node;
    const CROWN: u32 = 0x4700_2158;
    let mut h = world();
    let mut props = Props::default();
    props
        .set("Item_UsageAction", Prop::Word(0x1F))
        .set("Usage_MonarchOnly", Prop::Bool(true));
    template(&mut h, CROWN, props);
    h.server.item_uses.insert(
        CROWN,
        vec![item_use::UsageEffect {
            state: LUCK,
            spellcraft: 1.0,
        }],
    );
    let (b, me, world) = in_world(&mut h);
    let crown = give(&mut h, world, CROWN, 1);
    let from = send_use(&mut h, b, me, crown, 0.2);
    assert_eq!(
        answers(&h, b, from)
            .iter()
            .map(|a| a.status)
            .collect::<Vec<_>>(),
        vec![refusal::MONARCH_ONLY]
    );
    assert!(records_from(&h, world, crown).is_empty());
    h.server.allegiances.nodes.insert(me, Node::default());
    let from = send_use(&mut h, b, me, crown, 0.2);
    assert!(
        answers(&h, b, from)
            .iter()
            .all(|a| a.status != refusal::MONARCH_ONLY),
        "a monarch is not refused"
    );
    assert!(!records_from(&h, world, crown).is_empty(), "and it works");
}

/// An item used below its least level is refused (10009) and does nothing.
/// Behaviour: server.items.least-level
#[test]
fn an_egg_used_below_its_least_level_is_refused() {
    let mut h = world();
    let (b, me, world) = in_world(&mut h);
    at_level(&mut h, world, 50);
    let egg = give(&mut h, world, EGG, 1);
    let from = send_use(&mut h, b, me, egg, 0.2);
    assert_eq!(
        answers(&h, b, from)
            .iter()
            .map(|a| a.status)
            .collect::<Vec<_>>(),
        vec![refusal::LEVEL_TOO_LOW]
    );
    assert!(records_from(&h, world, egg).is_empty());
}

/// An item only looked at is shown to its user as an examination, then the use is answered at
/// once.
/// Behaviour: server.items.looked-at
#[test]
fn a_trophy_used_is_shown_then_answered() {
    let mut h = world();
    let (b, me, world) = in_world(&mut h);
    let trophy = give(&mut h, world, TROPHY, 1);
    let from = send_use(&mut h, b, me, trophy, 0.2);
    let calls: Vec<u32> = h
        .functions(b, from)
        .into_iter()
        .filter(|f| *f == yalain_world::places::FORCE_EXAMINE || *f == USE_DONE)
        .collect();
    assert_eq!(calls, vec![yalain_world::places::FORCE_EXAMINE, USE_DONE]);
    assert_eq!(
        answers(&h, b, from)[0],
        Answer {
            status: 0,
            flags: flags::AT_ONCE,
            text: 0,
        }
    );
    assert!(held(&h, world, TROPHY).is_some());
}

/// A potion's drink is cut short by a jump: the use answered as interrupted (91) in the jump's
/// instant with the flags of a use that takes its time, nothing put on, the potion kept, and
/// nothing more at the drink's end; a death cuts the next drink short the same way.
/// Behaviour: server.items.a-jump-cuts-a-use-short
#[test]
fn a_jump_or_a_death_cuts_a_drink_short() {
    let mut h = world();
    let (b, me, world) = in_world(&mut h);
    let potion = give(&mut h, world, POTION, 1);
    let from = send_use(&mut h, b, me, potion, 1.0);
    let at = h.server.players[&world].position;
    let mut jump = yalain_testkit::play::report(
        at.cell,
        auberean_primitives::Vec3::new(at.origin[0], at.origin[1], at.origin[2]),
        0,
        yalain_world::world::IDLE_MOTION,
        h.server.players[&world].teleport_stamp,
    );
    if let auberean_protocol::ClientMessage::Position(p) = &mut jump {
        p.flags |= auberean_protocol::physics::position_flags::JUMP;
    }
    h.bots[b].send(true, queue::WEENIE, &jump);
    run_for(&mut h, 0.2);
    assert_eq!(
        answers(&h, b, from),
        vec![Answer {
            status: item_use::INTERRUPTED,
            flags: flags::PLAYED,
            text: 0,
        }]
    );
    run_for(&mut h, 4.0);
    assert_eq!(answers(&h, b, from).len(), 1, "nothing at the drink's end");
    assert!(records_from(&h, world, potion).is_empty());
    assert!(held(&h, world, POTION).is_some(), "the potion is kept");
    let from = send_use(&mut h, b, me, potion, 1.0);
    let clock = h.clock();
    yalain_world::death::player_dies(&mut h.server, &mut h.net, world, None, clock);
    run_for(&mut h, 4.0);
    assert_eq!(
        answers(&h, b, from)
            .iter()
            .map(|a| a.status)
            .collect::<Vec<_>>(),
        vec![item_use::INTERRUPTED]
    );
    assert!(records_from(&h, world, potion).is_empty());
}

/// A forager's food: drunk as a potion is, then at once health and vigor each healed by 244 (its
/// effects at the spellcraft of 40, the greatest values a cap), the answer with the drink's text
/// and the flags of a use that took its time (never spent), and the food one durability down;
/// at its fifth use it is destroyed through overuse instead.
/// Behaviour: server.items.food-heals-and-wears
#[test]
fn a_forager_s_food_heals_health_and_vigor_and_wears_out_at_its_fifth_use() {
    use yalain_world::item_use::{FOOD, FOOD_EFFECTS};
    const HEALTH: u32 = 258;
    const VIGOR: u32 = 259;
    let mut h = world();
    template(&mut h, FOOD, Props::default());
    let heals = |state: u32, flags: u32, top: f32| {
        let mut d = def(
            state,
            Kind::InstantVital {
                flags,
                change: Scale(vec![(1.0, 30.0), (50.0, 300.0), (150.0, top)]),
            },
            None,
        );
        d.external_flags = flags;
        d
    };
    for d in [
        heals(FOOD_EFFECTS[0], 0x21, 600.0),
        heals(FOOD_EFFECTS[1], 0x22, 450.0),
    ] {
        h.server.effect_table.effects.insert(d.state, d);
    }
    let (b, me, world) = in_world(&mut h);
    let food = give(&mut h, world, FOOD, 1);
    let set = |h: &mut Harness, q: u32, v: i32| {
        let p = h.server.players.get_mut(&world).expect("in");
        yalain_world::quals::set_int(&mut p.qualities, q, v);
    };
    let vitals = |h: &Harness, from: usize| -> Vec<(u32, i32)> {
        h.world_messages(b, from)
            .iter()
            .filter_map(|m| match m {
                ServerMessage::UpdateInt(q) if q.quality == HEALTH || q.quality == VIGOR => {
                    Some((q.quality, q.value))
                }
                _ => None,
            })
            .collect()
    };
    let durability = |h: &Harness| h.server.things.get(food).and_then(|t| t.durability);
    set(&mut h, yalain_world::player::quality::HEALTH_MAX, 1000);
    set(&mut h, yalain_world::player::quality::VIGOR_MAX, 1000);
    set(&mut h, HEALTH, 100);
    set(&mut h, VIGOR, 900);
    let from = send_use(&mut h, b, me, food, 3.5);
    assert!(answers(&h, b, from).is_empty(), "drinking");
    let drunk = h.world_messages(b, from).iter().any(
        |m| matches!(m, ServerMessage::PlayBehavior(p) if p.action.behavior == item_use::DRINK),
    );
    assert!(drunk, "the drink played");
    run_for(&mut h, 1.0);
    let mut got = vitals(&h, from);
    got.sort_unstable();
    assert_eq!(
        got,
        [(HEALTH, 344), (VIGOR, 1000)],
        "244 each, to the greatest"
    );
    assert_eq!(
        answers(&h, b, from),
        vec![Answer {
            status: 0,
            flags: flags::PLAYED,
            text: item_use::text::DRINK.0,
        }]
    );
    assert_eq!(durability(&h), Some(4), "one down");
    for left in [3, 2, 1] {
        send_use(&mut h, b, me, food, 5.0);
        assert_eq!(durability(&h), Some(left));
    }
    let from = send_use(&mut h, b, me, food, 5.0);
    assert_eq!(
        answers(&h, b, from),
        vec![Answer {
            status: 0,
            flags: flags::PLAYED,
            text: item_use::text::DRINK.0,
        }],
        "answered as before, not spent"
    );
    assert!(held(&h, world, FOOD).is_none(), "destroyed through overuse");
}

/// A training hall's potion, whose template names no effect, is drunk as a potion is: the drink
/// played, then at its end its one effect at the spellcraft of 1, the potion spent and "You drink
/// {the} %1.".
/// Behaviour: server.items.training-hall-potions
#[test]
fn a_training_hall_potion_is_drunk_for_its_one_effect() {
    let mut h = world();
    let (potion_template, state) = item_use::TRAINING_POTIONS[0];
    let mut p = Props::default();
    p.set("Item_IsDestroyOnUse", Prop::Bool(true));
    template(&mut h, potion_template, p);
    let d = def(
        state,
        Kind::Numeric {
            flags: 0x4,
            stats: vec![speed::VELOCITY],
            magnitude: Scale(vec![(0.0, 1.2)]),
        },
        Some(300.0),
    );
    h.server.effect_table.effects.insert(d.state, d);
    let (b, me, world) = in_world(&mut h);
    let potion = give(&mut h, world, potion_template, 1);
    let from = send_use(&mut h, b, me, potion, 3.5);
    assert!(
        h.world_messages(b, from).iter().any(
            |m| matches!(m, ServerMessage::PlayBehavior(p) if p.action.behavior == item_use::DRINK)
        ),
        "the drink played"
    );
    assert!(answers(&h, b, from).is_empty(), "drinking");
    run_for(&mut h, 1.0);
    assert_eq!(
        answers(&h, b, from),
        vec![Answer {
            status: 0,
            flags: flags::PLAYED | flags::SPENT,
            text: item_use::text::DRINK.0,
        }]
    );
    let states: Vec<(u32, f32)> = records_from(&h, world, potion)
        .iter()
        .map(|e| (e.state, e.spellcraft))
        .collect();
    assert_eq!(states, vec![(state, 1.0)]);
    assert!(held(&h, world, potion_template).is_none(), "spent");
}

/// A treasure potion's own effect: dropped by a level 46 creature it is given one of the nine kinds
/// of the highest tier at a spellcraft from 0 to 100, named "{the potion} of {the effect}" and
/// colored by it; drunk, it puts on that effect at that spellcraft, spent, "You drink {the} %1.".
/// Behaviour: server.items.treasure-potions-drunk-for-their-own-effect
#[test]
fn a_treasure_potion_is_named_colored_and_drunk_for_its_own_effect() {
    use yalain_world::inventory::{item_create, potion_name, POTION_COLOR};
    use yalain_world::treasure::{self, POTION_EFFECTS};
    const MAGIC: u32 = 0x4700_11B4;
    const MAGIC_PACKAGE: u32 = 0xF15;
    let mut h = world();
    let mut p = Props::default();
    p.set("Item_IsDestroyOnUse", Prop::Bool(true));
    let mut t = yalain_testkit::test_template(MAGIC, MAGIC_PACKAGE, p);
    t.description.name = Some(ITEM_NAME);
    t.facts.name = Some(ITEM_NAME);
    h.server.catalog.insert(t);
    h.server.treasure.potions.insert(MAGIC_PACKAGE);
    for (n, &state) in POTION_EFFECTS.iter().flatten().enumerate() {
        let n = u32::try_from(n).unwrap();
        h.server
            .treasure
            .potion_looks
            .insert(state, (0x2500_08D0 + n, 0.1 + 0.01 * n as f32));
        let d = def(
            state,
            Kind::Numeric {
                flags: 0x4,
                stats: vec![speed::VELOCITY],
                magnitude: Scale(vec![(0.0, 1.1), (100.0, 1.3)]),
            },
            Some(300.0),
        );
        h.server.effect_table.effects.insert(state, d);
    }
    let (b, me, world) = in_world(&mut h);
    let potion = give(&mut h, world, MAGIC, 1);
    treasure::endow(&mut h.server, potion, 46, Some(470));
    let own = h.server.things.get(potion).unwrap().own;
    let e = own
        .effect
        .expect("a treasure potion has an effect of its own");
    assert!(
        POTION_EFFECTS.iter().any(|kind| kind[4] == e.state),
        "the highest tier's: {:08X}",
        e.state
    );
    assert!((0.0..=100.0).contains(&e.spellcraft));
    assert_eq!(
        own.min_level, None,
        "a potion has no least level of its own"
    );
    let made = item_create(h.server.things.get(potion).unwrap());
    assert_eq!(made.weenie.name, potion_name(ITEM_NAME, e.table));
    let colors = &made.visual.global.appearance.entries;
    assert_eq!(colors.len(), 1);
    assert_eq!(colors[0].0, POTION_COLOR.0);
    assert_eq!(colors[0].1.entries, vec![(POTION_COLOR.1, e.color)]);
    let from = send_use(&mut h, b, me, potion, 5.0);
    assert!(
        h.world_messages(b, from).iter().any(
            |m| matches!(m, ServerMessage::PlayBehavior(p) if p.action.behavior == item_use::DRINK)
        ),
        "the drink played"
    );
    assert_eq!(
        answers(&h, b, from),
        vec![Answer {
            status: 0,
            flags: flags::PLAYED | flags::SPENT,
            text: item_use::text::DRINK.0,
        }]
    );
    let states: Vec<(u32, f32)> = records_from(&h, world, potion)
        .iter()
        .map(|r| (r.state, r.spellcraft))
        .collect();
    assert_eq!(states, vec![(e.state, e.spellcraft)]);
    assert!(held(&h, world, MAGIC).is_none(), "spent");
}

/// A Tessera used from the pack plays its use (its user free to move), and at its end gives a
/// hero skill credit more, the skill repository sent with it; the Tessera is spent and the use
/// answered "You use {the} %1." with flags 309.
/// Behaviour: server.items.tessera-gives-a-hero-skill-credit
#[test]
fn a_tessera_used_gives_a_hero_skill_credit_and_is_spent() {
    const TESSERA: u32 = 0x4700_052F;
    const TESSERA_PACKAGE: u32 = 0x1B33;
    let mut h = world();
    let mut p = Props::default();
    p.set("Item_IsDestroyOnUse", Prop::Bool(true));
    let mut t = yalain_testkit::test_template(TESSERA, TESSERA_PACKAGE, p);
    t.description.name = Some(ITEM_NAME);
    h.server.catalog.insert(t);
    h.server.tesserae.insert(TESSERA_PACKAGE);
    let (b, me, world) = in_world(&mut h);
    {
        let p = h.server.players.get_mut(&world).unwrap();
        p.play.hero_credits = 38;
        p.play.credits = 1;
    }
    let tessera = give(&mut h, world, TESSERA, 1);
    let from = send_use(&mut h, b, me, tessera, 1.0);
    assert!(
        h.world_messages(b, from).iter().any(|m| matches!(
            m,
            ServerMessage::PlayBehavior(p) if p.action.behavior == item_use::TESSERA_BEHAVIOR
        )),
        "its use played"
    );
    assert!(answers(&h, b, from).is_empty(), "playing");
    run_for(&mut h, 3.0);
    assert_eq!(
        answers(&h, b, from),
        vec![Answer {
            status: 0,
            flags: 309,
            text: item_use::text::USE.0,
        }]
    );
    assert!(
        h.world_messages(b, from)
            .iter()
            .any(|m| *m == yalain_world::skills::repository_update(1, 39)),
        "the repository with a hero skill credit more"
    );
    assert_eq!(h.server.players[&world].play.hero_credits, 39);
    assert!(held(&h, world, TESSERA).is_none(), "spent");
}
