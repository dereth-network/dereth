//! Dye plants: a seed planted in its garden stands two meters ahead of its planter once the
//! planting has played; the plant takes its growing looks and its species' ripe look as it ages;
//! its needs grow, a watering can and a bag of fertilizer each lower theirs by a level, and a need
//! past either end withers it; pollen brushes onto it once; examined it shows its needs, its pollen
//! and when it is ripe; only its planter cares for it; harvested by its planter when ripe it gives
//! its plant and a pollen, or the seeds of a known cross; it leaves the world with its planter. A
//! species of its own plants several from one seed, needs no care, ripens at its own time and
//! speaks from its own table, its harvest coming to hand as an item taken.
//! Fixture: the test hall, its start a Berimphur garden; the Berimphur seed, plant, plant item and
//! pollen, the Hennacin pollen, the Siennan seed, a watering can and a bag of fertilizer; and a
//! crop's seed, the crop, its sheaf and a quest of five phases.

use auberean_protocol::types::{IconLayer, PartGroupDesc, VisualDesc};
use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use auberean_script::native_object::NativeObject;
use yalain_content::{Prop, Props};
use yalain_net::ConnId;
use yalain_testkit::play::server_call;
use yalain_testkit::Harness;
use yalain_world::calls::{self, Node, Value};
use yalain_world::graph::Builder;
use yalain_world::plants::{self, text};
use yalain_world::usable::interactions::{self, Interaction};
use yalain_world::{item_use, pets};

const USE: u32 = 0x8216_0016;
const USE_DONE: u32 = 0x8216_0021;

const HALL: u32 = 0x0299_0109;
const GARDEN: [f32; 3] = [60.0, 239.0, -44.6];

const SEED: u32 = 0x4700_1454;
const PLANT: u32 = 0x4700_3420;
const ITEM: u32 = 0x4700_0FF9;
const POLLEN: u32 = 0x4700_1448;
const HENNACIN_POLLEN: u32 = 0x4700_1451;
const SIENNAN_SEED: u32 = 0x4700_1453;
const CAN: u32 = plants::WATERING_CAN;
const BAG: u32 = plants::FERTILIZER;
const CAN_TABLE: u32 = 0x6F00_124F;
const BAG_TABLE: u32 = 0x6F00_1242;
const POLLEN_TABLE: u32 = 0x6F00_1245;
const HENNACIN_TABLE: u32 = 0x6F00_124E;
const NAME: (u32, u32) = (0xA860_7CEC, 0x2500_1DB6);

/// The ripe look the fixture gives the Berimphur plant item.
fn ripe_look() -> VisualDesc {
    use auberean_protocol::table::{Table, SMALL};
    VisualDesc {
        bits: VisualDesc::ICON | VisualDesc::GLOBAL | VisualDesc::PART_GROUPS,
        icon: vec![
            IconLayer(1000, 0x4100_5FF5, 0xFFFF_FFFF),
            IconLayer(2000, 0x4100_6F98, 0xFFFF_FFFF),
        ],
        global: PartGroupDesc {
            bits: PartGroupDesc::KEY | PartGroupDesc::APPEARANCE,
            key: 0x4000_0002,
            appearance: Table::from_inserted(
                SMALL,
                vec![(0x2000_0317, Table::from_inserted(SMALL, vec![(2, 0.04)]))],
            ),
            ..PartGroupDesc::default()
        },
        part_groups: Table::from_inserted(
            SMALL,
            vec![(
                0x4000_0001,
                PartGroupDesc {
                    bits: PartGroupDesc::KEY | PartGroupDesc::SETUP,
                    key: 0x4000_0001,
                    setup: 0x0400_056C,
                    ..PartGroupDesc::default()
                },
            )],
        ),
        ..VisualDesc::default()
    }
}

