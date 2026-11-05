//! Surveyor's maps: a map leads to a hidden place in its rectangle, chosen at its first use and
//! kept with it; far from it the map says how far, marks the player's map and says whether the
//! player is getting closer; within its radius it conjures its region's node in front of the
//! player and is thrown away. Fixture: the test hall with surveying, an iron map of a made-up
//! region rectangle and its node as made-up templates.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_content::{Prop, Props};
use yalain_store::Store as _;
use yalain_testkit::play::server_call;
use yalain_testkit::Harness;
use yalain_world::calls::{self, Node, Value};
use yalain_world::craft::{CraftSkillDef, LevelMap};
use yalain_world::mining;
use yalain_world::usable::maps::{self, function, text};

const MAP: u32 = 0x4700_0C2A;
const NODE: u32 = 0x4700_0C98;
const ORE: u32 = 0x4700_06C1;
const USE: u32 = 0x8216_0016;
const USE_DONE: u32 = 0x8216_0021;
const SHOW_TEXT: u32 = yalain_world::session::function::SHOW_TEXT;
const DISCRETE_DESTROY: u32 = 0x820E_0058;
/// The region's corner cells: landblock 0x7F64 cell 30 and landblock 0x8161 cell 3.
const MIN_CELL: u32 = 0x7F64_001E;
const MAX_CELL: u32 = 0x8161_0003;

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

fn surveying() -> CraftSkillDef {
    CraftSkillDef {
        bonus_stat: 0,
        common_name: (0, 0),
        did: mining::SURVEYING,
        cap: 150,
        name: (0xA860_7CEC, 0x2500_13CA),
        advancement: vec![0, 0, 10, 20, 30, 40, 50],
        daily: LevelMap {
            first: 1,
            values: vec![120; 10],
        },
        usage_xp: Some(LevelMap {
            first: 1,
            values: vec![2; 10],
        }),
        pool_share: 0.01,
        cannot_raise: false,
    }
}

fn world() -> Harness {
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    h.server.crafting.skills.push(surveying());
    let mut map = Props::default();
    map.set(
        "Usage_Effect1",
        Prop::Text("Eff_ResetLinvakResetTimers".into()),
    )
    .set(
        "Usage_Effect2",
        Prop::Text("Eff_SummonMine_Iron_Linvak".into()),
    )
    .set(
        "Usage_RequiredLocationTsysMin",
        Prop::Place(MIN_CELL, [0.0; 3]),
    )
    .set(
        "Usage_RequiredLocationTsysMax",
        Prop::Place(MAX_CELL, [0.0; 3]),
    )
    .set("Usage_RequiredLocationRadius", Prop::Int(40))
    .set("Usage_ErrorMessagesTableID", Prop::Word(0x2500_1B52))
    .set("Usage_Animation", Prop::Word(0x4100_00FC));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(MAP, 0x1BF8, map));
    let mut node = Props::default();
    node.set("Mine_Object", Prop::Word(ORE))
        .set("Mine_ObjectQuantity", Prop::Int(35))
        .set("Mine_MaxUses", Prop::Int(3))
        .set("CraftSkill", Prop::Word(0x6F00_069C));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(NODE, 0x1BFC, node));
    h
}

/// A player in the world with a map; the client, the player, its connection, the map.
fn with_map(h: &mut Harness) -> (usize, u64, yalain_net::ConnId, u64) {
    let (b, me) = h.player_in_world("one", "Elara");
    h.run(10);
    let world = *h.server.players.keys().next().expect("in");
    yalain_world::inventory::give(&mut h.server, &mut h.net, world, MAP, 1);
    h.run(3);
    let map = h.server.players[&world]
        .play
        .items
        .iter()
        .find(|i| i.template == MAP)
        .expect("given")
        .id;
    (b, me, world, map)
}

/// Put the player at a place in world meters, outdoors.
fn stand(h: &mut Harness, world: yalain_net::ConnId, gx: f32, gy: f32) {
    let p = maps::place_of(gx, gy);
    let me = h.server.players.get_mut(&world).expect("in");
    me.position.cell = p.cell;
    me.position.origin = [p.origin[0], p.origin[1], 50.0];
}

fn use_map(h: &mut Harness, b: usize, me: u64, map: u64) -> usize {
    let from = h.bots[b].inbox.len();
    h.bots[b].send(true, queue::WEENIE, &server_call(USE, &[use_of(map, me)]));
    h.run(4);
    from
}

