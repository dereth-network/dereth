//! A species' own care: a world's row names the items that water and fertilize its plants in place
//! of the dye plants' can and bag; its seed is given to a garden, which keeps it and stands its
//! plant where the garden stands, one at a time; and a plant with lines of its own keeps its own
//! look, shows its stage and how it fares when examined, and withers and refuses in its own lines.
//! Fixture: the test hall with a garden object beside the start; a seed, its plant and the plant's
//! harvest; a can and a trowel whose tables reach the plant; and the dye plants' can, whose table
//! reaches it too.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use auberean_script::native_object::NativeObject;
use yalain_content::plants::{Conditions, Garden, Species, Texts};
use yalain_content::{ContentObject, ContentPack, Description, Facts, Kind, Prop, Props};
use yalain_net::ConnId;
use yalain_testkit::play::server_call;
use yalain_testkit::Harness;
use yalain_world::calls::{self, Node};
use yalain_world::gives::{GIVE, HANDED_AFTER};
use yalain_world::graph::Builder;
use yalain_world::inventory::{move_graph, Move};
use yalain_world::plants;
use yalain_world::usable::interactions::{self, Interaction};

const USE: u32 = 0x8216_0016;

const HALL: u32 = 0x0299_0109;
const START: [f32; 3] = [60.0, 239.0, -44.6];

const GARDEN: u64 = 0x8130_0000_0000_0B01;
const GARDEN_T: u32 = 0x4700_3393;
const GARDEN_AT: [f32; 3] = [61.0, 240.0, -44.6];
const SEED: u32 = 0x4700_1B48;
const PLANT: u32 = 0x4700_1B4A;
const HARVEST: u32 = 0x4700_1B46;
const CAN: u32 = 0x4700_1B51;
const TROWEL: u32 = 0x4700_1B49;
const CAN_TABLE: u32 = 0x6F00_1286;
const TROWEL_TABLE: u32 = 0x6F00_1284;
const DYE_CAN_TABLE: u32 = 0x6F00_124F;

/// The garden's table and lines: planted, busy.
const GARDEN_LINES: u32 = 0x2500_2ADA;
const PLANTED: u32 = 0xC84A_F9AA;
const BUSY: u32 = 0x75BB_918D;

/// The plant's table and lines.
const LINES: u32 = 0x2500_2ADF;
const MATURE: u32 = 0x979A_5B8B;
const HARVESTED: u32 = 0x54AC_9502;
const REFUSED: u32 = 0xA544_694A;
const WITHERS: u32 = 0x200C_71DD;
const STAGES: [u32; 3] = [0xA8AE_7D4D, 0x2B64_913D, 0xAFA1_19CC];
const FARES: Conditions = Conditions {
    healthy: 0xF312_FCF9,
    dry: 0xCD52_FC4E,
    overwatered: 0x9944_8784,
    hungry: 0x28A1_0E7C,
    overfed: 0xFCA8_EFAF,
};
const NAME: (u32, u32) = (0xA860_7CEC, LINES);

fn species() -> Species {
    Species {
        water: CAN,
        fertilizer: TROWEL,
        garden: Some(Garden {
            template: GARDEN_T,
            planted: Some((PLANTED, GARDEN_LINES)),
            busy: Some((BUSY, GARDEN_LINES)),
        }),
        texts: Some(Texts {
            table: LINES,
            maturity_in: None,
            mature: MATURE,
            harvest: HARVESTED,
            not_mature: Some(REFUSED),
            not_yours: Some(REFUSED),
            withers: Some(WITHERS),
            stages: Some(STAGES),
            conditions: Some(FARES),
        }),
        ..Species::new(PLANT, SEED, None, HARVEST, 0)
    }
}

fn garden_object() -> ContentObject {
    ContentObject {
        object: Some(GARDEN),
        template: GARDEN_T,
        description: Description {
            name_text: None,
            package: 0x13A4,
            look: 0x1F00_0100,
            enter_fx: 0,
            scale: None,
            name: Some((0xA860_7CEC, GARDEN_LINES)),
            numbers: [0; 6],
            source: "authored for a test".into(),
        },
        cell: HALL,
        origin: GARDEN_AT,
        rotation: [0.0, 0.0, 0.0, 1.0],
        kind: Kind::Static,
        label: String::new(),
        placed: "authored for a test".into(),
        destination: None,
        facts: Facts::default(),
        walks: None,
        gate: None,
    }
}