/// The hall, a Berimphur garden within 80 m of its start.
fn garden() -> Harness {
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    let mut seed = Props::default();
    seed.set(
        interactions::USER_BEHAVIOR,
        Prop::Word(plants::PLANTING_BEHAVIOR),
    )
    .set("Item_IsDestroyOnUse", Prop::Bool(true))
    .set("Item_IsStackable", Prop::Bool(true))
    .set("Item_CanUseWhileMoving", Prop::Bool(false))
    .set("Usage_ShouldUnlockUserForUsageEffects", Prop::Bool(true))
    .set("Usage_RequiredLocation", Prop::Place(HALL, GARDEN))
    .set("Usage_RequiredLocationRadius", Prop::Float(80.0));
    let mut plant = Props::default();
    plant
        .set(
            interactions::USER_BEHAVIOR,
            Prop::Word(interactions::PLANT_BEHAVIOR),
        )
        .set("GameplayStatistics_Level", Prop::Int(1))
        .set("GameplayStatistics_MaxHealth", Prop::Int(50))
        .set("GameplayStatistics_MaxVigor", Prop::Int(50))
        .set("NPC_ArmorLevel", Prop::Int(100));
    let mut can = Props::default();
    can.set(interactions::TABLE, Prop::Word(CAN_TABLE))
        .set("Item_Durability_MaxLevel", Prop::Int(100));
    let mut bag = Props::default();
    bag.set(interactions::TABLE, Prop::Word(BAG_TABLE))
        .set("Item_Durability_MaxLevel", Prop::Int(100));
    let mut pollen = Props::default();
    pollen.set(interactions::TABLE, Prop::Word(POLLEN_TABLE));
    let mut hennacin = Props::default();
    hennacin.set(interactions::TABLE, Prop::Word(HENNACIN_TABLE));
    let mut plant_template = yalain_testkit::test_template(PLANT, 0x283D, plant);
    plant_template.description.name = Some(NAME);
    plant_template.facts.name = Some(NAME);
    h.server.catalog.insert(plant_template);
    for (t, package, p) in [
        (SEED, 0x280F, seed),
        (CAN, 0x1B34, can),
        (BAG, 0x1B34, bag),
        (POLLEN, 0x1B34, pollen),
        (HENNACIN_POLLEN, 0x1B34, hennacin),
        (ITEM, 0x1BF2, Props::default()),
        (SIENNAN_SEED, 0x280F, Props::default()),
    ] {
        h.server
            .catalog
            .insert(yalain_testkit::test_template(t, package, p));
    }
    for (table, uses_up) in [
        (CAN_TABLE, false),
        (BAG_TABLE, false),
        (POLLEN_TABLE, true),
        (HENNACIN_TABLE, true),
    ] {
        h.server.interactions.tables.insert(
            table,
            vec![Interaction {
                targets: vec![PLANT],
                reach: 4.0,
                behavior: None,
                message: None,
                uses_up,
            }],
        );
    }
    h.server.plants.looks.insert(ITEM, ripe_look());
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

fn held(h: &Harness, world: ConnId, template: u32) -> Option<(u64, u32)> {
    h.server.players[&world]
        .play
        .items
        .iter()
        .find(|i| i.template == template)
        .map(|i| (i.id, i.quantity.max(1)))
}

/// A player standing in the garden facing north, holding the items given: its client, id and
/// connection.
fn gardener(h: &mut Harness, name: &str, items: &[(u32, u32)]) -> (usize, u64, ConnId) {
    let (b, me) = h.player_in_world(name, name);
    h.run(10);
    let world = world_of(h, me);
    for &(t, n) in items {
        yalain_world::inventory::give(&mut h.server, &mut h.net, world, t, n);
    }
    h.run(3);
    stand(h, world, [0.0, 0.0]);
    (b, me, world)
}

/// Put a player at an offset from the garden's place, facing north.
fn stand(h: &mut Harness, world: ConnId, d: [f32; 2]) {
    let p = h.server.players.get_mut(&world).expect("in the world");
    p.position.cell = HALL;
    p.position.origin = [GARDEN[0] + d[0], GARDEN[1] + d[1], GARDEN[2]];
    p.position.rotation = [1.0, 0.0, 0.0, 0.0];
}

fn use_of(item: u64, me: u64, on: u64) -> Arg {
    use yalain_world::graph::Slot;
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

fn send_use(h: &mut Harness, b: usize, me: u64, item: u64, on: u64) {
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(USE, &[use_of(item, me, on)]),
    );
    h.run(3);
}

fn run_for(h: &mut Harness, seconds: f64) {
    let t = h.now.0 + seconds;
    while h.now.0 < t {
        h.step();
    }
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

/// The plants' texts a client was shown since `from`, in order.
fn plant_texts(h: &Harness, b: usize, from: usize) -> Vec<u32> {
    h.texts(b, from)
        .into_iter()
        .filter(|(kind, _)| *kind == text::SHOWN_AS)
        .map(|(_, id)| id)
        .collect()
}

/// Where in a client's messages since `from` the first use answer is, and the first plants' text.
fn answer_then_text(h: &Harness, b: usize, from: usize) -> (Option<usize>, Option<usize>) {
    let calls = h.calls(b, from);
    (
        calls.iter().position(|(f, _)| *f == USE_DONE),
        calls
            .iter()
            .position(|(f, _)| *f == yalain_world::session::function::SHOW_TEXT),
    )
}

/// The behaviors a player was seen to play since `from`: (behavior, bits, words of flags 0x20 and
/// 0x100).
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

/// The one plant standing, if any.
fn the_plant(h: &Harness) -> Option<u64> {
    h.server.plants.by_id.keys().next().copied()
}

/// Plant a seed where the player stands, and wait for it to stand: the plant.
fn planted(h: &mut Harness, b: usize, me: u64, world: ConnId) -> u64 {
    let (seed, _) = held(h, world, SEED).expect("a seed held");
    send_use(h, b, me, seed, 0);
    run_for(h, 2.1);
    the_plant(h).expect("planted")
}

/// Keep a plant's needs where care leaves them (none needed).
fn tend(h: &mut Harness, plant: u64) {
    if let Some(p) = h.server.plants.by_id.get_mut(&plant) {
        p.water = 3;
        p.fertilizer = 3;
    }
}

/// Let a plant grow ripe, tended all along.
fn ripen(h: &mut Harness, plant: u64) {
    let t = h.now.0 + plants::RIPE_SECONDS;
    while h.now.0 < t {
        tend(h, plant);
        h.step();
    }
}

/// A seed used in its garden plays the planting behavior with the seed in hand; when that has
/// played out (2.0 s) the planter is told of its new pet (the plants' class and flags, no icon),
/// the plant stands two meters straight ahead facing as the planter faces, one seed is spent and
/// the use answered "You use {the} %1." (the flags of a use that took its time, spent, not while
/// moving, letting its user go: 9021). Selected, it is answered with its health and vigor. A seed
/// used far from its garden is refused and kept.
/// Behaviour: server.plants.planted-from-a-seed
#[test]
fn a_seed_used_in_its_garden_stands_its_plant_two_meters_ahead_once_its_planting_has_played() {
    let mut h = garden();
    let (a, me, world) = gardener(&mut h, "one", &[(SEED, 2)]);
    let (seed, _) = held(&h, world, SEED).expect("seeds");
    let from = h.bots[a].inbox.len();
    send_use(&mut h, a, me, seed, 0);
    assert_eq!(
        played(&h, a, from, me).first().map(|x| (x.0, x.1)),
        Some((plants::PLANTING_BEHAVIOR, item_use::BEHAVIOR_BITS)),
        "the planting played at once"
    );
    run_for(&mut h, 1.7);
    assert!(the_plant(&h).is_none(), "not yet, 1.85 s in");
    run_for(&mut h, 0.4);
    let plant = the_plant(&h).expect("standing 2.25 s in");
    let calls = h.calls(a, from);
    let add = calls
        .iter()
        .find(|(f, _)| *f == pets::function::ADD)
        .expect("told of a new pet");
    assert!(add.1.contains(&Value::Long(plant)), "naming the plant");
    assert_eq!(
        answers(&h, a, from),
        [(0, 9021, item_use::text::USE.0)],
        "answered as a use that took its time and spent its item"
    );
    let t = h.server.things.get(plant).expect("a thing of the world");
    assert_eq!(t.kind, yalain_world::things::Kind::Plant);
    assert_eq!(t.summoner, Some(me));
    let at = t.at.expect("standing");
    assert_eq!(at.cell, HALL);
    assert!((at.origin[0] - GARDEN[0]).abs() < 1e-4, "straight ahead");
    assert!(
        (at.origin[1] - (GARDEN[1] + 2.0)).abs() < 1e-4,
        "two meters"
    );
    assert_eq!(at.rotation, [1.0, 0.0, 0.0, 0.0], "facing as the planter");
    assert_eq!(held(&h, world, SEED).map(|x| x.1), Some(1), "one spent");
    assert_eq!(
        yalain_world::requests::vitals_of(&h.server, world, plant).map(|v| (v.health, v.max_vigor)),
        Some((50, 50)),
        "selected, its vitals"
    );
    // Far from its garden: refused, kept, nothing planted.
    run_for(&mut h, 3.0);
    stand(&mut h, world, [120.0, 0.0]);
    let from = h.bots[a].inbox.len();
    send_use(&mut h, a, me, seed, 0);
    run_for(&mut h, 2.5);
    assert_eq!(
        answers(&h, a, from).first().map(|x| x.0),
        Some(item_use::refusal::NOT_THERE)
    );
    assert_eq!(held(&h, world, SEED).map(|x| x.1), Some(1), "kept");
    assert_eq!(h.server.plants.by_id.len(), 1, "no second plant");
}

/// The setups a client saw a thing's part group take, and the whole looks, since `from`.
fn looks(h: &Harness, b: usize, from: usize, id: u64) -> Vec<VisualDesc> {
    h.world_messages(b, from)
        .into_iter()
        .filter_map(|m| match m {
            ServerMessage::UpdateVisualDesc(e) if e.sender.id == id => Some(e.action),
            _ => None,
        })
        .collect()
}

/// A plant takes its first growing look at 45 s or a thought after, its second at 90 s, and its
/// species' ripe look (its plant item's) at 135 s, each shown to whoever sees it, each within a
/// thought (20 s) of its time; a player who comes to see it later is shown it as it then looks.
/// Behaviour: server.plants.grow-through-three-looks
#[test]
fn a_plant_takes_two_growing_looks_and_then_its_species_ripe_look_as_it_ages() {
    let mut h = garden();
    let (a, me, world) = gardener(&mut h, "one", &[(SEED, 1)]);
    let start = h.now.0;
    let plant = planted(&mut h, a, me, world);
    let planted_at = h.server.plants.by_id[&plant].planted;
    let from = h.bots[a].inbox.len();
    let mut seen = Vec::new();
    while h.now.0 < start + 170.0 {
        tend(&mut h, plant);
        let n = h.bots[a].inbox.len();
        h.step();
        for l in looks(&h, a, n, plant) {
            seen.push((h.now.0 - planted_at, l));
        }
    }
    assert_eq!(looks(&h, a, from, plant).len(), 3, "three looks");
    for (k, (age, look)) in seen.iter().enumerate() {
        let due = 45.0 * f64::from(u8::try_from(k + 1).expect("small"));
        assert!(*age >= due && *age <= due + 20.0, "look {k} at {age:.1} s");
        if k < 2 {
            assert_eq!(
                *look,
                plants::growing_look(u8::try_from(k + 1).expect("small"))
            );
            let setup = look.part_groups.entries[0].1.setup;
            assert_eq!(setup, [0x0400_0693, 0x0400_0694][k]);
        } else {
            assert_eq!(*look, ripe_look(), "its plant item's look");
        }
    }
    let (b, _) = h.player_in_world("two", "two");
    let from = 0;
    h.run(10);
    let shown = h.world_messages(b, from).into_iter().find_map(|m| match m {
        ServerMessage::CreateObject(o) if o.id == plant => Some(o.visual),
        _ => None,
    });
    assert_eq!(shown, Some(ripe_look()), "a newcomer sees it ripe");
}

/// A plant's needs grow at its thoughts while it is not ripe: by nothing, one level or two, as a
/// roll falls. A watering can lowers its need of water by one level and a bag of fertilizer its
/// need of fertilizer, each when its use has played out; a new plant badly needs both.
/// Behaviour: server.plants.needs-and-care
#[test]
fn a_watering_can_and_a_bag_of_fertilizer_each_lower_their_need_by_one_level() {
    assert_eq!(plants::growth(0.0), 2);
    assert_eq!(plants::growth(plants::GROWS_TWO - 0.001), 2);
    assert_eq!(plants::growth(plants::GROWS_TWO + 0.001), 1);
    assert_eq!(
        plants::growth(plants::GROWS_TWO + plants::GROWS_ONE - 0.001),
        1
    );
    assert_eq!(
        plants::growth(plants::GROWS_TWO + plants::GROWS_ONE + 0.001),
        0
    );
    let mut h = garden();
    let (a, me, world) = gardener(&mut h, "one", &[(SEED, 1), (CAN, 1), (BAG, 1)]);
    let plant = planted(&mut h, a, me, world);
    let needs = |h: &Harness| {
        let p = &h.server.plants.by_id[&plant];
        (p.water, p.fertilizer)
    };
    assert_eq!(needs(&h), (5, 5), "badly needs both");
    let (can, _) = held(&h, world, CAN).expect("a can");
    let (bag, _) = held(&h, world, BAG).expect("a bag");
    send_use(&mut h, a, me, can, plant);
    assert_eq!(needs(&h), (5, 5), "not while it plays");
    run_for(&mut h, 2.0);
    assert_eq!(needs(&h), (4, 5), "watered");
    send_use(&mut h, a, me, bag, plant);
    run_for(&mut h, 2.0);
    assert_eq!(needs(&h), (4, 4), "fertilized");
    // Its needs grow, tended or not, until it is ripe.
    let mut grew = false;
    while h.now.0 < h.server.plants.by_id[&plant].planted + 170.0 {
        let before = needs(&h);
        h.step();
        let after = needs(&h);
        grew |= after.0 > before.0 || after.1 > before.1;
        tend(&mut h, plant);
    }
    assert!(grew, "its needs grew");
}

/// A need grown past "Death imminent!" withers the plant at its next thought, and so does a plant
/// cared for past "Far too much!": its planter is told why, it is taken off the planter's pets and
/// it leaves the world.
/// Behaviour: server.plants.wither
#[test]
fn a_plant_neglected_past_death_or_cared_for_past_far_too_much_withers() {
    let mut h = garden();
    let (a, me, world) = gardener(&mut h, "one", &[(SEED, 2), (CAN, 1)]);
    let plant = planted(&mut h, a, me, world);
    let from = h.bots[a].inbox.len();
    // Its need of water past "Death imminent!": it withers at its next thought, 20 s at most.
    h.server
        .plants
        .by_id
        .get_mut(&plant)
        .expect("a plant")
        .water = 9;
    run_for(&mut h, 20.0);
    assert!(!h.server.plants.by_id.contains_key(&plant), "withered");
    assert!(h.server.things.get(plant).is_none(), "gone from the world");
    assert_eq!(plant_texts(&h, a, from), [text::NO_WATER]);
    assert!(h
        .calls(a, from)
        .iter()
        .any(|(f, v)| *f == pets::function::REMOVE && v.contains(&Value::Long(plant))));
    // Watered at "Far too much!": withers from too much water.
    stand(&mut h, world, [0.0, 0.0]);
    let plant = planted(&mut h, a, me, world);
    h.server
        .plants
        .by_id
        .get_mut(&plant)
        .expect("a plant")
        .water = 0;
    let (can, _) = held(&h, world, CAN).expect("a can");
    let from = h.bots[a].inbox.len();
    send_use(&mut h, a, me, can, plant);
    run_for(&mut h, 2.0);
    assert!(!h.server.plants.by_id.contains_key(&plant), "withered");
    assert_eq!(plant_texts(&h, a, from), [text::TOO_MUCH_WATER]);
}

/// Pollen used on a plant is used up (319) and brushes onto its leaves: "You brush the pollen on
/// to the plants' leaves." right after the answer. On a plant already pollinated the pollen does
/// not stick, and the plant keeps the first.
/// Behaviour: server.plants.pollinated
#[test]
fn pollen_brushes_onto_a_plant_once_and_is_blown_away_from_one_already_pollinated() {
    let mut h = garden();
    let (a, me, world) = gardener(
        &mut h,
        "one",
        &[(SEED, 1), (HENNACIN_POLLEN, 1), (POLLEN, 1)],
    );
    let plant = planted(&mut h, a, me, world);
    let (hennacin, _) = held(&h, world, HENNACIN_POLLEN).expect("pollen");
    let from = h.bots[a].inbox.len();
    send_use(&mut h, a, me, hennacin, plant);
    run_for(&mut h, 2.0);
    assert_eq!(
        answers(&h, a, from),
        [(
            0,
            interactions::FLAGS_USED_UP,
            interactions::text::USED_ON.0
        )]
    );
    assert_eq!(plant_texts(&h, a, from), [text::BRUSHED]);
    let (answer, said) = answer_then_text(&h, a, from);
    assert!(answer < said, "the text after the answer");
    assert_eq!(h.server.plants.by_id[&plant].pollen, Some(HENNACIN_POLLEN));
    let (other, _) = held(&h, world, POLLEN).expect("pollen");
    let from = h.bots[a].inbox.len();
    send_use(&mut h, a, me, other, plant);
    run_for(&mut h, 2.0);
    assert_eq!(plant_texts(&h, a, from), [text::BLOWN_AWAY]);
    assert_eq!(
        h.server.plants.by_id[&plant].pollen,
        Some(HENNACIN_POLLEN),
        "the first kept"
    );
    assert_eq!(held(&h, world, POLLEN), None, "used up all the same");
}

/// A watering can, a bag of fertilizer and a pollen used on another player's plant play their use
/// as ever, and when it has played out the use is answered with the item kept (316), unworn and
/// not used up, and the player told, as at a harvest, that it did not plant it and leaves it
/// alone: its needs and its pollen stay. Its planter's own care still counts.
/// Behaviour: server.plants.cared-for-by-their-planter
#[test]
fn only_its_planter_waters_fertilizes_or_pollinates_a_plant() {
    let mut h = garden();
    let (a, me, world) = gardener(&mut h, "one", &[(SEED, 1), (CAN, 1)]);
    let plant = planted(&mut h, a, me, world);
    let (b, you, your_world) = gardener(&mut h, "two", &[(CAN, 1), (BAG, 1), (HENNACIN_POLLEN, 1)]);
    stand(&mut h, your_world, [1.0, 2.0]);
    let state = |h: &Harness| {
        let p = &h.server.plants.by_id[&plant];
        (p.water, p.fertilizer, p.pollen)
    };
    assert_eq!(state(&h), (5, 5, None));
    for template in [CAN, BAG, HENNACIN_POLLEN] {
        let (item, _) = held(&h, your_world, template).expect("held");
        let worn = h.server.things.get(item).and_then(|t| t.durability);
        let from = h.bots[b].inbox.len();
        send_use(&mut h, b, you, item, plant);
        assert_eq!(played(&h, b, from, you).len(), 1, "its use plays as ever");
        run_for(&mut h, 2.0);
        assert_eq!(state(&h), (5, 5, None), "left alone");
        assert_eq!(
            answers(&h, b, from),
            [(0, interactions::FLAGS_KEPT, 0)],
            "answered, the item kept"
        );
        assert_eq!(plant_texts(&h, b, from), [text::NOT_YOURS]);
        let (answer, said) = answer_then_text(&h, b, from);
        assert!(answer < said, "the text after the answer");
        assert_eq!(
            held(&h, your_world, template).map(|x| x.0),
            Some(item),
            "kept"
        );
        assert_eq!(
            h.server.things.get(item).and_then(|t| t.durability),
            worn,
            "unworn"
        );
    }
    // Its planter's own can still waters it.
    let (can, _) = held(&h, world, CAN).expect("a can");
    send_use(&mut h, a, me, can, plant);
    run_for(&mut h, 2.0);
    assert_eq!(state(&h), (4, 5, None), "watered by its planter");
}

/// A plant used plays the plants' behavior (bits 0x400121) and is answered when it has played out
/// (956, "Item use succeeds."): not ripe, "not mature yet", and it stays; by a player who did not
/// plant it, left alone; ripe, by its planter, it is taken off its planter's pets and leaves the
/// world, and its plant item and a pollen of its kind are made in the pack.
/// Behaviour: server.plants.harvested
#[test]
fn a_ripe_plant_harvested_by_its_planter_gives_its_plant_and_its_pollen() {
    let mut h = garden();
    let (a, me, world) = gardener(&mut h, "one", &[(SEED, 1)]);
    let plant = planted(&mut h, a, me, world);
    let from = h.bots[a].inbox.len();
    send_use(&mut h, a, me, plant, 0);
    assert_eq!(
        played(&h, a, from, me),
        [(interactions::PLANT_BEHAVIOR, plants::HARVEST_BITS, 1, 1)]
    );
    assert!(answers(&h, a, from).is_empty(), "not while it plays");
    run_for(&mut h, 2.0);
    assert_eq!(
        answers(&h, a, from).first().map(|x| (x.0, x.1)),
        Some((0, plants::HARVEST_FLAGS))
    );
    assert_eq!(plant_texts(&h, a, from), [text::NOT_MATURE]);
    assert!(h.server.plants.by_id.contains_key(&plant), "it stays");
    ripen(&mut h, plant);
    // Another player's use: left alone.
    let (b, you) = h.player_in_world("two", "two");
    h.run(10);
    let your_world = world_of(&h, you);
    stand(&mut h, your_world, [1.0, 0.0]);
    let from = h.bots[b].inbox.len();
    send_use(&mut h, b, you, plant, 0);
    run_for(&mut h, 2.0);
    assert_eq!(plant_texts(&h, b, from), [text::NOT_YOURS]);
    assert!(h.server.plants.by_id.contains_key(&plant), "left alone");
    let from = h.bots[a].inbox.len();
    send_use(&mut h, a, me, plant, 0);
    run_for(&mut h, 2.0);
    assert_eq!(plant_texts(&h, a, from), [text::RIPE_WITH_POLLEN]);
    assert!(h.server.things.get(plant).is_none(), "gone");
    assert!(h
        .calls(a, from)
        .iter()
        .any(|(f, v)| *f == pets::function::REMOVE && v.contains(&Value::Long(plant))));
    assert_eq!(held(&h, world, ITEM).map(|x| x.1), Some(1), "its plant");
    assert_eq!(held(&h, world, POLLEN).map(|x| x.1), Some(1), "its pollen");
}

/// A ripe plant pollinated with a pollen of a known cross gives the cross's seeds: a Berimphur
/// plant with Hennacin pollen three Siennan seeds ("You pluck 3 seeds from the plant."), and no
/// plant or pollen.
/// Behaviour: server.plants.cross
#[test]
fn a_plant_pollinated_for_a_known_cross_gives_the_seeds_of_the_cross() {
    let mut h = garden();
    let (a, me, world) = gardener(&mut h, "one", &[(SEED, 1), (HENNACIN_POLLEN, 1)]);
    let plant = planted(&mut h, a, me, world);
    let (hennacin, _) = held(&h, world, HENNACIN_POLLEN).expect("pollen");
    send_use(&mut h, a, me, hennacin, plant);
    run_for(&mut h, 2.0);
    ripen(&mut h, plant);
    let from = h.bots[a].inbox.len();
    send_use(&mut h, a, me, plant, 0);
    run_for(&mut h, 2.0);
    assert_eq!(plant_texts(&h, a, from), [text::SEEDS[2]]);
    assert_eq!(held(&h, world, SIENNAN_SEED).map(|x| x.1), Some(3));
    assert_eq!(held(&h, world, ITEM), None);
    assert_eq!(held(&h, world, POLLEN), None);
}

fn about(object: u64) -> Arg {
    let mut bytes = Vec::new();
    for w in [0x4000_0002u32, 0] {
        bytes.extend(w.to_le_bytes());
    }
    bytes.extend(object.to_le_bytes());
    bytes.extend(0x3FFu32.to_le_bytes());
    let mut b = Builder::new();
    b.native(154, NativeObject::Packed { ty: 154, bytes });
    Arg::Object(b.graph())
}

/// The last examination profile a client was sent since `from`.
fn last_profile(h: &Harness, b: usize, from: usize) -> Option<Vec<u8>> {
    h.world_messages(b, from)
        .into_iter()
        .rev()
        .find_map(|m| match m {
            ServerMessage::PlayerCall(c)
                if c.function.0 == yalain_world::profile::EXAMINATION_PROFILE =>
            {
                let v = calls::values(&c.args?.0).ok()?;
                match v[0].root() {
                    Some(Node::Native { bytes, .. }) => Some(bytes.clone()),
                    _ => None,
                }
            }
            _ => None,
        })
}

/// Examined, a plant shows its planter's kingdom.
/// Behaviour: server.kingdoms.plant-shows-its-planters
#[test]
fn a_plants_profile_shows_its_planters_kingdom() {
    let mut h = garden();
    let (a, me, world) = gardener(&mut h, "one", &[(SEED, 1)]);
    if let Some(p) = h.server.players.get_mut(&world) {
        yalain_world::kingdoms::set_kingdom(
            &mut p.qualities,
            yalain_world::kingdoms::kingdom::ORDER,
        );
    }
    let plant = planted(&mut h, a, me, world);
    let from = h.bots[a].inbox.len();
    h.ask(
        a,
        yalain_world::requests::function::EXAMINE,
        &[about(plant)],
        3,
    );
    let shown = last_profile(&h, a, from).expect("a profile");
    let has = |id: u32| shown.windows(4).any(|w| w == id.to_le_bytes());
    assert!(has(yalain_world::profile::kingdom(
        yalain_world::kingdoms::kingdom::ORDER
    )));
    assert!(!has(yalain_world::profile::line::NEUTRAL));
}

/// Examined, a growing plant shows its need of water and of fertilizer, its pollen and a countdown
/// to the game time it is ripe (180 s after it was planted), sent again as they change; ripe, that
/// it is now mature.
/// Behaviour: server.plants.examined
#[test]
fn examining_a_plant_shows_its_needs_its_pollen_and_when_it_is_ripe() {
    let mut h = garden();
    let (a, me, world) = gardener(&mut h, "one", &[(SEED, 1), (CAN, 1)]);
    let plant = planted(&mut h, a, me, world);
    let p = h.server.plants.by_id[&plant].clone();
    let early = p.ripe_game - (h.server.game_now + 180.0);
    assert!((-0.2..=0.0).contains(&early), "ripe 180 s after planting");
    let from = h.bots[a].inbox.len();
    h.ask(
        a,
        yalain_world::requests::function::EXAMINE,
        &[about(plant)],
        3,
    );
    let request = {
        let mut r = Vec::new();
        for w in [0x4000_0002u32, 0] {
            r.extend(w.to_le_bytes());
        }
        r.extend(plant.to_le_bytes());
        r.extend(0x3FFu32.to_le_bytes());
        r
    };
    let growing = |water: i32| plants::Growing {
        needs: Some(plants::Needs {
            water,
            fertilizer: 5,
            pollen: text::NO_POLLEN,
        }),
        ripe_game: p.ripe_game,
    };
    let vitals = yalain_world::examine::Vitals {
        health: 50,
        max_health: 50,
        vigor: 50,
        max_vigor: 50,
        pk_damage: 0,
        pk_vigor_loss: 0,
    };
    let shown = plants::Shown {
        name: Some(NAME),
        level: 1,
        kingdom: 0,
        vitals,
        armor: 100,
    };
    let want = |g: Option<plants::Growing>| {
        plants::profile_of(0x3FF, &shown, g, plants::DYE_LINES).packed(&request)
    };
    assert_eq!(last_profile(&h, a, from), Some(want(Some(growing(5)))));
    let (can, _) = held(&h, world, CAN).expect("a can");
    let from = h.bots[a].inbox.len();
    send_use(&mut h, a, me, can, plant);
    run_for(&mut h, 5.5);
    assert_eq!(
        last_profile(&h, a, from),
        Some(want(Some(growing(4)))),
        "sent again as it changed"
    );
    ripen(&mut h, plant);
    let from = h.bots[a].inbox.len();
    h.ask(
        a,
        yalain_world::requests::function::EXAMINE,
        &[about(plant)],
        3,
    );
    assert_eq!(last_profile(&h, a, from), Some(want(None)), "ripe");
}

/// A planter leaving the world takes its plants out of it.
/// Behaviour: server.plants.leave-with-their-planter
#[test]
fn a_planter_leaving_the_world_takes_its_plants_with_it() {
    let mut h = garden();
    let (a, me, world) = gardener(&mut h, "one", &[(SEED, 1)]);
    let plant = planted(&mut h, a, me, world);
    yalain_world::world::leave(&mut h.server, &mut h.net, world);
    h.run(3);
    assert!(h.server.plants.by_id.is_empty());
    assert!(h.server.things.get(plant).is_none());
}

/// A seed whose plant this world can't make is refused and kept: nothing stands, none is spent, and
/// the planter is told the summoning fails.
/// Behaviour: server.makes.nothing-used-up-for-what-cannot-be-made
#[test]
fn a_seed_whose_plant_cannot_be_made_is_refused_and_kept() {
    let mut h = garden();
    let (a, me, world) = gardener(&mut h, "one", &[(SEED, 2)]);
    h.server.catalog.templates.remove(&PLANT);
    let (seed, before) = held(&h, world, SEED).expect("seeds");
    let from = h.bots[a].inbox.len();
    send_use(&mut h, a, me, seed, 0);
    run_for(&mut h, 2.1);
    assert_eq!(the_plant(&h), None, "nothing stands");
    assert_eq!(held(&h, world, SEED), Some((seed, before)), "kept");
    assert!(
        h.texts(a, from)
            .iter()
            .any(|(_, t)| *t == yalain_world::pets::text::SUMMONING_FAILS.0),
        "told"
    );
}

/// A `[[plant]]` row sets what its seed plants and what a harvest gives: the Berimphur row written
/// with no pollen gives its plant item alone ("You harvest the plant."); with no row for it, the
/// seed plants nothing.
/// Behaviour: server.plants.plant-by-world-rows
#[test]
fn a_plant_row_sets_what_its_seed_plants_and_what_its_harvest_gives() {
    let mut h = garden();
    h.server
        .plants
        .kinds
        .species
        .insert(PLANT, plants::Species::new(PLANT, SEED, None, ITEM, 0));
    let (a, me, world) = gardener(&mut h, "one", &[(SEED, 2)]);
    let plant = planted(&mut h, a, me, world);
    ripen(&mut h, plant);
    let from = h.bots[a].inbox.len();
    send_use(&mut h, a, me, plant, 0);
    run_for(&mut h, 2.0);
    assert_eq!(plant_texts(&h, a, from), [text::RIPE]);
    assert_eq!(held(&h, world, ITEM).map(|x| x.1), Some(1), "its plant");
    assert_eq!(held(&h, world, POLLEN), None, "no pollen");
    h.server.plants.kinds.species.remove(&PLANT);
    let (seed, _) = held(&h, world, SEED).expect("a seed held");
    send_use(&mut h, a, me, seed, 0);
    run_for(&mut h, 2.1);
    assert_eq!(the_plant(&h), None, "no row, nothing planted");
}

/// A `[[plant_cross]]` row sets the seeds a pollinated plant gives and how many: Berimphur with
/// Hennacin pollen written as two Berimphur seeds gives those ("You pluck 2 seeds from the
/// plant."); with no row for the pair, the plant gives its plant item and pollen.
/// Behaviour: server.plants.cross-by-world-rows
#[test]
fn a_plant_cross_row_sets_the_seeds_a_pollinated_plant_gives_and_how_many() {
    let crossed = |h: &mut Harness, name: &str| {
        let (a, me, world) = gardener(h, name, &[(SEED, 1), (HENNACIN_POLLEN, 1)]);
        let plant = planted(h, a, me, world);
        let (hennacin, _) = held(h, world, HENNACIN_POLLEN).expect("pollen");
        send_use(h, a, me, hennacin, plant);
        run_for(h, 2.0);
        ripen(h, plant);
        let from = h.bots[a].inbox.len();
        send_use(h, a, me, plant, 0);
        run_for(h, 2.0);
        (plant_texts(h, a, from), world)
    };
    let mut h = garden();
    h.server.plants.kinds.crosses.insert(
        (PLANT, HENNACIN_POLLEN),
        plants::Cross {
            plant: PLANT,
            pollen: HENNACIN_POLLEN,
            seeds: SEED,
            count: 2,
        },
    );
    let (said, world) = crossed(&mut h, "one");
    assert_eq!(said, [text::SEEDS[1]]);
    assert_eq!(held(&h, world, SEED).map(|x| x.1), Some(2));
    assert_eq!(held(&h, world, SIENNAN_SEED), None);
    let mut h = garden();
    h.server.plants.kinds.crosses.clear();
    let (said, world) = crossed(&mut h, "two");
    assert_eq!(said, [text::RIPE_WITH_POLLEN]);
    assert_eq!(held(&h, world, ITEM).map(|x| x.1), Some(1));
    assert_eq!(held(&h, world, POLLEN).map(|x| x.1), Some(1));
    assert_eq!(held(&h, world, SIENNAN_SEED), None);
}

/// A world with no `[[plant]]` or `[[plant_cross]]` rows has no dye plants and names the kinds it
/// lacks; with rows of a kind it has those alone and names the other.
/// Behaviour: server.plants.plants-without-rows
#[test]
fn a_world_without_plant_or_plant_cross_rows_has_none_and_names_the_kinds_it_lacks() {
    assert_eq!(
        plants::kinds_missing(&plants::DyePlants::default()),
        ["plant", "plant_cross"]
    );
    assert!(plants::kinds_missing(&yalain_testkit::test_plants()).is_empty());
    let one = plants::Species::new(PLANT, SEED, None, ITEM, 0);
    let rows = plants::DyePlants {
        species: [(PLANT, one)].into(),
        ..plants::DyePlants::default()
    };
    assert_eq!(plants::kinds_missing(&rows), ["plant_cross"]);
    assert_eq!(rows.sown_by(SEED), Some(one));
    assert_eq!(rows.sown_by(SIENNAN_SEED), None, "no other species");
}

// A species of its own: a seed that plants four crops, which need no care, ripen in five minutes
// and speak from their own table, under a quest the seed's use moves and the harvest's taking moves
// again.
const CROP_SEED: u32 = 0x4700_F161;
const CROP: u32 = 0x4700_F162;
const SHEAF: u32 = 0x4700_F163;
const CROP_TABLE: u32 = 0x2500_F164;
const CROP_LINES: plants::Texts = plants::Texts {
    table: CROP_TABLE,
    maturity_in: 0xA0BE_B96C,
    mature: 0x9D26_22AC,
    harvest: 0x39D2_1C3D,
};
const CROP_NAME: (u32, u32) = (0xA860_7CEC, CROP_TABLE);
const CROP_QUEST: u32 = 0x4000_0F16;
const CROP_RIPE: f64 = 300.0;

/// The garden, with the crop's seed (naming a summoning effect no install holds and the quest's
/// advance to its third phase, used only with the quest under way), the crop, its sheaf, its
/// species row, the quest of five phases, and a step: taking a sheaf at the third phase moves it to
/// the fourth.
fn crop_garden() -> Harness {
    use yalain_content::quests::{QuestDef, Step, Trigger, Update};
    let mut h = garden();
    let mut seed = h.server.catalog.get(SEED).expect("the seed").props.clone();
    seed.set(
        "Usage_Effect1",
        Prop::Text("Eff_ChainSummonCropsTest".into()),
    )
    .set(
        "Usage_Effect2",
        Prop::Text("Eff_AdvanceQuestCropTestPhase3".into()),
    )
    .set("Usage_RequiredQuest", Prop::Word(CROP_QUEST))
    .set("Usage_RequiredQuestStatus", Prop::Word(0x1000_0000));
    let mut crop = h
        .server
        .catalog
        .get(PLANT)
        .expect("the plant")
        .props
        .clone();
    crop.set("GameplayStatistics_MaxHealth", Prop::Int(3000))
        .set("GameplayStatistics_MaxVigor", Prop::Int(3000));
    let mut crop_template = yalain_testkit::test_template(CROP, 0x283E, crop);
    crop_template.description.name = Some(CROP_NAME);
    crop_template.facts.name = Some(CROP_NAME);
    h.server.catalog.insert(crop_template);
    h.server
        .catalog
        .insert(yalain_testkit::test_template(CROP_SEED, 0x280F, seed));
    h.server.catalog.insert(yalain_testkit::test_template(
        SHEAF,
        0x1005,
        Props::default(),
    ));
    h.server.plants.kinds.species.insert(
        CROP,
        plants::Species {
            count: 4,
            ripe: CROP_RIPE,
            tended: false,
            texts: Some(CROP_LINES),
            ..plants::Species::new(CROP, CROP_SEED, None, SHEAF, 0)
        },
    );
    h.server.quests.quests.push(QuestDef {
        id: CROP_QUEST,
        ..super::kill_quests::quest(5)
    });
    h.server
        .quest_ids
        .insert("QuestCropTest".into(), CROP_QUEST);
    h.server.quests.steps.push(Step {
        quest: CROP_QUEST,
        needs: 3,
        trigger: Trigger::Take(SHEAF),
        tell: None,
        update: Update::Advance(4),
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
        effects_quiet: false,
        wants: None,
        fellows: false,
        levels: None,
        xp_share: 0.0,
        to: None,
        transforms: None,
        chance: None,
    });
    h
}

/// Put the crop's quest under way for a player at a phase.
fn crop_quest_at(h: &mut Harness, world: ConnId, phase: u32) {
    let p = h.server.players.get_mut(&world).expect("in the world");
    p.play.quests.retain(|q| q.quest != CROP_QUEST);
    p.play.quests.push(yalain_world::state::QuestState {
        quest: CROP_QUEST,
        status: yalain_world::quests::status::UNDERWAY,
        phase,
        started: 0,
        updated: 0,
    });
}

fn crop_phase(h: &Harness, world: ConnId) -> u32 {
    yalain_world::quests::phase_of(&h.server.players[&world].play.quests, CROP_QUEST)
}

/// The crops standing, in the order they were made.
fn crops(h: &Harness) -> Vec<u64> {
    h.server
        .plants
        .by_id
        .values()
        .filter(|p| p.species.plant == CROP)
        .map(|p| p.id)
        .collect()
}

/// The texts a client was shown as a plant's since `from`: each string and its table.
fn plant_lines(h: &Harness, b: usize, from: usize) -> Vec<(u32, u32)> {
    h.calls(b, from)
        .iter()
        .filter(|(f, _)| *f == yalain_world::session::function::SHOW_TEXT)
        .filter(|(_, v)| v.contains(&Value::Int(text::SHOWN_AS)))
        .filter_map(|(_, v)| {
            v.iter().find_map(|x| match x.root() {
                Some(Node::TextInfo(t)) => Some((t.string_id, t.table)),
                _ => None,
            })
        })
        .collect()
}

/// Plant the crop's seed where the player stands, with its quest at the second phase: the crops.
fn crops_planted(h: &mut Harness, b: usize, me: u64, world: ConnId) -> Vec<u64> {
    crop_quest_at(h, world, 2);
    let (seed, _) = held(h, world, CROP_SEED).expect("the crop's seed");
    send_use(h, b, me, seed, 0);
    run_for(h, 2.1);
    crops(h)
}

/// A seed whose species plants several (four) stands each when its planting has played out, two
/// meters from the planter, the first 45 degrees to the left of straight ahead and the last 45 to
/// the right, the others evenly between, all facing as the planter faces; the planter is told of
/// each as a pet, the seed is spent, the use answered as any planting's, and the quest the seed's
/// template names for its use (an advance to the third phase) moves. Used with its quest not under
/// way, it is refused and kept, and nothing stands.
/// Behaviour: server.plants.several-from-one-seed
#[test]
fn a_seed_of_a_species_planting_four_stands_each_two_meters_off_spread_across_the_planters_heading()
{
    let mut h = crop_garden();
    let (a, me, world) = gardener(&mut h, "one", &[(CROP_SEED, 2)]);
    let (seed, _) = held(&h, world, CROP_SEED).expect("seeds");
    // Its quest not under way: refused, kept.
    let from = h.bots[a].inbox.len();
    send_use(&mut h, a, me, seed, 0);
    run_for(&mut h, 2.5);
    assert_eq!(
        answers(&h, a, from).first().map(|x| x.0),
        Some(item_use::refusal::LACKING_QUEST)
    );
    assert_eq!(held(&h, world, CROP_SEED).map(|x| x.1), Some(2), "kept");
    assert!(crops(&h).is_empty(), "nothing stands");
    run_for(&mut h, 3.0);
    let from = h.bots[a].inbox.len();
    let made = crops_planted(&mut h, a, me, world);
    assert_eq!(made.len(), 4, "four from one seed");
    let added = h
        .calls(a, from)
        .iter()
        .filter(|(f, v)| {
            *f == pets::function::ADD && made.iter().any(|c| v.contains(&Value::Long(*c)))
        })
        .count();
    assert_eq!(added, 4, "each told as a pet");
    assert_eq!(
        answers(&h, a, from),
        [(0, 9021, item_use::text::USE.0)],
        "answered as a planting"
    );
    assert_eq!(
        held(&h, world, CROP_SEED).map(|x| x.1),
        Some(1),
        "one spent"
    );
    assert_eq!(crop_phase(&h, world), 3, "its quest moved");
    let mut offs: Vec<f32> = made
        .iter()
        .map(|c| {
            let t = h.server.things.get(*c).expect("a thing of the world");
            assert_eq!(t.kind, yalain_world::things::Kind::Plant);
            assert_eq!(t.summoner, Some(me));
            assert_eq!(t.max_health, 3000, "its template's health");
            let at = t.at.expect("standing");
            assert_eq!(at.cell, HALL);
            assert_eq!(at.rotation, [1.0, 0.0, 0.0, 0.0], "facing as the planter");
            let (dx, dy) = (at.origin[0] - GARDEN[0], at.origin[1] - GARDEN[1]);
            assert!(
                ((dx * dx + dy * dy).sqrt() - 2.0).abs() < 1e-3,
                "two meters"
            );
            // Facing north: off the heading to the left is toward the west.
            dereth_primitives::num::math::atan2f(-dx, dy).to_degrees()
        })
        .collect();
    offs.sort_by(f32::total_cmp);
    for (got, want) in offs.iter().zip([-45.0, -15.0, 15.0, 45.0]) {
        assert!((got - want).abs() < 0.05, "{offs:?}");
    }
    assert_eq!(plants::off_heading(0, 1), 0.0, "one alone: straight ahead");
}

/// A plant of a species that needs no care ripens at its own time (five minutes), never needs
/// water or fertilizer, never withers untended, and keeps its own look; harvested before it is ripe
/// or by another, it says so from its own table; ripe, its planter's harvest says its own line,
/// takes it away and gives its plant item, which comes to hand as an item taken does and moves the
/// quest its taking moves.
/// Behaviour: server.plants.a-species-of-its-own
#[test]
fn a_plant_needing_no_care_ripens_at_its_own_time_and_its_harvest_comes_to_hand_as_taken() {
    let mut h = crop_garden();
    let (a, me, world) = gardener(&mut h, "one", &[(CROP_SEED, 1)]);
    let start = h.now.0;
    let made = crops_planted(&mut h, a, me, world);
    let crop = made[0];
    let from = h.bots[a].inbox.len();
    // Two hundred seconds: past a dye plant's ripening, and long past its death untended.
    let left = 200.0 - (h.now.0 - start);
    run_for(&mut h, left);
    assert_eq!(crops(&h).len(), 4, "none withered");
    let p = &h.server.plants.by_id[&crop];
    assert_eq!(
        (p.water, p.fertilizer),
        (plants::NEW_NEED, plants::NEW_NEED),
        "no needs grew"
    );
    assert!(
        made.iter().all(|c| looks(&h, a, from, *c).is_empty()),
        "its own look kept"
    );
    let from = h.bots[a].inbox.len();
    send_use(&mut h, a, me, crop, 0);
    run_for(&mut h, 2.0);
    assert_eq!(plant_lines(&h, a, from), [(text::NOT_MATURE, CROP_TABLE)]);
    assert!(h.server.plants.by_id.contains_key(&crop), "it stays");
    // Another player: left alone, from the crop's table.
    let (b, you) = h.player_in_world("two", "two");
    h.run(10);
    let your_world = world_of(&h, you);
    stand(&mut h, your_world, [1.0, 0.0]);
    let from = h.bots[b].inbox.len();
    send_use(&mut h, b, you, crop, 0);
    run_for(&mut h, 2.0);
    assert_eq!(plant_lines(&h, b, from), [(text::NOT_YOURS, CROP_TABLE)]);
    // Ripe at five minutes.
    let left = CROP_RIPE + 1.0 - (h.now.0 - start);
    run_for(&mut h, left);
    let from = h.bots[a].inbox.len();
    send_use(&mut h, a, me, crop, 0);
    run_for(&mut h, 2.0);
    assert_eq!(plant_lines(&h, a, from), [(CROP_LINES.harvest, CROP_TABLE)]);
    assert!(h.server.things.get(crop).is_none(), "gone");
    assert_eq!(held(&h, world, SHEAF).map(|x| x.1), Some(1), "its sheaf");
    assert_eq!(crop_phase(&h, world), 4, "its taking moved the quest");
    for c in &made[1..] {
        send_use(&mut h, a, me, *c, 0);
        run_for(&mut h, 2.5);
    }
    assert!(crops(&h).is_empty(), "each harvested");
    let sheaves: u32 = h.server.players[&world]
        .play
        .items
        .iter()
        .filter(|i| i.template == SHEAF)
        .map(|i| i.quantity.max(1))
        .sum();
    assert_eq!(sheaves, 4, "a sheaf each");
    assert_eq!(crop_phase(&h, world), 4, "moved once");
}

/// Examined, a growing plant that needs no care shows only the countdown to its ripening under its
/// own table's label; ripe, its own table's ripe line.
/// Behaviour: server.plants.examined-without-needs
#[test]
fn examining_a_plant_needing_no_care_shows_only_its_countdown_from_its_own_table() {
    let mut h = crop_garden();
    let (a, me, world) = gardener(&mut h, "one", &[(CROP_SEED, 1)]);
    let crop = crops_planted(&mut h, a, me, world)[0];
    let p = h.server.plants.by_id[&crop].clone();
    let early = p.ripe_game - (h.server.game_now + CROP_RIPE);
    assert!(
        (-0.2..=0.0).contains(&early),
        "ripe five minutes after planting"
    );
    let request = {
        let mut r = Vec::new();
        for w in [0x4000_0002u32, 0] {
            r.extend(w.to_le_bytes());
        }
        r.extend(crop.to_le_bytes());
        r.extend(0x3FFu32.to_le_bytes());
        r
    };
    let shown = plants::Shown {
        name: Some(CROP_NAME),
        level: 1,
        kingdom: 0,
        vitals: yalain_world::examine::Vitals {
            health: 3000,
            max_health: 3000,
            vigor: 3000,
            max_vigor: 3000,
            pk_damage: 0,
            pk_vigor_loss: 0,
        },
        armor: 100,
    };
    let lines = (CROP_TABLE, CROP_LINES.maturity_in, CROP_LINES.mature);
    let want = |g| plants::profile_of(0x3FF, &shown, g, lines).packed(&request);
    let from = h.bots[a].inbox.len();
    h.ask(
        a,
        yalain_world::requests::function::EXAMINE,
        &[about(crop)],
        3,
    );
    let growing = plants::Growing {
        needs: None,
        ripe_game: p.ripe_game,
    };
    assert_eq!(last_profile(&h, a, from), Some(want(Some(growing))));
    let bytes = last_profile(&h, a, from).expect("a profile");
    let has = |id: u32| bytes.windows(4).any(|w| w == id.to_le_bytes());
    assert!(!has(text::WATER) && !has(text::POLLEN), "no needs shown");
    run_for(&mut h, CROP_RIPE);
    let from = h.bots[a].inbox.len();
    h.ask(
        a,
        yalain_world::requests::function::EXAMINE,
        &[about(crop)],
        3,
    );
    assert_eq!(last_profile(&h, a, from), Some(want(None)));
}