/// What a client was told since `from`: texts shown, markers, the use's answers, the map gone,
/// the node made.
fn told(h: &Harness, b: usize, from: usize) -> Vec<String> {
    let first_text = |g: &[(u32, Node)]| {
        g.iter().find_map(|(_, n)| match n {
            Node::TextInfo(t) => Some(t.string_id),
            _ => None,
        })
    };
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) => {
                let v = c.args.as_ref().and_then(|a| calls::values(&a.0).ok());
                let graph = |i: usize| match v.as_ref().and_then(|v| v.get(i)) {
                    Some(Value::Graph(g)) => first_text(g),
                    _ => None,
                };
                match c.function.0 {
                    SHOW_TEXT => graph(0).map(|t| format!("text {t:08X}")),
                    function::DISPLAY_MARKER => match v.as_ref().and_then(|v| v.get(3)) {
                        Some(Value::Int(m)) => Some(format!("marker {m:08X}")),
                        _ => Some("marker".into()),
                    },
                    function::CLEAR_MARKERS => Some("markers cleared".into()),
                    DISCRETE_DESTROY => Some("map gone".into()),
                    USE_DONE => {
                        let Some(Value::Graph(g)) = v.as_ref().and_then(|v| v.first()) else {
                            return None;
                        };
                        g.iter().find_map(|(_, n)| match n {
                            Node::Script { slots, .. } => Some(format!("done {}", slots[11].1)),
                            _ => None,
                        })
                    }
                    _ => None,
                }
            }
            ServerMessage::CreateObject(o) if o.weenie.entity_did == NODE => Some("node".into()),
            _ => None,
        })
        .collect()
}

/// A map far from its place says how far (the band's text and marker) and notes the bearings;
/// moved closer it says so, moved little it says that; within its radius its find is told, the
/// markers cleared, the map thrown away, the use answered and the region's node made.
/// Behaviour: server.craft.surveyors-map-leads-to-its-place
#[test]
fn a_surveyors_map_tells_how_far_its_place_is_and_conjures_its_node_there() {
    let mut h = world();
    let (b, me, world, map) = with_map(&mut h);
    let (dx, dy) = (0x80 as f32 * 160.0 + 50.0, 0x62 as f32 * 160.0 + 70.0);
    h.server.things.get_mut(map).expect("map").destination = Some(maps::place_of(dx, dy));
    stand(&mut h, world, dx + 150.0, dy);
    let from = use_map(&mut h, b, me, map);
    assert_eq!(
        told(&h, b, from),
        [
            format!("text {:08X}", text::BANDS[1].0),
            format!("marker {:08X}", text::BANDS[1].1),
            format!("text {:08X}", text::FIRST),
            format!("done {}", maps::NOT_THERE),
        ]
    );
    stand(&mut h, world, dx + 120.0, dy);
    let from = use_map(&mut h, b, me, map);
    assert_eq!(
        told(&h, b, from)[2],
        format!("text {:08X}", text::PROGRESS[2]),
        "a little closer"
    );
    stand(&mut h, world, dx + 115.0, dy);
    let from = use_map(&mut h, b, me, map);
    assert_eq!(told(&h, b, from)[2], format!("text {:08X}", text::STILL));
    // About as far as before, 25 m from the bearings: "the same", and the bearings stay; 15 m on
    // from there (40 m from the bearings) is compared with them, not taken as not moved.
    stand(&mut h, world, dx + 118.0, dy + 25.0);
    let from = use_map(&mut h, b, me, map);
    assert_eq!(
        told(&h, b, from)[2],
        format!("text {:08X}", text::PROGRESS[3])
    );
    stand(&mut h, world, dx + 118.0, dy + 40.0);
    let from = use_map(&mut h, b, me, map);
    assert_eq!(
        told(&h, b, from)[2],
        format!("text {:08X}", text::PROGRESS[3]),
        "the bearings kept"
    );
    stand(&mut h, world, dx + 2000.0, dy);
    let from = use_map(&mut h, b, me, map);
    assert_eq!(
        told(&h, b, from),
        [
            format!("text {:08X}", text::OUTSIDE),
            format!("done {}", maps::NOT_THERE)
        ]
    );
    stand(&mut h, world, dx + 30.0, dy);
    let from = use_map(&mut h, b, me, map);
    assert_eq!(
        told(&h, b, from),
        [
            format!("text {:08X}", text::FOUND.0),
            "markers cleared".to_string(),
            "map gone".into(),
            "done 0".into(),
            "node".into(),
        ]
    );
    assert!(
        !h.server.players[&world]
            .play
            .items
            .iter()
            .any(|i| i.id == map),
        "thrown away"
    );
}