/// The hall with its garden, the species of the row, and the items.
fn hall() -> Harness {
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    let mut plant = Props::default();
    plant
        .set(
            interactions::USER_BEHAVIOR,
            Prop::Word(interactions::PLANT_BEHAVIOR),
        )
        .set("GameplayStatistics_Level", Prop::Int(1))
        .set("GameplayStatistics_MaxHealth", Prop::Int(1))
        .set("GameplayStatistics_MaxVigor", Prop::Int(1));
    let mut plant_template = yalain_testkit::test_template(PLANT, 0x284F, plant);
    plant_template.description.name = Some(NAME);
    plant_template.facts.name = Some(NAME);
    h.server.catalog.insert(plant_template);
    let tool = |table: u32| {
        let mut p = Props::default();
        p.set(interactions::TABLE, Prop::Word(table));
        p
    };
    for (t, package, p) in [
        (SEED, 0x1005, Props::default()),
        (HARVEST, 0x1005, Props::default()),
        (CAN, 0x1B34, tool(CAN_TABLE)),
        (TROWEL, 0x1B34, tool(TROWEL_TABLE)),
        (plants::WATERING_CAN, 0x1B34, tool(DYE_CAN_TABLE)),
    ] {
        h.server
            .catalog
            .insert(yalain_testkit::test_template(t, package, p));
    }
    for table in [CAN_TABLE, TROWEL_TABLE, DYE_CAN_TABLE] {
        h.server.interactions.tables.insert(
            table,
            vec![Interaction {
                targets: vec![PLANT],
                reach: 4.0,
                behavior: None,
                message: None,
                uses_up: false,
                uses_up_target: false,
                gate: None,
            }],
        );
    }
    h.server.plants.kinds.species.insert(PLANT, species());
    h.server.content = ContentPack {
        objects: vec![garden_object()],
        ..ContentPack::default()
    };
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

/// A player standing at the start, drawn, holding the items given: its client, id and connection.
fn gardener(h: &mut Harness, name: &str, items: &[(u32, u32)]) -> (usize, u64, ConnId) {
    let (b, me) = h.player_in_world(name, name);
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(10);
    let world = world_of(h, me);
    for &(t, n) in items {
        yalain_world::inventory::give(&mut h.server, &mut h.net, world, t, n);
    }
    h.run(3);
    let p = h.server.players.get_mut(&world).expect("in the world");
    p.position.cell = HALL;
    p.position.origin = START;
    p.position.rotation = [1.0, 0.0, 0.0, 0.0];
    (b, me, world)
}

/// A player gives one of a held item to the garden, handed over and answered.
fn give(h: &mut Harness, b: usize, me: u64, world: ConnId, template: u32) {
    let (item, _) = held(h, world, template).expect("held");
    let slot = h.server.players[&world]
        .play
        .items
        .iter()
        .find(|x| x.id == item)
        .map_or(0, |x| i32::try_from(x.slot).unwrap_or(0));
    let m = Move {
        move_type: GIVE,
        merge: true,
        unlock: true,
        check_take: true,
        all_taken: true,
        grab: true,
        contain: true,
        auto_merge: true,
        target: GARDEN,
        actual_target: GARDEN,
        merge_container: GARDEN,
        from: me,
        actual_from: me,
        item,
        split_item: item,
        quantity: 1,
        quantity_left: 1,
        from_slot: slot,
        actual_from_slot: slot,
        ..Move::default()
    };
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(yalain_world::drops::function::MOVE_ITEM, &[move_graph(&m)]),
    );
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    h.run((HANDED_AFTER / yalain_testkit::STEP).ceil() as usize + 8);
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

/// The string ids of the chat texts a client was shown since `from`, in order.
fn said(h: &Harness, b: usize, from: usize) -> Vec<u32> {
    h.texts(b, from).into_iter().map(|(_, id)| id).collect()
}

/// The one plant standing, if any.
fn the_plant(h: &Harness) -> Option<u64> {
    h.server.plants.by_id.keys().next().copied()
}

/// Plant a seed in the garden: the plant.
fn planted(h: &mut Harness, b: usize, me: u64, world: ConnId) -> u64 {
    give(h, b, me, world, SEED);
    the_plant(h).expect("planted")
}

fn needs(h: &Harness, plant: u64) -> (i32, i32) {
    let p = &h.server.plants.by_id[&plant];
    (p.water, p.fertilizer)
}

/// A seed of a species planted in a garden, given to an object of the garden's template, is kept:
/// the garden says its planting line, and the plant stands where the garden stands, facing as it
/// faces, its giver's. A second seed given while the plant stands is handed back with the garden's
/// busy line, and the garden still holds one plant. The seed used where it is held plants nothing.
/// Behaviour: server.plants.planted-in-a-garden
#[test]
fn a_seed_given_to_its_species_garden_is_kept_and_its_plant_stands_where_the_garden_stands() {
    let mut h = hall();
    let (a, me, world) = gardener(&mut h, "one", &[(SEED, 2)]);
    let from = h.bots[a].inbox.len();
    let plant = planted(&mut h, a, me, world);
    assert_eq!(held(&h, world, SEED).map(|x| x.1), Some(1), "one seed kept");
    assert_eq!(said(&h, a, from), [PLANTED]);
    let p = &h.server.plants.by_id[&plant];
    assert_eq!(
        (p.owner, p.garden),
        (me, Some(0)),
        "the giver's, in the garden"
    );
    let at = h
        .server
        .things
        .get(plant)
        .and_then(|t| t.at)
        .expect("in the world");
    assert_eq!(
        (at.cell, at.origin, at.rotation),
        (HALL, GARDEN_AT, [0.0, 0.0, 0.0, 1.0]),
        "where the garden stands, facing as it faces"
    );
    let from = h.bots[a].inbox.len();
    give(&mut h, a, me, world, SEED);
    assert_eq!(said(&h, a, from), [BUSY]);
    assert_eq!(held(&h, world, SEED).map(|x| x.1), Some(1), "handed back");
    assert_eq!(h.server.plants.by_id.len(), 1, "one plant at a time");
    // Used where it is held, the seed plants nothing.
    h.server.plants.by_id.clear();
    let (seed, _) = held(&h, world, SEED).expect("a seed");
    send_use(&mut h, a, me, seed, 0);
    run_for(&mut h, 2.5);
    assert!(h.server.plants.by_id.is_empty(), "nothing planted by a use");
    assert_eq!(held(&h, world, SEED).map(|x| x.1), Some(1), "the seed kept");
}

/// A plant's own care items lower its needs a level each, its can water and its trowel
/// fertilizer; the dye plants' watering can, though its table reaches the plant, lowers nothing.
/// Behaviour: server.plants.care-items-by-world-rows
#[test]
fn a_plants_own_care_items_lower_its_needs_and_the_dye_plants_can_does_not() {
    let mut h = hall();
    let (a, me, world) = gardener(
        &mut h,
        "one",
        &[(SEED, 1), (CAN, 1), (TROWEL, 1), (plants::WATERING_CAN, 1)],
    );
    let plant = planted(&mut h, a, me, world);
    assert_eq!(needs(&h, plant), (5, 5));
    // Its first thought, when a need may grow, is ten seconds after it stands at the soonest:
    // these three uses come before it.
    for (template, after) in [
        (CAN, (4, 5)),
        (TROWEL, (4, 4)),
        (plants::WATERING_CAN, (4, 4)),
    ] {
        let (item, _) = held(&h, world, template).expect("held");
        send_use(&mut h, a, me, item, plant);
        run_for(&mut h, 2.0);
        assert_eq!(needs(&h, plant), after, "{template:08X}");
    }
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

/// How a plant fares by its needs: healthy with none above "None needed" and none below; else
/// dry, overwatered, hungry and overfed, each that holds, in that order.
/// Behaviour: server.plants.examined-by-own-lines
#[test]
fn a_plant_fares_healthy_or_dry_overwatered_hungry_and_overfed_by_its_needs() {
    let some = |l: &[u32]| {
        let mut out = [None; 4];
        for (i, x) in l.iter().enumerate() {
            out[i] = Some(*x);
        }
        out
    };
    for (water, fertilizer, want) in [
        (2, 3, some(&[FARES.healthy])),
        (4, 3, some(&[FARES.dry])),
        (1, 3, some(&[FARES.overwatered])),
        (3, 4, some(&[FARES.hungry])),
        (3, 1, some(&[FARES.overfed])),
        (5, 5, some(&[FARES.dry, FARES.hungry])),
        (0, 8, some(&[FARES.overwatered, FARES.hungry])),
    ] {
        assert_eq!(
            plants::fares(&FARES, water, fertilizer),
            want,
            "{water}, {fertilizer}"
        );
    }
}

/// A plant with lines of its own, examined while it grows, shows its stage's line and how it fares
/// in place of its needs and pollen, and no countdown when its row names none; ripe, its ripe
/// line. It keeps its own look as it grows.
/// Behaviour: server.plants.examined-by-own-lines
#[test]
fn a_plant_with_its_own_lines_shows_its_stage_and_how_it_fares_and_keeps_its_own_look() {
    let mut h = hall();
    let (a, me, world) = gardener(&mut h, "one", &[(SEED, 1)]);
    let plant = planted(&mut h, a, me, world);
    let request = {
        let mut r = Vec::new();
        for w in [0x4000_0002u32, 0] {
            r.extend(w.to_le_bytes());
        }
        r.extend(plant.to_le_bytes());
        r.extend(0x3FFu32.to_le_bytes());
        r
    };
    let shown = plants::Shown {
        name: Some(NAME),
        level: 1,
        kingdom: 0,
        vitals: yalain_world::examine::Vitals {
            health: 1,
            max_health: 1,
            vigor: 1,
            max_vigor: 1,
            pk_damage: 0,
            pk_vigor_loss: 0,
        },
        armor: 0,
    };
    let ripe_game = h.server.plants.by_id[&plant].ripe_game;
    let want = |stage: Option<u32>, conditions: [Option<u32>; 4]| {
        let g = plants::Growing {
            needs: None,
            fares: Some(plants::Fares { stage, conditions }),
            ripe_game,
        };
        plants::profile_of(0x3FF, &shown, Some(g), (LINES, None, MATURE)).packed(&request)
    };
    let examine = |h: &mut Harness| {
        let from = h.bots[a].inbox.len();
        h.ask(
            a,
            yalain_world::requests::function::EXAMINE,
            &[about(plant)],
            3,
        );
        last_profile(h, a, from)
    };
    assert_eq!(
        examine(&mut h),
        Some(want(
            Some(STAGES[0]),
            [Some(FARES.dry), Some(FARES.hungry), None, None]
        )),
        "a new plant badly needs both"
    );
    // Grown into its third stage and past a thought there (one each 10 to 20 s), cared for:
    // healthy, and its own look all along. It is examined with its next thought at least a second
    // off: its thoughts are at least 10 s apart, so no need grows between its last care and the
    // look.
    let from = h.bots[a].inbox.len();
    let t = h.now.0 + 2.0 * plants::STAGE_SECONDS + 21.0;
    let next_thought = |h: &Harness| {
        h.server
            .timers
            .iter()
            .filter(
                |(_, k)| matches!(k, yalain_world::Timer::PlantThinks { plant: p } if *p == plant),
            )
            .map(|(at, _)| at.0)
            .fold(f64::INFINITY, f64::min)
    };
    while h.now.0 < t || next_thought(&h) < h.now.0 + 1.0 {
        h.step();
        let p = h.server.plants.by_id.get_mut(&plant).expect("standing");
        (p.water, p.fertilizer) = (3, 3);
    }
    assert_eq!(
        examine(&mut h),
        Some(want(
            Some(STAGES[2]),
            [Some(FARES.healthy), None, None, None]
        ))
    );
    let looks = h
        .world_messages(a, from)
        .iter()
        .filter(|m| matches!(m, ServerMessage::UpdateVisualDesc(e) if e.sender.id == plant))
        .count();
    assert_eq!(looks, 0, "it keeps its own look");
    let ripe = plants::profile_of(0x3FF, &shown, None, (LINES, None, MATURE)).packed(&request);
    let t = h.now.0 + plants::RIPE_SECONDS;
    while h.now.0 < t {
        let p = h.server.plants.by_id.get_mut(&plant).expect("standing");
        (p.water, p.fertilizer) = (3, 3);
        h.step();
    }
    assert_eq!(examine(&mut h), Some(ripe), "ripe");
}

/// A plant with lines of its own withers in its own line, whatever withered it; a harvest too early
/// and a harvest by another player are refused in its own line; ripe, harvested by its planter, it
/// says its harvest line and gives its harvest.
/// Behaviour: server.plants.own-lines
#[test]
fn a_plant_with_its_own_lines_withers_refuses_and_is_harvested_in_them() {
    let mut h = hall();
    let (a, me, world) = gardener(&mut h, "one", &[(SEED, 2), (CAN, 1)]);
    let plant = planted(&mut h, a, me, world);
    let from = h.bots[a].inbox.len();
    send_use(&mut h, a, me, plant, 0);
    run_for(&mut h, 2.0);
    assert_eq!(said(&h, a, from), [REFUSED], "not ripe");
    let (b, you, _) = gardener(&mut h, "two", &[]);
    let from = h.bots[b].inbox.len();
    send_use(&mut h, b, you, plant, 0);
    run_for(&mut h, 2.0);
    assert_eq!(said(&h, b, from), [REFUSED], "not its planter's");
    // Watered past "Far too much!": its own withering line.
    h.server
        .plants
        .by_id
        .get_mut(&plant)
        .expect("standing")
        .water = 0;
    let (can, _) = held(&h, world, CAN).expect("a can");
    let from = h.bots[a].inbox.len();
    send_use(&mut h, a, me, can, plant);
    run_for(&mut h, 2.0);
    assert!(!h.server.plants.by_id.contains_key(&plant), "withered");
    assert_eq!(said(&h, a, from), [WITHERS]);
    // The garden takes another; ripe, it is harvested.
    let plant = planted(&mut h, a, me, world);
    let t = h.now.0 + plants::RIPE_SECONDS;
    while h.now.0 < t {
        let p = h.server.plants.by_id.get_mut(&plant).expect("standing");
        (p.water, p.fertilizer) = (3, 3);
        h.step();
    }
    let from = h.bots[a].inbox.len();
    send_use(&mut h, a, me, plant, 0);
    run_for(&mut h, 2.0);
    assert_eq!(said(&h, a, from), [HARVESTED]);
    assert_eq!(held(&h, world, HARVEST).map(|x| x.1), Some(1));
    assert!(h.server.plants.by_id.is_empty(), "gone from the garden");
}

/// A plant whose template names no behavior for a use is harvested all the same, its harvester
/// playing the plants' own behavior, and gives its harvest.
/// Behaviour: server.plants.harvested-with-the-plants-behavior
#[test]
fn a_plant_whose_template_names_no_use_behavior_is_harvested_with_the_plants_own() {
    let mut h = hall();
    let mut plant = h.server.catalog.get(PLANT).expect("the plant").clone();
    plant.props = Props::default();
    h.server.catalog.insert(plant);
    let (a, me, world) = gardener(&mut h, "one", &[(SEED, 1)]);
    let plant = planted(&mut h, a, me, world);
    let t = h.now.0 + plants::RIPE_SECONDS;
    while h.now.0 < t {
        let p = h.server.plants.by_id.get_mut(&plant).expect("standing");
        (p.water, p.fertilizer) = (3, 3);
        h.step();
    }
    let from = h.bots[a].inbox.len();
    send_use(&mut h, a, me, plant, 0);
    run_for(&mut h, 2.0);
    let played = h.world_messages(a, from).iter().any(|m| {
        matches!(m, ServerMessage::PlayBehavior(e) if e.action.behavior == interactions::PLANT_BEHAVIOR)
    });
    assert!(played, "the plants' own behavior");
    assert_eq!(said(&h, a, from), [HARVESTED]);
    assert_eq!(held(&h, world, HARVEST).map(|x| x.1), Some(1));
}