/// A map's place is the center of one of its rectangle's cells (its two corner cells whole), chosen
/// at its first use and kept with the map.
/// Behaviour: server.craft.surveyors-map-place-kept
#[test]
fn a_maps_place_is_chosen_inside_its_rectangle_and_kept_with_it() {
    let mut h = world();
    let (b, me, _, map) = with_map(&mut h);
    use_map(&mut h, b, me, map);
    let to = h
        .server
        .things
        .get(map)
        .and_then(|t| t.destination)
        .expect("chosen");
    let (gx, gy) = maps::global(to.cell, to.origin[0], to.origin[1]);
    let ((x0, y0), (x1, y1)) = maps::rectangle(MIN_CELL, MAX_CELL);
    assert!(
        (x0..=x1).contains(&gx) && (y0..=y1).contains(&gy),
        "{gx} {gy}"
    );
    assert_eq!(
        ((gx - x0) % 20.0, (gy - y0) % 20.0),
        (10.0, 10.0),
        "a cell's center"
    );
    assert_eq!(
        (x0, x1),
        (0x7F as f32 * 160.0 + 60.0, 0x81 as f32 * 160.0 + 20.0)
    );
    assert_eq!(
        (y0, y1),
        (0x61 as f32 * 160.0 + 40.0, 0x64 as f32 * 160.0 + 120.0)
    );
    use_map(&mut h, b, me, map);
    assert_eq!(
        h.server.things.get(map).and_then(|t| t.destination),
        Some(to)
    );
    let kept = h
        .server
        .store
        .world_state(me)
        .expect("reads")
        .expect("kept");
    let play = yalain_world::state::PlayState::from_bytes(&kept.play).expect("reads");
    let state = play
        .item_states
        .iter()
        .find(|i| i.id == map)
        .expect("its state");
    assert_eq!(state.destination, Some(to), "kept with the map");
}

/// A map found at its place makes the quest step its use is the trigger of: the quest moves on with
/// the find.
/// Behaviour: server.craft.surveyors-map-find-moves-its-quest
#[test]
fn a_map_found_at_its_place_moves_on_the_quest_it_is_part_of() {
    use yalain_content::quests::{QuestDef, Step, Trigger, Update};
    use yalain_world::quests::{self, status};
    let mut h = world();
    let quest = 0x4000_00C7;
    h.server.quests.quests.push(QuestDef {
        id: quest,
        name: (1, 0x2500_14B4),
        description: (2, 0x2500_14B4),
        icon: 0,
        play_fx: false,
        journals: vec![(3, 0x2500_14B4), (4, 0x2500_14B4)],
        challenge: 6,
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
    });
    h.server.quests.steps.push(Step {
        quest,
        needs: 1,
        trigger: Trigger::Use(MAP),
        tell: None,
        update: Update::Advance(2),
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
    });
    let (b, me, world, map) = with_map(&mut h);
    h.server
        .players
        .get_mut(&world)
        .expect("in")
        .play
        .quests
        .push(yalain_world::state::QuestState {
            quest,
            status: status::UNDERWAY,
            phase: 1,
            started: 0,
            updated: 0,
        });
    let (dx, dy) = (0x80 as f32 * 160.0 + 50.0, 0x62 as f32 * 160.0 + 70.0);
    h.server.things.get_mut(map).expect("map").destination = Some(maps::place_of(dx, dy));
    stand(&mut h, world, dx + 150.0, dy);
    let from = use_map(&mut h, b, me, map);
    assert!(
        !h.calls(b, from)
            .iter()
            .any(|(f, _)| *f == quests::UPDATE_QUEST),
        "not found: no step"
    );
    stand(&mut h, world, dx + 10.0, dy);
    let from = use_map(&mut h, b, me, map);
    assert!(
        h.calls(b, from)
            .iter()
            .any(|(f, _)| *f == quests::UPDATE_QUEST),
        "the quest updated with the find"
    );
    assert_eq!(
        quests::phase_of(&h.server.players[&world].play.quests, quest),
        2
    );
}

/// A node a map led to may unearth a find in place of its material: a rough lodestone of the
/// material's family (or a Tessera) comes into the pack, one unit, and the use is told so by the
/// item's name; the material is not given for that use.
/// Behaviour: server.craft.mining-a-maps-node-may-unearth-a-find
#[test]
fn mining_a_maps_node_sometimes_unearths_a_lodestone_in_place_of_the_ore() {
    let mut h = world();
    for t in [mining::TESSERA, 0x4700_0C17, ORE] {
        h.server
            .catalog
            .insert(yalain_testkit::test_template(t, 0x3A1, Props::default()));
    }
    let (b, me, world, map) = with_map(&mut h);
    let (dx, dy) = (0x80 as f32 * 160.0 + 50.0, 0x62 as f32 * 160.0 + 70.0);
    h.server.things.get_mut(map).expect("map").destination = Some(maps::place_of(dx, dy));
    stand(&mut h, world, dx + 10.0, dy);
    use_map(&mut h, b, me, map);
    let node = h
        .server
        .things
        .by_id
        .values()
        .find(|t| t.template == NODE)
        .map(|t| t.id)
        .expect("conjured");
    let count = |h: &Harness, t: u32| -> u32 {
        h.server.players[&world]
            .play
            .items
            .iter()
            .filter(|i| i.template == t)
            .map(|i| i.quantity.max(1))
            .sum()
    };
    let mut found = None;
    for _ in 0..400 {
        if let Some(n) = h.server.things.get_mut(node).and_then(|t| t.node.as_mut()) {
            n.uses = 0;
        }
        let ore = count(&h, ORE);
        let from = h.bots[b].inbox.len();
        h.bots[b].send(true, queue::WEENIE, &server_call(USE, &[use_of(node, me)]));
        let t = h.now.0 + 2.5;
        while h.now.0 < t {
            h.step();
        }
        let texts: Vec<u32> = h
            .calls(b, from)
            .iter()
            .filter(|(f, _)| *f == USE_DONE)
            .filter_map(|(_, v)| match v.first() {
                Some(Value::Graph(g)) => g.iter().find_map(|(_, n)| match n {
                    Node::TextInfo(t) => Some(t.string_id),
                    _ => None,
                }),
                _ => None,
            })
            .collect();
        if texts.contains(&mining::FOUND.0) {
            found = Some(ore);
            break;
        }
    }
    let ore_before = found.expect("a find within 400 uses");
    assert_eq!(count(&h, ORE), ore_before, "no ore for the find's use");
    assert_eq!(
        count(&h, 0x4700_0C17) + count(&h, mining::TESSERA),
        1,
        "one lodestone or Tessera"
    );
}

/// A map of a region finding its place lets the region's five map recipes be drawn at once: the
/// five records, waiting until now, come in one call before the find's text.
/// Behaviour: server.craft.map-find-ends-the-recipes-wait
#[test]
fn a_maps_find_lets_its_regions_map_recipes_be_drawn_again() {
    use yalain_world::craft::{function as craft, new_recipe, MAP_RECIPES};
    let mut h = world();
    let (b, me, world, map) = with_map(&mut h);
    let five = MAP_RECIPES[0].2;
    {
        let p = h.server.players.get_mut(&world).expect("in");
        for r in five {
            let mut rec = new_recipe(r);
            rec.use_again = 1.0e12;
            p.play.recipes.push(rec);
        }
    }
    let (dx, dy) = (0x80 as f32 * 160.0 + 50.0, 0x62 as f32 * 160.0 + 70.0);
    h.server.things.get_mut(map).expect("map").destination = Some(maps::place_of(dx, dy));
    stand(&mut h, world, dx + 10.0, dy);
    let from = use_map(&mut h, b, me, map);
    let now = h.net.game_time(h.now);
    // In the order sent: a larger message may arrive after smaller ones sent later.
    let mut sent: Vec<(u64, u32)> = h.bots[b].inbox[from..]
        .iter()
        .filter(|r| r.world && r.queue == queue::EVENT)
        .filter_map(|r| match r.message()? {
            ServerMessage::PlayerCall(c) => Some((r.blob_id & 0xFFFF_FFFF, c.function.0)),
            _ => None,
        })
        .collect();
    sent.sort_by_key(|(n, _)| *n);
    let fns: Vec<u32> = sent.into_iter().map(|(_, f)| f).collect();
    let list = fns.iter().position(|f| *f == craft::UPDATE_RECIPES);
    let text = fns.iter().position(|f| *f == SHOW_TEXT);
    assert!(
        list.is_some() && list < text,
        "the list before the text: {fns:x?}"
    );
    let p = &h.server.players[&world];
    for r in five {
        let rec = p.play.recipes.iter().find(|x| x.recipe == r).expect("held");
        assert!(rec.use_again <= now, "{r:08X} may be drawn now");
    }
}
